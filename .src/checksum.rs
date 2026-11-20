//! The checksum a receipt carries: SHA-256 of the bytes as lower-case hex.
//!
//! A store that can compute it cheaply — a file, a script, an object put —
//! puts it in the receipt, and a restore refuses bytes that no longer match
//! it ([`verify`], once for every such store since 2026-11-20: a name-keyed
//! store's, the Parquet archive's and the SQL script archive's restore). A
//! database store answers `None`; its row id is its integrity.

use codec::hex;

use crate::{ArchiveError, ArchiveReceipt};

/// SHA-256 of `bytes`, sixty-four lower-case hex digits.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(&codec::digest::sha256(bytes))
}

/// The bytes `receipt` names, checked against it: their SHA-256 must be the
/// receipt's, where it carries one, and `kept` — what the store keeps
/// beside them, a name-keyed store's metadata — where it keeps one.
///
/// # Errors
/// They disagree: these are not the bytes archived under the receipt.
pub fn verify(
    receipt: &ArchiveReceipt,
    kept: Option<&str>,
    bytes: &[u8],
) -> Result<(), ArchiveError> {
    let actual = sha256_hex(bytes);
    let receipted = receipt.checksum.as_deref().unwrap_or(&actual);
    if receipted == actual && kept.is_none_or(|kept| kept == actual) {
        return Ok(());
    }
    let kept = kept.map_or_default(|kept| format!(", the metadata {kept}"));
    Err(ArchiveError {
        message: format!(
            "checksum mismatch at {}: the receipt says {receipted}{kept}, the bytes {actual}",
            receipt.location
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_the_known_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b"").len(), 64);
    }

    #[test]
    fn bytes_are_verified_against_the_receipt_and_what_the_store_kept() {
        let abc = sha256_hex(b"abc");
        let receipt = |checksum: Option<&str>| ArchiveReceipt {
            location: "file:///archive/json/a".to_string(),
            checksum: checksum.map(str::to_string),
        };
        verify(&receipt(Some(&abc)), Some(&abc), b"abc").expect("all agree");
        verify(&receipt(Some(&abc)), None, b"abc").expect("nothing kept");
        verify(&receipt(None), Some(&abc), b"abc").expect("no receipt checksum");
        let refused = verify(&receipt(Some(&abc)), None, b"abd").expect_err("other bytes");
        assert!(
            refused
                .message
                .starts_with("checksum mismatch at file:///archive/json/a"),
            "{refused}"
        );
        assert!(!refused.message.contains("metadata"), "{refused}");
        let refused = verify(&receipt(None), Some(&abc), b"abd").expect_err("kept other");
        assert!(
            refused.message.contains(&format!("the metadata {abc}")),
            "{refused}"
        );
        let lying = receipt(Some(&"0".repeat(64)));
        verify(&lying, Some(&abc), b"abc").expect_err("the receipt lies");
    }
}
