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

    // Operators
    Assign,         // =
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Dot,            // .
    Comma,          // ,

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
    NotEqual,       // !=
    Less,           // <
    Greater,        // >
    LessEqual,      // <=
    GreaterEqual,   // >=

    // End of file
    Eof,
}
