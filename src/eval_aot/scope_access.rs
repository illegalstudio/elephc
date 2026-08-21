//! Purpose:
//! Collects complete eval-scope read and write effects.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Assignment targets and callable references preserve conservative access tracking.

use super::*;

/// Adds one statement's eval-scope reads and writes to the accumulator.
pub(super) fn collect_stmt_scope_access(stmt: &Stmt, access: &mut EvalScopeAccess) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr)) => collect_expr_scope_access(expr, access),
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr_scope_access(value, access);
            access.write(name);
        }
        StmtKind::RefAssign { target, source } => {
            access.write(target);
            collect_expr_scope_access(source, access);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr_scope_access(condition, access);
            collect_block_scope_access(then_body, access);
            for (condition, body) in elseif_clauses {
                collect_expr_scope_access(condition, access);
                collect_block_scope_access(body, access);
            }
            if let Some(else_body) = else_body {
                collect_block_scope_access(else_body, access);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_block_scope_access(then_body, access);
            if let Some(else_body) = else_body {
                collect_block_scope_access(else_body, access);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr_scope_access(condition, access);
            collect_block_scope_access(body, access);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_scope_access(init, access);
            }
            if let Some(condition) = condition {
                collect_expr_scope_access(condition, access);
            }
            if let Some(update) = update {
                collect_stmt_scope_access(update, access);
            }
            collect_block_scope_access(body, access);
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            access.read(array);
            access.write(array);
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_assignment_target_scope_access(target, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::ArrayPush { array, value } => {
            access.read(array);
            access.write(array);
            collect_expr_scope_access(value, access);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            collect_expr_scope_access(array, access);
            if expr_is_static_empty_array_literal_source(array) {
                return;
            }
            if let Some(key_var) = key_var {
                access.write(key_var);
            }
            access.write(value_var);
            collect_block_scope_access(body, access);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr_scope_access(subject, access);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr_scope_access(condition, access);
                }
                collect_block_scope_access(body, access);
            }
            if let Some(default) = default {
                collect_block_scope_access(default, access);
            }
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            collect_block_scope_access(body, access);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_block_scope_access(try_body, access);
            for catch in catches {
                if let Some(variable) = &catch.variable {
                    access.write(variable);
                }
                collect_block_scope_access(&catch.body, access);
            }
            if let Some(finally_body) = finally_body {
                collect_block_scope_access(finally_body, access);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => collect_block_scope_access(body, access),
        StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
        StmtKind::ListUnpack { vars, value } => {
            collect_expr_scope_access(value, access);
            for var in vars {
                access.write(var);
            }
        }
        StmtKind::Global { vars } => {
            for var in vars {
                access.write(var);
            }
            access.creates_unknown_vars = true;
        }
        StmtKind::StaticVar { name, init } => {
            collect_expr_scope_access(init, access);
            access.write(name);
            access.creates_unknown_vars = true;
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            collect_expr_scope_access(value, access);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            collect_expr_scope_access(index, access);
            collect_expr_scope_access(value, access);
        }
        StmtKind::Include { path, .. } => collect_expr_scope_access(path, access),
    }
}

/// Adds every statement in a block to the scope access accumulator.
pub(super) fn collect_block_scope_access(body: &[Stmt], access: &mut EvalScopeAccess) {
    for stmt in body {
        collect_stmt_scope_access(stmt, access);
    }
}

/// Adds one expression's eval-scope reads and writes to the accumulator.
pub(super) fn collect_expr_scope_access(expr: &Expr, access: &mut EvalScopeAccess) {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_)
        | ExprKind::ArrayAppend => {}
        ExprKind::ObjectClassName { object } => collect_expr_scope_access(object, access),
        ExprKind::Variable(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            access.read(name);
            if matches!(
                &expr.kind,
                ExprKind::PreIncrement(_)
                    | ExprKind::PostIncrement(_)
                    | ExprKind::PreDecrement(_)
                    | ExprKind::PostDecrement(_)
            ) {
                access.write(name);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr_scope_access(left, access);
            collect_expr_scope_access(right, access);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_expr_scope_access(value, access);
            if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                collect_expr_scope_access(target, access);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::YieldFrom(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => collect_expr_scope_access(inner, access),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => {
            collect_expr_scope_access(value, access);
            collect_expr_scope_access(default, access);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            for stmt in prelude {
                collect_stmt_scope_access(stmt, access);
            }
            collect_assignment_target_scope_access(target, access);
            collect_expr_scope_access(value, access);
            if let Some(result_target) = result_target {
                collect_assignment_target_scope_access(result_target, access);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            if let ExprKind::ExprCall { callee, .. } = &expr.kind {
                collect_expr_scope_access(callee, access);
            }
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_expr_scope_access(item, access);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_expr_scope_access(key, access);
                collect_expr_scope_access(value, access);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_expr_scope_access(subject, access);
            for (conditions, value) in arms {
                for condition in conditions {
                    collect_expr_scope_access(condition, access);
                }
                collect_expr_scope_access(value, access);
            }
            if let Some(default) = default {
                collect_expr_scope_access(default, access);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_scope_access(array, access);
            collect_expr_scope_access(index, access);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_scope_access(condition, access);
            collect_expr_scope_access(then_expr, access);
            collect_expr_scope_access(else_expr, access);
        }
        ExprKind::Closure {
            params,
            body,
            captures,
            capture_refs,
            ..
        } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_expr_scope_access(default, access);
                }
            }
            for capture in captures.iter().chain(capture_refs.iter()) {
                access.read(capture);
            }
            collect_block_scope_access(body, access);
        }
        ExprKind::NamedArg { value, .. } => collect_expr_scope_access(value, access),
        ExprKind::IncludeValue { path, .. } => collect_expr_scope_access(path, access),
        ExprKind::NewDynamic { name_expr, args } => {
            collect_expr_scope_access(name_expr, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            collect_expr_scope_access(class_name, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_scope_access(object, access);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(property, access);
        }
        ExprKind::NullsafeMethodCall { object, args, .. }
        | ExprKind::MethodCall { object, args, .. } => {
            collect_expr_scope_access(object, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(method, access);
            for arg in args {
                collect_expr_scope_access(arg, access);
            }
        }
        ExprKind::StaticPropertyAccess { .. } | ExprKind::This => {}
        ExprKind::BufferNew { len, .. } => collect_expr_scope_access(len, access),
        ExprKind::FirstClassCallable(target) => {
            collect_callable_target_scope_access(target, access)
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_expr_scope_access(key, access);
            }
            if let Some(value) = value {
                collect_expr_scope_access(value, access);
            }
        }
    }
}

/// Records the variable effects of an assignment target expression.
pub(super) fn collect_assignment_target_scope_access(expr: &Expr, access: &mut EvalScopeAccess) {
    match &expr.kind {
        ExprKind::Variable(name) => access.write(name),
        ExprKind::ArrayAccess { array, index } => {
            collect_expr_scope_access(array, access);
            collect_expr_scope_access(index, access);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_expr_scope_access(object, access);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr_scope_access(object, access);
            collect_expr_scope_access(property, access);
        }
        _ => {
            collect_expr_scope_access(expr, access);
            access.unknown_write();
        }
    }
}

/// Adds variable reads from a first-class callable target.
pub(super) fn collect_callable_target_scope_access(
    target: &crate::parser::ast::CallableTarget,
    access: &mut EvalScopeAccess,
) {
    match target {
        crate::parser::ast::CallableTarget::Function(_) => {}
        crate::parser::ast::CallableTarget::StaticMethod { .. } => {}
        crate::parser::ast::CallableTarget::Method { object, .. } => {
            collect_expr_scope_access(object, access);
        }
    }
}
