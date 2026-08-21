//! Purpose:
//! Heap-debug coverage for SimpleXML views, DOM interop identity, aliasing, and finalization.
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_gc::simplexml_views` through the Rust test harness.
//!
//! Key details:
//! - Each case keeps native document/view ownership observable after PHP owners are released.
//! - Alias and clone cases distinguish shared views from independent native copies.
//! - Expected outputs were checked with the pinned PHP 8.5.8/libxml2 2.15.3 oracle.

use crate::support::compile_and_run_with_heap_debug;

/// One SimpleXML lifetime scenario and its PHP-visible identity or mutation invariant.
struct SimpleXmlGcCase {
    id: &'static str,
    source: &'static str,
    expected_stdout: &'static str,
}

/// Verifies SimpleXML view retention, alias/clone ownership, DOM cache identity, and finalization.
#[test]
fn simplexml_gc_views_aliases_identity_and_finalization_are_heap_clean() {
    let cases = [
        SimpleXmlGcCase {
            id: "SIMPLEXML-GC-DOM-VIEW-01",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child>one</child></root>');
$element = $document->documentElement;
if ($element === null) { exit(2); }
$view = simplexml_import_dom($element);
if ($view === false) { exit(3); }
unset($element, $document);
echo $view->getName() . '|' . $view->child->getName() . '|';
echo (string) $view->child . '|';
$view->addChild('added', 'two');
echo (string) $view->added;
unset($view);
"#,
            expected_stdout: "root|child|one|two",
        },
        SimpleXmlGcCase {
            id: "SIMPLEXML-GC-ALIAS-CLONE-02",
            source: r#"<?php
$root = simplexml_load_string('<root><child>before</child></root>');
if ($root === false) { exit(2); }
$alias = $root->child;
$copy = $alias;
$clone = clone $alias;
$copy[0] = 'alias';
$clone[0] = 'clone';
echo (string) $root->child . '|' . (string) $alias . '|' . (string) $clone . '|';
echo ($alias === $copy ? 'same' : 'different') . '|';
echo ($alias === $clone ? 'same' : 'different');
unset($clone, $copy, $alias, $root);
"#,
            expected_stdout: "alias|alias|clone|same|different",
        },
        SimpleXmlGcCase {
            id: "SIMPLEXML-GC-DOM-CACHE-03",
            source: r#"<?php
$simple = simplexml_load_string('<root><child/></root>');
if ($simple === false) { exit(2); }
$first = dom_import_simplexml($simple->child);
$second = dom_import_simplexml($simple->child);
if ($first === false || $second === false) { exit(3); }
echo ($first === $second ? 'I' : 'x') . '|';
unset($first);
$third = dom_import_simplexml($simple->child);
if ($third === false) { exit(4); }
echo ($second === $third ? 'I' : 'x') . '|' . $third->nodeName;
unset($third, $second, $simple);
"#,
            expected_stdout: "I|I|child",
        },
        SimpleXmlGcCase {
            id: "SIMPLEXML-GC-FINALIZE-LOOP-04",
            source: r#"<?php
for ($index = 0; $index < 32; $index++) {
    $simple = simplexml_load_string('<root><child/></root>');
    if ($simple === false) { exit(2); }
    $child = $simple->child;
    $first = dom_import_simplexml($child);
    $second = dom_import_simplexml($child);
    if ($first === false || $second === false) { exit(3); }
    echo ($first === $second ? 'I' : 'x');
    unset($second, $first, $child, $simple);
}
"#,
            expected_stdout: "IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII",
        },
    ];

    for case in cases {
        let out = compile_and_run_with_heap_debug(case.source);
        assert!(
            out.success,
            "{}: native program failed: stdout={:?} stderr={}",
            case.id,
            out.stdout,
            out.stderr,
        );
        assert_eq!(
            out.stdout, case.expected_stdout,
            "{}: SimpleXML ownership/identity invariant diverged",
            case.id,
        );
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "{}: SimpleXML/DOM native wrapper lifecycle leaked: {}",
            case.id,
            out.stderr,
        );
    }
}
