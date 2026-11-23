# 子任务：开机自动启动与开机轻量启动模式

- [x] 3.1 新增 `src-tauri/src/platform/autostart.rs` 跨平台开机自启管理（macOS LaunchAgent / Windows HKCU Run / Linux XDG Autostart）及路径转义/生成单元测试
  - 证据块: `src-tauri/src/platform/mod.rs:36-58`; `src-tauri/src/platform/command.rs`
  - 涉及文件: `src-tauri/src/platform/autostart.rs`, `src-tauri/src/platform/mod.rs`
  - _需求: FR-6_

- [x] 3.2 持久化 `launch_at_login` / `launch_in_lightweight_mode`，改造 `tauri.conf.json`、`window_chrome.rs`、`runtime.rs` 与 `lib.rs` 支持零 WebView 轻量启动、Reopen 恢复与 Rust 启动期 MCP 恢复
  - 证据块: `src-tauri/tauri.conf.json:12-25`; `src-tauri/src/lib.rs:345-495`; `src-tauri/src/commands/window_chrome.rs:14-175`; `src-tauri/src/commands/runtime.rs:346-362`; `src-tauri/src/commands/runtime_settings.rs:7-137`
  - 涉及文件: `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs`, `src-tauri/src/commands/window_chrome.rs`, `src-tauri/src/commands/runtime.rs`, `src-tauri/src/commands/runtime_settings.rs`, `src-tauri/src/data/model.rs`, `src-tauri/src/data/migrate.rs`, `src-tauri/src/settings/model.rs`
  - _需求: FR-6, FR-7_

- [x] 3.3 在 `GeneralSettingsView.vue` 启动行为卡片添加「开机自动启动」与「开机时轻量启动」开关、搜索关键词与前后端回归测试
  - 证据块: `src/views/settings/GeneralSettingsView.vue:105-120,349-363`; `src/lib/global-search.ts`; `tests/compact-menubar.test.mjs:1-45`
  - 涉及文件: `src/views/settings/GeneralSettingsView.vue`, `src/lib/api/settings.ts`, `src/lib/global-search.ts`, `tests/compact-menubar.test.mjs`, `tests/agent-context-picker-and-autostart.test.mjs`
  - _需求: FR-8_
