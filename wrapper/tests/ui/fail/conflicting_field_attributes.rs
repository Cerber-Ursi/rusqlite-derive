use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(select = "value", read_default)]
    value: i64,
}

fn main() {}
