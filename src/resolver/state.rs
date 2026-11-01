//! Purpose:
//! Stores resolver state for namespace context, defined constants, and include bookkeeping.
//! Provides helpers for constant lookup and PHP-style define/import normalization.
//!
//! Called from:
//! - `crate::resolver::engine`, discovery walkers, and include path folding.
//!
//! Key details:
//! - Constant lookup happens before name resolution, so namespace strings come from raw AST names.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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
    /// Invocation-level symbols used to select `ifdef` branches in every physical file.
    pub(super) conditional_defines: HashSet<String>,
    /// Canonical class-like name to the PHYSICAL file that declares it, recorded as each
    /// include is loaded and therefore before its own includes are spliced into it.
    ///
    /// `Reflection*::getFileName()` needs the declaring file, not the file that happened to pull
    /// the declaration in. Attribution done after splicing cannot tell them apart, so it is
    /// collected here where the path is still known.
    pub(super) declared_class_files: HashMap<String, String>,
    /// Canonical function name to the physical file that declares it, same rule as above.
    pub(super) declared_function_files: HashMap<String, String>,
    /// Immutable source inputs remain shared when semantic resolver scopes are cloned.
    pub(super) source_units: super::source_units::SourceUnitCollector,
    /// Files the compiler read but did NOT take whole: a conditional declaration in them was
    /// dropped rather than bound, mapped to the names it dropped.
    ///
    /// Reporting such a file as one the compiler included is a lie with teeth: the runtime skips
    /// exactly those files, so the dropped declaration is made by nobody and the first call to it
    /// fails with `Call to undefined function`. Every file on the include chain is recorded, not
    /// just the one holding the declaration — a skipped ancestor never reaches its descendant.
    /// SHARED across clones, like `source_units`, and for the same reason: `resolve_isolated`
    /// clones the state for every control-structure branch so that include and constant EFFECTS
    /// do not leak back. This is not an effect, it is a fact about the build — and a polyfill
    /// writes its delegation as `if (\PHP_VERSION_ID >= 80100) { require …; }`, so a per-clone
    /// field records the drop inside the branch and throws it away with the clone. MEASURED on a
    /// four-file fixture: the same `require` at file scope reports `dropped=["probe_dc"]` and
    /// inside an `if` reports `dropped=[]`.
    pub(super) unbound_conditional_declaration_sources: SharedDeclarationFacts,
    /// Names bound through the function-variant mechanism anywhere in this resolve, subtracted
    /// from the dropped names before anything is concluded.
    pub(super) bound_conditional_declaration_names: SharedNameSet,
}

impl ResolveState {
    /// Creates resolver state carrying the conditional symbols for subsequently loaded files.
    pub(super) fn with_conditional_defines(defines: &HashSet<String>) -> Self {
        Self {
            conditional_defines: defines.clone(),
            ..Self::default()
        }
    }
}

/// A path-to-dropped-names map shared by every clone of one `ResolveState`.
#[derive(Clone, Default)]
pub(super) struct SharedDeclarationFacts(
    std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, Vec<String>>>>,
);

impl SharedDeclarationFacts {
    /// Records names dropped while stripping `path`.
    pub(super) fn record(&self, path: PathBuf, names: impl IntoIterator<Item = String>) {
        let mut facts = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        facts.entry(path).or_default().extend(names);
    }

    /// Returns a copy of everything recorded so far.
    pub(super) fn snapshot(&self) -> HashMap<PathBuf, Vec<String>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// A name set shared by every clone of one `ResolveState`, same rule as `SharedDeclarationFacts`.
#[derive(Clone, Default)]
pub(super) struct SharedNameSet(std::sync::Arc<std::sync::Mutex<HashSet<String>>>);

impl SharedNameSet {
    /// Adds names bound through the function-variant mechanism.
    pub(super) fn extend(&self, names: impl IntoIterator<Item = String>) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(names);
    }

    /// Returns a copy of everything recorded so far.
    pub(super) fn snapshot(&self) -> HashSet<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
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
