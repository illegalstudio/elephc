//! Purpose:
//! Maps a token that may legally appear as a bareword name to its source-text spelling.
//! Centralizes the PHP "semi-reserved" rule: identifiers and (nearly) all keywords are
//! valid as named-argument labels, `->`/`?->`/`::` member names, and method/constant names.
//!
//! Called from:
//! - `crate::parser::expr` (named arguments, `->`/`?->` members, `::` scoped access)
//! - `crate::parser::stmt::oop` (method and class-constant declaration names)
//!
//! Key details:
//! - Returns the exact source lexeme so the resulting name matches what PHP would record.
//! - Callers that need PHP's narrow exceptions (e.g. `class` is reserved as a constant name,
//!   and `Foo::class` is the special class-name fetch) must special-case those tokens first.

use crate::lexer::Token;

/// Returns the source-text spelling of a token that can be used as a bareword name, or `None`
/// for tokens (operators, punctuation, literals) that cannot.
///
/// Accepts plain identifiers plus every reserved keyword and magic-constant token, matching
/// PHP 8's semi-reserved-word rule where keywords are permitted as member, method, constant,
/// and named-argument names. Callers requiring PHP's exceptions (`class` as a constant name,
/// the `Foo::class` fetch) must handle those tokens before consulting this helper.
pub(crate) fn bareword_name_from_token(token: &Token) -> Option<String> {
    match token {
        Token::Identifier(name) => Some(name.clone()),
        Token::Echo => Some("echo".to_string()),
        Token::If => Some("if".to_string()),
        Token::IfDef => Some("ifdef".to_string()),
        Token::Else => Some("else".to_string()),
        Token::ElseIf => Some("elseif".to_string()),
        Token::While => Some("while".to_string()),
        Token::For => Some("for".to_string()),
        Token::Break => Some("break".to_string()),
        Token::Continue => Some("continue".to_string()),
        Token::Function => Some("function".to_string()),
        Token::Return => Some("return".to_string()),
        Token::True => Some("true".to_string()),
        Token::False => Some("false".to_string()),
        Token::Null => Some("null".to_string()),
        Token::Do => Some("do".to_string()),
        Token::Foreach => Some("foreach".to_string()),
        Token::As => Some("as".to_string()),
        Token::Try => Some("try".to_string()),
        Token::Catch => Some("catch".to_string()),
        Token::Finally => Some("finally".to_string()),
        Token::Throw => Some("throw".to_string()),
        Token::Extends => Some("extends".to_string()),
        Token::Implements => Some("implements".to_string()),
        Token::Interface => Some("interface".to_string()),
        Token::Abstract => Some("abstract".to_string()),
        Token::Final => Some("final".to_string()),
        Token::Print => Some("print".to_string()),
        Token::Switch => Some("switch".to_string()),
        Token::Case => Some("case".to_string()),
        Token::Default => Some("default".to_string()),
        Token::Match => Some("match".to_string()),
        Token::Include => Some("include".to_string()),
        Token::IncludeOnce => Some("include_once".to_string()),
        Token::Require => Some("require".to_string()),
        Token::RequireOnce => Some("require_once".to_string()),
        Token::Fn => Some("fn".to_string()),
        Token::Use => Some("use".to_string()),
        Token::Namespace => Some("namespace".to_string()),
        Token::Const => Some("const".to_string()),
        Token::Global => Some("global".to_string()),
        Token::Static => Some("static".to_string()),
        Token::Self_ => Some("self".to_string()),
        Token::Trait => Some("trait".to_string()),
        Token::Parent => Some("parent".to_string()),
        Token::InsteadOf => Some("insteadof".to_string()),
        Token::Class => Some("class".to_string()),
        Token::Enum => Some("enum".to_string()),
        Token::New => Some("new".to_string()),
        Token::Public => Some("public".to_string()),
        Token::Protected => Some("protected".to_string()),
        Token::Private => Some("private".to_string()),
        Token::ReadOnly => Some("readonly".to_string()),
        Token::Extern => Some("extern".to_string()),
        Token::Packed => Some("packed".to_string()),
        Token::Yield => Some("yield".to_string()),
        Token::And => Some("and".to_string()),
        Token::Or => Some("or".to_string()),
        Token::Xor => Some("xor".to_string()),
        Token::InstanceOf => Some("instanceof".to_string()),
        Token::Inf => Some("INF".to_string()),
        Token::Nan => Some("NAN".to_string()),
        Token::PhpIntMax => Some("PHP_INT_MAX".to_string()),
        Token::PhpIntMin => Some("PHP_INT_MIN".to_string()),
        Token::PhpFloatMax => Some("PHP_FLOAT_MAX".to_string()),
        Token::MPi => Some("M_PI".to_string()),
        Token::ME => Some("M_E".to_string()),
        Token::MSqrt2 => Some("M_SQRT2".to_string()),
        Token::MPi2 => Some("M_PI_2".to_string()),
        Token::MPi4 => Some("M_PI_4".to_string()),
        Token::MLog2e => Some("M_LOG2E".to_string()),
        Token::MLog10e => Some("M_LOG10E".to_string()),
        Token::PhpFloatMin => Some("PHP_FLOAT_MIN".to_string()),
        Token::PhpFloatEpsilon => Some("PHP_FLOAT_EPSILON".to_string()),
        Token::Stdin => Some("STDIN".to_string()),
        Token::Stdout => Some("STDOUT".to_string()),
        Token::Stderr => Some("STDERR".to_string()),
        Token::PhpEol => Some("PHP_EOL".to_string()),
        Token::PhpOs => Some("PHP_OS".to_string()),
        Token::DirectorySeparator => Some("DIRECTORY_SEPARATOR".to_string()),
        Token::DunderDir => Some("__DIR__".to_string()),
        Token::DunderFile => Some("__FILE__".to_string()),
        Token::DunderLine => Some("__LINE__".to_string()),
        Token::DunderFunction => Some("__FUNCTION__".to_string()),
        Token::DunderClass => Some("__CLASS__".to_string()),
        Token::DunderMethod => Some("__METHOD__".to_string()),
        Token::DunderNamespace => Some("__NAMESPACE__".to_string()),
        Token::DunderTrait => Some("__TRAIT__".to_string()),
        _ => None,
    }
}
