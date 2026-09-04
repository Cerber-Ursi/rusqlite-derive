use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record {
    #[rusqlite(select = "first", select = "second")]
    id: i64,
}

fn main() {}
