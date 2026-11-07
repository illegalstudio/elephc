//! Purpose:
//! Marks `array_map(fn($v) => $v + C | $v - C | $v * C | $v * A + B | $v * $v |
//! $v * $v + C | $v * F | $v * F + G | $v + F | $v - F | $v / F | $v / C, $ints)`
//! assignments whose result is only observed through storage-neutral consumers, so
//! lowering may build a raw `array<int>` or `array<float>` instead of boxing one
//! Mixed cell per element.
//!
//! Called from:
//! - `crate::ir_lower::function::lower_body_into_function` before pre-lowering fusion.
//!
//! Key details:
//! - The marker is a synthetic call name consumed by `lower_function_call`; the checker
//!   has already typed the original call, so the rewrite changes no PHP-visible type.
//! - A variable source's packed `array<int>` proof is deferred to lowering, where the
//!   flow-sensitive local type is known (so function-local assignments fuse too). A failed
//!   proof falls back to the ordinary `array_map()` lowering.
//! - Non-empty all-int array literals, `range()` calls with integer-literal arguments, and
//!   key-renumbering `array_slice()` calls over another packed-int source are proven
//!   structurally (recursively for slices).
//! - A candidate is marked only when every use of its destination local and value aliases
//!   is one of: indexed reads, value-`foreach`, read-only aliasing, or a whitelisted
//!   storage-neutral builtin. Any element write, direct or through an alias, keeps the
//!   generic boxed map so copy-on-write and ownership stay on the established path.
//! - The scan is deliberately conservative: unsupported expression shapes fail closed.

use std::collections::{HashMap, HashSet};

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::types::FunctionSig;

use super::map_read_fusion::{aliased_locals, expr_mentions, stmt_mentions};

/// Synthetic call name consumed by `crate::ir_lower::expr::function_calls`.
pub(crate) const STORED_NUMERIC_MAP: &str = "__elephc_stored_numeric_map";

/// Marks every safe stored numeric map in one function body.
pub(crate) fn mark_stored_numeric_maps(
    body: &mut [Stmt],
    by_ref_params: &HashSet<String>,
    functions: &HashMap<String, FunctionSig>,
) {
    if functions.contains_key("array_map") {
        return;
    }
    let analysis = body.to_vec();
    let aliased = aliased_locals(&analysis, by_ref_params);
    mark_block(body, &analysis, &aliased, functions);
}

/// Rewrites candidates in one block and recurses into nested blocks.
fn mark_block(
    stmts: &mut [Stmt],
    analysis: &[Stmt],
    aliased: &HashSet<String>,
    functions: &HashMap<String, FunctionSig>,
) {
    for stmt in stmts.iter_mut() {
        mark_stmt(stmt, analysis, aliased, functions);
    }
}

/// Rewrites one candidate statement and recurses into its nested bodies.
fn mark_stmt(
    stmt: &mut Stmt,
    analysis: &[Stmt],
    aliased: &HashSet<String>,
    functions: &HashMap<String, FunctionSig>,
) {
    let candidate = match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            stored_map_shape(value, aliased, functions).map(|_| name.clone())
        }
        _ => None,
    };
    if let Some(mapped) = candidate {
        let tracked = aliases_of(analysis, &mapped);
        if uses_are_neutral(analysis, &tracked, &mapped, stmt.span) {
            if let StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } =
                &mut stmt.kind
            {
                if let ExprKind::FunctionCall { name, .. } = &mut value.kind {
                    *name = Name::from(STORED_NUMERIC_MAP);
                }
            }
        }
    }
    match &mut stmt.kind {
        StmtKind::For { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => mark_block(body, analysis, aliased, functions),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            mark_block(then_body, analysis, aliased, functions);
            for (_, body) in elseif_clauses {
                mark_block(body, analysis, aliased, functions);
            }
            if let Some(body) = else_body {
                mark_block(body, analysis, aliased, functions);
            }
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            mark_block(then_body, analysis, aliased, functions);
            if let Some(body) = else_body {
                mark_block(body, analysis, aliased, functions);
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                mark_block(body, analysis, aliased, functions);
            }
            if let Some(body) = default {
                mark_block(body, analysis, aliased, functions);
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            mark_block(try_body, analysis, aliased, functions);
            for catch in catches {
                mark_block(&mut catch.body, analysis, aliased, functions);
            }
            if let Some(body) = finally_body {
                mark_block(body, analysis, aliased, functions);
            }
        }
        _ => {}
    }
}

/// One safe stored numeric map callback shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum StoredNumericMapShape {
    /// `$v + C`, `$v - C`, or `$v * C` (with the literal on either side for `+` and `*`).
    Single { constant: i64, op: StoredNumericMapOp },
    /// `$v * A + B` (and its commutative/negated forms), evaluated multiply-then-add.
    Affine { scale: i64, offset: i64 },
    /// `$v * $v`, multiplied by itself.
    Square,
    /// `$v * $v + C` (or `C + $v * $v`, `$v * $v - C`), squared then offset.
    SquareAffine { offset: i64 },
    /// `$v * F` (or `F * $v`) with a floating-point literal scale.
    FloatScale { scale: f64 },
    /// `$v * F + G` (and its commutative/negated forms), evaluated multiply-then-add.
    /// The float add/sub shapes reuse it with a unit scale.
    FloatAffine { scale: f64, offset: f64 },
    /// `$v / F` with a nonzero floating-point literal divisor.
    FloatDiv { divisor: f64 },
    /// `$v / C` with a nonzero integer literal divisor.
    ///
    /// PHP keeps an int for exact division and promotes to float otherwise.
    IntDiv { divisor: i64 },
}

/// The single-operation map selector carried in `StoredNumericMapShape::Single`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredNumericMapOp {
    Add,
    Sub,
    Mul,
}

/// Parses one capture-free callback body into a supported single-op or affine map.
pub(crate) fn parse_stored_numeric_map(
    body: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    let ExprKind::BinaryOp { left, op, right } = &body.kind else {
        return None;
    };
    parse_single(left, op, right, param)
        .or_else(|| parse_square(left, op, right, param))
        .or_else(|| parse_square_affine(left, op, right, param))
        .or_else(|| parse_affine(left, op, right, param))
        .or_else(|| parse_float_affine(left, op, right, param))
        .or_else(|| parse_float_add(left, op, right, param))
        .or_else(|| parse_int_div(left, op, right, param))
        .or_else(|| parse_float_div(left, op, right, param))
        .or_else(|| parse_float_scale(left, op, right, param))
}

/// Parses `$v / C` with a nonzero integer literal divisor.
///
/// A zero divisor keeps the generic map so PHP's `DivisionByZeroError` is preserved.
fn parse_int_div(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    if !matches!(op, BinOp::Div)
        || !matches!(&left.kind, ExprKind::Variable(name) if name == param)
    {
        return None;
    }
    let divisor = literal_i64(right)?;
    if divisor == 0 {
        return None;
    }
    Some(StoredNumericMapShape::IntDiv { divisor })
}

/// Parses `$v + F`, `F + $v`, and `$v - F` with a floating-point literal operand.
///
/// The float-affine kernel with a unit scale evaluates `double($v) * 1.0 + offset`, which
/// is exactly PHP's int-to-double conversion followed by the float addition.
fn parse_float_add(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    match op {
        BinOp::Add => {
            if matches!(&left.kind, ExprKind::Variable(name) if name == param) {
                return literal_f64(right).map(|offset| {
                    StoredNumericMapShape::FloatAffine { scale: 1.0, offset }
                });
            }
            if matches!(&right.kind, ExprKind::Variable(name) if name == param) {
                return literal_f64(left).map(|offset| {
                    StoredNumericMapShape::FloatAffine { scale: 1.0, offset }
                });
            }
        }
        BinOp::Sub => {
            if matches!(&left.kind, ExprKind::Variable(name) if name == param) {
                return literal_f64(right).map(|offset| {
                    StoredNumericMapShape::FloatAffine {
                        scale: 1.0,
                        offset: -offset,
                    }
                });
            }
        }
        _ => {}
    }
    None
}

/// Parses `$v / F` with a nonzero floating-point literal divisor.
///
/// An integer divisor is rejected because PHP returns an int for evenly divisible int
/// operands. A zero divisor is rejected so the generic map keeps PHP's DivisionByZeroError.
fn parse_float_div(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    if !matches!(op, BinOp::Div)
        || !matches!(&left.kind, ExprKind::Variable(name) if name == param)
    {
        return None;
    }
    let divisor = literal_f64(right)?;
    if divisor == 0.0 {
        return None;
    }
    Some(StoredNumericMapShape::FloatDiv { divisor })
}

/// Parses `$v * F + G`, `G + $v * F`, `$v * F - G`, and `G - $v * F`.
fn parse_float_affine(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    match op {
        BinOp::Add => {
            if let (Some(scale), Some(offset)) =
                (float_scale_expr(left, param), literal_f64_any(right))
            {
                return Some(StoredNumericMapShape::FloatAffine { scale, offset });
            }
            if let (Some(offset), Some(scale)) =
                (literal_f64_any(left), float_scale_expr(right, param))
            {
                return Some(StoredNumericMapShape::FloatAffine { scale, offset });
            }
        }
        BinOp::Sub => {
            if let (Some(scale), Some(offset)) =
                (float_scale_expr(left, param), literal_f64_any(right))
            {
                return Some(StoredNumericMapShape::FloatAffine {
                    scale,
                    offset: -offset,
                });
            }
            if let (Some(offset), Some(scale)) =
                (literal_f64_any(left), float_scale_expr(right, param))
            {
                return Some(StoredNumericMapShape::FloatAffine {
                    scale: -scale,
                    offset,
                });
            }
        }
        _ => {}
    }
    None
}

/// Returns the floating-point scale in `$v * F` or `F * $v`.
fn float_scale_expr(expr: &Expr, param: &str) -> Option<f64> {
    let ExprKind::BinaryOp { left, op: BinOp::Mul, right } = &expr.kind else {
        return None;
    };
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), _) if name == param => literal_f64_any(right),
        (_, ExprKind::Variable(name)) if name == param => literal_f64_any(left),
        _ => None,
    }
}

/// Returns a floating-point or integer literal as an exact double.
fn literal_f64_any(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::FloatLiteral(value) => Some(*value),
        ExprKind::IntLiteral(value) => Some(*value as f64),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::FloatLiteral(value) => Some(-value),
            ExprKind::IntLiteral(value) => Some(-(*value as f64)),
            _ => None,
        },
        _ => None,
    }
}

/// Parses `$v * F` or `F * $v` with a floating-point literal scale.
fn parse_float_scale(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    if !matches!(op, BinOp::Mul) {
        return None;
    }
    let scale = match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), _) if name == param => literal_f64(right)?,
        (_, ExprKind::Variable(name)) if name == param => literal_f64(left)?,
        _ => return None,
    };
    Some(StoredNumericMapShape::FloatScale { scale })
}

/// Returns a floating-point literal, including a parser-level unary negation.
fn literal_f64(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::FloatLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::FloatLiteral(value) => Some(-value),
            _ => None,
        },
        _ => None,
    }
}

/// Parses `$v * $v + C`, `C + $v * $v`, and `$v * $v - C`.
fn parse_square_affine(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    match op {
        BinOp::Add => {
            if is_square(left, param) {
                return literal_i64(right).map(|offset| {
                    StoredNumericMapShape::SquareAffine { offset }
                });
            }
            if is_square(right, param) {
                return literal_i64(left).map(|offset| {
                    StoredNumericMapShape::SquareAffine { offset }
                });
            }
        }
        BinOp::Sub => {
            if is_square(left, param) {
                return literal_i64(right)
                    .and_then(i64::checked_neg)
                    .map(|offset| StoredNumericMapShape::SquareAffine { offset });
            }
        }
        _ => {}
    }
    None
}

/// Returns whether `expr` is `$param * $param`.
fn is_square(expr: &Expr, param: &str) -> bool {
    let ExprKind::BinaryOp { left, op: BinOp::Mul, right } = &expr.kind else {
        return false;
    };
    matches!(&left.kind, ExprKind::Variable(name) if name == param)
        && matches!(&right.kind, ExprKind::Variable(name) if name == param)
}

/// Parses `$v * $v`.
fn parse_square(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    if !matches!(op, BinOp::Mul)
        || !matches!(&left.kind, ExprKind::Variable(name) if name == param)
        || !matches!(&right.kind, ExprKind::Variable(name) if name == param)
    {
        return None;
    }
    Some(StoredNumericMapShape::Square)
}

/// Parses the original single-operation shapes.
fn parse_single(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    match op {
        BinOp::Add => Some(StoredNumericMapShape::Single {
            constant: additive_literal(left, right, param)?,
            op: StoredNumericMapOp::Add,
        }),
        BinOp::Sub => {
            if !matches!(&left.kind, ExprKind::Variable(name) if name == param) {
                return None;
            }
            Some(StoredNumericMapShape::Single {
                constant: literal_i64(right)?,
                op: StoredNumericMapOp::Sub,
            })
        }
        BinOp::Mul => {
            let constant = match (&left.kind, &right.kind) {
                (ExprKind::Variable(name), _) if name == param => literal_i64(right)?,
                (_, ExprKind::Variable(name)) if name == param => literal_i64(left)?,
                _ => return None,
            };
            Some(StoredNumericMapShape::Single {
                constant,
                op: StoredNumericMapOp::Mul,
            })
        }
        _ => None,
    }
}

/// Parses `$v * A + B`, `$v * A - B`, `B + $v * A`, and `B - $v * A`.
fn parse_affine(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    param: &str,
) -> Option<StoredNumericMapShape> {
    match op {
        BinOp::Add => {
            if let (Some(scale), Some(offset)) = (scale_literal(left, param), literal_i64(right)) {
                return Some(StoredNumericMapShape::Affine { scale, offset });
            }
            if let (Some(offset), Some(scale)) = (literal_i64(left), scale_literal(right, param)) {
                return Some(StoredNumericMapShape::Affine { scale, offset });
            }
        }
        BinOp::Sub => {
            if let (Some(scale), Some(offset)) = (scale_literal(left, param), literal_i64(right)) {
                return Some(StoredNumericMapShape::Affine {
                    scale,
                    offset: offset.checked_neg()?,
                });
            }
            if let (Some(offset), Some(scale)) = (literal_i64(left), scale_literal(right, param)) {
                return Some(StoredNumericMapShape::Affine {
                    scale: scale.checked_neg()?,
                    offset,
                });
            }
        }
        _ => {}
    }
    None
}

/// Returns the literal scale in `$v * A` or `A * $v`.
fn scale_literal(expr: &Expr, param: &str) -> Option<i64> {
    let ExprKind::BinaryOp { left, op: BinOp::Mul, right } = &expr.kind else {
        return None;
    };
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), _) if name == param => literal_i64(right),
        (_, ExprKind::Variable(name)) if name == param => literal_i64(left),
        _ => None,
    }
}

/// Returns an integer literal, including a parser-level unary negation.
fn literal_i64(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

/// Validates a candidate map call and its source.
///
/// A variable source is only checked for reference aliasing here; its packed-`array<int>`
/// type is proven by the lowering context at the actual call site, which is flow-sensitive
/// and therefore also works for locals assigned inside user functions. A failed proof falls
/// back to the ordinary `array_map` lowering.
fn stored_map_shape(
    value: &Expr,
    aliased: &HashSet<String>,
    functions: &HashMap<String, FunctionSig>,
) -> Option<()> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if name.as_str().trim_start_matches('\\') != "array_map" || args.len() != 2 {
        return None;
    }
    if !is_packed_int_source_candidate(&args[1], functions)
        || aliased.iter().any(|name| expr_mentions(&args[1], name))
    {
        return None;
    }
    let (param, body) = single_param_closure(&args[0])?;
    parse_stored_numeric_map(body, param)?;
    Some(())
}

/// Returns whether an `array_slice()` fourth argument keeps the packed-list layout.
///
/// PHP renumbers keys by default; `preserve_keys: true` can turn a slice that no longer
/// starts at key 0 into a hash, which the stored-map kernels cannot read as packed slots.
pub(crate) fn slice_preserves_packed_layout(arg: Option<&Expr>) -> bool {
    arg.is_none_or(|arg| matches!(arg.kind, ExprKind::BoolLiteral(false)))
}

/// Returns whether the expression is a side-effect-free source shape the stored-map marker
/// may consider: a variable, an all-int array literal, `range()` with int literals, or
/// `array_slice()` over one of those with default key renumbering.
///
/// A variable's actual `array<int>` proof is deferred to lowering, which sees the
/// flow-sensitive local type. Every other nested source is proven here structurally.
pub(crate) fn is_packed_int_source_candidate(
    expr: &Expr,
    functions: &HashMap<String, FunctionSig>,
) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) => true,
        ExprKind::ArrayLiteral(_) => is_int_literal_array(expr),
        ExprKind::FunctionCall { name, args } => {
            let key = crate::names::php_symbol_key(name.as_str().trim_start_matches('\\'));
            if functions.contains_key(key.as_str())
                || args.iter().any(|arg| {
                    matches!(arg.kind, ExprKind::Spread(_) | ExprKind::NamedArg { .. })
                })
            {
                return false;
            }
            match key.as_str() {
                "range" => {
                    (2..=3).contains(&args.len())
                        && args.iter().all(|arg| literal_i64(arg).is_some())
                }
                "array_slice" => {
                    (2..=4).contains(&args.len())
                        && slice_preserves_packed_layout(args.get(3))
                        && is_packed_int_source_candidate(&args[0], functions)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Returns whether the expression is a non-empty array literal whose items are all integer
/// literals, which the checker always types as a packed `array<int>`.
pub(crate) fn is_int_literal_array(expr: &Expr) -> bool {
    let ExprKind::ArrayLiteral(items) = &expr.kind else {
        return false;
    };
    !items.is_empty()
        && items.iter().all(|item| {
            matches!(item.kind, ExprKind::IntLiteral(_))
                || matches!(
                    &item.kind,
                    ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_))
                )
        })
}

/// Extracts the single parameter and return expression of a capture-free untyped closure.
fn single_param_closure(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type: None,
        body,
        captures,
        capture_refs,
        by_ref_return: false,
        ..
    } = &expr.kind
    else {
        return None;
    };
    let [(param, param_type, default, by_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int))
        || default.is_some()
        || *by_ref
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return None;
    }
    let [Stmt { kind: StmtKind::Return(Some(body)), .. }] = body.as_slice() else {
        return None;
    };
    Some((param.as_str(), body))
}

/// Returns `$param + $literal` or `$literal + $param`.
fn additive_literal(left: &Expr, right: &Expr, param: &str) -> Option<i64> {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), _) if name == param => literal_i64(right),
        (_, ExprKind::Variable(name)) if name == param => literal_i64(left),
        _ => None,
    }
}

/// Collects the destination local and every simple value alias assigned from it.
fn aliases_of(body: &[Stmt], mapped: &str) -> HashSet<String> {
    let mut aliases = HashSet::from([mapped.to_string()]);
    loop {
        let mut changed = false;
        collect_alias_assignments(body, &mut aliases, &mut changed);
        if !changed {
            return aliases;
        }
    }
}

/// Adds one level of simple `$alias = $tracked` assignments.
fn collect_alias_assignments(stmts: &[Stmt], aliases: &mut HashSet<String>, changed: &mut bool) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if let ExprKind::Variable(source) = &value.kind {
                    if aliases.contains(source) && aliases.insert(name.clone()) {
                        *changed = true;
                    }
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body) => collect_alias_assignments(body, aliases, changed),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_alias_assignments(then_body, aliases, changed);
                for (_, body) in elseif_clauses {
                    collect_alias_assignments(body, aliases, changed);
                }
                if let Some(body) = else_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                collect_alias_assignments(then_body, aliases, changed);
                if let Some(body) = else_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_alias_assignments(body, aliases, changed);
                }
                if let Some(body) = default {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                collect_alias_assignments(try_body, aliases, changed);
                for catch in catches {
                    collect_alias_assignments(&catch.body, aliases, changed);
                }
                if let Some(body) = finally_body {
                    collect_alias_assignments(body, aliases, changed);
                }
            }
            _ => {}
        }
    }
}

/// Returns whether every tracked use is storage-neutral.
fn uses_are_neutral(
    body: &[Stmt],
    tracked: &HashSet<String>,
    mapped: &str,
    candidate_span: Span,
) -> bool {
    body.iter()
        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
}

/// Checks one statement's tracked uses.
fn stmt_neutral(stmt: &Stmt, tracked: &HashSet<String>, mapped: &str, candidate_span: Span) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if name == mapped && stmt.span == candidate_span {
                return true;
            }
            if tracked.contains(name) {
                return matches!(&value.kind, ExprKind::Variable(source) if tracked.contains(source));
            }
            expr_neutral(value, tracked)
        }
        StmtKind::RefAssign { .. } => !any_tracked_stmt(stmt, tracked),
        // Any mutation of the mapped array, direct or through an alias, keeps the generic
        // boxed map. This is the ownership/COW boundary: the raw representation is read-only.
        StmtKind::ArrayAssign { array, index, value } => {
            !tracked.contains(array)
                && !expr_mentions_any(index, tracked)
                && expr_neutral(index, tracked)
                && expr_neutral(value, tracked)
        }
        StmtKind::ArrayPush { array, value } => {
            !tracked.contains(array) && !expr_mentions_any(value, tracked) && expr_neutral(value, tracked)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            root_name(target).is_none_or(|name| !tracked.contains(name))
                && !expr_mentions_any(target, tracked)
                && expr_neutral(value, tracked)
        }
        StmtKind::Foreach { array, key_var, value_var, value_by_ref, body } => {
            if *value_by_ref {
                return !expr_mentions_any(array, tracked);
            }
            if let ExprKind::Variable(name) = &array.kind {
                if tracked.contains(name) {
                    if key_var.as_ref().is_some_and(|key| tracked.contains(key))
                        || tracked.contains(value_var)
                    {
                        return false;
                    }
                    return body
                        .iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span));
                }
            }
            expr_neutral(array, tracked)
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_ref()
                .is_none_or(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && condition.as_ref().is_none_or(|expr| expr_neutral(expr, tracked))
                && update
                    .as_ref()
                    .is_none_or(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_neutral(condition, tracked)
                && body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_neutral(condition, tracked)
                && then_body
                    .iter()
                    .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && elseif_clauses.iter().all(|(condition, body)| {
                    expr_neutral(condition, tracked)
                        && body
                            .iter()
                            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && else_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            then_body
                .iter()
                .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && else_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Switch { subject, cases, default } => {
            expr_neutral(subject, tracked)
                && cases.iter().all(|(conditions, body)| {
                    conditions.iter().all(|condition| expr_neutral(condition, tracked))
                        && body
                            .iter()
                            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && default.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            try_body
                .iter()
                .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                && catches.iter().all(|catch| {
                    catch
                        .body
                        .iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
                && finally_body.as_ref().is_none_or(|body| {
                    body.iter()
                        .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span))
                })
        }
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. } => body
            .iter()
            .all(|stmt| stmt_neutral(stmt, tracked, mapped, candidate_span)),
        StmtKind::Echo(value) | StmtKind::Throw(value) | StmtKind::ExprStmt(value) => {
            expr_neutral(value, tracked)
        }
        StmtKind::Return(Some(value)) => expr_neutral(value, tracked),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => true,
        StmtKind::Include { path, .. } => expr_neutral(path, tracked),
        StmtKind::Global { vars } => !vars.iter().any(|name| tracked.contains(name)),
        StmtKind::StaticVar { name, .. } => !tracked.contains(name),
        _ => !any_tracked_stmt(stmt, tracked),
    }
}

/// Returns whether an expression is a storage-neutral tracked use.
fn expr_neutral(expr: &Expr, tracked: &HashSet<String>) -> bool {
    if !expr_mentions_any(expr, tracked) {
        return true;
    }
    match &expr.kind {
        ExprKind::ArrayAccess { array, index } => {
            if let ExprKind::Variable(name) = &array.kind {
                if tracked.contains(name) {
                    return expr_neutral(index, tracked);
                }
            }
            false
        }
        ExprKind::FunctionCall { name, args } => {
            let canonical = name.as_str().trim_start_matches('\\');
            if !neutral_builtin(canonical) {
                return false;
            }
            args.iter().all(|arg| match &arg.kind {
                ExprKind::Variable(_) => true,
                _ => expr_neutral(arg, tracked),
            })
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce { value: left, default: right }
        | ExprKind::ShortTernary { value: left, default: right }
        | ExprKind::Pipe { value: left, callable: right } => {
            expr_neutral(left, tracked) && expr_neutral(right, tracked)
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_neutral(condition, tracked)
                && expr_neutral(then_expr, tracked)
                && expr_neutral(else_expr, tracked)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::InstanceOf { value: inner, .. } => expr_neutral(inner, tracked),
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| expr_neutral(item, tracked)),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .all(|(key, value)| expr_neutral(key, tracked) && expr_neutral(value, tracked)),
        ExprKind::Match { subject, arms, default } => {
            expr_neutral(subject, tracked)
                && arms.iter().all(|(conditions, value)| {
                    conditions.iter().all(|condition| expr_neutral(condition, tracked))
                        && expr_neutral(value, tracked)
                })
                && default.as_ref().is_none_or(|value| expr_neutral(value, tracked))
        }
        _ => false,
    }
}

/// Returns whether a builtin observes the array without assuming boxed Mixed slots.
fn neutral_builtin(name: &str) -> bool {
    matches!(
        name,
        "count"
            | "sizeof"
            | "gettype"
            | "var_dump"
            | "print_r"
            | "implode"
            | "join"
            | "json_encode"
            | "array_sum"
            | "array_values"
            | "array_keys"
            | "array_key_exists"
            | "array_is_list"
    )
}

/// Returns whether any tracked variable occurs in an expression.
fn expr_mentions_any(expr: &Expr, tracked: &HashSet<String>) -> bool {
    tracked.iter().any(|name| expr_mentions(expr, name))
}

/// Returns whether any tracked variable occurs anywhere in a statement.
fn any_tracked_stmt(stmt: &Stmt, tracked: &HashSet<String>) -> bool {
    tracked.iter().any(|name| stmt_mentions(stmt, name))
}

/// Returns the root local of an array-access target, if any.
fn root_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => root_name(array),
        _ => None,
    }
}
