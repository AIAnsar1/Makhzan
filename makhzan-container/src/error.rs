//! Error types for Makhzan container operations.
//!
//! Makhzan provides detailed, actionable error messages.
//! No more `TypeNotFound: 0x7f3a2b1c`.

use crate::key::DependencyKey;
use crate::scope::Scope;
use std::fmt;

/// Main error type for all Makhzan operations.
#[derive(Debug, thiserror::Error)]
pub enum MakhzanError {
    /// Requested dependency was never registered.
    #[error("{}", .0)]
    NotRegistered(NotRegisteredError),

    /// Circular dependency detected during resolve.
    #[error("{}", .0)]
    CircularDependency(CircularDependencyError),

    /// Scope mismatch: tried to inject a shorter-lived dependency
    /// into a longer-lived one.
    #[error("{}", .0)]
    ScopeMismatch(ScopeMismatchError),

    /// Factory returned an error during construction.
    #[error("Failed to construct {key}: {source}")]
    ConstructionFailed {
        key: DependencyKey,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Dependency was already registered (when override is disabled).
    #[error("{}", .0)]
    AlreadyRegistered(AlreadyRegisteredError),

    /// Container is already built and cannot be modified.
    #[error("Container is already built. Register dependencies before calling .build()")]
    ContainerFrozen,
}

/// Error when a dependency was not registered.
///
/// Includes helpful hints about what went wrong.
#[derive(Debug)]
pub struct NotRegisteredError {
    /// The dependency that was requested
    pub requested: DependencyKey,
    /// What required this dependency (if known)
    pub required_by: Option<DependencyKey>,
    /// Similar types that ARE registered (for "did you mean?" suggestions)
    pub suggestions: Vec<DependencyKey>,
}

impl fmt::Display for NotRegisteredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dependency not registered: {}", self.requested)?;

        if let Some(ref parent) = self.required_by {
            write!(f, "\n  Required by: {parent}")?;
        }

        if !self.suggestions.is_empty() {
            write!(f, "\n  Did you mean one of:")?;
            for suggestion in &self.suggestions {
                write!(f, "\n    - {suggestion}")?;
            }
        }

        write!(
            f,
            "\n  Hint: Did you forget to call .register::<{}>()?",
            self.requested.type_name()
        )
    }
}

/// Error when a circular dependency is detected.
///
/// Shows the full dependency chain so you can see WHERE the cycle is.
#[derive(Debug)]
pub struct CircularDependencyError {
    /// The chain of dependencies that forms the cycle.
    /// Example: ["A", "B", "C", "A"]
    pub chain: Vec<DependencyKey>,
}

impl fmt::Display for CircularDependencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Circular dependency detected:\n  ")?;

        let chain_str: Vec<String> = self.chain.iter().map(|k| k.type_name().to_string()).collect();
        write!(f, "{}", chain_str.join(" → "))?;

        write!(
            f,
            "\n  Hint: Consider using lazy injection or restructuring your dependencies"
        )
    }
}

/// Error when scope rules are violated.
///
/// You cannot inject a Transient into a Singleton —
/// the Singleton would hold a stale reference.
#[derive(Debug)]
pub struct ScopeMismatchError {
    /// The dependency being injected
    pub dependency: DependencyKey,
    pub dependency_scope: Scope,
    /// Where it's being injected
    pub consumer: DependencyKey,
    pub consumer_scope: Scope,
}

impl fmt::Display for ScopeMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Scope mismatch: cannot inject {} ({}) into {} ({})",
            self.dependency, self.dependency_scope, self.consumer, self.consumer_scope,
        )?;
        write!(
            f,
            "\n  A {} dependency cannot depend on a {} dependency",
            self.consumer_scope, self.dependency_scope,
        )?;
        write!(
            f,
            "\n  Hint: Change {} to {} or wider",
            self.dependency, self.consumer_scope,
        )
    }
}

/// Error when trying to register a dependency that already exists.
#[derive(Debug)]
pub struct AlreadyRegisteredError {
    pub key: DependencyKey,
}

impl fmt::Display for AlreadyRegisteredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Dependency already registered: {}",
            self.key,
        )?;
        write!(
            f,
            "\n  Hint: Use .override_::<T>() to explicitly override, or enable allow_override in settings"
        )
    }
}

/// Convenient Result type for Makhzan operations.
pub type Result<T> = std::result::Result<T, MakhzanError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_registered_error_display() {
        let err = MakhzanError::NotRegistered(NotRegisteredError {
            requested: DependencyKey::of::<String>(),
            required_by: Some(DependencyKey::of::<Vec<u8>>()),
            suggestions: vec![],
        });

        let msg = format!("{err}");
        assert!(msg.contains("not registered"));
        assert!(msg.contains("String"));
    }

    #[test]
    fn circular_dependency_error_display() {
        let err = MakhzanError::CircularDependency(CircularDependencyError {
            chain: vec![
                DependencyKey::of::<String>(),
                DependencyKey::of::<i32>(),
                DependencyKey::of::<String>(),
            ],
        });

        let msg = format!("{err}");
        assert!(msg.contains("Circular"));
        assert!(msg.contains("→"));
    }

    #[test]
    fn scope_mismatch_error_display() {
        let err = MakhzanError::ScopeMismatch(ScopeMismatchError {
            dependency: DependencyKey::of::<String>(),
            dependency_scope: Scope::Transient,
            consumer: DependencyKey::of::<Vec<u8>>(),
            consumer_scope: Scope::Singleton,
        });

        let msg = format!("{err}");
        assert!(msg.contains("Scope mismatch"));
        assert!(msg.contains("Singleton"));
        assert!(msg.contains("Transient"));
    }
}
