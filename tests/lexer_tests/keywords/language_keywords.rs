//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of keywords, including control token, exception keywords tokens, and ifdef keyword token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

#[test]
fn test_error_control_token() {
    let t = tokens("<?php @file_get_contents(\"missing.txt\");");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::At,
            Token::Identifier("file_get_contents".into()),
            Token::LParen,
            Token::StringLiteral("missing.txt".into()),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Basic tokens ---

#[test]
fn test_exception_keywords_tokens() {
    let t = tokens("<?php try { throw $e; } catch (Exception $e) { } finally { }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Try,
            Token::LBrace,
            Token::Throw,
            Token::Variable("e".into()),
            Token::Semicolon,
            Token::RBrace,
            Token::Catch,
            Token::LParen,
            Token::Identifier("Exception".into()),
            Token::Variable("e".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Finally,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_ifdef_keyword_token() {
    let t = tokens("<?php ifdef DEBUG { echo 1; }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::IfDef,
            Token::Identifier("DEBUG".into()),
            Token::LBrace,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_namespace_and_backslash_tokens() {
    let t = tokens("<?php namespace Foo\\Bar; use Baz\\Qux;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Namespace,
            Token::Identifier("Foo".into()),
            Token::Backslash,
            Token::Identifier("Bar".into()),
            Token::Semicolon,
            Token::Use,
            Token::Identifier("Baz".into()),
            Token::Backslash,
            Token::Identifier("Qux".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_enum_tokens() {
    let t = tokens("<?php enum Color: int { case Red = 1; case Green = 2; }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Enum,
            Token::Identifier("Color".into()),
            Token::Colon,
            Token::Identifier("int".into()),
            Token::LBrace,
            Token::Case,
            Token::Identifier("Red".into()),
            Token::Assign,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Case,
            Token::Identifier("Green".into()),
            Token::Assign,
            Token::IntLiteral(2),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_instanceof_keyword_is_case_insensitive() {
    let t = tokens("<?php instanceof INSTANCEOF InstanceOf");
    assert_eq!(t[1..4], [Token::InstanceOf, Token::InstanceOf, Token::InstanceOf]);
}

#[test]
fn test_php_language_keywords_are_case_insensitive() {
    let t = tokens("<?php IF (TRUE) { ECHO NULL; } ELSEIF (FALSE) { PRINT 1; }");
    assert_eq!(t[1], Token::If);
    assert_eq!(t[3], Token::True);
    assert_eq!(t[6], Token::Echo);
    assert_eq!(t[7], Token::Null);
    assert_eq!(t[10], Token::ElseIf);
    assert_eq!(t[12], Token::False);
    assert_eq!(t[15], Token::Print);
}

#[test]
fn test_php_constant_tokens_remain_case_sensitive() {
    let t = tokens("<?php PHP_OS php_os INF inf STDOUT stdout");
    assert_eq!(t[1], Token::PhpOs);
    assert_eq!(t[2], Token::Identifier("php_os".into()));
    assert_eq!(t[3], Token::Inf);
    assert_eq!(t[4], Token::Identifier("inf".into()));
    assert_eq!(t[5], Token::Stdout);
    assert_eq!(t[6], Token::Identifier("stdout".into()));
}

#[test]
fn test_boolean_keywords() {
    let t = tokens("<?php true false");
    assert_eq!(t[1..3], [Token::True, Token::False]);
}

#[test]
fn test_keyword_echo() {
    assert_eq!(tokens("<?php echo")[1], Token::Echo);
}

#[test]
fn test_keyword_if() {
    assert_eq!(tokens("<?php if")[1], Token::If);
}

#[test]
fn test_keyword_else() {
    assert_eq!(tokens("<?php else")[1], Token::Else);
}

#[test]
fn test_keyword_elseif() {
    assert_eq!(tokens("<?php elseif")[1], Token::ElseIf);
}

#[test]
fn test_keyword_while() {
    assert_eq!(tokens("<?php while")[1], Token::While);
}

#[test]
fn test_keyword_for() {
    assert_eq!(tokens("<?php for")[1], Token::For);
}

#[test]
fn test_keyword_do() {
    assert_eq!(tokens("<?php do")[1], Token::Do);
}

#[test]
fn test_keyword_foreach() {
    assert_eq!(tokens("<?php foreach")[1], Token::Foreach);
}

#[test]
fn test_keyword_as() {
    assert_eq!(tokens("<?php as")[1], Token::As);
}

#[test]
fn test_keyword_break() {
    assert_eq!(tokens("<?php break")[1], Token::Break);
}

#[test]
fn test_keyword_continue() {
    assert_eq!(tokens("<?php continue")[1], Token::Continue);
}

#[test]
fn test_keyword_function() {
    assert_eq!(tokens("<?php function")[1], Token::Function);
}

#[test]
fn test_keyword_return() {
    assert_eq!(tokens("<?php return")[1], Token::Return);
}

#[test]
fn test_function_declaration_tokens() {
    let t = tokens("<?php function add($a, $b) { return $a; }");
    assert_eq!(t[1], Token::Function);
    assert_eq!(t[2], Token::Identifier("add".into()));
    assert_eq!(t[3], Token::LParen);
    assert_eq!(t[4], Token::Variable("a".into()));
    assert_eq!(t[5], Token::Comma);
    assert_eq!(t[6], Token::Variable("b".into()));
    assert_eq!(t[7], Token::RParen);
    assert_eq!(t[8], Token::LBrace);
    assert_eq!(t[9], Token::Return);
}

// --- Error cases ---

#[test]
fn test_print_keyword() {
    let t = tokens("<?php print \"hello\";");
    assert_eq!(t[1], Token::Print);
}

#[test]
fn test_switch_token() {
    let t = tokens("<?php switch ($x) {}");
    assert_eq!(t[1], Token::Switch);
}

#[test]
fn test_case_token() {
    let t = tokens("<?php case 1:");
    assert_eq!(t[1], Token::Case);
}

#[test]
fn test_default_token() {
    let t = tokens("<?php default:");
    assert_eq!(t[1], Token::Default);
}

#[test]
fn test_match_token() {
    let t = tokens("<?php match($x) {}");
    assert_eq!(t[1], Token::Match);
}

#[test]
fn test_fn_token() {
    let t = tokens("<?php fn($x) => $x;");
    assert_eq!(t[1], Token::Fn);
}

#[test]
fn test_use_token() {
    let t = tokens("<?php use;");
    assert_eq!(t[1], Token::Use);
}

#[test]
fn test_function_token_anonymous() {
    let t = tokens("<?php function($x) {}");
    assert_eq!(t[1], Token::Function);
    assert_eq!(t[2], Token::LParen);
}

// --- Bitwise operator tokens ---

#[test]
fn test_const_keyword() {
    let t = tokens("<?php const MAX = 100;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Const,
            Token::Identifier("MAX".into()),
            Token::Assign,
            Token::IntLiteral(100),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_global_keyword() {
    let t = tokens("<?php global $x;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Global,
            Token::Variable("x".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_static_keyword() {
    let t = tokens("<?php static $x = 0;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Static,
            Token::Variable("x".into()),
            Token::Assign,
            Token::IntLiteral(0),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Reference parameter ---

#[test]
fn test_lex_extern_keyword() {
    let t = tokens("<?php extern function abs(int $n): int;");
    assert!(t.contains(&Token::Extern));
    assert!(t.contains(&Token::Function));
}

#[test]
fn test_nullable_and_union_type_tokens() {
    let t = tokens("<?php ?int|string $value = null;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Question,
            Token::Identifier("int".into()),
            Token::Pipe,
            Token::Identifier("string".into()),
            Token::Variable("value".into()),
            Token::Assign,
            Token::Null,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Magic constants ---
