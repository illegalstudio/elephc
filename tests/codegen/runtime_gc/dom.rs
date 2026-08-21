//! Purpose:
//! Heap-debug regressions for DOM wrapper ownership, weak identity caches, and native graph retention.
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_gc::dom` through the Rust test harness.
//!
//! Key details:
//! - Every source row owns a PHP-observable identity or copy boundary and must finish heap-clean.
//! - The cases complement DOM behavior tests by concentrating on lifetime edges after `unset`.

use crate::support::compile_and_run_with_heap_debug;

/// One native-DOM lifetime scenario and its oracle-visible identity or copy invariant.
struct DomGcCase {
    id: &'static str,
    source: &'static str,
    expected_stdout: &'static str,
}

/// Verifies all DOM ownership matrix rows preserve their invariant and release every heap allocation.
///
/// Sources intentionally execute serially inside one test. Each case links a native bridge program,
/// and this shape avoids multiplying peak memory while retaining one independent binary per lifetime
/// boundary. Existing debug-projection cases remain in `codegen::dom` (DOM-GC-PROJECTION-01..04).
#[test]
fn dom_gc_retention_identity_and_copy_matrix_is_heap_clean() {
    let cases = [
        DomGcCase {
            id: "DOM-GC-DOCUMENT-NODE-01",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$first = $document->documentElement;
$second = $document->documentElement;
echo ($first === $second ? 'I' : 'x');
unset($document, $second);
echo '|' . $first->nodeName . '|';
echo ($first->ownerDocument !== null ? 'O' : 'x');
unset($first);
"#,
            expected_stdout: "I|root|O",
        },
        DomGcCase {
            id: "DOM-GC-LIVE-COLLECTION-02",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$nodes = $document->getElementsByTagName('child');
unset($document);
$child = $nodes->item(0);
echo $nodes->length . '|' . $child->nodeName . '|';
echo ($nodes->item(0) === $child ? 'I' : 'x');
unset($child, $nodes);
"#,
            expected_stdout: "1|child|I",
        },
        DomGcCase {
            id: "DOM-GC-XPATH-RETAINS-DOCUMENT-03",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$xpath = new DOMXPath($document);
$nodes = $xpath->query('//child');
unset($document, $xpath);
$child = $nodes->item(0);
echo $nodes->length . '|' . $child->nodeName . '|';
echo ($nodes->item(0) === $child ? 'I' : 'x');
unset($child, $nodes);
"#,
            expected_stdout: "1|child|I",
        },
        DomGcCase {
            id: "DOM-GC-DETACHED-SUBTREE-04",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$root = $document->documentElement;
$child = $root->firstChild;
$root->removeChild($child);
unset($root, $document);
echo $child->nodeName . '|';
echo ($child->parentNode === null ? 'D' : 'x');
echo ($child->ownerDocument !== null ? 'O' : 'x');
unset($child);
"#,
            expected_stdout: "child|DO",
        },
        DomGcCase {
            // This row intentionally keeps a second reference alive: it proves live-wrapper
            // identity, not eviction after the final wrapper is destroyed.
            id: "DOM-GC-LIVE-WRAPPER-CACHE-05",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$first = $document->documentElement;
$second = $document->documentElement;
echo ($first === $second ? 'I' : 'x');
unset($first);
$third = $document->documentElement;
echo ($second === $third ? 'I' : 'x');
unset($third, $second, $document);
"#,
            expected_stdout: "II",
        },
        DomGcCase {
            id: "DOM-GC-DYNAMIC-CYCLE-06",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$node = $document->documentElement;
$node->self = $node;
echo ($node->self === $node ? 'C' : 'x');
unset($node, $document);
"#,
            expected_stdout: "C",
        },
        DomGcCase {
            id: "DOM-GC-OBJECT-CLONE-VS-NATIVE-CLONE-07",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$node = $document->documentElement;
$node->marker = 'source';
$objectClone = clone $node;
$nativeClone = $node->cloneNode(false);
echo ($objectClone !== $node && $objectClone->marker === 'source' ? 'O' : 'x');
echo ($nativeClone !== $node && !isset($nativeClone->marker) ? 'N' : 'x');
unset($nativeClone, $objectClone, $node, $document);
"#,
            expected_stdout: "ON",
        },
        DomGcCase {
            id: "DOM-GC-IMPORT-ADOPT-08",
            source: r#"<?php
$source = new DOMDocument();
$source->loadXML('<source><child/></source>');
$target = new DOMDocument();
$target->loadXML('<target/>');
$node = $source->documentElement->firstChild;
$node->marker = 'retained';
$imported = $target->importNode($node, true);
$adopted = $target->adoptNode($node);
echo ($imported !== $node && !isset($imported->marker) ? 'I' : 'x');
echo ($adopted === $node && $adopted->marker === 'retained' ? 'A' : 'x');
unset($adopted, $imported, $node, $target, $source);
"#,
            expected_stdout: "IA",
        },
        DomGcCase {
            id: "DOM-GC-REGISTERED-SUBCLASS-09",
            source: r#"<?php
class GcElement extends DOMElement {}
$document = new DOMDocument();
$document->registerNodeClass(DOMElement::class, GcElement::class);
$document->loadXML('<root/>');
$first = $document->documentElement;
$second = $document->documentElement;
echo get_class($first) . '|';
echo ($first === $second ? 'I' : 'x');
unset($second, $first, $document);
"#,
            expected_stdout: "GcElement|I",
        },
        DomGcCase {
            id: "DOM-GC-FINALIZE-LOOP-10",
            source: r#"<?php
for ($index = 0; $index < 32; $index++) {
    $document = new DOMDocument();
    $document->loadXML('<root><child/></root>');
    $node = $document->documentElement->firstChild;
    $nodes = $document->getElementsByTagName('child');
    echo ($nodes->item(0) === $node ? 'I' : 'x');
    unset($nodes, $node, $document);
}
"#,
            expected_stdout: "IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII",
        },
        // Expected outputs verified with the pinned PHP 8.5.8/libxml2 2.15.3 oracle.
        DomGcCase {
            // A live cached wrapper keeps its original registered class after policy changes.
            id: "DOM-GC-LIVE-CACHE-POLICY-11",
            source: r#"<?php
class FirstGcElement extends DOMElement {}
class SecondGcElement extends DOMElement {}
$document = new DOMDocument();
$document->registerNodeClass(DOMElement::class, FirstGcElement::class);
$document->loadXML('<root/>');
$first = $document->documentElement;
$document->registerNodeClass(DOMElement::class, SecondGcElement::class);
$second = $document->documentElement;
echo get_class($first) . '|' . get_class($second) . '|';
echo ($first === $second ? 'I' : 'x');
unset($second, $first, $document);
"#,
            expected_stdout: "FirstGcElement|FirstGcElement|I",
        },
        DomGcCase {
            id: "DOM-GC-DETACH-REATTACH-12",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$root = $document->documentElement;
$child = $root->firstChild;
$root->removeChild($child);
$root->appendChild($child);
$again = $root->firstChild;
echo ($again === $child ? 'I' : 'x') . '|';
echo ($child->ownerDocument === $document ? 'O' : 'x') . '|' . $child->nodeName;
unset($again, $child, $root, $document);
"#,
            expected_stdout: "I|O|child",
        },
        DomGcCase {
            id: "DOM-GC-WEAK-CACHE-AFTER-DROP-13",
            source: r#"<?php
class FirstDroppedElement extends DOMElement {}
class SecondDroppedElement extends DOMElement {}
$document = new DOMDocument();
$document->registerNodeClass(DOMElement::class, FirstDroppedElement::class);
$document->loadXML('<root/>');
$first = $document->documentElement;
echo get_class($first) . '|';
unset($first);
$document->registerNodeClass(DOMElement::class, SecondDroppedElement::class);
$second = $document->documentElement;
echo get_class($second) . '|';
echo ($second instanceof SecondDroppedElement ? 'new' : 'x');
unset($second, $document);
"#,
            expected_stdout: "FirstDroppedElement|SecondDroppedElement|new",
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
            "{}: identity/COW invariant diverged",
            case.id,
        );
        assert!(
            out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "{}: native wrapper lifecycle leaked: {}",
            case.id,
            out.stderr,
        );
    }
}
