//! Purpose:
//! Decides whether a parsed program references PHP's `var_export` (so the prelude is
//! injected only for programs that use it) and whether it already declares its own
//! `var_export` function (so a user definition is never clobbered).
//!
//! Called from:
//! - `crate::var_export_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so function `Name`s are raw source text; matched
//!   case-insensitively on the unqualified last segment (PHP function names are
//!   case-insensitive and may be written `\var_export`).
//! - A `"var_export"` string literal also counts as a reference so the
//!   `function_exists('var_export')` and `'var_export'` callable forms still inject
//!   the function. Over-injection (e.g. an unrelated string) only adds a small, later
//!   dead-code-eliminated function; soundness (never missing a real use) is what
//!   matters, so the `match`es are exhaustive with no wildcard arm.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, Stmt, StmtKind, TraitUse, TypeExpr,
};

/// Returns whether any top-level statement references `var_export`, so the prelude
/// must be injected ahead of user code.
pub(super) fn program_references_var_export(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_ve)
}

/// Returns whether the program already declares its own `var_export` function (at top
/// level or inside a namespace/guard/synthetic block), in which case the prelude must
/// not be injected so the user definition wins and there is no redeclaration error.
pub(super) fn program_declares_var_export(program: &[Stmt]) -> bool {
    program.iter().any(stmt_declares_var_export)
}

/// Returns whether a function name is `var_export`, compared case-insensitively on its
/// unqualified last segment.
fn name_is_var_export(name: &Name) -> bool {
    name.last_segment()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("var_export"))
}

/// Returns whether a statement declares a top-level `var_export` function, recursing
/// only into the block forms that can host a hoisted function declaration.
fn stmt_declares_var_export(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => name.eq_ignore_ascii_case("var_export"),
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body.iter().any(stmt_declares_var_export),
        _ => false,
    }
}

/// Returns whether a first-class-callable target references `var_export` via a function
/// name; method/static-method targets cannot name `var_export` but their receiver is
/// still walked for nested references.
fn callable_target_refs_ve(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_var_export(name),
        CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_refs_ve(object),
    }
}

/// Returns whether any parameter's default value references `var_export` (type hints
/// cannot). Shared by function, method, and closure parameter lists.
fn params_ref_ve(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params
        .iter()
        .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_refs_ve))
}

/// Returns whether a `use Trait` clause references `var_export`; trait/method names in
/// adaptations are not call sites, so this is always false.
fn trait_use_refs_ve(_trait_use: &TraitUse) -> bool {
    false
}

/// Returns whether a class property's default value references `var_export`.
fn class_property_refs_ve(property: &ClassProperty) -> bool {
    property.default.as_ref().is_some_and(expr_refs_ve)
}

/// Returns whether a method's parameter defaults or body reference `var_export`.
fn class_method_refs_ve(method: &ClassMethod) -> bool {
    params_ref_ve(&method.params) || method.body.iter().any(stmt_refs_ve)
}

/// Returns whether a class constant's initializer references `var_export`.
fn class_const_refs_ve(constant: &ClassConst) -> bool {
    expr_refs_ve(&constant.value)
}

/// Returns whether an enum case's backing-value expression references `var_export`.
fn enum_case_refs_ve(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_ve)
}

/// Returns whether a `packed class` field references `var_export`; packed fields carry
/// only types, never call sites.
fn packed_field_refs_ve(_field: &PackedField) -> bool {
    false
}

/// Returns whether an `instanceof` target's runtime-expression operand references
/// `var_export` (name targets are class positions, never call sites).
fn instanceof_target_refs_ve(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_refs_ve(expr),
    }
}

/// Returns whether an expression references `var_export` at any call position or as a
/// `"var_export"` string literal, recursing into every child. The `match` is exhaustive
/// so a new `ExprKind` cannot silently bypass detection.
fn expr_refs_ve(expr: &Expr) -> bool {
    match &expr.kind {
        // `require`/`include` in expression position: recurse into the path expression. This is a
        // transient parser node expanded by the resolver before later passes, but the match must
        // stay exhaustive so a new `ExprKind` cannot silently bypass detection.
        ExprKind::IncludeValue { path, .. } => expr_refs_ve(path),
        // A "var_export" string literal counts (function_exists/callable forms).
        ExprKind::StringLiteral(value) => value.eq_ignore_ascii_case("var_export"),

        // Leaves and identifier-only forms carry no call site.
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
        | ExprKind::ConstRef(_)
        | ExprKind::MagicConstant(_) => false,

        ExprKind::FunctionCall { name, args } => {
            name_is_var_export(name) || args.iter().any(expr_refs_ve)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_ve(object) || args.iter().any(expr_refs_ve)
        }
        ExprKind::StaticMethodCall { args, .. } => args.iter().any(expr_refs_ve),
        ExprKind::FirstClassCallable(target) => callable_target_refs_ve(target),

        ExprKind::BinaryOp { left, right, .. } => expr_refs_ve(left) || expr_refs_ve(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_ve(value) || instanceof_target_refs_ve(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_ve(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_ve(value) || expr_refs_ve(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_ve(value) || expr_refs_ve(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_ve(target)
                || expr_refs_ve(value)
                || result_target.as_deref().is_some_and(expr_refs_ve)
                || prelude.iter().any(stmt_refs_ve)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_ve),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_ve),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_ve(key) || expr_refs_ve(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_ve(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_ve) || expr_refs_ve(body)
                })
                || default.as_deref().is_some_and(expr_refs_ve)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_ve(array) || expr_refs_ve(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_ve(condition) || expr_refs_ve(then_expr) || expr_refs_ve(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_ve(expr),
        ExprKind::Closure { params, body, .. } => {
            params_ref_ve(params) || body.iter().any(stmt_refs_ve)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_ve(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_ve(callee) || args.iter().any(expr_refs_ve)
        }
        ExprKind::NewObject { args, .. } => args.iter().any(expr_refs_ve),
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_ve(name_expr) || args.iter().any(expr_refs_ve)
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => expr_refs_ve(class_name) || args.iter().any(expr_refs_ve),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_ve(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_ve(object) || expr_refs_ve(property)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        ExprKind::BufferNew { len, .. } => expr_refs_ve(len),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_refs_ve),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_ve)
                || value.as_deref().is_some_and(expr_refs_ve)
        }
    }
}

/// Returns whether a statement references `var_export` at any call position or string
/// literal, recursing into nested statements, expressions, and class members. The
/// `match` is exhaustive so a new `StmtKind` cannot silently bypass detection.
fn stmt_refs_ve(stmt: &Stmt) -> bool {
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
            expr_refs_ve(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_ve(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_ve(condition)
                || then_body.iter().any(stmt_refs_ve)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_ve(cond) || body.iter().any(stmt_refs_ve))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_ve))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_ve)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_ve))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_ve(condition) || body.iter().any(stmt_refs_ve)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_ve)
                || condition.as_ref().is_some_and(expr_refs_ve)
                || update.as_deref().is_some_and(stmt_refs_ve)
                || body.iter().any(stmt_refs_ve)
        }
        StmtKind::ArrayAssign { index, value, .. } => expr_refs_ve(index) || expr_refs_ve(value),
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_ve(target) || expr_refs_ve(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_ve(value),
        StmtKind::TypedAssign { value, .. } => expr_refs_ve(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_ve(array) || body.iter().any(stmt_refs_ve)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_ve(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_ve) || body.iter().any(stmt_refs_ve)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_ve))
        }
        StmtKind::Include { path, .. } => expr_refs_ve(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_ve),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_ve)
                || catches.iter().any(|catch| catch.body.iter().any(stmt_refs_ve))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_ve))
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            params_ref_ve(params) || body.iter().any(stmt_refs_ve)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_ve),
        StmtKind::ConstDecl { value, .. } => expr_refs_ve(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_ve(value),
        StmtKind::StaticVar { init, .. } => expr_refs_ve(init),
        StmtKind::ClassDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_ve)
                || properties.iter().any(class_property_refs_ve)
                || methods.iter().any(class_method_refs_ve)
                || constants.iter().any(class_const_refs_ve)
        }
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(enum_case_refs_ve),
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_ve),
        StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            properties.iter().any(class_property_refs_ve)
                || methods.iter().any(class_method_refs_ve)
                || constants.iter().any(class_const_refs_ve)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_ve)
                || properties.iter().any(class_property_refs_ve)
                || methods.iter().any(class_method_refs_ve)
                || constants.iter().any(class_const_refs_ve)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_ve(object) || expr_refs_ve(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_refs_ve(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_refs_ve(index) || expr_refs_ve(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_ve(object) || expr_refs_ve(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_ve(object) || expr_refs_ve(index) || expr_refs_ve(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the `var_export`-usage AST walk: a procedural call, a string
    //! reference (function_exists/callable), and a nested reference are detected, an
    //! unrelated program is not, and a user-declared `var_export` is recognized.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   detection runs inside `inject_if_used`.

    use super::*;

    /// Parses source the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A procedural `var_export(...)` call is detected.
    #[test]
    fn detects_procedural_call() {
        assert!(program_references_var_export(&parse(
            r#"<?php var_export([1, 2]);"#
        )));
    }

    /// A `"var_export"` string (function_exists/callable form) is detected.
    #[test]
    fn detects_string_reference() {
        assert!(program_references_var_export(&parse(
            r#"<?php if (function_exists("var_export")) { echo "y"; }"#
        )));
    }

    /// A nested reference inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_references_var_export(&parse(
            r#"<?php function f($x) { return var_export($x, true); }"#
        )));
    }

    /// Case-insensitive matching, as PHP function names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_references_var_export(&parse(
            r#"<?php VAR_EXPORT($x);"#
        )));
    }

    /// A program with no `var_export` use is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_references_var_export(&parse(
            r#"<?php $a = [1, 2]; echo count($a);"#
        )));
    }

    /// A user-declared `var_export` function is recognized so the prelude is skipped.
    #[test]
    fn detects_user_declaration() {
        assert!(program_declares_var_export(&parse(
            r#"<?php function var_export($v, $r = false) { return ""; }"#
        )));
    }

    /// A program that only calls `var_export` does not count as declaring it.
    #[test]
    fn call_is_not_a_declaration() {
        assert!(!program_declares_var_export(&parse(r#"<?php var_export($x);"#)));
    }
}
