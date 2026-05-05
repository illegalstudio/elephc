use crate::parser::ast::{
    CallableTarget, ClassMethod, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
};

/// Check if any statement or closure expression recursively contains an Include.
pub(super) fn has_includes(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_includes)
}

fn stmt_has_includes(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Include { .. } => true,
        StmtKind::Synthetic(stmts) | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            has_includes(stmts)
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => expr_has_includes(expr),
        StmtKind::Return(expr) => expr.as_ref().is_some_and(expr_has_includes),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_has_includes(index) || expr_has_includes(value)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_has_includes(object) || expr_has_includes(value)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_has_includes(condition)
                || has_includes(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_includes(condition) || has_includes(body)
                })
                || else_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_includes(condition) || has_includes(body)
        }
        StmtKind::NamespaceBlock { body, .. } => has_includes(body),
        StmtKind::FunctionDecl { params, body, .. } => {
            params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(expr_has_includes)
            }) || has_includes(body)
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            has_includes(try_body)
                || catches.iter().any(|catch_clause| has_includes(&catch_clause.body))
                || finally_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::ClassDecl { properties, methods, .. }
        | StmtKind::TraitDecl { properties, methods, .. } => {
            properties
                .iter()
                .any(|property| property.default.as_ref().is_some_and(expr_has_includes))
                || methods_have_includes(methods)
        }
        StmtKind::InterfaceDecl { methods, .. } => methods_have_includes(methods),
        StmtKind::Switch { subject, cases, default } => {
            expr_has_includes(subject)
                || cases.iter().any(|(values, body)| {
                    values.iter().any(expr_has_includes) || has_includes(body)
                })
                || default.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::Foreach { array, body, .. } => expr_has_includes(array) || has_includes(body),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|stmt| stmt_has_includes(stmt))
                || condition.as_ref().is_some_and(expr_has_includes)
                || update.as_ref().is_some_and(|stmt| stmt_has_includes(stmt))
                || has_includes(body)
        }
        StmtKind::EnumDecl { cases, .. } => cases
            .iter()
            .any(|case| case.value.as_ref().is_some_and(expr_has_includes)),
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn methods_have_includes(methods: &[ClassMethod]) -> bool {
    methods.iter().any(|method| {
        method.params.iter().any(|(_, _, default, _)| {
            default.as_ref().is_some_and(expr_has_includes)
        }) || has_includes(&method.body)
    })
}

fn expr_has_includes(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_includes(left) || expr_has_includes(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_has_includes(value) || instanceof_target_has_includes(target)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => expr_has_includes(value),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ArrayAccess { array: value, index: default }
        | ExprKind::ShortTernary { value, default } => {
            expr_has_includes(value) || expr_has_includes(default)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_has_includes(target)
                || expr_has_includes(value)
                || result_target.as_ref().is_some_and(|expr| expr_has_includes(expr))
                || has_includes(prelude)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_has_includes),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_includes),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .any(|(key, value)| expr_has_includes(key) || expr_has_includes(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_includes(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_has_includes) || expr_has_includes(value)
                })
                || default.as_ref().is_some_and(|expr| expr_has_includes(expr))
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_includes(condition)
                || expr_has_includes(then_expr)
                || expr_has_includes(else_expr)
        }
        ExprKind::Cast { expr, .. } => expr_has_includes(expr),
        ExprKind::Closure { params, body, .. } => {
            params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(expr_has_includes)
            }) || has_includes(body)
        }
        ExprKind::NamedArg { value, .. } => expr_has_includes(value),
        ExprKind::ExprCall { callee, args } => {
            expr_has_includes(callee) || args.iter().any(expr_has_includes)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_has_includes(object),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_includes(object) || args.iter().any(expr_has_includes)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            expr_has_includes(object)
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
        | ExprKind::MagicConstant(_) => false,
    }
}

fn instanceof_target_has_includes(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_has_includes(expr),
    }
}
