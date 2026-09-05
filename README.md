# rusqlite-derive

`rusqlite-derive` maps SQLite rows to Rust structs. Deriving `RusqliteFetch` adds two helpers:

- `fetch`, which returns every row from a configured SQL source;
- `fetch_with_filter`, which adds a caller-provided expression after `WHERE`.

The crate is a small convenience layer over [`rusqlite`](https://crates.io/crates/rusqlite), not an ORM. It does not manage schemas or generate inserts, updates, deletes, relationships, or arbitrary queries.

## Installation

Add `rusqlite-derive` and `rusqlite` to your application. The generated code refers to `rusqlite` directly.

```toml
[dependencies]
rusqlite = "0.38"
rusqlite-derive = "0.1"
```

This crate does not enable any rusqlite features. If your system does not provide SQLite, enable rusqlite's `bundled` feature:

```toml
[dependencies]
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

    let active = User::fetch_with_filter(&conn, "active = ?1", rusqlite::params![true])?;
    assert_eq!(active, vec![User {
        id: 1,
        name: "Ada".to_owned(),
        active: true,
    }]);

    Ok(())
}
```

This derive generates queries equivalent to:

```sql
SELECT id, name, active FROM users;
SELECT id, name, active FROM users WHERE <filter>;
```

Selected values are decoded with `rusqlite::Row::get` in field declaration order. Each selected field type must therefore implement `FromSql`.

## Configuring the mapping

By default, the struct name becomes the SQL `FROM` source and each named field becomes a select expression. The following attributes override that behavior:

| Attribute                          | Location | Effect                                                                       |
|------------------------------------|----------|------------------------------------------------------------------------------|
| `#[rusqlite(table = "...")]`       | Struct   | Supplies a simple table as the read source when `from` is absent.            |
| `#[rusqlite(from = "...")]`        | Struct   | Replaces the complete `FROM` fragment.                                       |
| `#[rusqlite(column = "...")]`      | Field    | Supplies a simple field expression when `select` is absent.                  |
| `#[rusqlite(select = "...")]`      | Field    | Replaces the field's select expression.                                      |
| `#[rusqlite(read_default)]`        | Field    | Omits the field from the query and initializes it with `Default::default()`. |

`read_default` and `select` cannot be used on the same field.

Attribute values are unquoted SQL fragments. This permits qualified columns, expressions, aliases, and joins:

```rust
use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users AS u JOIN teams AS t ON t.id = u.team_id")]
struct UserWithTeam {
    #[rusqlite(select = "u.id")]
    user_id: i64,
    #[rusqlite(select = "upper(u.name)")]
    name: String,
    #[rusqlite(select = "t.name")]
    team_name: String,
}
```

The generated query is:

```sql
SELECT u.id, upper(u.name), t.name
FROM users AS u JOIN teams AS t ON t.id = u.team_id;
```

### Tuple structs

Tuple structs are supported. Because unnamed fields have no default column name, every field must use `select`, `column`, or `read_default`:

```rust
use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users")]
struct User(
    #[rusqlite(select = "id")] i64,
    #[rusqlite(select = "name")] String,
);
```

### Defaulted and generic fields

A defaulted field is not included in the `SELECT` list:

```rust
use std::marker::PhantomData;
use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users")]
struct User<T, Marker> {
    id: T,
    name: String,
    #[rusqlite(read_default)]
    marker: PhantomData<Marker>,
}
```

Generic parameters and existing `where` clauses are preserved. The derive adds `FromSql` bounds for selected field types that depend on generic parameters and `Default` bounds for defaulted field types that depend on them.

Unit structs and structs whose fields are all defaulted are also supported. The generated query selects a constant so that each matching source row still produces one value.

## Filtering safely

> **Warning:** `fetch_with_filter` inserts its filter argument into the SQL statement verbatim. Parameter binding protects values, not SQL structure.

Bind dynamic values with rusqlite placeholders:

```rust
let minimum_id = 10;
let users = User::fetch_with_filter(
    &conn,
    "id > ?1 ORDER BY id",
    rusqlite::params![minimum_id],
)?;
```

Named parameters are supported through rusqlite as well:

```rust
let users = User::fetch_with_filter(
    &conn,
    "id > :minimum_id ORDER BY id",
    rusqlite::named_params! { ":minimum_id": minimum_id },
)?;
```

Never construct the filter's SQL structure - such as column names, operators, or ordering expressions - from untrusted input. Parameters are bound to the complete generated statement, so placeholders in configured `select` or `from` fragments also consume parameters.

## Limitations

- Both helpers collect all matching rows into a `Vec`; they do not provide streaming or pagination.
- SQL identifiers and fragments are neither validated nor quoted.
- The derive only generates reads; writes and migrations remain the application's responsibility.
