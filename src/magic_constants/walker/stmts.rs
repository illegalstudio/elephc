//! Purpose:
//! Walks statement AST nodes for magic-constant substitution passes.
//! Rebuilds programs, control-flow bodies, declarations, and include expressions through pass hooks.
//!
//! Called from:
//! - `crate::magic_constants::walker::walk_program()`.
//!
//! Key details:
//! - Scope-bearing statements must enter and exit pass context in PHP lexical order.

use crate::names::Name;
use crate::parser::ast::{CatchClause, EnumCaseDecl, GenericDecl, Stmt, StmtKind};
use crate::span::Span;

use super::exprs::walk_expr;
use super::members::{walk_class_const, walk_class_method, walk_class_property};
use super::{walk_inherited, walk_inherited_list, walk_static_receiver, Pass};

/// Routes a class declaration's type parameters and inheritance clauses through the pass.
///
/// Returns the three fields together because they are one fact split across three places: the
/// names live on the declaration, their type arguments on its `GenericDecl`, and a substitution
/// that makes the last argument concrete has to update both AND drop the `GenericDecl` itself,
/// or the class would still look generic to `crate::generics::classes` and be stripped as a
/// template it no longer is.
fn walk_class_heritage<P: Pass>(
    generics: Option<Box<GenericDecl>>,
    extends: Option<Name>,
    implements: Vec<Name>,
    pass: &P,
    span: Span,
) -> (Option<Box<GenericDecl>>, Option<Name>, Vec<Name>) {
    let Some(generics) = generics else {
        return (None, extends, implements);
    };
    let GenericDecl {
        type_params,
        extends_args,
        interface_args,
    } = *generics;
    let type_params = walk_type_params(type_params, pass, span);
    let (extends, extends_args) = match extends {
        Some(parent) => {
            let (parent, args) = walk_inherited(parent, extends_args, pass, span);
            (Some(parent), args)
        }
        None => (None, Vec::new()),
    };
    let (implements, interface_args) = walk_inherited_list(implements, interface_args, pass, span);
    (
        GenericDecl::new(type_params, extends_args, interface_args),
        extends,
        implements,
    )
}

/// The interface form of [`walk_class_heritage`]: every inherited name is in `extends`.
fn walk_interface_heritage<P: Pass>(
    generics: Option<Box<GenericDecl>>,
    extends: Vec<Name>,
    pass: &P,
    span: Span,
) -> (Option<Box<GenericDecl>>, Vec<Name>) {
    let Some(generics) = generics else {
        return (None, extends);
    };
    let GenericDecl {
        type_params,
        extends_args: _,
        interface_args,
    } = *generics;
    let type_params = walk_type_params(type_params, pass, span);
    let (extends, interface_args) = walk_inherited_list(extends, interface_args, pass, span);
    (
        GenericDecl::new(type_params, Vec::new(), interface_args),
        extends,
    )
}

/// Routes each type parameter's bound and default through the pass.
///
/// A bound is a type (`<T : Box<int>>`) and so is a default (`<K = array<string>>`), so both are
/// substitution points; the parameter NAME is not a type and is left alone.
pub(super) fn walk_type_params<P: Pass>(
    type_params: Vec<crate::parser::ast::TypeParam>,
    pass: &P,
    span: Span,
) -> Vec<crate::parser::ast::TypeParam> {
    type_params
        .into_iter()
        .map(|param| crate::parser::ast::TypeParam {
            bound: param.bound.map(|ty| pass.transform_type(ty, span)),
            default: param.default.map(|ty| pass.transform_type(ty, span)),
            ..param
        })
        .collect()
}

/// Applies a magic-constant pass to a sequence of top-level statements.
///
/// Iterates through `stmts`, applies [`walk_stmt`] to each, and collects the results
/// into a new vector. This is the entry point for walking a program or block's statements.
pub(crate) fn walk_program<P: Pass>(stmts: Vec<Stmt>, pass: &mut P) -> Vec<Stmt> {
    stmts.into_iter().map(|s| walk_stmt(s, pass)).collect()
}

/// Applies a magic-constant pass to a single statement.
///
/// Dispatches on [`StmtKind`] variant, rebuilding the statement with pass hooks invoked
/// at the appropriate lexical scope boundaries (function, class, trait, namespace).
/// For expression-bearing variants, delegates to [`walk_expr`][super::exprs::walk_expr].
/// Statements with no expression children are returned unchanged.
pub(super) fn walk_stmt<P: Pass>(stmt: Stmt, pass: &mut P) -> Stmt {
    let span = stmt.span;
    let source_mode = stmt.source_mode;
    let strict_types = stmt.strict_types;
    let attributes = stmt.attributes.clone();
    let kind = match stmt.kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(walk_program(stmts, pass)),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark { label },
        StmtKind::FunctionVariantGroup { name, variants } => {
            StmtKind::FunctionVariantGroup { name, variants }
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            StmtKind::FunctionVariantMark { name, variant }
        }
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: walk_program(body, pass),
        },
        StmtKind::Echo(e) => StmtKind::Echo(walk_expr(e, pass)),
        StmtKind::Throw(e) => StmtKind::Throw(walk_expr(e, pass)),
        StmtKind::ExprStmt(e) => StmtKind::ExprStmt(walk_expr(e, pass)),
        StmtKind::Return(e) => StmtKind::Return(e.map(|x| walk_expr(x, pass))),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::RefAssign { target, source } => StmtKind::RefAssign { target, source },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr: pass.transform_type(type_expr, span),
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: walk_expr(init, pass),
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::NestedArrayAssign { target, value } => StmtKind::NestedArrayAssign {
            target: walk_expr(target, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver: walk_static_receiver(receiver, pass, span),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver: walk_static_receiver(receiver, pass, span),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver: walk_static_receiver(receiver, pass, span),
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: walk_expr(condition, pass),
            then_body: walk_program(then_body, pass),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(c, b)| (walk_expr(c, pass), walk_program(b, pass)))
                .collect(),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => StmtKind::IfDef {
            symbol,
            then_body: walk_program(then_body, pass),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: walk_expr(condition, pass),
            body: walk_program(body, pass),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: walk_program(body, pass),
            condition: walk_expr(condition, pass),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|s| Box::new(walk_stmt(*s, pass))),
            condition: condition.map(|e| walk_expr(e, pass)),
            update: update.map(|s| Box::new(walk_stmt(*s, pass))),
            body: walk_program(body, pass),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => StmtKind::Foreach {
            array: walk_expr(array, pass),
            key_var,
            value_var,
            value_by_ref,
            body: walk_program(body, pass),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: walk_expr(subject, pass),
            cases: cases
                .into_iter()
                .map(|(patterns, body)| {
                    (
                        patterns.into_iter().map(|e| walk_expr(e, pass)).collect(),
                        walk_program(body, pass),
                    )
                })
                .collect(),
            default: default.map(|b| walk_program(b, pass)),
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: walk_program(try_body, pass),
            catches: catches
                .into_iter()
                .map(|c| {
                    // A caught class is a class position like an implemented interface, and
                    // goes through the same helper — so a `catch (Err<int> $e)` collapses to an
                    // ordinary named catch the moment its type stops being generic.
                    let (exception_types, exception_type_args) =
                        walk_inherited_list(c.exception_types, c.exception_type_args, pass, span);
                    CatchClause {
                        exception_types,
                        exception_type_args,
                        variable: c.variable,
                        body: walk_program(c.body, pass),
                    }
                })
                .collect(),
            finally_body: finally_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::FunctionDecl {
            by_ref_return,
            name,
            type_params,
            params,
            param_attributes,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
        } => {
            pass.enter_function(&name);
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (
                        n,
                        t.map(|ty| pass.transform_type(ty, span)),
                        default.map(|d| walk_expr(d, pass)),
                        by_ref,
                    )
                })
                .collect();
            let new_body = walk_program(body, pass);
            // Transformed BEFORE leaving the scope. A struct literal's fields are evaluated
            // after the statements above it, so writing these inline below `leave_function`
            // handed them to the pass with the ENCLOSING scope active — the one type position
            // in a function that is not covered by its own declaration.
            let new_variadic_type = variadic_type.map(|ty| pass.transform_type(ty, span));
            let new_return_type = return_type.map(|ty| pass.transform_type(ty, span));
            pass.leave_function();
            StmtKind::FunctionDecl {
                by_ref_return,
                name,
                type_params,
                params: new_params,
                param_attributes,
                variadic,
                variadic_by_ref,
                variadic_type: new_variadic_type,
                return_type: new_return_type,
                body: new_body,
            }
        }
        StmtKind::ClassDecl {
            name,
            generics,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        constants,
        } => {
            pass.enter_class(&name);
            let (generics, extends, implements) =
                walk_class_heritage(generics, extends, implements, pass, span);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            let constants = constants
                .into_iter()
                .map(|c| walk_class_const(c, pass))
                .collect();
            pass.leave_class();
            StmtKind::ClassDecl {
                name,
                generics,
                extends,
                implements,
                is_abstract,
                is_final,
                is_readonly_class,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            constants,
            }
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
        } => {
            pass.enter_trait(&name);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            let constants = constants
                .into_iter()
                .map(|c| walk_class_const(c, pass))
                .collect();
            pass.leave_trait();
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            constants,
            }
        }
        StmtKind::InterfaceDecl {
            name,
            generics,
            extends,
            properties,
            methods,
        constants,
        } => {
            pass.enter_class(&name);
            // An interface has no single parent, so every inherited name — and every type
            // argument on one — lives in the `extends` list.
            let (generics, extends) = walk_interface_heritage(generics, extends, pass, span);
            let stmt = StmtKind::InterfaceDecl {
                name,
                generics,
                extends,
                properties: properties
                    .into_iter()
                    .map(|p| walk_class_property(p, pass))
                    .collect(),
                methods: methods
                    .into_iter()
                    .map(|m| walk_class_method(m, pass))
                    .collect(),
                constants: constants
                    .into_iter()
                    .map(|c| walk_class_const(c, pass))
                    .collect(),
            };
            pass.leave_class();
            stmt
        }
        StmtKind::EnumDecl {
            name,
            generics,
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
        } => {
            let cases = cases
                .into_iter()
                .map(|case| EnumCaseDecl {
                    name: case.name,
                    value: case.value.map(|e| walk_expr(e, pass)),
                    span: case.span,
                    attributes: case.attributes,
                })
                .collect();
            pass.enter_class(&name);
            let methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            let constants = constants
                .into_iter()
                .map(|c| walk_class_const(c, pass))
                .collect();
            pass.leave_class();
            // An enum's interface arguments go through the same rewrite a class's do:
            // `walk_inherited_list` turns `Labelled` + `[string]` into the instantiated name
            // `Labelled<string>` once the arguments are concrete, and drops the `GenericDecl`
            // with them. Transforming the argument TYPES alone left the enum implementing the
            // stripped template, which reaches codegen as missing interface metadata.
            let (implements, enum_interface_args) = walk_inherited_list(
                implements,
                generics.map(|g| g.interface_args).unwrap_or_default(),
                pass,
                span,
            );
            StmtKind::EnumDecl {
                name,
                generics: GenericDecl::new(Vec::new(), Vec::new(), enum_interface_args),
                backing_type: backing_type.map(|ty| pass.transform_type(ty, span)),
                cases,
                implements,
                trait_uses,
                methods,
                constants,
            }
        }
        StmtKind::NamespaceDecl { name } => {
            pass.enter_namespace_decl(&name);
            StmtKind::NamespaceDecl { name }
        }
        StmtKind::NamespaceBlock { name, body } => {
            pass.enter_namespace_block(&name);
            let new_body = walk_program(body, pass);
            pass.leave_namespace_block();
            StmtKind::NamespaceBlock {
                name,
                body: new_body,
            }
        }
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path: walk_expr(path, pass),
            once,
            required,
        },
        // Statements with no Expr children or only simple data:
        other @ (StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }) => other,
    };
    Stmt {
        kind,
        span,
        source_mode,
        strict_types,
        attributes,
    }
}
