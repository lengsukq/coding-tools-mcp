# Computer Use MCP — 需求与验收标准

## 定位

给 Coding Tools MCP 新增独立于 Workspace 的电脑交互能力：AI 通过统一 Global MCP 观察真实 macOS 屏幕，再根据用户许可执行鼠标/键盘操作。首版仅支持 macOS；Windows/Linux 保留后续扩展接口。

## R1：屏幕观察（P1）

- **R1.1**：屏幕图像必须以 MCP 标准 `content[].type=image` 返回，而不只是文件路径或包含 base64 的文本。
- **R1.2**：支持显示器、窗口、区域截图，并提供原始屏幕坐标、图片尺寸、缩放映射、时间、`snapshot_id`。
- **R1.3**：支持 Retina、多显示器负坐标、图片压缩上限、取消/超时；未解锁桌面、屏幕录制未授权时提供明确错误。
- **R1.4**：截图内容不得写入审计日志或 History。避免长期持久化；临时文件必须在操作结束后清除。

## R2：电脑操作（P2）

- 点击/双击/右键、移动/拖拽/滚动、文本输入、组合快捷键和窗口聚焦。
- 事件必须有动作边界、超时、取消和安全的修饰键释放；不能把窗口缩放误认为固定坐标。
- 操作完成后通过重新观察校验结果，失败不应无限循环操作。

## R3：语义控件（P3）

- 提供受限 Accessibility 元素树（role/name/frame/actions）；优先按语义操作，视觉坐标为回退路径。
- 密码框、认证验证码等敏感字段不允许返回明文，元素数量、深度和文本字节有上限。

## R4：安全（P0/P4）

- Computer Use 作为 **Host/Device Scope**，绝不自动继承 Workspace 的 `trusted`/`dangerous`、`exec_command`、`workspace_invoke` 权限。
- 默认拒绝。用户应在本机桌面程序中批准一段有期限的控制会话，可随时撤销。会话绑定服务端建立的 MCP transport session；不同会话不能复用批准记录。
- 屏幕录制、辅助功能遵循 macOS TCC 系统授权；拒绝、过期、锁屏和权限撤销须 fail-closed。
- 敏感操作（删除、发送、支付等）需独立确认；审计中只记录脱敏的行为类别。

## R5：不破坏已有功能

- 保留原有 21 个 compact Workspace tools、Global Gateway、OAuth、Cloudflare/FRP、Git/File/History/Planning。
- Computer Tools 不要求指定 workspace_id，且不应允许工作区工具假冒 Computer 权限。
- Rust 安全/合同测试、前端类型检查、真实 macOS 手工 E2E 均须通过后才能声明正式完成。

## 当前完成度（2026-10-13，第二轮）

已实现 ScreenCaptureKit/ObjC 同进程截图（屏幕/窗口/区域及坐标元数据）、独立 observe/control 授权、应用白名单单选、AX 元素树和 CGEvent 基础交互。新增 `computer_action`。源码构建与单测通过后也**不得**直接宣称电脑控制正式可用：需真实 Mac 用户授予系统屏幕录制/辅助功能权限、验证窗口/Retina 坐标/鼠标键盘和 ChatGPT 图片回包，再核对 OAuth 客户端与 TCC 生命周期边界。
