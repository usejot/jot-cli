//! `jot mcp`, spawned as an agent spawns it: initialize, then tools/list over
//! stdio. The first six tools must equal what the published @usejot/mcp 0.8.0
//! answers (the contract's conformance/mcp-tools-0.8.0.json), with the one intentional
//! change both servers make: update_status also accepts APPROVED. The CLI's
//! session tools (contract.json#cli.mcpSessionTools) come after them, only them.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/conformance/mcp-tools-0.8.0.json");
const CONTRACT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/contract.json");

/// Runs `jot mcp` with these lines on stdin; its stdout lines as JSON.
fn exchange(lines: &[Value]) -> Vec<Value> {
    let home = std::env::temp_dir().join(format!("jot-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jot"))
        .arg("mcp")
        .current_dir(&home)
        .env("JOT_HOME", &home)
        .env("JOT_API_TOKEN", "tok_test")
        // nothing listens on port 9: whoami fails fast and the server still starts
        .env("BACKEND_API_URL", "http://127.0.0.1:9")
        .env_remove("JOT_PROFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

/// Two artefacts of how the TS SDK prints zod as JSON Schema, said once so
/// nothing else can differ: a repeated property becomes a `$ref` to its first
/// use, and a property-less object loses `additionalProperties: false`.
fn normalize(schema: &Value, root: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            if let Some(target) = map.get("$ref").and_then(Value::as_str) {
                let pointer = target.trim_start_matches('#');
                if let Value::Object(referenced) = normalize(root.pointer(pointer).unwrap(), root) {
                    out.extend(referenced);
                }
            }
            for (key, value) in map {
                if key != "$ref" {
                    out.insert(key.clone(), normalize(value, root));
                }
            }
            let empty = out.get("properties").and_then(Value::as_object).is_some_and(|p| p.is_empty());
            if empty && out.get("additionalProperties") == Some(&json!(false)) {
                out.remove("additionalProperties");
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| normalize(item, root)).collect()),
        other => other.clone(),
    }
}

/// name, inputSchema and outputSchema: what the snapshot records.
fn comparable(tool: &Value) -> Value {
    let mut out = json!({ "name": tool["name"], "inputSchema": normalize(&tool["inputSchema"], &tool["inputSchema"]) });
    if let Some(output) = tool.get("outputSchema") {
        out["outputSchema"] = normalize(output, output);
    }
    out
}

#[test]
fn initialize_then_tools_list_matches_the_ts_server() {
    let answers = exchange(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ]);

    // 1. initialize: the version asked for, the server's name, tools, and the playbook
    assert_eq!(answers.len(), 2, "a notification gets no answer");
    let init = &answers[0]["result"];
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(init["protocolVersion"], "2024-11-05");
    assert_eq!(init["serverInfo"]["name"], "jot");
    assert_eq!(init["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(init["capabilities"], json!({ "tools": {} }));
    assert!(init["instructions"].as_str().unwrap().contains("# Working a Jot queue"));

    // 2. tools/list: the 0.8.0 snapshot, update_status widened to every requestable status
    let snapshot: Value = serde_json::from_str(&std::fs::read_to_string(SNAPSHOT).unwrap()).unwrap();
    let mut expected: Vec<Value> = snapshot["tools"].as_array().unwrap().iter().map(comparable).collect();
    let status = expected
        .iter_mut()
        .find(|tool| tool["name"] == "update_status")
        .and_then(|tool| tool.pointer_mut("/inputSchema/properties/status/enum"))
        .unwrap();
    assert_eq!(*status, json!(["IN_PROGRESS", "RESOLVED"]));
    *status = json!(["IN_PROGRESS", "RESOLVED", "APPROVED"]);

    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    let served: Vec<Value> = tools.iter().take(expected.len()).map(comparable).collect();
    assert_eq!(served, expected);

    // 3. then the session tools, in the contract's order, with the contract's schemas
    let contract: Value = serde_json::from_str(&std::fs::read_to_string(CONTRACT).unwrap()).unwrap();
    let session = contract["cli"]["mcpSessionTools"].as_object().unwrap();
    let after: Vec<&Value> = tools.iter().skip(expected.len()).collect();
    assert_eq!(after.len(), session.len());
    for (tool, (name, guide)) in after.iter().zip(session) {
        assert_eq!(tool["name"], json!(name));
        assert_eq!(tool["inputSchema"], guide["inputSchema"], "{name}");
        assert_eq!(tool["description"], guide["mcpDescription"], "{name}");
    }

    // 4. every tool carries the contract's guide
    for tool in tools {
        assert!(tool["description"].as_str().unwrap().contains("\nWhen: "), "{}", tool["name"]);
        assert!(tool["title"].is_string() && tool["annotations"].is_object());
    }
}

#[test]
fn a_tool_error_is_a_result_not_a_crash() {
    let answers = exchange(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "fetch_feedback", "arguments": {} } }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "fetch_feedback", "arguments": { "limit": 101 } } }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
    ]);
    assert_eq!(answers[0]["result"]["isError"], true);
    assert!(answers[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Could not reach the Jot API at http://127.0.0.1:9"));
    assert_eq!(answers[1]["result"]["isError"], true);
    assert!(answers[1]["result"]["content"][0]["text"].as_str().unwrap().starts_with("Input validation error"));
    assert_eq!(answers[2]["result"], json!({}));
}
