//! Profiles: which Jot workspace this terminal (or this repo) talks to.
//!
//! A profile is a name, an API URL and an agent token, kept in
//! `~/.jot/config.json` (0600, in a 0700 directory; `JOT_HOME` moves it). A
//! repo may name its profile in a committed `.jot.json`, which holds no secret.
//! Which profile wins is `cli::PROFILE_RESOLUTION`, walked in order. The
//! choosing is pure (`pick`); the file reading around it is thin.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::api::Api;
use crate::contract::agent::errors;
use crate::contract::cli::{self as c, copy::messages, env as names, paths};
use crate::contract::{CliConfig, CliProfile, RepoBinding};
use crate::text::fill;
use crate::{CliError, Context, Result};

/// The environment variables a resolution reads, captured once.
#[derive(Debug, Default, Clone)]
pub struct Env(pub BTreeMap<String, String>);

impl Env {
    pub fn from_process() -> Self {
        Env(std::env::vars().collect())
    }

    /// A variable's value; an empty one counts as unset.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str).filter(|value| !value.is_empty())
    }
}

/// The profile in use, by `cli::PROFILE_RESOLUTION`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The profile's name; `cli::profiles::LEGACY_ENV_NAME` when it came from JOT_API_TOKEN.
    pub name: String,
    pub api_url: String,
    pub token: String,
    /// The project the repo is bound to, when its .jot.json names one.
    pub project_id: Option<String>,
}

impl Resolved {
    pub fn api(&self) -> Api {
        Api::new(&self.api_url, &self.token)
    }

    pub fn is_legacy(&self) -> bool {
        self.name == c::profiles::LEGACY_ENV_NAME
    }
}

/// What `pick` chooses from.
pub struct PickInput<'a> {
    pub flag: Option<&'a str>,
    pub env: &'a Env,
    pub binding: Option<&'a RepoBinding>,
    pub config: &'a CliConfig,
}

// ---------------------------------------------------------------- paths and modes

/// `~/.jot`, or `$JOT_HOME` when set.
pub fn jot_home(env: &Env) -> PathBuf {
    if let Some(home) = env.get(names::home::NAME) {
        return PathBuf::from(home);
    }
    let user_home = env.get("HOME").or_else(|| env.get("USERPROFILE")).unwrap_or(".");
    let default = names::home::DEFAULT;
    PathBuf::from(user_home).join(default.trim_start_matches("~/"))
}

fn config_path(home: &Path) -> PathBuf {
    home.join(paths::CONFIG)
}

/// A contract mode ("0600") as a number.
fn mode(octal: &str) -> u32 {
    u32::from_str_radix(octal, 8).unwrap_or(0o600)
}

/// Writes a file readable by this user only, in a private directory: tokens are secrets.
pub fn write_private(path: &Path, content: &str) -> Result<()> {
    // 1. the directory, private (create_dir_all's mode does not apply to an existing one)
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
        set_mode(dir, mode(paths::HOME_MODE))?;
    }

    // 2. the file, private from the moment it exists
    write_with_mode(path, content, mode(paths::CONFIG_MODE))
}

#[cfg(unix)]
fn write_with_mode(path: &Path, content: &str, bits: u32) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(bits).open(path)?;
    file.write_all(content.as_bytes())?;
    set_mode(path, bits)
}

#[cfg(not(unix))]
fn write_with_mode(path: &Path, content: &str, _bits: u32) -> Result<()> {
    // ponytail: Windows has no mode bits; the file sits in the user's own profile directory.
    fs::write(path, content)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, bits: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(bits))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _bits: u32) -> Result<()> {
    Ok(())
}

/// The contract's file format: 2-space JSON and a trailing newline.
pub fn pretty(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

// ---------------------------------------------------------------- config file

/// The config, or an empty one when there is none yet.
pub fn read_config(home: &Path) -> Result<CliConfig> {
    let path = config_path(home);
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let text = fs::read_to_string(&path)?;
    let parsed: Value =
        serde_json::from_str(&text).map_err(|error| CliError(format!("{}: {error}", path.display())))?;
    Ok(config_from(&parsed))
}

/// Keeps only the entries that carry what a profile needs, and a default that names one.
pub fn config_from(parsed: &Value) -> CliConfig {
    // 1. profiles with a string apiUrl and token; the optional fields only when strings
    let mut config = CliConfig::default();
    let Some(entries) = parsed.get("profiles").and_then(Value::as_object) else { return config };
    for (name, value) in entries {
        let field = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let (Some(api_url), Some(token)) = (field("apiUrl"), field("token")) else { continue };
        let profile = CliProfile { api_url, token, org_name: field("orgName"), email: field("email") };
        config.profiles.insert(name.clone(), profile);
    }

    // 2. a default that names nothing is no default
    config.default = parsed
        .get("default")
        .and_then(Value::as_str)
        .filter(|name| config.profiles.contains_key(*name))
        .map(str::to_string);
    config
}

pub fn write_config(home: &Path, config: &CliConfig) -> Result<()> {
    write_private(&config_path(home), &pretty(config)?)
}

// ---------------------------------------------------------------- .jot.json

/// The nearest .jot.json at or above `cwd`, with what it says.
pub fn find_binding(cwd: &Path) -> Result<Option<(PathBuf, RepoBinding)>> {
    for dir in cwd.ancestors() {
        let path = dir.join(paths::REPO_BINDING);
        if path.is_file() {
            let text = fs::read_to_string(&path)?;
            let parsed: Value =
                serde_json::from_str(&text).map_err(|error| CliError(format!("{}: {error}", path.display())))?;
            return Ok(Some((path, binding_from(&parsed))));
        }
    }
    Ok(None)
}

/// Only string fields count: a .jot.json never carries anything else the CLI reads.
fn binding_from(parsed: &Value) -> RepoBinding {
    let field = |key: &str| parsed.get(key).and_then(Value::as_str).map(str::to_string);
    RepoBinding { profile: field("profile"), project: field("project") }
}

// ---------------------------------------------------------------- resolution

/// The profile to use, walking `cli::PROFILE_RESOLUTION`. None: nothing is set up.
///
/// A name from the flag, JOT_PROFILE or the repo that the config lacks is an
/// error, never a silent fall-through to another workspace.
pub fn pick(input: &PickInput) -> Result<Option<Resolved>> {
    for source in c::PROFILE_RESOLUTION {
        // 1. an explicit name, from one of the first three sources
        let named = match *source {
            "flag" => input.flag,
            "env" => input.env.get(names::profile::NAME),
            "repoFile" => input.binding.and_then(|binding| binding.profile.as_deref()),
            _ => None,
        };
        if let Some(name) = named {
            let profile = input
                .config
                .profiles
                .get(name)
                .ok_or_else(|| CliError(fill(errors::PROFILE_NOT_FOUND, &[("name", name)])))?;
            return Ok(Some(with_project(name, profile, input.binding)));
        }

        // 2. the default profile
        if *source == "default" {
            let found = input.config.default.as_ref().and_then(|name| Some((name, input.config.profiles.get(name)?)));
            if let Some((name, profile)) = found {
                return Ok(Some(with_project(name, profile, input.binding)));
            }
        }

        // 3. the legacy environment: JOT_API_TOKEN, never with a repo's project
        if *source == "legacyEnv" {
            if let Some(token) = input.env.get(names::api_token::NAME) {
                return Ok(Some(Resolved {
                    name: c::profiles::LEGACY_ENV_NAME.to_string(),
                    api_url: input.env.get(names::api_url::NAME).unwrap_or(names::api_url::DEFAULT).to_string(),
                    token: token.to_string(),
                    project_id: None,
                }));
            }
        }
    }
    Ok(None)
}

/// The repo's project rides along only when the repo's profile is the one in use.
fn with_project(name: &str, profile: &CliProfile, binding: Option<&RepoBinding>) -> Resolved {
    let project_id = binding
        .filter(|binding| binding.profile.as_deref().is_none_or(|bound| bound == name))
        .and_then(|binding| binding.project.clone());
    Resolved { name: name.to_string(), api_url: profile.api_url.clone(), token: profile.token.clone(), project_id }
}

/// `pick`, fed from the real files, the process env and the working directory.
pub fn resolve(ctx: &Context) -> Result<Option<Resolved>> {
    let env = Env::from_process();
    let binding = find_binding(&std::env::current_dir()?)?.map(|(_, binding)| binding);
    let config = read_config(&ctx.home)?;
    pick(&PickInput { flag: ctx.args.value("profile"), env: &env, binding: binding.as_ref(), config: &config })
}

/// `resolve`, or the contract's "not connected" error naming what to run.
pub fn connect(ctx: &Context) -> Result<Resolved> {
    resolve(ctx)?.ok_or_else(|| CliError(errors::NOT_CONNECTED.to_string()))
}

// ---------------------------------------------------------------- naming

/// "Acme Inc." → "acme-inc" (`cli::profiles::SLUG_PATTERN` runs become "-").
pub fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        c::profiles::SLUG_FALLBACK.to_string()
    } else {
        slug.to_string()
    }
}

/// `base`, or `base-2`, `base-3`…: the first name not taken.
pub fn unique_name(base: &str, taken: &BTreeMap<String, CliProfile>) -> String {
    if !taken.contains_key(base) {
        return base.to_string();
    }
    let mut n = c::profiles::FIRST_SUFFIX;
    while taken.contains_key(&format!("{base}-{n}")) {
        n += 1;
    }
    format!("{base}-{n}")
}

// ---------------------------------------------------------------- commands

pub fn list_command(ctx: &Context) -> Result<()> {
    let config = read_config(&ctx.home)?;
    if config.profiles.is_empty() {
        println!("{}", messages::PROFILES_EMPTY);
        return Ok(());
    }
    for (name, profile) in &config.profiles {
        let mark = if config.default.as_deref() == Some(name) { "*" } else { " " };
        let hosted = profile.api_url == names::api_url::DEFAULT;
        let details: Vec<&str> =
            [profile.org_name.as_deref(), profile.email.as_deref(), (!hosted).then_some(profile.api_url.as_str())]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect();
        let row = fill(
            messages::PROFILES_ROW,
            &[("mark", mark), ("name", &format!("{name:<16}")), ("details", &details.join(" · "))],
        );
        println!("{row}");
    }
    Ok(())
}

pub fn use_command(ctx: &Context) -> Result<()> {
    let name = ctx.operand(1, "use")?;
    let mut config = read_config(&ctx.home)?;
    if !config.profiles.contains_key(&name) {
        return Err(CliError(fill(errors::PROFILE_UNKNOWN, &[("name", &name)])));
    }
    config.default = Some(name.clone());
    write_config(&ctx.home, &config)?;
    println!("{}", fill(messages::USE_DONE, &[("name", &name)]));
    Ok(())
}

pub fn logout_command(ctx: &Context) -> Result<()> {
    let name = ctx.operand(1, "logout")?;
    let mut config = read_config(&ctx.home)?;

    // 1. forget it; a default that went with it passes to the next profile, if any
    if config.profiles.remove(&name).is_none() {
        return Err(CliError(fill(errors::PROFILE_UNKNOWN, &[("name", &name)])));
    }
    if config.default.as_deref() == Some(name.as_str()) {
        config.default = config.profiles.keys().next().cloned();
    }
    write_config(&ctx.home, &config)?;

    // 2. the token itself still works until revoked, and only a person can revoke it
    println!("{}", fill(messages::LOGOUT_DONE, &[("name", &name)]));
    Ok(())
}

pub fn whoami_command(ctx: &Context) -> Result<()> {
    let profile = connect(ctx)?;
    let reach = profile.api().whoami()?;
    let api = if profile.api_url == names::api_url::DEFAULT {
        String::new()
    } else {
        fill(messages::WHOAMI_API, &[("apiUrl", &profile.api_url)])
    };
    println!("{}", fill(messages::WHOAMI_PROFILE, &[("name", &profile.name), ("api", &api)]));
    println!("{}", fill(messages::WHOAMI_WORKSPACE, &[("org", &reach.org.name)]));
    let scope =
        if reach.scope == "project" { messages::WHOAMI_REACH_PROJECT } else { messages::WHOAMI_REACH_WORKSPACE };
    println!("{}", fill(messages::WHOAMI_REACH, &[("reach", scope)]));
    for project in &reach.projects {
        let bound =
            if profile.project_id.as_deref() == Some(project.id.as_str()) { messages::WHOAMI_BOUND } else { "" };
        println!(
            "{}",
            fill(messages::WHOAMI_PROJECT, &[("id", &project.id), ("name", &project.name), ("bound", bound)])
        );
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    /// The pinned conformance file every Jot CLI implementation runs.
    pub fn conformance() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/conformance/cli.json");
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// `{ message: key, params }` → the contract text it names, filled.
    pub fn error_text(error: &Value) -> String {
        let contract: Value = serde_json::from_str(
            &fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/contract/contract.json")).unwrap(),
        )
        .unwrap();
        let key = error["message"].as_str().unwrap();
        let template = contract["agent"]["errors"][key]
            .as_str()
            .or_else(|| contract["cli"]["copy"]["messages"][key].as_str())
            .unwrap_or_else(|| panic!("no contract text {key}"));
        let params: Vec<(String, String)> = error
            .get("params")
            .and_then(Value::as_object)
            .map(|params| params.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect())
            .unwrap_or_default();
        let vars: Vec<(&str, &str)> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        fill(template, &vars)
    }

    /// A fresh, empty directory under the system temp dir.
    pub fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("jot-cli-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn conformance_profile_resolution() {
        let all = conformance();
        for case in all["profileResolution"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let config = config_from(case.get("config").unwrap_or(&all["config"]));
            let env = Env(case["env"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect());
            let binding = case["binding"].as_object().map(|_| binding_from(&case["binding"]));
            let result =
                pick(&PickInput { flag: case["flag"].as_str(), env: &env, binding: binding.as_ref(), config: &config });

            if let Some(error) = case.get("error") {
                assert_eq!(result.unwrap_err().0, error_text(error), "{name}");
                continue;
            }
            let got = result.unwrap().map(|r| {
                let mut out = json!({ "name": r.name, "apiUrl": r.api_url, "token": r.token });
                if let Some(project) = r.project_id {
                    out["projectId"] = json!(project);
                }
                out
            });
            assert_eq!(got.unwrap_or(Value::Null), case["expect"], "{name}");
        }
    }

    #[test]
    fn conformance_config_file() {
        for case in conformance()["configFile"].as_array().unwrap() {
            let config = config_from(&case["file"]);
            assert_eq!(serde_json::to_value(&config).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn conformance_walk_up() {
        for case in conformance()["walkUp"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let root = temp_dir("walk");
            for (path, content) in case["files"].as_object().unwrap() {
                let file = root.join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(&file, content.to_string()).unwrap();
            }
            let cwd = root.join(case["cwd"].as_str().unwrap());
            fs::create_dir_all(&cwd).unwrap();
            let found = find_binding(&cwd).unwrap().map(|(path, binding)| {
                json!({ "path": path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"), "binding": binding })
            });
            assert_eq!(found.unwrap_or(Value::Null), case["expect"], "{name}");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn config_round_trips_privately() {
        let home = temp_dir("config").join("nested-home");
        let mut config = CliConfig { default: Some("a".into()), ..CliConfig::default() };
        config.profiles.insert(
            "a".into(),
            CliProfile { api_url: "https://api.usejot.co".into(), token: "t".into(), org_name: None, email: None },
        );
        write_config(&home, &config).unwrap();
        assert_eq!(read_config(&home).unwrap(), config);
        let text = fs::read_to_string(home.join(paths::CONFIG)).unwrap();
        assert!(text.starts_with("{\n  \"default\": \"a\",") && text.ends_with("}\n"), "{text}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(home.join(paths::CONFIG)).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::metadata(&home).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn names_profiles_like_the_prototype() {
        assert_eq!(slugify("Acme Inc."), "acme-inc");
        assert_eq!(slugify("!!!"), "default");
        let mut taken = BTreeMap::new();
        taken.insert("acme".to_string(), CliProfile::default());
        taken.insert("acme-2".to_string(), CliProfile::default());
        assert_eq!(unique_name("acme", &taken), "acme-3");
        assert_eq!(unique_name("other", &taken), "other");
    }
}
