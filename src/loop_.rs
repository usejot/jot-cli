//! `jot loop`: without an agent, how to loop inside Claude Code; with
//! `--agent claude`, each new comment goes to `claude -p` headless
//! (`cli::loop`), and its answer goes back on the thread: as a proposed plan
//! by default, or as the fix's summary with the comment RESOLVED under --fix.

use std::io::Read;
use std::process::{Command, Stdio};

use crate::contract::cli::copy::{self, messages};
use crate::contract::cli::r#loop as rules;
use crate::profiles::connect;
use crate::text::{fill, one_line};
use crate::watch::{each_new, watch_options};
use crate::{CliError, Context, Result};

/// The summary cut in the terminal line.
const SUMMARY: usize = 70;

/// What `claude -p` said, and how it exited.
struct AgentRun {
    code: i32,
    output: String,
}

/// The contract's args with `{prompt}` and `{mode}` filled.
fn agent_args(prompt: &str, fix: bool) -> Vec<String> {
    let mode = if fix { rules::permission_modes::FIX } else { rules::permission_modes::PROPOSE };
    rules::ARGS.iter().map(|arg| fill(arg, &[("prompt", prompt), ("mode", mode)])).collect()
}

/// Runs the agent: its answer captured, its progress on stderr.
fn run_agent(prompt: &str, fix: bool) -> Result<AgentRun> {
    let child = Command::new(rules::COMMAND)
        .args(agent_args(prompt, fix))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError(fill(messages::LOOP_AGENT_MISSING, &[("url", rules::INSTALL_URL)])));
        }
        Err(error) => return Err(error.into()),
    };
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut output)?;
    }
    let code = child.wait()?.code().unwrap_or(1);
    Ok(AgentRun { code, output: output.trim().to_string() })
}

pub fn run(ctx: &Context) -> Result<()> {
    // 1. without an agent, the loop lives inside the agent
    let Some(agent) = ctx.args.value("agent") else {
        println!("{}", messages::LOOP_INSIDE_AGENT);
        println!("{}", messages::LOOP_INIT_FIRST);
        println!("{}", messages::LOOP_HEADLESS);
        return Ok(());
    };
    if !rules::AGENTS.contains(&agent) {
        return Err(CliError(fill(messages::LOOP_AGENT_UNSUPPORTED, &[("agent", agent)])));
    }

    // 2. each new comment: in progress, to the agent, and the answer back on the thread
    let profile = connect(ctx)?;
    let options = watch_options(ctx)?;
    let fix = ctx.args.on("fix");
    let api = profile.api();
    let mode = if fix { "fix" } else { "propose" };
    eprintln!("{}", fill(messages::LOOP_STARTED, &[("profile", &profile.name), ("mode", mode)]));
    each_new(ctx, &profile, &options, |comment| {
        let id = comment.id;
        let _ = api.update_status(&id, "IN_PROGRESS");
        let markdown = api.markdown(&id)?;
        let task = if fix { copy::HEADLESS_TASK_FIX } else { copy::HEADLESS_TASK_PROPOSE };
        let prompt = fill(copy::HEADLESS_PROMPT, &[("commentId", &id), ("markdown", &markdown), ("task", task)]);
        let answer = run_agent(&prompt, fix)?;

        // a failed run leaves the comment IN_PROGRESS for a person to look at
        if answer.code != 0 || answer.output.is_empty() {
            println!("{}", fill(messages::LOOP_FAILED, &[("id", &id), ("code", &answer.code.to_string())]));
            return Ok(true);
        }
        let summary = one_line(&answer.output, SUMMARY);
        if fix {
            api.update_status(&id, "RESOLVED")?;
            api.reply(&id, &answer.output)?;
            println!("{}", fill(messages::LOOP_RESOLVED, &[("id", &id), ("summary", &summary)]));
        } else {
            api.reply(&id, &fill(messages::LOOP_PROPOSED_REPLY, &[("output", &answer.output)]))?;
            println!("{}", fill(messages::LOOP_PROPOSED, &[("id", &id), ("summary", &summary)]));
        }
        Ok(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_claude_the_contract_args() {
        assert_eq!(agent_args("fix it", false), ["-p", "fix it", "--permission-mode", "plan"]);
        assert_eq!(agent_args("{mode}", true), ["-p", "{mode}", "--permission-mode", "acceptEdits"]);
    }
}
