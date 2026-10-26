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
    let out = compile_and_run(r#"<?php
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
    assert_eq!(out, "close|r|close|r");
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
