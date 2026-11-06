//! `jot`: Jot feedback from the terminal, the setup that makes `/jot` work in
//! a coding agent, and (`jot mcp`) the MCP server itself.
//!
//! Every name, path, rule and sentence comes from the generated `contract.rs`.
//! The logic worth testing lives in args.rs, profiles.rs, init.rs, watch.rs
//! and mcp.rs; this file dispatches.

#[rustfmt::skip]
mod contract;

mod api;
mod args;
mod init;
mod login;
mod loop_;
mod mcp;
mod profiles;
mod text;
mod watch;

use std::fmt;
use std::process::ExitCode;

use args::Parsed;
use contract::cli::copy::messages;
use contract::cli::{self as c};
use text::fill;

/// The CLI's own version, printed by `--version` and sent as the MCP serverInfo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A failure the person can act on: printed as is, no backtrace.
#[derive(Debug, PartialEq, Eq)]
pub struct CliError(pub String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        CliError(error.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        CliError(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

/// What every command gets: its arguments and the home directory (~/.jot).
pub struct Context {
    pub args: Parsed,
    pub home: std::path::PathBuf,
}

impl Context {
    /// The positional at `index`, or a usage error quoting the command's contract usage.
    pub fn operand(&self, index: usize, command: &str) -> Result<String> {
        match self.args.positionals.get(index) {
            Some(value) if !value.is_empty() => Ok(value.clone()),
            _ => Err(usage_error(command)),
        }
    }
}

/// `Usage: <the command's usage line>`, from the contract's command table.
pub fn usage_error(command: &str) -> CliError {
    let usage = c::COMMANDS.iter().find(|row| row.name == command).map_or(command, |row| row.usage);
    CliError(fill(messages::USAGE, &[("usage", usage)]))
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", fill(messages::ERROR, &[("message", &error.0)]));
            ExitCode::FAILURE
        }
    }
}

fn run(argv: Vec<String>) -> Result<()> {
    // 1. the arguments, and the two flags that answer before any command
    let args = args::parse(&argv)?;
    if args.on("version") {
        println!("{VERSION}");
        return Ok(());
    }
    let command = args.positionals.first().cloned().unwrap_or_default();
    if command.is_empty() || command == "help" || args.on("help") {
        println!("{}", fill(c::copy::HELP, &[("version", VERSION)]));
        return Ok(());
    }

    // 2. the command
    let ctx = Context { args, home: profiles::jot_home(&profiles::Env::from_process()) };
    match command.as_str() {
        "login" => login::login(&ctx),
        "profiles" => profiles::list_command(&ctx),
        "use" => profiles::use_command(&ctx),
        "logout" => profiles::logout_command(&ctx),
        "whoami" => profiles::whoami_command(&ctx),
        "init" => init::init(&ctx),
        "list" => watch::list(&ctx),
        "show" => watch::show(&ctx),
        "status" => watch::status(&ctx),
        "reply" => watch::reply(&ctx),
        "watch" => watch::watch(&ctx),
        "loop" => loop_::run(&ctx),
        "prompt" => {
            println!("{}", c::copy::AGENT_PROMPT);
            Ok(())
        }
        "mcp" => mcp::serve(&ctx),
        _ => Err(CliError(fill(messages::UNKNOWN_COMMAND, &[("command", &command)]))),
    }
}
