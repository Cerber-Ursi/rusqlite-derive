use rusqlite_derive::RusqliteFetch;

struct NotFromSql;

#[derive(RusqliteFetch)]
struct Record {
    good: i64,
    bad: NotFromSql,
}

fn main() {}
