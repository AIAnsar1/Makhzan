//! # Makhzan — Dependency Injection Container for Rust
//!
//! A powerful, ergonomic IoC container inspired by DIshka, Laravel Container,
//! and .NET Dependency Injection.

//! # Makhzan — Dependency Injection Container for Rust
//!
//! مخزن — "The Vault"
//!
//! A powerful, ergonomic IoC container inspired by DIshka, Laravel, and .NET DI.
//!
//! # Quick Start
//! ```rust,ignore
//! use makhzan::prelude::*;
//! use std::sync::Arc;
//!
//! // Define your services
//! trait Logger: Send + Sync { fn log(&self, msg: &str); }
//! struct ConsoleLogger;
//! impl Logger for ConsoleLogger {
//!     fn log(&self, msg: &str) { println!("[LOG] {msg}"); }
//! }
//!
//! struct UserService { logger: Arc<dyn Logger> }
//!
//! // Build container
//! let container = Container::builder()
//!     .singleton_with::<Arc<dyn Logger>>(|_| {
//!         Ok(Arc::new(ConsoleLogger) as Arc<dyn Logger>)
//!     })
//!     .transient_with::<UserService>(|r| {
//!         let logger: Arc<dyn Logger> = r.resolve()?;
//!         Ok(UserService { logger })
//!     })
//!     .build()?;
//!
//! let service: UserService = container.resolve()?;
//! service.logger.log("It works!");
//! ```

pub use makhzan_container::*;
pub use makhzan_container::container::prelude::*;
pub use makhzan_support::rendering;