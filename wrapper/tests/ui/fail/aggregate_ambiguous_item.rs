use rusqlite_derive::RusqliteFetch;

struct Ambiguous;

impl FromIterator<i64> for Ambiguous {
    fn from_iter<T: IntoIterator<Item = i64>>(_: T) -> Self {
        Self
    }
}

impl FromIterator<String> for Ambiguous {
    fn from_iter<T: IntoIterator<Item = String>>(_: T) -> Self {
        Self
    }
}

#[derive(RusqliteFetch)]
struct Record {
    id: i64,
    #[rusqlite(aggregate)]
    values: Ambiguous,
}

fn main() {}
