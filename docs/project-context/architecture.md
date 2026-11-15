# 架构设计

> 本文档描述 Coding Tools MCP 当前实际架构。`docs/specs/` 可以保留历史设计过程，本目录只记录当前代码事实。

## 总体定位

Coding Tools MCP 是一个 **Workspace-first 的 AI 开发运行时 + Tauri 桌面控制面**。桌面端负责工作区、认证、公网入口、运行时和可视化配置；Rust Core 在本机直接提供 MCP，并通过统一工具内核访问代码、Git、命令、Planning、History 与 Harness。

```text
┌──────────────────────────────────────────────┐
│ Vue 3 Desktop UI                            │
│ Workspace / Dashboard / Planning / Settings │
└──────────────────────┬───────────────────────┘
                       │ Tauri IPC
┌──────────────────────▼───────────────────────┐
│ Rust Application Core                       │
│ Workspace / Runtime / Auth / Planning       │
│ History / Harness / Usage / Update          │
├──────────────────────────────────────────────┤
│ Unified Tool Runtime                        │
│ file / patch / exec / git / skill / manage │
├──────────────────────────────────────────────┤
│ MCP Streamable HTTP & Loopback Stdio Bridge │
└──────────────────────┬───────────────────────┘
                       │
      Local / Global Gateway / FRP / Cloudflare
```

## 当前目录边界

| 路径 | 当前职责 |
| --- | --- |
| `src-tauri/src/tools/` | 统一 Tool 内核、Schema、Policy、文件/Git/Exec/History/Planning/Skill |
| `src-tauri/src/tools/dispatch.rs` | 唯一工具调用编排入口（`call_tool`），串接门禁、策略、Harness 追踪与快照 |
| `src-tauri/src/tools/dispatch/` | `gates.rs`（Planning 门禁）、`routing.rs`（工具路由）、`cwd.rs`（默认目录重写与自检）、`ledger.rs`（Execution Ledger 与上下文装饰） |
| `src-tauri/src/mcp/` | MCP Streamable HTTP transport、会话状态与无头桥接 |
| `src-tauri/src/mcp/stdio_bridge.rs` | `mcp-stdio` 子命令入口门面，供桌面 MCP 宿主通过 stdio 连接回环 `/mcp` |
| `src-tauri/src/mcp/stdio_bridge/` | `auth.rs`（回环校验与私有凭据文件）、`compact.rs`（Schema 精简与错误适配）、`transport.rs`（stdio 帧读写与 HTTP/SSE 转发） |
| `src-tauri/src/mcp/server.rs` | Global MCP 入口重导出、协议协商及 legacy 单工作区兼容处理 |
| `src-tauri/src/mcp/server/` | `scope.rs`（`workspace_id` 解析校验）、`dispatch.rs`（Global MCP 请求分发与 Review 关联） |
| `src-tauri/src/mcp/listener.rs` | HTTP 监听、MCP RPC 主链、会话生命周期及共享 OAuth URL 解析 |
| `src-tauri/src/mcp/listener/` | `oauth_http.rs`（OAuth/DCR 端点）、`review_http.rs` & `review_page.html`（只读 Review 页面与 API） |
| `src-tauri/src/computer.rs` | macOS Computer Use 授权控制面、租约状态机与 MCP 调用编排 |
| `src-tauri/src/computer/` | `geometry.rs`（Retina 截图坐标映射）、`action_policy.rs`（参数校验与敏感操作识别）、`schema.rs`（MCP 工具定义）、`native.rs`（macOS 原生 FFI） |
| `src-tauri/src/review.rs` | 工作区/会话/单次操作级变更快照编排门面 |
| `src-tauri/src/review/` | `diff.rs`（目录剪枝、脱敏与 Unified Diff）、`git_scan.rs`（一级 Git 仓库与变更扫描）、`store.rs`（JSON 存储与多令牌签发） |
| `src-tauri/src/tool_audit.rs` | 已认证 MCP 工具调用的隐私安全审计门面 |
| `src-tauri/src/tool_audit/` | `sanitize.rs`（结果分类与标识符脱敏）、`store.rs`（按天与工作区分区的 JSONL 存储与过期轮转） |
| `src-tauri/src/platform/` | 跨平台系统原语、后台命令隐藏窗口（`command.rs`）与单实例进程协调（`single_instance.rs`） |
| `src-tauri/src/auth/` | Bearer、OAuth Authorization Code、PKCE S256、DCR、Refresh Token |
| `src-tauri/src/planning/` | Direct / Plan / Goal、Goal/Plan 生命周期、Execution Ledger |
| `src-tauri/src/harness/` | Durable Task、operation/event log、基线与恢复信息 |
| `src-tauri/src/runtime/` | MCP 生命周期与进程监督 |
| `src-tauri/src/tunnel/` | FRP / Cloudflare 下载、配置与进程监督 |
| `src-tauri/src/global_gateway.rs` | 唯一 Global MCP Endpoint 的公网 Tunnel |
| `src-tauri/src/workspace/` | Workspace 配置、持久化与兼容迁移 |
| `src/` | Vue 3 桌面 UI、Vue Router、Tauri API 封装与状态展示 |
| `docs/history-session/` | 项目内、可审计、无损 Markdown 会话档案 |
| `.coding-tools/planning/state.json` | 项目本地 Goal / Plan / Execution Ledger 状态 |

## 前端状态与界面边界

工作台、侧栏及工作区使用 `ui-model.ts` 的展示口径：计划优先，目标作为关联说明，步骤完成和跳过分开，验证记录不代表验证通过；应用版本与仓库链接常量统一收口在 `app-meta.ts`。六个工作区页签和六个设置分类纳入 Hash 路由；旧设置入口映射到对应分类和区块。

`async-resource.ts` 管理单个请求资源的去重、过期和代际失效；`workspace-status-cache.ts` 按工作区与资源类型持有共享缓存，并通过按需加载刷新。晚返回请求只更新自己仍有效的资源代际。Rust DTO、持久化与 IPC 保持原有归属，前端缓存是可丢弃的展示投影。

`queued-writes.ts` 持有模块级设置写入队列；各保存单元重新读取当前配置，只合并自己修改的字段，避免页面卸载或不同分类保存互相覆盖。`unsaved-changes.ts` 注册当前页面草稿，路由离开和关闭应用提供保存、放弃与取消；失败保留输入。

`src/styles/design-system.css` 是语义 token 与通用视觉规则来源，页面负责布局。组件展示页仅在开发构建注册。验收依据与原生桌面待验收项见 [记录](../ui/verification/acceptance.md)。

复杂视图统一遵循“组合式函数持有状态 + 纯展示子组件通过类型化 props/emits 协作”边界：

- **工具调用审计页（`ToolAuditView.vue`）**：由 `src/lib/audit/use-tool-audit.ts` 管理过滤、分页、保留策略与清理，视图组合 `AuditFilterBar.vue`、`AuditRecordRow.vue` 与 `AuditOverviewAndRetention.vue`；子组件不直接访问 API。
- **Computer Use 设置页（`ComputerSettingsView.vue`）**：由 `src/lib/computer/use-computer-settings.ts` 管理本机授权、轮询、TCC 排障建议与审批，组合 `ComputerDiagnosticsBanner.vue` 与 `ComputerAuditPanel.vue`。
- **MCP 网关设置页（`GatewaySettingsView.vue`）**：页面管理网关配置草稿与服务启停，组合 `GeminiAppConnector.vue`、`GatewayTunnelConfigCard.vue` 与 `GatewayHealthCard.vue`。

工作区 Tauri Command 名称和参数不变；`src-tauri/src/commands/workspace.rs` 仅聚合导出，具体实现分别位于 `workspace/crud.rs`、`workspace/git.rs`、`workspace/ide.rs` 和 `workspace/reviews.rs`。

`review::snapshot_text_files` 在目录遍历阶段剪枝 `node_modules`、`target`、`build` 等不进入 Review 的目录及敏感目录。仍保留任意合法命令的文件前后快照，以便捕获未跟踪、新建、修改及删除的源码文件；不能仅根据命令名称猜测其无写入。

## Tool Runtime

所有 MCP 工具调用最终必须进入：

```text
tools::dispatch::call_tool
```

这里统一处理：

- Workspace 路径边界与命令策略；
- Goal / Plan 模式写入门禁；
- Harness operation / task 记录；
- Tool 分发与统一错误结构；
- Execution Ledger 更新（`dispatch/ledger.rs`）；
- Planning context 与恢复提示。

### Stable Tool API v2

默认 `compact` profile 使用稳定聚合入口：

```text
history_manage
planning_manage
task_manage
```

生命周期动作通过 `action` 参数路由，避免每新增一个状态操作就改变顶层 MCP Tool Schema。旧的 `history_session_*`、`create_goal`、`update_plan`、`start_task` 等工具继续保留在兼容 profile，不直接删除。

`server_info.tool_api` 与 `capability_health_check.capability.tool_api` 会返回当前 Tool API 版本信息。

## Planning、Task 与 History

三者职责明确分离：

- **Planning**：用户目标、约束、计划与人工验收；
- **Harness Task**：可恢复执行任务、操作事件和工作区基线；
- **History**：对话与开发事实的长期无损归档。

统一的 **Execution Ledger** 位于 Planning State 顶层，投影当前：

```text
Goal / Plan / Step
Task
last tool / last error
changed files
history checkpoint ref
verification
```

旧 `Goal.execution_checkpoint` 继续同步，作为兼容字段存在。

## OAuth

MCP 使用统一 OAuth runtime：

- Authorization Code；
- PKCE `S256`；
- Dynamic Client Registration `/register`；
- `authorization_code` 与 `refresh_token` grant；
- Access / Refresh JWT 类型区分；
- 动态 Client 绑定已注册 redirect URI。

静态 Client ID / Secret 仍保留，用于旧客户端兼容。

## Runtime 与网络

应用只运行一个 Global MCP listener，固定提供 `/mcp`（同时支持桌面端通过 `coding-tools-mcp mcp-stdio` 桥接到回环 `/mcp`）。`workspace_list` 返回注册 Workspace；普通工具 schema 统一接受可选 `workspace_id`，推荐客户端在每次 project-scoped 调用中显式携带该 id，因此即使宿主不保持同一个 transport session，也能稳定路由到同一 Workspace。支持持久 MCP session 的客户端仍可通过 `workspace_select` 绑定 active Workspace 并省略后续 `workspace_id`。只有一个 Workspace 时允许安全自动选择。无论采用哪种方式，每个请求都会在执行开始前冻结不可变 `WorkspaceRequestContext`，文件、命令、Planning、History、输出缓存和策略继续受 Workspace root 隔离；不同请求和 session 不会通过桌面 UI 当前选中项隐式串线。FRP 与 Cloudflare Tunnel 只负责暴露这个唯一 Endpoint，由 Rust supervisor 管理，不要求外部 Python Runtime。

## 安全边界

- Workspace 内按 Policy 允许读写和执行；
- Workspace 外默认只读；
- `.git` / `.github` 等仓库资产受额外保护；
- Patch 预检后事务化应用；
- Dangerous operation 需要显式 `confirm=true`；
- MCP session 由服务端签发，未知或伪造的 session ID fail-closed；
- `workspace_id` 只能引用 Registry 中的 Workspace id，不能由客户端提交 root 路径；进入 Tool Runtime 前会从参数中剥离，避免下层工具改变路由；
- Windows 当前仍是 `policy_only` 边界，不宣称拥有完整 OS Sandbox。

---
*返回索引: [../project-context.md](../project-context.md)*
