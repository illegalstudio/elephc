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

/// One token plus its eval-fragment source line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    kind: TokenKind,
    line: i64,
}

impl Token {
    /// Creates one token at the given eval-fragment line.
    pub(crate) const fn new(kind: TokenKind, line: i64) -> Self {
        Self { kind, line }
    }

    /// Returns the parser-visible token kind.
    pub(crate) fn kind(&self) -> &TokenKind {
        &self.kind
    }

    /// Consumes the token and returns its parser-visible kind.
    pub(crate) fn into_kind(self) -> TokenKind {
        self.kind
    }

    /// Returns the one-based eval-fragment line where this token starts.
    pub(crate) const fn line(&self) -> i64 {
        self.line
    }
}

/// Token kinds used by the initial eval fragment parser.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    DollarLBrace,
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
    QuestionArrow,
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
    AttributeStart,
    Eof,
}
