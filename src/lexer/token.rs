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

    // Keywords
    Echo,
    If,
    Else,
    ElseIf,
    While,
    For,
    Break,
    Continue,

    // Operators
    Assign,         // =
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Dot,            // .

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
