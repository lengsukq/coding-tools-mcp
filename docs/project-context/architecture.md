# 架构设计

> 本文档描述 Coding Tools MCP 当前实际架构。`docs/specs/` 可以保留历史设计过程，本目录只记录当前代码事实。

## 总体定位

Coding Tools MCP 是一个 **Workspace-first 的 AI 开发运行时 + Tauri 桌面控制面**。桌面端负责工作区、认证、公网入口、运行时和可视化配置；Rust Core 在本机直接提供 MCP / GPT Actions，并通过统一工具内核访问代码、Git、命令、Planning、History 与 Harness。

```text
┌──────────────────────────────────────────────┐
│ SvelteKit Desktop UI                        │
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
├─────────────────────┬────────────────────────┤
│ MCP Streamable HTTP │ GPT Actions OpenAPI   │
└──────────┬──────────┴──────────┬─────────────┘
           │                     │
      Local / Global Gateway / FRP / Cloudflare
```

## 当前目录边界

| 路径 | 当前职责 |
| --- | --- |
| `src-tauri/src/tools/` | 统一 Tool 内核、Schema、Policy、Dispatch、文件/Git/Exec/History/Planning/Skill |
| `src-tauri/src/mcp/` | MCP Streamable HTTP transport 与客户端初始化 |
| `src-tauri/src/actions/` | GPT Actions HTTP/OpenAPI transport |
| `src-tauri/src/auth/` | Bearer、OAuth Authorization Code、PKCE S256、DCR、Refresh Token |
| `src-tauri/src/planning/` | Direct / Plan / Goal、Goal/Plan 生命周期、Execution Ledger |
| `src-tauri/src/harness/` | Durable Task、operation/event log、基线与恢复信息 |
| `src-tauri/src/runtime/` | MCP / Actions 生命周期与进程监督 |
| `src-tauri/src/tunnel/` | FRP / Cloudflare 下载、配置与进程监督 |
| `src-tauri/src/global_gateway.rs` | 多 Workspace 共享公网入口 `/w/<workspace-id>` |
| `src-tauri/src/workspace/` | Workspace 配置、持久化与兼容迁移 |
| `src/` | SvelteKit 5 桌面 UI、Tauri API 封装与状态展示 |
| `docs/history-session/` | 项目内、可审计、无损 Markdown 会话档案 |
| `.coding-tools/planning/state.json` | 项目本地 Goal / Plan / Execution Ledger 状态 |

## Tool Runtime

MCP 和 Actions 不各自实现工具逻辑。两条 transport 最终必须进入：

```text
tools::dispatch::call_tool
```

这里统一处理：

- Workspace 路径边界与命令策略；
- Goal / Plan 模式写入门禁；
- Harness operation / task 记录；
- Tool 分发与统一错误结构；
- Execution Ledger 更新；
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

MCP 与 Actions 共用同一 OAuth runtime：

- Authorization Code；
- PKCE `S256`；
- Dynamic Client Registration `/register`；
- `authorization_code` 与 `refresh_token` grant；
- Access / Refresh JWT 类型区分；
- 动态 Client 绑定已注册 redirect URI。

静态 Client ID / Secret 仍保留，用于旧客户端兼容。

## Runtime 与网络

每个 Workspace 可以独立运行 MCP / Actions。Global Gateway 提供统一公网入口并按 `/w/<workspace-id>` 路由到对应工作区。FRP 与 Cloudflare Tunnel 由 Rust supervisor 管理，不要求外部 Python Runtime。

## 安全边界

- Workspace 内按 Policy 允许读写和执行；
- Workspace 外默认只读；
- `.git` / `.github` 等仓库资产受额外保护；
- Patch 预检后事务化应用；
- Dangerous operation 需要显式 `confirm=true`；
- Windows 当前仍是 `policy_only` 边界，不宣称拥有完整 OS Sandbox。

---
*返回索引: [../project-context.md](../project-context.md)*
