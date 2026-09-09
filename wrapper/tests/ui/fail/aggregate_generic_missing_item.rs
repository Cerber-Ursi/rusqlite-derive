use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
struct Record<C> {
    id: i64,
    #[rusqlite(aggregate)]
    values: C,
}

fn main() {}
