//! Derive-based row mapping and record writing for [rusqlite](https://docs.rs/rusqlite).
//!
//! [`RusqliteFetch`] maps query rows into structs. [`RusqliteWrite`] generates
//! inserts, key-based updates, and SQLite upserts for complete table-backed
//! records. This crate is a small convenience layer, not a schema-aware ORM.
//!
//! # Example
//!
//! ```
//! use rusqlite_derive::{
//!     rusqlite::{self, Connection},
//!     RusqliteFetch, RusqliteWrite,
//! };
//!
//! #[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
//! #[rusqlite(table = "users")]
//! struct User {
//!     #[rusqlite(key)]
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
//! User { id: 3, name: "Linus".into() }.insert(&conn)?;
//!
//! let users = User::fetch(&conn)?;
//! let ada = User::fetch_with_filter(
//!     &conn,
//!     "name = ?1",
//!     rusqlite::params!["Ada"],
//! )?;
//!
//! assert_eq!(users.len(), 3);
//! assert_eq!(ada, vec![User { id: 1, name: "Ada".into() }]);
//! # Ok::<(), rusqlite::Error>(())
//! ```
//!
//! # Renaming the dependency
//!
//! Generated code refers to this wrapper as `::rusqlite_derive` by default. If
//! the dependency has been renamed, set its path on the derived struct:
//!
//! ```
//! use rusqlite_derive as rd;
//! use rd::RusqliteFetch;
//!
//! #[derive(RusqliteFetch)]
//! #[rusqlite(crate = "rd")]
//! struct Record {
//!     value: i64,
//! }
//! ```
//!
//! # Mapping attributes
//!
//! - `#[rusqlite(table = "...")]` names the writable table and acts as the
//!   default read source.
//! - `#[rusqlite(from = "...")]` replaces the complete read `FROM` fragment.
//! - `#[rusqlite(column = "...")]` maps a field to a storage column.
//! - `#[rusqlite(select = "...")]` replaces a field's read expression.
//! - `#[rusqlite(read_default)]` omits a field from reads and initializes it
//!   with [`Default::default`].
//! - `#[rusqlite(aggregate)]` collects selected values, grouping rows whose
//!   non-aggregated fields are equal. During item conversion, SQL `NULL` values
//!   rejected with
//!   [`FromSqlError::InvalidType`](rusqlite::types::FromSqlError::InvalidType)
//!   are omitted and other errors are returned. Use `aggregate(optional)` to
//!   handle an all-null group as `None` for an `Option<Collection>` field, and
//!   `aggregate(item = Type)` when the collection's `FromIterator` item type
//!   cannot be inferred.
//! - `#[rusqlite(key)]`, `#[rusqlite(skip_insert)]`,
//!   `#[rusqlite(skip_update)]`, and `#[rusqlite(skip_write)]` configure writes.
//!
//! Named fields default to their Rust field name for both reads and writes.
//! A `select`-only field on a writable struct must also specify `column` or
//! `skip_write`; tuple fields likewise require explicit mappings. Attribute
//! values are trusted SQL fragments and are not parsed, validated, or quoted.
//!
//! # Filtering safely
//!
//! [`RusqliteFetch::fetch_with_filter`] inserts its filter argument verbatim.
//! Bind dynamic values with placeholders and its parameter argument. Never
//! include untrusted SQL structure in the filter.

/// The rusqlite release used by the generated implementations.
pub use rusqlite;
pub use rusqlite_derive_impl::{RusqliteFetch, RusqliteWrite};

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "cannot infer an aggregate item type; use `#[rusqlite(aggregate(item = Type))]`"
)]
pub trait __Aggregate<Item>: FromIterator<Item> {}

impl<C, Item> __Aggregate<Item> for C where C: FromIterator<Item> {}

#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "an optional aggregate field must be `Option<C>` where `C: FromIterator<Item>`"
)]
pub trait __OptionalAggregate<Item>: Sized {
    fn __none() -> Self;

    fn __some<I: IntoIterator<Item = Item>>(values: I) -> Self;
}

impl<C, Item> __OptionalAggregate<Item> for Option<C>
where
    C: FromIterator<Item>,
{
    fn __none() -> Self {
        None
    }

    fn __some<I: IntoIterator<Item = Item>>(values: I) -> Self {
        Some(values.into_iter().collect())
    }
}

#[doc(hidden)]
pub fn __private_collect_aggregate_or_use_item_attribute<C, T>(
    values: Vec<rusqlite::types::Value>,
    column: usize,
    column_name: &str,
) -> rusqlite::Result<C>
where
    C: __Aggregate<T>,
    T: rusqlite::types::FromSql,
{
    values
        .into_iter()
        .filter_map(
            |value| match __private_aggregate_item(value, column, column_name) {
                Err(rusqlite::Error::InvalidColumnType(_, _, rusqlite::types::Type::Null)) => None,
                result => Some(result),
            },
        )
        .collect()
}

#[doc(hidden)]
pub fn __private_collect_optional_aggregate<O, T>(
    values: Vec<rusqlite::types::Value>,
    column: usize,
    column_name: &str,
) -> rusqlite::Result<O>
where
    O: __OptionalAggregate<T>,
    T: rusqlite::types::FromSql,
{
    if values
        .iter()
        .all(|value| value.data_type() == rusqlite::types::Type::Null)
    {
        Ok(O::__none())
    } else {
        __private_collect_aggregate_or_use_item_attribute::<Vec<T>, T>(values, column, column_name)
            .map(O::__some)
    }
}

#[doc(hidden)]
fn __private_aggregate_item<T>(
    value: rusqlite::types::Value,
    column: usize,
    column_name: &str,
) -> rusqlite::Result<T>
where
    T: rusqlite::types::FromSql,
{
    use rusqlite::types::FromSqlError;

    T::column_result((&value).into()).map_err(|error| match error {
        FromSqlError::InvalidType => {
            rusqlite::Error::InvalidColumnType(column, column_name.into(), value.data_type())
        }
        FromSqlError::OutOfRange(number) => {
            rusqlite::Error::IntegralValueOutOfRange(column, number)
        }
        FromSqlError::Utf8Error(error) => rusqlite::Error::Utf8Error(column, error),
        FromSqlError::Other(error) => {
            rusqlite::Error::FromSqlConversionFailure(column, value.data_type(), error)
        }
        error => {
            rusqlite::Error::FromSqlConversionFailure(column, value.data_type(), Box::new(error))
        }
    })
}

/// Fetches values of a struct from SQLite.
///
/// Implementations are normally generated by
/// [`#[derive(RusqliteFetch)]`](derive@RusqliteFetch). The derive selects fields
/// in declaration order and decodes non-aggregated fields with
/// [`rusqlite::Row::get`]. A field marked `#[rusqlite(read_default)]` is omitted
/// from the query and initialized with [`Default::default`].
///
/// A field marked `#[rusqlite(aggregate)]` collects its selected value across
/// rows whose non-aggregated fields are equal. The collection must implement
/// [`FromIterator`]. During item conversion, SQL `NULL` values rejected with
/// [`FromSqlError::InvalidType`](rusqlite::types::FromSqlError::InvalidType) are
/// omitted, values accepted by item types such as `Option<T>` are retained, and
/// other errors are returned. Use `#[rusqlite(aggregate(optional))]` to handle
/// an all-null group as `None` for an `Option<Collection>` field. Without this
/// option, every field—including an `Option`—uses its ordinary `FromIterator`
/// behavior. Use `#[rusqlite(aggregate(item = Type))]` when Rust cannot infer
/// the item type.
///
/// Use rusqlite directly for custom result handling or incremental row
/// processing.
pub trait RusqliteFetch: Sized {
    /// Fetches every row from the derive's configured `FROM` fragment.
    ///
    /// The generated statement has the form `SELECT <fields> FROM <source>;`.
    fn fetch(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Self>>;

    /// Fetches rows matching a SQL expression appended after `WHERE`.
    ///
    /// `filter` may include clauses that can follow a `WHERE` expression, such
    /// as `ORDER BY` or `LIMIT`.
    ///
    /// # Security
    ///
    /// The generated implementation inserts `filter` verbatim. Bind all
    /// dynamic values with placeholders and `params`; only the SQL structure
    /// of the filter must be controlled entirely by the application. Positional
    /// and named parameters are both supported through rusqlite's
    /// [`params!`](macro@rusqlite::params) and
    /// [`named_params!`](macro@rusqlite::named_params) macros.
    ///
    /// Parameters are bound to the complete generated statement, including any
    /// placeholders in configured `select` or `from` fragments.
    fn fetch_with_filter<P: rusqlite::Params>(
        conn: &rusqlite::Connection,
        filter: &str,
        params: P,
    ) -> rusqlite::Result<Vec<Self>>;
}

/// Writes a complete table-backed record to SQLite.
///
/// Implementations are normally generated by
/// [`#[derive(RusqliteWrite)]`](derive@RusqliteWrite). The derive requires an
/// explicit `#[rusqlite(table = "...")]` and one or more
/// `#[rusqlite(key)]` fields. All values are bound through
/// [`rusqlite::types::ToSql`].
///
/// `update` replaces every eligible non-key column. It is not a partial-update
/// API: in particular, `Option::None` writes SQL `NULL` rather than meaning
/// “leave unchanged”. Read-only fields must use `#[rusqlite(skip_write)]`;
/// generated keys normally use `#[rusqlite(key, skip_insert)]`.
pub trait RusqliteWrite {
    /// Inserts this record and returns SQLite's affected-row count.
    fn insert(&self, conn: &rusqlite::Connection) -> rusqlite::Result<usize>;

    /// Updates the row identified by all `key` fields and returns the
    /// affected-row count. Key fields themselves are not changed.
    fn update(&self, conn: &rusqlite::Connection) -> rusqlite::Result<usize>;

    /// Inserts this record or updates its eligible non-key columns when its
    /// declared key conflicts with an existing SQLite unique constraint.
    ///
    /// Keys should be non-null. Avoid this method when a key in the conflict
    /// target is generated or uses `skip_insert`: the insert path will normally
    /// generate or default that key instead of conflicting with `self`'s
    /// complete key. Use [`insert`](Self::insert) or [`update`](Self::update)
    /// explicitly for such models.
    fn upsert(&self, conn: &rusqlite::Connection) -> rusqlite::Result<usize>;
}
