//! Provider trait — a module of related dependency registrations.
//!
//! Providers group related dependencies together, similar to
//! Laravel's ServiceProvider or DIshka's Provider class.
//!
//! # Examples
//! ```rust,ignore
//! struct DatabaseProvider;
//!
//! impl Provider for DatabaseProvider {
//!     fn register(&self, builder: &mut ContainerBuilder) {
//!         builder.singleton::<Database>(|_| {
//!             Ok(Database::connect("postgres://localhost"))
//!         });
//!         builder.bind::<dyn Repository, PostgresRepository>();
//!     }
//! }
//! ```

/// A module that registers related dependencies into a container.
///
/// Implement this trait to group related services together.
/// This is the Rust equivalent of:
/// - DIshka's `Provider` class
/// - Laravel's `ServiceProvider`
/// - .NET's `IServiceCollection` extension methods
///
/// # Design Philosophy
/// Providers encourage modular architecture. Instead of one giant
/// registration block, split your dependencies by domain:
///
/// ```rust,ignore
/// // Good: separated by concern
/// container.add_provider(DatabaseProvider);
/// container.add_provider(AuthProvider);
/// container.add_provider(EmailProvider);
///
/// // Bad: everything in one place
/// container.register::<Database>(...);
/// container.register::<AuthService>(...);
/// container.register::<EmailService>(...);
/// // ... 200 more lines
/// ```
pub trait Provider: Send + Sync {
    /// Register dependencies into the container builder.
    ///
    /// Called once during container construction.
    fn register(&self, builder: &mut dyn ProviderRegistry);

    /// Optional: human-readable name for error messages.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Interface that providers use to register dependencies.
///
/// This is a subset of ContainerBuilder's API, exposed to Provider
/// implementations. This decoupling allows providers to be tested
/// independently.
pub trait ProviderRegistry {
    /// Register a singleton factory.
    fn register_singleton(
        &mut self,
        key: crate::key::DependencyKey,
        factory: crate::registry::FactoryFn,
        dependencies: Vec<crate::key::DependencyKey>,
    );

    /// Register a scoped factory.
    fn register_scoped(
        &mut self,
        key: crate::key::DependencyKey,
        factory: crate::registry::FactoryFn,
        dependencies: Vec<crate::key::DependencyKey>,
    );

    /// Register a transient factory.
    fn register_transient(
        &mut self,
        key: crate::key::DependencyKey,
        factory: crate::registry::FactoryFn,
        dependencies: Vec<crate::key::DependencyKey>,
    );

    /// Register a type alias (trait binding).
    fn register_alias(
        &mut self,
        from: crate::key::DependencyKey,
        to: crate::key::DependencyKey,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::DependencyKey;
    use crate::registry::FactoryFn;
    use std::sync::Arc;

    // Mock registry for testing providers
    struct MockRegistry {
        registered_count: usize,
        alias_count: usize,
    }

    impl MockRegistry {
        fn new() -> Self {
            Self {
                registered_count: 0,
                alias_count: 0,
            }
        }
    }

    impl ProviderRegistry for MockRegistry {
        fn register_singleton(
            &mut self,
            _key: DependencyKey,
            _factory: FactoryFn,
            _deps: Vec<DependencyKey>,
        ) {
            self.registered_count += 1;
        }

        fn register_scoped(
            &mut self,
            _key: DependencyKey,
            _factory: FactoryFn,
            _deps: Vec<DependencyKey>,
        ) {
            self.registered_count += 1;
        }

        fn register_transient(
            &mut self,
            _key: DependencyKey,
            _factory: FactoryFn,
            _deps: Vec<DependencyKey>,
        ) {
            self.registered_count += 1;
        }

        fn register_alias(
            &mut self,
            _from: DependencyKey,
            _to: DependencyKey,
        ) {
            self.alias_count += 1;
        }
    }

    // Test provider
    struct TestProvider;

    impl Provider for TestProvider {
        fn register(&self, builder: &mut dyn ProviderRegistry) {
            builder.register_singleton(
                DependencyKey::of::<String>(),
                Arc::new(|_| Ok(Box::new(String::from("hello")))),
                vec![],
            );

            builder.register_transient(
                DependencyKey::of::<i32>(),
                Arc::new(|_| Ok(Box::new(42i32))),
                vec![],
            );
        }
    }

    #[test]
    fn provider_registers_dependencies() {
        let mut registry = MockRegistry::new();
        let provider = TestProvider;

        provider.register(&mut registry);

        assert_eq!(registry.registered_count, 2);
    }

    #[test]
    fn provider_has_name() {
        let provider = TestProvider;
        assert!(provider.name().contains("TestProvider"));
    }
}
