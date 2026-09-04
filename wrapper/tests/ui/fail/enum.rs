use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
enum Record {
    First,
    Second,
}

fn main() {}
