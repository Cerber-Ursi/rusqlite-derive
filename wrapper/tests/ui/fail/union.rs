use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
union Record {
    integer: i64,
    float: f64,
}

fn main() {}
