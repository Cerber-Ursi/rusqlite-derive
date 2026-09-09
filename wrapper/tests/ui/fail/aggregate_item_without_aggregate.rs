use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(item = String)]
    value: Vec<String>,
}

fn main() {}
