//! Purpose:
//! Compile-time path-expression evaluation helpers shared by include-path folding
//! and the autoload symbolic interpreter.
//!
//! Called from:
//! - `crate::resolver::include_path::fold_include_path` — folds `dirname()` of compile-time
//!   strings inside include/require path expressions.
//! - `crate::autoload::interpret` — folds `dirname()` inside `spl_autoload_register` closure bodies.
//!
//! Key details:
//! - `fold_dirname` matches PHP semantics (see `src/codegen/runtime/io/dirname.rs`) so the
//!   compile-time fold and the runtime helper agree on edge cases (`/`, `.`, empty, trailing and
//!   internal slashes). All supported targets use `/` as the separator, so this helper is `/`-only.

use crate::names::{Name, NameKind};

/// Returns `true` if `name` refers to the builtin `dirname()` function at resolver time.
///
/// Mirrors `is_define_call_name` (`src/resolver/state.rs`) but is case-insensitive, because PHP
/// function names are case-insensitive and the name resolver (which lowercases builtins) has not
/// run yet at include-path-folding time. Accepts unqualified (`dirname`) and fully-qualified
/// (`\dirname`) single-segment names in any ASCII case; rejects multi-segment qualified names
/// (e.g. `Foo\dirname`) which are user-namespace calls, not the builtin.
pub(crate) fn is_dirname_call(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0].eq_ignore_ascii_case("dirname")
}

/// Folds `dirname(path, levels)` to a compile-time string, matching PHP semantics.
///
/// Strips `levels` trailing path components. Returns `None` when `levels < 1` (the caller then
/// surfaces the appropriate error); the type checker re-validates arity/levels later. Edge cases
/// mirror PHP and `src/codegen/runtime/io/dirname.rs`: empty → `.`, no separator → `.`, root-only
/// or all-slash input → `/`, trailing slashes are stripped before component removal, and internal
/// redundant slashes are preserved (e.g. `dirname("/usr///local///bin")` → `/usr///local`).
pub(crate) fn fold_dirname(path: &str, levels: i64) -> Option<String> {
    if levels < 1 {
        return None;
    }
    let mut current = path.to_string();
    for _ in 0..levels {
        current = dirname_once(&current);
    }
    Some(current)
}

/// Computes a single `dirname` level of `path`, matching PHP's per-component semantics.
fn dirname_once(path: &str) -> String {
    // PHP returns "." for an empty path.
    if path.is_empty() {
        return ".".to_string();
    }
    // Strip trailing slashes; if nothing remains the input was all-slash (root) → "/".
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    // No separator at all → "." (e.g. "foo" → ".").
    let Some(idx) = trimmed.rfind('/') else {
        return ".".to_string();
    };
    // Parent is everything before the last separator, with its own trailing slashes stripped so
    // internal redundant slashes are preserved but trailing ones collapse. An empty parent means
    // the only separator was the leading root → "/".
    let parent = trimmed[..idx].trim_end_matches('/');
    if parent.is_empty() {
        "/".to_string()
    } else {
        parent.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the headline Symfony pattern: `dirname(__DIR__)` of a canonical absolute path
    /// folds to its parent directory.
    #[test]
    fn fold_dirname_strips_one_component() {
        assert_eq!(fold_dirname("/srv/app/public", 1), Some("/srv/app".to_string()));
    }

    /// Verifies multi-level stripping and the PHP "." fallback when all components are consumed.
    #[test]
    fn fold_dirname_multiple_levels() {
        assert_eq!(fold_dirname("a/b/c", 1), Some("a/b".to_string()));
        assert_eq!(fold_dirname("a/b/c", 2), Some("a".to_string()));
        assert_eq!(fold_dirname("a/b/c", 3), Some(".".to_string()));
    }

    /// Verifies the root is stable under further levels (`dirname("/", 2)` is still "/").
    #[test]
    fn fold_dirname_root_is_stable() {
        assert_eq!(fold_dirname("/", 1), Some("/".to_string()));
        assert_eq!(fold_dirname("/", 2), Some("/".to_string()));
        assert_eq!(fold_dirname("///", 1), Some("/".to_string()));
    }

    /// Verifies `.` and no-separator inputs fold to "." (matching PHP), and empty input folds to "."
    /// matching the runtime helper (`src/codegen/runtime/io/dirname.rs`); PHP returns "" for empty,
    /// but that case is unreachable in include paths (`__DIR__` and autoload paths are non-empty).
    #[test]
    fn fold_dirname_empty_dot_no_separator() {
        assert_eq!(fold_dirname("", 1), Some(".".to_string()));
        assert_eq!(fold_dirname(".", 1), Some(".".to_string()));
        assert_eq!(fold_dirname("foo", 1), Some(".".to_string()));
        assert_eq!(fold_dirname("..", 1), Some(".".to_string()));
    }

    /// Verifies trailing slashes are stripped, then one component is removed, so `dirname("/foo/")`
    /// behaves like `dirname("/foo")` → "/", while `dirname("/foo/bar/")` → "/foo". Matches PHP and
    /// the runtime helper (`src/codegen/runtime/io/dirname.rs`).
    #[test]
    fn fold_dirname_trailing_slash() {
        assert_eq!(fold_dirname("/foo/", 1), Some("/".to_string()));
        assert_eq!(fold_dirname("/foo/bar/", 1), Some("/foo".to_string()));
        assert_eq!(fold_dirname("/foo///", 1), Some("/".to_string()));
        assert_eq!(fold_dirname("/a/", 1), Some("/".to_string()));
    }

    /// Verifies internal redundant slashes are preserved, matching PHP and the runtime helper.
    #[test]
    fn fold_dirname_preserves_internal_slashes() {
        assert_eq!(fold_dirname("/usr///local///bin", 1), Some("/usr///local".to_string()));
        assert_eq!(fold_dirname("/a///b", 1), Some("/a".to_string()));
        assert_eq!(fold_dirname("a//b", 1), Some("a".to_string()));
    }

    /// Verifies `levels < 1` is rejected (returns `None`) so the caller reports the misuse.
    #[test]
    fn fold_dirname_rejects_sub_one_levels() {
        assert_eq!(fold_dirname("/a/b", 0), None);
        assert_eq!(fold_dirname("/a/b", -1), None);
    }

    /// Verifies `is_dirname_call` is case-insensitive and accepts the FQN form, mirroring
    /// `is_define_call_name`'s shape with ASCII-case folding.
    #[test]
    fn is_dirname_call_case_insensitive_and_fqn() {
        assert!(is_dirname_call(&Name::unqualified("dirname")));
        assert!(is_dirname_call(&Name::unqualified("Dirname")));
        assert!(is_dirname_call(&Name::unqualified("DIRNAME")));
        assert!(is_dirname_call(&Name::from_parts(NameKind::FullyQualified, vec!["dirname".to_string()])));
        assert!(is_dirname_call(&Name::from_parts(NameKind::FullyQualified, vec!["Dirname".to_string()])));
    }

    /// Verifies `is_dirname_call` rejects multi-segment qualified names (user-namespace calls).
    #[test]
    fn is_dirname_call_rejects_qualified_names() {
        assert!(!is_dirname_call(&Name::qualified(vec!["Foo".to_string(), "dirname".to_string()])));
        assert!(!is_dirname_call(&Name::unqualified("basename")));
    }
}