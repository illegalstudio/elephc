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
    Inf,
    Nan,
    PhpIntMax,
    PhpIntMin,
    PhpFloatMax,
    MPi,
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
    PhpEol,
    PhpOs,
    DirectorySeparator,

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

    // Null coalescing
    QuestionQuestion, // ??

    // Variadic / spread
    Ellipsis,         // ...

    // End of file
    Eof,
}
