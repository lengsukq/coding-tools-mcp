use std::io::{self, Write};

use serde::Serialize;
use sha2::{Digest, Sha256};

struct CountingWriter {
    bytes: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Compute the compact JSON serialized byte length without allocating an output buffer.
pub fn serialized_json_bytes<T: Serialize>(value: &T) -> Option<usize> {
    let mut writer = CountingWriter { bytes: 0 };
    serde_json::to_writer(&mut writer, value).ok()?;
    Some(writer.bytes)
}

struct Sha256CountingWriter {
    hasher: Sha256,
    bytes: usize,
}

impl Write for Sha256CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.update(buf);
        self.bytes = self.bytes.saturating_add(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Stream-serialize a JSON value to compute both its byte length and `"sha256:<hex>"`
/// digest in a single zero-buffer pass.
pub fn hash_and_count_json<T: Serialize>(value: &T) -> Option<(usize, String)> {
    let mut writer = Sha256CountingWriter {
        hasher: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, value).ok()?;
    let digest = writer.hasher.finalize();
    let hash = format!("sha256:{}", crate::util::hash::encode_hex_lower(&digest));
    Some((writer.bytes, hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::hash::sha256_hex;
    use serde_json::json;

    #[test]
    fn counting_and_hashing_writers_match_serialized_vec() {
        let payload = json!({"hello": "world", "items": [1, 2, 3]});
        let expected_bytes = serde_json::to_vec(&payload).expect("vec");
        let expected_hash = format!("sha256:{}", sha256_hex(&expected_bytes));
        assert_eq!(serialized_json_bytes(&payload), Some(expected_bytes.len()));
        assert_eq!(
            hash_and_count_json(&payload),
            Some((expected_bytes.len(), expected_hash))
        );
    }
}
