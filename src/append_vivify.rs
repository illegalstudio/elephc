//! Purpose:
//! Finds the locals a scope CREATES by writing an array element into them — `$keys[] = $k` or
//! `$rows['id'] = $v` against a name nothing has assigned yet. PHP auto-vivifies those: the write
//! itself makes the array.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function`, which stores an empty array into each
//!   one in the entry block before lowering the body.
//! - `crate::types::checker::stmt_check::assignments::arrays`, which reaches the same answer one
//!   statement at a time: it binds the name when the write finds it unbound, which is exactly the
//!   "first binding is a vivifying write" rule this scan applies textually.
//!
//! Key details:
//! - The initialization has to be HOISTED to the scope entry. `foreach (...) { $keys[] = $k; }`
//!   creates the array once, on the first iteration; storing it at the write site would reset it
//!   on every later one.
//! - Only a name whose FIRST binding in the scope is such a write qualifies. A name that is
//!   assigned first (`$a = f(); … $a[] = 1;`) already has storage and must keep the assigned
//!   value — and its assigned TYPE, which an entry store of `[]` would otherwise retype.
//! - The scan walks one scope. It does not descend into a nested function, class or closure body,
//!   which are scopes of their own, but a closure's by-REFERENCE captures (`use (&$x)`) do bind
//!   `$x` here, so they count as explicit bindings.
//! - A name that is never written this way is not returned, so a scope with no such local pays
//!   one AST walk and nothing else.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::checker::loop_storage::visit_child_expressions;

/// Returns the scope locals whose first binding is an auto-vivifying array write, in the order
/// their writes appear.
///
/// `params` are already bound on entry, so a write to one of them is never a vivification.
pub(crate) fn vivified_array_locals<'a>(
    body: &[Stmt],
    params: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut scan = Scan {
        bound: params.into_iter().map(str::to_string).collect(),
        vivified: Vec::new(),
        non_integer_keyed: HashSet::new(),
    };
    scan.bound.insert("this".to_string());
    scan.block(body);
    scan.vivified
}

/// Returns the names this scope WRITES through an array key that is not an integer literal.
///
/// A `static $m = [];` whose body only appends (`$m[] = …`) or writes integer-literal keys is an
/// indexed queue, and its storage can be widened to `array<mixed>` at the declaration — which is
/// what stops `array_shift()` on it from returning `NULL`. One whose body writes a STRING or
/// COMPUTED key is a hash in the making, and `[]` still infers as `Array(never)`: widening it to
/// `array<mixed>` would pin INDEXED storage on a slot whose first string-keyed write promotes it
/// to a hash at runtime. The promotion works; the slot TYPE is what goes stale, and a later read
/// of the same static then reads hash storage through an indexed-array header. So those statics
/// are left exactly as they were — not correct either (`tests/static_local_array_tests.rs` has
/// the repro) but unchanged, which is the only safe answer a syntactic scan can give.
///
/// Deliberately over-approximate: `$m[$i] = …` with an integer `$i` counts as a non-integer key,
/// because nothing here can type `$i`. WRITES only — a read never decides storage, so `$q[$i]` on
/// a right-hand side leaves `$q` eligible.
///
/// Walks one scope, exactly as [`vivified_array_locals`] does, and does not descend into a nested
/// function, class or closure body: each is lowered as its own body and asks this for itself.
pub(crate) fn locals_written_with_non_integer_keys(body: &[Stmt]) -> HashSet<String> {
    let mut scan = Scan {
        bound: HashSet::new(),
        vivified: Vec::new(),
        non_integer_keyed: HashSet::new(),
    };
    scan.block(body);
    scan.non_integer_keyed
}

/// Returns the base variable name an array-access chain is rooted at.
fn array_access_root_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => array_access_root_name(array),
        _ => None,
    }
}

/// Tracks which names a scope has bound so far and which of them an array write created.
struct Scan {
    /// Every name bound so far, however it was bound.
    bound: HashSet<String>,
    /// Names whose first binding was an array element write, in source order.
    vivified: Vec<String>,
    /// Names this scope writes through an array key that is not an integer literal.
    ///
    /// Collected by the same walk because it asks the same question of the same nodes. See
    /// [`locals_written_with_non_integer_keys`] for what consumes it.
    non_integer_keyed: HashSet<String>,
}

impl Scan {
    /// Records a binding that supplies its own value, so a later array write finds storage.
    fn bind_explicit(&mut self, name: &str) {
        if !self.bound.contains(name) {
            self.bound.insert(name.to_string());
        }
    }

    /// Records an array element write, which creates the array only when nothing bound the name.
    fn bind_vivify(&mut self, name: &str) {
        if self.bound.insert(name.to_string()) {
            self.vivified.push(name.to_string());
        }
    }

    /// Walks a statement list in source order.
    fn block(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.stmt(statement);
        }
    }

    /// Walks bodies that are ALTERNATIVES of one another -- the arms of an `if`/`elseif`/`else`
    /// chain, the cases of a `switch`.
    ///
    /// Only one of them runs, so a binding made in one arm does not exist when a sibling arm
    /// starts: `if ($a) { $r = [1]; } else { $r[$k] = $v; }` CREATES `$r` in the `else`. Every
    /// arm therefore starts from the names bound before the chain, and the names bound by any
    /// arm are visible only afterwards. Walking the arms in plain source order made the
    /// `else` look like a write to an existing array, so the name was never vivified and the
    /// write hit an unbound slot.
    fn alternatives(&mut self, bodies: &[&[Stmt]]) {
        let before = self.bound.clone();
        let mut after = before.clone();
        for body in bodies {
            self.bound = before.clone();
            self.block(body);
            after.extend(self.bound.drain());
        }
        self.bound = after;
    }

    /// Walks one statement: its expressions first, then the names it binds.
    fn stmt(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Assign { name, value } => {
                self.expr(value);
                self.bind_explicit(name);
            }
            StmtKind::TypedAssign { name, value, .. } => {
                self.expr(value);
                self.bind_explicit(name);
            }
            StmtKind::RefAssign { target, source } => {
                self.expr(source);
                self.bind_explicit(target);
            }
            StmtKind::ArrayPush { array, value } => {
                self.expr(value);
                self.bind_vivify(array);
            }
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } => {
                if !matches!(index.kind, ExprKind::IntLiteral(_)) {
                    self.non_integer_keyed.insert(array.clone());
                }
                self.expr(index);
                self.expr(value);
                self.bind_vivify(array);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                // A nested write reaches storage through an inner container this scan cannot
                // type, so its root is never eligible for the declaration-time widening.
                if let Some(root) = array_access_root_name(target) {
                    self.non_integer_keyed.insert(root.to_string());
                }
                self.expr(target);
                self.expr(value);
            }
            StmtKind::ListUnpack { vars, value } => {
                self.expr(value);
                for name in vars {
                    self.bind_explicit(name);
                }
            }
            StmtKind::Global { vars } => {
                for name in vars {
                    self.bind_explicit(name);
                }
            }
            StmtKind::StaticVar { name, init } => {
                self.expr(init);
                self.bind_explicit(name);
            }
            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::ConstDecl { value: expr, .. }
            | StmtKind::Include { path: expr, .. } => self.expr(expr),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.expr(condition);
                for (condition, _) in elseif_clauses {
                    self.expr(condition);
                }
                let mut arms: Vec<&[Stmt]> = vec![then_body.as_slice()];
                arms.extend(elseif_clauses.iter().map(|(_, body)| body.as_slice()));
                if let Some(body) = else_body {
                    arms.push(body.as_slice());
                }
                self.alternatives(&arms);
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                self.block(then_body);
                if let Some(body) = else_body {
                    self.block(body);
                }
            }
            StmtKind::While { condition, body } => {
                self.expr(condition);
                self.block(body);
            }
            StmtKind::DoWhile { body, condition } => {
                self.block(body);
                self.expr(condition);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.stmt(init);
                }
                if let Some(condition) = condition {
                    self.expr(condition);
                }
                if let Some(update) = update {
                    self.stmt(update);
                }
                self.block(body);
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
                ..
            } => {
                self.expr(array);
                if let Some(key_var) = key_var {
                    self.bind_explicit(key_var);
                }
                self.bind_explicit(value_var);
                self.block(body);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.expr(subject);
                for (conditions, _) in cases {
                    for condition in conditions {
                        self.expr(condition);
                    }
                }
                let mut arms: Vec<&[Stmt]> = cases.iter().map(|(_, body)| body.as_slice()).collect();
                if let Some(body) = default {
                    arms.push(body.as_slice());
                }
                self.alternatives(&arms);
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                self.block(try_body);
                for catch in catches {
                    if let Some(var) = catch.variable.as_ref() {
                        self.bind_explicit(var);
                    }
                    self.block(&catch.body);
                }
                if let Some(body) = finally_body {
                    self.block(body);
                }
            }
            StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. } => self.block(body),
            StmtKind::PropertyAssign { object, value, .. }
            | StmtKind::PropertyRefAssign {
                object,
                source: value,
                ..
            }
            | StmtKind::PropertyArrayPush { object, value, .. } => {
                self.expr(object);
                self.expr(value);
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => {
                self.expr(object);
                self.expr(index);
                self.expr(value);
            }
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => self.expr(value),
            StmtKind::StaticPropertyArrayAssign { index, value, .. }
            | StmtKind::StaticPropertyElementRefAssign {
                index,
                source: value,
                ..
            } => {
                self.expr(index);
                self.expr(value);
            }
            StmtKind::DynamicStaticPropertyWrite {
                property,
                index,
                value,
                ..
            } => {
                self.expr(property);
                if let Some(index) = index {
                    self.expr(index);
                }
                self.expr(value);
            }
            // A nested function, class-like or compiler declaration opens its own scope, and the
            // remaining variants bind nothing.
            _ => {}
        }
    }

    /// Walks an expression, recording the names it binds without entering a nested closure body.
    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                self.block(prelude);
                if let ExprKind::ArrayAccess { array, index } = &target.kind {
                    if !matches!(index.kind, ExprKind::IntLiteral(_)) {
                        if let Some(root) = array_access_root_name(array) {
                            self.non_integer_keyed.insert(root.to_string());
                        }
                    }
                }
                self.expr(value);
                self.expr(target);
                if let ExprKind::Variable(name) = &target.kind {
                    self.bind_explicit(name);
                }
                if let Some(result_target) = result_target {
                    self.expr(result_target);
                    if let ExprKind::Variable(name) = &result_target.kind {
                        self.bind_explicit(name);
                    }
                }
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => self.bind_explicit(name),
            // A by-reference capture binds the name in THIS scope — PHP creates it as null when
            // the closure is made. By-value captures only read it.
            ExprKind::Closure { capture_refs, .. } => {
                for name in capture_refs {
                    self.bind_explicit(name);
                }
            }
            _ => visit_child_expressions(expr, &mut |child| self.expr(child)),
        }
    }
}
