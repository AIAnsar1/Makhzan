//! Dependency identification keys.
//!
//! [`DependencyKey`] uniquely identifies a dependency within the container.
//! It combines a [`TypeId`] with an optional name for named bindings.

use std::any::{TypeId, type_name};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Uniquely identifies a dependency in the container.
///
/// Each dependency is identified by its Rust type ([`TypeId`]) and an
/// optional name for cases where multiple instances of the same type
/// are needed.
///
/// # Examples
/// ```
/// use makhzan_container::key::DependencyKey;
///
/// // Simple key — just a type
/// let key = DependencyKey::of::<String>();
/// assert_eq!(key.type_name(), "alloc::string::String");
/// assert_eq!(key.name(), None);
///
/// // Named key — type + name
/// let key = DependencyKey::named::<String>("database_url");
/// assert_eq!(key.name(), Some("database_url"));
/// ```
#[derive(Clone)]
pub struct DependencyKey {
    type_id: TypeId,
    type_name: &'static str,
    name: Option<&'static str>,
}

impl DependencyKey {
    /// Creates a key for type `T`.
    ///
    /// # Examples
    /// ```
    /// use makhzan_container::key::DependencyKey;
    ///
    /// let key = DependencyKey::of::<i32>();
    /// ```
    #[inline]
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            name: None,
        }
    }

    /// Creates a named key for type `T`.
    ///
    /// Named keys allow registering multiple instances of the same type.
    ///
    /// # Examples
    /// ```
    /// use makhzan_container::key::DependencyKey;
    ///
    /// let primary = DependencyKey::named::<String>("primary_db");
    /// let replica = DependencyKey::named::<String>("replica_db");
    /// assert_ne!(primary, replica);
    /// ```
    #[inline]
    pub fn named<T: ?Sized + 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            name: Some(name),
        }
    }

    /// Creates a key from a raw [`TypeId`] and type name.
    ///
    /// Prefer [`DependencyKey::of`] when possible — this is for
    /// advanced use cases (e.g., inside proc-macros).
    #[inline]
    pub fn from_raw(type_id: TypeId, type_name: &'static str) -> Self {
        Self { type_id, type_name, name: None }
    }

    /// Returns the [`TypeId`] of this dependency.
    #[inline]
    pub fn type_id(&self) -> TypeId { 
        self.type_id
    }

    /// Returns the human-readable type name.
    ///
    /// Used in error messages for better developer experience.
    #[inline]
    pub fn type_name(&self) -> &'static str { 
        self.type_name 
    }

    /// Returns the optional name for named bindings.
    #[inline]
    pub fn name(&self) -> Option<&'static str> { 
        self.name 
    }
}

// PartialEq: два ключа равны если совпадает TypeId И name
impl PartialEq for DependencyKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.name == other.name
    }
}

impl Eq for DependencyKey {}

// Hash: хешируем по TypeId + name
impl Hash for DependencyKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.name.hash(state);
    }
}

impl fmt::Debug for DependencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name {
            Some(name) => write!(f, "DependencyKey({}, name={:?})", self.type_name, name),
            None => write!(f, "DependencyKey({})", self.type_name),
        }
    }
}

impl fmt::Display for DependencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name {
            Some(name) => write!(f, "{} (name={:?})", self.type_name, name),
            None => write!(f, "{}", self.type_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MyStruct;

    #[test]
    fn key_of_type() {
        let key = DependencyKey::of::<MyStruct>();
        assert!(key.type_name().contains("MyStruct"));
        assert_eq!(key.name(), None);
    }

    #[test]
    fn key_equality_same_type() {
        assert_eq!(DependencyKey::of::<String>(), DependencyKey::of::<String>());
    }

    #[test]
    fn key_inequality_different_types() {
        assert_ne!(DependencyKey::of::<String>(), DependencyKey::of::<i32>());
    }

    #[test]
    fn named_keys_different() {
        let k1 = DependencyKey::named::<String>("a");
        let k2 = DependencyKey::named::<String>("b");
        assert_ne!(k1, k2);
    }

    #[test]
    fn named_vs_unnamed_different() {
        assert_ne!(
            DependencyKey::named::<String>("a"),
            DependencyKey::of::<String>()
        );
    }

    #[test]
    fn key_in_hashmap() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(DependencyKey::of::<String>(), "string");
        map.insert(DependencyKey::of::<i32>(), "i32");
        assert_eq!(map.get(&DependencyKey::of::<String>()), Some(&"string"));
        assert_eq!(map.get(&DependencyKey::of::<bool>()), None);
    }

    #[test]
    fn unsized_type_key() {
        // dyn traits work as keys
        trait MyTrait {}
        let _key = DependencyKey::of::<dyn MyTrait>();
    }
}