# 项目图谱洞察

更新时间：2026-11-24。已完成全项目 5 轮 Clean Code 与工程化优化（Wave 1–5），保留完整 MCP-only、LAN 联邦与 Upstream MCP 聚合/接管架构，前端与脚本链已统一迁移至 `pnpm`。

## 索引与覆盖范围

本机已通过 GitNexus CLI 刷新索引（schema 4）：9,734 节点、26,424 关系、431 集群、668 执行流。全量工作区 `detect-changes --scope all` 覆盖 312 个文件、584 个符号、373 条受影响执行流（整体风险级 `critical`，无 `partial` / `truncated` 标记）。跨语言 Tauri IPC 契约已由 `tests/ipc-contract.test.mjs` 进行 1:1 自动化校验，消除图谱无法解析跨语言调用的盲区。

## 当前主链路

Vue 路由 / 页面 → `ui-model` / `workspace-status-cache` / 领域 composable（`use-gateway-settings`、`use-lan-peers`、`use-upstream-mcp` + `upstream-draft`）→ `api` Tauri invoke → Rust `commands` / `AppState` → `DataStore` / `RuntimeSupervisor` → MCP Gateway（`listener` / `gateway_handlers` / `upstream_mcp::manager` / `lan_peer`）与统一工具执行内核。

共享底层基础设施已收口：`util/{fs,http,json,text,time}`、`mcp/sse`、`tunnel/process`、`upstream_mcp/rpc_mux`、`platform/process_common`、`src/lib/{formatters,validators,events}`、`src/components/settings/KeyValueSecretEditor.vue`。

## 回归证据

- `pnpm run verify` 全绿：版本一致性（`0.3.10`）、UI 组件目录同步（14 项）、`vue-tsc --noEmit`、`biome lint --error-on-warnings`（0 error / 0 warning）、`knip`（0 未使用文件/导出/依赖）、70 项前端测试（含 IPC 契约与 `formatters`/`validators` 行为测试）、Vite 生产构建。
- Rust 双目标验证全绿：`cargo fmt --check`、桌面端与无头端（`--no-default-features`）`cargo clippy -- -D warnings` 零警告；桌面端 490 项（392 单元 + 98 集成）与无头端 484 项（386 单元 + 98 集成）测试全部通过。
