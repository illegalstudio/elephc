//! Purpose:
//! Lowers the shared Chopper AST for supported eval fragments into EvalIR.
//!
//! Called from:
//! - `crate::parser::parse_fragment()` after source tokenization.
//!
//! Key details:
//! - The subset preserves dynamic variable names and operator evaluation in EvalIR.
//! - Unsupported token or AST shapes return an eval parse diagnostic.

use elephc_chopper::syntax::{
    names::{Name, NameKind},
    parser::{
        self,
        ast::{CatchClause, Expr, ExprKind, RuntimeExpr, Stmt, StmtKind},
    },
};

use crate::eval_ir::{EvalCatch, EvalExpr, EvalStmt};
use crate::lexer::TokenKind;
use crate::parser::SharedParseFailure;

pub(crate) mod attributes;
mod classes;
mod class_like;
mod constant_expressions;
mod members;
mod signatures;
mod closures;
mod expressions;
mod switch;
pub(crate) mod templates;
mod tokens;
mod scope;
pub(crate) mod writes;

use expressions::{lower_args, lower_bin_op, lower_expression, lower_unary};
use scope::Scope;
use signatures::lower_function;

/// Lowers a fragment in the lexical scope retained with a generic declaration.
pub(crate) fn lower_fragment_in_scope(
    tokens: &[TokenKind],
    lines: &[i64],
    phpdoc: &[bool],
    namespace: &str,
    imports: &crate::parser::state::NamespaceImports,
    strict_types: bool,
) -> Result<(Vec<EvalStmt>, bool, bool), SharedParseFailure> {
    let input = tokens::adapt_fragment(tokens, lines).ok_or(SharedParseFailure::Tokens)?;
    let (ast, strict_types, syntax) = parser::parse_runtime_fragment_in_scope(&input, namespace, strict_types)
        .map_err(SharedParseFailure::Syntax)?;
    let uses_native_generics = syntax.extensions.iter().any(|span| {
        !usize::try_from(span.col).ok().and_then(|col| col.checked_sub(1))
            .and_then(|index| phpdoc.get(index)).copied().unwrap_or(false)
    });
    let source = templates::TemplateSource {
        tokens, lines, phpdoc, syntax: &syntax, uses_native_generics: uses_native_generics.into(),
        invalid_jump: false.into(),
    };
    let statements = scope::lower_program(ast, strict_types, namespace, imports, &source).ok_or_else(|| {
        if source.invalid_jump.get() {
            SharedParseFailure::InvalidJump(crate::errors::EvalParseError::InvalidJump)
        } else {
            SharedParseFailure::Lowering
        }
    })?;
    Ok((statements, strict_types, source.uses_native_generics.get()))
}

/// Validates a lowered function-like body within its own label scope.
fn validate_body_jumps(scope: &Scope<'_>, body: &[EvalStmt]) -> Option<()> {
    if crate::parser::jumps::validate_jump_scope(body).is_err() {
        scope.source.invalid_jump.set(true);
        return None;
    }
    Some(())
}

/// Parses standalone specialization types through the same Chopper type grammar.
pub(crate) fn try_lower_type(tokens: &[crate::lexer::Token]) -> Option<elephc_generics::Type> {
    let lines = tokens.iter().map(crate::lexer::Token::line).collect::<Vec<_>>();
    let tokens = tokens.iter().map(|token| token.kind().clone()).collect::<Vec<_>>();
    let input = tokens::adapt_fragment(&tokens, &lines)?;
    parser::parse_runtime_type_with(&input, scope::lower_type).ok().flatten()
}

/// Reads declaration parameter names through Chopper for PHPDoc attachment.
pub(crate) fn type_parameter_names(
    tokens: impl IntoIterator<Item = crate::lexer::Token>,
) -> Option<Vec<String>> {
    let (tokens, lines): (Vec<_>, Vec<_>) = tokens.into_iter()
        .map(|token| (token.kind().clone(), token.line())).unzip();
    let input = tokens::adapt_fragment(&tokens, &lines)?;
    parser::parse_runtime_type_parameters_with(&input, |parameters| {
        parameters.into_iter().map(|parameter| parameter.name).collect()
    }).ok()
}

/// Lowers statements in source order and expands multi-argument echo sequences.
fn lower_statements(scope: &Scope<'_>, ast: impl IntoIterator<Item = Stmt>, statements: &mut Vec<EvalStmt>) -> Option<()> {
    for stmt in ast {
        let stmt = match stmt.kind {
            StmtKind::Echo(value) => EvalStmt::Echo(lower_expression(scope, value)?),
            StmtKind::Return(Some(value)) => EvalStmt::Return(Some(lower_expression(scope, value)?)),
            StmtKind::Return(None) => EvalStmt::Return(None),
            StmtKind::Include { path, once, required } => EvalStmt::Expr(EvalExpr::Include {
                path: Box::new(lower_expression(scope, path)?), once, required,
            }),
            StmtKind::Assign { name, value } => EvalStmt::StoreVar {
                name: local_name(name)?,
                value: lower_expression(scope, value)?,
            },
            StmtKind::TypedAssign { name, type_expr, value } => {
                scope.source.uses_native_generics.set(true);
                EvalStmt::TypedVar {
                    name: local_name(name)?,
                    declared: signatures::lower_optional_type(scope, Some(type_expr))??,
                    value: lower_expression(scope, value)?,
                }
            }
            StmtKind::ArrayAssign { array, index, value } => EvalStmt::ArraySetVar {
                name: local_name(array)?,
                index: lower_expression(scope, index)?,
                value: lower_expression(scope, value)?,
            },
            StmtKind::ArrayPush { array, value } => EvalStmt::ArrayAppendVar {
                name: local_name(array)?,
                value: lower_expression(scope, value)?,
            },
            StmtKind::PropertyAssign { object, property, value } => EvalStmt::PropertySet {
                object: lower_expression(scope, *object)?,
                property,
                value: lower_expression(scope, value)?,
            },
            StmtKind::StaticPropertyAssign { receiver, property, value } => EvalStmt::StaticPropertySet {
                class_name: classes::lower_receiver(scope, receiver)?,
                property,
                value: lower_expression(scope, value)?,
            },
            StmtKind::RefAssign { target, source } => {
                let ExprKind::Variable(source) = source.kind else { return None };
                EvalStmt::ReferenceAssign { target: local_name(target)?, source: local_name(source)? }
            }
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let mut else_branch = lower_branch(scope, else_body.unwrap_or_default())?;
                for (condition, body) in elseif_clauses.into_iter().rev() {
                    else_branch = vec![EvalStmt::If {
                        condition: lower_expression(scope, condition)?,
                        then_branch: lower_branch(scope, body)?,
                        else_branch,
                    }];
                }
                EvalStmt::If {
                    condition: lower_expression(scope, condition)?,
                    then_branch: lower_branch(scope, then_body)?,
                    else_branch,
                }
            }
            StmtKind::While { condition, body } => EvalStmt::While {
                condition: lower_expression(scope, condition)?,
                body: lower_branch(scope, body)?,
            },
            StmtKind::DoWhile { body, condition } => EvalStmt::DoWhile {
                body: lower_branch(scope, body)?,
                condition: lower_expression(scope, condition)?,
            },
            StmtKind::For { init, condition, update, body } => EvalStmt::For {
                init: lower_clause(scope, init)?,
                condition: match condition {
                    Some(value) => Some(lower_expression(scope, value)?),
                    None => None,
                },
                update: lower_clause(scope, update)?,
                body: lower_branch(scope, body)?,
            },
            StmtKind::Foreach { array, key_var, value_var, value_by_ref: false, body } => {
                EvalStmt::Foreach {
                    array: lower_expression(scope, array)?,
                    key_name: key_var,
                    value_name: value_var,
                    body: lower_branch(scope, body)?,
                }
            }
            StmtKind::Break(1) => EvalStmt::Break,
            StmtKind::Continue(1) => EvalStmt::Continue,
            StmtKind::ClassDecl { .. } | StmtKind::InterfaceDecl { .. } | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. } => {
                templates::lower_declaration(scope, stmt)?
            }
            StmtKind::Goto(label) => EvalStmt::Goto(label),
            StmtKind::GotoLabel(label) if !crate::parser::is_reserved_keyword(&label) => EvalStmt::Label(label),
            StmtKind::Global { vars } => EvalStmt::Global { vars },
            StmtKind::StaticVar { name, init } => EvalStmt::StaticVar {
                name: local_name(name)?,
                init: lower_expression(scope, init)?,
            },
            StmtKind::Throw(value) => EvalStmt::Throw(lower_expression(scope, value)?),
            StmtKind::Try { try_body, catches, finally_body } => EvalStmt::Try {
                body: lower_branch(scope, try_body)?,
                catches: catches.into_iter().map(|value| lower_catch(scope, value)).collect::<Option<_>>()?,
                finally_body: lower_branch(scope, finally_body.unwrap_or_default())?,
            },
            StmtKind::Switch { subject, cases, default } => EvalStmt::Switch {
                expr: lower_expression(scope, subject)?,
                cases: switch::lower_switch_cases(scope, cases, default)?,
            },
            StmtKind::FunctionDecl {
                name, type_params, params, param_attributes, variadic, variadic_by_ref,
                variadic_type, return_type, by_ref_return, body,
            } => {
                if type_params.is_empty() {
                    lower_function(
                        scope, name, params, stmt.attributes, param_attributes, variadic, variadic_by_ref,
                        variadic_type, return_type, by_ref_return, body, stmt.span,
                    )?
                } else {
                    let type_scope = scope.with_type_parameters(&type_params);
                    let qualified = scope.qualify(&name);
                    let prototype = lower_function(
                        &type_scope, name, params, stmt.attributes, param_attributes, variadic,
                        variadic_by_ref, variadic_type, return_type, by_ref_return, body, stmt.span,
                    )?;
                    EvalStmt::GenericDecl(Box::new(templates::retain_template(
                        &type_scope, type_params, qualified, stmt.span, prototype,
                    )?))
                }
            }
            StmtKind::ExprStmt(value) => match value.kind {
                ExprKind::Runtime(write) if matches!(write.as_ref(), RuntimeExpr::Write { .. }) => {
                    let RuntimeExpr::Write { target, operation } = *write else { unreachable!() };
                    writes::lower_write(scope, target, operation)?
                }
                ExprKind::Closure { .. } => return None,
                ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name) => EvalStmt::IncDecVar {
                    name: local_name(name)?, increment: true,
                },
                ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => EvalStmt::IncDecVar {
                    name: local_name(name)?, increment: false,
                },
                ExprKind::FunctionCall { name, args } if is_unset_construct(&name) => {
                    lower_unset(scope, args, statements)?;
                    continue;
                }
                _ => EvalStmt::Expr(lower_expression(scope, value)?),
            },
            StmtKind::Synthetic(group) => {
                lower_statements(scope, group, statements)?;
                continue;
            }
            _ => return None,
        };
        statements.push(stmt);
    }
    Some(())
}

/// Lowers a `const` statement, which PHP allows only at file or namespace level.
///
/// Nested statement lists never reach this: `lower_statements` refuses a `ConstDecl`, so a
/// `const` inside a function, method, closure or block makes the fragment invalid, as in PHP.
/// An initializer that is not a constant expression makes it invalid too, before any statement
/// of the fragment runs, as PHP's compile error does.
fn lower_const_decl(scope: &Scope<'_>, name: String, value: Expr) -> Option<EvalStmt> {
    let value = lower_expression(scope, value)?;
    constant_expressions::is_constant_expression(&value)
        .then(|| EvalStmt::ConstDecl { name: scope.qualify(&name), value })
}

/// Recognizes an unqualified call to the unset language construct.
fn is_unset_construct(name: &Name) -> bool {
    name.kind == NameKind::Unqualified && name.as_str().eq_ignore_ascii_case("unset")
}

/// Emits writes in operand order followed by Magician's cycle-collection safe point.
fn lower_unset(scope: &Scope<'_>, args: Vec<Expr>, statements: &mut Vec<EvalStmt>) -> Option<()> {
    if args.is_empty() {
        return None;
    }
    for target in args {
        statements.push(match lower_expression(scope, target)? {
            EvalExpr::LoadVar(name) => EvalStmt::UnsetVar { name },
            EvalExpr::ArrayGet { array, index } => EvalStmt::UnsetArrayElement { array: *array, index: *index },
            EvalExpr::PropertyGet { object, property } => EvalStmt::UnsetProperty { object: *object, property },
            EvalExpr::DynamicPropertyGet { object, property } => {
                EvalStmt::UnsetDynamicProperty { object: *object, property: *property }
            }
            EvalExpr::StaticPropertyGet { class_name, property } => {
                EvalStmt::UnsetStaticProperty { class_name, property }
            }
            EvalExpr::DynamicStaticPropertyGet { class_name, property } => EvalStmt::UnsetDynamicStaticProperty {
                class_name: *class_name, property,
            },
            EvalExpr::DynamicStaticPropertyNameGet { class_name, property } => EvalStmt::UnsetDynamicStaticPropertyName {
                class_name: *class_name, property: *property,
            },
            _ => return None,
        });
    }
    statements.push(EvalStmt::GcCollect);
    Some(())
}

/// Lowers one catch clause with ordinary class names in the global namespace.
fn lower_catch(scope: &Scope<'_>, catch: CatchClause) -> Option<EvalCatch> {
    Some(EvalCatch {
        class_names: templates::lower_class_arguments_list(scope, catch.exception_types, catch.exception_type_args)?,
        var_name: catch.variable,
        body: lower_branch(scope, catch.body)?,
    })
}

/// Resolves a class spelling whose interpretation needs no namespace or template metadata.
fn ordinary_class_name(scope: &Scope<'_>, name: Name) -> Option<String> {
    (name.kind != NameKind::Unqualified || !crate::parser::is_reserved_word(name.as_str()))
        .then(|| scope.class_name(&name))
}

/// Lowers one control-flow body to an EvalIR statement sequence.
fn lower_branch(scope: &Scope<'_>, ast: Vec<Stmt>) -> Option<Vec<EvalStmt>> {
    let mut statements = Vec::new();
    lower_statements(scope, ast, &mut statements)?;
    Some(statements)
}

/// Lowers a possibly empty for-loop clause in the eval grammar's single-effect form.
fn lower_clause(scope: &Scope<'_>, stmt: Option<Box<Stmt>>) -> Option<Vec<EvalStmt>> {
    if matches!(stmt.as_deref().map(|stmt| &stmt.kind), Some(StmtKind::Synthetic(stmts)) if stmts.len() > 1) {
        return None;
    }
    lower_branch(scope, stmt.into_iter().map(|stmt| *stmt).collect())
}

/// Keeps compiler-generated temporaries out of the caller's dynamic symbol table.
fn local_name(name: String) -> Option<String> {
    (!elephc_chopper::syntax::names::is_generated_local_name(&name)).then_some(name)
}
