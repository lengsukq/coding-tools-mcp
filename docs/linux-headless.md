# Linux 无头模式与服务器部署指南

Coding Tools MCP 在 Linux 上提供两种运行形态：

| 形态 | 适用场景 | 依赖要求 | 安装方式 |
| --- | --- | --- | --- |
| **Linux 无头模式（Headless CLI / Daemon）** | 云服务器、家庭实验室（NAS / Home Lab）、SSH 开发机、CI/容器宿主机 | **零 GTK / 零 WebKit / 零 X11 依赖**，单文件独立二进制 `coding-tools-mcp` | 一键安装脚本 / Release `tar.gz` / 源码编译 |
| **Linux 桌面端（Desktop GUI）** | Ubuntu / Debian 等带桌面环境的开发工作站 | `libwebkit2gtk-4.1-0`、`libgtk-3-0`、`libayatana-appindicator3-1` | Release `.deb` / `.AppImage` |

无头守护进程与桌面端共用同一套 Rust 工具内核、Global MCP 网关、局域网/组网联邦协议（HMAC-SHA256 + 6 位配对码）以及配置文件格式。你可以将多台 Linux 服务器无头常驻，然后通过你手头的 macOS / Windows 桌面端（或唯一公网 MCP 入口）在一个 AI 对话中统一调度它们上面的全部 Git 项目。

---

## 一、一键安装（推荐）

支持 `x86_64` 与 `aarch64`（ARM64）架构的 Linux 服务器。

### 1. 仅安装命令行与无头二进制

```bash
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash
```

- 普通用户默认安装到 `~/.local/bin/coding-tools-mcp`（若 `/usr/local/bin` 可写则装到 `/usr/local/bin`）。
- `root` / `sudo` 默认安装到 `/usr/local/bin/coding-tools-mcp`。

### 2. 一键安装 + systemd 常驻 + 自动纳管代码目录 + 输出配对码

**普通用户（用户级 `systemd --user`）：**

```bash
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
  bash -s -- --service --lan --device-name dev-linux --auto-root ~/code --pair-pin

# 如需在退出 SSH 或重启无人登录时继续常驻，开启 linger：
sudo loginctl enable-linger "$USER"
```

**Root 系统级常驻（`/etc/systemd/system/coding-tools-mcp.service`，开机自动拉起，无需 linger）：**

```bash
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
  sudo bash -s -- --service --lan --device-name prod-box --auto-root /srv/code --pair-pin
```

### 3. 国内网络加速 / 源码编译 / 离线安装

```bash
# 使用 GitHub 镜像加速
curl -fsSL https://gh-proxy.com/https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
  bash -s -- --mirror https://gh-proxy.com/ --service --lan

# 直接从最新 main 源码编译无 UI 二进制（无需 GTK/WebKit，仅需 git + Rust cargo）
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
  bash -s -- --from-source --service --lan

# 离线安装包（从 GitHub Releases 下载 coding-tools-mcp-v*-linux-*-headless.tar.gz 后传到服务器）
bash scripts/install-linux.sh --archive ./coding-tools-mcp-v0.3.9-linux-x86_64-headless.tar.gz --service --lan
```

### 4. 平滑升级与卸载

```bash
# 再次运行安装脚本即可原子替换二进制，若 systemd 服务正在运行会自动平滑重启
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash

# 卸载 systemd 服务与二进制（默认保留 ~/.config/coding-tools-mcp 下的配置与配对数据）
curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash -s -- --uninstall
```

### 一键脚本参数速查

| 参数 | 说明 |
| --- | --- |
| `--version <TAG>` | 指定安装版本（如 `v0.3.9`）；默认 `latest` 自动匹配含 Linux 无头资产的最新 Release |
| `--install-dir <DIR>` | 自定义二进制安装目录 |
| `--mirror <URL>` | 指定 GitHub 下载镜像前缀（也可用 `GITHUB_MIRROR` 环境变量） |
| `--service` | 安装后自动执行 `coding-tools-mcp service install` 并启动 |
| `--lan` | 开启局域网 / EasyTier / Tailscale 跨设备联邦 |
| `--device-name <NAME>` | 设置本机在多设备联邦中显示的设备名称 |
| `--port <PORT>` | 设置 Global MCP 监听端口（默认 `28765`） |
| `--auto-root <DIR>` | 注册自动发现父目录（可重复传入），自动纳管其下一级 `.git` 仓库 |
| `--pair-pin` | 配合 `--service` 使用，服务就绪后自动输出 6 位一次性配对码 |
| `--from-source` | 从源码编译零 GTK 二进制（`cargo build --release --locked --no-default-features --bin coding-tools-mcp`） |
| `--ref <BRANCH\|TAG>` | 配合 `--from-source` 指定分支或标签（默认 `main`） |
| `--archive <FILE>` | 使用本地 `.tar.gz` 或裸二进制离线安装 |
| `--no-restart` | 覆盖升级时不自动重启正在运行的 systemd 服务 |
| `--uninstall` | 卸载 systemd 服务并删除二进制文件 |

---

## 二、3 分钟完成与桌面端跨机联邦配对

以下以 **Linux 服务器（无头） + macOS / Windows 桌面端（主入口）** 为例：

### 第 1 步：在 Linux 上纳管项目目录

```bash
# 方式 A：自动发现整个代码父目录下的一级 .git 仓库（推荐，新增仓库随时可见）
coding-tools-mcp workspace auto-root add ~/code

# 方式 B：手动注册单个项目目录
coding-tools-mcp workspace add /srv/my-app --name my-app

# 查看已纳管工作区
coding-tools-mcp workspace list
```

### 第 2 步：启动守护进程并生成 6 位配对码

```bash
# 如果已安装 systemd 常驻服务：
coding-tools-mcp lan pin

# 如果是前台临时测试启动：
coding-tools-mcp serve --lan --pair-pin
```

终端会打印本机设备名、可连接 IP:端口（自动过滤 `docker0` / `br-*` / `veth*` / `cni*` 等容器虚拟网桥）以及 2 分钟有效的 6 位一次性配对码。

### 第 3 步：在 Mac / Windows 桌面端输入配对码

1. 打开桌面端 **「应用设置 → MCP 连接 → 局域网多设备配对」**，开启局域网联邦。
2. 在发现列表中点击该 Linux 节点（或在跨网段时输入 Linux 的局域网 / EasyTier / Tailscale IP 与端口），填入 6 位配对码完成双向加密握手。
3. 回到已连接主设备的 AI 客户端（ChatGPT / Claude / Gemini），即可调用：
   - `device_list` 查看所有已配对设备；
   - `workspace_list` 列出本机与 Linux 节点上的工作区（形如 `{device_id}:{workspace_id}`）；
   - `workspace_select` 选中 Linux 节点上的项目直接读代码、改 Patch、跑命令与看 Git！

> 也可以在 Linux 端主动发起配对：先在桌面端点击「生成配对码」，再在 Linux 终端执行：
> ```bash
> coding-tools-mcp lan pair 192.168.1.10:28765 123456
> ```

---

## 三、完整 CLI 命令参考

### 守护进程生命周期

```bash
coding-tools-mcp serve [选项]     # 前台运行无头守护进程
  --port <PORT>                   # 覆盖本地监听端口（默认 28765）
  --lan                           # 开启局域网 / 虚拟组网联邦与发现
  --allow-lan-mcp                 # 允许局域网未配对客户端直连 /mcp（默认关闭）
  --pair-pin                      # 启动后立即打印 6 位配对码（自动开启 --lan）
  --auto-root <DIR>               # 注册父目录自动发现并同步扫描

coding-tools-mcp status           # 查看守护进程 PID、端口、工作区/会话数与组网 IP
coding-tools-mcp reload           # 通过本地 Unix Socket 通知守护进程热重载配置
coding-tools-mcp stop             # 优雅停止后台守护进程
```

### 工作区管理

```bash
coding-tools-mcp workspace list                        # 列出已注册与自动发现的工作区（简写: ws）
coding-tools-mcp workspace add <DIR> [--name <NAME>]   # 注册指定目录（幂等去重，自动通知热更新）
coding-tools-mcp workspace remove <ID|NAME|DIR>        # 移除工作区并清理会话
coding-tools-mcp workspace auto-root list              # 查看自动发现父目录
coding-tools-mcp workspace auto-root add <DIR>         # 添加父目录并扫描一级 .git 仓库
coding-tools-mcp workspace auto-root remove <DIR>      # 移除自动发现父目录
coding-tools-mcp workspace scan                        # 立即重新扫描并持久化新仓库
```

> 守护进程已支持 `profiles.json` 跨进程变更自动刷新内存缓存，通过 CLI 增删工作区后即使不手动重启服务也能立即生效。

### 局域网与组网联邦

```bash
coding-tools-mcp lan status                     # 查看本机设备 ID、指纹、组网 IP 与已配对节点在线状态
coding-tools-mcp lan enable | disable           # 开关联邦能力
coding-tools-mcp lan pin                        # 向运行中的守护进程申请 6 位配对码
coding-tools-mcp lan cancel-pin                 # 撤销当前活跃配对码
coding-tools-mcp lan scan                       # 扫描同局域网与 extra-subnets 内的在线节点
coding-tools-mcp lan pair <HOST[:PORT]> <PIN>   # 使用对方 6 位配对码完成双向加密配对
coding-tools-mcp lan unpair <DEVICE_ID>         # 解除配对并通知对方吊销
```

### 全局配置与 systemd 管理

```bash
coding-tools-mcp config show                    # 查看当前全部配置项
coding-tools-mcp config set <KEY> <VALUE>       # 修改配置并热通知守护进程
# 支持的 KEY：
#   port | auth (oauth/bearer/noauth) | server-name | device-name |
#   allow-local-noauth | allow-lan-mcp | lan-trust (full/readonly) |
#   extra-subnets (如 10.144.144.0/24 或 none) | proxy-mode (none/system/manual) | proxy-url

coding-tools-mcp service install                # 生成并启用 systemd 服务（root 为系统级，普通用户为用户级）
coding-tools-mcp service status                 # 查看单元文件路径与 active 状态
coding-tools-mcp service print                  # 仅预览生成的 systemd unit 内容
coding-tools-mcp service uninstall              # 停用并删除 systemd 单元文件

coding-tools-mcp mcp-stdio http://127.0.0.1:28765/mcp  # 启动本地 MCP stdio-to-HTTP 桥接
```

---

## 四、systemd 常驻、跨网段与排障指南

### 1. 查看后台日志

- **root 系统级服务**：
  ```bash
  sudo systemctl status coding-tools-mcp.service
  sudo journalctl -u coding-tools-mcp.service -f
  ```
- **普通用户级服务**：
  ```bash
  systemctl --user status coding-tools-mcp.service
  journalctl --user -u coding-tools-mcp.service -f
  ```

### 2. Root systemd 与交互式 SSH 会话的 IPC 寻址

守护进程通过 `0700` 目录下的 `0600` Unix Domain Socket（`daemon.sock`）接受 `status` / `reload` / `stop` / `lan pin` 等本地控制命令：

- 以 `root` 运行系统级 systemd 时，Socket 固定优先位于 `/run/coding-tools-mcp/daemon.sock`，在任何 `root` SSH 会话中直接执行 `coding-tools-mcp status` 或 `coding-tools-mcp lan pin` 均可精准命中运行中的守护进程。
- 以普通用户运行时，Socket 位于 `$XDG_RUNTIME_DIR/coding-tools-mcp/daemon.sock`（回退 `/tmp/coding-tools-mcp-<uid>/daemon.sock`）。

### 3. 跨网段组网（EasyTier / Tailscale / WireGuard）

- 若两台机器不在同一二层广播域，可配置目标子网或直接指定 IP 配对：
  ```bash
  coding-tools-mcp config set extra-subnets 10.144.144.0/24
  coding-tools-mcp lan scan
  # 或直接配对（端口默认为 28765）
  coding-tools-mcp lan pair 10.144.144.5:28765 123456
  ```
- 请确保 Linux 防火墙（`ufw` / `firewalld` / 云安全组）对信任网段放行 TCP `28765`（或你自定义的 `--port`）。无需向公网开放该端口。

### 4. 本地从源码手动构建无头二进制

```bash
cd src-tauri
cargo build --release --locked --no-default-features --bin coding-tools-mcp
# 产物位置：src-tauri/target/release/coding-tools-mcp
```

> 仅编译无 UI 目标时不需要安装 `libwebkit2gtk`、`libgtk-3` 或 Node.js。
