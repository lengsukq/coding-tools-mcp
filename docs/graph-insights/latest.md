# 项目图谱洞察

更新时间：2026-10-07。当前 UI 优化分支，业务 Runtime 保留现有 MCP-only 架构。

## 索引与覆盖范围

本机已有 GitNexus CLI 刷新索引：6,465 节点、15,068 关系、250 集群、494 执行流。索引构建明确报告 750 候选入口未进入执行流、11 遍历预算截断、319 下游分支未展开，1,109 个跨语言字段站点未关联；因此索引不覆盖全部运行路径，空结果不代表未使用。

编辑前关键缓存 HIGH 已报告并独立提交。所有批次提交前执行完整 structured detect_changes；最终 P5 为 MEDIUM，基线比较为 CRITICAL。检测结果没有 partial/truncated 标记，但不能消除上述索引级限制。UNKNOWN 页面、路由、Vue 模板与跨语言关系通过源码额外核验。

## 当前主链路

Vue 路由 / 页面 → ui-model 展示模型与 workspace-status-cache 共享资源 → api Tauri invoke → Rust commands / AppState → DataStore / RuntimeSupervisor → MCP gateway / unified tools。

前端缓存是可失效投影，业务持久化仍由 Rust 持有。queued-writes 模块队列避免设置组互相覆盖；unsaved-changes 管理界面草稿。页面归属六个工作区页和五类应用设置，保留旧地址映射。

## 回归证据

14 项前端回归、304 项 Rust 测试、类型检查/生产构建、fmt/Clippy 通过。浏览器隔离 IPC 检查通过主要交互与主题/宽度，真实 Tauri 交互因锁屏未验收。完整风险和检查边界见 [图谱报告](../ui/verification/graph-review.json)、[审查](../ui/verification/architecture-review.md)、[验收记录](../ui/verification/acceptance.md)。
