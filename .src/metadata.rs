//! The metadata pairs of an item as one text, and back.
//!
//! Every archive technology keeps an item's four fields — `data_type`,
//! `identifier`, `bytes`, `metadata` — and where the store has one text
//! column or one object for the metadata, the pairs travel in it like this:
//! record separator between pairs, unit separator between key and value.
//! The two control characters are what the separators were made for and
//! appear in no key or value an integration writes. One encoding across
//! the column-and-object technologies — parquet, sqlite, sql, postgresql,
//! mssql, mysql, s3, azure-blob, gcs — means a Parquet file, a `SQLite`
//! row and an S3 object read the same to an operator.
//!
//! The file archive is the one exception, on purpose: its `.meta` sidecar
//! is TOML an operator reads in any editor, and it carries the original
//! `data_type` and `identifier` beside the pairs because the file name
//! they became is sanitized and cannot give them back. That sidecar lives
//! with the file technology, not here.

/// Between pairs.
pub const PAIR: char = '\u{1e}';
/// Between a key and its value.
pub const KV: char = '\u{1f}';

/// The pairs as the one text a store holds.
#[must_use]
pub fn encode(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}{KV}{value}"))
        .collect::<Vec<_>>()
        .join(&PAIR.to_string())
}

/// The pairs a store's text holds.
#[must_use]
pub fn decode(encoded: &str) -> Vec<(String, String)> {
    if encoded.is_empty() {
        return Vec::new();
    }
    encoded
        .split(PAIR)
        .filter_map(|pair| pair.split_once(KV))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_survive_the_one_text() {
        let pairs = vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "two words".to_string()),
        ];
        assert_eq!(decode(&encode(&pairs)), pairs);
        assert_eq!(encode(&[]), "");
        assert!(decode("").is_empty());
        assert!(decode("no-unit-separator").is_empty());
    }
}
