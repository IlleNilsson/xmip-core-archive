//! The checksum a receipt carries: SHA-256 of the bytes as lower-case hex.
//!
//! A store that can compute it cheaply — a file, a script, an object put —
//! puts it in the receipt, and a restore refuses bytes that no longer match
//! it. A database store answers `None`; its row id is its integrity.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// SHA-256 of `bytes`, sixty-four lower-case hex digits.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
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
}
