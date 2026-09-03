//! Derive-based row mapping for [rusqlite](https://docs.rs/rusqlite).
//!
//! `rusqlite-derive` generates a pair of simple read helpers for structs. It is
//! intended as a small convenience layer, not as a full ORM.
//!
//! # Example
//!
//! ```
//! use rusqlite::Connection;
//! use rusqlite_derive::RusqliteFetch;
//!
//! #[derive(Debug, PartialEq, RusqliteFetch)]
//! #[rusqlite(from = "users")]
//! struct User {
//!     id: i64,
//!     name: String,
//! }
//!
//! let conn = Connection::open_in_memory()?;
//! conn.execute_batch(
//!     "CREATE TABLE users (id INTEGER, name TEXT);
//!      INSERT INTO users VALUES (1, 'Ada'), (2, 'Grace');",
//! )?;
//!
//! let all = User::fetch(&conn)?;
//! let only_ada = User::fetch_with_filter(&conn, "name = 'Ada'")?;
//!
//! assert_eq!(all.len(), 2);
//! assert_eq!(only_ada, vec![User { id: 1, name: "Ada".into() }]);
//! # Ok::<(), rusqlite::Error>(())
//! ```
//!
//! Put `#[rusqlite(from = "...")]` on a struct to customize its complete SQL
//! `FROM` fragment. Put `#[rusqlite(select = "...")]` on a field to customize
//! its select expression. This supports qualified columns, expressions, and
//! joins. See the [`RusqliteFetch`] derive for details and limitations.
//!
//! # Filtering safely
//!
//! [`RusqliteFetch::fetch_with_filter`] interpolates its argument verbatim. Do
//! not put untrusted data in that argument; use rusqlite placeholders and bound
//! parameters for dynamic values.

pub use rusqlite_derive_impl::RusqliteFetch;

/// Fetches values of a struct from SQLite.
///
/// Implementations are normally generated with [`#[derive(RusqliteFetch)]`](derive@RusqliteFetch).
/// The derive selects fields in declaration order and decodes each value through
/// [`rusqlite::Row::get`].
///
/// This trait is not intended to model writes or arbitrary queries. Use rusqlite
/// directly when a query needs bound parameters, custom result handling, or
/// incremental row processing.
pub trait RusqliteFetch: Sized {
    /// Fetches every row from the derive's configured `FROM` fragment.
    ///
    /// The generated statement has the form `SELECT <fields> FROM <source>;`.
    fn fetch(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Self>>;

    /// Fetches rows matching a SQL expression appended after `WHERE`.
    ///
    /// # Security
    ///
    /// The generated implementation interpolates `filter` into SQL verbatim; it
    /// does not escape data or bind parameters. Only pass SQL fragments fully
    /// controlled by the application. Use rusqlite directly for dynamic values.
    fn fetch_with_filter(conn: &rusqlite::Connection, filter: &str) -> rusqlite::Result<Vec<Self>>;
}
