# Computer Use V1 — 任务清单

Planning 中主计划 ID：`a65798ae0912497eb7f6eef50b43e97b`，完整 17 步，以 Planning 为唯一进度事实源。

## 当前交付

### 主动授权模式（2026-10-13）

- [x] 优化浏览器导航：本机授权浏览器新增单次 `open_url` 动作（仅 HTTP(S)，限制协议、地址长度和目标 Bundle ID）；比逐次模拟地址栏按键减少往返。
- [x] 优化截图传输：默认 fast（1280×850 上限、JPEG 0.56），需要精细文字时选择 detailed（1920×1200 上限）；保留与截图像素对应的精确坐标转换。
- [x] 改进 `type`：同一个 MCP 请求内按 Unicode 组合字符逐项发送原生键盘事件，保持密码字段拒绝、日志内容脱敏及系统剪贴板不变。
- [ ] 在安装最新签名 DMG 后进行真实 Edge 的 `open_url` 和中文/英文 `type` 场景性能对比；目前仅单元测试和构建检查完成。

- [x] 增加本机可配置的观察/控制权限和 5/15/60 分钟、8/24 小时、永久时限；永久授权必须二次确认。
- [x] 授权策略持久保存，支持本机紧急暂停/恢复/关闭；控制模式保留指定应用限制与高风险操作确认。
- [x] 经 OAuth/Bearer 验证的客户端可直接发出 Computer Use 调用，不依赖 ChatGPT 跨轮次持有 `session_token`。
- [x] 复用 MCP 工具审计，提供 Computer Use 专用列表，显示认证主体、时间、操作类型、结果，不保存截图或输入内容。
- [x] 修复 OAuth 主动授权下的会话身份：由 GatewaySessionStore 按认证主体生成并复用服务端签发的随机 Computer Use 会话，兼容不保留 MCP Session Header 或 session_token 的客户端；补充同客户端复用、跨客户端隔离和客户端元数据伪造回归测试。
- [x] 在真实 ChatGPT + macOS 环境通过 MCP 直接取得窗口列表与双显示器信息，未携带 session_token，返回 ok=true；对 Coding Tools MCP 应用窗口调用 ScreenCaptureKit 截图返回 ok=true（含图像尺寸和 snapshot_id 元数据）。
- [ ] 真实 Mac 上验收鼠标键盘控制、敏感动作审批、持续授权重启、到期撤销和审计日志显示。

- [x] 梳理 Global Gateway 与 `view_image`、Tool profiles、Tauri 本机设置和 Workspace 权限边界。
- [x] 建立规格文档及基本威胁模型。
- [x] 建立首轮 macOS 截图原型：MCP image 内容、显示器序号、JPEG 尺寸限制。
- [x] 建立默认拒绝的会话审批：MCP request/status/stop，本机 approve/revoke，5 分钟过期，跨会话隔离单测。
- [x] 增加 macOS CoreGraphics 截屏权限预检与本机系统权限申请按钮。
- [x] 补充 Gateway MCP 合同测试：截图未批准时拒绝、client metadata session 不可冒充 server-issued session。
- [x] ScreenCaptureKit 原生单帧截图、无中间文件、窗口/区域和 Retina 全局屏幕坐标元数据（编译通过，尚待实际多屏验证）。
- [x] 真实 macOS Tauri 与 ChatGPT 窗口列表和单窗口截图 E2E 验证；多显示器捕获、系统权限撤销及控制动作仍待单独验收。
- [x] CGEvent 基础鼠标/键盘/拖拽/滚动/快捷键、批准目标应用激活（代码已实现，真实桌面效果尚待验收）。
- [x] Accessibility 控件树和按 label/role 唯一匹配的语义点击（代码已实现，真实桌面效果尚待验收）。
- [x] 独立 observe/control 授权，控制对象由本机用户选择运行中的应用；每次授权最多 100 次动作，5 分钟到期，支持立即撤销。
- [x] 截图绑定不可复用的唯一授权租约，已撤销/替换的截图和 AX 观察返回前被拒绝；本机 revoke 和远程 stop 与控制动作串行化。
- [x] 在本机 Gateway 设置加入“紧急暂停/恢复”，暂停后拒绝新截图和输入、清除已有截图定位 ID；远程 MCP 不可自行恢复。
- [x] 鼠标点击/拖拽/移动要求同一会话 45 秒有效截图 ID，支持 image_pixels → macOS screen_points 自动转换；已为 Retina 非整数缩放、负原点、越界与拖拽增加纯单元测试。
- [x] macOS 键盘输入对密码或 Accessibility 不可识别的焦点控件采用拒绝策略，`type` 和 `key` 均禁止向其发送事件。
- [x] MCP HTTP 实际路由测试覆盖三工具发现、默认拒绝截图和点击、伪造客户端 metadata 会话拒绝。
- [x] 为不保留 MCP Header 的 ChatGPT 客户端增加显式 `session_token`：服务端随机签发、仅存哈希、绑定 OAuth 身份、短期失效、`stop` 撤销；仍需 Mac 本机审批，伪造 `_meta` 不可直接获取权限。
- [x] 为无状态会话的申请 / 查询 / 默认拒绝 / 停止 / 撤销，以及跨身份、超时凭证补充回归测试。
- [ ] 用户暂停/终止、客户端绑定、审计脱敏、锁屏及隐私增强。
- [ ] 全面多平台测试与用户验收文档。

**禁止误报**：当前已接入截图、控制和 AX 原生接口，但仍缺真实 Mac 的系统权限、坐标、鼠标键盘、锁屏场景 E2E；用户未实际验证前不得声称完整 Computer Use 正式可用。
