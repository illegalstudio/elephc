//! Purpose:
//! Projects PHP variables and words to compiler tokens.
//! Applies case-sensitive and keyword-specific token classification to scanned text.
//!
//! Called from:
//! - `crate::syntax::lexer::scan` through `crate::syntax::lexer::literals`.
//!
//! Key details:
//! - PHP-visible spellings and reserved words must remain compatible with the parser's syntax tables.

use super::super::cursor::Cursor;
use super::super::token::Token;
use crate::syntax::errors::CompileError;

/// Returns true if `c` may continue a PHP identifier: an ASCII alphanumeric, `_`, or any
/// non-ASCII character. Matches PHP's `[a-zA-Z0-9_\x80-\xff]*` continuation class.
pub(in crate::syntax::lexer) fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()
}

/// Projects a variable name, with the cursor after the lexeme for diagnostics.
pub(in crate::syntax::lexer) fn variable_token(name: &str, cursor: &Cursor) -> Result<Token, CompileError> {
    if name.is_empty() {
        // `$$name` / `${expr}` are variable variables: the variable name is computed at runtime.
        // elephc allocates locals to fixed compile-time slots and has no per-frame variable-name
        // table, so variable variables cannot be supported in the closed-world model.
        if matches!(cursor.peek(), Some('$') | Some('{')) {
            return Err(CompileError::new(
                cursor.span(),
                "Variable variables (`$$name`) are not supported: variable names must be known at compile time",
            ));
        }
        return Err(CompileError::new(cursor.span(), "Expected variable name after '$'"));
    }

    if name == "this" {
        return Ok(Token::This);
    }

    Ok(Token::Variable(name.to_owned()))
}

/// Longest word that can classify as a magic constant, built-in constant, or keyword.
const MAX_WORD_LEN: usize = 19;

/// Classifies a scanned word, including constants and compiler-specific keywords.
pub(in crate::syntax::lexer) fn word_token(word: &str) -> Token {
    match word {
        "INF" => return Token::Inf,
        "NAN" => return Token::Nan,
        "PHP_INT_MAX" => return Token::PhpIntMax,
        "PHP_INT_MIN" => return Token::PhpIntMin,
        "PHP_FLOAT_MAX" => return Token::PhpFloatMax,
        "M_PI" => return Token::MPi,
        "M_E" => return Token::ME,
        "M_SQRT2" => return Token::MSqrt2,
        "M_PI_2" => return Token::MPi2,
        "M_PI_4" => return Token::MPi4,
        "M_LOG2E" => return Token::MLog2e,
        "M_LOG10E" => return Token::MLog10e,
        "PHP_FLOAT_MIN" => return Token::PhpFloatMin,
        "PHP_FLOAT_EPSILON" => return Token::PhpFloatEpsilon,
        "STDIN" => return Token::Stdin,
        "STDOUT" => return Token::Stdout,
        "STDERR" => return Token::Stderr,
        "PHP_EOL" => return Token::PhpEol,
        "PHP_OS" => return Token::PhpOs,
        "DIRECTORY_SEPARATOR" => return Token::DirectorySeparator,
        _ => {}
    }

    if word.len() > MAX_WORD_LEN {
        return Token::Identifier(word.to_owned());
    }
    let mut lower = [0u8; MAX_WORD_LEN];
    let lower = &mut lower[..word.len()];
    for (destination, source) in lower.iter_mut().zip(word.as_bytes()) {
        *destination = source.to_ascii_lowercase();
    }
    match &*lower {
        b"__dir__" => Token::DunderDir,
        b"__file__" => Token::DunderFile,
        b"__line__" => Token::DunderLine,
        b"__function__" => Token::DunderFunction,
        b"__class__" => Token::DunderClass,
        b"__method__" => Token::DunderMethod,
        b"__namespace__" => Token::DunderNamespace,
        b"__trait__" => Token::DunderTrait,
        b"and" => Token::And,
        b"or" => Token::Or,
        b"xor" => Token::Xor,
        b"instanceof" => Token::InstanceOf,
        b"echo" => Token::Echo,
        b"if" => Token::If,
        b"ifdef" => Token::IfDef,
        b"else" => Token::Else,
        b"elseif" => Token::ElseIf,
        b"while" => Token::While,
        b"for" => Token::For,
        b"break" => Token::Break,
        b"continue" => Token::Continue,
        b"function" => Token::Function,
        b"return" => Token::Return,
        b"yield" => Token::Yield,
        b"true" => Token::True,
        b"false" => Token::False,
        b"null" => Token::Null,
        b"do" => Token::Do,
        b"foreach" => Token::Foreach,
        b"as" => Token::As,
        b"try" => Token::Try,
        b"catch" => Token::Catch,
        b"finally" => Token::Finally,
        b"throw" => Token::Throw,
        b"extends" => Token::Extends,
        b"implements" => Token::Implements,
        b"interface" => Token::Interface,
        b"abstract" => Token::Abstract,
        b"final" => Token::Final,
        b"print" => Token::Print,
        b"switch" => Token::Switch,
        b"case" => Token::Case,
        b"default" => Token::Default,
        b"match" => Token::Match,
        b"include" => Token::Include,
        b"include_once" => Token::IncludeOnce,
        b"require" => Token::Require,
        b"require_once" => Token::RequireOnce,
        b"fn" => Token::Fn,
        b"use" => Token::Use,
        b"namespace" => Token::Namespace,
        b"const" => Token::Const,
        b"global" => Token::Global,
        b"declare" => Token::Declare,
        b"enddeclare" => Token::EndDeclare,
        b"endif" => Token::EndIf,
        b"endwhile" => Token::EndWhile,
        b"endfor" => Token::EndFor,
        b"endforeach" => Token::EndForeach,
        b"endswitch" => Token::EndSwitch,
        b"goto" => Token::Goto,
        b"static" => Token::Static,
        b"self" => Token::Self_,
        b"trait" => Token::Trait,
        b"parent" => Token::Parent,
        b"insteadof" => Token::InsteadOf,
        b"class" => Token::Class,
        b"enum" => Token::Enum,
        b"new" => Token::New,
        b"clone" => Token::Clone,
        b"public" => Token::Public,
        b"protected" => Token::Protected,
        b"private" => Token::Private,
        b"readonly" => Token::ReadOnly,
        b"extern" => Token::Extern,
        b"packed" => Token::Packed,
        _ => Token::Identifier(word.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies keywords and magic constants keep PHP's case-insensitive classification and
    /// that words longer than the classification vocabulary fall back to identifiers.
    #[test]
    fn word_token_classifies_case_insensitively() {
        assert_eq!(word_token("CLASS"), Token::Class);
        assert_eq!(word_token("__DIR__"), Token::DunderDir);
        let long = "x".repeat(64);
        assert_eq!(word_token(&long), Token::Identifier(long));
    }
}
