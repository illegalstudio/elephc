//! Purpose:
//! Stores resolver state for namespace context, defined constants, and include bookkeeping.
//! Provides helpers for constant lookup and PHP-style define/import normalization.
//!
//! Called from:
//! - `crate::resolver::engine`, discovery walkers, and include path folding.
//!
//! Key details:
//! - Constant lookup happens before name resolution, so namespace strings come from raw AST names.

use std::collections::HashMap;

use crate::names::{Name, NameKind};
use crate::parser::ast::{Stmt, StmtKind, UseKind};

/// Tracks active namespace, constants defined via `define()`, and `use const` imports for the current resolution scope.
#[derive(Clone, Default)]
pub(super) struct ResolveState {
    /// Constants registered via `define()`. Maps canonical name to canonical value.
    pub(super) constants: HashMap<String, String>,
    /// The current namespace prefix, if any.
    pub(super) namespace: Option<String>,
    /// `use const X as Y` imports. Maps alias to canonical constant name.
    pub(super) const_imports: HashMap<String, String>,
}

/// Looks up a constant reference by name, applying PHP's namespace and import rules.
///
/// Checks in order: const imports (highest priority), namespace-qualified candidates,
/// and bare unqualified names. Returns the canonical value if found.
pub(super) fn resolve_constant_ref(name: &Name, state: &ResolveState) -> Option<String> {
    constant_lookup_candidates(name, state)
        .into_iter()
        .find_map(|candidate| state.constants.get(&candidate).cloned())
}

/// Generates the ordered list of canonical constant names to try when resolving a reference.
///
/// For unqualified names: checks const imports first, then namespace-qualified, then bare.
/// For qualified names: checks const imports for the first segment, then namespace-qualified.
/// For fully-qualified names: returns the canonical form directly.
fn constant_lookup_candidates(name: &Name, state: &ResolveState) -> Vec<String> {
    if name.is_fully_qualified() {
        return vec![name.as_canonical()];
    }

    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| state.const_imports.get(segment))
        {
            return vec![alias.clone()];
        }

        let raw = name.as_canonical();
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw), raw];
            }
        }
        return vec![raw];
    }

    if let Some(first) = name.parts.first() {
        if let Some(alias) = state.const_imports.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return vec![alias.clone()];
            }
            return vec![format!("{}\\{}", alias, suffix.join("\\"))];
        }
    }

    let raw = name.as_canonical();
    if name.kind == NameKind::Qualified {
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw)];
            }
        }
    }
    vec![raw]
}

/// Strips a leading backslash from a constant name as written in a `define()` call,
/// returning the normalized canonical name.
pub(super) fn normalize_defined_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}

/// Returns the canonical namespace string for a `Namespace` AST node,
/// or an empty string if the node is `None`.
pub(super) fn namespace_string(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

/// Extracts `use const` declarations from a `use` statement and registers them in the state's const imports map.
///
/// Only processes `UseKind::Const` items; other use kinds are ignored.
pub(super) fn register_const_imports(state: &mut ResolveState, stmt: &Stmt) {
    let StmtKind::UseDecl { imports } = &stmt.kind else {
        return;
    };
    for item in imports {
        if item.kind == UseKind::Const {
            state.const_imports.insert(
                item.alias.clone(),
                normalize_defined_constant_name(&item.name.as_canonical()),
            );
        }
    }
}

/// Returns `true` if the name refers to the builtin `define()` function,
/// i.e. an unqualified or fully-qualified single-segment name equal to `"define"`.
pub(super) fn is_define_call_name(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0] == "define"
}
