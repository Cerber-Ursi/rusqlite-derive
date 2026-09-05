use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(rename = "id")]
    id: i64,
}

fn main() {}
