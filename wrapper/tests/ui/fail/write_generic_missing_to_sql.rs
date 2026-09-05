use rusqlite_derive::RusqliteWrite;

struct NotSql;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record<T> {
    #[rusqlite(key)]
    id: i64,
    value: T,
}

fn main() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let record = Record { id: 1, value: NotSql };
    let _ = record.insert(&conn);
}
