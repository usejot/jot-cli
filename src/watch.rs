//! The feedback commands: list, show, status, reply, and watch, which polls
//! for new open comments and remembers what it has shown in a capped seen-set
//! (`~/.jot/state/<profile>.json`, `cli::watch::SEEN_CAP`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api::ListQuery;
use crate::contract::cli::{copy::messages, paths, watch as rules};
use crate::contract::constants::COMMENT_STATUSES;
use crate::contract::{AgentComment, WatchState};
use crate::profiles::{connect, slugify, write_private, Resolved};
use crate::text::{fill, one_line, where_of};
use crate::{CliError, Context, Result};

/// listRow cuts the body to this many characters.
const ROW_BODY: usize = 70;

// ---------------------------------------------------------------- the seen-set

fn state_path(home: &Path, profile: &str) -> PathBuf {
    home.join(paths::STATE.replace("<profile>", &slugify(profile)))
}

/// None when this profile has never been watched: the first run's signal.
pub fn read_seen(home: &Path, profile: &str) -> Result<Option<WatchState>> {
    let path = state_path(home, profile);
    if !path.exists() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let seen = parsed.get("seen").and_then(|seen| seen.as_array()).cloned().unwrap_or_default();
    Ok(Some(WatchState { seen: seen.into_iter().filter_map(|id| id.as_str().map(str::to_string)).collect() }))
}

/// Compact JSON and a newline, private like the config.
pub fn write_seen(home: &Path, profile: &str, state: &WatchState) -> Result<()> {
    write_private(&state_path(home, profile), &format!("{}\n", serde_json::to_string(state)?))
}

/// The comments not seen yet, oldest first, so a backlog is worked in order.
pub fn unseen(comments: Vec<AgentComment>, state: &WatchState) -> Vec<AgentComment> {
    let seen: HashSet<&str> = state.seen.iter().map(String::as_str).collect();
    let mut fresh: Vec<AgentComment> =
        comments.into_iter().filter(|comment| !seen.contains(comment.id.as_str())).collect();
    fresh.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    fresh
}

/// The state with these ids added at the end, the newest `cap` kept.
pub fn mark_seen(state: &WatchState, ids: &[String], cap: usize) -> WatchState {
    let mut seen: Vec<String> = state.seen.iter().filter(|id| !ids.contains(id)).cloned().collect();
    seen.extend(ids.iter().cloned());
    let drop = seen.len().saturating_sub(cap);
    WatchState { seen: seen.split_off(drop) }
}

// ---------------------------------------------------------------- list, show, status, reply

/// `--status open` → "OPEN"; `all` → no filter.
fn status_filter(ctx: &Context) -> Result<Option<String>> {
    let raw = ctx.args.value("status").unwrap_or(rules::STATUS).to_uppercase();
    if raw == "ALL" {
        return Ok(None);
    }
    if !COMMENT_STATUSES.contains(&raw.as_str()) {
        return Err(CliError(fill(
            messages::LIST_UNKNOWN_STATUS,
            &[("status", &raw), ("statuses", &COMMENT_STATUSES.join(", "))],
        )));
    }
    Ok(Some(raw))
}

fn query(profile: &Resolved, status: Option<String>) -> ListQuery {
    let mut params = Vec::new();
    if let Some(status) = status {
        params.push(("status", status));
    }
    params.push(("limit", rules::LIST_LIMIT.to_string()));
    ListQuery { project_id: profile.project_id.clone(), params }
}

fn summary_line(comment: &AgentComment) -> String {
    let project = comment
        .project
        .as_ref()
        .map(|project| fill(messages::LIST_ROW_PROJECT, &[("name", &project.name)]))
        .unwrap_or_default();
    fill(
        messages::LIST_ROW,
        &[
            ("id", &comment.id),
            ("status", &comment.status),
            ("body", &one_line(&comment.body, ROW_BODY)),
            ("where", &where_of(&comment.context)),
            ("project", &project),
        ],
    )
}

pub fn list(ctx: &Context) -> Result<()> {
    let profile = connect(ctx)?;
    let comments = profile.api().list_comments(&query(&profile, status_filter(ctx)?))?.comments;
    if ctx.args.on("json") {
        println!("{}", serde_json::to_string_pretty(&comments)?);
        return Ok(());
    }
    if comments.is_empty() {
        println!("{}", messages::LIST_EMPTY);
        return Ok(());
    }
    let lines: Vec<String> = comments.iter().map(summary_line).collect();
    println!("{}", lines.join("\n"));
    Ok(())
}

pub fn show(ctx: &Context) -> Result<()> {
    let id = ctx.operand(1, "show")?;
    println!("{}", connect(ctx)?.api().markdown(&id)?);
    Ok(())
}

pub fn status(ctx: &Context) -> Result<()> {
    let id = ctx.operand(1, "status")?;
    let wanted = ctx.operand(2, "status")?.to_uppercase();
    let api = connect(ctx)?.api();

    // the API decides which statuses this token may set (the workspace's policy),
    // so any word is passed through and its refusal is shown as is
    let updated = api.update_status(&id, &wanted)?;
    println!("{}", fill(messages::STATUS_DONE, &[("id", &updated.id), ("status", &updated.status)]));
    if let Some(note) = ctx.args.value("message") {
        api.reply(&id, note)?;
        println!("{}", messages::STATUS_NOTE_POSTED);
    }
    Ok(())
}

pub fn reply(ctx: &Context) -> Result<()> {
    let id = ctx.operand(1, "reply")?;
    let note = ctx.args.value("message").ok_or_else(|| crate::usage_error("reply"))?;
    connect(ctx)?.api().reply(&id, note)?;
    println!("{}", fill(messages::REPLY_DONE, &[("id", &id)]));
    Ok(())
}

// ---------------------------------------------------------------- watch

pub struct WatchOptions {
    pub interval: Duration,
    /// True: what is already open counts as new on the first run.
    pub all: bool,
}

pub fn watch_options(ctx: &Context) -> Result<WatchOptions> {
    let raw = ctx.args.value("interval").map_or(Ok(rules::INTERVAL_SECONDS as f64), str::parse::<f64>);
    let seconds = raw.ok().filter(|s| s.is_finite() && *s >= rules::MIN_INTERVAL_SECONDS as f64);
    let seconds = seconds.ok_or_else(|| CliError(messages::WATCH_INTERVAL_INVALID.to_string()))?;
    Ok(WatchOptions { interval: Duration::from_secs_f64(seconds), all: ctx.args.on("all") })
}

/// Every new open comment, oldest first, handed to `each` until it says stop.
/// Each is marked seen before it is handed out.
/// ponytail: polls every few seconds; an agent SSE stream on the API is the upgrade if that feels slow.
pub fn each_new(
    ctx: &Context,
    profile: &Resolved,
    options: &WatchOptions,
    mut each: impl FnMut(AgentComment) -> Result<bool>,
) -> Result<()> {
    let api = profile.api();
    let cap = rules::SEEN_CAP as usize;
    let open = || api.list_comments(&query(profile, Some(rules::STATUS.to_string()))).map(|list| list.comments);

    // 1. the first run: what is already open is the backlog, not news (unless --all)
    let mut state = match read_seen(&ctx.home, &profile.name)? {
        Some(state) => state,
        None => {
            let backlog = open()?;
            let ids: Vec<String> =
                if options.all { Vec::new() } else { backlog.iter().map(|c| c.id.clone()).collect() };
            let state = mark_seen(&WatchState::default(), &ids, cap);
            write_seen(&ctx.home, &profile.name, &state)?;
            if !ids.is_empty() {
                eprintln!("{}", fill(messages::WATCH_BACKLOG, &[("count", &ids.len().to_string())]));
            }
            state
        }
    };

    // 2. poll; a failed poll is said and retried, never fatal
    loop {
        match open() {
            Ok(comments) => {
                for comment in unseen(comments, &state) {
                    state = mark_seen(&state, std::slice::from_ref(&comment.id), cap);
                    write_seen(&ctx.home, &profile.name, &state)?;
                    if !each(comment)? {
                        return Ok(());
                    }
                }
            }
            Err(error) => {
                let seconds = options.interval.as_secs_f64().to_string();
                eprintln!("{}", fill(messages::WATCH_RETRY, &[("message", &error.0), ("seconds", &seconds)]));
            }
        }
        std::thread::sleep(options.interval);
    }
}

pub fn watch(ctx: &Context) -> Result<()> {
    let profile = connect(ctx)?;
    let options = watch_options(ctx)?;
    let api = profile.api();
    eprintln!("{}", fill(messages::WATCH_STARTED, &[("profile", &profile.name)]));
    each_new(ctx, &profile, &options, |comment| {
        if ctx.args.on("json") {
            let mut row = serde_json::to_value(&comment)?;
            row["where"] = serde_json::Value::String(where_of(&comment.context));
            println!("{}", serde_json::to_string(&row)?);
        } else {
            let markdown = api.markdown(&comment.id)?;
            println!("{}", fill(messages::WATCH_ITEM, &[("id", &comment.id), ("markdown", &markdown)]));
        }
        Ok(!ctx.args.on("once"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::tests::{conformance, temp_dir};

    fn ids(value: &serde_json::Value) -> Vec<String> {
        value.as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn conformance_watch() {
        for case in conformance()["watch"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let state = WatchState { seen: ids(&case["seen"]) };
            let comments: Vec<AgentComment> = case["comments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| AgentComment {
                    id: row["id"].as_str().unwrap().into(),
                    created_at: row["createdAt"].as_str().unwrap().into(),
                    ..AgentComment::default()
                })
                .collect();
            let fresh: Vec<String> = unseen(comments, &state).into_iter().map(|c| c.id).collect();
            assert_eq!(fresh, ids(&case["unseen"]), "{name}");
            let after = mark_seen(&state, &ids(&case["mark"]), case["cap"].as_u64().unwrap() as usize);
            assert_eq!(after.seen, ids(&case["seenAfter"]), "{name}");
        }
    }

    #[test]
    fn the_seen_set_is_capped_at_the_contract_cap() {
        let many: Vec<String> = (0..rules::SEEN_CAP + 5).map(|n| format!("c{n}")).collect();
        let state = mark_seen(&WatchState::default(), &many, rules::SEEN_CAP as usize);
        assert_eq!(state.seen.len() as u64, rules::SEEN_CAP);
        assert_eq!(state.seen.first().map(String::as_str), Some("c5"));
    }

    #[test]
    fn the_seen_set_is_stored_compact_per_profile() {
        let home = temp_dir("seen");
        assert_eq!(read_seen(&home, "Office HQ").unwrap(), None);
        let state = WatchState { seen: vec!["a".into(), "b".into()] };
        write_seen(&home, "Office HQ", &state).unwrap();
        assert_eq!(fs::read_to_string(home.join("state/office-hq.json")).unwrap(), "{\"seen\":[\"a\",\"b\"]}\n");
        assert_eq!(read_seen(&home, "Office HQ").unwrap(), Some(state));
        fs::remove_dir_all(home).unwrap();
    }
}
