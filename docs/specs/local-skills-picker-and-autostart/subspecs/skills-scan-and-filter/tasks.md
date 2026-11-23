# 子任务：本机与工作区 Skills/Instructions 扫描修复及按条过滤

- [x] 1.1 修复 `discovery.rs` 软链遍历与 `agents`/`gemini` 标准目录发现，修复 `presentation.rs` YAML 多行块标量与 CRLF 解析并补充单元测试
  - 证据块: `src-tauri/src/agent_context/discovery.rs:58-120,322-343,501-533`; `src-tauri/src/agent_context/presentation.rs:46-82`
  - 涉及文件: `src-tauri/src/agent_context/discovery.rs`, `src-tauri/src/agent_context/presentation.rs`, `src-tauri/src/agent_context.rs`
  - _需求: FR-1_

- [x] 1.2 为 `InstructionDocument` 增加确定性 `id`，扩展 `GlobalAgentContextScan` 返回已解析的 `instructions` 与 `skills`，并让 `scan_global_agent_context` 合并自定义路径
  - 证据块: `src-tauri/src/agent_context.rs:40-88,146-187`; `src-tauri/src/commands/agent_context.rs:10-61`
  - 涉及文件: `src-tauri/src/agent_context.rs`, `src-tauri/src/commands/agent_context.rs`
  - _需求: FR-2_

- [x] 1.3 持久化全局与工作区 `disabled_skill_ids` / `disabled_instruction_ids`，在 `GatewayState`、`ToolContext`、`workspace_select` 与 `lan_peer` 完成按条过滤与单元测试
  - 证据块: `src-tauri/src/data/model.rs:35-44`; `src-tauri/src/settings/model.rs:250-260`; `src-tauri/src/workspace/model.rs:130-142`; `src-tauri/src/tools/context.rs:167-198`; `src-tauri/src/mcp/gateway_state.rs:248-266`; `src-tauri/src/mcp/server/dispatch.rs:598-615`
  - 涉及文件: `src-tauri/src/data/model.rs`, `src-tauri/src/data/migrate.rs`, `src-tauri/src/settings/model.rs`, `src-tauri/src/workspace/model.rs`, `src-tauri/src/commands/runtime_settings.rs`, `src-tauri/src/agent_context.rs`, `src-tauri/src/tools/context.rs`, `src-tauri/src/mcp/gateway_state.rs`, `src-tauri/src/mcp/server/dispatch.rs`, `src-tauri/src/lan_peer/server.rs`
  - _需求: FR-3_
