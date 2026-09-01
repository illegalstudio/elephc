//! Purpose:
//! Dispatches PHP statement parsing and exposes shared statement parser helpers.
//! Owns statement recovery, blocks, names, semicolons, and top-level syntax routing.
//!
//! Called from:
//! - `crate::parser::parse()` and nested block/control parsers.
//!
//! Key details:
//! - Recovery stops at PHP statement boundaries so follow-up diagnostics remain useful.

mod assign;
mod blocks;
mod declare;
mod ffi;
mod goto_unsupported;
mod names;
mod namespace_use;
mod oop;
/// Parameter parsing helpers for function declarations, including typed parameters and return types.
pub(crate) mod params;
mod simple;

pub(crate) use simple::parse_void_expr_inline;
mod recovery;

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{AttributeGroup, ExprKind, Stmt, StmtKind};
use crate::parser::control;
use crate::parser::expr::parse_expr;
use crate::span::Span;

pub use ffi::parse_extern_stmts;
pub use blocks::{parse_block, parse_body};
pub(crate) use oop::parse_anonymous_class;
pub(crate) use params::{looks_like_typed_param, parse_type_expr};
pub(crate) use assign::{can_replay_assignment_target, lower_postfix_incdec_assignment};
pub(crate) use blocks::{expect_semicolon, expect_token};
pub(crate) use names::{name_part_from_token, name_starts_at, parse_name, parse_unqualified_name};
pub(crate) use assign::{parse_destructuring_pattern_unpack, starts_destructuring_pattern};
pub(crate) use recovery::recover_to_statement_boundary;

/// Parses a single PHP statement, including optional PHP 8 attribute groups.
pub fn parse_stmt(tokens: &[SpannedToken], pos: &mut usize) -> Result<Stmt, CompileError> {
    // PHP attribute groups (`#[...]`) may decorate any statement-level
    // declaration. We capture them here and attach the result to the parsed
    // statement; non-declaration kinds reject non-empty attribute lists below.
    let attributes = crate::parser::parse_attribute_lists(tokens, pos)?;

    if *pos >= tokens.len() {
        let span = tokens
            .last()
            .map(|(_, metadata)| metadata.span)
            .unwrap_or(Span::dummy());
        return Err(CompileError::new(span, "Unexpected end of input after attributes"));
    }
    let span = tokens[*pos].1.span;

    let stmt = parse_stmt_dispatch(tokens, pos, span)?;
    attach_attributes_to_stmt(stmt, attributes, span)
}

/// Attaches parsed attribute groups to a statement, validating that the statement kind supports attributes.
///
/// Returns an error if attributes are attached to a statement type that does not support them
/// (e.g., expressions, control flow). Callable only after `stmt_kind_supports_attributes` confirms eligibility.
fn attach_attributes_to_stmt(
    mut stmt: Stmt,
    attributes: Vec<AttributeGroup>,
    span: Span,
) -> Result<Stmt, CompileError> {
    if attributes.is_empty() {
        return Ok(stmt);
    }
    if stmt_kind_supports_attributes(&stmt.kind) {
        stmt.attributes = attributes;
        Ok(stmt)
    } else {
        Err(CompileError::new(
            span,
            "Attributes are only allowed before declarations \
             (class, interface, trait, enum, function)",
        ))
    }
}

/// Returns true if the given statement kind supports PHP 8 attribute groups.
///
/// Only class-like declarations (class, interface, trait, enum, function, packed class)
/// accept attribute groups per PHP syntax rules.
fn stmt_kind_supports_attributes(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::PackedClassDecl { .. }
    )
}

/// Dispatches to the appropriate statement parser based on the current token.
///
/// This is the top-level statement parser entry point. It handles all PHP statement types
/// including declarations, expressions, control flow, includes, and assignments.
/// Recovery errors within this function stop at PHP statement boundaries to preserve follow-up diagnostics.
fn parse_stmt_dispatch(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    match &tokens[*pos].0 {
        Token::Echo => simple::parse_echo(tokens, pos, span),
        Token::Print | Token::Clone => simple::parse_expr_stmt(tokens, pos, span),
        Token::At => simple::parse_error_suppressed_stmt(tokens, pos, span),
        Token::Variable(_) => assign::parse_variable_stmt(tokens, pos, span),
        Token::This => simple::parse_this_stmt(tokens, pos, span),
        Token::PlusPlus | Token::MinusMinus => assign::parse_incdec_stmt(tokens, pos, span),
        Token::LParen if crate::parser::expr::starts_void_statement_cast(tokens, *pos) => {
            simple::parse_void_expr_stmt(tokens, pos, span)
        }
        Token::Class => oop::parse_class_decl(tokens, pos, span, false, false, false),
        Token::Enum
            if tokens.get(*pos + 1).is_some_and(|(token, metadata)| {
                name_part_from_token(token, metadata).is_some()
            }) =>
        {
            oop::parse_enum_decl(tokens, pos, span)
        }
        Token::ReadOnly => oop::parse_readonly_decl(tokens, pos, span),
        Token::Packed => oop::parse_packed_decl(tokens, pos, span),
        Token::Interface => oop::parse_interface_decl(tokens, pos, span),
        Token::Trait => oop::parse_trait_decl(tokens, pos, span),
        Token::Abstract => oop::parse_abstract_decl(tokens, pos, span),
        Token::Final => oop::parse_final_decl(tokens, pos, span),
        Token::Function => params::parse_function_decl(tokens, pos, span),
        Token::Namespace => namespace_use::parse_namespace_stmt(tokens, pos, span),
        Token::Use => namespace_use::parse_use_stmt(tokens, pos, span),
        Token::Declare => declare::parse_declare(tokens, pos, span),
        Token::Return => simple::parse_return(tokens, pos, span),
        Token::Throw => simple::parse_throw(tokens, pos, span),
        Token::Yield => {
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        Token::Include => simple::parse_include(tokens, pos, span, false, false),
        Token::IncludeOnce => simple::parse_include(tokens, pos, span, true, false),
        Token::Require => simple::parse_include(tokens, pos, span, false, true),
        Token::RequireOnce => simple::parse_include(tokens, pos, span, true, true),
        Token::Const => simple::parse_const_decl(tokens, pos, span),
        Token::Global => assign::parse_global(tokens, pos, span),
        Token::EndIf
        | Token::EndWhile
        | Token::EndFor
        | Token::EndForeach
        | Token::EndSwitch
        | Token::EndDeclare => {
            let keyword = tokens[*pos]
                .0
                .canonical_word_spelling()
                .unwrap_or("end-block keyword");
            *pos += 1;
            Err(crate::parser::alt_syntax::unopened_terminator_error(
                keyword, span,
            ))
        }
        Token::Goto => Err(goto_unsupported::reject_goto_statement(span)),
        Token::Identifier(label) if goto_unsupported::starts_goto_label(tokens, *pos) => {
            Err(goto_unsupported::reject_goto_label(label, span))
        }
        Token::Static => {
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::DoubleColon {
                if let Some(stmt) =
                    assign::try_parse_scoped_property_assignment(tokens, pos, span)?
                {
                    return Ok(stmt);
                }
                if let Some(stmt) = assign::try_parse_scoped_postfix_incdec(tokens, pos, span)? {
                    return Ok(stmt);
                }
                let expr = parse_expr(tokens, pos)?;
                expect_semicolon(tokens, pos)?;
                Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
            } else {
                assign::parse_static_var(tokens, pos, span)
            }
        }
        Token::LBracket => assign::parse_list_unpack(tokens, pos, span),
        Token::Identifier(_)
        | Token::Enum
        | Token::Self_
        | Token::Parent
        | Token::Backslash
        | Token::Question
        | Token::New
        | Token::LParen
        | Token::Match => {
            if matches!(&tokens[*pos].0, Token::Identifier(name) if name.eq_ignore_ascii_case("list"))
                && matches!(tokens.get(*pos + 1).map(|(token, _)| token), Some(Token::LParen))
            {
                return assign::parse_list_construct_unpack(tokens, pos, span);
            }
            if assign::looks_like_typed_assign(tokens, *pos) {
                return assign::parse_typed_assign(tokens, pos, span);
            }
            if statement_lhs_contains_double_colon(tokens, *pos) {
                if let Some(stmt) =
                    assign::try_parse_scoped_property_assignment(tokens, pos, span)?
                {
                    return Ok(stmt);
                }
                if let Some(stmt) = assign::try_parse_scoped_postfix_incdec(tokens, pos, span)? {
                    return Ok(stmt);
                }
            }
            if let Some(stmt) = assign::try_parse_postfix_assignment(tokens, pos, span)? {
                return Ok(stmt);
            }
            let expr = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
        }
        // Control flow — delegated to control.rs
        Token::Switch => control::parse_switch(tokens, pos, span),
        Token::If => control::parse_if(tokens, pos, span),
        Token::IfDef => control::parse_ifdef(tokens, pos, span),
        Token::Try => control::parse_try(tokens, pos, span),
        Token::While => control::parse_while(tokens, pos, span),
        Token::Do => control::parse_do_while(tokens, pos, span),
        Token::For => control::parse_for(tokens, pos, span),
        Token::Foreach => control::parse_foreach(tokens, pos, span),
        Token::Break => {
            *pos += 1;
            let levels = parse_loop_exit_level("break", tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Break(levels), span))
        }
        Token::Continue => {
            *pos += 1;
            let levels = parse_loop_exit_level("continue", tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            Ok(Stmt::new(StmtKind::Continue(levels), span))
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token at statement position: {:?}", other),
        )),
    }
}

/// Parses the exit level for `break` or `continue` statements.
///
/// Accepts an optional positive integer literal to specify the number of loop levels to exit.
/// Returns 1 if no level is specified. Fails if the expression is not a positive integer literal
/// or exceeds `usize` range.
fn parse_loop_exit_level(
    keyword: &str,
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<usize, CompileError> {
    if tokens[*pos].0 == Token::Semicolon {
        return Ok(1);
    }

    let expr = parse_expr(tokens, pos)?;
    match expr.kind {
        ExprKind::IntLiteral(level) if level > 0 => usize::try_from(level).map_err(|_| {
            CompileError::new(
                expr.span,
                &format!("{} operator accepts only positive integers", keyword),
            )
        }),
        ExprKind::IntLiteral(_) => Err(CompileError::new(
            expr.span,
            &format!("{} operator accepts only positive integers", keyword),
        )),
        _ => Err(CompileError::new(
            expr.span,
            &format!("{} operator requires an integer literal level", keyword),
        )),
    }
}

/// Scans tokens from `start` forward to detect whether a `::` appears outside of parentheses or brackets.
///
/// Used to disambiguate scoped property accesses (e.g., `Foo::$bar`) from comparison operators
/// in expression statements. Stops scanning when it encounters an assignment operator at depth 0,
/// returning false in that case.
fn statement_lhs_contains_double_colon(tokens: &[SpannedToken], start: usize) -> bool {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    for (token, _) in tokens.iter().skip(start) {
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::DoubleColon if paren_depth == 0 && bracket_depth == 0 => return true,
            Token::Assign
            | Token::PlusAssign
            | Token::MinusAssign
            | Token::StarAssign
            | Token::StarStarAssign
            | Token::SlashAssign
            | Token::DotAssign
            | Token::PercentAssign
            | Token::AmpAssign
            | Token::PipeAssign
            | Token::CaretAssign
            | Token::LessLessAssign
            | Token::GreaterGreaterAssign
            | Token::QuestionQuestionAssign
            | Token::Semicolon
                if paren_depth == 0 && bracket_depth == 0 =>
            {
                return false;
            }
            _ => {}
        }
    }
    false
}
