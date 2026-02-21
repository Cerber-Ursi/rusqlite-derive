pub use rusqlite_derive_impl::RusqliteFetch;

pub trait RusqliteFetch: Sized {
    fn fetch(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Self>>;
    fn fetch_with_filter(conn: &rusqlite::Connection, filter: &str) -> rusqlite::Result<Vec<Self>>;
}
