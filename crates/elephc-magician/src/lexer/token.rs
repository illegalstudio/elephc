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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(crate) struct Token {
    kind: TokenKind,
    line: i64,
    phpdoc: bool,
}

impl Token {
    /// Creates one token at the given eval-fragment line.
    pub(crate) const fn new(kind: TokenKind, line: i64) -> Self {
        Self { kind, line, phpdoc: false }
    }

    /// Marks synthesized portable annotation tokens without losing their source line.
    pub(crate) fn with_phpdoc(mut self, enabled: bool) -> Self { self.phpdoc = enabled; self }

    /// Returns whether this token came from a supported PHPDoc annotation.
    pub(crate) fn phpdoc(&self) -> bool { self.phpdoc }

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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    DocComment(String),
    DollarLBrace,
    /// A `$` followed by another `$`: the outer sigil of a variable-variable (`$$name`).
    Dollar,
    DollarIdent(String),
    Ident(String),
    /// A reserved word glued to a following `\` and segment, as `Default` in
    /// `Default\Theme\Palette` (#826). PHP 8 lexes the whole name as one token, so the word is
    /// the first segment of a qualified name and never the keyword: keyword arms match
    /// `Ident` only, and every name-start position accepts this kind too.
    ReservedNameHead(String),
    Magic(EvalMagicConst),
    Int(i64),
    Float(f64),
    String(String),
    /// One run of source text outside every `<?php … ?>` block, kept as raw bytes.
    ///
    /// PHP's scanner emits `T_INLINE_HTML` for it and the grammar turns it into an echo, which is
    /// why a `{` opened in one block and closed in a later one parses: the whole file is one token
    /// stream. The bytes stay unvalidated because a template may carry any encoding.
    InlineHtml(Vec<u8>),
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
    At,
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
    QuestionQuestionEqual,
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

/// PHP keywords the reference lexer turns into reserved tokens rather than identifiers.
///
/// PHP's diagnostic says `unexpected token "return"` for a reserved word and
/// `unexpected identifier "bar"` for anything else, so naming the failing token the way PHP
/// does needs this split even though this lexer scans both as `Ident`.
const PHP_RESERVED_WORDS: &[&str] = &[
    "abstract", "and", "array", "as", "break", "callable", "case", "catch", "class", "clone",
    "const", "continue", "declare", "default", "die", "do", "echo", "else", "elseif", "empty",
    "enddeclare", "endfor", "endforeach", "endif", "endswitch", "endwhile", "enum", "eval", "exit",
    "extends", "final", "finally", "fn", "for", "foreach", "function", "global", "goto", "if",
    "implements", "include", "include_once", "instanceof", "insteadof", "interface", "isset",
    "list", "match", "namespace", "new", "or", "print", "private", "protected", "public",
    "readonly", "require", "require_once", "return", "static", "switch", "throw", "trait", "try",
    "unset", "use", "var", "while", "xor", "yield",
];

impl TokenKind {
    /// Returns PHP's description of this token inside a `syntax error, unexpected …` clause.
    ///
    /// Measured against `php -n` 8.5.6: a reserved word and every operator are named
    /// `token "x"`, a non-reserved word `identifier "x"`, a variable `variable "$x"`, and the
    /// numeric literals `integer "1"` and `floating-point number "1.5"`. A string literal is
    /// named `single-quoted string "x"` or `double-quoted string "x"` by PHP; this lexer keeps
    /// only the decoded contents, so the quote style is dropped.
    pub(crate) fn php_description(&self) -> String {
        match self {
            Self::Ident(name) => {
                if PHP_RESERVED_WORDS.contains(&name.to_ascii_lowercase().as_str()) {
                    format!("token \"{name}\"")
                } else {
                    format!("identifier \"{name}\"")
                }
            }
            Self::ReservedNameHead(name) => format!("token \"{name}\""),
            Self::DollarIdent(name) => format!("variable \"${name}\""),
            Self::Int(value) => format!("integer \"{value}\""),
            Self::Float(value) => format!("floating-point number \"{value}\""),
            Self::String(value) => format!("string \"{value}\""),
            Self::DocComment(_) => "comment".to_string(),
            Self::InlineHtml(_) => "end of file".to_string(),
            Self::Magic(_) => "magic constant".to_string(),
            Self::Eof => "end of file".to_string(),
            other => format!("token \"{}\"", other.php_spelling()),
        }
    }

    /// Returns the PHP source spelling of one fixed-shape token.
    fn php_spelling(&self) -> &'static str {
        match self {
            Self::DollarLBrace => "${",
            Self::Dollar => "$",
            Self::Plus => "+",
            Self::PlusPlus => "++",
            Self::PlusEqual => "+=",
            Self::Minus => "-",
            Self::MinusMinus => "--",
            Self::MinusEqual => "-=",
            Self::Arrow => "->",
            Self::Star => "*",
            Self::StarStar => "**",
            Self::StarStarEqual => "**=",
            Self::StarEqual => "*=",
            Self::Slash => "/",
            Self::SlashEqual => "/=",
            Self::Percent => "%",
            Self::PercentEqual => "%=",
            Self::Ampersand => "&",
            Self::AmpEqual => "&=",
            Self::Pipe => "|",
            Self::PipeEqual => "|=",
            Self::Caret => "^",
            Self::CaretEqual => "^=",
            Self::Tilde => "~",
            Self::At => "@",
            Self::Dot => ".",
            Self::DotEqual => ".=",
            Self::Ellipsis => "...",
            Self::Equal => "=",
            Self::EqualEqual => "==",
            Self::EqualEqualEqual => "===",
            Self::Bang => "!",
            Self::NotEqual => "!=",
            Self::NotEqualEqual => "!==",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Spaceship => "<=>",
            Self::LessLess => "<<",
            Self::LessLessEqual => "<<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::GreaterGreater => ">>",
            Self::GreaterGreaterEqual => ">>=",
            Self::FatArrow => "=>",
            Self::Question => "?",
            Self::QuestionArrow => "?->",
            Self::QuestionQuestion => "??",
            Self::QuestionQuestionEqual => "??=",
            Self::Semicolon => ";",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBracket => "[",
            Self::RBracket => "]",
            Self::LBrace => "{",
            Self::RBrace => "}",
            Self::Comma => ",",
            Self::Colon => ":",
            Self::DoubleColon => "::",
            Self::Backslash => "\\",
            Self::AttributeStart => "#[",
            Self::DollarIdent(_)
            | Self::Ident(_)
            | Self::ReservedNameHead(_)
            | Self::Magic(_)
            | Self::Int(_)
            | Self::Float(_)
            | Self::String(_)
            | Self::DocComment(_)
            | Self::InlineHtml(_)
            | Self::Eof => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies token descriptions reproduce the wordings measured from `php -n` 8.5.6.
    #[test]
    fn php_description_splits_reserved_words_from_identifiers() {
        assert_eq!(
            TokenKind::Ident("bar".to_string()).php_description(),
            "identifier \"bar\""
        );
        assert_eq!(
            TokenKind::Ident("null".to_string()).php_description(),
            "identifier \"null\""
        );
        assert_eq!(
            TokenKind::Ident("return".to_string()).php_description(),
            "token \"return\""
        );
        assert_eq!(
            TokenKind::DollarIdent("b".to_string()).php_description(),
            "variable \"$b\""
        );
        assert_eq!(TokenKind::Int(2).php_description(), "integer \"2\"");
        assert_eq!(
            TokenKind::Float(2.5).php_description(),
            "floating-point number \"2.5\""
        );
        assert_eq!(TokenKind::Semicolon.php_description(), "token \";\"");
        assert_eq!(TokenKind::Arrow.php_description(), "token \"->\"");
        assert_eq!(TokenKind::Eof.php_description(), "end of file");
    }
}
