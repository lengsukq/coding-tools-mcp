use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

const BYTES_PER_ESTIMATED_TOKEN: u64 = 4;

/// Runtime-local usage counters. Only aggregate sizes and counts are retained.
#[derive(Debug, Default)]
pub(crate) struct ServiceUsage {
    request_count: AtomicU64,
    tool_call_count: AtomicU64,
    error_count: AtomicU64,
    input_bytes: AtomicU64,
    output_bytes: AtomicU64,
}

impl ServiceUsage {
    pub(crate) fn record(
        &self,
        input_bytes: usize,
        output_bytes: usize,
        is_tool_call: bool,
        is_error: bool,
    ) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        if is_tool_call {
            self.tool_call_count.fetch_add(1, Ordering::Relaxed);
        }
        if is_error {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        self.input_bytes
            .fetch_add(input_bytes as u64, Ordering::Relaxed);
        self.output_bytes
            .fetch_add(output_bytes as u64, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self, workspace_id: &str, service: &str) -> ServiceUsageStats {
        let input_bytes = self.input_bytes.load(Ordering::Relaxed);
        let output_bytes = self.output_bytes.load(Ordering::Relaxed);
        let estimated_input_tokens = estimate_tokens(input_bytes);
        let estimated_output_tokens = estimate_tokens(output_bytes);

        ServiceUsageStats {
            workspace_id: workspace_id.to_string(),
            service: service.to_string(),
            request_count: self.request_count.load(Ordering::Relaxed),
            tool_call_count: self.tool_call_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            input_bytes,
            output_bytes,
            estimated_input_tokens,
            estimated_output_tokens,
            estimated_tokens: estimated_input_tokens.saturating_add(estimated_output_tokens),
        }
    }

    pub(crate) fn empty(workspace_id: &str, service: &str) -> ServiceUsageStats {
        ServiceUsageStats {
            workspace_id: workspace_id.to_string(),
            service: service.to_string(),
            request_count: 0,
            tool_call_count: 0,
            error_count: 0,
            input_bytes: 0,
            output_bytes: 0,
            estimated_input_tokens: 0,
            estimated_output_tokens: 0,
            estimated_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceUsageStats {
    pub workspace_id: String,
    pub service: String,
    pub request_count: u64,
    pub tool_call_count: u64,
    pub error_count: u64,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
    pub estimated_tokens: u64,
}

fn estimate_tokens(bytes: u64) -> u64 {
    bytes
        .saturating_add(BYTES_PER_ESTIMATED_TOKEN - 1)
        / BYTES_PER_ESTIMATED_TOKEN
}

#[cfg(test)]
mod tests {
    use super::{estimate_tokens, ServiceUsage};

    #[test]
    fn records_counts_and_json_sizes_without_retaining_content() {
        let usage = ServiceUsage::default();
        usage.record(8, 9, true, false);
        usage.record(4, 7, false, true);

        let stats = usage.snapshot("workspace-1", "mcp");
        assert_eq!(stats.request_count, 2);
        assert_eq!(stats.tool_call_count, 1);
        assert_eq!(stats.error_count, 1);
        assert_eq!(stats.input_bytes, 12);
        assert_eq!(stats.output_bytes, 16);
        assert_eq!(stats.estimated_input_tokens, 3);
        assert_eq!(stats.estimated_output_tokens, 4);
        assert_eq!(stats.estimated_tokens, 7);
    }

    #[test]
    fn token_estimation_rounds_up_each_payload() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(1), 1);
        assert_eq!(estimate_tokens(4), 1);
        assert_eq!(estimate_tokens(5), 2);
    }
}
