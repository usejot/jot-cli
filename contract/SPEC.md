# The Jot SDK contract

Every Jot SDK, on every platform, implements this contract and nothing else. That covers the JS
frameworks, Flutter (iOS, Android, macOS, Windows, Linux, web) and React Native (iOS, Android).
This file is the prose, `contract.json` is the machine-readable copy, and `conformance/` holds the
test vectors.

**Contract version: 0.9.0** (`CONTRACT_VERSION`, `contract.json#contractVersion`)

## How a package uses it

1. Each SDK repository carries a pinned copy in `contract/`: `contract.json`, `conformance/`,
   `SPEC.md` and `VERSION`.
2. Each SDK runs every vector in `conformance/` against its own implementation, in its own
   test suite. If a vector fails, the SDK does not ship.
3. JS packages also depend on `@usejot/contract` at exactly `VERSION`, and a test asserts the
   two match.
4. Native SDKs read `contract.json` directly. Swift, Kotlin and Dart never need npm. The Dart
   constants and tables are generated from it (`contract.g.dart`), and so are the Dart data
   classes for the wire schemas (`models.g.dart`, `fromJson` / `toJson`); neither is ever
   hand-copied, and CI fails when either is stale. TypeScript gets the same shapes by `z.infer`.

To change the contract — any new behaviour, on any platform, starts here:

1. Edit `@usejot/contract` and this SPEC, and add conformance vectors.
2. Bump its version.
3. Re-sync every SDK (`sync-contract`, which also regenerates the Dart constants) and implement
   it in each one; their conformance tests must pass.
4. Release.

## Packages

| Package                | What                                                        |
| ---------------------- | ----------------------------------------------------------- |
| `@usejot/contract`     | This contract: types, constants, zod schemas, contract.json |
| `@usejot/sdk-core`     | Framework-agnostic loader. The only JS package with logic   |
| `@usejot/react`        | `<JotWidget />` / `useJot()` over sdk-core                  |
| `@usejot/next`         | React wrapper marked `"use client"` for the App Router      |
| `@usejot/vue`          | Vue 3 plugin over sdk-core                                  |
| `@usejot/nuxt`         | Nuxt module (client plugin) over sdk-core                   |
| `@usejot/svelte`       | Svelte component over sdk-core                              |
| `@usejot/angular`      | Angular provider over sdk-core                              |
| `@usejot/mcp`          | MCP server for AI coding agents                             |
| `@usejot/cli`          | The `jot` CLI (Rust): login, repo setup, feedback, watch    |
| `usejot` (pub.dev)     | Flutter SDK — every Flutter platform                        |
| `@usejot/react-native` | React Native SDK — iOS and Android                          |

## 1. Config

`JotConfig` is the only thing a developer passes to an SDK. Every key maps 1:1 onto an embed
tag attribute (`constants.configAttributes`). On platforms without a tag, the attribute name is
the wire key.

| Key           | Attribute            | Type                                                                                                                   | Notes                                                                  |
| ------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `projectKey`  | `data-jot-key`       | string, non-empty                                                                                                      | Required. The project's public widget key                              |
| `trigger`     | `data-jot-trigger`   | `"fab"` (default) \| `"tab"`                                                                                           | The logo button, or the edge tab                                       |
| `position`    | `data-jot-position`  | `"bottom"` \| `"bottom-left"` \| `"bottom-right"` \| `"left"` \| `"right"` \| `"top"` \| `"top-left"` \| `"top-right"` | Tab default `right`, FAB default `bottom-right`. See below             |
| `edge`        | `data-jot-edge`      | `"left"` \| `"right"`                                                                                                  | Deprecated alias of `position`. `position` wins                        |
| `label`       | `data-jot-label`     | string, 1–24 chars                                                                                                     | Tab text and the trigger's accessible name. Default `Feedback`         |
| `icon`        | `data-jot-icon`      | `"jot"` (default) \| `"chat"` \| `"bug"` \| `"none"`                                                                   | The trigger's glyph, drawn inline. The tab defaults to none            |
| `offset`      | `data-jot-offset`    | number 0–100                                                                                                           | Where along its edge the tab sits, in percent. Written with `String()` |
| `environment` | `data-jot-env`       | string                                                                                                                 | Written as given. The widget trims and lowercases it                   |
| `autostart`   | `data-jot-autostart` | boolean                                                                                                                | Only `false` is written: mount waits for `init()`                      |

Rules:

- **No `apiUrl`, on purpose.** The widget talks only to the origin its script was served from,
  so a page cannot point screenshots or sign-in codes somewhere else. To run elsewhere, serve
  `jot.js` from there and pass `scriptUrl`, which is loader plumbing and is never forwarded.
- **Unknown keys are rejected.** The config is a closed shape.
- **Position by trigger.** The tab takes all eight spots: `left` and `right` stand it up on that
  side, `top` and `bottom` centre it along that edge, and a corner lays it along the top or
  bottom edge near that corner. The FAB takes `bottom`, `bottom-left`, `bottom-right` and
  the three `top` spots (a bare `left`/`right` means that bottom corner). An unknown value falls
  back to the trigger's default. `edge` is read only when `position` is absent.
- `scriptUrl` defaults to `constants.defaultScriptUrl` (`https://cdn.jsdelivr.net/npm/@usejot/widget@1/dist/jot.js`).

The loader also honours `data-jot-internal="true"`, which has no config key on purpose. It
gates auto-boot so the widget mounts only on `?jot=true`, remembered per origin; `?jot=false`
forgets it. An explicit `init()` bypasses the gate, because the page's own code deciding to mount
is consent enough.

## 2. Lifecycle

Every SDK guarantees all of these:

- **Idempotent mount.** Loading twice gives one widget. The second call is a no-op that returns
  a no-op cleanup, and the first caller keeps the real cleanup.
- **Cleanup tears down.** It removes what the SDK injected and calls `Jot.destroy()`, which
  removes `#jot-root`. Removing a script tag alone never un-runs it.
- **Mount again after cleanup works.**
- **Consent gating.** With `autostart: false` the widget loads but does not mount until
  `Jot.init()`.
- **No server-side work.** With no window/DOM (SSR, a headless test runner) the SDK does
  nothing and returns a no-op cleanup. No platform UI is touched at module or import scope.

## 3. `window.Jot`

| Member          | Signature                              | Does                                          |
| --------------- | -------------------------------------- | --------------------------------------------- |
| `version`       | `"1"` (`constants.jotGlobalVersion`)   | Bumped only on a breaking change              |
| `init(config?)` | `Partial<JotConfig>` minus `autostart` | Mount now, config overriding attributes       |
| `open()`        | —                                      | Open the widget as if the trigger was clicked |
| `destroy()`     | —                                      | Unmount completely                            |

A breaking change to `window.Jot` bumps `jotGlobalVersion`, and every SDK major moves with it.

## 4. Feedback vocabulary

- **Statuses:** `OPEN` → `IN_PROGRESS` → `RESOLVED` → `APPROVED`.
- **Legal moves** (`constants.statusTransitions`): Open → In progress / Resolved; In progress →
  Open / Resolved; Resolved → Approved / Open / In progress; Approved is final. Clients offer only
  these; the API refuses anything else.
- **Approval follows the workspace's policy** (`constants.approvalPolicies`: `REPORTER`, the
  default, `MEMBERS`, or `ANYONE_INCLUDING_AGENTS`). An agent may _ask_ for any of
  `constants.agentRequestableStatuses` (`IN_PROGRESS`, `RESOLVED`, `APPROVED`); the API refuses
  `APPROVED` unless the policy is `ANYONE_INCLUDING_AGENTS`. `constants.agentSettableStatuses`
  (`IN_PROGRESS`, `RESOLVED`) is deprecated since 0.9.0 and kept unchanged for older readers.
- **Labels:** `BUG`, `UI`, `COPY`, `CONTENT`, `PERFORMANCE`, `ACCESSIBILITY`, `QUESTION`,
  `ENHANCEMENT`, `FEEDBACK`. A comment may carry several.
- **Sources:** `TEAM` (a workspace member), `INVITED` (a signed-in reviewer from outside the
  team), `PUBLIC` (an anonymous visitor). Fixed when the comment is created. A `PUBLIC` comment
  with no label picked is labelled `FEEDBACK`; a developer promotes it by removing that label.

## 5. MCP tools

A Jot MCP server exposes exactly these six tools: five that do the work, and `list_projects` so
an agent can see its reach instead of guessing. Every extra tool costs an agent accuracy, so the
list is closed. Input shapes are in `contract.json#mcpTools`.

| Tool                | Input                                                                                                                |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `fetch_feedback`    | all optional: `projectId`, `status`, `page`, `environment`, `source` (list), `q`, `label`, `limit` (1–100), `cursor` |
| `get_comment`       | `commentId` → markdown repro bundle, recording summary, dashboard link, screenshot as image content                  |
| `update_status`     | `commentId` or `commentIds` (1–50), `status` ∈ `IN_PROGRESS` \| `RESOLVED`                                           |
| `post_reply`        | `commentId`, `message` (1–5000 chars)                                                                                |
| `create_batch_note` | `comment_ids` (1–50), `message`. Atomic: one unknown id and nothing posts                                            |
| `list_projects`     | nothing → the token's reach (`GET /api/v1/agent/whoami`)                                                             |

From 0.9.0 a server's `update_status` also takes `APPROVED` (`agentRequestableStatuses`, §4);
the API applies the workspace's approval policy. `mcpTools.update_status` keeps the 0.8.0 enum.

Aliases an agent reaches for first are accepted: `id` for `commentId`, `body` or `content` for
`message`. A call that names no comment is a tool error that says what to pass.

`fetch_feedback` without `projectId` lists every project the token reaches: its one project, or
every project of the workspace for a workspace-wide token, each row naming its `project`. A
server reads the reach (`GET /api/v1/agent/whoami`) once at startup and states it in its
instructions, so an agent never needs a projectId. More than `limit` rows: `nextCursor`, passed
back as `cursor`.

`fetch_feedback` without `source` leaves out `PUBLIC` comments still labelled `FEEDBACK` — an
agent works on what the team promoted, not on every visitor's opinion. Pass `source` to ask for
them explicitly.

Every tool answers in readable text. `fetch_feedback`, `update_status`, `create_batch_note` and
`list_projects` also return structured content in the shapes at `contract.json#mcpToolOutputs`. Tools carry MCP
annotations: `fetch_feedback`, `get_comment` and `list_projects` are read-only; `update_status` is idempotent;
none is destructive. A failed call comes back as a tool error (`isError`), never a crash.

The `jot` CLI's server (`jot mcp`) serves three more, after these six: its session tools (§15).
They are not in `MCP_TOOL_NAMES`, so the six above are the same on every Jot server.

## 6. Versioning

- `@usejot/contract` has its own semver. A new optional key is a minor bump; anything a
  conforming SDK could break on is a major bump.
- The JS SDKs version in lockstep with each other and pin `@usejot/contract` exactly.

### 6.1 Changes

**0.9.0** — additive; a 0.8.0 SDK still conforms on the wire.

- **The CLI** (§15, `contract.json#cli`, `conformance/cli.json`): the `jot` CLI's env, paths,
  profile resolution, `jot login` handshake, watch and loop settings, command table, copy and file
  shapes (`CliConfig`, `RepoBinding`, `WatchState`, `McpServerEntry`), and the agent API it and
  the MCP server speak.
- **The agent guide** (§16, `contract.json#agent`): the playbook, worked examples, per-tool
  guides, the Claude Code skill and the shared error copy.
- **`agentRequestableStatuses`** and **`approvalPolicies`** (§4). `agentSettableStatuses` is
  deprecated, unchanged.

**Unreleased** — optional for every reader; a 0.6.0 SDK still conforms on the wire.

- **`jot mcp` session tools** (`cli.mcpSessionTools`, §15): `connect`, `profiles` and
  `use_profile`, served by the CLI's server only, after the six shared tools. `MCP_TOOL_NAMES`,
  `mcpTools` and `@usejot/mcp` are unchanged. With them: `cli.env.viaPlugin`, `cli.claudeCode`,
  the `mcp*` and `initMcpPluginProvides` lines in `cli.copy.messages`, and a playbook section on
  several accounts.

- **`agent.install`** and the plugin names in **`cli.packages`** (§16): the setup paths every
  surface shows, written once.

- **MCP structured output** (`McpToolOutputSchemas`, `contract.json#mcpToolOutputs`): stable
  fields next to the text for `fetch_feedback`, `update_status`, `create_batch_note` and
  `list_projects` (§5).

- **`shareLink`** (`POST /comments/:id/share`, `ShareLink`): the public link "Copy link to this
  Jot" copies (§8, `conformance/share.json`).

**0.6.0** — every addition is optional for an older reader; a 0.5.0 SDK still conforms on the
wire.

- **Native page keys drop the app id.** `app://<bundleId>/<screen>` keys as `app:///<screen>`
  (`pageKeyFor`, `conformance/page-keys.json`): the same app ships under different ids per
  platform, and a Jot left on one now shows on every other.
- **Anchor scoring is platform-neutral, with locale.** One `scoreAnchor` for every SDK; a pin
  filed in another language is matched without its label and text (`anchorTextComparable`,
  `locale` on the native context and the pin; `conformance/anchor.json`).
- **`discardRecording`** (`DELETE /recordings/:id`): a dismissed recording is deleted, and every
  binding removes its local `rec-<id>/` on every way out (§10).
- **`CAPTURE_QUALITY`** (§10.1): one medium profile for screenshots, video and FRAMES, with
  `captureScale` and `videoKbps` and `conformance/capture.json`.
- **`isCompactViewport`** is narrow OR short (`compactHeight`): a landscape phone gets the compact
  sheets (`conformance/placement.json`).
- **All inputs masked on native** (§8): every text input, secure or not, typed into or not, with
  no opt-out — the web's `maskAllInputs`, proved by a privacy test in each SDK.
- **Picker design**: `WIDGET_UI.picker.outline` (the dashed outline, its halo and its glide),
  plus the motion and design table additions every SDK now draws from.
- **Dock ellipsis**: only the name gives way (`dock.child`, `dock.whoName`); the notice is one
  ellipsised line and, on a phone, takes who's place.
- **Ellipsis glyph**: `WIDGET_ICONS.ellipsis` is three solid dots (lucide circles), drawn at 16 in
  the thread and panel menus.

## 7. Budgets (JS)

Every JS SDK entry must be under **2KB gzipped**. The widget loads lazily, so an SDK must stay a
shim. The contract's root export is zod-free for that reason; zod validators live only in
`@usejot/contract/schemas`.

## 8. Wire protocol

Every SDK speaks to the same API. Paths are relative to `wire.widgetApiPrefix`
(`/api/v1/widget`) on the API origin; request and response bodies are the JSON Schemas in
`contract.json#wire.schemas`, by the names in `wire.endpoints`.

**Headers** (`wire.headers`):

| Header           | When                    | Value                                      |
| ---------------- | ----------------------- | ------------------------------------------ |
| `x-jot-key`      | every call              | the project key                            |
| `authorization`  | once signed in          | `Bearer <token>`                           |
| `x-jot-app`      | native SDKs, every call | bundle id / package name (`com.acme.shop`) |
| `x-jot-platform` | native SDKs, every call | one of `wire.nativePlatforms`              |

A native request has no `Origin`. A project that lists `allowedAppIds` admits such a request only
when `x-jot-app` is on the list (case-insensitive); browsers stay on the domain list. An app id
can be spoofed exactly like an Origin: it is a convenience gate, never authentication — the
reviewer's token is that.

**Endpoints** (`wire.endpoints`): `session`, `bootstrap`, `authRequest`, `authVerify`, `authRefresh`,
`authExchange`, `createComment`, `members`, `forPage`, `presence`, `guideSeen`, `updatePin`, `updateStatus`,
`deleteComment`, `reply`, `shareLink`, `screenshotUploadUrl`, `recordingUploadUrl`,
`completeRecording`, `discardRecording`, `events`. `auth` is `key` (project key only), `token` (Bearer required) or `optional`.

**First-time guide.** `guideSeen` (`POST /guide-seen`, no body, `204`) records that a signed-in
person finished or skipped the guide; `session.guideSeen` then reads `true` for them on every site.
Guests are remembered on the device (`jot.guide`), and the call is a no-op for them.

**Bootstrap.** `bootstrap` (`GET /bootstrap?url=…`, auth `optional`) answers
`WidgetBootstrapResponse` `{ session, forPage }`: exactly what `session` answers, and exactly what
`forPage` would answer for the same `url`, `ids` and `scope` — one round trip instead of two
before the first pin renders. The query is `WidgetBootstrapQuery`; `ids` is `forPage`'s array as
one comma-separated value. Every `session` error (unknown key, domain allowlist,
`INTERNAL_ONLY`) is `bootstrap`'s error. `forPage` is `null` when the caller may not list pins
yet — a `MEMBERS_ONLY` or `INTERNAL` project without a valid token, or a person not invited:
render from `session`, and call `forPage` after sign-in. `session` and `forPage` keep working.
Vectors: `conformance/bootstrap.json`.

**Share links.** `shareLink` (`POST /comments/:id/share`, token required) answers `ShareLink`
`{ url, shared }` to the Jot's author or a workspace member: the Jot's public link, viewable by
anyone who has it. The thread menu's "Copy link to this Jot" copies `url` when `shared` is true. A
`NOT_FOUND`, or `shared: false`, means sharing is off for this Jot or its workspace; a member then
copies the dashboard deep link as before, anyone else gets no item. Vectors:
`conformance/share.json`.

**Errors.** Every failure, on every endpoint, is `ApiErrorEnvelope`:
`{ "error": { "code", "message", "details"? } }`. `wire.apiErrorCodes` (`ApiErrorCode`) are the
codes the API produces on purpose — `UNAUTHENTICATED`, `FORBIDDEN`, `INTERNAL_ONLY`, `NOT_FOUND`,
`CONFLICT`, `VALIDATION_FAILED`, `QUOTA_EXCEEDED`, `FEATURE_NOT_IN_PLAN`, `RATE_LIMITED`,
`PROVIDER_ERROR`, `INTERNAL`. A transport may surface its own (`HTTP_422`, `FST_ERR_*`), so
`code` is a string: branch on the known names and treat any other code as a generic failure.
`INTERNAL_ONLY` on `session` means render nothing; `QUOTA_EXCEEDED` on `createComment` shows
`composer.error.quota`.

**Masking.** `session.maskSelectors` are CSS selectors (browsers); `session.maskIds` are their
native twin — exact ids matched against a `JotMask`'s id or a `JotAnchor`'s identifier (`testID`,
`accessibilityIdentifier`, a Flutter `Key`). A native SDK masks every view whose id is listed,
on the device, before a screenshot or a frame exists, on top of what it always masks: every text
input and `JotMask`.

Every text input is masked by default, on the device, in screenshots and recordings — exactly as
the web's `maskAllInputs`. Secure or not, typed into or not (an empty field is masked too, so the
first keystroke cannot beat the mask): Flutter's `TextField` / `TextFormField` /
`CupertinoTextField` (an editable or obscured `RenderEditable`), Android's `EditText` (React
Native's `ReactEditText`), iOS's `UITextField` and editable `UITextView` (React Native's
`RCTUITextField` / `RCTUITextView`). Read-only displayed text (`SelectableText`, a non-editable
text view) is not an input. There is no opt-out, as on the web. Each SDK proves it with a native
privacy test: a known string typed into a plain, non-secure field, captured through the real mask
path, leaves the field's rect uniform mask black.

### 8.1 Sign-in

- **Email code.** `authRequest {email}` emails a 6-digit code (10 minutes, 5 attempts);
  `authVerify {email, code}` returns `WidgetAuthVerifyResponse` — a project-scoped token and the
  user. Invited people only: workspace membership is the invite list.
- **Refresh on 401** (`wire.tokenRefresh`, vectors in `conformance/token-refresh.json`). A 401
  for a request that carried a token triggers ONE `authRefresh` with that token —
  single-flight: concurrent 401s share it — then the request is retried exactly once with
  whatever token is left. A refused or failed refresh drops the token everywhere and signs the
  person out; the retry then goes out with no bearer. A 401 for a request that carried no token
  is an access answer, never a refresh.
- **Google, web.** A popup to `/api/v1/auth/google?projectKey&state` posts the token back
  through the dashboard relay page; the SDK accepts it only from `dashboardOrigin` with its own
  `state` nonce (32 hex characters).
- **Where it starts** (`wire.googleAuthStart`): `GET /api/v1/auth/google` on the API origin —
  not under `widgetApiPrefix` — with `projectKey` and `state` (the SDK's nonce, 32 lowercase hex
  characters); a native SDK adds `client=native` and `redirect`.
- **Google, native** (`wire.nativeAuth`). The SDK opens the system auth session
  (ASWebAuthenticationSession, Custom Tabs, the system browser) at
  `/api/v1/auth/google?projectKey&state&client=native&redirect=<scheme>://jot-auth`. The scheme
  must be in the project's `allowedRedirectSchemes`, or the sign-in is refused before it starts.
  An app that configures no scheme gets `defaultAuthScheme(appId)`: `nativeAuth.defaultSchemePrefix`
  (`jot.`) + the iOS bundle id or Android applicationId, lowercased, any character a scheme
  cannot hold turned into `-` (`co.acme.shop_app` → `jot.co.acme.shop-app`). The prefix keeps it
  clear of a bundle-id scheme the app may already own. Vectors: `native-redirect.json#defaultSchemes`.
- **Desktop loopback** (RFC 8252 §7.3). On `nativeAuth.loopbackPlatforms` (macOS, Windows,
  Linux) an SDK may instead listen on a free port and pass
  `redirect=http://127.0.0.1:<port>/jot-auth` (or `http://[::1]:<port>/jot-auth`). The API
  accepts it only when the project turned on `allowLoopbackAuth` (off by default): `http`, a
  literal loopback address (never `localhost`), any port, path exactly `/jot-auth`, no
  credentials. The callback then arrives at the SDK's own listener exactly as below.
  `acceptedNativeRedirect` is the rule; vectors: `conformance/native-redirect.json`.
  The callback returns to `<scheme>://jot-auth?code=<one-time-code>&state=<nonce>` — never a
  token, since another app could claim the scheme. The SDK checks `state`, then trades the code
  at `authExchange {code}` for a `WidgetAuthVerifyResponse`. A code is single-use and lives
  `codeTtlSeconds` (60). On a refusal or cancel the redirect carries `error` (`cancelled`,
  `FORBIDDEN`…) instead of `code`. `session.googleSignIn` false means the API has no Google
  credentials: hide the button.

### 8.2 Realtime

`GET /events` is a server-sent event stream (`wire.sseEvents`, name → payload schema). `ready`
arrives first. `comment/created` carries ids only — refetch `forPage`, where the server's privacy
filter decides what this viewer may see. `presence/update` is sent to signed-in viewers only.
Browsers and native SDKs both read it with a streaming HTTP client, so the `x-jot-key` header
can be sent.

## 9. Capture context

`CaptureContext` is discriminated on `source` (`wire.captureSources`). A context with no
`source` is `web` — every context written before 0.4.0.

- **`web`** — the browser widget: `url`, `domSelector`, `browser`, `os`, optional viewport,
  `device`, `pin`, `screenshotUrl`, `environment`. Unchanged since 0.3.0.
- **`mobile` / `desktop`** — a native SDK: `screen` (the route name), `platform`, `osVersion`,
  `deviceModel`, `appVersion`, `appBuild`, `locale`, `viewportWidth`/`viewportHeight` (logical
  pixels), `pixelRatio`, `anchor`, `pin`, optional `screenshotUrl`, `environment` and `crashed`.
  `url` is synthetic: `nativeContextUrl(bundleId, screen)` = `app://<bundleId>/<screen>`, each
  route segment percent-encoded, so every URL-keyed path (`forPage`, `presence`, grouping,
  analytics) works unchanged. A Jot filed with `crashed: true` is labelled `BUG`.
- **Crash reports.** `anchor` and `pin` are required on a native context unless `crashed` is
  `true`: a crash has no element under a finger. `screen` stays required — it is the last screen
  the app reported (JotKit `setCurrentScreen`) — so the report groups and keys by screen like any
  other Jot. The rule is a refinement on `CaptureContext` (an `if/then` in
  `contract.json`); `NativeCaptureContext` on its own lists the fields only.

**Page keys.** `pageKeyFor(url)` is scheme + host + path, query and fragment dropped, for any
scheme, and never the string `null`. A native `app://` url drops its host as well: one project is
one app, and that app ships under a different id per platform (`co.usejot.jotExample` on iOS,
`co.usejot.jot_example` on Android). So `app://co.acme.shop/Checkout?step=2` keys as
`app:///Checkout` on iOS, Android, macOS, Windows, Linux and Flutter web alike — one set of Jots
per screen, every device. The id is not lost: it stays on the stored `url` and travels in
`x-jot-app`. Keys written before this rule were rewritten by the API's
`native_page_key_without_app_id` migration. Vectors: `conformance/page-keys.json`,
`conformance/context.json`.

### 9.1 Pins that land on the same element, on any device

A native pin stores a fingerprint of the element under the reviewer's finger (`NativeAnchor`):
the `screen`, the ancestry `path` (root first: type, key, sibling index), `semantic` identity
(developer `identifier`, accessibility `label`, visible `text`, `role`) and `geometry` (the
element's centre as a screen ratio, a tie-breaker only), plus `pin` — the tap as ratios of the
element's box.

When the screen is shown again, every candidate element is scored with `scoreAnchor` (weights
in `contract.json#anchor`):

1. another screen scores 0;
2. both carry an `identifier` and they differ: 0;
3. otherwise each signal the stored fingerprint has scores 0–1 — identifier, label, text and role
   by normalised equality (trimmed, whitespace collapsed, lowercased), path by 1 − edit distance
   over `type#key` tokens / the longer length, index by equality of the element's own sibling
   index, geometry by 1 − distance / `geometryRadius` — and the weighted sum is divided by the
   weight of the signals present, summed in weight-table order. Label and text are one signal
   seen two ways: a stored label also matches the candidate's text and a stored text its label
   (the same React Native `<Text>` is an accessibility label on iOS and text on Android).
4. `textComparable: false` drops label and text from the score entirely, so the identifier,
   path, role, index and geometry carry it. An SDK passes
   `anchorTextComparable(context.locale, viewerLocale)`: false when the pin was filed in another
   primary language than the viewer's ("Pay now" and "Payer" are the same button).

**Paths are framework names, never OS class names.** The same app on iOS, Android, macOS,
Windows, Linux and the web must produce the same `path` for the same element. Flutter paths are
widget types, with plumbing and platform-adaptive wrappers (page transitions, scrollbars,
overscroll indicators) skipped. React Native maps host views to the component that rendered them
— `RCTViewComponentView` and `ReactViewGroup` are both `View`, `RCTParagraphComponentView` and
`ReactTextView` both `Text` — and drops OS-only wrappers; stored paths are normalised the same
way when matched, so pins filed before this rule still anchor. Developer identifiers (`testID`,
`JotAnchor`, `Semantics.identifier`) remain the strongest, locale-proof signal.

The pin itself is a ratio of the matched element's box, so a different screen size, orientation
or safe area moves it with the element. Only the approximate fallback uses the stored screen
ratio — on another device that is the same relative point of the screen, not the same element.

`bestAnchorMatch` takes the highest score at or above `threshold` (0.6), the first on a tie. No
match: show the pin at its screen ratio, marked approximate; an element that left the screen
hides its pin. Vectors: `conformance/anchor.json`.

A matched pin rides its element on **every frame** anything moves it — a scroll, a fling, the
keyboard, a transition — never only once the screen settles, exactly as the web widget's one
rAF-gated pass. Matching (the walk and the scoring) stays debounced; tracking does not. While its
element is scrolled or clipped out of sight the pin hides, and it shows again, on the element,
when the element returns.

## 10. Uploads and recordings

Bytes never pass through the API: an SDK asks for a presigned URL, PUTs straight to storage, then
tells the API.

- **Screenshots.** `screenshotUploadUrl {contentLength ≤ 5 MB}` → PUT → put `screenshotUrl` in
  the context. Masking happens on the device, before the image exists.
- **Recordings.** `recordingUploadUrl {recordingId, kind, chunkIndex, contentLength ≤ 10 MB}`
  per file, then `completeRecording` with the counts. `kind` (`wire.uploadKinds`) names the
  object `recordings/{id}/<file>-NNNNN.<ext>`:

  | `kind`     | Object                   | What                                                |
  | ---------- | ------------------------ | --------------------------------------------------- |
  | `chunk`    | `chunk-00000.json.gz`    | rrweb events (the browser widget; the default)      |
  | `timeline` | `timeline-00000.json.gz` | a gzipped JSON array of `NativeTimelineEvent`       |
  | `video`    | `video-00000.mp4`        | H.264 MP4 segments, ~10 s each, in play order       |
  | `frames`   | `frames-00000.bin`       | keyframes: `wire.framesRecord` records back to back |

  `completeRecording.format` is `RRWEB` (default — the browser widget sends none), `VIDEO` or
  `FRAMES`. For native formats `chunkCount` counts `timeline` files and `segmentCount` the
  `video` or `frames` files; `width`/`height` are the frame size; `crashed` marks a recording
  that ends in a crash.

- **Discard.** Stop opens a composer with the recording attached; leaving it without sending
  (`DISMISS`, `CLOSE`) calls `discardRecording` (`DELETE /recordings/:id`), fire-and-forget. Only
  the reviewer who completed it may (an anonymous one: only an anonymous recording), and only
  while unattached — anything else is `NOT_FOUND`. Escape and the Android back button do
  nothing in `entered.recording.composing`: one stray key must not throw a recording away. On a
  phone sheet the visible way out is a Discard link under Send
  (`WIDGET_UI.composer.order.recordingCompactFoot`): the first tap arms it
  (`record.discard.confirm`), the second is the `DISMISS`.

- **Timeline events** (`NativeTimelineEvent`, `atMs` = ms since the recording started):
  `console`, `network` (a `NetworkPayload`, `type: "native"`, redacted on the device with §12's
  policy), `screen`, `tap` (a position only — keystrokes are never recorded), `crash` (message,
  stack, fatal), `lifecycle`.

### 10.1 Capture quality

One profile, `wire.captureQuality` (`CAPTURE_QUALITY`), for every SDK: **medium** — text in the
app is crisp when a fixer watches a phone at phone size on a laptop, and a recording stays a few
MB a minute. Sizes are device pixels; a capture is never upscaled (`captureScale`), so a small
window keeps its own size. Vectors: `conformance/capture.json`.

| What       | Profile                                                                                             | Why                                                                                                                                                                                                                                                                |
| ---------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Screenshot | PNG, ≤ 2× logical (`maxPixelRatio`), long edge ≤ 2560                                               | A phone at 2× is ~800 × 1750 — 1:1 on a retina laptop. PNG keeps text and the mask edges exact; a UI screen is ~100–400 KB, far under the 5 MB cap. 2560 bounds a large desktop window.                                                                            |
| Web shot   | The viewport only, WebP q 0.82 (JPEG q 0.85 where WebP cannot encode), 1× (1.5× at dpr ≥ 2) (`web`) | The web re-renders the DOM rather than reading the screen, so retina pixels add size, not detail. A desktop viewport is ~50–250 KB. The upload declares its `contentType` (png, webp or jpeg; png when absent).                                                    |
| Video      | H.264, long edge ≤ 1280, ≤ 6 fps, key frame every 2 s, 10 s segments                                | 1280 on an 874 pt phone is 1.46× logical: 13 pt text is ~19 px tall. The old 720-px _height_ cap left a portrait phone ~330 px wide, too few pixels per glyph to read.                                                                                             |
| Bitrate    | `width × height × fps × 0.12` bits/s, clamped to 400–1500 kbps (`videoKbps`)                        | A phone frame (~590 × 1280) targets ~540 kbps. Measured on a list scrolling non-stop: 2.6 MB a minute (Android) to 4.7 (iOS), so ≤ ~10 MB for the 2-minute ring buffer and < 1 MB per 10 s segment (cap 10 MB). Static screens skip frames and come in well under. |
| FRAMES     | JPEG quality 70, long edge ≤ 1280, ≤ 2 fps (1 on the web)                                           | The Dart backend has no video encoder; only changed frames are kept. ~40–80 KB a frame: ≤ ~6 MB a minute of constant motion, a fraction of that otherwise.                                                                                                         |

Frame capture keeps the threading rule (`native/JOTKIT.md`): the main thread only asks for the
pixels; scaling, masking and encoding run off it.

## 11. Shared tables

`contract.json#tables` is the one copy of what every client renders from. SDKs generate their
constants from it; none hand-copies a row.

- **`widgetStates`, `widgetEvents`, `widgetGuards`, `widgetTransitions`** — the state machine.
  The first row matching `from` + `on` whose guard holds wins; an event with no row is ignored.
  `crash.prompt` and `crash.sending` are native only: at launch an SDK that finds a pending crash
  dispatches `CRASH_FOUND` from `collapsed`; `CRASH_SEND` uploads the recording and files a Jot
  with `crashed: true`, `CRASH_DISMISS` clears it. The web widget never dispatches `CRASH_*`.
- **`copy`** — every user-visible string, keyed `screen.element[.state]`, `{slot}`
  interpolation. **Native variants:** a key ending in `nativeCopySuffix` (`.native`) is the
  native wording of the key without it ("Tap anything on this screen" for "Click anything on
  this page"). On a native platform an SDK shows `<key>.native` when it exists, else `<key>`
  (`copyFor(key, native)`); the web never shows a variant. Vectors: `conformance/copy.json`.
- **`placement`** — pin and card geometry: card margin and gap, dock clearance, pin capsule size,
  sheet snap points and threshold, `compactWidth` (640) and `compactHeight` (500). The UI is the
  compact sheet layout when the viewport is narrower than `compactWidth` **or** shorter than
  `compactHeight` (`isCompactViewport`) — a landscape phone gets sheets, never cards squeezed into
  its short height. Vectors: `conformance/placement.json`.
- **`networkRedaction`** — what of a recorded request may leave the device: credential headers
  (by name, and any name matching `secretHeaderPart`), body and query keys whose value is
  replaced (`secretKeyPart` over the key lowercased with separators removed; `secretKeyWords` as
  whole words only), text content types, and the size limits. Vectors:
  `conformance/redaction.json`.

## 12. Platforms

`contract.json#platforms.byPlatform` lists what each platform can do (`platforms.capabilities`).
A gap is declared, never silent: an SDK reports exactly its platform's list and hides what is
absent. `nativeCrashes` are signals and uncaught platform exceptions, caught by JotKit;
`bindingCrashes` are the SDK language's own fatal errors (Dart, JavaScript), caught by the
binding — one neutral name for Flutter and React Native (it was `dartCrashes` before 0.5.0).
Windows and Linux list both recorders: when JotKit's native core reports the video capability
the SDK records `VIDEO` and catches native crashes; otherwise it falls back to the Dart backend,
which records `FRAMES` and catches binding crashes only. Flutter web always records `FRAMES` (no
hardware encoder in pure Dart) and catches binding crashes only.

## 13. Conformance vectors

| File                   | What every SDK must reproduce                                                  |
| ---------------------- | ------------------------------------------------------------------------------ |
| `config.json`          | config → embed attributes; invalid configs rejected                            |
| `mcp.json`             | MCP tool inputs accepted and rejected                                          |
| `context.json`         | `nativeContextUrl`; valid and invalid capture contexts, crashes                |
| `page-keys.json`       | `pageKeyFor`                                                                   |
| `redaction.json`       | key, header, URL and body redaction, byte for byte                             |
| `token-refresh.json`   | the refresh-on-401 call sequence                                               |
| `anchor.json`          | `scoreAnchor` and `bestAnchorMatch`, to 6 decimal places                       |
| `copy.json`            | the `.native` copy lookup                                                      |
| `native-redirect.json` | which sign-in redirects are accepted, normalised; loopback; the default scheme |
| `design.json`          | the icons, mark variants, token groups and component parts to draw             |
| `placement.json`       | `isCompactViewport`: which viewports get the compact sheet layout              |
| `capture.json`         | `captureScale` and `videoKbps`: the capture profile's frame scale and bitrate  |
| `share.json`           | the `shareLink` endpoint row; `ShareLink` responses accepted and rejected      |
| `cli.json`             | profile resolution, `.jot.json` walk-up, CLI args, `/cli/connect`, watch       |
| `mcp-tools-0.8.0.json` | `tools/list` as `@usejot/mcp@0.8.0` answered: every Jot MCP server keeps it    |

## 14. Visual parity

Every SDK looks identical — a reviewer must not be able to tell which one they are using. The
web widget is the reference implementation; `contract.json#design` is its look, as data:

- **`design.tokens`** (`WIDGET_TOKENS`) — colours (light and dark), shadows as layers
  (`x`, `y`, `blur`, `spread`, hex `color`, `alpha`), radii, the two one-sharp-corner shapes
  (`r-pin`, `r-card-anchored`), the px type ladder, durations (ms) and easings (cubic-bezier
  points). Names are the brand's CSS custom properties without `--`. The web widget is
  light-only (a guest on someone else's page); a native app may follow the system theme with
  `dark`.
- **`design.icons`** (`WIDGET_ICONS`) — SVG path data on `design.iconStyle`'s grid: viewBox 24,
  stroke width 2, round caps and joins, no fill (a `fill` icon also fills with the stroke
  colour), always drawn in the surrounding text colour. **`design.jotMark`** (`JOT_MARK`) — the
  brand mark: a filled bubble path and a dot on a 32 grid, in three variants (`accent`,
  `on-accent`, `mono`).
- **`design.ui`** (`WIDGET_UI`) — every component's metrics: the FAB and edge tab, the dock
  (and how the annotate toolbar replaces it), cards and their anchored corners, buttons, icon
  buttons, inputs and the code boxes, status pills, label chips, avatars, the pin capsule, the
  compact sheet, menus, the composer, annotate (tool order and how marks are drawn), the thread,
  panel rows, the picker's hint chip, and the intro, sign-in, boot and crash cards. Motion
  (`enter`, `exit`, `rise`, `land`, `settle`) is data too: durations and easings by token name;
  reduced motion collapses every duration to 1 ms. Numbers are logical pixels; strings
  are token names or CSS keywords; the header of `src/design.ts` defines every value shape.

Rules:

1. A native SDK renders these components from `design.ui` + `design.tokens` + `design.icons`,
   **never stock platform controls** (no Material/Cupertino buttons, sheets, menus or system
   fonts' default metrics). Platform behaviour (gestures, accessibility, keyboards) stays
   native; the look does not.
2. The web widget's own test suite parses its stylesheet and
   components and asserts every number in `design.ui` and `design.tokens` is what it renders, so
   the table cannot drift from the reference. A change to the widget's look is a contract change.
3. In the compact layout (`isCompactViewport`, §11) every card is the compact sheet; otherwise
   cards float beside their pin or the trigger, exactly as the web widget places them. A sheet's
   content scrolls inside it and its foot (Send) stays on the visible bottom, above the keyboard.
4. Every floating surface — cards, the panel, sheets, the trigger — keeps clear of the platform's
   safe area on every side (status bar, notch and display cutouts, the navigation bar, the home
   indicator): an edge margin is the design margin measured from the safe-area inset, i.e.
   `inset + margin`, so it is never less than the design margin alone. The web widget measures
   from the viewport, which a browser already keeps inside the safe area, and adds
   `env(safe-area-inset-bottom)` where it docks to the bottom.

## 15. CLI

The `jot` CLI (`@usejot/cli`, a Rust binary on npm) and the MCP server (`@usejot/mcp`) share one
set of names, files and rules, in `contract.json#cli`. The CLI's `contract.rs` and the MCP
server's `contract.g.ts` are generated from it; neither hand-copies a value. Templates in `cli`
use `{name}` placeholders, replaced verbatim.

- **Env** (`cli.env`): `JOT_PROFILE`, `JOT_API_TOKEN`, `BACKEND_API_URL` (default
  `https://api.usejot.co`), `JOT_APP_URL` (default `https://app.usejot.co`), `JOT_HOME` (default
  `~/.jot`), `JOT_VIA_PLUGIN` (`"1"` in the Claude Code plugin's server entry).
- **Files** (`cli.paths`): `<home>/config.json` (`CliConfig`, mode 0600 in a 0700 directory: it
  holds tokens), `<home>/state/<profile>.json` (`WatchState`, the profile slugified), and in a
  repo `.jot.json` (`RepoBinding`: a profile name and a project id, never a token, safe to commit),
  `.mcp.json` (`mcpServers.jot` is `McpServerEntry`, no env: `cli.mcpServerEntryLocal`, `jot mcp`,
  when the `jot` on PATH is this CLI, else `cli.mcpServerEntry`, `npx -y @usejot/mcp`; other keys
  and servers are kept, an old `env.JOT_API_TOKEN` is dropped),
  `.claude/commands/jot.md` (`cli.copy.claudeCommand`) and `.claude/skills/jot/SKILL.md`
  (`agent.skill.file`), both byte for byte and kept when edited unless `--force`, and `AGENTS.md`
  (`cli.paths.agentsMd`), whose `agent.agentsMd` section is `start`, `body`, `end`: replaced in
  place when present, appended after a blank line when not, the rest of the file untouched.
- **Profile resolution** (`cli.profileResolution`), highest first: `flag` (`--profile`, CLI only),
  `env` (`JOT_PROFILE`), `repoFile` (the nearest `.jot.json` at or above the working directory),
  `default` (the config's), `legacyEnv` (`JOT_API_TOKEN` + `BACKEND_API_URL`, named `env`). A
  name from the first three that the config lacks is an error, never a fall-through. The repo's
  `project` applies only when the profile in use is the one its `.jot.json` names, or it names
  none. Reading the config drops entries without a string `apiUrl` and `token`, and a `default`
  naming nothing.
- **`jot login`** (`cli.connect`): the CLI listens on `127.0.0.1` (only) on a free port and opens
  `<appUrl>/cli/connect?port&state&host[&name]`. The dashboard accepts a port of 4–5 digits in
  1024–65535 and a state matching `^[A-Za-z0-9_-]{16,128}$` (the CLI sends 16 random bytes as
  hex), trims `host` to 60 characters (`terminal` when empty) and `name` to 40, mints a
  workspace-wide token labelled `CLI · <host>`, and sends the browser to
  `http://127.0.0.1:<port>/callback?state&token&org&email`. The CLI checks the state, verifies the
  token against `agent/whoami`, saves the profile, and answers with `cli.copy.pages`. It gives up
  after 5 minutes.
- **Commands** (`cli.commands`, `cli.globalFlags`): name, operands, flags (boolean when `value`
  is null) and one line of help; `cli.copy.help` is the help text, and each command's `usage` is a
  line of it. `profile <name> <cmd…>` is `--profile <name> <cmd…>`.
- **`jot watch`** (`cli.watch`): polls `OPEN` comments every 5 s. The first run counts what is
  open as seen (unless `--all`); after that each unseen comment, oldest first, is marked seen and
  then shown. The seen list keeps the newest 2000 ids. **`jot loop --agent claude`**
  (`cli.loop`) runs `claude -p <cli.copy.headlessPrompt> --permission-mode plan|acceptEdits` per
  new comment; the CLI sets the statuses and posts the answer itself.
- **`jot mcp` session tools** (`cli.mcpSessionTools`: title, description, `when`, `example`,
  annotations, `inputSchema`, and `mcpDescription` composed as `agent.tools`'): `connect
  {profile?}` starts the `jot login` handshake in the background and answers at once with the
  link (`mcpConnectOpened`); the profile it saves is the one the next call uses (`mcpConnected`).
  `profiles` lists the saved profiles (name, workspace, email, API; which one this session uses
  and which is the default), never a token. `use_profile {name}` switches this session only, as
  `--profile` would, and answers with the new reach (`mcpUseProfileDone`); `jot use` still sets
  the default. Not signed in, a tool answers `mcpNotConnected`, and the server resolves the
  profile again on each call until one is found, so a sign-in finished anywhere needs no restart.
- **Claude Code** (`cli.claudeCode`), read, never written: the plugin is installed when
  `<configDir>/plugins/installed_plugins.json` (`$CLAUDE_CONFIG_DIR` when set) has the key
  `jot@usejot` under `plugins`. Then `jot init` leaves `.mcp.json` alone
  (`initMcpPluginProvides`). A server started with `JOT_VIA_PLUGIN=1`, and `jot mcp doctor` when
  the plugin is installed, look for another `jot` entry in `.claude.json` (top-level
  `mcpServers`, or `projects[<cwd>].mcpServers`) and the working directory's `.mcp.json`, and say
  `mcpDuplicateServer` naming the file: in the instructions and on stderr, or in doctor's output.
- **Agent API** (`cli.api`): the endpoints under `/api/v1` both clients call with
  `authorization: Bearer <agent token>`, and their shapes in `cli.schemas`.

## 16. Agents

`contract.json#agent` is how an agent should work a Jot queue, written once and rendered to every
surface an agent reads.

- **`agent.playbook`** (markdown): the loop (fetch → read the full repro → reproduce → fix → test
  → status and reply), what a good reply says, when to ask instead of fixing, how to use the
  selector, screenshot, console and network evidence, what never to do, and batching. An MCP
  server sends it as its instructions, after a line on the token's reach and the repo's project.
- **`agent.examples`**: worked examples, each a report, the ideal tool calls (every input valid
  for `mcpTools`, none approving) and the ideal reply; `agent.examplesMarkdown` renders them.
- **`agent.tools`**: per tool, its title, description, `when` to use it and one `example` call.
  `mcpDescription` is the description a server registers: the three joined as
  `<description>\nWhen: <when>\nExample: <example>`. Names and schemas stay in `mcpTools`.
- **`agent.skill`**: the Claude Code skill's `name`, its one-line trigger `description`, and the
  whole `file` (frontmatter, playbook, examples) that `jot init` writes.
- **`agent.errors`**: the errors the CLI and the MCP server share. Each names the next command.
- **`agent.tools[name].annotations`**: the MCP tool annotations (read-only, destructive,
  idempotent, open-world hints) a server registers.
- **`agent.agentsMd`**: the marked `AGENTS.md` section `jot init` keeps (the playbook).
- **`agent.mcp`**: what an MCP server says besides the playbook: the reach line
  (`reachProject`/`reachWorkspace`, `{projects}` as `reachProjectItem` joined by ", "), the
  scope and bound-project lines that open its instructions, each tool's answer text, and the
  `doctor` lines. `get_comment` adds the markdown's first `![Screenshot](url)` as an image
  content item when it is an image of at most `screenshotMaxBytes`, fetched within
  `screenshotTimeoutMs`. `update_status` accepts `constants.agentRequestableStatuses` (the API
  applies the workspace's approval policy). Both `@usejot/mcp` and `jot mcp` serve the same six
  tools; `jot mcp` appends its session tools (§15).
- **`agent.install`**: how a person connects an agent, for every surface that shows it:
  `claudeCode` (two commands: `/plugin marketplace add <claudeMarketplaceRepo>`, then
  `/plugin install <claudePlugin>@<claudeMarketplace>`), `cli` (`login` then `init`), `manual` (the `npx` MCP entry and its
  token variable), `chat` (announced, `status: "soon"`), and the `approval` sentence.
