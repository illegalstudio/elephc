//! Purpose:
//! Detects the exact global zip function and class surface before injection.
//!
//! Called from:
//! - zip_prelude::inject_if_used before name resolution.
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

/// The exact PHP functions of the zip surface: every `PhpModule::Zip` contract in the shared
/// catalog. Whole-segment matches ensure user helpers with a `zip_` prefix do not silently
/// opt into the bridge.
fn zip_functions() -> &'static [&'static str] {
    &[
        "zip_open", "zip_close", "zip_read", "zip_entry_open", "zip_entry_close",
        "zip_entry_read", "zip_entry_name", "zip_entry_compressedsize",
        "zip_entry_filesize", "zip_entry_compressionmethod",
    ]
}

/// The `ZipArchive` class, from the shared class catalog.
fn zip_classes() -> &'static [&'static str] {
    &["ZipArchive"]
}

/// Returns whether any statement resolves to the global zip surface, so the prelude must
/// be injected ahead of user code. Resolution errors are left for the pipeline's normal
/// name-resolution phase; an invalid program does not need speculative bridge injection.
pub(super) fn program_uses_zip(program: &[Stmt]) -> bool {
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(),
        zip_functions(),
        zip_classes(),
    )
    .is_ok_and(|resolved| resolved.iter().any(stmt_refs_zip))
}

/// Returns whether a resolved name is one of the zip global functions. Function names are
/// case-insensitive, but a name with namespace segments is unrelated to the global surface.
fn name_is_zip_function(name: &Name) -> bool {
    zip_functions()
        .iter()
        .any(|candidate| crate::name_resolver::is_additional_global_symbol(name, candidate))
}

/// Returns whether a resolved name is `ZipArchive`: the seeded global the
/// resolver bound it to, or the bare single-segment spelling at a class-name position the
/// resolver leaves as written (a variadic parameter's element type, a `::class` or scoped
/// constant receiver). Class names are case-insensitive, but a name with namespace segments
/// is unrelated to the global surface.
fn name_is_zip_class(name: &Name) -> bool {
    zip_classes().iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
            || matches!(name.parts.as_slice(), [bare] if bare.eq_ignore_ascii_case(candidate))
    })
}

/// Literal function targets in reflection constructors require the same declarations as calls.
fn reflection_refs_zip(class_name: &Name, args: &[Expr]) -> bool {
    if !matches!(class_name.parts.as_slice(), [name]
        if name.eq_ignore_ascii_case("ReflectionFunction") || name.eq_ignore_ascii_case("ReflectionParameter")) {
        return false;
    }
    let target = args.iter().find_map(|arg| match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "function" => Some(value.as_ref()),
        _ => None,
    }).or_else(|| args.first().filter(|arg| !matches!(&arg.kind, ExprKind::NamedArg { .. })));
    matches!(target.map(|expr| &expr.kind), Some(ExprKind::StringLiteral(name))
        if zip_functions().iter().any(|candidate| name.trim_start_matches('\\').eq_ignore_ascii_case(candidate)))
}

/// Returns whether a literal name probe (`class_exists`, `extension_loaded`, …) names zip.
///
/// PHP programs commonly gate optional extensions with string probes rather than a direct
/// reference, so the prelude must be present for those probes to answer true.
fn name_lookup_refs_zip(name: &Name, args: &[Expr]) -> bool {
    let Some(probe) = name.parts.last() else { return false };
    if !matches!(
        probe.to_ascii_lowercase().as_str(),
        "class_exists"
            | "interface_exists"
            | "trait_exists"
            | "function_exists"
            | "extension_loaded"
    ) {
        return false;
    }
    args.iter().any(|arg| {
        let ExprKind::StringLiteral(literal) = &arg.kind else { return false };
        let literal = literal.trim_start_matches('\\');
        literal.eq_ignore_ascii_case("ZipArchive")
            || literal.eq_ignore_ascii_case("zip")
            || zip_functions().iter().any(|function| literal.eq_ignore_ascii_case(function))
    })
}

/// Returns whether a static receiver names a zip class in a scoped expression.
/// `self`, `static`, and `parent` never resolve to a zip class at this position.
fn receiver_refs_zip(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_zip_class(name))
}

/// Returns whether an `instanceof` target references a zip class, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_zip(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_zip_class(name),
        InstanceOfTarget::Generic(class_type) => type_refs_zip(class_type),
        InstanceOfTarget::Expr(expr) => expr_refs_zip(expr),
    }
}

/// Returns whether a first-class-callable target references the zip surface: a free
/// function (`zip_exec(...)`), a static-method receiver, or an instance-method object
/// expression.
fn callable_target_refs_zip(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_zip_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_zip(receiver),
        CallableTarget::Method { object, .. } => expr_refs_zip(object),
    }
}

/// Returns whether a type expression names a zip class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_zip(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(|name| name_is_zip_class(&crate::names::pointer_target_leaf_name(name))),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_zip(inner)
        }
        TypeExpr::AssocArray { key, value } => type_refs_zip(key) || type_refs_zip(value),
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_zip) || type_refs_zip(ret)
        }
        TypeExpr::Named(name) => name_is_zip_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_zip_class(name) || args.iter().any(type_refs_zip)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_zip)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the zip
/// surface. Shared by function, method, and closure parameter lists.
fn params_ref_zip(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_zip)
            || default.as_ref().is_some_and(expr_refs_zip)
    })
}

/// Returns whether a `use Trait` clause names a zip class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_zip(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_zip_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_zip_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_zip_class)
                    || instead_of.iter().any(name_is_zip_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the zip
/// surface.
fn class_property_refs_zip(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_zip)
        || property.default.as_ref().is_some_and(expr_refs_zip)
}

/// Returns whether a method's parameters, variadic element type, return type, or body
/// reference the zip surface.
fn class_method_refs_zip(method: &ClassMethod) -> bool {
    params_ref_zip(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_zip)
        || method.return_type.as_ref().is_some_and(type_refs_zip)
        || method.body.iter().any(stmt_refs_zip)
}

/// Returns whether a class constant's initializer references the zip surface.
fn class_const_refs_zip(constant: &ClassConst) -> bool {
    expr_refs_zip(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the zip surface.
fn enum_case_refs_zip(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_zip)
}

/// Returns whether a `packed class` field's type references a zip class. A zip class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_zip(field: &PackedField) -> bool {
    type_refs_zip(&field.type_expr)
}

/// Returns whether an expression references the zip surface at any function-call,
/// class-name, or constant position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_zip(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(value) => value.contains("zip://"),
        ExprKind::IntLiteral(_)
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

        ExprKind::BinaryOp { left, right, .. } => expr_refs_zip(left) || expr_refs_zip(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_zip(value) || instanceof_target_refs_zip(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_zip(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_zip(value) || expr_refs_zip(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_zip(value) || expr_refs_zip(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_zip(target)
                || expr_refs_zip(value)
                || result_target.as_deref().is_some_and(expr_refs_zip)
                || prelude.iter().any(stmt_refs_zip)
        }
        ExprKind::FunctionCall { name, args } => {
            name_is_zip_function(name)
                || name_lookup_refs_zip(name, args)
                || args.iter().any(expr_refs_zip)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_zip),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_zip),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_zip(key) || expr_refs_zip(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_refs_zip),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_zip(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_zip) || expr_refs_zip(body)
                })
                || default.as_deref().is_some_and(expr_refs_zip)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_zip(array) || expr_refs_zip(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_zip(condition) || expr_refs_zip(then_expr) || expr_refs_zip(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_zip(expr),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_zip(params)
                || variadic_type.as_ref().is_some_and(type_refs_zip)
                || return_type.as_ref().is_some_and(type_refs_zip)
                || body.iter().any(stmt_refs_zip)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_zip(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_zip(callee) || args.iter().any(expr_refs_zip)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_zip_class(class_name)
                || reflection_refs_zip(class_name, args)
                || args.iter().any(expr_refs_zip)
        }
        ExprKind::NewGeneric { class_type, args } => {
            type_refs_zip(class_type) || args.iter().any(expr_refs_zip)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_zip(name_expr) || args.iter().any(expr_refs_zip)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_zip(class_name)
                || name_is_zip_class(fallback_class)
                || name_is_zip_class(required_parent)
                || args.iter().any(expr_refs_zip)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_zip(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_zip(object) || expr_refs_zip(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_zip(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_zip(object) || args.iter().any(expr_refs_zip)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_zip(object) || expr_refs_zip(method) || args.iter().any(expr_refs_zip),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_zip(receiver) || args.iter().any(expr_refs_zip)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_zip(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_zip(element_type) || expr_refs_zip(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_zip(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_zip(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_zip(receiver) || args.iter().any(expr_refs_zip)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_zip)
                || value.as_deref().is_some_and(expr_refs_zip)
        }
        ExprKind::IncludeValue { path, .. } => expr_refs_zip(path),
    }
}

/// Returns whether a statement references the zip surface at any function-call,
/// class-name, or constant position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_zip(stmt: &Stmt) -> bool {
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
            .any(|item| name_is_zip_class(&item.name) || name_is_zip_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_zip(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_zip(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_zip(condition)
                || then_body.iter().any(stmt_refs_zip)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_zip(cond) || body.iter().any(stmt_refs_zip))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_zip))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_zip)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_zip))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_zip(condition) || body.iter().any(stmt_refs_zip)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_zip)
                || condition.as_ref().is_some_and(expr_refs_zip)
                || update.as_deref().is_some_and(stmt_refs_zip)
                || body.iter().any(stmt_refs_zip)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_zip(index) || expr_refs_zip(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_zip(target) || expr_refs_zip(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_zip(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_zip(type_expr) || expr_refs_zip(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_zip(array) || body.iter().any(stmt_refs_zip)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_zip(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_zip) || body.iter().any(stmt_refs_zip)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_zip))
        }
        StmtKind::Include { path, .. } => expr_refs_zip(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_zip),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_zip)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_zip_class)
                        || catch.body.iter().any(stmt_refs_zip)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_zip))
        }
        StmtKind::FunctionDecl {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_zip(params)
                || variadic_type.as_ref().is_some_and(type_refs_zip)
                || return_type.as_ref().is_some_and(type_refs_zip)
                || body.iter().any(stmt_refs_zip)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_zip),
        StmtKind::ConstDecl { value, .. } => expr_refs_zip(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_zip(value),
        StmtKind::StaticVar { init, .. } => expr_refs_zip(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_zip_class)
                || implements.iter().any(name_is_zip_class)
                || trait_uses.iter().any(trait_use_refs_zip)
                || properties.iter().any(class_property_refs_zip)
                || methods.iter().any(class_method_refs_zip)
                || constants.iter().any(class_const_refs_zip)
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
            backing_type.as_ref().is_some_and(type_refs_zip)
                || cases.iter().any(enum_case_refs_zip)
                || implements.iter().any(name_is_zip_class)
                || trait_uses.iter().any(trait_use_refs_zip)
                || methods.iter().any(class_method_refs_zip)
                || constants.iter().any(class_const_refs_zip)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_zip),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_zip_class)
                || properties.iter().any(class_property_refs_zip)
                || methods.iter().any(class_method_refs_zip)
                || constants.iter().any(class_const_refs_zip)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_zip)
                || properties.iter().any(class_property_refs_zip)
                || methods.iter().any(class_method_refs_zip)
                || constants.iter().any(class_const_refs_zip)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_zip(object) || expr_refs_zip(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_zip(receiver) || expr_refs_zip(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_zip(receiver) || expr_refs_zip(index) || expr_refs_zip(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_zip(object) || expr_refs_zip(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_zip(object) || expr_refs_zip(index) || expr_refs_zip(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Global functions, imports, callable references and nested declarations enable injection.
    #[test]
    fn zip_detection_covers_php_name_positions() {
        for source in [
            "<?php zip_read($zip);",
            "<?php namespace App; zip_open();",
            "<?php use ZipArchive as Info; new Info();",
            "<?php use function zip_open as open; open();",
            "<?php $zip = \\zip_open(...);",
            "<?php function f(ZipArchive ...$infos): void {}",
            "<?php enum E { case A; public function inspect() { return new \\ZipArchive(); } }",
            "<?php new ReflectionFunction('zip_close');",
            r"<?php new \ReflectionFunction(function: '\ZiP_Close');",
            "<?php namespace App; use ReflectionFunction as Ref; new Ref('zip_read');",
            "<?php new ReflectionParameter(parameter: 'ZipArchive', function: 'zip_close');",
            "<?php var_dump(class_exists('ZipArchive'));",
            "<?php var_dump(extension_loaded('zip'));",
            "<?php var_dump(function_exists('zip_open'));",
            r"<?php fopen('zip://archive.zip#entry', 'rb');",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let ast = crate::parser::parse(&tokens).unwrap();
            assert!(program_uses_zip(&ast), "{source}");
        }
    }

    /// User namespace declarations and constant-only programs never opt into the bridge.
    #[test]
    fn zip_detection_respects_local_symbols() {
        for source in [
            "<?php namespace App; function zip_open() { return 1; } zip_open();",
            "<?php \\App\\zip_open();",
            "<?php echo PHP_VERSION;",
            r"<?php new ReflectionFunction('App\zip_close');",
            "<?php namespace App; class ReflectionFunction { public function __construct(string $name) {} } new ReflectionFunction('zip_close');",
            "<?php function custom($zip_close) {} new ReflectionParameter(function: 'custom', parameter: 'zip_close');",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let ast = crate::parser::parse(&tokens).unwrap();
            assert!(!program_uses_zip(&ast), "{source}");
        }
    }
}
