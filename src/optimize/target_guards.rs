//! Purpose:
//! Resolves statically known function names in target-availability guards before type checking.
//!
//! Called from:
//! - `crate::optimize::control::fold::fold_block()`
//!
//! Key details:
//! - Only availability arguments are substituted; ordinary expressions retain their checker shape.
//! - Shared write invalidation and reference volatility prevent stale names from pruning branches.

use super::*;

/// Starts an independent callable scope while excluding reference parameters and captures.
pub(super) fn fold_callable_body<'a>(
    body: Vec<Stmt>,
    references: impl Iterator<Item = &'a str>,
) -> Vec<Stmt> {
    if active_fold_target().is_none() {
        return crate::optimize::generator_bodies::rewrite_preserving_yield(body, fold_block);
    }
    with_fresh_reference_volatile(|| {
        for name in references {
            mark_reference_volatile(name);
        }
        crate::optimize::generator_bodies::rewrite_preserving_yield(body, fold_block)
    })
}

/// String facts established in execution order within one statement block.
#[derive(Clone, Default)]
pub(super) struct GuardValues {
    variables: HashMap<String, String>,
    constants: HashMap<String, String>,
}

impl GuardValues {
    /// Resolves guard arguments, including nested branches that inherit the current facts.
    pub(super) fn prepare(&self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let mut values = self.clone();
                values.invalidate(expr_invalidation(condition));
                values.rewrite_condition(condition);
                values.prepare_block(then_body);
                for (condition, body) in elseif_clauses {
                    values.invalidate(expr_invalidation(condition));
                    values.rewrite_condition(condition);
                    values.prepare_block(body);
                }
                if let Some(body) = else_body {
                    values.prepare_block(body);
                }
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
                self.prepare_block(body);
            }
            _ => {}
        }
    }

    /// Carries facts through a nested block without leaking its writes into sibling branches.
    fn prepare_block(&self, body: &mut [Stmt]) {
        let mut values = self.clone();
        for stmt in body {
            values.prepare(stmt);
            values.observe(stmt);
        }
    }

    /// Records string assignments and discards facts that a statement can mutate or alias.
    pub(super) fn observe(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                self.invalidate(expr_invalidation(value));
                let string = self.string_value(value);
                self.variables.remove(name);
                if !is_reference_volatile(name) {
                    if let Some(string) = string {
                        self.variables.insert(name.clone(), string);
                    }
                }
            }
            StmtKind::ConstDecl { name, value } => {
                self.invalidate(expr_invalidation(value));
                if let Some(string) = self.string_value(value) {
                    self.constants.entry(name.clone()).or_insert(string);
                }
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
                for stmt in body {
                    self.observe(stmt);
                }
            }
            _ => self.invalidate(stmt_invalidation(stmt)),
        }
    }

    /// Removes written values; shared invalidation records persistent reference exclusions.
    fn invalidate(&mut self, invalidation: Invalidation) {
        match invalidation {
            Invalidation::All => self.variables.clear(),
            Invalidation::Names(names) => {
                for name in names {
                    self.variables.remove(&name);
                }
            }
        }
    }

    /// Evaluates only side-effect-free string expressions with established inputs.
    fn string_value(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::StringLiteral(value) => Some(value.clone()),
            ExprKind::Variable(name) if !is_reference_volatile(name) => {
                self.variables.get(name).cloned()
            }
            ExprKind::ConstRef(name) => self.constants.get(&name.as_canonical()).cloned(),
            ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
                Some(self.string_value(left)? + &self.string_value(right)?)
            }
            _ => None,
        }
    }

    /// Substitutes known registry names without folding unrelated variable reads or calls.
    fn rewrite_condition(&self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::FunctionCall { name, args }
                if name.as_canonical().trim_start_matches('\\')
                    .eq_ignore_ascii_case("function_exists") =>
            {
                if let [argument] = args.as_mut_slice() {
                    if let Some(candidate) = self.string_value(argument) {
                        if crate::builtins::registry::lookup(candidate.trim_start_matches('\\')).is_some()
                            || crate::name_resolver::is_global_date_procedural_alias(candidate.trim_start_matches('\\'))
                        {
                            argument.kind = ExprKind::StringLiteral(candidate);
                        }
                    }
                }
            }
            ExprKind::BinaryOp { left, right, .. } => {
                self.rewrite_condition(left);
                self.rewrite_condition(right);
            }
            ExprKind::Not(inner) | ExprKind::Cast { expr: inner, .. }
            | ExprKind::ErrorSuppress(inner) => self.rewrite_condition(inner),
            ExprKind::Ternary { condition, then_expr, else_expr } => {
                self.rewrite_condition(condition);
                self.rewrite_condition(then_expr);
                self.rewrite_condition(else_expr);
            }
            ExprKind::ShortTernary { value, default } => {
                self.rewrite_condition(value);
                self.rewrite_condition(default);
            }
            _ => {}
        }
    }
}
