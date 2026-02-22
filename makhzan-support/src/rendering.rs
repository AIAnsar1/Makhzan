//! Text rendering utilities for human-friendly error messages.
//!
//! Provides helpers to format dependency chains, type names,
//! and helpful suggestions in error output.

use std::fmt;

/// Renders a dependency chain as a readable string.
///
/// # Examples
/// ```
/// use makhzan_support::rendering::render_chain;
///
/// let chain = vec!["UserService", "UserRepo", "Database", "UserService"];
/// let rendered = render_chain(&chain);
/// assert_eq!(rendered, "UserService → UserRepo → Database → UserService");
/// ```
pub fn render_chain(chain: &[impl AsRef<str>]) -> String {
    chain
        .iter()
        .map(|s| s.as_ref())
        .collect::<Vec<_>>()
        .join(" → ")
}

/// Renders a dependency chain with scope annotations.
///
/// ```text
/// [Singleton] Database
///       ↓
/// [Scoped]    UserRepository
///       ↓
/// [Transient] UserService
/// ```
pub fn render_chain_vertical(entries: &[ChainEntry]) -> String {
    let mut result = String::new();
    let max_scope_len = entries
        .iter()
        .map(|e| e.scope.len())
        .max()
        .unwrap_or(0);

    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            // Arrow between entries
            result.push_str(&" ".repeat(max_scope_len + 3));
            result.push_str("↓\n");
        }

        result.push_str(&format!(
            "[{:<width$}] {}",
            entry.scope,
            entry.type_name,
            width = max_scope_len,
        ));

        if let Some(ref source) = entry.source_name {
            result.push_str(&format!("  (from: {source})"));
        }

        result.push('\n');
    }

    result
}

/// An entry in a dependency chain for vertical rendering.
#[derive(Debug)]
pub struct ChainEntry {
    /// The type name
    pub type_name: String,
    /// The scope (e.g., "Singleton", "Scoped")
    pub scope: String,
    /// Optional: what factory/function creates this
    pub source_name: Option<String>,
}

/// Shortens a fully qualified type name for display.
///
/// ```
/// use makhzan_support::rendering::shorten_type_name;
///
/// let short = shorten_type_name("my_app::services::user::UserService");
/// assert_eq!(short, "UserService");
///
/// let short = shorten_type_name("alloc::sync::Arc<dyn my_app::traits::Logger>");
/// assert_eq!(short, "Arc<dyn Logger>");
/// ```
pub fn shorten_type_name(full_name: &str) -> String {
    // Strategy: take the last segment of each path component
    // "my_app::services::UserService" → "UserService"
    // "Arc<dyn my_app::Logger>" → "Arc<dyn Logger>"

    let mut result = String::with_capacity(full_name.len());
    let mut chars = full_name.chars().peekable();
    let mut current_segment = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            ':' if chars.peek() == Some(&':') => {
                chars.next(); // consume second ':'
                current_segment.clear(); // discard path prefix
            }
            '<' | '>' | ',' | ' ' => {
                result.push_str(&current_segment);
                result.push(ch);
                current_segment.clear();
            }
            _ => {
                current_segment.push(ch);
            }
        }
    }

    result.push_str(&current_segment);
    result
}

/// Generates a "did you mean?" suggestion based on registered types.
///
/// Compares the requested type name against available types
/// and suggests close matches.
pub fn suggest_similar(
    requested: &str,
    available: &[&str],
    max_suggestions: usize,
) -> Vec<String> {
    let requested_lower = requested.to_lowercase();
    let requested_short = shorten_type_name(requested).to_lowercase();

    let mut scored: Vec<(&str, usize)> = available
        .iter()
        .filter_map(|&name| {
            let name_lower = name.to_lowercase();
            let name_short = shorten_type_name(name).to_lowercase();

            // Exact substring match (highest priority)
            if name_lower.contains(&requested_lower)
                || requested_lower.contains(&name_lower)
            {
                return Some((name, 100));
            }

            // Short name match
            if name_short.contains(&requested_short)
                || requested_short.contains(&name_short)
            {
                return Some((name, 80));
            }

            // Common prefix
            let common = name_short
                .chars()
                .zip(requested_short.chars())
                .take_while(|(a, b)| a == b)
                .count();

            if common >= 3 {
                return Some((name, common * 10));
            }

            None
        })
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored
        .into_iter()
        .take(max_suggestions)
        .map(|(name, _)| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_simple_chain() {
        let chain = vec!["A", "B", "C", "A"];
        assert_eq!(render_chain(&chain), "A → B → C → A");
    }

    #[test]
    fn render_single_element_chain() {
        let chain = vec!["A"];
        assert_eq!(render_chain(&chain), "A");
    }

    #[test]
    fn render_empty_chain() {
        let chain: Vec<&str> = vec![];
        assert_eq!(render_chain(&chain), "");
    }

    #[test]
    fn shorten_simple_path() {
        assert_eq!(
            shorten_type_name("my_app::services::UserService"),
            "UserService"
        );
    }

    #[test]
    fn shorten_with_generics() {
        assert_eq!(
            shorten_type_name("alloc::sync::Arc<dyn my_app::traits::Logger>"),
            "Arc<dyn Logger>"
        );
    }

    #[test]
    fn shorten_no_path() {
        assert_eq!(shorten_type_name("String"), "String");
    }

    #[test]
    fn suggest_similar_types() {
        let available = vec![
            "my_app::UserService",
            "my_app::UserRepository",
            "my_app::Logger",
            "my_app::Database",
        ];

        let suggestions = suggest_similar("UserServise", &available, 3);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("UserService"));
    }

    #[test]
    fn suggest_no_match() {
        let available = vec!["my_app::Database"];
        let suggestions = suggest_similar("XyzAbcDef", &available, 3);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn vertical_chain_rendering() {
        let entries = vec![
            ChainEntry {
                type_name: "Database".to_string(),
                scope: "Singleton".to_string(),
                source_name: None,
            },
            ChainEntry {
                type_name: "UserRepository".to_string(),
                scope: "Scoped".to_string(),
                source_name: Some("new()".to_string()),
            },
            ChainEntry {
                type_name: "UserService".to_string(),
                scope: "Transient".to_string(),
                source_name: None,
            },
        ];

        let rendered = render_chain_vertical(&entries);
        assert!(rendered.contains("Database"));
        assert!(rendered.contains("Singleton"));
        assert!(rendered.contains("↓"));
        assert!(rendered.contains("UserService"));
    }
}