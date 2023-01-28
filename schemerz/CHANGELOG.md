# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this library adheres to Rust's notion of
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).


<!-- next-header -->
## [Unreleased]

### Fixed
- `schemerz::Migrator::{register, register_multiple}` can now register dependent
  migrations before their dependencies. Previously this would result in a graph
  with missing edges, leading to some migrations not being applied.
- `schemerz::Migrator::{up, down}` now reliably run migrations in the correct
  order.

### Added
- `schemerz::test_schemerz_adapter`
- Blanket implementation of `schemerz::Migration` for the following types:
  - `Box<T>`
  - `Rc<T>`
  - `Arc<T>`

### Changed
- `schemerz::Migrator` no longer uses `Box` in its API:
  - `schemerz::Adapter::MigrationType` must now be `Sized`.
  - `schemerz::Migrator::{register, register_multiple}` now take the migrations
    without a `Box` wrapper`.
  - `schemerz::TestAdapter::mock` now returns the migration without a `Box`
    wrapper.
- `schemerz::Migration` is now generic over its index type, to make writing
  tests easier (as they can now use an alternative index type like `usize`).
  Production migrations should still use `uuid::Uuid` for resiliency.
  - The following traits and structs now have a generic parameter `I`:
    - `Migration`
    - `Adapter`
    - `testing::TestAdapter`
    - `testing::TestMigration`
  - The `Migrator` struct now has an `I: Clone + Display + Hash + Eq` parameter.
  - The following methods now take `I` as an argument instead of `uuid::Uuid`:
    - `Migrator::up`
    - `Migrator::down`
  - The return types of the following methods now involve `I` instead of
    `uuid::Uuid`:
    - `Migration::id`
    - `Migration::dependencies`
    - `Adapter::applied_migrations`
    - `testing::TestAdapter::mock`
    - `testing::TestMigration::new`
  - The `schemerz::{DependencyError, MigratorError>` enums now have a generic
    parameter `I` that replaces uses of `uuid::Uuid`.
  - The `schemerz::migration` macro now supports an optional initial argument
    with the index type (which defaults to `uuid::Uuid`).
  - The individual tests in the `schemerz::testing` module now require an
    adapter with an `I: Clone + FromStr + Debug + Display + Hash + Eq` bound.
- `schemerz::Migrator::register_multiple` now takes an iterator of migrations
  instead of a `Vec`.

### Removed
- `schemerz::test_schemer_adapter` (use `test_schemerz_adapter` instead).

## [0.1.0] - 2024-10-15
Initial release. The API is identical to `schemer 0.2.1`.


<!-- next-url -->
[Unreleased]: https://github.com/zcash/schemerz/compare/schemerz-0.1.0...HEAD
[0.1.0]: https://github.com/zcash/schemerz/compare/1bfd952b035b87a39df955376e0bdddf98eb6c99...schemerz-0.1.0
