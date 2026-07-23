//! Purpose:
//! Defines the complete token vocabulary accepted by the PHP frontend.
//! Represents PHP keywords, literals, operators, punctuation, magic constants, and extensions.
//!
//! Called from:
//! - `crate::lexer::scan` when emitting tokens and `crate::parser` when matching syntax.
//!
//! Key details:
//! - Token kinds drive syntax while `TokenMetadata` retains source spelling for
//!   context-dependent names that were lexed as case-insensitive keywords.

use crate::span::Span;

/// Metadata attached to one lexer token.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenMetadata {
    /// Source extent used by parser and semantic diagnostics.
    pub span: Span,
    source_spelling: Option<Box<str>>,
}

impl TokenMetadata {
    /// Creates metadata for a token that does not need an independently retained spelling.
    pub fn new(span: Span) -> Self {
        Self {
            span,
            source_spelling: None,
        }
    }

    /// Creates metadata that retains the exact source spelling of a word-like token.
    pub(crate) fn with_source_spelling(span: Span, spelling: &str) -> Self {
        Self {
            span,
            source_spelling: Some(spelling.into()),
        }
    }

    /// Returns the non-canonical source-spelling override retained for a keyword token.
    fn source_spelling_override(&self) -> Option<&str> {
        self.source_spelling.as_deref()
    }

    /// Replaces the source extent while retaining the token's original word spelling.
    pub(crate) fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
}

/// A syntax token paired with its source metadata.
pub type SpannedToken = (Token, TokenMetadata);

/// Creates a spanned token without separately retained word spelling.
pub(crate) fn spanned(token: Token, span: Span) -> SpannedToken {
    (token, TokenMetadata::new(span))
}

#[derive(Debug, Clone, PartialEq)]
/// Lexer token.
pub enum Token {
    // Structural
    OpenTag,        // <?php
    Semicolon,      // ;
    LParen,         // (
    RParen,         // )
    LBrace,         // {
    RBrace,         // }

    // Literals
    StringLiteral(String),
    IntLiteral(i64),
    FloatLiteral(f64),

    // Identifiers
    Variable(String),
    Identifier(String),

    // Keywords
    Echo,
    If,
    IfDef,
    Else,
    ElseIf,
    While,
    For,
    Break,
    Continue,
    Function,
    Return,
    True,
    False,
    Null,
    Do,
    Foreach,
    As,
    Try,
    Catch,
    Finally,
    Throw,
    Extends,
    Implements,
    Interface,
    Abstract,
    Final,
    Inf,
    Nan,
    PhpIntMax,
    PhpIntMin,
    PhpFloatMax,
    MPi,
    ME,
    MSqrt2,
    MPi2,
    MPi4,
    MLog2e,
    MLog10e,
    PhpFloatMin,
    PhpFloatEpsilon,
    Print,
    Switch,
    Case,
    Default,
    Match,
    Include,
    IncludeOnce,
    Require,
    RequireOnce,
    Stdin,
    Stdout,
    Stderr,
    Fn,             // fn (arrow functions)
    Use,            // use (closure captures — reserved for future)
    Namespace,      // namespace
    Const,          // const
    Global,         // global
    Declare,        // declare (strict_types/ticks/encoding directive)
    EndDeclare,     // enddeclare (alternative declare block terminator)
    Static,         // static
    Self_,          // self
    Trait,          // trait
    Parent,         // parent
    InsteadOf,      // insteadof
    InstanceOf,     // instanceof
    PhpEol,
    PhpOs,
    DirectorySeparator,
    PathSeparator,
    DunderDir,
    DunderFile,
    DunderLine,
    DunderFunction,
    DunderClass,
    DunderMethod,
    DunderNamespace,
    DunderTrait,
    Class,          // class
    Enum,           // enum
    New,            // new
    Clone,          // clone
    Public,         // public
    Protected,      // protected
    Private,        // private
    ReadOnly,       // readonly
    This,           // $this
    Extern,         // extern
    Packed,         // packed
    Yield,          // yield (also: `yield from`; `from` parsed contextually)
    AttrOpen,       // #[ (start of attribute group)

    // Operators
    Assign,         // =
    DoubleArrow,    // =>
    Plus,           // +
    Minus,          // -
    Star,           // *
    StarStar,       // **
    Slash,          // /
    Percent,        // %
    Dot,            // .
    Comma,          // ,
    Backslash,      // \
    LBracket,       // [
    RBracket,       // ]
    Question,       // ?
    Colon,          // :

    // Compound assignment
    PlusAssign,     // +=
    MinusAssign,    // -=
    StarAssign,     // *=
    StarStarAssign, // **=
    SlashAssign,    // /=
    DotAssign,      // .=
    PercentAssign,  // %=
    AmpAssign,      // &=
    PipeAssign,     // |=
    CaretAssign,    // ^=
    LessLessAssign, // <<=
    GreaterGreaterAssign, // >>=

    // Increment/Decrement
    PlusPlus,       // ++
    MinusMinus,     // --

    // Logical
    AndAnd,         // &&
    OrOr,           // ||
    And,            // and
    Or,             // or
    Xor,            // xor
    Bang,           // !

    // Comparison
    EqualEqual,     // ==
    EqualEqualEqual, // ===
    NotEqual,       // !=
    NotEqualEqual,  // !==
    Less,           // <
    Greater,        // >
    LessEqual,      // <=
    GreaterEqual,   // >=
    Spaceship,      // <=>

    // Bitwise
    Ampersand,      // &
    Pipe,           // |
    Caret,          // ^
    Tilde,          // ~
    At,             // @
    LessLess,       // <<
    GreaterGreater, // >>

    // Object access
    Arrow,          // ->
    QuestionArrow,  // ?->
    DoubleColon,    // ::

    // Null coalescing
    QuestionQuestion,       // ??
    QuestionQuestionAssign, // ??=

    // PHP 8.5 pipe operator
    PipeArrow,      // |>

    // Variadic / spread
    Ellipsis,         // ...

    // End of file
    Eof,
}

impl Token {
    /// Returns the exact source word for identifiers, keywords, and keyword-like constants.
    ///
    /// Canonically spelled keywords need no allocation: their token kind supplies the word.
    /// A spelling override in `metadata` is used only when case-insensitive lexing would
    /// otherwise erase source casing.
    pub fn word_spelling<'a>(&'a self, metadata: &'a TokenMetadata) -> Option<&'a str> {
        match self {
            Token::Identifier(name) => Some(name),
            _ => metadata
                .source_spelling_override()
                .or_else(|| self.canonical_word_spelling()),
        }
    }

    /// Returns the canonical spelling for a token that PHP permits as a bareword name.
    pub(crate) fn canonical_word_spelling(&self) -> Option<&'static str> {
        match self {
            Token::Echo => Some("echo"),
            Token::If => Some("if"),
            Token::IfDef => Some("ifdef"),
            Token::Else => Some("else"),
            Token::ElseIf => Some("elseif"),
            Token::While => Some("while"),
            Token::For => Some("for"),
            Token::Break => Some("break"),
            Token::Continue => Some("continue"),
            Token::Function => Some("function"),
            Token::Return => Some("return"),
            Token::True => Some("true"),
            Token::False => Some("false"),
            Token::Null => Some("null"),
            Token::Do => Some("do"),
            Token::Foreach => Some("foreach"),
            Token::As => Some("as"),
            Token::Try => Some("try"),
            Token::Catch => Some("catch"),
            Token::Finally => Some("finally"),
            Token::Throw => Some("throw"),
            Token::Extends => Some("extends"),
            Token::Implements => Some("implements"),
            Token::Interface => Some("interface"),
            Token::Abstract => Some("abstract"),
            Token::Final => Some("final"),
            Token::Print => Some("print"),
            Token::Switch => Some("switch"),
            Token::Case => Some("case"),
            Token::Default => Some("default"),
            Token::Match => Some("match"),
            Token::Include => Some("include"),
            Token::IncludeOnce => Some("include_once"),
            Token::Require => Some("require"),
            Token::RequireOnce => Some("require_once"),
            Token::Fn => Some("fn"),
            Token::Use => Some("use"),
            Token::Namespace => Some("namespace"),
            Token::Const => Some("const"),
            Token::Global => Some("global"),
            Token::Declare => Some("declare"),
            Token::EndDeclare => Some("enddeclare"),
            Token::Static => Some("static"),
            Token::Self_ => Some("self"),
            Token::Trait => Some("trait"),
            Token::Parent => Some("parent"),
            Token::InsteadOf => Some("insteadof"),
            Token::Class => Some("class"),
            Token::Enum => Some("enum"),
            Token::New => Some("new"),
            Token::Clone => Some("clone"),
            Token::Public => Some("public"),
            Token::Protected => Some("protected"),
            Token::Private => Some("private"),
            Token::ReadOnly => Some("readonly"),
            Token::Extern => Some("extern"),
            Token::Packed => Some("packed"),
            Token::Yield => Some("yield"),
            Token::And => Some("and"),
            Token::Or => Some("or"),
            Token::Xor => Some("xor"),
            Token::InstanceOf => Some("instanceof"),
            Token::Inf => Some("INF"),
            Token::Nan => Some("NAN"),
            Token::PhpIntMax => Some("PHP_INT_MAX"),
            Token::PhpIntMin => Some("PHP_INT_MIN"),
            Token::PhpFloatMax => Some("PHP_FLOAT_MAX"),
            Token::MPi => Some("M_PI"),
            Token::ME => Some("M_E"),
            Token::MSqrt2 => Some("M_SQRT2"),
            Token::MPi2 => Some("M_PI_2"),
            Token::MPi4 => Some("M_PI_4"),
            Token::MLog2e => Some("M_LOG2E"),
            Token::MLog10e => Some("M_LOG10E"),
            Token::PhpFloatMin => Some("PHP_FLOAT_MIN"),
            Token::PhpFloatEpsilon => Some("PHP_FLOAT_EPSILON"),
            Token::Stdin => Some("STDIN"),
            Token::Stdout => Some("STDOUT"),
            Token::Stderr => Some("STDERR"),
            Token::PhpEol => Some("PHP_EOL"),
            Token::PhpOs => Some("PHP_OS"),
            Token::DirectorySeparator => Some("DIRECTORY_SEPARATOR"),
            Token::PathSeparator => Some("PATH_SEPARATOR"),
            Token::DunderDir => Some("__DIR__"),
            Token::DunderFile => Some("__FILE__"),
            Token::DunderLine => Some("__LINE__"),
            Token::DunderFunction => Some("__FUNCTION__"),
            Token::DunderClass => Some("__CLASS__"),
            Token::DunderMethod => Some("__METHOD__"),
            Token::DunderNamespace => Some("__NAMESPACE__"),
            Token::DunderTrait => Some("__TRAIT__"),
            _ => None,
        }
    }
}
