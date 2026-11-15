use serde_json::Value;

pub(super) fn sensitive_action_description(args: &Value) -> Option<String> {
    match args.get("action").and_then(Value::as_str)? {
        "press_element" => {
            let label = args["label"].as_str()?.trim();
            let normalized = label.to_lowercase();
            let sensitive = [
                "delete",
                "remove",
                "pay",
                "purchase",
                "submit",
                "send",
                "erase",
                "delete all",
                "transfer",
                "删除",
                "支付",
                "付款",
                "购买",
                "提交",
                "发送",
                "转账",
                "移除",
                "清空",
                "确认订单",
                "卸载",
                "永久",
            ];
            sensitive
                .iter()
                .any(|needle| normalized.contains(needle))
                .then(|| format!("语义按钮：{}", label.chars().take(100).collect::<String>()))
        }
        "key" => {
            let modifiers = args.get("modifiers")?.as_array()?;
            let command = modifiers.iter().any(|v| v == "command");
            let control = modifiers.iter().any(|v| v == "control");
            let key = args["key"].as_str()?.to_ascii_lowercase();
            ((command && ["q", "w", "v", "delete", "backspace"].contains(&key.as_str()))
                || (control && ["delete", "backspace"].contains(&key.as_str())))
            .then(|| {
                format!(
                    "高风险快捷键：{} + {}",
                    if command { "Command" } else { "Control" },
                    key
                )
            })
        }
        _ => None,
    }
}

pub(super) fn validate_action(args: &Value) -> Result<(), String> {
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    let required = match action {
        "click" | "double_click" | "right_click" | "move" => &["x", "y"][..],
        "drag" => &["x", "y", "end_x", "end_y"][..],
        "scroll" => &["amount"][..],
        "type" => &["text"][..],
        "open_url" => &["url"][..],
        "key" | "focus_app" => {
            if action == "key" {
                &["key"][..]
            } else {
                &[][..]
            }
        }
        "press_element" => &["label"][..],
        _ => return Err("INVALID_ACTION: unsupported action".into()),
    };
    for field in required {
        let value = args
            .get(*field)
            .ok_or_else(|| format!("INVALID_ARGUMENT: missing {field}"))?;
        if matches!(*field, "x" | "y" | "end_x" | "end_y")
            && !value.as_f64().is_some_and(f64::is_finite)
        {
            return Err(format!("INVALID_COORDINATE: invalid {field}"));
        }
        if *field == "amount" && !value.as_i64().is_some_and(|n| (-15..=15).contains(&n)) {
            return Err("INVALID_SCROLL: amount out of range".into());
        }
        if matches!(*field, "text" | "key" | "label" | "url") && !value.is_string() {
            return Err(format!("INVALID_ARGUMENT: {field} must be string"));
        }
    }
    if action == "type" && args["text"].as_str().unwrap_or("").encode_utf16().count() > 2000 {
        return Err("INVALID_TEXT: too long".into());
    }
    if action == "open_url" {
        let url = args["url"].as_str().unwrap_or("");
        if url.len() > 2048
            || url.chars().any(char::is_whitespace)
            || url.chars().any(char::is_control)
            || !(url.starts_with("https://") || url.starts_with("http://"))
        {
            return Err("INVALID_BROWSER_URL: only bounded HTTP(S) URLs are supported".into());
        }
    }
    if action == "press_element"
        && !args["label"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.chars().count() <= 160)
    {
        return Err("INVALID_ELEMENT_LABEL".into());
    }
    if action == "key" {
        if let Some(modifiers) = args.get("modifiers") {
            let Some(all) = modifiers.as_array() else {
                return Err("INVALID_MODIFIERS".into());
            };
            if all.len() > 4
                || all.iter().any(|m| {
                    !matches!(m.as_str(), Some("command" | "shift" | "control" | "option"))
                })
            {
                return Err("INVALID_MODIFIERS".into());
            }
        }
    }
    Ok(())
}
