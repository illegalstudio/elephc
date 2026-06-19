//! Purpose:
//! Decides whether a parsed program references the timezone-identifier-listing
//! surface (`timezone_identifiers_list()` or `DateTimeZone::listIdentifiers()`) so
//! the `list_id_prelude` is injected only for programs that use it.
//!
//! Called from:
//! - `crate::list_id_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so function `Name`s are raw source text; matched
//!   case-insensitively on the unqualified last segment (PHP function names are
//!   case-insensitive and may be written `\timezone_identifiers_list`).
//! - Method receivers are untyped at this stage, so `listIdentifiers` is matched by
//!   name alone. A false positive (an unrelated `->listIdentifiers()`) only
//!   over-injects a self-contained function that codegen DCE drops if uncalled;
//!   soundness (never missing a real use, which would desugar to a call to an
//!   undefined function) is what matters, so the `match`es are exhaustive with no
//!   wildcard arm — a new AST node forces this file to be updated.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, Stmt, StmtKind, TraitUse, TypeExpr,
};

/// Returns whether any top-level statement references the identifier-listing
/// surface, so the prelude must be injected ahead of user code.
pub(super) fn program_uses_list_identifiers(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_listid)
}

/// Returns whether a function name is the `timezone_identifiers_list` procedural
/// alias, compared case-insensitively on its unqualified last segment.
fn name_is_listid_fn(name: &Name) -> bool {
    name.last_segment()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("timezone_identifiers_list"))
}

/// Returns whether a method name is `listIdentifiers`, compared case-insensitively
/// as PHP method names are.
fn method_is_listid(method: &str) -> bool {
    method.eq_ignore_ascii_case("listIdentifiers")
}

/// Returns whether a first-class-callable target references the surface via the
/// procedural function or the static/instance method name.
fn callable_target_refs_listid(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_listid_fn(name),
        CallableTarget::StaticMethod { method, .. } => method_is_listid(method),
        CallableTarget::Method { object, method } => {
            method_is_listid(method) || expr_refs_listid(object)
        }
    }
}

/// Returns whether any parameter's default value references the surface (type hints
/// cannot). Shared by function, method, and closure parameter lists.
fn params_ref_listid(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params
        .iter()
        .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_refs_listid))
}

/// Returns whether a `use Trait` clause's adaptations reference the surface (only
/// through nested expressions; trait/method names here are not call sites).
fn trait_use_refs_listid(_trait_use: &TraitUse) -> bool {
    false
}

/// Returns whether a class property's default value references the surface.
fn class_property_refs_listid(property: &ClassProperty) -> bool {
    property.default.as_ref().is_some_and(expr_refs_listid)
}

/// Returns whether a method's parameter defaults or body reference the surface.
fn class_method_refs_listid(method: &ClassMethod) -> bool {
    params_ref_listid(&method.params) || method.body.iter().any(stmt_refs_listid)
}

/// Returns whether a class constant's initializer references the surface.
fn class_const_refs_listid(constant: &ClassConst) -> bool {
    expr_refs_listid(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the surface.
fn enum_case_refs_listid(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_listid)
}

/// Returns whether a `packed class` field references the surface. Packed fields
/// carry only types, never call sites, but the field is walked for completeness.
fn packed_field_refs_listid(_field: &PackedField) -> bool {
    false
}

/// Returns whether an `instanceof` target's runtime-expression operand references
/// the surface (name targets are class positions, never call sites).
fn instanceof_target_refs_listid(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_refs_listid(expr),
    }
}

/// Returns whether an expression references the identifier-listing surface at any
/// call position, recursing into every child. The `match` is exhaustive so a new
/// `ExprKind` cannot silently bypass detection.
fn expr_refs_listid(expr: &Expr) -> bool {
    match &expr.kind {
        // `require`/`include` in expression position: recurse into the path expression. This is a
        // transient parser node expanded by the resolver before later passes, but the match must
        // stay exhaustive so a new `ExprKind` cannot silently bypass detection.
        ExprKind::IncludeValue { path, .. } => expr_refs_listid(path),
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
            name_is_listid_fn(name) || args.iter().any(expr_refs_listid)
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
        } => method_is_listid(method) || expr_refs_listid(object) || args.iter().any(expr_refs_listid),
        ExprKind::StaticMethodCall { method, args, .. } => {
            method_is_listid(method) || args.iter().any(expr_refs_listid)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_listid(target),

        ExprKind::BinaryOp { left, right, .. } => {
            expr_refs_listid(left) || expr_refs_listid(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_refs_listid(value) || instanceof_target_refs_listid(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_listid(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_listid(value) || expr_refs_listid(default)
        }
        ExprKind::Pipe { value, callable } => {
            expr_refs_listid(value) || expr_refs_listid(callable)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_listid(target)
                || expr_refs_listid(value)
                || result_target.as_deref().is_some_and(expr_refs_listid)
                || prelude.iter().any(stmt_refs_listid)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_listid),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_listid),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_listid(key) || expr_refs_listid(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_listid(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_listid) || expr_refs_listid(body)
                })
                || default.as_deref().is_some_and(expr_refs_listid)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_refs_listid(array) || expr_refs_listid(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refs_listid(condition)
                || expr_refs_listid(then_expr)
                || expr_refs_listid(else_expr)
        }
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_listid(expr),
        ExprKind::Closure { params, body, .. } => {
            params_ref_listid(params) || body.iter().any(stmt_refs_listid)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_listid(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_listid(callee) || args.iter().any(expr_refs_listid)
        }
        ExprKind::NewObject { args, .. } => args.iter().any(expr_refs_listid),
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_listid(name_expr) || args.iter().any(expr_refs_listid)
        }
        ExprKind::NewDynamicObject { class_name, args, .. } => {
            expr_refs_listid(class_name) || args.iter().any(expr_refs_listid)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_listid(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_listid(object) || expr_refs_listid(property)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        ExprKind::BufferNew { len, .. } => expr_refs_listid(len),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_refs_listid),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_listid)
                || value.as_deref().is_some_and(expr_refs_listid)
        }
    }
}

/// Returns whether a statement references the identifier-listing surface at any
/// call position, recursing into nested statements, expressions, and class
/// members. The `match` is exhaustive so a new `StmtKind` cannot silently bypass
/// detection.
fn stmt_refs_listid(stmt: &Stmt) -> bool {
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
            expr_refs_listid(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_listid(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_listid(condition)
                || then_body.iter().any(stmt_refs_listid)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_listid(cond) || body.iter().any(stmt_refs_listid))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_listid))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_listid)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_listid))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_listid(condition) || body.iter().any(stmt_refs_listid)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_listid)
                || condition.as_ref().is_some_and(expr_refs_listid)
                || update.as_deref().is_some_and(stmt_refs_listid)
                || body.iter().any(stmt_refs_listid)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_listid(index) || expr_refs_listid(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_listid(target) || expr_refs_listid(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_listid(value),
        StmtKind::TypedAssign { value, .. } => expr_refs_listid(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_listid(array) || body.iter().any(stmt_refs_listid)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_listid(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_listid) || body.iter().any(stmt_refs_listid)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_listid))
        }
        StmtKind::Include { path, .. } => expr_refs_listid(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_listid),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_listid)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(stmt_refs_listid))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_listid))
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            params_ref_listid(params) || body.iter().any(stmt_refs_listid)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_listid),
        StmtKind::ConstDecl { value, .. } => expr_refs_listid(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_listid(value),
        StmtKind::StaticVar { init, .. } => expr_refs_listid(init),
        StmtKind::ClassDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_listid)
                || properties.iter().any(class_property_refs_listid)
                || methods.iter().any(class_method_refs_listid)
                || constants.iter().any(class_const_refs_listid)
        }
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(enum_case_refs_listid),
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_listid),
        StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            properties.iter().any(class_property_refs_listid)
                || methods.iter().any(class_method_refs_listid)
                || constants.iter().any(class_const_refs_listid)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_listid)
                || properties.iter().any(class_property_refs_listid)
                || methods.iter().any(class_method_refs_listid)
                || constants.iter().any(class_const_refs_listid)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_listid(object) || expr_refs_listid(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_refs_listid(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_refs_listid(index) || expr_refs_listid(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_listid(object) || expr_refs_listid(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_listid(object) || expr_refs_listid(index) || expr_refs_listid(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the identifier-listing-usage AST walk: both call forms
    //! (the `timezone_identifiers_list` procedural function and the
    //! `DateTimeZone::listIdentifiers` static method) are detected, and unrelated
    //! programs are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_list_identifiers` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A procedural `timezone_identifiers_list(...)` call is detected.
    #[test]
    fn detects_procedural_call() {
        assert!(program_uses_list_identifiers(&parse(
            r#"<?php $z = timezone_identifiers_list(DateTimeZone::EUROPE);"#
        )));
    }

    /// A static `DateTimeZone::listIdentifiers()` call is detected.
    #[test]
    fn detects_static_method() {
        assert!(program_uses_list_identifiers(&parse(
            r#"<?php $z = DateTimeZone::listIdentifiers();"#
        )));
    }

    /// A nested reference inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_list_identifiers(&parse(
            r#"<?php function f() { return DateTimeZone::listIdentifiers(DateTimeZone::ASIA); }"#
        )));
    }

    /// Case-insensitive matching, as PHP function/method names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_uses_list_identifiers(&parse(
            r#"<?php $z = TIMEZONE_IDENTIFIERS_LIST();"#
        )));
    }

    /// A program with no identifier-listing use is not detected (including a
    /// mention only inside a string literal).
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_list_identifiers(&parse(
            r#"<?php $s = "listIdentifiers"; echo $s; $d = new DateTimeZone("UTC"); echo $d->getName();"#
        )));
    }
}
