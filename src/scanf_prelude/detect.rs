//! Purpose:
//! Decides whether a parsed program references PHP's `sscanf()` or `fscanf()`, so the scanf
//! prelude is injected only for programs that scan.
//!
//! Called from:
//! - `crate::scanf_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so function `Name`s are raw source text; matched
//!   case-insensitively on the unqualified last segment (PHP function names are
//!   case-insensitive and may be written `\sscanf`).
//! - THE NAME SET IS EXACT, NOT A PREFIX. `sscanf` and `fscanf` sit next to `sprintf`,
//!   `fprintf` and `fgets`, none of which needs the prelude; whole-segment equality keeps
//!   those programs untouched.
//! - A `"sscanf"` / `"fscanf"` string literal also counts, so `function_exists('sscanf')` and
//!   the `'sscanf'` callable form still inject the engine — the builtins lower to it through
//!   every path, not only a direct call.
//! - There is no "does the program declare its own" half, unlike the `dir`/`var_export`
//!   preludes: both names are registry builtins that PHP refuses to redeclare, so no user
//!   definition can collide with the injected one.
//! - Soundness over precision: a missed reference would leave the builtin lowering calling an
//!   undeclared function, so the `match`es are exhaustive with no wildcard arm. False positives
//!   only add declarations, which carry no top-level executable code.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, Stmt, StmtKind, TraitUse, TypeExpr,
};

/// The PHP functions whose lowering needs the scanf prelude.
pub(crate) const SCANF_FUNCTIONS: &[&str] = &["sscanf", "fscanf"];

/// Returns whether any top-level statement references `sscanf`/`fscanf`, so the prelude
/// must be injected ahead of user code.
pub(super) fn program_references_scanf(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_scanf)
}

/// Returns whether a name's unqualified last segment is one of the scanf builtins, compared
/// case-insensitively to match PHP's case-insensitive function names and any namespace or
/// leading-backslash form (`sscanf`, `\sscanf`, `\Some\sscanf`).
fn name_is_scanf(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        SCANF_FUNCTIONS
            .iter()
            .any(|candidate| segment.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether a string literal spells one of the scanf builtins, covering the
/// `function_exists('sscanf')` and `'sscanf'` callable forms.
fn literal_is_scanf(value: &str) -> bool {
    SCANF_FUNCTIONS
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

/// Returns whether a call picks its METHOD at run time, and so could reach `fscanf()`.
///
/// `$obj->$name()` desugars to `call_user_func([$obj, $name], …)`, and the backend then emits
/// every method of the classes the program constructs so the dispatch ladder has somewhere to
/// land — `SplFileObject::fscanf()` among them, whose synthesized body calls the engine. Without
/// this the engine is not injected and that body compiles a call to a symbol that does not
/// exist: a link failure, not a diagnostic.
///
/// Deliberately coarse, exactly like `method_is_scanf` above: a program that dispatches
/// dynamically pays for a few hundred lines of PHP it may never reach.
fn call_may_reach_any_method(name: &Name, args: &[Expr]) -> bool {
    let dispatches_dynamically = name.last_segment().is_some_and(|segment| {
        segment.eq_ignore_ascii_case("call_user_func")
            || segment.eq_ignore_ascii_case("call_user_func_array")
    });
    if !dispatches_dynamically {
        return false;
    }
    let Some(first) = args.first() else {
        return false;
    };
    let ExprKind::ArrayLiteral(parts) = &first.kind else {
        // A callable held in a variable names nothing this walk can read.
        return !matches!(first.kind, ExprKind::StringLiteral(_));
    };
    parts
        .get(1)
        .is_some_and(|method| !matches!(method.kind, ExprKind::StringLiteral(_)))
}

/// Returns whether a METHOD name is one the scanf engine serves.
///
/// `SplFileObject::fscanf()` is compiled from a synthetic body the CHECKER materializes, long
/// after this walk runs, and that body calls `sscanf()`. Detecting only the free-function
/// syntax therefore left `$file->fscanf(...)` compiling a call to an engine that was never
/// injected — which is a jump to an absent symbol, not a diagnostic. Matching the method name
/// here is deliberately coarse: an unrelated class with its own `fscanf()` method only pays for
/// a few hundred lines of unreachable PHP.
fn method_is_scanf(method: &str) -> bool {
    SCANF_FUNCTIONS
        .iter()
        .any(|candidate| method.eq_ignore_ascii_case(candidate))
}

/// Returns whether a first-class-callable target references a scanf builtin via a function
/// name; method/static-method targets cannot name one, but their receiver is still walked.
fn callable_target_refs_scanf(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_scanf(name),
        CallableTarget::StaticMethod { .. } => false,
        CallableTarget::Method { object, .. } => expr_refs_scanf(object),
    }
}

/// Returns whether any parameter's default value references a scanf builtin (type hints
/// cannot). Shared by function, method, and closure parameter lists.
fn params_ref_scanf(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params
        .iter()
        .any(|(_, _, default, _)| default.as_ref().is_some_and(expr_refs_scanf))
}

/// Returns whether a `use Trait` clause references a scanf builtin; trait and method names in
/// adaptations are not call sites, so this is always false.
fn trait_use_refs_scanf(_trait_use: &TraitUse) -> bool {
    false
}

/// Returns whether a class property's default value references a scanf builtin.
fn class_property_refs_scanf(property: &ClassProperty) -> bool {
    property.default.as_ref().is_some_and(expr_refs_scanf)
}

/// Returns whether a method's parameter defaults or body reference a scanf builtin.
fn class_method_refs_scanf(method: &ClassMethod) -> bool {
    params_ref_scanf(&method.params) || method.body.iter().any(stmt_refs_scanf)
}

/// Returns whether a class constant's initializer references a scanf builtin.
fn class_const_refs_scanf(constant: &ClassConst) -> bool {
    expr_refs_scanf(&constant.value)
}

/// Returns whether an enum case's backing-value expression references a scanf builtin.
fn enum_case_refs_scanf(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_scanf)
}

/// Returns whether a `packed class` field references a scanf builtin; packed fields carry
/// only types, never call sites.
fn packed_field_refs_scanf(_field: &PackedField) -> bool {
    false
}

/// Returns whether an `instanceof` target's runtime-expression operand references a scanf
/// builtin (name targets are class positions, never call sites).
fn instanceof_target_refs_scanf(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_refs_scanf(expr),
    }
}

/// Returns whether an expression references a scanf builtin at any call position or as a
/// matching string literal, recursing into every child. The `match` is exhaustive so a new
/// `ExprKind` cannot silently bypass detection.
fn expr_refs_scanf(expr: &Expr) -> bool {
    match &expr.kind {
        // `require`/`include` in expression position: recurse into the path expression. This is
        // a transient parser node expanded by the resolver before later passes, but the match
        // must stay exhaustive so a new `ExprKind` cannot silently bypass detection.
        ExprKind::IncludeValue { path, .. } => expr_refs_scanf(path),
        ExprKind::StringLiteral(value) => literal_is_scanf(value),

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
            name_is_scanf(name)
                || call_may_reach_any_method(name, args)
                || args.iter().any(expr_refs_scanf)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
            ..
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
            ..
        } => {
            method_is_scanf(method) || expr_refs_scanf(object) || args.iter().any(expr_refs_scanf)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_refs_scanf(object) || expr_refs_scanf(method) || args.iter().any(expr_refs_scanf)
        }
        ExprKind::StaticMethodCall { method, args, .. } => {
            method_is_scanf(method) || args.iter().any(expr_refs_scanf)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_scanf(target),

        ExprKind::BinaryOp { left, right, .. } => expr_refs_scanf(left) || expr_refs_scanf(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_scanf(value) || instanceof_target_refs_scanf(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_scanf(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_scanf(value) || expr_refs_scanf(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_scanf(value) || expr_refs_scanf(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_scanf(target)
                || expr_refs_scanf(value)
                || result_target.as_deref().is_some_and(expr_refs_scanf)
                || prelude.iter().any(stmt_refs_scanf)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_scanf),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_scanf),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_scanf(key) || expr_refs_scanf(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_scanf(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_scanf) || expr_refs_scanf(body)
                })
                || default.as_deref().is_some_and(expr_refs_scanf)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_refs_scanf(array) || expr_refs_scanf(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refs_scanf(condition)
                || expr_refs_scanf(then_expr)
                || expr_refs_scanf(else_expr)
        }
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_scanf(expr),
        ExprKind::Closure { params, body, .. } => {
            params_ref_scanf(params) || body.iter().any(stmt_refs_scanf)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_scanf(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_scanf(callee) || args.iter().any(expr_refs_scanf)
        }
        ExprKind::NewObject { args, .. } => args.iter().any(expr_refs_scanf),
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_scanf(name_expr) || args.iter().any(expr_refs_scanf)
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => expr_refs_scanf(class_name) || args.iter().any(expr_refs_scanf),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_scanf(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_scanf(object) || expr_refs_scanf(property)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        ExprKind::BufferNew { len, .. } => expr_refs_scanf(len),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
        ExprKind::ObjectClassName { object } => expr_refs_scanf(object),
        ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_refs_scanf),
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_scanf)
                || value.as_deref().is_some_and(expr_refs_scanf)
        }
    }
}

/// Returns whether a statement references a scanf builtin at any call position or string
/// literal, recursing into nested statements, expressions, and class members. The `match` is
/// exhaustive so a new `StmtKind` cannot silently bypass detection.
fn stmt_refs_scanf(stmt: &Stmt) -> bool {
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
            expr_refs_scanf(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_scanf(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_scanf(condition)
                || then_body.iter().any(stmt_refs_scanf)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_scanf(cond) || body.iter().any(stmt_refs_scanf))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_scanf))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_scanf)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_scanf))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_scanf(condition) || body.iter().any(stmt_refs_scanf)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_scanf)
                || condition.as_ref().is_some_and(expr_refs_scanf)
                || update.as_deref().is_some_and(stmt_refs_scanf)
                || body.iter().any(stmt_refs_scanf)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_scanf(index) || expr_refs_scanf(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_scanf(target) || expr_refs_scanf(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_scanf(value),
        StmtKind::TypedAssign { value, .. } => expr_refs_scanf(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_scanf(array) || body.iter().any(stmt_refs_scanf)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_scanf(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_scanf) || body.iter().any(stmt_refs_scanf)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_scanf))
        }
        StmtKind::Include { path, .. } => expr_refs_scanf(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_scanf),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_scanf)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(stmt_refs_scanf))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_scanf))
        }
        StmtKind::FunctionDecl { params, body, .. } => {
            params_ref_scanf(params) || body.iter().any(stmt_refs_scanf)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_scanf),
        StmtKind::ConstDecl { value, .. } => expr_refs_scanf(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_scanf(value),
        StmtKind::StaticVar { init, .. } => expr_refs_scanf(init),
        StmtKind::ClassDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_scanf)
                || properties.iter().any(class_property_refs_scanf)
                || methods.iter().any(class_method_refs_scanf)
                || constants.iter().any(class_const_refs_scanf)
        }
        StmtKind::EnumDecl { cases, .. } => cases.iter().any(enum_case_refs_scanf),
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_scanf),
        StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            properties.iter().any(class_property_refs_scanf)
                || methods.iter().any(class_method_refs_scanf)
                || constants.iter().any(class_const_refs_scanf)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_scanf)
                || properties.iter().any(class_property_refs_scanf)
                || methods.iter().any(class_method_refs_scanf)
                || constants.iter().any(class_const_refs_scanf)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_scanf(object) || expr_refs_scanf(value)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_refs_scanf(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_refs_scanf(index) || expr_refs_scanf(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_scanf(object) || expr_refs_scanf(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_scanf(object) || expr_refs_scanf(index) || expr_refs_scanf(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the scanf-usage AST walk: procedural calls, a string reference
    //! (function_exists/callable), and a nested reference are detected, while the
    //! neighbouring `sprintf`/`fprintf`/`fgets` builtins are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which detection
    //!   runs inside `inject_if_used`.

    use super::*;

    /// Parses source the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// Both procedural calls are detected.
    #[test]
    fn detects_procedural_calls() {
        assert!(program_references_scanf(&parse(
            r#"<?php sscanf("a 1", "%s %d");"#
        )));
        assert!(program_references_scanf(&parse(
            r#"<?php $h = fopen("f", "r"); fscanf($h, "%d");"#
        )));
    }

    /// A `"sscanf"` string (function_exists/callable form) is detected.
    #[test]
    fn detects_string_reference() {
        assert!(program_references_scanf(&parse(
            r#"<?php if (function_exists("sscanf")) { echo "y"; }"#
        )));
    }

    /// A nested reference inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_references_scanf(&parse(
            r#"<?php function f(string $x) { return sscanf($x, "%d"); }"#
        )));
    }

    /// Case-insensitive matching, as PHP function names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_references_scanf(&parse(r#"<?php SSCANF($x, "%d");"#)));
    }

    /// The neighbouring formatting and stream builtins never pull the prelude in.
    #[test]
    fn ignores_neighbouring_builtins() {
        assert!(!program_references_scanf(&parse(
            r#"<?php $h = fopen("f", "r"); echo sprintf("%d", 1); fprintf($h, "%d", 1); fgets($h);"#
        )));
    }
}
