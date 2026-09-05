use rusqlite_derive::{RusqliteFetch, RusqliteWrite};

#[derive(RusqliteFetch, RusqliteWrite)]
#[rusqlite(table = "records", from = "records AS r")]
struct Record<T>
where
    T: Clone,
{
    #[rusqlite(key, column = "id", select = "r.id")]
    id: T,
    #[rusqlite(column = "value", select = "r.value")]
    value: String,
    #[rusqlite(select = "upper(r.value)", skip_write)]
    display: String,
}

fn accepts_record<T: rusqlite_derive::RusqliteWrite>() {}

fn main() {
    accepts_record::<Record<i64>>();
}
