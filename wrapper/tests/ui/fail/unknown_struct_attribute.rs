use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(source = "records")]
struct Record {
    id: i64,
}

fn main() {}
