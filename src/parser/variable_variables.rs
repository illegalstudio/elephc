//! Purpose:
//! Parses PHP variable variables (`$$name`, `$$$name`, `${expr}`) into the frame-access shape
//! that `crate::variable_variables` lowers before type checking.
//!
//! Called from:
//! - `crate::parser::expr::prefix::parse_prefix()` on a `Token::Dollar`.
//!
//! Key details:
//! - `$$a` parses as `ArrayAccess { array: Variable(FRAME), index: Variable("a") }`, where
//!   `FRAME` is an unforgeable pseudo-variable naming the current frame's variable table. Keeping
//!   the ordinary array-access shape means every postfix (`$$a[k]`, `$$a->p`, `$$a(...)`) and
//!   every assignment form the expression parser already supports applies unchanged, and the
//!   desugar pass rewrites the frame accesses into plain local reads and writes.
//! - The operand binds to the next simple variable only, as in PHP 7+: `$$a[0]` is `($$a)[0]`.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;

/// Parses a variable variable starting at the `Token::Dollar` under `pos`.
pub(crate) fn parse_variable_variable(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1; // consume the leading `$`
    let Some((token, metadata)) = tokens.get(*pos) else {
        return Err(CompileError::new(span, "Expected variable name after '$'"));
    };
    let name_span = metadata.span;
    let name_expr = match token {
        Token::Variable(name) => {
            *pos += 1;
            Expr::new(ExprKind::Variable(name.clone()), name_span)
        }
        Token::This => {
            *pos += 1;
            Expr::new(ExprKind::This, name_span)
        }
        Token::Dollar => parse_variable_variable(tokens, pos, name_span)?,
        Token::LBrace => {
            *pos += 1;
            let inner = crate::parser::expr::parse_expr(tokens, pos)?;
            crate::parser::stmt::expect_token(
                tokens,
                pos,
                &Token::RBrace,
                "Expected '}' to close the variable-variable name expression `${...}`",
            )?;
            inner
        }
        _ => return Err(CompileError::new(span, "Expected variable name after '$'")),
    };
    Ok(crate::variable_variables::frame_access(name_expr, span))
}

/// Parses the variable member name of `C::$$name` / `C::${expr}` (the `$` is under `pos`) into
/// the dynamic static-property shape `crate::variable_variables::static_props` lowers: an
/// array access whose base is the unforgeable `C::$<marker>` property and whose index is the
/// name expression. Every postfix and assignment form then parses as for `C::$name`.
pub(crate) fn parse_dynamic_static_property(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    receiver: StaticReceiver,
) -> Result<Expr, CompileError> {
    let access = parse_variable_variable(tokens, pos, span)?;
    let Some(name) = crate::variable_variables::frame_access_name(&access) else {
        return Err(CompileError::new(span, "Expected static property name after '::$'"));
    };
    Ok(crate::variable_variables::static_props::dynamic_static_access(
        receiver,
        name.clone(),
        span,
    ))
}
