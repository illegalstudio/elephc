//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of keywords, including control token, exception keywords tokens, and ifdef keyword token.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `@` error-control operator tokenizes correctly before a function call.
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

/// Verifies `try/catch/finally` exception-handling keywords tokenize correctly.
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

/// Verifies `ifdef DEBUG` compiler conditional tokenizes as `IfDef` + identifier.
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

/// Verifies `namespace Foo\Bar;` and `use Baz\Qux;` token sequences.
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

/// Verifies `enum Color: int { case Red; }` token sequence.
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

/// Verifies `instanceof` is case-insensitive (PHP keyword).
#[test]
fn test_instanceof_keyword_is_case_insensitive() {
    let t = tokens("<?php instanceof INSTANCEOF InstanceOf");
    assert_eq!(t[1..4], [Token::InstanceOf, Token::InstanceOf, Token::InstanceOf]);
}

/// Verifies control-flow keywords (`if`, `else`, `elseif`, `while`, `for`, etc.)
/// are case-insensitive.
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

/// Verifies runtime constants (`PHP_OS`, `INF`, `STDOUT`) are case-sensitive;
/// unknown identifiers remain plain `Identifier`.
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

/// Verifies `true` and `false` tokenize as boolean literals.
#[test]
fn test_boolean_keywords() {
    let t = tokens("<?php true false");
    assert_eq!(t[1..3], [Token::True, Token::False]);
}

/// Verifies `echo` keyword tokenizes as `Echo`.
#[test]
fn test_keyword_echo() {
    assert_eq!(tokens("<?php echo")[1], Token::Echo);
}

/// Verifies `if` keyword tokenizes as `If`.
#[test]
fn test_keyword_if() {
    assert_eq!(tokens("<?php if")[1], Token::If);
}

/// Verifies `else` keyword tokenizes as `Else`.
#[test]
fn test_keyword_else() {
    assert_eq!(tokens("<?php else")[1], Token::Else);
}

/// Verifies `elseif` keyword tokenizes as `ElseIf`.
#[test]
fn test_keyword_elseif() {
    assert_eq!(tokens("<?php elseif")[1], Token::ElseIf);
}

/// Verifies `while` keyword tokenizes as `While`.
#[test]
fn test_keyword_while() {
    assert_eq!(tokens("<?php while")[1], Token::While);
}

/// Verifies `for` keyword tokenizes as `For`.
#[test]
fn test_keyword_for() {
    assert_eq!(tokens("<?php for")[1], Token::For);
}

/// Verifies `do` keyword tokenizes as `Do`.
#[test]
fn test_keyword_do() {
    assert_eq!(tokens("<?php do")[1], Token::Do);
}

/// Verifies `foreach` keyword tokenizes as `Foreach`.
#[test]
fn test_keyword_foreach() {
    assert_eq!(tokens("<?php foreach")[1], Token::Foreach);
}

/// Verifies `as` keyword tokenizes as `As`.
#[test]
fn test_keyword_as() {
    assert_eq!(tokens("<?php as")[1], Token::As);
}

/// Verifies `break` keyword tokenizes as `Break`.
#[test]
fn test_keyword_break() {
    assert_eq!(tokens("<?php break")[1], Token::Break);
}

/// Verifies `continue` keyword tokenizes as `Continue`.
#[test]
fn test_keyword_continue() {
    assert_eq!(tokens("<?php continue")[1], Token::Continue);
}

/// Verifies `function` keyword tokenizes as `Function`.
#[test]
fn test_keyword_function() {
    assert_eq!(tokens("<?php function")[1], Token::Function);
}

/// Verifies `return` keyword tokenizes as `Return`.
#[test]
fn test_keyword_return() {
    assert_eq!(tokens("<?php return")[1], Token::Return);
}

/// Verifies `function add($a, $b) { return $a; }` declaration tokenizes correctly.
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

/// Verifies `print` keyword tokenizes as `Print`.
#[test]
fn test_print_keyword() {
    let t = tokens("<?php print \"hello\";");
    assert_eq!(t[1], Token::Print);
}

/// Verifies `switch` keyword tokenizes as `Switch`.
#[test]
fn test_switch_token() {
    let t = tokens("<?php switch ($x) {}");
    assert_eq!(t[1], Token::Switch);
}

/// Verifies `case` keyword tokenizes as `Case`.
#[test]
fn test_case_token() {
    let t = tokens("<?php case 1:");
    assert_eq!(t[1], Token::Case);
}

/// Verifies `default` keyword tokenizes as `Default`.
#[test]
fn test_default_token() {
    let t = tokens("<?php default:");
    assert_eq!(t[1], Token::Default);
}

/// Verifies `match` keyword tokenizes as `Match`.
#[test]
fn test_match_token() {
    let t = tokens("<?php match($x) {}");
    assert_eq!(t[1], Token::Match);
}

/// Verifies `fn($x) => $x` arrow function tokenizes as `Fn`.
#[test]
fn test_fn_token() {
    let t = tokens("<?php fn($x) => $x;");
    assert_eq!(t[1], Token::Fn);
}

/// Verifies `use` keyword tokenizes as `Use`.
#[test]
fn test_use_token() {
    let t = tokens("<?php use;");
    assert_eq!(t[1], Token::Use);
}

/// Verifies anonymous `function($x) {}` tokenizes with `Function` + `LParen`.
#[test]
fn test_function_token_anonymous() {
    let t = tokens("<?php function($x) {}");
    assert_eq!(t[1], Token::Function);
    assert_eq!(t[2], Token::LParen);
}

// --- Bitwise operator tokens ---

/// Verifies `const MAX = 100;` token sequence includes `Const`.
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

/// Verifies `global $x;` token sequence includes `Global`.
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

/// Verifies `static $x = 0;` token sequence includes `Static`.
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

/// Verifies `declare(strict_types=1);` token sequence includes `Declare`.
#[test]
fn test_declare_keyword() {
    let t = tokens("<?php declare(strict_types=1);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Declare,
            Token::LParen,
            Token::Identifier("strict_types".into()),
            Token::Assign,
            Token::IntLiteral(1),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies the alternative `declare` syntax recognizes `enddeclare` as its closing keyword.
#[test]
fn test_enddeclare_keyword() {
    let t = tokens("<?php declare(ticks=1): echo 1; enddeclare;");
    assert!(t.contains(&Token::Declare));
    assert!(t.contains(&Token::Colon));
    assert!(t.contains(&Token::EndDeclare));
}

// --- Reference parameter ---

/// Verifies `extern` compiler extension keyword tokenizes alongside `function`.
#[test]
fn test_lex_extern_keyword() {
    let t = tokens("<?php extern function abs(int $n): int;");
    assert!(t.contains(&Token::Extern));
    assert!(t.contains(&Token::Function));
}

/// Verifies nullable (`?int`) and union (`int|string`) type tokens in parameter position.
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
