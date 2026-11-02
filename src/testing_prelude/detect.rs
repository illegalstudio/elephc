//! Purpose:
//! Decides whether a parsed program references the testing runtime: any name under the
//! `Elephc\Testing` namespace or the `PHPUnit\Framework\TestCase` alias, so the testing
//! prelude is injected only for programs that write tests.
//!
//! Called from:
//! - `crate::testing_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text and class names are
//!   case-insensitive. The walk matches the whole qualified spelling (leading `\` dropped):
//!   `Elephc\Testing\TestCase`, `Elephc\Testing` itself (`use Elephc\Testing;` then
//!   `Testing\TestCase`), and `PHPUnit\Framework\TestCase` or its `PHPUnit\Framework`
//!   namespace import. A bare `TestCase` is NOT a trigger: inside `namespace App\Tests` it
//!   names `App\Tests\TestCase`, and the import that makes it ours is matched instead.
//! - Only CLASS-NAME positions matter (new, static receivers, `instanceof`, `catch`,
//!   `extends`/`implements`, type hints, trait uses, `use` imports): the prelude declares no
//!   functions. Calls are still walked for the class names their arguments may carry.
//! - Soundness over precision: a missed reference would drop the prelude and turn a valid
//!   suite into an "undefined class" error, so the `match`es are exhaustive (no wildcard
//!   arm) and adding an AST node forces this file to be updated. `elephc test` forces the
//!   injection regardless, so a false negative here only affects hand-written entries.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    GenericDecl, InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation,
    TraitUse, TypeExpr, TypeParam,
};

/// The namespace the runtime lives in, compared case-insensitively against a name's full
/// spelling: a match on the namespace itself or on anything below it.
const TESTING_NAMESPACE: &str = "elephc\\testing";

/// The PHPUnit-compatible alias namespace and the one class it declares.
const PHPUNIT_NAMESPACE: &str = "phpunit\\framework";
const PHPUNIT_TEST_CASE: &str = "phpunit\\framework\\testcase";

/// Returns whether any top-level statement references the testing runtime, so the prelude
/// must be injected ahead of user code.
pub(super) fn program_uses_testing(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_testing)
}

/// Returns whether `name`, spelled in full with any leading `\` dropped and compared
/// case-insensitively, is the testing namespace, a name below it, or the PHPUnit alias.
fn name_is_testing_class(name: &Name) -> bool {
    let key = name.as_canonical().trim_start_matches('\\').to_ascii_lowercase();
    key == TESTING_NAMESPACE
        || key.starts_with(&format!("{TESTING_NAMESPACE}\\"))
        || key == PHPUNIT_NAMESPACE
        || key == PHPUNIT_TEST_CASE
}

/// Returns whether a static receiver names a testing class (`TestCase::class`, ...).
/// `self`, `static`, and `parent` never resolve to one at this position.
fn receiver_refs_testing(receiver: &StaticReceiver) -> bool {
    match receiver {
        StaticReceiver::Named(name) => name_is_testing_class(name),
        // `Box<TestCase>::of()`: the head and the type arguments are both class positions.
        StaticReceiver::Generic(type_expr) => type_refs_testing(type_expr),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => false,
    }
}

/// Returns whether an `instanceof` target references a testing class, recursing into
/// the operand when the target is a runtime expression.
fn instanceof_target_refs_testing(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_testing_class(name),
        InstanceOfTarget::Generic(class_type) => type_refs_testing(class_type),
        InstanceOfTarget::Expr(expr) => expr_refs_testing(expr),
    }
}

/// Returns whether a first-class-callable target references the testing runtime through a
/// static-method receiver or an instance-method object expression; a free function never
/// does, since the prelude declares none.
fn callable_target_refs_testing(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) => false,
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_testing(receiver),
        CallableTarget::Method { object, .. } => expr_refs_testing(object),
    }
}

/// Returns whether a type expression names a testing class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_testing(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(|name| name_is_testing_class(&crate::names::pointer_target_leaf_name(name))),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_testing(inner)
        }
        TypeExpr::AssocArray { key, value } => type_refs_testing(key) || type_refs_testing(value),
        // A declared signature can name one of these classes, and a program
        // that mentions it only through a callback still needs the prelude.
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_testing) || type_refs_testing(ret)
        }
        TypeExpr::Named(name) => name_is_testing_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_testing_class(name) || args.iter().any(type_refs_testing)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_testing)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the
/// testing runtime. Shared by function, method, and closure parameter lists.
fn params_ref_testing(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_testing)
            || default.as_ref().is_some_and(expr_refs_testing)
    })
}

/// Returns whether a `use Trait` clause names a testing class through its trait list
/// or any conflict-resolution adaptation.
fn trait_use_refs_testing(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_testing_class)
        || trait_use.type_args.iter().flatten().any(type_refs_testing)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_testing_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_testing_class)
                    || instead_of.iter().any(name_is_testing_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the
/// testing runtime.
fn class_property_refs_testing(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_testing)
        || property.default.as_ref().is_some_and(expr_refs_testing)
}

/// Returns whether a method's parameters (variadic element type included), type-parameter
/// bounds and defaults, return type, or body reference the testing runtime.
fn class_method_refs_testing(method: &ClassMethod) -> bool {
    params_ref_testing(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_testing)
        || type_params_ref_testing(&method.type_params)
        || method.return_type.as_ref().is_some_and(type_refs_testing)
        || method.body.iter().any(stmt_refs_testing)
}

/// Returns whether a class constant's declared type or initializer references the testing
/// runtime.
fn class_const_refs_testing(constant: &ClassConst) -> bool {
    constant.type_expr.as_ref().is_some_and(type_refs_testing)
        || expr_refs_testing(&constant.value)
}

/// Returns whether any type parameter's bound or default names a testing class.
fn type_params_ref_testing(type_params: &[TypeParam]) -> bool {
    type_params.iter().any(|param| {
        param.bound.as_ref().is_some_and(type_refs_testing)
            || param.default.as_ref().is_some_and(type_refs_testing)
    })
}

/// Returns whether a declaration's generic half names a testing class: its type-parameter
/// bounds and defaults, the type arguments on its parent, or those on its interfaces.
fn generics_ref_testing(generics: &Option<Box<GenericDecl>>) -> bool {
    generics.as_ref().is_some_and(|generics| {
        type_params_ref_testing(&generics.type_params)
            || generics.extends_args.iter().any(type_refs_testing)
            || generics
                .interface_args
                .iter()
                .any(|args| args.iter().any(type_refs_testing))
    })
}

/// Returns whether an enum case's backing-value expression references the testing
/// runtime.
fn enum_case_refs_testing(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_testing)
}

/// Returns whether a `packed class` field's type references a testing class. None is a
/// valid packed field type, but the field is walked for completeness.
fn packed_field_refs_testing(field: &PackedField) -> bool {
    type_refs_testing(&field.type_expr)
}

/// Returns whether an expression references the testing runtime at any class-name
/// position, recursing into every child expression and statement. The `match` is exhaustive so a newly added `ExprKind` cannot silently
/// bypass detection.
fn expr_refs_testing(expr: &Expr) -> bool {
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

        ExprKind::BinaryOp { left, right, .. } => expr_refs_testing(left) || expr_refs_testing(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_testing(value) || instanceof_target_refs_testing(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_testing(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_testing(value) || expr_refs_testing(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_testing(value) || expr_refs_testing(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_testing(target)
                || expr_refs_testing(value)
                || result_target.as_deref().is_some_and(expr_refs_testing)
                || prelude.iter().any(stmt_refs_testing)
        }
        // The prelude declares no functions, so a call is walked only for the class names
        // its arguments may carry.
        ExprKind::FunctionCall { args, .. } => args.iter().any(expr_refs_testing),
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_testing),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_testing),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_testing(key) || expr_refs_testing(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_refs_testing),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_testing(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_testing) || expr_refs_testing(body)
                })
                || default.as_deref().is_some_and(expr_refs_testing)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_testing(array) || expr_refs_testing(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_testing(condition) || expr_refs_testing(then_expr) || expr_refs_testing(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_testing(expr),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_testing(params)
                || variadic_type.as_ref().is_some_and(type_refs_testing)
                || return_type.as_ref().is_some_and(type_refs_testing)
                || body.iter().any(stmt_refs_testing)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_testing(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_testing(callee) || args.iter().any(expr_refs_testing)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_testing_class(class_name) || args.iter().any(expr_refs_testing)
        }
        ExprKind::NewGeneric { class_type, args } => {
            type_refs_testing(class_type) || args.iter().any(expr_refs_testing)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_testing(name_expr) || args.iter().any(expr_refs_testing)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_testing(class_name)
                || name_is_testing_class(fallback_class)
                || name_is_testing_class(required_parent)
                || args.iter().any(expr_refs_testing)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_testing(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_testing(object) || expr_refs_testing(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_testing(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_testing(object) || args.iter().any(expr_refs_testing)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_testing(object) || expr_refs_testing(method) || args.iter().any(expr_refs_testing),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_testing(receiver) || args.iter().any(expr_refs_testing)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_testing(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_testing(element_type) || expr_refs_testing(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_testing(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_testing(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_testing(receiver) || args.iter().any(expr_refs_testing)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_testing)
                || value.as_deref().is_some_and(expr_refs_testing)
        }
        // Transient: the resolver expands this into the included file's statements
        // before testing detection runs, so it should never reach here. Recurse into the
        // path expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_testing(path),
    }
}

/// Returns whether a statement references the testing runtime at any class-name
/// position, recursing into nested statements, expressions, and class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_testing(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // `$a =& Runner::make();`: the source is an expression and may name a class.
        StmtKind::RefAssign { source, .. } => expr_refs_testing(source),

        // Statements with no class-name position and no child expr/stmt.
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,

        // An import (`use Elephc\Testing\TestCase;`, `use PHPUnit\Framework\TestCase as
        // Base;`) names the class only here; the later `extends TestCase` carries the bare
        // or aliased spelling, which the walk cannot otherwise connect back, so skipping
        // imports would be a false negative.
        StmtKind::UseDecl { imports } => imports.iter().any(|item| name_is_testing_class(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_testing(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_testing(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_testing(condition)
                || then_body.iter().any(stmt_refs_testing)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_testing(cond) || body.iter().any(stmt_refs_testing))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_testing))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_testing)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_testing))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_testing(condition) || body.iter().any(stmt_refs_testing)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_testing)
                || condition.as_ref().is_some_and(expr_refs_testing)
                || update.as_deref().is_some_and(stmt_refs_testing)
                || body.iter().any(stmt_refs_testing)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_testing(index) || expr_refs_testing(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_testing(target) || expr_refs_testing(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_testing(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_testing(type_expr) || expr_refs_testing(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_testing(array) || body.iter().any(stmt_refs_testing)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_testing(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_testing) || body.iter().any(stmt_refs_testing)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_testing))
        }
        StmtKind::Include { path, .. } => expr_refs_testing(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_testing),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_testing)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_testing_class)
                        || catch
                            .exception_type_args
                            .iter()
                            .flatten()
                            .any(type_refs_testing)
                        || catch.body.iter().any(stmt_refs_testing)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_testing))
        }
        StmtKind::FunctionDecl {
            type_params,
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_testing(params)
                || variadic_type.as_ref().is_some_and(type_refs_testing)
                || type_params_ref_testing(type_params)
                || return_type.as_ref().is_some_and(type_refs_testing)
                || body.iter().any(stmt_refs_testing)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_testing),
        StmtKind::ConstDecl { value, .. } => expr_refs_testing(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_testing(value),
        StmtKind::StaticVar { init, .. } => expr_refs_testing(init),
        StmtKind::ClassDecl {
            generics,
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            generics_ref_testing(generics)
                || extends.as_ref().is_some_and(name_is_testing_class)
                || implements.iter().any(name_is_testing_class)
                || trait_uses.iter().any(trait_use_refs_testing)
                || properties.iter().any(class_property_refs_testing)
                || methods.iter().any(class_method_refs_testing)
                || constants.iter().any(class_const_refs_testing)
        }
        StmtKind::EnumDecl {
            generics,
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
            ..
        } => {
            generics_ref_testing(generics)
                || backing_type.as_ref().is_some_and(type_refs_testing)
                || cases.iter().any(enum_case_refs_testing)
                || implements.iter().any(name_is_testing_class)
                || trait_uses.iter().any(trait_use_refs_testing)
                || methods.iter().any(class_method_refs_testing)
                || constants.iter().any(class_const_refs_testing)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_testing),
        StmtKind::InterfaceDecl {
            generics,
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            generics_ref_testing(generics)
                || extends.iter().any(name_is_testing_class)
                || properties.iter().any(class_property_refs_testing)
                || methods.iter().any(class_method_refs_testing)
                || constants.iter().any(class_const_refs_testing)
        }
        StmtKind::TraitDecl {
            generics,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            generics_ref_testing(generics)
                || trait_uses.iter().any(trait_use_refs_testing)
                || properties.iter().any(class_property_refs_testing)
                || methods.iter().any(class_method_refs_testing)
                || constants.iter().any(class_const_refs_testing)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_testing(object) || expr_refs_testing(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_testing(receiver) || expr_refs_testing(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_testing(receiver) || expr_refs_testing(index) || expr_refs_testing(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_testing(object) || expr_refs_testing(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_testing(object) || expr_refs_testing(index) || expr_refs_testing(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the testing-runtime AST walk: every class-name position and import
    //! form of `Elephc\Testing\*` and `PHPUnit\Framework\TestCase` is detected across
    //! `\`-qualified and mixed-case spellings, while unrelated `TestCase` classes are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_testing` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before
    /// any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The common shape: an import of the runtime class followed by a bare `extends`.
    #[test]
    fn detects_imported_test_case() {
        assert!(program_uses_testing(&parse(
            "<?php namespace App\\Tests; use Elephc\\Testing\\TestCase; class T extends TestCase {}"
        )));
    }

    /// A fully qualified `extends`, in any case, is detected without an import.
    #[test]
    fn detects_fully_qualified_and_case_insensitive_extends() {
        assert!(program_uses_testing(&parse(
            "<?php class T extends \\elephc\\testing\\TESTCASE {}"
        )));
    }

    /// The PHPUnit alias is detected through its import and through its qualified form.
    #[test]
    fn detects_phpunit_alias() {
        assert!(program_uses_testing(&parse(
            "<?php use PHPUnit\\Framework\\TestCase; final class T extends TestCase {}"
        )));
        assert!(program_uses_testing(&parse(
            "<?php class T extends \\PHPUnit\\Framework\\TestCase {}"
        )));
    }

    /// A namespace import (`use Elephc\Testing;` then `Testing\Runner`) is detected through
    /// the import, which is the only position that carries the full spelling.
    #[test]
    fn detects_namespace_import() {
        assert!(program_uses_testing(&parse(
            "<?php use Elephc\\Testing; $r = new Testing\\Runner();"
        )));
    }

    /// `new`, a type hint, `instanceof` and `catch` positions are each detected.
    #[test]
    fn detects_other_class_positions() {
        assert!(program_uses_testing(&parse(
            "<?php $r = new \\Elephc\\Testing\\Runner();"
        )));
        assert!(program_uses_testing(&parse(
            "<?php function f(\\Elephc\\Testing\\TestCase $c): void {}"
        )));
        assert!(program_uses_testing(&parse(
            "<?php if ($x instanceof \\Elephc\\Testing\\TestCase) { echo 1; }"
        )));
        assert!(program_uses_testing(&parse(
            "<?php try { f(); } catch (\\Elephc\\Testing\\AssertionFailedError $e) { echo 1; }"
        )));
    }

    /// The positions an earlier walk skipped are detected: a variadic parameter's element
    /// type (function, method, closure), an enum's methods, generic type arguments on a
    /// parent, a class constant's declared type, and a by-reference assignment's source.
    #[test]
    fn detects_variadic_enum_generic_constant_and_ref_assign_positions() {
        for source in [
            "<?php function collect(\\Elephc\\Testing\\TestCase ...$cases): void {}",
            "<?php class K { public function m(\\Elephc\\Testing\\TestCase ...$cases): void {} }",
            "<?php $f = function (\\Elephc\\Testing\\TestCase ...$cases): void {};",
            "<?php enum Color { case Red; public function make(): \\Elephc\\Testing\\Runner { return new \\Elephc\\Testing\\Runner(); } }",
            "<?php class Wrapper<T> { } class Adapter extends Wrapper<\\Elephc\\Testing\\TestCase> {}",
            "<?php class Holder { public const ?\\Elephc\\Testing\\TestCase SAMPLE = null; }",
            "<?php function &make(): mixed { static $x = 1; return $x; } $alias =& \\Elephc\\Testing\\Runner::make();",
            "<?php class Box<T> { public static ?T $value = null; } Box<\\Elephc\\Testing\\AssertionFailedError>::$value = null;",
            "<?php trait Holder<T> { public ?T $value = null; } class Case_ { use Holder<\\Elephc\\Testing\\TestCase>; }",
            "<?php class Box<T> { } try { } catch (Box<\\Elephc\\Testing\\AssertionFailedError> $e) { }",
        ] {
            assert!(program_uses_testing(&parse(source)), "missed: {source}");
        }
    }

    /// A user's own `TestCase`, bare or in another namespace, is not a trigger.
    #[test]
    fn ignores_unrelated_test_case_classes() {
        assert!(!program_uses_testing(&parse(
            "<?php namespace App\\Tests; abstract class TestCase {} class T extends TestCase {}"
        )));
        assert!(!program_uses_testing(&parse(
            "<?php use App\\Testing\\TestCase; class T extends TestCase {}"
        )));
    }

    /// Mentions only inside string literals do not trigger detection.
    #[test]
    fn ignores_string_mentions() {
        assert!(!program_uses_testing(&parse(
            r#"<?php $note = "extends Elephc\\Testing\\TestCase"; echo $note;"#
        )));
    }

    /// A program with no testing mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_testing(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
