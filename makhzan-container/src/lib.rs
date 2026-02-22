//! Core container implementation for Makhzan DI.

pub mod container;
pub mod error;
pub mod graph;
pub mod key;
pub mod provider;
pub mod registry;
pub mod scope;

pub use container::prelude;
pub use error::{MakhzanError, Result};
pub use key::DependencyKey;
pub use scope::Scope;
