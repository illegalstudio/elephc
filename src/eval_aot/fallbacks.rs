//! Purpose:
//! Classifies the first statement or expression that requires eval fallback.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Human-readable fallback markers remain conservative and deterministic.

use super::*;

/// Classifies the first visible reason this fragment cannot avoid the bridge.
pub(super) fn classify_fallback_reason(program: &[Stmt]) -> EvalAotFallbackReason {
    program
        .iter()
        .find_map(stmt_fallback_reason)
        .unwrap_or(EvalAotFallbackReason::UnsupportedScope)
}

/// Classifies one statement for a human-readable eval AOT fallback marker.
pub(super) fn stmt_fallback_reason(stmt: &Stmt) -> Option<EvalAotFallbackReason> {
    match &stmt.kind {
        StmtKind::Include { .. } | StmtKind::IncludeOnceMark { .. } => {
            Some(EvalAotFallbackReason::IncludeOrRequire)
        }
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().find_map(stmt_fallback_reason),
        StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Some(EvalAotFallbackReason::Declaration),
        StmtKind::Global { .. } | StmtKind::StaticVar { .. } => {
            Some(EvalAotFallbackReason::GlobalOrStatic)
        }
        StmtKind::RefAssign { .. } => Some(EvalAotFallbackReason::ReferenceOrByRef),
        StmtKind::Foreach {
            array,
            value_by_ref,
            body,
            ..
        } => {
            if *value_by_ref {
                return Some(EvalAotFallbackReason::ReferenceOrByRef);
            }
            expr_fallback_reason(array)
                .or_else(|| body.iter().find_map(stmt_fallback_reason))
                .or(Some(EvalAotFallbackReason::ArrayOrIterable))
        }
        StmtKind::Try { .. } | StmtKind::Throw(_) => Some(EvalAotFallbackReason::TryOrThrow),
        StmtKind::ArrayAssign { .. }
        | StmtKind::NestedArrayAssign { .. }
        | StmtKind::ArrayPush { .. }
        | StmtKind::ListUnpack { .. } => Some(EvalAotFallbackReason::ArrayOrIterable),
        StmtKind::PropertyAssign { .. }
        | StmtKind::StaticPropertyAssign { .. }
        | StmtKind::StaticPropertyArrayPush { .. }
        | StmtKind::StaticPropertyArrayAssign { .. }
        | StmtKind::PropertyArrayPush { .. }
        | StmtKind::DynamicPropertyArrayPush { .. }
        | StmtKind::PropertyArrayAssign { .. } => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => {
            expr_fallback_reason(expr)
        }
        StmtKind::Assign { value, .. } => expr_fallback_reason(value),
        // Typed local declarations are an elephc extension: under `--strict-php`
        // the fragment must reach the runtime bridge, whose parser rejects the
        // syntax like the PHP interpreter would (runtime parse error), instead
        // of being AOT-compiled and silently executing non-PHP code.
        StmtKind::TypedAssign { value, .. } => {
            if crate::strict_php::is_enabled() {
                return Some(EvalAotFallbackReason::UnsupportedConstruct);
            }
            expr_fallback_reason(value)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_fallback_reason(condition)
            .or_else(|| then_body.iter().find_map(stmt_fallback_reason))
            .or_else(|| {
                elseif_clauses.iter().find_map(|(condition, body)| {
                    expr_fallback_reason(condition)
                        .or_else(|| body.iter().find_map(stmt_fallback_reason))
                })
            })
            .or_else(|| {
                else_body
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            }),
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_fallback_reason(condition).or_else(|| body.iter().find_map(stmt_fallback_reason))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_deref()
            .and_then(stmt_fallback_reason)
            .or_else(|| condition.as_ref().and_then(expr_fallback_reason))
            .or_else(|| update.as_deref().and_then(stmt_fallback_reason))
            .or_else(|| body.iter().find_map(stmt_fallback_reason)),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_fallback_reason(subject)
            .or_else(|| {
                cases.iter().find_map(|(conditions, body)| {
                    conditions
                        .iter()
                        .find_map(expr_fallback_reason)
                        .or_else(|| body.iter().find_map(stmt_fallback_reason))
                })
            })
            .or_else(|| {
                default
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            })
            .or(Some(EvalAotFallbackReason::UnsupportedControlFlow)),
        StmtKind::Break(_) | StmtKind::Continue(_) => {
            Some(EvalAotFallbackReason::UnsupportedControlFlow)
        }
        StmtKind::Return(None) | StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => None,
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            // `ifdef` is an elephc extension: under `--strict-php` the fragment
            // must reach the runtime bridge, whose parser rejects the syntax
            // like the PHP interpreter would, instead of being AOT-compiled.
            if crate::strict_php::is_enabled() {
                return Some(EvalAotFallbackReason::UnsupportedConstruct);
            }
            then_body.iter().find_map(stmt_fallback_reason).or_else(|| {
                else_body
                    .as_deref()
                    .and_then(|body| body.iter().find_map(stmt_fallback_reason))
            })
        }
    }
}

/// Classifies one expression for a human-readable eval AOT fallback marker.
pub(super) fn expr_fallback_reason(expr: &Expr) -> Option<EvalAotFallbackReason> {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => None,
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner) => expr_fallback_reason(inner),
        ExprKind::Throw(_) => Some(EvalAotFallbackReason::TryOrThrow),
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        } => expr_fallback_reason(left)
            .or_else(|| expr_fallback_reason(right))
            .or(Some(EvalAotFallbackReason::ArrayOrIterable)),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_fallback_reason(condition)
            .or_else(|| expr_fallback_reason(then_expr))
            .or_else(|| expr_fallback_reason(else_expr)),
        ExprKind::Cast { target, expr } => {
            if matches!(target, CastType::Array) {
                return Some(EvalAotFallbackReason::ArrayOrIterable);
            }
            expr_fallback_reason(expr)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_fallback_reason(subject)
            .or_else(|| {
                arms.iter().find_map(|(conditions, result)| {
                    conditions
                        .iter()
                        .find_map(expr_fallback_reason)
                        .or_else(|| expr_fallback_reason(result))
                })
            })
            .or_else(|| default.as_deref().and_then(expr_fallback_reason)),
        ExprKind::FunctionCall { args, .. } => args
            .iter()
            .find_map(expr_fallback_reason)
            .or(Some(EvalAotFallbackReason::UnsupportedStaticCall)),
        ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. } => {
            Some(EvalAotFallbackReason::DynamicCall)
        }
        ExprKind::Pipe { value, callable } => expr_fallback_reason(value)
            .or_else(|| expr_fallback_reason(callable))
            .or(Some(EvalAotFallbackReason::DynamicCall)),
        ExprKind::NewDynamic { .. } | ExprKind::NewDynamicObject { .. } => {
            Some(EvalAotFallbackReason::DynamicClassOrMember)
        }
        ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } => {
            Some(EvalAotFallbackReason::DynamicClassOrMember)
        }
        ExprKind::NewObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ObjectClassName { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::This => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::Spread(_) => {
            Some(EvalAotFallbackReason::ArrayOrIterable)
        }
        ExprKind::Assignment { .. }
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::NamedArg { .. } => Some(EvalAotFallbackReason::UnsupportedScope),
        ExprKind::Closure { .. } => Some(EvalAotFallbackReason::Declaration),
        ExprKind::IncludeValue { .. } => Some(EvalAotFallbackReason::IncludeOrRequire),
        ExprKind::InstanceOf { value, target } => expr_fallback_reason(value)
            .or_else(|| match target {
                crate::parser::ast::InstanceOfTarget::Name(_) => None,
                crate::parser::ast::InstanceOfTarget::Expr(expr) => expr_fallback_reason(expr),
            })
            .or(Some(EvalAotFallbackReason::ObjectOrMemberAccess)),
        ExprKind::FirstClassCallable(target) => callable_target_fallback_reason(target),
        ExprKind::ConstRef(_) | ExprKind::MagicConstant(_) => {
            Some(EvalAotFallbackReason::UnsupportedConstruct)
        }
        ExprKind::PtrCast { expr, .. } => {
            expr_fallback_reason(expr).or(Some(EvalAotFallbackReason::UnsupportedConstruct))
        }
        ExprKind::BufferNew { len, .. } => {
            expr_fallback_reason(len).or(Some(EvalAotFallbackReason::UnsupportedConstruct))
        }
        ExprKind::Yield { .. } => Some(EvalAotFallbackReason::UnsupportedControlFlow),
    }
}

/// Classifies first-class callable expressions for fallback markers.
pub(super) fn callable_target_fallback_reason(target: &CallableTarget) -> Option<EvalAotFallbackReason> {
    match target {
        CallableTarget::Function(_) => Some(EvalAotFallbackReason::DynamicCall),
        CallableTarget::StaticMethod { .. } => Some(EvalAotFallbackReason::ObjectOrMemberAccess),
        CallableTarget::Method { object, .. } => {
            expr_fallback_reason(object).or(Some(EvalAotFallbackReason::ObjectOrMemberAccess))
        }
    }
}
