# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this library adheres to Rust's notion of
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).


<!-- next-header -->
## [Unreleased]

### Changed
- **IMPORTANT BREAKING CHANGE**: `schemerz_postgres::PostgresAdapter::new` now
  uses a default table name of `_schemerz` when the `table_name` argument is
  `None`. If you were not setting this argument before and are migrating from
  `schemer`, you will need to set `table_name` to `Some("_schemer".into())`.

## [0.190.0] - 2024-10-15
Initial release. The API is identical to `schemer-postgres 0.2.0`.


<!-- next-url -->
[Unreleased]: https://github.com/zcash/schemerz/compare/schemerz-postgres-0.1.0...HEAD
[0.190.0]: https://github.com/zcash/schemerz/compare/1bfd952b035b87a39df955376e0bdddf98eb6c99...schemerz-postgres-0.1.0
