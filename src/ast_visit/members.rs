//! Purpose:
//! Walks members and their nested AST positions.
//!
//! Called from:
//! - The shared AST visitor traversal.
//!
//! Key details:
//! - Recursion remains exhaustive and never prunes unreachable source.

use super::*;

/// Visits the shared class-like member lists (properties, methods, constants).
pub(super) fn walk_class_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    constants: &[ClassConst],
    span: Span,
    visitor: &mut impl AstVisitor,
) {
    for property in properties {
        walk_attribute_groups(&property.attributes, visitor);
        if let Some(type_expr) = &property.type_expr {
            walk_type(type_expr, property.span, visitor);
        }
        if let Some(default) = &property.default {
            walk_expr(default, visitor);
        }
    }
    for method in methods {
        if !method.type_params.is_empty() {
            extension(
                visitor,
                method.span,
                "generic methods are an elephc extension and are not valid PHP",
            );
        }
        walk_attribute_groups(&method.attributes, visitor);
        for groups in &method.param_attributes {
            walk_attribute_groups(groups, visitor);
        }
        walk_params(&method.params, method.span, visitor);
        if let Some(variadic_type) = &method.variadic_type {
            walk_type(variadic_type, method.span, visitor);
        }
        if let Some(return_type) = &method.return_type {
            walk_type(return_type, method.span, visitor);
        }
        walk_stmts(&method.body, visitor);
    }
    for constant in constants {
        walk_attribute_groups(&constant.attributes, visitor);
        if let Some(type_expr) = &constant.type_expr {
            walk_type(type_expr, constant.span, visitor);
        }
        walk_expr(&constant.value, visitor);
        let _ = span;
    }
}

/// Visits a function/method/closure parameter list: type annotations and defaults.
pub(super) fn walk_params(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    span: Span,
    visitor: &mut impl AstVisitor,
) {
    for (_, type_ann, default, _) in params {
        if let Some(type_expr) = type_ann {
            walk_type(type_expr, span, visitor);
        }
        if let Some(default) = default {
            walk_expr(default, visitor);
        }
    }
}

/// Visits a static receiver: only a generic one is an extension.
///
/// `self`, `static`, `parent` and a plain class name are all PHP. `Box<int>::` is not, and it
/// reaches the audit through five different expression forms, so the check lives here rather
/// than five times over.
pub(super) fn walk_static_receiver(
    receiver: &crate::parser::ast::StaticReceiver,
    span: Span,
    visitor: &mut impl AstVisitor,
) {
    if let crate::parser::ast::StaticReceiver::Generic(class_type) = receiver {
        walk_type(class_type, span, visitor);
    }
}

/// Visits the generic half of a class or interface declaration.
///
/// Two separate extensions live here and php-src parses neither: DECLARING type parameters
/// (`class Box<T>`), and passing type ARGUMENTS to something inherited
/// (`implements Repository<User>`). They are reported separately because they are separately
/// removable - a class can stop being generic and still implement a generic interface at a
/// concrete type, and the file is only valid PHP once both are gone.
pub(super) fn walk_generic_decl(
    generics: &Option<Box<crate::parser::ast::GenericDecl>>,
    name: &str,
    span: Span,
    visitor: &mut impl AstVisitor,
) {
    let Some(generics) = generics else { return };
    if !generics.type_params.is_empty() {
        extension(
            visitor,
            span,
            &format!(
                "Type parameters on '{}' are an elephc extension and are not valid PHP",
                name
            ),
        );
        // A bound and a default are types, and a file that removes the `<T>` still has to lose
        // whatever they named.
        for param in &generics.type_params {
            if let Some(bound) = &param.bound {
                walk_type(bound, span, visitor);
            }
            if let Some(default) = &param.default {
                walk_type(default, span, visitor);
            }
        }
    }
    let inherited = generics
        .extends_args
        .iter()
        .chain(generics.interface_args.iter().flatten());
    let mut reported = false;
    for argument in inherited {
        if !reported {
            extension(
                visitor,
                span,
                &format!(
                    "Type arguments on what '{}' inherits are an elephc extension and are not \
                     valid PHP",
                    name
                ),
            );
            reported = true;
        }
        walk_type(argument, span, visitor);
    }
}

/// Reports type arguments written on a used trait (`use Holder<int>;`), once per declaration,
/// and audits the arguments themselves as [`walk_generic_decl`] does for inherited ones.
pub(super) fn walk_trait_use_arguments(
    trait_uses: &[crate::parser::ast::TraitUse],
    name: &str,
    span: Span,
    visitor: &mut impl AstVisitor,
) {
    let mut reported = false;
    for argument in trait_uses.iter().flat_map(|trait_use| trait_use.type_args.iter().flatten()) {
        if !reported {
            extension(
                visitor,
                span,
                &format!(
                    "Type arguments on the traits '{}' uses are an elephc extension and are not \
                     valid PHP",
                    name
                ),
            );
            reported = true;
        }
        walk_type(argument, span, visitor);
    }
}

/// Visits a type annotation, reporting extension forms and recursing
/// through nullable, union, intersection, and array element types.
///
/// `TypeExpr::Array` reaches this pass only from a user-written `array<T>`: the audit runs
/// per physical file at load time, before includes are spliced and before any compiler
/// prelude is injected, so the internally constructed `Array` values (builtin schemas,
/// reflection literals, the `*_prelude` surfaces) never arrive here. A bare `array` parses
/// as `TypeExpr::Named("array")` and stays valid PHP.
pub(super) fn walk_type(type_expr: &TypeExpr, span: Span, visitor: &mut impl AstVisitor) {
    visitor.annotation(type_expr, span);
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable
        | TypeExpr::Named(_) => {}
        // `callable(int): string` is not PHP: php-src reads the parentheses after the keyword as
        // a syntax error. The halves are audited too, so a file that removes the signature still
        // has to lose whatever it named.
        TypeExpr::CallableSig { params, ret } => {
            extension(
                visitor,
                span,
                "A declared callable signature is an elephc extension and is not valid PHP",
            );
            for param in params {
                walk_type(param, span, visitor);
            }
            walk_type(ret, span, visitor);
        }
        // `Box<int>` is not PHP: php-src reads the `<` as a comparison operator and the
        // declaration fails to parse. Rejected here so the audit names the extension rather
        // than leaving the programmer with a parse error from another engine.
        TypeExpr::GenericClass { name, args } => {
            extension(
                visitor,
                span,
                &format!(
                    "Generic class type '{}<...>' is an elephc extension, not PHP",
                    name.as_str()
                ),
            );
            for arg in args {
                walk_type(arg, span, visitor);
            }
        }
        TypeExpr::Array(inner) => {
            extension(
                visitor,
                span,
                "`array<T>` type arguments are an elephc extension and are not valid PHP",
            );
            walk_type(inner, span, visitor);
        }
        TypeExpr::AssocArray { key, value } => {
            extension(
                visitor,
                span,
                "`array<K, V>` type arguments are an elephc extension and are not valid PHP",
            );
            walk_type(key, span, visitor);
            walk_type(value, span, visitor);
        }
        TypeExpr::Ptr(_) => {
            extension(
                visitor,
                span,
                "`ptr` types are an elephc extension and are not valid PHP",
            );
        }
        TypeExpr::Buffer(inner) => {
            extension(
                visitor,
                span,
                "`buffer<T>` types are an elephc extension and are not valid PHP",
            );
            walk_type(inner, span, visitor);
        }
        TypeExpr::Nullable(inner) => walk_type(inner, span, visitor),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            for member in members {
                walk_type(member, span, visitor);
            }
        }
    }
}
