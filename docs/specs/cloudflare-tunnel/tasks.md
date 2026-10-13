# 任务清单：cloudflare-tunnel-force-restart

## 概述

为 Global MCP Gateway 设置页添加不重启本地服务的 Cloudflare Quick Tunnel 强制重启操作。

## 交付物清单（Scope-lock）

- **预计新建业务源码文件数**：0。
- **预计新增规格文档数**：3。
- **预计修改业务源码文件数**：7。
- **交付物逐项列举**：
  1. `src-tauri/src/global_gateway.rs`：配置校验、强制停止和重建 Cloudflare Quick runtime。
  2. `src-tauri/src/tunnel/cloudflare.rs`：停止确认与启动失败时清理 cloudflared 子进程。
  3. `src-tauri/src/commands/global_gateway.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`：暴露 Tauri 命令。
  4. `src/lib/api/global-gateway.ts`：新增前端调用。
  5. `src/views/settings/GatewaySettingsView.vue`：增加 Cloudflare 专属重启操作及成功/失败反馈。

## 任务列表

### 阶段 1：准备和规格

- [x] 1.1 核对 Gateway runtime、Cloudflare 子进程、Tauri command 和设置页调用链。
  - **证据块**：`global_gateway::ensure_started` 在配置匹配且 runtime 运行时提前返回；Cloudflare 句柄由 `RUNTIME` 保存；设置页原有 start/stop 会操作整个 MCP Runtime。
  - **涉及文件**：`src-tauri/src/global_gateway.rs`、`src-tauri/src/tunnel/cloudflare.rs`、`src-tauri/src/commands/global_gateway.rs`、`src/views/settings/GatewaySettingsView.vue`。
- [x] 1.2 完成规格检查和工作量估算。
  - **证据块**：规格由 `check_spec` 检查 requirements/design/tasks 的覆盖与完整性；Plan `estimate` 使用本清单中的七个业务源码文件估算。
  - **涉及文件**：`docs/specs/cloudflare-tunnel/requirements.md`（约 55 行）、`docs/specs/cloudflare-tunnel/design.md`（约 75 行）、`docs/specs/cloudflare-tunnel/tasks.md`（约 90 行）。
  - **估算结果**：5 个故事点；乐观 5 小时、正常 8 小时、悲观 12 小时。包含启动超时进程清理；假设本地 Rust/前端依赖已安装，实际 Cloudflare 公网验证单独执行。
  - **需求**：FR-1 至 FR-3。

### 阶段 2：核心实现

- [x] 2.1 增加强制 Cloudflare Quick 重启函数和后端配置门禁。
  - **验收**：绕过常规同配置短路；取得新 URL 并保存后才登记 runtime；启动超时、URL 解析失败或 URL 保存失败时回收新子进程并返回错误。
  - **证据块**：`src-tauri/src/global_gateway.rs:57-68` 先检查同配置运行态再提前返回，且后续通过 `start_tunnel` 启动；`src-tauri/src/tunnel/cloudflare.rs:345-351,460-478` 清理启动失败与确认强制停止。
  - **涉及文件**：`src-tauri/src/global_gateway.rs`（约 55 行）、`src-tauri/src/tunnel/cloudflare.rs`（约 30 行变更预算）。
  - **需求**：FR-1、FR-2、FR-3、NFR-2。
- [x] 2.2 暴露 Tauri 命令和 TypeScript API。
  - **验收**：前端命令名称与 Tauri 注册一致，返回现有状态 DTO。
  - **证据块**：`src-tauri/src/commands/global_gateway.rs:26-42` 定义 start/stop/status/health 命令；`src-tauri/src/commands/mod.rs:25-28` 导出命令；`src-tauri/src/lib.rs:239-244` 注册命令；`src/lib/api/global-gateway.ts:42-43` 调用配置命令。
  - **涉及文件**：`src-tauri/src/commands/global_gateway.rs`（约 5 行）、`src-tauri/src/commands/mod.rs`（约 1 行）、`src-tauri/src/lib.rs`（约 1 行）、`src/lib/api/global-gateway.ts`（约 1 行）。
  - **需求**：FR-2、NFR-1。
- [x] 2.3 在 Gateway 设置页加入 Cloudflare 专属重启按钮。
  - **验收**：只在启用的 Cloudflare Quick 配置显示；保存编辑值、调用隧道重启、刷新 URL/健康状态；不调用 MCP Runtime 启停 API。
  - **证据块**：`src/views/settings/GatewaySettingsView.vue:98-111` 的现有 start/stop handlers 调用整个 Runtime；`114-118` 复用健康检查并刷新 MCP 概览；`150` 已有 Cloudflare 配置提示。
  - **涉及文件**：`src/views/settings/GatewaySettingsView.vue`（约 25 行变更预算）。
  - **需求**：FR-1、FR-2、FR-3。

### 阶段 3：验证和审查

- [x] 3.1 运行用户允许的非构建 Rust 与前端静态检查并记录未执行项。
  - **证据块**：`src-tauri/src/global_gateway.rs` 包含模块级 Rust 测试；项目根目录 `package.json` 定义前端类型检查和构建脚本。
  - **涉及文件**：`src-tauri/src/global_gateway.rs`、`package.json`、`src-tauri/Cargo.toml`（仅执行验证，不改动）。
  - **结果**：`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`、`npm run check`、`git diff --check` 均通过。按用户要求未运行构建、Rust 测试编译或服务重启。
- [x] 3.2 检查 GitNexus 变更影响和最终代码审查，确认范围未扩展到 FRP 或 MCP Runtime 生命周期。
  - **证据块**：GitNexus 当前索引对应本次起始 HEAD；实现前已检查 `ensure_started`、`start_tunnel`、`stop_runtime`、`stop_child` 和 `spawn_cloudflare_tunnel` 上游影响。
  - **涉及文件**：七个业务源码文件及 `docs/specs/cloudflare-tunnel/` 三个规格文件（仅审查，不扩展实现范围）。
  - **审查结果**：Tauri command/API 是新增契约，现有 DTO、命令和数据格式未变；手工核对命令注册、消费者和失败清理后无待修复语义问题。GitNexus 变更图为 HIGH，已逐项核实其受影响的注册与启动流；普通 stop 和 FRP 调用仍复用原有尽力清理路径。

## 检查点

- [x] 阶段 1.1 完成后：确认 tunnel runtime 与本地 MCP Runtime 的所有权边界。
- [x] 阶段 2 完成后：确认配置门禁、runtime 锁、新 URL 持久化和失败清理均符合规格。
- [x] 阶段 3 完成后：静态检查和审查证据已记录；构建/测试编译及实际 Cloudflare 公网运行未执行。

## 需求覆盖矩阵

| 需求 ID | 设计章节 | 任务编号 |
|---|---|---|
| FR-1 | 后端流程、前端流程 | 2.1、2.3 |
| FR-2 | 后端流程、API 设计、前端流程 | 2.1、2.2、2.3、3.1 |
| FR-3 | 决策 1、前端流程 | 2.1、2.3 |
| NFR-1 | API 设计 | 2.2、3.2 |
| NFR-2 | 决策 2 | 2.1、3.1 |

## 文件变更清单

| 文件 | 操作 | 说明 |
|---|---|---|
| `src-tauri/src/global_gateway.rs` | 修改 | 校验并强制重建 Quick Tunnel runtime |
| `src-tauri/src/tunnel/cloudflare.rs` | 修改 | 停止确认并清理启动未就绪的子进程 |
| `src-tauri/src/commands/global_gateway.rs` | 修改 | 新增 Tauri command |
| `src-tauri/src/commands/mod.rs` | 修改 | 导出 Tauri command |
| `src-tauri/src/lib.rs` | 修改 | 注册 Tauri command |
| `src/lib/api/global-gateway.ts` | 修改 | 新增前端 API |
| `src/views/settings/GatewaySettingsView.vue` | 修改 | 提供操作按钮并刷新状态 |
