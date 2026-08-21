//! Purpose:
//! End-to-end regressions for SimpleXML serialization and registered PHP streams.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::simplexml_streams` through Rust's test harness.
//!
//! Key details:
//! - Stream callbacks re-enter SimpleXML without retaining the native context borrow.
//! - Zero-byte writes are successful for SimpleXML, while explicit false remains failure.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies `asXML()` and `saveXML()` use registered streams with PHP boolean results.
#[test]
fn simplexml_serialization_uses_reentrant_registered_streams() {
    let out = compile_and_run(
        r#"<?php
class SimpleXmlWriteStream {
    public mixed $context;
    public static int $mode = 1;
    public static string $bytes = '';

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $probe = simplexml_load_string('<probe/>');
        return $probe !== false;
    }

    public function stream_write($data) {
        if (self::$mode === 0) { return 0; }
        if (self::$mode === -1) { return false; }
        self::$bytes .= $data;
        return strlen($data);
    }

    public function stream_flush(): bool { return true; }
    public function stream_close(): void {}
}

stream_wrapper_register('sxewrite', SimpleXmlWriteStream::class);
$xml = simplexml_load_string('<root><item>value</item></root>');
if ($xml === false) { exit(2); }
echo ($xml->asXML('sxewrite://full') ? 'true' : 'false') . '|';
echo (strlen(SimpleXmlWriteStream::$bytes) > 0 ? 'bytes' : 'empty') . '|';
SimpleXmlWriteStream::$mode = 0;
echo ($xml->saveXML('sxewrite://zero') ? 'true' : 'false') . '|';
SimpleXmlWriteStream::$mode = -1;
echo ($xml->asXML('sxewrite://false') ? 'true' : 'false');
"#,
    );
    assert_eq!(out, "true|bytes|true|false");
}

/// Verifies a namespaced subnode dump does not synthesize its ancestor declaration.
#[test]
fn simplexml_subnode_serialization_matches_php_node_dump() {
    let out = compile_and_run(
        r#"<?php
$xml = simplexml_load_string('<root xmlns:p="urn:p"><p:item>one</p:item></root>');
if ($xml === false) { exit(2); }
$xml->registerXPathNamespace('p', 'urn:p');
$nodes = $xml->xpath('//p:item');
if ($nodes === false || $nodes === null) { exit(3); }
echo $nodes[0]->asXML();
"#,
    );
    assert_eq!(out, "<p:item>one</p:item>");
}

/// Verifies explicit null booleans emit every ordered PHP deprecation and keep defaults.
#[test]
fn simplexml_null_boolean_parameters_emit_php_deprecations() {
    let out = compile_and_run_capture(
        r#"<?php
$xml = simplexml_load_string('<root><a/></root>');
if ($xml === false) { exit(2); }
$xml->children(null, null);
$xml->attributes(null, null);
$xml->getDocNamespaces(null, null);
$xml->getNamespaces(null);
echo 'done';
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    let expected = [
        "Deprecated: SimpleXMLElement::children(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n",
        "Deprecated: SimpleXMLElement::attributes(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n",
        "Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n",
        "Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #2 ($fromRoot) of type bool is deprecated\n",
        "Deprecated: SimpleXMLElement::getNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n",
    ];
    let mut cursor = 0;
    for message in expected {
        let relative = out.stderr[cursor..]
            .find(message)
            .unwrap_or_else(|| panic!("missing deprecation {message:?}: {}", out.stderr));
        cursor += relative + message.len();
    }
}

/// Verifies direct native debug arrays retain nested shape, subclass, and freshness.
#[test]
fn simplexml_debug_info_materializes_recursive_fresh_values() {
    let out = compile_and_run(
        r#"<?php
class DebugXml extends SimpleXMLElement {}

$xml = simplexml_load_string(
    '<r id="7"><a><b>B</b></a><a>A2</a><c>C</c></r>',
    DebugXml::class
);
if ($xml === false) { exit(2); }
$first = $xml->__debugInfo();
$second = $xml->__debugInfo();
if ($first === null || $second === null) { exit(3); }
echo $first['@attributes']['id'] . '|';
echo count($first['a']) . '|';
echo get_class($first['a'][0]) . '|';
echo (string) $first['a'][1] . '|';
echo $first['c'] . '|';
echo ($first['a'][0] === $second['a'][0] ? 'same' : 'fresh');
"#,
    );
    assert_eq!(out, "7|2|DebugXml|A2|C|fresh");
}

/// Verifies SimpleXML file loading enters the PHP stream layer and receives the
/// libxml stream context selected before the read.
#[test]
fn simplexml_load_file_uses_registered_stream_and_active_libxml_context() {
    let out = compile_and_run(
        r#"<?php
class SimpleXmlContextReadStream {
    public mixed $context;
    public static string $contextName = '';
    private string $data = '<root><item>ctx</item></root>';
    private int $offset = 0;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $context = $this->context ? stream_context_get_options($this->context) : [];
        self::$contextName = $context['sxctx']['name'] ?? 'none';
        return true;
    }

    public function stream_read($count): string {
        $chunk = substr($this->data, $this->offset, $count);
        $this->offset += strlen($chunk);
        return $chunk;
    }

    public function stream_eof(): bool {
        return $this->offset >= strlen($this->data);
    }

    public function stream_stat(): array { return []; }
    public function url_stat($path, $flags): array { return []; }
}

stream_wrapper_register('sxread', SimpleXmlContextReadStream::class);
$context = stream_context_create(['sxctx' => ['name' => 'expected']]);
libxml_set_streams_context($context);
$xml = simplexml_load_file('sxread://one');
echo ($xml === false ? 'false' : (string) $xml->item) . '|';
echo SimpleXmlContextReadStream::$contextName;
"#,
    );
    assert_eq!(out, "ctx|expected");
}

/// Verifies malformed SimpleXML and DOM documents append to one ordered,
/// context-local libxml error queue without discarding the first parse failure.
#[test]
fn simplexml_and_dom_parse_errors_share_one_ordered_libxml_context() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
libxml_use_internal_errors(true);
libxml_clear_errors();
$simple = simplexml_load_string('<r>');
$first = libxml_get_errors();
$document = new DOMDocument();
$loaded = $document->loadXML('<x>');
$all = libxml_get_errors();
$last = libxml_get_last_error();
echo count($first) . '|' . count($all) . '|';
echo ($last === false ? 'false' : $last->level . '/' . $last->line) . '|';
echo ($simple === false ? 'false' : 'xml') . '|';
echo ($loaded ? 'true' : 'false');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1|2|3/1|false|false");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean shared libxml error ownership, got: {}",
        out.stderr
    );
}

/// Verifies an external entity loader installed through libxml is called by a
/// SimpleXML parse and that its callback result remains re-entrant and releasable.
#[test]
fn simplexml_external_entity_loader_callback_is_reentrant_and_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class SimpleXmlEntityLoader {
    public static int $calls = 0;
    public static string $system = '';

    public function __invoke($public, $system, $context): mixed {
        self::$calls++;
        self::$system = (string) $system;
        $probe = simplexml_load_string('<probe/>');
        if ($probe === false) { exit(2); }
        return null;
    }
}

libxml_use_internal_errors(true);
libxml_clear_errors();
$loader = new SimpleXmlEntityLoader();
libxml_set_external_entity_loader($loader);
$xml = simplexml_load_string(
    '<!DOCTYPE r SYSTEM "memory://missing.dtd"><r/>',
    'SimpleXMLElement',
    LIBXML_DTDLOAD,
);
echo ($xml === false ? 'false' : 'xml') . '|';
echo SimpleXmlEntityLoader::$calls . '|' . SimpleXmlEntityLoader::$system . '|';
echo count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "xml|1|memory://missing.dtd|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean external-loader callback ownership, got: {}",
        out.stderr
    );
}
