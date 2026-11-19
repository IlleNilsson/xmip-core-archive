//! The one rule of an archive whose items sit under names — a directory, a
//! bucket, a container: an archived item is never written over (ADR-0040,
//! amendment 2026-11-19).
//!
//! An item is two objects: its bytes at `<prefix>/<data_type>/<identifier>`
//! and its metadata, the TOML sidecar (`crate::sidecar`), at
//! `<prefix>/.meta/<data_type>/<identifier>` (`crate::layout`). A store
//! implements [`Named`]: it creates an object only where none is, never
//! replaces one, reads one back, and says where a key is as the location a
//! receipt carries. [`archive`] and [`restore`] are the rest, once, for
//! every such store:
//!
//! 1. The metadata is created first: it is the claim. Where it is there
//!    already, the item it names — data type, identifier and checksum —
//!    must be this one, archived before or interrupted, and the archive
//!    carries on; any other item is refused, and nothing is written.
//! 2. The bytes are created next. Where they are there already, their
//!    SHA-256 must be the item's; other bytes are refused.
//! 3. Only then is there a receipt. A store answers [`Created`] once the
//!    object is durable: an object store's success, a file synced with
//!    its directory.
//!
//! So an identifier archived again with the same bytes answers the same
//! receipt, with other bytes is refused while the earlier receipt still
//! restores, and an archive stopped between the two objects is completed by
//! its repeat and refuses another item. A restore reads the metadata first:
//! where there is none, the location is not an item this archive completed,
//! and it is refused; the receipt's checksum, the metadata's and the
//! SHA-256 of the bytes read must agree.

use crate::checksum::sha256_hex;
use crate::sidecar::Sidecar;
use crate::{ArchiveError, ArchiveItem, ArchiveReceipt, layout, timestamp};

/// What creating an object found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Created {
    /// The object is new, and durable.
    Created,
    /// An object was at the key already, and is left as it was.
    Exists,
}

/// A store that keeps objects under names and never writes one over.
pub trait Named {
    /// `bytes` as a new object at `key`: [`Created::Created`] once it is
    /// durable, [`Created::Exists`] where an object is at `key` already,
    /// which is left as it was.
    ///
    /// # Errors
    /// The store refused or could not reach its medium.
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Created, ArchiveError>;

    /// The object at `key`, or `None` where there is none.
    ///
    /// # Errors
    /// The store refused or could not reach its medium.
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, ArchiveError>;

    /// The location a receipt carries for the object at `key`.
    fn location(&self, key: &str) -> String;

    /// The key a receipt's location names.
    ///
    /// # Errors
    /// The location is not one of this store's.
    fn key(&self, location: &str) -> Result<String, ArchiveError>;

    /// What a data type or an identifier becomes in a key. An object key
    /// holds any character, so by default it is kept as it is; a store
    /// whose names are file names makes it one (`layout::sanitize`), and
    /// two that become the same are refused by the metadata already there.
    fn segment(&self, text: &str) -> String {
        text.to_string()
    }
}

/// Which item a name is claimed for: its data type, its identifier and the
/// SHA-256 of its bytes. Two claims of one name are the same item, or the
/// later is refused.
#[derive(Clone, Copy, Debug)]
pub struct Claim<'a> {
    pub data_type: &'a str,
    pub identifier: &'a str,
    pub checksum: &'a str,
}

impl<'a> Claim<'a> {
    /// The claim a sidecar records.
    #[must_use]
    pub fn of(sidecar: &'a Sidecar) -> Self {
        Self {
            data_type: &sidecar.data_type,
            identifier: &sidecar.identifier,
            checksum: &sidecar.checksum,
        }
    }

    /// This claim at `location`, where `held` was claimed first: the same
    /// item — archived before, or interrupted — carries on.
    ///
    /// # Errors
    /// `held` is another item laid out at the same name, or the same item
    /// with other bytes: an archived item is never written over.
    pub fn against(&self, held: &Claim<'_>, location: &str) -> Result<(), ArchiveError> {
        if (held.data_type, held.identifier) != (self.data_type, self.identifier) {
            return Err(ArchiveError {
                message: format!(
                    "{location} holds the data type {:?}, identifier {:?}: {:?}, {:?} is laid \
                     out at the same name and is refused",
                    held.data_type, held.identifier, self.data_type, self.identifier
                ),
            });
        }
        if held.checksum != self.checksum {
            return Err(ArchiveError {
                message: format!(
                    "{location} is archived already with other bytes, checksum {}: an \
                     archived item is never written over",
                    held.checksum
                ),
            });
        }
        Ok(())
    }
}

/// `item` archived in `store` under `prefix`: its metadata, then its bytes,
/// then the receipt.
///
/// # Errors
/// The data type begins with a dot; the name holds another item, or the
/// same item with other bytes; an object at either key is not what this
/// archive wrote there; or the store failed.
pub fn archive(
    store: &impl Named,
    prefix: &str,
    item: ArchiveItem,
) -> Result<ArchiveReceipt, ArchiveError> {
    let key = layout::item_key(prefix, &item.data_type, &item.identifier, |text| {
        store.segment(text)
    })?;
    let meta = layout::meta_of(prefix, &key)?;
    let claim = Sidecar {
        data_type: item.data_type,
        identifier: item.identifier,
        archived_at: timestamp::now(),
        checksum: sha256_hex(&item.bytes),
        metadata: item.metadata,
    };
    if store.create(&meta, claim.to_toml().as_bytes())? == Created::Exists {
        let held = sidecar_at(store, &meta)?.ok_or_else(|| gone(store, &meta))?;
        Claim::of(&claim).against(&Claim::of(&held), &store.location(&key))?;
    }
    if store.create(&key, &item.bytes)? == Created::Exists {
        let held = store.read(&key)?.ok_or_else(|| gone(store, &key))?;
        if sha256_hex(&held) != claim.checksum {
            return Err(ArchiveError {
                message: format!(
                    "{} holds bytes this archive did not write there: they are left as \
                     they are and the item is refused",
                    store.location(&key)
                ),
            });
        }
    }
    Ok(ArchiveReceipt {
        location: store.location(&key),
        checksum: Some(claim.checksum),
    })
}

/// The item `receipt` names in `store` under `prefix`: its metadata first,
/// then its bytes.
///
/// # Errors
/// The receipt is not this store's; there is no metadata or no bytes at its
/// name, so it is not an item this archive completed; the receipt's
/// checksum, the metadata's and the bytes' disagree; or the store failed.
pub fn restore(
    store: &impl Named,
    prefix: &str,
    receipt: &ArchiveReceipt,
) -> Result<ArchiveItem, ArchiveError> {
    let not_completed = || ArchiveError {
        message: format!("{} is not an item this archive completed", receipt.location),
    };
    let key = store.key(&receipt.location)?;
    let meta = layout::meta_of(prefix, &key)?;
    let sidecar = sidecar_at(store, &meta)?.ok_or_else(not_completed)?;
    let bytes = store.read(&key)?.ok_or_else(not_completed)?;
    let actual = sha256_hex(&bytes);
    let receipted = receipt.checksum.as_deref().unwrap_or(&actual);
    if actual != sidecar.checksum || actual != receipted {
        return Err(ArchiveError {
            message: format!(
                "checksum mismatch at {}: the receipt says {receipted}, the metadata {}, \
                 the bytes {actual}",
                receipt.location, sidecar.checksum
            ),
        });
    }
    Ok(ArchiveItem {
        data_type: sidecar.data_type,
        identifier: sidecar.identifier,
        bytes,
        metadata: sidecar.metadata,
    })
}

/// The sidecar at `meta`, `None` where there is no object, refused where
/// the object there is not an archive's metadata.
fn sidecar_at(store: &impl Named, meta: &str) -> Result<Option<Sidecar>, ArchiveError> {
    let Some(held) = store.read(meta)? else {
        return Ok(None);
    };
    let foreign = |reason: String| ArchiveError {
        message: format!(
            "{} is not an archive's metadata: {reason}",
            store.location(meta)
        ),
    };
    let text = String::from_utf8(held).map_err(|cause| foreign(cause.to_string()))?;
    Sidecar::parse(&text).map(Some).map_err(foreign)
}

/// An object that was at `key` a moment ago, and is not: an archive never
/// deletes, so something else did.
fn gone(store: &impl Named, key: &str) -> ArchiveError {
    ArchiveError {
        message: format!(
            "{} was there as it was created and is gone: something other than \
             this archive removed it",
            store.location(key)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::item;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    /// A store in memory: objects by key, never written over, and a create
    /// that fails once where a test says so.
    #[derive(Default)]
    struct Memory {
        objects: RefCell<BTreeMap<String, Vec<u8>>>,
        failing: RefCell<Option<String>>,
        sanitizing: Cell<bool>,
    }

    impl Memory {
        fn put(&self, key: &str, bytes: &[u8]) {
            self.objects
                .borrow_mut()
                .insert(key.to_string(), bytes.to_vec());
        }

        fn keys(&self) -> Vec<String> {
            self.objects.borrow().keys().cloned().collect()
        }
    }

    impl Named for Memory {
        fn create(&self, key: &str, bytes: &[u8]) -> Result<Created, ArchiveError> {
            if self.failing.borrow().as_deref() == Some(key) {
                self.failing.replace(None);
                return Err(ArchiveError::caused_by("the medium stopped"));
            }
            if self.objects.borrow().contains_key(key) {
                return Ok(Created::Exists);
            }
            self.put(key, bytes);
            Ok(Created::Created)
        }

        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, ArchiveError> {
            Ok(self.objects.borrow().get(key).cloned())
        }

        fn location(&self, key: &str) -> String {
            format!("memory://{key}")
        }

        fn key(&self, location: &str) -> Result<String, ArchiveError> {
            location
                .strip_prefix("memory://")
                .map(str::to_string)
                .ok_or_else(|| ArchiveError::caused_by(format!("{location} is not memory's")))
        }

        fn segment(&self, text: &str) -> String {
            if self.sanitizing.get() {
                layout::sanitize(text)
            } else {
                text.to_string()
            }
        }
    }

    fn other_bytes(id: &str) -> ArchiveItem {
        ArchiveItem {
            bytes: b"{\"kept\":false}".to_vec(),
            ..item(id)
        }
    }

    #[test]
    fn an_item_is_its_metadata_and_its_bytes_and_restores() {
        let store = Memory::default();
        let original = item("json#1");
        let receipt = archive(&store, "retained", original.clone()).expect("archive");
        assert_eq!(receipt.location, "memory://retained/json/json#1");
        assert_eq!(receipt.checksum, Some(sha256_hex(&original.bytes)));
        assert_eq!(
            store.keys(),
            ["retained/.meta/json/json#1", "retained/json/json#1"]
        );
        let meta = store.read("retained/.meta/json/json#1").expect("read");
        let sidecar =
            Sidecar::parse(&String::from_utf8(meta.expect("the metadata")).expect("utf-8"))
                .expect("a sidecar");
        assert_eq!(sidecar.checksum, sha256_hex(&original.bytes));
        assert_eq!(
            restore(&store, "retained", &receipt).expect("restore"),
            original
        );
    }

    #[test]
    fn an_identifier_archived_again_with_the_same_bytes_answers_the_same_receipt() {
        let store = Memory::default();
        let first = archive(&store, "retained", item("json#1")).expect("archive");
        let again = archive(&store, "retained", item("json#1")).expect("archived again");
        assert_eq!(again, first);
        assert_eq!(store.keys().len(), 2, "nothing more is written");
    }

    #[test]
    fn an_identifier_archived_again_with_other_bytes_is_refused_and_the_earlier_receipt_restores() {
        let store = Memory::default();
        let receipt = archive(&store, "retained", item("json#1")).expect("archive");
        let refused = archive(&store, "retained", other_bytes("json#1")).expect_err("other bytes");
        assert!(
            refused
                .message
                .contains("is archived already with other bytes"),
            "{refused}"
        );
        assert_eq!(
            restore(&store, "retained", &receipt).expect("restore"),
            item("json#1")
        );
    }

    #[test]
    fn order_and_order_meta_are_two_items() {
        let store = Memory::default();
        let order = item("order");
        let order_meta = ArchiveItem {
            bytes: b"not order's metadata".to_vec(),
            ..item("order.meta")
        };
        let first = archive(&store, "", order.clone()).expect("order");
        let second = archive(&store, "", order_meta.clone()).expect("order.meta");
        assert_eq!(restore(&store, "", &first).expect("order"), order);
        assert_eq!(
            restore(&store, "", &second).expect("order.meta"),
            order_meta
        );
    }

    #[test]
    fn an_interrupted_pair_is_completed_by_its_repeat_and_refuses_another_item() {
        let store = Memory::default();
        store
            .failing
            .replace(Some("retained/json/json#1".to_string()));
        archive(&store, "retained", item("json#1")).expect_err("stopped between the two");
        assert_eq!(
            store.keys(),
            ["retained/.meta/json/json#1"],
            "the claim alone"
        );
        let refused = archive(&store, "retained", other_bytes("json#1")).expect_err("claimed");
        assert!(
            refused
                .message
                .contains("is archived already with other bytes"),
            "{refused}"
        );
        let receipt = archive(&store, "retained", item("json#1")).expect("completed");
        assert_eq!(
            restore(&store, "retained", &receipt).expect("restore"),
            item("json#1")
        );
        let refused = archive(&store, "retained", other_bytes("json#1")).expect_err("taken");
        assert!(
            refused
                .message
                .contains("is archived already with other bytes"),
            "{refused}"
        );
    }

    #[test]
    fn bytes_without_metadata_are_refused_on_restore() {
        let store = Memory::default();
        store.put("retained/json/json#1", b"{\"kept\":true}");
        let receipt = ArchiveReceipt {
            location: "memory://retained/json/json#1".to_string(),
            checksum: Some(sha256_hex(b"{\"kept\":true}")),
        };
        let refused = restore(&store, "retained", &receipt).expect_err("no metadata");
        assert!(
            refused
                .message
                .contains("is not an item this archive completed"),
            "{refused}"
        );
        store
            .failing
            .replace(Some("retained/json/json#2".to_string()));
        archive(&store, "retained", item("json#2")).expect_err("stopped between the two");
        let half = ArchiveReceipt {
            location: "memory://retained/json/json#2".to_string(),
            checksum: None,
        };
        let refused = restore(&store, "retained", &half).expect_err("no bytes");
        assert!(
            refused
                .message
                .contains("is not an item this archive completed"),
            "{refused}"
        );
    }

    #[test]
    fn a_foreign_object_at_the_key_is_refused() {
        let store = Memory::default();
        store.put("retained/json/json#1", b"someone else's");
        let refused = archive(&store, "retained", item("json#1")).expect_err("foreign bytes");
        assert!(
            refused
                .message
                .contains("holds bytes this archive did not write there"),
            "{refused}"
        );
        assert_eq!(
            store.read("retained/json/json#1").expect("read"),
            Some(b"someone else's".to_vec()),
            "left as it was"
        );
        store.put("retained/.meta/json/json#2", b"not toml at all");
        let refused = archive(&store, "retained", item("json#2")).expect_err("foreign metadata");
        assert!(
            refused.message.contains("is not an archive's metadata"),
            "{refused}"
        );
        assert!(store.read("retained/json/json#2").expect("read").is_none());
    }

    #[test]
    fn a_data_type_beginning_with_a_dot_is_refused() {
        let store = Memory::default();
        let hidden = ArchiveItem {
            data_type: ".meta".to_string(),
            ..item("json#1")
        };
        let refused = archive(&store, "retained", hidden).expect_err("a dot first");
        assert!(refused.message.contains("begins with a dot"), "{refused}");
        assert!(store.keys().is_empty(), "nothing is written");
    }

    #[test]
    fn two_items_laid_out_at_one_name_are_refused_in_words() {
        let store = Memory::default();
        store.sanitizing.set(true);
        let receipt = archive(&store, "", item("a#1")).expect("archive");
        assert_eq!(receipt.location, "memory://json/a_1");
        let refused = archive(&store, "", item("a_1")).expect_err("the same name");
        assert!(
            refused
                .message
                .contains("is laid out at the same name and is refused"),
            "{refused}"
        );
        let typed = ArchiveItem {
            data_type: "text/plain".to_string(),
            ..item("n")
        };
        archive(&store, "", typed).expect("text/plain");
        let underscored = ArchiveItem {
            data_type: "text_plain".to_string(),
            ..item("n")
        };
        let refused = archive(&store, "", underscored).expect_err("the same name");
        assert!(refused.message.contains("\"text/plain\""), "{refused}");
        assert_eq!(restore(&store, "", &receipt).expect("restore"), item("a#1"));
    }

    #[test]
    fn a_receipt_disagreeing_with_the_metadata_or_the_bytes_is_refused() {
        let store = Memory::default();
        let mut receipt = archive(&store, "retained", item("json#1")).expect("archive");
        receipt.checksum = Some("0".repeat(64));
        let refused = restore(&store, "retained", &receipt).expect_err("the receipt lies");
        assert!(refused.message.contains("checksum mismatch"), "{refused}");
        let receipt = archive(&store, "retained", item("json#2")).expect("archive");
        store.put("retained/json/json#2", b"changed underneath");
        let refused = restore(&store, "retained", &receipt).expect_err("the bytes changed");
        assert!(refused.message.contains("checksum mismatch"), "{refused}");
        let elsewhere = ArchiveReceipt {
            location: "memory://elsewhere/json/json#1".to_string(),
            checksum: None,
        };
        let refused = restore(&store, "retained", &elsewhere).expect_err("another prefix");
        assert!(
            refused.message.contains("is not an item's bytes"),
            "{refused}"
        );
    }
}
