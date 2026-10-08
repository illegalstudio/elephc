//! Purpose:
//! Decides whether a parsed program references the Termwind-facing DOM HTML
//! surface: `DOMDocument`, `DOMNode`, `DOMElement`, `DOMText`, `DOMComment`,
//! `DOMCharacterData`, or `DOMNodeList` — so the HTML prelude is injected only
//! for programs that actually walk a loaded HTML tree.
//!
//! Called from:
//! - `crate::dom_html_prelude::inject_if_used`.
//!
//! Key details:
//! - A detector-only name-resolution pass seeds distinguishable global fallback
//!   symbols. User declarations and imports retain their ownership, so only
//!   references bound to the fallback DOM surface trigger injection.
//! - Class-name positions trigger injection: `new`, static receivers,
//!   `instanceof`, `catch`, `extends`/`implements`, type hints, trait uses, and
//!   `use` imports. There is no user-facing procedural `dom_*` function set.
//! - Generic bounds, defaults, inherited arguments and callable/array member
//!   types are visited before monomorphization removes their syntactic wrappers.
//! - Capability probes (`class_exists('DOMDocument')`) are string literals and
//!   deliberately do NOT trigger injection — same rule as the PDO/mysqli
//!   preludes. A probe-only program honestly reports that the class is absent.
//! - Soundness over precision: a missed reference would drop the prelude and
//!   turn a valid program into an "undefined class" error, so the `match`es are
//!   exhaustive (no wildcard arm). Adding an AST node forces this file to be
//!   updated. Namespaced and user-owned DOM classes do not opt into this surface.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    GenericDecl, InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind,
    TraitAdaptation, TraitUse, TypeExpr, TypeParam,
};

/// Global class fallbacks offered by the prelude, behind user-owned declarations.
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
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(), &[], DOM_HTML_CLASSES,
    )
    .is_ok_and(|resolved| resolved.iter().any(stmt_refs_dom))
}

/// Returns whether resolution bound this name to a seeded global DOM class.
fn name_is_dom_class(name: &Name) -> bool {
    DOM_HTML_CLASSES.iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
    })
}

/// Returns whether a static receiver names a DOM class (`DOMDocument::...`).
/// `self`, `static`, and `parent` never resolve to a DOM class at this position.
fn receiver_refs_dom(receiver: &StaticReceiver) -> bool {
    match receiver {
        StaticReceiver::Named(name) => name_is_dom_class(name),
        StaticReceiver::Generic(class_type) => type_refs_dom(class_type),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => false,
    }
}

/// Returns whether an `instanceof` target references a DOM class, recursing into
/// the operand when the target is a runtime expression.
fn instanceof_target_refs_dom(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_dom_class(name),
        InstanceOfTarget::Generic(class_type) => type_refs_dom(class_type),
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
        TypeExpr::AssocArray { key, value } => type_refs_dom(key) || type_refs_dom(value),
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_dom) || type_refs_dom(ret)
        }
        TypeExpr::Named(name) => name_is_dom_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_dom_class(name) || args.iter().any(type_refs_dom)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_dom)
        }
    }
}

/// Finds DOM references in generic parameter bounds and default type arguments.
fn type_params_ref_dom(params: &[TypeParam]) -> bool {
    params.iter().any(|param| {
        param.bound.as_ref().is_some_and(type_refs_dom)
            || param.default.as_ref().is_some_and(type_refs_dom)
    })
}

/// Visits a declaration's own parameters and arguments passed to its inherited types.
fn generics_ref_dom(generics: Option<&GenericDecl>) -> bool {
    generics.is_some_and(|generics| {
        type_params_ref_dom(&generics.type_params)
            || generics.extends_args.iter().any(type_refs_dom)
            || generics.interface_args.iter().flatten().any(type_refs_dom)
    })
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
        || trait_use.type_args.iter().flatten().any(type_refs_dom)
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
    type_params_ref_dom(&method.type_params)
        || params_ref_dom(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_dom)
        || method.return_type.as_ref().is_some_and(type_refs_dom)
        || method.body.iter().any(stmt_refs_dom)
}

/// Returns whether a class constant's initializer references the hashing surface.
fn class_const_refs_dom(constant: &ClassConst) -> bool {
    constant.type_expr.as_ref().is_some_and(type_refs_dom) || expr_refs_dom(&constant.value)
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
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_refs_dom),
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
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_dom(params)
                || variadic_type.as_ref().is_some_and(type_refs_dom)
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
        ExprKind::NewGeneric { class_type, args } => {
            type_refs_dom(class_type) || args.iter().any(expr_refs_dom)
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
            type_params,
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            type_params_ref_dom(type_params)
                || params_ref_dom(params)
                || variadic_type.as_ref().is_some_and(type_refs_dom)
                || return_type.as_ref().is_some_and(type_refs_dom)
                || body.iter().any(stmt_refs_dom)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_dom),
        StmtKind::ConstDecl { value, .. } => expr_refs_dom(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_dom(value),
        StmtKind::StaticVar { init, .. } => expr_refs_dom(init),
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
            generics_ref_dom(generics.as_deref())
                || extends.as_ref().is_some_and(name_is_dom_class)
                || implements.iter().any(name_is_dom_class)
                || trait_uses.iter().any(trait_use_refs_dom)
                || properties.iter().any(class_property_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
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
            generics_ref_dom(generics.as_deref())
                || backing_type.as_ref().is_some_and(type_refs_dom)
                || cases.iter().any(enum_case_refs_dom)
                || implements.iter().any(name_is_dom_class)
                || trait_uses.iter().any(trait_use_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_dom),
        StmtKind::InterfaceDecl {
            generics,
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            generics_ref_dom(generics.as_deref())
                || extends.iter().any(name_is_dom_class)
                || properties.iter().any(class_property_refs_dom)
                || methods.iter().any(class_method_refs_dom)
                || constants.iter().any(class_const_refs_dom)
        }
        StmtKind::TraitDecl {
            generics,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            generics_ref_dom(generics.as_deref())
                || trait_uses.iter().any(trait_use_refs_dom)
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

    /// Mixed arrays visit keys, values, and spread sources introduced on main.
    #[test]
    fn detects_dom_in_mixed_array_entries() {
        for source in [
            "<?php $items = [new DOMDocument() => 1, 2];",
            "<?php $items = ['dom' => new DOMDocument(), 2];",
            "<?php $items = ['other' => 1, ...[new DOMDocument()]];",
        ] {
            assert!(program_uses_dom_html(&parse(source)), "{source}");
        }
        assert!(!program_uses_dom_html(&parse(
            "<?php $items = ['other' => 1, ...[2]];"
        )));
    }

    /// Generic and composite types retain DOM references until the prelude is injected.
    #[test]
    fn detects_dom_in_generic_type_shapes() {
        for source in [
            "<?php function f(array<string, DOMNode> $items): void {}",
            "<?php function f(callable(DOMNode): int $visit): void {}",
            "<?php function f(callable(int): DOMNode $visit): void {}",
            "<?php function f(Box<DOMNode> $box): void {}",
            "<?php $box = new Box<DOMNode>();",
            "<?php $box = new Box<int>(new DOMDocument());",
            "<?php if ($box instanceof Box<DOMNode>) { echo 1; }",
            "<?php Box<DOMNode>::run();",
            "<?php $run = Box<DOMNode>::run(...);",
        ] {
            assert!(program_uses_dom_html(&parse(source)), "{source}");
        }
    }

    /// Bound/default parameters and inherited class/interface/trait arguments are visited.
    #[test]
    fn detects_dom_in_generic_declarations() {
        for source in [
            "<?php function f<T: DOMNode>(T $node): void {}",
            "<?php function f<T = DOMNode>(): void {}",
            "<?php class Box<T: DOMNode> {}",
            "<?php class Box<T = DOMNode> {}",
            "<?php class Box { public function f<T: DOMNode>(T $node): void {} }",
            "<?php class Box extends Source<DOMNode> {}",
            "<?php class Box implements Source<DOMNode> {}",
            "<?php interface Box extends Source<DOMNode> {}",
            "<?php trait Box<T: DOMNode> {}",
            "<?php class Box { use Source<DOMNode>; }",
            "<?php enum Box implements Source<DOMNode> { case A; }",
            "<?php enum Box { public function f(DOMNode $node): void {} case A; }",
        ] {
            assert!(program_uses_dom_html(&parse(source)), "{source}");
        }
    }

    /// Generic syntax alone, user declarations and shadowing parameters do not opt into DOM.
    #[test]
    fn ignores_unrelated_and_user_owned_generic_types() {
        for source in [
            "<?php $box = new Box<int>();",
            "<?php function f(array<string, int> $items, callable(int): int $visit): void {}",
            "<?php class DOMDocument<T> {} $box = new DOMDocument<int>();",
            "<?php class DOMNode {} function f(Box<DOMNode> $box): void {}",
            "<?php class Box<DOMNode> { public DOMNode $value; }",
            "<?php namespace App; class DOMNode {} class Box extends Source<DOMNode> {}",
        ] {
            assert!(!program_uses_dom_html(&parse(source)), "{source}");
        }
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

    /// Every user-owned global DOM class keeps ownership under case-folded references and hints.
    #[test]
    fn ignores_user_owned_global_dom_classes() {
        for name in DOM_HTML_CLASSES {
            let source = format!(
                "<?php class {name} {{}} function f({name} $x): {name} {{ return $x; }} $x = new \\{}();",
                name.to_ascii_lowercase(),
            );
            assert!(!program_uses_dom_html(&parse(&source)), "{name}");
        }
    }

    /// Namespace and import binding must not confuse user classes with global fallbacks.
    #[test]
    fn preserves_user_namespace_and_import_ownership() {
        for source in [
            "<?php namespace App; class DOMDocument {} $x = new DOMDocument();",
            "<?php namespace App { class DOMDocument {} } namespace Client { use App\\DOMDocument as Doc; $x = new Doc(); }",
            "<?php namespace { class DOMDocument {} } namespace Client { use DOMDocument as Doc; $x = new Doc(); }",
            "<?php $x = new \\App\\DOMDocument();",
        ] {
            assert!(!program_uses_dom_html(&parse(source)), "{source}");
        }
    }

    /// A program with no DOM mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_dom_html(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
