use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key, skip_insert)]
    id: i64,
    #[rusqlite(skip_insert)]
    value: String,
}

fn main() {}
