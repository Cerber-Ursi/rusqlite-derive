use rusqlite_derive::RusqliteWrite;

#[derive(RusqliteWrite)]
#[rusqlite(table = "pairs")]
struct Pair(
    #[rusqlite(column = "left_id", key)] i64,
    #[rusqlite(column = "right_id", key)] i64,
    #[rusqlite(column = "value")] String,
);

fn accepts_record<T: rusqlite_derive::RusqliteWrite>() {}

fn main() {
    accepts_record::<Pair>();
}
