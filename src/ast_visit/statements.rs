//! Purpose:
//! Walks statements and their nested AST positions.
//!
//! Called from:
//! - The shared AST visitor traversal.
//!
//! Key details:
//! - Recursion remains exhaustive and never prunes unreachable source.

use super::*;

/// Visits a statement list in order.
pub(super) fn walk_stmts(stmts: &[Stmt], visitor: &mut impl AstVisitor) {
    for stmt in stmts {
        walk_stmt(stmt, visitor);
    }
}

/// Visits PHP attribute groups: attribute arguments are ordinary expressions
/// (`#[Foo(buffer_new<int>(4))]` is a real call site once the attribute is
/// reflected), so they get the same expression audit as any other code.
pub(super) fn walk_attribute_groups(groups: &[AttributeGroup], visitor: &mut impl AstVisitor) {
    for group in groups {
        for attribute in &group.attributes {
            walk_exprs(&attribute.args, visitor);
        }
    }
}

/// Visits one statement, reporting extension syntax and recursing into
/// every nested expression, type annotation, statement body, and the
/// statement's own attribute arguments.
pub(super) fn walk_stmt(stmt: &Stmt, visitor: &mut impl AstVisitor) {
    visitor.statement(stmt);
    let span = stmt.span;
    walk_attribute_groups(&stmt.attributes, visitor);
    match &stmt.kind {
        StmtKind::Echo(expr) => walk_expr(expr, visitor),
        StmtKind::Assign { name: _, value } => walk_expr(value, visitor),
        StmtKind::RefAssign { target: _, source } => walk_expr(source, visitor),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            walk_expr(condition, visitor);
            walk_stmts(then_body, visitor);
            for (clause_condition, clause_body) in elseif_clauses {
                walk_expr(clause_condition, visitor);
                walk_stmts(clause_body, visitor);
            }
            if let Some(body) = else_body {
                walk_stmts(body, visitor);
            }
        }
        StmtKind::IfDef {
            symbol: _,
            then_body,
            else_body,
        } => {
            extension(
                visitor,
                span,
                "`ifdef` conditional compilation is an elephc extension and is not valid PHP",
            );
            walk_stmts(then_body, visitor);
            if let Some(body) = else_body {
                walk_stmts(body, visitor);
            }
        }
        StmtKind::While { condition, body } => {
            walk_expr(condition, visitor);
            walk_stmts(body, visitor);
        }
        StmtKind::DoWhile { body, condition } => {
            walk_stmts(body, visitor);
            walk_expr(condition, visitor);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                walk_stmt(init, visitor);
            }
            if let Some(condition) = condition {
                walk_expr(condition, visitor);
            }
            if let Some(update) = update {
                walk_stmt(update, visitor);
            }
            walk_stmts(body, visitor);
        }
        StmtKind::ArrayAssign {
            array: _,
            index,
            value,
        } => {
            walk_expr(index, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            walk_expr(target, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::ArrayPush { array: _, value } => walk_expr(value, visitor),
        StmtKind::TypedAssign {
            type_expr,
            name: _,
            value,
        } => {
            extension(
                visitor,
                span,
                "typed local variable declarations are an elephc extension and are not valid PHP",
            );
            walk_type(type_expr, span, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::Foreach {
            array,
            key_var: _,
            value_var: _,
            value_by_ref: _,
            body,
        } => {
            walk_expr(array, visitor);
            walk_stmts(body, visitor);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            walk_expr(subject, visitor);
            for (case_values, case_body) in cases {
                for value in case_values {
                    walk_expr(value, visitor);
                }
                walk_stmts(case_body, visitor);
            }
            if let Some(body) = default {
                walk_stmts(body, visitor);
            }
        }
        StmtKind::Include {
            path,
            once: _,
            required: _,
        } => walk_expr(path, visitor),
        StmtKind::IncludeOnceMark { label: _ } => {}
        StmtKind::IncludeOnceGuard { label: _, body } => walk_stmts(body, visitor),
        StmtKind::Throw(expr) => walk_expr(expr, visitor),
        StmtKind::Synthetic(body) => walk_stmts(body, visitor),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            walk_stmts(try_body, visitor);
            for CatchClause {
                exception_type_args,
                exception_types,
                variable: _,
                body,
            } in catches
            {
                // `catch (Err<int> $e)` is not PHP, and the caught class is the fifth place a
                // generic type can be written. The MENTION is what is rejected, not its
                // arguments - `int` is perfectly good PHP on its own - so it is rebuilt the way
                // the AST walker rebuilds it and audited whole, which also gives it the same
                // message every other generic mention produces.
                for (name, args) in exception_types.iter().zip(exception_type_args) {
                    if args.is_empty() {
                        continue;
                    }
                    walk_type(
                        &TypeExpr::GenericClass {
                            name: name.clone(),
                            args: args.clone(),
                        },
                        span,
                        visitor,
                    );
                }
                walk_stmts(body, visitor);
            }
            if let Some(body) = finally_body {
                walk_stmts(body, visitor);
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) => {}
        StmtKind::ExprStmt(expr) => walk_expr(expr, visitor),
        StmtKind::NamespaceDecl { name: _ } => {}
        StmtKind::NamespaceBlock { name: _, body } => walk_stmts(body, visitor),
        StmtKind::UseDecl { imports: _ } => {}
        StmtKind::FunctionDecl {
            name,
            type_params,
            params,
            param_attributes,
            variadic: _,
            variadic_by_ref: _,
            variadic_type,
            return_type,
            by_ref_return: _,
            body,
        } => {
            visitor.function_name(span, &Name::unqualified(name));
            if !type_params.is_empty() {
                extension(
                    visitor,
                    span,
                    "generic functions are an elephc extension and are not valid PHP",
                );
            }
            for groups in param_attributes {
                walk_attribute_groups(groups, visitor);
            }
            walk_params(params, span, visitor);
            if let Some(variadic_type) = variadic_type {
                walk_type(variadic_type, span, visitor);
            }
            if let Some(return_type) = return_type {
                walk_type(return_type, span, visitor);
            }
            walk_stmts(body, visitor);
        }
        StmtKind::FunctionVariantGroup {
            name: _,
            variants: _,
        } => {}
        StmtKind::FunctionVariantMark {
            name: _,
            variant: _,
        } => {}
        StmtKind::Return(value) => {
            if let Some(value) = value {
                walk_expr(value, visitor);
            }
        }
        StmtKind::ConstDecl { name: _, value } => walk_expr(value, visitor),
        StmtKind::ListUnpack { vars: _, value } => walk_expr(value, visitor),
        StmtKind::Global { vars: _ } => {}
        StmtKind::StaticVar { name: _, init } => walk_expr(init, visitor),
        StmtKind::ClassDecl {
            generics,
            name,
            extends: _,
            implements: _,
            is_abstract: _,
            is_final: _,
            is_readonly_class: _,
            trait_uses,
            properties,
            methods,
            constants,
        } => {
            walk_generic_decl(generics, name, span, visitor);
            walk_trait_use_arguments(trait_uses, name, span, visitor);
            walk_class_members(properties, methods, constants, span, visitor);
        }
        StmtKind::EnumDecl {
            name,
            generics,
            backing_type,
            cases,
            implements: _,
            trait_uses,
            methods,
            constants,
        } => {
            // Type arguments on what an enum implements are an elephc extension, exactly as they
            // are on a class; `walk_generic_decl` already words that and audits the arguments.
            walk_generic_decl(generics, name, span, visitor);
            walk_trait_use_arguments(trait_uses, name, span, visitor);
            if let Some(backing_type) = backing_type {
                walk_type(backing_type, span, visitor);
            }
            for EnumCaseDecl {
                name: _,
                value,
                span: case_span,
                attributes,
            } in cases
            {
                let _ = case_span;
                walk_attribute_groups(attributes, visitor);
                if let Some(value) = value {
                    walk_expr(value, visitor);
                }
            }
            walk_class_members(&[], methods, constants, span, visitor);
        }
        StmtKind::PackedClassDecl { name: _, fields: _ } => {
            extension(
                visitor,
                span,
                "`packed class` is an elephc extension and is not valid PHP",
            );
        }
        StmtKind::InterfaceDecl {
            generics,
            name,
            extends: _,
            properties,
            methods,
            constants,
        } => {
            walk_generic_decl(generics, name, span, visitor);
            walk_class_members(properties, methods, constants, span, visitor);
        }
        StmtKind::TraitDecl {
            name,
            generics,
            trait_uses,
            properties,
            methods,
            constants,
        } => {
            walk_generic_decl(generics, name, span, visitor);
            walk_trait_use_arguments(trait_uses, name, span, visitor);
            walk_class_members(properties, methods, constants, span, visitor);
        }
        StmtKind::PropertyAssign {
            object,
            property: _,
            value,
        } => {
            walk_expr(object, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::StaticPropertyAssign {
            receiver: _,
            property: _,
            value,
        } => walk_expr(value, visitor),
        StmtKind::StaticPropertyArrayPush {
            receiver: _,
            property: _,
            value,
        } => walk_expr(value, visitor),
        StmtKind::StaticPropertyArrayAssign {
            receiver: _,
            property: _,
            index,
            value,
        } => {
            walk_expr(index, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::PropertyArrayPush {
            object,
            property: _,
            value,
        } => {
            walk_expr(object, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::PropertyArrayAssign {
            object,
            property: _,
            index,
            value,
        } => {
            walk_expr(object, visitor);
            walk_expr(index, visitor);
            walk_expr(value, visitor);
        }
        StmtKind::ExternFunctionDecl {
            name: _,
            params: _,
            return_type: _,
            library: _,
        }
        | StmtKind::ExternClassDecl { name: _, fields: _ }
        | StmtKind::ExternGlobalDecl { name: _, c_type: _ } => {
            extension(
                visitor,
                span,
                "`extern` declarations are an elephc extension and are not valid PHP",
            );
        }
    }
}

