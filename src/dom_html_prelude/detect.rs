//! Purpose:
//! Decides whether a parsed program references the Termwind-facing DOM HTML
//! surface — `DOMDocument`, `DOMNode`, `DOMElement`, `DOMText`, `DOMComment`,
//! `DOMCharacterData`, or `DOMNodeList` — so the HTML prelude is injected only
//! for programs that actually walk a loaded HTML tree.
//!
//! Called from:
//! - `crate::dom_html_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text and PHP class
//!   names are case-insensitive. A reference may be written `DOMDocument`,
//!   `\DOMDocument`, or `\Termwind\DOMDocument`. The walk matches the
//!   unqualified last segment case-insensitively.
//! - Class-name positions trigger injection: `new`, static receivers,
//!   `instanceof`, `catch`, `extends`/`implements`, type hints, trait uses, and
//!   `use` imports. There is no user-facing procedural `dom_*` function set.
//! - Capability probes (`class_exists('DOMDocument')`) are string literals and
//!   deliberately do NOT trigger injection — same rule as the PDO/mysqli
//!   preludes. A probe-only program honestly reports that the class is absent.
//! - Soundness over precision: a missed reference would drop the prelude and
//!   turn a valid program into an "undefined class" error, so the `match`es are
//!   exhaustive (no wildcard arm). Adding an AST node forces this file to be
//!   updated. False positives only inject declarations, which is harmless.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The OOP classes the Termwind HTML prelude declares. Last-segment,
/// case-insensitive match so `\DOMDocument` and `domdocument` both inject.
const DOM_HTML_CLASSES: &[&str] = &[
    "DOMDocument",
    "DOMNode",
    "DOMElement",
    "DOMText",
    "DOMComment",
    "DOMCharacterData",
    "DOMNodeList",
];

/// Returns whether any top-level statement references the Termwind DOM HTML
/// surface, so the prelude must be injected ahead of user code.
pub(super) fn program_uses_dom_html(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_dom)
}

/// Returns whether `name`'s unqualified last segment is a Termwind DOM class,
/// compared case-insensitively and tolerant of any namespace/leading-backslash
/// form (`DOMDocument`, `\DOMDocument`, `\Termwind\DOMElement`).
fn name_is_dom_class(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        DOM_HTML_CLASSES
            .iter()
            .any(|candidate| segment.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether a static receiver names a DOM class (`DOMDocument::...`).
/// `self`, `static`, and `parent` never resolve to a DOM class at this position.
fn receiver_refs_dom(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_dom_class(name))
}

/// Returns whether an `instanceof` target references a DOM class, recursing into
/// the operand when the target is a runtime expression.
fn instanceof_target_refs_dom(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_dom_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_dom(expr),
    }
}

/// Returns whether a first-class-callable target references a DOM class through
/// a static-method receiver or an instance-method object expression.
fn callable_target_refs_dom(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(_) => false,
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_dom(receiver),
        CallableTarget::Method { object, .. } => expr_refs_dom(object),
    }
}

/// Returns whether a type expression names a DOM class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_dom(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_dom_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_dom(inner)
        }
        TypeExpr::Named(name) => name_is_dom_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_dom)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the
/// hashing surface. Shared by function, method, and closure parameter lists.
fn params_ref_dom(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_dom)
            || default.as_ref().is_some_and(expr_refs_dom)
    })
}

/// Returns whether a `use Trait` clause names the hash class through its trait list
/// or any conflict-resolution adaptation.
fn trait_use_refs_dom(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_dom_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_dom_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_dom_class)
                    || instead_of.iter().any(name_is_dom_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the
/// hashing surface.
fn class_property_refs_dom(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_dom)
        || property.default.as_ref().is_some_and(expr_refs_dom)
}

/// Returns whether a method's parameters, return type, or body reference the
/// hashing surface.
fn class_method_refs_dom(method: &ClassMethod) -> bool {
    params_ref_dom(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_dom)
        || method.body.iter().any(stmt_refs_dom)
}

/// Returns whether a class constant's initializer references the hashing surface.
fn class_const_refs_dom(constant: &ClassConst) -> bool {
    expr_refs_dom(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the hashing
/// surface.
fn enum_case_refs_dom(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_dom)
}

/// Returns whether a `packed class` field's type references a DOM class.
/// DOM nodes are never a valid packed field type, but the field is walked for
/// completeness.
fn packed_field_refs_dom(field: &PackedField) -> bool {
    type_refs_dom(&field.type_expr)
}

/// Returns whether an expression references a DOM class at any class-name
/// position, recursing into every child expression and statement. The `match`
/// is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_dom(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no DOM reference.
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

        ExprKind::BinaryOp { left, right, .. } => expr_refs_dom(left) || expr_refs_dom(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_dom(value) || instanceof_target_refs_dom(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_dom(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_dom(value) || expr_refs_dom(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_dom(value) || expr_refs_dom(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_dom(target)
                || expr_refs_dom(value)
                || result_target.as_deref().is_some_and(expr_refs_dom)
                || prelude.iter().any(stmt_refs_dom)
        }
        ExprKind::FunctionCall { args, .. } => args.iter().any(expr_refs_dom),
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_dom),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_dom),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_dom(key) || expr_refs_dom(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_dom(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_dom) || expr_refs_dom(body)
                })
                || default.as_deref().is_some_and(expr_refs_dom)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_dom(array) || expr_refs_dom(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_dom(condition) || expr_refs_dom(then_expr) || expr_refs_dom(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_dom(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_dom(params)
                || return_type.as_ref().is_some_and(type_refs_dom)
                || body.iter().any(stmt_refs_dom)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_dom(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_dom(callee) || args.iter().any(expr_refs_dom)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_dom_class(class_name) || args.iter().any(expr_refs_dom)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_dom(name_expr) || args.iter().any(expr_refs_dom)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_dom(class_name)
                || name_is_dom_class(fallback_class)
                || name_is_dom_class(required_parent)
                || args.iter().any(expr_refs_dom)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_dom(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_dom(object) || expr_refs_dom(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_dom(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_dom(object) || args.iter().any(expr_refs_dom)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_dom(object) || expr_refs_dom(method) || args.iter().any(expr_refs_dom),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_dom(receiver) || args.iter().any(expr_refs_dom)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_dom(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_dom(element_type) || expr_refs_dom(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_dom(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_dom(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_dom(receiver) || args.iter().any(expr_refs_dom)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_dom)
                || value.as_deref().is_some_and(expr_refs_dom)
        }
        // Transient: the resolver expands this into the included file's statements
        // before hash detection runs, so it should never reach here. Recurse into the
        // path expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_dom(path),
    }
}

/// Returns whether a statement references the hashing surface at any function-call
/// or class-name position, recursing into nested statements, expressions, and class
/// members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_dom(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no hash-name position and no child expr/stmt.
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

        // An aliased import (`use DOMDocument as Doc;`) names the class only here;
        // the later `new Doc()` / `Doc $d` carries the alias, which the walk cannot
        // otherwise connect back — so skipping imports would be a false negative.
        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_dom_class(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_dom(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_dom(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_dom(condition)
                || then_body.iter().any(stmt_refs_dom)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_dom(cond) || body.iter().any(stmt_refs_dom))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_dom))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_dom)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_dom))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_dom(condition) || body.iter().any(stmt_refs_dom)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_dom)
                || condition.as_ref().is_some_and(expr_refs_dom)
                || update.as_deref().is_some_and(stmt_refs_dom)
                || body.iter().any(stmt_refs_dom)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_dom(index) || expr_refs_dom(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_dom(target) || expr_refs_dom(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_dom(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_dom(type_expr) || expr_refs_dom(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_dom(array) || body.iter().any(stmt_refs_dom)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_dom(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_dom) || body.iter().any(stmt_refs_dom)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_dom))
        }
        StmtKind::Include { path, .. } => expr_refs_dom(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_dom),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_dom)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_dom_class)
                        || catch.body.iter().any(stmt_refs_dom)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_dom))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_dom(params)
                || return_type.as_ref().is_some_and(type_refs_dom)
                || body.iter().any(stmt_refs_dom)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_dom),
        StmtKind::ConstDecl { value, .. } => expr_refs_dom(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_dom(value),
        StmtKind::StaticVar { init, .. } => expr_refs_dom(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_dom_class)
                || implements.iter().any(name_is_dom_class)
                || trait_uses.iter().any(trait_use_refs_dom)
                || properties.iter().any(class_property_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_dom)
                || cases.iter().any(enum_case_refs_dom)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_dom),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_dom_class)
                || properties.iter().any(class_property_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_dom)
                || properties.iter().any(class_property_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_dom(object) || expr_refs_dom(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_dom(receiver) || expr_refs_dom(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_dom(receiver) || expr_refs_dom(index) || expr_refs_dom(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_dom(object) || expr_refs_dom(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_dom(object) || expr_refs_dom(index) || expr_refs_dom(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the Termwind DOM HTML AST walk: every DOM class-name
    //! position is detected across `\`-qualified and mixed-case spellings, while
    //! string-literal probes and unrelated programs are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_dom_html` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse,
    /// before any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// `new DOMDocument` is the Termwind entry point and must inject the prelude.
    #[test]
    fn detects_new_dom_document() {
        assert!(program_uses_dom_html(&parse(
            "<?php $dom = new DOMDocument();"
        )));
    }

    /// A `DOMElement` parameter type hint is detected as a class reference.
    #[test]
    fn detects_class_type_hint() {
        assert!(program_uses_dom_html(&parse(
            "<?php function f(DOMElement $n): bool { return true; }"
        )));
    }

    /// An `instanceof DOMText` check is detected — Termwind's Node wrapper uses it.
    #[test]
    fn detects_instanceof() {
        assert!(program_uses_dom_html(&parse(
            "<?php if ($x instanceof DOMText) { echo 1; }"
        )));
        assert!(program_uses_dom_html(&parse(
            "<?php if ($x instanceof \\DOMComment) { echo 1; }"
        )));
    }

    /// A fully-qualified, differently-cased reference is detected.
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_dom_html(&parse(
            "<?php $dom = new \\domdocument();"
        )));
        assert!(program_uses_dom_html(&parse(
            "<?php function f(\\domnodelist $c): bool { return true; }"
        )));
    }

    /// An aliased import (`use DOMDocument as Doc;`) is detected through the import
    /// name: the later `Doc $d` carries only the alias.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_dom_html(&parse(
            "<?php use DOMDocument as Doc; function f(Doc $d): bool { return true; }"
        )));
    }

    /// A reference nested inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_dom_html(&parse(
            "<?php function run() { return new DOMDocument(); }"
        )));
    }

    /// Capability probes and string mentions do not trigger injection.
    #[test]
    fn ignores_string_probes_and_mentions() {
        assert!(!program_uses_dom_html(&parse(
            "<?php var_dump(class_exists('DOMDocument'));"
        )));
        assert!(!program_uses_dom_html(&parse(
            r#"<?php $note = "new DOMDocument first"; echo $note;"#
        )));
    }

    /// A program with no DOM mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_dom_html(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
