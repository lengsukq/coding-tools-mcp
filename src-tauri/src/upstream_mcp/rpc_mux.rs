//! Shared pending JSON-RPC request multiplexer for bidirectional upstream MCP transports
//! (`StdioTransport` and `LegacySseTransport`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::oneshot;

type RpcWaiter = oneshot::Sender<Result<Value, String>>;

#[derive(Clone, Default)]
pub(super) struct PendingRpcMap {
    inner: Arc<Mutex<HashMap<u64, RpcWaiter>>>,
}

impl PendingRpcMap {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn register(&self, id: u64) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        rx
    }

    pub(super) fn remove(&self, id: u64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }

    pub(super) fn resolve(&self, id: u64, value: Value) {
        let tx = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(tx) = tx {
            let _ = tx.send(Ok(value));
        }
    }

    pub(super) fn fail_all(&self, message: &str) {
        let drained: Vec<_> = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .collect();
        for (_, tx) in drained {
            let _ = tx.send(Err(message.to_string()));
        }
    }
}
