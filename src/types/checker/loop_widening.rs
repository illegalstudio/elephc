//! Purpose:
//! Pre-scans a loop body for local-array growth/write sites whose element types widen an
//! indexed array to `mixed` across the loop back-edge (issue #452), so both the type
//! checker and EIR lowering can fix the array's element type to `mixed` *before*
//! processing the body once.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` (loop arms widen the `TypeEnv`).
//! - `crate::ir_lower::stmt` (loop lowering widens `local_types` and materializes the
//!   promotion before emitting the body).
//!
//! Key details:
//! - Both passes are single-pass over loop bodies; without this scan an early write/read site
//!   is typed/lowered against the pre-promotion element type and writes or reads an unboxed
//!   scalar in mixed-element storage on iterations >= 2, corrupting the heap.
//! - Growth/write sites covered by `loop_grown_mixed_array_pushes`: `$name[] =`, `$name[$i] =`,
//!   and `array_push($name, ...)`.
//! - Full reassignments `$name = [...]` are covered separately by `loop_reassigned_mixed_arrays`
//!   (issue #594): a rebind that rebuilds a raw-scalar array with a boxed `mixed` or tagged
//!   `int|null` element changes the read representation, so the local is widened before the body.
//! - Only the widening-to-`mixed` transition is reported: it is the one that changes the
//!   element representation (raw scalar slots vs boxed cells). Same-type growth and
//!   `never -> T` keep their current lowering.
//! - Concrete evidence comes from the caller's semantic expression inference, with a
//!   literal/cast fallback for lowering-only contexts. Opaque sources do *not* force
//!   `mixed` alone — that would spuriously widen same-typed rebuild loops such as
//!   `MultipleIterator::detachIterator`. Opaque evidence alongside at least one concrete
//!   sibling type on the same array does force `mixed`, because the back edge can promote
//!   storage while the opaque site still emits a raw write.

use crate::parser::ast::{CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::types::PhpType;

/// Evidence a growth/write site contributes to the element-type join.
enum PushEvidence {
    /// A self-evident concrete element type.
    Known(PhpType),
    /// A value whose static type is not safe to treat as concrete join evidence.
    Opaque,
}

/// Source recorded for a local assignment that can feed a later array write.
enum AssignedValue<'a> {
    /// An ordinary assignment whose RHS can be inferred by the caller.
    Expr(&'a Expr),
    /// A binding without an RHS expression in this AST, such as a `foreach` variable.
    Opaque,
}

/// Returns the names of locals that currently hold a non-`mixed` indexed array and whose
/// element type joins to `mixed` across the local-array growth/write sites found in the
/// loop body (and the optional `for` update statement). `lookup` supplies the current type
/// of a local at loop entry; names it does not know are skipped as targets. `infer_value`
/// supplies the caller's best semantic type for pushed values and assignment RHSs.
pub fn loop_grown_mixed_array_pushes(
    body: &[Stmt],
    update: Option<&Stmt>,
    lookup: &dyn Fn(&str) -> Option<PhpType>,
    infer_value: &mut dyn FnMut(&Expr) -> Option<PhpType>,
) -> Vec<String> {
    let mut pushes: Vec<(&str, &Expr)> = Vec::new();
    collect_array_pushes(body, &mut pushes);
    if let Some(stmt) = update {
        collect_array_push_stmt(stmt, &mut pushes);
    }
    let mut assignments: Vec<(&str, AssignedValue<'_>)> = Vec::new();
    collect_value_assignments(body, &mut assignments);
    if let Some(stmt) = update {
        collect_value_assignment_stmt(stmt, &mut assignments);
    }
    let mut names: Vec<String> = Vec::new();
    for (name, _) in &pushes {
        if names.iter().any(|n| n == name) {
            continue;
        }
        let Some(PhpType::Array(elem)) = lookup(name) else {
            continue;
        };
        if *elem == PhpType::Mixed {
            continue;
        }
        let mut joined = (*elem).clone();
        let mut saw_known = false;
        let mut saw_opaque = false;
        for (_, value) in pushes.iter().filter(|(n, _)| n == name) {
            match resolve_pushed_value_evidence(value, lookup, &assignments, infer_value) {
                Some(PushEvidence::Known(pushed)) => {
                    saw_known = true;
                    joined = join_pushed_element_type(joined, pushed);
                }
                Some(PushEvidence::Opaque) => {
                    saw_opaque = true;
                }
                None => {}
            }
        }
        // Opaque sibling next to concrete evidence: the opaque site may still emit a raw
        // write after a concrete site has promoted storage across the back edge.
        if saw_opaque && saw_known {
            joined = PhpType::Mixed;
        }
        if joined == PhpType::Mixed {
            names.push(name.to_string());
        }
    }
    names
}

/// Returns the names of locals that hold a raw-scalar indexed array at loop entry and are
/// *reassigned* inside the loop body (or the optional `for` update) to an array literal whose
/// element storage stops being a raw scalar — a boxed `mixed` cell or the inline tagged
/// `int|null` scalar (issue #594). Unlike `loop_grown_mixed_array_pushes`, this covers full
/// rebinds `$name = [...]`, not in-place growth: a self-referential rebuild such as
/// `$r = [$r[0] - 1, 0]` (mixed) or a `[$r[1], $r[0]]` swap (`int|null`) changes the element
/// representation on the back edge, but a read of `$r[0]` at the top of the single-pass loop body
/// is still typed against the entry `array<int>`, so it reads the promoted cell of iterations
/// >= 2 as a raw scalar (a heap pointer surfaces as an int).
///
/// `lookup` supplies the local's type at loop entry; only names that currently hold an array of
/// a raw *unboxed* scalar element (`int`/`float`/`string`/`bool`) are candidates, because those
/// are the element representations that change shape when the storage becomes boxed/tagged.
/// `infer_rhs` supplies the caller's array type for a reassignment RHS; a name is reported only
/// when that type is an indexed array whose element lowers to a boxed/tagged cell. Same-
/// representation rebinds (`array<int>` -> `array<int>`, e.g. `$r = [count($r), 0]`) and rebinds
/// to a differently-typed raw scalar array are left untouched: the former needs no promotion, and
/// the latter is a distinct representation gap that this boxed-storage widening does not model.
pub fn loop_reassigned_mixed_arrays(
    body: &[Stmt],
    update: Option<&Stmt>,
    lookup: &dyn Fn(&str) -> Option<PhpType>,
    infer_rhs: &mut dyn FnMut(&Expr) -> Option<PhpType>,
) -> Vec<String> {
    let mut assignments: Vec<(&str, AssignedValue<'_>)> = Vec::new();
    collect_value_assignments(body, &mut assignments);
    if let Some(stmt) = update {
        collect_value_assignment_stmt(stmt, &mut assignments);
    }
    let mut names: Vec<String> = Vec::new();
    for (name, source) in &assignments {
        let AssignedValue::Expr(rhs) = source else {
            continue;
        };
        if names.iter().any(|n| n == name) {
            continue;
        }
        let Some(PhpType::Array(entry_elem)) = lookup(name) else {
            continue;
        };
        if !is_raw_scalar_array_element(&entry_elem) {
            continue;
        }
        let Some(PhpType::Array(new_elem)) = infer_rhs(rhs) else {
            continue;
        };
        if rebind_element_changes_representation(&new_elem) {
            names.push(name.to_string());
        }
    }
    names
}

/// Returns whether a reassignment's array element storage is a boxed `mixed` cell or the inline
/// tagged `int|null` scalar. Both differ in shape from a raw unboxed scalar, so rebinding a
/// raw-scalar array to one of them across a loop back edge makes a top-of-body read misread the
/// element (issue #594). A raw scalar rebind (e.g. `$r = [count($r)]`, still `array<int>`) keeps
/// the entry representation and must not be widened, so it is excluded here. A self-referential
/// rebind reads `$r`'s own elements, so once `$r` is widened to `array<mixed>` the rebind's reads
/// return `mixed` and the rebuilt literal is itself `array<mixed>` — no element coercion needed.
fn rebind_element_changes_representation(new_elem: &PhpType) -> bool {
    matches!(
        new_elem.codegen_repr(),
        PhpType::Mixed | PhpType::TaggedScalar
    )
}

/// Returns whether a resolved array element type is a raw (unboxed) scalar in EIR array storage.
/// These element representations are read as a direct machine value, so a loop back-edge that
/// swaps them for boxed `mixed` cells makes an early read misinterpret a boxed pointer as a raw
/// scalar (issue #594). Boxed (`mixed`), array, and object elements are already heap references
/// and read consistently across the promotion, so they are not candidates for this widening.
fn is_raw_scalar_array_element(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool
    )
}

/// Resolves the evidence a pushed/written value contributes to the element join.
fn resolve_pushed_value_evidence(
    value: &Expr,
    lookup: &dyn Fn(&str) -> Option<PhpType>,
    assignments: &[(&str, AssignedValue<'_>)],
    infer_value: &mut dyn FnMut(&Expr) -> Option<PhpType>,
) -> Option<PushEvidence> {
    match &value.kind {
        ExprKind::Variable(name) => {
            let entry = lookup(name);
            let body = assigned_value_evidence(name, assignments, infer_value);
            match (entry, body) {
                (_, Some(PushEvidence::Opaque)) => Some(PushEvidence::Opaque),
                (Some(a), Some(PushEvidence::Known(b))) => {
                    Some(PushEvidence::Known(join_pushed_element_type(a, b)))
                }
                (Some(a), None) => Some(PushEvidence::Known(a)),
                (None, Some(PushEvidence::Known(b))) => Some(PushEvidence::Known(b)),
                (None, None) => None,
            }
        }
        _ => match infer_value(value).or_else(|| precise_scalar_expr_type(value)) {
            Some(ty) => Some(PushEvidence::Known(ty)),
            None => Some(PushEvidence::Opaque),
        },
    }
}

/// Joins the recorded in-loop assignment types for `name`, preserving opaque assignments
/// as poison so stale loop-entry evidence cannot overrule an unknown reassignment.
fn assigned_value_evidence(
    name: &str,
    assignments: &[(&str, AssignedValue<'_>)],
    infer_value: &mut dyn FnMut(&Expr) -> Option<PhpType>,
) -> Option<PushEvidence> {
    let mut joined: Option<PhpType> = None;
    for (candidate, source) in assignments {
        if *candidate != name {
            continue;
        }
        let ty = match source {
            AssignedValue::Expr(expr) => {
                infer_value(expr).or_else(|| precise_scalar_expr_type(expr))
            }
            AssignedValue::Opaque => None,
        };
        let Some(ty) = ty else {
            return Some(PushEvidence::Opaque);
        };
        joined = Some(match joined {
            Some(acc) => join_pushed_element_type(acc, ty),
            None => ty,
        });
    }
    joined.map(PushEvidence::Known)
}

/// Returns a self-evident concrete scalar type, or `None` when the expression must be
/// treated as opaque join evidence (the shared syntactic helper defaults too many unknown
/// constructs to `Int` to be safe here).
fn precise_scalar_expr_type(value: &Expr) -> Option<PhpType> {
    match &value.kind {
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::StringLiteral(_) => Some(PhpType::Str),
        ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
        ExprKind::Null => Some(PhpType::Void),
        ExprKind::Cast { target, .. } => match target {
            CastType::Int => Some(PhpType::Int),
            CastType::Float => Some(PhpType::Float),
            CastType::String => Some(PhpType::Str),
            CastType::Bool => Some(PhpType::Bool),
            _ => None,
        },
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = precise_scalar_expr_type(then_expr)?;
            let else_ty = precise_scalar_expr_type(else_expr)?;
            Some(join_pushed_element_type(then_ty, else_ty))
        }
        ExprKind::ErrorSuppress(inner) => precise_scalar_expr_type(inner),
        _ => None,
    }
}

/// Collects local assignments from every statement in `stmts`, recursively.
fn collect_value_assignments<'a>(
    stmts: &'a [Stmt],
    out: &mut Vec<(&'a str, AssignedValue<'a>)>,
) {
    for stmt in stmts {
        collect_value_assignment_stmt(stmt, out);
    }
}

/// Collects local assignments from one statement, mirroring the nested-statement recursion
/// of `collect_array_push_stmt` for bodies that execute within the loop iteration.
fn collect_value_assignment_stmt<'a>(
    stmt: &'a Stmt,
    out: &mut Vec<(&'a str, AssignedValue<'a>)>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            out.push((name.as_str(), AssignedValue::Expr(value)));
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_value_assignments(then_body, out);
            for (_, clause_body) in elseif_clauses {
                collect_value_assignments(clause_body, out);
            }
            if let Some(else_body) = else_body {
                collect_value_assignments(else_body, out);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_value_assignments(then_body, out);
            if let Some(else_body) = else_body {
                collect_value_assignments(else_body, out);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => collect_value_assignments(body, out),
        StmtKind::Foreach {
            key_var,
            value_var,
            body,
            ..
        } => {
            // The foreach bindings assign non-literal values each iteration: poison them.
            if let Some(key_var) = key_var {
                out.push((key_var.as_str(), AssignedValue::Opaque));
            }
            out.push((value_var.as_str(), AssignedValue::Opaque));
            collect_value_assignments(body, out);
        }
        StmtKind::For {
            init,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                collect_value_assignment_stmt(init, out);
            }
            if let Some(update) = update {
                collect_value_assignment_stmt(update, out);
            }
            collect_value_assignments(body, out);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, case_body) in cases {
                collect_value_assignments(case_body, out);
            }
            if let Some(default) = default {
                collect_value_assignments(default, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_value_assignments(try_body, out);
            for catch in catches {
                collect_value_assignments(&catch.body, out);
            }
            if let Some(finally_body) = finally_body {
                collect_value_assignments(finally_body, out);
            }
        }
        StmtKind::Synthetic(stmts) => collect_value_assignments(stmts, out),
        _ => {}
    }
}

/// Joins two indexed-array element types on the widening lattice used by growth sites:
/// equal types stay, `never` adopts the other side, and any other combination widens to
/// `mixed` (the representation-changing transition this scan exists to detect).
fn join_pushed_element_type(a: PhpType, b: PhpType) -> PhpType {
    if a == b {
        a
    } else if a == PhpType::Never {
        b
    } else if b == PhpType::Never {
        a
    } else {
        PhpType::Mixed
    }
}

/// Collects local-array growth/write sites from every statement in `stmts`, recursively.
fn collect_array_pushes<'a>(stmts: &'a [Stmt], out: &mut Vec<(&'a str, &'a Expr)>) {
    for stmt in stmts {
        collect_array_push_stmt(stmt, out);
    }
}

/// Collects growth/write sites from one statement, recursing into every nested statement
/// body that executes as part of the enclosing loop iteration. Declaration bodies
/// (functions, classes) do not execute in the loop and closures capture by value by
/// default, so neither is descended into.
fn collect_array_push_stmt<'a>(stmt: &'a Stmt, out: &mut Vec<(&'a str, &'a Expr)>) {
    match &stmt.kind {
        StmtKind::ArrayPush { array, value } => out.push((array.as_str(), value)),
        StmtKind::ArrayAssign { array, value, .. } => out.push((array.as_str(), value)),
        StmtKind::ExprStmt(expr) => collect_growth_calls_from_expr(expr, out),
        StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } => {
            collect_growth_calls_from_expr(value, out);
        }
        StmtKind::Echo(expr) => collect_growth_calls_from_expr(expr, out),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_array_pushes(then_body, out);
            for (_, clause_body) in elseif_clauses {
                collect_array_pushes(clause_body, out);
            }
            if let Some(else_body) = else_body {
                collect_array_pushes(else_body, out);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_array_pushes(then_body, out);
            if let Some(else_body) = else_body {
                collect_array_pushes(else_body, out);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => collect_array_pushes(body, out),
        StmtKind::For {
            init,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                collect_array_push_stmt(init, out);
            }
            if let Some(update) = update {
                collect_array_push_stmt(update, out);
            }
            collect_array_pushes(body, out);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, case_body) in cases {
                collect_array_pushes(case_body, out);
            }
            if let Some(default) = default {
                collect_array_pushes(default, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_array_pushes(try_body, out);
            for catch in catches {
                collect_array_pushes(&catch.body, out);
            }
            if let Some(finally_body) = finally_body {
                collect_array_pushes(finally_body, out);
            }
        }
        StmtKind::Synthetic(stmts) => collect_array_pushes(stmts, out),
        _ => {}
    }
}

/// Collects `array_push($name, ...)` growth sites from an expression tree.
fn collect_growth_calls_from_expr<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, &'a Expr)>) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } if name.as_str().eq_ignore_ascii_case("array_push") => {
            if let Some(array_name) = array_push_target_name(args) {
                for arg in args.iter().skip(1) {
                    out.push((array_name, call_arg_value(arg)));
                }
            }
            for arg in args {
                collect_growth_calls_from_expr(call_arg_value(arg), out);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NullsafeMethodCall { args, .. }
        | ExprKind::ExprCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewDynamic { args, .. }
        | ExprKind::NewDynamicObject { args, .. } => {
            for arg in args {
                collect_growth_calls_from_expr(call_arg_value(arg), out);
            }
        }
        ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Negate(value)
        | ExprKind::BitNot(value)
        | ExprKind::Not(value)
        | ExprKind::Clone(value)
        | ExprKind::Throw(value) => collect_growth_calls_from_expr(value, out),
        ExprKind::Cast { expr: inner, .. } => collect_growth_calls_from_expr(inner, out),
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        } => {
            collect_growth_calls_from_expr(left, out);
            collect_growth_calls_from_expr(right, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_growth_calls_from_expr(condition, out);
            collect_growth_calls_from_expr(then_expr, out);
            collect_growth_calls_from_expr(else_expr, out);
        }
        ExprKind::ArrayLiteral(elems) => {
            for elem in elems {
                collect_growth_calls_from_expr(elem, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                collect_growth_calls_from_expr(key, out);
                collect_growth_calls_from_expr(value, out);
            }
        }
        _ => {}
    }
}

/// Returns the local array name targeted by `array_push`'s first argument, if any.
fn array_push_target_name<'a>(args: &'a [Expr]) -> Option<&'a str> {
    let first = args.first()?;
    match &call_arg_value(first).kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Unwraps a possible named argument to its value expression.
fn call_arg_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}
