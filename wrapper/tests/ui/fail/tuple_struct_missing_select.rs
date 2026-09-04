use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record(#[rusqlite(select = "id")] i64, String);

fn main() {}
