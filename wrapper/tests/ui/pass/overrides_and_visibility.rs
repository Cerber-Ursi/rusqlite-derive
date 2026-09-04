use rusqlite_derive::RusqliteFetch;

#[derive(RusqliteFetch)]
#[rusqlite(from = "records AS r")]
pub struct PublicRecord {
    #[rusqlite(select = "r.id")]
    pub id: i64,
    #[rusqlite(select = "r.name")]
    pub(crate) name: String,
    pub optional_value: Option<i64>,
}

fn main() {}
