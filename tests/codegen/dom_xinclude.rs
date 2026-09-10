//! Purpose:
//! End-to-end regressions for legacy and modern DOM XInclude processing.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_xinclude` through Rust's test harness.
//!
//! Key details:
//! - Fixtures cover destroyed-wrapper invalidation and retained collection liveness.
//! - PHP stream callbacks execute re-entrantly and preserve Throwable identity.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies fallback substitution, return values, and unrelated live wrappers.
#[test]
fn xinclude_substitutes_fallback_and_preserves_unrelated_wrappers() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"><xi:fallback><included/></xi:fallback></xi:include>'
    . '<keep/>'
    . '</root>'
);
$keep = $document->documentElement->lastElementChild;
$elements = $document->getElementsByTagName('included');
$result = @$document->xinclude();
echo $result . ":" . $elements->length . ":" . $elements->item(0)->nodeName;
echo ":" . $keep->nodeName;
$keep->textContent = "alive";
echo ":" . $keep->textContent;

$modern = Dom\XMLDocument::createFromString(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"><xi:fallback><modern/></xi:fallback></xi:include>'
    . '</root>'
);
$modernResult = @$modern->xinclude();
echo "|" . $modernResult . ":" . $modern->documentElement->firstElementChild->nodeName;
"#,
    );
    assert_eq!(out, "1:1:included:keep:alive|1:modern");
}

/// Verifies every wrapper backed by an XInclude-owned subtree becomes invalid.
#[test]
fn xinclude_destroyed_nodes_throw_invalid_state_error() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"><xi:fallback>'
    . '<discard attr="value">text</discard>'
    . '</xi:fallback></xi:include>'
    . '</root>'
);
$include = $document->documentElement->firstElementChild;
$fallback = $include->firstElementChild;
$discard = $fallback->firstElementChild;
$attribute = $discard->getAttributeNode("attr");
$text = $discard->firstChild;
@$document->xinclude();
try {
    echo $include->localName;
} catch (DOMException $error) {
    echo $error->getCode() . ":" . $error->getMessage() . "|";
}
try {
    echo $fallback->localName;
} catch (DOMException $error) {
    echo $error->getCode() . ":" . $error->getMessage() . "|";
}
try {
    echo $discard->localName;
} catch (DOMException $error) {
    echo $error->getCode() . ":" . $error->getMessage() . "|";
}
try {
    echo $attribute->localName;
} catch (DOMException $error) {
    echo $error->getCode() . ":" . $error->getMessage() . "|";
}
try {
    echo $text->localName;
} catch (DOMException $error) {
    echo $error->getCode() . ":" . $error->getMessage() . "|";
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(
        out.stdout,
        "11:Invalid State Error|11:Invalid State Error|11:Invalid State Error|11:Invalid State Error|11:Invalid State Error|"
    );
}

/// Verifies releasing invalidated wrappers does not retain runtime allocations.
#[test]
fn xinclude_invalidated_wrapper_release_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"><xi:fallback>'
    . '<discard attr="value">text</discard>'
    . '</xi:fallback></xi:include>'
    . '</root>'
);
$include = $document->documentElement->firstElementChild;
$fallback = $include->firstElementChild;
$discard = $fallback->firstElementChild;
$attribute = $discard->getAttributeNode("attr");
$text = $discard->firstChild;
@$document->xinclude();
unset($include, $fallback, $discard, $attribute, $text);
echo "clean";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected invalid wrappers to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies collections and token lists never dereference XInclude-freed roots.
#[test]
fn xinclude_invalidates_derived_views_without_stale_native_access() {
    let out = compile_and_run(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include class="one two" href="missing.xml">'
    . '<xi:fallback><included/></xi:fallback>'
    . '</xi:include>'
    . '</root>'
);
$include = $document->documentElement->firstElementChild;
$children = $include->childNodes;
$attributes = $include->attributes;
$tokens = $include->classList;
$snapshot = $document->querySelectorAll("include");
@$document->xinclude();
echo $children->length;
echo ":" . ($children->item(0) === null ? "N" : "X");
echo ":" . $attributes->length;
echo ":" . $snapshot->length;
echo ":" . ($snapshot->item(0) === null ? "N" : "X");
try {
    echo ":" . $tokens->length;
} catch (DOMException $error) {
    echo ":" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert_eq!(out, "0:N:0:1:N:11:Invalid State Error");
}

/// Verifies legacy and modern APIs keep their distinct range and failure channels.
#[test]
fn xinclude_maps_legacy_and_modern_error_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
echo $legacy->xinclude(PHP_INT_MAX) === false ? "F" : "X";

$modern = Dom\XMLDocument::createEmpty();
try {
    $modern->xinclude(PHP_INT_MAX);
} catch (ValueError $error) {
    echo "|" . $error->getMessage();
}
try {
    $modern->xinclude();
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}

$modern = Dom\XMLDocument::createFromString(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="missing.xml"/>'
    . '</root>'
);
try {
    @$modern->xinclude();
} catch (DOMException $error) {
    echo "|" . $error->getCode() . ":" . $error->getMessage();
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "F",
            "|Dom\\XMLDocument::xinclude(): Argument #1 ($options) is too large",
            "|13:Invalid Modification Error",
            "|13:Invalid Modification Error",
        )
    );
    assert_eq!(
        out.stderr,
        "Warning: DOMDocument::xinclude(): Invalid flags\n"
    );
}

/// Verifies XInclude loads registered streams outside the DOM context borrow.
#[test]
fn xinclude_uses_reentrant_php_streams_and_balanced_ownership() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class XIncludeStream {
    public $context;
    private string $data = "";
    private int $offset = 0;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        $nested = new DOMDocument();
        $nested->loadXML('<nested/>');
        if ($nested->documentElement->nodeName !== 'nested') {
            return false;
        }
        $this->data = '<included>stream</included>';
        return true;
    }

    public function stream_read($count) {
        $chunk = substr($this->data, $this->offset, $count);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof() {
        return $this->offset >= strlen($this->data);
    }

    public function stream_close() {
    }
}

stream_wrapper_register('xinclude', XIncludeStream::class);
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="xinclude://fixture.xml"/>'
    . '</root>'
);
echo $document->xinclude();
$included = $document->documentElement->firstElementChild;
echo ":" . $included->nodeName . ":" . $included->textContent;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "1:included:stream");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected XInclude callbacks to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies an external-loader exception is rethrown as the original object.
#[test]
fn xinclude_preserves_external_loader_throwable_identity() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML(
    '<root xmlns:xi="http://www.w3.org/2001/XInclude">'
    . '<xi:include href="loader://fixture.xml"/>'
    . '</root>'
);
$expected = new Exception("xinclude loader");
libxml_set_external_entity_loader(
    function ($public, $system, $context) use ($expected) {
        throw $expected;
    }
);
try {
    $document->xinclude();
} catch (Throwable $actual) {
    echo $actual === $expected ? "same" : "different";
}
"#,
    );
    assert_eq!(out, "same");
}
