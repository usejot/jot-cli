// GENERATED from @usejot/contract 0.9.0 by
// scripts/release/gen-contract-rust.mjs — do not edit. Re-run sync-contract.
//
// The contract the `jot` CLI implements: names, paths, rules, copy and
// wire shapes, all from contract.json. Templates use `{name}` placeholders,
// replaced verbatim. Declare it as `#[rustfmt::skip] mod contract;`.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The contract version this CLI implements; its `contract/VERSION` must match.
pub const CONTRACT_VERSION: &str = "0.9.0";

/// contract.json#constants (the parts a CLI needs)
pub mod constants {
    pub const COMMENT_STATUSES: &[&str] = &["OPEN", "IN_PROGRESS", "RESOLVED", "APPROVED"];
    pub const AGENT_REQUESTABLE_STATUSES: &[&str] = &["IN_PROGRESS", "RESOLVED", "APPROVED"];
    #[deprecated(note = "use AGENT_REQUESTABLE_STATUSES")]
    pub const AGENT_SETTABLE_STATUSES: &[&str] = &["IN_PROGRESS", "RESOLVED"];
    pub const APPROVAL_POLICIES: &[&str] = &["REPORTER", "MEMBERS", "ANYONE_INCLUDING_AGENTS"];
    pub const COMMENT_LABELS: &[&str] = &["BUG", "UI", "COPY", "CONTENT", "PERFORMANCE", "ACCESSIBILITY", "QUESTION", "ENHANCEMENT", "FEEDBACK"];
    pub const COMMENT_SOURCES: &[&str] = &["TEAM", "INVITED", "PUBLIC"];
    pub const MCP_TOOL_NAMES: &[&str] = &["fetch_feedback", "get_comment", "update_status", "post_reply", "create_batch_note", "list_projects"];
}

/// contract.json#agent
pub mod agent {
    pub const PLAYBOOK: &str = r#"# Working a Jot queue

Jot holds feedback people left on a live app. Each comment is pinned to the exact element, with a screenshot, the page URL and selector (or the app screen), the browser and OS, and sometimes a session recording with its console errors and failed requests. Use the jot tools (or the `jot` CLI), never curl or the REST API: they already carry the token, the ids and the screenshot.

## The loop

1. **Fetch.** `fetch_feedback` with status OPEN lists the open work. No projectId is needed unless this repo is bound to one.
2. **Read the full repro.** `get_comment` for one comment: the reporter's sentence, where it was left, the thread, the recording summary and the screenshot.
3. **Reproduce.** Find the element from the selector (or the screen) in this codebase and the route from the URL. Read the console errors and failed requests before guessing.
4. **Mark it.** `update_status` IN_PROGRESS when you start, so nobody else picks it up.
5. **Fix and test.** Make the smallest change that fixes what was reported, run the project's tests, and add one when the bug could come back.
6. **Close the loop.** `update_status` RESOLVED, then `post_reply` saying what changed. Never resolve without a reply.

## What a good reply says

- What was wrong, in one sentence the reporter understands.
- What changed: the files, and the PR or commit.
- How to check it: the page, the steps, and the browser when it mattered.

For example: "The Pay button sat under the cookie banner on Safari, so clicks never reached it. Fixed the banner's stacking in src/components/CookieBanner.tsx (PR #204). To check: open /checkout in Safari and press Pay."

## Ask instead of fixing when

- the report is unclear, or you cannot find what it points at;
- it is a design or product decision (copy, layout, pricing, scope) rather than a bug;
- the fix belongs to another repo or project, or is far bigger than the report.

Ask in the thread with `post_reply`, leave the status as it is, and tell the user.

## Using the evidence

- **Selector** (`domSelector`): search the code for its ids, classes and test ids; the closest component that renders it is where to start. A comment from a native app names the screen and the element instead.
- **Screenshot:** what the reporter saw, with their marks on it. Compare it with what the code means to render.
- **Console errors and failed requests** (the recording summary): the first error, or the first 4xx/5xx, is usually the cause. Match a request's path to its handler.
- **Browser, OS and viewport:** reproduce in the same conditions. Many reports happen in one browser or at one width only.

## Never

- Never say a fix is approved. RESOLVED means "please check", not "closed". Approval follows the workspace's policy (by default only the reporter approves): set APPROVED only when the user asks, and say it is approved only when the API accepted it.
- Never resolve a comment without a reply that says what changed.
- Never work on comments outside the project this repo is bound to, unless the user asks.
- Never paste the token anywhere, or call the API by hand.

## Several at once

When one change fixes several comments, move them together with one `update_status` (`commentIds`) and post one `create_batch_note` (`comment_ids`, `message`) instead of a reply in every thread.

## Several accounts

Work and personal Jot accounts are profiles: with the `jot` CLI's server, `profiles` lists the ones on this machine, `use_profile` switches this session to another, and `connect` signs in to one more.
"#;
    pub const EXAMPLES_MARKDOWN: &str = r#"## Examples

### 1. A button that does nothing

The report: "Pay button does nothing on Safari" · https://shop.your-app.example/checkout · button#pay · Safari 18 / macOS · recording: 12s, 1 console error (TypeError: Cannot read properties of undefined (reading 'total') at checkout.ts:88)

```text
get_comment {"commentId":"cm_pay"}
update_status {"commentId":"cm_pay","status":"IN_PROGRESS"}
update_status {"commentId":"cm_pay","status":"RESOLVED"}
post_reply {"commentId":"cm_pay","message":"Pay read the cart total before the cart had loaded, so it threw on Safari and did nothing. It now waits for the cart (src/checkout/checkout.ts, PR #204), with a test. To check: open /checkout in Safari and press Pay."}
```

The reply:

> Pay read the cart total before the cart had loaded, so it threw on Safari and did nothing. It now waits for the cart (src/checkout/checkout.ts, PR #204), with a test. To check: open /checkout in Safari and press Pay.

### 2. One fix for several comments

The report: Three COPY comments: "Sign Up should be Sign up" on /, /pricing and /signup, each pinned to the header button a.signup

```text
fetch_feedback {"status":"OPEN","label":"COPY"}
update_status {"commentIds":["cm_a","cm_b","cm_c"],"status":"IN_PROGRESS"}
update_status {"commentIds":["cm_a","cm_b","cm_c"],"status":"RESOLVED"}
create_batch_note {"comment_ids":["cm_a","cm_b","cm_c"],"message":"The header button now says \"Sign up\" everywhere: one label in src/components/Header.tsx (PR #311). To check: the header on /, /pricing and /signup."}
```

The reply:

> The header button now says "Sign up" everywhere: one label in src/components/Header.tsx (PR #311). To check: the header on /, /pricing and /signup.

### 3. A request to ask about, not fix

The report: "Make the hero more exciting" · https://your-app.example/ · section.hero · label ENHANCEMENT

```text
get_comment {"commentId":"cm_hero"}
post_reply {"commentId":"cm_hero","message":"Happy to change it. What should change: the headline, the image, or adding motion? A link to a hero you like would help."}
```

The reply:

> Happy to change it. What should change: the headline, the image, or adding motion? A link to a hero you like would help.
"#;
    /// contract.json#agent.tools
    pub mod tools {
        /// contract.json#agent.tools.fetch_feedback
        pub mod fetch_feedback {
            pub const TITLE: &str = "List feedback";
            pub const DESCRIPTION: &str = "List feedback comments. No projectId needed: with none, it lists everything the token reaches (each row names its project). Filter with status, q (text), label, page or environment. Returns up to `limit` (default 25); pass the returned nextCursor as `cursor` for more. Public visitor feedback is left out unless `source` asks for it.";
            pub const WHEN: &str = "Start here: to find open work, or to see what is left after a fix.";
            pub const EXAMPLE: &str = "fetch_feedback { \"status\": \"OPEN\" }";
            /// contract.json#agent.tools.fetch_feedback.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = true;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"List feedback comments. No projectId needed: with none, it lists everything the token reaches (each row names its project). Filter with status, q (text), label, page or environment. Returns up to `limit` (default 25); pass the returned nextCursor as `cursor` for more. Public visitor feedback is left out unless `source` asks for it.
When: Start here: to find open work, or to see what is left after a fix.
Example: fetch_feedback { "status": "OPEN" }"#;
        }
        /// contract.json#agent.tools.get_comment
        pub mod get_comment {
            pub const TITLE: &str = "Read a Jot";
            pub const DESCRIPTION: &str = "Full detail for one comment (pass commentId): the page, element and browser, the thread, the recording summary, a dashboard link, and the screenshot as an image.";
            pub const WHEN: &str = "Before touching code for a comment: it is the full repro.";
            pub const EXAMPLE: &str = "get_comment { \"commentId\": \"cm_123\" }";
            /// contract.json#agent.tools.get_comment.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = true;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"Full detail for one comment (pass commentId): the page, element and browser, the thread, the recording summary, a dashboard link, and the screenshot as an image.
When: Before touching code for a comment: it is the full repro.
Example: get_comment { "commentId": "cm_123" }"#;
        }
        /// contract.json#agent.tools.update_status
        pub mod update_status {
            pub const TITLE: &str = "Set status";
            pub const DESCRIPTION: &str = "Move a comment (commentId) or several (commentIds) to IN_PROGRESS, RESOLVED or APPROVED. Approval follows the workspace's policy (by default only the reporter approves); the API refuses if this token may not, so set RESOLVED unless the user asks you to approve.";
            pub const WHEN: &str = "IN_PROGRESS when you start; RESOLVED once the fix is in and tested, then reply.";
            pub const EXAMPLE: &str = "update_status { \"commentId\": \"cm_123\", \"status\": \"RESOLVED\" }";
            /// contract.json#agent.tools.update_status.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = false;
                pub const DESTRUCTIVE_HINT: bool = false;
                pub const IDEMPOTENT_HINT: bool = true;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"Move a comment (commentId) or several (commentIds) to IN_PROGRESS, RESOLVED or APPROVED. Approval follows the workspace's policy (by default only the reporter approves); the API refuses if this token may not, so set RESOLVED unless the user asks you to approve.
When: IN_PROGRESS when you start; RESOLVED once the fix is in and tested, then reply.
Example: update_status { "commentId": "cm_123", "status": "RESOLVED" }"#;
        }
        /// contract.json#agent.tools.post_reply
        pub mod post_reply {
            pub const TITLE: &str = "Reply in thread";
            pub const DESCRIPTION: &str = "Add a note (message) to a comment thread (commentId) so the reporter can see what happened, e.g. 'Fixed in PR #204'.";
            pub const WHEN: &str = "After every fix (what changed, how to check it), or to ask the reporter a question.";
            pub const EXAMPLE: &str = "post_reply { \"commentId\": \"cm_123\", \"message\": \"Fixed in PR #204: … To check: open /checkout and press Pay.\" }";
            /// contract.json#agent.tools.post_reply.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = false;
                pub const DESTRUCTIVE_HINT: bool = false;
                pub const IDEMPOTENT_HINT: bool = false;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"Add a note (message) to a comment thread (commentId) so the reporter can see what happened, e.g. 'Fixed in PR #204'.
When: After every fix (what changed, how to check it), or to ask the reporter a question.
Example: post_reply { "commentId": "cm_123", "message": "Fixed in PR #204: … To check: open /checkout and press Pay." }"#;
        }
        /// contract.json#agent.tools.create_batch_note
        pub mod create_batch_note {
            pub const TITLE: &str = "Post one note to several Jots";
            pub const DESCRIPTION: &str = "Report one action that spanned several comments (e.g. 'resolved 3 comments in PR #204') as a single note, linked across every thread it names. Atomic: one unknown id and nothing is posted.";
            pub const WHEN: &str = "One change fixed several comments: one note for all of them, not a reply each.";
            pub const EXAMPLE: &str = "create_batch_note { \"comment_ids\": [\"cm_a\", \"cm_b\"], \"message\": \"Fixed both in PR #311: …\" }";
            /// contract.json#agent.tools.create_batch_note.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = false;
                pub const DESTRUCTIVE_HINT: bool = false;
                pub const IDEMPOTENT_HINT: bool = false;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"Report one action that spanned several comments (e.g. 'resolved 3 comments in PR #204') as a single note, linked across every thread it names. Atomic: one unknown id and nothing is posted.
When: One change fixed several comments: one note for all of them, not a reply each.
Example: create_batch_note { "comment_ids": ["cm_a", "cm_b"], "message": "Fixed both in PR #311: …" }"#;
        }
        /// contract.json#agent.tools.list_projects
        pub mod list_projects {
            pub const TITLE: &str = "What this token reaches";
            pub const DESCRIPTION: &str = "The workspace and projects this token can read and update. You rarely need it: fetch_feedback without a projectId already covers them all.";
            pub const WHEN: &str = "Only to see which workspace and projects this token reaches.";
            pub const EXAMPLE: &str = "list_projects {}";
            /// contract.json#agent.tools.list_projects.annotations
            pub mod annotations {
                pub const READ_ONLY_HINT: bool = true;
                pub const OPEN_WORLD_HINT: bool = true;
            }
            pub const MCP_DESCRIPTION: &str = r#"The workspace and projects this token can read and update. You rarely need it: fetch_feedback without a projectId already covers them all.
When: Only to see which workspace and projects this token reaches.
Example: list_projects {}"#;
        }
    }
    /// contract.json#agent.skill
    pub mod skill {
        pub const NAME: &str = "jot";
        pub const DESCRIPTION: &str = "Work through Jot feedback: bug reports and change requests that people pinned to elements of the live app, with screenshots and recordings. Use when the user mentions Jot, feedback, a bug report, a pin or a comment id, or asks what is open, to fix reported issues, or to reply to reviewers.";
        pub const FILE: &str = r#"---
name: jot
description: Work through Jot feedback: bug reports and change requests that people pinned to elements of the live app, with screenshots and recordings. Use when the user mentions Jot, feedback, a bug report, a pin or a comment id, or asks what is open, to fix reported issues, or to reply to reviewers.
---

# Working a Jot queue

Jot holds feedback people left on a live app. Each comment is pinned to the exact element, with a screenshot, the page URL and selector (or the app screen), the browser and OS, and sometimes a session recording with its console errors and failed requests. Use the jot tools (or the `jot` CLI), never curl or the REST API: they already carry the token, the ids and the screenshot.

## The loop

1. **Fetch.** `fetch_feedback` with status OPEN lists the open work. No projectId is needed unless this repo is bound to one.
2. **Read the full repro.** `get_comment` for one comment: the reporter's sentence, where it was left, the thread, the recording summary and the screenshot.
3. **Reproduce.** Find the element from the selector (or the screen) in this codebase and the route from the URL. Read the console errors and failed requests before guessing.
4. **Mark it.** `update_status` IN_PROGRESS when you start, so nobody else picks it up.
5. **Fix and test.** Make the smallest change that fixes what was reported, run the project's tests, and add one when the bug could come back.
6. **Close the loop.** `update_status` RESOLVED, then `post_reply` saying what changed. Never resolve without a reply.

## What a good reply says

- What was wrong, in one sentence the reporter understands.
- What changed: the files, and the PR or commit.
- How to check it: the page, the steps, and the browser when it mattered.

For example: "The Pay button sat under the cookie banner on Safari, so clicks never reached it. Fixed the banner's stacking in src/components/CookieBanner.tsx (PR #204). To check: open /checkout in Safari and press Pay."

## Ask instead of fixing when

- the report is unclear, or you cannot find what it points at;
- it is a design or product decision (copy, layout, pricing, scope) rather than a bug;
- the fix belongs to another repo or project, or is far bigger than the report.

Ask in the thread with `post_reply`, leave the status as it is, and tell the user.

## Using the evidence

- **Selector** (`domSelector`): search the code for its ids, classes and test ids; the closest component that renders it is where to start. A comment from a native app names the screen and the element instead.
- **Screenshot:** what the reporter saw, with their marks on it. Compare it with what the code means to render.
- **Console errors and failed requests** (the recording summary): the first error, or the first 4xx/5xx, is usually the cause. Match a request's path to its handler.
- **Browser, OS and viewport:** reproduce in the same conditions. Many reports happen in one browser or at one width only.

## Never

- Never say a fix is approved. RESOLVED means "please check", not "closed". Approval follows the workspace's policy (by default only the reporter approves): set APPROVED only when the user asks, and say it is approved only when the API accepted it.
- Never resolve a comment without a reply that says what changed.
- Never work on comments outside the project this repo is bound to, unless the user asks.
- Never paste the token anywhere, or call the API by hand.

## Several at once

When one change fixes several comments, move them together with one `update_status` (`commentIds`) and post one `create_batch_note` (`comment_ids`, `message`) instead of a reply in every thread.

## Several accounts

Work and personal Jot accounts are profiles: with the `jot` CLI's server, `profiles` lists the ones on this machine, `use_profile` switches this session to another, and `connect` signs in to one more.

## Examples

### 1. A button that does nothing

The report: "Pay button does nothing on Safari" · https://shop.your-app.example/checkout · button#pay · Safari 18 / macOS · recording: 12s, 1 console error (TypeError: Cannot read properties of undefined (reading 'total') at checkout.ts:88)

```text
get_comment {"commentId":"cm_pay"}
update_status {"commentId":"cm_pay","status":"IN_PROGRESS"}
update_status {"commentId":"cm_pay","status":"RESOLVED"}
post_reply {"commentId":"cm_pay","message":"Pay read the cart total before the cart had loaded, so it threw on Safari and did nothing. It now waits for the cart (src/checkout/checkout.ts, PR #204), with a test. To check: open /checkout in Safari and press Pay."}
```

The reply:

> Pay read the cart total before the cart had loaded, so it threw on Safari and did nothing. It now waits for the cart (src/checkout/checkout.ts, PR #204), with a test. To check: open /checkout in Safari and press Pay.

### 2. One fix for several comments

The report: Three COPY comments: "Sign Up should be Sign up" on /, /pricing and /signup, each pinned to the header button a.signup

```text
fetch_feedback {"status":"OPEN","label":"COPY"}
update_status {"commentIds":["cm_a","cm_b","cm_c"],"status":"IN_PROGRESS"}
update_status {"commentIds":["cm_a","cm_b","cm_c"],"status":"RESOLVED"}
create_batch_note {"comment_ids":["cm_a","cm_b","cm_c"],"message":"The header button now says \"Sign up\" everywhere: one label in src/components/Header.tsx (PR #311). To check: the header on /, /pricing and /signup."}
```

The reply:

> The header button now says "Sign up" everywhere: one label in src/components/Header.tsx (PR #311). To check: the header on /, /pricing and /signup.

### 3. A request to ask about, not fix

The report: "Make the hero more exciting" · https://your-app.example/ · section.hero · label ENHANCEMENT

```text
get_comment {"commentId":"cm_hero"}
post_reply {"commentId":"cm_hero","message":"Happy to change it. What should change: the headline, the image, or adding motion? A link to a hero you like would help."}
```

The reply:

> Happy to change it. What should change: the headline, the image, or adding motion? A link to a hero you like would help.
"#;
    }
    /// contract.json#agent.errors
    pub mod errors {
        pub const NOT_CONNECTED: &str = "Not connected to Jot yet. Run `jot login` (or `npx @usejot/cli login`), or set JOT_API_TOKEN to an agent token from Jot → Settings → Agents.";
        pub const PROFILE_NOT_FOUND: &str = "No profile named \"{name}\". Run `jot login --profile {name}`, or `jot profiles` to see yours.";
        pub const PROFILE_UNKNOWN: &str = "No profile named \"{name}\". `jot profiles` lists them.";
        pub const TOKEN_REFUSED: &str = "The Jot API refused the token ({message}). It may be revoked: run `jot login` again, or make a new one in Jot → Settings → Agents.";
        pub const API_UNREACHABLE: &str = "Could not reach the Jot API at {apiUrl} ({message}). Check your connection, or BACKEND_API_URL when self-hosting.";
        pub const DOCTOR_LEGACY_HINT: &str = "  API: {apiUrl}. Check the token in Jot → Settings → Agents (revoked or mistyped?).";
        pub const DOCTOR_PROFILE_HINT: &str = "  API: {apiUrl}. Profile {name}: revoked? Run `npx @usejot/cli login` again.";
        pub const MISSING_COMMENT_ID: &str = "Name the comment: pass commentId (the id fetch_feedback returned).";
        pub const MISSING_MESSAGE: &str = "Pass the note as message.";
        pub const NOT_BOUND: &str = "This repo is not bound to a Jot project yet. Run `jot init` in it, or pass projectId.";
    }
    /// contract.json#agent.agentsMd
    pub mod agents_md {
        pub const START: &str = "<!-- jot -->";
        pub const END: &str = "<!-- /jot -->";
        pub const BODY: &str = r#"# Working a Jot queue

Jot holds feedback people left on a live app. Each comment is pinned to the exact element, with a screenshot, the page URL and selector (or the app screen), the browser and OS, and sometimes a session recording with its console errors and failed requests. Use the jot tools (or the `jot` CLI), never curl or the REST API: they already carry the token, the ids and the screenshot.

## The loop

1. **Fetch.** `fetch_feedback` with status OPEN lists the open work. No projectId is needed unless this repo is bound to one.
2. **Read the full repro.** `get_comment` for one comment: the reporter's sentence, where it was left, the thread, the recording summary and the screenshot.
3. **Reproduce.** Find the element from the selector (or the screen) in this codebase and the route from the URL. Read the console errors and failed requests before guessing.
4. **Mark it.** `update_status` IN_PROGRESS when you start, so nobody else picks it up.
5. **Fix and test.** Make the smallest change that fixes what was reported, run the project's tests, and add one when the bug could come back.
6. **Close the loop.** `update_status` RESOLVED, then `post_reply` saying what changed. Never resolve without a reply.

## What a good reply says

- What was wrong, in one sentence the reporter understands.
- What changed: the files, and the PR or commit.
- How to check it: the page, the steps, and the browser when it mattered.

For example: "The Pay button sat under the cookie banner on Safari, so clicks never reached it. Fixed the banner's stacking in src/components/CookieBanner.tsx (PR #204). To check: open /checkout in Safari and press Pay."

## Ask instead of fixing when

- the report is unclear, or you cannot find what it points at;
- it is a design or product decision (copy, layout, pricing, scope) rather than a bug;
- the fix belongs to another repo or project, or is far bigger than the report.

Ask in the thread with `post_reply`, leave the status as it is, and tell the user.

## Using the evidence

- **Selector** (`domSelector`): search the code for its ids, classes and test ids; the closest component that renders it is where to start. A comment from a native app names the screen and the element instead.
- **Screenshot:** what the reporter saw, with their marks on it. Compare it with what the code means to render.
- **Console errors and failed requests** (the recording summary): the first error, or the first 4xx/5xx, is usually the cause. Match a request's path to its handler.
- **Browser, OS and viewport:** reproduce in the same conditions. Many reports happen in one browser or at one width only.

## Never

- Never say a fix is approved. RESOLVED means "please check", not "closed". Approval follows the workspace's policy (by default only the reporter approves): set APPROVED only when the user asks, and say it is approved only when the API accepted it.
- Never resolve a comment without a reply that says what changed.
- Never work on comments outside the project this repo is bound to, unless the user asks.
- Never paste the token anywhere, or call the API by hand.

## Several at once

When one change fixes several comments, move them together with one `update_status` (`commentIds`) and post one `create_batch_note` (`comment_ids`, `message`) instead of a reply in every thread.

## Several accounts

Work and personal Jot accounts are profiles: with the `jot` CLI's server, `profiles` lists the ones on this machine, `use_profile` switches this session to another, and `connect` signs in to one more.
"#;
    }
    /// contract.json#agent.mcp
    pub mod mcp {
        pub const SERVER_NAME: &str = "jot";
        pub const REACH_PROJECT: &str = "This token reaches one project: {projects}, in {org}.";
        pub const REACH_WORKSPACE: &str = "This token reaches every project in {org}: {projects}.";
        pub const REACH_PROJECT_ITEM: &str = "\"{name}\" ({id})";
        pub const SCOPE_KNOWN: &str = "{reach} You never need a projectId: fetch_feedback without one covers all of it.";
        pub const SCOPE_UNKNOWN: &str = "You never need a projectId: fetch_feedback without one lists everything this token reaches.";
        pub const BOUND_PROJECT: &str = "This repo is bound to project {projectId}: pass it as projectId to fetch_feedback unless the user asks for other projects.";
        pub const NO_COMMENTS: &str = "No comments match that filter.";
        pub const MORE_COMMENTS: &str = "More: call again with cursor \"{cursor}\".";
        pub const OPEN_IN_JOT: &str = "Open in Jot: {link}";
        pub const STATUS_UPDATED: &str = "{id} is now {status}.";
        pub const REPLY_POSTED: &str = "Reply posted.";
        pub const BATCH_NOTE_POSTED: &str = "Note posted to {count} comments.";
        pub const UNREACHABLE: &str = "The Jot API could not be reached.";
        pub const DOCTOR_REACHABLE: &str = "✓ API reachable at {apiUrl}";
        pub const DOCTOR_ACCEPTED: &str = "✓ Token accepted{which}. {reach}";
        pub const DOCTOR_PROFILE: &str = " (profile {name})";
        pub const DOCTOR_PASTE: &str = r#"
Paste into your agent's MCP config (.mcp.json, .cursor/mcp.json, …):
"#;
        pub const DOCTOR_RESTART: &str = r#"
Then restart the agent so the tools load."#;
        pub const DOCTOR_TOKEN_PLACEHOLDER: &str = "<your token>";
        pub const SCREENSHOT_MAX_BYTES: u64 = 1500000;
        pub const SCREENSHOT_TIMEOUT_MS: u64 = 8000;
    }
    /// contract.json#agent.install
    pub mod install {
        /// contract.json#agent.install.claudeCode
        pub mod claude_code {
            pub const LABEL: &str = "Claude Code";
            pub const COMMANDS: &[&str] = &["/plugin marketplace add usejot/claude-plugins", "/plugin install jot@usejot"];
            pub const NOTE: &str = "Then ask Claude to connect Jot (it opens your browser), or paste an agent token when Claude Code asks.";
        }
        /// contract.json#agent.install.cli
        pub mod cli {
            pub const LABEL: &str = "Cursor, Codex, Windsurf, any agent";
            pub const COMMANDS: &[&str] = &["npx @usejot/cli login", "npx @usejot/cli init"];
            pub const NOTE: &str = "`login` opens your browser to pick a workspace. `init` connects the repo. No token to paste.";
        }
        /// contract.json#agent.install.manual
        pub mod manual {
            pub const LABEL: &str = "By hand";
            /// contract.json#agent.install.manual.mcpServer
            pub mod mcp_server {
                pub const COMMAND: &str = "npx";
                pub const ARGS: &[&str] = &["-y", "@usejot/mcp"];
            }
            pub const TOKEN_ENV: &str = "JOT_API_TOKEN";
            pub const NOTE: &str = "Any MCP client: run `npx -y @usejot/mcp` with JOT_API_TOKEN set to an agent token from Settings → Agents.";
        }
        /// contract.json#agent.install.chat
        pub mod chat {
            pub const LABEL: &str = "claude.ai and ChatGPT";
            pub const STATUS: &str = "soon";
        }
        pub const APPROVAL: &str = "Agents reply and resolve; who approves is your workspace's choice (by default, the person who reported it).";
    }
}

/// contract.json#mcpTools, #mcpToolOutputs and #agent.tools, as JSON: what `jot mcp` serves.
pub mod mcp {
    pub const INPUT_SCHEMAS: &str = "{\"fetch_feedback\":{\"type\":\"object\",\"properties\":{\"projectId\":{\"type\":\"string\",\"description\":\"Optional. Leave it out: the token's own project, or every project of a workspace token. Never look one up\"},\"status\":{\"type\":\"string\",\"enum\":[\"OPEN\",\"IN_PROGRESS\",\"RESOLVED\",\"APPROVED\"],\"description\":\"Only return comments in this state\"},\"page\":{\"type\":\"string\",\"description\":\"Only comments made on this page — any URL on the page works\"},\"environment\":{\"type\":\"string\",\"description\":\"Only comments captured on this deployment, e.g. \\\"staging\\\"\"},\"source\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"enum\":[\"TEAM\",\"INVITED\",\"PUBLIC\"]},\"minItems\":1,\"description\":\"Only comments from these reporters. Omitted: public feedback still tagged FEEDBACK is left out\"},\"q\":{\"type\":\"string\",\"maxLength\":200,\"description\":\"Text search over the comment and its replies\"},\"label\":{\"type\":\"string\",\"enum\":[\"BUG\",\"UI\",\"COPY\",\"CONTENT\",\"PERFORMANCE\",\"ACCESSIBILITY\",\"QUESTION\",\"ENHANCEMENT\",\"FEEDBACK\"],\"description\":\"Only comments with this label, e.g. BUG\"},\"limit\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":100,\"description\":\"How many to return, 1–100 (default 25)\"},\"cursor\":{\"type\":\"string\",\"description\":\"The nextCursor of the previous page, to read on past it\"}},\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"get_comment\":{\"type\":\"object\",\"properties\":{\"commentId\":{\"type\":\"string\",\"minLength\":1,\"description\":\"The comment id (from fetch_feedback)\"},\"id\":{\"type\":\"string\",\"minLength\":1,\"description\":\"Same as commentId\"}},\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"update_status\":{\"type\":\"object\",\"properties\":{\"commentId\":{\"type\":\"string\",\"minLength\":1,\"description\":\"The comment id (from fetch_feedback)\"},\"id\":{\"type\":\"string\",\"minLength\":1,\"description\":\"Same as commentId\"},\"commentIds\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"minLength\":1},\"minItems\":1,\"maxItems\":50,\"description\":\"Several comments at once, instead of commentId\"},\"status\":{\"type\":\"string\",\"enum\":[\"IN_PROGRESS\",\"RESOLVED\"]}},\"required\":[\"status\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"post_reply\":{\"type\":\"object\",\"properties\":{\"commentId\":{\"type\":\"string\",\"minLength\":1,\"description\":\"The comment id (from fetch_feedback)\"},\"id\":{\"type\":\"string\",\"minLength\":1,\"description\":\"Same as commentId\"},\"message\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":5000,\"description\":\"The note to post\"},\"body\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":5000,\"description\":\"Same as message\"},\"content\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":5000,\"description\":\"Same as message\"}},\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"create_batch_note\":{\"type\":\"object\",\"properties\":{\"comment_ids\":{\"type\":\"array\",\"items\":{\"type\":\"string\"},\"minItems\":1,\"maxItems\":50},\"message\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":5000}},\"required\":[\"comment_ids\",\"message\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"list_projects\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"}}";
    pub const OUTPUT_SCHEMAS: &str = "{\"fetch_feedback\":{\"type\":\"object\",\"properties\":{\"comments\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"string\"},\"status\":{\"type\":\"string\",\"enum\":[\"OPEN\",\"IN_PROGRESS\",\"RESOLVED\",\"APPROVED\"]},\"source\":{\"type\":\"string\",\"enum\":[\"TEAM\",\"INVITED\",\"PUBLIC\"]},\"labels\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}},\"body\":{\"type\":\"string\"},\"where\":{\"type\":\"string\"},\"project\":{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"string\"},\"name\":{\"type\":\"string\"}},\"required\":[\"id\",\"name\"],\"additionalProperties\":false}},\"required\":[\"id\",\"status\",\"source\",\"labels\",\"body\",\"where\"],\"additionalProperties\":false}},\"nextCursor\":{\"type\":[\"string\",\"null\"]}},\"required\":[\"comments\",\"nextCursor\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"update_status\":{\"type\":\"object\",\"properties\":{\"updated\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"string\"},\"status\":{\"type\":\"string\",\"enum\":[\"OPEN\",\"IN_PROGRESS\",\"RESOLVED\",\"APPROVED\"]}},\"required\":[\"id\",\"status\"],\"additionalProperties\":false}}},\"required\":[\"updated\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"create_batch_note\":{\"type\":\"object\",\"properties\":{\"replyIds\":{\"type\":\"array\",\"items\":{\"type\":\"string\"}}},\"required\":[\"replyIds\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"},\"list_projects\":{\"type\":\"object\",\"properties\":{\"org\":{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"string\"},\"name\":{\"type\":\"string\"}},\"required\":[\"id\",\"name\"],\"additionalProperties\":false},\"scope\":{\"type\":\"string\",\"enum\":[\"project\",\"workspace\"]},\"projects\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"id\":{\"type\":\"string\"},\"name\":{\"type\":\"string\"}},\"required\":[\"id\",\"name\"],\"additionalProperties\":false}},\"dashboardUrl\":{\"type\":\"string\",\"format\":\"uri\"}},\"required\":[\"org\",\"scope\",\"projects\",\"dashboardUrl\"],\"additionalProperties\":false,\"$schema\":\"http://json-schema.org/draft-07/schema#\"}}";
    pub const TOOLS: &str = "{\"fetch_feedback\":{\"title\":\"List feedback\",\"description\":\"List feedback comments. No projectId needed: with none, it lists everything the token reaches (each row names its project). Filter with status, q (text), label, page or environment. Returns up to `limit` (default 25); pass the returned nextCursor as `cursor` for more. Public visitor feedback is left out unless `source` asks for it.\",\"when\":\"Start here: to find open work, or to see what is left after a fix.\",\"example\":\"fetch_feedback { \\\"status\\\": \\\"OPEN\\\" }\",\"annotations\":{\"readOnlyHint\":true,\"openWorldHint\":true},\"mcpDescription\":\"List feedback comments. No projectId needed: with none, it lists everything the token reaches (each row names its project). Filter with status, q (text), label, page or environment. Returns up to `limit` (default 25); pass the returned nextCursor as `cursor` for more. Public visitor feedback is left out unless `source` asks for it.\\nWhen: Start here: to find open work, or to see what is left after a fix.\\nExample: fetch_feedback { \\\"status\\\": \\\"OPEN\\\" }\"},\"get_comment\":{\"title\":\"Read a Jot\",\"description\":\"Full detail for one comment (pass commentId): the page, element and browser, the thread, the recording summary, a dashboard link, and the screenshot as an image.\",\"when\":\"Before touching code for a comment: it is the full repro.\",\"example\":\"get_comment { \\\"commentId\\\": \\\"cm_123\\\" }\",\"annotations\":{\"readOnlyHint\":true,\"openWorldHint\":true},\"mcpDescription\":\"Full detail for one comment (pass commentId): the page, element and browser, the thread, the recording summary, a dashboard link, and the screenshot as an image.\\nWhen: Before touching code for a comment: it is the full repro.\\nExample: get_comment { \\\"commentId\\\": \\\"cm_123\\\" }\"},\"update_status\":{\"title\":\"Set status\",\"description\":\"Move a comment (commentId) or several (commentIds) to IN_PROGRESS, RESOLVED or APPROVED. Approval follows the workspace's policy (by default only the reporter approves); the API refuses if this token may not, so set RESOLVED unless the user asks you to approve.\",\"when\":\"IN_PROGRESS when you start; RESOLVED once the fix is in and tested, then reply.\",\"example\":\"update_status { \\\"commentId\\\": \\\"cm_123\\\", \\\"status\\\": \\\"RESOLVED\\\" }\",\"annotations\":{\"readOnlyHint\":false,\"destructiveHint\":false,\"idempotentHint\":true,\"openWorldHint\":true},\"mcpDescription\":\"Move a comment (commentId) or several (commentIds) to IN_PROGRESS, RESOLVED or APPROVED. Approval follows the workspace's policy (by default only the reporter approves); the API refuses if this token may not, so set RESOLVED unless the user asks you to approve.\\nWhen: IN_PROGRESS when you start; RESOLVED once the fix is in and tested, then reply.\\nExample: update_status { \\\"commentId\\\": \\\"cm_123\\\", \\\"status\\\": \\\"RESOLVED\\\" }\"},\"post_reply\":{\"title\":\"Reply in thread\",\"description\":\"Add a note (message) to a comment thread (commentId) so the reporter can see what happened, e.g. 'Fixed in PR #204'.\",\"when\":\"After every fix (what changed, how to check it), or to ask the reporter a question.\",\"example\":\"post_reply { \\\"commentId\\\": \\\"cm_123\\\", \\\"message\\\": \\\"Fixed in PR #204: … To check: open /checkout and press Pay.\\\" }\",\"annotations\":{\"readOnlyHint\":false,\"destructiveHint\":false,\"idempotentHint\":false,\"openWorldHint\":true},\"mcpDescription\":\"Add a note (message) to a comment thread (commentId) so the reporter can see what happened, e.g. 'Fixed in PR #204'.\\nWhen: After every fix (what changed, how to check it), or to ask the reporter a question.\\nExample: post_reply { \\\"commentId\\\": \\\"cm_123\\\", \\\"message\\\": \\\"Fixed in PR #204: … To check: open /checkout and press Pay.\\\" }\"},\"create_batch_note\":{\"title\":\"Post one note to several Jots\",\"description\":\"Report one action that spanned several comments (e.g. 'resolved 3 comments in PR #204') as a single note, linked across every thread it names. Atomic: one unknown id and nothing is posted.\",\"when\":\"One change fixed several comments: one note for all of them, not a reply each.\",\"example\":\"create_batch_note { \\\"comment_ids\\\": [\\\"cm_a\\\", \\\"cm_b\\\"], \\\"message\\\": \\\"Fixed both in PR #311: …\\\" }\",\"annotations\":{\"readOnlyHint\":false,\"destructiveHint\":false,\"idempotentHint\":false,\"openWorldHint\":true},\"mcpDescription\":\"Report one action that spanned several comments (e.g. 'resolved 3 comments in PR #204') as a single note, linked across every thread it names. Atomic: one unknown id and nothing is posted.\\nWhen: One change fixed several comments: one note for all of them, not a reply each.\\nExample: create_batch_note { \\\"comment_ids\\\": [\\\"cm_a\\\", \\\"cm_b\\\"], \\\"message\\\": \\\"Fixed both in PR #311: …\\\" }\"},\"list_projects\":{\"title\":\"What this token reaches\",\"description\":\"The workspace and projects this token can read and update. You rarely need it: fetch_feedback without a projectId already covers them all.\",\"when\":\"Only to see which workspace and projects this token reaches.\",\"example\":\"list_projects {}\",\"annotations\":{\"readOnlyHint\":true,\"openWorldHint\":true},\"mcpDescription\":\"The workspace and projects this token can read and update. You rarely need it: fetch_feedback without a projectId already covers them all.\\nWhen: Only to see which workspace and projects this token reaches.\\nExample: list_projects {}\"}}";
    /// contract.json#cli.mcpSessionTools: `jot mcp`'s own tools, served after the shared six, in this order.
    pub const SESSION_TOOL_NAMES: &[&str] = &["connect", "profiles", "use_profile"];
    pub const SESSION_TOOLS: &str = "{\"connect\":{\"title\":\"Connect Jot\",\"description\":\"Sign in to Jot: opens the person's browser to pick a workspace, then saves it as a profile on this machine. Returns at once with the link; once they have picked, the next jot call uses the new profile. Pass profile to name it (e.g. work).\",\"when\":\"Not connected yet, or the person wants another account (work and personal, say).\",\"example\":\"connect { \\\"profile\\\": \\\"work\\\" }\",\"annotations\":{\"readOnlyHint\":false,\"destructiveHint\":false,\"idempotentHint\":false,\"openWorldHint\":true},\"inputSchema\":{\"type\":\"object\",\"properties\":{\"profile\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":40,\"description\":\"Optional. Save it under this name; else the workspace's name\"}},\"additionalProperties\":false},\"mcpDescription\":\"Sign in to Jot: opens the person's browser to pick a workspace, then saves it as a profile on this machine. Returns at once with the link; once they have picked, the next jot call uses the new profile. Pass profile to name it (e.g. work).\\nWhen: Not connected yet, or the person wants another account (work and personal, say).\\nExample: connect { \\\"profile\\\": \\\"work\\\" }\"},\"profiles\":{\"title\":\"Jot profiles\",\"description\":\"The Jot accounts saved on this machine (work and personal, say): each one's workspace, email and API, which one this session uses and which is the default. Never a token.\",\"when\":\"The person has more than one account, or asks which one is in use.\",\"example\":\"profiles {}\",\"annotations\":{\"readOnlyHint\":true,\"openWorldHint\":false},\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"mcpDescription\":\"The Jot accounts saved on this machine (work and personal, say): each one's workspace, email and API, which one this session uses and which is the default. Never a token.\\nWhen: The person has more than one account, or asks which one is in use.\\nExample: profiles {}\"},\"use_profile\":{\"title\":\"Switch Jot profile\",\"description\":\"Use another saved profile (name, from profiles) for the rest of this session, and say what it reaches. The default stays: `jot use <name>` in a terminal changes it.\",\"when\":\"The person wants another of their accounts for this session.\",\"example\":\"use_profile { \\\"name\\\": \\\"work\\\" }\",\"annotations\":{\"readOnlyHint\":false,\"destructiveHint\":false,\"idempotentHint\":true,\"openWorldHint\":true},\"inputSchema\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"description\":\"The profile, as profiles lists it\"}},\"required\":[\"name\"],\"additionalProperties\":false},\"mcpDescription\":\"Use another saved profile (name, from profiles) for the rest of this session, and say what it reaches. The default stays: `jot use <name>` in a terminal changes it.\\nWhen: The person wants another of their accounts for this session.\\nExample: use_profile { \\\"name\\\": \\\"work\\\" }\"}}";
}

/// A CLI flag. `value` is the placeholder it takes; None for a boolean flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    pub name: &'static str,
    pub short: Option<&'static str>,
    pub value: Option<&'static str>,
    pub help: &'static str,
}

/// A CLI command. `usage` is its line in `cli::copy::HELP`, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    pub group: &'static str,
    pub usage: &'static str,
    pub args: &'static [&'static str],
    pub flags: &'static [Flag],
    pub help: &'static str,
}

/// An agent API endpoint. `path` is relative to `cli::api::PREFIX`; `:x` segments are URL-encoded parameters; `request`/`response` name a struct below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub name: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub request: Option<&'static str>,
    pub response: Option<&'static str>,
}

/// contract.json#cli
pub mod cli {
    use super::{Command, Endpoint, Flag};

    /// contract.json#cli.packages
    pub mod packages {
        pub const CLI: &str = "@usejot/cli";
        pub const MCP: &str = "@usejot/mcp";
        pub const BIN: &str = "jot";
        pub const MCP_BIN: &str = "jot-mcp";
        pub const CLAUDE_PLUGIN: &str = "jot";
        pub const CLAUDE_MARKETPLACE: &str = "usejot";
        pub const CLAUDE_MARKETPLACE_REPO: &str = "usejot/claude-plugins";
    }

    /// contract.json#cli.env
    pub mod env {
        /// contract.json#cli.env.profile
        pub mod profile {
            pub const NAME: &str = "JOT_PROFILE";
            pub const DEFAULT: Option<&str> = None;
        }
        /// contract.json#cli.env.apiToken
        pub mod api_token {
            pub const NAME: &str = "JOT_API_TOKEN";
            pub const DEFAULT: Option<&str> = None;
        }
        /// contract.json#cli.env.apiUrl
        pub mod api_url {
            pub const NAME: &str = "BACKEND_API_URL";
            pub const DEFAULT: &str = "https://api.usejot.co";
        }
        /// contract.json#cli.env.appUrl
        pub mod app_url {
            pub const NAME: &str = "JOT_APP_URL";
            pub const DEFAULT: &str = "https://app.usejot.co";
        }
        /// contract.json#cli.env.home
        pub mod home {
            pub const NAME: &str = "JOT_HOME";
            pub const DEFAULT: &str = "~/.jot";
        }
        /// contract.json#cli.env.viaPlugin
        pub mod via_plugin {
            pub const NAME: &str = "JOT_VIA_PLUGIN";
            pub const DEFAULT: Option<&str> = None;
        }
    }

    /// contract.json#cli.paths
    pub mod paths {
        pub const CONFIG: &str = "config.json";
        pub const CONFIG_MODE: &str = "0600";
        pub const HOME_MODE: &str = "0700";
        pub const STATE: &str = "state/<profile>.json";
        pub const REPO_BINDING: &str = ".jot.json";
        pub const MCP_CONFIG: &str = ".mcp.json";
        pub const CLAUDE_COMMAND: &str = ".claude/commands/jot.md";
        pub const CLAUDE_SKILL: &str = ".claude/skills/jot/SKILL.md";
        pub const AGENTS_MD: &str = "AGENTS.md";
    }

    pub const PROFILE_RESOLUTION: &[&str] = &["flag", "env", "repoFile", "default", "legacyEnv"];

    /// contract.json#cli.profiles
    pub mod profiles {
        pub const LEGACY_ENV_NAME: &str = "env";
        pub const SLUG_PATTERN: &str = "[^a-z0-9]+";
        pub const SLUG_FALLBACK: &str = "default";
        pub const FIRST_SUFFIX: u64 = 2;
    }

    /// contract.json#cli.connect
    pub mod connect {
        pub const DASHBOARD_PATH: &str = "/cli/connect";
        pub const QUERY_PARAMS: &[&str] = &["port", "state", "host", "name"];
        pub const CALLBACK_PATH: &str = "/callback";
        pub const CALLBACK_PARAMS: &[&str] = &["state", "token", "org", "email"];
        pub const LOOPBACK_HOST: &str = "127.0.0.1";
        pub const PORT_MIN: u64 = 1024;
        pub const PORT_MAX: u64 = 65535;
        pub const PORT_PATTERN: &str = "^\\d{4,5}$";
        pub const STATE_MIN_LENGTH: u64 = 16;
        pub const STATE_MAX_LENGTH: u64 = 128;
        pub const STATE_PATTERN: &str = "^[A-Za-z0-9_-]{16,128}$";
        pub const STATE_BYTES: u64 = 16;
        pub const HOST_MAX_LENGTH: u64 = 60;
        pub const HOST_FALLBACK: &str = "terminal";
        pub const NAME_MAX_LENGTH: u64 = 40;
        pub const TOKEN_LABEL_PREFIX: &str = "CLI · ";
        pub const TIMEOUT_SECONDS: u64 = 300;
    }

    /// contract.json#cli.watch
    pub mod watch {
        pub const INTERVAL_SECONDS: u64 = 5;
        pub const MIN_INTERVAL_SECONDS: u64 = 1;
        pub const STATUS: &str = "OPEN";
        pub const LIST_LIMIT: u64 = 100;
        pub const SEEN_CAP: u64 = 2000;
    }

    /// contract.json#cli.loop
    pub mod r#loop {
        pub const AGENTS: &[&str] = &["claude"];
        pub const COMMAND: &str = "claude";
        pub const ARGS: &[&str] = &["-p", "{prompt}", "--permission-mode", "{mode}"];
        /// contract.json#cli.loop.permissionModes
        pub mod permission_modes {
            pub const PROPOSE: &str = "plan";
            pub const FIX: &str = "acceptEdits";
        }
        pub const INSTALL_URL: &str = "https://claude.com/claude-code";
    }

    /// contract.json#cli.mcpServerEntry
    pub mod mcp_server_entry {
        pub const COMMAND: &str = "npx";
        pub const ARGS: &[&str] = &["-y", "@usejot/mcp"];
    }

    /// contract.json#cli.mcpServerEntryLocal
    pub mod mcp_server_entry_local {
        pub const COMMAND: &str = "jot";
        pub const ARGS: &[&str] = &["mcp"];
    }

    /// contract.json#cli.claudeCode
    pub mod claude_code {
        pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
        pub const CONFIG_DIR: &str = ".claude";
        pub const USER_CONFIG: &str = ".claude.json";
        pub const INSTALLED_PLUGINS: &str = "plugins/installed_plugins.json";
    }

    /// contract.json#cli.copy
    pub mod copy {
        pub const HELP: &str = r#"jot {version}: your Jot feedback, in the terminal and in your coding agent.

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

The profile in use: --profile, then JOT_PROFILE, then the repo's .jot.json, then the default."#;
        pub const CLAUDE_COMMAND: &str = r#"---
description: Work through Jot feedback (list, fix one, or loop on new ones)
argument-hint: "[comment id | loop | loop fix]"
---

Arguments: $ARGUMENTS

Jot holds feedback people left on the live app: each comment is pinned to a page element, with a screenshot, browser details and sometimes a session recording. Use the jot MCP tools (fetch_feedback, get_comment, update_status, post_reply). If they are not loaded, the `jot` CLI does the same: `jot list`, `jot show <id>`, `jot status <id> <STATUS> -m "…"`, `jot reply <id> -m "…"`.

- No argument: list the open feedback (fetch_feedback with status OPEN, or `jot list`). Show one line each and ask which to take.
- A comment id: get_comment for the full repro, find the cause in this codebase, fix it, run the project's tests, then update_status RESOLVED and post_reply saying what changed (files, PR link).
- `loop`: repeat until stopped:
  1. Run `jot watch --once` with the Bash tool. It waits for the next new comment and prints it.
  2. update_status IN_PROGRESS on it.
  3. Propose the change as a short plan and wait for the user's OK before editing anything.
  4. Once approved: implement, run the tests, update_status RESOLVED and post_reply with what changed.
  5. Go back to step 1.
- `loop fix`: the same loop, but implement straight away instead of waiting for an OK.

RESOLVED means "please check", not "closed". Approval follows the workspace's policy and the API refuses if you may not approve: set APPROVED only when the user asks, and never say a fix is approved unless the API accepted it.
"#;
        pub const AGENT_PROMPT: &str = r#"Work through my Jot feedback. When I name a comment id, fix that one; when I say "loop", keep going as new comments arrive.

Jot holds feedback people left on the live app: each comment is pinned to a page element, with a screenshot, browser details and sometimes a session recording. Use the jot MCP tools (fetch_feedback, get_comment, update_status, post_reply). If they are not loaded, the `jot` CLI does the same: `jot list`, `jot show <id>`, `jot status <id> <STATUS> -m "…"`, `jot reply <id> -m "…"`.

- No argument: list the open feedback (fetch_feedback with status OPEN, or `jot list`). Show one line each and ask which to take.
- A comment id: get_comment for the full repro, find the cause in this codebase, fix it, run the project's tests, then update_status RESOLVED and post_reply saying what changed (files, PR link).
- `loop`: repeat until stopped:
  1. Run `jot watch --once` with the Bash tool. It waits for the next new comment and prints it.
  2. update_status IN_PROGRESS on it.
  3. Propose the change as a short plan and wait for the user's OK before editing anything.
  4. Once approved: implement, run the tests, update_status RESOLVED and post_reply with what changed.
  5. Go back to step 1.
- `loop fix`: the same loop, but implement straight away instead of waiting for an OK.

RESOLVED means "please check", not "closed". Approval follows the workspace's policy and the API refuses if you may not approve: set APPROVED only when the user asks, and never say a fix is approved unless the API accepted it.
"#;
        pub const HEADLESS_PROMPT: &str = r#"A reviewer left this Jot feedback (comment {commentId}) on the live app:

{markdown}

{task}"#;
        pub const HEADLESS_TASK_FIX: &str = "Fix it in this codebase now. Run the project's tests if you are allowed to. Finish with a short summary of what you changed (files, and why): that summary is posted to the reporter as a reply.";
        pub const HEADLESS_TASK_PROPOSE: &str = "Do not edit any file. Work out the cause and propose the change as a short plan: which files, what changes, how to verify. Your answer is posted on the comment as a reply for the team to approve.";
        pub const PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>{title}</title><body style=\"font:16px system-ui;max-width:28rem;margin:20vh auto;text-align:center\"><h1 style=\"font-size:1.25rem\">{title}</h1><p>{body}</p></body>";
        /// contract.json#cli.copy.pages
        pub mod pages {
            /// contract.json#cli.copy.pages.connected
            pub mod connected {
                pub const TITLE: &str = "Connected";
                pub const BODY: &str = "Your terminal is connected to Jot. You can close this tab.";
            }
            /// contract.json#cli.copy.pages.wrongState
            pub mod wrong_state {
                pub const TITLE: &str = "That link is not for this terminal";
                pub const BODY: &str = "Run <code>jot login</code> again.";
            }
            /// contract.json#cli.copy.pages.failed
            pub mod failed {
                pub const TITLE: &str = "Could not connect";
                pub const BODY: &str = "The terminal says why. Run <code>jot login</code> again.";
            }
        }
        /// contract.json#cli.copy.messages
        pub mod messages {
            pub const ERROR: &str = "✗ {message}";
            pub const USAGE: &str = "Usage: {usage}";
            pub const UNKNOWN_COMMAND: &str = "Unknown command \"{command}\". `jot help` lists them.";
            pub const FLAG_NEEDS_VALUE: &str = "--{name} needs a value.";
            pub const PROFILE_COMMAND_USAGE: &str = "Usage: jot profile <name> <command…>, e.g. jot profile office list";
            pub const API_ERROR: &str = "Jot API returned {status}";
            pub const LOGIN_OPENING: &str = r#"Opening your browser to connect this terminal. If it does not open, visit:
"#;
            pub const LOGIN_URL: &str = r#"  {url}
"#;
            pub const LOGIN_TIMEOUT: &str = "Timed out after 5 minutes. Run `jot login` again.";
            pub const LOGIN_TOKEN_REFUSED: &str = "The token came back but did not work: {message}";
            pub const LOGIN_CONNECTED: &str = "✓ Connected to {org}{who}. Profile \"{name}\"{isDefault}.";
            pub const LOGIN_WHO: &str = " as {email}";
            pub const LOGIN_IS_DEFAULT: &str = " (default)";
            pub const LOGIN_NOT_DEFAULT: &str = " (not the default: `jot use {name}`)";
            pub const LOGIN_NEXT: &str = "Next: in a repo, run `jot init`.";
            pub const PROFILES_EMPTY: &str = "No profiles yet. Run `jot login`.";
            pub const PROFILES_ROW: &str = "{mark} {name} {details}";
            pub const USE_DONE: &str = "✓ \"{name}\" is the default profile.";
            pub const LOGOUT_DONE: &str = "✓ Forgot \"{name}\". To revoke its token too: Jot → Settings → Agents.";
            pub const WHOAMI_PROFILE: &str = "Profile    {name}{api}";
            pub const WHOAMI_API: &str = " ({apiUrl})";
            pub const WHOAMI_WORKSPACE: &str = "Workspace  {org}";
            pub const WHOAMI_REACH: &str = "Reach      {reach}";
            pub const WHOAMI_REACH_PROJECT: &str = "one project";
            pub const WHOAMI_REACH_WORKSPACE: &str = "every project";
            pub const WHOAMI_PROJECT: &str = "  {id}  {name}{bound}";
            pub const WHOAMI_BOUND: &str = "  ← this repo";
            pub const INIT_NEEDS_SAVED_PROFILE: &str = "`jot init` binds a saved profile. Run `jot login` first.";
            pub const INIT_CHOOSE_PROJECT: &str = "Which project is this repo? ({org})";
            pub const INIT_CHOICE: &str = "  {n}. {name}";
            pub const INIT_CHOICE_ALL: &str = "  0. all of them";
            pub const INIT_PROMPT: &str = "> ";
            pub const INIT_CHOICE_INVALID: &str = "Type a number from 0 to {max}.";
            pub const INIT_INVALID_JSON: &str = "{path} is not valid JSON. Fix or remove it, then run `jot init` again.";
            pub const INIT_ALL_PROJECTS: &str = "all projects";
            pub const INIT_BINDING_WRITTEN: &str = "✓ .jot.json: profile \"{profile}\", {project}. No secrets, safe to commit.";
            pub const INIT_MCP_WRITTEN: &str = "✓ .mcp.json: the jot server, no token inside.";
            pub const INIT_MCP_PLUGIN_PROVIDES: &str = "✓ .mcp.json: left alone. The Jot plugin for Claude Code already runs the jot server.";
            pub const INIT_MCP_TOKEN_REMOVED: &str = "  (removed the old JOT_API_TOKEN from it; the profile carries the token now)";
            pub const INIT_COMMAND_KEPT: &str = "! .claude/commands/jot.md exists and differs; left it alone. `jot init --force` replaces it.";
            pub const INIT_COMMAND_WRITTEN: &str = "✓ .claude/commands/jot.md: the /jot command.";
            pub const INIT_SKILL_KEPT: &str = "! .claude/skills/jot/SKILL.md exists and differs; left it alone. `jot init --force` replaces it.";
            pub const INIT_SKILL_WRITTEN: &str = "✓ .claude/skills/jot/SKILL.md: the jot skill, how to work the queue.";
            pub const INIT_AGENTS_MD_WRITTEN: &str = "✓ AGENTS.md: the jot section, for Codex, Cursor and other agents.";
            pub const INIT_AGENTS_MD_CURRENT: &str = "✓ AGENTS.md: the jot section is up to date.";
            pub const INIT_DONE: &str = r#"
Done. In Claude Code type /jot (restart it first if it was open, so the jot server loads)."#;
            pub const LIST_UNKNOWN_STATUS: &str = "Unknown status \"{status}\". Use {statuses} or all.";
            pub const LIST_EMPTY: &str = "Nothing here. New feedback shows up the moment someone leaves it: `jot watch`.";
            pub const LIST_ROW: &str = r#"{id}  [{status}]  {body}
    {where}{project}"#;
            pub const LIST_ROW_PROJECT: &str = "  · {name}";
            pub const STATUS_DONE: &str = "✓ {id} → {status}";
            pub const STATUS_NOTE_POSTED: &str = "✓ note posted";
            pub const REPLY_DONE: &str = "✓ replied on {id}";
            pub const WATCH_STARTED: &str = "Watching {profile} for new feedback… (Ctrl+C to stop)";
            pub const WATCH_BACKLOG: &str = "({count} already open; `jot list` shows them. Watching for new ones.)";
            pub const WATCH_RETRY: &str = "! {message}. Trying again in {seconds}s.";
            pub const WATCH_INTERVAL_INVALID: &str = "--interval is in seconds, 1 or more.";
            pub const WATCH_ITEM: &str = r#"Jot {id}

{markdown}
"#;
            pub const LOOP_INSIDE_AGENT: &str = "Inside Claude Code, type /jot loop (propose, then wait for your OK) or /jot loop fix.";
            pub const LOOP_INIT_FIRST: &str = "Run `jot init` in the repo first if /jot is not there yet.";
            pub const LOOP_HEADLESS: &str = "Headless instead: jot loop --agent claude [--fix]";
            pub const LOOP_AGENT_UNSUPPORTED: &str = "Only --agent claude is supported for now (got \"{agent}\").";
            pub const LOOP_AGENT_MISSING: &str = "Claude Code is not on your PATH. Install it from {url}.";
            pub const LOOP_STARTED: &str = "Looping on {profile} ({mode} mode). Ctrl+C to stop.";
            pub const LOOP_FAILED: &str = "✗ {id}  Claude exited {code}; left IN_PROGRESS.";
            pub const LOOP_RESOLVED: &str = "✓ {id}  resolved: {summary}";
            pub const LOOP_PROPOSED: &str = "• {id}  plan posted: {summary}";
            pub const LOOP_PROPOSED_REPLY: &str = r#"Proposed fix (not applied yet):

{output}"#;
            pub const MCP_NOT_CONNECTED: &str = "Not connected to Jot yet. Call the `connect` tool: it opens the person's browser to pick a workspace. Or they run `jot login` in a terminal.";
            pub const MCP_CONNECT_OPENED: &str = r#"I opened the browser to connect Jot. Ask the person to pick the workspace there and tell you when it is done; the next jot call then uses it. If no browser opened, they can visit:
{url}"#;
            pub const MCP_CONNECT_FAILED: &str = "Connecting Jot did not finish: {message}";
            pub const MCP_CONNECTED: &str = "Connected: profile \"{name}\". {reach}";
            pub const MCP_PROFILES_EMPTY: &str = "No profiles saved on this machine yet. Call `connect` to sign in.";
            pub const MCP_PROFILES_ROW: &str = "- {name}{marks}: {details}";
            pub const MCP_PROFILE_IN_USE: &str = " (this session)";
            pub const MCP_PROFILE_DEFAULT: &str = " (default)";
            pub const MCP_PROFILES_HINT: &str = "use_profile switches this session; `jot use <name>` in a terminal changes the default.";
            pub const MCP_USE_PROFILE_DONE: &str = "This session now uses profile \"{name}\". {reach} The default is unchanged: `jot use {name}` in a terminal makes it the default.";
            pub const MCP_DUPLICATE_SERVER: &str = "Another `jot` server is configured in {file}; remove it so the plugin's is the only one.";
        }
    }

    /// contract.json#cli.globalFlags: every command takes these.
    pub const GLOBAL_FLAGS: &[Flag] = &[
        Flag { name: "profile", short: Some("p"), value: Some("name"), help: "run as this profile" },
        Flag { name: "help", short: Some("h"), value: None, help: "print the help" },
        Flag { name: "version", short: Some("v"), value: None, help: "print the version" },
    ];

    /// contract.json#cli.commands, in help order.
    pub const COMMANDS: &[Command] = &[
        Command { name: "login", group: "start", usage: "jot login", args: &[], flags: &[Flag { name: "profile", short: Some("p"), value: Some("name"), help: "save as this profile name" }, Flag { name: "api", short: None, value: Some("url"), help: "the API (default BACKEND_API_URL, then the hosted one)" }, Flag { name: "app", short: None, value: Some("url"), help: "the dashboard (default JOT_APP_URL, then the hosted one)" }], help: "connect this terminal to a workspace (opens the browser)" },
        Command { name: "init", group: "start", usage: "jot init", args: &[], flags: &[Flag { name: "project", short: None, value: Some("id"), help: "bind this project instead of asking" }, Flag { name: "force", short: None, value: None, help: "replace an edited /jot command or jot skill" }], help: "in a repo: bind it to a project and add /jot for Claude Code" },
        Command { name: "list", group: "feedback", usage: "jot list [--status S] [--json]", args: &[], flags: &[Flag { name: "status", short: None, value: Some("S"), help: "OPEN (default), IN_PROGRESS, RESOLVED, APPROVED or all" }, Flag { name: "json", short: None, value: None, help: "print JSON" }], help: "open feedback, one line each (--status all for every state)" },
        Command { name: "show", group: "feedback", usage: "jot show <id>", args: &["id"], flags: &[], help: "the full repro, as markdown" },
        Command { name: "status", group: "feedback", usage: "jot status <id> <STATUS> [-m note]", args: &["id", "STATUS"], flags: &[Flag { name: "message", short: Some("m"), value: Some("note"), help: "post this note too" }], help: "move it to IN_PROGRESS, RESOLVED or APPROVED; the API applies the workspace's policy" },
        Command { name: "reply", group: "feedback", usage: "jot reply <id> -m \"text\"", args: &["id"], flags: &[Flag { name: "message", short: Some("m"), value: Some("text"), help: "the reply" }], help: "reply in the thread" },
        Command { name: "watch", group: "feedback", usage: "jot watch [--once] [--json] [--interval 5] [--all]", args: &[], flags: &[Flag { name: "once", short: None, value: None, help: "stop after the first new comment" }, Flag { name: "json", short: None, value: None, help: "one JSON object per line" }, Flag { name: "interval", short: None, value: Some("seconds"), help: "poll interval, 1 or more (default 5)" }, Flag { name: "all", short: None, value: None, help: "on the first run, count what is already open as new" }], help: "wait for new feedback and print it" },
        Command { name: "loop", group: "feedback", usage: "jot loop [--agent claude] [--fix]", args: &[], flags: &[Flag { name: "agent", short: None, value: Some("claude"), help: "run headless with this agent" }, Flag { name: "fix", short: None, value: None, help: "let the agent edit, then resolve" }, Flag { name: "interval", short: None, value: Some("seconds"), help: "poll interval, 1 or more (default 5)" }, Flag { name: "all", short: None, value: None, help: "on the first run, count what is already open as new" }], help: "hand each new comment to an agent" },
        Command { name: "profiles", group: "profiles", usage: "jot profiles", args: &[], flags: &[], help: "list them (* is the default)" },
        Command { name: "use", group: "profiles", usage: "jot use <name>", args: &["name"], flags: &[], help: "make one the default" },
        Command { name: "logout", group: "profiles", usage: "jot logout <name>", args: &["name"], flags: &[], help: "forget one" },
        Command { name: "whoami", group: "profiles", usage: "jot whoami", args: &[], flags: &[], help: "the profile, workspace and projects in use" },
        Command { name: "profile", group: "profiles", usage: "jot profile <name> <command…>", args: &["name", "command…"], flags: &[], help: "run any command as that profile (same as --profile)" },
        Command { name: "prompt", group: "other", usage: "jot prompt", args: &[], flags: &[], help: "instructions to paste into Cursor, Codex or any agent" },
        Command { name: "mcp", group: "other", usage: "jot mcp", args: &[], flags: &[], help: "the MCP server, over stdio (what .mcp.json runs)" },
        Command { name: "help", group: "other", usage: "jot help | --version", args: &[], flags: &[], help: "this help, or the version" },
    ];

    /// contract.json#cli.api: the agent HTTP API. Every call sends `authorization: Bearer <token>`.
    pub mod api {
        use super::Endpoint;

        pub const PREFIX: &str = "/api/v1";
        pub const WHOAMI: Endpoint = Endpoint { name: "whoami", method: "GET", path: "/agent/whoami", request: None, response: Some("AgentWhoamiResponse") };
        pub const LIST_COMMENTS: Endpoint = Endpoint { name: "listComments", method: "GET", path: "/comments", request: None, response: Some("AgentCommentList") };
        pub const LIST_PROJECT_COMMENTS: Endpoint = Endpoint { name: "listProjectComments", method: "GET", path: "/projects/:projectId/comments", request: None, response: Some("AgentCommentList") };
        pub const GET_COMMENT: Endpoint = Endpoint { name: "getComment", method: "GET", path: "/comments/:commentId", request: None, response: Some("AgentComment") };
        pub const UPDATE_STATUS: Endpoint = Endpoint { name: "updateStatus", method: "PATCH", path: "/comments/:commentId/status", request: Some("AgentStatusRequest"), response: Some("AgentStatusUpdated") };
        pub const REPLY: Endpoint = Endpoint { name: "reply", method: "POST", path: "/comments/:commentId/replies", request: Some("AgentReplyRequest"), response: Some("AgentReplyCreated") };
        pub const BATCH_NOTE: Endpoint = Endpoint { name: "batchNote", method: "POST", path: "/comments/batch-note", request: Some("AgentBatchNoteRequest"), response: Some("AgentBatchNoteCreated") };
        pub const MARKDOWN: Endpoint = Endpoint { name: "markdown", method: "GET", path: "/comments/:commentId/markdown", request: None, response: Some("AgentMarkdown") };
        pub const MINT_ORG_AGENT_TOKEN: Endpoint = Endpoint { name: "mintOrgAgentToken", method: "POST", path: "/orgs/:orgId/agent-tokens", request: None, response: None };
        pub const ENDPOINTS: &[Endpoint] = &[WHOAMI, LIST_COMMENTS, LIST_PROJECT_COMMENTS, GET_COMMENT, UPDATE_STATUS, REPLY, BATCH_NOTE, MARKDOWN, MINT_ORG_AGENT_TOKEN];
    }
}

/// contract.json#cli.schemas.NamedRef
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedRef {
    pub id: String,
    pub name: String,
}

/// contract.json#cli.schemas.CliProfile
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliProfile {
    pub api_url: String,
    pub token: String,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// contract.json#cli.schemas.CliConfig
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConfig {
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub profiles: BTreeMap<String, CliProfile>,
}

/// contract.json#cli.schemas.RepoBinding
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoBinding {
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// contract.json#cli.schemas.WatchState
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    pub seen: Vec<String>,
}

/// contract.json#cli.schemas.McpServerEntry
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    pub command: String,
    pub args: Vec<String>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
}

/// contract.json#cli.schemas.AgentWhoamiResponse
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWhoamiResponse {
    pub org: NamedRef,
    /// One of: "project", "workspace".
    pub scope: String,
    pub projects: Vec<NamedRef>,
    pub dashboard_url: String,
}

/// contract.json#cli.schemas.AgentCommentLabel
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommentLabel {
    pub label: String,
}

/// contract.json#cli.schemas.AgentComment
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentComment {
    pub id: String,
    pub body: String,
    /// One of: "OPEN", "IN_PROGRESS", "RESOLVED", "APPROVED".
    pub status: String,
    /// One of: "TEAM", "INVITED", "PUBLIC".
    pub source: String,
    pub created_at: String,
    pub labels: Vec<AgentCommentLabel>,
    pub context: serde_json::Map<String, serde_json::Value>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<NamedRef>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

/// contract.json#cli.schemas.AgentCommentList
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommentList {
    pub comments: Vec<AgentComment>,
    /// Always present; null when there is none.
    pub next_cursor: Option<String>,
}

/// contract.json#cli.schemas.AgentReply
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReply {
    pub body: String,
    pub source: String,
    pub created_at: String,
}

/// contract.json#cli.schemas (a field of AgentCommentDetail)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommentDetailRecording {
    pub id: String,
    pub duration_ms: f64,
    pub console_error_count: f64,
    pub network_failure_count: f64,
}

/// contract.json#cli.schemas (a field of AgentCommentDetail)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommentDetailAuthor {
    pub name: String,
    pub email: String,
}

/// contract.json#cli.schemas.AgentCommentDetail
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommentDetail {
    pub id: String,
    pub body: String,
    /// One of: "OPEN", "IN_PROGRESS", "RESOLVED", "APPROVED".
    pub status: String,
    /// One of: "TEAM", "INVITED", "PUBLIC".
    pub source: String,
    pub created_at: String,
    pub labels: Vec<AgentCommentLabel>,
    pub context: serde_json::Map<String, serde_json::Value>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<NamedRef>,
    /// Optional: left out of the JSON when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub replies: Vec<AgentReply>,
    /// Always present; null when there is none.
    pub recording: Option<AgentCommentDetailRecording>,
    /// Always present; null when there is none.
    pub author: Option<AgentCommentDetailAuthor>,
}

/// contract.json#cli.schemas.AgentMarkdown
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMarkdown {
    pub markdown: String,
}

/// contract.json#cli.schemas.AgentStatusRequest
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusRequest {
    /// One of: "IN_PROGRESS", "RESOLVED", "APPROVED".
    pub status: String,
}

/// contract.json#cli.schemas.AgentStatusUpdated
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusUpdated {
    pub id: String,
    /// One of: "OPEN", "IN_PROGRESS", "RESOLVED", "APPROVED".
    pub status: String,
}

/// contract.json#cli.schemas.AgentReplyRequest
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplyRequest {
    pub body: String,
}

/// contract.json#cli.schemas.AgentReplyCreated
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReplyCreated {
    pub id: String,
}

/// contract.json#cli.schemas.AgentBatchNoteRequest
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBatchNoteRequest {
    pub comment_ids: Vec<String>,
    pub body: String,
}

/// contract.json#cli.schemas.AgentBatchNoteCreated
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBatchNoteCreated {
    pub batch_id: String,
    pub reply_ids: Vec<String>,
}
