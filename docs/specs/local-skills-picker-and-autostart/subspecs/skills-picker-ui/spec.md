# 子规格：Skills/Agents 列表勾选注入界面与隐藏默认 Harness

## 范围

构建共享可勾选的 Skills 与 Agents（Instructions）列表组件，并改造全局设置 `GeneralContextCard.vue` 与工作区设置 `RuntimePolicyPanel.vue`，默认不展示客户端 Harness 勾选器而直接呈现扫描结果供逐条勾选注入。

## 需求回链

- FR-4
- FR-5

## 验收标准（EARS）

1. WHEN 用户打开「应用设置 → 执行与桌面操控 → AI 指令与技能来源」 THEN 系统 SHALL 自动扫描并直接展示本机发现的 Skills 与 Agents（Instructions）可勾选列表，且客户端 Harness 勾选器与自定义路径默认处于收起状态。
2. WHEN 用户在列表中使用搜索、类型标签（全部 / Skills / Agents 指令）以及单条勾选或「全选 / 全部取消」操作 THEN 系统 SHALL 实时更新 `disabledSkillIds` / `disabledInstructionIds` 草稿并显示保存栏，保存后立即热更新到运行中的 MCP。
3. WHEN 用户在全局设置中先前保存了旧版客户端来源筛选（非 `auto` 且非空） THEN 系统 SHALL 在高级折叠入口给出简洁状态提示并提供一键「恢复自动扫描全部」操作，不强行在主视图展开 Harness 列表。
4. WHEN 用户打开工作区「设置 → AI 指令与技能上下文」 THEN 系统 SHALL 自动扫描当前工作区生效的 Skills 与 Agents 列表并支持逐条勾选是否注入（对全局已禁用项明确标注「已在全局禁用」），同时默认隐藏工作区 Harness 勾选列表于可折叠高级区中。

## 涉及文件

- `src/lib/agent-context.ts`
- `src/lib/api/agent-context.ts`
- `src/lib/api/settings.ts`
- `src/lib/types.ts`
- `src/lib/workspace-page.ts`
- `src/lib/global-search.ts`
- `src/components/settings/AgentContextPicker.vue`
- `src/components/settings/GeneralContextCard.vue`
- `src/components/workspace/RuntimePolicyPanel.vue`
- `src/components/workspace/WorkspaceConfiguration.vue`
- `src/components/workspace/AgentContextInspector.vue`
- `src/views/settings/GeneralSettingsView.vue`

## 不做项

- 不新增独立顶层路由或破坏现有设置子导航 Hash 地址映射。
- 不删除底层 `instructionSources` / `skillSources` 字段，保留于高级折叠区确保老配置兼容。

## 设计要点

- 抽离纯交互展示组件 `AgentContextPicker.vue`，接收 `instructions`、`skills`、`disabledSkillIds`、`disabledInstructionIds` 及可选的 `globallyDisabledSkillIds` / `globallyDisabledInstructionIds`，通过 emit 回传更新后的禁用 ID 数组，遵循「容器持有状态 + 子组件 props/emits 协作」架构约束。
- `GeneralSettingsView.vue` 的 `owned` 与 `cloneRuntimeSettings` 纳入 `disabledSkillIds` 与 `disabledInstructionIds`，确保保存队列不互相覆盖。
- `RuntimePolicyPanel.vue` 挂载时自动扫描，并在保存后自动刷新扫描结果而不清空列表。
