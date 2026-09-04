use rusqlite_derive::RusqliteFetch;

struct NotFromSql;

#[derive(RusqliteFetch)]
struct Record<T> {
    value: T,
}

fn require_fetch<T: RusqliteFetch>() {}

fn main() {
    require_fetch::<Record<NotFromSql>>();
}
