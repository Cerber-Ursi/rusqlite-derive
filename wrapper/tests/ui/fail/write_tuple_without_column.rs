use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record(#[rusqlite(column = "id", key)] i64, String);

fn main() {}
