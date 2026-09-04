//! Purpose:
//! Models expression and statement side effects for optimizer safety decisions.
//! Classifies reads, writes, calls, throws, output, and runtime-state interactions used by DCE and pruning.
//!
//! Called from:
//! - `crate::optimize::prune_constant_control_flow()`
//! - `crate::optimize::eliminate_dead_code()`
//!
//! Key details:
//! - Effects are deliberately conservative; purity must not be claimed for code that can observe or mutate PHP/runtime state.

use super::*;

mod aliases;
mod calls;

use aliases::apply_stmt_callable_aliases;
pub(super) use calls::{
    callable_alias_effect,
    expr_call_effect,
    function_call_effect,
    instance_property_read_effect,
    instance_method_call_effect,
    static_method_call_effect,
};

/// Returns true when a binary operator can raise a catchable PHP error at runtime.
///
/// PHP 8 arithmetic is not exception-free: `/` and `%` raise `DivisionByZeroError` for a zero
/// divisor, and `<<` / `>>` raise `ArithmeticError` for a negative shift count. Reporting these
/// as pure would let the try/catch DCE pass (`optimize::control::dce::tries`) drop the very
/// `catch` clause that is supposed to observe them. A literal right operand that is provably
/// safe keeps the expression pure so ordinary arithmetic is not pessimized.
fn binary_op_may_throw(op: &BinOp, right: &Expr) -> bool {
    match op {
        BinOp::Div | BinOp::Mod => !matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value != 0
        ) && !matches!(
            &right.kind,
            ExprKind::FloatLiteral(value) if *value != 0.0
        ),
        BinOp::ShiftLeft | BinOp::ShiftRight => !matches!(
            &right.kind,
            ExprKind::IntLiteral(value) if *value >= 0
        ),
        _ => false,
    }
}

/// Returns true when rendering `expr` as a string can raise a catchable php `Error`.
///
/// php converts a value to a string for `echo`, `print`, `.`, `.=` and `(string)`. An object
/// whose class publishes no `__toString` raises `Error: Object of class X could not be converted
/// to string` — a Closure, which first-class callable syntax produces, is one of them — and
/// `catch (Error $e)` observes it. Reporting the conversion as pure let the try/catch DCE pass
/// (`optimize::control::dce::tries`) drop the very `catch` clause meant to see it, so
/// `try { echo $o; } catch (Error $e)` never ran its catch.
///
/// A literal operand cannot be an object, so it keeps the expression pure and ordinary output —
/// `echo "text"`, the overwhelmingly common case — is not pessimized. The same shape as
/// [`binary_op_may_throw`], for the same reason.
fn string_conversion_may_throw(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => false,
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => string_conversion_may_throw(left) || string_conversion_may_throw(right),
        _ => true,
    }
}

/// Returns true if any statement in `stmts` may throw an exception.
/// Shorthand for checking `block_effect(stmts).may_throw`.
pub(super) fn block_may_throw(stmts: &[Stmt]) -> bool {
    block_effect(stmts).may_throw
}

/// Returns true if `stmt` may throw an exception.
/// Shorthand for `stmt_effect(stmt).may_throw`.
pub(super) fn stmt_may_throw(stmt: &Stmt) -> bool {
    stmt_effect(stmt).may_throw
}

/// Computes the combined `Effect` for a single statement, including all nested expressions.
/// Covers all `StmtKind` variants, classifying reads, writes, calls, throws, output, and runtime-state interactions.
pub(super) fn stmt_effect(stmt: &Stmt) -> Effect {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) => block_effect(stmts),
        StmtKind::IncludeOnceMark { .. } => Effect::PURE.with_side_effects(),
        StmtKind::IncludeOnceGuard { body, .. } => block_effect(body).with_side_effects(),
        StmtKind::Echo(expr) => {
            let effect = expr_effect(expr).with_side_effects();
            if string_conversion_may_throw(expr) {
                effect.with_may_throw()
            } else {
                effect
            }
        }
        StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr)) => expr_effect(expr),
        StmtKind::Throw(expr) => expr_effect(expr).with_side_effects().with_may_throw(),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. } => {
            expr_effect(value).with_side_effects()
        }
        StmtKind::RefAssign { .. } => Effect::PURE.with_side_effects(),
        StmtKind::ArrayPush { value, .. } | StmtKind::StaticPropertyArrayPush { value, .. } => {
            expr_effect(value).with_side_effects().with_may_throw()
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_effect(index)
                .combine(expr_effect(value))
                .with_side_effects()
                .with_may_throw()
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_effect(target)
                .combine(expr_effect(value))
                .with_side_effects()
                .with_may_throw()
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_effect(object)
                .combine(expr_effect(value))
                .with_side_effects()
                .with_may_throw()
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_effect(object)
                .combine(expr_effect(value))
                .with_side_effects()
                .with_may_throw()
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_effect(condition)
            .combine(block_effect(then_body))
            .combine(combine_effects(
                elseif_clauses.iter().map(|(condition, body)| {
                    expr_effect(condition).combine(block_effect(body))
                }),
            ))
            .combine(
                else_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => block_effect(then_body).combine(
            else_body
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_effect(condition).combine(block_effect(body))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_ref()
            .map(|stmt| stmt_effect(stmt))
            .unwrap_or(Effect::PURE)
            .combine(
                condition
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    .unwrap_or(Effect::PURE),
            )
            .combine(
                update
                    .as_ref()
                    .map(|stmt| stmt_effect(stmt))
                    .unwrap_or(Effect::PURE),
            )
            .combine(block_effect(body)),
        StmtKind::Foreach { array, body, .. } => expr_effect(array)
            .combine(block_effect(body))
            .with_side_effects(),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_effect(subject).combine(combine_effects(cases.iter().map(|(patterns, body)| {
            combine_effects(patterns.iter().map(expr_effect)).combine(block_effect(body))
        })))
        .combine(
            default
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => block_effect(try_body)
            .combine(combine_effects(
                catches.iter().map(|catch| block_effect(&catch.body)),
            ))
            .combine(
                finally_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::NamespaceBlock { body, .. } => block_effect(body),
        // Declaring `global` conservatively counts as writing global storage
        // (a declaration almost always precedes a write). The bit stays out of
        // `is_observable`, so the declaration itself remains removable.
        StmtKind::Global { .. } => Effect::PURE.with_writes_globals(),
        StmtKind::FunctionDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Effect::PURE,
        StmtKind::FunctionVariantGroup { .. } => Effect::PURE,
        StmtKind::FunctionVariantMark { .. } => Effect::PURE.with_side_effects(),
        StmtKind::Include { .. } => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

/// Returns true if `expr` may produce observable side effects (writes, calls, output, or throws).
/// Used by DCE to determine whether discarding the expression would be observable.
pub(super) fn expr_is_observable(expr: &Expr) -> bool {
    expr_effect(expr).is_observable()
}

/// Computes the combined `Effect` for an expression, including all sub-expressions and call effects.
/// Covers all `ExprKind` variants, classifying reads, writes, calls, throws, output, and runtime-state interactions.
/// For `MagicConstant`, returns `unreachable!` because they must be lowered before optimizer passes.
pub(super) fn expr_effect(expr: &Expr) -> Effect {
    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This => Effect::PURE,
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Spread(inner) => expr_effect(inner),
        ExprKind::Cast { expr: inner, target } => {
            let effect = expr_effect(inner);
            // Only the STRING cast renders its operand; `(int)`, `(bool)` and the rest coerce an
            // object without ever calling `__toString`.
            if matches!(target, CastType::String) && string_conversion_may_throw(inner) {
                effect.with_may_throw()
            } else {
                effect
            }
        }
        ExprKind::Print(inner) => {
            let effect = expr_effect(inner).with_side_effects();
            if string_conversion_may_throw(inner) {
                effect.with_may_throw()
            } else {
                effect
            }
        }
        ExprKind::Clone(inner) => expr_effect(inner)
            .with_side_effects()
            .with_may_throw(),
        ExprKind::BinaryOp { left, op, right } => {
            let operands = expr_effect(left).combine(expr_effect(right));
            // A concatenation renders BOTH operands, so either side can raise the
            // object-to-string `Error`; every other operator is judged by its right operand.
            let concat_throws = matches!(op, BinOp::Concat)
                && (string_conversion_may_throw(left) || string_conversion_may_throw(right));
            if binary_op_may_throw(op, right) || concat_throws {
                operands.with_may_throw()
            } else {
                operands
            }
        }
        ExprKind::InstanceOf { value, target } => {
            expr_effect(value).combine(instanceof_target_effect(target))
        }
        ExprKind::Throw(inner) => expr_effect(inner).with_side_effects().with_may_throw(),
        ExprKind::NullCoalesce { value, default } => expr_effect(value).combine(expr_effect(default)),
        ExprKind::Pipe { value, callable } => expr_effect(value)
            .combine(expr_effect(callable))
            .combine(expr_call_effect(callable)),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => block_effect(prelude)
            .combine(expr_effect(target))
            .combine(expr_effect(value))
            .combine(
                result_target
                    .as_deref()
                    .map(expr_effect)
                    .unwrap_or(Effect::PURE),
            )
            .with_side_effects(),
        ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) => Effect::PURE.with_side_effects(),
        ExprKind::FunctionCall { name, args } => combine_effects(args.iter().map(expr_effect))
            .combine(function_call_effect(name.as_str(), args)),
        ExprKind::ClosureCall { var, args } => combine_effects(args.iter().map(expr_effect))
            .combine(callable_alias_effect(var)),
        ExprKind::ExprCall { callee, args } => expr_effect(callee)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(expr_call_effect(callee)),
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_effect(object)
            .combine(expr_effect(method))
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(match &method.kind {
                ExprKind::StringLiteral(method) => {
                    instance_method_call_effect(object, method)
                }
                _ => Effect::PURE
                    .with_side_effects()
                    .with_may_throw()
                    .with_writes_globals(),
            }),
        ExprKind::NewObject { args, .. } => combine_effects(args.iter().map(expr_effect))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::NewDynamic { name_expr, args } => expr_effect(name_expr)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => expr_effect(class_name)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::MethodCall { object, method, args } => expr_effect(object)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(instance_method_call_effect(object, method)),
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => expr_effect(object)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(instance_method_call_effect(object, method)),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => combine_effects(args.iter().map(expr_effect))
            .combine(static_method_call_effect(receiver, method)),
        ExprKind::ArrayLiteral(items) => combine_effects(items.iter().map(expr_effect)),
        ExprKind::ArrayLiteralAssoc(items) => combine_effects(
            items
                .iter()
                .map(|(key, value)| expr_effect(key).combine(expr_effect(value))),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_effect(subject)
            .combine(combine_effects(arms.iter().map(|(patterns, value)| {
                combine_effects(patterns.iter().map(expr_effect)).combine(expr_effect(value))
            })))
            .combine(
                default
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    // A match with NO default is the one that can THROW: php raises
                    // `UnhandledMatchError` when no arm answers. Calling that absence PURE made a
                    // match in STATEMENT position — its value discarded — a pure expression the
                    // statement pruner dropped whole, so `match (99) { 1 => "a" };` did nothing
                    // where php throws.
                    .unwrap_or_else(|| Effect::PURE.with_may_throw()),
            ),
        ExprKind::ArrayAccess { array, index } => {
            let evaluated = expr_effect(array).combine(expr_effect(index));
            match statically_known_array_read(array, index) {
                Some(true) => evaluated,
                Some(false) => evaluated.with_side_effects(),
                None => evaluated.with_side_effects().with_may_throw(),
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_effect(condition)
            .combine(expr_effect(then_expr))
            .combine(expr_effect(else_expr)),
        ExprKind::ShortTernary { value, default } => {
            expr_effect(value).combine(expr_effect(default))
        }
        ExprKind::Closure { .. } => Effect::PURE,
        ExprKind::NamedArg { value, .. } => expr_effect(value),
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => expr_effect(object)
            .combine(instance_property_read_effect(object, property)),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => expr_effect(object)
            .combine(expr_effect(property))
            .combine(match &property.kind {
                ExprKind::StringLiteral(property) => {
                    instance_property_read_effect(object, property)
                }
                _ => Effect::PURE
                    .with_side_effects()
                    .with_may_throw()
                    .with_writes_globals(),
            }),
        ExprKind::StaticPropertyAccess { .. } => Effect::PURE.with_may_throw(),
        ExprKind::FirstClassCallable(target) => callable_target_effect(target),
        ExprKind::BufferNew { len, .. } => expr_effect(len).with_side_effects(),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => Effect::PURE,
        ExprKind::ObjectClassName { object } => expr_effect(object),
        ExprKind::NewScopedObject { args, .. } => combine_effects(args.iter().map(expr_effect))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::Yield { key, value } => {
            let mut e = Effect::PURE.with_side_effects().with_may_throw();
            if let Some(k) = key {
                e = e.combine(expr_effect(k));
            }
            if let Some(v) = value {
                e = e.combine(expr_effect(v));
            }
            e
        }
        ExprKind::YieldFrom(inner) => expr_effect(inner)
            .with_side_effects()
            .with_may_throw(),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    }
}

/// Returns whether a literal container read is statically in bounds/present.
///
/// `Some(true)` is a warning-free read, `Some(false)` is a known missing offset that can warn
/// but cannot throw, and `None` means runtime type/key behavior remains dynamic.
fn statically_known_array_read(array: &Expr, index: &Expr) -> Option<bool> {
    match (&array.kind, &index.kind) {
        (ExprKind::ArrayLiteral(items), ExprKind::IntLiteral(index))
            if items
                .iter()
                .all(|item| !matches!(item.kind, ExprKind::Spread(_))) =>
        {
            let len = i64::try_from(items.len()).ok()?;
            Some(*index >= 0 && *index < len)
        }
        (ExprKind::ArrayLiteralAssoc(items), ExprKind::IntLiteral(index)) => Some(
            items
                .iter()
                .any(|(key, _)| matches!(key.kind, ExprKind::IntLiteral(key) if key == *index)),
        ),
        (ExprKind::ArrayLiteralAssoc(items), ExprKind::StringLiteral(index)) => Some(
            items.iter().any(
                |(key, _)| matches!(&key.kind, ExprKind::StringLiteral(key) if key == index),
            ),
        ),
        (ExprKind::StringLiteral(value), ExprKind::IntLiteral(index)) => {
            let len = i64::try_from(value.len()).ok()?;
            Some((*index >= 0 && *index < len) || (*index < 0 && -*index <= len))
        }
        _ => None,
    }
}

/// Returns the effect for the target of an `InstanceOf` expression.
/// Name targets are pure; expression targets require recursive `expr_effect` analysis.
fn instanceof_target_effect(target: &InstanceOfTarget) -> Effect {
    match target {
        InstanceOfTarget::Name(_) => Effect::PURE,
        InstanceOfTarget::Expr(expr) => expr_effect(expr),
    }
}

/// Computes the combined `Effect` for a block of statements, short-circuiting on non-falling terminators.
/// Tracks callable aliases across statements to correctly model closure captures and callable aliases.
/// Stops accumulating effects when a `return`/`break`/`continue`/`throw` is encountered.
pub(super) fn block_effect(stmts: &[Stmt]) -> Effect {
    let mut aliases = current_callable_alias_effects();
    let mut effect = Effect::PURE;
    for stmt in stmts {
        let stmt_effect = with_callable_alias_effects(aliases.clone(), || stmt_effect(stmt));
        effect = effect.combine(stmt_effect);
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    effect
}

/// Combines an arbitrary iterator of `Effect` values into a single `Effect` by folding with `combine`.
/// Returns `Effect::PURE` when the iterator is empty.
pub(super) fn combine_effects(effects: impl IntoIterator<Item = Effect>) -> Effect {
    effects
        .into_iter()
        .fold(Effect::PURE, |acc, effect| acc.combine(effect))
}
