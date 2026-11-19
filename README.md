<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="Coding Tools MCP 图标">
</p>

<h1 align="center">Coding Tools MCP</h1>

<p align="center">
  把本地项目变成 AI 可直接开发、能够跨会话延续上下文的持久工作区。
</p>

<p align="center">
  <a href="https://github.com/lengsukq/coding-tools-mcp/releases/latest"><img src="https://img.shields.io/github/v/release/lengsukq/coding-tools-mcp?label=Release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-x64-0078D4?logo=windows" alt="Windows x64">
  <img src="https://img.shields.io/badge/macOS-Apple%20Silicon-000000?logo=apple" alt="macOS Apple Silicon">
  <a href="https://www.apache.org/licenses/LICENSE-2.0"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0"></a>
</p>

<p align="center">
  <a href="README.md">中文</a> · <a href="README.en.md">English</a> · <a href="https://github.com/lengsukq/coding-tools-mcp/releases/latest">下载最新版</a>
</p>

Coding Tools MCP 是一个 Rust + Tauri 2 构建的 **local-first AI coding runtime**。桌面端只需要启动一个 Global MCP Endpoint，多个本地项目作为彼此隔离的 Workspace 注册进去；每个 Chat Session 再选择自己要操作的 Workspace。AI Agent 可以通过 MCP 读取和修改代码、运行命令与测试、检查 Git、维护 Goal / Plan，并把关键进度保存为项目内的历史会话。

它更接近一个“给 AI 用的长期开发工作台”，而不是为每个项目重复启动一套 MCP 服务：**Workspace 负责项目边界，Planning 负责意图与进度，Execution / Verification 负责执行证据，History 负责跨会话恢复。**

## v0.3.4 – v0.3.8+：近期核心更新

在“一个 Global MCP 管理所有隔离 Workspace”的基础底座上，近期版本重点补齐了 **主流 AI 零门槛接入、跨设备局域网协同、macOS 原生屏幕控制、全流程审计与 Vue 3 设计系统**：

- **Gemini App 连接器与原生 `mcp-stdio` 桥接**：新增「Gemini App · MCP 连接」面板，支持 **Local MCP**（内置毫秒级 Rust `mcp-stdio` 本地桥接，不开公网、不依赖 Node.js，自动生成 `0600` 权限凭据）与 **Cloud MCP**（一键体检并复制公网地址，适配 Gemini 网页版/手机端）；同时基于 OAuth 2.1（PKCE S256 + DCR）和 Bearer 兼容 **ChatGPT、claude.ai、Grok、Gemini**。
- **局域网对等配对与多设备联邦网关**：支持跨 macOS / Windows 局域网（及 EasyTier / Tailscale 等组网）设备发现与 **6 位配对码**双向握手；通过 HMAC 签名对等 RPC（`/api/lan/v1/*`）实现公网隧道硬隔离，支持 `{device_id}:{workspace_id}` 复合工作区路由、`device_list` 设备发现、跨设备 Computer Use 转发、变更审查代理（`/review/peer/{device_id}/{id}`）与双向审计。
- **macOS 原生 Computer Use（受控屏幕观察与输入）**：基于 `ScreenCaptureKit` 实现按需截屏观察与目标应用受控鼠键输入；内置截图快照绑定防错位、高危动作单次确认、一键紧急暂停/恢复、macOS TCC 权限自检，以及独立的 **Computer Use** 设置页。
- **工具调用审计与多主题调色盘**：新增工具调用审计中心（按天分区的 JSONL 日志、自动过期轮转、筛选统计与详情抽屉）；推出统一毛玻璃设计系统、6 大设置分类与草稿保护，以及 **6 套全站主题调色盘**（海洋蓝、珊瑚橙、葡萄紫、翡翠绿、琥珀金、樱花粉）。
- **Tunnel 控制、健康监控与轻量运行态**：支持强制重启 Cloudflare Quick Tunnel 与全局健康状态实时汇总；新增 macOS **轻量菜单栏模式**、每 5 分钟空闲会话/命令内存回收，并优化任务验收与分栏 Diff 行对齐。
- **架构与界面全面重构**：前端全面迁移至 Vue 3 + TypeScript + Tailwind CSS 4，并完成网关、Computer Use、Stdio 桥接、Review、Audit 与调度器的模块化拆分。

这意味着现在推荐的使用路径更加立体：**连接一次 Global MCP（本机 `mcp-stdio`、局域网联邦或公网 `/mcp`）→ 选择本地或对等设备 Workspace → 维护 Goal / Plan → 执行与审计验收 → History 留档。**

## 功能全景：特点与优势

Coding Tools MCP 不只是一个 MCP 地址转发器，而是一个以 **Workspace-first** 为核心的 AI 开发运行时：桌面端管理项目和服务，统一工具运行时负责安全执行，并通过 MCP 为本地、局域网与公网 AI 客户端提供开发能力。

| 功能 | 主要特点 | 带来的优势 |
| --- | --- | --- |
| 工作区管理 | 每个项目有独立目录、执行策略、Planning 与 History；侧边栏直接展示分支、变更和当前 Goal / Plan | 多项目切换更清晰，降低把命令执行到错误项目的风险 |
| MCP 工具运行时 | 文件、Patch、命令、Git、图片、Skill 和状态管理共用一套工具内核 | 单一协议入口减少重复状态机和兼容分支，策略、错误和权限更容易保持一致 |
| 连接、桥接与联邦网关 | 支持本机 `mcp-stdio` 桥接、Global Gateway、FRP、Cloudflare Quick Tunnel 及局域网多设备联邦 | 本机 Gemini/Claude、局域网跨机协同和远程网页 AI 可共用同一套工作区模型 |
| 身份认证 | OAuth Authorization Code、PKCE S256、DCR、Refresh Token，并兼容 Bearer、静态 Client 与 LAN HMAC 配对 | 既能满足现代网页端 OAuth 流程，也能兼顾本机桥接、旧客户端与局域网安全通信 |
| macOS Computer Use | 基于 `ScreenCaptureKit` 的按需截屏与目标应用受控输入，含快照绑定、紧急暂停与 TCC 诊断 | 需要界面操作时 AI 可安全代劳，同时通过多重边界锁定权限、防止误操作 |
| Planning / Goal / Task | Direct、Plan、Goal 三种约束模式，Execution Ledger 统一记录执行状态与人工验收 | 复杂任务可拆分、可恢复、可验收，减少 AI 在中途偏离目标的情况 |
| 历史会话 | 项目内保存无损 Markdown 档案，并提供有界状态、搜索、分页读取和启动提示词 | 上下文跟着仓库走，不依赖某一个聊天窗口，也不会每次注入全部历史 |
| 审计、日志与健康检查 | 提供工具调用审计（筛选/轮转）、请求日志、OAuth 元数据与连接健康汇总 | AI 每次调用都透明可查，出现连接、权限或授权问题时能快速定位根因 |
| Workspace-first 安全边界 | 工作区外默认只读，`.git` 永久保护；`.github`、依赖清单/锁文件、构建配置等高风险文件默认保护，可按 Workspace 显式放开 Patch 写入 | 权限范围明确、修改可控，适合把 AI 接入真实项目而不是临时演示目录 |

### 桌面端全局 Dashboard：不进入工作区也能掌握全局状态

全局 Dashboard 把多个 Workspace 的运行与执行状态汇总到一个控制面板中。它不要求先进入某个项目，就能查看当前 AI Execution、验证证据、Global MCP 状态、Chat Session 路由、Planning 焦点和需要人工关注的异常。

Dashboard 还会持续刷新 MCP Token 估算，并提供 Token Flow、请求成功率、Planning 完成度、Workspace Token Distribution、最近 History Session 与系统健康状态。Token 统计来自本地服务请求大小估算，不保存请求正文。

### 1. 工作区管理：把项目变成长期可识别的开发上下文

工作区是 Coding Tools MCP 的项目上下文单位。它绑定一个本地项目目录，同时维护自己的执行策略、Planning 与 History。MCP Listener、认证和公网入口属于应用级 Global MCP；桌面端可以保存多个项目，而 Chat Session 的 Workspace 选择彼此隔离。

**特点**

- 项目目录是事实来源，工作区名称和项目配置分离维护。
- MCP session 在全局连接内选择 Workspace，规划状态和历史会话仍围绕所选工作区组织。
- 侧边栏直接展示当前分支、clean / changed、当前 Goal / Plan、完成进度和执行状态。
- Workspace Overview 支持主仓库与一级子 Git 状态、7 天修改趋势、执行质量、整个工作区 Diff / Review 和 IDE 快速打开。
- 后台空闲内存自动维护每 5 分钟执行：MCP Session 空闲 30 分钟过期（进行中的 RPC 受保护）；完成并且 30 分钟未访问的命令从内存回收，但完整输出仍按原磁盘缓存规则保留。运行中命令、MCP 服务和隧道不会被清理。菜单栏显示定时回收的累计会话数。
- 远程 AI Change Review 支持 Unified / Split 切换；Split 使用左右独立横向滚动的代码栏、单侧行号、对齐的差异行和自动截断的文件标题，避免长代码覆盖相邻栏位。

**优势**

- 多项目并行时不容易混淆目录、当前 Plan 和执行上下文。
- AI 每次连接都能获得明确的项目边界，减少“对错仓库执行命令”的风险。
- 状态读取采用缓存优先并在后台刷新，频繁切换 Workspace 时减少重复加载造成的卡顿。

### 2. MCP 工具运行时：一套内核覆盖真实开发动作

Rust 工具内核统一提供文件读取、搜索、Patch、命令执行、Git、图片、Skill 和状态管理能力。所有远程工具调用都通过 MCP Streamable HTTP 进入同一个分发入口。

**特点**

- Stable Tool API v2 用 `history_manage`、`planning_manage`、`task_manage` 等聚合入口承载生命周期操作，同时保留兼容 profile。
- 文件、命令和 Patch 会经过统一的路径边界、命令策略、确认和错误结构处理。
- Patch 支持预检和失败恢复，减少半写入状态。

**优势**

- 不同客户端得到一致的工具行为，维护成本和兼容风险更低。
- AI 可以完成从理解代码、修改文件、运行测试到检查 Git 的完整闭环。
- 复杂操作的权限与失败原因更容易解释、审计和恢复。

### 3. 连接、本地桥接与公网入口：从本机直连平滑走向远程开发

整个应用只保留一个 Global MCP Endpoint（`/mcp`）。本地或远程 AI 客户端先建立 MCP session，再用 `workspace_list` / `workspace_select` 选择项目；支持本机回环桥接、局域网多设备联邦和公网隧道三种接入形态。

**特点**

- **本机无头桥接（`mcp-stdio`）**：内置原生 Rust `mcp-stdio` 子命令与「Gemini App · MCP 连接」向导，冷启动小于 10ms，无需 Node.js/`npx` 与公网隧道即可直连本地桌面 AI 客户端。
- **公网隧道监督与重启**：支持 FRP 和 Cloudflare Quick Tunnel（含面板一键强制重启），隧道进程由桌面端统一监督；服务停止时关联隧道同步断开。
- **会话隔离**：每个 Chat session 的 Workspace 选择彼此隔离，请求开始前固定项目上下文。

**优势**

- 本机 Gemini/Claude、局域网跨机协同和远程网页 AI（ChatGPT、claude.ai、Grok、Gemini）共用同一套工作区模型。
- 多个项目不必为每个工作区都单独维护一套公网基础设施。
- 端点、隧道、健康检查与服务状态集中可见，排查网络问题更直接。

### 4. OAuth 与认证：兼顾安全流程和客户端兼容

MCP OAuth runtime 支持 Authorization Code、PKCE S256、Dynamic Client Registration 和 Refresh Token；同时保留 Bearer 及静态 Client ID / Secret，方便旧客户端或简单环境继续接入。

**特点**

- 动态注册的客户端可以根据服务端 metadata 自助完成 Client 注册。
- PKCE S256 降低授权码被截获后的滥用风险。
- Access Token 与 Refresh Token 类型区分，并绑定已注册的 redirect URI。

**优势**

- 新客户端可以使用更完整的 OAuth 流程，减少手工复制凭据。
- 旧客户端仍有清晰的兼容路径，不需要为了升级一次性重做接入。
- 认证设置和健康检查集中在桌面端，授权失败更容易定位。

### 5. Planning、Goal、Task：让复杂任务可控、可恢复、可验收

规划页面把“目标”“计划”和“执行任务”分开管理。Direct 适合小改动，Plan 适合需要拆步骤的任务，Goal 适合需要长期推进和明确验收标准的工作。AI 可以在对话中创建和维护计划，桌面端负责显示约束、状态和最终验收归档。

**特点**

- Goal 表达要达成的结果和约束，Plan 表达步骤，Harness Task 表达可恢复的执行过程。
- 写操作可以要求绑定当前激活 Goal，并遵守当前 Plan。
- Execution Ledger 统一投影当前步骤、最近工具、错误、变更文件、历史检查点和验证结果。

**优势**

- 大任务不会只依赖聊天上下文，暂停后能从明确状态继续。
- 计划、执行和验收分层，既保留灵活性，又能对高风险修改增加约束。
- 开发者可以在桌面端快速看到“现在做到了哪一步、还差什么、是否已验证”。

### 6. 历史会话：让上下文跟着仓库走

每个项目的长期开发记录保存在 `docs/history-session/`。Markdown 档案保留完整事实，`memory/state.json` 和 `memory/manifest.json` 只提供有界的当前状态与索引；新对话可以通过启动提示词、搜索和分页读取恢复需要的上下文。

**特点**

- 支持初始化、检查点、校验、搜索和无损分页读取。
- 当前会话可默认记录，旧会话则由工作区面板按需选择注入。
- 只注入有界快照和精选片段，不把全部历史一次性塞进新对话。

**优势**

- 项目可以独立备份、审阅和提交历史档案，不被某个聊天平台锁定。
- 新 Agent 能快速获得当前状态，再按需精读旧决策，节省上下文和时间。
- 重要的修改、测试、风险和下一步可以形成可追溯的交接记录。

### 7. 日志、健康检查与安全边界：让问题可见，让权限有边界

桌面端提供服务日志、端点检查、OAuth 元数据检查和运行状态展示。统一工具内核则把工作区路径、命令策略、仓库保护和危险操作确认放在同一套 Policy 中处理。

通用设置还集中管理应用版本与 Releases 入口、界面内存释放、全局 Agent Runtime、启动时恢复运行状态和局域网访问开关。涉及网络暴露的选项默认关闭，并明确提示启用后的影响。

**macOS 轻量驻留（菜单栏）**：主界面侧栏点击「轻量驻留」，或关闭窗口时选择「轻量驻留」，会卸载主 WebView、隐藏 Dock 图标（仅保留右上角菜单栏入口），降低界面内存占用，同时保持全局 MCP、工作区会话及已启动的隧道继续运行。点击 macOS 右上角菜单栏图标可查看约每 15 秒更新的主进程/界面内存估算、WebView 进程数、MCP 状态、工作区与活跃会话，并可恢复主窗口、刷新信息或完全退出。恢复界面时 Dock 图标也会恢复；页面重新加载并保留窗口尺寸与位置。未保存的表单修改会在进入轻量模式前要求确认。Windows 暂保留原有托盘隐藏模式，不声称释放 WebView 内存。

**特点**

- 工作区内允许按策略读写和执行；工作区外默认只读。
- `.git` 永久不可写；`.github`、依赖清单/锁文件、构建配置、README/LICENSE 等高风险文件默认受保护，可在 Workspace 设置中显式允许 Patch 修改，删除高风险文件仍需确认。
- 健康检查分别报告本地服务、公网入口和认证元数据的状态。
- Windows 当前是 `policy_only` 执行边界，项目不会把静态策略夸大为完整 OS Sandbox。

**优势**

- AI 能力可以接近真实开发流程，同时保持清晰的最小权限范围。
- 连接失败、授权失败和工具调用失败有明确的排查入口。
- 对安全能力的边界如实呈现，方便团队根据部署环境补充系统级隔离。

### 8. 局域网对等配对与多设备联邦：一个入口协同多台电脑

当开发者同时拥有多台设备（如 macOS 笔记本与 Windows 台式机）时，可通过「应用设置 → MCP 连接 → 局域网多设备配对」建立点对点联邦，在同一个 AI 会话中跨设备访问共享工作区。

**特点**

- 支持 UDP 多播/广播、虚拟网卡单播扫描及 HTTP 直连探测，并通过 **6 位一次性配对码**完成双向安全握手。
- 对等通信采用 HMAC-SHA256 签名与防重放校验，局域网接口（`/api/lan/v1/*`）与公网隧道硬隔离，不暴露给公网。
- AI 可调用 `device_list` 发现已配对设备，并通过 `{device_id}:{workspace_id}` 复合作用域路由工具调用、转发 Computer Use 操作或通过 `/review/peer/{device_id}/{id}` 查看跨设备变更审查。

**优势**

- 只需连接一台主设备的 MCP 入口，即可安全调度局域网或组网内多台主机上的项目与算力。
- 跨设备调用记录双向审计，共享工作区范围可按设备独立勾选与随时停用。

### 9. macOS 原生 Computer Use 与审计/主题系统：安全可控、全程留痕

针对需要查看界面或模拟点击的桌面操作场景，提供 opt-in 的 macOS 原生 Computer Use 能力；同时在应用层补全工具调用审计与多主题界面体验。

**特点**

- **macOS 原生 Computer Use**：基于 `ScreenCaptureKit` 按需截图观察并执行受控鼠键输入；支持在「应用设置 → Computer Use」配置持久化本地授权与**目标应用白名单**，内置操作绑定快照防错位、高危动作单次确认、一键紧急暂停/恢复与 macOS TCC 权限诊断。
- **工具调用审计中心**：记录每次已认证 MCP 工具调用的摘要、脱敏结果与耗时（按日期与工作区分区的 JSONL 存储，支持过期自动轮转），可在桌面端筛选统计与查看详情。
- **多主题与设置防护**：提供 6 套全站主题调色盘（海洋蓝/珊瑚橙/葡萄紫/翡翠绿/琥珀金/樱花粉）与三层毛玻璃设计系统，并为设置页提供草稿保护与分组串行保存。

**优势**

- 需要界面操作时 AI 可安全代劳，遇到风险或屏幕变化可立即阻断或由人工拍板。
- 每一次工具调用均有账可查，长时间跨项目开发也能保持清晰的可追溯性。

## 30 秒看懂怎么用

```text
下载安装桌面端
  → 添加项目目录
  → 启动 MCP 和公网隧道
  → 复制“公网 MCP 地址”
  → ChatGPT 开启开发人员模式
  → 新建 MCP 插件并粘贴地址
  → 完成授权
  → workspace_list / workspace_select 选择项目
  → 在新对话中开始开发
```

第一次使用只需要记住两件事：**桌面端负责把项目变成 MCP 工作区（并支持本机 `mcp-stdio` 桥接、局域网多设备联邦或公网 `/mcp` 入口），AI 客户端负责连上它并按 Workspace 边界开发。**

- [查看完整安装和桌面端启动步骤](#五分钟开始使用)
- [直接查看 ChatGPT 插件配置](#mcp-connector)
- [查看 Gemini / claude.ai / Grok 接入矩阵](docs/web-chat-connectors.md)

## 五分钟开始使用

### 1. 安装桌面客户端

打开 [Releases](https://github.com/lengsukq/coding-tools-mcp/releases/latest) 并下载对应安装包：

| 系统 | 安装包 |
| --- | --- |
| Windows 10/11 x64 | `Coding.Tools.MCP_*_x64-setup.exe` |
| macOS Apple Silicon | `Coding Tools MCP_*_aarch64.dmg` |

macOS 安装包目前未签名。如果系统阻止首次打开，请在“系统设置 → 隐私与安全性”中确认打开。

### 2. 添加项目工作区

1. 点击左侧的“添加工作区”。
2. 选择项目根目录。
3. 设置工作区名称并保存；工作区会长期保留在左侧列表中。

### 3. 配置公网隧道或本地连接

- **同一台 Mac / 本机客户端（推荐优先使用）**：打开 **「应用设置 → MCP 连接」**，无需开启公网隧道即可使用本地回环地址（`http://127.0.0.1:<端口>/mcp`），或直接在 **「Gemini App · MCP 连接 → Local MCP」** 点击「生成凭据并复制」，通过内置原生 `mcp-stdio` 桥接直连。
- **跨设备局域网协同**：在 **「应用设置 → MCP 连接 → 局域网多设备配对」** 中扫描或探测同局域网设备，通过 6 位配对码安全握手，即可在一个 MCP 入口中使用 `{device_id}:{workspace_id}` 跨机操作 macOS / Windows 上的项目。
- **公网远程访问（ChatGPT / Gemini 网页版 / claude.ai / Grok）**：在 **「应用设置 → MCP 连接」** 配置公网入口：
  - **零门槛推荐：Cloudflare Quick Tunnel**。选择 Cloudflare 快速模式，无需账号即可生成 `https://<随机>.trycloudflare.com` 公网地址，并支持在面板中一键强制重启隧道；长期使用建议 FRP 固定子域名或外部反向代理。
  - 在 **「应用设置 → 网络与工具」** 中安装或识别 `frpc` / `cloudflared`。
  - 在 **「应用设置 → MCP 连接 → FRP 服务器配置」** 中保存服务器、端口和 Token，再回到 Global MCP 连接选择 FRP 并填写唯一子域名。

如果还没有可用的 FRPS 服务端，可以参考：[FRPS 服务端安装教程（微信公众号）](https://mp.weixin.qq.com/s/kmpQhHsvmHlaLfj4rw3A0Q)。安装完成后，把服务端地址、端口和 Token 填入客户端的“FRP 配置”即可。

### 4. 启动 MCP

在 Dashboard 或“应用设置 → MCP 连接”点击“启动 Global MCP”。客户端会显示：

- 唯一本地 MCP 地址，例如 `http://127.0.0.1:28765/mcp`；
- 公网 HTTPS MCP 地址；
- ChatGPT / Gemini 连接所需的认证与桥接信息；
- 实时日志和健康检查结果（支持分别检查本地与公网端点、OAuth 元数据和 MCP 受保护资源，并在桌面端直接查看最近的 MCP 请求日志，确认工具列表、历史与检查点调用是否真正到达服务端）。

### 5. 连接 AI 客户端

除 ChatGPT 外，claude.ai、Grok、Gemini 等支持远程 MCP Connector 的网页客户端都可以接入同一工作区：[网页 Chat 客户端接入矩阵](docs/web-chat-connectors.md) 提供逐客户端配置、认证选择和排查表。

支持 MCP 的客户端使用界面中的公网 MCP URL。OAuth 当前支持 Authorization Code + PKCE S256、Dynamic Client Registration（`/register`）和 Refresh Token。支持动态注册的客户端可以直接读取服务端 metadata 并注册自己的 Client；不支持 DCR 的旧客户端仍可继续使用桌面端配置的静态 Client ID / Secret。

首次连接建议先明确选择 Workspace；历史初始化不再是必需步骤：

```text
workspace_list
workspace_select(workspace_id=...)
server_info
git_status
```

这样 Agent 不需要依赖聊天上下文猜测当前项目。如果客户端不能稳定保留 MCP Session，单次跨 Workspace 操作可以改用 `workspace_invoke(workspace_id=..., tool=..., arguments=...)`，而不是反复切换或猜测目录。需要显式创建或恢复历史目标时，再使用 History 能力。

## ChatGPT 接入方式

### MCP Connector

配置前请先确认：

1. Global MCP 服务和公网隧道均处于运行状态。
2. “健康检查”中的公网 MCP 检查通过；如果使用 OAuth，再确认 OAuth 受保护资源和授权元数据检查通过。
3. 从桌面端“GPT 配置”卡片复制“公网 MCP 地址”。使用 OAuth 时需要授权口令；只有客户端不支持动态注册时才需要额外填写静态 Client ID / Secret。

> ChatGPT 必须使用公网 HTTPS `/mcp` 地址，不能使用 `http://127.0.0.1:28766/mcp` 之类的本地地址。ChatGPT 的菜单名称可能随版本和语言设置略有变化。

#### 1. 开启 ChatGPT 开发人员模式

打开 ChatGPT 设置，进入“账户安全与登录”，开启“开发人员模式”。该开关允许添加未经验证的 MCP 连接器。

![在 ChatGPT 中开启开发人员模式](docs/images/gpt-config-1.png)

*开发人员模式具有较高权限，只应连接你自己部署或明确可信的 MCP 服务。*

#### 2. 创建 MCP 插件

在 ChatGPT 左侧进入“插件”，点击右上角的 `+` 新建插件，然后选择 MCP（测试版）并填写：

| ChatGPT 字段 | 填写内容 |
| --- | --- |
| 名称 | 自定义一个容易识别的名称，例如 `Coding Tools MCP` |
| 描述 | 简要说明它连接的项目或用途 |
| 连接 | 粘贴桌面端“GPT 配置”中的公网 MCP 地址，URL 应以 `/mcp` 结尾 |
| 身份验证 | 与桌面端保持一致；截图以 OAuth 为例 |

![在 ChatGPT 中新建 MCP 插件并填写连接信息](docs/images/gpt-config-2-detail.png)

使用 OAuth 时，优先让支持 Dynamic Client Registration 的客户端根据 metadata 自动注册；若当前客户端只支持静态 OAuth，则继续填写桌面端提供的 Client ID 和 Client Secret。保存或连接后会进入授权页面，输入桌面端“GPT 配置”卡片中的授权口令完成授权。

> Client Secret、授权口令和 Bearer Token 都属于敏感信息，不要粘贴到对话、Issue 或公开截图中。若桌面端使用 Bearer 或不启用认证，请在 ChatGPT 中选择当前界面提供的对应认证方式。

#### 3. 验证连接

创建一个启用了该插件的新对话，并发送：

```text
请使用 Coding Tools MCP 先调用 workspace_list，选择 coding-tools-mcp 工作区，
然后调用 server_info 和 git_status，告诉我当前 Workspace、MCP 状态和 Git 状态。
```

如果能够返回当前项目的信息，说明“桌面端 → 公网隧道 → OAuth → ChatGPT → MCP 工具”链路已经打通。当前会话默认允许记录检查点；旧会话是否注入由工作区的“历史上下文”面板多选控制。

如果 ChatGPT 仍显示旧的工具列表，请断开并重新连接插件，或创建一个新对话后再次验证。

#### 常见问题

| 现象 | 优先检查 |
| --- | --- |
| ChatGPT 无法连接 | 是否使用公网 HTTPS `/mcp` 地址，而不是 `127.0.0.1`；桌面端公网 MCP 健康检查是否通过 |
| OAuth 授权失败 | Client ID、Client Secret 和授权口令是否来自同一个工作区；OAuth 元数据检查是否通过 |
| 看不到新增工具 | 断开并重新连接插件，然后创建一个新对话 |
| 工具调用失败 | 打开桌面端“日志”和“健康检查”，确认请求是否到达 MCP 服务 |

## 为什么需要它

- **面向真实开发**：文件、命令、Git、测试和长时间运行的进程都在同一个 Workspace 中。
- **跨会话持续开发**：新对话先获得有界的当前状态，需要精确旧上下文时按关键词定位并读取原始档案，无需反复向 AI 解释项目背景和当前进度。
- **进度可追溯**：每轮任务完成后可保存结构化检查点，决策、修改、测试结果和下一步都留在项目目录中。
- **多工作区管理**：一个 Global MCP 连接管理多个本地项目，Chat Session 分别选择自己的 Workspace，不需要为每个项目重复配置 MCP 和公网入口。
- **连接 ChatGPT 更直接**：内置 Streamable HTTP、OAuth、Bearer Token、FRP 和 Cloudflare 隧道。
- **默认工具面保持简单**：稳定的核心工具默认可用，高级 Harness 能力按需开启。

## 让项目记住每次对话

普通聊天记录适合回看交流内容，但不适合作为长期开发交接。Coding Tools MCP 将会话进度写入当前项目的 `docs/history-session/`，让上下文跟随项目，而不是困在某一个聊天窗口里。

历史上下文面板提供当前会话记录开关、历史会话多选、预览和清除选择。应用选择后会刷新 MCP 上下文，只注入会话索引、最近检查点、关键文件和精选片段；每个会话最多 3 条片段、每条最多 512 字节。完整 Markdown 历史仍保存在 `docs/history-session/`，需要时再用搜索和分页读取工具恢复。

默认 `compact` profile 通过 Stable Tool API v2 的 `history_manage` 聚合入口访问历史能力；`core` / `advanced` 兼容 profile 仍保留以下五个旧工具名：

| 工具 | 作用 |
| --- | --- |
| `history_session_bootstrap` | 兼容或手动初始化/恢复项目会话；compact 模式只返回有界索引，不返回全量历史 |
| `history_session_checkpoint` | 当前会话默认按需追加结构化进度；可省略 `session_key` 和 `expected_path`，服务端会懒初始化目标 |
| `history_session_validate` | 检查历史编号、文件和会话映射；必要时重建派生索引，不删除已有历史 |
| `history_session_search` | 按确定性关键词搜索长期 Markdown 档案，返回有界的命中位置和短片段 |
| `history_session_read` | 按编号或搜索结果位置，无损、UTF-8 安全地分页读取一份原始 Markdown 档案；默认每页 `16 KiB`，最多 `64 KiB`，根据 `next_cursor` 继续读取 |

典型效果：

```text
对话 1：分析项目 → 修改代码 → 运行测试 → 保存检查点
                                      ↓
对话 2：读取有界当前状态 → 搜索并精读需要的旧档案 → 从上次进度继续 → 保存新检查点
```

历史档案使用可读的 Markdown 格式，可以随项目备份或纳入 Git，也方便开发者直接审阅和修订。`memory/state.json` 是有界当前状态投影，`memory/manifest.json` 只保存位置、哈希与关键词，不复制正文；Markdown 才是长期、无损的事实来源。若手动调用 bootstrap，首次输入可作为 `initial_user_input` 传入；每轮检查点使用 `raw_user_input`，服务端无法读取未传入的远程聊天文本。检查点采用幂等追加，同一 `turn_id` 内容变化时保留 revision 与 supersedes 证据，并要求返回成功且会话目标一致后才确认保存成功。

> 历史持久化由 AI 调用 MCP 工具完成，并非桌面端在后台录制聊天内容。若客户端未触发工具调用，服务端无法凭空感知新的对话或任务进度。

## Agent 可以做什么

默认 `compact` profile 提供一组稳定、可组合的开发工具；`core` 和 `advanced` 作为 legacy 兼容 profile 保留：

| 类别 | 主要工具 |
| --- | --- |
| 文件读取 | `read_file`、`list_dir`、`list_files`、`search_text`、`grep_text`、`view_image` |
| 文件修改 | `apply_patch` |
| 命令执行 | `exec_command`、`write_stdin`、`read_output`、`kill_session` |
| Git | `git_status`、`git_diff`、`git_log`、`git_show`、`git_blame` |
| Workspace 路由 | `workspace_list`、`workspace_current`、`workspace_select`、`workspace_invoke` |
| 运行时 | `server_info` |
| 状态管理 | `history_manage`、`planning_manage`；Stable Tool API v2 继续提供聚合式生命周期接口 |

聚合工具通过 `action` 参数完成生命周期操作，例如 `history_manage(action=search)` 或 `planning_manage(action=update_plan)`。这样新增生命周期行为时不需要持续扩大顶层 MCP Tool Schema；兼容 profile 仍可保留旧工具名。

长运行命令现在归属于 Workspace Runtime，而不是某一次 MCP transport 连接。`exec_command` 返回稳定 `command_id`，重新连接或从同一 Workspace 的另一运行入口仍可继续 `read_output` / `write_stdin` / `kill_session`；旧 `session_id` 参数继续兼容。stdout/stderr 在内存中保留固定预算的 Head + Tail 作为低延迟预览，同时完整原始输出会写入应用缓存目录；因此普通同步命令结束并从 SessionStore 回收后，稳定 `output_ref` 仍可分页读取中间内容。`read_output` 还支持 `query` / `regex` / `case_sensitive` 的受限日志搜索，完整日志不直接塞回模型上下文。

`exec_command(action=quality_gate)` 复用同一 Tool Surface 自动发现项目已有的 lint/check/typecheck/test/build/format 检查，支持 `dry_run`、`checks` 过滤和 `stop_on_failure`。每个派生检查都会重新经过现有命令策略，并返回紧凑的 `PASS/WARN/FAIL`、耗时、关键错误/警告计数及独立 `output_ref`；不会自动安装依赖、deploy 或 publish。

`server_info` 的 `runtime` 元数据包含 `runtime_id`、启动时间、应用版本、构建身份以及当前 Tool Profile 的 `tool_schema_version` / `tool_schema_hash`。客户端可以回传 `known_schema_hash`，在工具 Schema 已变化时得到 `schema_changed` / `reconnect_recommended`，避免继续误用旧 Runtime 或旧工具定义。

典型开发过程：

```text
打开 Workspace
  → 理解项目和 Git 状态
  → 搜索并读取代码
  → 事务化应用 Patch
  → 运行命令和测试
  → 检查 diff 并提交
```

高级 profile 还保留项目状态、操作记录等 Harness 能力，但普通文件修改和命令执行不要求先创建 Task。

## 权限与恢复模型

项目采用 Workspace-first 权限模型：

- Workspace 内普通文件可以读取、创建、修改、删除和执行。
- Workspace 外允许完整只读：`read_file`、`list_dir`、`list_files`、`search_text`、`view_image`。
- Workspace 外写入、删除和执行会被阻止。
- `.git/**` 永久禁止写入。高风险文件（包括 `.github/**`、`.gitignore`、`Cargo.toml` / `Cargo.lock`、`package.json` / lockfile、`tauri.conf.json`、`vite.config.*`、`pyproject.toml`、README/LICENSE）默认禁止 Patch 写入，可在单个 Workspace 的“设置 → 仓库保护 → 允许 AI 修改高风险文件”中开启。即使开启，删除高风险文件仍需显式确认，命令级危险操作继续受原有策略保护。
- Patch 在单次操作内进行预检和失败恢复；长期恢复统一使用 Git，不创建全量 Workspace Snapshot。

> Windows 子进程目前仍是 `policy_only` 执行边界，返回中的 `sandbox_enforced: false` 是真实状态。静态命令策略不能等同于完整的操作系统文件系统沙箱。

## 本地开发

环境要求：Node.js 20+、Rust stable，以及当前系统的 [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run desktop
```

常用验证命令：

```bash
npm run test:workspace
npm run test:release
npm run check
npm run build
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

`npm run test:workspace` 专门运行真实 Workspace 生命周期回归，包括旧/损坏 Planning、Plan/Goal 门禁、History/Harness 内部状态和真实外部修改检测。`npm run test:release` 是本地发布前的统一 dogfood 入口，会串联版本检查、前端检查/构建和全部 Rust test targets。

Windows 也可以双击 `dev-desktop.cmd`。不要只用 `npm run dev` 验证桌面应用，它只启动 Vite，不会启动 Tauri 外壳。

## 项目结构

| 路径 | 作用 |
| --- | --- |
| `src-tauri/src/tools/` | 统一 Tool 内核、Schema、Policy、文件/Git/Exec/History/Planning/Skill |
| `src-tauri/src/mcp/` | Global MCP Streamable HTTP 服务、OAuth/Review 路由与 `mcp-stdio` 无头桥接 |
| `src-tauri/src/lan_peer/` & `lan_peer.rs` | 跨平台局域网设备发现、6 位配对码握手、HMAC 对等 RPC 与公网隔离 |
| `src-tauri/src/computer/` & `computer.rs` | macOS 原生 Computer Use（ScreenCaptureKit 截图观察、受控输入与权限门控） |
| `src-tauri/src/tool_audit/` & `tool_audit.rs` | 工具调用隐私安全审计、日期分区 JSONL 存储与过期轮转 |
| `src-tauri/src/review/` & `review.rs` | 工作区/会话/跨设备变更快照生成与 Unified/Split Diff 服务 |
| `src-tauri/src/tunnel/` | FRP / Cloudflare 隧道下载、配置与进程监督 |
| `src-tauri/tests/fixtures/` | 当前 Rust 集成测试使用的隔离 fixture |
| `src/` | Vue 3 + Vue Router + Tailwind CSS 4 桌面界面与多主题设计系统 |

## 致谢
感谢 [Linux.do](https://linux.do/) 社区对项目推广与反馈的支持。

## License

[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)
