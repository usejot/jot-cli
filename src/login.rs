//! `jot login`: the browser picks the workspace, the dashboard mints an agent
//! token and sends it back to a one-shot server on 127.0.0.1. Every name,
//! number and page is the contract's (`cli::connect`, `cli::copy`).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::api::Api;
use crate::contract::cli::copy::{self, messages, pages};
use crate::contract::cli::{connect as rules, env as names};
use crate::contract::{AgentWhoamiResponse, CliProfile};
use crate::profiles::{read_config, slugify, unique_name, write_config, Env};
use crate::text::{fill, parse_query, url_encode};
use crate::{CliError, Context, Result};

/// What came back on the callback, verified.
#[derive(Debug)]
struct Received {
    token: String,
    email: Option<String>,
    reach: AgentWhoamiResponse,
}

// ---------------------------------------------------------------- the state

/// `cli::connect::STATE_BYTES` random bytes as hex: only this run knows it.
pub fn new_state() -> String {
    let bytes = random_bytes(rules::STATE_BYTES as usize);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(unix)]
fn random_bytes(count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; count];
    match std::fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes)) {
        Ok(()) => bytes,
        Err(_) => hashed_bytes(count),
    }
}

#[cfg(not(unix))]
fn random_bytes(count: usize) -> Vec<u8> {
    hashed_bytes(count)
}

/// No /dev/urandom: std's RandomState is seeded from the OS generator on every
/// platform (Windows included), mixed with the time, the pid and a stack address.
fn hashed_bytes(count: usize) -> Vec<u8> {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    let marker = 0u8;
    let mut out = Vec::with_capacity(count + 8);
    while out.len() < count {
        let mixed = (std::time::SystemTime::now(), std::process::id(), &marker as *const u8 as usize, out.len());
        out.extend_from_slice(&RandomState::new().hash_one(mixed).to_le_bytes());
    }
    out.truncate(count);
    out
}

/// The dashboard's rule for a state: `STATE_MIN_LENGTH`–`STATE_MAX_LENGTH` of [A-Za-z0-9_-].
pub fn valid_state(state: &str) -> bool {
    let length = state.len() as u64;
    (rules::STATE_MIN_LENGTH..=rules::STATE_MAX_LENGTH).contains(&length)
        && state.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The dashboard's rule for a port: 4 or 5 digits, `PORT_MIN`–`PORT_MAX`.
pub fn valid_port(raw: &str) -> Option<u16> {
    if !(4..=5).contains(&raw.len()) || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let port: u64 = raw.parse().ok()?;
    if !(rules::PORT_MIN..=rules::PORT_MAX).contains(&port) {
        return None;
    }
    u16::try_from(port).ok()
}

// ---------------------------------------------------------------- the browser

/// Opens a URL in the default browser. Best effort: the URL is printed too.
pub fn open_browser(url: &str) {
    use std::process::{Command, Stdio};
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else if cfg!(windows) {
        // ponytail: rundll32, not `cmd /c start`: cmd would split the URL at each `&`.
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// The machine's name, for the token label; the contract's fallback when unknown.
fn hostname() -> String {
    let env = Env::from_process();
    if let Some(name) = env.get("HOSTNAME").or_else(|| env.get("COMPUTERNAME")) {
        return name.to_string();
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| rules::HOST_FALLBACK.to_string())
}

/// The dashboard URL this run opens: `DASHBOARD_PATH` with `QUERY_PARAMS`.
pub fn connect_url(app_url: &str, port: u16, state: &str, host: &str, name: Option<&str>) -> String {
    let port = port.to_string();
    let values: Vec<(&str, Option<&str>)> = rules::QUERY_PARAMS
        .iter()
        .map(|key| {
            let value = match *key {
                "port" => Some(port.as_str()),
                "state" => Some(state),
                "host" => Some(host),
                "name" => name,
                _ => None,
            };
            (*key, value)
        })
        .collect();
    let query: Vec<String> =
        values.into_iter().filter_map(|(key, value)| Some(format!("{key}={}", url_encode(value?)))).collect();
    format!("{}{}?{}", app_url.trim_end_matches('/'), rules::DASHBOARD_PATH, query.join("&"))
}

// ---------------------------------------------------------------- the callback

fn page(title: &str, body: &str) -> String {
    fill(copy::PAGE, &[("title", title), ("body", body)])
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body.as_bytes()));
}

/// The request line's method and target, read from the head (capped at 8 KB).
fn request_target(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut head = Vec::new();
    let mut buffer = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") && head.len() < 8192 {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        head.extend_from_slice(&buffer[..read]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut parts = text.lines().next()?.split_whitespace();
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
}

/// Waits for the dashboard to send the token back, checks the state, and
/// checks the token works before anything is saved.
fn receive(listener: &TcpListener, state: &str, api_url: &str) -> Result<Received> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(rules::TIMEOUT_SECONDS);
    loop {
        // 1. the next connection, or the timeout
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(CliError(messages::LOGIN_TIMEOUT.to_string()));
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        // 2. only GET /callback
        let Some((method, target)) = request_target(&mut stream) else { continue };
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        if method != "GET" || path != rules::CALLBACK_PATH {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            continue;
        }

        // 3. only the browser tab this run opened knows the state
        let params = parse_query(query);
        let param = |key: &str| params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
        let (Some(token), true) = (param("token"), param("state").as_deref() == Some(state)) else {
            respond(&mut stream, "400 Bad Request", &page(pages::wrong_state::TITLE, pages::wrong_state::BODY));
            continue;
        };

        // 4. the token must actually work before it is saved
        return match Api::new(api_url, &token).whoami() {
            Ok(reach) => {
                respond(&mut stream, "200 OK", &page(pages::connected::TITLE, pages::connected::BODY));
                Ok(Received { token, email: param("email"), reach })
            }
            Err(error) => {
                respond(&mut stream, "400 Bad Request", &page(pages::failed::TITLE, pages::failed::BODY));
                Err(CliError(fill(messages::LOGIN_TOKEN_REFUSED, &[("message", &error.0)])))
            }
        };
    }
}

/// Where to sign in, and what to save the profile as.
pub struct SignInInput {
    pub home: PathBuf,
    pub api_url: String,
    pub app_url: String,
    /// The profile name asked for; None: the workspace's existing profile, or a new slug.
    pub wanted: Option<String>,
}

/// The profile a finished sign-in saved.
#[derive(Debug, PartialEq, Eq)]
pub struct SignedIn {
    pub name: String,
    pub org: String,
    pub email: Option<String>,
    pub is_default: bool,
}

/// A sign-in waiting for the browser: the loopback listener, and the URL that
/// opens the dashboard. It prints nothing, so `jot mcp` can run it without
/// touching stdout.
pub struct PendingSignIn {
    pub url: String,
    listener: TcpListener,
    state: String,
    input: SignInInput,
}

/// Binds the loopback port and builds the dashboard URL. Nothing is opened or printed.
pub fn begin_sign_in(input: SignInInput) -> Result<PendingSignIn> {
    // 1. a state only this run knows, and a free port on the loopback host
    let state = new_state();
    let listener = TcpListener::bind((rules::LOOPBACK_HOST, 0))?;
    let port = listener.local_addr()?.port();

    // 2. the dashboard must accept both, or the browser would show an error
    if !valid_state(&state) || valid_port(&port.to_string()).is_none() {
        return Err(CliError(fill(
            messages::LOGIN_TOKEN_REFUSED,
            &[("message", "the dashboard would refuse this port or state")],
        )));
    }
    let url = connect_url(&input.app_url, port, &state, &hostname(), input.wanted.as_deref());
    Ok(PendingSignIn { url, listener, state, input })
}

impl PendingSignIn {
    /// Waits (up to `TIMEOUT_SECONDS`) for the token, checks it, and saves the profile.
    pub fn finish(self) -> Result<SignedIn> {
        let received = receive(&self.listener, &self.state, &self.input.api_url)?;
        save_profile(&self.input, received)
    }
}

/// The profile: the name asked for, the one already used for this workspace, or a new slug.
fn save_profile(input: &SignInInput, received: Received) -> Result<SignedIn> {
    // 1. which name
    let mut config = read_config(&input.home)?;
    let org = received.reach.org.name.clone();
    let existing = config
        .profiles
        .iter()
        .find(|(_, profile)| profile.org_name.as_deref() == Some(org.as_str()) && profile.api_url == input.api_url)
        .map(|(name, _)| name.clone());
    let name = input.wanted.clone().or(existing).unwrap_or_else(|| unique_name(&slugify(&org), &config.profiles));

    // 2. saved; the first profile becomes the default
    let profile = CliProfile {
        api_url: input.api_url.clone(),
        token: received.token,
        org_name: Some(org.clone()),
        email: received.email.clone(),
    };
    config.profiles.insert(name.clone(), profile);
    if config.default.is_none() {
        config.default = Some(name.clone());
    }
    write_config(&input.home, &config)?;

    let is_default = config.default.as_deref() == Some(name.as_str());
    Ok(SignedIn { name, org, email: received.email, is_default })
}

/// Where `jot login` signs in: the flags, then the env, then the hosted defaults.
pub fn sign_in_input(ctx: &Context, wanted: Option<String>) -> SignInInput {
    let env = Env::from_process();
    let api_url =
        ctx.args.value("api").or(env.get(names::api_url::NAME)).unwrap_or(names::api_url::DEFAULT).to_string();
    let app_url =
        ctx.args.value("app").or(env.get(names::app_url::NAME)).unwrap_or(names::app_url::DEFAULT).to_string();
    SignInInput { home: ctx.home.clone(), api_url, app_url, wanted }
}

pub fn login(ctx: &Context) -> Result<()> {
    // 1. the dashboard URL, with a state only this run knows
    let pending = begin_sign_in(sign_in_input(ctx, ctx.args.value("profile").map(str::to_string)))?;

    // 2. the browser picks the workspace; the token comes back to the loopback host
    println!("{}", messages::LOGIN_OPENING);
    println!("{}", fill(messages::LOGIN_URL, &[("url", &pending.url)]));
    open_browser(&pending.url);
    let signed = pending.finish()?;

    // 3. said plainly, with the next step
    let who = signed.email.as_deref().map(|email| fill(messages::LOGIN_WHO, &[("email", email)])).unwrap_or_default();
    let is_default = if signed.is_default {
        messages::LOGIN_IS_DEFAULT.to_string()
    } else {
        fill(messages::LOGIN_NOT_DEFAULT, &[("name", &signed.name)])
    };
    let connected = fill(
        messages::LOGIN_CONNECTED,
        &[("org", &signed.org), ("who", &who), ("name", &signed.name), ("isDefault", &is_default)],
    );
    println!("{connected}");
    println!("{}", messages::LOGIN_NEXT);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::tests::conformance;
    use serde_json::{json, Value};

    /// The dashboard's parse of the /cli/connect query, from the same rules the CLI checks itself against.
    fn parse_connect(query: &str) -> Value {
        let params = parse_query(query);
        let get = |key: &str| params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let Some(port) = valid_port(&get("port").unwrap_or_default()) else { return Value::Null };
        let state = get("state").unwrap_or_default();
        if !valid_state(&state) {
            return Value::Null;
        }
        let cut = |value: String, max: u64| value.trim().chars().take(max as usize).collect::<String>();
        let host = Some(cut(get("host").unwrap_or_default(), rules::HOST_MAX_LENGTH)).filter(|h| !h.is_empty());
        let name = get("name").map(|n| cut(n, rules::NAME_MAX_LENGTH)).filter(|n| !n.is_empty());
        json!({ "port": port, "state": state, "host": host.unwrap_or_else(|| rules::HOST_FALLBACK.into()), "name": name })
    }

    #[test]
    fn conformance_connect() {
        for case in conformance()["connect"].as_array().unwrap() {
            assert_eq!(parse_connect(case["query"].as_str().unwrap()), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn its_own_request_passes_the_dashboard_rules() {
        let state = new_state();
        assert_eq!(state.len() as u64, rules::STATE_BYTES * 2);
        assert_ne!(state, new_state());
        let url = connect_url("https://app.test/", 51234, &state, "my box", Some("office"));
        assert_eq!(url, format!("https://app.test/cli/connect?port=51234&state={state}&host=my%20box&name=office"));
        let parsed = parse_connect(url.split_once('?').unwrap().1);
        assert_eq!(parsed["host"], "my box");
        assert_eq!(hashed_bytes(16).len(), 16);
    }

    #[test]
    fn a_sign_in_saves_the_workspace_once_and_the_first_is_the_default() {
        let home = crate::profiles::tests::temp_dir("signin");
        let input = |wanted: Option<&str>| SignInInput {
            home: home.clone(),
            api_url: "https://api.test".into(),
            app_url: "https://app.test".into(),
            wanted: wanted.map(str::to_string),
        };
        let received = |org: &str, token: &str| Received {
            token: token.into(),
            email: Some("me@x.test".into()),
            reach: AgentWhoamiResponse {
                org: crate::contract::NamedRef { id: "o".into(), name: org.into() },
                ..AgentWhoamiResponse::default()
            },
        };

        // 1. the first: named from the workspace, and the default
        let first = save_profile(&input(None), received("Acme Inc.", "t1")).unwrap();
        assert_eq!((first.name.as_str(), first.is_default), ("acme-inc", true));

        // 2. the same workspace again: the same profile, its token replaced
        let again = save_profile(&input(None), received("Acme Inc.", "t2")).unwrap();
        assert_eq!(again.name, "acme-inc");
        assert_eq!(read_config(&home).unwrap().profiles["acme-inc"].token, "t2");

        // 3. another, by name: saved beside it, not the default
        let work = save_profile(&input(Some("work")), received("Work Co", "t3")).unwrap();
        assert_eq!((work.name.as_str(), work.is_default), ("work", false));
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn the_callback_saves_nothing_unless_the_state_matches() {
        let listener = TcpListener::bind((rules::LOOPBACK_HOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let send = |target: &str| {
                let mut stream = TcpStream::connect((rules::LOOPBACK_HOST, port)).unwrap();
                write!(stream, "GET {target} HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
                let mut answer = String::new();
                stream.read_to_string(&mut answer).unwrap();
                answer
            };
            let wrong = send("/callback?state=nope&token=t");
            let other = send("/favicon.ico");
            // the right state, but an API that is not there: refused, not saved
            let right = send("/callback?state=s0123456789abcdef&token=t");
            (wrong, other, right)
        });
        let result = receive(&listener, "s0123456789abcdef", "http://127.0.0.1:9");
        let (wrong, other, right) = client.join().unwrap();
        assert!(wrong.starts_with("HTTP/1.1 400") && wrong.contains(pages::wrong_state::TITLE));
        assert!(other.starts_with("HTTP/1.1 404"));
        assert!(right.contains(pages::failed::TITLE));
        assert!(result.unwrap_err().0.starts_with(&messages::LOGIN_TOKEN_REFUSED[..20]));
    }
}
