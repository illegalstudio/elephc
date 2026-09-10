//! Purpose:
//! End-to-end regressions for DOM DTD, XML Schema, and Relax NG validation.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_validation` through Rust's test harness.
//!
//! Key details:
//! - Expected diagnostics follow the pinned libxml2 2.15.3 bridge engine.
//! - Modern XML coverage locks namespace relinking for QName-valued schema data.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies legacy and modern documents return PHP-compatible DTD results.
#[test]
fn document_dtd_validation_reports_validity_and_structured_errors() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$legacy = new DOMDocument();
$legacy->loadXML('<!DOCTYPE root [<!ELEMENT root (child)><!ELEMENT child EMPTY>]><root/>');
libxml_clear_errors();
echo $legacy->validate() ? "T" : "F";
$error = libxml_get_last_error();
echo ":" . $error->level . ":" . $error->code . ":" . trim($error->message);

$legacy->loadXML('<!DOCTYPE root [<!ELEMENT root (child)><!ELEMENT child EMPTY>]><root><child/></root>');
libxml_clear_errors();
echo "|" . ($legacy->validate() ? "T" : "F") . ":" . count(libxml_get_errors());

$modern = Dom\XMLDocument::createFromString(
    '<!DOCTYPE root [<!ELEMENT root (child)><!ELEMENT child EMPTY>]><root><child/></root>'
);
libxml_clear_errors();
echo "|" . ($modern->validate() ? "T" : "F") . ":" . count(libxml_get_errors());
"#,
    );
    assert_eq!(
        out,
        "F:2:504:Element root content does not follow the DTD, expecting (child), got|T:0|T:0"
    );
}

/// Verifies XSD validation, default attributes, and modern in-scope QName prefixes.
#[test]
fn document_schema_validation_supports_flags_and_modern_namespaces() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML('<root/>');
$defaults = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
    . '<element name="root"><complexType>'
    . '<attribute name="mode" type="string" default="on"/>'
    . '</complexType></element></schema>';
echo $document->schemaValidateSource($defaults, LIBXML_SCHEMA_CREATE) ? "T" : "F";
echo ":" . $document->documentElement->getAttribute("mode");

$xml = '<root xmlns="urn:test" xmlns:ref="urn:other">'
    . '<item target="ref:something"/></root>';
$xsd = '<schema xmlns="http://www.w3.org/2001/XMLSchema" '
    . 'targetNamespace="urn:test" elementFormDefault="qualified">'
    . '<element name="root"><complexType><sequence>'
    . '<element name="item"><complexType>'
    . '<attribute name="target" type="QName"/>'
    . '</complexType></element></sequence></complexType></element></schema>';
$modern = Dom\XMLDocument::createFromString($xml, LIBXML_NSCLEAN);
libxml_clear_errors();
echo "|" . ($modern->schemaValidateSource($xsd) ? "T" : "F");
echo ":" . count(libxml_get_errors());

try {
    $modern->schemaValidateSource('');
} catch (ValueError $error) {
    echo "|" . $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "T:on|T:0|Dom\\Document::schemaValidateSource(): Argument #1 ($source) must not be empty"
    );
}

/// Verifies Relax NG success, validity failures, invalid grammars, and diagnostics.
#[test]
fn document_relaxng_validation_maps_php_warnings_and_internal_errors() {
    let out = compile_and_run_capture(
        r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML('<root><child/></root>');
$grammar = '<element name="root" xmlns="http://relaxng.org/ns/structure/1.0">'
    . '<element name="child"><empty/></element></element>';
libxml_clear_errors();
echo $document->relaxNGValidateSource($grammar) ? "T" : "F";
echo ":" . count(libxml_get_errors());

$document->loadXML('<root/>');
libxml_clear_errors();
echo "|" . ($document->relaxNGValidateSource($grammar) ? "T" : "F");
$error = libxml_get_last_error();
echo ":" . count(libxml_get_errors()) . ":" . $error->code;

libxml_clear_errors();
echo "|" . ($document->relaxNGValidateSource('<bad/>') ? "T" : "F");
echo ":" . count(libxml_get_errors());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "T:0|F:1:22|F:1");
    assert_eq!(
        out.stderr,
        "Warning: DOMDocument::relaxNGValidateSource(): Invalid RelaxNG\n"
    );
}

/// Verifies local grammar files, schema flags, and path `ValueError` messages.
#[test]
fn document_file_validation_loads_local_xsd_and_relaxng_grammars() {
    let out = compile_and_run(
        r#"<?php
$xsd = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
    . '<element name="root"><complexType>'
    . '<attribute name="mode" type="string" default="file"/>'
    . '</complexType></element></schema>';
$rng = '<element name="root" xmlns="http://relaxng.org/ns/structure/1.0">'
    . '<optional><attribute name="mode"><text/></attribute></optional>'
    . '</element>';
file_put_contents('validation.xsd', $xsd);
file_put_contents('validation.rng', $rng);

$document = new DOMDocument();
$document->loadXML('<root/>');
echo $document->schemaValidate('validation.xsd', LIBXML_SCHEMA_CREATE) ? "T" : "F";
echo ":" . $document->documentElement->getAttribute("mode");
echo "|" . ($document->relaxNGValidate('validation.rng') ? "T" : "F");

try {
    $document->schemaValidate('');
} catch (ValueError $error) {
    echo "|" . $error->getMessage();
}
try {
    $document->relaxNGValidate('');
} catch (ValueError $error) {
    echo "|" . $error->getMessage();
}
unlink('validation.xsd');
unlink('validation.rng');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "T:file|T",
            "|DOMDocument::schemaValidate(): Argument #1 ($filename) must not be empty",
            "|DOMDocument::relaxNGValidate(): Argument #1 ($filename) must not be empty",
        )
    );
}

/// Verifies grammar URLs and their relative imports use re-entrant PHP wrappers.
#[test]
fn document_file_validation_uses_php_streams_for_relative_grammar_imports() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$opens = 0;
$document = new DOMDocument();
$document->loadXML('<root/>');

class ValidationGrammarStream {
    public $context;
    private string $data = "";
    private int $offset = 0;

    public function url_stat($path, $flags) {
        return [];
    }

    public function stream_open($path, $mode, $options, &$openedPath) {
        global $opens;
        $opens++;
        $nested = new DOMDocument();
        $nested->loadXML('<nested/>');
        if ($nested->documentElement->nodeName !== 'nested') {
            return false;
        }
        if (strpos($path, 'schema-part') !== false) {
            $this->data = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
                . '<element name="root"/></schema>';
        } elseif (strpos($path, 'schema-main') !== false) {
            $this->data = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
                . '<include schemaLocation="schema-part.xsd"/></schema>';
        } elseif (strpos($path, 'relax-part') !== false) {
            $this->data = '<element name="root" '
                . 'xmlns="http://relaxng.org/ns/structure/1.0"><empty/></element>';
        } else {
            $this->data = '<externalRef href="relax-part.rng" '
                . 'xmlns="http://relaxng.org/ns/structure/1.0"/>';
        }
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

stream_wrapper_register('grammar', ValidationGrammarStream::class);
libxml_use_internal_errors(true);
echo $document->schemaValidate('grammar://schema-main.xsd') ? "T" : "F";
echo "|" . ($document->relaxNGValidate('grammar://relax-main.rng') ? "T" : "F");
$schemaSource = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
    . '<include schemaLocation="grammar://schema-part.xsd"/></schema>';
$relaxSource = '<externalRef href="grammar://relax-part.rng" '
    . 'xmlns="http://relaxng.org/ns/structure/1.0"/>';
echo "|" . ($document->schemaValidateSource($schemaSource) ? "T" : "F");
echo "|" . ($document->relaxNGValidateSource($relaxSource) ? "T" : "F");
echo "|" . ($opens >= 6 ? "R" : "r");
echo ":" . count(libxml_get_errors());
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "T|T|T|T|R:0");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected validation callbacks to remain heap-clean, got: {}",
        out.stderr
    );
}

/// Verifies validation rethrows the exact Throwable from an external loader.
#[test]
fn document_validation_preserves_external_loader_throwable_identity() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$expected = new Exception('validation loader');
libxml_set_external_entity_loader(
    function ($public, $system, $context) use ($expected) {
        throw $expected;
    }
);
$schema = '<schema xmlns="http://www.w3.org/2001/XMLSchema">'
    . '<include schemaLocation="loader://part.xsd"/></schema>';
try {
    $document->schemaValidateSource($schema);
} catch (Throwable $caught) {
    echo $caught === $expected ? "T" : "F";
    echo ":" . $caught->getMessage();
}
libxml_set_external_entity_loader(null);
"#,
    );
    assert_eq!(out, "T:validation loader");
}

/// Verifies malformed XML Schema input preserves PHP's ordered warnings.
#[test]
fn document_schema_validation_reports_malformed_grammar_warnings() {
    let out = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
echo $document->schemaValidateSource('string that is not a schema')
    ? "T"
    : "F";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "F");
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: DOMDocument::schemaValidateSource(): Entity: line 1: parser error : Start tag expected, '<' not found\n",
            "Warning: DOMDocument::schemaValidateSource(): string that is not a schema\n",
            "Warning: DOMDocument::schemaValidateSource(): ^\n",
            "Warning: DOMDocument::schemaValidateSource(): Failed to parse the XML resource 'in_memory_buffer'.\n",
            "Warning: DOMDocument::schemaValidateSource(): Invalid Schema\n",
        )
    );
}

/// Verifies php-src rejects oversized local grammar paths before libxml parsing.
#[test]
fn document_validation_rejects_overlong_local_grammar_paths() {
    let out = compile_and_run_capture(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML('<root/>');
var_dump($legacy->schemaValidate(str_repeat(' ', 5000)));
var_dump($legacy->relaxNGValidate(str_repeat(' ', 5000)));

$modern = Dom\XMLDocument::createFromString('<root/>');
var_dump($modern->schemaValidate(str_repeat(' ', 5000)));
var_dump($modern->relaxNgValidate(str_repeat(' ', 5000)));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\n"
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: DOMDocument::schemaValidate(): Invalid Schema file source\n",
            "Warning: DOMDocument::relaxNGValidate(): Invalid RelaxNG file source\n",
            "Warning: Dom\\Document::schemaValidate(): Invalid Schema file source\n",
            "Warning: Dom\\Document::relaxNgValidate(): Invalid RelaxNG file source\n",
        )
    );
}
