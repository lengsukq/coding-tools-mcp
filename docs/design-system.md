# 设计系统：Coding Tools MCP Desktop

> 当前实现基线：Workbench + Glass Surface。以信息层级、状态可信和低干扰交互为核心。

## 设计方向

**风格名称**: Workbench Glass（工作台毛玻璃）

**参考方向**: 桌面开发工具、Raycast / Linear 的信息密度，以及 macOS 的半透明层级感。

**核心感受**: 专业、安静、可信。页面首先回答「正在做什么、做到哪里、是否需要处理」，技术统计属于第二层信息。

## 与旧版 PySide6 的对比

| 维度 | 旧版 | 新版 |
|------|------|------|
| 布局 | 左右分栏 + 表单堆叠 | Workspace 卡片 + 详情画布 |
| 色彩 | 浅灰底 + 黑色按钮 | 跟随系统主题 + 蓝色主强调 + 受控状态色 |
| 状态 | 文字描述 | 脉冲状态灯 + 颜色编码 |
| 字体 | Segoe UI 系统默认 | 系统 UI 字体栈 + 系统等宽字体 |
| 信息密度 | 高（四宫格表单） | 低（渐进披露，主操作突出） |

## 色彩策略

- **默认跟随系统主题**，同时支持浅色与深色
- **蓝色为主强调色**，用于主 CTA、Focus 和选中态
- Indigo / Purple / Cyan 只作为少量功能识别色或弱渐变，不承担状态语义
- **状态色独立**：运行绿、启动黄、停止灰、错误红
- **禁止**：用装饰色表达成功/警告/错误；同屏大量不同强调色竞争注意力

## 字体

| 用途 | 字体 | 说明 |
|------|------|------|
| UI 正文/标题 | 系统 UI 字体 | macOS / Windows 原生观感，避免额外字体资源 |
| 代码/Endpoint | 系统等宽字体 | 路径、URL、日志、revision |

## Surface 层级

1. **Canvas**：页面背景，只承载弱渐变/环境色。
2. **Section**：通过留白、标题和轻分隔组织内容，不强制套 Card。
3. **Card**：需要明确边界的功能块使用 GlassCard；同一区域避免层层套 Card。
4. **Floating**：Modal、Popover、Command Palette 使用更强 blur / shadow，与普通 Card 拉开层级。

圆角允许 `12–24px`，由层级决定；不要为了“无界”删除所有 Card，也不要让所有内容都变成同等重量的毛玻璃块。

## 交互原则

- 保存成功只能在真实持久化完成后反馈；失败时必须保留 dirty 状态。
- 自动刷新只在页面可见时运行，并以 revision / 状态变化控制重渲染。
- 切换 Workspace 或路由参数不能展示上一个 Workspace 的残留数据。
- Modal 必须支持 Escape、焦点进入/恢复和焦点约束；分段选择支持方向键。
- 所有动画遵守 `prefers-reduced-motion`。

## 核心 Token

详见 [`design-system.json`](./design-system.json)

## 文档索引

| 文档 | 内容 |
|------|------|
| [设计原则](./design-guidelines/01-principles.md) | 价值观与决策指导 |
| [交互规范](./design-guidelines/02-interaction.md) | 八态、动效、反馈 |
| [布局规范](./design-guidelines/03-layout.md) | 页面结构、栅格、组件层级 |
| [技术配置](./design-guidelines/04-config.md) | Tailwind + Vue 实现 |
| [UI 规格](../specs/rust-desktop-client/ui-design.md) | 页面线框与组件清单 |

## 交付检查清单

- [ ] 使用系统字体，不打包额外 UI 字体
- [ ] 主强调色保持蓝色，装饰色不过度竞争
- [ ] Card / Floating 层级清晰，无无意义嵌套
- [ ] 交互元素八态齐全
- [ ] 对比度正文 ≥ 4.5:1
- [ ] prefers-reduced-motion 已处理
- [ ] Modal / Segmented / Toast 具备必要 ARIA 与键盘能力
- [ ] 成功反馈与真实后端状态一致
- [ ] 图标使用 Lucide SVG，不用 emoji
- [ ] 中文界面文案自然，无「赋能」「一站式」

---
*当前实现基线更新: 2026-09-16*
