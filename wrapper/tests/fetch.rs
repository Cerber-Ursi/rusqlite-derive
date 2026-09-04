use rusqlite::Connection;
use rusqlite_derive::RusqliteFetch;

#[derive(Debug, PartialEq, RusqliteFetch)]
struct DefaultRecord {
    id: i64,
    label: String,
    enabled: bool,
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "source_records")]
struct RenamedRecord {
    value: String,
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "users AS u JOIN teams AS t ON t.id = u.team_id")]
struct UserWithTeam {
    #[rusqlite(select = "u.id")]
    user_id: i64,
    #[rusqlite(select = "upper(u.name)")]
    upper_name: String,
    #[rusqlite(select = "t.name")]
    team_name: String,
}

#[allow(dead_code)]
#[derive(Debug, RusqliteFetch)]
#[rusqlite(from = "bad_values")]
struct ExpectedInteger {
    value: i64,
}

#[allow(dead_code)]
#[derive(Debug, RusqliteFetch)]
#[rusqlite(from = "table_that_does_not_exist")]
struct MissingTable {
    value: i64,
}

fn records_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE DefaultRecord (
             id INTEGER NOT NULL,
             label TEXT NOT NULL,
             enabled INTEGER NOT NULL
         );
         INSERT INTO DefaultRecord VALUES
             (2, 'second', 0),
             (1, 'first', 1),
             (3, 'third', 1);",
    )
    .unwrap();
    conn
}

#[test]
fn fetch_uses_default_source_and_field_mappings() {
    let conn = records_connection();

    let mut records = DefaultRecord::fetch(&conn).unwrap();
    records.sort_by_key(|record| record.id);

    assert_eq!(
        records,
        vec![
            DefaultRecord {
                id: 1,
                label: "first".into(),
                enabled: true,
            },
            DefaultRecord {
                id: 2,
                label: "second".into(),
                enabled: false,
            },
            DefaultRecord {
                id: 3,
                label: "third".into(),
                enabled: true,
            },
        ]
    );
}

#[test]
fn fetch_with_filter_applies_program_controlled_sql() {
    let conn = records_connection();

    let records = DefaultRecord::fetch_with_filter(&conn, "enabled = 1 ORDER BY id DESC").unwrap();

    assert_eq!(
        records,
        vec![
            DefaultRecord {
                id: 3,
                label: "third".into(),
                enabled: true,
            },
            DefaultRecord {
                id: 1,
                label: "first".into(),
                enabled: true,
            },
        ]
    );
}

#[test]
fn custom_source_is_used() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE source_records (value TEXT NOT NULL);
         INSERT INTO source_records VALUES ('mapped');",
    )
    .unwrap();

    assert_eq!(
        RenamedRecord::fetch(&conn).unwrap(),
        vec![RenamedRecord {
            value: "mapped".into()
        }]
    );
}

#[test]
fn custom_select_expressions_and_joins_are_used() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE teams (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
         CREATE TABLE users (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL,
             team_id INTEGER NOT NULL
         );
         INSERT INTO teams VALUES (10, 'Compilers');
         INSERT INTO users VALUES (1, 'Ada', 10);",
    )
    .unwrap();

    assert_eq!(
        UserWithTeam::fetch(&conn).unwrap(),
        vec![UserWithTeam {
            user_id: 1,
            upper_name: "ADA".into(),
            team_name: "Compilers".into(),
        }]
    );
}

#[test]
fn fetch_returns_an_empty_vector_for_an_empty_table() {
    let conn = records_connection();
    conn.execute("DELETE FROM DefaultRecord", []).unwrap();

    assert!(DefaultRecord::fetch(&conn).unwrap().is_empty());
}

#[test]
fn fetch_with_filter_returns_an_empty_vector_when_no_rows_match() {
    let conn = records_connection();

    assert!(
        DefaultRecord::fetch_with_filter(&conn, "id > 100")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn row_decode_errors_are_returned() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE bad_values (value TEXT NOT NULL);
         INSERT INTO bad_values VALUES ('not an integer');",
    )
    .unwrap();

    let error = ExpectedInteger::fetch(&conn).unwrap_err();
    assert!(matches!(error, rusqlite::Error::InvalidColumnType(..)));
}

#[test]
fn sql_errors_are_returned() {
    let conn = records_connection();

    assert!(MissingTable::fetch(&conn).is_err());
    assert!(DefaultRecord::fetch_with_filter(&conn, "not valid SQL").is_err());
}
