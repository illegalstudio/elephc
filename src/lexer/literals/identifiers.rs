//! Purpose:
//! Scans PHP variables, identifiers, keywords, constants, and compiler-extension words.
//! Applies case-sensitive and keyword-specific token classification after collecting text.
//!
//! Called from:
//! - `crate::lexer::scan` through `crate::lexer::literals`.
//!
//! Key details:
//! - PHP-visible spellings and reserved words must remain compatible with the parser's syntax tables.

use super::super::cursor::Cursor;
use super::super::token::Token;
use crate::errors::CompileError;

pub(in crate::lexer) fn scan_variable(cursor: &mut Cursor) -> Result<Token, CompileError> {
    cursor.advance();
    let mut name = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    if name.is_empty() {
        return Err(CompileError::new(cursor.span(), "Expected variable name after '$'"));
    }

    if name == "this" {
        return Ok(Token::This);
    }

    Ok(Token::Variable(name))
}

pub(in crate::lexer) fn scan_keyword(cursor: &mut Cursor) -> Result<Token, CompileError> {
    let mut word = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    if word.eq_ignore_ascii_case("__DIR__") {
        return Ok(Token::DunderDir);
    }
    if word.eq_ignore_ascii_case("__FILE__") {
        return Ok(Token::DunderFile);
    }
    if word.eq_ignore_ascii_case("__LINE__") {
        return Ok(Token::DunderLine);
    }
    if word.eq_ignore_ascii_case("__FUNCTION__") {
        return Ok(Token::DunderFunction);
    }
    if word.eq_ignore_ascii_case("__CLASS__") {
        return Ok(Token::DunderClass);
    }
    if word.eq_ignore_ascii_case("__METHOD__") {
        return Ok(Token::DunderMethod);
    }
    if word.eq_ignore_ascii_case("__NAMESPACE__") {
        return Ok(Token::DunderNamespace);
    }
    if word.eq_ignore_ascii_case("__TRAIT__") {
        return Ok(Token::DunderTrait);
    }

    match word.as_str() {
        "INF" => return Ok(Token::Inf),
        "NAN" => return Ok(Token::Nan),
        "PHP_INT_MAX" => return Ok(Token::PhpIntMax),
        "PHP_INT_MIN" => return Ok(Token::PhpIntMin),
        "PHP_FLOAT_MAX" => return Ok(Token::PhpFloatMax),
        "M_PI" => return Ok(Token::MPi),
        "M_E" => return Ok(Token::ME),
        "M_SQRT2" => return Ok(Token::MSqrt2),
        "M_PI_2" => return Ok(Token::MPi2),
        "M_PI_4" => return Ok(Token::MPi4),
        "M_LOG2E" => return Ok(Token::MLog2e),
        "M_LOG10E" => return Ok(Token::MLog10e),
        "PHP_FLOAT_MIN" => return Ok(Token::PhpFloatMin),
        "PHP_FLOAT_EPSILON" => return Ok(Token::PhpFloatEpsilon),
        "STDIN" => return Ok(Token::Stdin),
        "STDOUT" => return Ok(Token::Stdout),
        "STDERR" => return Ok(Token::Stderr),
        "PHP_EOL" => return Ok(Token::PhpEol),
        "PHP_OS" => return Ok(Token::PhpOs),
        "DIRECTORY_SEPARATOR" => return Ok(Token::DirectorySeparator),
        _ => {}
    }

    match word.to_ascii_lowercase().as_str() {
        "and" => Ok(Token::And),
        "or" => Ok(Token::Or),
        "xor" => Ok(Token::Xor),
        "instanceof" => Ok(Token::InstanceOf),
        "echo" => Ok(Token::Echo),
        "if" => Ok(Token::If),
        "ifdef" => Ok(Token::IfDef),
        "else" => Ok(Token::Else),
        "elseif" => Ok(Token::ElseIf),
        "while" => Ok(Token::While),
        "for" => Ok(Token::For),
        "break" => Ok(Token::Break),
        "continue" => Ok(Token::Continue),
        "function" => Ok(Token::Function),
        "return" => Ok(Token::Return),
        "yield" => Ok(Token::Yield),
        "true" => Ok(Token::True),
        "false" => Ok(Token::False),
        "null" => Ok(Token::Null),
        "do" => Ok(Token::Do),
        "foreach" => Ok(Token::Foreach),
        "as" => Ok(Token::As),
        "try" => Ok(Token::Try),
        "catch" => Ok(Token::Catch),
        "finally" => Ok(Token::Finally),
        "throw" => Ok(Token::Throw),
        "extends" => Ok(Token::Extends),
        "implements" => Ok(Token::Implements),
        "interface" => Ok(Token::Interface),
        "abstract" => Ok(Token::Abstract),
        "final" => Ok(Token::Final),
        "print" => Ok(Token::Print),
        "switch" => Ok(Token::Switch),
        "case" => Ok(Token::Case),
        "default" => Ok(Token::Default),
        "match" => Ok(Token::Match),
        "include" => Ok(Token::Include),
        "include_once" => Ok(Token::IncludeOnce),
        "require" => Ok(Token::Require),
        "require_once" => Ok(Token::RequireOnce),
        "fn" => Ok(Token::Fn),
        "use" => Ok(Token::Use),
        "namespace" => Ok(Token::Namespace),
        "const" => Ok(Token::Const),
        "global" => Ok(Token::Global),
        "static" => Ok(Token::Static),
        "self" => Ok(Token::Self_),
        "trait" => Ok(Token::Trait),
        "parent" => Ok(Token::Parent),
        "insteadof" => Ok(Token::InsteadOf),
        "class" => Ok(Token::Class),
        "enum" => Ok(Token::Enum),
        "new" => Ok(Token::New),
        "public" => Ok(Token::Public),
        "protected" => Ok(Token::Protected),
        "private" => Ok(Token::Private),
        "readonly" => Ok(Token::ReadOnly),
        "extern" => Ok(Token::Extern),
        "packed" => Ok(Token::Packed),
        _ => Ok(Token::Identifier(word)),
    }
}
