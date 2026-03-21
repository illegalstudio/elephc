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
    Variable(String), // $name (stored without the $)

    // Identifiers (function names etc.)
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

    // Operators
    Assign,         // =
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Dot,            // .

    Comma,          // ,

    // Increment/Decrement
    PlusPlus,       // ++
    MinusMinus,     // --

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
