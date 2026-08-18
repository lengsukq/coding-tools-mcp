# Design

## Runtime

新增 `ServiceUsage` 原子计数器，挂载到 MCP/Actions listener 的 `ToolContext` 或 listener state。请求完成后只提交输入/输出 JSON 的字节数、是否为工具调用和是否为错误。

估算规则为 `ceil(JSON UTF-8 bytes / 4)`，该值仅用于服务侧趋势观察，界面明确标注“估算 token”。

`RuntimeSupervisor` 为每个 `(workspace_id, service_kind)` 保留一个共享计数器，启动 listener 时注入；停止或重启只替换 listener，不清除当前应用会话累计值。

## IPC and UI

新增 `get_service_usage_stats` Tauri command，返回每个 workspace 的 MCP/Actions 快照。Dashboard 定时刷新并汇总 token、请求和输入/输出统计；前端保留最近 24 个采样点，渲染横跨两列的累计 token SVG 折线图，并在左侧提供 sticky 锚点导航。没有采样数据时保持空的 0 基线，不生成虚假数据。

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml usage`
- `npm run check`
