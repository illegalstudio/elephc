//! Purpose:
//! Collects function references and dynamic-call hazards from user/web-prelude AST.
//! Drives pay-for-use injection and declaration reachability for `--web` helpers.
//!
//! Called from:
//! - `crate::web_prelude::inject_if_web()` before the web prelude is combined with user code.
//!
//! Key details:
//! - Literal `function_exists`, `is_callable`, and callback names count as references.
//! - Unknown dynamic calls conservatively disable function pruning.

use std::collections::HashSet;

use crate::names::php_symbol_key;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget,
    Program, Stmt, StmtKind,
};

/// Function-reference summary for one AST subtree.
#[derive(Clone, Debug, Default)]
pub(super) struct Usage {
    pub(super) functions: HashSet<String>,
    pub(super) dynamic_function_call: bool,
}

impl Usage {
    /// Merges another subtree summary into this one.
    pub(super) fn merge(&mut self, other: Self) {
        self.functions.extend(other.functions);
        self.dynamic_function_call |= other.dynamic_function_call;
    }

    /// Returns true when a PHP function is referenced case-insensitively.
    pub(super) fn references(&self, name: &str) -> bool {
        self.functions.contains(&php_symbol_key(name))
    }
}

/// Collects direct and literal-indirect function references from a program.
pub(super) fn collect(program: &Program) -> Usage {
    let mut usage = Usage::default();
    for stmt in program {
        scan_stmt(stmt, &mut usage);
    }
    usage
}

/// Collects references from one statement and all nested AST children.
pub(super) fn collect_stmt(stmt: &Stmt) -> Usage {
    let mut usage = Usage::default();
    scan_stmt(stmt, &mut usage);
    usage
}

/// Records one normalized PHP function name.
fn record_name(usage: &mut Usage, name: &str) {
    usage
        .functions
        .insert(php_symbol_key(name.trim_start_matches('\\')));
}

/// Records a literal callback/probe target when it denotes a free function.
fn record_literal_function(usage: &mut Usage, expr: Option<&Expr>) {
    let Some(Expr {
        kind: ExprKind::StringLiteral(name),
        ..
    }) = expr
    else {
        return;
    };
    if !name.contains("::") && !name.is_empty() {
        record_name(usage, name);
    }
}

/// Scans parameter default expressions.
fn scan_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    usage: &mut Usage,
) {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            scan_expr(default, usage);
        }
    }
}

/// Scans one class property initializer.
fn scan_property(property: &ClassProperty, usage: &mut Usage) {
    if let Some(default) = &property.default {
        scan_expr(default, usage);
    }
}

/// Scans one class-like method body and parameter defaults.
fn scan_method(method: &ClassMethod, usage: &mut Usage) {
    scan_params(&method.params, usage);
    scan_program(&method.body, usage);
}

/// Scans one class constant initializer.
fn scan_class_const(constant: &ClassConst, usage: &mut Usage) {
    scan_expr(&constant.value, usage);
}

/// Scans a statement list into an existing summary.
fn scan_program(program: &[Stmt], usage: &mut Usage) {
    for stmt in program {
        scan_stmt(stmt, usage);
    }
}

/// Scans every expression-bearing position of one statement.
fn scan_stmt(stmt: &Stmt, usage: &mut Usage) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => scan_expr(expr, usage),
        StmtKind::RefAssign { source, .. } => scan_expr(source, usage),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            scan_expr(object, usage);
            scan_expr(value, usage);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            scan_expr(object, usage);
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            scan_expr(target, usage);
            scan_expr(value, usage);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            scan_expr(condition, usage);
            scan_program(then_body, usage);
            for (condition, body) in elseif_clauses {
                scan_expr(condition, usage);
                scan_program(body, usage);
            }
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            scan_program(then_body, usage);
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            scan_expr(condition, usage);
            scan_program(body, usage);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                scan_stmt(init, usage);
            }
            if let Some(condition) = condition {
                scan_expr(condition, usage);
            }
            if let Some(update) = update {
                scan_stmt(update, usage);
            }
            scan_program(body, usage);
        }
        StmtKind::Foreach { array, body, .. } => {
            scan_expr(array, usage);
            scan_program(body, usage);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, body) in cases {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_program(body, usage);
            }
            if let Some(body) = default {
                scan_program(body, usage);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => scan_program(body, usage),
        StmtKind::FunctionDecl { params, body, .. } => {
            scan_params(params, usage);
            scan_program(body, usage);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            scan_program(try_body, usage);
            for catch in catches {
                scan_program(&catch.body, usage);
            }
            if let Some(body) = finally_body {
                scan_program(body, usage);
            }
        }
        StmtKind::ClassDecl {
            properties,
            methods,
            constants,
            ..
        }
        | StmtKind::TraitDecl {
            properties,
            methods,
            constants,
            ..
        }
        | StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                scan_method(method, usage);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::EnumDecl { cases, methods, constants, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    scan_expr(value, usage);
                }
            }
            for method in methods {
                scan_method(method, usage);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Scans every child and callable position of one expression.
fn scan_expr(expr: &Expr, usage: &mut Usage) {
    match &expr.kind {
        ExprKind::IncludeValue { path, .. } => scan_expr(path, usage),
        ExprKind::FunctionCall { name, args } => {
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            record_name(usage, &name);
            match name.as_str() {
                "function_exists" | "is_callable" => {
                    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_))) {
                        record_literal_function(usage, args.first());
                    } else {
                        usage.dynamic_function_call = true;
                    }
                }
                "call_user_func" | "call_user_func_array" => {
                    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_))) {
                        record_literal_function(usage, args.first());
                    } else {
                        usage.dynamic_function_call = true;
                    }
                }
                "eval" => usage.dynamic_function_call = true,
                _ => {}
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            record_name(usage, name.as_str());
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            scan_expr(object, usage);
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { .. }) => {}
        ExprKind::ExprCall { callee, args } => {
            usage.dynamic_function_call = true;
            scan_expr(callee, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::ClosureCall { args, .. } => {
            usage.dynamic_function_call = true;
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            scan_expr(left, usage);
            scan_expr(right, usage);
        }
        ExprKind::InstanceOf { value, target } => {
            scan_expr(value, usage);
            if let InstanceOfTarget::Expr(target) = target {
                scan_expr(target, usage);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::YieldFrom(inner) => scan_expr(inner, usage),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => {
            scan_expr(value, usage);
            scan_expr(default, usage);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            scan_expr(target, usage);
            scan_expr(value, usage);
            if let Some(result_target) = result_target {
                scan_expr(result_target, usage);
            }
            scan_program(prelude, usage);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                scan_expr(item, usage);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                scan_expr(key, usage);
                scan_expr(value, usage);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, value) in arms {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_expr(value, usage);
            }
            if let Some(default) = default {
                scan_expr(default, usage);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            scan_expr(array, usage);
            scan_expr(index, usage);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            scan_expr(condition, usage);
            scan_expr(then_expr, usage);
            scan_expr(else_expr, usage);
        }
        ExprKind::Closure { params, body, .. } => {
            scan_params(params, usage);
            scan_program(body, usage);
        }
        ExprKind::NamedArg { value, .. } => scan_expr(value, usage),
        ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            scan_expr(name_expr, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            scan_expr(class_name, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => scan_expr(object, usage),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            scan_expr(object, usage);
            scan_expr(property, usage);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            scan_expr(object, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            scan_expr(object, usage);
            scan_expr(method, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BufferNew { len, .. } => scan_expr(len, usage),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                scan_expr(key, usage);
            }
            if let Some(value) = value {
                scan_expr(value, usage);
            }
        }
        ExprKind::Variable(_) => {}
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}
