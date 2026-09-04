use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record(i64, String);

fn main() {}
