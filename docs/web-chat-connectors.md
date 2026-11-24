# 网页 Chat 客户端接入矩阵

本文档覆盖支持远程 MCP Connector 的网页版 Chat 客户端：ChatGPT、claude.ai、Grok、Gemini，以及任意通用 MCP Connector。应用通过一个 Global MCP 入口提供本机与已授权对等设备的 Workspace；各网页客户端的公网配置差异集中在本文。桌面、CLI 或自研 Harness 的本地接入见 [通用 Harness 指南](./harness-connectors.md)。

> 各客户端的 Connector 能力（是否支持动态注册、是否允许自定义 OAuth 端点）会随产品迭代变化。遇到字段缺失时，以客户端当前界面为准，并优先选择 Bearer 这类最简认证兜底。

## 前置条件（所有客户端通用）

1. 入口设备已启动 Global MCP，并注册至少一个 Workspace。
2. 公网隧道处于运行状态，且"健康检查"中的公网 MCP 检查通过。
3. 使用 OAuth 时，OAuth 受保护资源与授权元数据检查通过。

**零门槛推荐：Cloudflare Quick Tunnel。** 在应用级 Global MCP 隧道配置中选择 Cloudflare 快速模式（无需 Cloudflare 账号），应用会启动 `cloudflared` 并生成一个 `https://<随机>.trycloudflare.com` 地址。适合首次接入和临时演示；长期使用建议 FRP 固定子域名或 Cloudflare 命名隧道，地址稳定且可绑定自有域名。

从桌面端"GPT 配置"卡片可复制：

| 字段 | 说明 |
| --- | --- |
| 公网 MCP 地址 | 以 `/mcp` 结尾的 HTTPS URL，所有 Connector 都填这个 |
| OAuth Client ID / Client Secret | 静态客户端凭据，仅当客户端不支持动态注册（DCR）时需要 |
| 授权口令 | OAuth 授权页面输入的口令 |
| Bearer Token | Global MCP 认证选择 Bearer Token 时使用 |

服务端支持 OAuth Authorization Code + PKCE S256、Dynamic Client Registration（`/register`）和 Refresh Token；也支持 Bearer 静态 Token。认证方式必须与Global MCP 配置一致。

## ChatGPT（MCP Connector）

完整步骤见 [README](../README.md#mcp-connector)，要点：

1. ChatGPT 设置 → 账户安全与登录 → 开启开发人员模式。
2. 插件 → 新建 MCP 插件 → 粘贴公网 MCP 地址。
3. 身份验证：支持 DCR 的客户端直接走 OAuth 授权；仅支持静态 OAuth 时填写桌面端提供的 Client ID / Secret。授权页面输入授权口令。
4. 新建对话验证连接。

## claude.ai（Connectors）

1. claude.ai → Settings → Connector settings → Add custom connector。
2. URL 填写公网 MCP 地址（`https://…/mcp`）。
3. 认证：claude.ai 的自定义 Connector 走 OAuth 授权流程。若当前版本不支持动态客户端注册，使用桌面端的静态 Client ID / Client Secret 完成授权；授权页面输入授权口令。若界面不提供 OAuth 选项，将Global MCP 认证切换为 Bearer Token 并在 Connector 的认证头中使用 Bearer Token。
4. 在对话中启用该 Connector 后验证。

注意：claude.ai Connector 对授权回调和 metadata 的要求较严格，务必确认公网地址可被 claude.ai 服务端访问（健康检查的公网 MCP 项必须通过）。

## Grok

1. grok.com 的开发者/连接器设置中新增 MCP 服务器（入口名称随版本变化，寻找 "MCP" 或 "Connectors"）。
2. 填入公网 MCP 地址，认证方式与 ChatGPT 相同：优先 OAuth（DCR 或静态客户端），不支持时改用 Bearer。

## Gemini

Coding Tools MCP 的 **Global MCP** 可同时提供本机 Streamable HTTP 和公网 HTTPS 连接，不必为 Gemini 再启动一套服务。针对提供「Local MCP / Cloud MCP」选项的 Gemini 界面，桌面端「应用设置 → 连接 → Gemini App · MCP 连接」支持两种配置。**Google 官方面向 Gemini 网页版的公开文档主要说明 Cloud / HTTPS Custom apps**；Local MCP 命令只有在 Gemini 所在客户端确实能在用户电脑启动本地进程时才可使用。

### Local MCP（同一台 Mac，无需公网 MCP 隧道）

1. 在 Coding Tools MCP 启动 Global MCP，本地地址应显示为 `http://127.0.0.1:<端口>/mcp`；认证优先选 OAuth。
2. 在 Coding Tools MCP 的 Gemini 面板选择「Local MCP · 本地」，点击「生成凭据并复制」（或「复制命令」）。桌面端会自动创建无空格原生硬链接入口 `~/.local/bin/coding-tools-mcp`（避免 symlink 解析到 `/Applications` 被 Gemini 沙盒拦截），并在 OAuth 模式下生成权限为 `0600` 的本机凭据 `~/.local/share/coding-tools-mcp/gemini-stdio-auth.json`。示例命令：

   ```sh
   ~/.local/bin/coding-tools-mcp mcp-stdio http://127.0.0.1:28765/mcp
   ```

   该命令使用内置原生 Rust `mcp-stdio` 子命令直接转发 stdio JSON-RPC 到本机 HTTP `/mcp`，不依赖 Node.js / `npx`，也不会在 Gemini 的 ~5 秒启动超时内卡在交互式浏览器 OAuth 流程上。命令本身不包含任何密钥。
3. 在 Gemini 自定义关联应用中选择 **Local MCP**，填写名称 `Coding Tools MCP`，说明可填写 `本地工作区管理与开发工具`，将复制的整行命令填入 Gemini「命令」输入框，保存后调用 `workspace_list` 验证。
4. 如果 Global MCP 认证为 Bearer，面板为了避免把密钥暴露在命令行而不自动提供可复制命令；保持现有远程认证不变，可切换到 OAuth、无认证（仅可信本机），或改用 Cloud MCP。

**Local MCP 导入提示 `MCP.MCPError code=4` 排查**：
- Gemini 默认沙盒已允许读取 `~/.local`，因此原生桥接入口与 `~/.local/share/coding-tools-mcp/gemini-stdio-auth.json` 无需放行 `~/.mcp-auth`。若仍报 `code=4`，先检查 Coding Tools MCP 中 **Global MCP 是否已启动**，以及是否已重新构建/安装包含 `mcp-stdio` 子命令的最新版应用。
- 若使用自定义沙盒限制了网络或 `~/.local`，在 Gemini 的「沙盒设置」中允许读取 `~/.local` 与访问回环网络 `127.0.0.1` 即可，不要关闭整个沙盒或 `chmod 777`。

Local MCP 的调用流是 `Gemini → 原生 mcp-stdio 桥接进程 → 127.0.0.1:<端口>/mcp → Global MCP → Workspace`；**不需要启用 FRP 或 Cloudflare**。若 Gemini 运行在网页沙盒或远程服务器上、不能启动本地程序，这个命令无法连接到你 Mac 的 loopback 地址；应改用 Cloud MCP。

### Cloud MCP（Gemini 网页版 Custom apps）

**适用资格**：该功能目前面向美国地区、18 岁及以上、使用个人 Google 账号的用户，要求 Gemini 为英语界面、开启 Keep Activity。资格与开通状态由 Google 控制；桌面端准备就绪不等于 Google 账号已被授权。

1. 在 Coding Tools MCP 的「应用设置 → 连接」中将认证模式设为 **OAuth**，启动/重启 Global MCP。服务端已经支持 OAuth Authorization Code、PKCE S256、动态客户端注册（DCR）和 Refresh Token。
2. 在「MCP 端点与公网连接」开启 Cloudflare / FRP / 外部 HTTPS 代理。确认「Global MCP 公网入口」健康检查通过，并复制 Gemini 专用面板中的 `https://<公网域名>/mcp` 地址。勿使用局域网 IP、`http://localhost`、带账号密码或 Token 的 URL。
3. 用符合资格的个人 Google 账号打开 [Gemini](https://gemini.google.com/)，进入 **Settings → Connected Apps → Custom apps → Add custom app**，添加 MCP Server URL。
4. 首选 Google 的 **动态客户端注册 DCR + OAuth**，按照浏览器授权流程在本地 MCP 授权页面输入口令；如果界面仅支持手动 OAuth 客户端凭据，可到「应用设置 → 认证密钥」查看静态 Client ID/Secret，切勿把 Secret 放进 URL 或提交代码。
5. 在 Gemini 对话里启用/选择刚创建的应用，发送：`请调用 workspace_list 列出工作区，不要猜测工作区路径。` 完成连通测试。同一 Google 账号登录的 Gemini 移动 App 可使用已连接应用，但创建和管理连接建议在网页端进行。

高级诊断可打开 [Gemini Developer Mode](https://gemini.google.com/devmode)，检查 MCP tool schema、授权失败和请求日志。若提示无法添加自定义应用，首先检查账号资格和语言设置，而不是关闭 MCP 安全策略；若授权失败，检查服务端 OAuth metadata、DCR 和公网 URL 是否一致。Cloudflare Quick Tunnel 的地址在重启后可能变化，需要重新更新 Gemini 中的 MCP URL。

参考：[Google Gemini 自定义应用说明](https://support.google.com/gemini/answer/17209137)、[Google Developer Mode](https://developers.google.com/connected-apps/mcp/dev-mode/usage)。

## 通用 Connector（任意支持远程 MCP 的客户端）

| 配置项 | 取值 |
| --- | --- |
| Transport | Streamable HTTP |
| Endpoint | 公网 MCP 地址（`/mcp` 结尾） |
| 协议版本 | 服务端支持 `2024-11-05`、`2025-03-26`、`2025-06-18`、`2025-11-25`，按 MCP 规则自动协商 |
| 认证 | OAuth 2.1（Authorization Code + PKCE S256，支持 DCR `/register`）或 Bearer |
| 会话 | 支持持久 MCP Session；不保留 Session 的客户端须在项目工具调用中显式传入 `workspace_id` |

元数据发现地址：

```text
<公网地址>/.well-known/oauth-authorization-server
<公网地址>/.well-known/oauth-protected-resource
```

## 验证提示词

连接完成后，在启用该 Connector 的新对话中发送：

```text
请先调用 workspace_list，列出已注册的工作区及所属设备。
选择我指定的工作区后，在 server_info、get_default_cwd 和 git_status 中显式传入 workspace_id，
告诉我目标设备、默认目录和 Git 状态。
```

能返回目标 Workspace 信息即表示该工作区的调用链路打通。若仅 workspace_list 成功，不代表远程命令执行已验证。

## 常见失败排查

| 现象 | 优先检查 |
| --- | --- |
| 客户端提示无法连接 | 公网 MCP 健康检查是否通过；地址是否为 HTTPS 且以 `/mcp` 结尾；隧道是否仍在运行（Quick Tunnel 重启后地址会变） |
| OAuth 授权页无法打开或回调失败 | `/.well-known/oauth-authorization-server` 是否可公网访问；Client 配置是否来自同一个 Global MCP 入口 |
| 授权成功但工具列表为空 | 断开并重新连接 Connector，或新建对话；部分客户端会缓存旧工具列表 |
| 401 / invalid_client | 客户端不支持 DCR 却未填静态 Client ID / Secret；或 Bearer Token 与工作区配置不一致 |
| 工具调用超时 | 桌面端"日志"确认请求是否到达；命令类工具默认 30s 超时，可传 `timeout_ms` |
