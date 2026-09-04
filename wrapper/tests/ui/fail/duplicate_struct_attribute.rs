use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "first", from = "second")]
struct Record {
    id: i64,
}

fn main() {}
