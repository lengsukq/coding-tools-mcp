use serde_json::{json, Value};

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "computer_session",
            "title": "Computer session",
            "description": "Legacy temporary Computer Use session (request/status/stop). If the Mac owner already enabled Computer Use in local settings, DO NOT request a session: use computer_observe / computer_action directly, without session_token. For legacy temporary approvals only, call request and retain the private session_token across calls. Never expose tokens. Only local desktop UI can enable persistent access.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["request", "status", "stop"] },
                "scope": { "type": "string", "enum": ["observe", "control"], "default": "observe" },
                "device_id": { "type": "string", "description": "Optional target device_id from device_list (or 'local') when multiple LAN computers are paired." },
                "session_token": { "type": "string", "description": "Needed only when using legacy per-session approval without a persistent MCP header; NOT required for locally enabled Computer Use." }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false }
        }),
        json!({
            "name": "computer_observe",
            "title": "Observe computer screen",
            "description": "Read windows or a ScreenCaptureKit screenshot. If the Mac user manually enabled observe/control in Coding Tools MCP settings, call directly with NO computer_session(request) and NO session_token. Each OAuth/Bearer-authenticated request is bound to that client's identity and audited. If the user has not enabled it, use legacy per-session approval. Coordinates are global macOS points.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["screenshot", "windows", "ui_tree"] },
                "device_id": { "type": "string", "description": "Optional target device_id from device_list (or 'local') when multiple LAN computers are paired." },
                "session_token": { "type": "string", "description": "Legacy approvals only; omit for locally enabled Computer Use." },
                "display_index": { "type": "integer", "minimum": 1, "maximum": 16, "default": 1 },
                "display_id": { "type": "integer", "minimum": 1 },
                "window_id": { "type": "integer", "minimum": 1 },
                "quality": { "type": "string", "enum": ["fast", "detailed"], "default": "fast", "description": "Fast 1280x850 JPEG by default; detailed up to 1920x1200." },
                "region": { "type": "object", "properties": {
                    "x": { "type": "number" }, "y": { "type": "number" },
                    "width": { "type": "number", "exclusiveMinimum": 0 },
                    "height": { "type": "number", "exclusiveMinimum": 0 }
                }, "required": ["x", "y", "width", "height"], "additionalProperties": false }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "openWorldHint": true }
        }),
        json!({
            "name": "computer_action",
            "title": "Control approved Mac application",
            "description": "Execute mouse/keyboard action ONLY while Mac owner enabled local CONTROL permission for one selected application. When local policy is enabled, call directly without computer_session(request) or session_token; requests are authenticated and audited. Otherwise legacy per-session approval applies. Pointer actions require a recent matching snapshot_id; sensitive shortcuts/semantic send/delete/payment require separate 60-second local one-time confirmation.",
            "inputSchema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["click", "double_click", "right_click", "move", "drag", "scroll", "type", "key", "press_element", "focus_app", "open_url"] },
                "device_id": { "type": "string", "description": "Optional target device_id from device_list (or 'local') when multiple LAN computers are paired." },
                "session_token": { "type": "string", "description": "Legacy approvals only; omit for locally enabled Computer Use." },
                "x": { "type": "number" }, "y": { "type": "number" },
                "end_x": { "type": "number" }, "end_y": { "type": "number" },
                "amount": { "type": "integer", "minimum": -15, "maximum": 15 },
                "text": { "type": "string", "maxLength": 2000 },
                "url": { "type": "string", "maxLength": 2048, "description": "HTTP(S) navigation of the locally approved web browser; no file or script schemes." },
                "label": { "type": "string", "minLength": 1, "maxLength": 160 },
                "role": { "type": "string", "maxLength": 80 },
                "snapshot_id": { "type": "string", "description": "Required for all pointer actions; returned by computer_observe screenshot; valid for 45 seconds" },
                "confirmation_id": { "type": "string", "description": "For locally approved sensitive actions only. After approval retry the exact same request with this ID; one-time consumption, 60 second TTL." },
                "coordinate_space": { "type": "string", "enum": ["screen_points", "image_pixels"], "default": "screen_points" },
                "key": { "type": "string", "minLength": 1, "maxLength": 12 },
                "modifiers": { "type": "array", "uniqueItems": true, "maxItems": 4,
                    "items": { "type": "string", "enum": ["command", "shift", "control", "option"] } }
            }, "required": ["action"], "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
        }),
    ]
}
