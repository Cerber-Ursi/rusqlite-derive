use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(table = "fetch_only")]
struct FetchOnly {
    #[rusqlite(key, skip_insert)]
    id: i64,
    #[rusqlite(skip_update)]
    value: String,
}

fn main() {}
