//! Purpose:
//! Exercises SimpleXML file factories with PHP stream URLs through AOT and runtime eval.
//!
//! Called from:
//! - Managed-libxml2 codegen CI.
//!
//! Key details:
//! - URI bytes keep their source URL inside the native parser context.
//! - Both the constructor and loading function share one stream-read implementation.

use crate::support::*;

/// Data and filtered URLs work through both factories and the shared eval prelude.
#[test]
fn test_simplexml_stream_data_and_filters_bundle() {
    if skip_without_xml_native("test_simplexml_stream_data_and_filters_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$uri = 'data://text/plain;base64,PHIvPg==';
echo simplexml_load_file($uri)->getName(), ':', (new SimpleXMLElement($uri, 0, true))->getName(), '|';
echo (new SimpleXMLElement($uri . "\0ignored", 0, true))->getName(), '|';
echo simplexml_load_file('data://text/plain,%3Cr%3Eone+two%3C%2Fr%3E'), '|';
echo simplexml_load_file('php://filter/read=string.tolower/resource=data://text/plain,%3CR%3EYES%3C%2FR%3E')->getName(), '|';
$source = $argc > 0 ? 'echo simplexml_load_file($uri)->getName(), ":", (new SimpleXMLElement($uri, 0, true))->getName(), "|";' : '';
eval($source);
var_dump(simplexml_load_file('php://input', null, LIBXML_NOERROR | LIBXML_NOWARNING));
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "r:r|r|one two|r|r:r|bool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Registered PHP wrappers supply document bytes without any libxml global callback.
#[test]
fn test_simplexml_registered_stream_document() {
    if skip_without_xml_native("test_simplexml_registered_stream_document") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class XmlDocumentStream {
    public mixed $context = null;
    private int $position = 0;
    public function url_stat(string $path, int $flags): array { return ['mode' => 0100644, 'size' => 4]; }
    public function stream_open(string $path, string $mode, int $options, mixed &$opened_path): bool { return true; }
    public function stream_read(int $count): string {
        $data = substr('<r/>', $this->position, $count);
        $this->position += strlen($data);
        return $data;
    }
    public function stream_stat(): array { return ['mode' => 0100644, 'size' => 4]; }
    public function stream_eof(): bool { return $this->position >= 4; }
    public function stream_close(): void { echo 'close|'; }
}
stream_wrapper_register('xmldocument', 'XmlDocumentStream');
echo simplexml_load_file('xmldocument://document')->getName(), '|';
echo (new SimpleXMLElement('xmldocument://document', 0, true))->getName();
stream_wrapper_unregister('xmldocument');
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "close|r|close|r");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// HTTP URLs use PHP's network reader even though libxml2 has no built-in HTTP client.
#[test]
fn test_simplexml_http_document_factories() {
    if skip_without_xml_native("test_simplexml_http_document_factories") { return; }
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut requests = 0;
        while requests < 2 && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                    let mut request = [0; 4096];
                    socket.read(&mut request).unwrap();
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n<r/>").unwrap();
                    requests += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock =>
                    std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => panic!("HTTP fixture accept failed: {error}"),
            }
        }
        assert_eq!(requests, 2, "both SimpleXML factories must request their document");
    });
    let source = format!("<?php $uri = 'http://127.0.0.1:{port}/document.xml'; echo simplexml_load_file($uri)->getName(), ':', (new SimpleXMLElement($uri, 0, true))->getName();");
    let out = compile_and_run(&source);
    server.join().unwrap();
    assert_eq!(out, "r:r");
}

/// Encoded NUL rejection keeps stream warnings and constructor previous-exception chains.
#[test]
fn test_simplexml_stream_encoded_nul_rejection() {
    if skip_without_xml_native("test_simplexml_stream_encoded_nul_rejection") { return; }
    let out = compile_and_run(r#"<?php
set_error_handler(function(int $level, string $message): bool {
    echo str_contains($message, 'URI must not contain percent-encoded NUL bytes') ? 'nul|' : 'io|';
    return true;
});
var_dump(simplexml_load_file('data://text/plain,%00'));
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('stream-warning'); });
try { new SimpleXMLElement('data://text/plain,%00', 0, true); }
catch (Exception $error) { echo get_class($error), ':', $error->getMessage(), ':', get_class($error->getPrevious()); }
restore_error_handler();
"#);
    assert_eq!(out, "nul|io|bool(false)\nException:String could not be parsed as XML:RuntimeException");
}

/// PHP schemes are case insensitive, while uppercase data fails and unknown filters only warn.
#[test]
fn test_simplexml_stream_scheme_and_filter_warning_contract() {
    if skip_without_xml_native("test_simplexml_stream_scheme_and_filter_warning_contract") { return; }
    let out = compile_and_run(r#"<?php
set_error_handler(function(int $level, string $message): bool {
    if (str_contains($message, 'Failed to open stream: operation failed')) { echo 'case|'; }
    elseif (str_contains($message, 'I/O warning')) { echo 'io|'; }
    else { echo $message, '|'; }
    return true;
});
var_dump(simplexml_load_file('DATA://text/plain,%3Cr%2F%3E'));
echo simplexml_load_file('PHP://filter/read=string.tolower/resource=data://text/plain,%3CR%2F%3E')->getName(), '|';
echo simplexml_load_file('php://filter/read=not-a-filter/resource=data://text/plain,%3Cr%2F%3E')->getName(), '|';
class RejectedXmlFilter extends php_user_filter {
    public function onCreate(): bool { return false; }
    public function filter($in, $out, &$consumed, bool $closing): int { return PSFS_PASS_ON; }
}
stream_filter_register('rejectedxmlfilter', 'RejectedXmlFilter');
echo simplexml_load_file('php://filter/read=rejectedxmlfilter/resource=data://text/plain,%3Cr%2F%3E')->getName();
restore_error_handler();
"#);
    assert_eq!(out, "case|io|bool(false)\nr|simplexml_load_file(): Unable to locate filter \"not-a-filter\"|simplexml_load_file(): Unable to create filter (not-a-filter)|r|simplexml_load_file(): Unable to create or locate filter \"rejectedxmlfilter\"|simplexml_load_file(): Unable to create filter (rejectedxmlfilter)|r");
}

/// A read exception releases the wrapper owner without running stream_close on the pending throw.
#[test]
fn test_simplexml_stream_read_exception_cleanup() {
    if skip_without_xml_native("test_simplexml_stream_read_exception_cleanup") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingXmlInput {
    public mixed $context = null;
    public string $owned = '';
    public function url_stat(string $path, int $flags): array { return ['mode' => 0100644]; }
    public function stream_open(string $path, string $mode, int $options, mixed &$opened_path): bool {
        $this->owned = str_repeat('x', 128); return true;
    }
    public function stream_read(int $count): string { throw new RuntimeException('read'); }
    public function stream_eof(): bool { return false; }
    public function stream_close(): void { echo 'unexpected-close'; }
    public function __destruct() {
        global $nestedXmlStream;
        if (is_resource($nestedXmlStream)) { fclose($nestedXmlStream); }
    }
}
class NestedXmlInput {
    public mixed $context = null;
    public function stream_open(string $path, string $mode, int $options, mixed &$opened_path): bool { return true; }
    public function stream_close(): void { echo 'nested-close|'; }
}
stream_wrapper_register('nestedxml', 'NestedXmlInput');
stream_wrapper_register('throwingxml', 'ThrowingXmlInput');
$nestedXmlStream = fopen('nestedxml://document', 'rb');
try { simplexml_load_file('throwingxml://document'); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
try { new SimpleXMLElement('throwingxml://document', 0, true); }
catch (Exception $error) { echo $error->getMessage(), ':', get_class($error->getPrevious()); }
stream_wrapper_unregister('throwingxml');
stream_wrapper_unregister('nestedxml');
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "nested-close|read|String could not be parsed as XML:RuntimeException");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Filter resources retain local file URL and localhost support in both public factories.
#[test]
fn test_simplexml_filtered_file_urls() {
    if skip_without_xml_native("test_simplexml_filtered_file_urls") { return; }
    let out = compile_and_run_capture(r#"<?php
$path = tempnam(sys_get_temp_dir(), 'sxml-filter-');
file_put_contents($path, '<R/>');
try {
    foreach (['file://' . $path, 'file://localhost' . $path] as $file) {
        $uri = 'php://filter/read=string.tolower/resource=' . $file;
        echo simplexml_load_file($uri)->getName(), ':', (new SimpleXMLElement($uri, 0, true))->getName(), '|';
    }
} finally { unlink($path); }
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "r:r|r:r|");
}

/// Invalid scalar stat results fail the XML open without dereferencing them as heap pointers.
#[test]
fn test_simplexml_stream_invalid_stat_result() {
    if skip_without_xml_native("test_simplexml_stream_invalid_stat_result") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class ScalarXmlStat {
    public mixed $context = null;
    public function url_stat(string $path, int $flags): bool { return true; }
    public function stream_open(string $path, string $mode, int $options, mixed &$opened_path): bool { echo 'unexpected-open'; return true; }
}
stream_wrapper_register('scalarxmlstat', 'ScalarXmlStat');
set_error_handler(function(int $level, string $message): bool { return true; });
var_dump(simplexml_load_file('scalarxmlstat://document'));
restore_error_handler();
stream_wrapper_unregister('scalarxmlstat');
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reading empty stdin closes each temporary wrapper without closing STDIN or retaining resources.
#[test]
fn test_simplexml_stdin_factories_retire_temporary_streams() {
    if skip_without_xml_native("test_simplexml_stdin_factories_retire_temporary_streams") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
$input = STDIN;
$before = count(get_resources('stream'));
$options = LIBXML_NOERROR | LIBXML_NOWARNING;
var_dump(simplexml_load_file('php://stdin', null, $options));
try { new SimpleXMLElement('php://stdin', $options, true); }
catch (Exception $error) { echo 'caught|'; }
echo get_resource_type($input), ':', count(get_resources('stream')) - $before;
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bool(false)\ncaught|stream:0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
