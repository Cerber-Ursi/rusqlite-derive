use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key, skip_update)]
    id: i64,
    value: String,
}

fn main() {}
