//! An archive on a file system: the directory it writes under, the file a
//! key names there, and the location a receipt carries for it,
//! `file:///<root>/<key>`, read back.
//!
//! The file archive's and the Parquet archive's receipts are this one
//! shape (ADR-0040, amendment 2026-11-20): the file archive wrote it alone
//! until then, and the Parquet archive's receipt was the file's path as
//! the platform prints it. A key is the receipt's own segments under the
//! root, so a receipt names nothing outside it: a segment that is empty,
//! `.` or `..`, or that holds a `\` or a `:` — which Windows reads as a
//! separator or a drive — is refused.

use std::path::PathBuf;

use crate::ArchiveError;

/// The directory an archive writes under.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Directory {
    root: PathBuf,
}

impl Directory {
    /// The directory at `root`; nothing is made until an item is written.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The file `key` names under the root: its `/`-separated segments.
    ///
    /// # Errors
    /// A segment is empty, `.` or `..`, or holds a `\` or a `:`: the key
    /// names no file under the root.
    pub fn path_of(&self, key: &str) -> Result<PathBuf, ArchiveError> {
        let mut path = self.root.clone();
        for segment in key.split('/') {
            if segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.contains(['\\', ':'])
            {
                return Err(ArchiveError {
                    message: format!("{key} does not name an item under the archive"),
                });
            }
            path.push(segment);
        }
        Ok(path)
    }

    /// The location a receipt carries for the file at `key`.
    #[must_use]
    pub fn location(&self, key: &str) -> String {
        format!("{}{key}", self.prefix())
    }

    /// The key a receipt's location names.
    ///
    /// # Errors
    /// The location is not under this root, or its key names no file
    /// under it ([`Directory::path_of`]).
    pub fn key<'a>(&self, location: &'a str) -> Result<&'a str, ArchiveError> {
        let key = location
            .strip_prefix(&self.prefix())
            .ok_or_else(|| ArchiveError {
                message: format!(
                    "{location} is not a receipt of the archive at {}",
                    self.root.display()
                ),
            })?;
        self.path_of(key)?;
        Ok(key)
    }

    /// What every receipt of this root shares: `file:///<root>/`.
    fn prefix(&self) -> String {
        format!("file://{}/", net::uri::path_of(&self.root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_key_is_a_file_under_the_root_and_its_location_reads_back() {
        let directory = Directory::new(Path::new("archive"));
        let location = directory.location("json/order_1.parquet");
        assert_eq!(location, "file:///archive/json/order_1.parquet");
        assert_eq!(
            directory.key(&location).expect("a key"),
            "json/order_1.parquet"
        );
        assert_eq!(
            directory.path_of("json/order_1.parquet").expect("a file"),
            Path::new("archive").join("json").join("order_1.parquet")
        );
    }

    #[test]
    fn a_key_naming_anything_outside_the_root_is_refused() {
        let directory = Directory::new(Path::new("archive"));
        for key in [
            "",
            "json//x",
            "json/./x",
            "json/../../x",
            "json/..\\..\\x",
            "C:x",
            "json/C:\\x",
        ] {
            let refused = directory.path_of(key).expect_err(key);
            assert!(
                refused.message.contains("does not name an item"),
                "{refused}"
            );
            let refused = directory.key(&directory.location(key)).expect_err(key);
            assert!(
                refused.message.contains("does not name an item"),
                "{refused}"
            );
        }
        let refused = directory
            .key("file:///elsewhere/json/x")
            .expect_err("another root");
        assert!(refused.message.contains("not a receipt"), "{refused}");
    }
}
