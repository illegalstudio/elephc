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

/// Classifies a scanned word, including constants and compiler-specific keywords.
pub(in crate::syntax::lexer) fn word_token(word: &str) -> Token {
    if word.eq_ignore_ascii_case("__DIR__") {
        return Token::DunderDir;
    }
    if word.eq_ignore_ascii_case("__FILE__") {
        return Token::DunderFile;
    }
    if word.eq_ignore_ascii_case("__LINE__") {
        return Token::DunderLine;
    }
    if word.eq_ignore_ascii_case("__FUNCTION__") {
        return Token::DunderFunction;
    }
    if word.eq_ignore_ascii_case("__CLASS__") {
        return Token::DunderClass;
    }
    if word.eq_ignore_ascii_case("__METHOD__") {
        return Token::DunderMethod;
    }
    if word.eq_ignore_ascii_case("__NAMESPACE__") {
        return Token::DunderNamespace;
    }
    if word.eq_ignore_ascii_case("__TRAIT__") {
        return Token::DunderTrait;
    }

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

    match word.to_ascii_lowercase().as_str() {
        "and" => Token::And,
        "or" => Token::Or,
        "xor" => Token::Xor,
        "instanceof" => Token::InstanceOf,
        "echo" => Token::Echo,
        "if" => Token::If,
        "ifdef" => Token::IfDef,
        "else" => Token::Else,
        "elseif" => Token::ElseIf,
        "while" => Token::While,
        "for" => Token::For,
        "break" => Token::Break,
        "continue" => Token::Continue,
        "function" => Token::Function,
        "return" => Token::Return,
        "yield" => Token::Yield,
        "true" => Token::True,
        "false" => Token::False,
        "null" => Token::Null,
        "do" => Token::Do,
        "foreach" => Token::Foreach,
        "as" => Token::As,
        "try" => Token::Try,
        "catch" => Token::Catch,
        "finally" => Token::Finally,
        "throw" => Token::Throw,
        "extends" => Token::Extends,
        "implements" => Token::Implements,
        "interface" => Token::Interface,
        "abstract" => Token::Abstract,
        "final" => Token::Final,
        "print" => Token::Print,
        "switch" => Token::Switch,
        "case" => Token::Case,
        "default" => Token::Default,
        "match" => Token::Match,
        "include" => Token::Include,
        "include_once" => Token::IncludeOnce,
        "require" => Token::Require,
        "require_once" => Token::RequireOnce,
        "fn" => Token::Fn,
        "use" => Token::Use,
        "namespace" => Token::Namespace,
        "const" => Token::Const,
        "global" => Token::Global,
        "declare" => Token::Declare,
        "enddeclare" => Token::EndDeclare,
        "endif" => Token::EndIf,
        "endwhile" => Token::EndWhile,
        "endfor" => Token::EndFor,
        "endforeach" => Token::EndForeach,
        "endswitch" => Token::EndSwitch,
        "goto" => Token::Goto,
        "static" => Token::Static,
        "self" => Token::Self_,
        "trait" => Token::Trait,
        "parent" => Token::Parent,
        "insteadof" => Token::InsteadOf,
        "class" => Token::Class,
        "enum" => Token::Enum,
        "new" => Token::New,
        "clone" => Token::Clone,
        "public" => Token::Public,
        "protected" => Token::Protected,
        "private" => Token::Private,
        "readonly" => Token::ReadOnly,
        "extern" => Token::Extern,
        "packed" => Token::Packed,
        _ => Token::Identifier(word.to_owned()),
    }
}
