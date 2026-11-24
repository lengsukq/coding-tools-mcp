#!/usr/bin/env bash
# Coding Tools MCP — Linux Headless One-Click Installer / Upgrader / Uninstaller
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash -s -- --service --lan --auto-root ~/code

set -euo pipefail

REPO="${CODING_TOOLS_MCP_REPO:-lengsukq/coding-tools-mcp}"
BIN_NAME="coding-tools-mcp"
SERVICE_NAME="coding-tools-mcp.service"

VERSION="${CODING_TOOLS_MCP_VERSION:-latest}"
INSTALL_DIR="${CODING_TOOLS_MCP_INSTALL_DIR:-}"
GITHUB_MIRROR="${GITHUB_MIRROR:-${CODING_TOOLS_MCP_MIRROR:-}}"
ARCHIVE_PATH=""
SOURCE_REF=""
FROM_SOURCE="0"
ENABLE_SERVICE="0"
NO_RESTART="0"
ENABLE_LAN="0"
ISSUE_PIN="0"
UNINSTALL="0"
PORT=""
DEVICE_NAME=""
AUTO_ROOTS=()

TMP_DIR=""
STAGED_BIN=""
INSTALLED_BIN=""

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}
trap cleanup EXIT

info() {
  printf '[coding-tools-mcp] %s\n' "$*"
}

warn() {
  printf '[coding-tools-mcp] 警告: %s\n' "$*" >&2
}

die() {
  printf '[coding-tools-mcp] 错误: %s\n' "$*" >&2
  exit 1
}

print_help() {
  cat <<'EOF'
Coding Tools MCP — Linux 无头版一键安装脚本

用法:
  curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash
  curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash -s -- [选项]

常用选项:
  --version <TAG>        指定安装版本（如 v0.3.9，默认 latest 自动匹配含 Linux 产物的最新 Release）
  --install-dir <DIR>    指定安装目录（root 默认 /usr/local/bin，普通用户默认 ~/.local/bin）
  --mirror <URL>         指定 GitHub 下载镜像前缀（如 https://gh-proxy.com/）
  --service              安装后自动注册并启动 systemd 常驻服务（root 为系统级，普通用户为 --user）
  --lan                  安装后启用局域网 / EasyTier / Tailscale 跨设备联邦
  --device-name <NAME>   设置本机在联邦网络中显示的设备名称
  --port <PORT>          设置本地 Global MCP 监听端口（默认 28765）
  --auto-root <DIR>      注册父目录自动发现（自动纳管其下一级 .git 仓库，可重复传入）
  --pair-pin             配合 --service 使用，服务就绪后立即生成并打印 6 位配对码
  --from-source          从 GitHub 源码编译零 GTK 无头二进制（需要 git 与 Rust cargo）
  --ref <BRANCH|TAG>     配合 --from-source 指定拉取的分支或标签（默认 main）
  --archive <FILE>       从本地离线 tar.gz 压缩包或裸二进制安装
  --no-restart           覆盖升级时不自动重启已在运行的 systemd 服务
  --uninstall            卸载 systemd 服务并删除已安装的二进制文件（保留用户配置）
  -h, --help             显示本帮助信息

环境变量:
  CODING_TOOLS_MCP_REPO         覆盖 GitHub 仓库（默认 lengsukq/coding-tools-mcp）
  CODING_TOOLS_MCP_VERSION      同 --version
  CODING_TOOLS_MCP_INSTALL_DIR  同 --install-dir
  GITHUB_MIRROR                 同 --mirror
  GITHUB_TOKEN                  可选，访问 GitHub API 时避免匿名限流

示例:
  # 普通用户安装二进制
  curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | bash

  # root 系统级常驻 + 开启联邦 + 自动发现 /srv/code 下全部 Git 仓库 + 打印配对码
  curl -fsSL https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
    sudo bash -s -- --service --lan --device-name home-linux --auto-root /srv/code --pair-pin

  # 国内网络使用镜像加速
  curl -fsSL https://gh-proxy.com/https://raw.githubusercontent.com/lengsukq/coding-tools-mcp/main/scripts/install-linux.sh | \
    bash -s -- --mirror https://gh-proxy.com/ --service --lan
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version|-v)
        [[ $# -ge 2 ]] || die "--version 需要提供版本号（例如 v0.3.9）"
        VERSION="$2"
        shift 2
        ;;
      --install-dir|-d)
        [[ $# -ge 2 ]] || die "--install-dir 需要提供目录路径"
        INSTALL_DIR="$2"
        shift 2
        ;;
      --mirror|-m)
        [[ $# -ge 2 ]] || die "--mirror 需要提供镜像地址"
        GITHUB_MIRROR="$2"
        shift 2
        ;;
      --archive|-a)
        [[ $# -ge 2 ]] || die "--archive 需要提供本地文件路径"
        ARCHIVE_PATH="$2"
        shift 2
        ;;
      --ref)
        [[ $# -ge 2 ]] || die "--ref 需要提供分支名或标签"
        SOURCE_REF="$2"
        shift 2
        ;;
      --from-source)
        FROM_SOURCE="1"
        shift
        ;;
      --service)
        ENABLE_SERVICE="1"
        shift
        ;;
      --no-restart)
        NO_RESTART="1"
        shift
        ;;
      --lan)
        ENABLE_LAN="1"
        shift
        ;;
      --device-name)
        [[ $# -ge 2 ]] || die "--device-name 需要提供设备名称"
        DEVICE_NAME="$2"
        shift 2
        ;;
      --port|-p)
        [[ $# -ge 2 ]] || die "--port 需要提供端口号"
        PORT="$2"
        shift 2
        ;;
      --auto-root)
        [[ $# -ge 2 ]] || die "--auto-root 需要提供父目录路径"
        AUTO_ROOTS+=("$2")
        shift 2
        ;;
      --pair-pin)
        ENABLE_LAN="1"
        ISSUE_PIN="1"
        shift
        ;;
      --uninstall)
        UNINSTALL="1"
        shift
        ;;
      -h|--help)
        print_help
        exit 0
        ;;
      *)
        die "未知参数: $1（使用 --help 查看完整用法）"
        ;;
    esac
  done
}

detect_platform() {
  local os_name arch_name
  os_name="${CODING_TOOLS_MCP_OS:-$(uname -s)}"
  arch_name="${CODING_TOOLS_MCP_ARCH:-$(uname -m)}"

  if [[ "${os_name}" != "Linux" ]]; then
    die "本安装脚本仅支持 Linux（当前系统: ${os_name}）。macOS / Windows 请前往 GitHub Releases 下载桌面安装包。"
  fi

  case "${arch_name}" in
    x86_64|amd64)
      ARCH="x86_64"
      ;;
    aarch64|arm64)
      ARCH="aarch64"
      ;;
    *)
      die "暂不支持当前 CPU 架构: ${arch_name}（预编译支持 x86_64 / aarch64，或尝试 --from-source）"
      ;;
  esac
}

resolve_install_dir() {
  if [[ -n "${INSTALL_DIR}" ]]; then
    return
  fi

  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    INSTALL_DIR="/usr/local/bin"
  elif [[ -d "/usr/local/bin" && -w "/usr/local/bin" ]]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

apply_mirror() {
  local url="$1"
  if [[ -z "${GITHUB_MIRROR}" ]]; then
    printf '%s' "${url}"
    return
  fi
  local prefix="${GITHUB_MIRROR%/}"
  printf '%s/%s' "${prefix}" "${url}"
}

http_get_text() {
  local url="$1"
  if have_cmd curl; then
    if [[ -n "${GITHUB_TOKEN:-}" && "${url}" == https://api.github.com/* ]]; then
      curl -fsSL -H "Accept: application/vnd.github+json" -H "Authorization: Bearer ${GITHUB_TOKEN}" "${url}"
    else
      curl -fsSL -H "Accept: application/vnd.github+json" "${url}"
    fi
  elif have_cmd wget; then
    if [[ -n "${GITHUB_TOKEN:-}" && "${url}" == https://api.github.com/* ]]; then
      wget -qO- --header="Accept: application/vnd.github+json" --header="Authorization: Bearer ${GITHUB_TOKEN}" "${url}"
    else
      wget -qO- --header="Accept: application/vnd.github+json" "${url}"
    fi
  else
    die "未找到 curl 或 wget，请先安装其中之一。"
  fi
}

http_download_file() {
  local url="$1"
  local dest="$2"
  if have_cmd curl; then
    curl -fL --retry 3 --retry-delay 1 --progress-bar -o "${dest}" "${url}"
  elif have_cmd wget; then
    wget -O "${dest}" "${url}"
  else
    die "未找到 curl 或 wget，请先安装其中之一。"
  fi
}

resolve_release_download_url() {
  local suffix="-linux-${ARCH}-headless.tar.gz"

  if [[ "${VERSION}" != "latest" ]]; then
    local tag="${VERSION}"
    [[ "${tag}" == v* ]] || tag="v${tag}"
    local bare="${tag#v}"
    printf 'https://github.com/%s/releases/download/%s/%s-v%s%s' \
      "${REPO}" "${tag}" "${BIN_NAME}" "${bare}" "${suffix}"
    return 0
  fi

  local api_url="https://api.github.com/repos/${REPO}/releases?per_page=15"
  local body=""
  if body="$(http_get_text "${api_url}" 2>/dev/null)"; then
    local matched
    matched="$(
      printf '%s' "${body}" \
        | grep -oE "\"browser_download_url\"[[:space:]]*:[[:space:]]*\"[^\"]+${BIN_NAME}-v[^\"]+${suffix}\"" \
        | head -n 1 \
        | sed -E 's/^"browser_download_url"[[:space:]]*:[[:space:]]*"([^"]+)"$/\1/' \
        || true
    )"
    if [[ -n "${matched}" ]]; then
      printf '%s' "${matched}"
      return 0
    fi
  fi

  # Fallback when GitHub API is rate-limited or unreachable: inspect /releases/latest redirect.
  if have_cmd curl; then
    local latest_url
    latest_url="$(apply_mirror "https://github.com/${REPO}/releases/latest")"
    local effective=""
    if effective="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${latest_url}" 2>/dev/null)"; then
      local tag="${effective##*/}"
      if [[ "${tag}" == v* ]]; then
        local bare="${tag#v}"
        printf 'https://github.com/%s/releases/download/%s/%s-v%s%s' \
          "${REPO}" "${tag}" "${BIN_NAME}" "${bare}" "${suffix}"
        return 0
      fi
    fi
  fi

  return 1
}

extract_or_stage_binary() {
  local archive="$1"
  local staged="${TMP_DIR}/${BIN_NAME}"

  [[ -f "${archive}" ]] || die "找不到指定的安装包文件: ${archive}"

  if [[ "${archive}" == *.tar.gz || "${archive}" == *.tgz ]]; then
    have_cmd tar || die "需要 tar 解压安装包，请先安装 tar。"
    mkdir -p "${TMP_DIR}/unpack"
    tar -xzf "${archive}" -C "${TMP_DIR}/unpack"
    local extracted
    extracted="$(find "${TMP_DIR}/unpack" -type f -name "${BIN_NAME}" | head -n 1 || true)"
    [[ -n "${extracted}" ]] || die "压缩包 ${archive} 中未找到可执行文件 ${BIN_NAME}"
    cp "${extracted}" "${staged}"
  else
    cp "${archive}" "${staged}"
  fi

  chmod 0755 "${staged}"
  STAGED_BIN="${staged}"
}

build_from_source() {
  have_cmd git || die "--from-source 需要 git，请先安装 git。"
  have_cmd cargo || die "--from-source 需要 Rust 工具链（cargo），请先执行：curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"

  local clone_url
  clone_url="$(apply_mirror "https://github.com/${REPO}.git")"
  local ref="${SOURCE_REF}"
  if [[ -z "${ref}" ]]; then
    if [[ "${VERSION}" != "latest" ]]; then
      ref="${VERSION}"
      [[ "${ref}" == v* ]] || ref="v${ref}"
    else
      ref="main"
    fi
  fi

  info "从源码拉取 ${REPO} (${ref}) 并编译零 GTK 无头二进制…"
  git clone --depth 1 --branch "${ref}" "${clone_url}" "${TMP_DIR}/src"
  (
    cd "${TMP_DIR}/src/src-tauri"
    cargo build --release --locked --no-default-features --bin "${BIN_NAME}"
  )

  local built="${TMP_DIR}/src/src-tauri/target/release/${BIN_NAME}"
  [[ -x "${built}" ]] || die "源码编译未生成预期的二进制文件: ${built}"
  cp "${built}" "${TMP_DIR}/${BIN_NAME}"
  chmod 0755 "${TMP_DIR}/${BIN_NAME}"
  STAGED_BIN="${TMP_DIR}/${BIN_NAME}"
}

acquire_binary() {
  if [[ -n "${ARCHIVE_PATH}" ]]; then
    info "使用本地安装包: ${ARCHIVE_PATH}"
    extract_or_stage_binary "${ARCHIVE_PATH}"
    return 0
  fi

  if [[ "${FROM_SOURCE}" == "1" ]]; then
    build_from_source
    return 0
  fi

  local raw_url=""
  if raw_url="$(resolve_release_download_url)"; then
    local download_url
    download_url="$(apply_mirror "${raw_url}")"
    local archive_dest="${TMP_DIR}/${BIN_NAME}-headless.tar.gz"
    info "正在下载预编译无头包 (${ARCH}): ${download_url}"
    if http_download_file "${download_url}" "${archive_dest}"; then
      extract_or_stage_binary "${archive_dest}"
      return 0
    fi
    warn "预编译包下载失败（可能尚未发布对应 Linux 无头 Release 资产）。"
  else
    warn "未在 GitHub Releases 中找到包含 linux-${ARCH}-headless.tar.gz 的已发布版本。"
  fi

  if have_cmd git && have_cmd cargo; then
    warn "检测到本机已安装 git 与 cargo，自动回退为源码编译模式…"
    build_from_source
    return 0
  fi

  die "暂无可直接下载的 linux-${ARCH} 预编译无头包。请任选其一：\n  1) 等待或发布含 Linux 无头资产的 Release 后重试；\n  2) 安装 Rust (https://rustup.rs) 后追加 --from-source 编译安装；\n  3) 手动下载压缩包后使用 --archive <tar.gz> 离线安装。"
}

install_binary_atomically() {
  local staged="$1"
  local target="${INSTALL_DIR}/${BIN_NAME}"
  local tmp_target="${INSTALL_DIR}/.${BIN_NAME}.tmp.$$"

  mkdir -p "${INSTALL_DIR}"
  cp "${staged}" "${tmp_target}"
  chmod 0755 "${tmp_target}"
  # Atomic rename avoids ETXTBSY when upgrading a running daemon.
  mv -f "${tmp_target}" "${target}"
  INSTALLED_BIN="${target}"
}

systemd_is_active() {
  have_cmd systemctl || return 1
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    systemctl is-active --quiet "${SERVICE_NAME}" 2>/dev/null
  else
    systemctl --user is-active --quiet "${SERVICE_NAME}" 2>/dev/null
  fi
}

systemd_restart() {
  have_cmd systemctl || return 0
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    systemctl restart "${SERVICE_NAME}"
  else
    systemctl --user restart "${SERVICE_NAME}"
  fi
}

do_uninstall() {
  resolve_install_dir
  local target="${INSTALL_DIR}/${BIN_NAME}"
  if [[ ! -x "${target}" ]] && have_cmd "${BIN_NAME}"; then
    target="$(command -v "${BIN_NAME}")"
  fi

  if [[ -x "${target}" ]]; then
    info "尝试卸载 systemd 常驻服务…"
    "${target}" service uninstall || true
    rm -f "${target}"
    info "已移除二进制文件: ${target}"
  else
    warn "未在 ${target} 或 PATH 中找到 ${BIN_NAME}。"
  fi

  local cfg_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/coding-tools-mcp"
  info "卸载完成（配置与配对数据默认保留在 ${cfg_dir}，如需彻底清空可手动删除）。"
}

apply_post_install_config() {
  local target="$1"

  if [[ -n "${DEVICE_NAME}" ]]; then
    info "设置本机设备名称: ${DEVICE_NAME}"
    "${target}" config set device-name "${DEVICE_NAME}"
  fi

  if [[ -n "${PORT}" ]]; then
    info "设置监听端口: ${PORT}"
    "${target}" config set port "${PORT}"
  fi

  if [[ "${ENABLE_LAN}" == "1" ]]; then
    info "开启局域网 / 组网联邦能力…"
    "${target}" lan enable
  fi

  if [[ ${#AUTO_ROOTS[@]} -gt 0 ]]; then
    local root_dir
    for root_dir in "${AUTO_ROOTS[@]}"; do
      info "注册父目录 .git 自动发现: ${root_dir}"
      "${target}" workspace auto-root add "${root_dir}"
    done
  fi
}

wait_for_daemon_and_issue_pin() {
  local target="$1"
  local attempt=0
  while [[ ${attempt} -lt 12 ]]; do
    if "${target}" lan pin 2>/dev/null; then
      return 0
    fi
    attempt=$((attempt + 1))
    sleep 0.5
  done
  warn "守护进程尚未完全就绪，可稍后手动执行: ${BIN_NAME} lan pin"
}

print_next_steps() {
  local target="$1"
  local in_path="0"
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) in_path="1" ;;
  esac

  echo ""
  info "安装完成！"
  if [[ "${in_path}" == "0" ]]; then
    warn "${INSTALL_DIR} 当前不在 PATH 中，可执行以下命令加入环境变量："
    printf '  export PATH="%s:$PATH"\n' "${INSTALL_DIR}"
    printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc\n' "${INSTALL_DIR}"
  fi

  local cmd="${BIN_NAME}"
  [[ "${in_path}" == "1" ]] || cmd="${target}"

  if [[ "${ENABLE_SERVICE}" == "1" ]]; then
    cat <<EOF

常用运维命令:
  ${cmd} status                              # 查看运行状态与监听地址
  ${cmd} lan pin                             # 生成 6 位一次性跨设备配对码
  ${cmd} workspace auto-root add ~/code      # 自动纳管父目录下一级 .git 仓库
  ${cmd} workspace list                      # 查看已纳管工作区
EOF
  else
    cat <<EOF

快速开始:
  1. 添加项目或自动发现目录:
     ${cmd} workspace add /path/to/repo
     # 或自动纳管某父目录下全部 Git 仓库:
     ${cmd} workspace auto-root add ~/code

  2. 前台启动并立即生成 6 位配对码（与 Mac / Windows 桌面端配对）:
     ${cmd} serve --lan --pair-pin

  3. 安装为 systemd 开机常驻后台服务（推荐）:
     ${cmd} service install
     # 普通用户如需退出 SSH 后保持常驻，另执行: sudo loginctl enable-linger \$USER
EOF
  fi
}

main() {
  parse_args "$@"

  if [[ "${UNINSTALL}" == "1" ]]; then
    do_uninstall
    exit 0
  fi

  detect_platform
  resolve_install_dir
  TMP_DIR="$(mktemp -d)"

  local was_active="0"
  if systemd_is_active; then
    was_active="1"
  fi

  acquire_binary
  install_binary_atomically "${STAGED_BIN}"
  local target="${INSTALLED_BIN}"

  local installed_ver
  installed_ver="$("${target}" --version 2>/dev/null || echo "${BIN_NAME}")"
  info "已安装 ${installed_ver} -> ${target}"

  apply_post_install_config "${target}"

  if [[ "${ENABLE_SERVICE}" == "1" ]]; then
    info "正在安装并启动 systemd 常驻服务…"
    "${target}" service install
    if [[ "${ISSUE_PIN}" == "1" ]]; then
      wait_for_daemon_and_issue_pin "${target}"
    fi
  elif [[ "${was_active}" == "1" && "${NO_RESTART}" == "0" ]]; then
    info "检测到 ${SERVICE_NAME} 正在运行，正在平滑重启以加载新版本…"
    systemd_restart
    if [[ "${ISSUE_PIN}" == "1" ]]; then
      wait_for_daemon_and_issue_pin "${target}"
    fi
  elif [[ "${ISSUE_PIN}" == "1" ]]; then
    warn "指定了 --pair-pin 但未启用 --service，请先运行 \`${BIN_NAME} serve --lan --pair-pin\`。"
  fi

  print_next_steps "${target}"
}

main "$@"
