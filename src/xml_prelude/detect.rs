//! Purpose:
//! Decides whether a parsed program references PHP's `ext/xml` or `ext/xmlwriter`
//! surface — any of the `xml_*` / `xmlwriter_*` functions or the `XMLParser` /
//! `XMLWriter` classes — so the xml prelude is injected only for programs that use it.
//!
//! Called from:
//! - `crate::xml_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before the pipeline's name-resolution phase, so [`program_uses_xml`] first resolves
//!   a clone with the exact global xml function/class surface seeded as fallback symbols.
//!   This applies the authoritative PHP namespace/import rules without injecting the
//!   prelude: `namespace App; xml_parser_create()` falls back to the global function, while
//!   `\App\xml_parse()`, an imported user function, and a namespace-local declaration remain
//!   unrelated user symbols.
//! - TWO KINDS OF POSITION trigger injection: (a) a *function call* naming one of the 64
//!   functions (including `xml_parse_into_struct`, a registry builtin whose lowering calls
//!   the prelude's helpers); (b) a *class-name* position naming `XMLParser` or `XMLWriter`
//!   (new, static receivers, `instanceof`, `catch`, `extends`/`implements`, type hints,
//!   trait uses, `use` imports). `XML_*` constants are NOT a trigger: they resolve from the
//!   shared constant catalog whether or not the prelude is injected.
//! - THE FUNCTION SET IS EXACT (the shared catalog's `Area::Xml` names), never a prefix, so
//!   a user `xml_helper()` does not pull in the bridge.
//! - A CLASS NAME THE RESOLVER LEAVES BARE still counts. The resolver rewrites the class
//!   names it visits to the seeded global form, but it copies a variadic parameter's
//!   element type verbatim and resolves `X::class` receivers without the seeded table, so
//!   `XMLParser ...$parsers` and `XMLWriter::class` stay the bare spelling; matching that
//!   spelling (one segment, any case) keeps those positions sound. Inside a namespace the
//!   bare spelling may over-inject for a same-named local class — the accepted trade-off.
//! - Soundness over precision: a missed reference would drop the prelude and turn a valid
//!   program into an "undefined function/class" error, so the `match`es are exhaustive (no
//!   wildcard arm). Adding an AST node forces this file to be updated. `eval()` is not a
//!   trigger: opaque eval source reaches the surface through `--with-xml`, exactly like
//!   the other bridge-backed preludes.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The exact PHP functions of the xml surface: every `Area::Xml` contract in the shared
/// catalog. Whole-segment matches ensure user helpers with an `xml_` prefix do not silently
/// opt into the bridge.
fn xml_functions() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| {
                contract.area == elephc_builtin_contract::Area::Xml && !contract.internal
            })
            .map(|contract| contract.name)
            .collect()
    })
}

/// The `XMLParser` and `XMLWriter` classes, from the shared class catalog.
fn xml_classes() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<&'static str> = crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Xml,
        )
        .to_vec();
        names.extend_from_slice(crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Xmlwriter,
        ));
        names
    })
}

/// Returns whether any statement resolves to the global xml surface, so the prelude must
/// be injected ahead of user code. Resolution errors are left for the pipeline's normal
/// name-resolution phase; an invalid program does not need speculative bridge injection.
pub(super) fn program_uses_xml(program: &[Stmt]) -> bool {
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(),
        xml_functions(),
        xml_classes(),
    )
    .is_ok_and(|resolved| resolved.iter().any(stmt_refs_xml))
}

/// Returns whether a resolved name is one of the xml global functions. Function names are
/// case-insensitive, but a name with namespace segments is unrelated to the global surface.
fn name_is_xml_function(name: &Name) -> bool {
    xml_functions()
        .iter()
        .any(|candidate| crate::name_resolver::is_additional_global_symbol(name, candidate))
}

/// Returns whether a resolved name is `XMLParser` or `XMLWriter`: the seeded global the
/// resolver bound it to, or the bare single-segment spelling at a class-name position the
/// resolver leaves as written (a variadic parameter's element type, a `::class` or scoped
/// constant receiver). Class names are case-insensitive, but a name with namespace segments
/// is unrelated to the global surface.
fn name_is_xml_class(name: &Name) -> bool {
    xml_classes().iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
            || matches!(name.parts.as_slice(), [bare] if bare.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether a static receiver names an xml class (`XMLWriter::toMemory()`).
/// `self`, `static`, and `parent` never resolve to a xml class at this position.
fn receiver_refs_xml(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_xml_class(name))
}

/// Returns whether an `instanceof` target references a xml class, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_xml(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_xml_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_xml(expr),
    }
}

/// Returns whether a first-class-callable target references the xml surface: a free
/// function (`xml_exec(...)`), a static-method receiver, or an instance-method object
/// expression.
fn callable_target_refs_xml(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_xml_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_xml(receiver),
        CallableTarget::Method { object, .. } => expr_refs_xml(object),
    }
}

/// Returns whether a type expression names a xml class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_xml(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_xml_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_xml(inner)
        }
        TypeExpr::Named(name) => name_is_xml_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_xml)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the xml
/// surface. Shared by function, method, and closure parameter lists.
fn params_ref_xml(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_xml)
            || default.as_ref().is_some_and(expr_refs_xml)
    })
}

/// Returns whether a `use Trait` clause names a xml class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_xml(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_xml_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_xml_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_xml_class)
                    || instead_of.iter().any(name_is_xml_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the xml
/// surface.
fn class_property_refs_xml(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_xml)
        || property.default.as_ref().is_some_and(expr_refs_xml)
}

/// Returns whether a method's parameters, variadic element type, return type, or body
/// reference the xml surface.
fn class_method_refs_xml(method: &ClassMethod) -> bool {
    params_ref_xml(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_xml)
        || method.return_type.as_ref().is_some_and(type_refs_xml)
        || method.body.iter().any(stmt_refs_xml)
}

/// Returns whether a class constant's initializer references the xml surface.
fn class_const_refs_xml(constant: &ClassConst) -> bool {
    expr_refs_xml(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the xml surface.
fn enum_case_refs_xml(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_xml)
}

/// Returns whether a `packed class` field's type references a xml class. A xml class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_xml(field: &PackedField) -> bool {
    type_refs_xml(&field.type_expr)
}

/// Returns whether an expression references the xml surface at any function-call,
/// class-name, or constant position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_xml(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no xml reference.
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

        // `XML_*` constants resolve from the shared constant catalog with or without the
        // prelude, so a bare constant is not a trigger; the call or class-name position
        // that needs the prelude is caught elsewhere.
        ExprKind::ConstRef(_) => false,

        ExprKind::BinaryOp { left, right, .. } => expr_refs_xml(left) || expr_refs_xml(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_xml(value) || instanceof_target_refs_xml(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_xml(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_xml(value) || expr_refs_xml(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_xml(value) || expr_refs_xml(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_xml(target)
                || expr_refs_xml(value)
                || result_target.as_deref().is_some_and(expr_refs_xml)
                || prelude.iter().any(stmt_refs_xml)
        }
        // A free-function call is the dominant xml position: `xml_parser_create()` and
        // `xmlwriter_open_memory()` mint the objects, so a program may parse or write
        // without ever naming a class.
        ExprKind::FunctionCall { name, args } => {
            name_is_xml_function(name) || args.iter().any(expr_refs_xml)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_xml),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_xml),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_xml(key) || expr_refs_xml(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_xml(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_xml) || expr_refs_xml(body)
                })
                || default.as_deref().is_some_and(expr_refs_xml)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_xml(array) || expr_refs_xml(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_xml(condition) || expr_refs_xml(then_expr) || expr_refs_xml(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_xml(expr),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_xml(params)
                || variadic_type.as_ref().is_some_and(type_refs_xml)
                || return_type.as_ref().is_some_and(type_refs_xml)
                || body.iter().any(stmt_refs_xml)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_xml(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_xml(callee) || args.iter().any(expr_refs_xml)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_xml_class(class_name) || args.iter().any(expr_refs_xml)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_xml(name_expr) || args.iter().any(expr_refs_xml)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_xml(class_name)
                || name_is_xml_class(fallback_class)
                || name_is_xml_class(required_parent)
                || args.iter().any(expr_refs_xml)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_xml(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_xml(object) || expr_refs_xml(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_xml(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_xml(object) || args.iter().any(expr_refs_xml)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_xml(object) || expr_refs_xml(method) || args.iter().any(expr_refs_xml),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_xml(receiver) || args.iter().any(expr_refs_xml)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_xml(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_xml(element_type) || expr_refs_xml(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_xml(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_xml(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_xml(receiver) || args.iter().any(expr_refs_xml)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_xml)
                || value.as_deref().is_some_and(expr_refs_xml)
        }
        // Transient: the resolver expands this into the included file's statements before
        // xml detection runs, so it should never reach here. Recurse into the path
        // expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_xml(path),
    }
}

/// Returns whether a statement references the xml surface at any function-call,
/// class-name, or constant position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_xml(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no xml-name position and no child expr/stmt.
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

        // An aliased import (`use XMLWriter as W;`) names the class only here; the later
        // `new W()` / `W $w` carries the alias, which the walk cannot otherwise connect
        // back — so skipping imports would be a false negative. Function imports
        // (`use function xml_parse as p;`) land in the same list and are caught by the
        // same check, which is why both name tests are applied.
        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_xml_class(&item.name) || name_is_xml_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_xml(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_xml(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_xml(condition)
                || then_body.iter().any(stmt_refs_xml)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_xml(cond) || body.iter().any(stmt_refs_xml))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_xml))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_xml)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_xml))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_xml(condition) || body.iter().any(stmt_refs_xml)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_xml)
                || condition.as_ref().is_some_and(expr_refs_xml)
                || update.as_deref().is_some_and(stmt_refs_xml)
                || body.iter().any(stmt_refs_xml)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_xml(index) || expr_refs_xml(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_xml(target) || expr_refs_xml(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_xml(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_xml(type_expr) || expr_refs_xml(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_xml(array) || body.iter().any(stmt_refs_xml)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_xml(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_xml) || body.iter().any(stmt_refs_xml)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_xml))
        }
        StmtKind::Include { path, .. } => expr_refs_xml(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_xml),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_xml)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_xml_class)
                        || catch.body.iter().any(stmt_refs_xml)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_xml))
        }
        StmtKind::FunctionDecl {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_xml(params)
                || variadic_type.as_ref().is_some_and(type_refs_xml)
                || return_type.as_ref().is_some_and(type_refs_xml)
                || body.iter().any(stmt_refs_xml)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_xml),
        StmtKind::ConstDecl { value, .. } => expr_refs_xml(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_xml(value),
        StmtKind::StaticVar { init, .. } => expr_refs_xml(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_xml_class)
                || implements.iter().any(name_is_xml_class)
                || trait_uses.iter().any(trait_use_refs_xml)
                || properties.iter().any(class_property_refs_xml)
                || methods.iter().any(class_method_refs_xml)
                || constants.iter().any(class_const_refs_xml)
        }
        // An enum carries the same member kinds as a class (methods, constants, trait
        // uses, implemented interfaces) on top of its backing type and cases; a program
        // whose only xml call sits in an enum method must still inject the prelude.
        StmtKind::EnumDecl {
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_xml)
                || cases.iter().any(enum_case_refs_xml)
                || implements.iter().any(name_is_xml_class)
                || trait_uses.iter().any(trait_use_refs_xml)
                || methods.iter().any(class_method_refs_xml)
                || constants.iter().any(class_const_refs_xml)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_xml),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_xml_class)
                || properties.iter().any(class_property_refs_xml)
                || methods.iter().any(class_method_refs_xml)
                || constants.iter().any(class_const_refs_xml)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_xml)
                || properties.iter().any(class_property_refs_xml)
                || methods.iter().any(class_method_refs_xml)
                || constants.iter().any(class_const_refs_xml)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_xml(object) || expr_refs_xml(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_xml(receiver) || expr_refs_xml(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_xml(receiver) || expr_refs_xml(index) || expr_refs_xml(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_xml(object) || expr_refs_xml(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_xml(object) || expr_refs_xml(index) || expr_refs_xml(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the xml AST walk: every `xml_*` / `xmlwriter_*` call and every
    //! `XMLParser` / `XMLWriter` class-name position is detected across `\`-qualified and
    //! mixed-case spellings, while unrelated `xml`-shaped user names and non-call mentions
    //! are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_xml` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before
    /// any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The catalog names both modules' functions and nothing else.
    #[test]
    fn the_function_set_is_the_catalog() {
        assert_eq!(xml_functions().len(), 64);
        assert!(xml_functions().contains(&"xml_parse_into_struct"));
        assert!(xml_functions().contains(&"xmlwriter_flush"));
        assert_eq!(xml_classes(), &["XMLParser", "XMLWriter"]);
    }

    /// Every enumerated function is detected on its own, so a helper that only consumes
    /// a parser passed in still pulls the prelude.
    #[test]
    fn detects_every_enumerated_xml_function() {
        for name in xml_functions() {
            let source = format!("<?php function f($a, $b, $c) {{ return {name}($a, $b, $c); }}");
            assert!(
                program_uses_xml(&parse(&source)),
                "{name} must trigger xml prelude injection"
            );
        }
    }

    /// Both classes are detected in a parameter type hint, even when the body never
    /// calls an xml function.
    #[test]
    fn detects_every_class_type_hint() {
        for class in xml_classes() {
            let source = format!("<?php function f({class} $c): bool {{ return true; }}");
            assert!(
                program_uses_xml(&parse(&source)),
                "{class} must trigger xml prelude injection"
            );
        }
    }

    /// `new XMLWriter()`, the static constructors and `instanceof` are class-name positions.
    #[test]
    fn detects_class_name_positions() {
        assert!(program_uses_xml(&parse("<?php $w = new XMLWriter();")));
        assert!(program_uses_xml(&parse("<?php $w = XMLWriter::toMemory();")));
        assert!(program_uses_xml(&parse(
            "<?php if ($x instanceof XMLParser) { echo 1; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php class W extends XMLWriter {}"
        )));
    }

    /// Fully-qualified global functions/classes remain case-insensitive.
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_xml(&parse("<?php $p = \\XML_PARSER_CREATE();")));
        assert!(program_uses_xml(&parse(
            "<?php function f(\\xmlwriter $w): bool { return true; }"
        )));
    }

    /// An aliased import is detected through the import name.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_xml(&parse(
            "<?php use XMLWriter as W; function f(W $w): bool { return true; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php use function xml_parse as p; $r = p($x, $d);"
        )));
    }

    /// An enum method body is a call position like a class method's.
    #[test]
    fn detects_enum_methods() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; public function parse(string $d): int { $p = xml_parser_create(); return xml_parse($p, $d, true); } }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; public static function make(): XMLWriter { return new XMLWriter(); } }"
        )));
    }

    /// An enum constant initializer naming a xml class is a class-name position.
    #[test]
    fn detects_enum_constants() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; const WRITER = XMLWriter::class; }"
        )));
    }

    /// An enum's implemented interfaces and trait uses are walked like a class's.
    #[test]
    fn detects_enum_implements_and_trait_uses() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt implements XMLParser { case Xml; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { use XMLWriter; case Xml; }"
        )));
    }

    /// An enum without any xml reference in its members does not inject.
    #[test]
    fn ignores_unrelated_enum() {
        assert!(!program_uses_xml(&parse(
            "<?php enum Fmt: string { case Xml = 'xml'; const LABEL = 'x'; public function label(): string { return $this->value; } }"
        )));
    }

    /// A typed variadic parameter is a class-name position on a function, a method and a
    /// closure alike; a scalar element type is not.
    #[test]
    fn detects_variadic_parameter_types() {
        assert!(program_uses_xml(&parse(
            "<?php function f(XMLParser ...$parsers): int { return count($parsers); }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php class C { public function m(XMLWriter ...$writers): int { return count($writers); } }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php $f = function (XMLParser ...$parsers): int { return count($parsers); };"
        )));
        assert!(!program_uses_xml(&parse(
            "<?php function f(int ...$xs): int { return count($xs); } $g = function (string ...$s): int { return count($s); };"
        )));
    }

    /// An unqualified call inside a namespace falls back to the global function when no
    /// local declaration or function import claims the name.
    #[test]
    fn detects_namespaced_function_fallback() {
        assert!(program_uses_xml(&parse(
            "<?php namespace App; $p = xml_parser_create();"
        )));
    }

    /// Fully-qualified user symbols that merely end in xml names stay in their namespace.
    #[test]
    fn ignores_fully_qualified_user_symbols() {
        assert!(!program_uses_xml(&parse(
            "<?php $value = \\Some\\xml_parse($handle, $data);"
        )));
        assert!(!program_uses_xml(&parse(
            "<?php function f(\\Acme\\XMLWriter $w): bool { return true; }"
        )));
    }

    /// A user helper with an `xml_` prefix, a string mentioning a function, and an `XML_*`
    /// constant on its own do not inject the prelude.
    #[test]
    fn ignores_unrelated_programs() {
        assert!(!program_uses_xml(&parse(
            "<?php function xml_helper($x) { return $x; } echo xml_helper(1);"
        )));
        assert!(!program_uses_xml(&parse(
            r#"<?php $s = "xml_parser_create"; echo $s;"#
        )));
        assert!(!program_uses_xml(&parse("<?php echo XML_ERROR_SYNTAX;")));
    }
}
