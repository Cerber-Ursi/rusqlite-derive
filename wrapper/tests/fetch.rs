use rusqlite_derive::{
    RusqliteFetch,
    rusqlite::{
        Connection,
        types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    },
};
use std::{
    collections::{BTreeSet, VecDeque},
    marker::PhantomData,
};

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
#[rusqlite(from = "tuple_records")]
struct TupleRecord(
    #[rusqlite(select = "record_id")] i64,
    #[rusqlite(select = "label || '!'")] String,
);

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

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "generic_records")]
struct GenericRecord<T> {
    id: T,
    label: String,
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "generic_records")]
struct DefaultedRecord {
    id: i64,
    #[rusqlite(read_default)]
    skipped: bool,
    label: String,
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "generic_records")]
struct AllDefault<T>(#[rusqlite(read_default)] PhantomData<T>);

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "unit_records")]
struct UnitRecord;

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "brace_records")]
struct BraceRecord {
    #[rusqlite(select = "'{}'")]
    value: String,
}

#[derive(Debug, PartialEq)]
struct Sum(i64);

impl FromIterator<i64> for Sum {
    fn from_iter<T: IntoIterator<Item = i64>>(iter: T) -> Self {
        Self(iter.into_iter().sum())
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
struct StrictString(String);

impl FromSql for StrictString {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Null => Err(FromSqlError::Other(Box::new(std::io::Error::other(
                "null is invalid for StrictString",
            )))),
            value => String::column_result(value).map(Self),
        }
    }
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "parents AS p JOIN children AS c ON c.parent_id = p.id")]
struct ParentWithChildren {
    #[rusqlite(select = "p.id")]
    id: i64,
    #[rusqlite(select = "p.name")]
    name: String,
    #[rusqlite(select = "c.value", aggregate)]
    children: Vec<String>,
    #[rusqlite(select = "c.value", aggregate)]
    unique_children: BTreeSet<String>,
    #[rusqlite(select = "c.value", aggregate)]
    queued_children: VecDeque<String>,
    #[rusqlite(select = "c.parent_id", aggregate)]
    child_id_sum: Sum,
}

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "parents AS p JOIN children AS c ON c.parent_id = p.id")]
struct GenericParent<C> {
    #[rusqlite(select = "p.id")]
    id: i64,
    #[rusqlite(select = "c.value", aggregate(item = String))]
    children: C,
}

type OptionalChildren = Option<Vec<String>>;

#[derive(Debug, PartialEq, RusqliteFetch)]
#[rusqlite(from = "parents AS p LEFT JOIN children AS c ON c.parent_id = p.id")]
struct ParentWithOptionalChildren {
    #[rusqlite(select = "p.id")]
    id: i64,
    #[rusqlite(select = "c.value", aggregate)]
    children: Vec<String>,
    #[rusqlite(select = "c.value", aggregate(optional))]
    optional_children: Option<Vec<String>>,
    #[rusqlite(select = "c.value", aggregate)]
    nullable_children: Vec<Option<String>>,
    #[rusqlite(select = "c.value", aggregate(optional))]
    optional_nullable_children: Option<Vec<Option<String>>>,
    #[rusqlite(select = "c.value", aggregate(optional))]
    aliased_optional_children: OptionalChildren,
    #[rusqlite(select = "c.value", aggregate)]
    ordinary_option_accumulator: Option<Vec<String>>,
}

#[allow(dead_code)]
#[derive(Debug, RusqliteFetch)]
#[rusqlite(from = "parents AS p LEFT JOIN children AS c ON c.parent_id = p.id")]
struct ParentWithStrictChildren {
    #[rusqlite(select = "p.id")]
    id: i64,
    #[rusqlite(select = "c.value", aggregate)]
    children: Vec<StrictString>,
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
fn fetch_with_filter_accepts_a_static_filter_and_ordering() {
    let conn = records_connection();

    let records =
        DefaultRecord::fetch_with_filter(&conn, "enabled = 1 ORDER BY id DESC", []).unwrap();

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
fn fetch_with_filter_binds_positional_values_and_a_limit() {
    let conn = records_connection();

    let records = DefaultRecord::fetch_with_filter(
        &conn,
        "enabled = ?1 AND id >= ?2 ORDER BY id DESC LIMIT ?3",
        rusqlite::params![true, 1_i64, 1_i64],
    )
    .unwrap();

    assert_eq!(records[0].id, 3);
    assert_eq!(records.len(), 1);
}

#[test]
fn fetch_with_filter_binds_named_values() {
    let conn = records_connection();

    let records = DefaultRecord::fetch_with_filter(
        &conn,
        "enabled = :enabled ORDER BY id",
        rusqlite::named_params! { ":enabled": true },
    )
    .unwrap();

    assert_eq!(
        records.iter().map(|record| record.id).collect::<Vec<_>>(),
        [1, 3]
    );
}

#[test]
fn fetch_with_filter_treats_quotes_as_part_of_a_bound_value() {
    let conn = records_connection();
    conn.execute(
        "INSERT INTO DefaultRecord VALUES (?1, ?2, ?3)",
        rusqlite::params![4_i64, "O'Reilly", true],
    )
    .unwrap();

    let records =
        DefaultRecord::fetch_with_filter(&conn, "label = ?1", rusqlite::params!["O'Reilly"])
            .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, 4);
}

#[test]
fn fetch_with_filter_returns_parameter_count_errors() {
    let conn = records_connection();

    let error = DefaultRecord::fetch_with_filter(&conn, "id = ?1", []).unwrap_err();
    assert!(matches!(
        error,
        rusqlite::Error::InvalidParameterCount(0, 1)
    ));
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
fn tuple_struct_fields_use_explicit_select_expressions() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tuple_records (record_id INTEGER NOT NULL, label TEXT NOT NULL);
         INSERT INTO tuple_records VALUES (7, 'tuple');",
    )
    .unwrap();

    assert_eq!(
        TupleRecord::fetch(&conn).unwrap(),
        vec![TupleRecord(7, "tuple!".into())]
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
fn generic_fields_are_decoded() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generic_records (id INTEGER NOT NULL, label TEXT NOT NULL);
         INSERT INTO generic_records VALUES (8, 'generic'), (9, 'second');",
    )
    .unwrap();

    assert_eq!(
        GenericRecord::<i64>::fetch(&conn).unwrap(),
        vec![
            GenericRecord {
                id: 8,
                label: "generic".into(),
            },
            GenericRecord {
                id: 9,
                label: "second".into(),
            },
        ]
    );
}

#[test]
fn generic_field_decode_errors_are_returned() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generic_records (id TEXT NOT NULL, label TEXT NOT NULL);
         INSERT INTO generic_records VALUES ('not an integer', 'generic');",
    )
    .unwrap();

    let error = GenericRecord::<i64>::fetch(&conn).unwrap_err();
    assert!(matches!(error, rusqlite::Error::InvalidColumnType(..)));
}

#[test]
fn default_fields_are_not_selected() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generic_records (id INTEGER NOT NULL, label TEXT NOT NULL);
         INSERT INTO generic_records VALUES (8, 'defaulted');",
    )
    .unwrap();

    assert_eq!(
        DefaultedRecord::fetch(&conn).unwrap(),
        vec![DefaultedRecord {
            id: 8,
            skipped: false,
            label: "defaulted".into(),
        }]
    );
}

#[test]
fn an_all_default_mapping_still_produces_one_value_per_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE generic_records (id INTEGER NOT NULL);
         INSERT INTO generic_records VALUES (1), (2);",
    )
    .unwrap();

    assert_eq!(
        AllDefault::<String>::fetch(&conn).unwrap(),
        vec![AllDefault(PhantomData), AllDefault(PhantomData)]
    );
}

#[test]
fn unit_structs_produce_one_value_per_row() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE unit_records (id INTEGER NOT NULL);
         INSERT INTO unit_records VALUES (1), (2);",
    )
    .unwrap();

    assert_eq!(
        UnitRecord::fetch(&conn).unwrap(),
        vec![UnitRecord, UnitRecord]
    );
}

#[test]
fn configured_sql_braces_are_not_treated_as_format_placeholders() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE brace_records (id INTEGER NOT NULL);
         INSERT INTO brace_records VALUES (1), (2);",
    )
    .unwrap();

    assert_eq!(
        BraceRecord::fetch(&conn).unwrap(),
        vec![
            BraceRecord { value: "{}".into() },
            BraceRecord { value: "{}".into() },
        ]
    );
    assert_eq!(
        BraceRecord::fetch_with_filter(&conn, "id = ?1", rusqlite::params![2_i64]).unwrap(),
        vec![BraceRecord { value: "{}".into() }]
    );
}

#[test]
fn aggregate_fields_combine_rows_by_non_aggregated_values() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE parents (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
         CREATE TABLE children (parent_id INTEGER NOT NULL, value TEXT NOT NULL);
         INSERT INTO parents VALUES (1, 'first'), (2, 'second');
         INSERT INTO children VALUES
             (1, 'beta'), (2, 'only'), (1, 'alpha'), (1, 'alpha');",
    )
    .unwrap();

    let records = ParentWithChildren::fetch_with_filter(
        &conn,
        "p.id >= ?1 ORDER BY p.id, c.rowid",
        rusqlite::params![1_i64],
    )
    .unwrap();

    assert_eq!(
        records,
        vec![
            ParentWithChildren {
                id: 1,
                name: "first".into(),
                children: vec!["beta".into(), "alpha".into(), "alpha".into()],
                unique_children: BTreeSet::from(["alpha".into(), "beta".into()]),
                queued_children: VecDeque::from(["beta".into(), "alpha".into(), "alpha".into()]),
                child_id_sum: Sum(3),
            },
            ParentWithChildren {
                id: 2,
                name: "second".into(),
                children: vec!["only".into()],
                unique_children: BTreeSet::from(["only".into()]),
                queued_children: VecDeque::from(["only".into()]),
                child_id_sum: Sum(2),
            },
        ]
    );

    assert_eq!(
        GenericParent::<Vec<String>>::fetch_with_filter(&conn, "p.id = 1 ORDER BY c.rowid", [],)
            .unwrap(),
        vec![GenericParent {
            id: 1,
            children: vec!["beta".into(), "alpha".into(), "alpha".into()],
        }]
    );
}

#[test]
fn aggregate_fields_handle_nulls_from_left_joins() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE parents (id INTEGER PRIMARY KEY);
         CREATE TABLE children (parent_id INTEGER NOT NULL, value TEXT);
         INSERT INTO parents VALUES (1), (2);
         INSERT INTO children VALUES (1, 'first'), (1, NULL), (1, 'second');",
    )
    .unwrap();

    let records =
        ParentWithOptionalChildren::fetch_with_filter(&conn, "1 ORDER BY p.id, c.rowid", [])
            .unwrap();

    assert_eq!(
        records,
        vec![
            ParentWithOptionalChildren {
                id: 1,
                children: vec!["first".into(), "second".into()],
                optional_children: Some(vec!["first".into(), "second".into()]),
                nullable_children: vec![Some("first".into()), None, Some("second".into())],
                optional_nullable_children: Some(vec![
                    Some("first".into()),
                    None,
                    Some("second".into()),
                ]),
                aliased_optional_children: Some(vec!["first".into(), "second".into()]),
                ordinary_option_accumulator: None,
            },
            ParentWithOptionalChildren {
                id: 2,
                children: Vec::new(),
                optional_children: None,
                nullable_children: vec![None],
                optional_nullable_children: None,
                aliased_optional_children: None,
                ordinary_option_accumulator: None,
            },
        ]
    );
}

#[test]
fn aggregate_fields_return_custom_null_conversion_errors() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE parents (id INTEGER PRIMARY KEY);
         CREATE TABLE children (parent_id INTEGER NOT NULL, value TEXT);
         INSERT INTO parents VALUES (1);",
    )
    .unwrap();

    let error = ParentWithStrictChildren::fetch(&conn).unwrap_err();
    assert!(matches!(
        error,
        rusqlite::Error::FromSqlConversionFailure(_, rusqlite::types::Type::Null, _)
    ));
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
        DefaultRecord::fetch_with_filter(&conn, "id > ?1", rusqlite::params![100_i64])
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

    let error = ExpectedInteger::fetch_with_filter(
        &conn,
        "value = ?1",
        rusqlite::params!["not an integer"],
    )
    .unwrap_err();
    assert!(matches!(error, rusqlite::Error::InvalidColumnType(..)));
}

#[test]
fn parametrized_filters_reuse_the_unit_struct_mapper() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE unit_records (id INTEGER NOT NULL);
         INSERT INTO unit_records VALUES (1), (2);",
    )
    .unwrap();

    assert_eq!(
        UnitRecord::fetch_with_filter(&conn, "id = ?1", rusqlite::params![2_i64]).unwrap(),
        vec![UnitRecord]
    );
}

#[test]
fn sql_errors_are_returned() {
    let conn = records_connection();

    assert!(MissingTable::fetch(&conn).is_err());
    assert!(DefaultRecord::fetch_with_filter(&conn, "not valid SQL", []).is_err());
}
