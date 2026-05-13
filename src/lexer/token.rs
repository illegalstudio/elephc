//! Purpose:
//! Defines the complete token vocabulary accepted by the PHP frontend.
//! Represents PHP keywords, literals, operators, punctuation, magic constants, and extensions.
//!
//! Called from:
//! - `crate::lexer::scan` when emitting tokens and `crate::parser` when matching syntax.
//!
//! Key details:
//! - Token names must track PHP-compatible spelling and precedence-sensitive operators exactly.

#[derive(Debug, Clone, PartialEq)]
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
    Static,         // static
    Self_,          // self
    Trait,          // trait
    Parent,         // parent
    InsteadOf,      // insteadof
    InstanceOf,     // instanceof
    PhpEol,
    PhpOs,
    DirectorySeparator,
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

    // Variadic / spread
    Ellipsis,         // ...

    // End of file
    Eof,
}
