# 设计文档：local-skills-picker-and-autostart

## 概述

对应需求：FR-1、FR-2、FR-3、FR-4、FR-5、FR-6、FR-7、FR-8。

本设计将改动收敛为三个低耦合模块：
1. **Rust `agent_context` 扫描与过滤内核**（`skills-scan-and-filter`）：统一软链遍历、YAML frontmatter 解析、本机全局根目录发现与按 ID 过滤，作为全局设置、工作区设置与 MCP Runtime 的唯一事实来源。
2. **Vue 可勾选 Skills/Agents 选择器界面**（`skills-picker-ui`）：抽离共享展示与交互组件 `AgentContextPicker.vue`，统一全局与工作区的扫描展示、逐条勾选与默认折叠的高级来源配置。
3. **跨平台开机自启与轻量启动生命周期**（`autostart-and-lightweight-launch`）：在 `platform/autostart.rs` 封装系统启动项操作，在 `tauri.conf.json` 禁用自动建窗并由 `lib.rs` 按启动模式决定零 WebView 轻量驻留或正常建窗，同时在 Rust 侧直接触发幂等 MCP 恢复。

## 技术方案

### 跨模块数据契约

- **稳定条目 ID**：Skill 与 Instruction 均使用 `sha256_hex(canonical_path)[..16]` 作为 `id`，确保同一文件在全局扫描、工作区扫描与 MCP 工具运行时拥有完全一致的标识。
- **黑名单语义（默认注入，取消勾选即排除）**：
  - 全局持久化：`AppData` / `AppSettings` 新增 `global_disabled_skill_ids: Vec<String>`、`global_disabled_instruction_ids: Vec<String>`（`GlobalRuntimeSettingsDto` 字段名 `disabledSkillIds`、`disabledInstructionIds`）。
  - 工作区持久化：`RuntimeConfig` 新增 `disabled_skill_ids: Vec<String>`、`disabled_instruction_ids: Vec<String>`。
  - 运行时合并：`GatewayState::build_request_context` 将全局与工作区禁用 ID 取并集注入 `AgentContextRuntimeConfig`，由 `ToolContext::current_skills()` 与 `ToolContext::current_ai_instructions()` 统一过滤；扫描接口始终返回完整列表供 UI 重新勾选。
- **开机自启与轻量启动持久化**：
  - `AppData` / `AppSettings` 新增 `launch_at_login: bool`、`launch_in_lightweight_mode: bool`（`GlobalRuntimeSettingsDto` 字段名 `launchAtLogin`、`launchInLightweightMode`）。
  - 仅在 `launch_at_login` 发生变化时同步操作系统启动项，避免保存其他设置页时误改系统 LaunchAgent/注册表。

## 文件结构

- 后端扫描与过滤：
  - `src-tauri/src/agent_context.rs`
  - `src-tauri/src/agent_context/discovery.rs`
  - `src-tauri/src/agent_context/presentation.rs`
  - `src-tauri/src/commands/agent_context.rs`
  - `src-tauri/src/tools/context.rs`
  - `src-tauri/src/mcp/gateway_state.rs`
  - `src-tauri/src/mcp/server/dispatch.rs`
  - `src-tauri/src/lan_peer/server.rs`
- 后端设置与自启/轻量启动：
  - `src-tauri/src/platform/autostart.rs`（新增）
  - `src-tauri/src/platform/mod.rs`
  - `src-tauri/src/data/model.rs`
  - `src-tauri/src/data/migrate.rs`
  - `src-tauri/src/settings/model.rs`
  - `src-tauri/src/workspace/model.rs`
  - `src-tauri/src/commands/runtime_settings.rs`
  - `src-tauri/src/commands/runtime.rs`
  - `src-tauri/src/commands/window_chrome.rs`
  - `src-tauri/src/lib.rs`
  - `src-tauri/tauri.conf.json`
- 前端 API、状态与界面：
  - `src/lib/agent-context.ts`
  - `src/lib/api/agent-context.ts`
  - `src/lib/api/settings.ts`
  - `src/lib/types.ts`
  - `src/lib/workspace-page.ts`
  - `src/lib/global-search.ts`
  - `src/components/settings/AgentContextPicker.vue`（新增）
  - `src/components/settings/GeneralContextCard.vue`
  - `src/components/workspace/RuntimePolicyPanel.vue`
  - `src/components/workspace/WorkspaceConfiguration.vue`
  - `src/components/workspace/AgentContextInspector.vue`
  - `src/views/settings/GeneralSettingsView.vue`
- 回归测试：
  - `tests/compact-menubar.test.mjs`
  - `tests/agent-context-picker-and-autostart.test.mjs`（新增）

## 设计决策

1. **采用排除列表（disabled IDs）而非白名单**：兼容旧版本升级与新安装 Skill 自动生效，且用户可随时单条或批量关闭不需要注入的 Skill（例如 `.system/*`）。
2. **用户态 LaunchAgent 替代 `SMAppService`**：项目支持自签名 macOS 打包发布，用户态 `~/Library/LaunchAgents/com.codingtools.mcp.desktop.plist` 无需官方 Apple Developer 签名，且天然支持传入 `--autostart` 参数区分开机自启与手动启动。
3. **启动参数 `--autostart` + 读取配置 `launch_in_lightweight_mode`**：切换“开机时轻量启动”开关时无需重写系统启动项；同时保留 `--lightweight` 显式参数便于本地验证与排障。
4. **`tauri.conf.json` 配置 `create: false` 由 Rust 托管建窗**：在 macOS 轻量启动时彻底避免预先创建和销毁主 WebView，杜绝启动闪窗与多余 WebKit 进程；非 macOS 轻量启动时以 `visible(false)` 静默建窗驻留托盘，避免 Windows WebView2 同步建窗死锁。
