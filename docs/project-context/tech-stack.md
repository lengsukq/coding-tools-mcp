# 技术栈

> 本文档只描述当前仓库实际依赖，不列“计划使用”的未落地技术。

## 基本信息

| 属性 | 当前值 |
| --- | --- |
| 应用版本 | `0.2.1` |
| Rust Edition | 2021 |
| 前端语言 | TypeScript |
| 桌面框架 | Tauri 2 |
| 前端框架 | SvelteKit 2 / Svelte 5 |
| 构建工具 | Vite 6 |

## Rust Core

| 技术 | 用途 |
| --- | --- |
| `tauri` / `tauri-plugin-dialog` | 桌面外壳、Tray、IPC、文件选择 |
| `tokio` | 异步 Runtime、网络、进程、文件与 Session |
| `axum` | MCP / Actions / OAuth HTTP 服务 |
| `tower-http` | CORS |
| `reqwest` + rustls | 更新、健康检查、网络请求 |
| `serde` / `serde_json` | 配置、Tool Schema、状态持久化 |
| `jsonwebtoken` / `sha2` / `base64` | OAuth Token 与 PKCE |
| `uuid` | Goal / Plan / Task / OAuth Client 等 ID |
| `walkdir` / `glob` / `regex` | Workspace 文件发现与搜索 |
| `image` | Workspace 图片 Tool |
| `zip` / `flate2` / `tar` | 下载与归档处理 |
| `fs2` | 文件锁与运行时协调 |
| `windows` / `libc` | 平台特定能力 |

当前 MCP 协议和 Tool Schema 由项目自己的 Rust 实现维护，仓库没有使用 `rmcp` 或 `git2` 作为当前核心依赖；Git 工具通过受控 Git 命令实现。

## Frontend

| 技术 | 用途 |
| --- | --- |
| `@sveltejs/kit` | 路由与应用结构 |
| `svelte` 5 | UI |
| `@tauri-apps/api` | 前端到 Tauri IPC |
| `@tauri-apps/plugin-dialog` | 原生 Dialog |
| `@lucide/svelte` | 图标 |
| Tailwind CSS 4 | 样式能力，与项目自定义 CSS/设计系统共存 |
| TypeScript 5.6 | 类型检查 |

## 包管理与命令

仓库使用 `npm` / `package-lock.json`，不是 pnpm。

```bash
npm ci
npm run check
npm run build
npm run desktop

cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## CI

GitHub Actions 在 `push(main)`、`pull_request` 和手动触发时运行：

- Frontend check + production build；
- Rust fmt；
- Clippy `-D warnings`；
- Locked build；
- 全 target Rust tests。

---
*返回索引: [../project-context.md](../project-context.md)*
