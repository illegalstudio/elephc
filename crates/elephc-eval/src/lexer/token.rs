//! Purpose:
//! Defines token kinds for runtime PHP eval fragment parsing.
//! Tokens are intentionally scoped to the eval subset and do not expose the
//! main compiler lexer token contract.
//!
//! Called from:
//! - `crate::lexer::scan::tokenize()`
//! - `crate::parser::state::Parser`
//!
//! Key details:
//! - Magic constants carry precomputed fragment line metadata when needed.

use crate::eval_ir::EvalMagicConst;

/// Token kinds used by the initial eval fragment parser.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    DollarIdent(String),
    Ident(String),
    Magic(EvalMagicConst),
    Int(i64),
    Float(f64),
    String(String),
    Plus,
    PlusPlus,
    PlusEqual,
    Minus,
    MinusMinus,
    MinusEqual,
    Arrow,
    Star,
    StarStar,
    StarStarEqual,
    StarEqual,
    Slash,
    SlashEqual,
    Percent,
    PercentEqual,
    Ampersand,
    AmpEqual,
    Pipe,
    PipeEqual,
    Caret,
    CaretEqual,
    Tilde,
    Dot,
    DotEqual,
    Ellipsis,
    Equal,
    EqualEqual,
    EqualEqualEqual,
    Bang,
    NotEqual,
    NotEqualEqual,
    AndAnd,
    OrOr,
    Less,
    LessEqual,
    Spaceship,
    LessLess,
    LessLessEqual,
    Greater,
    GreaterEqual,
    GreaterGreater,
    GreaterGreaterEqual,
    FatArrow,
    Question,
    QuestionQuestion,
    Semicolon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    DoubleColon,
    Backslash,
    Eof,
}
