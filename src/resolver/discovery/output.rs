use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Stmt, StmtKind};

use super::super::declarations::extract_discoverable_declarations;
use super::super::engine::resolve_stmts;
use super::super::function_variants;
use super::super::state::ResolveState;

pub(in crate::resolver) struct IncludeDiscovery {
    pub(in crate::resolver) declarations: Vec<Stmt>,
    pub(in crate::resolver) function_variants: FunctionVariantRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::resolver) struct FunctionVariantKey {
    canonical: PathBuf,
    function_key: String,
}

impl FunctionVariantKey {
    pub(in crate::resolver) fn new(canonical: &Path, function_name: &str) -> Self {
        Self {
            canonical: canonical.to_path_buf(),
            function_key: php_symbol_key(function_name),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::resolver) struct FunctionVariantInfo {
    pub(in crate::resolver) public_name: String,
    pub(in crate::resolver) variant_name: String,
}

pub(in crate::resolver) type FunctionVariantRegistry = HashMap<FunctionVariantKey, FunctionVariantInfo>;

#[derive(Clone)]
pub(in crate::resolver) struct DiscoveryEntry {
    pub(in crate::resolver) canonical: PathBuf,
    pub(in crate::resolver) span: crate::span::Span,
    pub(in crate::resolver) declarations: Vec<Stmt>,
    source_stmts: Vec<Stmt>,
    base_dir: PathBuf,
    declaration_state: ResolveState,
    include_chain: Vec<PathBuf>,
    pub(in crate::resolver) repeatable: bool,
    pub(in crate::resolver) exclusive_group: Option<String>,
    pub(in crate::resolver) exclusive_branch: Option<usize>,
}

#[derive(Default)]
pub(super) struct DiscoveryOutput {
    entries: Vec<DiscoveryEntry>,
}

impl DiscoveryOutput {
    pub(super) fn push(
        &mut self,
        canonical: PathBuf,
        span: crate::span::Span,
        declarations: Vec<Stmt>,
        source_stmts: Vec<Stmt>,
        base_dir: PathBuf,
        declaration_state: ResolveState,
        include_chain: Vec<PathBuf>,
        repeatable: bool,
    ) {
        if declarations.is_empty() {
            return;
        }
        if !repeatable && self.contains_canonical(&canonical) {
            return;
        }
        self.entries.push(DiscoveryEntry {
            canonical,
            span,
            declarations,
            source_stmts,
            base_dir,
            declaration_state,
            include_chain,
            repeatable,
            exclusive_group: None,
            exclusive_branch: None,
        });
    }

    pub(super) fn extend(&mut self, other: DiscoveryOutput) {
        self.entries.extend(other.entries);
    }

    fn contains_canonical(&self, canonical: &Path) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.canonical.as_path() == canonical)
    }

    pub(super) fn extend_once_guarded(&mut self, mut other: DiscoveryOutput) {
        for entry in &mut other.entries {
            entry.repeatable = false;
        }
        self.extend(other);
    }

    pub(super) fn extend_loop_body(&mut self, other: DiscoveryOutput) {
        let repeated = other
            .entries
            .iter()
            .filter(|entry| entry.repeatable)
            .cloned()
            .collect::<Vec<_>>();
        self.extend(other);
        self.entries.extend(repeated);
    }

    pub(super) fn merge_alternatives(alternatives: Vec<DiscoveryOutput>, group_id: String) -> DiscoveryOutput {
        let mut order: Vec<PathBuf> = Vec::new();
        let mut merged: HashMap<PathBuf, (DiscoveryEntry, usize)> = HashMap::new();

        for (branch_idx, alternative) in alternatives.into_iter().enumerate() {
            let mut branch_order: Vec<PathBuf> = Vec::new();
            let mut branch: HashMap<PathBuf, (DiscoveryEntry, usize)> = HashMap::new();

            for mut entry in alternative.entries {
                if entry.exclusive_group.is_none() {
                    entry.exclusive_group = Some(group_id.clone());
                    entry.exclusive_branch = Some(branch_idx);
                }
                let key = entry.canonical.clone();
                let branch_entry = branch.entry(key.clone()).or_insert_with(|| {
                    branch_order.push(key);
                    (entry.clone(), 0)
                });
                branch_entry.0.repeatable |= entry.repeatable;
                branch_entry.1 += 1;
            }

            for key in branch_order {
                let (entry, count) = branch.remove(&key).expect("branch key should exist");
                let merged_entry = merged.entry(key.clone()).or_insert_with(|| {
                    order.push(key);
                    (entry.clone(), 0)
                });
                merged_entry.0.repeatable |= entry.repeatable;
                merged_entry.1 = merged_entry.1.max(count);
            }
        }

        let mut output = DiscoveryOutput::default();
        for key in order {
            let (entry, count) = merged.remove(&key).expect("merged key should exist");
            for _ in 0..count {
                output.entries.push(entry.clone());
            }
        }
        output
    }

    pub(super) fn into_include_discovery(mut self) -> Result<IncludeDiscovery, CompileError> {
        let (_, preliminary_function_variants) =
            function_variants::rewrite_include_loaded_function_variants(&mut self.entries);
        self.rebuild_declarations(&preliminary_function_variants)?;
        let (groups, function_variants) =
            function_variants::rewrite_include_loaded_function_variants(&mut self.entries);
        let mut declarations = groups;
        declarations.extend(self.entries
            .into_iter()
            .map(|entry| {
                Stmt::new(
                    StmtKind::NamespaceBlock {
                        name: None,
                        body: entry.declarations,
                    },
                    entry.span,
                )
            })
        );
        Ok(IncludeDiscovery {
            declarations,
            function_variants,
        })
    }

    fn rebuild_declarations(
        &mut self,
        function_variants: &FunctionVariantRegistry,
    ) -> Result<(), CompileError> {
        for entry in &mut self.entries {
            let mut declaration_declared_once = HashSet::new();
            let mut declaration_include_chain = entry.include_chain.clone();
            let mut declaration_state = entry.declaration_state.clone();
            let resolved_declarations = resolve_stmts(
                entry.source_stmts.clone(),
                &entry.base_dir,
                &mut declaration_declared_once,
                &mut declaration_include_chain,
                &mut declaration_state,
                function_variants,
            )?;
            entry.declarations = extract_discoverable_declarations(&resolved_declarations);
        }
        Ok(())
    }
}
