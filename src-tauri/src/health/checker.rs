use std::path::Path;

use serde::Serialize;

use crate::global_gateway::GatewayHealthItem;
use crate::workspace::{RuntimeStatusDto, WorkspaceProfile};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthItem {
    pub label: String,
    pub ok: bool,
    pub detail: String,
    pub hint: String,
}

pub fn run_health_checks(
    profile: &WorkspaceProfile,
    global_runtime: &RuntimeStatusDto,
    gateway_health: Vec<GatewayHealthItem>,
) -> Vec<HealthItem> {
    let root = Path::new(&profile.path);
    let mut items = vec![
        health_item(
            "Workspace Root",
            root.is_dir(),
            if root.is_dir() {
                profile.path.clone()
            } else {
                format!("目录不可用：{}", profile.path)
            },
            "确认 Workspace 目录仍然存在，并在工作区设置中更新路径。",
        ),
        health_item(
            "Workspace Context",
            !profile.runtime.tool_profile.trim().is_empty(),
            format!(
                "Tool Profile={} · Policy={}",
                profile.runtime.tool_profile, profile.runtime.permission_mode
            ),
            "检查 Workspace Tool Profile 与执行策略配置。",
        ),
        health_item(
            "Global MCP Runtime",
            global_runtime.state == "running",
            format!(
                "{} · {}",
                global_runtime.state, global_runtime.local_endpoint
            ),
            "Global MCP 是所有 Workspace 共用的唯一连接，请在工作台或设置中启动它。",
        ),
    ];

    items.extend(gateway_health.into_iter().map(|item| HealthItem {
        label: item.label,
        ok: item.ok,
        detail: item.detail,
        hint: if item.ok {
            String::new()
        } else {
            "检查 Global MCP 连接/公网 Tunnel 配置；该状态不属于单个 Workspace。".into()
        },
    }));
    items
}

fn health_item(label: &str, ok: bool, detail: String, hint: &str) -> HealthItem {
    HealthItem {
        label: label.into(),
        ok,
        detail,
        hint: if ok { String::new() } else { hint.into() },
    }
}
