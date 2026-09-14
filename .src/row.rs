//! One item as one row of an archive table, in the part every SQL server
//! shares: the four columns in their order, the SELECT that brings a row
//! back, the table name quoted segment by segment, and the row read back
//! into an item.
//!
//! `PostgreSQL`, SQL Server and `MySQL` each carried this file until
//! 2026-09-14; it moved up to the capability (ADR-0044). What stays in the
//! technology is the [`Dialect`]: how an identifier is quoted, how the
//! bytes column is spelled in a SELECT, and how the text the transport
//! answers for it becomes the bytes. The INSERT stays there too, because
//! how a new row's id is asked for — `RETURNING`, `OUTPUT INSERTED`, a
//! second statement — is the dialect all through.

use crate::{ArchiveError, ArchiveItem, metadata};

/// What one SQL server does differently from the next, as three choices
/// the shared row code is given.
#[derive(Clone, Copy)]
pub struct Dialect {
    /// One identifier — a schema, a table, a column — quoted the way this
    /// server reads it: `"name"`, `[name]` or `` `name` ``.
    pub quote_identifier: fn(&str) -> String,
    /// The expression that selects the bytes column: `bytes` where the
    /// transport answers it in a form [`Dialect::column_bytes`] reads, or a
    /// spelling-out such as `CONCAT('0x', HEX(bytes))` where the protocol
    /// would hand a BLOB over raw.
    pub bytes_expression: &'static str,
    /// The text the transport answered for the bytes column, as the bytes.
    pub column_bytes: fn(String) -> Vec<u8>,
}

impl Dialect {
    /// A table name quoted segment by segment, so `audit.archive` stays a
    /// table in a schema and `Archive` keeps its case.
    #[must_use]
    pub fn table_name(&self, table: &str) -> String {
        table
            .split('.')
            .map(self.quote_identifier)
            .collect::<Vec<_>>()
            .join(".")
    }

    /// The statement that brings row `id` of `table` back, the four columns
    /// in the order [`Dialect::item_from_row`] reads them.
    #[must_use]
    pub fn select_sql(&self, table: &str, id: u64) -> String {
        format!(
            "SELECT data_type, identifier, {}, metadata FROM {} WHERE id = {id}",
            self.bytes_expression,
            self.table_name(table)
        )
    }

    /// One row, the four columns in order, back into an item.
    ///
    /// # Errors
    /// Where a column is missing or NULL.
    pub fn item_from_row(
        &self,
        row: &[Option<String>],
        location: &str,
    ) -> Result<ArchiveItem, ArchiveError> {
        let column = |index: usize, name: &str| {
            row.get(index)
                .cloned()
                .flatten()
                .ok_or_else(|| ArchiveError {
                    message: format!("column {name} is missing or NULL in {location}"),
                })
        };
        Ok(ArchiveItem {
            data_type: column(0, "data_type")?,
            identifier: column(1, "identifier")?,
            bytes: (self.column_bytes)(column(2, "bytes")?),
            metadata: metadata::decode(&column(3, "metadata")?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server that quotes with double quotes and answers bytes as text.
    const PLAIN: Dialect = Dialect {
        quote_identifier: |name| format!("\"{name}\""),
        bytes_expression: "bytes",
        column_bytes: String::into_bytes,
    };

    #[test]
    fn a_table_name_is_quoted_segment_by_segment() {
        assert_eq!(PLAIN.table_name("audit.archive"), "\"audit\".\"archive\"");
        assert_eq!(PLAIN.table_name("Archive"), "\"Archive\"");
    }

    #[test]
    fn the_select_names_the_four_columns_as_the_dialect_spells_them() {
        assert_eq!(
            PLAIN.select_sql("Archive", 41),
            "SELECT data_type, identifier, bytes, metadata FROM \"Archive\" WHERE id = 41"
        );
        let spelled = Dialect {
            bytes_expression: "HEX(bytes)",
            ..PLAIN
        };
        assert_eq!(
            spelled.select_sql("a", 1),
            "SELECT data_type, identifier, HEX(bytes), metadata FROM \"a\" WHERE id = 1"
        );
    }

    #[test]
    fn a_row_is_the_item_again_and_a_short_or_null_row_is_refused() {
        let pairs = vec![("source".to_string(), "playground".to_string())];
        let row = vec![
            Some("json".to_string()),
            Some("it's #1".to_string()),
            Some("plain".to_string()),
            Some(metadata::encode(&pairs)),
        ];
        let item = PLAIN.item_from_row(&row, "here").expect("row");
        assert_eq!(item.data_type, "json");
        assert_eq!(item.identifier, "it's #1");
        assert_eq!(item.bytes, b"plain");
        assert_eq!(item.metadata, pairs);
        let short = [Some("json".to_string())];
        let failure = PLAIN.item_from_row(&short, "here").expect_err("missing");
        assert_eq!(
            failure.message,
            "column identifier is missing or NULL in here"
        );
        let null = [None, Some("x".to_string())];
        let failure = PLAIN.item_from_row(&null, "here").expect_err("NULL");
        assert!(failure.message.contains("data_type"), "{failure}");
    }
}
