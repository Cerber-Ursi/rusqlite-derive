use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    id: i64,
    name: String,
    active: bool,
}

fn main() {}
