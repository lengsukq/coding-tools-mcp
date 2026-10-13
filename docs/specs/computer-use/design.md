# Computer Use MCP — 架构设计与威胁模型

## 1. 现有组件与演进边界

当前 Global MCP `src-tauri/src/mcp/server.rs` 负责 `tools/list` 与 `tools/call`，并把 Workspace 工具路由至 `tools/dispatch`。图片文件的 `view_image` 经 `tools/tool_output.rs` 转为 MCP image 内容，但只能读取 Workspace 内已有文件。

Computer Use 属于 **Host 层**，不能伪装成 Workspace 工具。新 `src-tauri/src/computer.rs` 在 Gateway 的 Workspace 解析之前分发，输入身份取自 **服务端 MCP transport session**，不接受用户构造的 `workspace_id` 或 `_meta` 作为授予依据。现有 21 个 compact tool 不变。

## 2. 会话控制状态

```text
MCP computer_session(request)
  → server-managed transport session
  → request ID / pending (2 min TTL, only one pending)
  → 本机 Tauri 设置页显式 approve
  → grant(session ID, 5 min TTL, only one active)
  → MCP computer_observe(screenshot)
  → expires | MCP stop | 本机 revoke
```

- 无有效 MCP transport session 不允许申请或获取屏幕。
- 必须使用服务端初始化时签发的 MCP Session Header；兼容 Workspace 功能的客户端 `_meta` 会话 ID 不是截图身份凭据。
- 不同 session 请求并发时拒绝，不共享授权。
- 内存状态不写入应用配置；应用重启授权消失，用户可随时撤销。
- 本地 Tauri 的 approve/revoke 命令不通过 MCP 暴露。
- 不在 `structuredContent` 中复制截图 base64，避免日志和上下文重复暴露；截图仅在 MCP image `content` 中。

## 3. 截图数据流

当前引导实现：`/usr/sbin/screencapture` → 临时 PNG → 内存解码 → 最大 1920×1200 JPEG → MCP image + 文本 metadata。文件由 RAII TempFile 自动删除。没有经过通用 `exec_command` 的工具权限路径。

本机设置页使用 CoreGraphics `CGPreflightScreenCaptureAccess` 检测系统权限，只在本机用户点击按钮时调用 `CGRequestScreenCaptureAccess` 弹出系统授权。此预检以 Tauri 进程为准，不保证独立 `screencapture` 子进程的系统权限完全一致。

**已知限制**：这只是最小可用截图后端，不是原生 ScreenCaptureKit；操作可能受系统权限与桌面登录状态限制。当前有 12 秒子进程超时，但尚未完成多屏负原点与原始像素坐标的映射，也未进行 ChatGPT 端到端图片验收。下一步用 ScreenCaptureKit 的原生单帧截图替换该捕获适配器，加入 display/window/region 语义。

拟长期拆分：

- `ComputerProvider`：平台接口；macOS / Windows / Linux 各自实现。
- `ComputerSessionManager`：本机批准、会话身份、TTL、应用白名单、撤销。
- `ComputerObservation`：屏幕、窗口、Accessibility 元素树、图像像素与坐标元数据。
- `ComputerAction`：点击、键盘、拖拽、滚动与有限步调度。

## 4. 威胁模型

| 威胁 | 当前应对 | 后续 |
| --- | --- | --- |
| 远程 MCP 用户在未授权时获取屏幕 | 无有效本机 grant 拒绝；请求需在桌面明确批准 | 客户端可验证身份展示、应用/窗口白名单 |
| 客户端假冒其他 workspace 获取 Host 权限 | Host 工具在 Workspace 路由前处理，授权绑服务端 transport session | 更细分 host/scoped capability |
| 审计/History 意外记下敏感截图 | `structuredContent` 仅元数据；MCP image content 含图，必须控制服务端实际请求日志是否记录完整请求响应 | 逐链路日志脱敏和安全审计 |
| 过期授权或其他 MCP 会话复用 | TTL、内存 revoke、session ID 比较 | 授权与 OAuth 客户端 ID 双绑定、防重放 |
| 屏幕截图中包含密码/邮件 | 用户批准前显示隐私说明；目前不自动遮挡 | 窗口白名单、敏感窗口遮挡/拒绝 |
| 锁屏/拒绝屏幕权限 | 截图失败；不会绕过 TCC | 独立权限诊断与显式恢复提示 |
| 系统截图子进程卡住 | 12 秒超时后强制停止并回收子进程 | ScreenCaptureKit + async timeout/cancel |

## 5. 验证闸门

1. Rust：授权默认拒绝、授权/拒绝、超时、跨 session 隔离。
2. MCP：`tools/list` 两工具，`tools/call` 在无 Workspace 选择时可处理但仍拒绝未授权。
3. 前端：Gateway 设置页显示 pending ID、允许批准、撤销和剩余期限。
4. macOS：真实屏幕录制权限、运行中截图/锁屏失败、多屏与窗口/区域捕获 E2E。
5. 全量：`npm run test:release`、`cargo fmt`、Clippy、GitNexus 回归；核心路径手工安全审查。

## 6. 截图原型使用步骤

1. 重新构建并启动最新 Tauri 桌面应用以及 Global MCP Runtime（运行中的旧 MCP Runtime 不会自动更新工具列表）。
2. 使用支持 Streamable HTTP 且**保留服务端签发 `mcp-session-id` 请求头**的 MCP 客户端连接，发现 `computer_session`、`computer_observe`。
3. 客户端调用 `computer_session({"action":"request"})`，会收到待本机批准的请求编号。
4. 在 Coding Tools MCP → 设置 → MCP 连接 → Computer Use · 屏幕访问中批准此请求；必要时点击“请求系统权限”，在 macOS 中授予屏幕录制。
5. 客户端调用 `computer_observe({"action":"screenshot","display_index":1})`，取得 JPEG MCP image 和 `snapshot_id`/宽高等元信息。
6. 客户端调用 `computer_session({"action":"stop"})` 或用户在本机点击“立即撤销”；若无操作，批准将在 5 分钟后过期。

当前对仅传递 `_meta` 的无状态客户端严格拒绝授权，这类客户端的安全兼容方案应在 P4 专门设计，不得自动退回信任可伪造的会话 ID。
