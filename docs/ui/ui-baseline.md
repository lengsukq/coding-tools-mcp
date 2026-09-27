# UI Baseline — 2026-09-26

## 当前问题

- `ios-*` 与 `wb-*` 两套页面语言并存，Vue 模板中两者都约有 194 处引用。
- `BaseButton` 已广泛使用，但仍存在约 53 个原生 `<button>`；Dashboard 额外维护 `wb-primary-button / wb-soft-button / wb-command-button`。
- `GlassCard` 是全站高复用 Surface，但大量静态卡片使用 `hoverable`，产生“都可点击”的错误 affordance。
- Vue 页面约 70 处硬编码 HEX、120 处任意 `text-[xpx]`，设计 token 未真正成为唯一来源。
- `app.css` 内 `.page-header` 存在多轮覆盖定义，最终样式依赖 CSS 顺序。
- Dashboard 的 Analytics/KPI 在首屏视觉重量上压过 Needs Attention；Workspace Overview 同时强调过多状态与统计。
- 页面容器宽度不统一：Settings / Audit / Dashboard / Workspace 各自维护不同 max-width。

## 必须保留的业务能力

- Dashboard：当前执行、Needs Attention、Workspace 列表、Runtime/Health、Activity、Token Analytics、Command Palette、偏好设置。
- Workspace：Header、Overview、Services、Planning、Settings、Review/Diff、History、Git、IDE 打开。
- Tool Audit：健康状态、筛选、分页、详情、Retention、清理记录。
- Settings：General、Keys、Global MCP、FRP、Software。

## 迁移原则

1. 共享组件先行，页面随后迁移。
2. 保持组件 API 尽量兼容，降低高复用组件变更风险。
3. L1/L2 信息必须先于 L3/L4。
4. 静态 Surface 不使用 lift hover。
5. Legacy CSS 只做兼容，不再新增规则；新实现统一进入 `styles/design-system.css`。

