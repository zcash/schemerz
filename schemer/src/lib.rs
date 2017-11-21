extern crate daggy;
extern crate uuid;


use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;

use daggy::Dag;
use daggy::petgraph::EdgeDirection;
use uuid::Uuid;


pub trait Migration {
    fn id(&self) -> Uuid;

    fn dependencies(&self) -> HashSet<Uuid>;

    fn description(&self) -> &'static str;
}

pub trait Adapter {
    type MigrationType: Migration + ?Sized;

    type Error: Debug; // TODO: How does this work with error_chain?

    fn applied_migrations(&self) -> Result<HashSet<Uuid>, Self::Error>;

    fn apply_migration(&mut self, &Self::MigrationType) -> Result<(), Self::Error>;

    fn revert_migration(&mut self, &Self::MigrationType) -> Result<(), Self::Error>;
}

pub struct Migrator<T: Adapter> {
    adapter: T,
    dependencies: Dag<Box<T::MigrationType>, ()>,
    id_map: HashMap<Uuid, daggy::NodeIndex>,
}

impl<T: Adapter> Migrator<T> {
    fn new(adapter: T) -> Migrator<T> {
        Migrator {
            adapter: adapter,
            dependencies: Dag::new(),
            id_map: HashMap::new(),
        }
    }

    pub fn register(&mut self, migration: Box<T::MigrationType>) -> Result<(), T::Error> {
        // TODO: check that this Id doesn't already exist in the graph.
        let id = migration.id();
        let depends = migration.dependencies();
        let migration_idx = self.dependencies.add_node(migration);

        for d in depends {
            let parent_idx = self.id_map.get(&d).expect("TODO");
            self.dependencies.add_edge(*parent_idx, migration_idx, ());
        }

        self.id_map.insert(id, migration_idx);

        Ok(())
    }

    /// Collect the ids of recursively dependent migrations in `dir` induced
    /// starting from `id`. If `dir` is `Incoming`, this is all ancestors
    /// (dependencies); if `Outgoing`, this is all descendents (dependents).
    /// If `id` is `None`, this is all migrations starting from the sources or
    /// the sinks, respectively.
    fn induced_stream(&self, id: Option<Uuid>, dir: EdgeDirection) -> HashSet<Uuid> {
        let mut target_ids = HashSet::new();
        match id {
            Some(id) => {
                target_ids.insert(id);
            }
            // This will eventually yield all migrations, so could be optimized.
            None => {
                target_ids.extend(
                    self.dependencies
                        .graph()
                        .externals(dir.opposite())
                        .map(|idx| {
                            self.dependencies.node_weight(idx).expect("Impossible").id()
                        }),
                )
            }
        }

        let mut to_visit: VecDeque<_> = target_ids
            .clone()
            .iter()
            .map(|id| *self.id_map.get(id).expect("ID map is malformed"))
            .collect();
        while !to_visit.is_empty() {
            let idx = to_visit.pop_front().expect("Not empty");
            let id = self.dependencies.node_weight(idx).expect("Impossible").id();
            target_ids.insert(id);
            to_visit.extend(
                self.dependencies
                    .graph()
                    .neighbors_directed(idx, dir),
            );
        }

        target_ids
    }

    pub fn up(&mut self, to: Option<Uuid>) -> Result<(), T::Error> {
        let target_ids = self.induced_stream(to, EdgeDirection::Incoming);

        // TODO: This is assuming the applied_migrations state is consistent
        // with the dependency graph.
        let applied_migrations = self.adapter.applied_migrations()?;
        for idx in &daggy::petgraph::algo::toposort(self.dependencies.graph(), None)
            .expect("Impossible because dependencies are a DAG")
        {
            let migration = self.dependencies.node_weight(*idx).expect("Impossible");
            let id = migration.id();
            if applied_migrations.contains(&id) || !target_ids.contains(&id) {
                continue;
            }

            self.adapter.apply_migration(migration)?;
        }

        Ok(())
    }

    pub fn down(&mut self, to: Option<Uuid>) -> Result<(), T::Error> {
        let mut target_ids = self.induced_stream(to, EdgeDirection::Outgoing);
        if let Some(sink_id) = to {
            target_ids.remove(&sink_id);
        }

        let applied_migrations = self.adapter.applied_migrations()?;
        for idx in daggy::petgraph::algo::toposort(self.dependencies.graph(), None)
            .expect("Impossible because dependencies are a DAG")
            .iter()
            .rev()
        {
            let migration = self.dependencies.node_weight(*idx).expect("Impossible");
            let id = migration.id();
            if !applied_migrations.contains(&id) || !target_ids.contains(&id) {
                continue;
            }

            self.adapter.revert_migration(migration)?;
        }

        Ok(())
    }
}

#[macro_use]
pub mod testing {
    use super::*;

    pub trait TestAdapter: Adapter {
        fn mock(id: Uuid, dependencies: HashSet<Uuid>) -> Box<Self::MigrationType>;
    }

    pub struct TestMigration {
        id: Uuid,
        dependencies: HashSet<Uuid>,
    }

    impl TestMigration {
        pub fn new(id: Uuid, dependencies: HashSet<Uuid>) -> TestMigration {
            TestMigration { id, dependencies }
        }
    }

    impl Migration for TestMigration {
        fn id(&self) -> Uuid {
            self.id
        }

        fn dependencies(&self) -> HashSet<Uuid> {
            self.dependencies.clone()
        }

        fn description(&self) -> &'static str {
            "Test Migration"
        }
    }

    #[macro_export]
    macro_rules! test_schemer_adapter {
        ($constructor:expr) => {
            #[test]
            fn test_single_migration() {
                let adapter = $constructor;
                $crate::testing::test_single_migration(adapter);
            }

            #[test]
            fn test_migration_chain() {
                let adapter = $constructor;
                $crate::testing::test_migration_chain(adapter);
            }
        }
    }

    pub fn test_single_migration<A: TestAdapter>(adapter: A) {
        let migration1 = A::mock(
            Uuid::parse_str("bc960dc8-0e4a-4182-a62a-8e776d1e2b30").unwrap(),
            HashSet::new(),
        );
        let uuid1 = migration1.id();

        let mut migrator: Migrator<A> = Migrator::new(adapter);

        migrator.register(migration1).expect("Migration 1 registration failed");
        migrator.up(None).expect("Up migration failed");

        assert!(migrator.adapter.applied_migrations().unwrap().contains(
            &uuid1,
        ));

        migrator.down(None).expect("Down migration failed");

        assert!(!migrator.adapter.applied_migrations().unwrap().contains(
            &uuid1,
        ));
    }

    pub fn test_migration_chain<A: TestAdapter>(adapter: A) {
        let migration1 = A::mock(
            Uuid::parse_str("bc960dc8-0e4a-4182-a62a-8e776d1e2b30").unwrap(),
            HashSet::new(),
        );
        let migration2 = A::mock(
            Uuid::parse_str("4885e8ab-dafa-4d76-a565-2dee8b04ef60").unwrap(),
            vec![migration1.id()].into_iter().collect(),
        );
        let migration3 = A::mock(
            Uuid::parse_str("c5d07448-851f-45e8-8fa7-4823d5250609").unwrap(),
            vec![migration2.id()].into_iter().collect(),
        );

        let uuid1 = migration1.id();
        let uuid2 = migration2.id();
        let uuid3 = migration3.id();

        let mut migrator = Migrator::new(adapter);

        migrator.register(migration1).expect("Migration 1 registration failed");
        migrator.register(migration2).expect("Migration 2 registration failed");
        migrator.register(migration3).expect("Migration 3 registration failed");

        migrator.up(Some(uuid2)).expect("Up migration failed");

        {
            let applied = migrator.adapter.applied_migrations().unwrap();
            assert!(applied.contains(&uuid1));
            assert!(applied.contains(&uuid2));
            assert!(!applied.contains(&uuid3));
        }

        migrator.down(Some(uuid1)).expect("Down migration failed");

        {
            let applied = migrator.adapter.applied_migrations().unwrap();
            assert!(applied.contains(&uuid1));
            assert!(!applied.contains(&uuid2));
            assert!(!applied.contains(&uuid3));
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use super::testing::*;

    struct DefaultTestAdapter {
        applied_migrations: HashSet<Uuid>,
    }

    impl DefaultTestAdapter {
        fn new() -> DefaultTestAdapter {
            DefaultTestAdapter { applied_migrations: HashSet::new() }
        }
    }

    #[derive(Debug)]
    struct DefaultTestAdapterError;

    impl Adapter for DefaultTestAdapter {
        type MigrationType = Migration;

        type Error = DefaultTestAdapterError;

        fn applied_migrations(&self) -> Result<HashSet<Uuid>, Self::Error> {
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

    impl TestAdapter for DefaultTestAdapter {
        fn mock(id: Uuid, dependencies: HashSet<Uuid>) -> Box<Self::MigrationType> {
            Box::new(TestMigration::new(id, dependencies))
        }
    }

    test_schemer_adapter!(DefaultTestAdapter::new());
}
