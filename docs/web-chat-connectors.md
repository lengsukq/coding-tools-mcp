# 网页 Chat 客户端接入矩阵

本文档覆盖支持远程 MCP Connector 的网页版 Chat 客户端：ChatGPT、claude.ai、Grok、Gemini，以及任意通用 MCP Connector。桌面端只负责把 Workspace 变成公网可达的 MCP 服务，各客户端的配置差异集中在本文。

> 各客户端的 Connector 能力（是否支持动态注册、是否允许自定义 OAuth 端点）会随产品迭代变化。遇到字段缺失时，以客户端当前界面为准，并优先选择 Bearer 这类最简认证兜底。

## 前置条件（所有客户端通用）

1. 桌面端已启动该 Workspace 的 MCP 服务。
2. 公网隧道处于运行状态，且"健康检查"中的公网 MCP 检查通过。
3. 使用 OAuth 时，OAuth 受保护资源与授权元数据检查通过。

**零门槛推荐：Cloudflare Quick Tunnel。** 在 Workspace 的隧道配置中选择 Cloudflare 快速模式（无需 Cloudflare 账号），应用会启动 `cloudflared` 并生成一个 `https://<随机>.trycloudflare.com` 地址。适合首次接入和临时演示；长期使用建议 FRP 固定子域名或 Cloudflare 命名隧道，地址稳定且可绑定自有域名。

从桌面端"GPT 配置"卡片可复制：

| 字段 | 说明 |
| --- | --- |
| 公网 MCP 地址 | 以 `/mcp` 结尾的 HTTPS URL，所有 Connector 都填这个 |
| OAuth Client ID / Client Secret | 静态客户端凭据，仅当客户端不支持动态注册（DCR）时需要 |
| 授权口令 | OAuth 授权页面输入的口令 |
| Bearer Token | 工作区认证选择 Bearer Token 时使用 |

服务端支持 OAuth Authorization Code + PKCE S256、Dynamic Client Registration（`/register`）和 Refresh Token；也支持 Bearer 静态 Token。认证方式必须与工作区配置一致。

## ChatGPT（MCP Connector）

完整步骤见 [README](../README.md#mcp-connector)，要点：

1. ChatGPT 设置 → 账户安全与登录 → 开启开发人员模式。
2. 插件 → 新建 MCP 插件 → 粘贴公网 MCP 地址。
3. 身份验证：支持 DCR 的客户端直接走 OAuth 授权；仅支持静态 OAuth 时填写桌面端提供的 Client ID / Secret。授权页面输入授权口令。
4. 新建对话验证连接。

## claude.ai（Connectors）

1. claude.ai → Settings → Connector settings → Add custom connector。
2. URL 填写公网 MCP 地址（`https://…/mcp`）。
3. 认证：claude.ai 的自定义 Connector 走 OAuth 授权流程。若当前版本不支持动态客户端注册，使用桌面端的静态 Client ID / Client Secret 完成授权；授权页面输入授权口令。若界面不提供 OAuth 选项，将工作区认证切换为 Bearer Token 并在 Connector 的认证头中使用 Bearer Token。
4. 在对话中启用该 Connector 后验证。

注意：claude.ai Connector 对授权回调和 metadata 的要求较严格，务必确认公网地址可被 claude.ai 服务端访问（健康检查的公网 MCP 项必须通过）。

## Grok

1. grok.com 的开发者/连接器设置中新增 MCP 服务器（入口名称随版本变化，寻找 "MCP" 或 "Connectors"）。
2. 填入公网 MCP 地址，认证方式与 ChatGPT 相同：优先 OAuth（DCR 或静态客户端），不支持时改用 Bearer。

## Gemini

Google Gemini Apps 的 **Custom apps（自定义应用）** 可以接入远程 MCP Streamable HTTP 服务。Coding Tools MCP 的 Global MCP 可以直接复用，不需要另起一套 Gemini 专用服务。

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
| 会话 | 无状态，不要求 `Mcp-Session-Id` |

元数据发现地址：

```text
<公网地址>/.well-known/oauth-authorization-server
<公网地址>/.well-known/oauth-protected-resource
```

## 验证提示词

连接完成后，在启用该 Connector 的新对话中发送：

```text
请使用 Coding Tools MCP 调用 server_info、get_default_cwd 和 git_status，
告诉我当前连接的工作区、默认目录和 Git 状态。
```

能返回当前 Workspace 信息即表示链路打通。

## 常见失败排查

| 现象 | 优先检查 |
| --- | --- |
| 客户端提示无法连接 | 公网 MCP 健康检查是否通过；地址是否为 HTTPS 且以 `/mcp` 结尾；隧道是否仍在运行（Quick Tunnel 重启后地址会变） |
| OAuth 授权页无法打开或回调失败 | `/.well-known/oauth-authorization-server` 是否可公网访问；Client 配置是否来自同一个工作区 |
| 授权成功但工具列表为空 | 断开并重新连接 Connector，或新建对话；部分客户端会缓存旧工具列表 |
| 401 / invalid_client | 客户端不支持 DCR 却未填静态 Client ID / Secret；或 Bearer Token 与工作区配置不一致 |
| 工具调用超时 | 桌面端"日志"确认请求是否到达；命令类工具默认 30s 超时，可传 `timeout_ms` |
