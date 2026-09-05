use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
struct Record {
    #[rusqlite(key)]
    id: i64,
    value: String,
}

fn main() {}
