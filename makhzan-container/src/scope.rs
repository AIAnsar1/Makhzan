//! Dependency lifecycle scopes.
//!
//! Scopes determine how long a resolved dependency lives:
//! - [`Scope::Singleton`] — one instance for the entire application
//! - [`Scope::Scoped`] — one instance per scope (e.g., HTTP request)
//! - [`Scope::Transient`] — new instance every time
//!
//! # Ordering
//! Scopes have a natural ordering: `Singleton > Scoped > Transient`.
//! A Singleton "outlives" a Scoped, which "outlives" a Transient.
use std::fmt;
/// Defines the lifetime of a dependency within the container.
///
/// # Examples
/// ```
/// use makhzan_container::scope::Scope;
///
/// // Singletons live longest
/// assert!(Scope::Singleton > Scope::Scoped);
/// assert!(Scope::Scoped > Scope::Transient);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// One instance shared across the entire application.
    ///
    /// Created on first resolve, lives until the container is dropped.
    /// Equivalent to: DIshka `APP`, Laravel `singleton()`, .NET `Singleton`.
    ///
    /// # When to use
    /// - Database connection pools
    /// - Configuration objects
    /// - Shared caches
    Singleton,

    /// One instance per scope (e.g., per HTTP request).
    ///
    /// Created on first resolve within a scope, dropped when the scope ends.
    /// Equivalent to: DIshka `REQUEST`, .NET `Scoped`.
    ///
    /// # When to use
    /// - Per-request database transactions
    /// - User session data
    /// - Request-specific loggers
    Scoped,

    /// New instance created on every resolve call.
    ///
    /// Never cached. Each `resolve()` returns a fresh instance.
    /// Equivalent to: DIshka `ACTION`, .NET `Transient`.
    ///
    /// # When to use
    /// - Lightweight stateless services
    /// - Command/query handlers
    /// - Objects with mutable state that shouldn't be shared
    Transient,
}

impl Scope {
    /// Returns `true` if this scope caches instances.
    ///
    /// Singleton and Scoped both cache; Transient does not.
    #[inline]
    pub fn is_cached(&self) -> bool {
        matches!(self, Scope::Singleton | Scope::Scoped)
    }

    /// Returns `true` if this scope lives for the entire application.
    #[inline]
    pub fn is_singleton(&self) -> bool {
        matches!(self, Scope::Singleton)
    }

    /// Returns the ordering value (higher = longer lifetime).
    #[inline]
    fn ordering(&self) -> u8 {
        match self {
            Scope::Singleton => 2,
            Scope::Scoped => 1,
            Scope::Transient => 0,
        }
    }
}

impl PartialOrd for Scope {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Scope {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordering().cmp(&other.ordering())
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Singleton => write!(f, "Singleton"),
            Scope::Scoped => write!(f, "Scoped"),
            Scope::Transient => write!(f, "Transient"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_ordering() {
        assert!(Scope::Singleton > Scope::Scoped);
        assert!(Scope::Scoped > Scope::Transient);
        assert!(Scope::Singleton > Scope::Transient);
    }

    #[test]
    fn scope_equality() {
        assert_eq!(Scope::Singleton, Scope::Singleton);
        assert_ne!(Scope::Singleton, Scope::Transient);
    }

    #[test]
    fn scope_is_cached() {
        assert!(Scope::Singleton.is_cached());
        assert!(Scope::Scoped.is_cached());
        assert!(!Scope::Transient.is_cached());
    }

    #[test]
    fn scope_display() {
        assert_eq!(format!("{}", Scope::Singleton), "Singleton");
        assert_eq!(format!("{}", Scope::Scoped), "Scoped");
        assert_eq!(format!("{}", Scope::Transient), "Transient");
    }
}
