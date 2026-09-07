extern crate rusqlite_derive as rd;

use rd::{RusqliteFetch, RusqliteWrite};

#[derive(RusqliteFetch, RusqliteWrite)]
#[rusqlite(crate = "rd", table = "records")]
struct Record {
    #[rusqlite(key)]
    id: i64,
    value: String,
}

fn assert_traits<T: rd::RusqliteFetch + rd::RusqliteWrite>() {}

fn main() {
    assert_traits::<Record>();
}
