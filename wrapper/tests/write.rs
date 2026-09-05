use rusqlite::{Connection, Error};
use rusqlite_derive::{RusqliteFetch, RusqliteWrite};
use std::marker::PhantomData;

#[derive(Clone, Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "users")]
struct User {
    #[rusqlite(key)]
    id: i64,
    #[rusqlite(column = "display_name")]
    name: String,
    active: bool,
}

fn users_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE users (
             id INTEGER PRIMARY KEY,
             display_name TEXT NOT NULL,
             active INTEGER NOT NULL
         );",
    )
    .unwrap();
    conn
}

#[test]
fn insert_uses_default_and_custom_columns() {
    let conn = users_connection();
    let user = User {
        id: 1,
        name: "Ada".into(),
        active: true,
    };

    assert_eq!(user.insert(&conn).unwrap(), 1);
    assert_eq!(User::fetch(&conn).unwrap(), vec![user]);
}

#[test]
fn update_uses_the_key_and_returns_the_affected_count() {
    let conn = users_connection();
    conn.execute("INSERT INTO users VALUES (1, 'old', 0)", [])
        .unwrap();

    let user = User {
        id: 1,
        name: "new".into(),
        active: true,
    };
    assert_eq!(user.update(&conn).unwrap(), 1);
    assert_eq!(User::fetch(&conn).unwrap(), vec![user.clone()]);

    let absent = User { id: 99, ..user };
    assert_eq!(absent.update(&conn).unwrap(), 0);
}

#[test]
fn upsert_handles_both_insert_and_update_paths() {
    let conn = users_connection();
    let mut user = User {
        id: 1,
        name: "first".into(),
        active: false,
    };

    assert_eq!(user.upsert(&conn).unwrap(), 1);
    user.name = "updated".into();
    user.active = true;
    assert_eq!(user.upsert(&conn).unwrap(), 1);
    assert_eq!(User::fetch(&conn).unwrap(), vec![user]);
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "memberships")]
struct Membership {
    #[rusqlite(key)]
    user_id: i64,
    #[rusqlite(key)]
    group_id: i64,
    role: String,
}

#[test]
fn composite_keys_are_used_for_updates_and_upserts() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE memberships (
             user_id INTEGER NOT NULL,
             group_id INTEGER NOT NULL,
             role TEXT NOT NULL,
             PRIMARY KEY (user_id, group_id)
         );
         INSERT INTO memberships VALUES (1, 1, 'member'), (1, 2, 'owner');",
    )
    .unwrap();

    let mut membership = Membership {
        user_id: 1,
        group_id: 1,
        role: "admin".into(),
    };
    assert_eq!(membership.update(&conn).unwrap(), 1);
    membership.role = "maintainer".into();
    assert_eq!(membership.upsert(&conn).unwrap(), 1);
    assert_eq!(
        Membership::fetch_with_filter(&conn, "user_id = 1 ORDER BY group_id", []).unwrap(),
        vec![
            membership,
            Membership {
                user_id: 1,
                group_id: 2,
                role: "owner".into(),
            },
        ]
    );
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "generated_records")]
struct GeneratedRecord {
    #[rusqlite(key, skip_insert)]
    id: i64,
    value: String,
}

#[test]
fn skip_insert_allows_a_database_generated_key() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generated_records (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             value TEXT NOT NULL
         );",
    )
    .unwrap();

    let record = GeneratedRecord {
        id: 0,
        value: "generated".into(),
    };
    assert_eq!(record.insert(&conn).unwrap(), 1);
    assert_eq!(
        conn.query_row("SELECT id FROM generated_records", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );

    // The skipped key is generated again, so this does not conflict with id 1.
    assert_eq!(record.upsert(&conn).unwrap(), 1);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM generated_records", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "audit_records", from = "audit_records AS a")]
struct AuditRecord {
    #[rusqlite(key, column = "id", select = "a.id")]
    id: i64,
    #[rusqlite(column = "value", select = "upper(a.value)")]
    value: String,
    #[rusqlite(skip_update)]
    created_by: String,
    #[rusqlite(select = "a.value || '!'", skip_write)]
    display: String,
    #[rusqlite(read_default, skip_write)]
    marker: PhantomData<()>,
}

#[test]
fn read_expressions_and_skip_attributes_are_independent_from_columns() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE audit_records (
             id INTEGER PRIMARY KEY,
             value TEXT NOT NULL,
             created_by TEXT NOT NULL
         );",
    )
    .unwrap();

    let mut record = AuditRecord {
        id: 1,
        value: "first".into(),
        created_by: "creator".into(),
        display: String::new(),
        marker: PhantomData,
    };
    record.insert(&conn).unwrap();
    record.value = "second".into();
    record.created_by = "replacement".into();
    record.update(&conn).unwrap();
    record.value = "third".into();
    record.upsert(&conn).unwrap();

    let stored = conn
        .query_row(
            "SELECT value, created_by FROM audit_records WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(stored, ("third".into(), "creator".into()));

    assert_eq!(
        AuditRecord::fetch(&conn).unwrap(),
        vec![AuditRecord {
            id: 1,
            value: "THIRD".into(),
            created_by: "creator".into(),
            display: "third!".into(),
            marker: PhantomData,
        }]
    );
}

#[derive(Debug, PartialEq, RusqliteWrite)]
#[rusqlite(table = "tuple_records")]
struct TupleRecord(
    #[rusqlite(column = "id", key)] i64,
    #[rusqlite(column = "value")] Option<String>,
);

#[test]
fn tuple_fields_and_null_values_are_written() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE tuple_records (id INTEGER PRIMARY KEY, value TEXT);")
        .unwrap();

    let mut record = TupleRecord(1, None);
    record.insert(&conn).unwrap();
    assert!(
        conn.query_row("SELECT value IS NULL FROM tuple_records", [], |row| {
            row.get::<_, bool>(0)
        })
        .unwrap()
    );

    record.1 = Some("updated".into());
    assert_eq!(record.update(&conn).unwrap(), 1);
    assert_eq!(
        conn.query_row("SELECT value FROM tuple_records", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "updated"
    );

    record.1 = Some("upserted".into());
    assert_eq!(record.upsert(&conn).unwrap(), 1);
    assert_eq!(
        conn.query_row("SELECT value FROM tuple_records", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "upserted"
    );
}

#[test]
fn write_methods_work_inside_a_transaction() {
    let mut conn = users_connection();
    {
        let transaction = conn.transaction().unwrap();
        User {
            id: 1,
            name: "rolled back".into(),
            active: true,
        }
        .insert(&transaction)
        .unwrap();
        transaction.rollback().unwrap();
    }
    assert!(User::fetch(&conn).unwrap().is_empty());

    {
        let transaction = conn.transaction().unwrap();
        User {
            id: 2,
            name: "committed".into(),
            active: true,
        }
        .insert(&transaction)
        .unwrap();
        transaction.commit().unwrap();
    }
    assert_eq!(User::fetch(&conn).unwrap().len(), 1);
}

#[derive(RusqliteWrite)]
#[rusqlite(table = "missing_column_records")]
struct MissingColumnRecord {
    #[rusqlite(key)]
    id: i64,
    missing: String,
}

fn assert_sqlite_failure(
    result: rusqlite::Result<usize>,
    expected_extended_code: i32,
    expected_message: &str,
) {
    match result.unwrap_err() {
        Error::SqliteFailure(error, Some(message)) => {
            assert_eq!(error.extended_code, expected_extended_code);
            assert!(
                message.contains(expected_message),
                "expected `{message}` to contain `{expected_message}`"
            );
        }
        error => panic!("expected a SQLite failure, got {error:?}"),
    }
}

#[test]
fn sqlite_write_errors_are_returned() {
    let conn = users_connection();
    let user = User {
        id: 1,
        name: "duplicate".into(),
        active: true,
    };
    user.insert(&conn).unwrap();
    assert_sqlite_failure(
        user.insert(&conn),
        rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY,
        "UNIQUE constraint failed: users.id",
    );

    let missing_table = Connection::open_in_memory().unwrap();
    assert_sqlite_failure(
        user.insert(&missing_table),
        rusqlite::ffi::SQLITE_ERROR,
        "no such table: users",
    );

    conn.execute_batch(
        "CREATE TABLE missing_column_records (id INTEGER PRIMARY KEY, actual TEXT NOT NULL);",
    )
    .unwrap();
    assert_sqlite_failure(
        MissingColumnRecord {
            id: 1,
            missing: "value".into(),
        }
        .insert(&conn),
        rusqlite::ffi::SQLITE_ERROR,
        "table missing_column_records has no column named missing",
    );
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "generic_write_records")]
struct GenericWriteRecord<T> {
    #[rusqlite(key)]
    id: i64,
    value: T,
}

#[test]
fn generic_writable_fields_round_trip() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generic_write_records (id INTEGER PRIMARY KEY, value TEXT NOT NULL);",
    )
    .unwrap();

    let mut record = GenericWriteRecord {
        id: 1,
        value: String::from("inserted"),
    };
    record.insert(&conn).unwrap();
    record.value = "updated".into();
    record.upsert(&conn).unwrap();
    assert_eq!(GenericWriteRecord::fetch(&conn).unwrap(), vec![record]);
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "raw_records")]
struct RawRecord {
    #[rusqlite(key)]
    id: i64,
    r#type: String,
}

#[test]
fn raw_rust_identifiers_drop_the_raw_prefix_in_sql() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE raw_records (id INTEGER PRIMARY KEY, type TEXT NOT NULL);")
        .unwrap();

    let record = RawRecord {
        id: 1,
        r#type: "event".into(),
    };
    record.insert(&conn).unwrap();
    assert_eq!(RawRecord::fetch(&conn).unwrap(), vec![record]);
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "default_records")]
struct DatabaseDefaultRecord {
    #[rusqlite(key)]
    id: i64,
    #[rusqlite(skip_insert)]
    defaulted: String,
    value: String,
}

#[test]
fn upsert_binds_updatable_fields_that_are_skipped_on_insert() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE default_records (
             id INTEGER PRIMARY KEY,
             defaulted TEXT NOT NULL DEFAULT 'database default',
             value TEXT NOT NULL
         );",
    )
    .unwrap();

    let mut record = DatabaseDefaultRecord {
        id: 1,
        defaulted: "ignored during insert".into(),
        value: "first".into(),
    };
    record.insert(&conn).unwrap();
    assert_eq!(
        DatabaseDefaultRecord::fetch(&conn).unwrap()[0].defaulted,
        "database default"
    );

    record.defaulted = "from self".into();
    record.value = "second".into();
    record.upsert(&conn).unwrap();
    assert_eq!(DatabaseDefaultRecord::fetch(&conn).unwrap(), vec![record]);
}

#[derive(Debug, PartialEq, RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "read_default_records")]
struct WritableReadDefault {
    #[rusqlite(key)]
    id: i64,
    #[rusqlite(read_default)]
    hidden: String,
    visible: String,
}

#[test]
fn read_default_fields_can_still_be_written() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE read_default_records (
             id INTEGER PRIMARY KEY,
             hidden TEXT NOT NULL,
             visible TEXT NOT NULL
         );",
    )
    .unwrap();

    let record = WritableReadDefault {
        id: 1,
        hidden: "stored".into(),
        visible: "shown".into(),
    };
    record.insert(&conn).unwrap();
    assert_eq!(
        conn.query_row("SELECT hidden FROM read_default_records", [], |row| row
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "stored"
    );
    assert_eq!(
        WritableReadDefault::fetch(&conn).unwrap()[0].hidden,
        String::default()
    );
}
