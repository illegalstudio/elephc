use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{
    CatchClause, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
};

use super::declarations::extract_discoverable_declarations;
use super::engine::resolve_stmts;
use super::files::{parse_file, resolve_path};
use super::include_path::fold_include_path;
use super::state::{
    is_define_call_name, namespace_string, normalize_defined_constant_name,
    register_const_imports, ResolveState,
};

pub(super) fn discover_include_declarations(
    stmts: &[Stmt],
    base_dir: &Path,
) -> Result<IncludeDiscovery, CompileError> {
    let mut output = DiscoveryOutput::default();
    let mut loaded_paths = HashSet::new();
    let mut include_chain = Vec::new();
    let mut state = ResolveState::default();

    discover_stmts(
        stmts,
        base_dir,
        &mut loaded_paths,
        &mut include_chain,
        &mut state,
        &mut output,
    )?;

    output.into_include_discovery()
}

pub(super) struct IncludeDiscovery {
    pub declarations: Vec<Stmt>,
    pub function_variants: FunctionVariantRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct FunctionVariantKey {
    canonical: PathBuf,
    function_key: String,
}

impl FunctionVariantKey {
    pub(super) fn new(canonical: &Path, function_name: &str) -> Self {
        Self {
            canonical: canonical.to_path_buf(),
            function_key: php_symbol_key(function_name),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FunctionVariantInfo {
    pub public_name: String,
    pub variant_name: String,
}

pub(super) type FunctionVariantRegistry = HashMap<FunctionVariantKey, FunctionVariantInfo>;

#[derive(Clone)]
pub(super) struct DiscoveryEntry {
    pub canonical: PathBuf,
    pub span: crate::span::Span,
    pub declarations: Vec<Stmt>,
    source_stmts: Vec<Stmt>,
    base_dir: PathBuf,
    declaration_state: ResolveState,
    include_chain: Vec<PathBuf>,
    pub repeatable: bool,
    pub exclusive_group: Option<String>,
    pub exclusive_branch: Option<usize>,
}

#[derive(Default)]
struct DiscoveryOutput {
    entries: Vec<DiscoveryEntry>,
}

impl DiscoveryOutput {
    fn push(
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

    fn extend(&mut self, other: DiscoveryOutput) {
        self.entries.extend(other.entries);
    }

    fn contains_canonical(&self, canonical: &Path) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.canonical.as_path() == canonical)
    }

    fn extend_once_guarded(&mut self, mut other: DiscoveryOutput) {
        for entry in &mut other.entries {
            entry.repeatable = false;
        }
        self.extend(other);
    }

    fn extend_loop_body(&mut self, other: DiscoveryOutput) {
        let repeated = other
            .entries
            .iter()
            .filter(|entry| entry.repeatable)
            .cloned()
            .collect::<Vec<_>>();
        self.extend(other);
        self.entries.extend(repeated);
    }

    fn merge_alternatives(alternatives: Vec<DiscoveryOutput>, group_id: String) -> DiscoveryOutput {
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

    fn into_include_discovery(mut self) -> Result<IncludeDiscovery, CompileError> {
        let (_, preliminary_function_variants) =
            super::function_variants::rewrite_include_loaded_function_variants(&mut self.entries);
        self.rebuild_declarations(&preliminary_function_variants)?;
        let (groups, function_variants) =
            super::function_variants::rewrite_include_loaded_function_variants(&mut self.entries);
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

struct BranchDiscovery {
    output: DiscoveryOutput,
    loaded_paths: HashSet<PathBuf>,
}

impl BranchDiscovery {
    fn empty(loaded_paths: &HashSet<PathBuf>) -> Self {
        Self {
            output: DiscoveryOutput::default(),
            loaded_paths: loaded_paths.clone(),
        }
    }
}

fn discover_stmts(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for stmt in stmts {
        discover_stmt(stmt, base_dir, loaded_paths, include_chain, state, output)?;
    }
    Ok(())
}

fn discover_stmt(
    stmt: &Stmt,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Include { path, once, required } => {
            discover_include(
                path,
                *once,
                *required,
                stmt.span,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::ConstDecl { name, value } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            if let Ok(s) = fold_include_path(value, state) {
                let key = canonical_name_for_decl(state.namespace.as_deref(), name);
                state.constants.insert(key, s);
            }
        }
        StmtKind::ExprStmt(expr) => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
            if let ExprKind::FunctionCall { name, args } = &expr.kind {
                if is_define_call_name(name) && args.len() == 2 {
                    if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                        if let Ok(value) = fold_include_path(&args[1], state) {
                            state
                                .constants
                                .insert(normalize_defined_constant_name(const_name), value);
                        }
                    }
                }
            }
        }
        StmtKind::NamespaceDecl { name } => {
            state.namespace = Some(namespace_string(name));
            state.const_imports = HashMap::new();
        }
        StmtKind::NamespaceBlock { name, body } => {
            let saved_namespace = state.namespace.clone();
            let saved_imports = state.const_imports.clone();
            state.namespace = Some(namespace_string(name));
            state.const_imports = HashMap::new();
            discover_stmts(body, base_dir, loaded_paths, include_chain, state, output)?;
            state.namespace = saved_namespace;
            state.const_imports = saved_imports;
        }
        StmtKind::UseDecl { .. } => {
            register_const_imports(state, stmt);
        }
        StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            let group_id = exclusive_group_id(stmt.span, base_dir, include_chain);

            match constant_truthiness(condition) {
                Some(true) => {
                    discover_isolated(
                        then_body,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                        output,
                    )?;
                }
                Some(false) => discover_if_tail(
                    elseif_clauses,
                    else_body.as_deref(),
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                    group_id,
                    Vec::new(),
                )?,
                None => {
                    let then_output = discover_branch_output(
                        then_body,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                    )?;
                    discover_if_tail(
                        elseif_clauses,
                        else_body.as_deref(),
                        base_dir,
                        loaded_paths,
                        include_chain,
                        state,
                        output,
                        group_id,
                        vec![then_output],
                    )?;
                }
            }
        }
        StmtKind::While { condition, body } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            if constant_truthiness(condition) != Some(false) {
                let body_output =
                    discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
                output.extend_loop_body(body_output);
            }
        }
        StmtKind::DoWhile { condition, body } => {
            let body_output =
                discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            if constant_truthiness(condition) == Some(false) {
                output.extend(body_output);
            } else {
                output.extend_loop_body(body_output);
            }
        }
        StmtKind::For { init, condition, update, body } => {
            if let Some(init) = init {
                discover_stmt(init, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if let Some(condition) = condition {
                discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if condition.as_ref().and_then(constant_truthiness) != Some(false) {
                let mut loop_output =
                    discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
                if let Some(update) = update {
                    let mut update_state = state.clone();
                    discover_stmt(
                        update,
                        base_dir,
                        loaded_paths,
                        include_chain,
                        &mut update_state,
                        &mut loop_output,
                    )?;
                }
                output.extend_loop_body(loop_output);
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            discover_expr(array, base_dir, loaded_paths, include_chain, state, output)?;
            let body_output =
                discover_isolated_output(body, base_dir, loaded_paths, include_chain, state)?;
            output.extend_loop_body(body_output);
        }
        StmtKind::Switch { subject, cases, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, output)?;
            for (values, body) in cases {
                for value in values {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
                }
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            if let Some(body) = default {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            discover_isolated(
                try_body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
            for CatchClause { body, .. } in catches {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            if let Some(body) = finally_body {
                discover_isolated(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, output)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::ClassDecl { properties, methods, .. }
        | StmtKind::TraitDecl { properties, methods, .. } => {
            discover_properties(
                properties,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::InterfaceDecl { methods, .. } => {
            discover_methods(
                methods,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        StmtKind::EnumDecl { cases, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
                }
            }
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            discover_expr(index, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        StmtKind::Return(None)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn discover_include(
    path: &Expr,
    once: bool,
    required: bool,
    span: crate::span::Span,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    let path_str = fold_include_path(path, state).map_err(|msg| CompileError::new(span, &msg))?;
    let resolved = resolve_path(&path_str, base_dir);
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

    if !resolved.exists() {
        if required {
            return Err(CompileError::new(
                span,
                &format!("Required file not found: '{}'", path_str),
            ));
        }
        return Ok(());
    }

    if once && loaded_paths.contains(&canonical) {
        return Ok(());
    }

    if include_chain.contains(&canonical) {
        if once {
            return Ok(());
        }
        return Err(CompileError::new(
            span,
            &format!("Circular include detected: '{}'", path_str),
        ));
    }

    let included_stmts = parse_file(&resolved, span)?;
    let included_stmts =
        crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);

    let included_dir = resolved.parent().unwrap_or(base_dir);
    let mut declaration_state = state.clone();
    declaration_state.namespace = None;
    declaration_state.const_imports = HashMap::new();
    include_chain.push(canonical.clone());

    let saved_namespace = state.namespace.clone();
    let saved_imports = state.const_imports.clone();
    state.namespace = None;
    state.const_imports = HashMap::new();
    let mut nested_output = DiscoveryOutput::default();
    discover_stmts(
        &included_stmts,
        included_dir,
        loaded_paths,
        include_chain,
        state,
        &mut nested_output,
    )?;
    state.namespace = saved_namespace;
    state.const_imports = saved_imports;

    let entry_declaration_state = declaration_state.clone();
    let entry_include_chain = include_chain.clone();
    let mut declaration_declared_once = HashSet::new();
    let mut declaration_include_chain = entry_include_chain.clone();
    let mut declaration_state_for_resolution = declaration_state.clone();
    let declaration_function_variants = FunctionVariantRegistry::default();
    let resolved_declarations = resolve_stmts(
        included_stmts.clone(),
        included_dir,
        &mut declaration_declared_once,
        &mut declaration_include_chain,
        &mut declaration_state_for_resolution,
        &declaration_function_variants,
    )?;

    include_chain.pop();
    loaded_paths.insert(canonical.clone());
    if once {
        output.extend_once_guarded(nested_output);
    } else {
        output.extend(nested_output);
    }

    let file_declarations = extract_discoverable_declarations(&resolved_declarations);
    output.push(
        canonical,
        span,
        file_declarations,
        included_stmts,
        included_dir.to_path_buf(),
        entry_declaration_state,
        entry_include_chain,
        !once,
    );

    Ok(())
}

fn discover_isolated_output(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<DiscoveryOutput, CompileError> {
    let mut local = state.clone();
    let mut output = DiscoveryOutput::default();
    discover_stmts(
        stmts,
        base_dir,
        loaded_paths,
        include_chain,
        &mut local,
        &mut output,
    )?;
    Ok(output)
}

fn discover_isolated(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    output.extend(discover_isolated_output(
        stmts,
        base_dir,
        loaded_paths,
        include_chain,
        state,
    )?);
    Ok(())
}

fn discover_branch_output(
    stmts: &[Stmt],
    base_dir: &Path,
    loaded_paths: &HashSet<PathBuf>,
    include_chain: &[PathBuf],
    state: &ResolveState,
) -> Result<BranchDiscovery, CompileError> {
    let mut local_state = state.clone();
    let mut local_loaded_paths = loaded_paths.clone();
    let mut local_include_chain = include_chain.to_vec();
    let mut output = DiscoveryOutput::default();
    discover_stmts(
        stmts,
        base_dir,
        &mut local_loaded_paths,
        &mut local_include_chain,
        &mut local_state,
        &mut output,
    )?;
    Ok(BranchDiscovery {
        output,
        loaded_paths: local_loaded_paths,
    })
}

fn merge_branch_discoveries(
    branches: Vec<BranchDiscovery>,
    loaded_paths: &mut HashSet<PathBuf>,
    group_id: String,
) -> DiscoveryOutput {
    let mut outputs = Vec::with_capacity(branches.len());
    let mut merged_loaded_paths: Option<HashSet<PathBuf>> = None;

    for branch in branches {
        match &mut merged_loaded_paths {
            Some(paths) => {
                paths.retain(|path| branch.loaded_paths.contains(path));
            }
            None => {
                merged_loaded_paths = Some(branch.loaded_paths);
            }
        }
        outputs.push(branch.output);
    }

    if let Some(paths) = merged_loaded_paths {
        *loaded_paths = paths;
    }

    DiscoveryOutput::merge_alternatives(outputs, group_id)
}

#[allow(clippy::too_many_arguments)]
fn discover_if_tail(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
    group_id: String,
    mut alternatives: Vec<BranchDiscovery>,
) -> Result<(), CompileError> {
    for (condition, body) in elseif_clauses {
        let mut condition_state = state.clone();
        discover_expr(
            condition,
            base_dir,
            loaded_paths,
            include_chain,
            &mut condition_state,
            output,
        )?;

        match constant_truthiness(condition) {
            Some(false) => {}
            Some(true) => {
                alternatives.push(discover_branch_output(
                    body,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                )?);
                output.extend(merge_branch_discoveries(
                    alternatives,
                    loaded_paths,
                    group_id,
                ));
                return Ok(());
            }
            None => alternatives.push(discover_branch_output(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
            )?),
        }
    }

    alternatives.push(match else_body {
        Some(body) => discover_branch_output(body, base_dir, loaded_paths, include_chain, state)?,
        None => BranchDiscovery::empty(loaded_paths),
    });
    output.extend(merge_branch_discoveries(
        alternatives,
        loaded_paths,
        group_id,
    ));
    Ok(())
}

fn exclusive_group_id(
    span: crate::span::Span,
    base_dir: &Path,
    include_chain: &[PathBuf],
) -> String {
    let owner = include_chain
        .last()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| base_dir.to_string_lossy().into_owned());
    format!("{}:{}:{}", owner, span.line, span.col)
}

fn discover_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
    }
    Ok(())
}

fn discover_properties(
    properties: &[ClassProperty],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for property in properties {
        if let Some(default) = &property.default {
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
    }
    Ok(())
}

fn discover_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for method in methods {
        let mut local = state.clone();
        discover_params(
            &method.params,
            base_dir,
            loaded_paths,
            include_chain,
            &mut local,
            output,
        )?;
        discover_isolated(
            &method.body,
            base_dir,
            loaded_paths,
            include_chain,
            state,
            output,
        )?;
    }
    Ok(())
}

fn discover_expr(
    expr: &Expr,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            discover_expr(left, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(right, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::InstanceOf { value, target } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            discover_instanceof_target(target, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ArrayAccess { array: value, index: default }
        | ExprKind::ShortTernary { value, default } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            discover_expr(target, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            if let Some(result_target) = result_target {
                discover_expr(
                    result_target,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            discover_isolated(
                prelude,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::ArrayLiteral(items) => {
            discover_exprs(items, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                discover_expr(key, base_dir, loaded_paths, include_chain, state, output)?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            }
        }
        ExprKind::Match { subject, arms, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, output)?;
            for (patterns, value) in arms {
                discover_exprs(
                    patterns,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if let Some(default) = default {
                discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
            }
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(then_expr, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(else_expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Cast { expr, .. } | ExprKind::NamedArg { value: expr, .. } => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Closure { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, output)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        ExprKind::ExprCall { callee, args } => {
            discover_expr(callee, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method { object, .. }) => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => {}
    }
    Ok(())
}

fn discover_exprs(
    exprs: &[Expr],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for expr in exprs {
        discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
    }
    Ok(())
}

fn discover_instanceof_target(
    target: &InstanceOfTarget,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match target {
        InstanceOfTarget::Name(_) => Ok(()),
        InstanceOfTarget::Expr(expr) => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)
        }
    }
}

fn constant_truthiness(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::Null => Some(false),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!(value.is_empty() || value == "0")),
        _ => None,
    }
}
