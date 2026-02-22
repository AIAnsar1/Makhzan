//! # The Container — heart of Makhzan
//!
//! The dependency injection container that resolves and manages
//! the lifecycle of your application's dependencies.
//!
//! # Architecture
//! ```text
//! ContainerBuilder  ──build()──>  Container
//!                                    │
//!                              create_scope()
//!                                    │
//!                                    ▼
//!                              ScopedContainer
//! ```
//!
//! # Examples
//! ```rust
//! use makhzan_container::prelude::*;
//! use std::sync::Arc;
//!
//! trait Logger: Send + Sync {
//!     fn log(&self, msg: &str);
//! }
//!
//! struct ConsoleLogger;
//! impl Logger for ConsoleLogger {
//!     fn log(&self, msg: &str) { println!("{msg}"); }
//! }
//!
//! struct UserService {
//!     logger: Arc<dyn Logger>,
//! }
//!
//! let container = Container::builder()
//!     .singleton_with::<Arc<dyn Logger>>(|_| {
//!         Ok(Box::new(Arc::new(ConsoleLogger) as Arc<dyn Logger>))
//!     })
//!     .transient_with::<UserService>(|resolver| {
//!         let logger: Arc<dyn Logger> = resolver.resolve()?;
//!         Ok(Box::new(UserService { logger }))
//!     })
//!     .build()
//!     .expect("Failed to build container");
//!
//! let service: UserService = container.resolve().expect("Failed to resolve");
//! ```

use std::any::{Any, type_name};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use once_cell::sync::OnceCell;
use tracing::{debug, info, instrument, trace};

use crate::error::{MakhzanError, NotRegisteredError, Result};
use crate::graph::{DependencyInfo, GraphValidator};
use crate::key::DependencyKey;
use crate::provider::{Provider, ProviderRegistry};
use crate::registry::{FactoryFn, Registration, Registry, Resolver};
use crate::scope::Scope;


// ============================================================
// ContainerBuilder
// ============================================================

/// Builds a [`Container`] with registered dependencies.
///
/// Use the builder to register all your dependencies, then call
/// [`build()`](ContainerBuilder::build) to create an immutable,
/// thread-safe container.
///
/// # Examples
/// ```rust,ignore
/// let container = Container::builder()
///     .singleton_value(Config::load())
///     .singleton_with::<Database>(|resolver| { ... })
///     .transient_with::<UserService>(|resolver| { ... })
///     .build()?;
/// ```
pub struct ContainerBuilder {
    registry: Registry,
    allow_override: bool,
}
impl ContainerBuilder {
    fn new() -> Self {
        Self {
            registry: Registry::new(),
            allow_override: false,
        }
    }

    /// Allow overriding previously registered dependencies.
    pub fn allow_override(mut self, allow: bool) -> Self {
        self.allow_override = allow;
        self
    }

    // ── Singleton: pre-built value ──

    /// Register a pre-built value as a singleton.
    ///
    /// Cloned on every resolve (use `Arc<T>` for cheap sharing).
    pub fn singleton_value<T: Clone + Send + Sync + 'static>(self, value: T) -> Self {
        self.register_internal(
            DependencyKey::of::<T>(),
            Scope::Singleton,
            Arc::new(move |_: &dyn Resolver| {
                Ok(Box::new(value.clone()) as Box<dyn Any + Send + Sync>)
            }),
            vec![],
        )
    }

    // ── Singleton: factory ──

    /// Register a singleton factory.
    ///
    /// Called ONCE on first resolve (via `OnceCell`).
    /// Result is cloned on subsequent resolves.
    ///
    /// **`T` must implement `Clone`** — use `Arc<T>` for services.
    pub fn singleton_with<T: Clone + Send + Sync + 'static>(
        self,
        factory: impl Fn(&dyn Resolver) -> Result<T> + Send + Sync + 'static,
    ) -> Self {
        let cell: Arc<OnceCell<T>> = Arc::new(OnceCell::new());

        self.register_internal(
            DependencyKey::of::<T>(),
            Scope::Singleton,
            {
                let cell = cell.clone();
                Arc::new(move |resolver: &dyn Resolver| {
                    let value = cell.get_or_try_init(|| factory(resolver))?;
                    Ok(Box::new(value.clone()) as Box<dyn Any + Send + Sync>)
                })
            },
            vec![],
        )
    }

    // ── Scoped ──

    /// Register a scoped factory.
    ///
    /// Creates a new instance per scope. (Full per-scope caching is Phase 2.)
    pub fn scoped_with<T: Send + Sync + 'static>(
        self,
        factory: impl Fn(&dyn Resolver) -> Result<T> + Send + Sync + 'static,
    ) -> Self {
        self.register_internal(
            DependencyKey::of::<T>(),
            Scope::Scoped,
            Arc::new(move |resolver: &dyn Resolver| {
                Ok(Box::new(factory(resolver)?) as Box<dyn Any + Send + Sync>)
            }),
            vec![],
        )
    }

    // ── Transient ──

    /// Register a transient factory.
    ///
    /// Creates a NEW instance on every `resolve()` call.
    pub fn transient_with<T: Send + Sync + 'static>(
        self,
        factory: impl Fn(&dyn Resolver) -> Result<T> + Send + Sync + 'static,
    ) -> Self {
        self.register_internal(
            DependencyKey::of::<T>(),
            Scope::Transient,
            Arc::new(move |resolver: &dyn Resolver| {
                Ok(Box::new(factory(resolver)?) as Box<dyn Any + Send + Sync>)
            }),
            vec![],
        )
    }

    // ── Provider modules ──

    /// Add a [`Provider`] module.
    pub fn add_provider(mut self, provider: &dyn Provider) -> Self {
        provider.register(&mut self);
        self
    }

    // ── Build ──

    /// Build the container, validating the dependency graph.
    ///
    /// Checks: all deps registered, no cycles, scope compatibility.
    #[instrument(skip(self), name = "container_build")]
    pub fn build(self) -> Result<Container> {
        info!(registered = self.registry.len(), "Building container");

        let dep_infos: HashMap<DependencyKey, DependencyInfo> = self
            .registry
            .all_registrations()
            .iter()
            .map(|(key, reg)| {
                (
                    key.clone(),
                    DependencyInfo {
                        key: key.clone(),
                        dependencies: reg.dependencies.clone(),
                        scope: reg.scope,
                    },
                )
            })
            .collect();

        let mut validator = GraphValidator::new(dep_infos);
        validator.validate()?;

        info!("Container built successfully ✓");
        Ok(Container {
            registry: Arc::new(self.registry),
        })
    }

    // ── Internal ──

    fn register_internal(
        mut self,
        key: DependencyKey,
        scope: Scope,
        factory: FactoryFn,
        dependencies: Vec<DependencyKey>,
    ) -> Self {
        let registration = Registration {
            key,
            factory,
            scope,
            dependencies,
        };
        let _ = self.registry.register(registration, self.allow_override);
        self
    }
}

// ProviderRegistry impl so providers can register into builder
impl ProviderRegistry for ContainerBuilder {
    fn register_singleton(
        &mut self, key: DependencyKey, factory: FactoryFn, deps: Vec<DependencyKey>,
    ) {
        let reg = Registration { key, factory, scope: Scope::Singleton, dependencies: deps };
        let _ = self.registry.register(reg, self.allow_override);
    }

    fn register_scoped(
        &mut self, key: DependencyKey, factory: FactoryFn, deps: Vec<DependencyKey>,
    ) {
        let reg = Registration { key, factory, scope: Scope::Scoped, dependencies: deps };
        let _ = self.registry.register(reg, self.allow_override);
    }

    fn register_transient(
        &mut self, key: DependencyKey, factory: FactoryFn, deps: Vec<DependencyKey>,
    ) {
        let reg = Registration { key, factory, scope: Scope::Transient, dependencies: deps };
        let _ = self.registry.register(reg, self.allow_override);
    }

    fn register_alias(&mut self, from: DependencyKey, to: DependencyKey) {
        self.registry.register_alias(from, to);
    }
}

// ═══════════════════════════════════════════
// Container
// ═══════════════════════════════════════════

/// Immutable, thread-safe dependency injection container.
///
/// Created by [`ContainerBuilder::build()`].
pub struct Container {
    registry: Arc<Registry>,
}

impl Container {
    /// Create a new builder.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }

    /// Resolve a dependency by type.
    ///
    /// ```rust,ignore
    /// let db: Arc<Database> = container.resolve()?;
    /// ```
    pub fn resolve<T: Send + Sync + 'static>(&self) -> Result<T> {
        let key = DependencyKey::of::<T>();
        trace!(key = %key, "Resolving");

        let boxed = self.resolve_internal(&key)?;

        boxed.downcast::<T>().map(|b| *b).map_err(|_| {
            MakhzanError::ConstructionFailed {
                key,
                source: format!(
                    "Type mismatch: expected {}",
                    type_name::<T>()
                )
                .into(),
            }
        })
    }

    /// Create a scoped child container.
    pub fn create_scope(&self) -> ScopedContainer<'_> {
        debug!("Creating new scope");
        ScopedContainer { parent: self }
    }

    /// Internal resolve — returns type-erased value.
    fn resolve_internal(
        &self,
        key: &DependencyKey,
    ) -> Result<Box<dyn Any + Send + Sync>> {
        let registration = self.registry.get(key).ok_or_else(|| {
            MakhzanError::NotRegistered(NotRegisteredError {
                requested: key.clone(),
                required_by: None,
                suggestions: self.find_suggestions(key),
            })
        })?;

        let resolver = ContainerResolver { container: self };
        (registration.factory)(&resolver)
    }

    fn find_suggestions(&self, key: &DependencyKey) -> Vec<DependencyKey> {
        let target = key.type_name().to_lowercase();
        self.registry
            .registered_keys()
            .into_iter()
            .filter(|k| {
                if k == key {
                    return false;
                }
                let name = k.type_name().to_lowercase();
                name.contains(&target) || target.contains(&name)
            })
            .collect()
    }
}

impl fmt::Debug for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Container")
            .field("registered", &self.registry.len())
            .finish()
    }
}

// ═══════════════════════════════════════════
// ScopedContainer
// ═══════════════════════════════════════════

/// A scoped child container.
///
/// Currently delegates to parent. Per-scope caching is Phase 2.
pub struct ScopedContainer<'a> {
    parent: &'a Container,
}

impl ScopedContainer<'_> {
    /// Resolve a dependency within this scope.
    pub fn resolve<T: Send + Sync + 'static>(&self) -> Result<T> {
        // Phase 2: per-scope caching for Scope::Scoped
        self.parent.resolve::<T>()
    }
}

impl fmt::Debug for ScopedContainer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopedContainer").finish()
    }
}

// ═══════════════════════════════════════════
// ContainerResolver (internal bridge)
// ═══════════════════════════════════════════

/// Internal resolver passed to factory functions.
struct ContainerResolver<'a> {
    container: &'a Container,
}

impl Resolver for ContainerResolver<'_> {
    fn resolve_key(
        &self,
        key: &DependencyKey,
    ) -> Result<Box<dyn Any + Send + Sync>> {
        self.container.resolve_internal(key)
    }
}

// ═══════════════════════════════════════════
// Free function for use inside factories
// ═══════════════════════════════════════════

/// Resolve a typed dependency from a [`Resolver`].
///
/// Use this inside factory closures:
///
/// ```rust,ignore
/// builder.singleton_with::<MyService>(|r| {
///     let db: Arc<Database> = makhzan_container::container::resolve(r)?;
///     Ok(MyService { db })
/// })
/// ```
pub fn resolve<T: Send + Sync + 'static>(resolver: &dyn Resolver) -> Result<T> {
    let key = DependencyKey::of::<T>();
    let boxed = resolver.resolve_key(&key)?;
    boxed.downcast::<T>().map(|b| *b).map_err(|_| {
        MakhzanError::ConstructionFailed {
            key,
            source: format!(
                "Type mismatch: expected {}",
                type_name::<T>()
            )
            .into(),
        }
    })
}

// ═══════════════════════════════════════════
// Prelude
// ═══════════════════════════════════════════

pub mod prelude {
    pub use super::{resolve, Container, ContainerBuilder, ScopedContainer};
    pub use crate::error::{MakhzanError, Result};
    pub use crate::key::DependencyKey;
    pub use crate::provider::Provider;
    pub use crate::scope::Scope;
}

// ═══════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_singleton_value() {
        let container = Container::builder()
            .singleton_value(42i32)
            .build()
            .unwrap();

        let value: i32 = container.resolve().unwrap();
        assert_eq!(value, 42);

        // Resolve again — same value
        let value2: i32 = container.resolve().unwrap();
        assert_eq!(value2, 42);
    }

    #[test]
    fn resolve_singleton_string() {
        let container = Container::builder()
            .singleton_value(String::from("hello"))
            .build()
            .unwrap();

        let s: String = container.resolve().unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn resolve_transient_creates_new_each_time() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));

        let container = Container::builder()
            .transient_with::<u32>({
                let counter = counter.clone();
                move |_| {
                    Ok(counter.fetch_add(1, Ordering::SeqCst))
                }
            })
            .build()
            .unwrap();

        let a: u32 = container.resolve().unwrap();
        let b: u32 = container.resolve().unwrap();
        let c: u32 = container.resolve().unwrap();

        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
    }

    #[test]
    fn singleton_factory_called_once() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));

        let container = Container::builder()
            .singleton_with::<i32>({
                let counter = counter.clone();
                move |_| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                }
            })
            .build()
            .unwrap();

        let _a: i32 = container.resolve().unwrap();
        let _b: i32 = container.resolve().unwrap();
        let _c: i32 = container.resolve().unwrap();

        // Factory called only once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn resolve_with_dependency() {
        let container = Container::builder()
            .singleton_value(String::from("postgres://localhost"))
            .transient_with::<Vec<u8>>(|r| {
                let url: String = resolve(r)?;
                Ok(url.into_bytes())
            })
            .build()
            .unwrap();

        let bytes: Vec<u8> = container.resolve().unwrap();
        assert_eq!(bytes, b"postgres://localhost");
    }

    #[test]
    fn resolve_not_registered() {
        let container = Container::builder().build().unwrap();

        let result = container.resolve::<i32>();
        assert!(result.is_err());

        match result.unwrap_err() {
            MakhzanError::NotRegistered(e) => {
                assert!(e.requested.type_name().contains("i32"));
            }
            other => panic!("Expected NotRegistered, got: {other:?}"),
        }
    }

    #[test]
    fn scoped_container_resolves() {
        let container = Container::builder()
            .singleton_value(42i32)
            .build()
            .unwrap();

        let scope = container.create_scope();
        let value: i32 = scope.resolve().unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn arc_singleton_pattern() {
        // The idiomatic way: wrap services in Arc
        #[derive(Clone)]
        struct Database {
            url: String,
        }

        struct UserService {
            db: Arc<Database>,
        }

        let container = Container::builder()
            .singleton_with::<Arc<Database>>(|_| {
                Ok(Arc::new(Database {
                    url: "postgres://localhost".into(),
                }))
            })
            .transient_with::<UserService>(|r| {
                let db: Arc<Database> = resolve(r)?;
                Ok(UserService { db })
            })
            .build()
            .unwrap();

        let svc: UserService = container.resolve().unwrap();
        assert_eq!(svc.db.url, "postgres://localhost");
    }

    #[test]
    fn debug_display() {
        let container = Container::builder()
            .singleton_value(1i32)
            .singleton_value(String::from("x"))
            .build()
            .unwrap();

        let debug = format!("{container:?}");
        assert!(debug.contains("Container"));
        assert!(debug.contains("2")); // 2 registered
    }
}