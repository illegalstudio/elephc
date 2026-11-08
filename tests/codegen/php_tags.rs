//! Purpose:
//! End-to-end coverage of PHP's open/close tags and inline output: case-insensitive
//! `<?php`, `<?=`, the short `<?` tag, `?>` as a statement terminator that swallows one
//! newline, literal text outside code blocks, line comments ending at `?>`, and the CLI's
//! `#!` line.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output was produced by `php -n` (PHP 8.5.8) on the same source.

use crate::support::*;

/// Text around a code block is output verbatim; a `?>` not followed by a newline keeps the
/// next character.
#[test]
fn test_inline_text_around_a_code_block_is_output() {
    assert_eq!(compile_and_run("Hello <?php echo 1 ?> world\n"), "Hello 1 world\n");
}

/// `?>` swallows exactly one directly following newline.
#[test]
fn test_close_tag_swallows_one_newline() {
    assert_eq!(compile_and_run("<?php echo 'a'; ?>\nb\n"), "ab\n");
}

/// The short open tag swallows a CRLF pair as one newline.
#[test]
fn test_short_open_tag_and_crlf_after_close_tag() {
    assert_eq!(compile_and_run("<? echo 2; ?>\r\nz"), "2z");
}

/// `<?php` is matched case-insensitively.
#[test]
fn test_uppercase_open_tag() {
    assert_eq!(compile_and_run("<?PHP echo 'x';"), "x");
}

/// `<?=` echoes its expression; `?>` terminates it.
#[test]
fn test_short_echo_tag() {
    assert_eq!(compile_and_run("<?= 1 + 2 ?>|"), "3|");
}

/// A file without any open tag is entirely literal output.
#[test]
fn test_source_without_open_tag_is_literal_output() {
    assert_eq!(compile_and_run("echo \"hi\";"), "echo \"hi\";");
}

/// Text before the first open tag, including whitespace, is output.
#[test]
fn test_leading_whitespace_before_open_tag_is_output() {
    assert_eq!(compile_and_run("\n  <?php echo 'y';"), "\n  y");
}

/// `//` and `#` comments end right before `?>`, which still closes the block.
#[test]
fn test_line_comments_end_at_close_tag() {
    assert_eq!(compile_and_run("<?php // c ?>out"), "out");
    assert_eq!(compile_and_run("<?php # hash ?>\n<?php\necho \"x\";\n"), "x");
}

/// The CLI skips a leading `#!` interpreter line.
#[test]
fn test_shebang_line_is_skipped() {
    assert_eq!(compile_and_run("#!/usr/bin/env php\n<?php echo 1;"), "1");
}

/// Alternative control syntax interleaved with template text.
#[test]
fn test_alternative_syntax_template() {
    assert_eq!(
        compile_and_run("<?php foreach ([1, 2] as $i): ?>[<?= $i ?>]<?php endforeach; ?>\n"),
        "[1][2]"
    );
}

/// An included file may use an upper-case open tag and end with `?>` plus a newline.
#[test]
fn test_included_file_with_upper_case_tag_and_close_tag() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include __DIR__ . '/lib.inc'; echo f();"),
            ("lib.inc", "<?PHP\nfunction f() { return 'ok'; }\n?>\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}
