# Computer Use V1 — 任务清单

Planning 中主计划 ID：`a65798ae0912497eb7f6eef50b43e97b`，完整 17 步，以 Planning 为唯一进度事实源。

## 当前交付

- [x] 梳理 Global Gateway 与 `view_image`、Tool profiles、Tauri 本机设置和 Workspace 权限边界。
- [x] 建立规格文档及基本威胁模型。
- [x] 建立首轮 macOS 截图原型：MCP image 内容、显示器序号、JPEG 尺寸限制、RAII 清理。
- [x] 建立默认拒绝的会话审批：MCP request/status/stop，本机 approve/revoke，5 分钟过期，跨会话隔离单测。
- [x] 增加 macOS CoreGraphics 截屏权限预检与本机系统权限申请按钮；子进程截图上限 12 秒，超时终止并回收。
- [x] 补充 Gateway MCP 合同测试：截图未批准时拒绝、client metadata session 不可冒充 server-issued session。
- [ ] ScreenCaptureKit 原生截图、系统权限诊断、无中间文件、多显示器与窗口/区域截图。
- [ ] 真实 macOS Tauri 与 ChatGPT 截图 E2E 验证及系统授权提示。
- [ ] 鼠标、键盘、拖拽、窗口焦点控制。
- [ ] Accessibility 控件树和语义点击。
- [ ] 用户暂停/终止、客户端绑定、审计脱敏、锁屏及隐私增强。
- [ ] 全面多平台测试与用户验收文档。

**禁止误报**：当前原型只支持有限的截图能力。系统可能因 macOS 权限/锁屏拒绝；不可声称已能控制鼠标、键盘，也不可声称所有 Computer Use 计划均完成。
