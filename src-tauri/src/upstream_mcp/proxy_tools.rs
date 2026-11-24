use serde_json::{json, Value};

pub const TOOL_UPSTREAM_LIST: &str = "mcp_upstream_list";
pub const TOOL_UPSTREAM_TOOLS: &str = "mcp_upstream_tools";
pub const TOOL_UPSTREAM_CALL: &str = "mcp_upstream_call";

pub fn is_proxy_meta_tool(name: &str) -> bool {
    matches!(
        name,
        TOOL_UPSTREAM_LIST | TOOL_UPSTREAM_TOOLS | TOOL_UPSTREAM_CALL
    )
}

pub fn proxy_meta_tool_definitions() -> Vec<Value> {
    vec![
        meta_tool(
            TOOL_UPSTREAM_LIST,
            "List aggregated MCP servers",
            "List external MCP servers aggregated in Coding Tools MCP (alias, name, description, expose_mode, runtime status, cached tool count). Call this first to discover proxy-mode upstream MCP servers.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            true,
            false,
        ),
        meta_tool(
            TOOL_UPSTREAM_TOOLS,
            "Search or list upstream MCP tools",
            "List or search tools provided by aggregated upstream MCP servers along with their exact inputSchema. Filter by server alias/id or keyword query before calling mcp_upstream_call.",
            json!({
                "type": "object",
                "properties": {
                    "server": {
                        "type": "string",
                        "description": "Optional upstream server alias or id from mcp_upstream_list."
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional case-insensitive keyword to match tool name or description."
                    },
                    "include_schema": {
                        "type": "boolean",
                        "description": "Include full JSON inputSchema for each matched tool (default true)."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Pagination offset (default 0)."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Maximum tools returned per page (default 20, max 50)."
                    }
                },
                "additionalProperties": false
            }),
            true,
            false,
        ),
        meta_tool(
            TOOL_UPSTREAM_CALL,
            "Call upstream MCP tool",
            "Invoke a tool exposed by an aggregated upstream MCP server. Inspect the exact server alias, tool name, and inputSchema via mcp_upstream_tools first.",
            json!({
                "type": "object",
                "properties": {
                    "server": {
                        "type": "string",
                        "description": "Upstream server alias or id returned by mcp_upstream_list."
                    },
                    "tool": {
                        "type": "string",
                        "description": "Original tool name on the upstream MCP server (without alias prefix)."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "JSON arguments object matching the upstream tool's inputSchema."
                    }
                },
                "required": ["server", "tool"],
                "additionalProperties": false
            }),
            false,
            true,
        ),
    ]
}

fn meta_tool(
    name: &str,
    title: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    open_world: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": read_only,
            "openWorldHint": open_world
        }
    })
}
