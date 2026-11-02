//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of PHP's closing tag and the
//! inline HTML around it — the template shape Symfony's `welcome.html.php` is written in.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Every expectation below was taken from reference PHP running the same fixture.

use crate::support::*;

/// Verifies `?>` leaves PHP mode, that the text after it is output verbatim, and that `<?php`
/// re-enters. The closing tag also terminates the statement it interrupts, so `echo $v` needs no
/// semicolon before it.
#[test]
fn test_inline_html_after_closing_tag_is_echoed() {
    let out = compile_and_run("<?php\n$v = 7;\n?>\nA:<?php echo $v; ?>:B\nC:<?php echo $v ?>:D\n");
    assert_eq!(out, "A:7:B\nC:7:D\n");
}

/// Verifies the short echo tag `<?=` prints the expression that follows it.
#[test]
fn test_inline_html_short_echo_tag() {
    let out = compile_and_run("<?php\n$v = 7;\n?>\nE:<?= $v ?>:F\nG:<?= $v * 2 ?>:H\n");
    assert_eq!(out, "E:7:F\nG:14:H\n");
}

/// Verifies PHP's rule that exactly ONE newline directly after a closing tag is swallowed, so a
/// template's tag lines do not each leave a blank line behind. The second blank line survives.
#[test]
fn test_inline_html_swallows_one_newline_after_the_closing_tag() {
    let out = compile_and_run("<?php\n$v = 1;\n?>\nfirst\n<?php ?>\n\nsecond\n");
    assert_eq!(out, "first\n\nsecond\n");
}

/// Verifies alternative syntax spanning tag boundaries — the loop and branch spelling every PHP
/// template uses — because the closing tag is lexed as a statement terminator plus an `echo`.
#[test]
fn test_inline_html_alternative_syntax_across_tags() {
    let out = compile_and_run(
        "<?php $rows = ['a', 'b']; $v = 7; ?>\
<?php foreach ($rows as $r): ?>[<?= $r ?>]<?php endforeach; ?>\
<?php if ($v > 5): ?>big<?php else: ?>small<?php endif; ?>",
    );
    assert_eq!(out, "[a][b]big");
}

/// Verifies braced blocks work across tags too, including the `}` … `{` split of an `if`/`else`.
#[test]
fn test_inline_html_braced_blocks_across_tags() {
    let out = compile_and_run("<?php $v = 2; if ($v > 5) { ?>big<?php } else { ?>small<?php } ?>");
    assert_eq!(out, "small");
}

/// Verifies text BEFORE the first opening tag is output, exactly as the text after a closing tag
/// is. Leading whitespace on its own is not a template and keeps behaving as an indented open tag.
#[test]
fn test_inline_html_before_the_first_open_tag() {
    let out = compile_and_run("HEAD\n<?php echo \"body\\n\"; ?>\nTAIL\n");
    assert_eq!(out, "HEAD\nbody\nTAIL\n");
}

/// Verifies a tag-shaped byte sequence inside a string literal or a heredoc is content, not
/// syntax — the lexer only sees `?>` at code level because strings scan themselves.
#[test]
fn test_inline_html_tags_inside_literals_are_not_syntax() {
    let out = compile_and_run(
        r#"<?php
$s = '<?php not a tag ?>';
echo $s, "|";
$h = <<<TXT
inline <?php stays
TXT;
echo $h;
"#,
    );
    assert_eq!(out, "<?php not a tag ?>|inline <?php stays");
}

/// Verifies a `//` line comment ends at the closing tag, as PHP's does. Running the comment to
/// the end of the line instead would swallow the tag and the whole template behind it.
#[test]
fn test_inline_html_line_comment_ends_at_the_closing_tag() {
    let out = compile_and_run("<?php // a comment ended by the tag ?>\nafter\n");
    assert_eq!(out, "after\n");
}

/// Verifies a `switch` whose case bodies are written as template fragments.
#[test]
fn test_inline_html_inside_switch_cases() {
    let out = compile_and_run(
        "<?php $v = 7; switch ($v) { case 7: ?>seven<?php break; default: ?>other<?php } ?>",
    );
    assert_eq!(out, "seven");
}
