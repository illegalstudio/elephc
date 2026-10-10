//! Purpose:
//! Integration or regression tests for lexer tokenization coverage of PHP source structure, including open tag, line comment, and block comment.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP source is tokenized and assertions check exact token kinds, literals, and source structure.

use super::*;

/// Verifies `<?php` produces `OpenTag` and EOF, the bare minimum valid PHP script.
#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
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

/// Verifies a source with no `<?php` open tag is inline HTML, exactly as PHP treats it:
/// the whole file is echoed, so the structural `OpenTag` is followed by an `echo` of the
/// literal text.
#[test]
fn test_missing_open_tag_is_inline_html() {
    let t = tokens("echo \"hi\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("echo \"hi\";".to_string()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies leading inline HTML before the first `<?php` becomes an `echo` ahead of the code.
#[test]
fn test_leading_inline_html_is_echoed_before_code() {
    let t = tokens("<!doctype html>\n<?php echo 1;");
    assert_eq!(t[0], Token::OpenTag);
    assert_eq!(t[1], Token::Echo);
    assert_eq!(t[2], Token::StringLiteral("<!doctype html>\n".to_string()));
    assert_eq!(t[3], Token::Semicolon);
    assert_eq!(t[4], Token::Echo);
}

/// Verifies a `?>` close tag lowers to `;` and the HTML after it to an `echo`, with the one
/// newline directly after the tag swallowed.
#[test]
fn test_close_tag_lowers_to_semicolon_and_html() {
    let t = tokens("<?php echo \"a\"; ?>\nHTML\n<?php echo \"b\"; ?>");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("a".to_string()),
            Token::Semicolon,
            Token::Semicolon,
            Token::Echo,
            Token::StringLiteral("HTML\n".to_string()),
            Token::Semicolon,
            Token::Echo,
            Token::StringLiteral("b".to_string()),
            Token::Semicolon,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies `<?=` opens code and implies `echo`.
#[test]
fn test_short_echo_tag_implies_echo() {
    let t = tokens("<?= 1 ?>");
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

/// Verifies a `<?=` after inline HTML gets an `echo` token anchored at the tag, not at the HTML
/// run start, and that the lowered HTML tokens carry the extent of the HTML text. A point span
/// on the HTML run made the doc-comment pass read the HTML back as code.
#[test]
fn test_inline_html_tokens_carry_their_source_extent() {
    let spanned = tokenize("<?php echo 1; ?><div><?= 2; ?>").expect("tokenizes");
    let literal = spanned
        .iter()
        .find_map(|(token, meta)| match token {
            Token::StringLiteral(text) if text == "<div>" => Some(meta.span),
            _ => None,
        })
        .expect("the HTML literal");
    assert_eq!((literal.col, literal.end_column()), (17, 22));
    let short_echo = spanned
        .iter()
        .filter_map(|(token, meta)| matches!(token, Token::Echo).then_some(meta.span))
        .find(|span| span.col == 22)
        .expect("an echo anchored at the `<?=` tag");
    assert_eq!(short_echo.end_column(), 25);
}

/// Verifies `<?php` opens code only when followed by a separator, so `<?phpX` stays HTML.
#[test]
fn test_php_prefix_without_separator_is_html() {
    let t = tokens("<?phpX");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("<?phpX".to_string()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

/// Verifies a block that spans the tags keeps its structure: `if (1) { ?>IN<?php }`.
#[test]
fn test_html_inside_a_block_keeps_structure() {
    let t = tokens("<?php if (1) { ?>IN<?php }");
    assert_eq!(t[0], Token::OpenTag);
    assert_eq!(t[1], Token::If);
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "IN")
    }));
    assert!(t.contains(&Token::RBrace));
}

/// Verifies a `//` comment ends at `?>`, so the close tag and the HTML after it are lexed.
#[test]
fn test_line_comment_ends_at_close_tag() {
    let t = tokens("<?php // note ?>HTML");
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "HTML")
    }));
}

/// Verifies a `#` comment ends at `?>` too.
#[test]
fn test_hash_comment_ends_at_close_tag() {
    let t = tokens("<?php # note ?>HTML");
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "HTML")
    }));
}

/// Verifies a `?>` inside a string or a `/* */` comment is ordinary data, not a close tag.
#[test]
fn test_close_tag_inside_string_or_block_comment_is_data() {
    let t = tokens("<?php echo \"?> <?php\"; /* ?> */ echo 1;");
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "?> <?php")
    }));
    assert_eq!(t.last(), Some(&Token::Eof));
}

/// Verifies a file-initial `#!` shebang is dropped through its newline, but a later `#!` is
/// ordinary inline HTML.
#[test]
fn test_shebang_is_dropped_only_at_the_start() {
    let t = tokens("#!/usr/bin/env php\n<?php echo 1;");
    assert_eq!(t[0], Token::OpenTag);
    assert_eq!(t[1], Token::Echo);
    assert_eq!(t[2], Token::IntLiteral(1));

    let t = tokens("hello\n#!/usr/bin/env php\n<?php echo 1;");
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "hello\n#!/usr/bin/env php\n")
    }));

    // A CRLF shebang is consumed as a unit.
    let t = tokens("#!/usr/bin/env php\r\n<?php echo 1;");
    assert_eq!(t[1], Token::Echo);

    // A bare `\r` is shebang content: the shebang ends at the next `\n`, not at the `\r`.
    let t = tokens("#!/usr/bin/env php\rbar\n<?php echo 1;");
    assert_eq!(t[1], Token::Echo);

    // With no `\n` at all, the shebang consumes the whole file.
    assert_eq!(
        tokens("#!/usr/bin/env php\r<?php echo 1;"),
        vec![Token::OpenTag, Token::Eof]
    );
}

/// Verifies a shebang with no trailing newline is dropped entirely.
#[test]
fn test_shebang_at_end_of_input_is_dropped() {
    assert_eq!(tokens("#!/usr/bin/env php"), vec![Token::OpenTag, Token::Eof]);
}

/// Verifies a BOM before `#!` makes the line ordinary inline HTML: PHP recognizes a shebang only
/// at the raw first two bytes.
#[test]
fn test_bom_before_shebang_stays_html() {
    let t = tokens("\u{feff}#!/usr/bin/env php\n<?php echo 1;");
    assert!(t.iter().any(|token| {
        matches!(token, Token::StringLiteral(value) if value == "#!/usr/bin/env php\n")
    }));
}

/// Verifies a `//` or `#` line comment ends at a bare `\r` as well as at `\n`.
#[test]
fn test_line_comment_ends_at_carriage_return() {
    let t = tokens("<?php echo 1; // c\recho 2;");
    assert_eq!(t.iter().filter(|token| **token == Token::Echo).count(), 2);
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
