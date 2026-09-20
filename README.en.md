<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="Coding Tools MCP icon">
</p>

<h1 align="center">Coding Tools MCP</h1>

<p align="center">
  Turn a local project into a persistent AI development workspace that carries context across conversations.
</p>

<p align="center">
  <a href="https://github.com/lengsukq/coding-tools-mcp/releases/latest"><img src="https://img.shields.io/github/v/release/lengsukq/coding-tools-mcp?label=Release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-x64-0078D4?logo=windows" alt="Windows x64">
  <img src="https://img.shields.io/badge/macOS-Apple%20Silicon-000000?logo=apple" alt="macOS Apple Silicon">
  <a href="https://www.apache.org/licenses/LICENSE-2.0"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0"></a>
</p>

<p align="center">
  <a href="README.md">中文</a> · <a href="README.en.md">English</a> · <a href="https://github.com/lengsukq/coding-tools-mcp/releases/latest">Download latest</a>
</p>

Coding Tools MCP is a **local-first AI coding runtime** built with Rust and Tauri 2. The desktop app exposes one Global MCP endpoint, while local projects are registered as isolated Workspaces. Each Chat Session selects the Workspace it should operate on. Through MCP, an AI agent can read and edit code, run commands and tests, inspect Git, maintain Goals / Plans, and preserve development progress inside the project.

It is closer to a long-lived AI development workbench than a separate MCP server per project: **Workspace defines the project boundary, Planning carries intent and progress, Execution / Verification carries runtime evidence, and History enables cross-conversation recovery.**

![Coding Tools MCP workspace overview](docs/images/workspace-overview.png)

*Current workspace overview: branch state, whole-workspace Diff, active Sessions, verification evidence, 7-day AI activity, Git Health, Goal / Plan progress, and execution quality are visible together.*

## v0.3.3: recent improvements

The 0.3.x line focuses less on adding isolated tools and more on turning multi-workspace AI development into one coherent workflow:

- **One Global MCP for every Workspace**: since 0.3.0, the app no longer runs a separate MCP / Tunnel stack per project. A single `/mcp` endpoint serves all registered Workspaces, while Chat Sessions select projects through `workspace_list` / `workspace_select`.
- **More resilient Workspace session routing**: 0.3.2 improved Workspace selection for clients that do not reliably preserve the transport session. For explicit one-off operations against another project, `workspace_invoke` avoids mutating other Chat Sessions.
- **A real Workspace Overview**: current branch, first-level nested Git repositories, changed files, active Sessions, verification evidence, recent commit, 7-day AI Activity, Git Health, Goal / Plan progress, and execution errors are surfaced together. The page can also generate a whole-workspace Diff / Review and open the project in a detected IDE.
- **Development state directly in the sidebar**: Workspace rows show branch and clean / changed state, the focused Goal / Plan, progress, and execution status. Workspace switching uses cache-first loading with background refresh to reduce repeated requests and UI stalls.
- **Dashboard upgraded from service status to an AI workbench**: AI Executions, Verification, Token Flow, Chat → Workspace routing, Planning completion, Workspace Token Distribution, recent History Sessions, and attention-required issues are summarized globally.
- **Responsive UI refresh**: 0.3.3 unifies Apple / iOS-style design tokens, light and dark themes, form and status components, and responsive layouts across Dashboard, Workspace, and Planning screens.

The recommended path is now straightforward: **connect one Global MCP → select a Workspace → maintain Goal / Plan → execute and verify → preserve History.**

## Feature overview: characteristics and advantages

Coding Tools MCP is more than an MCP URL forwarder. It is a **Workspace-first** AI development runtime: the desktop app manages projects and services, one tool runtime performs controlled operations, and MCP provides the client-facing entry point.

| Feature | Main characteristics | Practical advantages |
| --- | --- | --- |
| Workspace management | Each project has its own directory, execution policy, Planning, and History; the sidebar shows branch, changes, and current Goal / Plan | Clearer multi-project switching and less risk of sending a command to the wrong project |
| MCP tool runtime | Files, patches, commands, Git, images, Skills, and state management share one core | A single protocol surface reduces duplicated state machines and compatibility branches |
| Connectivity and public entry points | Local endpoints, Global Gateway, FRP, and Cloudflare Tunnel | The same workspace can serve local development and remote ChatGPT access |
| Authentication | OAuth Authorization Code, PKCE S256, DCR, and Refresh Tokens, with Bearer and static-client compatibility | Modern OAuth flows are available without abandoning older clients or simple deployments |
| Planning / Goal / Task | Direct, Plan, and Goal modes with one Execution Ledger | Complex work can be decomposed, resumed, verified, and handed off with less drift |
| Conversation history | Lossless Markdown archives plus bounded state, search, pagination, and startup prompts | Context follows the repository instead of one chat window, without injecting every old message |
| Logs and health checks | Inspect requests, endpoints, OAuth metadata, and connectivity results in the desktop app | Faster diagnosis of whether a problem is local service, tunnel, authentication, or client-side |
| Workspace-first security | Read-only by default outside the workspace, protected Git assets, patch preflight, explicit dangerous-operation confirmation | Clear and controllable permissions make it suitable for real projects, not just demos |

### Global Dashboard: see the whole system without opening a workspace

The global Dashboard summarizes multiple Workspaces in one control surface. Without opening a project first, it shows current AI executions, verification evidence, Global MCP state, Chat Session routing, Planning focus, and issues that need attention.

![Global Dashboard runtime overview](docs/images/dashboard-overview.png)

*Current execution, verification, and Session routing are visible together, making it easy to see what the AI is doing, whether anything is blocked, and where requests are routed.*

The Dashboard also refreshes estimated MCP token usage and exposes Token Flow, request success rate, Planning completion, Workspace Token Distribution, recent History Sessions, and system health. Token usage is estimated from local request sizes; request bodies are not stored.

![Dashboard metrics, connection modes, AI Planning, and token usage](docs/images/dashboard-insights.png)

*Execution counts, verification, Planning, Session routing, and token trends provide a compact picture of workload and pending attention.*

### 1. Workspace management: turn a project into durable, identifiable context

A Workspace is the basic project context of Coding Tools MCP. It binds a local project directory and owns project policy, Planning, and conversation history. The MCP listener, authentication, and public entry point are application-global, while each Chat Session keeps its own Workspace selection.

**Characteristics**

- The project directory is the source of truth; the workspace name is maintained separately from service configuration.
- MCP requests select a workspace inside the global session; planning state and conversation history remain organized around that workspace.
- The sidebar shows branch, clean / changed state, the focused Goal / Plan, progress, and execution status.
- Workspace Overview covers the main repository and first-level nested Git repositories, 7-day activity, execution quality, whole-workspace Diff / Review, and IDE shortcuts.

**Advantages**

- Fewer mix-ups between directories, active Plans, and execution contexts when working on multiple projects.
- Every connection gives the AI an explicit project boundary, reducing the risk of running commands in the wrong repository.
- Cache-first state loading with background refresh reduces repeated requests and makes Workspace switching feel more immediate.

### 2. MCP tool runtime: one core for real development actions

The Rust tool runtime provides file reading, search, patches, command execution, Git, images, Skills, and state management. Remote tool calls enter the same dispatch path through MCP Streamable HTTP.

**Characteristics**

- Stable Tool API v2 uses aggregate entry points such as `history_manage`, `planning_manage`, and `task_manage`, while compatibility profiles remain available.
- Files, commands, and patches pass through one set of path boundaries, command policies, confirmations, and error structures.
- Patch operations support preflight checks and failure recovery to reduce partial writes.

**Advantages**

- Different clients receive the same tool behavior, lowering maintenance and compatibility risk.
- The AI can complete the full loop from understanding code to editing files, running tests, and checking Git.
- Permission decisions and failures are easier to explain, audit, and recover from.

### 3. Connectivity and public entry points: move from local debugging to remote development

The application exposes one Global MCP endpoint at `/mcp`. A client establishes a session, calls `workspace_list` and `workspace_select`, and then uses the selected project context; FRP or Cloudflare only provides public transport for that single endpoint.

**Characteristics**

- FRP and Cloudflare Tunnel are supported, with tunnel processes supervised by the desktop app.
- Each Chat session keeps an isolated workspace selection, and every request captures its workspace context before tool dispatch.
- Stopping Global MCP also disconnects its associated tunnel, reducing orphaned public processes.

**Advantages**

- Local development, LAN debugging, and remote ChatGPT access share the same workspace model.
- Multiple projects do not each need a separately maintained public-infrastructure stack.
- Endpoints, tunnels, and service states are visible together, making network debugging more direct.

![Global Gateway shared public entry point](docs/images/global-gateway.png)

*Global MCP provides one `/mcp` entry point for multiple workspaces; session-scoped selection supplies the project boundary, while FRP and Cloudflare provide public transport.*

### 4. OAuth and authentication: modern security with client compatibility

The MCP OAuth runtime supports Authorization Code, PKCE S256, Dynamic Client Registration, and Refresh Tokens. Bearer auth and static Client ID / Secret remain available for older clients and simpler environments.

**Characteristics**

- Dynamic-registration clients can register themselves from the server metadata.
- PKCE S256 reduces the value of an intercepted authorization code.
- Access and Refresh Tokens are distinguished and bound to registered redirect URIs.

**Advantages**

- Modern clients can use a fuller OAuth flow with less manual credential copying.
- Older clients have a clear compatibility path instead of requiring a one-time migration.
- Auth settings and health checks live in the desktop app, making authorization failures easier to locate.

### 5. Planning, Goals, and Tasks: keep complex work controlled and resumable

The planning page separates goals, plans, and execution tasks. Direct suits small changes, Plan suits work that needs steps, and Goal suits long-running work with explicit acceptance criteria. The AI can create and maintain plans in conversation while the desktop app displays constraints, progress, and final acceptance records.

![AI Planning and acceptance](docs/images/planning-overview.png)

*The planning page shows the active execution mode, Goal / Plan state, pending acceptance count, and the Goal acceptance checklist together.*

**Characteristics**

- A Goal expresses the result and constraints, a Plan expresses the steps, and a Harness Task expresses a recoverable execution process.
- Write operations can be required to bind to the active Goal and follow the active Plan.
- The Execution Ledger projects the current step, last tool, errors, changed files, history checkpoint, and verification result in one place.

**Advantages**

- Large tasks do not depend only on chat context; work can resume from explicit state after a pause.
- Planning, execution, and acceptance stay separate, adding control to high-risk changes without removing flexibility.
- Developers can quickly see what is done, what remains, and whether the result has been verified.

### 6. Conversation history: let context follow the repository

Long-term project records live in `docs/history-session/`. Markdown archives preserve the full record, while `memory/state.json` and `memory/manifest.json` provide bounded current state and indexes. A new conversation can recover the needed context through startup prompts, search, and paginated reads.

**Characteristics**

- Supports initialization, checkpoints, validation, search, and lossless paginated reads.
- The current conversation can be recorded by default; older conversations are selected explicitly in the workspace panel.
- Only bounded snapshots and selected excerpts are injected instead of the entire history.

**Advantages**

- Project history can be backed up, reviewed, and committed without being locked to one chat platform.
- A new agent gets the current state quickly and reads older decisions only when needed, saving context and time.
- Important changes, tests, risks, and next steps become a traceable handoff record.

### 7. Logs, health checks, and security boundaries: make failures visible and permissions explicit

The desktop app exposes service logs, endpoint checks, OAuth metadata checks, and runtime status. The unified tool runtime applies workspace paths, command policies, repository protection, and dangerous-operation confirmations through one Policy layer.

Global settings also bring together the application version and Releases links, UI-memory release, the global Agent Runtime, startup restoration of running services, and LAN-access controls. Network-exposure options are off by default and explain their impact when enabled.

![General settings and Agent Runtime](docs/images/settings-general.png)

*General settings combine update links, WebView memory management, runtime-state restoration, LAN access, and the global executable PATH.*

**Characteristics**

- Reads, writes, and execution are allowed inside the workspace according to policy; outside the workspace is read-only by default.
- `.git`, `.github`, and other repository assets receive extra protection; dangerous operations require explicit confirmation.
- Health checks report local service, public entry point, and authentication metadata separately.
- Windows currently uses a `policy_only` execution boundary; the project does not present static policy as a full OS sandbox.

**Advantages**

- AI capabilities can approach a real development workflow while keeping the least-privilege scope explicit.
- Connection, authorization, and tool-call failures have clear diagnostic entry points.
- Security boundaries are stated honestly, making it easier to add system-level isolation where deployment requires it.

## Understand the workflow in 30 seconds

```text
Install the desktop app
  → add a project directory
  → start MCP and a public tunnel
  → copy the Public MCP URL
  → enable ChatGPT developer mode
  → create an MCP plugin and paste the URL
  → authorize it
  → use workspace_list / workspace_select to choose the project
  → start developing in a new conversation
```

For a first connection, remember only this: **the desktop app turns the project into an MCP workspace, and ChatGPT connects to it through the public `/mcp` URL.**

- [See the complete desktop setup](#get-started-in-five-minutes)
- [Go directly to the ChatGPT plugin setup](#mcp-connector)

## Get started in five minutes

### 1. Install the desktop client

Open [Releases](https://github.com/lengsukq/coding-tools-mcp/releases/latest) and download the package for your platform:

| Platform | Package |
| --- | --- |
| Windows 10/11 x64 | `Coding.Tools.MCP_*_x64-setup.exe` |
| macOS Apple Silicon | `Coding Tools MCP_*_aarch64.dmg` |

The macOS build is currently unsigned. If macOS blocks the first launch, allow it from System Settings → Privacy & Security.

### 2. Add a project workspace

1. Click **Add workspace** in the sidebar.
2. Select the project root directory.
3. Configure the workspace name and save it. The workspace remains available in the sidebar across conversations and restarts.

### 3. Configure a public tunnel

When the AI client is not running on the same machine, expose MCP through HTTPS:

- Open **Settings → Global MCP Connection** and configure the single local endpoint and public transport.
- **Zero-setup default: Cloudflare Quick Tunnel.** Select Cloudflare Quick Tunnel in the Global MCP connection settings to generate a `https://<random>.trycloudflare.com` public URL without an account — ideal for first-time setup and temporary demos. For long-term use, prefer FRP with a fixed subdomain or an external reverse proxy.
- Install or detect `frpc` / `cloudflared` from **Software management**.
- Save the server, port, and token under **FRP settings**, then select that profile and one subdomain in **Global MCP Connection**.

![FRP configuration](docs/images/frp-configuration.png)

*FRP server profiles are stored centrally; the single Global MCP connection selects one profile and supplies one public subdomain.*

If you do not have an FRPS server yet, follow this [FRPS server installation guide (Chinese, WeChat)](https://mp.weixin.qq.com/s/kmpQhHsvmHlaLfj4rw3A0Q). After deployment, enter the server address, port, and token under **FRP settings** in the desktop client.

### 4. Start MCP

Open Dashboard or **Settings → Global MCP Connection** and click **Start Global MCP**. The desktop client shows:

- the single local MCP URL such as `http://127.0.0.1:28765/mcp`;
- the public HTTPS MCP URL;
- authentication details for ChatGPT;
- live logs and health-check results.

![Local, public, and ChatGPT MCP connection details](docs/images/workspace-connection.png)

The desktop app can verify the local and public endpoints, OAuth metadata, and the MCP protected-resource document:

![MCP health-check results](docs/images/health-check.png)

*Each connectivity and authentication check reports its result separately.*

When a connection fails, inspect recent MCP requests without leaving the desktop app:

![MCP runtime logs](docs/images/runtime-logs.png)

*The log quickly confirms whether tool discovery, history bootstrap, and checkpoint calls reached the server.*

### 5. Connect an AI client

Beyond ChatGPT, any web client with a remote MCP connector can reach the same workspace — claude.ai, Grok, Gemini, and generic connectors included. See the [web chat connector matrix](docs/web-chat-connectors.md) for per-client configuration, authentication choices, and troubleshooting.

Use the public MCP URL shown by the app. OAuth now supports Authorization Code + PKCE S256, Dynamic Client Registration (`/register`), and Refresh Tokens. Clients with DCR support can register themselves from the server metadata; older clients can continue to use the static Client ID / Secret configured in the desktop app.

For a first connection, explicitly choose a Workspace first; history bootstrap is optional:

```text
workspace_list
workspace_select(workspace_id=...)
server_info
git_status
```

This keeps the agent from guessing the project from chat context. If a client cannot reliably preserve MCP Session state, use `workspace_invoke(workspace_id=..., tool=..., arguments=...)` for explicit one-off operations rather than repeatedly selecting or inferring a directory.

## Connect ChatGPT

### MCP Connector

Before configuring ChatGPT, make sure that:

1. Global MCP and the public tunnel are both running.
2. The public MCP endpoint passes the desktop health check. If OAuth is enabled, also verify the protected-resource document and authorization metadata.
3. You have copied the **Public MCP URL** from the desktop **GPT configuration** card. OAuth needs the authorization password; static Client ID / Secret values are only needed when the client does not support dynamic registration.

> ChatGPT must use the public HTTPS `/mcp` URL. A local address such as `http://127.0.0.1:28765/mcp` is not reachable from ChatGPT. After connecting, call `workspace_list` and `workspace_select` to choose the project context. Menu names may vary slightly by ChatGPT version and language.

#### 1. Enable ChatGPT developer mode

Open ChatGPT settings, go to **Account security and sign-in**, and enable **Developer mode**. This allows unverified MCP connectors to be added.

![Enable developer mode in ChatGPT](docs/images/gpt-config-1.png)

*Developer mode grants powerful access. Only connect MCP servers that you operate or explicitly trust.*

#### 2. Create the MCP plugin

Open **Plugins** from the ChatGPT sidebar, click the `+` button, select the MCP beta option, and enter:

| ChatGPT field | Value |
| --- | --- |
| Name | A recognizable name such as `Coding Tools MCP` |
| Description | A short description of the connected project or purpose |
| Connection | The public MCP URL from the desktop **GPT configuration** card; it should end in `/mcp` |
| Authentication | The same mode configured in the desktop app; the screenshot uses OAuth |

![Create an MCP plugin and enter its connection details](docs/images/gpt-config-2-detail.png)

For OAuth, prefer Dynamic Client Registration when the client supports it and let the client register from the server metadata. If the client only supports static OAuth credentials, use the Client ID and Client Secret shown by the desktop app. When the authorization page opens, enter the authorization password from the desktop **GPT configuration** card.

> Client Secrets, authorization passwords, and Bearer tokens are sensitive. Never paste them into chats, issues, or public screenshots. If the desktop app uses Bearer or no authentication, select the matching option currently offered by ChatGPT.

#### 3. Verify the connection

Start a new conversation with the plugin enabled and ask:

```text
Use Coding Tools MCP to call workspace_list, select the coding-tools-mcp Workspace,
then call server_info and git_status. Tell me the active Workspace, MCP state, and Git status.
```

If ChatGPT returns information from the current project, the desktop app, public tunnel, authentication, ChatGPT, and MCP tool chain are connected end to end. History can be initialized explicitly when a client or workflow needs a dedicated archived session, but it is not a prerequisite for normal development.

If ChatGPT still shows an old tool list, disconnect and reconnect the plugin or verify again in a new conversation.

#### Troubleshooting

| Symptom | Check first |
| --- | --- |
| ChatGPT cannot connect | Confirm that the URL is the public HTTPS `/mcp` endpoint rather than `127.0.0.1`, and that the public MCP health check passes |
| OAuth authorization fails | Confirm that the Client ID, Client Secret, and authorization password come from the same workspace, and check the OAuth metadata results |
| New tools are missing | Disconnect and reconnect the plugin, then start a new conversation |
| A tool call fails | Open **Logs** and **Health checks** in the desktop app and confirm that the request reached the MCP service |

## Why use it

- **Built for real development**: files, commands, Git, tests, and retained processes live in one Workspace.
- **Cross-conversation continuity**: a new conversation starts from bounded current state and can search/read exact older archives only when they are needed.
- **Auditable progress**: structured checkpoints preserve decisions, changed files, test results, remaining issues, and next steps inside the project.
- **Multiple workspaces**: one Global MCP connection manages multiple local projects, while each Chat Session selects its own Workspace. There is no need to configure a separate MCP and public endpoint per project.
- **Direct ChatGPT connectivity**: Streamable HTTP, OAuth, Bearer tokens, FRP, and Cloudflare are built in.
- **A focused default tool surface**: stable core tools are available by default; advanced Harness capabilities are opt-in.

## Let the project remember every conversation

Chat transcripts are useful for rereading a discussion, but they are a poor long-term development handoff. Coding Tools MCP stores progress in `docs/history-session/` under the current project, so context follows the repository instead of staying trapped in one chat window.

![ChatGPT new-conversation startup prompt](docs/images/history-session-prompt.png)

*The current conversation can be recorded by default; older sessions are selected from the Workspace History Context panel and injected only as bounded context.*

The History Context panel controls current-session recording, multi-selects older sessions, previews the selected context, and clears selections. Applying a selection refreshes MCP context with only a bounded session index, recent checkpoints, key files, and selected snippets; the full Markdown archives remain in `docs/history-session/` and are searched/read on demand when exact older context is required.

The default `compact` profile exposes history through the Stable Tool API v2 `history_manage` aggregate entry point. The five lifecycle-specific names below remain available in compatibility profiles:

| Tool | Purpose |
| --- | --- |
| `history_session_bootstrap` | Initialize or restore a project session; preserve verbatim `initial_user_input` and return a stable `session_key`, `current_path`, and bounded current state instead of all history |
| `history_session_checkpoint` | Append structured progress and verbatim `raw_user_input` to the stable target returned by bootstrap; reject mismatched targets instead of writing to another history file |
| `history_session_validate` | Validate numbering, history files, and session mappings; rebuild derived indexes when needed without deleting existing history |
| `history_session_search` | Search lossless Markdown archives by deterministic keywords and return a bounded page of locations and snippets |
| `history_session_read` | Read one original Markdown archive losslessly in UTF-8-safe pages by number or a search result path; pages default to `16 KiB`, are capped at `64 KiB`, and continue with `next_cursor` |

History uses readable Markdown that can be backed up or committed with the project. `memory/state.json` is a bounded current-state projection, while `memory/manifest.json` stores only archive locations, hashes, and keywords; Markdown remains the lossless source of truth. ChatGPT must pass verbatim first-turn and per-turn text as `initial_user_input` and `raw_user_input`, because the server cannot inspect remote chat text that was not provided as a tool argument. Checkpoints are idempotent, changed content for the same `turn_id` is retained as a revision with supersession evidence, and progress should only be reported as saved after the tool returns `ok=true` with the same session target.

> History persistence is performed when the AI calls the MCP tools; the desktop app does not record chat content in the background. If the client does not invoke a tool, the server cannot infer that a new conversation or task has happened.

## What an agent can do

The default `compact` profile provides a stable, composable development tool set; `core` and `advanced` remain available as legacy-compatible profiles:

| Category | Main tools |
| --- | --- |
| File reading | `read_file`, `list_dir`, `list_files`, `search_text`, `grep_text`, `view_image` |
| File modification | `apply_patch` |
| Command execution | `exec_command`, `write_stdin`, `read_output`, `kill_session` |
| Git | `git_status`, `git_diff`, `git_log`, `git_show`, `git_blame` |
| Workspace routing | `workspace_list`, `workspace_current`, `workspace_select`, `workspace_invoke` |
| Runtime | `server_info` |
| State management | `history_manage`, `planning_manage`; Stable Tool API v2 continues to provide aggregate lifecycle interfaces |

Aggregate tools use an `action` field, for example `history_manage(action=search)` or `planning_manage(action=update_plan)`. This keeps the top-level MCP schema stable as lifecycle behavior grows while compatibility profiles can retain legacy tool names.

Long-running commands are owned by the Workspace Runtime rather than one MCP transport connection. `exec_command` returns a stable `command_id`, so reconnecting or using another runtime entry point for the same Workspace can continue with `read_output`, `write_stdin`, or `kill_session`; legacy `session_id` arguments remain supported. stdout and stderr keep a bounded in-memory head and tail for low-latency previews while the complete raw streams are appended to the application cache. A stable `output_ref` therefore remains pageable after a normal synchronous command has completed and its SessionStore entry has been reclaimed. `read_output` also supports bounded `query` / `regex` / `case_sensitive` searches without injecting the entire raw log into model context.

`exec_command(action=quality_gate)` reuses the same Tool Surface to discover existing lint/check/typecheck/test/build/format checks. It supports `dry_run`, `checks` filtering, and `stop_on_failure`; every derived command is revalidated by the normal command policy and returns compact `PASS/WARN/FAIL` evidence, duration, key error/warning counts, and its own `output_ref`. The gate does not install dependencies, deploy, or publish.

`server_info.runtime` exposes a `runtime_id`, start time, application version, build identity, and the active Tool Profile's `tool_schema_version` / `tool_schema_hash`. A client can send `known_schema_hash` and receive `schema_changed` / `reconnect_recommended` when its cached Tool Schema is stale.

A typical development loop is:

```text
Open Workspace
  → understand project and Git state
  → search and read code
  → apply a transactional patch
  → run commands and tests
  → inspect the diff and commit
```

The advanced profile retains project-state and operation-history Harness capabilities, but normal edits and command execution do not require a Task.

## Permission and recovery model

The project uses a Workspace-first permission model:

- Normal files inside the Workspace can be read, created, modified, deleted, and executed.
- Outside the Workspace, `read_file`, `list_dir`, `list_files`, `search_text`, and `view_image` provide read-only access.
- Writes, deletes, and command execution outside the Workspace are blocked.
- `.git` and `.github` cannot be damaged through ordinary file tools, Patch, or interpreter commands.
- Patch performs preflight validation and operation-local recovery; long-term recovery uses Git instead of full Workspace snapshots.

> Windows child-process execution currently uses a `policy_only` boundary. The honest runtime value is `sandbox_enforced: false`; static command policy is not a complete OS filesystem sandbox.

## Local development

Requirements: Node.js 20+, Rust stable, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run desktop
```

Useful verification commands:

```bash
npm run test:workspace
npm run test:release
npm run check
npm run build
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

`npm run test:workspace` runs the real Workspace lifecycle regression suite, including legacy/corrupt Planning, Plan/Goal gates, managed History/Harness state, and real external-change detection. `npm run test:release` is the local pre-release dogfood entry point and runs version checks, frontend checks/build, and every Rust test target.

On Windows, you can also run `dev-desktop.cmd`. Do not use `npm run dev` alone to validate the desktop application; it starts Vite without the Tauri shell.

## Project layout

| Path | Purpose |
| --- | --- |
| `src-tauri/src/tools/` | Shared file, Patch, Exec, and Git tool kernel |
| `src-tauri/src/mcp/` | MCP Streamable HTTP server |
| `src-tauri/src/tunnel/` | FRP / Cloudflare tunnel and process management |
| `src-tauri/tests/fixtures/` | Isolated fixtures used by current Rust integration tests |
| `src/` | Vue 3 + Vue Router + Tailwind CSS desktop UI |

## License

[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)
