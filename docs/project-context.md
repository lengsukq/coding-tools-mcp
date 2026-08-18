# Coding Tools MCP - 项目上下文

> 本目录记录当前代码事实，供开发者和 Agent 快速恢复架构上下文。历史方案与已经完成的迁移过程保留在 `docs/specs/`，不应反向覆盖这里的当前状态。

## 项目概览

| 属性 | 当前值 |
| --- | --- |
| 项目名称 | Coding Tools MCP |
| 版本 | `0.2.0` |
| 桌面端 | Tauri 2 |
| 前端 | SvelteKit 2 + Svelte 5 + TypeScript |
| 后端 | Rust 2021 + Tokio + Axum |
| 类型 | AI Coding Workspace Desktop + 内嵌 MCP / GPT Actions Runtime |
| 核心定位 | 把本地项目变成可被 AI 安全开发、跨会话恢复、可视化管理的持久 Workspace |

## 当前核心能力

- 多 Workspace 配置与运行时管理；
- MCP Streamable HTTP 与 GPT Actions OpenAPI；
- Bearer / OAuth、PKCE S256、Dynamic Client Registration、Refresh Token；
- 文件、Patch、命令、Git、图片与 Skill 工具；
- Stable Tool API v2 聚合管理入口；
- Direct / Plan / Goal 模式与人工验收；
- Planning / Harness Task / History 通过 Execution Ledger 收口执行状态；
- 项目内 `docs/history-session/` 无损历史；
- FRP / Cloudflare 与 Global Gateway；
- 系统 Keyring 密钥存储、运行日志、健康检查与 usage 统计。

## 文档导航

- [技术栈](./project-context/tech-stack.md)
- [架构设计](./project-context/architecture.md)
- [如何开发](./project-context/how-to-develop.md)
- [如何测试](./project-context/how-to-test.md)
- [代码图谱洞察](./graph-insights/latest.md)
- [设计系统](./design-system.md)

## 开发时的事实来源

优先级从高到低：

1. 当前代码与测试；
2. `docs/project-context/` 当前事实文档；
3. 当前功能对应的 `docs/specs/`；
4. `old/` Python 参考实现，仅用于兼容性对照。

不要再使用已废弃的 `start_feature`、`add_feature`、`check_spec` 等旧 MCP Probe Kit 流程作为当前开发前置条件。

## 参考实现

`old/` 仍用于行为回归和迁移参考，但不是当前 Runtime：

- `old/coding_tools_mcp/server.py`
- `old/apps/desktop-client/`
- `old/docs/profile-v0.1.md`
- `old/tests/compliance/`

---
*当前事实更新: 2026-08-18*
