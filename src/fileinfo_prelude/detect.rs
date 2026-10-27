//! Purpose:
//! Detects the exact global fileinfo function and class surface before injection.
//!
//! Called from:
//! - fileinfo_prelude::inject_if_used before name resolution.
//!
//! Key details:
//! - Resolves a clone with catalogued fallback symbols to respect namespaces and imports.
//! - Exhaustive AST walking covers closures, enums, type hints and callable syntax.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The exact PHP functions of the fileinfo surface: every `PhpModule::Fileinfo` contract in the shared
/// catalog. Whole-segment matches ensure user helpers with a `finfo_` prefix do not silently
/// opt into the bridge.
fn fileinfo_functions() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| contract.module == elephc_builtin_contract::PhpModule::Fileinfo && !contract.internal)
            .map(|contract| contract.name)
            .collect()
    })
}

/// The `finfo` class, from the shared class catalog.
fn fileinfo_classes() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        crate::types::builtin_classes::class_names_in_module(elephc_builtin_contract::PhpModule::Fileinfo).to_vec()
    })
}

/// Returns whether any statement resolves to the global fileinfo surface, so the prelude must
/// be injected ahead of user code. Resolution errors are left for the pipeline's normal
/// name-resolution phase; an invalid program does not need speculative bridge injection.
pub(super) fn program_uses_fileinfo(program: &[Stmt]) -> bool {
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(),
        fileinfo_functions(),
        fileinfo_classes(),
    )
    .is_ok_and(|resolved| resolved.iter().any(stmt_refs_fileinfo))
}

/// Returns whether a resolved name is one of the fileinfo global functions. Function names are
/// case-insensitive, but a name with namespace segments is unrelated to the global surface.
fn name_is_fileinfo_function(name: &Name) -> bool {
    fileinfo_functions()
        .iter()
        .any(|candidate| crate::name_resolver::is_additional_global_symbol(name, candidate))
}

/// Returns whether a resolved name is `finfo`: the seeded global the
/// resolver bound it to, or the bare single-segment spelling at a class-name position the
/// resolver leaves as written (a variadic parameter's element type, a `::class` or scoped
/// constant receiver). Class names are case-insensitive, but a name with namespace segments
/// is unrelated to the global surface.
fn name_is_fileinfo_class(name: &Name) -> bool {
    fileinfo_classes().iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
            || matches!(name.parts.as_slice(), [bare] if bare.eq_ignore_ascii_case(candidate))
    })
}

/// Literal function targets in reflection constructors require the same declarations as calls.
fn reflection_refs_fileinfo(class_name: &Name, args: &[Expr]) -> bool {
    if !matches!(class_name.parts.as_slice(), [name]
        if name.eq_ignore_ascii_case("ReflectionFunction") || name.eq_ignore_ascii_case("ReflectionParameter")) {
        return false;
    }
    let target = args.iter().find_map(|arg| match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "function" => Some(value.as_ref()),
        _ => None,
    }).or_else(|| args.first().filter(|arg| !matches!(&arg.kind, ExprKind::NamedArg { .. })));
    matches!(target.map(|expr| &expr.kind), Some(ExprKind::StringLiteral(name))
        if fileinfo_functions().iter().any(|candidate| name.trim_start_matches('\\').eq_ignore_ascii_case(candidate)))
}

/// Returns whether a static receiver names a fileinfo class in a scoped expression.
/// `self`, `static`, and `parent` never resolve to a fileinfo class at this position.
fn receiver_refs_fileinfo(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_fileinfo_class(name))
}

/// Returns whether an `instanceof` target references a fileinfo class, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_fileinfo(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_fileinfo_class(name),
        InstanceOfTarget::Generic(class_type) => type_refs_fileinfo(class_type),
        InstanceOfTarget::Expr(expr) => expr_refs_fileinfo(expr),
    }
}

/// Returns whether a first-class-callable target references the fileinfo surface: a free
/// function (`fileinfo_exec(...)`), a static-method receiver, or an instance-method object
/// expression.
fn callable_target_refs_fileinfo(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_fileinfo_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_fileinfo(receiver),
        CallableTarget::Method { object, .. } => expr_refs_fileinfo(object),
    }
}

/// Returns whether a type expression names a fileinfo class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_fileinfo(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_fileinfo_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_fileinfo(inner)
        }
        TypeExpr::AssocArray { key, value } => type_refs_fileinfo(key) || type_refs_fileinfo(value),
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_fileinfo) || type_refs_fileinfo(ret)
        }
        TypeExpr::Named(name) => name_is_fileinfo_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_fileinfo_class(name) || args.iter().any(type_refs_fileinfo)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_fileinfo)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the fileinfo
/// surface. Shared by function, method, and closure parameter lists.
fn params_ref_fileinfo(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_fileinfo)
            || default.as_ref().is_some_and(expr_refs_fileinfo)
    })
}

/// Returns whether a `use Trait` clause names a fileinfo class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_fileinfo(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_fileinfo_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_fileinfo_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_fileinfo_class)
                    || instead_of.iter().any(name_is_fileinfo_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the fileinfo
/// surface.
fn class_property_refs_fileinfo(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_fileinfo)
        || property.default.as_ref().is_some_and(expr_refs_fileinfo)
}

/// Returns whether a method's parameters, variadic element type, return type, or body
/// reference the fileinfo surface.
fn class_method_refs_fileinfo(method: &ClassMethod) -> bool {
    params_ref_fileinfo(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_fileinfo)
        || method.return_type.as_ref().is_some_and(type_refs_fileinfo)
        || method.body.iter().any(stmt_refs_fileinfo)
}

/// Returns whether a class constant's initializer references the fileinfo surface.
fn class_const_refs_fileinfo(constant: &ClassConst) -> bool {
    expr_refs_fileinfo(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the fileinfo surface.
fn enum_case_refs_fileinfo(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_fileinfo)
}

/// Returns whether a `packed class` field's type references a fileinfo class. A fileinfo class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_fileinfo(field: &PackedField) -> bool {
    type_refs_fileinfo(&field.type_expr)
}

/// Returns whether an expression references the fileinfo surface at any function-call,
/// class-name, or constant position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_fileinfo(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::This
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::MagicConstant(_) => false,

        ExprKind::ConstRef(_) => false,

        ExprKind::BinaryOp { left, right, .. } => expr_refs_fileinfo(left) || expr_refs_fileinfo(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_fileinfo(value) || instanceof_target_refs_fileinfo(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_fileinfo(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_fileinfo(value) || expr_refs_fileinfo(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_fileinfo(value) || expr_refs_fileinfo(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_fileinfo(target)
                || expr_refs_fileinfo(value)
                || result_target.as_deref().is_some_and(expr_refs_fileinfo)
                || prelude.iter().any(stmt_refs_fileinfo)
        }
        ExprKind::FunctionCall { name, args } => {
            name_is_fileinfo_function(name) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_fileinfo),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_fileinfo),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_fileinfo(key) || expr_refs_fileinfo(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_refs_fileinfo),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_fileinfo(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_fileinfo) || expr_refs_fileinfo(body)
                })
                || default.as_deref().is_some_and(expr_refs_fileinfo)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_fileinfo(array) || expr_refs_fileinfo(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_fileinfo(condition) || expr_refs_fileinfo(then_expr) || expr_refs_fileinfo(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_fileinfo(expr),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_fileinfo(params)
                || variadic_type.as_ref().is_some_and(type_refs_fileinfo)
                || return_type.as_ref().is_some_and(type_refs_fileinfo)
                || body.iter().any(stmt_refs_fileinfo)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_fileinfo(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_fileinfo(callee) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_fileinfo_class(class_name)
                || reflection_refs_fileinfo(class_name, args)
                || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::NewGeneric { class_type, args } => {
            type_refs_fileinfo(class_type) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_fileinfo(name_expr) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_fileinfo(class_name)
                || name_is_fileinfo_class(fallback_class)
                || name_is_fileinfo_class(required_parent)
                || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_fileinfo(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_fileinfo(object) || expr_refs_fileinfo(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_fileinfo(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_fileinfo(object) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_fileinfo(object) || expr_refs_fileinfo(method) || args.iter().any(expr_refs_fileinfo),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_fileinfo(receiver) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_fileinfo(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_fileinfo(element_type) || expr_refs_fileinfo(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_fileinfo(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_fileinfo(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_fileinfo(receiver) || args.iter().any(expr_refs_fileinfo)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_fileinfo)
                || value.as_deref().is_some_and(expr_refs_fileinfo)
        }
        ExprKind::IncludeValue { path, .. } => expr_refs_fileinfo(path),
    }
}

/// Returns whether a statement references the fileinfo surface at any function-call,
/// class-name, or constant position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_fileinfo(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::RefAssign { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,

        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_fileinfo_class(&item.name) || name_is_fileinfo_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_fileinfo(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_fileinfo(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_fileinfo(condition)
                || then_body.iter().any(stmt_refs_fileinfo)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_fileinfo(cond) || body.iter().any(stmt_refs_fileinfo))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_fileinfo))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_fileinfo)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_fileinfo))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_fileinfo(condition) || body.iter().any(stmt_refs_fileinfo)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_fileinfo)
                || condition.as_ref().is_some_and(expr_refs_fileinfo)
                || update.as_deref().is_some_and(stmt_refs_fileinfo)
                || body.iter().any(stmt_refs_fileinfo)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_fileinfo(index) || expr_refs_fileinfo(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_fileinfo(target) || expr_refs_fileinfo(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_fileinfo(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_fileinfo(type_expr) || expr_refs_fileinfo(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_fileinfo(array) || body.iter().any(stmt_refs_fileinfo)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_fileinfo(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_fileinfo) || body.iter().any(stmt_refs_fileinfo)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_fileinfo))
        }
        StmtKind::Include { path, .. } => expr_refs_fileinfo(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_fileinfo),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_fileinfo)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_fileinfo_class)
                        || catch.body.iter().any(stmt_refs_fileinfo)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_fileinfo))
        }
        StmtKind::FunctionDecl {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_fileinfo(params)
                || variadic_type.as_ref().is_some_and(type_refs_fileinfo)
                || return_type.as_ref().is_some_and(type_refs_fileinfo)
                || body.iter().any(stmt_refs_fileinfo)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_fileinfo),
        StmtKind::ConstDecl { value, .. } => expr_refs_fileinfo(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_fileinfo(value),
        StmtKind::StaticVar { init, .. } => expr_refs_fileinfo(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_fileinfo_class)
                || implements.iter().any(name_is_fileinfo_class)
                || trait_uses.iter().any(trait_use_refs_fileinfo)
                || properties.iter().any(class_property_refs_fileinfo)
                || methods.iter().any(class_method_refs_fileinfo)
                || constants.iter().any(class_const_refs_fileinfo)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_fileinfo)
                || cases.iter().any(enum_case_refs_fileinfo)
                || implements.iter().any(name_is_fileinfo_class)
                || trait_uses.iter().any(trait_use_refs_fileinfo)
                || methods.iter().any(class_method_refs_fileinfo)
                || constants.iter().any(class_const_refs_fileinfo)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_fileinfo),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_fileinfo_class)
                || properties.iter().any(class_property_refs_fileinfo)
                || methods.iter().any(class_method_refs_fileinfo)
                || constants.iter().any(class_const_refs_fileinfo)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_fileinfo)
                || properties.iter().any(class_property_refs_fileinfo)
                || methods.iter().any(class_method_refs_fileinfo)
                || constants.iter().any(class_const_refs_fileinfo)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_fileinfo(object) || expr_refs_fileinfo(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_fileinfo(receiver) || expr_refs_fileinfo(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_fileinfo(receiver) || expr_refs_fileinfo(index) || expr_refs_fileinfo(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_fileinfo(object) || expr_refs_fileinfo(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_fileinfo(object) || expr_refs_fileinfo(index) || expr_refs_fileinfo(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Global functions, imports, callable references and nested declarations enable injection.
    #[test]
    fn fileinfo_detection_covers_php_name_positions() {
        for source in [
            "<?php finfo_buffer($f, 'x');",
            "<?php namespace App; finfo_open();",
            "<?php use finfo as Info; new Info();",
            "<?php use function finfo_open as open; open();",
            "<?php $f = \\FINFO_OPEN(...);",
            "<?php function f(finfo ...$infos): void {}",
            "<?php enum E { case A; public function inspect() { return new \\finfo(); } }",
            "<?php new ReflectionFunction('finfo_close');",
            r"<?php new \ReflectionFunction(function: '\FiNfO_Close');",
            "<?php namespace App; use ReflectionFunction as Ref; new Ref('finfo_buffer');",
            "<?php new ReflectionParameter(parameter: 'finfo', function: 'finfo_close');",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let ast = crate::parser::parse(&tokens).unwrap();
            assert!(program_uses_fileinfo(&ast), "{source}");
        }
    }

    /// User namespace declarations and constant-only programs never opt into the bridge.
    #[test]
    fn fileinfo_detection_respects_local_symbols() {
        for source in [
            "<?php namespace App; function finfo_open() { return 1; } finfo_open();",
            "<?php \\App\\finfo_open();",
            "<?php echo FILEINFO_MIME_TYPE;",
            r"<?php new ReflectionFunction('App\finfo_close');",
            "<?php namespace App; class ReflectionFunction { public function __construct(string $name) {} } new ReflectionFunction('finfo_close');",
            "<?php function custom($finfo_close) {} new ReflectionParameter(function: 'custom', parameter: 'finfo_close');",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let ast = crate::parser::parse(&tokens).unwrap();
            assert!(!program_uses_fileinfo(&ast), "{source}");
        }
    }
}
