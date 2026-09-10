use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    id: i64,
    #[rusqlite(aggregate(optional, item = String))]
    values: Vec<String>,
}

fn main() {}
