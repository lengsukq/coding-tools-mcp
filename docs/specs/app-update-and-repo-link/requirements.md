# 需求文档：app-update-and-repo-link

## 功能概述

用户反馈在桌面客户端里几乎找不到官方仓库地址。本功能在侧边栏版本区域提供一键打开 GitHub 仓库的入口，并在「设置 → 通用」增加关于区块：展示当前版本、手动检查 GitHub Releases 是否有更新，有新版本时引导用户打开 Releases 页面下载。不做静默自动安装。

## 历史经验与坑（来自记忆库）

- **可复用经验**: 软件下载已用 reqwest + 下载代理/镜像配置访问 GitHub；打开目录已有跨平台 `platform/open.rs` 模式，扩展为打开 URL 即可。
- **必须规避的坑**: 不引入独立自动更新器插件；不把仓库 URL 硬编码散落多处导致不一致；检查更新失败时必须给出可读错误，禁止静默当“已是最新”。

## 术语定义

- **仓库主页**: 固定为 `https://github.com/lengsukq/coding-tools-mcp`。
- **Releases 最新页**: `https://github.com/lengsukq/coding-tools-mcp/releases/latest`。
- **当前版本**: 与 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 同步的应用版本字符串（前端由 `src/lib/app-version.ts` 读取）。
- **最新发布版本**: GitHub Releases API `releases/latest` 返回的 `tag_name`（可带或不带 `v` 前缀）。

---

## 范围边界

**In Scope（本次要做）**
- 侧边栏 footer 在版本旁提供仓库链接，一键用系统默认浏览器打开仓库主页。
- 设置 → 通用页增加「关于」卡片：显示当前版本、打开仓库、打开 Releases、检查更新。
- 后端提供 `open_url` 与 `check_app_update` Tauri 命令；版本比较忽略可选 `v` 前缀，按语义化主.次.补丁比较。
- 检查更新复用现有下载代理配置（`AppSettings.download`），请求超时有界。
- 覆盖 URL 校验、版本比较、更新判定的 Rust 单元测试。

**Out of Scope（本次不做）**
- `tauri-plugin-updater` 或任何应用内静默下载/安装/重启。
- 启动时强制弹窗检查更新、后台定时轮询。
- 修改 Release 发布流水线或版本号递增规则。
- 独立 About 模态框或新路由页（复用通用设置页）。

---

## 需求列表

### FR-1: 侧边栏仓库入口

**优先级:** Must
**用户故事:** 作为桌面端用户，我想在侧边栏直接打开官方仓库，以便反馈问题和查找文档，而不必到处搜索。

#### 验收标准（EARS）

1. WHEN 用户点击侧边栏仓库入口 THEN 系统 SHALL 用系统默认浏览器打开仓库主页 URL。
2. WHEN 侧边栏 footer 渲染 THEN 系统 SHALL 同时展示当前版本字符串与可点击的仓库入口。
3. IF 打开浏览器失败 THEN 系统 SHALL 向用户展示可读错误信息。

### FR-2: 通用设置关于区块

**优先级:** Must
**用户故事:** 作为用户，我想在设置里看到当前版本并快速打开仓库或 Releases，以便确认安装包来源。

#### 验收标准（EARS）

1. WHEN 打开「设置 → 通用」THEN 系统 SHALL 展示「关于」卡片，包含当前版本。
2. WHEN 用户点击「打开仓库」THEN 系统 SHALL 打开仓库主页。
3. WHEN 用户点击「打开 Releases」THEN 系统 SHALL 打开 Releases 最新页。

### FR-3: 手动检查更新

**优先级:** Must
**用户故事:** 作为用户，我想手动检查是否有新版本，以便及时下载更新。

#### 验收标准（EARS）

1. WHEN 用户点击「检查更新」THEN 系统 SHALL 请求 GitHub `repos/lengsukq/coding-tools-mcp/releases/latest`，读取 `tag_name` 与 `html_url`。
2. WHEN 最新 `tag_name` 语义化版本高于当前版本 THEN 系统 SHALL 提示有新版本并允许打开该 Release 页面。
3. WHEN 最新版本等于或低于当前版本 THEN 系统 SHALL 提示已是最新或无需更新。
4. IF 网络失败、非 2xx、JSON 无效或 `tag_name` 不可解析 THEN 系统 SHALL 返回可读错误，不得谎称已是最新。
5. WHILE 检查进行中 THE 系统 SHALL 禁用重复点击或展示进行中状态，避免并发重复请求。

### FR-4: 安全的 URL 打开

**优先级:** Must
**用户故事:** 作为维护者，我希望只允许打开 http/https 链接，以避免误打开危险协议。

#### 验收标准（EARS）

1. WHEN `open_url` 收到以 `http://` 或 `https://` 开头的 URL THEN 系统 SHALL 调用系统打开该 URL。
2. IF URL 为空、不是 http/https、或含控制字符 THEN 系统 SHALL 拒绝并返回错误。

---

## 非功能需求

- **NFR-1（性能）**: 检查更新 HTTP 超时不超过 15 秒；失败快速返回，不阻塞 UI 线程超过命令本身等待时间。
- **NFR-2（安全）**: 仅打开 http/https；请求使用明确 User-Agent；不记录或展示 GitHub token。
- **NFR-3（兼容性）**: Windows、macOS、Linux 均可打开 URL；无网络时检查更新失败信息可读。
- **NFR-4（体验）**: UI 使用现有 Refined Utilitarian 风格与 Lucide 图标，不新增 emoji。

---

## 依赖关系

- 依赖现有 `platform/open.rs` 模式扩展打开 URL。
- 依赖现有 `AppSettings.download` 代理配置（与软件管理下载一致）。
- 依赖公开 GitHub Releases API（无需鉴权，受速率限制）。
- 依赖前端 `src/lib/app-version.ts` 与后端 `CARGO_PKG_VERSION` 同源版本约定。

---

## 检查清单

- [x] 已消化记忆库的历史经验，并逐条规避「历史坑」
- [x] 需求覆盖核心场景与边界场景
- [x] 每条需求有唯一 ID（FR-n），将在 design.md / tasks.md 中被引用
- [x] 验收标准使用 EARS 格式且可测
- [x] 已标注优先级（MoSCoW）
- [x] 范围边界（In/Out of Scope）明确
- [x] 非功能需求明确、尽量可量化
- [x] 依赖关系完整
