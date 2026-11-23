# 子规格：本机与工作区 Skills/Instructions 扫描修复及按条过滤

## 范围

修复 Rust `agent_context` 目录扫描、YAML frontmatter 解析与全局/工作区扫描结果输出；引入全局与工作区级按条禁用列表并在 MCP Runtime 统一过滤。

## 需求回链

- FR-1
- FR-2
- FR-3

## 验收标准（EARS）

1. WHEN 本机或工作区技能目录中包含指向真实技能目录的符号链接（如 `~/.cursor/skills/*`、`~/.opencode/skills/*`、`~/.claude/skills/*`、`~/.codex/skills`、`~/.gemini/skills`） THEN 系统 SHALL 跟随符号链接完成扫描并按规范路径去重，同时安全忽略软链循环。
2. WHEN `SKILL.md` 使用 YAML 多行块标量（`>`、`>-`、`>+`、`|`、`|-`、`|+`）、CRLF 换行或超过 1024 字符的 `description` 时 THEN 系统 SHALL 完整解析出真实描述文本（超长时按 UTF-8 字符边界截断而非丢弃整条 Skill），并仅匹配顶层 frontmatter 键而不被缩进子键误覆盖。
3. WHEN 调用 `scan_global_agent_context` 扫描本机来源 THEN 系统 SHALL 覆盖 `~/.agents/skills`、`~/.agents/AGENTS.md`、`~/.gemini/skills`、`~/.gemini/config/skills`、`~/.gemini/GEMINI.md` 及各既有客户端目录与自定义路径，返回完整去重的 `skills: Vec<SkillDescriptor>` 与 `instructions: Vec<InstructionDocument>`（每条均带确定性 16 位 hex `id`）。
4. WHEN 用户在全局或工作区配置 `disabled_skill_ids` / `disabled_instruction_ids` THEN 系统 SHALL 在 `ToolContext::current_skills()`、`ToolContext::current_ai_instructions()`、`list_skills`、`get_skill`、`workspace_select` 及 `initialize` 中过滤对应条目，但在 UI 扫描命令（`scan_global_agent_context` / `scan_agent_context`）中仍返回完整条目列表以便重新勾选。

## 涉及文件

- `src-tauri/src/agent_context.rs`
- `src-tauri/src/agent_context/discovery.rs`
- `src-tauri/src/agent_context/presentation.rs`
- `src-tauri/src/commands/agent_context.rs`
- `src-tauri/src/commands/runtime_settings.rs`
- `src-tauri/src/data/model.rs`
- `src-tauri/src/data/migrate.rs`
- `src-tauri/src/settings/model.rs`
- `src-tauri/src/workspace/model.rs`
- `src-tauri/src/tools/context.rs`
- `src-tauri/src/mcp/gateway_state.rs`
- `src-tauri/src/mcp/server/dispatch.rs`
- `src-tauri/src/lan_peer/server.rs`

## 不做项

- 不修改 `compact` 工具档位暴露的 21 个工具名称列表。
- 不对用户本机 `~/.agents/skills` 或任意 `SKILL.md` 文件执行写操作。

## 设计要点

- `collect_skill_candidates`、`discover_global_skill_paths`、`add_cursor_rule_files` 与全局 cursor rules 扫描统一开启 `WalkDir::follow_links(true)`；工作区全盘 fallback 扫描 `collect_auto_workspace_skills` 保持不盲目跨出工作区跟随软链。
- `InstructionDocument` 新增 `id: String`（与 `SkillDescriptor.id` 同算法：`sha256_hex(canonical_path)[..16]`）。
- `scan_global_agent_context` 接收可选参数（兼容无参调用），默认扫描本机全部标准目录并合并已保存的 `global_custom_instruction_paths` / `global_custom_skill_paths`。
- `handle_workspace_select`（含 `lan_peer/server.rs`）附带已启用技能的轻量元数据列表 `skills`（上限 50 条），确保 `compact` 档位下勾选的 Skills 也可被客户端感知并按需读取。
