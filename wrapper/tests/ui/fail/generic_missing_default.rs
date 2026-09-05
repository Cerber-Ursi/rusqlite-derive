use rusqlite_derive::RusqliteFetch;

struct NotDefault;

#[derive(RusqliteFetch)]
struct Record<T> {
    value: i64,
    #[rusqlite(read_default)]
    skipped: T,
}

fn require_fetch<T: RusqliteFetch>() {}

fn main() {
    require_fetch::<Record<NotDefault>>();
}
