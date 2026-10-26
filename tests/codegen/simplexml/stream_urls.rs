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

/// Includes only stream helper bodies when a remote heap regression needs producer diagnostics.
fn stream_owner_assembly(assembly: &str) -> String {
    let symbols: Vec<_> = ["__elephc_simplexml_file_handle", "__elephc_simplexml_stream_bytes", "__elephc_simplexml_data_bytes", "__elephc_simplexml_filter_bytes", "__elephc_simplexml_read_stream", "__elephc_simplexml_close_stream"]
        .into_iter().flat_map(|name| {
            let symbol = elephc::names::function_symbol(name);
            [symbol.clone(), format!("{symbol}__cdylib_exception_cleanup")]
        }).collect();
    let mut selected = selected_function_assembly(assembly, &symbols);
    let lines: Vec<_> = assembly.lines().collect();
    for (index, _) in lines.iter().enumerate().filter(|(_, line)|
        line.trim_end().ends_with(':') && line.contains("mixed_method_SimpleXMLElement_")
    ).take(4) {
        selected.push_str(&lines[index..(index + 48).min(lines.len())].join("\n"));
        selected.push('\n');
    }
    selected
}

/// Data and filtered URLs work through both factories and the shared eval prelude.
#[test]
fn test_simplexml_stream_data_and_filters_bundle() {
    if skip_without_xml_native("test_simplexml_stream_data_and_filters_bundle") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r###"<?php
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
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\nStream owner assembly:\n{}", out.stderr, stream_owner_assembly(&assembly));
}

/// Isolates stream factories, filter dispatch and string processing with one shared eval catalog.
#[test]
fn test_simplexml_stream_document_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_stream_document_owners_isolated_bundle") { return; }
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r###"<?php
$seed = new SimpleXMLElement('<r/>');
$uri = 'data://text/plain;base64,PHIvPg==';
if ($argc === 1) { echo 'baseline'; }
elseif ($argc === 2) { echo simplexml_load_file($uri)->getName(); }
elseif ($argc === 3) { echo (new SimpleXMLElement($uri, 0, true))->getName(); }
elseif ($argc === 4) { echo (new SimpleXMLElement($uri . "\0ignored", 0, true))->getName(); }
elseif ($argc === 5) { echo simplexml_load_file('data://text/plain,%3Cr%3Eone+two%3C%2Fr%3E'); }
elseif ($argc === 6) { echo simplexml_load_file('php://filter/read=string.tolower/resource=data://text/plain,%3CR%3EYES%3C%2FR%3E')->getName(); }
elseif ($argc === 7) {
    $source = $argc > 0 ? 'echo simplexml_load_file($uri)->getName(), ":", (new SimpleXMLElement($uri, 0, true))->getName();' : '';
    eval($source);
} elseif ($argc === 8) { var_dump(simplexml_load_file('php://input', null, LIBXML_NOERROR | LIBXML_NOWARNING)); }
elseif ($argc === 9) { echo strlen(__elephc_simplexml_filter_bytes('php://filter/read=string.tolower/resource=data://text/plain,%3CR%3EYES%3C%2FR%3E', 'simplexml_load_file()')); }
elseif ($argc === 10) {
    echo __elephc_simplexml_filter_bytes('php://filter/resource=data://text/plain,%3CR%3EYES%3C%2FR%3E', 'simplexml_load_file()');
} elseif ($argc === 11) {
    $stream = fopen('php://temp', 'w+'); fwrite($stream, '<R>YES</R>'); rewind($stream);
    stream_filter_append($stream, 'string.tolower', STREAM_FILTER_READ);
    echo stream_get_contents($stream); fclose($stream);
} elseif ($argc === 12) {
    $stream = fopen('php://temp', 'w+'); fwrite($stream, '<R>YES</R>'); rewind($stream);
    $filter_name = strtolower('STRING.TOLOWER');
    stream_filter_append($stream, $filter_name, STREAM_FILTER_READ);
    echo stream_get_contents($stream); fclose($stream);
} elseif ($argc === 13) {
    foreach (explode('/', 'read=string.tolower') as $filter_list) {
        if (str_starts_with($filter_list, 'read=')) { $filter_list = substr($filter_list, 5); }
        foreach (explode('|', $filter_list) as $filter) { echo $filter; }
    }
} else {
    echo strlen(__elephc_simplexml_filter_bytes('php://filter/read=string.tolower/resource=data://text/plain,%3CR%3EYES%3C%2FR%3E   ', 'simplexml_load_file()'));
}
"###, &[
        &[], &["x"], &["x", "x"], &["x", "x", "x"],
        &["x", "x", "x", "x"], &["x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x"], &["x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
        &["x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x"],
    ]);
    let expected = ["baseline", "r", "r", "r", "one two", "r", "r:r", "bool(false)\n", "10", "<R>YES</R>", "<r>yes</r>", "<r>yes</r>", "string.tolower", "13"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}\nStream owner assembly:\n{}", failures.join("\n"), stream_owner_assembly(&assembly));
}

/// Registered PHP wrappers supply document bytes without any libxml global callback.
#[test]
fn test_simplexml_registered_stream_document() {
    if skip_without_xml_native("test_simplexml_registered_stream_document") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
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
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\nStream owner assembly:\n{}", out.stderr, stream_owner_assembly(&assembly));
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

/// Separates wrapper lifecycle, read failures and mutable URI parameter cleanup on exception.
#[test]
fn test_simplexml_throwing_stream_uri_owners_isolated_bundle() {
    if skip_without_xml_native("test_simplexml_throwing_stream_uri_owners_isolated_bundle") { return; }
    let args: Vec<_> = (0..14).map(|count| vec!["x"; count]).collect();
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
class IsolatedThrowingXmlInput {
    public mixed $context = null;
    public function url_stat(string $path, int $flags): array { return ['mode' => 0100644]; }
    public function stream_open(string $path, string $mode, int $options, mixed &$opened_path): bool { return true; }
    public function stream_read(int $count): string { throw new RuntimeException('read'); }
    public function stream_eof(): bool { return false; }
    public function stream_close(): void { echo 'close|'; }
}
function throwImmutableXmlUri(string $uri): void { throw new RuntimeException('read'); }
function throwMutableXmlUri(string $uri, bool $change): void {
    if ($change) { $uri = 'changed'; }
    throw new RuntimeException('read');
}
stream_wrapper_register('isolatedxml', 'IsolatedThrowingXmlInput');
$uri = 'isolatedxml://document';
if ($argc === 1) { echo 'registered'; }
elseif ($argc === 2) { $stream = fopen($uri, 'rb'); fclose($stream); }
elseif ($argc === 3) {
    $stream = fopen($uri, 'rb');
    try { stream_get_contents($stream); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
    fclose($stream);
} elseif ($argc === 4) {
    try { simplexml_load_file($uri); } catch (RuntimeException $error) { echo $error->getMessage(); }
} elseif ($argc === 5) {
    try { new SimpleXMLElement($uri, 0, true); }
    catch (Exception $error) {
        $previous = $error->getPrevious();
        if ($previous instanceof RuntimeException) { echo $previous->getMessage(); }
        else { echo 'missing-previous'; }
    }
} elseif ($argc === 6) {
    try { __elephc_simplexml_stream_bytes($uri, 'simplexml_load_file()'); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
} elseif ($argc === 7) {
    try { throwImmutableXmlUri($uri); } catch (RuntimeException $error) { echo $error->getMessage(); }
} elseif ($argc === 8) {
    try { throwMutableXmlUri($uri, $argc < 0); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
} elseif ($argc === 9) { echo (int)file_exists($uri); }
elseif ($argc === 10) {
    $dynamicUri = str_repeat($uri, 1); echo (int)file_exists($dynamicUri);
} elseif ($argc === 11) {
    $stream = fopen($uri, 'rb');
    try { __elephc_simplexml_read_stream($stream); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
} elseif ($argc === 12) {
    echo (int)in_array('isolatedxml', stream_get_wrappers(), true);
} elseif ($argc === 13) {
    $dynamicUri = str_repeat($uri, 1);
    $stream = fopen($dynamicUri, 'rb');
    try { __elephc_simplexml_read_stream($stream); }
    catch (RuntimeException $error) { echo $error->getMessage(); }
} else {
    $separator = strpos($uri, '://');
    echo strtolower(substr($uri, 0, intval($separator)));
}
stream_wrapper_unregister('isolatedxml');
"#, &argvs);
    let expected = ["registered", "close|", "read|close|", "read", "read", "read", "read", "read", "1", "1", "read", "1", "read", "isolatedxml"];
    assert_eq!(runs.len(), expected.len());
    let mut failures = Vec::new();
    for (mode, (out, expected)) in runs.iter().zip(expected).enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} success={} stdout={:?} expected={expected:?}\n{}", out.success, out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}\nThrowing stream assembly:\n{}", failures.join("\n"), stream_owner_assembly(&assembly));
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
