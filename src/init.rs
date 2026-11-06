//! `jot init`: binds a repo to a profile (and a project), and sets up every
//! agent that reads it: the jot MCP server in .mcp.json (tokenless), the /jot
//! command and the jot skill for Claude Code, and a marked section in
//! AGENTS.md for Codex, Cursor and the rest. Every file and sentence is the
//! contract's; nothing a person edited is replaced without --force.

use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::contract::agent::{agents_md, skill};
use crate::contract::cli::{self as c, copy::messages, env as names, paths};
use crate::contract::{AgentWhoamiResponse, McpServerEntry, RepoBinding};
use crate::profiles::{connect, pretty, user_home, Env};
use crate::text::fill;
use crate::{CliError, Context, Result};

/// The .mcp.json after merging, and whether an old token had to go.
#[derive(Debug, PartialEq)]
pub struct McpMerge {
    pub config: Value,
    pub removed_token: bool,
}

/// `jot mcp` when `jot` is on PATH; else the contract's npx entry, which reads the same profiles.
pub fn server_entry(jot_on_path: bool) -> McpServerEntry {
    let (command, args) = if jot_on_path {
        (c::mcp_server_entry_local::COMMAND, c::mcp_server_entry_local::ARGS)
    } else {
        (c::mcp_server_entry::COMMAND, c::mcp_server_entry::ARGS)
    };
    McpServerEntry { command: command.to_string(), args: args.iter().map(|arg| arg.to_string()).collect(), env: None }
}

/// True when the `bin` an agent would start from PATH is this CLI. A bare
/// lookup is not enough: macOS ships an unrelated BSD `/usr/bin/jot`, which
/// refuses `--version`; this CLI (or its npm shim) prints a version.
pub fn jot_on_path(bin: &str) -> bool {
    let probe = std::process::Command::new(bin).arg("--version").stdin(std::process::Stdio::null()).output();
    let Ok(output) = probe else { return false };
    let said = String::from_utf8_lossy(&output.stdout);
    output.status.success() && looks_like_version(said.trim())
}

/// "0.1.0" and the like: digits and dots, with a dot.
fn looks_like_version(text: &str) -> bool {
    text.contains('.') && text.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// The existing .mcp.json with the jot entry replaced: every other server and key kept.
pub fn merge_mcp_config(existing: Option<Value>, entry: &McpServerEntry) -> Result<McpMerge> {
    // 1. whatever was there, if it was an object
    let mut config = match existing {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let mut servers = match config.remove("mcpServers") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };

    // 2. an old jot entry with JOT_API_TOKEN: the profile wins now, and it is a secret in a committed file
    let removed_token = servers
        .get("jot")
        .and_then(|jot| jot.get("env"))
        .and_then(Value::as_object)
        .is_some_and(|env| env.contains_key(names::api_token::NAME));

    // 3. the jot entry, tokenless
    servers.insert("jot".to_string(), serde_json::to_value(entry)?);
    config.insert("mcpServers".to_string(), Value::Object(servers));
    Ok(McpMerge { config: Value::Object(config), removed_token })
}

// ---------------------------------------------------------------- Claude Code

/// Where Claude Code keeps its user config and its plugin registry (`cli::claude_code`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudePaths {
    pub user_config: PathBuf,
    pub installed_plugins: PathBuf,
}

impl ClaudePaths {
    /// `$CLAUDE_CONFIG_DIR` holds both when set; else `~/.claude.json` and `~/.claude/plugins/…`.
    pub fn from_env(env: &Env) -> Self {
        let registry = c::claude_code::INSTALLED_PLUGINS;
        match env.get(c::claude_code::CONFIG_DIR_ENV) {
            Some(dir) => ClaudePaths {
                user_config: Path::new(dir).join(c::claude_code::USER_CONFIG),
                installed_plugins: Path::new(dir).join(registry),
            },
            None => {
                let home = user_home(env);
                ClaudePaths {
                    user_config: home.join(c::claude_code::USER_CONFIG),
                    installed_plugins: home.join(c::claude_code::CONFIG_DIR).join(registry),
                }
            }
        }
    }
}

/// A JSON file, None when it is missing or not JSON: these files are Claude Code's, read best effort.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// True when the Jot plugin for Claude Code is installed: `jot@usejot` in the registry's `plugins`.
pub fn plugin_installed(claude: &ClaudePaths) -> bool {
    let key = format!("{}@{}", c::packages::CLAUDE_PLUGIN, c::packages::CLAUDE_MARKETPLACE);
    read_json(&claude.installed_plugins).and_then(|registry| registry.get("plugins")?.get(&key).cloned()).is_some()
}

/// The first file besides the plugin that also configures a `jot` MCP server:
/// `.claude.json` (top level, or this directory's project entry), then `./.mcp.json`.
pub fn other_jot_server(claude: &ClaudePaths, cwd: &Path) -> Option<PathBuf> {
    let has_jot = |config: &Value| config.get("mcpServers").and_then(|servers| servers.get("jot")).is_some();

    // 1. the user config: every project's, and this directory's own entry
    if let Some(config) = read_json(&claude.user_config) {
        let project = config.get("projects").and_then(|projects| projects.get(cwd.to_string_lossy().as_ref()));
        if has_jot(&config) || project.is_some_and(has_jot) {
            return Some(claude.user_config.clone());
        }
    }

    // 2. the repo's .mcp.json, which `jot init` used to write
    let repo = cwd.join(paths::MCP_CONFIG);
    read_json(&repo).filter(has_jot).map(|_| repo)
}

/// The one-line warning about a second `jot` server, naming its file.
pub fn duplicate_warning(file: &Path) -> String {
    fill(messages::MCP_DUPLICATE_SERVER, &[("file", &file.display().to_string())])
}

/// AGENTS.md with the jot section in place: replaced between its markers when
/// present, appended when not. None when the file already says exactly that.
pub fn upsert_agents_md(existing: Option<&str>) -> Option<String> {
    let block = format!("{}\n{}{}", agents_md::START, agents_md::BODY, agents_md::END);
    let current = existing.unwrap_or("");

    // 1. a section already there: only what sits between the markers changes
    if let Some(start) = current.find(agents_md::START) {
        if let Some(offset) = current[start..].find(agents_md::END) {
            let end = start + offset + agents_md::END.len();
            let next = format!("{}{block}{}", &current[..start], &current[end..]);
            return (next != current).then_some(next);
        }
    }

    // 2. none yet: after whatever the repo wrote, separated by a blank line
    let kept = current.trim_end();
    Some(if kept.is_empty() { format!("{block}\n") } else { format!("{kept}\n\n{block}\n") })
}

/// The file's text, None when it does not exist.
fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Writes `content` unless a different file is there and --force was not given.
/// True when written.
fn write_unless_edited(path: &Path, content: &str, force: bool) -> Result<bool> {
    let current = read_optional(path)?;
    if current.as_deref().is_some_and(|text| text != content) && !force {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content)?;
    Ok(true)
}

/// A numbered choice on a terminal; None for "all projects" or when there is no terminal.
fn choose_project(reach: &AgentWhoamiResponse) -> Result<Option<String>> {
    // 1. nothing to choose
    if reach.projects.len() == 1 {
        return Ok(Some(reach.projects[0].id.clone()));
    }
    if reach.projects.is_empty() || !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    // 2. ask
    println!("{}", fill(messages::INIT_CHOOSE_PROJECT, &[("org", &reach.org.name)]));
    for (index, project) in reach.projects.iter().enumerate() {
        println!("{}", fill(messages::INIT_CHOICE, &[("n", &(index + 1).to_string()), ("name", &project.name)]));
    }
    println!("{}", messages::INIT_CHOICE_ALL);
    let stdin = std::io::stdin();
    loop {
        print!("{}", messages::INIT_PROMPT);
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        match line.trim().parse::<usize>() {
            Ok(0) => return Ok(None),
            Ok(n) if n <= reach.projects.len() => return Ok(Some(reach.projects[n - 1].id.clone())),
            _ => println!("{}", fill(messages::INIT_CHOICE_INVALID, &[("max", &reach.projects.len().to_string())])),
        }
    }
}

/// Writes the jot entry into the repo's .mcp.json, or leaves the file alone
/// when the Claude Code plugin already runs the server (two would load). The lines to print.
fn setup_mcp(cwd: &Path, plugin_installed: bool) -> Result<Vec<&'static str>> {
    // 1. the plugin provides it: a second entry would start a second server
    if plugin_installed {
        return Ok(vec![messages::INIT_MCP_PLUGIN_PROVIDES]);
    }

    // 2. merged into whatever is there
    let mcp_path = cwd.join(paths::MCP_CONFIG);
    let existing =
        match read_optional(&mcp_path)? {
            Some(text) => Some(serde_json::from_str(&text).map_err(|_| {
                CliError(fill(messages::INIT_INVALID_JSON, &[("path", &mcp_path.display().to_string())]))
            })?),
            None => None,
        };
    let merged = merge_mcp_config(existing, &server_entry(jot_on_path(c::packages::BIN)))?;
    fs::write(&mcp_path, pretty(&merged.config)?)?;
    let mut lines = vec![messages::INIT_MCP_WRITTEN];
    if merged.removed_token {
        lines.push(messages::INIT_MCP_TOKEN_REMOVED);
    }
    Ok(lines)
}

pub fn init(ctx: &Context) -> Result<()> {
    // 1. a saved profile: .jot.json names it, so a bare env token cannot be bound
    let profile = connect(ctx)?;
    if profile.is_legacy() {
        return Err(CliError(messages::INIT_NEEDS_SAVED_PROFILE.to_string()));
    }
    let reach = profile.api().whoami()?;
    let cwd = std::env::current_dir()?;
    let force = ctx.args.on("force");

    // 2. the project: the flag, else a pick
    let project = match ctx.args.value("project") {
        Some(id) => Some(id.to_string()),
        None => choose_project(&reach)?,
    };
    let binding = RepoBinding { profile: Some(profile.name.clone()), project: project.clone() };
    fs::write(cwd.join(paths::REPO_BINDING), pretty(&binding)?)?;
    let project_name = reach
        .projects
        .iter()
        .find(|row| Some(&row.id) == project.as_ref())
        .map_or(messages::INIT_ALL_PROJECTS, |row| row.name.as_str());
    println!("{}", fill(messages::INIT_BINDING_WRITTEN, &[("profile", &profile.name), ("project", project_name)]));

    // 3. the MCP server, tokenless (it finds the token through .jot.json), unless the plugin runs it
    let plugin = plugin_installed(&ClaudePaths::from_env(&Env::from_process()));
    for line in setup_mcp(&cwd, plugin)? {
        println!("{line}");
    }

    // 4. /jot and the jot skill for Claude Code, never clobbering one somebody edited
    let command = write_unless_edited(&cwd.join(paths::CLAUDE_COMMAND), c::copy::CLAUDE_COMMAND, force)?;
    println!("{}", if command { messages::INIT_COMMAND_WRITTEN } else { messages::INIT_COMMAND_KEPT });
    let skill = write_unless_edited(&cwd.join(paths::CLAUDE_SKILL), skill::FILE, force)?;
    println!("{}", if skill { messages::INIT_SKILL_WRITTEN } else { messages::INIT_SKILL_KEPT });

    // 5. AGENTS.md: the marked section only, the rest is the repo's
    let agents_path = cwd.join(paths::AGENTS_MD);
    match upsert_agents_md(read_optional(&agents_path)?.as_deref()) {
        Some(next) => {
            fs::write(&agents_path, next)?;
            println!("{}", messages::INIT_AGENTS_MD_WRITTEN);
        }
        None => println!("{}", messages::INIT_AGENTS_MD_CURRENT),
    }

    println!("{}", messages::INIT_DONE);
    Ok(())
}

/// The .mcp.json entry as a reader sees it, for tests and docs.
#[cfg(test)]
fn entry_json(jot_on_path: bool) -> Value {
    serde_json::to_value(server_entry(jot_on_path)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_bsd_jot_is_not_this_cli() {
        assert!(looks_like_version("0.1.0"));
        assert!(!looks_like_version("jot: illegal option -- -"));
        assert!(!looks_like_version("1"));
        assert!(!jot_on_path("/usr/bin/false"));
    }

    #[test]
    fn the_entry_is_jot_mcp_when_jot_is_installed() {
        assert_eq!(entry_json(true), json!({ "command": "jot", "args": ["mcp"] }));
        assert_eq!(entry_json(false), json!({ "command": "npx", "args": ["-y", "@usejot/mcp"] }));
    }

    #[test]
    fn merging_keeps_other_servers_and_drops_the_old_token() {
        let existing = json!({
            "mcpServers": {
                "other": { "command": "x" },
                "jot": { "command": "npx", "args": ["-y", "@usejot/mcp"], "env": { "JOT_API_TOKEN": "secret" } }
            },
            "extra": true
        });
        let merged = merge_mcp_config(Some(existing), &server_entry(true)).unwrap();
        assert!(merged.removed_token);
        assert_eq!(
            merged.config,
            json!({ "extra": true, "mcpServers": { "other": { "command": "x" }, "jot": { "command": "jot", "args": ["mcp"] } } })
        );
        assert!(!pretty(&merged.config).unwrap().contains("secret"));
    }

    #[test]
    fn merging_starts_fresh_from_nothing_or_junk() {
        for existing in [None, Some(json!([1])), Some(json!({ "mcpServers": "nope" }))] {
            let merged = merge_mcp_config(existing, &server_entry(false)).unwrap();
            assert!(!merged.removed_token);
            assert_eq!(merged.config["mcpServers"]["jot"]["command"], "npx");
        }
    }

    #[test]
    fn agents_md_is_idempotent_and_never_clobbers() {
        // 1. created from nothing, then left alone
        let created = upsert_agents_md(None).unwrap();
        assert!(created.starts_with(agents_md::START) && created.ends_with(&format!("{}\n", agents_md::END)));
        assert_eq!(upsert_agents_md(Some(&created)), None);

        // 2. appended after the repo's own text, which stays
        let mine = "# My repo\n\nRun `make test`.\n";
        let appended = upsert_agents_md(Some(mine)).unwrap();
        assert!(appended.starts_with(mine));
        assert_eq!(appended.matches(agents_md::START).count(), 1);
        assert_eq!(upsert_agents_md(Some(&appended)), None);

        // 3. an old section is replaced in place; text on both sides stays
        let old = format!("before\n{}\nold text\n{}\nafter\n", agents_md::START, agents_md::END);
        let updated = upsert_agents_md(Some(&old)).unwrap();
        assert!(updated.starts_with("before\n") && updated.ends_with("\nafter\n"));
        assert!(!updated.contains("old text") && updated.contains(agents_md::BODY));
        assert_eq!(upsert_agents_md(Some(&updated)), None);
    }

    /// Claude Code's files in a temp dir, as CLAUDE_CONFIG_DIR would place them.
    fn claude_in(dir: &Path, registry: Option<Value>, user_config: Option<Value>) -> ClaudePaths {
        let env = Env([(c::claude_code::CONFIG_DIR_ENV.to_string(), dir.display().to_string())].into());
        let claude = ClaudePaths::from_env(&env);
        fs::create_dir_all(dir).unwrap();
        if let Some(registry) = registry {
            fs::create_dir_all(claude.installed_plugins.parent().unwrap()).unwrap();
            fs::write(&claude.installed_plugins, registry.to_string()).unwrap();
        }
        if let Some(config) = user_config {
            fs::write(&claude.user_config, config.to_string()).unwrap();
        }
        claude
    }

    #[test]
    fn finds_the_plugin_in_the_registry() {
        let dir = crate::profiles::tests::temp_dir("plugin");
        assert!(!plugin_installed(&claude_in(&dir.join("none"), None, None)));
        let other = json!({ "version": 2, "plugins": { "other@x": [] } });
        assert!(!plugin_installed(&claude_in(&dir.join("other"), Some(other), None)));
        let jot = json!({ "version": 2, "plugins": { "jot@usejot": [{ "scope": "user" }] } });
        assert!(plugin_installed(&claude_in(&dir.join("jot"), Some(jot), None)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_leaves_mcp_json_alone_when_the_plugin_runs_the_server() {
        let dir = crate::profiles::tests::temp_dir("init-plugin");
        assert_eq!(setup_mcp(&dir, true).unwrap(), vec![messages::INIT_MCP_PLUGIN_PROVIDES]);
        assert!(!dir.join(paths::MCP_CONFIG).exists());
        assert_eq!(setup_mcp(&dir, false).unwrap(), vec![messages::INIT_MCP_WRITTEN]);
        assert!(dir.join(paths::MCP_CONFIG).exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_a_second_jot_server_and_names_its_file() {
        let root = crate::profiles::tests::temp_dir("dupes");
        let cwd = root.join("repo");
        fs::create_dir_all(&cwd).unwrap();
        let jot = json!({ "jot": { "command": "jot", "args": ["mcp"] } });

        // 1. nothing else: no warning
        let clean = claude_in(&root.join("clean"), None, Some(json!({ "mcpServers": { "other": {} } })));
        assert_eq!(other_jot_server(&clean, &cwd), None);

        // 2. top level, or this directory's project entry, in .claude.json
        let top = claude_in(&root.join("top"), None, Some(json!({ "mcpServers": jot })));
        assert_eq!(other_jot_server(&top, &cwd), Some(top.user_config.clone()));
        let projects = json!({ "projects": { cwd.to_string_lossy(): { "mcpServers": jot } } });
        let project = claude_in(&root.join("project"), None, Some(projects));
        assert_eq!(other_jot_server(&project, &cwd), Some(project.user_config.clone()));
        let elsewhere = json!({ "projects": { "/somewhere/else": { "mcpServers": jot } } });
        assert_eq!(other_jot_server(&claude_in(&root.join("elsewhere"), None, Some(elsewhere)), &cwd), None);

        // 3. the repo's .mcp.json, named in the warning
        fs::write(cwd.join(paths::MCP_CONFIG), json!({ "mcpServers": jot }).to_string()).unwrap();
        let found = other_jot_server(&clean, &cwd).unwrap();
        assert_eq!(found, cwd.join(paths::MCP_CONFIG));
        assert!(duplicate_warning(&found).contains(&format!("configured in {}", found.display())));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edited_files_are_kept_without_force() {
        let dir = crate::profiles::tests::temp_dir("init");
        let path = dir.join(paths::CLAUDE_SKILL);
        assert!(write_unless_edited(&path, skill::FILE, false).unwrap());
        assert!(write_unless_edited(&path, skill::FILE, false).unwrap());
        fs::write(&path, "mine").unwrap();
        assert!(!write_unless_edited(&path, skill::FILE, false).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "mine");
        assert!(write_unless_edited(&path, skill::FILE, true).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
