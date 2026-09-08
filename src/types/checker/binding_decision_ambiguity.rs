//! Purpose:
//! Rejects a local-binding decision whose `(span, name)` key names more than ONE syntactic site in
//! the program, so such a decision can never lower as a re-bind.
//!
//! Called from:
//! - `crate::types::checker::check_types_with_options`, once, after every checker walk has settled.
//!
//! Key details:
//! - `CheckResult::local_bind_kill_sites` / `local_retype_sites` / `mixed_storage_store_sites` are
//!   keyed by `(Span, local name)`
//!   and EIR lowering consults them by that key. A `Span` carries line/column and NOTHING about
//!   which FILE they are in, and include resolution splices every included file's statements into
//!   one program WITHOUT rebasing line numbers (`resolver::engine_includes` rewrites no spans). So
//!   line 4 column 1 of `main.php` and of `lib.php` are equal `Span`s. Adding the local's name
//!   removes almost every collision; what it cannot remove is the same NAME at the same position
//!   in two files.
//! - Abandoning a binding is not a no-op: it releases the old value and re-binds the name to a
//!   fresh frame slot. Firing that at a site the checker never approved produces a wrong answer
//!   with no diagnostic — measured on a two-file fixture as printing `|s` where PHP prints `a1|s`,
//!   and that program was a plain compile error before this feature existed.
//! - File identity in `Span` is the real fix and is out of scope here. The binding invariant this
//!   pass delivers instead is NO SILENT WRONG OUTPUT: an ambiguous key is a hard compile error, so
//!   the checker and lowering stay in lock-step on exactly which programs are accepted.
//! - The scan counts MATCHING NODES, not decisions. One decision matching two nodes is the hazard;
//!   one node visited by several checker walks is not (the walks re-decide one AST node).
//! - The scan covers `program` ONLY. An `eval` fragment is parsed from a string literal into its
//!   own span space (line 1 of that string), which no walk here reaches, so a fragment node can
//!   neither raise a collision nor be protected by one. Nothing is missing: the eval-AOT lowerers
//!   receive EMPTY decision maps (`ir_lower::function::eval_aot_decision_maps`), so a fragment
//!   never consults a decision this pass could have been asked to arbitrate.
//! - RETIRED mixed-storage keys are checked alongside the live ones. A re-decision REMOVES the
//!   decisions filed against the sites it is about to re-decide, and the removal matches by
//!   `(span, name)` — so one body's scan can strip another body's decision and leave this pass
//!   nothing to look at. Checking the retired keys restores the invariant: a key that was
//!   legitimately re-decided names ONE node and passes, while a key retired by collision names
//!   two or more and is rejected here, exactly as a live one would be.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{
    CatchClause, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Program, Stmt, StmtKind,
    TypeExpr,
};
use crate::span::Span;

use super::is_unset_call;

/// Rejects every recorded decision whose key matches more than one node OF ITS OWN ROLE.
///
/// All three maps are keyed the same way and are walked in one pass, but each is checked against
/// the tally for the node kind it names, never against another kind's. That matches how lowering
/// consults them and is not a weakening: `lower_assign` reads the retype decisions and fires only
/// at a `StmtKind::Assign`, `unset_local` reads the kill decisions and fires only at an `unset`
/// argument. A retype and a kill filed under one span for one name could therefore never make
/// either site re-bind for the other's reason, so cross-role coincidence is not a hazard to detect.
///
/// The MIXED-STORAGE store sites share the retype tally rather than getting one of their own,
/// because they name the same node kind: both are statement-form assignments, and both are read
/// at a `StmtKind::Assign` during lowering. Giving them a separate tally would count the same
/// nodes twice over and change nothing, while lumping them with the `unset` arguments would be
/// wrong. A span shared by a retype and a mixed store site for one name cannot happen — a marked
/// name binds `Mixed`, so its assignments always merge and never record a retype — and if it
/// somehow did, both decisions would still name that one assignment.
///
/// `retired_mixed_storage_store_sites` carries the mixed-storage keys a later scan REMOVED. They
/// are checked against the assignment tally like the live ones, because a removal matched by
/// `(span, name)` cannot tell "this walk re-decided its own site" from "another body's site
/// happens to sit at the same line and column with the same local name". The first names one node
/// and passes; the second names two and is the hazard — measured, before this was checked, as a
/// compiler PANIC (`strlen cannot lower checked operand type Int`) on valid PHP, because
/// `CheckResult::mixed_storage_local_names` is derived from the map that had just been emptied.
///
/// The kill and retype maps are re-decided the same way and are deliberately NOT given a retired
/// set. Losing one of those costs only precision in LOWERING — the site falls back to the
/// null-store / slot-widening path it used before those decisions were lowered at all, which is
/// correct — whereas losing a mixed-storage decision changes what the rest of the compiler
/// believes about a local the checker still types `Mixed`. Rejecting the safe case for uniformity
/// would turn working programs into compile errors
/// (`test_stripped_retype_decision_still_compiles_and_runs` pins one).
pub(super) fn reject_ambiguous_local_binding_decisions(
    program: &Program,
    kill_sites: &HashMap<Span, HashSet<String>>,
    retype_sites: &HashMap<Span, HashSet<String>>,
    mixed_storage_store_sites: &HashMap<Span, HashSet<String>>,
    retired_mixed_storage_store_sites: &HashSet<(Span, String)>,
) -> Result<(), CompileError> {
    if kill_sites.is_empty()
        && retype_sites.is_empty()
        && mixed_storage_store_sites.is_empty()
        && retired_mixed_storage_store_sites.is_empty()
    {
        return Ok(());
    }
    // Only spans a decision was filed against are worth counting, and there are a handful of them
    // in a whole program. Everything else short-circuits on this set lookup.
    let mut tally = Tally {
        watched: kill_sites
            .keys()
            .chain(retype_sites.keys())
            .chain(mixed_storage_store_sites.keys())
            .chain(
                retired_mixed_storage_store_sites
                    .iter()
                    .map(|(span, _)| span),
            )
            .copied()
            .collect(),
        assigns: HashMap::new(),
        unset_args: HashMap::new(),
    };
    count_block(program, &mut tally);
    // Counted BY ROLE, matching how lowering consults each map: `lower_assign` reads the retype
    // and mixed-storage decisions at a statement-form assignment, `unset_local` reads the kill
    // decisions at an `unset` argument. Lumping the roles together is wrong rather than merely
    // imprecise — the parser gives `$x .= "a"` a synthesized `$x` READ at the very span of the
    // assignment statement, so one ordinary compound assignment would look like two colliding
    // sites.
    // EVERY decision map carries a SET of names per span (two different locals can be killed,
    // re-bound or marked at the same line and column in two different files), and each name is its
    // own decision to check against the tally.
    let decisions = retype_sites
        .iter()
        .flat_map(|(span, names)| names.iter().map(move |name| (span, name)))
        .chain(
            mixed_storage_store_sites
                .iter()
                .flat_map(|(span, names)| names.iter().map(move |name| (span, name))),
        )
        // A retired mixed-storage key names the same node kind a live one does, so it is checked
        // against the same tally. A key that is both live and retired (re-decided, then recorded
        // again) is simply checked twice, with the same answer.
        .chain(
            retired_mixed_storage_store_sites
                .iter()
                .map(|(span, name)| (span, name)),
        )
        .map(|(span, name)| (span, name, &tally.assigns))
        .chain(
            kill_sites
                .iter()
                .flat_map(|(span, names)| names.iter().map(move |name| (span, name)))
                .map(|(span, name)| (span, name, &tally.unset_args)),
        );
    for (span, name, sites) in decisions {
        let count = sites.get(&(*span, name.clone())).copied().unwrap_or(0);
        if count > 1 {
            // ONE message for both shapes, deliberately. Structural equality of the sites would
            // hint at "same file included twice", but it does not PROVE it — two different files
            // can hold identical code at identical positions — so a message that guessed would be
            // wrong exactly where it sounded most confident. Naming both causes is accurate in
            // every case and tells the reader what to look for.
            return Err(CompileError::new(
                *span,
                &format!(
                    "Cannot re-bind ${} here: {} statements in this program sit at line {} \
                     column {} and name ${}. elephc identifies a local binding decision (an \
                     `unset`, an incompatible reassignment, or a branch-divergent assignment \
                     compiled as boxed mixed storage) by source position, and a position carries \
                     no file, so it cannot tell them apart. Either two files have a statement at the \
                     same line and column, or the same file is included more than once (every \
                     `require`/`include` splices its statements in again). Keep the two \
                     assignments type-compatible, or move one statement to a different line or \
                     column.",
                    name,
                    count,
                    span.line,
                    span.col,
                    name,
                ),
            ));
        }
    }
    Ok(())
}

/// Counts, for each watched key, how many nodes in the block carry it.
fn count_block(stmts: &[Stmt], tally: &mut Tally) {
    for stmt in stmts {
        count_stmt(stmt, tally);
    }
}

/// Counts one statement and everything nested inside it.
///
/// Exhaustive on purpose: a new `StmtKind` must fail to compile here rather than silently stop
/// being counted, which would let an ambiguous decision through.
fn count_stmt(stmt: &Stmt, tally: &mut Tally) {
    // A RETYPE decision is filed against the statement-form assignment's own span.
    if let StmtKind::Assign { name, .. } = &stmt.kind {
        bump_assign(tally, stmt.span, name);
    }
    match &stmt.kind {
        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            count_expr(expr, tally);
        }
        StmtKind::Assign { name: _, value } => count_expr(value, tally),
        StmtKind::RefAssign { target: _, source } => count_expr(source, tally),
        StmtKind::TypedAssign { type_expr: _, name: _, value } => count_expr(value, tally),
        StmtKind::ConstDecl { name: _, value } => count_expr(value, tally),
        StmtKind::StaticVar { name: _, init } => count_expr(init, tally),
        StmtKind::ListUnpack { vars: _, value } => count_expr(value, tally),
        StmtKind::ArrayAssign { array: _, index, value } => {
            count_expr(index, tally);
            count_expr(value, tally);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            count_expr(target, tally);
            count_expr(value, tally);
        }
        StmtKind::ArrayPush { array: _, value } => count_expr(value, tally),
        StmtKind::PropertyAssign { object, property: _, value } => {
            count_expr(object, tally);
            count_expr(value, tally);
        }
        StmtKind::PropertyArrayPush { object, property: _, value } => {
            count_expr(object, tally);
            count_expr(value, tally);
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => {
            count_expr(object, tally);
            count_expr(property, tally);
            count_expr(value, tally);
        }
        StmtKind::PropertyArrayAssign { object, property: _, index, value } => {
            count_expr(object, tally);
            count_expr(index, tally);
            count_expr(value, tally);
        }
        StmtKind::StaticPropertyAssign { receiver: _, property: _, value }
        | StmtKind::StaticPropertyArrayPush { receiver: _, property: _, value } => {
            count_expr(value, tally);
        }
        StmtKind::StaticPropertyArrayAssign { receiver: _, property: _, index, value } => {
            count_expr(index, tally);
            count_expr(value, tally);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                count_expr(value, tally);
            }
        }
        StmtKind::Include { path, once: _, required: _ } => count_expr(path, tally),
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { name: _, body }
        | StmtKind::IncludeOnceGuard { label: _, body } => count_block(body, tally),
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            count_expr(condition, tally);
            count_block(body, tally);
        }
        StmtKind::For { init, condition, update, body } => {
            if let Some(init) = init {
                count_stmt(init, tally);
            }
            if let Some(condition) = condition {
                count_expr(condition, tally);
            }
            if let Some(update) = update {
                count_stmt(update, tally);
            }
            count_block(body, tally);
        }
        StmtKind::Foreach { array, key_var: _, value_var: _, value_by_ref: _, body } => {
            count_expr(array, tally);
            count_block(body, tally);
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            count_expr(condition, tally);
            count_block(then_body, tally);
            for (condition, body) in elseif_clauses {
                count_expr(condition, tally);
                count_block(body, tally);
            }
            if let Some(else_body) = else_body {
                count_block(else_body, tally);
            }
        }
        StmtKind::IfDef { symbol: _, then_body, else_body } => {
            count_block(then_body, tally);
            if let Some(else_body) = else_body {
                count_block(else_body, tally);
            }
        }
        StmtKind::Switch { subject, cases, default } => {
            count_expr(subject, tally);
            for (patterns, body) in cases {
                for pattern in patterns {
                    count_expr(pattern, tally);
                }
                count_block(body, tally);
            }
            if let Some(default) = default {
                count_block(default, tally);
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            count_block(try_body, tally);
            for CatchClause { exception_types: _, variable: _, body } in catches {
                count_block(body, tally);
            }
            if let Some(finally_body) = finally_body {
                count_block(finally_body, tally);
            }
        }
        // EVERY declaration that can hold statements is walked. A trait's methods are checked and
        // lowered exactly like a class's, so a decision recorded inside one is consulted inside one:
        // skipping traits (and enums, and interface default bodies) let a `trait T` in an included
        // file share a key with a retype in the main file and print `|5` for `a1|5`. The list below
        // is derived from the `StmtKind` definition rather than from memory, and the match stays
        // exhaustive so a new declaration variant has to be classified here rather than defaulting
        // to "contains nothing".
        StmtKind::FunctionDecl { params, body, .. } => {
            count_param_defaults(params, tally);
            count_block(body, tally);
        }
        StmtKind::ClassDecl { properties, methods, constants, .. }
        | StmtKind::InterfaceDecl { properties, methods, constants, .. }
        | StmtKind::TraitDecl { properties, methods, constants, .. } => {
            count_class_members(properties, methods, constants, tally);
        }
        StmtKind::EnumDecl { cases, methods, constants, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    count_expr(value, tally);
                }
            }
            count_class_members(&[], methods, constants, tally);
        }
        // A packed class declares typed FIELDS only — no defaults, no bodies, no expressions.
        StmtKind::PackedClassDecl { name: _, fields: _ }
        // Externs are C declarations: types and names, never PHP statements.
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        // Leaves: no sub-statements and no sub-expressions.
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Global { vars: _ }
        | StmtKind::IncludeOnceMark { label: _ }
        | StmtKind::NamespaceDecl { name: _ }
        | StmtKind::UseDecl { imports: _ }
        // Variant groups/marks carry function NAMES; the bodies live in their own `FunctionDecl`s.
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
    }
}

/// Counts the statement- and expression-bearing members shared by every class-like declaration.
fn count_class_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    constants: &[ClassConst],
    tally: &mut Tally,
) {
    for property in properties {
        if let Some(default) = &property.default {
            count_expr(default, tally);
        }
    }
    for method in methods {
        count_param_defaults(&method.params, tally);
        count_block(&method.body, tally);
    }
    for constant in constants {
        count_expr(&constant.value, tally);
    }
}

/// Counts the default-value expressions of a parameter list.
fn count_param_defaults(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    tally: &mut Tally,
) {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            count_expr(default, tally);
        }
    }
}

/// Counts one expression and everything nested inside it.
///
/// A KILL decision is filed against the `unset` ARGUMENT's span. Only the arguments of an actual
/// `unset` call are counted, not every `Variable` node: an ordinary read of the same name at the
/// same position is not a site lowering could ever re-bind, and counting it would reject valid
/// programs.
fn count_expr(expr: &Expr, tally: &mut Tally) {
    // A KILL decision is filed against the `unset` ARGUMENT's span. Only arguments of an actual
    // `unset` call qualify, so a plain read of the same variable elsewhere is not a rival site.
    if let ExprKind::FunctionCall { name, args } = &expr.kind {
        if is_unset_call(name.as_str()) {
            for arg in args {
                if let ExprKind::Variable(arg_name) = &arg.kind {
                    bump_unset_arg(tally, arg.span, arg_name);
                }
            }
        }
    }
    match &expr.kind {
        ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp: _ } => {
            count_expr(target, tally);
            count_expr(value, tally);
            if let Some(result_target) = result_target {
                count_expr(result_target, tally);
            }
            count_block(prelude, tally);
        }
        ExprKind::FunctionCall { name: _, args }
        | ExprKind::StaticMethodCall { receiver: _, method: _, args }
        | ExprKind::NewScopedObject { receiver: _, args }
        | ExprKind::NewObject { class_name: _, args }
        | ExprKind::ClosureCall { var: _, args } => count_exprs(args, tally),
        ExprKind::MethodCall { object, method: _, args }
        | ExprKind::NullsafeMethodCall { object, method: _, args } => {
            count_expr(object, tally);
            count_exprs(args, tally);
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            count_expr(object, tally);
            count_expr(method, tally);
            count_exprs(args, tally);
        }
        ExprKind::ExprCall { callee, args } => {
            count_expr(callee, tally);
            count_exprs(args, tally);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            count_expr(name_expr, tally);
            count_exprs(args, tally);
        }
        ExprKind::NewDynamicObject { class_name, fallback_class: _, required_parent: _, args } => {
            count_expr(class_name, tally);
            count_exprs(args, tally);
        }
        ExprKind::Pipe { value, callable } => {
            count_expr(value, tally);
            count_expr(callable, tally);
        }
        ExprKind::IncludeValue { path, once: _, required: _ } => count_expr(path, tally),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                count_expr(key, tally);
            }
            if let Some(value) = value {
                count_expr(value, tally);
            }
        }
        ExprKind::Closure { params, body, .. } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    count_expr(default, tally);
                }
            }
            count_block(body, tally);
        }
        ExprKind::BinaryOp { left, op: _, right } => {
            count_expr(left, tally);
            count_expr(right, tally);
        }
        ExprKind::InstanceOf { value, target } => {
            count_expr(value, tally);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                count_expr(target, tally);
            }
        }
        ExprKind::YieldFrom(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { target: _, expr: inner }
        | ExprKind::PtrCast { target_type: _, expr: inner }
        | ExprKind::NamedArg { name: _, value: inner }
        | ExprKind::BufferNew { element_type: _, len: inner } => count_expr(inner, tally),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            count_expr(value, tally);
            count_expr(default, tally);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            count_expr(condition, tally);
            count_expr(then_expr, tally);
            count_expr(else_expr, tally);
        }
        ExprKind::Match { subject, arms, default } => {
            count_expr(subject, tally);
            for (patterns, body) in arms {
                count_exprs(patterns, tally);
                count_expr(body, tally);
            }
            if let Some(default) = default {
                count_expr(default, tally);
            }
        }
        ExprKind::ArrayLiteral(items) => count_exprs(items, tally),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                count_expr(key, tally);
                count_expr(value, tally);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            count_expr(array, tally);
            count_expr(index, tally);
        }
        ExprKind::PropertyAccess { object, property: _ }
        | ExprKind::NullsafePropertyAccess { object, property: _ }
        | ExprKind::ObjectClassName { object } => count_expr(object, tally),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            count_expr(object, tally);
            count_expr(property, tally);
        }
        ExprKind::FirstClassCallable(target) => {
            if let crate::parser::ast::CallableTarget::Method { object, method: _ } = target {
                count_expr(object, tally);
            }
        }
        ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}

/// Counts a list of expressions.
fn count_exprs(exprs: &[Expr], tally: &mut Tally) {
    for expr in exprs {
        count_expr(expr, tally);
    }
}

/// Records one more statement-form assignment to `name` at `span`.
fn bump_assign(tally: &mut Tally, span: Span, name: &str) {
    if !tally.watched.contains(&span) {
        return;
    }
    *tally.assigns.entry((span, name.to_string())).or_insert(0) += 1;
}

/// Records one more `unset` argument naming `name` at `span`.
fn bump_unset_arg(tally: &mut Tally, span: Span, name: &str) {
    if !tally.watched.contains(&span) {
        return;
    }
    *tally.unset_args.entry((span, name.to_string())).or_insert(0) += 1;
}

/// The watched spans plus the per-ROLE `(span, name)` node counts collected for them.
struct Tally {
    /// Spans a decision was filed against; every other node short-circuits here.
    watched: HashSet<Span>,
    /// Statement-form assignments, the only nodes a RETYPE decision can name.
    assigns: HashMap<(Span, String), usize>,
    /// `unset` arguments, the only nodes a KILL decision can name.
    unset_args: HashMap<(Span, String), usize>,
}
