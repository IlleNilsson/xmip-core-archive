#![forbid(unsafe_code)]

//! What an archive is to Xmip, and what every archive technology shares.
//!
//! An [`ArchiveStore`] takes an [`ArchiveItem`] — a data type, an
//! identifier, the bytes and the metadata pairs — and answers an
//! [`ArchiveReceipt`] it can restore the item from. Archiving is the last
//! thing Xmip does with a piece of data (ADR-0040): the store keeps it, the
//! archive owner decides what becomes of it, Xmip never deletes.
//!
//! The technologies — parquet, sqlite, file, sql, postgresql, mssql, mysql,
//! s3, azure-blob, gcs — each decide how a row, a file or an object holds
//! the four fields. What they hold in common lives here rather than in ten
//! copies (ADR-0044): `metadata` is the pairs as one text, `timestamp` is
//! when an item was archived, `layout` is where a name-keyed store puts
//! it, `checksum` is what a receipt carries.

pub mod checksum;
pub mod layout;
pub mod metadata;
pub mod timestamp;

use std::error::Error;
use std::fmt;

/// One retained item on its way to the archive: what it is, which one it
/// is, its bytes, and the pairs an integration attached to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveItem {
    pub data_type: String,
    pub identifier: String,
    pub bytes: Vec<u8>,
    pub metadata: Vec<(String, String)>,
}

/// Where a store put an item, as a URI the same store restores from, and
/// the checksum of its bytes where the store computes one cheaply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReceipt {
    pub location: String,
    pub checksum: Option<String>,
}

/// Why a store could not archive or restore, in words.
#[derive(Debug)]
pub struct ArchiveError {
    pub message: String,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for ArchiveError {}

/// A store items are archived into and restored from.
pub trait ArchiveStore: Send + Sync {
    /// Put `item` in the store and answer the receipt it restores from.
    ///
    /// # Errors
    /// The store refused or could not reach its medium.
    fn archive(&self, item: ArchiveItem) -> Result<ArchiveReceipt, ArchiveError>;
    /// The item a receipt from this store names.
    ///
    /// # Errors
    /// The receipt is not this store's, names nothing, or the bytes no
    /// longer match its checksum.
    fn restore(&self, receipt: &ArchiveReceipt) -> Result<ArchiveItem, ArchiveError>;
}
