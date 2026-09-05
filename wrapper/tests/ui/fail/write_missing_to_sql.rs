use rusqlite_derive::RusqliteWrite;

struct NotSql;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key)]
    id: i64,
    value: NotSql,
}

fn main() {}
