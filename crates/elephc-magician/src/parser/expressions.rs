//! Purpose:
//! Parses PHP eval expressions using PHP-compatible precedence and postfix syntax.
//!
//! Called from:
//! - `crate::parser::statements` for expression-bearing statements.
//!
//! Key details:
//! - Logical keyword precedence, ternary associativity, coalesce, and exponentiation follow PHP grammar.
//! - Name resolution uses parser namespace/import state while building EvalIR call and constant nodes.

use super::cursor::*;
use super::state::*;
use super::statements::{EvalTypePosition, ParsedMethodParams};
use crate::errors::EvalParseError;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalCastType, EvalClosureCapture, EvalConst,
    EvalExpr, EvalFunction, EvalInstanceOfTarget, EvalMagicConst, EvalMatchArm, EvalSourceLocation,
    EvalUnaryOp,
};
use crate::lexer::TokenKind;

mod callables_arrays;
mod postfix;
mod precedence;
mod primary;
mod static_names;
