<!-- mcp-probe:context begin — auto-generated; re-run init_project_context updates this block only -->
<!-- mcp-probe:context-version: 4.0.1 -->
## MCP（必须先调）
需已配置 mcp-probe-kit。写代码前先读 Skill：@.agents/skills/mcp-probe-kit/SKILL.md（或 [MCP 调用时机](.agents/skills/mcp-probe-kit/SKILL.md)）（首次 MCP 调用自动创建 Skill 文件）。

- 用户只说“继续 / 开始 / 往下做” → **先调用 `resume_plan`**；已知 `plan_id` 则传入，未知则只传 `project_root` 自动恢复最近的 active/blocked Plan；未确认无可恢复 Plan 前，禁止先用 Bash 探索、调用 `workflow` 或重新调用 `start_*`；恢复成功且 `mustContinue=true` 后禁止只汇报“已恢复”，必须立即执行 `nextStep/nextTool`，每步后调用 `plan_heartbeat`，直到阻断、取消或收敛
- 不确定用哪个 MCP → `workflow`（只返回工具选择指南；由 Agent 自己判断，必要时再澄清）
- 当前会话看不到 MCP 工具 → 读取 Skill 的“执行通道与自动降级”，通过 `.mcp-probe-kit/bin/probe.*` 调用同版本 CLI；不要要求用户安装
- 新功能 → `start_feature`（会先搜记忆）
- Bug → `start_bugfix`（会先搜记忆）
- UI → `start_ui`（会先搜记忆）
- 不熟代码 / 影响面 → `code_insight`（context / impact / auto）
- 缺上下文 → `init_project_context`
- 提交 → `gencommit`

上下文：写代码前先读 [project-context](./docs/project-context.md)（链到 `docs/project-context/` 各文档）
图谱：大改前读 [latest](./docs/graph-insights/latest.md)；过期 `code_insight` mode=auto save_to_docs=true
记忆（需 MEMORY_QDRANT_URL 等已配置）：
- 检索：`start_*` 命中后**自动注入**历史经验全文；中途补查可用 `search_memory`；单条精读仍可用 `read_memory_asset`
- 沉淀：跨仓库共享**勿填** source_project/source_path；路径写进 content；summary 写检索关键词
- 修正：已有资产可用 `update_memory_asset` 按 asset_id 原地更新（保留 ID）
- 清理：过时/错误/重复沉淀可用 `delete_memory_asset`（删除前建议 `read_memory_asset` 确认）
- Bug 每轮验证后先准备成功/失败/证伪/回归候选并写入 `plan_heartbeat`；`converge` 通过后再 `memorize_asset`
- 功能/UI 验证后先准备候选并写入 `plan_heartbeat`；`converge` 通过后再 `memorize_asset` type=`pattern`/`component`
<!-- mcp-probe:context end -->

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **coding-tools-mcp** (5492 symbols, 12767 relationships, 441 execution flows).

> Index stale? Run `node .gitnexus/run.cjs analyze --index-only` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? Bootstrap with `npx`, `bunx`, or `pnpm dlx` — e.g. `bunx gitnexus@latest analyze` (npm 11 npx crash; #1939).

## Always Do

- **MUST run impact before editing.** Use `impact({target: "symbolName", direction: "upstream"})` or `node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .`; report callers, processes, and risk. Never substitute grep for graph analysis.
- **MUST analyze graph changes before committing.** Use `detect_changes({scope: "all"})` (MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` (CLI fallback). `partial: true` or `truncated: true` is not a clean check — a zero means unseen, not unaffected; re-run it. For regression review: `detect_changes({scope: "compare", base_ref: "main"})` or `node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .`.
- MUST warn on HIGH/CRITICAL `risk` pre-edit; never use `riskSharedAxes` to waive a HIGH/CRITICAL `risk` warning. Compare File/symbol: MCP File omits axes; Graph-RAG expands File.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can also mean the callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). `impact` pairs `UNKNOWN` with a `riskNote` saying so. Confirm with a text search before treating the symbol as safe to change or delete; do not proceed on the strength of a zero.
- **MUST use `query({search_query: "concept"})` for concepts/flows, `context({name: "symbolName"})` for a named symbol, or `impact` for blast radius, on read-only callers, dependencies, imports, or execution flow.** Graph first; text search only for empty/`UNKNOWN`/literals.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method before MCP/CLI impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis, and never read `UNKNOWN` as an all-clear — it means the walk could not answer, which is the one verdict that requires confirming by other means.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit before MCP/CLI graph change analysis.

## Resources

| Resource | Use for |
| --- | --- |
| `gitnexus://repo/coding-tools-mcp/context` | Codebase overview, check index freshness |
| `gitnexus://repo/coding-tools-mcp/clusters` | All functional areas |
| `gitnexus://repo/coding-tools-mcp/processes` | All execution flows |
| `gitnexus://repo/coding-tools-mcp/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
| --- | --- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
