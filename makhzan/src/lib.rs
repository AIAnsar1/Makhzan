//! # Makhzan — Dependency Injection Container for Rust
//!
//! A powerful, ergonomic IoC container inspired by DIshka, Laravel Container,
//! and .NET Dependency Injection.

pub use makhzan_container::*;
pub use makhzan_derive::*;
pub use makhzan_support::*;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
