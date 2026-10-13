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
  → grant(session ID, observe/control scope, 5 min TTL, only one active)
  → MCP computer_observe(windows/screenshot/ui_tree)
  → MCP computer_action(click/key/type/drag/...；control grant only)
  → expires | MCP stop | 本机 revoke
```

- 无有效 MCP transport session 不允许申请或获取屏幕。
- 必须使用服务端初始化时签发的 MCP Session Header；兼容 Workspace 功能的客户端 `_meta` 会话 ID 不是截图身份凭据。
- HTTP 请求通过现有认证层验证后，Computer Use 将该 Session 与已验证的 OAuth client_id / 共用 Bearer 主体 / 本地无认证模式主体绑定。同一会话头不能被另一个 OAuth 客户端直接复用。共用 Bearer 和无认证模式本身不能区分多个使用者，应只在可信部署中使用；公网应优先采用 OAuth。
- 不同 session 请求并发时拒绝，不共享授权。
- 内存状态不写入应用配置；应用重启授权消失，用户可随时撤销。
- 本地 Tauri 的 approve/revoke 命令不通过 MCP 暴露。
- control 只能由用户从运行中的程序里明确选择一个 bundle ID；运行时验证目标仍是前台应用，`focus_app` 只能激活该应用。observe 不能调用任何操作工具。
- 每次控制授权最多 100 个动作，跨 MCP 请求串行执行；密码输入框禁止通过 type 输入，明显破坏性 AX 按钮和部分危险快捷键默认禁用。这些限制**不能完全识别纯坐标点击的风险**。
- 每次授权附带唯一 lease token。屏幕/窗口/AX 观察在执行结束、返回数据之前再次检查同一 token，以拒绝撤销或更换租约期间完成的旧观察。用户在本机撤销和客户端 stop 与控制动作串行化，保证撤销完成后不启动旧动作。已发送到系统的事件不能回滚。
- 每次截图生成仅限当前 lease 的 `snapshot_id` 和坐标框（仅保存元数据，不缓存图像）；鼠标位置类动作必须在 **45 秒内**携带该 `snapshot_id`。老快照、跨会话快照、越界坐标都拒绝。不提供无限期复用截图坐标的能力。
- 不在 `structuredContent` 中复制截图 base64，避免日志和上下文重复暴露；截图仅在 MCP image `content` 中。

## 3. 截图数据流

当前实现：`src/platform/macos/computer_native.m` 的同进程 Objective-C 桥接 → `SCShareableContent` 获取显示器和窗口 → `SCScreenshotManager captureImageWithFilter:configuration:completionHandler:` → 内存 JPEG（最大 1920×1200，8MiB）→ MCP image + 元信息。没有截图文件或通用 `exec_command`。

映射由可单测的 `src-tauri/src/computer_geometry.rs` 完成，使用截图中 `origin_x`/`origin_y`/`point_width`/`point_height`/`pixel_width`/`pixel_height`。动作设置 `coordinate_space=image_pixels` 时，`x/y` 是图像内像素序号（0 起始）；默认 `screen_points` 则直接使用全局 macOS 逻辑坐标。两种模式都必须附带有效的 `snapshot_id`。转换后再由 CGEvent 二次核对屏幕实际边界，避免将 Retina 缩放像素误当屏幕 points。

本机设置页使用 `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess` 检测/请求系统截图授权，以及 `AXIsProcessTrusted` / `AXIsProcessTrustedWithOptions` 检测/请求辅助功能授权。系统授权申请只由本机用户手势触发。

**已知限制**：当前已有原生 ScreenCaptureKit 以及坐标映射元数据，仍需验证其在 Retina、多显示器负坐标、窗口/区域下的实际准确性；尚未确认锁屏、TCC 权限撤销及 ChatGPT 图片回包的端到端表现。高风险点击目前不能按实际 UI 语义自动判断，需另行实现独立确认。

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
| 过期授权或其他 MCP 会话复用 | TTL、唯一 lease token、跨会话隔离，已验证 OAuth client_id 与 Session 绑定，调用结束前二次授权检查 | 更细分的 token 轮换/撤销与 OAuth 防重放测试 |
| 过时截图引发误点 | 45 秒 snapshot TTL、租约/session 绑定、截图区域边界检查及 Retina 负坐标映射单测 | 锁定具体窗口及更强的界面变化检测 |
| 屏幕截图中包含密码/邮件 | 用户批准前显示隐私说明；目前不自动遮挡 | 窗口白名单、敏感窗口遮挡/拒绝 |
| 锁屏/拒绝屏幕权限 | 截图失败；不会绕过 TCC | 独立权限诊断与显式恢复提示 |
| 原生截图等待卡住 | SC 内容发现 7 秒超时、帧获取 8 秒超时；没有文件和截图子进程 | 强化取消与锁屏检测 |

## 5. 验证闸门

1. Rust：授权默认拒绝、授权/拒绝、超时、跨 session 隔离。
2. MCP：`tools/list` 三工具，`tools/call` 在无 Workspace 选择时可处理但仍拒绝未授权。
3. 前端：Gateway 设置页显示 pending ID、允许批准、撤销和剩余期限。
4. macOS：真实屏幕录制权限、运行中截图/锁屏失败、多屏与窗口/区域捕获 E2E。
5. 全量：`npm run test:release`、`cargo fmt`、Clippy、GitNexus 回归；核心路径手工安全审查。

## 6. 截图原型使用步骤

1. 重新构建并启动最新 Tauri 桌面应用以及 Global MCP Runtime（运行中的旧 MCP Runtime 不会自动更新工具列表）。
2. 使用支持 Streamable HTTP 且**保留服务端签发 `mcp-session-id` 请求头**的 MCP 客户端连接，发现 `computer_session`、`computer_observe`。
3. 客户端调用 `computer_session({"action":"request"})`，会收到待本机批准的请求编号。
4. 在 Coding Tools MCP → 设置 → MCP 连接 → Computer Use · 屏幕访问中批准此请求；必要时点击“请求系统权限”，在 macOS 中授予屏幕录制。
5. 客户端调用 `computer_observe({"action":"windows"})` 获得显示器/窗口 ID；再调用 `computer_observe({"action":"screenshot","display_index":1})`，取得 JPEG MCP image、`snapshot_id` 和坐标转换元信息。也可传 `window_id` 或全局屏幕 points 的 `region`。
6. 若要操作，请重新申请 `computer_session({"action":"request","scope":"control"})`，在本机明确选择可控制应用并批准。之后通过 `computer_action({"action":"focus_app"})` 激活目标应用，再使用该控制会话执行一次 `computer_observe({"action":"screenshot"})`。截图返回 `snapshot_id`，例如 `computer_action({"action":"click","x":220,"y":120,"coordinate_space":"image_pixels","snapshot_id":"截图中实际返回的 ID"})`。也支持 `coordinate_space=screen_points` 传入全局屏幕 points；两者都必须在 45 秒有效期内并处于截图范围内。滚动、`type`、`key`、`press_element` 等不依赖像素坐标的操作继续要求控制授权。可以通过 `computer_observe({"action":"ui_tree"})` 读取不包含输入值的 AX 控件树。操作后重新截图验证结果。
7. 客户端调用 `computer_session({"action":"stop"})` 或用户在本机点击“立即撤销”；若无操作，批准将在 5 分钟后过期。

当前对仅传递 `_meta` 的无状态客户端严格拒绝授权，这类客户端的安全兼容方案应在 P4 专门设计，不得自动退回信任可伪造的会话 ID。
