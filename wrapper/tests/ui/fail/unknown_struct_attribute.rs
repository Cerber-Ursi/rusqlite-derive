use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(table = "records")]
struct Record {
    id: i64,
}

fn main() {}
