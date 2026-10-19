//! Purpose:
//! End-to-end coverage for PHP inline HTML outside `<?php ... ?>`: leading/middle/trailing
//! text, `<?=`, the newline swallowed by `?>`, and alternative control syntax around HTML.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 (short_open_tag=0) from the same source.

use crate::support::*;

/// Verifies HTML at the start, middle and end of a file with `<?=` and the `?>` newline rule.
#[test]
fn test_inline_html_start_middle_end() {
    let out = compile_and_run(
        "Start HTML\n<?php echo \"a\\n\"; ?>\nMid HTML <?= 1+2 ?> x\n<?php echo \"b\\n\"; ?>\nEnd HTML\n",
    );
    assert_eq!(out, "Start HTML\na\nMid HTML 3 x\nb\nEnd HTML\n");
}

/// Verifies a file that is only HTML, and `?>` followed by blank lines, CRLF and trailing spaces.
#[test]
fn test_inline_html_newline_rules() {
    assert_eq!(compile_and_run("only html\nline2"), "only html\nline2");
    let out = compile_and_run(
        "<?php echo \"x\"; ?>\n\ny\n<?php echo \"p\"; ?>  \nq<?php echo \"r\" ?>\r\ns <?= \"t\", \"u\" ?>\r\n\r\nv<?php echo \"w\";",
    );
    assert_eq!(out, "x\ny\np  \nqrs tu\r\nvw");
}

/// Verifies alternative control syntax with HTML between its parts.
#[test]
fn test_inline_html_alternative_syntax() {
    let out = compile_and_run(
        "<?php $items = [\"x\", \"y\"]; ?>\n<ul>\n<?php foreach ($items as $i => $v): ?>\n  <li><?= $i ?>:<?= strtoupper($v) ?></li>\n<?php endforeach; ?>\n</ul>\n<?php if (count($items) > 5): ?>\nbig\n<?php elseif (count($items) == 2): ?>\ntwo\n<?php else: ?>\nsmall\n<?php endif ?>\n<?php for ($i = 0; $i < 2; $i++): ?>for<?= $i ?>,<?php endfor; ?>\n<?php $n = 0; while ($n < 2): ?>w<?= $n++ ?>;<?php endwhile; ?>\n<?php switch (2): ?><?php case 1: ?>one<?php break; ?><?php case 2: ?>two<?php break; ?><?php endswitch; ?>\n",
    );
    assert_eq!(
        out,
        "<ul>\n  <li>0:X</li>\n  <li>1:Y</li>\n</ul>\ntwo\nfor0,for1,w0;w1;two"
    );
}

/// Verifies CSS braces and `<?xml` inside HTML are literal, and `//` comments end at `?>`.
#[test]
fn test_inline_html_literals_and_line_comment_close() {
    let out = compile_and_run(
        "<?php // c ?>post\n<?xml version=\"1.0\"?> <?phpx\n<style>h1 { width: 10px; font-size: 1.5em; }</style>\n<?php echo __LINE__; ?>\n",
    );
    assert_eq!(
        out,
        "post\n<?xml version=\"1.0\"?> <?phpx\n<style>h1 { width: 10px; font-size: 1.5em; }</style>\n4"
    );
}

/// Verifies a function building a template under output buffering.
#[test]
fn test_inline_html_in_function_with_output_buffering() {
    let out = compile_and_run(
        "<?php\nfunction tpl(array $d) {\n    ob_start();\n    ?>\nHello <?= $d[\"name\"] ?>!\n<?php foreach ($d[\"l\"] as $x): ?>- <?= $x ?>\n<?php endforeach; ?>\n<?php\n    return ob_get_clean();\n}\nvar_dump(tpl([\"name\" => \"W\", \"l\" => [1, 2]]));\n",
    );
    assert_eq!(out, "string(15) \"Hello W!\n- 1- 2\"\n");
}
