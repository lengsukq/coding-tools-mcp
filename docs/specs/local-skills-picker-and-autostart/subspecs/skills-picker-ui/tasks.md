# 子任务：Skills/Agents 列表勾选注入界面与隐藏默认 Harness

- [x] 2.1 更新前端类型与 API 契约，新增共享可勾选组件 `AgentContextPicker.vue`（支持搜索、类型筛选、单条勾选、全选/取消与全局禁用标识）
  - 证据块: `src/lib/agent-context.ts:1-22`; `src/lib/api/agent-context.ts:1-43`; `src/lib/api/settings.ts:54-77`; `src/components/workspace/AgentContextInspector.vue:1-154`
  - 涉及文件: `src/lib/agent-context.ts`, `src/lib/api/agent-context.ts`, `src/lib/api/settings.ts`, `src/lib/types.ts`, `src/lib/workspace-page.ts`, `src/lib/global-search.ts`, `src/components/settings/AgentContextPicker.vue`, `src/components/workspace/AgentContextInspector.vue`
  - _需求: FR-4, FR-5_

- [x] 2.2 改造 `GeneralContextCard.vue` 与 `GeneralSettingsView.vue`，默认隐藏 Harness 并直接展示本机扫描出的 Skills 与 Agents 勾选列表
  - 证据块: `src/components/settings/GeneralContextCard.vue:1-98`; `src/views/settings/GeneralSettingsView.vue:51-120,197-217,397-411`
  - 涉及文件: `src/components/settings/GeneralContextCard.vue`, `src/views/settings/GeneralSettingsView.vue`
  - _需求: FR-4_

- [x] 2.3 改造 `RuntimePolicyPanel.vue` 与 `WorkspaceConfiguration.vue`，默认自动扫描并展示可勾选的 Skills 与 Agents 列表（默认折叠 Harness，标记全局禁用项）
  - 证据块: `src/components/workspace/RuntimePolicyPanel.vue:30-165,220-308`; `src/components/workspace/WorkspaceConfiguration.vue:40-54`
  - 涉及文件: `src/components/workspace/RuntimePolicyPanel.vue`, `src/components/workspace/WorkspaceConfiguration.vue`
  - _需求: FR-5_
