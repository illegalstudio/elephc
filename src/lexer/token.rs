#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Structural
    OpenTag,        // <?php
    Semicolon,      // ;
    LParen,         // (
    RParen,         // )

    // Literals
    StringLiteral(String),
    IntLiteral(i64),

    // Identifiers
    Variable(String), // $name (stored without the $)

    // Keywords
    Echo,

    // Operators
    Assign,     // =
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Dot,        // .

    // End of file
    Eof,
}
