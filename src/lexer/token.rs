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
    Extends,
    Implements,
    Interface,
    Abstract,
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
    Const,          // const
    Global,         // global
    Static,         // static
    Self_,          // self
    Trait,          // trait
    Parent,         // parent
    InsteadOf,      // insteadof
    PhpEol,
    PhpOs,
    DirectorySeparator,
    Class,          // class
    New,            // new
    Public,         // public
    Protected,      // protected
    Private,        // private
    ReadOnly,       // readonly
    This,           // $this
    Extern,         // extern

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
    LBracket,       // [
    RBracket,       // ]
    Question,       // ?
    Colon,          // :

    // Compound assignment
    PlusAssign,     // +=
    MinusAssign,    // -=
    StarAssign,     // *=
    SlashAssign,    // /=
    DotAssign,      // .=
    PercentAssign,  // %=

    // Increment/Decrement
    PlusPlus,       // ++
    MinusMinus,     // --

    // Logical
    AndAnd,         // &&
    OrOr,           // ||
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
    LessLess,       // <<
    GreaterGreater, // >>

    // Object access
    Arrow,          // ->
    DoubleColon,    // ::

    // Null coalescing
    QuestionQuestion, // ??

    // Variadic / spread
    Ellipsis,         // ...

    // End of file
    Eof,
}
