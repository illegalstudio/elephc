use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

use super::discovery::{
    DiscoveryEntry, FunctionVariantInfo, FunctionVariantKey, FunctionVariantRegistry,
};
use super::state::namespace_string;

pub(super) fn rewrite_conditional_function_variants(
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
        if !is_supported_conditional_function_group(occurrences) {
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

#[derive(Clone)]
struct FunctionVariantRewrite {
    local_name: String,
}

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

fn is_supported_conditional_function_group(occurrences: &[FunctionOccurrence]) -> bool {
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
