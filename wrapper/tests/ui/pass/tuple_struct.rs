use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "records")]
struct Record(
    #[rusqlite(select = "id")] i64,
    #[rusqlite(select = "name")] String,
);

fn main() {}
