use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
struct Record {
    #[rusqlite(key, select = "r.id")]
    id: i64,
    value: String,
}

fn main() {}
