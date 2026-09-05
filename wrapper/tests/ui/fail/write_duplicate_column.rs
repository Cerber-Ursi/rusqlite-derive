use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key, column = "same")]
    id: i64,
    #[rusqlite(column = "same")]
    value: String,
}

fn main() {}
