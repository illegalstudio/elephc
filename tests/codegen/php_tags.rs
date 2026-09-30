//! Purpose:
//! Integration tests for PHP's tag boundaries: the `?>` closing tag and the literal text it hands
//! to the output, and `<?=`, the short echo tag.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout, so what is
//!   pinned here is what the program PRINTS, not how the lexer got there.
//! - Every expectation is measured against `php -n` 8.5.6.

use crate::support::*;

/// Verifies the `?>` closing tag and the literal text that follows it.
///
/// elephc had no closing tag at all: `?>` was a parse error, so a PHP file that leaves and
/// re-enters code — the shape almost every template uses, and the shape php-src's own `.phpt`
/// corpus is written in — could not compile.
///
/// php's rules here are three, each measured:
///
/// - `?>` terminates the current statement, so the `;` before it is optional.
/// - ONE newline directly after the tag is swallowed: `<?php echo "A";?>\nX\n<?php echo "B";`
///   prints `AX\nB`, not `A\nX\nB`. A `\r\n` counts as that one newline.
/// - Everything up to the next `<?php` is output verbatim.
#[test]
fn test_closing_tag_emits_the_literal_text_that_follows() {
    let out = compile_and_run("<?php\necho \"before\\n\";\n?>\nliteral text\n<?php\necho \"after\\n\";\n");
    assert_eq!(out, "before\nliteral text\nafter\n");

    // The newline directly after the tag is swallowed; the next one is not.
    let swallowed = compile_and_run("<?php echo \"A\";?>\nX\n<?php echo \"B\";");
    assert_eq!(swallowed, "AX\nB");

    // A `\r\n` after the tag counts as that one newline.
    let crlf = compile_and_run("<?php echo \"x\";?>\r\nkept\r\n<?php echo \"y\";");
    assert_eq!(crlf, "xkept\r\ny");
}

/// Verifies a closing tag where NO semicolon belongs after it.
///
/// The tag stands in for the statement's `;`, but only where one belongs. php accepts the empty
/// statement a doubled `;` makes and this parser does not, so `<?php echo "A"; ?>` would fail on a
/// spare token — and a `;` after `{`, `}`, `<?php` or the `:` of `if (...):` is not a statement
/// terminator at all. The alternative syntax is the case that matters most: it is what php
/// templates use to wrap literal text.
#[test]
fn test_closing_tag_supplies_a_semicolon_only_where_one_belongs() {
    // A tag with no statement before it, and two in a row.
    let bare = compile_and_run("<?php ?>A<?php ?>B<?php echo \"C\";\n");
    assert_eq!(bare, "ABC");

    // A statement that already ended in `;`.
    let terminated = compile_and_run("<?php echo \"A\"; ?>B<?php echo \"C\";\n");
    assert_eq!(terminated, "ABC");

    // A statement that did not.
    let unterminated = compile_and_run("<?php echo \"A\" ?>B<?php echo \"C\";\n");
    assert_eq!(unterminated, "ABC");

    // Braces and the alternative syntax both wrap literal text.
    let blocks = compile_and_run(
        "<?php\n$show = true;\nif ($show): ?>\nshown\n<?php else: ?>\nhidden\n<?php endif;\nforeach ([1, 2] as $i): ?>\nitem\n<?php endforeach;\necho \"end\\n\";\n",
    );
    assert_eq!(blocks, "shown\nitem\nitem\nend\n");

    let braced = compile_and_run(
        "<?php\n$x = 1;\nif ($x) { ?>\ninside\n<?php } else { ?>\nother\n<?php }\necho \"end\\n\";\n",
    );
    assert_eq!(braced, "inside\nend\n");
}

/// Verifies `<?=`, php's short echo tag, in both the positions php accepts it.
///
/// It is an OPENING tag, not literal text. Leaving it in the text stream made `Hello <?= $name ?>`
/// print the tag instead of the value, and a file that OPENED with one was rejected outright.
/// `<?= expr ?>` is exactly `<?php echo expr; ?>`, comma form included.
#[test]
fn test_short_echo_tag_is_an_opening_tag() {
    let inline = compile_and_run("<?php $name = \"world\"; ?>\nHello <?= $name ?>!\n<?php echo \"done\\n\";\n");
    assert_eq!(inline, "Hello world!\ndone\n");

    // Several on one line, with an expression, an explicit semicolon and the comma form.
    let several = compile_and_run("<?php $x = 7; ?>\n<?= $x; ?>|<?= \"a\", \"b\" ?>|\n<?php echo \"z\\n\";\n");
    assert_eq!(several, "7|ab|\nz\n");

    // A file may OPEN with one. The `?>` still swallows the newline after it, so nothing
    // separates the two outputs.
    let opens = compile_and_run("<?= 1 + 2 ?>\n<?php echo \"!\\n\";\n");
    assert_eq!(opens, "3!\n");
}

/// Verifies a closing tag inside a COMMENT, which php treats differently per comment kind.
///
/// A `//` or `#` comment ENDS at `?>`: `<?php echo "A"; // comment ?>TEXT` prints `ATEXT`, because
/// the tag closes even though it sits in the comment. elephc ran the comment to the newline, so the
/// tag was swallowed and the `<?php` on the next line arrived as code — a parse error on a file php
/// accepts.
///
/// A `/* */` comment does NOT end there: `/* block ?> still comment */ echo "B";` prints `AB`, so
/// the tag inside it is ordinary comment text and only `*/` closes it. Both measured on `php -n`
/// 8.5.6.
///
/// A tag inside a STRING or a heredoc is likewise not a tag — those are scanned as literals before
/// the tag probe ever sees them — which the third case pins.
#[test]
fn test_closing_tag_inside_a_comment_follows_the_comment_kind() {
    let line = compile_and_run("<?php echo \"A\"; // comment ?>TEXT\n<?php echo \"B\";\n");
    assert_eq!(line, "ATEXT\nB");

    let hash = compile_and_run("<?php echo \"A\"; # hash ?>TEXT\n<?php echo \"B\";\n");
    assert_eq!(hash, "ATEXT\nB");

    let block = compile_and_run("<?php echo \"A\"; /* block ?> still comment */ echo \"B\";\n");
    assert_eq!(block, "AB");

    let literals = compile_and_run(
        "<?php\necho \"a ?> b\\n\";\necho 'c ?> d', \"\\n\";\n$h = <<<TXT\ninside ?> heredoc\nTXT;\necho $h, \"\\n\";\n?>\nafter\n<?php\necho \"end\\n\";\n",
    );
    assert_eq!(literals, "a ?> b\nc ?> d\ninside ?> heredoc\nafter\nend\n");
}
