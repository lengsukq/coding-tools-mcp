# 局域网对等配对与多设备联邦网关设计规格（LAN Peer Federation）

- 状态：Implemented Draft
- 适用平台：macOS、Windows、Linux
- 关联模块：`src-tauri/src/lan_peer`、`src-tauri/src/mcp`、`src-tauri/src/computer`、`src-tauri/src/tool_audit`、`src/lib/lan`、`src/components/settings`

## 1. 背景与目标

在拥有多台局域网电脑（例如 MacBook Pro + Mac mini + Windows 台式机）的开发场景下，用户希望：

1. **对等双向配对**：局域网内任意两台运行 Coding Tools MCP 的电脑完成一次配对后自动建立双向互信，不存在单点主从限制；
2. **单一入口适配全网**：AI 客户端（本地 `mcp-stdio`、Streamable HTTP `/mcp` 或 Cloudflare / FRP 公网隧道）只需连接局域网内**任意一台电脑**，即可透明访问所有已配对在线电脑上的工作区与主机能力；
3. **严格的设备区分与工作区隔离**：解决多机同名仓库、相同目录路径、跨平台（macOS / Windows）环境变量差异、会话串线及屏幕控制目标歧义问题。

## 2. 跨平台发现与双向 PIN 配对协议

### 2.1 持久设备身份（`DeviceIdentity`）

- 首次启动自动生成持久 `device_id`（格式 `dev_<12hex>`，与局域网 DHCP IP 变化解耦）及 256-bit 节点主密钥（存于 `app_secrets["lan_federation"]["node_secret"]`）。
- 派生公开设备指纹 `fingerprint`（`ABCD-EF01-2345-6789`），默认设备名取系统主机名（Windows `COMPUTERNAME` / macOS `gethostname`），支持用户在 UI 自定义别名（如 `MacBook Pro`、`桌面 Mac mini`、`Windows 编译机`）。

### 2.2 零外部依赖跨平台局域网发现（macOS & Windows）

为避免在 Windows 上依赖未安装的 Apple Bonjour 服务或引入 C/C++ 编译链，采用纯 Rust 跨平台双通道发现：

1. **UDP 多播 + 广播信标（端口 `28767`）**：
   - 同时加入 IPv4 本地管理多播组 `239.255.77.67:28767` 并向子网定向广播（`x.y.z.255:28767`）及受限广播（`255.255.255.255:28767`）周期发送只读节点摘要（`protocol: "ctm-lan-v1"`、`device_id`、`device_name`、`os`、`arch`、`version`、`mcp_port`、`fingerprint`）。
   - macOS/Unix 通过 `libc::getifaddrs` 枚举非回环物理网卡 IPv4；Windows 通过主机名 `getaddrinfo` 与无包 UDP 路由表探测枚举所有私有网段适配器 IPv4，兼容 WSL2 / Hyper-V 多虚拟网卡环境。
2. **HTTP 直连探测兜底（`GET /api/lan/v1/hello`）**：
   - 当路由器开启 AP 隔离或 Windows 防火墙拦截 UDP 广播时，支持手动输入 `IP:Port` 直接通过 Global MCP 端口完成握手探测。

### 2.3 一次性 6 位配对码双向握手

1. **生成配对码**：任意一端（设为节点 B）点击“生成配对码”，在内存中生成 120 秒有效、最多允许 5 次尝试的 6 位数字配对码 `PIN`。
2. **发起配对**：另一端（节点 A）选择节点 B 并输入 `PIN`，生成 256-bit 随机盐 `secret_part_a`，向 `POST http://{B_ip}:{B_port}/api/lan/v1/pair/complete` 发送 A 的身份信息与 `HMAC-SHA256(key=PIN, ...)` 证明（不传输明文 PIN）。
3. **双向落盘**：节点 B 恒定时间验签通过后，立即销毁一次性 `PIN`，生成 256-bit 随机盐 `secret_part_b`，双方共同派生对称会话根密钥 `peer_secret = SHA256("ctm-peer-secret-v1|" + secret_part_a + "|" + secret_part_b)` 并各自写入本机 `paired_peers` 与 `app_secrets`。一次操作即完成双向对等互信。

## 3. 安全传输与公网隧道隔离

- 所有对等 RPC 复用 Global MCP 监听端口下的 `/api/lan/v1/*` 路由，避免在 Windows / macOS 触发多端口防火墙弹窗。
- **公网隧道硬隔离**：若请求携带任何反向代理或公网隧道特征头（`cf-connecting-ip`、`cf-ray`、`x-forwarded-for`、`x-forwarded-host`、`x-real-ip`、`forwarded`）或来源 Socket 非本机回环/RFC1918/CGNAT 局域网地址，服务端直接以 `403 Forbidden` 拒绝 `/api/lan/v1/*`，确保开启 Cloudflare / FRP 公网入口绝不暴露局域网配对与内部联邦接口。
- **防重放与防环路**：每个对等请求携带 `X-CTM-Peer-Id`、`X-CTM-Timestamp`（允许 ±60s 时钟偏移）、`X-CTM-Nonce`（120s 去重窗口）、`X-CTM-Hop: 1` 与 `X-CTM-Signature`（`HMAC-SHA256` 覆盖 HTTP 方法、路径、时间戳、Nonce 与 Body SHA-256）。服务端拒绝任何 `Hop > 1` 的请求，严格保证单跳拓扑无环。

## 4. 设备区分、工作区命名空间与目标机主权

### 4.1 复合工作区标识符（`{device_id}:{workspace_id}`）

- **本机工作区**：`workspace_list` 保持返回原 `id`（同时在解析层兼容 `local:{id}` 与 `{self_device_id}:{id}`），确保现有配置、测试与脚本 100% 向后兼容。
- **远端配对工作区**：统一编码为 `{peer_device_id}:{remote_workspace_id}`，并在 `WorkspaceDescriptor` 中附带：
  - `deviceId`、`deviceName`、`isLocal`、`os`、`online`、`accessMode`（`full` / `readonly`）与 `qualifiedName`（例如 `[远端 · Mac mini] coding-tools-mcp`）。
- **单工作区自动选择门禁**：仅当“本机工作区 + 已启用配对设备暴露的工作区”总数恰好为 `1` 时才允许省略 `workspace_id` 自动命中；只要全网存在多个工作区，未指定作用域时均 fail-closed 返回 `WORKSPACE_NOT_SELECTED`。

### 4.2 主机级工具与跨设备 Computer Use

- 新增网关只读工具 `device_list`，返回本机与所有已配对设备的在线状态、OS/架构、延迟（RTT）、暴露工作区数及 `computerUse` 状态。
- `computer_session`、`computer_observe`、`computer_action` 新增可选 `device_id` 参数：
  - 指定远端 `device_id` 时透明转发至目标 Mac，且**目标 Mac 仍强制执行其本机 TCC 权限、本机桌面开关与高危动作 60 秒单次确认**；
  - 未指定 `device_id` 但存在多台已启用 Computer Use 的设备（或本机为 Windows 而远端 Mac 已开启 Computer Use）时，返回明确的设备消歧提示。

### 4.3 目标机主权、跨机变更审查与双向审计

- **目标机主权**：远端执行完全使用目标电脑自身的 `Workspace` 根目录校验、`RuntimeConfig` 命令白名单、`executable_paths`（macOS 与 Windows 各自独立）、Planning Gate 与 `SecretStore`。
- **变更审查代理**：远端工具产生 `change_id` 时，入口机生成 `{entry_base}/review/peer/{device_id}/{change_id}?t={token}` 代理链接，通过单条公网隧道即可查看全网任意电脑的 Diff 快照。
- **双向工具审计**：`ToolAuditRecord` 新增可选 `executionHost` 与 `sourcePeer` 字段，入口机与目标机各自记录完整的跨设备调用轨迹。
