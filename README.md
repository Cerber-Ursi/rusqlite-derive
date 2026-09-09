# rusqlite-derive

`rusqlite-derive` maps SQLite rows to Rust structs and generates complete-record write operations. It is a small convenience layer over [`rusqlite`](https://crates.io/crates/rusqlite), not a schema-aware ORM.

Two derives are available:

- `RusqliteFetch` reads rows from a configured SQL source;
- `RusqliteWrite` inserts, updates, and upserts complete table-backed records.

The derives are independent: projections and views can derive only `RusqliteFetch`, while writable records can derive either or both traits.

## Installation

Add `rusqlite-derive` to your application. It re-exports the compatible rusqlite release used by the generated code.

```toml
[dependencies]
rusqlite-derive = "1"
```

If your system does not provide SQLite, enable the forwarded `bundled` feature:

```toml
rusqlite-derive = { version = "1", features = ["bundled"] }
```

You can also depend directly on a compatible `rusqlite` 0.40 release when you need features that `rusqlite-derive` does not forward. Cargo will unify the dependency and its enabled features.

### Renaming the dependency

Generated code refers to the wrapper as `::rusqlite_derive` by default. Set `crate` when the dependency has been renamed:

```rust
use rd::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(crate = "rd")]
struct Record {
    value: i64,
}
```

## Quick start

```rust
use rusqlite_derive::{
    rusqlite::{self, Connection},
    RusqliteFetch, RusqliteWrite,
};

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "users")]
struct User {
    #[rusqlite(key)]
    id: i64,
    #[rusqlite(column = "display_name")]
    name: String,
    active: bool,
}

fn main() -> rusqlite::Result<()> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "CREATE TABLE users (
             id INTEGER PRIMARY KEY,
             display_name TEXT NOT NULL,
             active INTEGER NOT NULL
         );",
    )?;

    let mut user = User {
        id: 1,
        name: "Ada".into(),
        active: true,
    };
    user.insert(&conn)?;

    user.active = false;
    user.update(&conn)?;

    let users = User::fetch(&conn)?;
    assert_eq!(users, vec![user]);
    Ok(())
}
```

## Mapping attributes

Struct attributes:

| Attribute                    | Used by         | Effect                                                                         |
|------------------------------|-----------------|--------------------------------------------------------------------------------|
| `#[rusqlite(table = "...")]` | Both derives    | Names the writable table and default read source. Required by `RusqliteWrite`. |
| `#[rusqlite(from = "...")]`  | `RusqliteFetch` | Replaces the complete read `FROM` fragment.                                    |

The read source is chosen in this order: `from`, `table`, then the Rust struct name. The write target is always `table`; it is never inferred from `from`, which may contain a join or subquery.

Field attributes:

| Attribute                     | Effect                                                                  |
|-------------------------------|-------------------------------------------------------------------------|
| `#[rusqlite(column = "...")]` | Sets the column used for writes and as the default read expression.     |
| `#[rusqlite(select = "...")]` | Overrides the read expression.                                          |
| `#[rusqlite(read_default)]`   | Omits the field from reads and uses `Default::default()`.               |
| `#[rusqlite(aggregate)]`      | Collects one selected value per row while grouping equal records.       |
| `aggregate(item = Type)`      | Specifies an aggregate item type when it cannot be inferred.            |
| `#[rusqlite(key)]`            | Includes the field in update predicates and the upsert conflict target. |
| `#[rusqlite(skip_insert)]`    | Omits the field from inserts and the insert path of upserts.            |
| `#[rusqlite(skip_update)]`    | Omits a non-key field from update assignments.                          |
| `#[rusqlite(skip_write)]`     | Excludes a field from all write operations.                             |

For an ordinary named field, the Rust field name is the default read expression and writable column. `column` can rename both; `select` can independently override the read expression:

```rust
#[rusqlite(column = "name", select = "upper(u.name)")]
name: String,
```

A `select` expression alone does not identify a writable column. When deriving `RusqliteWrite`, such a field must also specify `column` or `skip_write`.

Tuple fields have no implicit column name. For reads, each tuple field requires `select`, `column`, or `read_default`; for writes, it requires `column` or `skip_write`.

Attribute strings are trusted, unquoted SQL fragments. The macro checks mapping consistency but does not parse identifiers or inspect the database schema.

## Reading

`RusqliteFetch` supplies:

```rust
fn fetch(conn: &Connection) -> rusqlite::Result<Vec<Self>>;
fn fetch_with_filter<P: rusqlite::Params>(
    conn: &Connection,
    filter: &str,
    params: P,
) -> rusqlite::Result<Vec<Self>>;
```

For the quick-start model, `fetch` generates:

```sql
SELECT id, display_name, active FROM users;
```

Selected values are decoded with `rusqlite::Row::get` in field declaration order. Selected field types must implement `FromSql`.

### Projections and joins

`from` and `select` may contain aliases, joins, and expressions:

```rust
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

### Aggregated fields

Mark a collection field with `aggregate` to combine one-to-many query rows into records:

```rust
use std::collections::BTreeSet;

#[derive(RusqliteFetch)]
#[rusqlite(from = "users AS u JOIN user_tags AS t ON t.user_id = u.id")]
struct UserTags {
    #[rusqlite(select = "u.id")]
    user_id: i64,
    #[rusqlite(select = "t.tag", aggregate)]
    tags: BTreeSet<String>,
}
```

Each SQL row contributes one selected value to every aggregated field. Rows whose selected, non-aggregated fields compare equal are combined, so those field types must implement `PartialEq`. `read_default` fields do not participate in that comparison. `Vec<T>` preserves the SQL row order and duplicates; `BTreeSet<T>` sorts and deduplicates. Use an `ORDER BY` clause when vector order matters.

Aggregation is not limited to iterable collection types. The field type only needs to implement `FromIterator<T>` for an item type `T` that implements `FromSql`, so custom accumulators that consume values without storing or yielding them are supported. This also works directly with standard collections such as `Vec`, `VecDeque`, `LinkedList`, `BTreeSet`, and `HashSet`. For a nullable value from a `LEFT JOIN`, use a collection such as `Vec<Option<T>>`; aggregation does not implicitly discard `NULL`.

The macro asks Rust to infer `T` from the field type. Specify it explicitly when the field type is generic or has multiple applicable `FromIterator` implementations:

```rust
#[rusqlite(select = "t.value", aggregate(item = i64))]
total: Accumulator,
```

For this example, the generated implementation requires `Accumulator: FromIterator<i64>` and `i64: FromSql`.

Aggregation happens in memory after executing the ordinary generated `SELECT`; it does not add a SQL `GROUP BY`. The result groups preserve the order in which their first rows occur. On a struct that also derives `RusqliteWrite`, an aggregated projection normally needs `skip_write` because the collection itself is not a writable SQLite value.

### Filtering safely

`fetch_with_filter` inserts its filter argument into SQL verbatim. Parameter binding protects values, not SQL structure:

```rust
let users = User::fetch_with_filter(
    &conn,
    "active = ?1 ORDER BY id LIMIT ?2",
    rusqlite::params![true, 10],
)?;
```

Never construct SQL structure - such as column names or operators - from untrusted input.

Placeholders in configured `select` and `from` fragments also consume parameters.

## Writing

`RusqliteWrite` requires an explicit `table`, at least one `key`, at least one insertable field, and at least one updatable non-key field. It supplies:

```rust
fn insert(&self, conn: &Connection) -> rusqlite::Result<usize>;
fn update(&self, conn: &Connection) -> rusqlite::Result<usize>;
fn upsert(&self, conn: &Connection) -> rusqlite::Result<usize>;
```

Every value is bound through `ToSql`. The methods return SQLite's affected-row count, including `Ok(0)` when an update finds no matching row.

For the quick-start model, the generated statements are equivalent to:

```sql
INSERT INTO users (id, display_name, active) VALUES (?1, ?2, ?3);

UPDATE users
SET display_name = ?1, active = ?2
WHERE id = ?3;

INSERT INTO users (id, display_name, active) VALUES (?1, ?2, ?3)
ON CONFLICT (id) DO UPDATE SET
    display_name = excluded.display_name,
    active = excluded.active;
```

### Keys and generated values

Multiple `key` fields form an `AND` update predicate and a composite upsert conflict target. SQLite must have a corresponding primary-key or unique constraint; the derive cannot verify this.

Use `skip_insert` for a database-generated key:

```rust
#[rusqlite(key, skip_insert)]
id: i64,
```

A key cannot use `skip_write`, because it is still required by update predicates. The derive diagnostic points generated-key users to `skip_insert`.

Avoid `upsert` when any key in the conflict target is generated or skipped on insert. The insert path does not supply that key from `self`, so SQLite usually generates or defaults it instead of finding a conflict with `self`'s complete key. Use `insert` for new records and `update` for records fetched previously.

Keys should also be non-null. An update predicate such as `key = NULL` matches nothing, and SQLite unique constraints commonly permit multiple `NULL` values, so an upsert with a null key may repeatedly insert rows.

### Complete updates

`update` assigns every eligible non-key field. It does not track changes or provide patch semantics. In particular, `Option::None` writes SQL `NULL`; it does not mean “leave this column unchanged”. Use `skip_update` for a field that must never be assigned by updates, or use rusqlite directly for dynamic patches.

`upsert` uses the same insert field set as `insert` and the same assignment set as `update`. Fields included in the insert are assigned from `excluded`; an updatable field marked `skip_insert` is bound separately so the conflict path writes its value from `self`, while the insert path still uses its database default.

### Read-only and local fields

Fields that are not stored must explicitly opt out of writes:

```rust
#[rusqlite(select = "upper(name)", skip_write)]
uppercase_name: String,

#[rusqlite(read_default, skip_write)]
local_state: LocalState,
```

`read_default` and write participation are independent. A read-defaulted field can still be writable when it has a resolved column and is not skipped.

## Generics and fieldless structs

Generic parameters and existing `where` clauses are preserved. The derives add bounds for generic-dependent fields:

- `FromSql` for selected fields;
- `Default` for `read_default` fields;
- `ToSql` for writable fields.

`RusqliteFetch` supports unit, empty, and all-read-defaulted structs by selecting a constant. `RusqliteWrite` intentionally rejects models without a key, insertable field, or updatable non-key field.

## Limitations

- Fetch helpers materialize all matching SQL rows; aggregated fields are grouped in memory.
- Writes operate on complete records; there are no partial updates or batches.
- There is no `RETURNING` helper or automatic hydration of generated values.
- Deletes, migrations, relationships, optimistic locking, and schema management remain the application's responsibility.
- SQL identifiers and fragments are neither parsed, validated, nor quoted. Inferred raw Rust identifiers have their `r#` prefix removed, but names that are SQLite keywords still need an explicit quoted `column` fragment.
- Schema mismatches, missing unique constraints, and constraint violations are reported by SQLite at runtime.
