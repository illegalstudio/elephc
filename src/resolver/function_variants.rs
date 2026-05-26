//! Purpose:
//! Rewrites include-loaded functions that have mutually exclusive declaration variants.
//! Collects occurrences, chooses supported variant groups, and rewrites local function names.
//!
//! Called from:
//! - `crate::resolver::resolve()` after include declaration discovery.
//!
//! Key details:
//! - Variant symbol names are stable and deterministic so runtime activation checks can link correctly.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

use super::discovery::{
    DiscoveryEntry, FunctionVariantInfo, FunctionVariantKey, FunctionVariantRegistry,
};
use super::state::namespace_string;

/// Rewrites include-loaded function declarations into variant groups and registers them.
///
/// Scans all discovery entries for functions with mutually exclusive branches (e.g., inside
/// `ifdef`/`ifndef` blocks), collects occurrences by public name, filters to supported variant
/// groups, generates stable variant symbol names, builds a `FunctionVariantRegistry` for runtime
/// activation, and rewrites the AST function declarations to use the variant local names.
///
/// Returns a vector of synthetic `FunctionVariantGroup` statements (used by codegen to emit
/// variant metadata) and the registry mapping canonical public keys to variant infos.
///
/// # Arguments
/// * `entries` — mutable slice of discovery entries from include resolution, each containing
///   declarations from one source file along with exclusive group/branch metadata
///
/// # Registry invariant
/// The registry maps `(canonical_path, public_name)` to `FunctionVariantInfo` so the runtime
/// can activate the correct variant at link time based on which include branch was taken.
pub(super) fn rewrite_include_loaded_function_variants(
    entries: &mut [DiscoveryEntry],
) -> (Vec<Stmt>, FunctionVariantRegistry) {
    let mut occurrences = Vec::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        let mut occurrence_index = 0;
        collect_function_occurrences(
            &entry.declarations,
            entry_index,
            entry,
            None,
            &mut occurrence_index,
            &mut occurrences,
        );
    }

    let mut by_public: BTreeMap<String, Vec<FunctionOccurrence>> = BTreeMap::new();
    for occurrence in occurrences {
        by_public
            .entry(occurrence.public_key.clone())
            .or_default()
            .push(occurrence);
    }

    let mut rewrites: HashMap<(usize, usize), FunctionVariantRewrite> = HashMap::new();
    let mut registry = FunctionVariantRegistry::default();
    let mut groups = Vec::new();

    for occurrences in by_public.values() {
        if !is_supported_include_loaded_function_group(occurrences) {
            continue;
        }

        let public_name = occurrences[0].public_name.clone();
        let mut variants = Vec::new();
        for occurrence in occurrences {
            let local_name = variant_local_name(occurrence);
            let variant_name = canonical_name_for_decl(occurrence.namespace.as_deref(), &local_name);
            let info = FunctionVariantInfo {
                public_name: public_name.clone(),
                variant_name: variant_name.clone(),
            };
            registry.insert(
                FunctionVariantKey::new(&occurrence.canonical, &public_name),
                info,
            );
            rewrites.insert(
                (occurrence.entry_index, occurrence.occurrence_index),
                FunctionVariantRewrite { local_name },
            );
            variants.push(variant_name);
        }

        groups.push(Stmt::new(
            StmtKind::FunctionVariantGroup {
                name: public_name,
                variants,
            },
            occurrences[0].span,
        ));
    }

    for (entry_index, entry) in entries.iter_mut().enumerate() {
        let mut occurrence_index = 0;
        rewrite_function_occurrences(
            &mut entry.declarations,
            entry_index,
            &rewrites,
            &mut occurrence_index,
        );
    }

    (groups, registry)
}

/// Tracks a single function occurrence discovered during include resolution.
///
/// Used to collect all variants of a public function name across multiple include entries
/// before filtering and rewriting.
#[derive(Clone)]
struct FunctionOccurrence {
    entry_index: usize,
    occurrence_index: usize,
    canonical: PathBuf,
    public_name: String,
    public_key: String,
    local_name: String,
    namespace: Option<String>,
    span: crate::span::Span,
    exclusive_group: Option<String>,
    exclusive_branch: Option<usize>,
}

/// Holds the new local name for a function declaration that was rewritten to a variant symbol.
///
/// The key in the rewrites map is `(entry_index, occurrence_index)` which uniquely identifies
/// which function declaration in which entry this rewrite applies to.
#[derive(Clone)]
struct FunctionVariantRewrite {
    local_name: String,
}

/// Recursively collects function declarations from a statement list, tracking namespaces.
///
/// Pushes a `FunctionOccurrence` onto `occurrences` for every `FunctionDecl` encountered,
/// carrying entry index, occurrence index (within the entry), canonical path, public name,
/// public key (PHP symbol key for case-insensitive lookup), local name, namespace, source span,
/// and exclusive group/branch metadata from the discovery entry.
///
/// # Arguments
/// * `stmts` — statement list to walk
/// * `entry_index` — index into the resolver's discovery entries (identifies the source file)
/// * `entry` — the discovery entry providing exclusive group/branch metadata
/// * `namespace` — current namespace scope, built up as we descend into `NamespaceBlock`
/// * `occurrence_index` — incremented for each function found; passed by-mut to assign stable indices
/// * `occurrences` — output vector appended to in depth-first order
fn collect_function_occurrences(
    stmts: &[Stmt],
    entry_index: usize,
    entry: &DiscoveryEntry,
    namespace: Option<String>,
    occurrence_index: &mut usize,
    occurrences: &mut Vec<FunctionOccurrence>,
) {
    let mut namespace = namespace;
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                namespace = Some(namespace_string(name));
            }
            StmtKind::NamespaceBlock { name, body } => {
                collect_function_occurrences(
                    body,
                    entry_index,
                    entry,
                    Some(namespace_string(name)),
                    occurrence_index,
                    occurrences,
                );
            }
            StmtKind::Synthetic(body) => {
                collect_function_occurrences(
                    body,
                    entry_index,
                    entry,
                    namespace.clone(),
                    occurrence_index,
                    occurrences,
                );
            }
            StmtKind::FunctionDecl { name, .. } => {
                let public_name = canonical_name_for_decl(namespace.as_deref(), name);
                occurrences.push(FunctionOccurrence {
                    entry_index,
                    occurrence_index: *occurrence_index,
                    canonical: entry.canonical.clone(),
                    public_key: php_symbol_key(&public_name),
                    public_name,
                    local_name: name.clone(),
                    namespace: namespace.clone(),
                    span: stmt.span,
                    exclusive_group: entry.exclusive_group.clone(),
                    exclusive_branch: entry.exclusive_branch,
                });
                *occurrence_index += 1;
            }
            _ => {}
        }
    }
}

/// Returns true if a group of same-named function occurrences is supported for variant rewriting.
///
/// A group is supported when:
/// - It has exactly one occurrence (no variant needed), OR
/// - All occurrences share the same `exclusive_group` id AND each occurrence has a distinct
///   `exclusive_branch` number (mutually exclusive branches with no overlap)
///
/// If any occurrence lacks `exclusive_group` or `exclusive_branch`, or if two occurrences share
/// the same branch, the group is not supported and is skipped during rewriting.
fn is_supported_include_loaded_function_group(occurrences: &[FunctionOccurrence]) -> bool {
    if occurrences.len() == 1 {
        return true;
    }

    let Some(group_id) = occurrences
        .first()
        .and_then(|occurrence| occurrence.exclusive_group.as_ref())
    else {
        return false;
    };
    let mut seen_branches = HashSet::new();
    for occurrence in occurrences {
        if occurrence.exclusive_group.as_deref() != Some(group_id) {
            return false;
        }
        let Some(branch) = occurrence.exclusive_branch else {
            return false;
        };
        if !seen_branches.insert(branch) {
            return false;
        }
    }
    true
}

/// Generates a stable, unique local name for a function variant.
///
/// The name encodes the include canonical path, public key, exclusive group, branch, and the
/// original local function name. Uses a FNV-like hash for determinism so the same source file
/// and branch produce identical variant names across compilations.
///
/// Format: `__elephc_include_variant_{hash}_{sanitized_local_name}`
///
/// # Arguments
/// * `occurrence` — the function occurrence with canonical path, public key, exclusive metadata
///
/// # Sanitization
/// Non-alphanumeric characters in the local name are replaced with `_`; empty names become `fn`.
fn variant_local_name(occurrence: &FunctionOccurrence) -> String {
    let branch = occurrence
        .exclusive_branch
        .map(|branch| branch.to_string())
        .unwrap_or_default();
    let canonical = occurrence.canonical.to_string_lossy();
    format!(
        "__elephc_include_variant_{}_{}",
        stable_hash_hex(&[
            canonical.as_ref(),
            &occurrence.public_key,
            occurrence.exclusive_group.as_deref().unwrap_or(""),
            &branch,
        ]),
        sanitize_identifier_segment(&occurrence.local_name)
    )
}

/// Sanitizes a PHP identifier segment for use in a generated symbol name.
///
/// Keeps only ASCII alphanumerics and underscores; replaces all other characters with `_`.
/// Returns `"fn"` if the input is empty after sanitization (prevents empty symbol segments).
fn sanitize_identifier_segment(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("fn");
    }
    out
}

/// Computes a 64-bit FNV-1a hash of the concatenated input parts and returns it as a 16-digit hex string.
///
/// Used to produce deterministic, stable hashes for variant symbol names from include path,
/// public key, exclusive group, and branch. The hash is NOT cryptographically secure; it only
/// needs to be stable and collision-resistant for symbol formation within a compilation.
fn stable_hash_hex(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Recursively rewrites function declarations in place using a precomputed rewrite map.
///
/// Walks the statement tree depth-first; for each `FunctionDecl`, looks up
/// `(entry_index, occurrence_index)` in `rewrites` and replaces the function's `name` with the
/// stored variant local name. Updates `occurrence_index` as it visits each function to stay in
/// sync with the indices assigned during collection.
///
/// # Arguments
/// * `stmts` — statement list to walk and mutate in place
/// * `entry_index` — index identifying which discovery entry this statement list belongs to
/// * `rewrites` — map from `(entry_index, occurrence_index)` to the new local name
/// * `occurrence_index` — current occurrence counter, incremented after each `FunctionDecl` visited
fn rewrite_function_occurrences(
    stmts: &mut [Stmt],
    entry_index: usize,
    rewrites: &HashMap<(usize, usize), FunctionVariantRewrite>,
    occurrence_index: &mut usize,
) {
    for stmt in stmts {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                rewrite_function_occurrences(body, entry_index, rewrites, occurrence_index);
            }
            StmtKind::FunctionDecl { name, .. } => {
                if let Some(rewrite) = rewrites.get(&(entry_index, *occurrence_index)) {
                    *name = rewrite.local_name.clone();
                }
                *occurrence_index += 1;
            }
            _ => {}
        }
    }
}
