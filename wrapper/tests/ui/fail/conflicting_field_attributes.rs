use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(select = "value", default)]
    value: i64,
}

fn main() {}
