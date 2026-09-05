use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key)]
    id: i64,
    #[rusqlite(skip_write, skip_update)]
    ignored: String,
    value: String,
}

fn main() {}
