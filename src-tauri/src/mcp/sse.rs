//! Shared Server-Sent Events (SSE) payload parser for MCP HTTP clients.

/// Extract the `data:` payload of each SSE event block in `body`.
pub(crate) fn parse_sse_data_events(body: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut current_lines = Vec::new();

    for raw_line in body.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            if !current_lines.is_empty() {
                events.push(current_lines.join("\n"));
                current_lines.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            current_lines.push(rest.trim_start().to_string());
        }
    }
    if !current_lines.is_empty() {
        events.push(current_lines.join("\n"));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_multiline_sse_data_events() {
        let body = "event: message\ndata: {\"a\":1}\n\ndata: line1\ndata: line2\r\n\r\n";
        assert_eq!(
            parse_sse_data_events(body),
            vec!["{\"a\":1}".to_string(), "line1\nline2".to_string()]
        );
    }
}
