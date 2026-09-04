use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = 42)]
struct Record {
    id: i64,
}

fn main() {}
