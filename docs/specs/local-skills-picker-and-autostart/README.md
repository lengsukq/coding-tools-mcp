# local-skills-picker-and-autostart

## 原则

1. 诚实扫描、单一事实来源：本机与工作区 Skills/Instructions 的扫描、解析与按条启用过滤统一由 Rust `agent_context` 内核提供，前后端共享同一套基于规范路径生成的稳定 ID。
2. 开箱即用、减少干扰：默认直接呈现扫描到的本机/工作区 Skills 与 Agents（Instructions）列表并支持逐条勾选注入与否；传统客户端 Harness 勾选器退居默认折叠的高级面板。
3. 零冗余原语：开机自动启动使用各平台原生机制（macOS LaunchAgent、Windows HKCU Run、Linux XDG Autostart），开机轻量启动在 macOS 下不创建主 WebView（仅保留菜单栏托盘与后台 MCP 运行时），在 Rust 启动阶段直接完成 MCP 运行态恢复。

## 子规格索引

| ID | 标题 | FR | 依赖 |
|---|---|---|---|
| skills-scan-and-filter | 本机与工作区 Skills/Instructions 扫描修复及按条过滤 | FR-1, FR-2, FR-3 | 无 |
| skills-picker-ui | Skills/Agents 列表勾选注入界面与隐藏默认 Harness | FR-4, FR-5 | skills-scan-and-filter |
| autostart-and-lightweight-launch | 开机自动启动与开机轻量启动模式 | FR-6, FR-7, FR-8 | 无 |

## 依赖关系

- `skills-picker-ui` 依赖 `skills-scan-and-filter` 提供的扫描 DTO（完整 Skills/Instructions 列表与稳定 ID）以及全局/工作区禁用列表持久化字段。
- `autostart-and-lightweight-launch` 独立于 Skills 扫描模块，仅在 `GlobalRuntimeSettingsDto` / `AppSettings` / `AppData` 契约与 `GeneralSettingsView.vue` 设置页汇合。

## 里程碑

- M1：完成 `skills-scan-and-filter`（软链遍历、YAML 多行 frontmatter、全目录发现、按条过滤与单测）。
- M2：完成 `skills-picker-ui`（共享可勾选列表组件、全局与工作区视图改造、默认折叠 Harness）。
- M3：完成 `autostart-and-lightweight-launch`（跨平台开机自启原语、零 WebView 轻量启动、Rust 侧 MCP 启动恢复、启动行为设置 UI）。
- M4：全量回归验证（`npm run check`、`npm run build`、`npm run test:ui`、`cargo test --all-targets`、`cargo clippy`、`code_review` 与 `converge`）。
