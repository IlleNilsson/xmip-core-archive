//! An archive in a SQL server's table: [`SqlArchive`], one type for every
//! server, and the [`Dialect`] each server's technology gives it.
//!
//! One item is one row with the four columns every archive technology
//! carries — `data_type`, `identifier`, `bytes`, `metadata` — and
//! `archived_at`, when it was handed over. On a server the table is the
//! operator's to create; a file-based engine creates its own
//! ([`Dialect::prepare`]). An archive never deletes (ADR-0040): this one
//! inserts and selects, nothing else. The receipt is
//! `<scheme>://<server>/<database>/<table>?id=<n>`, and restoring reads the
//! table and the id from it on the store's own connection, refusing a
//! receipt of another server or database.
//!
//! `PostgreSQL`, SQL Server and `MySQL` each carried the store, the row, the
//! SELECT and the receipt until 2026-09-14, and the store until 2026-09-24;
//! `SQLite` carried all of it again until the same day. All of it is here
//! now (ADR-0044). What stays in the technology is the [`Dialect`]: how an
//! identifier and a literal are quoted, how the bytes go in and come back,
//! how a new row's id is asked for, the moment's form, and the connection —
//! the server's transport technology, or the engine a file is opened with.

use std::marker::PhantomData;
use std::time::Duration;

use crate::{ArchiveError, ArchiveItem, ArchiveReceipt, ArchiveStore, location, metadata};

/// The table written to unless told otherwise.
pub const DEFAULT_TABLE: &str = "archive";

/// The columns an INSERT names, in the order it writes the values.
pub const COLUMNS: &str = "data_type, identifier, bytes, metadata, archived_at";

/// A row as a SELECT answers it: a cell is text or NULL.
pub type Row = Vec<Option<String>>;

/// Where a store connects and who it logs in as.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Server {
    /// `host:port`, or for a file-based engine the file as the path of a
    /// URI (`net::uri`): `/C:/xmip/archive.sqlite`.
    pub address: String,
    pub database: String,
    pub user: String,
    /// `None` logs in without one: trust, or an empty password.
    pub password: Option<String>,
    /// How long to wait on a server that stops mid-message.
    pub timeout: Option<Duration>,
}

/// What one SQL server does its own way. A technology implements it on a
/// type of its own and hands that type to [`SqlArchive`].
pub trait Dialect {
    /// The receipt's scheme: `postgresql`, `mssql`, `mysql`.
    const SCHEME: &'static str;
    /// The expression that selects the bytes column: `bytes` where the
    /// transport answers it in a form [`Dialect::column_bytes`] reads, or a
    /// spelling-out such as `CONCAT('0x', HEX(bytes))` where the protocol
    /// would hand a BLOB over raw.
    const BYTES_EXPRESSION: &'static str = "bytes";
    /// What an INSERT says between its columns and `VALUES` to answer the
    /// new row's id: ` OUTPUT INSERTED.id`.
    const ID_BEFORE_VALUES: &'static str = "";
    /// What an INSERT says after its values to answer the new row's id:
    /// ` RETURNING id`.
    const ID_AFTER_VALUES: &'static str = "";

    /// An open connection to the server.
    type Connection;

    /// Make `table` ready on a fresh connection, before any statement:
    /// nothing for a server, whose table is the operator's to create; a
    /// file-based engine creates it where it is not there.
    ///
    /// # Errors
    /// Where the engine refused.
    fn prepare(connection: &mut Self::Connection, table: &str) -> Result<(), ArchiveError> {
        let _ = (connection, table);
        Ok(())
    }

    /// One identifier — a schema, a table — quoted as this server reads it.
    fn quote_identifier(name: &str) -> String;
    /// A text value as this server's string literal.
    fn quote_literal(text: &str) -> String;
    /// The bytes as a literal this server stores in the bytes column.
    fn bytes_literal(bytes: &[u8]) -> String;
    /// The bytes the transport's answer for the bytes column names in
    /// this server's binary form, or `None` where the answer is not in it:
    /// the column is written in that form and nothing else is taken for
    /// bytes (ADR-0038).
    fn column_bytes(text: &str) -> Option<Vec<u8>>;
    /// The moment, as the `archived_at` column takes it: RFC 3339 in UTC.
    #[must_use]
    fn archived_at() -> String {
        crate::timestamp::now()
    }

    /// Connect and log in.
    ///
    /// # Errors
    /// Where the server could not be reached or refused the login.
    fn connect(server: &Server) -> Result<Self::Connection, ArchiveError>;
    /// Run a SELECT, or a statement that answers rows, and answer them.
    ///
    /// # Errors
    /// Where the server refused the statement or went away.
    fn select(connection: &mut Self::Connection, sql: &str) -> Result<Vec<Row>, ArchiveError>;
    /// Run [`insert_sql`]'s statement and answer the new row's id: by
    /// default the first cell it answers, as `RETURNING` and `OUTPUT`
    /// have it; a server that answers an INSERT with a count asks after.
    ///
    /// # Errors
    /// Where the server refused the statement or went away.
    fn insert(
        connection: &mut Self::Connection,
        sql: &str,
    ) -> Result<Option<String>, ArchiveError> {
        Ok(first_cell(Self::select(connection, sql)?))
    }
    /// Say goodbye.
    ///
    /// # Errors
    /// Where the server had already gone.
    fn close(connection: Self::Connection) -> Result<(), ArchiveError>;
}

/// The first cell of the first row, where there is one and it is not NULL.
#[must_use]
pub fn first_cell(rows: Vec<Row>) -> Option<String> {
    rows.into_iter().next()?.into_iter().next().flatten()
}

/// A table name quoted segment by segment, so `audit.archive` stays a table
/// in a schema and `Archive` keeps its case.
#[must_use]
pub fn table_name<D: Dialect>(table: &str) -> String {
    table
        .split('.')
        .map(D::quote_identifier)
        .collect::<Vec<_>>()
        .join(".")
}

/// The statement that stores `item` in `table` at `archived_at`, asking for
/// the new row's id where the dialect's INSERT can.
#[must_use]
pub fn insert_sql<D: Dialect>(table: &str, item: &ArchiveItem, archived_at: &str) -> String {
    format!(
        "INSERT INTO {} ({COLUMNS}){} VALUES ({}, {}, {}, {}, {}){}",
        table_name::<D>(table),
        D::ID_BEFORE_VALUES,
        D::quote_literal(&item.data_type),
        D::quote_literal(&item.identifier),
        D::bytes_literal(&item.bytes),
        D::quote_literal(&metadata::encode(&item.metadata)),
        D::quote_literal(archived_at),
        D::ID_AFTER_VALUES
    )
}

/// The statement that brings row `id` of `table` back, the four columns in
/// the order [`item_from_row`] reads them.
#[must_use]
pub fn select_sql<D: Dialect>(table: &str, id: u64) -> String {
    format!(
        "SELECT data_type, identifier, {}, metadata FROM {} WHERE id = {id}",
        D::BYTES_EXPRESSION,
        table_name::<D>(table)
    )
}

/// One row, the four columns in order, back into an item.
///
/// # Errors
/// Where a column is missing or NULL, or the bytes column is not in the
/// dialect's binary form.
pub fn item_from_row<D: Dialect>(
    row: &[Option<String>],
    at: &str,
) -> Result<ArchiveItem, ArchiveError> {
    let column = |index: usize, name: &str| {
        row.get(index)
            .cloned()
            .flatten()
            .ok_or_else(|| ArchiveError {
                message: format!("column {name} is missing or NULL in {at}"),
            })
    };
    Ok(ArchiveItem {
        data_type: column(0, "data_type")?,
        identifier: column(1, "identifier")?,
        bytes: D::column_bytes(&column(2, "bytes")?).ok_or_else(|| ArchiveError {
            message: format!("column bytes in {at} is not in {}'s binary form", D::SCHEME),
        })?,
        metadata: metadata::decode(&column(3, "metadata")?),
    })
}

/// An archive that keeps items as rows of one table on one server, the
/// server's ways its [`Dialect`]'s.
pub struct SqlArchive<D> {
    server: Server,
    table: String,
    dialect: PhantomData<fn() -> D>,
}

impl<D: Dialect> SqlArchive<D> {
    /// An archive writing to [`DEFAULT_TABLE`] in `database` at `address`,
    /// logging in as `user` with no password.
    #[must_use]
    pub fn new(
        address: impl Into<String>,
        database: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        Self {
            server: Server {
                address: address.into(),
                database: database.into(),
                user: user.into(),
                password: None,
                timeout: None,
            },
            table: DEFAULT_TABLE.to_string(),
            dialect: PhantomData,
        }
    }

    /// The password the login carries.
    #[must_use]
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.server.password = Some(password.into());
        self
    }

    /// The table to write to, `audit.archive` say.
    #[must_use]
    pub fn with_table(mut self, table: impl Into<String>) -> Self {
        self.table = table.into();
        self
    }

    /// Give up on a server that stops mid-message.
    #[must_use]
    pub const fn timing_out_after(mut self, timeout: Duration) -> Self {
        self.server.timeout = Some(timeout);
        self
    }

    /// What every receipt of this store shares, up to its table:
    /// `<scheme>://<server>/<database>/`.
    fn prefix(&self) -> String {
        format!(
            "{}://{}/{}/",
            D::SCHEME,
            self.server.address,
            self.server.database
        )
    }

    fn location(&self, id: &str) -> String {
        format!("{}{}?id={id}", self.prefix(), self.table)
    }
}

impl<D: Dialect> ArchiveStore for SqlArchive<D> {
    fn archive(&self, item: ArchiveItem) -> Result<ArchiveReceipt, ArchiveError> {
        let sql = insert_sql::<D>(&self.table, &item, &D::archived_at());
        let mut connection = D::connect(&self.server)?;
        D::prepare(&mut connection, &self.table)?;
        let id = D::insert(&mut connection, &sql)?;
        D::close(connection)?;
        let id = id.ok_or_else(|| ArchiveError {
            message: format!("the insert into {} returned no id", self.table),
        })?;
        Ok(ArchiveReceipt {
            location: self.location(&id),
            checksum: None,
        })
    }

    fn restore(&self, receipt: &ArchiveReceipt) -> Result<ArchiveItem, ArchiveError> {
        let (table, id) = location::table_row(D::SCHEME, &receipt.location)?;
        if !receipt.location.starts_with(&self.prefix()) {
            return Err(ArchiveError {
                message: format!("{} is not a receipt of {}", receipt.location, self.prefix()),
            });
        }
        let mut connection = D::connect(&self.server)?;
        D::prepare(&mut connection, table)?;
        let rows = D::select(&mut connection, &select_sql::<D>(table, id))?;
        D::close(connection)?;
        let first = rows.first().ok_or_else(|| ArchiveError {
            message: format!("no row at {}", receipt.location),
        })?;
        item_from_row::<D>(first, &receipt.location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::item;
    use std::cell::RefCell;

    thread_local! {
        /// Every statement the test server was sent, in order.
        static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// A server that quotes with double quotes, answers bytes as text,
    /// returns id 41 from an INSERT and the fixture row from any SELECT
    /// but one of table `gone`.
    struct Plain;

    impl Dialect for Plain {
        const SCHEME: &'static str = "plain";
        const ID_AFTER_VALUES: &'static str = " RETURNING id";
        type Connection = ();

        fn quote_identifier(name: &str) -> String {
            format!("\"{name}\"")
        }
        fn quote_literal(text: &str) -> String {
            format!("'{text}'")
        }
        fn bytes_literal(bytes: &[u8]) -> String {
            format!("'{}'", String::from_utf8_lossy(bytes))
        }
        fn column_bytes(text: &str) -> Option<Vec<u8>> {
            Some(text.as_bytes().to_vec())
        }
        fn connect(server: &Server) -> Result<(), ArchiveError> {
            if server.password.as_deref() == Some("wrong") {
                return Err(ArchiveError::caused_by("refused"));
            }
            Ok(())
        }
        fn select((): &mut (), sql: &str) -> Result<Vec<Row>, ArchiveError> {
            SENT.with(|sent| sent.borrow_mut().push(sql.to_string()));
            let held = item("json#1");
            Ok(if sql.starts_with("INSERT") {
                vec![vec![Some("41".to_string())]]
            } else if sql.contains("\"gone\"") {
                Vec::new()
            } else {
                vec![vec![
                    Some(held.data_type),
                    Some(held.identifier),
                    Some(String::from_utf8_lossy(&held.bytes).into_owned()),
                    Some(metadata::encode(&held.metadata)),
                ]]
            })
        }
        fn close((): ()) -> Result<(), ArchiveError> {
            Ok(())
        }
    }

    #[test]
    fn the_statements_are_the_dialects_and_the_table_is_quoted_by_segment() {
        assert_eq!(
            table_name::<Plain>("audit.archive"),
            "\"audit\".\"archive\""
        );
        let sql = insert_sql::<Plain>("Archive", &item("a"), "then");
        assert_eq!(
            sql,
            "INSERT INTO \"Archive\" (data_type, identifier, bytes, metadata, archived_at) \
             VALUES ('json', 'a', '{\"kept\":true}', 'source\u{1f}playground', 'then') \
             RETURNING id"
        );
        assert_eq!(
            select_sql::<Plain>("Archive", 41),
            "SELECT data_type, identifier, bytes, metadata FROM \"Archive\" WHERE id = 41"
        );
    }

    #[test]
    fn an_item_is_one_insert_and_its_receipt_restores_it() {
        let store =
            SqlArchive::<Plain>::new("host:1", "orders", "xmip").with_table("audit.archive");
        let receipt = store.archive(item("json#1")).expect("archive");
        assert_eq!(
            receipt.location,
            "plain://host:1/orders/audit.archive?id=41"
        );
        assert_eq!(receipt.checksum, None);
        assert_eq!(store.restore(&receipt).expect("restore"), item("json#1"));
        let sent = SENT.with(|sent| sent.borrow().clone());
        assert_eq!(sent.len(), 2);
        assert!(sent[0].starts_with("INSERT INTO \"audit\".\"archive\" "));
        assert_eq!(
            sent[1],
            "SELECT data_type, identifier, bytes, metadata FROM \"audit\".\"archive\" WHERE id = 41"
        );
    }

    #[test]
    fn a_refused_login_a_foreign_receipt_and_a_missing_row_are_errors() {
        let store = SqlArchive::<Plain>::new("h", "d", "u").with_password("wrong");
        assert!(store.archive(item("x")).is_err());
        let store = SqlArchive::<Plain>::new("h", "d", "u");
        for location in ["s3://b/k", "plain://h/d/archive", "plain://h/d?id=1"] {
            let receipt = ArchiveReceipt {
                location: location.to_string(),
                checksum: None,
            };
            let failure = store.restore(&receipt).expect_err(location);
            assert!(failure.message.contains("is not plain://"), "{failure}");
        }
        let receipt = ArchiveReceipt {
            location: "plain://h/d/gone?id=1".to_string(),
            checksum: None,
        };
        let failure = store.restore(&receipt).expect_err("no row");
        assert_eq!(failure.message, "no row at plain://h/d/gone?id=1");
        for location in ["plain://other/d/archive?id=1", "plain://h/e/archive?id=1"] {
            let receipt = ArchiveReceipt {
                location: location.to_string(),
                checksum: None,
            };
            let failure = store.restore(&receipt).expect_err(location);
            assert_eq!(
                failure.message,
                format!("{location} is not a receipt of plain://h/d/")
            );
        }
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
        let restored = item_from_row::<Plain>(&row, "here").expect("row");
        assert_eq!(restored.identifier, "it's #1");
        assert_eq!(restored.bytes, b"plain");
        assert_eq!(restored.metadata, pairs);
        let short = [Some("json".to_string())];
        let failure = item_from_row::<Plain>(&short, "here").expect_err("missing");
        assert_eq!(
            failure.message,
            "column identifier is missing or NULL in here"
        );
        let null = [None, Some("x".to_string())];
        let failure = item_from_row::<Plain>(&null, "here").expect_err("NULL");
        assert!(failure.message.contains("data_type"), "{failure}");
        assert_eq!(first_cell(Vec::new()), None);
        assert_eq!(first_cell(vec![vec![None]]), None);
    }
}
