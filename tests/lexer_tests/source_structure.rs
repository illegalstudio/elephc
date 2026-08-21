//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of PHP source structure, including open tag, line comment, and block comment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;
use elephc::lexer::tokenize_bytes_with_mode;
use elephc::source::SourceMode;

/// Verifies `<?php` produces `OpenTag` and EOF, the bare minimum valid PHP script.
#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies PHP's long opening tag is ASCII case-insensitive, including in required files.
#[test]
fn test_open_tag_is_ascii_case_insensitive() {
    for source in ["<?PHP", "<?Php echo 1;", "<?pHp\necho 1;"] {
        assert_eq!(tokens(source).first(), Some(&Token::OpenTag));
    }
}

/// Verifies a leading UTF-8 BOM (U+FEFF) before `<?php` is stripped, so files saved by
/// editors that emit BOM-prefixed UTF-8 still tokenize starting at `OpenTag`.
#[test]
fn test_utf8_bom_before_open_tag_is_stripped() {
    let t = tokens("\u{feff}<?php echo \"hi\";");
    assert_eq!(t[0], Token::OpenTag);
    assert_eq!(t[1], Token::Echo);
}

/// Verifies `// ...` line comments are consumed and do not appear in the token stream.
#[test]
fn test_line_comment() {
    let t = tokens("<?php // this is a comment\necho \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

/// Verifies `/* ... */` block comments are consumed and do not appear in the token stream.
#[test]
fn test_block_comment() {
    let t = tokens("<?php /* block */ echo \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

/// Verifies consecutive comments (block and line) are all skipped correctly.
#[test]
fn test_consecutive_comments() {
    let t = tokens("<?php /* a *//* b */// c\necho \"ok\";");
    assert_eq!(t[1], Token::Echo);
}

// --- Complex tokens ---

/// Verifies missing `<?php` open tag produces a lex error.
#[test]
fn test_missing_open_tag() {
    assert!(tokenize("echo \"hi\";").is_err());
}

/// Verifies an unterminated double-quoted string produces a lex error.
#[test]
fn test_unterminated_string() {
    assert!(tokenize("<?php \"no closing").is_err());
}

// --- Spans ---

/// Verifies line tracking: `echo` on line 2 reports line=2, col=1.
#[test]
fn test_span_tracking() {
    let spanned = tokenize("<?php\necho \"hi\";").unwrap();
    let echo_span = spanned[1].1.span;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}

/// Verifies multiline sources report the correct line number for the last token.
#[test]
fn test_span_multiline() {
    let spanned = tokenize("<?php\n\n\n$x").unwrap();
    let var_span = spanned[1].1.span;
    assert_eq!(var_span.line, 4);
}

// --- Strict comparison ---

/// Verifies trailing space after `<?php` still produces only `OpenTag` + `Eof`.
#[test]
fn test_empty_after_open_tag() {
    let t = tokens("<?php ");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies `<?php` with no trailing whitespace produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_no_trailing_space() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies `<?php\n` (newline only after open tag) produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_newline_only() {
    let t = tokens("<?php\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a line comment after open tag with no trailing code produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_with_comment_no_code() {
    let t = tokens("<?php // nothing here\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a block comment after open tag with no trailing code produces `OpenTag` + `Eof`.
#[test]
fn test_open_tag_with_block_comment_no_code() {
    let t = tokens("<?php /* empty */");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a terminal PHP closing tag supplies PHP's implicit statement terminator.
#[test]
fn test_terminal_closing_tag() {
    let t = tokens("<?php echo 1 ?>\n");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies terminal inline HTML is lowered to an exact literal echo token sequence.
#[test]
fn test_terminal_inline_html_tokens_and_spans() {
    let spanned = tokenize("<?php echo 'A'; ?>\nHello\nworld").unwrap();
    let token_kinds: Vec<_> = spanned.iter().map(|(token, _)| token.clone()).collect();
    assert_eq!(
        token_kinds,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("A".into()),
            Token::Semicolon,
            Token::Semicolon,
            Token::Echo,
            Token::StringLiteral("Hello\nworld".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );

    let inline_span = spanned[5].1.span;
    assert_eq!((inline_span.line, inline_span.col), (2, 1));
    assert_eq!((inline_span.end_line, inline_span.end_col), (3, 6));
    assert_eq!(spanned[6].1.span, inline_span);
}

/// Verifies LF/CRLF absorption and preservation of every other inline byte.
#[test]
fn test_closing_tag_absorbs_only_one_adjacent_line_ending() {
    for (source, expected) in [
        ("<?php ?>\nB", "B"),
        ("<?php ?>\rB", "B"),
        ("<?php ?>\r\nB", "B"),
        ("<?php ?> B", " B"),
        ("<?php ?>\n\nB", "\nB"),
    ] {
        let token_kinds = tokens(source);
        assert!(token_kinds.contains(&Token::StringLiteral(expected.into())));
    }
}

/// Verifies XML processing-instruction text stays literal instead of looking like PHP reopening.
#[test]
fn test_terminal_inline_xml_processing_instruction_is_literal() {
    let token_kinds = tokens("<?php ?><?xml version='1.0'?>");
    assert!(token_kinds.contains(&Token::StringLiteral("<?xml version='1.0'?>".into())));
}

/// Verifies a valid halt directive terminates lexing before arbitrary binary payload bytes.
#[test]
fn test_halt_compiler_keeps_binary_suffix_opaque() {
    let source = b"<?php __HALT_COMPILER();\xff\x00<?php broken";
    let token_kinds: Vec<_> = tokenize_bytes_with_mode(source, SourceMode::Php)
        .expect("binary bytes after a valid halt directive are opaque")
        .into_iter()
        .map(|(token, _)| token)
        .collect();
    assert_eq!(
        token_kinds,
        vec![
            Token::OpenTag,
            Token::HaltCompiler(24),
            Token::LParen,
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies malformed UTF-8 remains an error when it occurs before the halt boundary.
#[test]
fn test_halt_compiler_requires_valid_utf8_before_offset() {
    let error = tokenize_bytes_with_mode(
        b"<?php echo \xff; __HALT_COMPILER();DATA",
        SourceMode::Php,
    )
    .expect_err("the executable PHP prefix must remain valid UTF-8");
    assert!(
        error
            .to_string()
            .contains("PHP source before __HALT_COMPILER() must be valid UTF-8")
    );
}

/// Verifies the close-tag terminator contributes its bytes and one adjacent LF to the offset.
#[test]
fn test_halt_compiler_close_tag_offset_matches_php() {
    let token_kinds: Vec<_> = tokenize_bytes_with_mode(
        b"<?php __HALT_COMPILER()?>\nDATA",
        SourceMode::Php,
    )
    .expect("close tag is a valid PHP statement terminator")
    .into_iter()
    .map(|(token, _)| token)
    .collect();
    assert!(token_kinds.contains(&Token::HaltCompiler(26)));
}

/// Verifies PHP's standalone CR newline is absorbed into a close-tag halt offset.
#[test]
fn test_halt_compiler_close_tag_cr_offset_matches_php() {
    let source = b"<?php __HALT_COMPILER()?>\rDATA";
    let token_kinds: Vec<_> = tokenize_bytes_with_mode(source, SourceMode::Php)
        .expect("standalone CR is a PHP newline")
        .into_iter()
        .map(|(token, _)| token)
        .collect();
    assert!(matches!(
        token_kinds.iter().find(|token| matches!(token, Token::HaltCompiler(_))),
        Some(Token::HaltCompiler(offset)) if *offset == source.len() - 4
    ));
}

/// Verifies a close tag terminates a line comment and supplies the halt semicolon token.
#[test]
fn test_halt_compiler_line_comment_close_tag_offset_matches_php() {
    let source = b"<?php __HALT_COMPILER() //x ?>\nDATA";
    let token_kinds: Vec<_> = tokenize_bytes_with_mode(source, SourceMode::Php)
        .expect("PHP close tag terminates a line comment")
        .into_iter()
        .map(|(token, _)| token)
        .collect();
    assert!(matches!(
        token_kinds.iter().find(|token| matches!(token, Token::HaltCompiler(_))),
        Some(Token::HaltCompiler(offset)) if *offset == source.len() - 4
    ));
}

/// Verifies strings, comments, fully-qualified calls, and member calls do not stop the scanner.
#[test]
fn test_halt_compiler_detection_is_lexical_and_context_sensitive() {
    let source = r#"<?php
$literal = '__HALT_COMPILER();';
// __HALT_COMPILER();
\__HALT_COMPILER();
$object->__HALT_COMPILER();
__HALT_COMPILER();PAYLOAD
"#;
    let token_kinds: Vec<_> = tokenize_bytes_with_mode(source.as_bytes(), SourceMode::Php)
        .expect("only the direct statement form halts lexing")
        .into_iter()
        .map(|(token, _)| token)
        .collect();
    assert_eq!(
        token_kinds
            .iter()
            .filter(|token| matches!(token, Token::HaltCompiler(_)))
            .count(),
        1
    );
    assert!(matches!(
        token_kinds.iter().find(|token| matches!(token, Token::HaltCompiler(_))),
        Some(Token::HaltCompiler(offset)) if *offset == source.find("PAYLOAD").unwrap()
    ));
}

/// Verifies static keyword syntax stays a parse error without hiding a later direct halt payload.
#[test]
fn test_halt_compiler_static_keyword_is_not_a_terminal_directive() {
    let source = b"<?php HaltFacade::__HALT_COMPILER(); __HALT_COMPILER();PAYLOAD";
    let halt_tokens = tokenize_bytes_with_mode(source, SourceMode::Php)
        .expect("the later direct directive still defines the opaque boundary")
        .into_iter()
        .filter_map(|(token, _)| match token {
            Token::HaltCompiler(offset) => Some(offset),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(halt_tokens, vec![0, source.len() - 7]);
}

/// Verifies ASCII controls outside PHP's four scanner whitespace bytes cannot form a halt statement.
#[test]
fn test_halt_compiler_rejects_non_php_ascii_whitespace() {
    for control in [b'\x0b', b'\x0c'] {
        let mut source = b"<?php __HALT_COMPILER".to_vec();
        source.push(control);
        source.extend_from_slice(b"();PAYLOAD");
        tokenize_bytes_with_mode(&source, SourceMode::Php)
            .expect_err("vertical-tab and form-feed are not PHP scanner whitespace");
    }
}
