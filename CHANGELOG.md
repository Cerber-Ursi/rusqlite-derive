# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-09-07

### Added

- `RusqliteFetch` for reading named, tuple, generic, defaulted, and fieldless structs from tables, joins, subqueries, and custom SQL expressions.
- Parameterized filtering with positional or named rusqlite parameters.
- `RusqliteWrite` for complete-record inserts, key-based updates, and SQLite upserts, including composite and database-generated keys.
- Independent read and write mappings through `table`, `from`, `column`, `select`, `read_default`, `key`, and write-skip attributes.
- An explicit `#[rusqlite(crate = "...")]` override for renamed wrapper dependencies.
- A `rusqlite` re-export and an opt-in `bundled` feature.
- Compile-time diagnostics and runtime coverage for supported mappings and invalid attribute combinations.
- Explicit declaration of Rust 1.85 as the minimum supported Rust version.

[1.0.0]: https://github.com/Cerber-Ursi/rusqlite-derive/releases/tag/v1.0.0
