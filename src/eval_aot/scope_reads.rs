//! Purpose:
//! Collects caller-scope reads before definite local writes.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Branch merges retain only variables assigned along every path.

use super::*;

/// Variable read/write metadata collected from a parsed eval fragment.
pub(super) struct EvalScopeAccess {
    pub(super) reads: BTreeSet<String>,
    pub(super) warning_reads: BTreeSet<String>,
    pub(super) writes: BTreeSet<String>,
    pub(super) creates_unknown_vars: bool,
}

impl EvalScopeAccess {
    /// Creates an empty eval scope access accumulator.
    pub(super) fn new() -> Self {
        Self {
            reads: BTreeSet::new(),
            warning_reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            creates_unknown_vars: false,
        }
    }

    /// Returns true when the fragment touches any eval-visible variable storage.
    pub(super) fn has_scope_access(&self) -> bool {
        !self.reads.is_empty() || !self.writes.is_empty() || self.creates_unknown_vars
    }

    /// Records a variable read.
    pub(super) fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
        self.warning_reads.insert(name.to_string());
    }

    /// Records a read whose missing-variable warning PHP suppresses.
    pub(super) fn quiet_read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }

    /// Records a variable write.
    pub(super) fn write(&mut self, name: &str) {
        self.writes.insert(name.to_string());
    }

    /// Marks an access shape that cannot be mapped to a static variable name.
    pub(super) fn unknown_write(&mut self) {
        self.creates_unknown_vars = true;
    }
}

/// Collects conservative eval-scope reads and writes from a parsed fragment.
pub(super) fn collect_scope_accesses(program: &[Stmt]) -> EvalScopeAccess {
    let mut access = EvalScopeAccess::new();
    for stmt in program {
        collect_stmt_scope_access(stmt, &mut access);
    }
    access
}

/// Collects variable reads that must come from the caller before local writes exist.
pub(super) fn collect_scope_reads_before_writes(program: &[Stmt]) -> BTreeSet<String> {
    let mut reads = BTreeSet::new();
    let mut assigned = BTreeSet::new();
    collect_block_scope_reads_before_writes(program, &mut assigned, &mut reads);
    reads
}

/// Collects caller reads across a statement block and tracks definite local writes.
pub(super) fn collect_block_scope_reads_before_writes(
    body: &[Stmt],
    assigned: &mut BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    for stmt in body {
        collect_stmt_scope_reads_before_writes(stmt, assigned, reads);
    }
}

/// Collects caller reads for one statement before updating local assignment facts.
pub(super) fn collect_stmt_scope_reads_before_writes(
    stmt: &Stmt,
    assigned: &mut BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr_scope_reads_before_writes(value, assigned, reads);
            assigned.insert(name.clone());
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr)) => {
            collect_expr_scope_reads_before_writes(expr, assigned, reads);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_scope_reads_before_writes(condition, assigned, reads);
            let before = assigned.clone();
            let mut branch_outputs = Vec::new();
            let mut then_assigned = before.clone();
            collect_block_scope_reads_before_writes(then_body, &mut then_assigned, reads);
            branch_outputs.push(then_assigned);
            for (condition, body) in elseif_clauses {
                collect_expr_scope_reads_before_writes(condition, &before, reads);
                let mut branch_assigned = before.clone();
                collect_block_scope_reads_before_writes(body, &mut branch_assigned, reads);
                branch_outputs.push(branch_assigned);
            }
            if let Some(else_body) = else_body {
                let mut else_assigned = before.clone();
                collect_block_scope_reads_before_writes(else_body, &mut else_assigned, reads);
                branch_outputs.push(else_assigned);
                retain_definitely_assigned_after_branches(assigned, before, &branch_outputs);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_scope_reads_before_writes(condition, assigned, reads);
            let mut body_assigned = assigned.clone();
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_scope_reads_before_writes(init, assigned, reads);
            }
            if let Some(condition) = condition {
                collect_expr_scope_reads_before_writes(condition, assigned, reads);
            }
            let mut body_assigned = assigned.clone();
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
            if let Some(update) = update {
                collect_stmt_scope_reads_before_writes(update, &mut body_assigned, reads);
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            collect_expr_scope_reads_before_writes(array, assigned, reads);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            let mut body_assigned = assigned.clone();
            body_assigned.insert(value_var.clone());
            if let Some(key_var) = key_var {
                body_assigned.insert(key_var.clone());
            }
            collect_block_scope_reads_before_writes(body, &mut body_assigned, reads);
            if expr_is_non_empty_static_array_literal_source(array) {
                assigned.insert(value_var.clone());
                if let Some(key_var) = key_var {
                    assigned.insert(key_var.clone());
                }
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_scope_reads_before_writes(subject, assigned, reads);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_scope_reads_before_writes(condition, assigned, reads);
                }
                let mut case_assigned = assigned.clone();
                collect_block_scope_reads_before_writes(body, &mut case_assigned, reads);
            }
            if let Some(default) = default {
                let mut default_assigned = assigned.clone();
                collect_block_scope_reads_before_writes(default, &mut default_assigned, reads);
            }
        }
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
            collect_block_scope_reads_before_writes(body, assigned, reads);
        }
        _ => {
            let mut access = EvalScopeAccess::new();
            collect_stmt_scope_access(stmt, &mut access);
            extend_reads_not_assigned(reads, assigned, access.reads);
            assigned.extend(access.writes);
        }
    }
}

/// Keeps only names assigned on every branch after an if/elseif/else chain.
pub(super) fn retain_definitely_assigned_after_branches(
    assigned: &mut BTreeSet<String>,
    before: BTreeSet<String>,
    branch_outputs: &[BTreeSet<String>],
) {
    let mut definitely = before;
    for name in branch_outputs
        .first()
        .into_iter()
        .flat_map(|branch| branch.iter())
    {
        if branch_outputs.iter().all(|branch| branch.contains(name)) {
            definitely.insert(name.clone());
        }
    }
    *assigned = definitely;
}

/// Collects caller reads from one expression using current assignment facts.
pub(super) fn collect_expr_scope_reads_before_writes(
    expr: &Expr,
    assigned: &BTreeSet<String>,
    reads: &mut BTreeSet<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if !assigned.contains(name) {
                reads.insert(name.clone());
            }
        }
        ExprKind::Assignment {
            prelude,
            target,
            value,
            result_target,
            ..
        } => {
            let mut expr_assigned = assigned.clone();
            for stmt in prelude {
                collect_stmt_scope_reads_before_writes(stmt, &mut expr_assigned, reads);
            }
            collect_expr_scope_reads_before_writes(value, &expr_assigned, reads);
            match &target.kind {
                ExprKind::Variable(name) => {
                    expr_assigned.insert(name.clone());
                }
                _ => collect_expr_scope_reads_before_writes(target, &expr_assigned, reads),
            }
            if let Some(result_target) = result_target {
                collect_expr_scope_reads_before_writes(result_target, &expr_assigned, reads);
            }
        }
        _ => {
            let mut access = EvalScopeAccess::new();
            collect_expr_scope_access(expr, &mut access);
            extend_reads_not_assigned(reads, assigned, access.reads);
        }
    }
}

/// Adds collected reads that are not already definitely local to this fragment.
pub(super) fn extend_reads_not_assigned(
    reads: &mut BTreeSet<String>,
    assigned: &BTreeSet<String>,
    names: BTreeSet<String>,
) {
    reads.extend(names.into_iter().filter(|name| !assigned.contains(name)));
}
