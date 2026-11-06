# jot

Your [Jot](https://usejot.co) feedback in the terminal and in your coding agent.
People pin bug reports to the live app; `jot` hands each one, with its screenshot,
element, browser and recording, to you or to Claude Code, Cursor and Codex.

## Install

```sh
npm i -g @usejot/cli     # then: jot
npx @usejot/cli login    # or run it without installing
```

One small native binary for macOS, Linux and Windows; npm picks the one for your machine.

## Start

```sh
jot login   # opens the browser: pick a workspace, done (no token to paste)
jot init    # in a repo: binds it to a project, adds `jot mcp`, /jot and the Jot skill
```

Then in Claude Code: `/jot`, `/jot <id>`, `/jot loop` (propose a fix for each new comment), `/jot loop fix`.

Work and personal accounts are profiles: `jot login --profile office`, `jot login --profile personal`,
then `jot init` in each repo picks which one it uses. `jot profile <name> <command>` runs any command as one.

## Commands

```
jot 0.1.4: your Jot feedback, in the terminal and in your coding agent.

Get started
  jot login                       connect this terminal to a workspace (opens the browser)
  jot init                        in a repo: bind it to a project and add /jot for Claude Code

Feedback
  jot list [--status S] [--json]  open feedback, one line each (--status all for every state)
  jot show <id>                   the full repro, as markdown
  jot status <id> <STATUS> [-m note]
  jot reply <id> -m "text"
  jot watch [--once] [--json] [--interval 5] [--all]
                                  wait for new feedback and print it
  jot loop [--agent claude] [--fix]
                                  hand each new comment to an agent

Profiles
  jot profiles                    list them (* is the default)
  jot use <name>                  make one the default
  jot logout <name>               forget one
  jot whoami                      the profile, workspace and projects in use
  jot profile <name> <command…>   run any command as that profile (same as --profile)

Other
  jot prompt                      instructions to paste into Cursor, Codex or any agent
  jot mcp                         the MCP server, over stdio (what .mcp.json runs)
  jot help | --version

The profile in use: --profile, then JOT_PROFILE, then the repo's .jot.json, then the default.
```

## Claude Code plugin

```
/plugin marketplace add usejot/claude-plugins
/plugin install jot@usejot
```

Docs: https://docs.usejot.co/docs/ai/cli
