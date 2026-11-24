use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use crate::data::DataStore;
use crate::platform::local_ipc::LocalDaemonRequest;
use crate::settings::AppSettings;
use crate::upstream_mcp::{
    is_sensitive_key, parse_mcp_servers_json, scan_local_mcp_clients,
    validate_and_normalize_server, SelfLoopContext, UpstreamExposeMode, UpstreamHub,
    UpstreamSecretPatch, UpstreamServerConfig, UpstreamTransportConfig, DEFAULT_CALL_TIMEOUT_SECS,
    DEFAULT_IDLE_TIMEOUT_SECS,
};

use super::daemon::try_send_daemon;

pub async fn cmd_upstream(args: &[String]) -> Result<(), String> {
    let Some(action) = args.first().map(String::as_str) else {
        print_upstream_help();
        return Ok(());
    };
    let rest = &args[1..];
    match action {
        "list" | "ls" => cmd_list(rest),
        "add" => cmd_add(rest).await,
        "remove" | "rm" | "delete" => cmd_remove(rest).await,
        "enable" => cmd_set_enabled(rest, true).await,
        "disable" => cmd_set_enabled(rest, false).await,
        "mode" => cmd_set_mode(rest).await,
        "test" => cmd_test(rest).await,
        "sync" => cmd_sync(rest).await,
        "tools" => cmd_tools(rest).await,
        "import" => cmd_import(rest).await,
        "-h" | "--help" | "help" => {
            print_upstream_help();
            Ok(())
        }
        other => Err(format!(
            "未知 upstream 子命令 `{other}`。运行 `coding-tools-mcp upstream --help` 查看用法。"
        )),
    }
}

fn print_upstream_help() {
    println!(
        "\
管理聚合的上游 MCP 服务（Stdio / Streamable HTTP / Legacy SSE）

USAGE:
  coding-tools-mcp upstream list [--json]
  coding-tools-mcp upstream add <ALIAS> --stdio <CMD> [--args <ARG>...] [--cwd <DIR>]
                                [--env K=V] [--secret-env K=V] [--mode direct|proxy] [--name <NAME>]
  coding-tools-mcp upstream add <ALIAS> --http <URL> | --sse <URL> [--bearer <TOKEN>]
                                [--header K:V] [--secret-header K:V] [--use-proxy]
                                [--mode direct|proxy] [--name <NAME>]
  coding-tools-mcp upstream remove <ALIAS_OR_ID>
  coding-tools-mcp upstream enable <ALIAS_OR_ID>
  coding-tools-mcp upstream disable <ALIAS_OR_ID>
  coding-tools-mcp upstream mode <ALIAS_OR_ID> <direct|proxy>
  coding-tools-mcp upstream test <ALIAS_OR_ID>
  coding-tools-mcp upstream sync [ALIAS_OR_ID]
  coding-tools-mcp upstream tools <ALIAS_OR_ID> [--enable <TOOL> | --disable <TOOL>]
  coding-tools-mcp upstream import --scan [--yes] | --file <PATH> | --stdin"
    );
}

fn cmd_list(args: &[String]) -> Result<(), String> {
    let as_json = args.iter().any(|a| a == "--json");
    let hub = UpstreamHub::new();
    let overview = hub.overview(None);

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&overview)
                .map_err(|err| format!("序列化输出失败: {err}"))?
        );
        return Ok(());
    }

    println!(
        "MCP 聚合状态 : {}（内置 {} + 直出 {} + 代理元工具 {} = 共 {} 工具）",
        if overview.enabled {
            "已开启"
        } else {
            "已关闭"
        },
        overview.builtin_tool_count,
        overview.direct_exposed_tool_count,
        overview.proxy_meta_tool_count,
        overview.total_exposed_tool_count
    );
    if overview.servers.is_empty() {
        println!("尚未配置任何上游 MCP 服务。可使用 `coding-tools-mcp upstream add` 或 `upstream import --scan` 添加。");
        return Ok(());
    }
    println!();
    println!(
        "{:<18} {:<15} {:<8} {:<8} {:<10} TARGET",
        "ALIAS", "TRANSPORT", "MODE", "ENABLED", "TOOLS"
    );
    for item in overview.servers {
        let cfg = &item.config;
        let tools_col = if item.runtime.synced_at_ms.is_some() {
            format!("{}/{}", item.enabled_tool_count, item.total_tool_count)
        } else {
            "未同步".to_string()
        };
        println!(
            "{:<18} {:<15} {:<8} {:<8} {:<10} {}",
            cfg.alias,
            cfg.transport.kind_label(),
            cfg.expose_mode.as_str(),
            if cfg.enabled { "yes" } else { "no" },
            tools_col,
            cfg.transport.summary_target()
        );
    }
    Ok(())
}

async fn cmd_add(args: &[String]) -> Result<(), String> {
    let Some(alias_raw) = args.first().map(String::as_str) else {
        return Err("缺少 <ALIAS>。运行 `coding-tools-mcp upstream --help` 查看用法。".into());
    };
    if alias_raw == "-h" || alias_raw == "--help" {
        print_upstream_help();
        return Ok(());
    }

    let mut stdio_cmd: Option<String> = None;
    let mut stdio_args: Vec<String> = Vec::new();
    let mut cwd: Option<String> = None;
    let mut http_url: Option<String> = None;
    let mut sse_url: Option<String> = None;
    let mut bearer: Option<String> = None;
    let mut use_proxy = false;
    let mut env_plain = BTreeMap::new();
    let mut env_secret = BTreeMap::new();
    let mut headers_plain = BTreeMap::new();
    let mut headers_secret = BTreeMap::new();
    let mut mode = UpstreamExposeMode::Direct;
    let mut name: Option<String> = None;
    let mut description = String::new();

    let mut idx = 1usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--stdio" => {
                let v = args.get(idx + 1).ok_or("`--stdio` 需要指定可执行命令")?;
                stdio_cmd = Some(v.clone());
                idx += 2;
            }
            "--arg" | "--args" => {
                idx += 1;
                while idx < args.len() && !args[idx].starts_with("--") {
                    stdio_args.push(args[idx].clone());
                    idx += 1;
                }
            }
            "--cwd" => {
                let v = args.get(idx + 1).ok_or("`--cwd` 需要指定目录")?;
                cwd = Some(v.clone());
                idx += 2;
            }
            "--http" => {
                let v = args.get(idx + 1).ok_or("`--http` 需要指定 URL")?;
                http_url = Some(v.clone());
                idx += 2;
            }
            "--sse" => {
                let v = args.get(idx + 1).ok_or("`--sse` 需要指定 URL")?;
                sse_url = Some(v.clone());
                idx += 2;
            }
            "--bearer" => {
                let v = args.get(idx + 1).ok_or("`--bearer` 需要指定 Token")?;
                bearer = Some(v.clone());
                idx += 2;
            }
            "--use-proxy" => {
                use_proxy = true;
                idx += 1;
            }
            "--env" => {
                let v = args.get(idx + 1).ok_or("`--env` 需要 K=V 格式")?;
                let (k, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("无效环境变量 `{v}`，应为 KEY=VALUE"))?;
                if is_sensitive_key(k) {
                    env_secret.insert(k.trim().to_string(), val.to_string());
                } else {
                    env_plain.insert(k.trim().to_string(), val.to_string());
                }
                idx += 2;
            }
            "--secret-env" => {
                let v = args.get(idx + 1).ok_or("`--secret-env` 需要 K=V 格式")?;
                let (k, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("无效加密环境变量 `{v}`，应为 KEY=VALUE"))?;
                env_secret.insert(k.trim().to_string(), val.to_string());
                idx += 2;
            }
            "--header" => {
                let v = args.get(idx + 1).ok_or("`--header` 需要 Key:Value 格式")?;
                let (k, val) = v
                    .split_once(':')
                    .ok_or_else(|| format!("无效 Header `{v}`，应为 Key:Value"))?;
                if is_sensitive_key(k) {
                    headers_secret.insert(k.trim().to_string(), val.trim().to_string());
                } else {
                    headers_plain.insert(k.trim().to_string(), val.trim().to_string());
                }
                idx += 2;
            }
            "--secret-header" => {
                let v = args
                    .get(idx + 1)
                    .ok_or("`--secret-header` 需要 Key:Value 格式")?;
                let (k, val) = v
                    .split_once(':')
                    .ok_or_else(|| format!("无效加密 Header `{v}`，应为 Key:Value"))?;
                headers_secret.insert(k.trim().to_string(), val.trim().to_string());
                idx += 2;
            }
            "--mode" => {
                let v = args.get(idx + 1).ok_or("`--mode` 需要 direct 或 proxy")?;
                mode = UpstreamExposeMode::parse_str(v)?;
                idx += 2;
            }
            "--name" => {
                let v = args.get(idx + 1).ok_or("`--name` 需要名称")?;
                name = Some(v.clone());
                idx += 2;
            }
            "--desc" | "--description" => {
                let v = args.get(idx + 1).ok_or("`--description` 需要说明文本")?;
                description = v.clone();
                idx += 2;
            }
            other => return Err(format!("`upstream add` 不支持参数 `{other}`")),
        }
    }

    let transport = match (stdio_cmd, http_url, sse_url) {
        (Some(command), None, None) => UpstreamTransportConfig::Stdio {
            command,
            args: stdio_args,
            cwd,
            env: env_plain,
            secret_env_keys: env_secret.keys().cloned().collect(),
        },
        (None, Some(url), None) => UpstreamTransportConfig::StreamableHttp {
            url,
            headers: headers_plain,
            secret_header_keys: headers_secret.keys().cloned().collect(),
            has_bearer_token: bearer.is_some(),
            use_proxy,
        },
        (None, None, Some(url)) => UpstreamTransportConfig::Sse {
            url,
            headers: headers_plain,
            secret_header_keys: headers_secret.keys().cloned().collect(),
            has_bearer_token: bearer.is_some(),
            use_proxy,
        },
        _ => {
            return Err(
                "必须且只能指定一种传输类型：`--stdio <CMD>`、`--http <URL>` 或 `--sse <URL>`"
                    .into(),
            );
        }
    };

    let settings = AppSettings::load_or_default();
    let mut server = UpstreamServerConfig {
        id: String::new(),
        alias: alias_raw.to_string(),
        name: name.unwrap_or_else(|| alias_raw.to_string()),
        description,
        enabled: true,
        expose_mode: mode,
        transport,
        disabled_tools: Vec::new(),
        call_timeout_secs: DEFAULT_CALL_TIMEOUT_SECS,
        idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
        source: "cli".into(),
    };

    validate_and_normalize_server(
        &mut server,
        &settings.upstream_mcp.servers,
        &SelfLoopContext::from_settings(&settings),
    )?;

    let patch = UpstreamSecretPatch {
        set_env_secrets: env_secret,
        set_header_secrets: headers_secret,
        bearer_token: bearer.map(Some),
        ..UpstreamSecretPatch::default()
    };

    let saved = DataStore::save_upstream_server_file(server, patch).map_err(|e| e.to_string())?;
    notify_daemon_upstream_reload().await;

    println!(
        "已添加上游 MCP `{}`（id: {} · 模式: {} · 传输: {}）",
        saved.alias,
        saved.id,
        saved.expose_mode.as_str(),
        saved.transport.kind_label()
    );
    println!(
        "提示：运行 `coding-tools-mcp upstream test {}` 以验证连接并同步工具清单。",
        saved.alias
    );
    Ok(())
}

async fn cmd_remove(args: &[String]) -> Result<(), String> {
    let Some(target) = args.first().map(String::as_str) else {
        return Err("请指定要移除的上游别名或 ID".into());
    };
    let settings = AppSettings::load_or_default();
    let srv = find_server(&settings, target)?;
    DataStore::delete_upstream_server_file(&srv.id).map_err(|e| e.to_string())?;
    let hub = UpstreamHub::new();
    hub.disconnect_server(&srv.id, true).await;
    notify_daemon_upstream_reload().await;
    println!(
        "已移除上游 MCP `{}`（{}）并清理关联凭据。",
        srv.alias, srv.id
    );
    Ok(())
}

async fn cmd_set_enabled(args: &[String], enabled: bool) -> Result<(), String> {
    let Some(target) = args.first().map(String::as_str) else {
        return Err("请指定上游别名或 ID".into());
    };
    let settings = AppSettings::load_or_default();
    let mut srv = find_server(&settings, target)?;
    srv.enabled = enabled;
    DataStore::save_upstream_server_file(srv.clone(), UpstreamSecretPatch::default())
        .map_err(|e| e.to_string())?;
    notify_daemon_upstream_reload().await;
    println!(
        "已{}上游 MCP `{}`。",
        if enabled { "启用" } else { "停用" },
        srv.alias
    );
    Ok(())
}

async fn cmd_set_mode(args: &[String]) -> Result<(), String> {
    let (Some(target), Some(raw_mode)) = (args.first(), args.get(1)) else {
        return Err("用法: `coding-tools-mcp upstream mode <ALIAS_OR_ID> <direct|proxy>`".into());
    };
    let mode = UpstreamExposeMode::parse_str(raw_mode)?;
    let settings = AppSettings::load_or_default();
    let mut srv = find_server(&settings, target)?;
    srv.expose_mode = mode;
    DataStore::save_upstream_server_file(srv.clone(), UpstreamSecretPatch::default())
        .map_err(|e| e.to_string())?;
    notify_daemon_upstream_reload().await;
    println!(
        "已将上游 MCP `{}` 暴露模式切换为 `{}`。",
        srv.alias,
        mode.as_str()
    );
    Ok(())
}

async fn cmd_test(args: &[String]) -> Result<(), String> {
    let Some(target) = args.first().map(String::as_str) else {
        return Err("请指定要测试的上游别名或 ID".into());
    };
    let hub = UpstreamHub::new();
    let res = hub.test_and_sync_server(target, None).await;
    hub.shutdown_all().await;
    if res.ok {
        notify_daemon_upstream_reload().await;
        println!(
            "连接成功 ✓  {} ({}) · {} ms · 协议 {} · 发现 {} 个工具",
            res.alias,
            if res.server_name.is_empty() {
                "upstream"
            } else {
                res.server_name.as_str()
            },
            res.latency_ms,
            res.protocol_version,
            res.tool_count
        );
        Ok(())
    } else {
        let err = res.error.unwrap_or_else(|| "未知错误".into());
        if !res.stderr_tail.is_empty() {
            eprintln!("最近 stderr 输出:");
            for line in res.stderr_tail.iter().rev().take(6).rev() {
                eprintln!("  {line}");
            }
        }
        Err(format!("测试上游 MCP `{target}` 失败: {err}"))
    }
}

async fn cmd_sync(args: &[String]) -> Result<(), String> {
    let settings = AppSettings::load_or_default();
    let hub = Arc::new(UpstreamHub::new());
    let targets: Vec<UpstreamServerConfig> = if let Some(target) = args.first() {
        vec![find_server(&settings, target)?]
    } else {
        settings
            .upstream_mcp
            .servers
            .iter()
            .filter(|s| s.enabled)
            .cloned()
            .collect()
    };
    if targets.is_empty() {
        println!("没有需要同步的启用上游 MCP。");
        return Ok(());
    }
    let mut failed = 0usize;
    for srv in targets {
        let res = hub.test_and_sync_server(&srv.id, None).await;
        if res.ok {
            println!(
                "  ✓ {:<18} {} 个工具 ({} ms)",
                res.alias, res.tool_count, res.latency_ms
            );
        } else {
            failed += 1;
            println!(
                "  ✗ {:<18} 失败: {}",
                res.alias,
                res.error.unwrap_or_else(|| "未知错误".into())
            );
        }
    }
    hub.shutdown_all().await;
    notify_daemon_upstream_reload().await;
    if failed > 0 {
        Err(format!("同步完成，但有 {failed} 个上游失败"))
    } else {
        Ok(())
    }
}

async fn cmd_tools(args: &[String]) -> Result<(), String> {
    let Some(target) = args.first().map(String::as_str) else {
        return Err(
            "用法: `coding-tools-mcp upstream tools <ALIAS_OR_ID> [--enable <T> | --disable <T>]`"
                .into(),
        );
    };
    let settings = AppSettings::load_or_default();
    let mut srv = find_server(&settings, target)?;

    let mut enable_list = Vec::new();
    let mut disable_list = Vec::new();
    let mut idx = 1usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--enable" => {
                let t = args.get(idx + 1).ok_or("`--enable` 需要指定工具名")?;
                enable_list.push(t.clone());
                idx += 2;
            }
            "--disable" => {
                let t = args.get(idx + 1).ok_or("`--disable` 需要指定工具名")?;
                disable_list.push(t.clone());
                idx += 2;
            }
            other => return Err(format!("`upstream tools` 不支持参数 `{other}`")),
        }
    }

    if !enable_list.is_empty() || !disable_list.is_empty() {
        for t in &enable_list {
            srv.disabled_tools.retain(|d| d != t);
        }
        for t in &disable_list {
            if !srv.disabled_tools.iter().any(|d| d == t) {
                srv.disabled_tools.push(t.clone());
            }
        }
        DataStore::save_upstream_server_file(srv.clone(), UpstreamSecretPatch::default())
            .map_err(|e| e.to_string())?;
        notify_daemon_upstream_reload().await;
        println!(
            "已更新 `{}` 的工具过滤配置（当前禁用 {} 个工具）。",
            srv.alias,
            srv.disabled_tools.len()
        );
        return Ok(());
    }

    let hub = UpstreamHub::new();
    let overview = hub.overview(None);
    let Some(item) = overview.servers.into_iter().find(|s| s.config.id == srv.id) else {
        return Err(format!("未找到上游 `{target}`"));
    };
    if item.tools.is_empty() {
        println!(
            "上游 `{}` 尚未缓存工具列表。请先运行 `coding-tools-mcp upstream test {}`。",
            srv.alias, srv.alias
        );
        return Ok(());
    }
    println!(
        "{:<8} {:<34} {:<34} DESCRIPTION",
        "STATE", "ORIGINAL TOOL", "EXPOSED NAME"
    );
    for tool in item.tools {
        let desc = crate::util::text::truncate_utf8(tool.description.trim(), 56);
        println!(
            "{:<8} {:<34} {:<34} {}",
            if tool.disabled { "off" } else { "on" },
            tool.original_name,
            tool.exposed_name,
            desc
        );
    }
    Ok(())
}

async fn cmd_import(args: &[String]) -> Result<(), String> {
    let mut scan = false;
    let mut yes = false;
    let mut file_path: Option<String> = None;
    let mut from_stdin = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--scan" => {
                scan = true;
                idx += 1;
            }
            "--yes" | "-y" => {
                yes = true;
                idx += 1;
            }
            "--file" | "-f" => {
                let p = args.get(idx + 1).ok_or("`--file` 需要指定 JSON 文件路径")?;
                file_path = Some(p.clone());
                idx += 2;
            }
            "--stdin" => {
                from_stdin = true;
                idx += 1;
            }
            other => return Err(format!("`upstream import` 不支持参数 `{other}`")),
        }
    }

    let settings = AppSettings::load_or_default();
    let existing_aliases: HashSet<String> = settings
        .upstream_mcp
        .servers
        .iter()
        .map(|s| s.alias.to_ascii_lowercase())
        .collect();
    let loop_ctx = SelfLoopContext::from_settings(&settings);

    let candidates = if scan {
        scan_local_mcp_clients(&existing_aliases, &loop_ctx)
    } else if let Some(path) = file_path {
        let raw =
            std::fs::read_to_string(&path).map_err(|err| format!("读取 `{path}` 失败: {err}"))?;
        parse_mcp_servers_json(
            &raw,
            "json",
            "JSON 文件",
            &path,
            &existing_aliases,
            &loop_ctx,
        )?
    } else if from_stdin {
        let mut raw = String::new();
        std::io::stdin()
            .read_to_string(&mut raw)
            .map_err(|err| format!("读取 stdin 失败: {err}"))?;
        parse_mcp_servers_json(&raw, "json", "STDIN", "", &existing_aliases, &loop_ctx)?
    } else {
        return Err("请指定 `--scan`、`--file <PATH>` 或 `--stdin`".into());
    };

    if candidates.is_empty() {
        println!("未发现可导入的上游 MCP 配置。");
        return Ok(());
    }

    if scan && !yes {
        println!(
            "扫描到 {} 个候选上游 MCP（加 `--yes` 确认写入导入）：",
            candidates.len()
        );
        for item in &candidates {
            let note = if let Some(reason) = &item.blocked_reason {
                format!(" [已拦截: {reason}]")
            } else if item.already_exists_alias {
                format!(" [别名冲突 -> 建议改为 {}]", item.suggested_alias)
            } else {
                String::new()
            };
            println!(
                "  - {:<18} ({:<14} · {:<14}) {}{}",
                item.suggested_alias,
                item.source_label,
                item.transport_kind,
                item.summary_target,
                note
            );
        }
        return Ok(());
    }

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut current_servers = settings.upstream_mcp.servers.clone();
    for mut item in candidates {
        if let Some(reason) = item.blocked_reason {
            eprintln!("  跳过 `{}`: {reason}", item.original_key);
            skipped += 1;
            continue;
        }
        if let Err(err) =
            validate_and_normalize_server(&mut item.config, &current_servers, &loop_ctx)
        {
            eprintln!("  跳过 `{}`: {err}", item.original_key);
            skipped += 1;
            continue;
        }
        let saved = DataStore::save_upstream_server_file(item.config, item.secret_patch)
            .map_err(|e| e.to_string())?;
        println!(
            "  ✓ 导入 `{}` ({})",
            saved.alias,
            saved.transport.kind_label()
        );
        current_servers.push(saved);
        imported += 1;
    }

    if imported > 0 {
        notify_daemon_upstream_reload().await;
    }
    println!("导入完成：成功 {imported} 个，跳过 {skipped} 个。");
    Ok(())
}

fn find_server(settings: &AppSettings, key: &str) -> Result<UpstreamServerConfig, String> {
    settings
        .upstream_mcp
        .servers
        .iter()
        .find(|s| s.alias.eq_ignore_ascii_case(key) || s.id == key)
        .cloned()
        .ok_or_else(|| {
            format!("未找到上游 MCP `{key}`（可使用 `coding-tools-mcp upstream list` 查看）")
        })
}

async fn notify_daemon_upstream_reload() {
    let _ = try_send_daemon(&LocalDaemonRequest::ReloadUpstream).await;
}
