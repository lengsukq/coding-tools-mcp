# 如何开发

> 本文档描述当前 Coding Tools MCP 的开发流程。开发前置条件来自实际 Planning Mode 与仓库 Policy，不再依赖旧 MCP Probe Kit 工具。

## 开发入口

桌面端运行：

```bash
npm ci
npm run desktop
```

只开发前端时可以使用 `npm run dev`，但它不能验证 Tauri IPC、系统托盘、Runtime、Keyring 等桌面能力，因此功能验收仍应回到 `npm run desktop`。

## Planning Mode

Planning Mode 由桌面端控制：

- **Direct**：普通开发工具可以直接执行；
- **Plan**：项目写入与命令执行被服务端阻止，只允许分析和规划；
- **Goal**：写操作必须存在 Active focused Goal；若同时 focused Plan，则 Plan 必须属于该 Goal 且可执行。

Agent 不应通过聊天绕过模式。适合长期跟踪的工作可以创建 Goal / Plan，并在完成后请求人工验收。

## 推荐开发流程

```text
理解 Workspace / git status
→ 阅读相关代码和当前 project-context
→ 明确 Goal / Plan（需要时）
→ 小批次修改
→ 运行定向测试
→ 检查 diff
→ 运行质量门
→ 分批 commit
→ 请求 Goal / Plan review
```

不要求为了修改普通代码先创建 Harness Task。Harness Task 适合需要额外基线、事件流或恢复能力的长任务。

## Tool API 开发规则

默认 `compact` profile 使用 Stable Tool API v2。新增 Goal、Plan、Task、History 生命周期行为时，优先在以下聚合入口增加 `action`：

```text
history_manage
planning_manage
task_manage
```

不要无必要增加新的顶层 Tool。旧生命周期 Tool 只作为兼容层继续维护。

所有 MCP 与 Actions 工具执行必须收口到：

```rust
tools::dispatch::call_tool
```

Transport 层不得复制 Policy、Planning Gate 或工具业务逻辑。

## OAuth 开发规则

MCP 与 Actions 共用 `auth/oauth_flow.rs`。修改 OAuth 时至少保持：

- PKCE 仅接受 `S256`；
- 动态 Client 只能使用已注册 redirect URI；
- Access Token 与 Refresh Token 类型不可混用；
- 静态 Client 兼容流程不得被破坏；
- metadata 与真实 endpoint/grant 保持一致。

## Planning / Execution 状态规则

职责边界：

- Planning 保存 Goal / Plan 与人工验收；
- Harness 保存 Task / operation / event；
- History 保存长期对话事实；
- Execution Ledger 只保存当前执行投影并关联三者。

不要再创建第四套重复的“当前任务状态”。

## 版本与发布

产生安装包的功能/修复发布必须先同步升级：

1. `package.json`；
2. `package-lock.json` 根版本；
3. `src-tauri/Cargo.toml`；
4. `src-tauri/Cargo.lock` 中本项目版本；
5. `src-tauri/tauri.conf.json`。

只做代码开发、测试或文档修改且本轮不构建交付安装包时，不强制提前升版本。

## 提交前质量门

```bash
npm run check
npm run build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --locked
```

本机缺少 rustfmt/clippy component 时不要静默跳过；记录环境限制，并保证 CI 的 stable toolchain 安装对应 component。

## 参考旧版

`old/` 只用于兼容行为对照。当前代码、当前测试和 `docs/project-context/` 的优先级高于旧 Python 实现。

---
*返回索引: [../project-context.md](../project-context.md)*
