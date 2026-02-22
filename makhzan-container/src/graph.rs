//! Dependency graph validation.
//!
//! Validates the dependency graph at build time:
//! - Detects circular dependencies
//! - Checks that all dependencies are registered
//! - Validates scope compatibility
//!
//! All validation happens during [`ContainerBuilder::build()`],
//! BEFORE the first `resolve()` call.

use std::collections::{HashMap, HashSet};

use tracing::{debug, warn, instrument};

use crate::error::{
    CircularDependencyError, MakhzanError, NotRegisteredError,
    ScopeMismatchError,
};
use crate::key::DependencyKey;
use crate::scope::Scope;

/// Information about a registered dependency needed for validation.
#[derive(Debug, Clone)]
pub(crate) struct DependencyInfo {
    /// What this factory produces
    pub key: DependencyKey,
    /// What this factory needs (its dependencies)
    pub dependencies: Vec<DependencyKey>,
    /// Scope of this factory
    pub scope: Scope,
}

/// Validates the dependency graph for correctness.
///
/// Checks performed:
/// 1. **Completeness**: Every dependency is registered
/// 2. **Acyclicity**: No circular dependencies
/// 3. **Scope compatibility**: No short-lived deps in long-lived consumers
///
/// # Algorithm
/// Uses Depth-First Search (DFS) to traverse the graph.
/// Maintains a "path" set to detect cycles.
pub(crate) struct GraphValidator {
    /// All registered dependencies
    dependencies: HashMap<DependencyKey, DependencyInfo>,
    /// Currently being visited (for cycle detection)
    visiting: HashSet<DependencyKey>,
    /// Already validated (cache)
    validated: HashSet<DependencyKey>,
    /// Current DFS path (for error reporting)
    path: Vec<DependencyKey>,
}

impl GraphValidator {
    /// Creates a new validator with the given dependency registrations.
    pub fn new(dependencies: HashMap<DependencyKey, DependencyInfo>) -> Self {
        Self {
            dependencies,
            visiting: HashSet::new(),
            validated: HashSet::new(),
            path: Vec::new(),
        }
    }

    /// Validates the entire dependency graph.
    ///
    /// Returns `Ok(())` if the graph is valid, or an error describing
    /// what went wrong.
    ///
    /// # Errors
    /// - [`MakhzanError::CircularDependency`] — cycle detected
    /// - [`MakhzanError::NotRegistered`] — missing dependency
    /// - [`MakhzanError::ScopeMismatch`] — scope incompatibility
    #[instrument(skip(self), name = "graph_validation")]
    pub fn validate(&mut self) -> Result<(), MakhzanError> {
        let keys: Vec<DependencyKey> = self.dependencies.keys().cloned().collect();

        debug!(
            dependency_count = keys.len(),
            "Starting dependency graph validation"
        );

        for key in keys {
            if !self.validated.contains(&key) {
                self.validate_key(&key)?;
            }
        }

        debug!("Dependency graph validation passed ✓");
        Ok(())
    }

    /// Validates a single dependency key (recursive DFS).
    fn validate_key(&mut self, key: &DependencyKey) -> Result<(), MakhzanError> {
        // Already validated — skip
        if self.validated.contains(key) {
            return Ok(());
        }

        // Currently visiting — CYCLE DETECTED!
        if self.visiting.contains(key) {
            let cycle_start = self.path
                .iter()
                .position(|k| k == key)
                .unwrap_or(0);

            let mut chain: Vec<DependencyKey> = self.path[cycle_start..].to_vec();
            chain.push(key.clone());

            warn!(
                cycle = ?chain,
                "Circular dependency detected!"
            );

            return Err(MakhzanError::CircularDependency(
                CircularDependencyError { chain },
            ));
        }

        // Check if the dependency is registered
        let info = self.dependencies.get(key).cloned().ok_or_else(|| {
            let suggestions = self.find_similar_keys(key);

            MakhzanError::NotRegistered(NotRegisteredError {
                requested: key.clone(),
                required_by: self.path.last().cloned(),
                suggestions,
            })
        })?;

        // Mark as "currently visiting" and add to path
        self.visiting.insert(key.clone());
        self.path.push(key.clone());

        // Recursively validate all dependencies
        for dep_key in &info.dependencies {
            // Check scope compatibility BEFORE recursing
            if let Some(dep_info) = self.dependencies.get(dep_key) {
                self.check_scope_compatibility(&info, dep_info)?;
            }

            self.validate_key(dep_key)?;
        }

        // Done visiting — remove from path, mark as validated
        self.path.pop();
        self.visiting.remove(key);
        self.validated.insert(key.clone());

        Ok(())
    }

    /// Checks that scope rules are not violated.
    ///
    /// Rule: A dependency cannot have a SHORTER lifetime than its consumer.
    /// - Singleton CANNOT depend on Scoped or Transient
    /// - Scoped CANNOT depend on Transient
    /// - Transient CAN depend on anything
    fn check_scope_compatibility(
        &self,
        consumer: &DependencyInfo,
        dependency: &DependencyInfo,
    ) -> Result<(), MakhzanError> {
        // If consumer lives LONGER than dependency — problem!
        // Singleton > Scoped > Transient
        if consumer.scope > dependency.scope {
            warn!(
                consumer = %consumer.key,
                consumer_scope = %consumer.scope,
                dependency = %dependency.key,
                dependency_scope = %dependency.scope,
                "Scope mismatch detected"
            );

            return Err(MakhzanError::ScopeMismatch(ScopeMismatchError {
                consumer: consumer.key.clone(),
                consumer_scope: consumer.scope,
                dependency: dependency.key.clone(),
                dependency_scope: dependency.scope,
            }));
        }

        Ok(())
    }

    /// Finds registered keys with similar type names (for "did you mean?" suggestions).
    fn find_similar_keys(&self, target: &DependencyKey) -> Vec<DependencyKey> {
        let target_name = target.type_name().to_lowercase();

        self.dependencies
            .keys()
            .filter(|k| {
                let name = k.type_name().to_lowercase();
                // Simple substring matching for suggestions
                name.contains(&target_name)
                    || target_name.contains(&name)
                    || levenshtein_close(&target_name, &name)
            })
            .cloned()
            .collect()
    }
}

/// Simple check if two strings are "close enough" (edit distance ≤ 3).
///
/// Not a full Levenshtein — just a quick heuristic for suggestions.
fn levenshtein_close(a: &str, b: &str) -> bool {
    let len_diff = a.len().abs_diff(b.len());
    if len_diff > 3 {
        return false;
    }

    let common: usize = a
        .chars()
        .zip(b.chars())
        .filter(|(ca, cb)| ca == cb)
        .count();

    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return true;
    }

    // At least 60% of characters match
    common * 100 / max_len >= 60
}

#[cfg(test)]
mod tests {
    use super::*;

    // Хелпер для создания DependencyInfo
    fn dep_info(
        key: DependencyKey,
        scope: Scope,
        deps: Vec<DependencyKey>,
    ) -> DependencyInfo {
        DependencyInfo {
            key,
            dependencies: deps,
            scope,
        }
    }

    fn make_graph(
        infos: Vec<DependencyInfo>,
    ) -> HashMap<DependencyKey, DependencyInfo> {
        infos.into_iter().map(|i| (i.key.clone(), i)).collect()
    }

    // === Типы для тестов ===
    struct Database;
    struct UserRepo;
    struct UserService;
    struct Logger;

    #[test]
    fn valid_simple_graph() {
        let graph = make_graph(vec![
            dep_info(
                DependencyKey::of::<Database>(),
                Scope::Singleton,
                vec![],
            ),
            dep_info(
                DependencyKey::of::<UserRepo>(),
                Scope::Singleton,
                vec![DependencyKey::of::<Database>()],
            ),
            dep_info(
                DependencyKey::of::<UserService>(),
                Scope::Scoped,
                vec![DependencyKey::of::<UserRepo>()],
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn detect_circular_dependency() {
        // A → B → C → A  (cycle!)
        struct A;
        struct B;
        struct C;

        let graph = make_graph(vec![
            dep_info(
                DependencyKey::of::<A>(),
                Scope::Transient,
                vec![DependencyKey::of::<B>()],
            ),
            dep_info(
                DependencyKey::of::<B>(),
                Scope::Transient,
                vec![DependencyKey::of::<C>()],
            ),
            dep_info(
                DependencyKey::of::<C>(),
                Scope::Transient,
                vec![DependencyKey::of::<A>()],  // CYCLE!
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        let result = validator.validate();

        assert!(result.is_err());
        match result.unwrap_err() {
            MakhzanError::CircularDependency(err) => {
                assert!(err.chain.len() >= 3);
            }
            other => panic!("Expected CircularDependency, got: {other:?}"),
        }
    }

    #[test]
    fn detect_self_dependency() {
        // A → A (self-cycle)
        struct A;

        let graph = make_graph(vec![dep_info(
            DependencyKey::of::<A>(),
            Scope::Transient,
            vec![DependencyKey::of::<A>()],  // depends on itself!
        )]);

        let mut validator = GraphValidator::new(graph);
        assert!(validator.validate().is_err());
    }

    #[test]
    fn detect_missing_dependency() {
        // A → B, but B is NOT registered
        struct A;
        struct B;

        let graph = make_graph(vec![dep_info(
            DependencyKey::of::<A>(),
            Scope::Transient,
            vec![DependencyKey::of::<B>()],  // B not registered!
        )]);

        let mut validator = GraphValidator::new(graph);
        let result = validator.validate();

        assert!(result.is_err());
        match result.unwrap_err() {
            MakhzanError::NotRegistered(err) => {
                assert!(err.requested.type_name().contains("B"));
                assert!(err.required_by.is_some());
            }
            other => panic!("Expected NotRegistered, got: {other:?}"),
        }
    }

    #[test]
    fn detect_scope_mismatch() {
        // Singleton → Transient (BAD!)
        // Singleton lives forever, Transient dies quickly
        let graph = make_graph(vec![
            dep_info(
                DependencyKey::of::<Database>(),
                Scope::Transient,  // short-lived
                vec![],
            ),
            dep_info(
                DependencyKey::of::<UserService>(),
                Scope::Singleton,  // long-lived, depends on short-lived!
                vec![DependencyKey::of::<Database>()],
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        let result = validator.validate();

        assert!(result.is_err());
        match result.unwrap_err() {
            MakhzanError::ScopeMismatch(err) => {
                assert_eq!(err.consumer_scope, Scope::Singleton);
                assert_eq!(err.dependency_scope, Scope::Transient);
            }
            other => panic!("Expected ScopeMismatch, got: {other:?}"),
        }
    }

    #[test]
    fn singleton_depends_on_singleton_ok() {
        let graph = make_graph(vec![
            dep_info(
                DependencyKey::of::<Database>(),
                Scope::Singleton,
                vec![],
            ),
            dep_info(
                DependencyKey::of::<UserService>(),
                Scope::Singleton,
                vec![DependencyKey::of::<Database>()],
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn transient_depends_on_singleton_ok() {
        // Transient → Singleton is FINE
        // Short-lived can use long-lived
        let graph = make_graph(vec![
            dep_info(
                DependencyKey::of::<Database>(),
                Scope::Singleton,
                vec![],
            ),
            dep_info(
                DependencyKey::of::<UserService>(),
                Scope::Transient,
                vec![DependencyKey::of::<Database>()],
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn diamond_dependency_ok() {
        // Diamond shape — NOT a cycle
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        struct A;
        struct B;
        struct C;
        struct D;

        let graph = make_graph(vec![
            dep_info(DependencyKey::of::<D>(), Scope::Singleton, vec![]),
            dep_info(
                DependencyKey::of::<B>(),
                Scope::Singleton,
                vec![DependencyKey::of::<D>()],
            ),
            dep_info(
                DependencyKey::of::<C>(),
                Scope::Singleton,
                vec![DependencyKey::of::<D>()],
            ),
            dep_info(
                DependencyKey::of::<A>(),
                Scope::Singleton,
                vec![
                    DependencyKey::of::<B>(),
                    DependencyKey::of::<C>(),
                ],
            ),
        ]);

        let mut validator = GraphValidator::new(graph);
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn levenshtein_close_check() {
        assert!(levenshtein_close("UserService", "UserServise")); // typo
        assert!(levenshtein_close("Database", "Databse"));        // typo
        assert!(!levenshtein_close("Database", "Logger"));        // different
    }
}
