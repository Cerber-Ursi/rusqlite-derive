use rusqlite_derive::RusqliteFetch;
use std::marker::PhantomData;

#[derive(RusqliteFetch)]
struct Record<'a, T, const N: usize>
where
    T: Clone,
{
    value: T,
    #[rusqlite(default)]
    marker: PhantomData<(&'a (), [(); N])>,
}

fn assert_fetch<T: rusqlite_derive::RusqliteFetch>() {}

fn main() {
    assert_fetch::<Record<'static, i64, 4>>();
}
