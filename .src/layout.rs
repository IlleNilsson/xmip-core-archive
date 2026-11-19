//! Where one item sits in a store that is laid out by name: a directory, a
//! bucket, a container, a script per type.
//!
//! Two questions every such store answers the same way. What does a
//! segment look like when it becomes a file name — `sanitize`, so an
//! identifier like `poison-json#3/../x` is a safe name and no traversal
//! survives. And where are the bytes and the metadata — the bytes at
//! `<prefix>/<data_type>/<identifier>` ([`item_key`]), the metadata at
//! `<prefix>/.meta/<data_type>/<identifier>` ([`meta_of`]), so an operator
//! listing a prefix sees the archive laid out by type, its metadata in a
//! tree of its own beside it, and can open either object with any tool.
//! No item's name is ever another item's metadata: `order` and
//! `order.meta` are two items. A data type beginning with `.` is refused,
//! so none is laid out where the metadata is. The data type's `/` becomes
//! `_`, so it is one segment of the key; two data types or identifiers
//! that become the same name are refused by the metadata already there
//! (`crate::named`), in words. The identifier is kept as the store keeps
//! it: as it is in an object key, sanitized in a file name.

use crate::ArchiveError;

/// The first segment under the prefix of every key holding metadata.
pub const META: &str = ".meta";

/// One path segment made safe: anything but a plain filename character
/// becomes an underscore, so `poison-json#3/../x` is one valid file name
/// and no `..` survives.
#[must_use]
pub fn sanitize(segment: &str) -> String {
    segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The key for an item's bytes: `<prefix>/<data_type>/<identifier>`, or
/// `<data_type>/<identifier>` under an empty prefix. The data type's `/`
/// becomes `_`; then each of the two is what `segment` makes it in the
/// store — itself in an object key, a safe file name in a directory.
///
/// # Errors
/// The data type begins with `.`, where the metadata is laid out.
pub fn item_key(
    prefix: &str,
    data_type: &str,
    identifier: &str,
    segment: impl Fn(&str) -> String,
) -> Result<String, ArchiveError> {
    if data_type.starts_with('.') {
        return Err(ArchiveError {
            message: format!(
                "the data type {data_type:?} begins with a dot, where an archive keeps \
                 its metadata: it is refused"
            ),
        });
    }
    let data_type = segment(&data_type.replace('/', "_"));
    let identifier = segment(identifier);
    Ok(under(prefix, &format!("{data_type}/{identifier}")))
}

/// The key for the metadata of the item whose bytes are at `key`:
/// `<prefix>/.meta/<data_type>/<identifier>`.
///
/// # Errors
/// `key` is not under `prefix`, or names no item's bytes: it begins with
/// a dot, as metadata does.
pub fn meta_of(prefix: &str, key: &str) -> Result<String, ArchiveError> {
    let rest = match prefix.trim_matches('/') {
        "" => Some(key),
        prefix => key
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/')),
    };
    match rest {
        Some(rest) if !rest.is_empty() && !rest.starts_with('.') => {
            Ok(under(prefix, &format!("{META}/{rest}")))
        }
        _ => Err(ArchiveError {
            message: format!(
                "{key} is not an item's bytes laid out under {prefix}/<data_type>/<identifier>"
            ),
        }),
    }
}

/// `rest` under `prefix`, its slashes at either end trimmed.
fn under(prefix: &str, rest: &str) -> String {
    match prefix.trim_matches('/') {
        "" => rest.to_string(),
        prefix => format!("{prefix}/{rest}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(text: &str) -> String {
        text.to_string()
    }

    #[test]
    fn a_segment_is_made_a_safe_file_name() {
        assert_eq!(sanitize("poison-json#3/../x"), "poison-json_3____x");
        assert_eq!(sanitize("plain-1"), "plain-1");
        assert_eq!(sanitize(""), "");
    }

    #[test]
    fn the_bytes_are_prefix_type_and_identifier_and_the_metadata_a_tree_beside() {
        let at = |prefix, data_type, identifier| {
            item_key(prefix, data_type, identifier, kept).expect("laid out")
        };
        assert_eq!(at("retained", "json", "a#1"), "retained/json/a#1");
        assert_eq!(at("/retained/", "json", "a/b"), "retained/json/a/b");
        assert_eq!(at("", "text/plain", "n"), "text_plain/n");
        assert_eq!(
            item_key("", "json", "a#1", sanitize).expect("laid out"),
            "json/a_1"
        );
        assert_eq!(
            meta_of("retained", "retained/json/a#1").expect("an item"),
            "retained/.meta/json/a#1"
        );
        assert_eq!(meta_of("", "json/n").expect("an item"), ".meta/json/n");
        assert_eq!(
            meta_of("/retained/", "retained/json/order.meta").expect("an item"),
            "retained/.meta/json/order.meta"
        );
    }

    #[test]
    fn a_key_outside_the_prefix_or_of_metadata_names_no_item() {
        for (prefix, key) in [
            ("retained", "elsewhere/json/n"),
            ("retained", "retainedx/json/n"),
            ("retained", "retained/.meta/json/n"),
            ("retained", "retained/"),
            ("", ".meta/json/n"),
        ] {
            let refused = meta_of(prefix, key).expect_err(key);
            assert!(
                refused.message.contains("is not an item's bytes"),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_data_type_beginning_with_a_dot_is_refused() {
        for data_type in [".meta", ".x"] {
            let refused = item_key("retained", data_type, "n", kept).expect_err(data_type);
            assert!(refused.message.contains("begins with a dot"), "{refused}");
        }
        assert_eq!(
            item_key("retained", "x.meta", "n", kept).expect("a dot inside"),
            "retained/x.meta/n"
        );
    }
}
