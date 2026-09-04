use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record<T> {
    value: T,
}

fn main() {}
