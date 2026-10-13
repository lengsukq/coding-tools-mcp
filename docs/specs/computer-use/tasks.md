# Computer Use V1 — 任务清单

Planning 中主计划 ID：`a65798ae0912497eb7f6eef50b43e97b`，完整 17 步，以 Planning 为唯一进度事实源。

## 当前交付

- [x] 梳理 Global Gateway 与 `view_image`、Tool profiles、Tauri 本机设置和 Workspace 权限边界。
- [x] 建立规格文档及基本威胁模型。
- [x] 建立首轮 macOS 截图原型：MCP image 内容、显示器序号、JPEG 尺寸限制。
- [x] 建立默认拒绝的会话审批：MCP request/status/stop，本机 approve/revoke，5 分钟过期，跨会话隔离单测。
- [x] 增加 macOS CoreGraphics 截屏权限预检与本机系统权限申请按钮。
- [x] 补充 Gateway MCP 合同测试：截图未批准时拒绝、client metadata session 不可冒充 server-issued session。
- [x] ScreenCaptureKit 原生单帧截图、无中间文件、窗口/区域和 Retina 全局屏幕坐标元数据（编译通过，尚待实际多屏验证）。
- [ ] 真实 macOS Tauri 与 ChatGPT 截图 E2E 验证及系统授权提示。
- [x] CGEvent 基础鼠标/键盘/拖拽/滚动/快捷键、批准目标应用激活（代码已实现，真实桌面效果尚待验收）。
- [x] Accessibility 控件树和按 label/role 唯一匹配的语义点击（代码已实现，真实桌面效果尚待验收）。
- [x] 独立 observe/control 授权，控制对象由本机用户选择运行中的应用；每次授权最多 100 次动作，5 分钟到期，支持立即撤销。
- [ ] 用户暂停/终止、客户端绑定、审计脱敏、锁屏及隐私增强。
- [ ] 全面多平台测试与用户验收文档。

**禁止误报**：当前已接入截图、控制和 AX 原生接口，但仍缺真实 Mac 的系统权限、坐标、鼠标键盘、锁屏场景 E2E；用户未实际验证前不得声称完整 Computer Use 正式可用。
