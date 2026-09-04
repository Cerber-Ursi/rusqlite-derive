use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(column = "id")]
    id: i64,
}

fn main() {}
