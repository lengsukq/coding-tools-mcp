# 如何测试

> 本文档描述当前仓库真实存在的测试与质量门。

## 测试层级

| 层级 | 命令 / 框架 | 位置 |
| --- | --- | --- |
| Rust 单元测试 | `cargo test` | `src-tauri/src/**` 中 `#[cfg(test)]` |
| Rust 集成测试 | `cargo test` | `src-tauri/tests/` |
| MCP Tool Contract | `cargo test --test call_tool_contract` | `src-tauri/tests/call_tool_contract.rs` |
| Security Contract | `cargo test --test call_tool_security` | `src-tauri/tests/call_tool_security.rs` |
| Harness | `cargo test --test harness_state --test harness_tool_contract` | `src-tauri/tests/` |
| History Session | `cargo test --test history_session` | `src-tauri/tests/history_session.rs` |
| Frontend 类型/组件检查 | `npm run check` | Vue 3 / `vue-tsc` / TypeScript |
| Frontend 状态/并发/路由回归 | `npm run test:ui`，Node test runner | `tests/*.test.mjs` |
| Frontend 生产构建 | `npm run build` | Vite 6 |

当前仓库没有配置 vitest。历史 Python compliance suite 已退出当前代码路径；Rust contract/security 测试所需的最小 fixture 已迁入 `src-tauri/tests/fixtures/compliance/`。

## 日常验证

小范围修改先跑定向测试：

```bash
cd src-tauri
cargo test auth::
cargo test planning::
cargo test tools::
```

提交前跑完整质量门：

```bash
npm run check
npm run build

cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --locked
```

## 核心回归要求

### Tool API / Dispatch

- `compact` profile 应优先暴露 Stable Tool API v2 聚合入口；
- legacy profile 继续保留旧 Tool；
- MCP 必须进入统一 `call_tool`；
- Plan / Goal 权限门必须根据聚合 Tool 的 `action` 区分读写。

### OAuth

- Authorization Code + PKCE S256；
- Dynamic Client Registration；
- redirect URI 精确约束；
- Refresh Token 可以换发新 token pair；
- Refresh JWT 不能作为 MCP Access Token 使用；
- 静态 Client 兼容测试继续通过。

### Execution Ledger

- focused Goal / Plan / Step 能正确投影；
- Harness Task ID 能关联；
- History checkpoint path 能关联；
- 写操作失败能记录 last error；
- 旧 `Goal.execution_checkpoint` 与 Ledger 保持兼容同步。

### 安全

以下行为必须由集成测试持续覆盖：

- Workspace 路径穿越；
- Shell chaining / command policy；
- Repository protected assets；
- Dangerous operation confirmation；
- Goal / Plan 模式下的写入门禁。

## CI

`.github/workflows/ci.yml` 是 PR / main push 的强制质量门，安装 `rustfmt`、`clippy` 后执行 Rust fmt、Clippy `-D warnings`、locked build/test，以及前端 check/build。

旧 Python 合规套件仍可以作为迁移参考，但不应替代当前 Rust tests。

---
*返回索引: [../project-context.md](../project-context.md)*

## UI 优化回归

`npm run test:release` 包含版本一致性、前端类型检查、生产构建、14 项前端回归和 Rust 完整测试。前端测试覆盖状态映射、进度、未知健康、旧地址、缓存失效竞争、设置队列和草稿生命周期。

`npm run dev` 后访问 `/tests/fixtures/ui.html#/` 可运行隔离测试入口，IPC 只读写内存模拟数据；此入口不进入生产构建。控制区可模拟无工作区、保存失败、验收失败及成功后刷新失败。不得用模拟数据验收替代真实 Tauri 的 IPC、窗口与平台交互。

最新截图、实际执行日志、验证范围及未通过项见 [全应用优化验收记录](../ui/verification/acceptance.md)。
