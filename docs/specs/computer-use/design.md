# Computer Use MCP — 架构设计与威胁模型

## 1. 现有组件与演进边界

当前 Global MCP `src-tauri/src/mcp/server.rs` 负责 `tools/list` 与 `tools/call`，并把 Workspace 工具路由至 `tools/dispatch`。图片文件的 `view_image` 经 `tools/tool_output.rs` 转为 MCP image 内容，但只能读取 Workspace 内已有文件。

Computer Use 属于 **Host 层**，不能伪装成 Workspace 工具。新 `src-tauri/src/computer.rs` 在 Gateway 的 Workspace 解析之前分发，输入身份取自 **服务端 MCP transport session**，不接受用户构造的 `workspace_id` 或 `_meta` 作为授予依据。现有 21 个 compact tool 不变。

## 2. 授权模式：本机主动开放（优先）与旧版逐次审批

### 低延迟 Computer Use（浏览器与截图）

- **浏览器导航**：客户端应优先调用一次 `computer_action({action:"open_url",url:"https://github.com"})`，代替 `focus_app → Command+L → type → Enter` 多次网络往返。只有本机已授权的 **control** 目标应用是受支持浏览器时才可使用；Rust 和 macOS 桥接分别校验 URL，仅接受长度受限的 HTTP(S) 地址，拒绝文件、脚本、自定义协议与嵌入凭据。目标必须是当前运行中的已授权浏览器，绝不把 URL 交给系统默认应用任意打开。返回的 `dispatched=true` 表示浏览器确认接收了导航请求，不等于远端网页已加载完；需要再按需查询窗口标题或截图验证。
- **快速截图**：`computer_observe({action:"screenshot"})` 默认 `quality:"fast"`，限制约 1280×850 像素、JPEG 压缩质量 0.56，以减少回传的图片体积和视觉处理延迟。只有需要辨认细小文字时才设置 `quality:"detailed"`（最高 1920×1200，JPEG 0.72）。截图返回的 `point_per_pixel_x/y` 始终根据**实际尺寸**计算，原有鼠标定位映射与 45 秒截图时效仍然有效。
- **文本输入**：`type` 保留单个 MCP 请求，macOS 端以 Unicode 组合字符逐个发送键盘按下/抬起事件，兼容 Chromium 类应用对单个超长合成键事件的丢弃情况；保持 AX 密码/未知焦点保护。不会读写系统剪贴板，不记录输入文字到审计日志。
- 此轮避免引入无条件的动作批处理：每项输入仍经过独立的目标应用、授权时效、锁屏与高风险确认检查；浏览器导航用专用动作完成真正的**单请求**优化。

**默认关闭（fail closed）。** 用户在「设置 → MCP 连接 → Computer Use」本机主动选择：仅查看屏幕或控制指定应用、5/15/60 分钟、8/24 小时或永久。永久模式需要额外明确确认；本机可随时暂停、恢复、关闭，暂停与关闭对已打开的授权立即生效（不会撤销已发生的系统事件）。

- 通过 OAuth 或共享 Bearer 完成 HTTP 身份验证后，服务端按**已验证的认证主体**计算稳定且不可由客户端自行指定的 `computer-auto-<sha256(principal)>` 会话身份。**免调用 `computer_session(request)`，也无需保存 `session_token` 或 MCP Session Header**，可直接调用 `computer_observe` / `computer_action`。客户端 `_meta`、`workspace_id`、自定义会话 ID 都不是授权依据。
- 新模式的授权范围、目标应用、到期时间、暂停状态保存到应用自身的 `profiles.json`；除非用户手动关闭或期限届满，**应用重启后仍生效**。永久意味着不按时间自动关闭，并不意味着可绕过系统录屏/辅助功能许可，也不取消敏感操作的本机二次确认。
- 只有 OAuth/Bearer 认证主体使用主动授权；`noauth` 模式不得因此获得不经审批的屏幕/控制权限。共享 Bearer 只可识别“共享凭据”而无法区分持有同一凭据的不同设备，建议公网接入启用 OAuth。
- 用户在本机选择 control 时必须指定一个当前运行的应用。鼠标/拖拽仍需 45 秒内对应客户端最新截图、窗口遮挡检查；明显敏感行为仍需本机逐次批准。跨客户端切换时旧截图/敏感确认立即失效。控制每 100 次动作自动轮换租约，轮换后需重新截图，避免旧快照永久有效。
- 新模式在 UI 提供 **Computer Use 审计列表**，按时间记录 `computer_*` 工具名、枚举操作类型、OAuth client_id 或共享 Bearer 类型、请求时间、成功/失败与错误类别；不记录截图字节、键盘输入文本、密码或 `session_token`。使用项目既有工具审计存储和保留策略（默认 30 天，最长 365 天）；禁止将其误认为具有不可篡改保证的安全审计服务。
- 老版 `computer_session(request)` → 本机批准 → 5 分钟仍保留作兼容回退，但**不再是主动授权模式的前提**。

### 旧版会话控制流程（兼容模式）

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

- 无有效的服务端签发身份（MCP transport session 或独立 Computer Use capability）不允许获取屏幕。
- 保留原有 `initialize` → 服务端 `mcp-session-id` Header 方式。对 ChatGPT 等**不保留 Header** 的客户端，`computer_session(request)` 可在通过原有 HTTP 认证后生成一个独立、随机的 `session_token`，仅通过 MCP 工具结果返回；客户端必须在后续 `status/stop/observe/action` 的参数中显式传入。客户端 `_meta` / `workspace_id` **不是**授权凭据，即使它们在普通 Workspace 工具中可用。
- `session_token` 由两个随机 UUIDv4 组成（约 244 位随机熵），仅在 Gateway 内存保存 SHA-256 摘要；10 分钟到期、`stop` 成功后失效、程序重启失效。凭证每次使用都必须绑定**当前已验证的 HTTP OAuth client_id**（或原有 Shared Bearer / local auth 主体），并映射到服务端签发的真实会话 ID。不得在 UI、History 或审计日志中打印或持久化该凭证。此凭证只用于标识会话，**不能跳过桌面批准**。
- HTTP 请求通过现有认证层验证后，Computer Use 将该 Session 与已验证的 OAuth client_id / 共用 Bearer 主体 / 本地无认证模式主体绑定。同一会话头不能被另一个 OAuth 客户端直接复用。共用 Bearer 和无认证模式本身不能区分多个使用者，应只在可信部署中使用；公网应优先采用 OAuth。
- 不同 session 请求并发时拒绝，不共享授权。
- 本段描述的是**旧版 5 分钟会话授权**：其内存状态不写入应用配置，重启失效；与上方新模式的持久主动授权不同。
- 本地 Tauri 的 approve/revoke 命令不通过 MCP 暴露。
- control 只能由用户从运行中的程序里明确选择一个 bundle ID；运行时验证目标仍是前台应用，`focus_app` 只能激活该应用。observe 不能调用任何操作工具。
- 本机 Tauri 用户可以随时点击“紧急暂停”或“立即撤销”。暂停保留租约但禁止新的屏幕读取和输入操作，并清除旧截图 ID；只有本机 UI 能恢复，MCP 不能越权自行恢复，恢复后必须重新截图。已发送给操作系统的事件不能回滚。
- 每次控制授权最多 100 个动作，跨 MCP 请求串行执行；密码输入框禁止通过 type 输入，明显破坏性 AX 按钮和部分危险快捷键默认禁用。这些限制**不能完全识别纯坐标点击的风险**。
- 每次授权附带唯一 lease token。屏幕/窗口/AX 观察在执行结束、返回数据之前再次检查同一 token，以拒绝撤销或更换租约期间完成的旧观察。用户在本机撤销和客户端 stop 与控制动作串行化，保证撤销完成后不启动旧动作。已发送到系统的事件不能回滚。
- 每次截图生成仅限当前 lease 的 `snapshot_id` 和坐标框（仅保存元数据，不缓存图像）；鼠标位置类动作必须在 **45 秒内**携带该 `snapshot_id`。老快照、跨会话快照、越界坐标都拒绝。不提供无限期复用截图坐标的能力。
- 本机设置页支持紧急暂停与恢复：暂停会立即拒绝新的屏幕/AX 观察与输入，并失效之前的 `snapshot_id`；恢复后的实际录屏权限由下一次 ScreenCaptureKit 请求检查，控制模式仍需辅助功能权限，恢复后必须重新截图。暂停不会延长原有 5 分钟授权。
- 鼠标坐标进一步通过 CoreGraphics 按屏幕窗口 Z 序核对，只有命中本机批准应用的无遮挡普通窗口才发送输入；跨其他应用窗口或覆盖层的拖拽直接拒绝。若系统未提供可信的窗口归属信息则拒绝，而不是降级到仅检查前台 Bundle ID。
- 不在 `structuredContent` 中复制截图 base64，避免日志和上下文重复暴露；截图仅在 MCP image `content` 中。

## 3. 截图数据流

当前实现：`src/platform/macos/computer_native.m` 的同进程 Objective-C 桥接 → `SCShareableContent` 获取显示器和窗口 → `SCScreenshotManager captureImageWithFilter:configuration:completionHandler:` → 内存 JPEG（最大 1920×1200，8MiB）→ MCP image + 元信息。没有截图文件或通用 `exec_command`。

由于 ScreenCaptureKit 的 `SCStreamConfiguration.sourceRect` 对单窗口捕获不起作用，当前明确拒绝同时指定 `window_id` 与 `region`，避免返回截图与声称的屏幕坐标不一致。区域捕获应指定 `display_id` 或 `display_index`。

映射由可单测的 `src-tauri/src/computer_geometry.rs` 完成，使用截图中 `origin_x`/`origin_y`/`point_width`/`point_height`/`pixel_width`/`pixel_height`。动作设置 `coordinate_space=image_pixels` 时，`x/y` 是图像内像素序号（0 起始）；默认 `screen_points` 则直接使用全局 macOS 逻辑坐标。两种模式都必须附带有效的 `snapshot_id`。转换后再由 CGEvent 二次核对屏幕实际边界，避免将 Retina 缩放像素误当屏幕 points。

本机设置页使用 `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess` 检测/请求系统截图授权，以及 `AXIsProcessTrusted` / `AXIsProcessTrustedWithOptions` 检测/请求辅助功能授权。系统授权申请只由本机用户手势触发。

**预检结果不作为截图调用的最终裁决。** `CGPreflightScreenCaptureAccess() == false` 现在仅显示在本机诊断栏；只要 MCP 客户端已通过原有本机显式批准（5 分钟/固定会话），就可继续向 ScreenCaptureKit 发起 `SCShareableContent` / `SCScreenshotManager` 请求。系统拒绝时会把原生错误回传给 MCP，不返回截图，不会修改 TCC 数据库或调用 `sudo`。没有本机 lease 时连窗口枚举也不得调用。控制模式仍要求 Accessibility 权限、选择目标应用；锁屏、撤销、暂停和应用窗口范围限制不变。这种“实际访问验证”不是绕过 macOS TCC，只是消除项目预检误判。

可验收路径：客户端先发 `computer_session({"action":"request","scope":"observe"})`；即使本机面板显示预检未通过，也能手动批准；客户端在同一服务器签发会话下调用 `computer_observe({"action":"screenshot"})`。如果系统许可有效，返回 JPEG；如果仍被拒，记录 `SCREEN_DISCOVERY_FAILED` / `SCREEN_CAPTURE_FAILED` 及其系统说明，再考虑稳定证书签名。不要在没有用户本机批准时自动尝试截图。

### macOS 录屏权限开关已打开，但仍显示未授权

系统设置中的「录屏与系统录音」开关，并不证明**当前正在运行的进程**已经通过 `CGPreflightScreenCaptureAccess()`。TCC 授权关联实际运行的应用签名/Bundle 身份；开发时的 `tauri dev` 可执行文件、构建目录里的 `.app`、安装到「应用程序」的 `.app`，不应仅凭相同显示名称视为同一个授权主体。

排障顺序：

1. 在 Computer Use 设置页的「查看当前运行进程的权限诊断」中检查实际 Bundle ID、应用路径、是否从 `.app` 启动，以及当前进程预检结果。正式发行版预期 Bundle ID 为 `com.codingtools.mcp.desktop`。此诊断由 Tauri 本机命令读取 `NSBundle.mainBundle` 与 `CGPreflightScreenCaptureAccess`，**不截屏、不读取其他应用窗口，也不通过 MCP 暴露**。
2. 如果系统权限开关刚被打开，必须**完全退出进程**再启动。Coding Tools MCP 有托盘常驻功能；只关闭主窗口可能仅隐藏到托盘，无法刷新进程的 TCC 授权状态。应从菜单栏托盘选择「退出」，再启动系统设置里批准的那份 `.app`。
3. 如果诊断显示从开发可执行文件启动，或实际 Bundle ID/路径与要授权的正式应用不一致，改为从「应用程序」启动同一正式应用。不要用修改显示名称的方式冒充系统许可。
4. 完全退出后重新启动，点击「重新检测」。若仍失败，检查实际应用签名、安装位置和系统设置对应条目；**不能凭系统设置开关为真就绕过 TCC 预检或强行批准 MCP 屏幕访问**。

#### 已重新授权但预检仍失败

- 打包的 macOS 应用应包含 `src-tauri/Info.plist` 中的 `NSScreenCaptureUsageDescription`。设置页「权限诊断」会检查**当前运行进程**是否真的包含该声明；如果显示「未包含」，说明还没有安装并启动修复后的构建。Apple ScreenCaptureKit 官方文档要求提供捕获用途说明。
- `CGPreflightScreenCaptureAccess() == false` **只是一个状态检测结果**，不能等同于已经尝试过 ScreenCaptureKit 截图。先让 ChatGPT 发起 `computer_session(request)`，在本机批准，再调用 `computer_observe(screenshot)` 检查是否返回真实图片或明确的 `SCREEN_CAPTURE_FAILED` 等系统错误。
- 若已证实 macOS 实际拒绝，而且该 `.app` 已固定证书签名、Bundle ID 和安装路径，可由 Mac 用户**完全退出**应用（含菜单栏常驻进程），在终端运行 `tccutil reset ScreenCapture com.codingtools.mcp.desktop`，**只重置本应用**的屏幕录制授权。之后重新启动同一已签名应用，点击「请求系统权限」，在「隐私与安全性 → 屏幕与系统音频录制」允许该应用，按提示完全退出并重启。这符合 Apple `tccutil reset SERVICE BUNDLE_ID` 的开发排障方式；不要使用 `tccutil reset All` 或直接写入 TCC 数据库。
- 修复确认标准不是「按钮开关已开启」，而是当前运行的应用通过预检，或本地批准后 ScreenCaptureKit 真实截图成功。

#### 开发安装包使用 Ad-hoc 临时签名导致 TCC 授权失效

Apple 技术说明 [TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements) 指出：Ad-hoc 签名的 Designated Requirement 与当前代码版本绑定，重建后 macOS 无法可靠地识别为同一受信应用。Apple DTS 已[确认 ScreenCaptureKit 与 CGPreflight 在每次重建后丢失授权的案例](https://developer.apple.com/forums/thread/819406)。**应用名、Bundle ID、安装路径都正确，仍可能无法使用之前勾选的录屏权限。**

- 现在的本机「权限诊断」同时显示 `signature_type`、`signing_team_id` 和签名标识：`ad_hoc`（或无证书签名）时，不要继续安装临时签名包测试录屏。这个只读信息通过 Apple Security.framework 查询当前 `.app` 签名，不执行外部命令、不枚举 TCC 数据库。
- 在 Xcode 为你的 Apple ID 建立 **Apple Development** 签名证书（免费开发账号可用于本机开发）；正式分发使用合适的 **Developer ID Application** 签名与公证流程。查看本机已经安装的有效证书：`security find-identity -v -p codesigning`。无需将证书文件或私钥上传到仓库。
- 运行 `npm run desktop:build:signed` 构建 macOS ARM64 DMG。在此 Mac 的**本地未提交** `scripts/build-macos-signed.mjs` 中已有默认 Apple Development 证书指纹，无需先导出环境变量；**此个人配置不得加入 Git 提交**。在其他机器或没有此本地修改时，需要在构建终端设置 `APPLE_SIGNING_IDENTITY="Apple Development: <证书名称> (<TEAM_ID>)"`。显式指定 `-`（Ad-hoc）时脚本会拒绝；指定证书不可用时签名构建也会失败。证书私钥不写入代码仓库。
- 在退出旧进程后安装**由同一稳定证书签名**的新 `.app`。如果旧授权记录仍显示开关已打开但新签名尚未通过预检，在 macOS「隐私与安全性 → 录屏与系统录音」中由用户手动移除旧的授权项（如果系统提供移除控件），随后在新应用点「请求系统权限」并重新允许，按提示完整退出应用后再次打开。必要时检查系统是否要求注销或重启；不要自动清空其他程序的 TCC 授权。
- 快速验证安装包签名（在**本机终端**运行，不改动系统权限）：`codesign -dv --verbose=4 "/Applications/Coding Tools MCP.app" 2>&1`。若显示 `Signature=adhoc` / `TeamIdentifier=not set`，则说明还没有完成稳定的证书签名。使用证书签名后再次重建时，尽量保持签名 Team ID、Bundle ID 与安装路径不变。

常规 `npm run desktop:build` 依然保留供无需录屏 TCC 的测试使用，**不能将其输出自动视作已经满足持久录屏授权要求**。以上操作不等同于未经用户明确选择就重置系统授权。

诊断结果的 `screenRecordingGranted` 与展示的 `preflight_granted` 使用同一次原生预检值，避免前端一个轮询周期内显示矛盾状态。不会自动调用 `tccutil reset` 或修改系统隐私设置。

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
| 过时截图引发误点 | 45 秒 snapshot TTL、租约/session 绑定、截图区域边界检查、Retina 负坐标映射单测、真实目标窗口 Z 序检查 | 更强的控件移动/界面变化检测 |
| 遥控中的意外操作 | 本机紧急暂停与立即撤销、控制动作串行化、100 次/5 分钟控制上限 | 高风险操作逐次本机确认 |
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
2. 使用支持 Streamable HTTP 的 MCP 客户端连接，发现 `computer_session`、`computer_observe`、`computer_action`。保留 Header 的客户端可继续使用原有流程；不保留 Header 的客户端按下述方式使用短期凭证。
3. 客户端调用 `computer_session({"action":"request","scope":"observe"})`，会收到待本机批准的请求编号，以及无状态模式专用的 `session_token`。该值是私密授权凭据，只在后续工具参数中使用，不应展示在普通聊天记录中。
4. 在 Coding Tools MCP → 设置 → MCP 连接 → Computer Use · 屏幕访问中批准此请求；必要时点击“请求系统权限”，在 macOS 中授予屏幕录制。
5. 无状态客户端调用 `computer_observe({"action":"windows","session_token":"<刚才返回的凭证>"})` 获得显示器/窗口 ID；再调用 `computer_observe({"action":"screenshot","display_index":1,"session_token":"<同一凭证>"})`，取得 JPEG MCP image、`snapshot_id` 和坐标转换元信息。保留 Header 的客户端可省略 `session_token`。也可传 `window_id` 或全局屏幕 points 的 `region`。
6. 若要操作，先 stop 或从本机撤销旧 observe 授权，再重新申请 `computer_session({"action":"request","scope":"control"})` 并取得**新的** `session_token`，在本机明确选择可控制应用并批准。之后通过 `computer_action({"action":"focus_app","session_token":"<新凭证>"})` 激活目标应用，再使用该控制会话执行一次 `computer_observe({"action":"screenshot","session_token":"<新凭证>","window_id":123})`。截图返回 `snapshot_id`，例如 `computer_action({"action":"click","x":220,"y":120,"coordinate_space":"image_pixels","snapshot_id":"截图中实际返回的 ID","session_token":"<新凭证>"})`。两种坐标模式都必须在 45 秒有效期内使用新的截图 ID；滚动、`type`、`key`、`press_element` 等操作继续要求控制授权，且受目标应用白名单约束。
7. 客户端调用 `computer_session({"action":"stop","session_token":"<同一凭证>"})` 或用户在本机点击“立即撤销”；也可在本机点击“紧急暂停”，恢复后必须重新截图获取新的 `snapshot_id`。若无操作，批准将在 5 分钟后过期；短期凭证自身最长可用 10 分钟。

仅传递 `_meta` 而**未申请并提供**独立 `session_token` 的请求仍会严格拒绝。无状态客户端不能通过伪造 `_meta`、已有 Workspace 选择或自填 Header 获得 Computer Use 权限。部署到 ChatGPT 时，须重新构建并完全重启运行中的 Tauri Global MCP Runtime，让新工具 Schema 生效；真实 Mac 截图和输入能力仍须本机验收。
