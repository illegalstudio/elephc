//! Purpose:
//! The `--strict-php` AST audit pass: walks a parsed user program and reports a
//! `CompileError` for every elephc-only construct (`ifdef`, `packed class`,
//! `extern`, `ptr_cast<T>`, `buffer_new<T>`, typed local declarations,
//! `ptr`/`buffer<T>` type annotations, and compiler-reserved `__elephc_*` names).
//!
//! Called from:
//! - `crate::pipeline::compile()` — on the main file right after parsing, and on
//!   the resolved program (which includes `include`/`require`d user files) before
//!   any compiler prelude is injected, so injected compiler code is never audited.
//! - Integration tests through `crate::strict_php::check`.
//!
//! Key details:
//! - Every `match` on `StmtKind`, `ExprKind`, and `TypeExpr` is exhaustive with
//!   no wildcard arm: adding a new AST variant fails compilation here until the
//!   author decides whether the construct is PHP or an elephc extension.
//! - All violations are collected in one pass (not first-error-only) so a file
//!   can be fixed in a single round.
//! - `TypeExpr` carries no span, so type violations reuse the enclosing
//!   statement's or expression's span.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{
    AttributeGroup, CallableTarget, CatchClause, ClassConst, ClassMethod, ClassProperty,
    EnumCaseDecl, Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;

/// Audits a parsed user program and returns every strict-PHP violation found.
///
/// An empty result means the program uses only PHP-compatible constructs at the
/// syntax level (builtin availability is enforced separately by the catalog and
/// checker). The caller decides how to report the collected errors.
pub fn check(program: &Program) -> Vec<CompileError> {
    let mut errors = Vec::new();
    audit_stmts(program, &mut errors);
    errors
}

/// Pushes a violation for an elephc-only construct with the standard suffix.
fn reject(errors: &mut Vec<CompileError>, span: Span, lead: &str) {
    errors.push(CompileError::new(
        span,
        &format!("{lead}; rejected by --strict-php"),
    ));
}

/// Pushes a violation when `name`'s bare (last) segment uses the
/// compiler-reserved `__elephc_` prefix.
///
/// Applied to free-function declarations and call targets only: the compiler
/// reserves the prefix exclusively for internal builtins and synthesized
/// functions, which live in the function namespace. Class, method, and property
/// names are separate PHP symbol spaces the compiler never synthesizes
/// `__elephc_*` names into, so they are deliberately not checked.
fn reject_reserved_name(errors: &mut Vec<CompileError>, span: Span, name: &Name) {
    let bare = name.parts.last().map(String::as_str).unwrap_or_default();
    if bare.to_ascii_lowercase().starts_with("__elephc_") {
        reject(
            errors,
            span,
            &format!("'{bare}' is reserved for the compiler ('__elephc_' prefix)"),
        );
    }
}

/// Audits a statement list in order.
fn audit_stmts(stmts: &[Stmt], errors: &mut Vec<CompileError>) {
    for stmt in stmts {
        audit_stmt(stmt, errors);
    }
}

/// Audits PHP attribute groups: attribute arguments are ordinary expressions
/// (`#[Foo(buffer_new<int>(4))]` is a real call site once the attribute is
/// reflected), so they get the same expression audit as any other code.
fn audit_attribute_groups(groups: &[AttributeGroup], errors: &mut Vec<CompileError>) {
    for group in groups {
        for attribute in &group.attributes {
            audit_exprs(&attribute.args, errors);
        }
    }
}

/// Audits one statement: rejects extension statement forms and recurses into
/// every nested expression, type annotation, statement body, and the
/// statement's own attribute arguments.
fn audit_stmt(stmt: &Stmt, errors: &mut Vec<CompileError>) {
    let span = stmt.span;
    audit_attribute_groups(&stmt.attributes, errors);
    match &stmt.kind {
        StmtKind::Echo(expr) => audit_expr(expr, errors),
        StmtKind::Assign { name: _, value } => audit_expr(value, errors),
        StmtKind::RefAssign { target: _, source } => audit_expr(source, errors),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            audit_expr(condition, errors);
            audit_stmts(then_body, errors);
            for (clause_condition, clause_body) in elseif_clauses {
                audit_expr(clause_condition, errors);
                audit_stmts(clause_body, errors);
            }
            if let Some(body) = else_body {
                audit_stmts(body, errors);
            }
        }
        StmtKind::IfDef {
            symbol: _,
            then_body,
            else_body,
        } => {
            reject(
                errors,
                span,
                "`ifdef` conditional compilation is an elephc extension and is not valid PHP",
            );
            audit_stmts(then_body, errors);
            if let Some(body) = else_body {
                audit_stmts(body, errors);
            }
        }
        StmtKind::While { condition, body } => {
            audit_expr(condition, errors);
            audit_stmts(body, errors);
        }
        StmtKind::DoWhile { body, condition } => {
            audit_stmts(body, errors);
            audit_expr(condition, errors);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                audit_stmt(init, errors);
            }
            if let Some(condition) = condition {
                audit_expr(condition, errors);
            }
            if let Some(update) = update {
                audit_stmt(update, errors);
            }
            audit_stmts(body, errors);
        }
        StmtKind::ArrayAssign {
            array: _,
            index,
            value,
        } => {
            audit_expr(index, errors);
            audit_expr(value, errors);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            audit_expr(target, errors);
            audit_expr(value, errors);
        }
        StmtKind::ArrayPush { array: _, value } => audit_expr(value, errors),
        StmtKind::TypedAssign {
            type_expr,
            name: _,
            value,
        } => {
            reject(
                errors,
                span,
                "typed local variable declarations are an elephc extension and are not valid PHP",
            );
            audit_type(type_expr, span, errors);
            audit_expr(value, errors);
        }
        StmtKind::Foreach {
            array,
            key_var: _,
            value_var: _,
            value_by_ref: _,
            body,
        } => {
            audit_expr(array, errors);
            audit_stmts(body, errors);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            audit_expr(subject, errors);
            for (case_values, case_body) in cases {
                for value in case_values {
                    audit_expr(value, errors);
                }
                audit_stmts(case_body, errors);
            }
            if let Some(body) = default {
                audit_stmts(body, errors);
            }
        }
        StmtKind::Include {
            path,
            once: _,
            required: _,
        } => audit_expr(path, errors),
        StmtKind::IncludeOnceMark { label: _ } => {}
        StmtKind::IncludeOnceGuard { label: _, body } => audit_stmts(body, errors),
        StmtKind::Throw(expr) => audit_expr(expr, errors),
        StmtKind::Synthetic(body) => audit_stmts(body, errors),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            audit_stmts(try_body, errors);
            for CatchClause {
                exception_types: _,
                variable: _,
                body,
            } in catches
            {
                audit_stmts(body, errors);
            }
            if let Some(body) = finally_body {
                audit_stmts(body, errors);
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) => {}
        StmtKind::ExprStmt(expr) => audit_expr(expr, errors),
        StmtKind::NamespaceDecl { name: _ } => {}
        StmtKind::NamespaceBlock { name: _, body } => audit_stmts(body, errors),
        StmtKind::UseDecl { imports: _ } => {}
        StmtKind::FunctionDecl {
            name,
            params,
            param_attributes,
            variadic: _,
            variadic_by_ref: _,
            variadic_type,
            return_type,
            by_ref_return: _,
            body,
        } => {
            reject_reserved_name(errors, span, &Name::unqualified(name));
            for groups in param_attributes {
                audit_attribute_groups(groups, errors);
            }
            audit_params(params, span, errors);
            if let Some(variadic_type) = variadic_type {
                audit_type(variadic_type, span, errors);
            }
            if let Some(return_type) = return_type {
                audit_type(return_type, span, errors);
            }
            audit_stmts(body, errors);
        }
        StmtKind::FunctionVariantGroup {
            name: _,
            variants: _,
        } => {}
        StmtKind::FunctionVariantMark {
            name: _,
            variant: _,
        } => {}
        StmtKind::Return(value) => {
            if let Some(value) = value {
                audit_expr(value, errors);
            }
        }
        StmtKind::ConstDecl { name: _, value } => audit_expr(value, errors),
        StmtKind::ListUnpack { vars: _, value } => audit_expr(value, errors),
        StmtKind::Global { vars: _ } => {}
        StmtKind::StaticVar { name: _, init } => audit_expr(init, errors),
        StmtKind::ClassDecl {
            name: _,
            extends: _,
            implements: _,
            is_abstract: _,
            is_final: _,
            is_readonly_class: _,
            trait_uses: _,
            properties,
            methods,
            constants,
        } => {
            audit_class_members(properties, methods, constants, span, errors);
        }
        StmtKind::EnumDecl {
            name: _,
            backing_type,
            cases,
            implements: _,
            trait_uses: _,
            methods,
            constants,
        } => {
            if let Some(backing_type) = backing_type {
                audit_type(backing_type, span, errors);
            }
            for EnumCaseDecl {
                name: _,
                value,
                span: case_span,
                attributes,
            } in cases
            {
                let _ = case_span;
                audit_attribute_groups(attributes, errors);
                if let Some(value) = value {
                    audit_expr(value, errors);
                }
            }
            audit_class_members(&[], methods, constants, span, errors);
        }
        StmtKind::PackedClassDecl { name: _, fields: _ } => {
            reject(
                errors,
                span,
                "`packed class` is an elephc extension and is not valid PHP",
            );
        }
        StmtKind::InterfaceDecl {
            name: _,
            extends: _,
            properties,
            methods,
            constants,
        } => {
            audit_class_members(properties, methods, constants, span, errors);
        }
        StmtKind::TraitDecl {
            name: _,
            trait_uses: _,
            properties,
            methods,
            constants,
        } => {
            audit_class_members(properties, methods, constants, span, errors);
        }
        StmtKind::PropertyAssign {
            object,
            property: _,
            value,
        } => {
            audit_expr(object, errors);
            audit_expr(value, errors);
        }
        StmtKind::StaticPropertyAssign {
            receiver: _,
            property: _,
            value,
        } => audit_expr(value, errors),
        StmtKind::StaticPropertyArrayPush {
            receiver: _,
            property: _,
            value,
        } => audit_expr(value, errors),
        StmtKind::StaticPropertyArrayAssign {
            receiver: _,
            property: _,
            index,
            value,
        } => {
            audit_expr(index, errors);
            audit_expr(value, errors);
        }
        StmtKind::PropertyArrayPush {
            object,
            property: _,
            value,
        } => {
            audit_expr(object, errors);
            audit_expr(value, errors);
        }
        StmtKind::PropertyArrayAssign {
            object,
            property: _,
            index,
            value,
        } => {
            audit_expr(object, errors);
            audit_expr(index, errors);
            audit_expr(value, errors);
        }
        StmtKind::ExternFunctionDecl {
            name: _,
            params: _,
            return_type: _,
            library: _,
        }
        | StmtKind::ExternClassDecl { name: _, fields: _ }
        | StmtKind::ExternGlobalDecl { name: _, c_type: _ } => {
            reject(
                errors,
                span,
                "`extern` declarations are an elephc extension and are not valid PHP",
            );
        }
    }
}

/// Audits the shared class-like member lists (properties, methods, constants).
fn audit_class_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    constants: &[ClassConst],
    span: Span,
    errors: &mut Vec<CompileError>,
) {
    for property in properties {
        audit_attribute_groups(&property.attributes, errors);
        if let Some(type_expr) = &property.type_expr {
            audit_type(type_expr, property.span, errors);
        }
        if let Some(default) = &property.default {
            audit_expr(default, errors);
        }
    }
    for method in methods {
        audit_attribute_groups(&method.attributes, errors);
        for groups in &method.param_attributes {
            audit_attribute_groups(groups, errors);
        }
        audit_params(&method.params, method.span, errors);
        if let Some(variadic_type) = &method.variadic_type {
            audit_type(variadic_type, method.span, errors);
        }
        if let Some(return_type) = &method.return_type {
            audit_type(return_type, method.span, errors);
        }
        audit_stmts(&method.body, errors);
    }
    for constant in constants {
        audit_attribute_groups(&constant.attributes, errors);
        if let Some(type_expr) = &constant.type_expr {
            audit_type(type_expr, constant.span, errors);
        }
        audit_expr(&constant.value, errors);
        let _ = span;
    }
}

/// Audits a function/method/closure parameter list: type annotations and defaults.
fn audit_params(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    span: Span,
    errors: &mut Vec<CompileError>,
) {
    for (_, type_ann, default, _) in params {
        if let Some(type_expr) = type_ann {
            audit_type(type_expr, span, errors);
        }
        if let Some(default) = default {
            audit_expr(default, errors);
        }
    }
}

/// Audits an argument list.
fn audit_exprs(exprs: &[Expr], errors: &mut Vec<CompileError>) {
    for expr in exprs {
        audit_expr(expr, errors);
    }
}

/// Audits one expression: rejects extension expression forms and reserved
/// `__elephc_*` call targets, recursing into every operand.
fn audit_expr(expr: &Expr, errors: &mut Vec<CompileError>) {
    let span = expr.span;
    match &expr.kind {
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
        | ExprKind::This
        | ExprKind::ClassConstant { receiver: _ }
        | ExprKind::ScopedConstantAccess {
            receiver: _,
            name: _,
        }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::BinaryOp { left, op: _, right } => {
            audit_expr(left, errors);
            audit_expr(right, errors);
        }
        ExprKind::InstanceOf { value, target } => {
            audit_expr(value, errors);
            match target {
                InstanceOfTarget::Name(_) => {}
                InstanceOfTarget::Expr(target_expr) => audit_expr(target_expr, errors),
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
        | ExprKind::YieldFrom(inner) => audit_expr(inner, errors),
        ExprKind::NullCoalesce { value, default } => {
            audit_expr(value, errors);
            audit_expr(default, errors);
        }
        ExprKind::Pipe { value, callable } => {
            audit_expr(value, errors);
            audit_expr(callable, errors);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp: _,
        } => {
            audit_expr(target, errors);
            audit_expr(value, errors);
            if let Some(result_target) = result_target {
                audit_expr(result_target, errors);
            }
            audit_stmts(prelude, errors);
        }
        ExprKind::FunctionCall { name, args } => {
            reject_reserved_name(errors, span, name);
            audit_exprs(args, errors);
        }
        ExprKind::ArrayLiteral(items) => audit_exprs(items, errors),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                audit_expr(key, errors);
                audit_expr(value, errors);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            audit_expr(subject, errors);
            for (conditions, arm_value) in arms {
                audit_exprs(conditions, errors);
                audit_expr(arm_value, errors);
            }
            if let Some(default) = default {
                audit_expr(default, errors);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            audit_expr(array, errors);
            audit_expr(index, errors);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            audit_expr(condition, errors);
            audit_expr(then_expr, errors);
            audit_expr(else_expr, errors);
        }
        ExprKind::ShortTernary { value, default } => {
            audit_expr(value, errors);
            audit_expr(default, errors);
        }
        ExprKind::Cast { target: _, expr } => audit_expr(expr, errors),
        ExprKind::Closure {
            params,
            variadic: _,
            variadic_by_ref: _,
            variadic_type,
            return_type,
            body,
            is_arrow: _,
            is_static: _,
            by_ref_return: _,
            captures: _,
            capture_refs: _,
        } => {
            audit_params(params, span, errors);
            if let Some(variadic_type) = variadic_type {
                audit_type(variadic_type, span, errors);
            }
            if let Some(return_type) = return_type {
                audit_type(return_type, span, errors);
            }
            audit_stmts(body, errors);
        }
        ExprKind::NamedArg { name: _, value } => audit_expr(value, errors),
        ExprKind::IncludeValue {
            path,
            once: _,
            required: _,
        } => audit_expr(path, errors),
        ExprKind::ClosureCall { var: _, args } => audit_exprs(args, errors),
        ExprKind::ExprCall { callee, args } => {
            audit_expr(callee, errors);
            audit_exprs(args, errors);
        }
        ExprKind::NewObject {
            class_name: _,
            args,
        } => audit_exprs(args, errors),
        ExprKind::NewDynamic { name_expr, args } => {
            audit_expr(name_expr, errors);
            audit_exprs(args, errors);
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class: _,
            required_parent: _,
            args,
        } => {
            audit_expr(class_name, errors);
            audit_exprs(args, errors);
        }
        ExprKind::PropertyAccess {
            object,
            property: _,
        }
        | ExprKind::NullsafePropertyAccess {
            object,
            property: _,
        } => audit_expr(object, errors),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            audit_expr(object, errors);
            audit_expr(property, errors);
        }
        ExprKind::StaticPropertyAccess {
            receiver: _,
            property: _,
        } => {}
        ExprKind::MethodCall {
            object,
            method: _,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method: _,
            args,
        } => {
            audit_expr(object, errors);
            audit_exprs(args, errors);
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            audit_expr(object, errors);
            audit_expr(method, errors);
            audit_exprs(args, errors);
        }
        ExprKind::StaticMethodCall {
            receiver: _,
            method: _,
            args,
        } => audit_exprs(args, errors),
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Function(name) => reject_reserved_name(errors, span, name),
            CallableTarget::StaticMethod {
                receiver: _,
                method: _,
            } => {}
            CallableTarget::Method { object, method: _ } => audit_expr(object, errors),
        },
        ExprKind::PtrCast {
            target_type: _,
            expr,
        } => {
            reject(
                errors,
                span,
                "`ptr_cast<T>` is an elephc extension and is not valid PHP",
            );
            audit_expr(expr, errors);
        }
        ExprKind::BufferNew { element_type, len } => {
            reject(
                errors,
                span,
                "`buffer_new<T>` is an elephc extension and is not valid PHP",
            );
            audit_type(element_type, span, errors);
            audit_expr(len, errors);
        }
        ExprKind::NewScopedObject { receiver: _, args } => audit_exprs(args, errors),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                audit_expr(key, errors);
            }
            if let Some(value) = value {
                audit_expr(value, errors);
            }
        }
    }
}

/// Audits a type annotation: rejects `ptr`/`buffer<T>` and recurses through
/// nullable, union, intersection, and array element types.
fn audit_type(type_expr: &TypeExpr, span: Span, errors: &mut Vec<CompileError>) {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable
        | TypeExpr::Named(_) => {}
        TypeExpr::Array(inner) => audit_type(inner, span, errors),
        TypeExpr::Ptr(_) => {
            reject(
                errors,
                span,
                "`ptr` types are an elephc extension and are not valid PHP",
            );
        }
        TypeExpr::Buffer(inner) => {
            reject(
                errors,
                span,
                "`buffer<T>` types are an elephc extension and are not valid PHP",
            );
            audit_type(inner, span, errors);
        }
        TypeExpr::Nullable(inner) => audit_type(inner, span, errors),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            for member in members {
                audit_type(member, span, errors);
            }
        }
    }
}
