//! Purpose:
//! Fuses a pure `array_map()` temporary with its constant-indexed reads.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before lowering a function body.
//!
//! Key details:
//! - Only an untagged, capture-free closure whose body is a side-effect-free arithmetic
//!   tree over the parameter and scalar literals (`+`, `-`, `*`, unary minus, nested freely)
//!   over a packed `array<int>` qualifies: such a callback has no side effects, so reading
//!   one mapped element does not require mapping every element. Typed returns keep the map
//!   because an overflowing element must still throw.
//! - A missing source key must keep PHP's warning and null result; every fused read is
//!   guarded by `isset($source[$index])` and falls back to a normal source read.
//! - The temporary must not be observed anywhere else in the block. Unsupported statement
//!   or expression shapes fail closed instead of risking a missed use.
//! - The same guarded substitution also rewrites a direct `array_map(...)[$key]` read, which
//!   never materializes the temporary in the first place.
//! - A value-`foreach` over the same pure map is delegated to the sibling
//!   `map_foreach_fusion` module, which iterates the source with an unforgeable generated value
//!   local and assigns the substituted callback expression to the original loop variable.

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

/// One pure map assignment found in a straight-line block.
struct Candidate {
    mapped: String,
    source: String,
    param: String,
    body: Expr,
}

/// Rewrites every straight-line block in `body` that contains one safe map temporary.
pub(crate) fn fuse_map_reads(
    body: &mut Vec<Stmt>,
    env: &TypeEnv,
    by_ref_params: &std::collections::HashSet<String>,
) {
    let aliased = aliased_locals(body, by_ref_params);
    let mut fresh_foreach = 0usize;
    let initialized = by_ref_params.clone();
    fuse_direct_stmts(body, env, &aliased, &mut fresh_foreach, &initialized);
    fuse_block(body, env, &aliased);
}

/// Collects locals whose storage another binding can mutate.
///
/// The indexed-read fusion defers a source read that PHP's `array_map()` performed at the
/// assignment. A reference, global, static binding, or by-reference parameter can change that
/// source between the two points, so those sources keep the materialized map.
pub(super) fn aliased_locals(
    body: &[Stmt],
    by_ref_params: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    /// Adds every local bound through a reference, global or static alias in `stmts`.
    fn collect(stmts: &[Stmt], aliased: &mut std::collections::HashSet<String>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::RefAssign { target, source } => {
                    aliased.insert(target.clone());
                    if let Some(root) = root_local(source) {
                        aliased.insert(root.to_string());
                    }
                }
                StmtKind::Foreach {
                    value_var,
                    value_by_ref,
                    body,
                    ..
                } => {
                    if *value_by_ref {
                        aliased.insert(value_var.clone());
                    }
                    collect(body, aliased);
                }
                StmtKind::Global { vars } => {
                    aliased.extend(vars.iter().cloned());
                }
                StmtKind::StaticVar { name, .. } => {
                    aliased.insert(name.clone());
                }
                StmtKind::For { body, .. }
                | StmtKind::While { body, .. }
                | StmtKind::DoWhile { body, .. }
                | StmtKind::IncludeOnceGuard { body, .. }
                | StmtKind::NamespaceBlock { body, .. }
                | StmtKind::Synthetic(body) => collect(body, aliased),
                StmtKind::IfDef { then_body, else_body, .. } => {
                    collect(then_body, aliased);
                    if let Some(body) = else_body {
                        collect(body, aliased);
                    }
                }
                StmtKind::If {
                    then_body,
                    elseif_clauses,
                    else_body,
                    ..
                } => {
                    collect(then_body, aliased);
                    for (_, body) in elseif_clauses {
                        collect(body, aliased);
                    }
                    if let Some(body) = else_body {
                        collect(body, aliased);
                    }
                }
                StmtKind::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect(body, aliased);
                    }
                    if let Some(body) = default {
                        collect(body, aliased);
                    }
                }
                StmtKind::Try {
                    try_body,
                    catches,
                    finally_body,
                } => {
                    collect(try_body, aliased);
                    for catch in catches {
                        collect(&catch.body, aliased);
                    }
                    if let Some(body) = finally_body {
                        collect(body, aliased);
                    }
                }
                _ => {}
            }
        }
    }
    let mut aliased = by_ref_params.clone();
    collect(body, &mut aliased);
    aliased
}

/// Returns the simple local at the root of a reference source, if any.
pub(super) fn root_local(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => root_local(array),
        ExprKind::ErrorSuppress(inner) | ExprKind::NamedArg { value: inner, .. } => {
            root_local(inner)
        }
        _ => None,
    }
}

/// Marks locals this statement definitely binds when it completes.
///
/// Only straight-line assignments, ref assignments, array writes, and static declarations are
/// tracked. Branches and loops may leave a name unbound, so they deliberately add nothing.
pub(super) fn mark_definite_assignments(
    stmt: &Stmt,
    initialized: &mut std::collections::HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. } => {
            initialized.insert(name.clone());
        }
        StmtKind::RefAssign { target, .. } => {
            initialized.insert(target.clone());
        }
        StmtKind::ArrayAssign { index, value, array } => {
            initialized.insert(array.clone());
            let _ = (index, value);
        }
        StmtKind::ArrayPush { array, value } => {
            initialized.insert(array.clone());
            let _ = value;
        }
        StmtKind::NestedArrayAssign { target, .. } => {
            if let Some(root) = root_local(target) {
                initialized.insert(root.to_string());
            }
        }
        StmtKind::StaticVar { name, .. } => {
            initialized.insert(name.clone());
        }
        StmtKind::Synthetic(body) => {
            for stmt in body {
                mark_definite_assignments(stmt, initialized);
            }
        }
        _ => {}
    }
}

/// Rewrites direct `array_map(...)[$key]` reads throughout the statement tree.
fn fuse_direct_stmts(
    stmts: &mut [Stmt],
    env: &TypeEnv,
    aliased: &std::collections::HashSet<String>,
    fresh_foreach: &mut usize,
    outer_initialized: &std::collections::HashSet<String>,
) {
    let mut initialized = outer_initialized.clone();
    for stmt in stmts {
        fuse_direct_stmt(stmt, env, aliased, fresh_foreach, &initialized);
        mark_definite_assignments(stmt, &mut initialized);
    }
}

/// Rewrites direct map reads inside one statement and recurses into nested bodies.
fn fuse_direct_stmt(
    stmt: &mut Stmt,
    env: &TypeEnv,
    aliased: &std::collections::HashSet<String>,
    fresh_foreach: &mut usize,
    initialized: &std::collections::HashSet<String>,
) {
    let span = stmt.span;
    let source_mode = stmt.source_mode;
    let strict_types = stmt.strict_types;
    match &mut stmt.kind {
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            if !*value_by_ref {
                super::map_foreach_fusion::try_fuse_foreach_map(
                    array,
                    key_var.as_deref(),
                    value_var,
                    body,
                    env,
                    aliased,
                    span,
                    source_mode,
                    strict_types,
                    fresh_foreach,
                    initialized,
                );
            }
            fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
        }
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            fuse_direct_stmts(then_body, env, aliased, fresh_foreach, initialized);
            for (_, body) in elseif_clauses {
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
            if let Some(body) = else_body {
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
        }
        StmtKind::Switch { subject, cases, default } => {
            fuse_direct_expr(subject, env, aliased, fresh_foreach);
            for (conditions, body) in cases {
                for condition in conditions {
                    fuse_direct_expr(condition, env, aliased, fresh_foreach);
                }
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
            if let Some(body) = default {
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            fuse_direct_stmts(try_body, env, aliased, fresh_foreach, initialized);
            for catch in catches {
                fuse_direct_stmts(&mut catch.body, env, aliased, fresh_foreach, initialized);
            }
            if let Some(body) = finally_body {
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
        }
        StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::Return(Some(value))
        | StmtKind::Assign { value, .. }
        | StmtKind::RefAssign { source: value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::Include { path: value, .. }
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::ListUnpack { value, .. }
        | StmtKind::StaticVar { init: value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => fuse_direct_expr(value, env, aliased, fresh_foreach),
        StmtKind::ArrayAssign { index: first, value: second, .. }
        | StmtKind::NestedArrayAssign { target: first, value: second }
        | StmtKind::StaticPropertyArrayAssign { index: first, value: second, .. } => {
            fuse_direct_expr(first, env, aliased, fresh_foreach);
            fuse_direct_expr(second, env, aliased, fresh_foreach);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            fuse_direct_expr(object, env, aliased, fresh_foreach);
            fuse_direct_expr(value, env, aliased, fresh_foreach);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            fuse_direct_expr(object, env, aliased, fresh_foreach);
            fuse_direct_expr(index, env, aliased, fresh_foreach);
            fuse_direct_expr(value, env, aliased, fresh_foreach);
        }
        StmtKind::Synthetic(body) => fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized),
        StmtKind::IfDef { then_body, else_body, .. } => {
            fuse_direct_stmts(then_body, env, aliased, fresh_foreach, initialized);
            if let Some(body) = else_body {
                fuse_direct_stmts(body, env, aliased, fresh_foreach, initialized);
            }
        }
        StmtKind::Return(None)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. } => {}
    }
}

/// Rewrites a direct map read in one expression and recurses into its subexpressions.
fn fuse_direct_expr(
    expr: &mut Expr,
    env: &TypeEnv,
    aliased: &std::collections::HashSet<String>,
    fresh_foreach: &mut usize,
) {
    if let Some(replacement) = direct_map_read(expr, env) {
        *expr = replacement;
        return;
    }
    match &mut expr.kind {
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce { value: left, default: right }
        | ExprKind::ShortTernary { value: left, default: right }
        | ExprKind::Pipe { value: left, callable: right }
        | ExprKind::ArrayAccess { array: left, index: right }
        | ExprKind::DynamicPropertyAccess { object: left, property: right }
        | ExprKind::NullsafeDynamicPropertyAccess { object: left, property: right } => {
            fuse_direct_expr(left, env, aliased, fresh_foreach);
            fuse_direct_expr(right, env, aliased, fresh_foreach);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            fuse_direct_expr(condition, env, aliased, fresh_foreach);
            fuse_direct_expr(then_expr, env, aliased, fresh_foreach);
            fuse_direct_expr(else_expr, env, aliased, fresh_foreach);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::IncludeValue { path: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => fuse_direct_expr(inner, env, aliased, fresh_foreach),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ArrayLiteral(args) => {
            for arg in args {
                fuse_direct_expr(arg, env, aliased, fresh_foreach);
            }
        }
        ExprKind::ExprCall { callee, args }
        | ExprKind::NewDynamic { name_expr: callee, args }
        | ExprKind::NewDynamicObject { class_name: callee, args, .. }
        | ExprKind::MethodCall { object: callee, args, .. }
        | ExprKind::NullsafeMethodCall { object: callee, args, .. } => {
            fuse_direct_expr(callee, env, aliased, fresh_foreach);
            for arg in args {
                fuse_direct_expr(arg, env, aliased, fresh_foreach);
            }
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            fuse_direct_expr(object, env, aliased, fresh_foreach);
            fuse_direct_expr(method, env, aliased, fresh_foreach);
            for arg in args {
                fuse_direct_expr(arg, env, aliased, fresh_foreach);
            }
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                fuse_direct_expr(key, env, aliased, fresh_foreach);
                fuse_direct_expr(value, env, aliased, fresh_foreach);
            }
        }
        ExprKind::Match { subject, arms, default } => {
            fuse_direct_expr(subject, env, aliased, fresh_foreach);
            for (conditions, value) in arms {
                for condition in conditions {
                    fuse_direct_expr(condition, env, aliased, fresh_foreach);
                }
                fuse_direct_expr(value, env, aliased, fresh_foreach);
            }
            if let Some(default) = default {
                fuse_direct_expr(default, env, aliased, fresh_foreach);
            }
        }
        ExprKind::Assignment { target, value, prelude, .. } => {
            // Prelude statements are a nested block whose definite-initialization state is not
            // visible here; their direct map reads keep the ordinary lowering.
            let _ = prelude;
            fuse_direct_expr(target, env, aliased, fresh_foreach);
            fuse_direct_expr(value, env, aliased, fresh_foreach);
        }
        // Closure bodies run in a nested scope whose locals are not in this environment; a
        // direct read there can still be fused by the closure's own lowering pass.
        ExprKind::Closure { .. } | ExprKind::ArrayLiteralMixed(_) => {}
        _ => {}
    }
}

/// Builds the guarded source read for a direct `array_map(...)[$key]`, if it is safe.
fn direct_map_read(expr: &Expr, env: &TypeEnv) -> Option<Expr> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &array.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[1].kind else {
        return None;
    };
    if env.get(source).map(PhpType::codegen_repr)
        != Some(PhpType::Array(Box::new(PhpType::Int)))
    {
        return None;
    }
    let (param, body) = closure_arithmetic(&args[0])?;
    if !matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::Variable(_)) {
        return None;
    }
    let read = source_read_expr(source, index, expr.span);
    Some(guarded_map_read(body, param, read, expr.span))
}

/// Recurses into nested scopes and then fuses candidates in this block.
fn fuse_block(stmts: &mut Vec<Stmt>, env: &TypeEnv, aliased: &std::collections::HashSet<String>) {
    for stmt in stmts.iter_mut() {
        recurse_stmt(stmt, env, aliased);
    }
    let mut index = 0;
    while index < stmts.len() {
        let Some(candidate) = map_candidate(&stmts[index], env, aliased) else {
            index += 1;
            continue;
        };
        // A previous value of the temporary may survive across loop iterations.
        if stmts[..index]
            .iter()
            .any(|stmt| stmt_mentions(stmt, &candidate.mapped))
        {
            index += 1;
            continue;
        }
        let mut trial = stmts.clone();
        trial.remove(index);
        let mut supported = true;
        for stmt in trial.iter_mut().skip(index) {
            if rewrite_stmt(stmt, &candidate).is_err() {
                supported = false;
                break;
            }
        }
        if supported {
            *stmts = trial;
        } else {
            index += 1;
        }
    }
}

/// Recurses into the bodies of nested control-flow statements.
fn recurse_stmt(stmt: &mut Stmt, env: &TypeEnv, aliased: &std::collections::HashSet<String>) {
    match &mut stmt.kind {
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => fuse_block(body, env, aliased),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            fuse_block(then_body, env, aliased);
            for (_, body) in elseif_clauses {
                fuse_block(body, env, aliased);
            }
            if let Some(body) = else_body {
                fuse_block(body, env, aliased);
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                fuse_block(body, env, aliased);
            }
            if let Some(body) = default {
                fuse_block(body, env, aliased);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            fuse_block(try_body, env, aliased);
            for catch in catches {
                fuse_block(&mut catch.body, env, aliased);
            }
            if let Some(body) = finally_body {
                fuse_block(body, env, aliased);
            }
        }
        StmtKind::Synthetic(body) => fuse_block(body, env, aliased),
        _ => {}
    }
}

/// Returns the safe `array_map()` assignment represented by one statement.
fn map_candidate(
    stmt: &Stmt,
    env: &TypeEnv,
    aliased: &std::collections::HashSet<String>,
) -> Option<Candidate> {
    let StmtKind::Assign { name: mapped, value } = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[1].kind else {
        return None;
    };
    if env.get(source).map(PhpType::codegen_repr)
        != Some(PhpType::Array(Box::new(PhpType::Int)))
        || aliased.contains(source)
    {
        return None;
    }
    let (param, body) = closure_arithmetic(&args[0])?;
    Some(Candidate {
        mapped: mapped.clone(),
        source: source.clone(),
        param: param.to_string(),
        body: body.clone(),
    })
}

/// Returns the parameter and return expression of a pure untyped single-parameter closure.
pub(super) fn closure_arithmetic(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type,
        body,
        captures,
        capture_refs,
        by_ref_return: false,
        ..
    } = &expr.kind
    else {
        return None;
    };
    if return_type.is_some() || !captures.is_empty() || !capture_refs.is_empty() {
        return None;
    }
    let [(param, param_type, default, by_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int)) || default.is_some() || *by_ref {
        return None;
    }
    let [Stmt {
        kind: StmtKind::Return(Some(body)),
        ..
    }] = body.as_slice()
    else {
        return None;
    };
    pure_arithmetic(body, param).then_some((param.as_str(), body))
}

/// Returns true for a total arithmetic tree over the parameter and scalar literals.
fn pure_arithmetic(expr: &Expr, param: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => name == param,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => true,
        ExprKind::Negate(inner) => pure_arithmetic(inner, param),
        ExprKind::BinaryOp { left, op, right } => {
            matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul)
                && pure_arithmetic(left, param)
                && pure_arithmetic(right, param)
        }
        _ => false,
    }
}

/// Copies a pure arithmetic body, replacing its parameter with the source read.
pub(super) fn substitute(expr: &Expr, param: &str, replacement: &Expr) -> Expr {
    match &expr.kind {
        ExprKind::Variable(name) if name == param => replacement.clone(),
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => expr.clone(),
        ExprKind::Negate(inner) => Expr {
            kind: ExprKind::Negate(Box::new(substitute(inner, param, replacement))),
            span: expr.span,
        },
        ExprKind::BinaryOp { left, op, right } => Expr {
            kind: ExprKind::BinaryOp {
                left: Box::new(substitute(left, param, replacement)),
                op: op.clone(),
                right: Box::new(substitute(right, param, replacement)),
            },
            span: expr.span,
        },
        _ => unreachable!("pure_arithmetic admitted a non-arithmetic node"),
    }
}

/// Builds one `$source[$index]` read at the original span.
fn source_read(source: &str, index: i64, span: Span) -> Expr {
    source_read_expr(
        source,
        &Expr {
            kind: ExprKind::IntLiteral(index),
            span,
        },
        span,
    )
}

/// Builds one `$source[$index]` read from an already-validated index expression.
fn source_read_expr(source: &str, index: &Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::ArrayAccess {
            array: Box::new(Expr {
                kind: ExprKind::Variable(source.to_string()),
                span,
            }),
            index: Box::new(index.clone()),
        },
        span,
    }
}

/// Builds `isset($source[$index]) ? <mapped body> : $source[$index]`.
///
/// The else arm is the ordinary source read, so a missing key keeps PHP's warning and null
/// result without invoking the callback.
fn guarded_map_read(body: &Expr, param: &str, read: Expr, span: Span) -> Expr {
    let condition = Expr {
        kind: ExprKind::FunctionCall {
            name: Name::from("isset"),
            args: vec![read.clone()],
        },
        span,
    };
    let then_expr = substitute(body, param, &read);
    Expr {
        kind: ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(read),
        },
        span,
    }
}

/// Rewrites one statement after the map assignment.
fn rewrite_stmt(stmt: &mut Stmt, candidate: &Candidate) -> Result<(), ()> {
    match &mut stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if name == &candidate.mapped || name == &candidate.source {
                return Err(());
            }
            rewrite_expr(value, candidate)
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => rewrite_expr(expr, candidate),
        StmtKind::Return(Some(expr)) => rewrite_expr(expr, candidate),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => Ok(()),
        StmtKind::ArrayAssign { array, index, value } => {
            if array == &candidate.mapped || array == &candidate.source {
                return Err(());
            }
            rewrite_expr(index, candidate)?;
            rewrite_expr(value, candidate)
        }
        StmtKind::ArrayPush { array, value } => {
            if array == &candidate.mapped || array == &candidate.source {
                return Err(());
            }
            rewrite_expr(value, candidate)
        }
        StmtKind::Synthetic(body) => {
            for stmt in body {
                rewrite_stmt(stmt, candidate)?;
            }
            Ok(())
        }
        _ => Err(()),
    }
}

/// Rewrites every matching constant-indexed read inside one expression.
fn rewrite_expr(expr: &mut Expr, candidate: &Candidate) -> Result<(), ()> {
    if let ExprKind::ArrayAccess { array, index } = &expr.kind {
        if let ExprKind::Variable(name) = &array.kind {
            if name == &candidate.mapped {
                let ExprKind::IntLiteral(index) = &index.kind else {
                    return Err(());
                };
                let read = source_read(&candidate.source, *index, expr.span);
                *expr = guarded_map_read(&candidate.body, &candidate.param, read, expr.span);
                return Ok(());
            }
        }
    }
    match &mut expr.kind {
        ExprKind::Variable(name) => {
            if name == &candidate.mapped || name == &candidate.source {
                Err(())
            } else {
                Ok(())
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            if !matches!(&array.kind, ExprKind::Variable(name) if name == &candidate.source) {
                rewrite_expr(array, candidate)?;
            }
            rewrite_expr(index, candidate)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            rewrite_expr(left, candidate)?;
            rewrite_expr(right, candidate)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => rewrite_expr(inner, candidate),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            rewrite_expr(condition, candidate)?;
            rewrite_expr(then_expr, candidate)?;
            rewrite_expr(else_expr, candidate)
        }
        ExprKind::ShortTernary { value, default } => {
            rewrite_expr(value, candidate)?;
            rewrite_expr(default, candidate)
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::NewObject { args, .. } => {
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ExprCall { callee, args } => {
            rewrite_expr(callee, candidate)?;
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ClosureCall { var, args } => {
            if var == &candidate.mapped || var == &candidate.source {
                return Err(());
            }
            for arg in args {
                rewrite_expr(arg, candidate)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                rewrite_expr(item, candidate)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                rewrite_expr(key, candidate)?;
                rewrite_expr(value, candidate)?;
            }
            Ok(())
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            rewrite_expr(subject, candidate)?;
            for (conditions, value) in arms {
                for condition in conditions {
                    rewrite_expr(condition, candidate)?;
                }
                rewrite_expr(value, candidate)?;
            }
            if let Some(default) = default {
                rewrite_expr(default, candidate)?;
            }
            Ok(())
        }
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } => {
            if captures.iter().any(|name| name == &candidate.mapped || name == &candidate.source)
                || capture_refs
                    .iter()
                    .any(|name| name == &candidate.mapped || name == &candidate.source)
            {
                return Err(());
            }
            Ok(())
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            if name == &candidate.mapped || name == &candidate.source {
                Err(())
            } else {
                Ok(())
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant { .. } => Ok(()),
        ExprKind::ArrayLiteralMixed(_) => Err(()),
        _ => Err(()),
    }
}

/// Reports whether a statement may mention one local, conservatively.
pub(super) fn stmt_mentions(stmt: &Stmt, name: &str) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name: target, value } | StmtKind::TypedAssign { name: target, value, .. } => {
            target == name || expr_mentions(value, name)
        }
        StmtKind::ExprStmt(expr) | StmtKind::Echo(expr) => expr_mentions(expr, name),
        StmtKind::Return(Some(expr)) => expr_mentions(expr, name),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => false,
        StmtKind::ArrayAssign { array, index, value } => {
            array == name || expr_mentions(index, name) || expr_mentions(value, name)
        }
        StmtKind::ArrayPush { array, value } => array == name || expr_mentions(value, name),
        StmtKind::Synthetic(body) => body.iter().any(|stmt| stmt_mentions(stmt, name)),
        _ => true,
    }
}

/// Reports whether an expression may mention one local, conservatively.
pub(super) fn expr_mentions(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Variable(local) => local == name,
        ExprKind::ArrayAccess { array, index } => {
            expr_mentions(array, name) || expr_mentions(index, name)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_mentions(left, name) || expr_mentions(right, name)
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_mentions(inner, name),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_mentions(condition, name)
                || expr_mentions(then_expr, name)
                || expr_mentions(else_expr, name)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_mentions(value, name) || expr_mentions(default, name)
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::NewObject { args, .. } => {
            args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_mentions(callee, name) || args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ClosureCall { var, args } => {
            var == name || args.iter().any(|arg| expr_mentions(arg, name))
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(|item| expr_mentions(item, name)),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_mentions(key, name) || expr_mentions(value, name)),
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } => captures.iter().any(|local| local == name) || capture_refs.iter().any(|local| local == name),
        ExprKind::PreIncrement(local)
        | ExprKind::PostIncrement(local)
        | ExprKind::PreDecrement(local)
        | ExprKind::PostDecrement(local) => local == name,
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant { .. } => false,
        _ => true,
    }
}
