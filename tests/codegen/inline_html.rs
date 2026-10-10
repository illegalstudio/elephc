//! Purpose:
//! End-to-end regressions for inline HTML in PHP files (issue #839): leading and
//! interleaved HTML around `<?php`/`?>` tags, the `<?=` short echo tag, the newline a
//! `?>` swallows, the empty statement a `?>` lowers to, and HTML in an included file.
//!
//! Called from:
//! - `cargo test --test codegen_tests inline_html` through the integration test harness.
//!
//! Key details:
//! - Expected stdout is real `LC_ALL=C php` 8.5 output for the same source.
//! - The multi-file fixture writes a real project and runs the CLI so `include` resolves.

use crate::support::*;

/// Writes a temporary PHP project, compiles its entry through the CLI, and returns stdout.
fn compile_php_project_and_run(files: &[(&str, &str)], entry: &str) -> String {
    let dir = make_cli_test_dir("elephc_cli_inline_html");
    for (path, source) in files {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("project directory should be created");
        }
        fs::write(path, source).expect("project source should be written");
    }

    let entry_path = dir.join(entry);
    let compile = elephc_cli_command(&dir)
        .arg(&entry_path)
        .output()
        .expect("elephc CLI should run");
    assert!(
        compile.status.success(),
        "compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let binary = entry_path.with_extension("");
    let output = run_binary(&binary, &dir);
    assert!(
        output.status.success(),
        "binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("program output should be UTF-8");
    let _ = fs::remove_dir_all(dir);
    stdout
}

/// Leading inline HTML is emitted before the first statement.
#[test]
fn inline_html_leading_text_is_emitted_before_code() {
    let out = compile_and_run("<!doctype html>\n<?php echo \"ok\";");
    assert_eq!(out, "<!doctype html>\nok");
}

/// HTML between two code blocks is emitted in place, with the newline after `?>` swallowed.
#[test]
fn inline_html_between_blocks_is_emitted_in_order() {
    let out = compile_and_run("<?php echo \"a\"; ?>\nHTML\n<?php echo \"b\"; ?>");
    assert_eq!(out, "aHTML\nb");
}

/// `?>` terminates a statement, so a preceding `;` is not required.
#[test]
fn inline_html_close_tag_terminates_the_statement() {
    let out = compile_and_run("<?php echo \"no-semicolon\" ?>");
    assert_eq!(out, "no-semicolon");
}

/// Only one newline is swallowed right after `?>`.
#[test]
fn inline_html_swallows_exactly_one_newline() {
    let out = compile_and_run("<?php echo \"a\" ?>\n\nX");
    assert_eq!(out, "a\nX");
}

/// The `<?=` short echo tag echoes its expression.
#[test]
fn inline_html_short_echo_tag_echoes() {
    let out = compile_and_run("<?= 1 + 2 ?>");
    assert_eq!(out, "3");
}

/// A block can span the tags: `if (1) { ?>IN<?php }`.
#[test]
fn inline_html_inside_a_block_keeps_control_flow() {
    let out = compile_and_run("<?php if (1) { ?>IN<?php }");
    assert_eq!(out, "IN");
}

/// A file that is entirely HTML echoes its whole content.
#[test]
fn inline_html_pure_html_file_is_echoed() {
    let out = compile_and_run("plain html only\nsecond line\n");
    assert_eq!(out, "plain html only\nsecond line\n");
}

/// `<?php` opens code only when followed by a separator, so `<?phpX` stays HTML.
#[test]
fn inline_html_php_prefix_without_separator_stays_html() {
    let out = compile_and_run("<?phpX");
    assert_eq!(out, "<?phpX");
}

/// An empty statement is valid PHP, both bare and as a braceless control body.
#[test]
fn inline_html_empty_statement_is_a_noop() {
    let out =
        compile_and_run("<?php\n$a = [1, 2];\nforeach ($a as $v);\nif ($v);\n;\necho \"done\";\n");
    assert_eq!(out, "done");
}

/// Inline HTML in an included file — leading, and between the tags — is emitted where the
/// include runs, exactly as PHP parses the included file.
#[test]
fn inline_html_in_an_included_file_is_emitted() {
    let out = compile_php_project_and_run(
        &[
            (
                "main.php",
                "<?php echo \"A\";\ninclude \"part.php\";\necho \"C\";\n",
            ),
            (
                "part.php",
                "LEAD\n<?php echo \"B\"; ?>\nTAIL\n<?php echo \"D\"; ?>",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ALEAD\nBTAIL\nDC");
}

/// A `//` comment ends at `?>`, so a common template idiom's markup is emitted.
#[test]
fn inline_html_line_comment_ends_at_close_tag() {
    let out = compile_and_run(
        "<?php $items = [\"a\", \"b\"];\nforeach ($items as $i): // one row ?>\n<li><?= $i ?></li>\n<?php endforeach; ?>\ndone",
    );
    assert_eq!(out, "<li>a</li>\n<li>b</li>\ndone");
}

/// A private-use marker character in inline HTML survives byte-for-byte.
#[test]
fn inline_html_preserves_private_use_characters() {
    let out = compile_and_run("A\u{e000}B<?php echo 1;");
    assert_eq!(out, "A\u{e000}B1");
}

/// A file-initial `#!` shebang is not part of the output; a later `#!` is ordinary HTML.
#[test]
fn inline_html_leading_shebang_is_dropped() {
    let out = compile_and_run("#!/usr/bin/env php\n<?php echo \"hi\";");
    assert_eq!(out, "hi");

    let out = compile_and_run("hello\n#!/usr/bin/env php\n<?php echo \"X\";");
    assert_eq!(out, "hello\n#!/usr/bin/env php\nX");

    // A CRLF shebang is dropped as a unit; a bare `\r` is shebang content.
    let out = compile_and_run("#!/usr/bin/env php\r\n<?php echo \"hi\";");
    assert_eq!(out, "hi");
    let out = compile_and_run("#!/usr/bin/env php\rbar\n<?php echo \"X\";");
    assert_eq!(out, "X");

    // A BOM before `#!` makes the line ordinary HTML: PHP recognizes a shebang only at the raw
    // first two bytes. (The BOM byte itself is dropped, as it is for any source.)
    let out = compile_and_run("\u{feff}#!/usr/bin/env php\n<?php echo \"b\";");
    assert_eq!(out, "#!/usr/bin/env php\nb");
}

/// An empty statement before `declare(strict_types=1)` does not move it out of first position.
#[test]
fn inline_html_empty_statement_before_declare_is_allowed() {
    let out = compile_and_run("<?php ; declare(strict_types=1); echo 1;");
    assert_eq!(out, "1");

    let out = compile_and_run("<?php ?>\n<?php declare(strict_types=1); echo 1;");
    assert_eq!(out, "1");
}

/// A `declare` (block form) may precede a namespace, matching PHP's "or after any declare call".
#[test]
fn inline_html_declare_block_before_namespace_is_allowed() {
    let out = compile_and_run("<?php declare(ticks=1) { echo 1; } namespace A; echo \"ok\";");
    assert_eq!(out, "1ok");
}

/// `declare(strict_types=1)` is allowed after an earlier `declare`, in either form, including the
/// bare single-statement form and a NESTED colon form.
#[test]
fn inline_html_declare_before_strict_types_is_allowed() {
    let out = compile_and_run("<?php declare(ticks=1); declare(strict_types=1); echo 1;");
    assert_eq!(out, "1");

    let out = compile_and_run("<?php declare(ticks=1) { } declare(strict_types=1); echo 1;");
    assert_eq!(out, "1");

    let out = compile_and_run("<?php declare(ticks=1) echo 1; declare(strict_types=1); echo 2;");
    assert_eq!(out, "12");

    let out = compile_and_run(
        "<?php declare(ticks=1): declare(ticks=2): enddeclare; enddeclare; declare(strict_types=1); echo 1;",
    );
    assert_eq!(out, "1");
}

/// A `//` comment ends at a bare `\r`, so the statement after it still runs.
#[test]
fn inline_html_line_comment_ends_at_carriage_return() {
    let out = compile_and_run("<?php echo 1; // c\recho 2;");
    assert_eq!(out, "12");
}

/// A `@template` docblock stays bound to the declaration directly below it when leading inline
/// HTML precedes the code. The bound is enforced only when the annotation is read, so a dropped
/// docblock would let this compile and run instead of failing.
#[test]
fn inline_html_keeps_a_docblock_bound_to_the_declaration_below_it() {
    let source = r#"<div>
<?php
class Entity {}
class Other {}
/**
 * @template E of Entity
 * @param E $entity
 */
function accept($entity): int { return 1; }
echo accept(new Other());
"#;
    let error = compile_cli_file_with_flags_expect_failure(source, &[]);
    assert!(error.contains("does not satisfy its bound"), "{error}");
}

/// The same binding holds after a `?>`/`<?php` round trip with no HTML between the tags.
#[test]
fn inline_html_docblock_after_a_close_and_reopen_binds() {
    let source = r#"<?php
class Entity {}
class Other {}
echo 1;
?>
<?php
/**
 * @template E of Entity
 * @param E $entity
 */
function accept($entity): int { return 1; }
echo accept(new Other());
"#;
    let error = compile_cli_file_with_flags_expect_failure(source, &[]);
    assert!(error.contains("does not satisfy its bound"), "{error}");
}
