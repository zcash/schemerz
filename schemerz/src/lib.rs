//! A database schema migration library that supports directed acyclic graph
//! (DAG) dependencies between migrations.
//!
//! To use with a specific database, an adapter is required. Known adapter
//! crates:
//!
//! - PostgreSQL: [`schemerz-postgres`](https://crates.io/crates/schemerz-postgres)
//! - SQLite: [`schemerz-rusqlite`](https://crates.io/crates/schemerz-rusqlite)
#![warn(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use daggy::petgraph::EdgeDirection;
use daggy::Dag;
use itertools::Itertools;
use log::{debug, info};
use thiserror::Error;

#[macro_use]
pub mod testing;

/// Metadata for defining the identity and dependence relations of migrations.
/// Specific adapters require additional traits for actual application and
/// reversion of migrations.
pub trait Migration<I> {
    /// Unique identifier for this migration.
    fn id(&self) -> I;

    /// Set of IDs of all direct dependencies of this migration.
    fn dependencies(&self) -> HashSet<I>;

    /// User-targeted description of this migration.
    fn description(&self) -> &'static str;
}

impl<I, T> Migration<I> for Box<T>
where
    T: Migration<I> + ?Sized,
{
    fn id(&self) -> I {
        self.as_ref().id()
    }

    fn dependencies(&self) -> HashSet<I> {
        self.as_ref().dependencies()
    }

    fn description(&self) -> &'static str {
        self.as_ref().description()
    }
}

impl<I, T> Migration<I> for Rc<T>
where
    T: Migration<I> + ?Sized,
{
    fn id(&self) -> I {
        self.as_ref().id()
    }

    fn dependencies(&self) -> HashSet<I> {
        self.as_ref().dependencies()
    }

    fn description(&self) -> &'static str {
        self.as_ref().description()
    }
}

impl<I, T> Migration<I> for Arc<T>
where
    T: Migration<I> + ?Sized,
{
    fn id(&self) -> I {
        self.as_ref().id()
    }

    fn dependencies(&self) -> HashSet<I> {
        self.as_ref().dependencies()
    }

    fn description(&self) -> &'static str {
        self.as_ref().description()
    }
}

/// Create a trivial implementation of `Migration` for a type.
///
/// ## Example
///
/// ```rust
/// #[macro_use]
/// extern crate schemerz;
/// extern crate uuid;
///
/// use schemerz::Migration;
/// use uuid::uuid;
///
/// struct ParentMigration;
/// migration!(
///     ParentMigration,
///     uuid!("bc960dc8-0e4a-4182-a62a-8e776d1e2b30"),
///     [],
///     "Parent migration in a DAG");
///
/// struct ChildMigration;
/// migration!(
///     ChildMigration,
///     uuid!("4885e8ab-dafa-4d76-a565-2dee8b04ef60"),
///     [uuid!("bc960dc8-0e4a-4182-a62a-8e776d1e2b30")],
///     "Child migration in a DAG");
///
/// fn main() {
///     let parent = ParentMigration;
///     let child = ChildMigration;
///
///     assert!(child.dependencies().contains(&parent.id()));
/// }
/// ```
#[macro_export]
macro_rules! migration {
    ($name:ident, $id:expr, [ $( $dependency_id:expr ),*], $description:expr) => {
        migration!(::uuid::Uuid, $name, $id, [$($dependency_id),*], $description);
    };
    ($ty:path, $name:ident, $id:expr, [ $( $dependency_id:expr ),*], $description:expr) => {
        impl $crate::Migration<$ty> for $name
        {
            fn id(&self) -> $ty {
                $id
            }

            fn dependencies(&self) -> ::std::collections::HashSet<$ty> {
                ::std::collections::HashSet::from([
                    $(
                        $dependency_id,
                    )*
                ])
            }

            fn description(&self) -> &'static str {
                $description
            }
        }
    };
}

/// Direction in which a migration is applied (`Up`) or reverted (`Down`).
#[derive(Debug)]
pub enum MigrationDirection {
    Up,
    Down,
}

impl Display for MigrationDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printable = match *self {
            MigrationDirection::Up => "up",
            MigrationDirection::Down => "Down",
        };
        write!(f, "{}", printable)
    }
}

/// Trait necessary to adapt schemerz's migration management to a stateful
/// backend.
pub trait Adapter<I> {
    /// Type migrations must implement for this adapter.
    type MigrationType: Migration<I>;

    /// Type of errors returned by this adapter.
    type Error: std::error::Error + 'static;

    /// Returns the set of IDs for migrations that have been applied.
    fn applied_migrations(&mut self) -> Result<HashSet<I>, Self::Error>;

    /// Apply a single migration.
    fn apply_migration(&mut self, _: &Self::MigrationType) -> Result<(), Self::Error>;

    /// Revert a single migration.
    fn revert_migration(&mut self, _: &Self::MigrationType) -> Result<(), Self::Error>;
}

/// Error resulting from the definition of migration identity and dependency.
#[derive(Debug, Error)]
pub enum DependencyError<I> {
    #[error("Duplicate migration ID {0}")]
    DuplicateId(I),
    #[error("Unknown migration ID {0}")]
    UnknownId(I),
    #[error("Cyclic dependency caused by edge from migration IDs {from} to {to}")]
    Cycle { from: I, to: I },
}

/// Error resulting either from migration definitions or from migration
/// application with an adapter.
#[derive(Debug, Error)]
pub enum MigratorError<I, T: std::error::Error + 'static> {
    #[error("An error occurred due to migration dependencies")]
    Dependency(#[source] DependencyError<I>),
    #[error("An error occurred while interacting with the adapter.")]
    Adapter(#[from] T),
    #[error(
        "An error occurred while applying migration {id} ({description}) {direction}: {error}."
    )]
    Migration {
        id: I,
        description: &'static str,
        direction: MigrationDirection,
        #[source]
        error: T,
    },
}

/// Primary schemerz type for defining and applying migrations.
pub struct Migrator<I, T: Adapter<I>> {
    adapter: T,
    dependencies: Dag<T::MigrationType, ()>,
    id_map: HashMap<I, daggy::NodeIndex>,
}

impl<I, T> Migrator<I, T>
where
    I: Hash + Display + Eq + Clone,
    T: Adapter<I>,
{
    /// Create a `Migrator` using the given `Adapter`.
    pub fn new(adapter: T) -> Migrator<I, T> {
        Migrator {
            adapter,
            dependencies: Dag::new(),
            id_map: HashMap::new(),
        }
    }

    /// Register a migration into the dependency graph.
    pub fn register(
        &mut self,
        migration: T::MigrationType,
    ) -> Result<(), MigratorError<I, T::Error>> {
        let id = migration.id();
        debug!("Registering migration {}", id);
        if self.id_map.contains_key(&id) {
            return Err(MigratorError::Dependency(DependencyError::DuplicateId(id)));
        }

        let migration_idx = self.dependencies.add_node(migration);
        self.id_map.insert(id, migration_idx);

        Ok(())
    }

    /// Register multiple migrations into the dependency graph.
    pub fn register_multiple(
        &mut self,
        migrations: impl Iterator<Item = T::MigrationType>,
    ) -> Result<(), MigratorError<I, T::Error>> {
        for migration in migrations {
            let id = migration.id();
            debug!("Registering migration (with multiple) {}", id);
            if self.id_map.contains_key(&id) {
                return Err(MigratorError::Dependency(DependencyError::DuplicateId(id)));
            }

            let migration_idx = self.dependencies.add_node(migration);
            self.id_map.insert(id, migration_idx);
        }

        Ok(())
    }

    /// Creates the edges for the current migrations into the dependency graph.
    fn register_edges(&mut self) -> Result<(), MigratorError<I, T::Error>> {
        for (id, migration_idx) in self.id_map.iter() {
            let depends = self
                .dependencies
                .node_weight(*migration_idx)
                .expect("We registered these nodes")
                .dependencies();

            for d in depends {
                let parent_idx = self.id_map.get(&d).ok_or(MigratorError::Dependency(
                    DependencyError::UnknownId(d.clone()),
                ))?;
                self.dependencies
                    .add_edge(*parent_idx, *migration_idx, ())
                    .map_err(|_| {
                        MigratorError::Dependency(DependencyError::Cycle {
                            from: d,
                            to: id.clone(),
                        })
                    })?;
            }
        }
        Ok(())
    }

    /// Collect the ids of recursively dependent migrations in `dir` induced
    /// starting from `id`. If `dir` is `Incoming`, this is all ancestors
    /// (dependencies); if `Outgoing`, this is all descendents (dependents).
    /// If `id` is `None`, this is all migrations starting from the sources or
    /// the sinks, respectively.
    fn induced_stream(
        &self,
        id: Option<I>,
        dir: EdgeDirection,
    ) -> Result<Vec<daggy::NodeIndex>, DependencyError<I>> {
        let mut target_ids = Vec::new();
        match id {
            Some(id) => {
                if let Some(id) = self.id_map.get(&id) {
                    target_ids.push(*id);
                } else {
                    return Err(DependencyError::UnknownId(id));
                }
            }
            // This will eventually yield all migrations, so could be optimized.
            None => target_ids.extend(self.dependencies.graph().externals(dir.opposite())),
        }

        let mut to_visit: VecDeque<_> = target_ids.iter().cloned().collect();
        while let Some(idx) = to_visit.pop_front() {
            target_ids.push(idx);
            to_visit.extend(self.dependencies.graph().neighbors_directed(idx, dir));
        }

        Ok(target_ids.into_iter().unique().rev().collect())
    }

    /// Apply migrations as necessary to so that the specified migration is
    /// applied (inclusive).
    ///
    /// If `to` is `None`, apply all registered migrations.
    pub fn up(&mut self, to: Option<I>) -> Result<(), MigratorError<I, T::Error>> {
        if let Some(to) = &to {
            info!("Migrating up to target: {}", to);
        } else {
            info!("Migrating everything");
        }

        // Register the edges
        self.register_edges()?;

        let target_idxs = self
            .induced_stream(to, EdgeDirection::Incoming)
            .map_err(MigratorError::Dependency)?;

        // TODO: This is assuming the applied_migrations state is consistent
        // with the dependency graph.
        let applied_migrations = self.adapter.applied_migrations()?;
        for idx in target_idxs.into_iter() {
            let migration = &self.dependencies[idx];
            let id = migration.id();
            if applied_migrations.contains(&id) {
                continue;
            }

            info!("Applying migration {}", id);
            self.adapter
                .apply_migration(migration)
                .map_err(|e| MigratorError::Migration {
                    id,
                    description: migration.description(),
                    direction: MigrationDirection::Up,
                    error: e,
                })?;
        }

        Ok(())
    }

    /// Revert migrations as necessary so that no migrations dependent on the
    /// specified migration are applied. If the specified migration was already
    /// applied, it will still be applied.
    ///
    /// If `to` is `None`, revert all applied migrations.
    pub fn down(&mut self, to: Option<I>) -> Result<(), MigratorError<I, T::Error>> {
        if let Some(to) = &to {
            info!("Migrating up to target: {}", to);
        } else {
            info!("Migrating everything");
        }

        // Register the edges
        self.register_edges()?;

        let mut target_idxs = self
            .induced_stream(to.clone(), EdgeDirection::Outgoing)
            .map_err(MigratorError::Dependency)?;
        if let Some(sink_id) = to {
            target_idxs = target_idxs
                .into_iter()
                .filter(|idx| {
                    self.id_map
                        .get(&sink_id)
                        .expect("Id is checked in induced_stream and exists")
                        != idx
                })
                .collect();
        }

        let applied_migrations = self.adapter.applied_migrations()?;
        for idx in target_idxs.into_iter() {
            let migration = &self.dependencies[idx];
            let id = migration.id();
            if !applied_migrations.contains(&id) {
                continue;
            }

            info!("Reverting migration {}", id);
            self.adapter
                .revert_migration(migration)
                .map_err(|e| MigratorError::Migration {
                    id,
                    description: migration.description(),
                    direction: MigrationDirection::Down,
                    error: e,
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use uuid::Uuid;

    use super::testing::*;
    use super::*;

    struct DefaultTestAdapter {
        applied_migrations: HashSet<Uuid>,
    }

    impl DefaultTestAdapter {
        fn new() -> DefaultTestAdapter {
            DefaultTestAdapter {
                applied_migrations: HashSet::new(),
            }
        }
    }

    #[derive(Debug, Error)]
    #[error("An error occurred.")]
    struct DefaultTestAdapterError;

    impl Adapter<Uuid> for DefaultTestAdapter {
        type MigrationType = TestMigration<Uuid>;

        type Error = DefaultTestAdapterError;

        fn applied_migrations(&mut self) -> Result<HashSet<Uuid>, Self::Error> {
            Ok(self.applied_migrations.clone())
        }

        fn apply_migration(&mut self, migration: &Self::MigrationType) -> Result<(), Self::Error> {
            self.applied_migrations.insert(migration.id());
            Ok(())
        }

        fn revert_migration(&mut self, migration: &Self::MigrationType) -> Result<(), Self::Error> {
            self.applied_migrations.remove(&migration.id());
            Ok(())
        }
    }

    impl TestAdapter<Uuid> for DefaultTestAdapter {
        fn mock(id: Uuid, dependencies: HashSet<Uuid>) -> Self::MigrationType {
            TestMigration::new(id, dependencies)
        }
    }

    test_schemerz_adapter!(DefaultTestAdapter::new());
}
