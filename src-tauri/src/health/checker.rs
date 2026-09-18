use serde::Serialize;

use crate::global_gateway::GatewayHealthItem;
use crate::workspace::RuntimeStatusDto;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthItem {
    pub label: String,
    pub ok: bool,
    pub detail: String,
    pub hint: String,
}

pub fn run_global_health_checks(
    global_runtime: &RuntimeStatusDto,
    gateway_health: Vec<GatewayHealthItem>,
) -> Vec<HealthItem> {
    let mut items = vec![
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
