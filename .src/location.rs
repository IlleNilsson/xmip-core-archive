//! The location a receipt names, read back into what the store needs to
//! find the item.
//!
//! Two shapes cover most of the technologies. A row in a database is
//! `<scheme>://<server>/<database>/<table>?id=<n>` — the table and the id
//! are what a restore selects with; the server and the database are the
//! store's own and stand in the receipt for the operator reading it. An
//! object in a bucket is `<scheme>://<bucket>/<key>`. Five technologies each
//! carried a parser of one of these until 2026-09-14; the parsers moved up
//! to the capability (ADR-0044), and each technology hands over its scheme.
//! A shape only one technology has — a blob under an account and a
//! container, a file under a root — stays with that technology.

use crate::ArchiveError;

/// The table and id a row receipt names:
/// `<scheme>://<server>/<database>/<table>?id=<n>`.
///
/// # Errors
/// The location does not have that shape, or the id is not a number.
pub fn table_row<'a>(scheme: &str, location: &'a str) -> Result<(&'a str, u64), ArchiveError> {
    let malformed = || ArchiveError {
        message: format!("{location} is not {scheme}://server/database/table?id=n"),
    };
    let rest = location
        .strip_prefix(scheme)
        .and_then(|rest| rest.strip_prefix("://"))
        .ok_or_else(malformed)?;
    let (path, query) = rest.split_once('?').ok_or_else(malformed)?;
    let id = query
        .strip_prefix("id=")
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(malformed)?;
    match path.splitn(3, '/').collect::<Vec<_>>().as_slice() {
        [_, _, table] if !table.is_empty() => Ok((table, id)),
        _ => Err(malformed()),
    }
}

/// The bucket and key an object receipt names: `<scheme>://<bucket>/<key>`.
///
/// # Errors
/// The location does not have that shape, or either part is empty.
pub fn bucket_key<'a>(scheme: &str, location: &'a str) -> Result<(&'a str, &'a str), ArchiveError> {
    location
        .strip_prefix(scheme)
        .and_then(|rest| rest.strip_prefix("://"))
        .and_then(|rest| rest.split_once('/'))
        .filter(|(bucket, key)| !bucket.is_empty() && !key.is_empty())
        .ok_or_else(|| ArchiveError {
            message: format!("{location} is not {scheme}://bucket/key"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_receipt_gives_the_table_and_the_id() {
        assert_eq!(
            table_row("postgresql", "postgresql://h:5432/db/audit.archive?id=7").expect("row"),
            ("audit.archive", 7)
        );
        assert_eq!(
            table_row("mssql", "mssql://h/db/Archive?id=41").expect("row"),
            ("Archive", 41)
        );
        for location in [
            "s3://bucket/key",
            "mysql://host/orders/archive",
            "mysql://host/orders/archive?id=x",
            "mysql://host/orders?id=1",
            "mysql://host/orders/?id=1",
            "mysqlx://host/orders/archive?id=1",
        ] {
            let failure = table_row("mysql", location).expect_err(location);
            assert_eq!(
                failure.message,
                format!("{location} is not mysql://server/database/table?id=n")
            );
        }
    }

    #[test]
    fn an_object_receipt_gives_the_bucket_and_the_key() {
        assert_eq!(
            bucket_key("s3", "s3://orders-archive/retained/json/a#1").expect("object"),
            ("orders-archive", "retained/json/a#1")
        );
        assert_eq!(bucket_key("gcs", "gcs://b/k").expect("object"), ("b", "k"));
        for location in [
            "gcs://bucket/key",
            "s3://bucket",
            "s3://bucket/",
            "s3:///key",
        ] {
            let failure = bucket_key("s3", location).expect_err(location);
            assert_eq!(
                failure.message,
                format!("{location} is not s3://bucket/key")
            );
        }
    }
}
