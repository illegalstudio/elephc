//! Purpose:
//! Resolves expressions and nested bodies owned by statements during include processing.
//! Rewrites statement children after declaration/include handling has chosen the statement shell.
//!
//! Called from:
//! - `crate::resolver::engine::resolve_stmts()`.
//!
//! Key details:
//! - Nested declarations and closures are resolved in isolated contexts to avoid leaking local traversal state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};

use super::discovery::FunctionVariantRegistry;
use super::engine::resolve_isolated;
use super::exprs::{resolve_expr, resolve_method_exprs, resolve_params, resolve_properties};
use super::state::ResolveState;

pub(super) fn resolve_stmt_exprs(
    stmt: Stmt,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Stmt, CompileError> {
    let span = stmt.span;
    let attributes = stmt.attributes.clone();
    let kind = match stmt.kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(resolve_isolated(
            stmts,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark { label },
        StmtKind::FunctionVariantGroup { name, variants } => {
            StmtKind::FunctionVariantGroup { name, variants }
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            StmtKind::FunctionVariantMark { name, variant }
        }
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: resolve_isolated(
                body,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
        },
        StmtKind::Echo(expr) => StmtKind::Echo(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?),
        StmtKind::Throw(expr) => StmtKind::Throw(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?),
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?),
        StmtKind::Return(expr) => StmtKind::Return(
            expr.map(|expr| {
                resolve_expr(
                    expr,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )
            })
                .transpose()?,
        ),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: resolve_expr(init, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state, function_variants)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            property,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state, function_variants)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state, function_variants)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state, function_variants)?,
            then_body,
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| {
                    Ok((
                        resolve_expr(condition, base_dir, declared_once, include_chain, state, function_variants)?,
                        body,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            else_body,
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state, function_variants)?,
            body,
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body,
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init
                .map(|stmt| {
                    resolve_stmt_exprs(
                        *stmt,
                        base_dir,
                        declared_once,
                        include_chain,
                        state,
                        function_variants,
                    )
                        .map(Box::new)
                })
                .transpose()?,
            condition: condition
                .map(|expr| {
                    resolve_expr(
                        expr,
                        base_dir,
                        declared_once,
                        include_chain,
                        state,
                        function_variants,
                    )
                })
                .transpose()?,
            update: update
                .map(|stmt| {
                    resolve_stmt_exprs(
                        *stmt,
                        base_dir,
                        declared_once,
                        include_chain,
                        state,
                        function_variants,
                    )
                        .map(Box::new)
                })
                .transpose()?,
            body,
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: resolve_expr(array, base_dir, declared_once, include_chain, state, function_variants)?,
            key_var,
            value_var,
            body,
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: resolve_expr(subject, base_dir, declared_once, include_chain, state, function_variants)?,
            cases: cases
                .into_iter()
                .map(|(values, body)| {
                    Ok((
                        values
                            .into_iter()
                            .map(|value| {
                                resolve_expr(
                                    value,
                                    base_dir,
                                    declared_once,
                                    include_chain,
                                    state,
                                    function_variants,
                                )
                            })
                            .collect::<Result<Vec<_>, CompileError>>()?,
                        body,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default,
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: resolve_params(
                params,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
            variadic,
            return_type,
            body,
        },
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        constants,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties: resolve_properties(
                properties,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
            methods: resolve_method_exprs(
                methods,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
        constants,
        },
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        constants,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: resolve_method_exprs(
                methods,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
        constants,
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
        } => StmtKind::TraitDecl {
            name,
            trait_uses,
            properties: resolve_properties(
                properties,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
            methods: resolve_method_exprs(
                methods,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
        constants,
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.value = case
                        .value
                        .map(|expr| {
                            resolve_expr(
                                expr,
                                base_dir,
                                declared_once,
                                include_chain,
                                state,
                                function_variants,
                            )
                        })
                        .transpose()?;
                    Ok(case)
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock { name, body },
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path,
            once,
            required,
        },
        other @ (StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }) => other,
    };
    Ok(Stmt::with_attributes(kind, span, attributes))
}
