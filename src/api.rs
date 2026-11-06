//! A thin client of the Jot agent API. Paths come from `cli::api`; every
//! call carries `authorization: Bearer <token>`. It holds no rule of its own:
//! the API decides, and its refusals are passed on in its own words.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::contract::agent::{errors, mcp as copy};
use crate::contract::cli::{api, copy::messages};
use crate::contract::{AgentCommentList, AgentMarkdown, AgentStatusUpdated, AgentWhoamiResponse, Endpoint};
use crate::text::{fill, url_encode};
use crate::{CliError, Result};

/// ponytail: one fixed request timeout; a per-call knob when a slow endpoint needs one.
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Api {
    base: String,
    token: String,
    agent: ureq::Agent,
}

/// The query `listComments` takes, in the order the TS client sends it.
#[derive(Debug, Default, Clone)]
pub struct ListQuery {
    pub project_id: Option<String>,
    pub params: Vec<(&'static str, String)>,
}

/// A screenshot, ready for MCP image content.
pub struct Image {
    pub base64: String,
    pub mime_type: String,
}

/// An agent that hands back every status, so the API's own error text can be read.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(timeout)).build().into()
}

impl Api {
    pub fn new(base: &str, token: &str) -> Self {
        Api { base: base.trim_end_matches('/').to_string(), token: token.to_string(), agent: agent(TIMEOUT) }
    }

    /// The endpoint's URL: `:x` segments replaced, in order, by the encoded `params`.
    fn url(&self, endpoint: &Endpoint, params: &[&str], query: &[(&str, String)]) -> String {
        let mut values = params.iter();
        let path: Vec<String> = endpoint
            .path
            .split('/')
            .map(|segment| match segment.strip_prefix(':') {
                Some(_) => url_encode(values.next().copied().unwrap_or("")),
                None => segment.to_string(),
            })
            .collect();
        let mut url = format!("{}{}{}", self.base, api::PREFIX, path.join("/"));
        if !query.is_empty() {
            let pairs: Vec<String> = query.iter().map(|(key, value)| format!("{key}={}", url_encode(value))).collect();
            url = format!("{url}?{}", pairs.join("&"));
        }
        url
    }

    /// One call: the endpoint's method, the token, an optional JSON body; the
    /// response as `T`, or the API's own error message.
    fn call<T: DeserializeOwned>(
        &self,
        endpoint: &Endpoint,
        params: &[&str],
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<T> {
        // 1. send it
        let url = self.url(endpoint, params, query);
        let auth = format!("Bearer {}", self.token);
        let sent = match (endpoint.method, body) {
            ("GET", _) => self.agent.get(&url).header("authorization", &auth).call(),
            ("DELETE", _) => self.agent.delete(&url).header("authorization", &auth).call(),
            ("POST", body) => self.agent.post(&url).header("authorization", &auth).send_json(body.unwrap_or(json!({}))),
            (_, body) => self.agent.patch(&url).header("authorization", &auth).send_json(body.unwrap_or(json!({}))),
        };
        let mut response = sent.map_err(|error| {
            CliError(fill(errors::API_UNREACHABLE, &[("apiUrl", &self.base), ("message", &error.to_string())]))
        })?;

        // 2. a refusal, in the API's words when it gave any
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            let said: Option<String> = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|body| body.pointer("/error/message").and_then(Value::as_str).map(str::to_string));
            let message = said.unwrap_or_else(|| fill(messages::API_ERROR, &[("status", &status.to_string())]));
            if status == 401 {
                return Err(CliError(fill(errors::TOKEN_REFUSED, &[("message", &message)])));
            }
            return Err(CliError(message));
        }

        // 3. the answer
        serde_json::from_str(&text).map_err(|error| CliError(format!("{} {url}: {error}", endpoint.method)))
    }

    pub fn whoami(&self) -> Result<AgentWhoamiResponse> {
        self.call(&api::WHOAMI, &[], &[], None)
    }

    /// The raw whoami answer, for MCP structured content (every field the API sent).
    pub fn whoami_raw(&self) -> Result<Value> {
        self.call(&api::WHOAMI, &[], &[], None)
    }

    /// No project named: the token's whole reach (its project, or its workspace).
    pub fn list_comments(&self, query: &ListQuery) -> Result<AgentCommentList> {
        match &query.project_id {
            Some(project) => self.call(&api::LIST_PROJECT_COMMENTS, &[project], &query.params, None),
            None => self.call(&api::LIST_COMMENTS, &[], &query.params, None),
        }
    }

    /// One comment in full, untyped: callers read only the fields they need.
    pub fn get_comment(&self, comment_id: &str) -> Result<Value> {
        self.call(&api::GET_COMMENT, &[comment_id], &[], None)
    }

    pub fn markdown(&self, comment_id: &str) -> Result<String> {
        let answer: AgentMarkdown = self.call(&api::MARKDOWN, &[comment_id], &[], None)?;
        Ok(answer.markdown)
    }

    /// Any status word is passed through: the API applies the workspace's approval policy.
    pub fn update_status(&self, comment_id: &str, status: &str) -> Result<AgentStatusUpdated> {
        self.call(&api::UPDATE_STATUS, &[comment_id], &[], Some(json!({ "status": status })))
    }

    pub fn reply(&self, comment_id: &str, body: &str) -> Result<Value> {
        self.call(&api::REPLY, &[comment_id], &[], Some(json!({ "body": body })))
    }

    pub fn batch_note(&self, comment_ids: &[String], body: &str) -> Result<Value> {
        self.call(&api::BATCH_NOTE, &[], &[], Some(json!({ "commentIds": comment_ids, "body": body })))
    }

    /// A screenshot's bytes as base64, or None when it is missing, unreachable,
    /// not an image or too big to hand a model (contract `agent.mcp`).
    pub fn image(&self, url: &str) -> Option<Image> {
        let timeout = Duration::from_millis(copy::SCREENSHOT_TIMEOUT_MS);
        let mut response = agent(timeout).get(url).call().ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mime_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map_or("image/png", |value| value.split(';').next().unwrap_or("").trim())
            .to_string();
        if !mime_type.starts_with("image/") {
            return None;
        }
        let bytes = response.body_mut().with_config().limit(copy::SCREENSHOT_MAX_BYTES + 1).read_to_vec().ok()?;
        if bytes.len() as u64 > copy::SCREENSHOT_MAX_BYTES {
            return None;
        }
        Some(Image { base64: crate::text::base64(&bytes), mime_type })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_paths_from_the_contract() {
        let api = Api::new("https://api.test/", "t");
        assert_eq!(api.url(&api::GET_COMMENT, &["c 1"], &[]), "https://api.test/api/v1/comments/c%201");
        assert_eq!(
            api.url(&api::LIST_COMMENTS, &[], &[("status", "OPEN".into()), ("q", "a&b".into())]),
            "https://api.test/api/v1/comments?status=OPEN&q=a%26b"
        );
    }
}
