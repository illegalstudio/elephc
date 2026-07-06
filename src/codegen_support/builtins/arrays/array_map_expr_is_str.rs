//! Purpose:
//! Classifies array_map callback expressions whose return value can be treated as a string.
//! Keeps AST-level callback inspection separate from array_map emission mechanics.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::array_map_callback_returns_str::callback_returns_str()`.
//!
//! Key details:
//! - Only use syntactic facts here; semantic callable validation remains in the type checker.

use crate::parser::ast::{BinOp, Expr, ExprKind};

/// Returns true if the expression syntactically produces a string result.
///
/// Recognizes: string literals, `.` (concat) binary ops, explicit `(string)` casts,
/// and calls to known string-returning builtins (`substr`, `strtolower`, `strtoupper`,
/// `trim`, `ltrim`, `rtrim`, `str_repeat`, `strrev`, `chr`, `str_replace`, `ucfirst`,
/// `lcfirst`, `ucwords`, `str_pad`, `implode`, `join`, `sprintf`, `str_word_count`,
/// `nl2br`, `wordwrap`, `number_format`, `chunk_split`, `md5`, `sha1`, `hash`).
///
/// This is a purely syntactic check; no semantic callable validation is performed.
pub(super) fn expr_is_str(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::BinaryOp {
            op: BinOp::Concat, ..
        } => true,
        ExprKind::FunctionCall { name, .. } => {
            matches!(
                name.as_str(),
                "substr"
                    | "strtolower"
                    | "strtoupper"
                    | "trim"
                    | "ltrim"
                    | "rtrim"
                    | "str_repeat"
                    | "strrev"
                    | "chr"
                    | "str_replace"
                    | "ucfirst"
                    | "lcfirst"
                    | "ucwords"
                    | "str_pad"
                    | "implode"
                    | "join"
                    | "sprintf"
                    | "str_word_count"
                    | "nl2br"
                    | "wordwrap"
                    | "number_format"
                    | "chunk_split"
                    | "md5"
                    | "sha1"
                    | "hash"
            )
        }
        ExprKind::Cast {
            target: crate::parser::ast::CastType::String,
            ..
        } => true,
        _ => false,
    }
}
