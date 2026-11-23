# 任务清单：local-skills-picker-and-autostart

## 交付物清单

- 子规格任务是唯一任务明细；本文件只维护阶段索引和引用矩阵。

## 任务列表

1. 阶段一（后端扫描与过滤）：执行 `subspecs/skills-scan-and-filter/tasks.md`（1.1 ~ 1.3）。
2. 阶段二（前端勾选列表与默认隐藏 Harness）：执行 `subspecs/skills-picker-ui/tasks.md`（2.1 ~ 2.3）。
3. 阶段三（开机自启与轻量启动）：执行 `subspecs/autostart-and-lightweight-launch/tasks.md`（3.1 ~ 3.3）。

## 需求覆盖矩阵

| FR ID | 子规格 | 任务引用 | 状态 |
|---|---|---|---|
| FR-1 | skills-scan-and-filter | skills-scan-and-filter/1.1 | 已完成 |
| FR-2 | skills-scan-and-filter | skills-scan-and-filter/1.2 | 已完成 |
| FR-3 | skills-scan-and-filter | skills-scan-and-filter/1.3 | 已完成 |
| FR-4 | skills-picker-ui | skills-picker-ui/2.1, skills-picker-ui/2.2 | 已完成 |
| FR-5 | skills-picker-ui | skills-picker-ui/2.1, skills-picker-ui/2.3 | 已完成 |
| FR-6 | autostart-and-lightweight-launch | autostart-and-lightweight-launch/3.1 | 已完成 |
| FR-7 | autostart-and-lightweight-launch | autostart-and-lightweight-launch/3.2 | 已完成 |
| FR-8 | autostart-and-lightweight-launch | autostart-and-lightweight-launch/3.3 | 已完成 |

## 文件变更清单

由各子规格汇总：
- `skills-scan-and-filter`: `src-tauri/src/agent_context.rs`, `src-tauri/src/agent_context/discovery.rs`, `src-tauri/src/agent_context/presentation.rs`, `src-tauri/src/commands/agent_context.rs`, `src-tauri/src/tools/context.rs`, `src-tauri/src/mcp/gateway_state.rs`, `src-tauri/src/mcp/server/dispatch.rs`, `src-tauri/src/lan_peer/server.rs`, `src-tauri/src/data/model.rs`, `src-tauri/src/data/migrate.rs`, `src-tauri/src/settings/model.rs`, `src-tauri/src/workspace/model.rs`, `src-tauri/src/commands/runtime_settings.rs`
- `skills-picker-ui`: `src/lib/agent-context.ts`, `src/lib/api/agent-context.ts`, `src/lib/api/settings.ts`, `src/lib/types.ts`, `src/lib/workspace-page.ts`, `src/lib/global-search.ts`, `src/components/settings/AgentContextPicker.vue`, `src/components/settings/GeneralContextCard.vue`, `src/components/workspace/RuntimePolicyPanel.vue`, `src/components/workspace/WorkspaceConfiguration.vue`, `src/components/workspace/AgentContextInspector.vue`（已删除，由共享组件替代）, `src/views/settings/GeneralSettingsView.vue`
- `autostart-and-lightweight-launch`: `src-tauri/src/platform/autostart.rs`, `src-tauri/src/platform/mod.rs`, `src-tauri/src/commands/runtime.rs`, `src-tauri/src/commands/runtime_settings.rs`, `src-tauri/src/commands/window_chrome.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src/views/settings/GeneralSettingsView.vue`, `tests/compact-menubar.test.mjs`, `tests/agent-context-picker-and-autostart.test.mjs`

## 子规格任务覆盖矩阵

| 子规格任务 | 覆盖 FR | 依赖 | 状态 |
|---|---|---|---|
| skills-scan-and-filter/1.1 | FR-1 | 无 | 已完成 |
| skills-scan-and-filter/1.2 | FR-2 | skills-scan-and-filter/1.1 | 已完成 |
| skills-scan-and-filter/1.3 | FR-3 | skills-scan-and-filter/1.2 | 已完成 |
| skills-picker-ui/2.1 | FR-4, FR-5 | skills-scan-and-filter/1.2 | 已完成 |
| skills-picker-ui/2.2 | FR-4 | skills-picker-ui/2.1, skills-scan-and-filter/1.3 | 已完成 |
| skills-picker-ui/2.3 | FR-5 | skills-picker-ui/2.1, skills-scan-and-filter/1.3 | 已完成 |
| autostart-and-lightweight-launch/3.1 | FR-6 | 无 | 已完成 |
| autostart-and-lightweight-launch/3.2 | FR-7 | autostart-and-lightweight-launch/3.1 | 已完成 |
| autostart-and-lightweight-launch/3.3 | FR-8 | autostart-and-lightweight-launch/3.1, autostart-and-lightweight-launch/3.2 | 已完成 |
