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

![Coding Tools MCP 工作区总览](docs/images/workspace-overview.png)

*当前版本的工作区概览：集中查看当前分支、全部 Diff、活跃 Session、验证证据、7 天 AI 修改趋势、Git Health、Goal / Plan 进度以及执行质量。*

## v0.3.3：最近的重点改进

0.3.x 的主要变化不是继续增加孤立工具，而是把“多工作区 AI 开发”这条主链路收拢成一个完整工作台：

- **一个 Global MCP 管理所有 Workspace**：从 0.3.0 开始取消“每个项目一套 MCP / Tunnel”的模型，只保留一个 `/mcp` 入口；Chat Session 通过 `workspace_list` / `workspace_select` 选择项目，Workspace 之间仍保持数据和 Planning 隔离。
- **Workspace Session 路由更稳定**：0.3.2 补强了客户端不稳定保留 Transport Session 时的 Workspace 选择保持；需要显式、一次性操作其他项目时可以使用 `workspace_invoke`，不会污染其他 Chat Session。
- **Workspace Overview 变成真正的项目总览**：展示当前分支、一级子 Git、变更文件、活跃 Session、验证证据、最近提交、7 天 AI Activity、Git Health、Goal / Plan 完成度和执行错误；支持一键生成整个 Workspace 的 Diff / Review，并可直接用检测到的 IDE 打开项目。
- **侧边栏直接显示开发状态**：Workspace 项会显示当前 Git 分支、clean / changed 状态、当前 Goal / Plan、完成进度和执行状态；切换工作区采用缓存优先加载，减少重复请求造成的卡顿。
- **Dashboard 从“服务状态页”升级为 AI 工作台**：新增 AI Executions、Verification、Token Flow、Chat → Workspace Session 路由、Planning 完成度、Workspace Token Distribution、最近 History Session 和“需要关注”异常聚合。
- **界面和响应式布局重做**：0.3.3 统一 Apple / iOS 风格设计 token、浅色/深色模式、表单与状态组件，并重新整理 Dashboard、Workspace、Planning 等页面在宽屏和窄屏下的布局。

这意味着现在推荐的使用路径已经非常明确：**连接一次 Global MCP → 选择 Workspace → 维护 Goal / Plan → 执行与验证 → History 留档。**

## 功能全景：特点与优势

Coding Tools MCP 不只是一个 MCP 地址转发器，而是一个以 **Workspace-first** 为核心的 AI 开发运行时：桌面端管理项目和服务，统一工具运行时负责安全执行，并通过 MCP 为 AI 客户端提供开发能力。

| 功能 | 主要特点 | 带来的优势 |
| --- | --- | --- |
| 工作区管理 | 每个项目有独立目录、执行策略、Planning 与 History；侧边栏直接展示分支、变更和当前 Goal / Plan | 多项目切换更清晰，降低把命令执行到错误项目的风险 |
| MCP 工具运行时 | 文件、Patch、命令、Git、图片、Skill 和状态管理共用一套工具内核 | 单一协议入口减少重复状态机和兼容分支，策略、错误和权限更容易保持一致 |
| 连接与公网入口 | 支持本地地址、Global Gateway、FRP 和 Cloudflare Tunnel | 本机开发和远程 ChatGPT 接入可以使用同一工作区，部署方式更灵活 |
| 身份认证 | OAuth Authorization Code、PKCE S256、DCR、Refresh Token，并兼容 Bearer 和静态 Client | 既能提供现代 OAuth 安全流程，也能兼容旧客户端和简单部署 |
| Planning / Goal / Task | Direct、Plan、Goal 三种约束模式，Execution Ledger 统一记录执行状态 | 复杂任务可拆分、可恢复、可验收，减少 AI 在中途偏离目标的情况 |
| 历史会话 | 项目内保存无损 Markdown 档案，并提供有界状态、搜索、分页读取和启动提示词 | 上下文跟着仓库走，不依赖某一个聊天窗口，也不会每次注入全部历史 |
| 日志与健康检查 | 在桌面端查看请求日志、端点、OAuth 元数据和连接检查结果 | 出现连接或授权问题时，可以快速定位在本地服务、隧道还是客户端 |
| Workspace-first 安全边界 | 工作区外默认只读，`.git` 永久保护；`.github`、依赖清单/锁文件、构建配置等高风险文件默认保护，可按 Workspace 显式放开 Patch 写入 | 权限范围明确、修改可控，适合把 AI 接入真实项目而不是临时演示目录 |

### 桌面端全局 Dashboard：不进入工作区也能掌握全局状态

全局 Dashboard 把多个 Workspace 的运行与执行状态汇总到一个控制面板中。它不要求先进入某个项目，就能查看当前 AI Execution、验证证据、Global MCP 状态、Chat Session 路由、Planning 焦点和需要人工关注的异常。

![全局 Dashboard 运行总览](docs/images/dashboard-overview.png)

*从当前执行到 Session 路由集中展示，先判断“AI 正在做什么、是否有阻塞、请求落在哪个 Workspace”。*

Dashboard 还会持续刷新 MCP Token 估算，并提供 Token Flow、请求成功率、Planning 完成度、Workspace Token Distribution、最近 History Session 与系统健康状态。Token 统计来自本地服务请求大小估算，不保存请求正文。

![Dashboard 指标、连接方式、AI Planning 与 Token 用量](docs/images/dashboard-insights.png)

*从执行数量、验证状态、Planning、Session 路由到 Token 趋势，帮助开发者快速判断当前工作负载和待处理事项。*

### 1. 工作区管理：把项目变成长期可识别的开发上下文

工作区是 Coding Tools MCP 的项目上下文单位。它绑定一个本地项目目录，同时维护自己的执行策略、Planning 与 History。MCP Listener、认证和公网入口属于应用级 Global MCP；桌面端可以保存多个项目，而 Chat Session 的 Workspace 选择彼此隔离。

**特点**

- 项目目录是事实来源，工作区名称和项目配置分离维护。
- MCP session 在全局连接内选择 Workspace，规划状态和历史会话仍围绕所选工作区组织。
- 侧边栏直接展示当前分支、clean / changed、当前 Goal / Plan、完成进度和执行状态。
- Workspace Overview 支持主仓库与一级子 Git 状态、7 天修改趋势、执行质量、整个工作区 Diff / Review 和 IDE 快速打开。

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

### 3. 连接与公网入口：从本机调试平滑走向远程开发

整个应用只保留一个 Global MCP Endpoint（`/mcp`）。本地或远程 AI 客户端先建立 MCP session，再用 `workspace_list` / `workspace_select` 选择项目；需要远程访问时，FRP 或 Cloudflare 只暴露这一个入口。

**特点**

- 支持 FRP 和 Cloudflare Tunnel，隧道进程由桌面端统一监督。
- 每个 Chat session 的 Workspace 选择彼此隔离，请求开始时会固定项目上下文。
- 服务停止时，关联的公网 Tunnel 也会一起断开，减少遗留公网进程。

**优势**

- 本地开发、局域网调试和远程 ChatGPT 接入可以使用同一套工作区模型。
- 多个项目不必为每个工作区都单独维护一套公网基础设施。
- 端点、隧道和服务状态集中可见，排查网络问题更直接。

![Global Gateway 共享公网入口](docs/images/global-gateway.png)

*Global MCP 通过一个 `/mcp` 入口承载多个 Workspace；session 内选择项目，FRP 和 Cloudflare 只负责公网传输。*

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

![AI Planning 规划与验收](docs/images/planning-overview.png)

*规划页同时展示当前执行模式、Goal / Plan 状态、待验收数量和 Goal 的验收清单。*

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

![通用设置与 Agent Runtime](docs/images/settings-general.png)

*通用设置把更新入口、WebView 内存管理、运行状态恢复、局域网访问和全局可执行 PATH 放在同一处。*

**特点**

- 工作区内允许按策略读写和执行；工作区外默认只读。
- `.git` 永久不可写；`.github`、依赖清单/锁文件、构建配置、README/LICENSE 等高风险文件默认受保护，可在 Workspace 设置中显式允许 Patch 修改，删除高风险文件仍需确认。
- 健康检查分别报告本地服务、公网入口和认证元数据的状态。
- Windows 当前是 `policy_only` 执行边界，项目不会把静态策略夸大为完整 OS Sandbox。

**优势**

- AI 能力可以接近真实开发流程，同时保持清晰的最小权限范围。
- 连接失败、授权失败和工具调用失败有明确的排查入口。
- 对安全能力的边界如实呈现，方便团队根据部署环境补充系统级隔离。

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

第一次使用只需要记住两件事：**桌面端负责把项目变成 MCP 工作区，ChatGPT 负责通过公网 `/mcp` 地址连接它。**

- [查看完整安装和桌面端启动步骤](#五分钟开始使用)
- [直接查看 ChatGPT 插件配置](#mcp-connector)

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

### 3. 配置公网隧道

如果 AI 客户端不在本机，需要把本地 MCP 暴露为 HTTPS 地址：

- 打开“设置 → Global MCP 连接”，确认唯一 MCP 本地端口和公网方式。
- **零门槛推荐：Cloudflare Quick Tunnel**。选择 Cloudflare 快速模式，无需账号即可生成 `https://<随机>.trycloudflare.com` 公网地址，适合首次接入和临时演示；长期使用建议 FRP 固定子域名或外部反向代理。
- 在“软件管理”中安装或识别 `frpc` / `cloudflared`。
- 在“FRP 配置”中保存服务器、端口和 Token，再回到 Global MCP 连接选择 FRP 并填写唯一子域名。

![FRP 配置页面](docs/images/frp-configuration.png)

*FRP 服务器配置集中保存，Global MCP 连接只需选择配置并填写公网子域名。*

如果还没有可用的 FRPS 服务端，可以参考：[FRPS 服务端安装教程（微信公众号）](https://mp.weixin.qq.com/s/kmpQhHsvmHlaLfj4rw3A0Q)。安装完成后，把服务端地址、端口和 Token 填入客户端的“FRP 配置”即可。

### 4. 启动 MCP

在 Dashboard 或“设置 → Global MCP 连接”点击“启动 Global MCP”。客户端会显示：

- 唯一本地 MCP 地址，例如 `http://127.0.0.1:28765/mcp`；
- 公网 HTTPS MCP 地址；
- ChatGPT 连接所需的认证信息；
- 实时日志和健康检查结果。

![MCP 本地、公网与 ChatGPT 连接信息](docs/images/workspace-connection.png)

启动后可以直接检查本地与公网端点、OAuth 元数据和 MCP 受保护资源：

![MCP 健康检查结果](docs/images/health-check.png)

*健康检查会逐项显示连接和认证元数据是否可用。*

遇到连接问题时，无需离开桌面端即可查看最近的 MCP 请求日志：

![MCP 运行日志](docs/images/runtime-logs.png)

*日志可快速确认工具列表、历史初始化和检查点调用是否真正到达服务端。*

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

![ChatGPT 新会话启动提示词](docs/images/history-session-prompt.png)

*当前会话默认记录；旧会话通过工作区的“历史上下文”面板选择后，才会以有界快照注入。*

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
| `src-tauri/src/tools/` | 文件、Patch、Exec、Git 等共享工具内核 |
| `src-tauri/src/mcp/` | MCP Streamable HTTP 服务 |
| `src-tauri/src/tunnel/` | FRP / Cloudflare 隧道和进程管理 |
| `src-tauri/tests/fixtures/` | 当前 Rust 集成测试使用的隔离 fixture |
| `src/` | Vue 3 + Tailwind CSS 4 桌面界面 |

## 致谢
感谢 [Linux.do](https://linux.do/) 社区对项目推广与反馈的支持。

## License

[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)
