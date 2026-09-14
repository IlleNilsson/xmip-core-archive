//! What a technology's test archives: one item every technology's tests
//! agree on, and the seconds a test waits for a far end.
//!
//! Compiled for the capability's own tests and, under the `test-support`
//! feature, for a technology's — never into a production build. Nine
//! technologies each carried these two functions until 2026-09-14; they
//! moved up to the capability (ADR-0044). A technology whose test wants a
//! different item — the file archive's, with quotes and a newline in a
//! value — writes its own.

use std::time::Duration;

use crate::ArchiveItem;

/// A JSON item called `id`, with one metadata pair.
#[must_use]
pub fn item(id: &str) -> ArchiveItem {
    ArchiveItem {
        data_type: "json".to_string(),
        identifier: id.to_string(),
        bytes: b"{\"kept\":true}".to_vec(),
        metadata: vec![("source".to_string(), "playground".to_string())],
    }
}

/// `n` seconds, as a test writes a timeout.
#[must_use]
pub const fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_is_the_one_every_test_archives() {
        let fixture = item("json#1");
        assert_eq!(fixture.identifier, "json#1");
        assert_eq!(fixture.data_type, "json");
        assert_eq!(fixture.bytes, b"{\"kept\":true}");
        assert_eq!(fixture.metadata.len(), 1);
        assert_eq!(secs(2), Duration::from_secs(2));
    }
}
