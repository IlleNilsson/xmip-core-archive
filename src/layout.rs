//! Where one item sits in a store that is laid out by name: a directory, a
//! bucket, a container, a script per type.
//!
//! Two questions every such store answers the same way. What does a
//! segment look like when it becomes a file name — `sanitise`, so an
//! identifier like `poison-json#3/../x` is a safe name and no traversal
//! survives. And how are the bytes and the metadata keyed beside each
//! other — `<prefix>/<data_type>/<identifier>` for the bytes and the same
//! key with `.meta` for the pairs, so an operator listing a prefix sees the
//! archive laid out by type and can open either object with any tool. The
//! data type is made slash-free in a key so it splits back into its two
//! parts; the identifier is kept as it is.

/// What follows an item's key to name the object holding its metadata.
pub const META_SUFFIX: &str = ".meta";

/// One path segment made safe: anything but a plain filename character
/// becomes an underscore, so `poison-json#3/../x` is one valid file name
/// and no `..` survives.
#[must_use]
pub fn sanitise(segment: &str) -> String {
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
/// `<data_type>/<identifier>` under an empty prefix.
#[must_use]
pub fn key(prefix: &str, data_type: &str, identifier: &str) -> String {
    let data_type = data_type.replace('/', "_");
    match prefix.trim_matches('/') {
        "" => format!("{data_type}/{identifier}"),
        prefix => format!("{prefix}/{data_type}/{identifier}"),
    }
}

/// The key for the metadata beside the bytes at `key`.
#[must_use]
pub fn meta_key(key: &str) -> String {
    format!("{key}{META_SUFFIX}")
}

/// The data type and identifier a key under `prefix` names, or `None` when
/// the key is not laid out that way.
#[must_use]
pub fn split_key(prefix: &str, key: &str) -> Option<(String, String)> {
    let rest = match prefix.trim_matches('/') {
        "" => key,
        prefix => key.strip_prefix(prefix)?.strip_prefix('/')?,
    };
    let (data_type, identifier) = rest.split_once('/')?;
    (!data_type.is_empty() && !identifier.is_empty())
        .then(|| (data_type.to_string(), identifier.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_is_made_a_safe_file_name() {
        assert_eq!(sanitise("poison-json#3/../x"), "poison-json_3____x");
        assert_eq!(sanitise("plain-1"), "plain-1");
        assert_eq!(sanitise(""), "");
    }

    #[test]
    fn a_key_is_prefix_type_and_identifier_and_splits_back() {
        assert_eq!(key("retained", "json", "a#1"), "retained/json/a#1");
        assert_eq!(key("/retained/", "json", "a/b"), "retained/json/a/b");
        assert_eq!(key("", "text/plain", "n"), "text_plain/n");
        assert_eq!(meta_key("retained/json/a#1"), "retained/json/a#1.meta");
        assert_eq!(
            split_key("retained", "retained/json/a/b"),
            Some(("json".to_string(), "a/b".to_string()))
        );
        assert_eq!(
            split_key("", "json/n"),
            Some(("json".to_string(), "n".to_string()))
        );
        assert_eq!(split_key("retained", "elsewhere/json/n"), None);
        assert_eq!(split_key("retained", "retained/json"), None);
        assert_eq!(split_key("retained", "retained//n"), None);
    }
}
