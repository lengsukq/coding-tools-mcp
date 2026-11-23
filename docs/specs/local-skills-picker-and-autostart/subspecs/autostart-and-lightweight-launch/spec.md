# 子规格：开机自动启动与开机轻量启动模式

## 范围

实现跨平台开机自动启动注册/注销能力、开机轻量启动（macOS 零 WebView 菜单栏驻留 / Windows & Linux 静默托盘启动）与 Rust 启动期 MCP 恢复，并在「外观与启动」设置页提供对应开关。

## 需求回链

- FR-6
- FR-7
- FR-8

## 验收标准（EARS）

1. WHEN 用户开启「开机自动启动」并保存 THEN 系统 SHALL 在当前平台写入开机自启项（macOS：`~/Library/LaunchAgents/com.codingtools.mcp.desktop.plist`，Windows：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 键 `CodingToolsMCP`，Linux：`~/.config/autostart/com.codingtools.mcp.desktop.desktop`），启动参数包含 `--autostart`；关闭并保存时 SHALL 清理该启动项；系统写入失败时 SHALL 返回明确错误且不将状态误存为成功。
2. WHEN 应用以 `--autostart` 启动且 `launch_in_lightweight_mode` 为 `true`（或显式携带 `--lightweight`） THEN 系统 SHALL 在 macOS 下不创建主 WebView、启用 `COMPACT_MODE` 并切换为 `ActivationPolicy::Accessory` 仅驻留菜单栏（在非 macOS 下以隐藏态建窗驻留系统托盘），且在 Rust `.setup()` 中幂等执行共享 MCP 服务恢复。
3. WHEN 应用处于轻量启动态且用户点击托盘「恢复主窗口」或在 macOS 触发 `RunEvent::Reopen` THEN 系统 SHALL 按窗口配置创建并展示主窗口，恢复 `ActivationPolicy::Regular` 并同步刷新托盘状态。
4. WHEN 用户打开「应用设置 → 外观与启动 → 启动行为」 THEN 系统 SHALL 展示「开机自动启动」、「开机时轻量启动」（仅在开启开机自启时可编辑）与「启动时恢复 MCP 服务」三个开关，并在开启开机轻量启动但未开启恢复 MCP 时给出友好提示。

## 涉及文件

- `src-tauri/src/platform/autostart.rs`
- `src-tauri/src/platform/mod.rs`
- `src-tauri/src/data/model.rs`
- `src-tauri/src/data/migrate.rs`
- `src-tauri/src/settings/model.rs`
- `src-tauri/src/commands/runtime_settings.rs`
- `src-tauri/src/commands/runtime.rs`
- `src-tauri/src/commands/window_chrome.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/tauri.conf.json`
- `src/lib/api/settings.ts`
- `src/lib/global-search.ts`
- `src/views/settings/GeneralSettingsView.vue`
- `tests/compact-menubar.test.mjs`
- `tests/agent-context-picker-and-autostart.test.mjs`

## 不做项

- 不引入额外第三方 Rust crate（如 `tauri-plugin-autostart`），直接使用各平台稳定原生机制。
- 不改变用户手动点击应用图标启动时的正常主窗口行为。

## 设计要点

- `platform/autostart.rs` 提供纯函数拼装 plist / `.desktop` / Windows 命令值，以便在单元测试中无副作用验证路径转义与参数格式；真实读写隔离在平台函数中。
- `commands/runtime.rs` 抽取 `restore_runtime_state_inner(&AppState)`：Rust `.setup()` 异步任务启动恢复；前端 `restore_runtime_state` 若检测到恢复已触发则等待 `RESTART_GATE` 完成，消除前端启动后立即读到 `stopped` 的竞态。
- `tauri.conf.json` 为主窗口显式声明 `"label": "main"` 与 `"create": false`，在 `lib.rs` `.setup()` 根据是否轻量启动决定建窗或直接进入轻量驻留。
- 仅当 `set_global_runtime_settings` 检测到 `launch_at_login` 发生变化时才调用 `platform::autostart::set_enabled(...)`，避免保存其他设置分区时误写系统启动项。
