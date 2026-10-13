# 设计文档：cloudflare-tunnel-force-restart

## 概述

增加 `restart_global_cloudflare_tunnel` Tauri 命令，复用 `global_gateway::RUNTIME` 锁、Cloudflare Quick 启动器和现有受管子进程停止流程。该命令不调用 `startRuntime` 或 `stopRuntime`，因此本地 MCP Runtime 生命周期不受影响。

**对应需求：** FR-1、FR-2、FR-3、NFR-1、NFR-2。

## 技术方案

### 后端流程

1. 读取最新 `AppSettings` 和 Global Gateway 配置。
2. 校验公网连接已启用、`tunnel_type` 为 `cloudflare`，且 `cloudflare_mode` 为 `quick`。
3. 获取 `RUNTIME` 锁，移除并停止现有托管 tunnel runtime。此流程不采用 `ensure_started` 的“配置相同且运行中则直接返回”快捷路径。
4. 启动 Cloudflare Quick Tunnel 并等待现有启动器取得 `trycloudflare.com` URL。
5. 持久化新 URL；持久化失败时停止刚创建的 tunnel 并返回错误。
6. 仅在启动和持久化都成功后登记新 `GatewayRuntime` 并返回状态。

启动器在子进程创建后若日志目录准备、Quick URL 等待或 URL 解析失败，必须先停止并等待该子进程退出，再返回启动错误。`force_stop_child` 的停止超时或进程树终止失败应回传错误，阻止后续 tunnel 被误报为已替换。常规 Cloudflare 启停仍沿用既有尽力清理行为。

### API 设计

新增 `restart_global_cloudflare_tunnel` Tauri 命令和对应前端 API。返回值复用 `GlobalGatewayStatusDto`。不改变现有 start/stop/status/health 命令。

### 前端流程

- 仅在已启用 Cloudflare Quick 配置时显示独立重启按钮。
- 操作前保存当前编辑的配置，随后调用新命令。
- 成功后刷新配置、Global MCP 概览和健康检查，使新 Quick URL 立即可见；失败时展示错误提示。
- 重启 handler 不调用本地 MCP Runtime 的 start/stop API。

## 文件结构

```text
src-tauri/src/global_gateway.rs                 强制重启与配置验证
src-tauri/src/tunnel/cloudflare.rs               可靠停止与启动失败清理
src-tauri/src/commands/global_gateway.rs        注册 Tauri 命令
src-tauri/src/commands/mod.rs                  导出命令
src-tauri/src/lib.rs                            注入 invoke handler
src/lib/api/global-gateway.ts                  暴露前端 API
src/views/settings/GatewaySettingsView.vue      增加独立操作和反馈
```

## 设计决策

### 决策 1：独立重启命令，不复用 MCP Runtime 重启（FR-3）

**决策：** 命令直接串行化并重建 Gateway tunnel runtime。

**理由：** tunnel 子进程由 `global_gateway::RUNTIME` 管理；本地 MCP 服务有独立生命周期。使用 Runtime 启停会扩大中断范围。

### 决策 2：先取得新 URL，再登记运行状态（FR-2、NFR-2）

**决策：** 复用 Quick Tunnel 启动器的就绪等待；URL 持久化失败时停止新子进程，且不写入新 runtime。

**理由：** 避免 UI 看到运行成功但实际没有可用地址，或遗漏无法管理的新子进程。

## 验证策略

- Rust：验证配置门禁以及 runtime 重启路径相关单元行为；运行受影响的 Rust 检查。
- 前端：运行项目已有的类型检查和构建检查，确认命令名、按钮门禁及健康刷新路径。
- 运行时：此改动不自动连接 Cloudflare 公网；实际 Cloudflare 网络可用性需通过用户环境中的 tunnel 运行确认。
