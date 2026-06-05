//! Purpose:
//! Decides whether a parsed program references the PDO standard-library classes
//! (`PDO`, `PDOStatement`, `PDOException`) so the prelude is injected only for
//! PDO-using programs. Replaces a `format!("{:?}")` substring scan with a precise
//! AST walk that inspects only class-name positions.
//!
//! Called from:
//! - `crate::pdo_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text: a reference may
//!   be written `PDO`, `\PDO`, or `\Some\PDO`, and PHP class names are
//!   case-insensitive. The walk therefore matches the unqualified last segment
//!   case-insensitively.
//! - Soundness over precision: a missed reference would drop the prelude and break
//!   compilation, so the `match`es are exhaustive (no wildcard arm). Adding an AST
//!   node forces this file to be updated. False positives (e.g. a class literally
//!   named `PDOThing`) only over-link the bridge harmlessly.
//! - Positions where a `Name` denotes a *class* are matched (new, static
//!   receivers, `instanceof`, `catch`, `extends`/`implements`, type hints, trait
//!   uses). `use` imports are matched too: `use PDO as Db;` references the PDO
//!   name only in the import, and the later `new Db()` carries the alias, which
//!   the walk cannot otherwise connect back to PDO — so skipping imports would be
//!   a false negative. Function/constant name positions are not class references
//!   and are skipped.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// Returns whether any top-level statement references a PDO class, so the prelude
/// must be injected ahead of user code.
pub(super) fn program_uses_pdo(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_pdo)
}

/// Returns whether `name`'s unqualified last segment is one of the PDO classes,
/// compared case-insensitively to match PHP's case-insensitive class names and
/// any namespace/leading-backslash form (`PDO`, `\PDO`, `\Some\PDO`).
fn name_is_pdo(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        segment.eq_ignore_ascii_case("PDO")
            || segment.eq_ignore_ascii_case("PDOStatement")
            || segment.eq_ignore_ascii_case("PDOException")
    })
}

/// Returns whether a static receiver names a PDO class (`PDO::...`). `self`,
/// `static`, and `parent` never resolve to a PDO class at this position.
fn receiver_refs_pdo(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_pdo(name))
}

/// Returns whether an `instanceof` target references a PDO class, recursing into
/// the operand when the target is a runtime expression.
fn instanceof_target_refs_pdo(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_pdo(name),
        InstanceOfTarget::Expr(expr) => expr_refs_pdo(expr),
    }
}

/// Returns whether a first-class-callable target references a PDO class via a
/// static-method receiver or an instance-method object expression.
fn callable_target_refs_pdo(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) => false,
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_pdo(receiver),
        CallableTarget::Method { object, .. } => expr_refs_pdo(object),
    }
}

/// Returns whether a type expression names a PDO class, recursing through
/// nullable/union/buffer wrappers and `ptr<Class>` targets.
fn type_refs_pdo(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_pdo),
        TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => type_refs_pdo(inner),
        TypeExpr::Named(name) => name_is_pdo(name),
        TypeExpr::Union(members) => members.iter().any(type_refs_pdo),
    }
}

/// Returns whether any parameter's type hint or default value references a PDO
/// class. Shared by function, method, and closure parameter lists.
fn params_ref_pdo(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_pdo)
            || default.as_ref().is_some_and(expr_refs_pdo)
    })
}

/// Returns whether a `use Trait` clause names a PDO class through its trait list
/// or any conflict-resolution adaptation.
fn trait_use_refs_pdo(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_pdo)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_pdo)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => trait_name.as_ref().is_some_and(name_is_pdo) || instead_of.iter().any(name_is_pdo),
        })
}

/// Returns whether a class property's type hint or default value references a PDO
/// class.
fn class_property_refs_pdo(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_pdo)
        || property.default.as_ref().is_some_and(expr_refs_pdo)
}

/// Returns whether a method's parameters, return type, or body reference a PDO
/// class.
fn class_method_refs_pdo(method: &ClassMethod) -> bool {
    params_ref_pdo(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_pdo)
        || method.body.iter().any(stmt_refs_pdo)
}

/// Returns whether a class constant's initializer references a PDO class.
fn class_const_refs_pdo(constant: &ClassConst) -> bool {
    expr_refs_pdo(&constant.value)
}

/// Returns whether an enum case's backing-value expression references a PDO class.
fn enum_case_refs_pdo(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_pdo)
}

/// Returns whether a `packed class` field's type references a PDO class. PDO is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_pdo(field: &PackedField) -> bool {
    type_refs_pdo(&field.type_expr)
}

/// Returns whether an expression references a PDO class at any class-name
/// position, recursing into every child expression and statement. The `match` is
/// exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_pdo(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no class reference.
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

        ExprKind::BinaryOp { left, right, .. } => expr_refs_pdo(left) || expr_refs_pdo(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_pdo(value) || instanceof_target_refs_pdo(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_pdo(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_pdo(value) || expr_refs_pdo(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_pdo(value) || expr_refs_pdo(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_pdo(target)
                || expr_refs_pdo(value)
                || result_target.as_deref().is_some_and(expr_refs_pdo)
                || prelude.iter().any(stmt_refs_pdo)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_pdo),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_pdo),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_pdo(key) || expr_refs_pdo(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_pdo(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_pdo) || expr_refs_pdo(body)
                })
                || default.as_deref().is_some_and(expr_refs_pdo)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_pdo(array) || expr_refs_pdo(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_pdo(condition) || expr_refs_pdo(then_expr) || expr_refs_pdo(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_pdo(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_pdo(params)
                || return_type.as_ref().is_some_and(type_refs_pdo)
                || body.iter().any(stmt_refs_pdo)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_pdo(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_pdo(callee) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_pdo(class_name) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_pdo(name_expr) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_pdo(class_name)
                || name_is_pdo(fallback_class)
                || name_is_pdo(required_parent)
                || args.iter().any(expr_refs_pdo)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_pdo(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_pdo(object) || expr_refs_pdo(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_pdo(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_pdo(object) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_pdo(receiver) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_pdo(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_pdo(element_type) || expr_refs_pdo(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_pdo(receiver),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_pdo(receiver) || args.iter().any(expr_refs_pdo)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_pdo)
                || value.as_deref().is_some_and(expr_refs_pdo)
        }
    }
}

/// Returns whether a statement references a PDO class at any class-name position,
/// recursing into nested statements, expressions, and class members. The `match`
/// is exhaustive so a newly added `StmtKind` cannot silently bypass detection.
fn stmt_refs_pdo(stmt: &Stmt) -> bool {
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

        // An aliased import (`use PDO as Db;`) names PDO only here; the later
        // `new Db()` carries the alias, so the import must be inspected too.
        StmtKind::UseDecl { imports } => imports.iter().any(|item| name_is_pdo(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_pdo(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_pdo(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_pdo(condition)
                || then_body.iter().any(stmt_refs_pdo)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_pdo(cond) || body.iter().any(stmt_refs_pdo))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_pdo))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_pdo)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_pdo))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_pdo(condition) || body.iter().any(stmt_refs_pdo)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_pdo)
                || condition.as_ref().is_some_and(expr_refs_pdo)
                || update.as_deref().is_some_and(stmt_refs_pdo)
                || body.iter().any(stmt_refs_pdo)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_pdo(index) || expr_refs_pdo(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_pdo(target) || expr_refs_pdo(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_pdo(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_pdo(type_expr) || expr_refs_pdo(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_pdo(array) || body.iter().any(stmt_refs_pdo)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_pdo(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_pdo) || body.iter().any(stmt_refs_pdo)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_pdo))
        }
        StmtKind::Include { path, .. } => expr_refs_pdo(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_pdo),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_pdo)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_pdo)
                        || catch.body.iter().any(stmt_refs_pdo)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_pdo))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_pdo(params)
                || return_type.as_ref().is_some_and(type_refs_pdo)
                || body.iter().any(stmt_refs_pdo)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_pdo),
        StmtKind::ConstDecl { value, .. } => expr_refs_pdo(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_pdo(value),
        StmtKind::StaticVar { init, .. } => expr_refs_pdo(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_pdo)
                || implements.iter().any(name_is_pdo)
                || trait_uses.iter().any(trait_use_refs_pdo)
                || properties.iter().any(class_property_refs_pdo)
                || methods.iter().any(class_method_refs_pdo)
                || constants.iter().any(class_const_refs_pdo)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_pdo)
                || cases.iter().any(enum_case_refs_pdo)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_pdo),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_pdo)
                || properties.iter().any(class_property_refs_pdo)
                || methods.iter().any(class_method_refs_pdo)
                || constants.iter().any(class_const_refs_pdo)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_pdo)
                || properties.iter().any(class_property_refs_pdo)
                || methods.iter().any(class_method_refs_pdo)
                || constants.iter().any(class_const_refs_pdo)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_pdo(object) || expr_refs_pdo(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_pdo(receiver) || expr_refs_pdo(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_pdo(receiver) || expr_refs_pdo(index) || expr_refs_pdo(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_pdo(object) || expr_refs_pdo(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_pdo(object) || expr_refs_pdo(index) || expr_refs_pdo(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the PDO-usage AST walk: every class-name position is detected
    //! across PHP and `\`-qualified/case spellings, and non-class mentions (string
    //! literals, identifiers) are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_pdo` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse,
    /// before any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// `new PDO(...)` at statement level is detected.
    #[test]
    fn detects_new_object() {
        assert!(program_uses_pdo(&parse(
            r#"<?php $db = new PDO("sqlite::memory:");"#
        )));
    }

    /// A class constant access (`PDO::ERRMODE_EXCEPTION`) is detected.
    #[test]
    fn detects_class_constant() {
        assert!(program_uses_pdo(&parse(
            "<?php $mode = PDO::ERRMODE_EXCEPTION;"
        )));
    }

    /// A `catch (PDOException $e)` clause type is detected.
    #[test]
    fn detects_catch_type() {
        assert!(program_uses_pdo(&parse(
            "<?php try { something(); } catch (PDOException $e) { echo 1; }"
        )));
    }

    /// A parameter type hint (`function f(PDO $db)`) is detected.
    #[test]
    fn detects_parameter_type() {
        assert!(program_uses_pdo(&parse(
            "<?php function f(PDO $db) { return $db; }"
        )));
    }

    /// A union return type (`PDOStatement|bool`) is detected.
    #[test]
    fn detects_union_return_type() {
        assert!(program_uses_pdo(&parse(
            "<?php function f(): PDOStatement|bool { return false; }"
        )));
    }

    /// A typed property (`private PDO $db;`) is detected.
    #[test]
    fn detects_property_type() {
        assert!(program_uses_pdo(&parse(
            "<?php class C { private PDO $db; }"
        )));
    }

    /// A class extending PDO is detected through the `extends` clause.
    #[test]
    fn detects_extends() {
        assert!(program_uses_pdo(&parse("<?php class MyPdo extends PDO {}")));
    }

    /// An `instanceof PDO` check is detected.
    #[test]
    fn detects_instanceof() {
        assert!(program_uses_pdo(&parse(
            "<?php if ($x instanceof PDO) { echo 1; }"
        )));
    }

    /// A reference nested inside a call argument and a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_pdo(&parse(
            r#"<?php function run() { return helper(new PDO("sqlite::memory:")); }"#
        )));
    }

    /// A fully-qualified `\PDO` reference is detected (last segment matches).
    #[test]
    fn detects_fully_qualified() {
        assert!(program_uses_pdo(&parse(
            r#"<?php $db = new \PDO("sqlite::memory:");"#
        )));
    }

    /// Class-name matching is case-insensitive, as PHP class names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_uses_pdo(&parse(
            r#"<?php $db = new pdo("sqlite::memory:");"#
        )));
    }

    /// An aliased import (`use PDO as Db;`) is detected through the import name:
    /// the later `new Db()` carries only the alias, so without inspecting the
    /// import the program would be a false negative (no prelude injected).
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_pdo(&parse(
            r#"<?php use PDO as Db; $db = new Db("sqlite::memory:");"#
        )));
    }

    /// Mentions of "PDO" only inside string literals, variable names, and a
    /// function declaration name do not trigger detection — the key precision win
    /// over the previous `Debug`-string scan.
    #[test]
    fn ignores_non_class_mentions() {
        assert!(!program_uses_pdo(&parse(
            r#"<?php $pdoNote = "connect with PDO"; echo $pdoNote; function pdoHelper() { return 42; }"#
        )));
    }

    /// A program with no PDO mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_pdo(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
