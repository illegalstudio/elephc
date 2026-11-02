//! Purpose:
//! Checks fileinfo's wrapper routing and public diagnostic attribution.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - Unknown schemes retain PHP's warned local fallback; registered wrappers still dispatch.
//! - Network fixtures use only loopback peers and preserve causes across notifications.

use crate::support::*;

/// Unknown schemes warn twice before PHP's local fallback, while invalid php URLs stay wrappers.
#[test]
fn test_fileinfo_wrapper_routing_and_warnings() {
    if fileinfo_native::skip("test_fileinfo_wrapper_routing_and_warnings") { return; }
    let out = compile_and_run(r#"<?php
$before = getcwd();
$dir = tempnam(sys_get_temp_dir(), "finfo-wrap");
unlink($dir); mkdir($dir); chdir($dir);
mkdir("unknown:"); file_put_contents("unknown:/payload", "HELLO");
mkdir("php:"); file_put_contents("php:/not-a-real-stream", "HELLO");
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
echo finfo_file($f, "unknown://payload"), "\n";
echo $f->file("php://not-a-real-stream") === false ? "php-failed\n" : "bad\n";
restore_error_handler();
class FileinfoReadWrapper {
    public $context;
    private int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool {
        if ($path === "finfowrap://warning") { trigger_error("fileinfo wrapper warning", E_USER_WARNING); return false; }
        return true;
    }
    public function stream_read(int $count): string { $value = substr("HELLO", $this->position, $count); $this->position += strlen($value); return $value; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_stat(): array { return ["mode" => 0100400, "size" => 5]; }
    public function stream_cast(int $kind): mixed { return false; }
}
stream_wrapper_register("finfowrap", "FileinfoReadWrapper");
echo $f->file("finfowrap://payload"), "\n";
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
echo $f->file("php://filter/resource=finfowrap://warning") === false ? "wrapper-failed\n" : "bad\n";
restore_error_handler();
stream_wrapper_unregister("finfowrap");
unlink("unknown:/payload"); rmdir("unknown:");
unlink("php:/not-a-real-stream"); rmdir("php:");
chdir($before); rmdir($dir);
"#);
    let warning = "finfo_file(): Unable to find the wrapper \"unknown\" - did you forget to enable it when you configured PHP?\n";
    assert_eq!(out, warning.repeat(2) + "text/plain\nfinfo::file(): Invalid php:// URL specified\nfinfo::file(php://not-a-real-stream): Failed to open stream: operation failed\nphp-failed\ntext/plain\nfileinfo wrapper warning\nfinfo::file(php://filter/resource=finfowrap://warning): Failed to open stream: operation failed\nwrapper-failed\n");
}

/// Root file URLs identify a directory and remote hosts fail silently during wrapper location.
#[test]
fn test_fileinfo_file_uri_root_and_remote_host() {
    if fileinfo_native::skip("test_fileinfo_file_uri_root_and_remote_host") { return; }
    let out = compile_and_run(r#"<?php
set_error_handler(function(int $n, string $message): bool { echo "unexpected warning\n"; return true; });
$f = new finfo(FILEINFO_MIME_TYPE);
echo finfo_file($f, "file://"), "\n";
echo $f->file("file://localhost/"), "\n";
echo finfo_file($f, "file://example.com/tmp") === false ? "remote\n" : "bad\n";
restore_error_handler();
"#);
    assert_eq!(out, "directory\ndirectory\nremote\n");
}

/// Data URL failures retain PHP's reason, original URL and all three public call styles.
#[test]
fn test_fileinfo_data_url_warning_frames() {
    if fileinfo_native::skip("test_fileinfo_data_url_warning_frames") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
finfo_file($f, "data:text");
$f->file("data:text/plain;base64;x=y,SEVMTE8=");
mime_content_type("data:;base64,****");
restore_error_handler();
"#);
    assert_eq!(out, "finfo_file(data:text): Failed to open stream: rfc2397: no comma in URL\nfinfo::file(data:text/plain;base64;x=y,SEVMTE8=): Failed to open stream: rfc2397: illegal parameter\nmime_content_type(data:;base64,****): Failed to open stream: rfc2397: unable to decode\n");
}

/// The case-sensitive data opener rejects mixed-case URLs even if a literal local path exists.
#[test]
fn test_fileinfo_data_scheme_case_sensitivity() {
    if fileinfo_native::skip("test_fileinfo_data_scheme_case_sensitivity") { return; }
    let out = compile_and_run(r#"<?php
$before = getcwd();
$dir = tempnam(sys_get_temp_dir(), "finfo-data");
unlink($dir); mkdir($dir); chdir($dir);
mkdir("DATA:"); mkdir("DATA:/text"); file_put_contents("DATA:/text/plain,HELLO", "HELLO");
mkdir("DATA:text"); file_put_contents("DATA:text/plain,HELLO", "HELLO");
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
foreach (["DATA", "Data", "dAta"] as $scheme) {
    echo finfo_file($f, $scheme . "://text/plain,HELLO") === false ? "failed\n" : "bad\n";
}
restore_error_handler();
echo $f->file("data://text/plain,HELLO"), "\n";
echo $f->file("DATA:text/plain,HELLO"), "\n";
unlink("DATA:/text/plain,HELLO"); rmdir("DATA:/text"); rmdir("DATA:");
unlink("DATA:text/plain,HELLO"); rmdir("DATA:text");
chdir($before); rmdir($dir);
"#);
    let expected = ["DATA", "Data", "dAta"].iter().map(|scheme|
        format!("finfo_file({scheme}://text/plain,HELLO): Failed to open stream: operation failed\nfailed\n")).collect::<String>();
    assert_eq!(out, expected + "text/plain\ntext/plain\n");
}

/// Failed filter resources report one outer warning, while direct remote file URLs stay silent.
#[test]
fn test_fileinfo_filter_resource_failure_frames() {
    if fileinfo_native::skip("test_fileinfo_filter_resource_failure_frames") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
foreach (["file://example.com/tmp", "data:text", "/elephc-fileinfo-missing/file", "php://filter/resource=data:text"] as $resource) {
    echo finfo_file($f, "php://filter/resource=" . $resource) === false ? "failed\n" : "bad\n";
}
echo finfo_file($f, "file://example.com/tmp") === false ? "silent\n" : "bad\n";
restore_error_handler();
echo $f->file("php://filter/read=string.toupper/resource=data:text/plain,HELLO"), "\n";
$file = tempnam(sys_get_temp_dir(), "finfo-bytes");
file_put_contents($file, "HELLO\0binary");
echo $f->file("php://filter/read=string.toupper/resource=" . $file), "\n";
unlink($file); unset($file, $resource, $f);
"#);
    let mut expected = String::new();
    for resource in ["file://example.com/tmp", "data:text", "/elephc-fileinfo-missing/file", "php://filter/resource=data:text"] {
        expected.push_str(&format!("finfo_file(php://filter/resource={resource}): Failed to open stream: operation failed\nfailed\n"));
    }
    expected.push_str("silent\ntext/plain\napplication/octet-stream\n");
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested filter directories retain descriptor identity across ordinary, file URL and eval queries.
#[test]
fn test_fileinfo_filtered_directories() {
    if fileinfo_native::skip("test_fileinfo_filtered_directories") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo "unexpected:", $message, "\n"; return true; });
foreach (["php://filter/read=string.rot13/resource=.",
          "php://filter/read=string.toupper/resource=file://" . getcwd(),
          "php://filter/read=zlib.inflate/resource=.",
          "php://filter/read=string.toupper/resource=php://filter/read=string.rot13/resource=."] as $path) {
    echo finfo_file($f, $path), "\n";
}
echo mime_content_type("php://filter/read=string.rot13/resource=."), "\n";
$code = $argc > 0 ? 'echo $f->file("php://filter/read=string.rot13/resource=."), "\n";' : '';
eval($code);
restore_error_handler(); unset($f, $path, $code);
"#);
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, "directory\n".repeat(6));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// No-chain URLs tokenize resource segments as filters, with both public warning frames preserved.
#[test]
fn test_fileinfo_filter_resource_tokenization() {
    if fileinfo_native::skip("test_fileinfo_filter_resource_tokenization") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
echo finfo_file($f, "php://filter/resource=data:text/plain,HELLO"), "\n";
echo $f->file("php://filter/read=missing/resource=data:,HELLO"), "\n";
echo mime_content_type("php://filter/read=missing/resource=data:,HELLO"), "\n";
echo $f->file("php://filter/resource=."), "\n";
$database = tempnam(sys_get_temp_dir(), "finfo-chain");
file_put_contents($database, "0 string HELLO transformed\n!:mime application/x-fileinfo-filter\n");
$custom = new finfo(FILEINFO_MIME_TYPE, $database);
echo $custom->file("php://filter/resource=data:,URYYB/string.rot13"), "\n";
unlink($database); restore_error_handler();
"#);
    let warnings = |function: &str, name: &str| format!("{function}(): Unable to locate filter \"{name}\"\n{function}(): Unable to create filter ({name})\n");
    let expected = warnings("finfo_file", "resource=data:text") + &warnings("finfo_file", "plain,HELLO") + "text/plain\n"
        + &warnings("finfo::file", "missing") + "text/plain\n"
        + &warnings("mime_content_type", "missing") + "text/plain\n"
        + &warnings("finfo::file", "resource=.") + "directory\n"
        + &warnings("finfo::file", "resource=data:,URYYB") + "application/x-fileinfo-filter\n";
    assert_eq!(out, expected);
}

/// Wrapper diagnostics cap the displayed scheme at 31 bytes while retaining the original path.
#[test]
fn test_fileinfo_unknown_wrapper_name_truncation() {
    if fileinfo_native::skip("test_fileinfo_unknown_wrapper_name_truncation") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
echo finfo_file($f, str_repeat("a", 40) . "://missing") === false ? "failed\n" : "bad\n";
restore_error_handler();
"#);
    let warning = format!("finfo_file(): Unable to find the wrapper \"{}\" - did you forget to enable it when you configured PHP?\n", "a".repeat(31));
    assert_eq!(out, warning.repeat(2) + &format!("finfo_file({}://missing): Failed to open stream: No such file or directory\nfailed\n", "a".repeat(40)));
}

/// Error callbacks observe temporary options, which are restored after normal and throwing handlers.
#[test]
fn test_fileinfo_temporary_flags_during_warning_callbacks() {
    if fileinfo_native::skip("test_fileinfo_temporary_flags_during_warning_callbacks") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_NONE);
set_error_handler(function(int $n, string $message) use ($f): bool { echo $f->buffer("HELLO"), "\n"; return true; });
echo finfo_file($f, "fileinfo-missing-scheme://payload", FILEINFO_MIME_TYPE) === false ? "failed\n" : "bad\n";
restore_error_handler();
echo $f->buffer("HELLO"), "\n";
set_error_handler(function(int $n, string $message) use ($f): bool { echo $f->buffer("HELLO"), "\n"; throw new RuntimeException("handler"); });
try { $f->file("data:text", FILEINFO_MIME_TYPE); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
restore_error_handler();
echo $f->buffer("HELLO"), "\n";
"#);
    let description = "ASCII text, with no line terminators\n";
    assert_eq!(out, "text/plain\n".repeat(3) + "failed\n" + description + "text/plain\nhandler\n" + description);
}

/// Mixed MIME arguments report the offending scalar, null or object type.
#[test]
fn test_fileinfo_mime_argument_type_names() {
    if fileinfo_native::skip("test_fileinfo_mime_argument_type_names") { return; }
    let out = compile_and_run(r#"<?php
try { mime_content_type(1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mime_content_type(null); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { mime_content_type(new stdClass()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#);
    let prefix = "mime_content_type(): Argument #1 ($filename) must be of type resource|string, ";
    assert_eq!(out, format!("{prefix}int given\n{prefix}null given\n{prefix}stdClass given\n"));
}

/// Database expansion removes dot components even when the intermediate directory is absent.
#[test]
fn test_fileinfo_relative_database_expansion() {
    if fileinfo_native::skip("test_fileinfo_relative_database_expansion") { return; }
    let out = compile_and_run(r#"<?php
$cwd = getcwd();
$f = new finfo(FILEINFO_MIME_TYPE);
try { new finfo(0, "./absent-part/../missing.mgc"); } catch (Exception $e) {
    echo $e->getMessage() === 'finfo::__construct(): Failed to load magic database at "' . $cwd . '/missing.mgc"' ? "constructor\n" : $e->getMessage();
}
set_error_handler(function(int $n, string $message) use ($cwd): bool {
    echo $message === 'finfo_open(): Failed to load magic database at "' . $cwd . '/missing.mgc"' ? "procedural\n" : $message;
    return true;
});
finfo_open(0, "./absent-part/../missing.mgc");
restore_error_handler();
echo $f->buffer("HELLO"), "\n";
"#);
    assert_eq!(out, "constructor\nprocedural\ntext/plain\n");
}

/// Every network scheme reports the original URL and the captured connection-refused cause.
#[test]
fn test_fileinfo_network_failure_diagnostics() {
    if fileinfo_native::skip("test_fileinfo_network_failure_diagnostics") { return; }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let source = format!(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool {{ echo $message, "\n"; return true; }});
foreach (["http", "https", "ftp", "ftps", "HTTP"] as $scheme) {{
    echo finfo_file($f, $scheme . "://127.0.0.1:{port}/payload") === false ? "failed\n" : "bad\n";
}}
restore_error_handler();
"#);
    let out = compile_and_run(&source);
    let expected = ["http", "https", "ftp", "ftps", "HTTP"].iter().map(|scheme|
        format!("finfo_file({scheme}://127.0.0.1:{port}/payload): Failed to open stream: Connection refused\nfailed\n")).collect::<String>();
    assert_eq!(out, expected);
}

/// Notification callbacks may perform another failed connection without replacing the outer cause.
#[test]
fn test_fileinfo_network_failure_notification_cause() {
    if fileinfo_native::skip("test_fileinfo_network_failure_notification_cause") { return; }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let source = format!(r#"<?php
$context = stream_context_create([], ["notification" => function($code, $severity, $message, $message_code, $bytes, $maximum) {{
    if ($code === STREAM_NOTIFY_FAILURE) {{ echo "notify\n"; @stream_socket_client("unix:///elephc-fileinfo-missing/socket"); }}
}}]);
set_error_handler(function(int $n, string $message): bool {{ if (str_starts_with($message, "finfo")) {{ echo $message, "\n"; }} return true; }});
$f = new finfo(FILEINFO_MIME_TYPE);
echo finfo_file($f, "http://127.0.0.1:{port}/", 0, $context) === false ? "failed\n" : "bad\n";
restore_error_handler();
"#);
    assert_eq!(compile_and_run(&source), format!("notify\nfinfo_file(http://127.0.0.1:{port}/): Failed to open stream: Connection refused\nfailed\n"));
}

/// A failed initial TLS read propagates its handshake cause instead of identifying empty content.
#[test]
fn test_fileinfo_tls_handshake_failure_diagnostics() {
    use std::io::{Read, Write};
    if fileinfo_native::skip("test_fileinfo_tls_handshake_failure_diagnostics") { return; }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let mut hello = [0u8; 1024];
        let _ = stream.read(&mut hello);
        let _ = stream.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n");
    });
    let source = format!(r#"<?php
set_error_handler(function(int $n, string $message): bool {{
    $prefix = "finfo_file(https://127.0.0.1:{port}/): Failed to open stream: ";
    echo str_starts_with($message, $prefix) && $message !== $prefix . "operation failed" ? "cause\n" : $message;
    return true;
}});
$f = new finfo(FILEINFO_MIME_TYPE);
echo finfo_file($f, "https://127.0.0.1:{port}/") === false ? "failed\n" : "bad\n";
restore_error_handler();
"#);
    let out = compile_and_run(&source);
    server.join().unwrap();
    assert_eq!(out, "cause\nfailed\n");
}
