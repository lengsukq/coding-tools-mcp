# 全应用设计规范

本轮保留毛玻璃风格，主要服务于查看 AI 工作、处理阻塞、追溯变更和人工验收。任务由 AI 会话发起，桌面端提供管理入口。

## 唯一视觉来源

`src/styles/design-system.css` 定义语义 token、公共布局和交互状态；`src/app.css` 只负责基础结构与字体，`src/palette.css` 只定义现有品牌色。页面 scoped 样式只处理局部布局。旧 workbench / ios-vue 样式层逐页迁移后删除。

## 层次与尺寸

- 侧栏使用半透明背景；内容面板使用 24px 模糊；浮层使用 32px 模糊。行、字段组和详情说明使用轻背景与分隔线。
- 标题 26px，区块标题 16px，正文与标准控件 14px，辅助文字 12px。路径和 ID 使用系统等宽字体。
- 间距以 4px 为基础；控件圆角 12px，面板 24px，浮层 28px；按钮默认 36px，紧凑 32px。
- 品牌色用于主要动作与选中状态；成功、警告、异常使用独立语义 token，浅色和深色分别调整对比度。
- 静态卡片没有点击动效；交互行有 hover；键盘 focus 清晰；禁用和 busy 显式；减少动态效果时关闭非必要动画。

## 内容宽度适配

内容区 ≥1000px 使用主辅列；700–999px 辅助列转为后续区块；<700px 单列。详情表格在区域内滚动。桌面应用保留现有 960px 最小窗口宽度，窄宽度通过本地 Web 测试壳验证。

## 状态口径

同一份 `ui-model` 定义任务焦点、中文状态与进度。计划标题优先，关联目标为说明。完成和跳过分别计数，进度表示已处理步骤。无步骤不显示虚假完成度。

运行中、未结束、待验收、完成分别表达。存在验证记录仅说明“已有验证记录”，需要用户结合变更与验收条件判断结果。运行状态不代表连接健康；未知、请求失败和过期数据均有文字提示。

## 公共组件

PageHeader、PageShell、GlassCard、BaseButton、TextField、SelectField、TextAreaField、ToggleSwitch、StatusPill、PageTabs、SegmentedControl、EmptyState、SaveBar、ModalDialog、ConfirmDialog。页签使用 tab 语义；执行模式使用 radio 语义；两者支持方向键、Home、End。

开发环境 `/dev/ui` 提供组件展示页；生产环境不注册该地址。完整目录见 `docs/ui/component-catalog.json`。
