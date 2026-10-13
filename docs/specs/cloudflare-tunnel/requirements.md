# 需求文档：cloudflare-tunnel-force-restart

## 功能概述

在 Gateway 设置中增加独立的 Cloudflare Quick Tunnel 强制重启操作。操作停止当前受管理的公网隧道进程并重新创建隧道，取得新地址后保存和展示新 URL；整个过程不重启本地 Global MCP Runtime。

## 范围边界

**In Scope**

- 仅允许在公网连接已启用且连接方式为 Cloudflare Quick 时执行。
- 新增 Tauri 命令及前端 API，串行化对受管理 tunnel runtime 的重启。
- 新隧道成功建立后保存新 Quick URL，并刷新状态和健康检查。
- 重启失败时返回明确错误，不将网关状态伪报为运行中。

**Out of Scope**

- Cloudflare Named Tunnel、FRP、外部反向代理。
- 重启或停止本地 MCP Runtime。
- Cloudflare 账户、DNS 或隧道配置管理。

## 需求列表

### FR-1：可用性和配置校验

**优先级：** Must

#### 验收标准

1. WHEN 公网连接已启用且方式为 Cloudflare Quick THEN 设置页 SHALL 提供“强制重启 Cloudflare Tunnel”操作。
2. IF 公网连接关闭、方式不是 Cloudflare 或模式为 Named THEN 后端 SHALL 拒绝强制重启并给出原因。

### FR-2：强制重建隧道并更新地址

**优先级：** Must

#### 验收标准

1. WHEN 用户执行强制重启 THEN 系统 SHALL 停止当前托管的隧道进程并创建新的 Cloudflare Quick Tunnel，即使现有配置与 runtime 一致。
2. WHEN 新隧道成功就绪 THEN 系统 SHALL 保存并返回新 Quick URL，前端 SHALL 展示该 URL 并刷新健康状态。
3. IF 新隧道启动失败或未能及时就绪 THEN 系统 SHALL 回收刚启动的 cloudflared 子进程，返回错误且不报告新隧道运行中。

### FR-3：隔离本地 MCP Runtime

**优先级：** Must

#### 验收标准

1. WHEN 用户强制重启 Cloudflare Tunnel THEN 本地 Global MCP Runtime SHALL 保持原有运行状态，不调用 Runtime 启停命令。
2. 强制重启 SHALL 与常规 Gateway 启停通过同一 runtime 锁串行化。

## 非功能需求

- **NFR-1（兼容性）**：既有启动、停止、健康检查与 FRP 路径保持原有 API 行为。
- **NFR-2（可靠性）**：只有在新 tunnel 已返回 Quick URL 且 URL 已保存后，才将其登记为运行状态。

## 依赖关系

- `global_gateway::RUNTIME` 串行管理 Cloudflare/FRP 的公网 Gateway 子进程。
- `cloudflare::spawn_cloudflare_tunnel` 负责启动 Quick Tunnel 并等待 Cloudflare 分配 URL。
- `cloudflare::force_stop_child` 负责终止并等待受管 cloudflared 进程退出；启动超时清理也复用此能力。
- `DataStore` 保存新分配的 `public_url`；前端复用 Gateway 配置、概览和健康检查 API。
