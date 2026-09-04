# rusqlite-derive

`rusqlite-derive` is a small derive macro for mapping SQLite query rows to Rust structs. It generates two read helpers on top of [`rusqlite`](https://crates.io/crates/rusqlite): one that fetches every row and one that appends a caller-supplied predicate after `WHERE`.

This crate is intentionally ORM-*ish*, rather than a full ORM. It generates `SELECT` statements and decodes rows; schema management, relationships, inserts, updates, deletes, and parameterized query construction remain the application's responsibility.

## Installation

Add both `rusqlite-derive` and `rusqlite` to your application. The generated implementation refers to `rusqlite` directly.

```toml
[dependencies]
rusqlite = "0.38"
rusqlite-derive = "0.1"
```

The crate uses rusqlite without default features. If a system SQLite library is not available, enable rusqlite's `bundled` feature:

```toml
rusqlite = { version = "0.38", features = ["bundled"] }
rusqlite-derive = "0.1"
```

## Quick start

```rust
use rusqlite::Connection;
use rusqlite_derive::RusqliteFetch;

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "users")]
struct User {
    id: i64,
    name: String,
    active: bool,
}

fn main() -> rusqlite::Result<()> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "CREATE TABLE users (id INTEGER, name TEXT, active INTEGER);
         INSERT INTO users VALUES
             (1, 'Ada', 1),
             (2, 'Grace', 0);",
    )?;

    let users = User::fetch(&conn)?;
    assert_eq!(users.len(), 2);

    let active_users = User::fetch_with_filter(&conn, "active = 1")?;
    assert_eq!(active_users, vec![User {
        id: 1,
        name: "Ada".to_owned(),
        active: true,
    }]);

    Ok(())
}
```

The derive above generates queries equivalent to:

```sql
SELECT id, name, active FROM users;
SELECT id, name, active FROM users WHERE <filter>;
```

Each selected value is decoded with `rusqlite::Row::get`, in struct field declaration order. Consequently, every field type must implement rusqlite's `FromSql`.

## Mapping SQL expressions

Without attributes, the Rust struct name is used as the `FROM` fragment and each Rust field name is used as its select expression. Attributes can override either value:

- `#[rusqlite(from = "...")]` on the struct sets the complete SQL `FROM` fragment.
- `#[rusqlite(select = "...")]` on a field sets that field's SQL select expression.

Tuple structs are also supported, but every field must have an explicit `select` expression because unnamed fields have no column name to use by default:

```rust
use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users")]
struct User(
    #[rusqlite(select = "id")] i64,
    #[rusqlite(select = "name")] String,
);
```

The attribute values are SQL fragments, not quoted identifiers. This makes aliases, expressions, and joins possible:

```rust
use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users AS u JOIN teams AS t ON t.id = u.team_id")]
struct UserWithTeam {
    #[rusqlite(select = "u.id")]
    user_id: i64,
    #[rusqlite(select = "u.name")]
    user_name: String,
    #[rusqlite(select = "t.name")]
    team_name: String,
}
```

This mapping produces:

```sql
SELECT u.id, u.name, t.name
FROM users AS u JOIN teams AS t ON t.id = u.team_id;
```

## Filtering and security

`fetch_with_filter` inserts its `filter` argument verbatim after `WHERE`. It does **not** bind parameters or escape values. Never construct the filter from untrusted or user-provided input:

```rust
# use rusqlite_derive::RusqliteFetch;
# #[derive(RusqliteFetch)]
# #[rusqlite(from = "users")]
# struct User { id: i64 }
# fn example(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
// Appropriate only when the entire fragment is controlled by the program.
let users = User::fetch_with_filter(conn, "id > 10")?;
# Ok(())
# }
```

For dynamic values, use rusqlite directly with placeholders and bound parameters.

## Current limitations

- Only non-generic structs with one or more fields are supported; unit structs are not.
- Every tuple struct field must specify `#[rusqlite(select = "...")]`.
- Fetches return all matching rows as a `Vec`; pagination and streaming are not generated.
- SQL identifiers and fragments are not validated or quoted by the macro.
- `fetch_with_filter` has no parameter-binding API.
- The derive only generates reads; it does not generate write operations or migrations.
