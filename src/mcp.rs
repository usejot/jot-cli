//! `jot mcp`: the Jot MCP server, over stdio. JSON-RPC 2.0, one message per
//! line. It serves the contract's six tools (names, descriptions, input and
//! output schemas, annotations) and behaves as `@usejot/mcp`'s tools.ts does,
//! so the two servers are interchangeable: a test holds those six in
//! `tools/list` to the 0.8.0 snapshot. After them come the CLI's own session
//! tools (`cli::mcpSessionTools`): `connect`, `profiles` and `use_profile`.
//!
//! A thin client of the API, like the TS server: no rule of its own. stdout
//! is the protocol's alone; anything else goes to stderr.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::api::{Api, ListQuery};
use crate::contract::agent::{self, errors, mcp as copy};
use crate::contract::cli::{copy::messages, env as names};
use crate::contract::constants::{AGENT_REQUESTABLE_STATUSES, MCP_TOOL_NAMES};
use crate::contract::{mcp as surface, AgentWhoamiResponse};
use crate::init::{duplicate_warning, other_jot_server, plugin_installed, ClaudePaths};
use crate::login::{begin_sign_in, open_browser, SignInInput};
use crate::profiles::{find_binding, pick, read_config, resolve, Env, PickInput, Resolved};
use crate::text::{fill, where_of};
use crate::{CliError, Context, Result, VERSION};

/// The MCP protocol versions this server speaks, newest first.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC error codes (the spec's).
const METHOD_NOT_FOUND: i64 = -32601;
const PARSE_ERROR: i64 = -32700;

/// A tool's answer: MCP `content`, optional `structuredContent`, and whether it failed.
struct ToolResult {
    content: Vec<Value>,
    structured: Option<Value>,
    is_error: bool,
}

impl ToolResult {
    fn text(text: String, structured: Option<Value>) -> Self {
        ToolResult { content: vec![json!({ "type": "text", "text": text })], structured, is_error: false }
    }

    fn error(message: String) -> Self {
        ToolResult { content: vec![json!({ "type": "text", "text": message })], structured: None, is_error: true }
    }

    fn into_json(self) -> Value {
        let mut out = json!({ "content": self.content });
        if let Some(structured) = self.structured {
            out["structuredContent"] = structured;
        }
        if self.is_error {
            out["isError"] = json!(true);
        }
        out
    }
}

// ---------------------------------------------------------------- the tool list

fn parse(raw: &str) -> Map<String, Value> {
    // the generated contract is valid JSON by construction; an empty map is never served
    serde_json::from_str(raw).unwrap_or_default()
}

/// The input schemas as served: the contract's, with update_status taking every
/// status an agent may request (the API applies the workspace's approval policy),
/// as tools.ts does.
fn input_schemas() -> Map<String, Value> {
    let mut schemas = parse(surface::INPUT_SCHEMAS);
    if let Some(status) =
        schemas.get_mut("update_status").and_then(|schema| schema.pointer_mut("/properties/status/enum"))
    {
        *status = json!(AGENT_REQUESTABLE_STATUSES);
    }
    schemas
}

/// `tools/list`: the six shared tools in the contract's order, each with its
/// guide and schemas, then the CLI's session tools in theirs.
pub fn tool_list() -> Vec<Value> {
    // 1. the shared six, exactly as @usejot/mcp serves them
    let inputs = input_schemas();
    let outputs = parse(surface::OUTPUT_SCHEMAS);
    let guides = parse(surface::TOOLS);
    let mut tools: Vec<Value> = MCP_TOOL_NAMES
        .iter()
        .map(|name| {
            let guide = &guides[*name];
            let mut tool = json!({
                "name": name,
                "title": guide["title"],
                "description": guide["mcpDescription"],
                "inputSchema": inputs[*name],
                "annotations": guide["annotations"],
            });
            if let Some(output) = outputs.get(*name) {
                tool["outputSchema"] = output.clone();
            }
            tool
        })
        .collect();

    // 2. this CLI's own: sign in and switch accounts without leaving the agent
    let session = parse(surface::SESSION_TOOLS);
    for name in surface::SESSION_TOOL_NAMES {
        let guide = &session[*name];
        tools.push(json!({
            "name": name,
            "title": guide["title"],
            "description": guide["mcpDescription"],
            "inputSchema": guide["inputSchema"],
            "annotations": guide["annotations"],
        }));
    }
    tools
}

/// The input schema a tool is served with, shared or session; None for an unknown tool.
fn schema_for(name: &str) -> Option<Value> {
    if let Some(schema) = input_schemas().get(name) {
        return Some(schema.clone());
    }
    parse(surface::SESSION_TOOLS).get(name).map(|tool| tool["inputSchema"].clone())
}

// ---------------------------------------------------------------- input validation

/// The JSON Schema subset the contract's tool inputs use. None: valid.
pub fn invalid(schema: &Value, value: &Value, at: &str) -> Option<String> {
    let fail = |why: &str| Some(format!("{}: {why}", if at.is_empty() { "input" } else { at }));

    // 1. the type
    let typed = match schema.get("type").and_then(Value::as_str) {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        _ => true,
    };
    if !typed {
        return fail(&format!("expected {}", schema["type"].as_str().unwrap_or("a value")));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return fail(&format!("expected one of {}", Value::Array(allowed.clone())));
        }
    }

    // 2. bounds
    let number = |key: &str| schema.get(key).and_then(Value::as_f64);
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as f64;
        if number("minLength").is_some_and(|min| length < min) || number("maxLength").is_some_and(|max| length > max) {
            return fail("wrong length");
        }
    }
    if let Some(n) = value.as_f64() {
        if number("minimum").is_some_and(|min| n < min) || number("maximum").is_some_and(|max| n > max) {
            return fail("out of range");
        }
    }
    if let Some(items) = value.as_array() {
        let count = items.len() as f64;
        if number("minItems").is_some_and(|min| count < min) || number("maxItems").is_some_and(|max| count > max) {
            return fail("wrong number of items");
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                if let Some(why) = invalid(item_schema, item, &format!("{at}[{index}]")) {
                    return Some(why);
                }
            }
        }
    }

    // 3. an object's properties
    let object = value.as_object()?;
    let properties = schema.get("properties").and_then(Value::as_object);
    for key in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if !object.contains_key(key) {
            return fail(&format!("{key} is required"));
        }
    }
    for (key, item) in object {
        let path = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
        match properties.and_then(|properties| properties.get(key)) {
            Some(property) => {
                if let Some(why) = invalid(property, item, &path) {
                    return Some(why);
                }
            }
            None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                return fail(&format!("unknown field {key}"));
            }
            None => {}
        }
    }
    None
}

// ---------------------------------------------------------------- the tools

/// The projects a token reaches, said in one line (contract `agent.mcp`).
pub fn describe_reach(reach: &AgentWhoamiResponse) -> String {
    let projects: Vec<String> = reach
        .projects
        .iter()
        .map(|project| fill(copy::REACH_PROJECT_ITEM, &[("name", &project.name), ("id", &project.id)]))
        .collect();
    let template = if reach.scope == "project" { copy::REACH_PROJECT } else { copy::REACH_WORKSPACE };
    fill(template, &[("projects", &projects.join(", ")), ("org", &reach.org.name)])
}

/// What the client shows the model: the reach and the repo's project first, then the playbook.
pub fn instructions(reach: Option<&AgentWhoamiResponse>, bound_project: Option<&str>) -> String {
    let mut lines = vec![match reach {
        Some(reach) => fill(copy::SCOPE_KNOWN, &[("reach", &describe_reach(reach))]),
        None => copy::SCOPE_UNKNOWN.to_string(),
    }];
    if let Some(project) = bound_project {
        lines.push(fill(copy::BOUND_PROJECT, &[("projectId", project)]));
    }
    lines.push(String::new());
    lines.push(agent::PLAYBOOK.to_string());
    lines.join("\n")
}

/// A string argument, when given.
fn arg<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

/// The comment a call names: `commentId`, or its alias `id`.
fn comment_id(input: &Value) -> Result<String> {
    arg(input, "commentId")
        .or(arg(input, "id"))
        .map(str::to_string)
        .ok_or_else(|| CliError(errors::MISSING_COMMENT_ID.to_string()))
}

/// The note a call carries: `message`, or its aliases `body` / `content`.
fn message(input: &Value) -> Result<String> {
    arg(input, "message")
        .or(arg(input, "body"))
        .or(arg(input, "content"))
        .map(str::to_string)
        .ok_or_else(|| CliError(errors::MISSING_MESSAGE.to_string()))
}

/// The screenshot the markdown links: the first `![Screenshot](<url>)` with no space in the url.
fn screenshot_url(markdown: &str) -> Option<&str> {
    const MARK: &str = "![Screenshot](";
    let mut rest = markdown;
    while let Some(at) = rest.find(MARK) {
        let after = &rest[at + MARK.len()..];
        let end = after.find(|ch: char| ch == ')' || ch.is_whitespace());
        if let Some(end) = end.filter(|&end| end > 0 && after[end..].starts_with(')')) {
            return Some(&after[..end]);
        }
        rest = after;
    }
    None
}

/// The profile a session talks to, its client, and what its token reaches.
struct Connection {
    profile: Resolved,
    api: Api,
    /// None when whoami failed at connect time: the tools still run and report the API's error.
    reach: Option<AgentWhoamiResponse>,
}

impl Connection {
    fn open(profile: Resolved) -> Self {
        let api = profile.api();
        let reach = api.whoami().ok();
        Connection { profile, api, reach }
    }
}

/// How a background `connect` ended: the profile it saved, or why it did not.
type SignInOutcome = std::result::Result<String, String>;

/// Where the server finds its profiles, captured once (tests point it at temp dirs).
pub struct ServerInput {
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub env: Env,
    /// `--profile` on `jot mcp`.
    pub flag: Option<String>,
}

pub struct Server {
    input: ServerInput,
    /// The profile `use_profile` or a finished `connect` chose; it outranks every other source.
    session_profile: Option<String>,
    /// None while nothing is signed in: the server still serves, and every tool
    /// says how to connect. Resolved again on each call until it is Some.
    connection: Option<Connection>,
    /// Why the last resolution failed (a profile named that this machine lacks).
    unresolved: Option<String>,
    /// Filled by the `connect` thread when its sign-in ends; taken on the next call.
    signed_in: Arc<Mutex<Option<SignInOutcome>>>,
}

impl Server {
    /// A server that has resolved nothing yet; `refresh` does that.
    pub fn new(input: ServerInput) -> Self {
        Server { input, session_profile: None, connection: None, unresolved: None, signed_in: Arc::default() }
    }

    fn api(&self) -> Result<&Api> {
        match (&self.connection, &self.unresolved) {
            (Some(connection), _) => Ok(&connection.api),
            (None, Some(why)) => Err(CliError(why.clone())),
            (None, None) => Err(CliError(messages::MCP_NOT_CONNECTED.to_string())),
        }
    }

    /// The profile resolution of every command, from this server's files and env.
    /// `flag` outranks the session's choice, which outranks `--profile`.
    fn pick(&self, flag: Option<&str>) -> Result<Option<Resolved>> {
        let binding = find_binding(&self.input.cwd)?.map(|(_, binding)| binding);
        let config = read_config(&self.input.home)?;
        let flag = flag.or(self.session_profile.as_deref()).or(self.input.flag.as_deref());
        pick(&PickInput { flag, env: &self.input.env, binding: binding.as_ref(), config: &config })
    }

    /// Before every call: take a finished `connect`, and resolve again while
    /// not connected, so a sign-in finished anywhere (here, or `jot login` in a
    /// terminal) is used with no restart. Some: a line to put before the answer.
    fn refresh(&mut self) -> Option<String> {
        // 1. a background connect ended: its profile is this session's now
        let outcome = self.signed_in.lock().ok().and_then(|mut slot| slot.take());
        let mut connected_as = None;
        match outcome {
            Some(Ok(name)) => {
                self.session_profile = Some(name.clone());
                self.connection = None;
                connected_as = Some(name);
            }
            Some(Err(message)) => return Some(fill(messages::MCP_CONNECT_FAILED, &[("message", &message)])),
            None => {}
        }

        // 2. not connected: resolve again
        if self.connection.is_none() {
            match self.pick(None) {
                Ok(profile) => {
                    self.unresolved = None;
                    self.connection = profile.map(Connection::open);
                }
                Err(error) => self.unresolved = Some(error.0),
            }
        }

        // 3. say so once, with what the new profile reaches
        let name = connected_as?;
        let reach = self.connection.as_ref().and_then(|c| c.reach.as_ref()).map(describe_reach).unwrap_or_default();
        Some(fill(messages::MCP_CONNECTED, &[("name", &name), ("reach", &reach)]))
    }

    fn fetch_feedback(&self, input: &Value) -> Result<ToolResult> {
        // 1. the query, in the order the TS client sends it
        let mut params: Vec<(&'static str, String)> = Vec::new();
        for key in ["status", "page", "environment"] {
            if let Some(value) = arg(input, key) {
                params.push((key, value.to_string()));
            }
        }
        if let Some(sources) = input.get("source").and_then(Value::as_array) {
            let joined: Vec<&str> = sources.iter().filter_map(Value::as_str).collect();
            params.push(("source", joined.join(",")));
        }
        for key in ["q", "label"] {
            if let Some(value) = arg(input, key) {
                params.push((key, value.to_string()));
            }
        }
        if let Some(limit) = input.get("limit").and_then(Value::as_u64) {
            params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = arg(input, "cursor") {
            params.push(("cursor", cursor.to_string()));
        }
        let list = self
            .api()?
            .list_comments(&ListQuery { project_id: arg(input, "projectId").map(str::to_string), params })?;

        // 2. nothing there
        if list.comments.is_empty() {
            return Ok(ToolResult::text(
                copy::NO_COMMENTS.to_string(),
                Some(json!({ "comments": [], "nextCursor": null })),
            ));
        }

        // 3. one block per comment, and the same rows structured
        let mut blocks = Vec::new();
        let mut rows = Vec::new();
        for comment in &list.comments {
            let labels: Vec<String> = comment.labels.iter().map(|entry| entry.label.to_lowercase()).collect();
            let place = where_of(&comment.context);
            let tagged = if labels.is_empty() { String::new() } else { format!(" ({})", labels.join(", ")) };
            let mut block = vec![
                format!("{}  [{}] {}{tagged}", comment.id, comment.status, comment.source.to_lowercase()),
                format!("  {}", comment.body),
                format!("  {place}"),
            ];
            let mut row = json!({ "id": comment.id, "status": comment.status, "source": comment.source, "labels": labels, "body": comment.body, "where": place });
            if let Some(project) = &comment.project {
                block.push(format!("  project: {} ({})", project.name, project.id));
                row["project"] = json!({ "id": project.id, "name": project.name });
            }
            blocks.push(block.join("\n"));
            rows.push(row);
        }
        let more = list
            .next_cursor
            .as_deref()
            .map(|cursor| format!("\n\n{}", fill(copy::MORE_COMMENTS, &[("cursor", cursor)])))
            .unwrap_or_default();
        Ok(ToolResult::text(
            blocks.join("\n\n") + &more,
            Some(json!({ "comments": rows, "nextCursor": list.next_cursor })),
        ))
    }

    fn get_comment(&self, input: &Value) -> Result<ToolResult> {
        let id = comment_id(input)?;
        let markdown = self.api()?.markdown(&id)?;
        let detail = self.api()?.get_comment(&id).ok();

        // 1. the readable repro, with where a person can open it
        let project = detail.as_ref().and_then(|detail| detail.get("projectId")).and_then(Value::as_str);
        let link = self
            .connection
            .as_ref()
            .and_then(|connection| connection.reach.as_ref())
            .zip(project)
            .map(|(reach, project)| format!("{}/projects/{project}/comments/{id}", reach.dashboard_url));
        let text = match link {
            Some(link) => format!("{markdown}\n\n{}", fill(copy::OPEN_IN_JOT, &[("link", &link)])),
            None => markdown.clone(),
        };
        let mut result = ToolResult::text(text, None);

        // 2. the screenshot itself, so a model that sees images needs no fetch of its own
        if let Some(image) = screenshot_url(&markdown).and_then(|url| self.api().ok().and_then(|api| api.image(url))) {
            result.content.push(json!({ "type": "image", "data": image.base64, "mimeType": image.mime_type }));
        }
        Ok(result)
    }

    fn update_status(&self, input: &Value) -> Result<ToolResult> {
        let ids: Vec<String> = match input.get("commentIds").and_then(Value::as_array) {
            Some(ids) => ids.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            None => vec![comment_id(input)?],
        };
        let status = arg(input, "status").unwrap_or_default();
        let mut said = Vec::new();
        let mut updated = Vec::new();
        for id in &ids {
            let row = self.api()?.update_status(id, status)?;
            said.push(fill(copy::STATUS_UPDATED, &[("id", &row.id), ("status", &row.status.to_lowercase())]));
            updated.push(json!({ "id": row.id, "status": row.status }));
        }
        Ok(ToolResult::text(said.join("\n"), Some(json!({ "updated": updated }))))
    }

    fn post_reply(&self, input: &Value) -> Result<ToolResult> {
        self.api()?.reply(&comment_id(input)?, &message(input)?)?;
        Ok(ToolResult::text(copy::REPLY_POSTED.to_string(), None))
    }

    fn create_batch_note(&self, input: &Value) -> Result<ToolResult> {
        let ids: Vec<String> = input["comment_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let created = self.api()?.batch_note(&ids, arg(input, "message").unwrap_or_default())?;
        let reply_ids = created.get("replyIds").cloned().unwrap_or(json!([]));
        let count = reply_ids.as_array().map_or(0, Vec::len).to_string();
        Ok(ToolResult::text(
            fill(copy::BATCH_NOTE_POSTED, &[("count", &count)]),
            Some(json!({ "replyIds": reply_ids })),
        ))
    }

    fn list_projects(&self) -> Result<ToolResult> {
        let raw = self.api()?.whoami_raw()?;
        let reach: AgentWhoamiResponse = serde_json::from_value(raw.clone())?;
        Ok(ToolResult::text(describe_reach(&reach), Some(raw)))
    }

    // ------------------------------------------------ session tools (this CLI only)

    /// `connect`: the `jot login` handshake, started in the background. It
    /// answers at once with the link, so neither the server nor the client's
    /// tool timeout waits the up-to-5 minutes a person may take in the browser.
    fn connect(&self, input: &Value) -> Result<ToolResult> {
        // 1. the loopback listener and the dashboard URL, as `jot login` makes them
        let env = &self.input.env;
        let pending = begin_sign_in(SignInInput {
            home: self.input.home.clone(),
            api_url: env.get(names::api_url::NAME).unwrap_or(names::api_url::DEFAULT).to_string(),
            app_url: env.get(names::app_url::NAME).unwrap_or(names::app_url::DEFAULT).to_string(),
            wanted: arg(input, "profile").map(str::to_string),
        })?;
        let url = pending.url.clone();
        open_browser(&url);

        // 2. a thread waits for the browser and saves the profile; it never writes to stdout
        let slot = Arc::clone(&self.signed_in);
        std::thread::spawn(move || {
            let outcome: SignInOutcome = pending.finish().map(|signed| signed.name).map_err(|error| error.0);
            match &outcome {
                Ok(name) => eprintln!("{}", fill(messages::MCP_CONNECTED, &[("name", name), ("reach", "")]).trim_end()),
                Err(message) => eprintln!("{}", fill(messages::MCP_CONNECT_FAILED, &[("message", message)])),
            }
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(outcome);
            }
        });
        Ok(ToolResult::text(fill(messages::MCP_CONNECT_OPENED, &[("url", &url)]), None))
    }

    /// `profiles`: the saved profiles, which one this session uses and which is
    /// the default. Built field by field: a token is never part of the answer.
    fn profiles(&self) -> Result<ToolResult> {
        // 1. none yet
        let config = read_config(&self.input.home)?;
        if config.profiles.is_empty() {
            return Ok(ToolResult::text(messages::MCP_PROFILES_EMPTY.to_string(), Some(json!({ "profiles": [] }))));
        }

        // 2. one line and one row each
        let in_use = self.connection.as_ref().map(|connection| connection.profile.name.as_str());
        let mut lines = Vec::new();
        let mut rows = Vec::new();
        for (name, profile) in &config.profiles {
            let active = in_use == Some(name.as_str());
            let default = config.default.as_deref() == Some(name.as_str());
            let mut marks = String::new();
            if active {
                marks.push_str(messages::MCP_PROFILE_IN_USE);
            }
            if default {
                marks.push_str(messages::MCP_PROFILE_DEFAULT);
            }
            let details: Vec<&str> = [profile.org_name.as_deref(), profile.email.as_deref(), Some(&profile.api_url)]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect();
            let row = [("name", name.as_str()), ("marks", &marks), ("details", &details.join(" · "))];
            lines.push(fill(messages::MCP_PROFILES_ROW, &row));
            rows.push(json!({
                "name": name,
                "org": profile.org_name,
                "email": profile.email,
                "apiUrl": profile.api_url,
                "inUse": active,
                "default": default,
            }));
        }
        lines.push(String::new());
        lines.push(messages::MCP_PROFILES_HINT.to_string());
        Ok(ToolResult::text(lines.join("\n"), Some(json!({ "profiles": rows }))))
    }

    /// `use_profile`: this session only, as `--profile` would; the default stays.
    /// The instructions cannot be sent again, so the answer says the new reach.
    fn use_profile(&mut self, input: &Value) -> Result<ToolResult> {
        // 1. a profile this machine has
        let name = arg(input, "name").unwrap_or_default().to_string();
        if !read_config(&self.input.home)?.profiles.contains_key(&name) {
            return Err(CliError(fill(errors::PROFILE_UNKNOWN, &[("name", &name)])));
        }
        let profile = self.pick(Some(&name))?.ok_or_else(|| CliError(messages::MCP_NOT_CONNECTED.to_string()))?;

        // 2. its token must work before the session moves to it
        let api = profile.api();
        let raw = api.whoami_raw()?;
        let reach: AgentWhoamiResponse = serde_json::from_value(raw.clone())?;
        let said = fill(messages::MCP_USE_PROFILE_DONE, &[("name", &name), ("reach", &describe_reach(&reach))]);

        // 3. switched
        self.session_profile = Some(name);
        self.unresolved = None;
        self.connection = Some(Connection { profile, api, reach: Some(reach) });
        Ok(ToolResult::text(said, Some(raw)))
    }

    /// `tools/call`: the input checked against the served schema, then the tool.
    /// A failure is a tool error the agent can read and act on, never a crash.
    fn call(&mut self, name: &str, input: &Value) -> ToolResult {
        // 1. a known tool, with a valid input
        let Some(schema) = schema_for(name) else {
            return ToolResult::error(format!("Tool {name} not found"));
        };
        if let Some(why) = invalid(&schema, input, "") {
            return ToolResult::error(format!("Input validation error: {why}"));
        }

        // 2. the profile, picked up again if a sign-in finished since the last call
        let notice = self.refresh();

        // 3. the tool
        let result = match name {
            "fetch_feedback" => self.fetch_feedback(input),
            "get_comment" => self.get_comment(input),
            "update_status" => self.update_status(input),
            "post_reply" => self.post_reply(input),
            "create_batch_note" => self.create_batch_note(input),
            "list_projects" => self.list_projects(),
            "connect" => self.connect(input),
            "profiles" => self.profiles(),
            _ => self.use_profile(input),
        };
        let mut answer = result.unwrap_or_else(|error| {
            ToolResult::error(if error.0.is_empty() { copy::UNREACHABLE.to_string() } else { error.0 })
        });

        // 4. a finished connect is said first
        if let Some(notice) = notice {
            answer.content.insert(0, json!({ "type": "text", "text": notice }));
        }
        answer
    }
}

// ---------------------------------------------------------------- JSON-RPC

/// The answer to one message, or None for a notification.
pub fn handle(server: &mut Server, instructions: &str, message: &Value) -> Option<Value> {
    // 1. notifications (no id) need no answer
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(json!({}));

    // 2. the methods a tools-only server answers
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = PROTOCOL_VERSIONS.iter().find(|known| **known == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": copy::SERVER_NAME, "version": VERSION },
                "instructions": instructions,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_list() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let input = params.get("arguments").cloned().unwrap_or(json!({}));
            server.call(name, &input).into_json()
        }
        _ => {
            let error = json!({ "code": METHOD_NOT_FOUND, "message": format!("Method not found: {method}") });
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": error }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// `jot mcp doctor`, as `jot-mcp doctor`: the token, the API and the reach
/// checked, and a ready-to-paste config printed.
fn doctor(profile: &Resolved) -> Result<()> {
    // 1. the API answers, and takes the token
    let reach = profile.api().whoami().map_err(|error| {
        let hint = if profile.is_legacy() { errors::DOCTOR_LEGACY_HINT } else { errors::DOCTOR_PROFILE_HINT };
        let hint = fill(hint, &[("apiUrl", &profile.api_url), ("name", &profile.name)]);
        CliError(format!("{}\n{hint}", error.0))
    })?;
    println!("{}", fill(copy::DOCTOR_REACHABLE, &[("apiUrl", &profile.api_url)]));
    let which =
        if profile.is_legacy() { String::new() } else { fill(copy::DOCTOR_PROFILE, &[("name", &profile.name)]) };
    println!("{}", fill(copy::DOCTOR_ACCEPTED, &[("which", &which), ("reach", &describe_reach(&reach))]));

    // 2. the config: a saved profile needs no env; the legacy env token still does
    let mut entry = crate::init::server_entry(crate::init::jot_on_path(crate::contract::cli::packages::BIN));
    if profile.is_legacy() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(names::api_token::NAME.to_string(), copy::DOCTOR_TOKEN_PLACEHOLDER.to_string());
        if profile.api_url != names::api_url::DEFAULT {
            env.insert(names::api_url::NAME.to_string(), profile.api_url.clone());
        }
        entry.env = Some(env);
    }
    println!("{}", copy::DOCTOR_PASTE);
    println!("{}", serde_json::to_string_pretty(&json!({ "mcpServers": { "jot": entry } }))?);
    println!("{}", copy::DOCTOR_RESTART);
    Ok(())
}

/// Another `jot` server besides the one asking, as a warning line; None when there is none.
fn duplicate(claude: &ClaudePaths, cwd: &std::path::Path) -> Option<String> {
    other_jot_server(claude, cwd).map(|file| duplicate_warning(&file))
}

pub fn serve(ctx: &Context) -> Result<()> {
    let env = Env::from_process();
    let cwd = std::env::current_dir()?;
    let claude = ClaudePaths::from_env(&env);

    // 1. `jot mcp doctor`: the plugin's server should be the only jot server, then the token
    if ctx.args.positionals.get(1).map(String::as_str) == Some("doctor") {
        if let Some(warning) = duplicate(&claude, &cwd).filter(|_| plugin_installed(&claude)) {
            println!("{warning}");
        }
        let profile = resolve(ctx)?.ok_or_else(|| CliError(errors::NOT_CONNECTED.to_string()))?;
        return doctor(&profile);
    }

    // 2. started by the plugin while another jot server is configured: two would load
    let via_plugin = env.get(names::via_plugin::NAME) == Some("1");
    let warning = duplicate(&claude, &cwd).filter(|_| via_plugin);
    if let Some(warning) = &warning {
        eprintln!("{warning}");
    }

    // 3. the profile, by the same order as every other command. Not signed in:
    //    still serve (a plugin install must not show "failed to connect"), and
    //    say how to connect, up front and from every tool
    let input = ServerInput { home: ctx.home.clone(), cwd, env, flag: ctx.args.value("profile").map(str::to_string) };
    let mut server = Server::new(input);
    server.refresh();
    let mut lines: Vec<String> = warning.into_iter().collect();
    match &server.connection {
        Some(connection) => {
            // what this token reaches, said up front (best effort: an unreachable API still serves)
            let reach = connection.reach.as_ref();
            lines.push(instructions(reach, connection.profile.project_id.as_deref()));
            let scope = reach.map(|reach| format!(" · {}", describe_reach(reach))).unwrap_or_default();
            eprintln!("jot mcp {VERSION} ready (api: {}){scope}", connection.profile.api_url);
        }
        None => {
            lines.push(server.unresolved.clone().unwrap_or_else(|| messages::MCP_NOT_CONNECTED.to_string()));
            lines.push(String::new());
            lines.push(instructions(None, None));
            eprintln!("jot mcp {VERSION} ready, not signed in");
        }
    }
    answer_lines(&mut server, &lines.join("\n"))
}

/// One JSON-RPC message per line, until the client closes stdin.
fn answer_lines(server: &mut Server, instructions: &str) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(server, instructions, &message),
            Err(error) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": PARSE_ERROR, "message": error.to_string() } }),
            ),
        };
        if let Some(answer) = answer {
            writeln!(stdout, "{answer}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CliConfig, CliProfile, NamedRef};
    use crate::profiles::tests::temp_dir;
    use crate::profiles::write_config;
    use std::io::Read;
    use std::net::TcpListener;

    /// A server on a fresh home with no env: nothing from this machine leaks in.
    fn server_in(home: &std::path::Path) -> Server {
        Server::new(ServerInput { home: home.into(), cwd: home.into(), env: Env::default(), flag: None })
    }

    /// An API on the loopback host whose whoami says `org` (any path answers it).
    fn fake_api(org: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = json!({ "org": { "id": "o1", "name": org }, "scope": "workspace",
            "projects": [{ "id": "p1", "name": "Web" }], "dashboardUrl": "https://app.test" })
        .to_string();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut head = [0u8; 4096];
                let _ = stream.read(&mut head);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            }
        });
        url
    }

    /// Saves these profiles (name, apiUrl, token) with the first as the default.
    fn save(home: &std::path::Path, profiles: &[(&str, &str, &str)]) {
        let mut config =
            CliConfig { default: profiles.first().map(|(name, _, _)| name.to_string()), ..CliConfig::default() };
        for (name, api_url, token) in profiles {
            let profile = CliProfile {
                api_url: api_url.to_string(),
                token: token.to_string(),
                org_name: Some(format!("{name} org")),
                email: Some(format!("{name}@x.test")),
            };
            config.profiles.insert(name.to_string(), profile);
        }
        write_config(home, &config).unwrap();
    }

    fn call_tool(server: &mut Server, name: &str, arguments: Value) -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
        handle(server, "", &message).unwrap()["result"].clone()
    }

    fn text(result: &Value, index: usize) -> String {
        result["content"][index]["text"].as_str().unwrap_or_default().to_string()
    }

    fn vectors() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/conformance/mcp.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The contract's own input schemas (before the APPROVED widening), as the TS conformance test uses them.
    fn contract_schema(tool: &str) -> Value {
        parse(surface::INPUT_SCHEMAS)[tool].clone()
    }

    #[test]
    fn conformance_mcp_inputs() {
        let all = vectors();
        for case in all["valid"].as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            assert_eq!(invalid(&contract_schema(tool), &case["input"], ""), None, "{tool} {}", case["input"]);
        }
        for case in all["invalid"].as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            assert!(invalid(&contract_schema(tool), &case["input"], "").is_some(), "{}", case["name"]);
        }
    }

    #[test]
    fn serves_approved_as_a_request_the_api_decides() {
        let schema = &input_schemas()["update_status"];
        assert_eq!(invalid(schema, &json!({ "commentId": "c1", "status": "APPROVED" }), ""), None);
        assert!(invalid(schema, &json!({ "commentId": "c1", "status": "OPEN" }), "").is_some());
    }

    #[test]
    fn says_the_reach_like_the_ts_server() {
        let reach = AgentWhoamiResponse {
            org: NamedRef { id: "o1".into(), name: "Acme".into() },
            scope: "workspace".into(),
            projects: vec![
                NamedRef { id: "p1".into(), name: "Web".into() },
                NamedRef { id: "p2".into(), name: "App".into() },
            ],
            dashboard_url: "https://app.test".into(),
        };
        assert_eq!(describe_reach(&reach), "This token reaches every project in Acme: \"Web\" (p1), \"App\" (p2).");
        let text = instructions(Some(&reach), Some("p1"));
        assert!(text.starts_with("This token reaches every project in Acme"));
        assert!(text.contains("\nThis repo is bound to project p1: pass it"));
        assert!(text.ends_with(agent::PLAYBOOK));
    }

    #[test]
    fn finds_the_screenshot_in_the_markdown() {
        assert_eq!(screenshot_url("a ![Screenshot](https://x.test/s.png) b"), Some("https://x.test/s.png"));
        assert_eq!(screenshot_url("![Screenshot](bad url) ![Screenshot](https://ok)"), Some("https://ok"));
        assert_eq!(screenshot_url("![Screenshot]()"), None);
    }

    #[test]
    fn answers_unknown_methods_and_ignores_notifications() {
        let home = temp_dir("mcp-methods");
        save(&home, &[("a", "http://127.0.0.1:9", "t")]);
        let mut server = server_in(&home);
        assert_eq!(handle(&mut server, "", &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })), None);
        let answer =
            handle(&mut server, "", &json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" })).unwrap();
        assert_eq!(answer["error"]["code"], METHOD_NOT_FOUND);
        let refused = call_tool(&mut server, "post_reply", json!({ "commentId": "c1" }));
        assert_eq!(refused["isError"], true);
        assert_eq!(text(&refused, 0), errors::MISSING_MESSAGE);
    }

    #[test]
    fn serves_when_not_signed_in_and_every_tool_says_how_to_connect() {
        let home = temp_dir("mcp-none");
        let mut server = server_in(&home);
        let listed = handle(&mut server, "", &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let names: Vec<&str> =
            listed["result"]["tools"].as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
        let expected: Vec<&str> = MCP_TOOL_NAMES.iter().chain(surface::SESSION_TOOL_NAMES).copied().collect();
        assert_eq!(names, expected);

        let answered = call_tool(&mut server, "fetch_feedback", json!({}));
        assert_eq!(answered["isError"], true);
        assert_eq!(text(&answered, 0), messages::MCP_NOT_CONNECTED);
        let empty = call_tool(&mut server, "profiles", json!({}));
        assert_eq!(text(&empty, 0), messages::MCP_PROFILES_EMPTY);
    }

    #[test]
    fn profiles_never_include_a_token() {
        let home = temp_dir("mcp-profiles");
        save(
            &home,
            &[
                ("personal", "http://127.0.0.1:9", "tok_secret_personal"),
                ("work", "http://127.0.0.1:9", "tok_secret_work"),
            ],
        );
        let mut server = server_in(&home);
        let listed = call_tool(&mut server, "profiles", json!({}));

        // 1. no token anywhere in the answer, text or structured
        assert!(!listed.to_string().contains("tok_secret"), "{listed}");

        // 2. which one is in use, which is the default
        let said = text(&listed, 0);
        assert!(said.starts_with("- personal (this session) (default): personal org · personal@x.test"), "{said}");
        assert!(said.contains("\n- work: work org"), "{said}");
        let rows = listed["structuredContent"]["profiles"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1],
            json!({ "name": "work", "org": "work org", "email": "work@x.test",
            "apiUrl": "http://127.0.0.1:9", "inUse": false, "default": false })
        );
        fs_cleanup(&home);
    }

    #[test]
    fn use_profile_switches_this_session_only() {
        let home = temp_dir("mcp-use");
        save(&home, &[("personal", &fake_api("Personal"), "t1"), ("work", &fake_api("Work"), "t2")]);
        let mut server = server_in(&home);
        assert!(text(&call_tool(&mut server, "list_projects", json!({})), 0).contains("every project in Personal"));

        // 1. switched, and the answer says the new reach
        let switched = call_tool(&mut server, "use_profile", json!({ "name": "work" }));
        assert_eq!(switched["isError"], Value::Null, "{switched}");
        assert!(text(&switched, 0)
            .starts_with("This session now uses profile \"work\". This token reaches every project in Work"));
        assert!(text(&call_tool(&mut server, "list_projects", json!({})), 0).contains("every project in Work"));

        // 2. the default on disk is untouched
        assert_eq!(read_config(&home).unwrap().default.as_deref(), Some("personal"));

        // 3. an unknown name changes nothing
        let refused = call_tool(&mut server, "use_profile", json!({ "name": "nope" }));
        assert_eq!(text(&refused, 0), fill(errors::PROFILE_UNKNOWN, &[("name", "nope")]));
        assert!(text(&call_tool(&mut server, "list_projects", json!({})), 0).contains("in Work"));
        fs_cleanup(&home);
    }

    #[test]
    fn a_sign_in_finished_elsewhere_is_used_with_no_restart() {
        // 1. nothing yet
        let home = temp_dir("mcp-later");
        let mut server = server_in(&home);
        assert_eq!(text(&call_tool(&mut server, "list_projects", json!({})), 0), messages::MCP_NOT_CONNECTED);

        // 2. `jot login` in a terminal saves a profile: the next call uses it
        save(&home, &[("personal", &fake_api("Personal"), "t1")]);
        assert!(text(&call_tool(&mut server, "list_projects", json!({})), 0).contains("in Personal"));

        // 3. a `connect` finishes with another account: said first, and used from then on
        let mut config = read_config(&home).unwrap();
        let work = CliProfile { api_url: fake_api("Work"), token: "t2".into(), org_name: None, email: None };
        config.profiles.insert("work".into(), work);
        write_config(&home, &config).unwrap();
        *server.signed_in.lock().unwrap() = Some(Ok("work".into()));
        let answered = call_tool(&mut server, "list_projects", json!({}));
        assert!(text(&answered, 0).starts_with("Connected: profile \"work\". This token reaches every project in Work"));
        assert!(text(&answered, 1).contains("in Work"));
        assert!(text(&call_tool(&mut server, "list_projects", json!({})), 0).contains("in Work"));

        // 4. one that failed says why, once
        *server.signed_in.lock().unwrap() = Some(Err("Timed out".into()));
        let answered = call_tool(&mut server, "list_projects", json!({}));
        assert_eq!(text(&answered, 0), fill(messages::MCP_CONNECT_FAILED, &[("message", "Timed out")]));
        fs_cleanup(&home);
    }

    fn fs_cleanup(home: &std::path::Path) {
        let _ = std::fs::remove_dir_all(home);
    }
}
