//! Unified CLI & headless server entrypoint for `coding-tools-mcp`.

mod daemon;
mod lan_config_cmd;
mod systemd_cmd;
mod workspace_cmd;

#[cfg(test)]
mod tests;

/// Returns true when `arg` is an explicit CLI subcommand or help/version flag
/// (as opposed to desktop GUI flags like `--autostart`, `--lightweight`, or macOS `-psn_*`).
pub fn is_cli_subcommand(arg: &str) -> bool {
    matches!(
        arg,
        "mcp-stdio"
            | "serve"
            | "status"
            | "stop"
            | "reload"
            | "workspace"
            | "ws"
            | "lan"
            | "config"
            | "service"
            | "help"
            | "-h"
            | "--help"
            | "version"
            | "-V"
            | "--version"
    )
}

pub fn print_usage() {
    println!(
        "\
Coding Tools MCP v{ver} — Local AI Workspace & Headless MCP Runtime

USAGE:
  coding-tools-mcp <COMMAND> [OPTIONS]

COMMANDS:
  serve                     前台启动无 UI 守护进程（Global MCP + 局域网联邦 + 可选隧道）
    --port <PORT>           指定本地监听端口（覆盖保存配置）
    --lan                   开启局域网 / EasyTier 跨设备配对与联邦发现
    --allow-lan-mcp         允许局域网未配对客户端直连 /mcp
    --pair-pin              启动后立即生成并打印 6 位跨机配对码（2 分钟有效）
    --auto-root <DIR>       注册父目录自动发现（自动纳管其下一级 .git 仓库）

  status                    查看运行中的守护进程状态、监听地址与已纳管工作区统计
  reload                    通知运行中的守护进程热重载配置与监听策略
  stop                      优雅停止正在运行的后台无头守护进程

  workspace list            列出全部已注册及父目录自动发现的工作区
  workspace add <DIR> [--name <NAME>]
                            注册指定项目目录为工作区（幂等去重）
  workspace remove <ID|NAME|DIR>
                            移除指定工作区并清理其关联会话
  workspace auto-root list | add <DIR> | remove <DIR>
                            管理工作区自动发现父目录（例如 ~/code）
  workspace scan            立即扫描所有 auto-root 并将新 .git 仓库持久化入库

  lan status                查看本机设备标识、局域网/组网 IP 与已配对节点列表
  lan enable | disable      开启或关闭局域网与虚拟组网联邦能力
  lan pin                   向运行中的守护进程申请 6 位一次性配对码
  lan scan                  主动扫描局域网及 EasyTier / Tailscale 组网内的在线节点
  lan pair <HOST[:PORT]> <PIN>
                            使用对方显示的 6 位配对码发起双向加密配对
  lan unpair <DEVICE_ID>    解除与指定设备 ID 的配对信任

  config show               显示全局 MCP 端口、认证模式、组网子网与代理配置
  config set <KEY> <VALUE>  修改配置项（port|auth|server-name|allow-local-noauth|
                            allow-lan-mcp|lan-trust|extra-subnets|proxy-mode|proxy-url）

  service install | uninstall | status | print
                            管理 Linux systemd --user 开机常驻后台服务

  mcp-stdio <http://127.0.0.1:<PORT>/mcp>
                            启动本地 MCP stdio-to-HTTP 桥接器",
        ver = env!("CARGO_PKG_VERSION")
    );
}

pub fn run_cli(args: &[String]) -> i32 {
    let Some(sub) = args.get(1).map(String::as_str) else {
        print_usage();
        return 0;
    };
    match sub {
        "mcp-stdio" => return crate::mcp::stdio_bridge::run_cli(&args[2..]),
        "-V" | "--version" | "version" => {
            println!("coding-tools-mcp {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        "-h" | "--help" | "help" => {
            print_usage();
            return 0;
        }
        _ => {}
    }

    let worker_threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 4))
        .unwrap_or(2);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(16)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("初始化 Tokio 异步运行时失败: {err}");
            return 1;
        }
    };
    crate::async_rt::set(runtime.handle().clone());

    let rest = &args[2..];
    let outcome = runtime.block_on(async move {
        match sub {
            "serve" => daemon::cmd_serve(rest).await,
            "status" => daemon::cmd_status().await,
            "stop" => daemon::cmd_stop().await,
            "reload" => daemon::cmd_reload().await,
            "workspace" | "ws" => workspace_cmd::cmd_workspace(rest).await,
            "lan" => lan_config_cmd::cmd_lan(rest).await,
            "config" => lan_config_cmd::cmd_config(rest).await,
            "service" => systemd_cmd::cmd_service(rest),
            other => Err(format!(
                "未知子命令 `{other}`。运行 `coding-tools-mcp --help` 查看完整用法。"
            )),
        }
    });

    match outcome {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("错误: {err}");
            1
        }
    }
}
