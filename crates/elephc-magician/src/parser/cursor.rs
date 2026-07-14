//! Purpose:
//! Provides parser cursor primitives and small grammar helper functions shared by statement and expression parsing.
//!
//! Called from:
//! - `crate::parser::statements` and `crate::parser::expressions`.
//!
//! Key details:
//! - Cursor helpers are intentionally thin wrappers over the token stream.
//! - Helper predicates centralize PHP keyword and assignment-token recognition.

use super::state::Parser;
use crate::errors::EvalParseError;
use crate::eval_ir::{EvalBinOp, EvalConst, EvalExpr, EvalStmt};
use crate::lexer::TokenKind;

impl Parser {
    /// Consumes `expected` or returns a parse error.
    pub(super) fn expect(&mut self, expected: TokenKind) -> Result<(), EvalParseError> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(EvalParseError::UnexpectedToken)
        }
    }

    /// Consumes a semicolon or returns the semicolon-specific parse error.
    pub(super) fn expect_semicolon(&mut self) -> Result<(), EvalParseError> {
        if self.consume_semicolon() {
            Ok(())
        } else {
            Err(EvalParseError::ExpectedSemicolon)
        }
    }

    /// Consumes a semicolon if present.
    pub(super) fn consume_semicolon(&mut self) -> bool {
        self.consume(TokenKind::Semicolon)
    }

    /// Consumes `expected` if the current token matches it.
    pub(super) fn consume(&mut self, expected: TokenKind) -> bool {
        if *self.current() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Returns the current token.
    pub(super) fn current(&self) -> &TokenKind {
        self.tokens.get(self.pos).unwrap_or(&TokenKind::Eof)
    }

    /// Returns the line attached to the current token.
    pub(super) fn current_line(&self) -> i64 {
        self.token_lines.get(self.pos).copied().unwrap_or(1)
    }

    /// Returns the next token without advancing.
    pub(super) fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos + 1).unwrap_or(&TokenKind::Eof)
    }

    /// Advances to the next token.
    pub(super) fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }
}

/// Returns true when the current token closes or starts a switch case arm.
pub(super) fn is_switch_case_boundary(token: &TokenKind) -> bool {
    matches!(token, TokenKind::RBrace)
        || matches!(token, TokenKind::Ident(name) if ident_eq(name, "case") || ident_eq(name, "default"))
}

/// Maps simple variable assignment tokens to an optional compound EvalIR operator.
pub(super) fn assignment_op(token: &TokenKind) -> Option<Option<EvalBinOp>> {
    match token {
        TokenKind::Equal => Some(None),
        TokenKind::PlusEqual => Some(Some(EvalBinOp::Add)),
        TokenKind::MinusEqual => Some(Some(EvalBinOp::Sub)),
        TokenKind::StarEqual => Some(Some(EvalBinOp::Mul)),
        TokenKind::StarStarEqual => Some(Some(EvalBinOp::Pow)),
        TokenKind::SlashEqual => Some(Some(EvalBinOp::Div)),
        TokenKind::PercentEqual => Some(Some(EvalBinOp::Mod)),
        TokenKind::AmpEqual => Some(Some(EvalBinOp::BitAnd)),
        TokenKind::PipeEqual => Some(Some(EvalBinOp::BitOr)),
        TokenKind::CaretEqual => Some(Some(EvalBinOp::BitXor)),
        TokenKind::LessLessEqual => Some(Some(EvalBinOp::ShiftLeft)),
        TokenKind::GreaterGreaterEqual => Some(Some(EvalBinOp::ShiftRight)),
        TokenKind::DotEqual => Some(Some(EvalBinOp::Concat)),
        _ => None,
    }
}

/// Builds the assigned value expression for plain and compound variable assignment.
pub(super) fn assignment_value(name: &str, op: Option<EvalBinOp>, value: EvalExpr) -> EvalExpr {
    match op {
        Some(op) => EvalExpr::Binary {
            op,
            left: Box::new(EvalExpr::LoadVar(name.to_string())),
            right: Box::new(value),
        },
        None => value,
    }
}

/// Builds the StoreVar statement for a simple variable increment or decrement.
pub(super) fn inc_dec_store(name: String, increment: bool) -> EvalStmt {
    EvalStmt::StoreVar {
        value: EvalExpr::Binary {
            op: if increment {
                EvalBinOp::Add
            } else {
                EvalBinOp::Sub
            },
            left: Box::new(EvalExpr::LoadVar(name.clone())),
            right: Box::new(EvalExpr::Const(EvalConst::Int(1))),
        },
        name,
    }
}

/// Compares a source identifier to a PHP keyword using ASCII case-insensitive rules.
pub(super) fn ident_eq(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// Returns true when PHP forbids a name for class, interface, trait, or enum declarations.
pub(super) fn is_reserved_class_like_name(name: &str) -> bool {
    [
        "__halt_compiler",
        "abstract",
        "and",
        "array",
        "as",
        "break",
        "callable",
        "case",
        "catch",
        "class",
        "clone",
        "const",
        "continue",
        "declare",
        "default",
        "die",
        "do",
        "echo",
        "else",
        "elseif",
        "empty",
        "enddeclare",
        "endfor",
        "endforeach",
        "endif",
        "endswitch",
        "endwhile",
        "eval",
        "exit",
        "extends",
        "false",
        "final",
        "finally",
        "fn",
        "for",
        "foreach",
        "function",
        "global",
        "goto",
        "if",
        "implements",
        "include",
        "include_once",
        "instanceof",
        "insteadof",
        "interface",
        "isset",
        "list",
        "match",
        "namespace",
        "new",
        "null",
        "or",
        "print",
        "private",
        "protected",
        "public",
        "readonly",
        "require",
        "require_once",
        "return",
        "static",
        "switch",
        "throw",
        "trait",
        "true",
        "try",
        "unset",
        "use",
        "var",
        "while",
        "xor",
        "yield",
        "bool",
        "int",
        "float",
        "string",
        "object",
        "mixed",
        "never",
        "void",
        "iterable",
        "self",
        "parent",
    ]
    .iter()
    .any(|keyword| ident_eq(name, keyword))
}

/// Returns true for PHP statement forms that the eval subset intentionally does not parse yet.
pub(super) fn is_unsupported_statement_keyword(name: &str) -> bool {
    let _ = name;
    false
}

/// Returns true for class member modifiers outside the current eval class subset.
pub(super) fn is_unsupported_class_member_modifier(name: &str) -> bool {
    let _ = name;
    false
}

/// Returns true when an identifier is an include/require expression construct.
pub(super) fn is_include_construct_name(name: &str) -> bool {
    ["include", "include_once", "require", "require_once"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}

/// Returns the first namespace segment and the optional remaining suffix.
pub(super) fn split_first_name_segment(name: &str) -> (&str, Option<&str>) {
    name.split_once('\\')
        .map_or((name, None), |(first, tail)| (first, Some(tail)))
}

/// Returns the final segment of a PHP qualified name.
pub(super) fn last_name_segment(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// Combines a grouped use prefix with one relative member name.
pub(super) fn join_grouped_use_name(prefix: &str, member: &str) -> String {
    format!("{prefix}\\{member}")
}

/// Returns true for PHP expression forms that the eval subset intentionally does not parse yet.
pub(super) fn is_unsupported_expression_keyword(name: &str) -> bool {
    ["yield"]
        .iter()
        .any(|keyword| ident_eq(name, keyword))
}
