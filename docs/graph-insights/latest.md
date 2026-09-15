# 项目图谱洞察

更新时间：2026-09-15

## 分析状态

- GitNexus 索引已刷新：5,338 个节点、10,793 条边、300 条执行流。
- 当前索引对本仓库 Rust 符号的查询仍不稳定，`start_runtime`、`RuntimeSupervisor`、`call_tool` 等目标未能可靠解析。
- 以下结构结论以当前源码和项目文档为准，GitNexus 仅作为辅助索引。

## 项目定位

这是一个 Rust + Tauri 2 + Vue 3 的桌面客户端，前端使用 Vue Router 4、Tailwind CSS 4 与 Vite 6，将 Coding Tools MCP 能力以内嵌 HTTP 服务形式暴露。应用只运行一个 Global MCP 服务，多个工作区通过 session-scoped workspace selection 隔离，并可配合 FRP 或 Cloudflare 隧道提供公网入口。

## 主执行链路

```text
Vue 页面 / Vue Router
  → src/lib/api/* 的 Tauri invoke
  → src-tauri/src/commands/*
  → AppState
      ├─ DataStore：工作区、设置和密钥数据
      └─ RuntimeSupervisor：唯一 Global MCP 生命周期
            ├─ GatewayState：registry / session / request context
            └─ mcp::spawn_gateway_listener：/mcp
  → global gateway：FRP / Cloudflare 公网隧道
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

- `src-tauri/src/runtime/supervisor.rs` 管理应用级唯一 Global MCP 状态。
- 生命周期为 `Stopped → Starting → Running/ Error → Stopping`。
- `src-tauri/src/commands/runtime.rs` 负责端口占用检查、启动/停止、隧道联动及公网 URL 回写。
- `src-tauri/src/runtime/port.rs` 和 `src-tauri/src/platform/windows/net.rs` 提供端口与进程检测。

### HTTP 服务

- `src-tauri/src/mcp/listener.rs` 启动唯一 `/mcp` Streamable HTTP 服务，并接入 Bearer/OAuth/无认证。
- `src-tauri/src/mcp/gateway_state.rs` 维护动态 Workspace Registry、服务端签发的 session、session 选择和请求上下文快照。
- `src-tauri/src/mcp/gateway_handlers.rs` 提供 `workspace_list/current/select/invoke` 网关工具，并复用 `src-tauri/src/tools/` 的统一工具内核和策略配置。

### 隧道

- `src-tauri/src/global_gateway.rs` 统一管理唯一 Global MCP 公网入口。
- `src-tauri/src/tunnel/frp/` 直接接收 GlobalGatewayConfig，生成一个 `global-mcp` 代理。
- `src-tauri/src/tunnel/cloudflare.rs` 负责 cloudflared 进程和公网 URL 处理。

### 前端

- `src/main.ts` 创建 Vue 应用，`src/App.vue` 加载工作区、刷新 MCP 状态并承载全局导航、Toast 与关闭确认。
- `src/router.ts` 使用 Hash History，兼容 Tauri 静态产物；`src/views/WorkspaceView.vue` 管理项目上下文、策略、History、日志和 Planning；Global MCP 认证与公网连接集中在 Settings。
- `src/lib/api/` 封装 Tauri IPC；`src/components/` 提供 Vue 配置表单和状态面板；`src/lib/stores/` 使用 Vue `ref` 管理前端共享状态。

## 当前工作区观察

- 当前产品运行时已经收敛为 MCP-only：Actions listener、OpenAPI 网关、双服务状态、Actions 隧道/认证/日志/健康与前端入口均已删除。
- 旧 `profiles.json` 中的 `actions` 配置、`restore_actions_workspace_ids` 与 Actions 专属密钥会在升级加载时被定向清理；MCP 密钥保持不变。
- `docs/specs/**`、带日期的 `docs/verification/**` 与 `docs/history-session/**` 仍保留历史 Actions 记录，不代表当前产品能力；旧 Python/Actions 参考实现目录已从当前仓库移除。
- 当前 README、project-context 与本文件已经同步 Global MCP 多工作区架构。

## 验证结果

- `npm run version:check`：通过，项目版本 `0.3.0` 一致。
- `npm run check`：通过，0 错误、0 警告。
- `npm run build`：通过，Vue 3 + Vite 生产构建成功，静态产物输出到 `build/`。
- `npm run test:release`：通过；Rust library 184/184，Tool Contract 22/22，Security 24/24，Harness 4/4 + 11/11，History 20/20，Workspace E2E 7/7，共 272 项。
- Global MCP 额外覆盖 request-scoped `workspace_id`：即使宿主在连续工具调用间更换 transport session，也能显式保持 Workspace 路由；绝对路径仍不能逃逸所选 Workspace。
- `cargo clippy --all-targets --locked -- -D warnings`：通过。
- `cargo fmt --all -- --check`：通过。
- `git diff --check`：通过。
- GitNexus 索引刷新及关键符号上游影响分析已完成；最终变更范围核验待 release gate 后执行。

## 建议优先级

1. 完成当前工作树的发布提交和 `v0.3.0` 标签。
2. 继续保持 MCP-only 产品边界，避免重新引入第二套 transport/runtime 状态机。
3. 持续补充 MCP/tunnel 的运行时集成测试，尤其是端口冲突、停止等待、OAuth 回调和隧道自动启动失败场景。

---
*来源：当前源码、README、项目上下文文档、Git 状态与构建验证；GitNexus 索引作为辅助。*
