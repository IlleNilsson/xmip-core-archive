#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveItem {
    pub data_type: String,
    pub identifier: String,
    pub bytes: Vec<u8>,
    pub metadata: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveReceipt {
    pub location: String,
    pub checksum: Option<String>,
}

#[derive(Debug)]
pub struct ArchiveError {
    pub message: String,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.message) }
}
impl Error for ArchiveError {}

pub trait ArchiveStore: Send + Sync {
    fn archive(&self, item: ArchiveItem) -> Result<ArchiveReceipt, ArchiveError>;
    fn restore(&self, receipt: &ArchiveReceipt) -> Result<ArchiveItem, ArchiveError>;
}
