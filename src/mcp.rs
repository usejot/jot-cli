//! `jot mcp`: the Jot MCP server, over stdio. JSON-RPC 2.0, one message per
//! line. It serves the contract's six tools (names, descriptions, input and
//! output schemas, annotations) and behaves as `@usejot/mcp`'s tools.ts does,
//! so the two servers are interchangeable: a test holds `tools/list` to the
//! 0.8.0 snapshot.
//!
//! A thin client of the API, like the TS server: no rule of its own.

use std::io::{BufRead, Write};

use serde_json::{json, Map, Value};

use crate::api::{Api, ListQuery};
use crate::contract::agent::{self, errors, mcp as copy};
use crate::contract::cli::env as names;
use crate::contract::constants::{AGENT_REQUESTABLE_STATUSES, MCP_TOOL_NAMES};
use crate::contract::{mcp as surface, AgentWhoamiResponse};
use crate::profiles::{resolve, Resolved};
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

/// `tools/list`: in the contract's order, each with its guide and schemas.
pub fn tool_list() -> Vec<Value> {
    let inputs = input_schemas();
    let outputs = parse(surface::OUTPUT_SCHEMAS);
    let guides = parse(surface::TOOLS);
    MCP_TOOL_NAMES
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
        .collect()
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

pub struct Server {
    /// None when nothing is signed in: the server still starts, and every tool
    /// answers with how to connect, so the agent can tell the person.
    api: Option<Api>,
    reach: Option<AgentWhoamiResponse>,
}

impl Server {
    fn api(&self) -> Result<&Api> {
        self.api.as_ref().ok_or_else(|| CliError(errors::NOT_CONNECTED.to_string()))
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
            .reach
            .as_ref()
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

    /// `tools/call`: the input checked against the served schema, then the tool.
    /// A failure is a tool error the agent can read and act on, never a crash.
    fn call(&self, name: &str, input: &Value) -> ToolResult {
        let Some(schema) = input_schemas().get(name).cloned() else {
            return ToolResult::error(format!("Tool {name} not found"));
        };
        if let Some(why) = invalid(&schema, input, "") {
            return ToolResult::error(format!("Input validation error: {why}"));
        }
        let result = match name {
            "fetch_feedback" => self.fetch_feedback(input),
            "get_comment" => self.get_comment(input),
            "update_status" => self.update_status(input),
            "post_reply" => self.post_reply(input),
            "create_batch_note" => self.create_batch_note(input),
            _ => self.list_projects(),
        };
        result.unwrap_or_else(|error| {
            ToolResult::error(if error.0.is_empty() { copy::UNREACHABLE.to_string() } else { error.0 })
        })
    }
}

// ---------------------------------------------------------------- JSON-RPC

/// The answer to one message, or None for a notification.
pub fn handle(server: &Server, instructions: &str, message: &Value) -> Option<Value> {
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

pub fn serve(ctx: &Context) -> Result<()> {
    // 1. the token: the same profiles and order as every other command
    let profile: Option<Resolved> = resolve(ctx)?;
    if ctx.args.positionals.get(1).map(String::as_str) == Some("doctor") {
        let profile = profile.ok_or_else(|| CliError(errors::NOT_CONNECTED.to_string()))?;
        return doctor(&profile);
    }

    // 2. not signed in: still serve (a plugin install must not show "failed to
    //    connect"), and say how to connect, up front and from every tool
    let Some(profile) = profile else {
        eprintln!("jot mcp {VERSION} ready, not signed in");
        let instructions = format!("{}\n\n{}", errors::NOT_CONNECTED, instructions(None, None));
        return answer_lines(&Server { api: None, reach: None }, &instructions);
    };
    let api = profile.api();

    // 3. what this token reaches, said to the agent up front (best effort: an
    //    unreachable API still starts the server, and the tools report the error)
    let reach = api.whoami().ok();
    let instructions = instructions(reach.as_ref(), profile.project_id.as_deref());
    let scope = reach.as_ref().map(|reach| format!(" · {}", describe_reach(reach))).unwrap_or_default();
    eprintln!("jot mcp {VERSION} ready (api: {}){scope}", profile.api_url);
    answer_lines(&Server { api: Some(api), reach }, &instructions)
}

/// One JSON-RPC message per line, until the client closes stdin.
fn answer_lines(server: &Server, instructions: &str) -> Result<()> {
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
    use crate::contract::NamedRef;

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
        let server = Server { api: Some(Api::new("http://127.0.0.1:9", "t")), reach: None };
        assert_eq!(handle(&server, "", &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })), None);
        let answer = handle(&server, "", &json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" })).unwrap();
        assert_eq!(answer["error"]["code"], METHOD_NOT_FOUND);
        let call = json!({ "jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": { "name": "post_reply", "arguments": { "commentId": "c1" } } });
        let refused = handle(&server, "", &call).unwrap();
        assert_eq!(refused["result"]["isError"], true);
        assert_eq!(refused["result"]["content"][0]["text"], errors::MISSING_MESSAGE);
    }
    #[test]
    fn serves_when_not_signed_in_and_every_tool_says_how_to_connect() {
        let server = Server { api: None, reach: None };
        let listed = handle(&server, "", &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(6));

        let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "fetch_feedback", "arguments": {} } });
        let answered = handle(&server, "", &call).unwrap();
        assert_eq!(answered["result"]["isError"], true);
        assert_eq!(answered["result"]["content"][0]["text"], errors::NOT_CONNECTED);
    }
}
