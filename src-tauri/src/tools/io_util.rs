//! Domain-local facade combining `crate::util` filesystem/text/hash primitives
//! with streaming JSON measurement and zero-allocation path-component filters.

use std::ffi::OsStr;
use std::io::{self, Write};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::tools::workspace::DEFAULT_EXCLUDED_NAMES;
pub(crate) use crate::util::fs::{atomic_write_bytes, lock_file as acquire_file_lock, FileLock};
pub(crate) use crate::util::hash::sha256_hex;
pub(crate) use crate::util::text::truncate_utf8;

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
pub(crate) fn serialized_json_bytes<T: Serialize>(value: &T) -> Option<usize> {
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
pub(crate) fn hash_and_count_json<T: Serialize>(value: &T) -> Option<(usize, String)> {
    let mut writer = Sha256CountingWriter {
        hasher: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, value).ok()?;
    let digest = writer.hasher.finalize();
    let hash = format!("sha256:{}", crate::util::hash::encode_hex_lower(&digest));
    Some((writer.bytes, hash))
}

/// Zero-allocation check against `DEFAULT_EXCLUDED_NAMES` for directory pruning.
pub(crate) fn is_default_excluded_os(name: &OsStr) -> bool {
    let Some(text) = name.to_str() else {
        return false;
    };
    DEFAULT_EXCLUDED_NAMES.contains(&text)
}

/// Zero-allocation check for a single path component during `WalkDir::filter_entry`.
pub(crate) fn is_ignored_os_component(
    name: &OsStr,
    include_hidden: bool,
    include_ignored: bool,
) -> bool {
    if !include_hidden && is_hidden_os_component(name) {
        return true;
    }
    if !include_ignored && is_default_excluded_os(name) {
        return true;
    }
    false
}

fn is_hidden_os_component(name: &OsStr) -> bool {
    if let Some(text) = name.to_str() {
        return text.starts_with('.') && text != ".";
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = name.as_bytes();
        bytes.starts_with(b".") && bytes != b"."
    }
    #[cfg(not(unix))]
    {
        let text = name.to_string_lossy();
        text.starts_with('.') && text != "."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn is_ignored_os_component_matches_hidden_and_excluded() {
        assert!(is_ignored_os_component(
            OsStr::new("node_modules"),
            false,
            false
        ));
        assert!(!is_ignored_os_component(
            OsStr::new("node_modules"),
            false,
            true
        ));
        assert!(is_ignored_os_component(OsStr::new(".git"), false, true));
        assert!(is_ignored_os_component(OsStr::new(".git"), true, false));
        assert!(!is_ignored_os_component(OsStr::new(".git"), true, true));
        assert!(!is_ignored_os_component(OsStr::new("src"), false, false));
    }
}
