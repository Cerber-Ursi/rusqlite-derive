use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(optional)]
    values: Option<Vec<String>>,
}

fn main() {}
