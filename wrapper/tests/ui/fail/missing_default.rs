use rusqlite_derive::RusqliteFetch;

struct NotDefault;

#[derive(RusqliteFetch)]
struct Record {
    value: i64,
    #[rusqlite(read_default)]
    skipped: NotDefault,
}

fn main() {}
