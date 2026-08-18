# Service Token Usage

## Goal

在全局 Dashboard 展示 MCP 与 Actions 服务当前应用会话的 token 使用统计。统计必须由服务运行时内置采集，不记录请求正文、认证密钥或其他敏感内容。

## Requirements

### R1. 服务请求统计

- MCP 服务在认证通过后统计 JSON-RPC 请求；Actions 服务在认证中间件通过后统计 Action 请求。
- 每个服务至少统计请求数、工具调用数、错误数、输入 JSON 字节数和输出 JSON 字节数。
- 统计数据按 workspace 与服务类型隔离，且并发请求下计数正确。

### R2. Token 展示

- Dashboard 汇总所有 workspace 的服务统计并展示估算 token 总数。
- 同时展示输入/输出 token 估算和请求次数，明确标注为“估算”，不宣称等同于模型供应商账单 token。
- Dashboard 提供横跨两列的累计 token 趋势图，并展示请求次数、平均 token/请求和输入/输出 token（支持 M 单位）。
- Dashboard 左侧提供快速锚点导航，可跳转到总览、指标、工作区矩阵、Token 趋势和详细统计。
- 服务未启动或尚无请求时显示 0，不影响已有运行状态、Planning 和连接方式统计。

### R3. 数据安全与生命周期

- 统计器只保存计数和大小，不保存请求/响应正文、认证 token 或正文哈希。
- 统计值在当前桌面应用会话内跨服务重启保留；应用重新启动后从 0 开始。
- Dashboard 通过受 Tauri 保护的 IPC 查询统计，不新增公开的未认证统计端点。

### R4. 验证

- Rust 单元测试覆盖计数累加、错误计数和 token 估算。
- 前端类型检查通过。
