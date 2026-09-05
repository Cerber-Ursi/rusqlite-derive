use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "records")]
union Record {
    value: i64,
}

fn main() {}
