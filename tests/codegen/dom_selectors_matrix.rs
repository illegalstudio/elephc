//! Purpose:
//! Oracle-pinned CSS selector behavior matrices for the living DOM API.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_selectors_matrix` through Rust's test harness.
//!
//! Key details:
//! - Cases cover escaping, selector ordering, snapshots, scope diagnostics, pseudo-classes, and closest/matches.
//! - The fixtures complement the broader selector smoke test in `dom.rs` without repeating its selectors.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug};

/// One PHP 8.5.8 selector fixture with a case identifier retained in assertion output.
struct SelectorCase {
    id: &'static str,
    source: &'static str,
    stdout: &'static str,
}

/// Runs a selector matrix against Elephc and compares the result with its PHP oracle trace.
fn assert_selector_cases(cases: &[SelectorCase]) {
    for case in cases {
        let output = compile_and_run_capture(case.source);
        assert!(
            output.success,
            "{} failed: stdout={:?} stderr={}",
            case.id,
            output.stdout,
            output.stderr,
        );
        assert_eq!(output.stdout, case.stdout, "{} stdout", case.id);
        assert_eq!(output.stderr, "", "{} stderr", case.id);
    }
}

/// Pins escaped identifiers, comma-selector ordering, selector snapshots, and unsupported scope errors.
#[test]
fn living_dom_selector_escape_order_snapshot_scope_and_closest_matrix_matches_php_8_5_8() {
    assert_selector_cases(&[SelectorCase {
        id: "selector_escape_order_snapshot_scope_closest_and_matches",
        source: r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><div id="scope"><p id="a:b" class="item first" data-value="a,b">one</p><p id="two" class="item">two<span class="inside">x</span></p><p id="three" class="item">three</p></div>'
);
$scope = $document->querySelector("#scope");
$selected = $scope->querySelectorAll(".item, #two");
echo $selected->length, ":", $selected->item(0)->id, ":", $selected->item(1)->id, ":";
echo $selected->item(2)->id, "|";
$escaped = $document->querySelector("#a\\:b[data-value=\"a,b\"]");
echo $escaped->textContent, ":";
echo ($escaped->matches(":is(.item, .nope):not(.disabled):first-child") ? "T" : "F"), ":";
echo $document->querySelector(".inside")->closest(".item")->id, "|";
$snapshot = $scope->querySelectorAll(".item");
$scope->firstElementChild->remove();
$scope->append($document->createElement("p"));
echo $snapshot->length, ":", $snapshot->item(0)->id, ":", $scope->querySelectorAll(".item")->length, "|";
try {
    $scope->querySelector(":scope");
} catch (DOMException $error) {
    echo $error->code, ":", $error->getMessage();
}
"#,
        stdout: concat!(
            "4:a:b:two:two|one:T:two|3:a:b:2|12:",
            "Invalid selector (Selectors. Not supported: scope)",
        ),
    }]);
}

/// Pins supported functional pseudo-classes independently from the existing selector smoke test.
#[test]
fn living_dom_selector_pseudo_class_matrix_matches_php_8_5_8() {
    assert_selector_cases(&[SelectorCase {
        id: "selector_nth_has_not_and_where",
        source: r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><main><p id="one" class="item">one</p><p id="two" class="item">two<span class="inside">x</span></p><p id="three" class="item">three</p></main>'
);
foreach ([
    ".item:nth-child(2n+1)",
    ".item:has(.inside)",
    ".item:not(:has(.inside))",
    ".item:where(#two, #none)",
] as $selector) {
    $nodes = $document->querySelectorAll($selector);
    echo $selector, ":", $nodes->length, ":", $nodes->item(0)->id, "|";
}
"#,
        stdout: ".item:nth-child(2n+1):2:one|.item:has(.inside):1:two|.item:not(:has(.inside)):2:one|.item:where(#two, #none):1:two|",
    }]);
}

/// Verifies selector snapshots release their detached wrapper graph after document mutation.
#[test]
fn living_dom_selector_snapshot_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><main><p id="one" class="item"></p><p id="two" class="item"></p></main>'
);
$main = $document->querySelector("main");
$snapshot = $main->querySelectorAll(".item");
$main->firstElementChild->remove();
echo $snapshot->length, ":", $snapshot->item(0)->id, "\n";
unset($snapshot, $main, $document);
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "2:one\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "selector snapshot leaked: {}",
        output.stderr,
    );
}
