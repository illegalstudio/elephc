//! Purpose:
//! Decides whether a parsed program references the mysqli surface — one of the
//! mysqli classes (`mysqli`, `mysqli_stmt`, `mysqli_result`,
//! `mysqli_sql_exception`) or a `mysqli_*` procedural function — so the mysqli
//! prelude is injected only for mysqli-using programs.
//!
//! Called from:
//! - `crate::mysqli_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text: a reference may
//!   be written `mysqli`, `\mysqli`, or `\Some\mysqli`, and PHP class/function
//!   names are case-insensitive. The walk matches the unqualified last segment
//!   case-insensitively.
//! - Two kinds of positions trigger injection (mirroring `image_prelude::detect`):
//!   (a) a *function call* (or first-class callable) whose name's last segment
//!   starts with `mysqli_`, and (b) a *class-name* position naming a mysqli class
//!   (new, static receivers, `instanceof`, `catch`, `extends`/`implements`, type
//!   hints, trait uses, `use` imports). The prefix is `mysqli_`, NOT `mysql`:
//!   the ancient `mysql_query` API must not inject this prelude.
//! - A bare global `MYSQLI_*` constant reference is a trigger too (the prelude
//!   declares ~50 of them, so a program whose only mention is a constant must
//!   still get them — see `name_is_mysqli_constant`).
//! - Capability probes (`class_exists('mysqli')`, `function_exists('mysqli_…')`,
//!   `defined('MYSQLI_…')`, `extension_loaded('mysqli')`) are string literals and
//!   deliberately do NOT trigger injection — same rule as the PDO prelude. A
//!   probe-only program honestly reports "not compiled in"; `--with-mysqli`
//!   exists to force the surface for such programs.
//! - Soundness over precision: a missed reference would drop the prelude and turn
//!   a valid program into an "undefined function/class" error, so the `match`es
//!   are exhaustive (no wildcard arm). Adding an AST node forces this file to be
//!   updated. False positives (e.g. a user function named `mysqli_helper`) only
//!   over-inject harmlessly — the prelude carries only declarations.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// Returns whether any top-level statement references the mysqli surface, so the
/// prelude must be injected ahead of user code.
pub(super) fn program_uses_mysqli(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_mysqli)
}

/// Returns whether `s` begins with `prefix`, compared ASCII-case-insensitively.
fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Returns whether `name`'s unqualified last segment is one of the mysqli
/// classes, compared case-insensitively to match PHP's case-insensitive class
/// names and any namespace/leading-backslash form.
fn name_is_mysqli_class(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        segment.eq_ignore_ascii_case("mysqli")
            || segment.eq_ignore_ascii_case("mysqli_stmt")
            || segment.eq_ignore_ascii_case("mysqli_result")
            || segment.eq_ignore_ascii_case("mysqli_sql_exception")
    })
}

/// Returns whether `name` is a bare global `MYSQLI_*` constant. Only the
/// single-segment global spelling matches (a namespaced `Vendor\MYSQLI_ASSOC`
/// is unrelated); PHP constant names are case-sensitive and the surface is
/// all-uppercase, so `MYSQLI_` is matched case-sensitively.
fn name_is_mysqli_constant(name: &Name) -> bool {
    matches!(name.parts.as_slice(), [constant] if constant.starts_with("MYSQLI_"))
}

/// Returns whether a function-call name denotes a mysqli procedural function:
/// the unqualified last segment starts with `mysqli_` (ASCII case-insensitive).
/// The `mysqli_` prefix deliberately excludes the ancient `mysql_*` API
/// (`mysql_query` must never inject this prelude).
fn name_is_mysqli_function(name: &Name) -> bool {
    name.last_segment()
        .is_some_and(|segment| starts_with_ci(segment, "mysqli_"))
}

/// Returns whether a `use` import names a mysqli class or function: an aliased
/// import (`use mysqli as Db;`) mentions the mysqli name only here, so skipping
/// imports would be a false negative.
fn import_refs_mysqli(name: &Name) -> bool {
    name_is_mysqli_class(name) || name_is_mysqli_function(name)
}

/// Returns whether a static receiver names a mysqli class (`mysqli::...`). `self`,
/// `static`, and `parent` never resolve to a mysqli class at this position.
fn receiver_refs_mysqli(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_mysqli_class(name))
}

/// Returns whether an `instanceof` target references a mysqli class, recursing
/// into the operand when the target is a runtime expression.
fn instanceof_target_refs_mysqli(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_mysqli_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_mysqli(expr),
    }
}

/// Returns whether a first-class-callable target references the mysqli surface:
/// a `mysqli_*` free function (`mysqli_query(...)`), a static-method receiver, or
/// an instance-method object expression.
fn callable_target_refs_mysqli(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_mysqli_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_mysqli(receiver),
        CallableTarget::Method { object, .. } => expr_refs_mysqli(object),
    }
}

/// Returns whether a type expression names a mysqli class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_mysqli(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_mysqli_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_mysqli(inner)
        }
        TypeExpr::Named(name) => name_is_mysqli_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_mysqli)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the
/// mysqli surface. Shared by function, method, and closure parameter lists.
fn params_ref_mysqli(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_mysqli)
            || default.as_ref().is_some_and(expr_refs_mysqli)
    })
}

/// Returns whether a `use Trait` clause names a mysqli class through its trait
/// list or any conflict-resolution adaptation.
fn trait_use_refs_mysqli(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_mysqli_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_mysqli_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_mysqli_class)
                    || instead_of.iter().any(name_is_mysqli_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the
/// mysqli surface.
fn class_property_refs_mysqli(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_mysqli)
        || property.default.as_ref().is_some_and(expr_refs_mysqli)
}

/// Returns whether a method's parameters, return type, or body reference the
/// mysqli surface.
fn class_method_refs_mysqli(method: &ClassMethod) -> bool {
    params_ref_mysqli(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_mysqli)
        || method.body.iter().any(stmt_refs_mysqli)
}

/// Returns whether a class constant's initializer references the mysqli surface.
fn class_const_refs_mysqli(constant: &ClassConst) -> bool {
    expr_refs_mysqli(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the mysqli
/// surface.
fn enum_case_refs_mysqli(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_mysqli)
}

/// Returns whether a `packed class` field's type references a mysqli class. A
/// mysqli class is never a valid packed field type, but the field is walked for
/// completeness.
fn packed_field_refs_mysqli(field: &PackedField) -> bool {
    type_refs_mysqli(&field.type_expr)
}

/// Returns whether an expression references the mysqli surface at any class-name
/// or function-call position, recursing into every child expression and
/// statement. The `match` is exhaustive so a newly added `ExprKind` cannot
/// silently bypass detection.
fn expr_refs_mysqli(expr: &Expr) -> bool {
    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        // Leaves and identifier-only forms carry no mysqli reference.
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

        // A bare global `MYSQLI_*` constant is a mysqli reference: the prelude
        // declares ~50 of them, and a program whose only mention is e.g.
        // `['mode' => MYSQLI_ASSOC]` would otherwise fail with "undefined
        // constant" — the failure the exhaustive walk exists to prevent. PDO
        // has no such gap because its constants are class constants reached
        // through a detected static receiver.
        ExprKind::ConstRef(name) => name_is_mysqli_constant(name),

        ExprKind::BinaryOp { left, right, .. } => {
            expr_refs_mysqli(left) || expr_refs_mysqli(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_refs_mysqli(value) || instanceof_target_refs_mysqli(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_mysqli(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_mysqli(value) || expr_refs_mysqli(default)
        }
        ExprKind::Pipe { value, callable } => {
            expr_refs_mysqli(value) || expr_refs_mysqli(callable)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_mysqli(target)
                || expr_refs_mysqli(value)
                || result_target.as_deref().is_some_and(expr_refs_mysqli)
                || prelude.iter().any(stmt_refs_mysqli)
        }
        ExprKind::FunctionCall { name, args } => {
            name_is_mysqli_function(name) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_mysqli),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_mysqli),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_mysqli(key) || expr_refs_mysqli(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_mysqli(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_mysqli) || expr_refs_mysqli(body)
                })
                || default.as_deref().is_some_and(expr_refs_mysqli)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_refs_mysqli(array) || expr_refs_mysqli(index)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refs_mysqli(condition)
                || expr_refs_mysqli(then_expr)
                || expr_refs_mysqli(else_expr)
        }
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_mysqli(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_mysqli(params)
                || return_type.as_ref().is_some_and(type_refs_mysqli)
                || body.iter().any(stmt_refs_mysqli)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_mysqli(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_mysqli(callee) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_mysqli_class(class_name) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_mysqli(name_expr) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_mysqli(class_name)
                || name_is_mysqli_class(fallback_class)
                || name_is_mysqli_class(required_parent)
                || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_mysqli(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_mysqli(object) || expr_refs_mysqli(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_mysqli(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_mysqli(object) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_refs_mysqli(object)
                || expr_refs_mysqli(method)
                || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_mysqli(receiver) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_mysqli(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_mysqli(element_type) || expr_refs_mysqli(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_mysqli(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_mysqli(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_mysqli(receiver) || args.iter().any(expr_refs_mysqli)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_mysqli)
                || value.as_deref().is_some_and(expr_refs_mysqli)
        }
    }
}

/// Returns whether a statement references the mysqli surface at any class-name or
/// function-call position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot
/// silently bypass detection.
fn stmt_refs_mysqli(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no class-name position and no child expr/stmt.
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

        // An aliased import (`use mysqli as Db;`) names mysqli only here; the
        // later `new Db()` carries the alias, so the import must be inspected too.
        StmtKind::UseDecl { imports } => imports.iter().any(|item| import_refs_mysqli(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_mysqli(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_mysqli(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_mysqli(condition)
                || then_body.iter().any(stmt_refs_mysqli)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_mysqli(cond) || body.iter().any(stmt_refs_mysqli))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_mysqli))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_mysqli)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_mysqli))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_mysqli(condition) || body.iter().any(stmt_refs_mysqli)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_mysqli)
                || condition.as_ref().is_some_and(expr_refs_mysqli)
                || update.as_deref().is_some_and(stmt_refs_mysqli)
                || body.iter().any(stmt_refs_mysqli)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_mysqli(index) || expr_refs_mysqli(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_mysqli(target) || expr_refs_mysqli(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_mysqli(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_mysqli(type_expr) || expr_refs_mysqli(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_mysqli(array) || body.iter().any(stmt_refs_mysqli)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_mysqli(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_mysqli) || body.iter().any(stmt_refs_mysqli)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_mysqli))
        }
        StmtKind::Include { path, .. } => expr_refs_mysqli(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_mysqli),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_mysqli)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_mysqli_class)
                        || catch.body.iter().any(stmt_refs_mysqli)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_mysqli))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_mysqli(params)
                || return_type.as_ref().is_some_and(type_refs_mysqli)
                || body.iter().any(stmt_refs_mysqli)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_mysqli),
        StmtKind::ConstDecl { value, .. } => expr_refs_mysqli(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_mysqli(value),
        StmtKind::StaticVar { init, .. } => expr_refs_mysqli(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_mysqli_class)
                || implements.iter().any(name_is_mysqli_class)
                || trait_uses.iter().any(trait_use_refs_mysqli)
                || properties.iter().any(class_property_refs_mysqli)
                || methods.iter().any(class_method_refs_mysqli)
                || constants.iter().any(class_const_refs_mysqli)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_mysqli)
                || cases.iter().any(enum_case_refs_mysqli)
        }
        StmtKind::PackedClassDecl { fields, .. } => {
            fields.iter().any(packed_field_refs_mysqli)
        }
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_mysqli_class)
                || properties.iter().any(class_property_refs_mysqli)
                || methods.iter().any(class_method_refs_mysqli)
                || constants.iter().any(class_const_refs_mysqli)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_mysqli)
                || properties.iter().any(class_property_refs_mysqli)
                || methods.iter().any(class_method_refs_mysqli)
                || constants.iter().any(class_const_refs_mysqli)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_mysqli(object) || expr_refs_mysqli(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_mysqli(receiver) || expr_refs_mysqli(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => {
            receiver_refs_mysqli(receiver)
                || expr_refs_mysqli(index)
                || expr_refs_mysqli(value)
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_mysqli(object) || expr_refs_mysqli(value)
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => {
            expr_refs_mysqli(object)
                || expr_refs_mysqli(property)
                || expr_refs_mysqli(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_refs_mysqli(object) || expr_refs_mysqli(index) || expr_refs_mysqli(value)
        }
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the mysqli-usage AST walk: class-name positions, `mysqli_*`
    //! procedural calls, and `use` imports are detected across `\`-qualified/case
    //! spellings; the ancient `mysql_*` API and non-mysqli mentions are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_mysqli` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse,
    /// before any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// `new mysqli(...)` at statement level is detected.
    #[test]
    fn detects_new_mysqli() {
        assert!(program_uses_mysqli(&parse(
            r#"<?php $db = new mysqli("localhost", "user", "pass");"#
        )));
    }

    /// A bare `new mysqli` with no arguments is detected.
    #[test]
    fn detects_new_mysqli_no_args() {
        assert!(program_uses_mysqli(&parse("<?php $db = new mysqli();")));
    }

    /// A `mysqli_query($db, $sql)` procedural call is detected via the prefix.
    #[test]
    fn detects_procedural_call() {
        assert!(program_uses_mysqli(&parse(
            "<?php $r = mysqli_query($db, $sql);"
        )));
    }

    /// `mysqli_report(...)` is detected (a `mysqli_`-prefixed function).
    #[test]
    fn detects_mysqli_report() {
        assert!(program_uses_mysqli(&parse("<?php mysqli_report(0);")));
    }

    /// An aliased import (`use mysqli as Db;`) is detected through the import name:
    /// the later `new Db()` carries only the alias.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_mysqli(&parse(
            r#"<?php use mysqli as Db; $db = new Db("localhost");"#
        )));
    }

    /// A `catch (mysqli_sql_exception $e)` clause type is detected.
    #[test]
    fn detects_catch_type() {
        assert!(program_uses_mysqli(&parse(
            "<?php try { f(); } catch (mysqli_sql_exception $e) { echo 1; }"
        )));
    }

    /// A parameter type hint (`function f(mysqli_result $r)`) is detected.
    #[test]
    fn detects_parameter_type() {
        assert!(program_uses_mysqli(&parse(
            "<?php function f(mysqli_result $r) { return $r; }"
        )));
    }

    /// A union return type (`mysqli_stmt|false`) is detected.
    #[test]
    fn detects_union_return_type() {
        assert!(program_uses_mysqli(&parse(
            "<?php function f(): mysqli_stmt|false { return false; }"
        )));
    }

    /// A class constant / static access (`mysqli::$reportMode`) is detected.
    #[test]
    fn detects_static_receiver() {
        assert!(program_uses_mysqli(&parse("<?php $m = mysqli::$reportMode;")));
    }

    /// A bare global `MYSQLI_*` constant is a mysqli reference (a config-array
    /// value or a helper that only forwards the constant would otherwise fail
    /// with "undefined constant").
    #[test]
    fn detects_bare_constant() {
        assert!(program_uses_mysqli(&parse(
            "<?php $cfg = ['mode' => MYSQLI_ASSOC];"
        )));
        assert!(program_uses_mysqli(&parse(
            "<?php function m() { return MYSQLI_REPORT_STRICT; }"
        )));
    }

    /// A non-mysqli bare constant, and a namespaced `MYSQLI_*`, are not
    /// mysqli references.
    #[test]
    fn ignores_unrelated_or_namespaced_constant() {
        assert!(!program_uses_mysqli(&parse("<?php echo SORT_ASC;")));
        assert!(!program_uses_mysqli(&parse("<?php echo Vendor\\MYSQLI_ASSOC;")));
    }

    /// Class-name matching is case-insensitive, as PHP class names are.
    #[test]
    fn detects_case_insensitive_class() {
        assert!(program_uses_mysqli(&parse(
            r#"<?php $db = new MySQLi("localhost");"#
        )));
    }

    /// Procedural matching is case-insensitive, as PHP function names are.
    #[test]
    fn detects_case_insensitive_function() {
        assert!(program_uses_mysqli(&parse("<?php MYSQLI_CONNECT();")));
    }

    /// A fully-qualified `\mysqli` reference is detected (last segment matches).
    #[test]
    fn detects_fully_qualified() {
        assert!(program_uses_mysqli(&parse(
            r#"<?php $db = new \mysqli("localhost");"#
        )));
    }

    /// A first-class callable over a mysqli procedural function is detected.
    #[test]
    fn detects_first_class_callable() {
        assert!(program_uses_mysqli(&parse(
            "<?php $fn = mysqli_connect(...);"
        )));
    }

    /// Capability probes are string literals and never trigger injection (same
    /// rule as the PDO prelude): a probe-only program honestly reports "not
    /// compiled in", and `--with-mysqli` exists to force the surface.
    #[test]
    fn ignores_string_literal_probes() {
        assert!(!program_uses_mysqli(&parse(
            "<?php echo class_exists('mysqli') ? '1' : '0'; \
             echo function_exists('mysqli_connect') ? '1' : '0'; \
             echo defined('MYSQLI_ASSOC') ? '1' : '0'; \
             var_dump(extension_loaded('mysqli'));"
        )));
    }

    /// The ancient `mysql_query` API (no `i`) must NOT inject the mysqli prelude.
    #[test]
    fn ignores_ancient_mysql_api() {
        assert!(!program_uses_mysqli(&parse(
            "<?php $r = mysql_query($sql); mysql_connect();"
        )));
    }

    /// Mentions of "mysqli" only inside string literals and variable names do not
    /// trigger detection.
    #[test]
    fn ignores_non_class_mentions() {
        assert!(!program_uses_mysqli(&parse(
            r#"<?php $mysqliNote = "use mysqli"; echo $mysqliNote;"#
        )));
    }

    /// A program with no mysqli mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_mysqli(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }

    /// A PDO program is not a mysqli program: the surfaces stay independent.
    #[test]
    fn ignores_pdo_program() {
        assert!(!program_uses_mysqli(&parse(
            r#"<?php $db = new PDO("sqlite::memory:"); $db->exec("SELECT 1");"#
        )));
    }
}
