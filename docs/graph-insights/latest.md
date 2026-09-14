# 项目图谱洞察

更新时间：2026-09-14

## 分析状态

- GitNexus 索引已刷新：5,338 个节点、10,793 条边、300 条执行流。
- 当前索引对本仓库 Rust 符号的查询仍不稳定，`start_runtime`、`RuntimeSupervisor`、`call_tool` 等目标未能可靠解析。
- 以下结构结论以当前源码和项目文档为准，GitNexus 仅作为辅助索引。

## 项目定位

这是一个 Rust + Tauri 2 + Svelte 的桌面客户端，将 Coding Tools MCP 能力以内嵌 HTTP 服务形式暴露。每个工作区独立运行 MCP 服务，并可配合 Global Gateway、FRP 或 Cloudflare 隧道提供公网入口。

## 主执行链路

```text
Svelte 页面
  → src/lib/api/* 的 Tauri invoke
  → src-tauri/src/commands/*
  → AppState
      ├─ DataStore：工作区、设置和密钥数据
      └─ RuntimeSupervisor：MCP 生命周期
            └─ mcp::spawn_listener：/mcp
  → tunnel supervisor：FRP / Cloudflare 公网隧道
```

## 核心模块

### 应用入口与状态

- `src-tauri/src/lib.rs` 注册 Tauri 插件、`AppState` 和全部 IPC commands。
- `src-tauri/src/app_state.rs` 以两个 `Mutex` 管理 `DataStore` 与 `RuntimeSupervisor`。
- `src-tauri/src/commands/mod.rs` 聚合工作区、运行时、隧道、密钥、软件和设置命令。

### 数据与工作区

- `src-tauri/src/data/store.rs` 负责统一数据文件的读取、迁移、保存及工作区 CRUD。
- `src-tauri/src/data/model.rs` 的 `AppData` 聚合 FRP 配置、代理、下载配置、工作区和 secrets。
- `src-tauri/src/workspace/` 定义工作区模型、旧版导入和兼容层。

### 运行时

- `src-tauri/src/runtime/supervisor.rs` 管理每个 Workspace 的 MCP 状态。
- 生命周期为 `Stopped → Starting → Running/ Error → Stopping`。
- `src-tauri/src/commands/runtime.rs` 负责端口占用检查、启动/停止、隧道联动及公网 URL 回写。
- `src-tauri/src/runtime/port.rs` 和 `src-tauri/src/platform/windows/net.rs` 提供端口与进程检测。

### HTTP 服务

- `src-tauri/src/mcp/listener.rs` 启动 MCP Streamable HTTP 服务，并接入 Bearer/OAuth/无认证。
- MCP listener 复用 `src-tauri/src/tools/` 的统一工具内核和策略配置。

### 隧道

- `src-tauri/src/tunnel/supervisor.rs` 统一管理隧道生命周期。
- `src-tauri/src/tunnel/frp/` 负责 FRP 配置与客户端。
- `src-tauri/src/tunnel/cloudflare.rs` 负责 cloudflared 进程和公网 URL 处理。

### 前端

- `src/routes/+layout.svelte` 加载工作区、刷新 MCP 状态并承载全局导航和 Toast。
- `src/routes/workspace/[id]/+page.svelte` 是核心工作区页面，管理 MCP、认证、策略、隧道、日志和健康检查。
- `src/lib/api/` 封装 Tauri IPC；`src/lib/components/` 提供配置表单和状态面板；`src/lib/stores/` 管理前端共享状态。

## 当前工作区观察

- 当前产品运行时已经收敛为 MCP-only：Actions listener、OpenAPI 网关、双服务状态、Actions 隧道/认证/日志/健康与前端入口均已删除。
- 旧 `profiles.json` 中的 `actions` 配置、`restore_actions_workspace_ids` 与 Actions 专属密钥会在升级加载时被定向清理；MCP 密钥保持不变。
- `docs/specs/**`、带日期的 `docs/verification/**` 与 `docs/history-session/**` 仍保留历史 Actions 记录，不代表当前产品能力；旧 Python/Actions 参考实现目录已从当前仓库移除。
- 当前 README、project-context 与本文件已经同步 MCP-only 架构。

## 验证结果

- `npm run version:check`：通过，项目版本 `0.2.3` 一致。
- `npm run check`：通过，0 错误、0 警告。
- `npm run build`：通过，SvelteKit/Vite 生产构建成功。
- `npm run test:release`：通过；Rust library 164/164，Tool Contract 22/22，Security 24/24，Harness 4/4 + 11/11，History 20/20，Workspace E2E 7/7。
- 额外 `cargo clippy --all-targets -- -D warnings` 仍会被仓库既有的全局 lint 债务阻塞，主要分布于 Agent Context、Planning、MCP Server 等与 Actions 删除无关的模块。
- `cargo fmt --all -- --check` 同样揭示仓库既有的大范围格式差异；本次未执行全局自动格式化，以免覆盖当前未提交的其他 UI/代码改动。

## 建议优先级

1. 单独安排一次 Rust lint/format 基线整理，不与功能删除任务混在一起。
2. 继续保持 MCP-only 产品边界，避免重新引入第二套 transport/runtime 状态机。
3. 持续补充 MCP/tunnel 的运行时集成测试，尤其是端口冲突、停止等待、OAuth 回调和隧道自动启动失败场景。

---
*来源：当前源码、README、项目上下文文档、Git 状态与构建验证；GitNexus 索引作为辅助。*
