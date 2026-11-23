use crate::data::DataStore;
use crate::mcp::gateway::WorkspaceRegistry;
use crate::platform::local_ipc::LocalDaemonRequest;
use crate::workspace::auto_discover::{
    add_auto_root, add_workspace_by_path, remove_auto_root, remove_workspace_by_target,
    sync_auto_discovered_workspaces,
};

use super::daemon::try_send_daemon;

pub async fn cmd_workspace(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" | "ls" => {
            let persisted_ids: std::collections::HashSet<String> =
                DataStore::read_file(|d| Ok(d.profiles.iter().map(|p| p.id.clone()).collect()))
                    .unwrap_or_default();
            let all = WorkspaceRegistry::default().list()?;
            if all.is_empty() {
                println!("当前尚未注册任何工作区。可使用 `coding-tools-mcp workspace add <目录>` 或 `workspace auto-root add <父目录>` 添加。");
                return Ok(());
            }
            println!("共 {} 个可用工作区：", all.len());
            for item in all {
                let source = if persisted_ids.contains(&item.id) {
                    "已注册"
                } else {
                    "自动发现"
                };
                println!(
                    "  - {}  [{}]  {}  ({})",
                    item.id, source, item.name, item.path
                );
            }
            Ok(())
        }
        "add" => {
            let Some(path) = args.get(1) else {
                return Err("用法: coding-tools-mcp workspace add <目录> [--name <名称>]".into());
            };
            let mut custom_name: Option<&str> = None;
            let mut idx = 2;
            while idx < args.len() {
                match args[idx].as_str() {
                    "--name" | "-n" => {
                        let val = args
                            .get(idx + 1)
                            .ok_or_else(|| "`--name` 需要提供工作区显示名称".to_string())?;
                        custom_name = Some(val.as_str());
                        idx += 2;
                    }
                    other => return Err(format!("`workspace add` 不支持参数 `{other}`")),
                }
            }
            let (profile, created) =
                add_workspace_by_path(path, custom_name).map_err(|e| e.to_string())?;
            let _ = try_send_daemon(&LocalDaemonRequest::Reload {
                tunnel_changed: false,
            })
            .await;
            if created {
                println!(
                    "已添加工作区 `{}` (ID: {}) -> {}",
                    profile.name, profile.id, profile.path
                );
            } else {
                println!(
                    "工作区已存在 `{}` (ID: {}) -> {}",
                    profile.name, profile.id, profile.path
                );
            }
            Ok(())
        }
        "remove" | "rm" | "delete" => {
            let Some(target) = args.get(1) else {
                return Err("用法: coding-tools-mcp workspace remove <ID|名称|目录路径>".into());
            };
            let removed = remove_workspace_by_target(target).map_err(|e| e.to_string())?;
            let _ = try_send_daemon(&LocalDaemonRequest::Reload {
                tunnel_changed: false,
            })
            .await;
            println!("已移除工作区 `{}` (ID: {})", removed.name, removed.id);
            Ok(())
        }
        "scan" => {
            let report = sync_auto_discovered_workspaces().map_err(|e| e.to_string())?;
            println!(
                "扫描完成：新增持久化 {} 个工作区，当前共 {} 个工作区。",
                report.added.len(),
                report.total_workspaces
            );
            for item in report.added {
                println!("  + {} ({}) -> {}", item.name, item.id, item.path);
            }
            Ok(())
        }
        "auto-root" | "roots" => cmd_auto_root(&args[1..]).await,
        other => Err(format!(
            "未知 `workspace` 子命令 `{other}`（支持: list, add, remove, scan, auto-root）"
        )),
    }
}

async fn cmd_auto_root(args: &[String]) -> Result<(), String> {
    let action = args.first().map(String::as_str).unwrap_or("list");
    match action {
        "list" | "ls" => {
            let roots = DataStore::read_file(|d| Ok(d.workspace_auto_roots.clone()))
                .map_err(|e| e.to_string())?;
            if roots.is_empty() {
                println!("当前未配置自动发现父目录。可使用 `coding-tools-mcp workspace auto-root add ~/code` 添加。");
            } else {
                println!("已配置 {} 个自动发现父目录：", roots.len());
                for root in roots {
                    println!("  - {root}");
                }
            }
            Ok(())
        }
        "add" => {
            let Some(dir) = args.get(1) else {
                return Err("用法: coding-tools-mcp workspace auto-root add <父目录>".into());
            };
            let (canon, report) = add_auto_root(dir).map_err(|e| e.to_string())?;
            let _ = try_send_daemon(&LocalDaemonRequest::Reload {
                tunnel_changed: false,
            })
            .await;
            println!(
                "已注册自动发现父目录 `{canon}`（本次同步新增 {} 个 .git 仓库，总计 {} 个工作区）",
                report.added.len(),
                report.total_workspaces
            );
            Ok(())
        }
        "remove" | "rm" => {
            let Some(dir) = args.get(1) else {
                return Err("用法: coding-tools-mcp workspace auto-root remove <父目录>".into());
            };
            let removed = remove_auto_root(dir).map_err(|e| e.to_string())?;
            if removed {
                println!("已移除自动发现父目录 `{dir}`");
            } else {
                println!("未找到匹配的自动发现父目录 `{dir}`");
            }
            Ok(())
        }
        other => Err(format!(
            "未知 `workspace auto-root` 动作 `{other}`（支持: list, add, remove）"
        )),
    }
}
