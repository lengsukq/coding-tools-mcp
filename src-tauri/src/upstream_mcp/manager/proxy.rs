use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::mcp::gateway::GatewayError;
use crate::settings::AppSettings;
use crate::tools::workspace::tool_ok;
use crate::tools::wrap_mcp_tool_result;

use super::super::model::{
    unix_time_ms, UpstreamServerConfig, MAX_CALL_TIMEOUT_SECS, MIN_CALL_TIMEOUT_SECS,
};
use super::super::proxy_tools::{TOOL_UPSTREAM_CALL, TOOL_UPSTREAM_LIST, TOOL_UPSTREAM_TOOLS};
use super::pool::untrack_stdio_pid;
use super::{effective_settings, UpstreamCallOutcome, UpstreamHub};

impl UpstreamHub {
    /// Execute either one of the 3 `mcp_upstream_*` proxy meta-tools or a `direct`-mode `{alias}__{tool}` call.
    pub async fn handle_tool_call(
        &self,
        tool_name: &str,
        args: &Value,
        settings_override: Option<&AppSettings>,
    ) -> Result<UpstreamCallOutcome, Value> {
        let settings = effective_settings(settings_override);
        if !settings.upstream_mcp.enabled {
            return Err(GatewayError::invalid(
                "UPSTREAM_MCP_DISABLED",
                "MCP 聚合功能当前已在设置中关闭。",
            )
            .to_rpc_error());
        }

        match tool_name {
            TOOL_UPSTREAM_LIST => self.handle_meta_list(&settings, args),
            TOOL_UPSTREAM_TOOLS => self.handle_meta_tools(&settings, args).await,
            TOOL_UPSTREAM_CALL => self.handle_meta_call(&settings, args).await,
            _ => {
                let route = self
                    .resolve_direct_tool(tool_name, Some(&settings))
                    .ok_or_else(|| {
                        GatewayError::not_found(
                            "UPSTREAM_TOOL_NOT_FOUND",
                            format!("未找到聚合工具 `{tool_name}`，可能对应上游 MCP 已禁用或工具已隐藏。"),
                        )
                        .to_rpc_error()
                    })?;
                let server = settings
                    .upstream_mcp
                    .servers
                    .iter()
                    .find(|s| s.id == route.server_id && s.enabled)
                    .ok_or_else(|| {
                        GatewayError::not_found(
                            "UPSTREAM_SERVER_UNAVAILABLE",
                            format!("上游 MCP `{}` 未启用或不存在。", route.alias),
                        )
                        .to_rpc_error()
                    })?;
                self.invoke_server_tool(&settings, server, &route.original_tool_name, args.clone())
                    .await
            }
        }
    }

    fn handle_meta_list(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        self.ensure_catalog_loaded();
        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let servers: Vec<Value> = settings
            .upstream_mcp
            .servers
            .iter()
            .filter(|s| s.enabled)
            .map(|srv| {
                let cached = catalogs.get(&srv.id);
                let slot = self.slot_for(&srv.id);
                let rt = slot.snapshot_status(srv.enabled, cached);
                let total_tools = cached.map(|c| c.tools.len()).unwrap_or(0);
                let enabled_tools = cached
                    .map(|c| {
                        c.tools
                            .iter()
                            .filter(|t| !srv.is_tool_disabled(&t.name))
                            .count()
                    })
                    .unwrap_or(0);
                json!({
                    "id": srv.id,
                    "alias": srv.alias,
                    "name": srv.name,
                    "description": srv.description,
                    "transport": srv.transport.kind_label(),
                    "expose_mode": srv.expose_mode.as_str(),
                    "status": rt.status,
                    "synced": cached.is_some(),
                    "tool_count": enabled_tools,
                    "total_tool_count": total_tools,
                    "last_error": rt.last_error,
                })
            })
            .collect();

        let structured = tool_ok(json!({
            "count": servers.len(),
            "servers": servers,
            "hint": "Use mcp_upstream_tools(server=..., query=...) to inspect tool schemas, then mcp_upstream_call(server=..., tool=..., arguments=...) to invoke."
        }));
        Ok(UpstreamCallOutcome {
            result: wrap_mcp_tool_result(TOOL_UPSTREAM_LIST, args, structured),
            execution_host: "local".into(),
        })
    }

    async fn handle_meta_tools(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let server_filter = args
            .get("server")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty());
        let include_schema = args
            .get("include_schema")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 50) as usize;

        let target_servers: Vec<&UpstreamServerConfig> = settings
            .upstream_mcp
            .servers
            .iter()
            .filter(|srv| {
                if !srv.enabled {
                    return false;
                }
                if let Some(filter) = server_filter {
                    srv.alias.eq_ignore_ascii_case(filter) || srv.id == filter
                } else {
                    true
                }
            })
            .collect();

        if let Some(filter) = server_filter {
            if target_servers.is_empty() {
                return Err(GatewayError::not_found(
                    "UPSTREAM_SERVER_NOT_FOUND",
                    format!("未找到已启用的上游 MCP 服务 `{filter}`；请先调用 mcp_upstream_list 查看可用别名。"),
                )
                .to_rpc_error());
            }
        }

        // Lazy-sync any selected server whose catalog has not been fetched yet or was invalidated by list_changed.
        self.ensure_catalog_loaded();
        let mut sync_warnings: Vec<Value> = Vec::new();
        for srv in &target_servers {
            let needs_sync = {
                let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
                let slot = self.slot_for(&srv.id);
                !catalogs.contains_key(&srv.id) || slot.tools_dirty.load(Ordering::Relaxed)
            };
            if needs_sync {
                if let Err(err) = self.sync_server_catalog(&srv.id, Some(settings)).await {
                    sync_warnings.push(json!({
                        "server": srv.alias,
                        "error": err
                    }));
                }
            }
        }

        let catalogs = self.catalogs.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<Value> = Vec::new();
        for srv in &target_servers {
            let Some(catalog) = catalogs.get(&srv.id) else {
                continue;
            };
            for tool in &catalog.tools {
                if srv.is_tool_disabled(&tool.name) {
                    continue;
                }
                if let Some(ref q) = query {
                    let in_name = tool.name.to_ascii_lowercase().contains(q);
                    let in_desc = tool.description.to_ascii_lowercase().contains(q);
                    let in_srv = srv.alias.to_ascii_lowercase().contains(q)
                        || srv.name.to_ascii_lowercase().contains(q);
                    if !in_name && !in_desc && !in_srv {
                        continue;
                    }
                }
                let mut item = json!({
                    "server": srv.alias,
                    "server_name": srv.name,
                    "expose_mode": srv.expose_mode.as_str(),
                    "tool": tool.name,
                    "description": tool.description,
                });
                if let Some(title) = &tool.title {
                    item["title"] = Value::String(title.clone());
                }
                if include_schema {
                    item["inputSchema"] = tool.input_schema.clone();
                }
                matched.push(item);
            }
        }
        drop(catalogs);

        let total = matched.len();
        let page: Vec<Value> = matched.into_iter().skip(offset).take(limit).collect();
        let has_more = offset.saturating_add(page.len()) < total;
        let structured = tool_ok(json!({
            "total": total,
            "offset": offset,
            "limit": limit,
            "has_more": has_more,
            "tools": page,
            "warnings": sync_warnings,
        }));
        Ok(UpstreamCallOutcome {
            result: wrap_mcp_tool_result(TOOL_UPSTREAM_TOOLS, args, structured),
            execution_host: "local".into(),
        })
    }

    async fn handle_meta_call(
        &self,
        settings: &AppSettings,
        args: &Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let server_key = args
            .get("server")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                GatewayError::invalid(
                    "INVALID_ARGUMENT",
                    "缺少必填参数 `server`（上游 MCP 别名或 ID）",
                )
                .to_rpc_error()
            })?;
        let tool_name = args
            .get("tool")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                GatewayError::invalid("INVALID_ARGUMENT", "缺少必填参数 `tool`（上游原始工具名）")
                    .to_rpc_error()
            })?;
        let nested_args = args
            .get("arguments")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));

        let server = settings
            .upstream_mcp
            .servers
            .iter()
            .find(|s| s.enabled && (s.alias.eq_ignore_ascii_case(server_key) || s.id == server_key))
            .ok_or_else(|| {
                GatewayError::not_found(
                    "UPSTREAM_SERVER_NOT_FOUND",
                    format!("未找到已启用的上游 MCP 服务 `{server_key}`。请先调用 mcp_upstream_list 核对。"),
                )
                .to_rpc_error()
            })?;

        if server.is_tool_disabled(tool_name) {
            return Err(GatewayError::invalid(
                "UPSTREAM_TOOL_DISABLED",
                format!(
                    "上游 MCP `{}` 的工具 `{tool_name}` 已在管理界面中禁用。",
                    server.alias
                ),
            )
            .to_rpc_error());
        }

        self.invoke_server_tool(settings, server, tool_name, nested_args)
            .await
    }

    async fn invoke_server_tool(
        &self,
        settings: &AppSettings,
        server: &UpstreamServerConfig,
        original_tool_name: &str,
        arguments: Value,
    ) -> Result<UpstreamCallOutcome, Value> {
        let client = self
            .get_or_connect(settings, server, false)
            .await
            .map_err(|err| {
                GatewayError::internal(
                    "UPSTREAM_CONNECT_FAILED",
                    format!("连接上游 MCP `{}` 失败: {err}", server.alias),
                )
                .to_rpc_error()
            })?;

        let timeout = Duration::from_secs(
            server
                .call_timeout_secs
                .clamp(MIN_CALL_TIMEOUT_SECS, MAX_CALL_TIMEOUT_SECS),
        );
        let started = Instant::now();
        let slot = self.slot_for(&server.id);
        let call_res = client
            .call_tool(original_tool_name, arguments, timeout)
            .await;
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let now_ms = unix_time_ms();

        match call_res {
            Ok(result) => {
                slot.set_status(|st| {
                    st.status = "connected".into();
                    st.last_error = None;
                    st.last_latency_ms = Some(elapsed_ms);
                    st.last_used_ms = Some(now_ms);
                    st.stderr_tail = client.stderr_tail();
                });
                Ok(UpstreamCallOutcome {
                    result,
                    execution_host: format!("upstream:{} ({})", server.alias, server.name),
                })
            }
            Err(err) => {
                let tail = client.stderr_tail();
                if !client.is_alive() {
                    let mut conn_guard = slot.connection.lock().await;
                    if let Some(active) = conn_guard.take() {
                        untrack_stdio_pid(active.client.stdio_pid());
                        active.client.shutdown().await;
                    }
                }
                slot.set_status(|st| {
                    st.status = "error".into();
                    st.last_error = Some(err.clone());
                    st.last_latency_ms = Some(elapsed_ms);
                    st.stderr_tail = tail;
                });
                Err(GatewayError::internal(
                    "UPSTREAM_TOOL_CALL_FAILED",
                    format!(
                        "调用上游 MCP `{}` 工具 `{original_tool_name}` 失败: {err}",
                        server.alias
                    ),
                )
                .to_rpc_error())
            }
        }
    }
}
