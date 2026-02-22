//! Dependency registry — stores all registrations for a scope.
//!
//! The registry maps [`DependencyKey`] to factory functions
//! that know how to create instances.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use tracing::{debug, trace};

use crate::error::{MakhzanError, AlreadyRegisteredError};
use crate::key::DependencyKey;
use crate::scope::Scope;

/// Type alias for factory functions.
///
/// A factory takes a reference to the [`Resolver`] (to resolve sub-dependencies)
/// and returns a boxed `Any` or an error.
///
/// # Why `Arc` and not `Box`?
/// Factories are shared between threads (Container is `Send + Sync`).
/// `Arc` allows cloning without copying the closure.
pub type FactoryFn = Arc<dyn Fn(&dyn Resolver) -> Result<Box<dyn Any + Send + Sync>, MakhzanError> + Send + Sync>;

/// Trait for resolving dependencies.
///
/// This is what factory functions receive to resolve their own dependencies.
/// Separated from Container to avoid circular references.
pub trait Resolver: Send + Sync {
    fn resolve_key(&self,key: &DependencyKey) -> Result<Box<dyn Any + Send + Sync>, MakhzanError>;
}
/// Registration entry for a single dependency.
#[derive(Clone)]
pub(crate) struct Registration {
    pub key: DependencyKey,
    pub factory: FactoryFn,
    pub scope: Scope,
    pub dependencies: Vec<DependencyKey>,
}


impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("key", &self.key)
            .field("scope", &self.scope)
            .field("dependencies", &self.dependencies)
            .finish()
    }
}

/// Stores all dependency registrations.
///
/// The registry is populated during the build phase and becomes
/// immutable once the container is constructed.
#[derive(Debug)]
pub(crate) struct Registry {
    registrations: HashMap<DependencyKey, Registration>,
    aliases: HashMap<DependencyKey, DependencyKey>,
}

impl Registry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            aliases: HashMap::new(),
        }
    }


    /// Registers a factory for a dependency key.
    ///
    /// # Errors
    /// Returns [`MakhzanError::AlreadyRegistered`] if the key is
    /// already registered and `allow_override` is false.
    pub fn register(
        &mut self,
        registration: Registration,
        allow_override: bool,
    ) -> Result<(), MakhzanError> {
        let key = registration.key.clone();

        if !allow_override && self.registrations.contains_key(&key) {
            return Err(MakhzanError::AlreadyRegistered(
                AlreadyRegisteredError { key },
            ));
        }

        debug!(key = %key, scope = %registration.scope, "Registered dependency");
        self.registrations.insert(key, registration);
        Ok(())
    }

    /// Registers an alias: resolving `from` will resolve `to` instead.
    ///
    /// Used for trait bindings: `bind::<dyn Logger, ConsoleLogger>()`
    /// creates an alias from `dyn Logger` to `ConsoleLogger`.
    pub fn register_alias(&mut self, from: DependencyKey, to: DependencyKey) {
        debug!(from = %from, to = %to, "Registered alias");
        self.aliases.insert(from, to);
    }

    /// Looks up a registration by key, following aliases.
    pub fn get(&self, key: &DependencyKey) -> Option<&Registration> {
        if let Some(aliased_key) = self.aliases.get(key) {
            trace!(from = %key, to = %aliased_key, "Following alias");
            return self.registrations.get(aliased_key);
        }
        self.registrations.get(key)
    }

    /// Returns all registrations (for validation).
    pub fn all_registrations(&self) -> &HashMap<DependencyKey, Registration> {
        &self.registrations
    }


    /// Returns all aliases (for validation).
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Returns the number of registered dependencies.
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    /// Returns true if no dependencies are registered.
     pub fn registered_keys(&self) -> Vec<DependencyKey> {
        let mut keys: Vec<_> = self.registrations.keys().cloned().collect();
        keys.extend(self.aliases.keys().cloned());
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Database;

    fn dummy_factory() -> FactoryFn {
        Arc::new(|_| Ok(Box::new(42i32)))
    }

    fn make_reg(key: DependencyKey, scope: Scope) -> Registration {
        Registration { key, factory: dummy_factory(), scope, dependencies: vec![] }
    }

    #[test]
    fn register_and_get() {
        let mut reg = Registry::new();
        let key = DependencyKey::of::<Database>();
        reg.register(make_reg(key.clone(), Scope::Singleton), false).unwrap();
        assert!(reg.get(&key).is_some());
    }

    #[test]
    fn duplicate_fails() {
        let mut reg = Registry::new();
        let key = DependencyKey::of::<Database>();
        reg.register(make_reg(key.clone(), Scope::Singleton), false).unwrap();
        assert!(reg.register(make_reg(key, Scope::Singleton), false).is_err());
    }

    #[test]
    fn duplicate_with_override_ok() {
        let mut reg = Registry::new();
        let key = DependencyKey::of::<Database>();
        reg.register(make_reg(key.clone(), Scope::Singleton), false).unwrap();
        assert!(reg.register(make_reg(key, Scope::Singleton), true).is_ok());
    }

    #[test]
    fn alias_resolves() {
        let mut reg = Registry::new();
        let concrete = DependencyKey::of::<String>();
        reg.register(make_reg(concrete.clone(), Scope::Singleton), false).unwrap();

        let alias_key = DependencyKey::of::<i64>();
        reg.register_alias(alias_key.clone(), concrete);
        assert!(reg.get(&alias_key).is_some());
    }
}
