//! Purpose:
//! Decides whether a parsed program references the timezone-introspection surface
//! (`timezone_location_get`/`timezone_transitions_get`/`timezone_abbreviations_list`
//! or the `getLocation`/`getTransitions`/`listAbbreviations` methods) so the
//! `tz_prelude` is injected only for programs that use it.
//!
//! Called from:
//! - `crate::tz_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so function `Name`s are raw source text; matched
//!   case-insensitively on the unqualified last segment (PHP function names are
//!   case-insensitive and may be written `\timezone_location_get`).
//! - Method receivers are untyped at this stage, so instance methods are matched
//!   by name alone. A false positive (an unrelated `->getLocation()`) only
//!   over-injects the prelude and over-links the bridge harmlessly; soundness
//!   (never missing a real use) is what matters, so the `match`es are exhaustive
//!   with no wildcard arm — a new AST node forces this file to be updated.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, Stmt, StmtKind, TraitUse, TypeExpr,
};

/// Returns whether any top-level statement references the introspection surface,
/// so the prelude must be injected ahead of user code.
pub(super) fn program_uses_tz_introspection(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_tz)
}

/// Returns whether a function name is one of the introspection procedural
/// functions, compared case-insensitively on its unqualified last segment.
fn name_is_tz_fn(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        segment.eq_ignore_ascii_case("timezone_location_get")
            || segment.eq_ignore_ascii_case("timezone_transitions_get")
            || segment.eq_ignore_ascii_case("timezone_abbreviations_list")
    })
}

/// Returns whether a method name is one of the three introspection methods,
/// compared case-insensitively as PHP method names are.
fn method_is_tz(method: &str) -> bool {
    method.eq_ignore_ascii_case("getLocation")
        || method.eq_ignore_ascii_case("getTransitions")
        || method.eq_ignore_ascii_case("listAbbreviations")
}

/// Returns whether a first-class-callable target references the introspection
/// surface via a procedural function or a method name.
fn callable_target_refs_tz(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_tz_fn(name),
        CallableTarget::StaticMethod { method, .. } => method_is_tz(method),
        CallableTarget::Method { object, method } => method_is_tz(method) || expr_refs_tz(object),
    }
}

/// Returns whether any parameter's default value references the introspection
/// surface (type hints cannot). Shared by function, method, and closure lists.
fn params_ref_tz(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params
        .iter()
        .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_refs_tz))
}

/// Returns whether a `use Trait` clause's adaptations reference the surface (only
/// through nested expressions; trait/method names here are not call sites).
fn trait_use_refs_tz(_trait_use: &TraitUse) -> bool {
    false
}

/// Returns whether a class property's default value references the surface.
fn class_property_refs_tz(property: &ClassProperty) -> bool {
    property.default.as_ref().is_some_and(expr_refs_tz)
}

/// Returns whether a method's parameter defaults or body reference the surface.
fn class_method_refs_tz(method: &ClassMethod) -> bool {
    params_ref_tz(&method.params) || method.body.iter().any(stmt_refs_tz)
}

/// Returns whether a class constant's initializer references the surface.
fn class_const_refs_tz(constant: &ClassConst) -> bool {
    expr_refs_tz(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the surface.
fn enum_case_refs_tz(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_tz)
}

/// Returns whether a `packed class` field references the surface. Packed fields
/// carry only types, never call sites, but the field is walked for completeness.
fn packed_field_refs_tz(_field: &PackedField) -> bool {
    false
}

/// Returns whether an `instanceof` target's runtime-expression operand references
/// the surface (name targets are class positions, never call sites).
fn instanceof_target_refs_tz(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_refs_tz(expr),
    }
}

/// Returns whether an expression references the introspection surface at any call
/// position, recursing into every child. The `match` is exhaustive so a new
/// `ExprKind` cannot silently bypass detection.
fn expr_refs_tz(expr: &Expr) -> bool {
    match &expr.kind {
        // `require`/`include` in expression position: recurse into the path expression. This is a
        // transient parser node expanded by the resolver before later passes, but the match must
        // stay exhaustive so a new `ExprKind` cannot silently bypass detection.
        ExprKind::IncludeValue { path, .. } => expr_refs_tz(path),
        // Leaves and identifier-only forms carry no call site.
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
        | ExprKind::ConstRef(_)
        | ExprKind::MagicConstant(_) => false,

        ExprKind::FunctionCall { name, args } => {
            name_is_tz_fn(name) || args.iter().any(expr_refs_tz)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => method_is_tz(method) || expr_refs_tz(object) || args.iter().any(expr_refs_tz),
        ExprKind::StaticMethodCall { method, args, .. } => {
            method_is_tz(method) || args.iter().any(expr_refs_tz)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_tz(target),

        ExprKind::BinaryOp { left, right, .. } => expr_refs_tz(left) || expr_refs_tz(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_tz(value) || instanceof_target_refs_tz(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_tz(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_tz(value) || expr_refs_tz(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_tz(value) || expr_refs_tz(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_tz(target)
                || expr_refs_tz(value)
                || result_target.as_deref().is_some_and(expr_refs_tz)
                || prelude.iter().any(stmt_refs_tz)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_tz),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_tz),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_tz(key) || expr_refs_tz(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_tz(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_tz) || expr_refs_tz(body)
                })
                || default.as_deref().is_some_and(expr_refs_tz)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_tz(array) || expr_refs_tz(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_tz(condition) || expr_refs_tz(then_expr) || expr_refs_tz(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_tz(expr),
        ExprKind::Closure { params, body, .. } => {
            params_ref_tz(params) || body.iter().any(stmt_refs_tz)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_tz(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_tz(callee) || args.iter().any(expr_refs_tz)
        }
        ExprKind::NewObject { args, .. } => args.iter().any(expr_refs_tz),
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_tz(name_expr) || args.iter().any(expr_refs_tz)
        }
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            expr_refs_tz(class_name) || args.iter().any(expr_refs_tz)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_tz(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_tz(object) || expr_refs_tz(property)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        ExprKind::BufferNew { len, .. } => expr_refs_tz(len),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_refs_tz),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_tz)
                || value.as_deref().is_some_and(expr_refs_tz)
        }
    }
}

/// Returns whether a statement references the introspection surface at any call
/// position, recursing into nested statements, expressions, and class members.
/// The `match` is exhaustive so a new `StmtKind` cannot silently bypass detection.
fn stmt_refs_tz(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no call position and no child expr/stmt.
        StmtKind::RefAssign { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_tz(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_tz(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_tz(condition)
                || then_body.iter().any(stmt_refs_tz)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_tz(cond) || body.iter().any(stmt_refs_tz))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tz))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_tz)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tz))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_tz(condition) || body.iter().any(stmt_refs_tz)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_tz)
                || condition.as_ref().is_some_and(expr_refs_tz)
                || update.as_deref().is_some_and(stmt_refs_tz)
                || body.iter().any(stmt_refs_tz)
        }
        StmtKind::ArrayAssign { index, value, .. } => expr_refs_tz(index) || expr_refs_tz(value),
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_tz(target) || expr_refs_tz(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_tz(value),
        StmtKind::TypedAssign { value, .. } => expr_refs_tz(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_tz(array) || body.iter().any(stmt_refs_tz)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_tz(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_tz) || body.iter().any(stmt_refs_tz)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tz))
        }
        StmtKind::Include { path, .. } => expr_refs_tz(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_tz),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_tz)
                || catches.iter().any(|catch| catch.body.iter().any(stmt_refs_tz))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tz))
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            params_ref_tz(params) || body.iter().any(stmt_refs_tz)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_tz),
        StmtKind::ConstDecl { value, .. } => expr_refs_tz(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_tz(value),
        StmtKind::StaticVar { init, .. } => expr_refs_tz(init),
        StmtKind::ClassDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_tz)
                || properties.iter().any(class_property_refs_tz)
                || methods.iter().any(class_method_refs_tz)
                || constants.iter().any(class_const_refs_tz)
        }
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(enum_case_refs_tz),
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_tz),
        StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            properties.iter().any(class_property_refs_tz)
                || methods.iter().any(class_method_refs_tz)
                || constants.iter().any(class_const_refs_tz)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_tz)
                || properties.iter().any(class_property_refs_tz)
                || methods.iter().any(class_method_refs_tz)
                || constants.iter().any(class_const_refs_tz)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_tz(object) || expr_refs_tz(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_refs_tz(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_refs_tz(index) || expr_refs_tz(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_tz(object) || expr_refs_tz(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_tz(object) || expr_refs_tz(index) || expr_refs_tz(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the tz-introspection-usage AST walk: every call form (the
    //! three procedural functions and the three methods) is detected, and
    //! unrelated programs are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_tz_introspection` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A procedural `timezone_location_get(...)` call is detected.
    #[test]
    fn detects_procedural_call() {
        assert!(program_uses_tz_introspection(&parse(
            r#"<?php $l = timezone_location_get($tz);"#
        )));
    }

    /// An instance `->getTransitions()` call is detected (by method name).
    #[test]
    fn detects_instance_method() {
        assert!(program_uses_tz_introspection(&parse(
            r#"<?php $t = $tz->getTransitions();"#
        )));
    }

    /// A static `DateTimeZone::listAbbreviations()` call is detected.
    #[test]
    fn detects_static_method() {
        assert!(program_uses_tz_introspection(&parse(
            r#"<?php $a = DateTimeZone::listAbbreviations();"#
        )));
    }

    /// A nested reference inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_tz_introspection(&parse(
            r#"<?php function f($z) { return $z->getLocation(); }"#
        )));
    }

    /// Case-insensitive matching, as PHP function/method names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_uses_tz_introspection(&parse(
            r#"<?php $l = TIMEZONE_LOCATION_GET($tz);"#
        )));
    }

    /// A program with no introspection use is not detected (including a mention
    /// only inside a string literal).
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_tz_introspection(&parse(
            r#"<?php $s = "timezone_location_get"; echo $s; $d = new DateTimeZone("UTC"); echo $d->getName();"#
        )));
    }
}
