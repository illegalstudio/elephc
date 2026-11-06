//! Purpose:
//! Exercises the complete PHP fileinfo surface, including native and eval ownership.
//!
//! Called from:
//! - The codegen integration harness, on every executable supported target.
//!
//! Key details:
//! - Requires the pinned managed libmagic artifact; CI refuses silent skips.

use crate::support::*;

/// Both PHP call styles share MIME, encoding and description modes, including binary bytes.
#[test]
fn test_fileinfo_buffer_flags_and_aliases() {
    if fileinfo_native::skip("test_fileinfo_buffer_flags_and_aliases") { return; }
    let out = compile_and_run(r#"<?php
$f = new \FiNfO(FILEINFO_MIME_TYPE);
echo \FiNfO_BuFfEr(finfo: $f, string: "hello"), "\n";
echo $f->buffer("hello", FILEINFO_MIME_ENCODING), "\n";
echo $f->buffer("hello"), "\n";
echo finfo_buffer($f, "hello", FILEINFO_MIME), "\n";
echo $f->buffer("\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0"), "\n";
echo $f->buffer(""), "\n";
echo finfo_set_flags($f, FILEINFO_NONE) ? "set\n" : "bad\n";
echo $f->buffer("hello"), "\n";
echo finfo_close($f) ? "closed\n" : "bad\n";
echo $f->buffer("hello", FILEINFO_MIME_TYPE), "\n";
"#);
    assert_eq!(out, "text/plain\nus-ascii\ntext/plain\ntext/plain; charset=us-ascii\nimage/png\napplication/x-empty\nset\nASCII text, with no line terminators\nclosed\ntext/plain\n");
}

/// Files, stream URLs and resource inputs preserve PHP results and stream position.
#[test]
fn test_fileinfo_files_and_streams() {
    if fileinfo_native::skip("test_fileinfo_files_and_streams") { return; }
    let out = compile_and_run(r#"<?php
$f = finfo_open(FILEINFO_MIME_TYPE);
if ($f === false) { throw new RuntimeException("Could not open fileinfo"); }
$p = tempnam(sys_get_temp_dir(), "finfo");
file_put_contents($p, "hello\n");
echo finfo_file($f, $p), "\n";
echo $f->file("file://" . $p), "\n";
echo mime_content_type($p), "\n";
$s = fopen($p, "rb");
fseek($s, 3);
echo mime_content_type($s), ":", ftell($s), "\n";
fclose($s);
$context = stream_context_create();
echo get_resource_type($context), "\n";
echo $f->file("data://text/plain;base64,aGVsbG8=", 0, $context), "\n";
echo $f->file(sys_get_temp_dir()), "\n";
echo $f->file("/dev/null"), "\n";
unlink($p);
"#);
    assert_eq!(out, "text/plain\ntext/plain\ntext/plain\ntext/plain:3\nstream-context\ntext/plain\ndirectory\napplication/x-empty\n");
}

/// Plain data URLs use form decoding while escaped and base64 plus bytes remain literal.
#[test]
fn test_fileinfo_data_url_decoding() {
    if fileinfo_native::skip("test_fileinfo_data_url_decoding") { return; }
    let out = compile_and_run(r#"<?php
$p = tempnam(sys_get_temp_dir(), "magic");
file_put_contents($p, "0 string Hello\\040world decoded-space\n0 string Hello+world decoded-plus\n");
$f = new finfo(FILEINFO_NONE, $p);
echo $f->file("data://text/plain,Hello+world"), "\n";
echo finfo_file($f, "data://text/plain,Hello%2Bworld"), "\n";
echo $f->file("data://text/plain;base64,SGVsbG8rd29ybGQ="), "\n";
$code = $argc > 0 ? 'echo $f->file("data://text/plain,Hello+world"), "\n";' : '';
eval($code);
unlink($p);
"#);
    assert_eq!(out, "decoded-space\ndecoded-plus\ndecoded-plus\ndecoded-space\n");
}

/// Runtime wrapper names preserve filters, nested resources and PHP's compressed stat failures.
#[test]
fn test_fileinfo_runtime_stream_wrappers() {
    if fileinfo_native::skip("test_fileinfo_runtime_stream_wrappers") { return; }
    let out = compile_and_run(r#"<?php
$db = tempnam(sys_get_temp_dir(), "magic");
$p = tempnam(sys_get_temp_dir(), "finfo");
$gz = tempnam(sys_get_temp_dir(), "gzip");
$bz = tempnam(sys_get_temp_dir(), "bzip");
$raw = tempnam(sys_get_temp_dir(), "deflate");
file_put_contents($db, "0 string HELLO decoded-document\n");
file_put_contents($p, "URYYB");
file_put_contents($gz, base64_decode("H4sIAAAAAAAAA/Nw9fHxBwA2ZETBBQAAAA=="));
file_put_contents($bz, base64_decode("QlpoOTFBWSZTWXMoMkUAAACEAAJEoAAhmmgzTQczi7kinChIOZQZIoA="));
file_put_contents($raw, base64_decode("83D18fEHAA=="));
$f = new finfo(FILEINFO_NONE, $db);
echo $f->file("php://filter/read=string.rot13/resource=" . $p), "\n";
echo finfo_file($f, "php://filter/read=string.rot13|string.toupper/resource=data://text/plain,uryyb"), "\n";
echo $f->file("php://filter/read=convert.base64-decode/resource=data://text/plain,SEVMTE8="), "\n";
echo $f->file("php://filter/read=convert.quoted-printable-decode/resource=data://text/plain,=48=45=4c=4c=4f"), "\n";
echo $f->file("php://filter/write=string.rot13/resource=data://text/plain,HELLO"), "\n";
echo $f->file("php://filter/read=zlib.inflate/resource=" . $raw), "\n";
echo $f->file("php://filter/read=zlib.deflate|zlib.inflate/resource=data://text/plain,HELLO"), "\n";
echo $f->file("php://filter/read=bzip2.decompress/resource=" . $bz), "\n";
$warned = false;
set_error_handler(function (int $level, string $message) use (&$warned) { $warned = $level === E_WARNING; return true; });
echo $f->file("php://filter/read=elephc.missing/resource=data://text/plain,HELLO"), ":", $warned ? "warned\n" : "silent\n";
restore_error_handler();
echo @$f->file("compress.zlib://" . $gz) === false ? "gzip-unstatable\n" : "bad\n";
echo @finfo_file($f, "compress.bzip2://" . $bz) === false ? "bzip-unstatable\n" : "bad\n";
$code = $argc > 0 ? 'echo $f->file("php://filter/read=string.rot13/resource=" . $p), "\n"; echo $f->file("php://filter/read=zlib.inflate/resource=" . $raw), "\n"; echo $f->file("php://filter/read=bzip2.decompress/resource=" . $bz), "\n"; echo $f->file("compress.zlib://" . $gz) === false ? "gzip-unstatable\n" : "bad\n"; echo finfo_file($f, "compress.bzip2://" . $bz) === false ? "bzip-unstatable\n" : "bad\n";' : '';
eval($code);
unlink($db);
unlink($p);
unlink($gz);
unlink($bz);
unlink($raw);
"#);
    assert_eq!(out, "decoded-document\n".repeat(8) + "decoded-document:warned\ngzip-unstatable\nbzip-unstatable\n" + &"decoded-document\n".repeat(3) + "gzip-unstatable\nbzip-unstatable\n");
}

/// Standard-input wrappers read their bytes without closing the process's borrowed descriptor.
#[test]
fn test_fileinfo_standard_input_wrapper() {
    if fileinfo_native::skip("test_fileinfo_standard_input_wrapper") { return; }
    let out = compile_and_run_with_stdin(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
echo $f->file("php://stdin"), "\n";
echo $f->file("php://stdin"), "\n";
echo get_resource_type(STDIN), "\n";
"#, "hello\n");
    assert_eq!(out, "text/plain\napplication/x-empty\nstream\n");
}

/// Filter stages release their byte buffers and temporary stream cells on normal returns.
#[test]
fn test_fileinfo_filtered_stream_heap_ownership() {
    if fileinfo_native::skip("test_fileinfo_filtered_stream_heap_ownership") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
for ($i = 0; $i < 3; $i++) {
    $value = $f->file("php://filter/read=string.rot13|string.toupper/resource=data://text/plain,uryyb");
    unset($value);
    $decoded = $f->file("php://filter/read=bzip2.decompress/resource=data://application/octet-stream;base64,QlpoOTFBWSZTWXMoMkUAAACEAAJEoAAhmmgzTQczi7kinChIOZQZIoA=");
    echo $decoded, "\n";
    unset($decoded);
    $broken = $f->file("php://filter/read=bzip2.decompress/resource=data://text/plain,invalid");
    unset($broken);
}
unset($f);
echo "ok\n";
"#);
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, "text/plain\ntext/plain\ntext/plain\nok\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Highly compressible filter input grows its output buffer and releases every allocation.
#[test]
fn test_fileinfo_high_ratio_inflate() {
    if fileinfo_native::skip("test_fileinfo_high_ratio_inflate") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$db = tempnam(sys_get_temp_dir(), "magic");
$raw = tempnam(sys_get_temp_dir(), "deflate");
file_put_contents($db, "700000 string ELEPHC expanded-document\n");
$payload = str_repeat("x", 700000) . "ELEPHC" . str_repeat("y", 300000);
$compressed = gzdeflate($payload);
file_put_contents($raw, $compressed);
$decoded = gzinflate($compressed);
echo $decoded === $payload ? "complete\n" : "truncated\n";
unset($decoded, $payload, $compressed);
$f = new finfo(FILEINFO_NONE, $db);
echo $f->file("php://filter/read=zlib.inflate/resource=" . $raw), "\n";
unset($f);
$invalid = @gzinflate("invalid");
echo $invalid === false ? "invalid\n" : "bad\n";
unset($invalid);
unlink($db);
unlink($raw);
unset($db, $raw);
"#);
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, "complete\nexpanded-document\ninvalid\n");
    let mut failures = Vec::new();
    if !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
        failures.push(format!("high-ratio file query: {}", out.stderr));
    }
    for (label, source) in [
        ("invalid inflate", "<?php $value = @gzinflate('invalid'); unset($value); echo 'ok';"),
        ("compression", "<?php $source = str_repeat('x', 1000000); $packed = gzdeflate($source); $value = gzinflate($packed); unset($value, $packed, $source); echo 'ok';"),
        ("payload concatenation", "<?php $source = str_repeat('x', 700000) . 'ELEPHC' . str_repeat('y', 300000); unset($source); echo 'ok';"),
        ("temporary paths", "<?php $db = tempnam(sys_get_temp_dir(), 'magic'); $raw = tempnam(sys_get_temp_dir(), 'deflate'); unlink($db); unlink($raw); unset($db, $raw); echo 'ok';"),
    ] {
        let (control, asm) = compile_and_run_with_heap_debug_and_asm(source);
        assert_eq!(control.stdout, "ok", "{label}");
        if !control.stderr.contains("HEAP DEBUG: leak summary: clean") {
            let main = asm.lines().skip_while(|line| !line.trim_end().ends_with("main:")).take(550).collect::<Vec<_>>().join("\n");
            eprintln!("Fileinfo {label} control assembly:\n{main}");
            failures.push(format!("{label}: {}", control.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Network opens preserve HTTP context options but PHP 8.5 rejects streams without stream stat.
#[test]
fn test_fileinfo_network_stream_context() {
    use std::io::{Read, Write};
    if fileinfo_native::skip("test_fileinfo_network_stream_context") { return; }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while stream.read(&mut byte).unwrap_or(0) == 1 {
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") { break; }
        }
        stream.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: 5\r\n\r\nhello").unwrap();
        String::from_utf8(request).unwrap()
    });
    let out = compile_and_run(&format!(r#"<?php
$context = stream_context_create(["http" => ["method" => "POST", "header" => "X-Fileinfo: yes\r\n"]]);
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool {{ echo "unexpected:", $message; return true; }});
echo $f->file("http://127.0.0.1:{port}/payload", 0, $context) === false ? "unstatable" : "bad";
restore_error_handler();
"#));
    assert_eq!(out, "unstatable");
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /payload HTTP/"), "{request}");
    assert!(request.contains("X-Fileinfo: yes\r\n"), "{request}");
}

/// User databases support descriptions, MIME annotations, extensions and persistent options.
#[test]
fn test_fileinfo_custom_database() {
    if fileinfo_native::skip("test_fileinfo_custom_database") { return; }
    let out = compile_and_run(r#"<?php
$p = tempnam(sys_get_temp_dir(), "magic");
file_put_contents($p, "0 string ELEPHC Elephc document\n!:mime application/x-elephc\n!:ext elph\n");
$f = new finfo(FILEINFO_NONE, $p);
echo $f->buffer("ELEPHC\0body"), "\n";
echo $f->buffer("ELEPHC\0body", FILEINFO_MIME_TYPE), "\n";
echo $f->buffer("ELEPHC\0body", FILEINFO_EXTENSION), "\n";
echo $f->buffer("ELEPHC\0body"), "\n";
unlink($p);
"#);
    assert_eq!(out, "Elephc document\napplication/x-elephc\nelph\nElephc document\n");
}

/// Native failures and invalid object operations are recoverable without invalidating peers.
#[test]
fn test_fileinfo_failures_clone_and_serialization() {
    if fileinfo_native::skip("test_fileinfo_failures_clone_and_serialization") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
echo @$f->file("/elephc-fileinfo-missing/file") === false ? "missing\n" : "bad\n";
echo @finfo_open(0, "/elephc-fileinfo-missing/magic") === false ? "database\n" : "bad\n";
try { new finfo(0, "/elephc-fileinfo-missing/magic"); } catch (Exception $e) { echo "constructor\n"; }
try { $f->file(""); } catch (ValueError $e) { echo "empty\n"; }
try { $f->file("a\0b"); } catch (ValueError $e) { echo "nul\n"; }
try { $f->__construct(0, "a\0b"); } catch (ValueError $e) { echo "database-nul\n"; }
try { $g = clone $f; } catch (Error $e) { echo "clone\n"; }
try { serialize($f); } catch (Exception $e) { echo "serialize\n"; }
echo $f->buffer("hello"), "\n";
"#);
    assert_eq!(out, "missing\ndatabase\nconstructor\nempty\nnul\ndatabase-nul\nclone\nserialize\ntext/plain\n");
}

/// Runtime-unknown eval, named arguments and callable aliases use the host implementation.
#[test]
fn test_fileinfo_eval_bundle() {
    if fileinfo_native::skip("test_fileinfo_eval_bundle") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
$code = $argc > 0 ? '$x = finfo_open(flags: FILEINFO_MIME_TYPE); echo get_class($x), ":", finfo_buffer(string: "hello", finfo: $x), "\n"; $fn = "finfo_buffer"; echo $fn($x, "hello"), "\n"; echo $x->buffer("hello", FILEINFO_MIME_ENCODING), "\n"; finfo_close($x); echo $x->buffer("hello"), "\n"; finfo_set_flags($x, FILEINFO_MIME_TYPE); echo finfo_file($x, "data://text/plain,hello"), ":", mime_content_type(filename: "data://text/plain,hello"), "\n";' : '';
eval($code);
"#);
    assert_eq!(out, "finfo:text/plain\ntext/plain\nus-ascii\ntext/plain\ntext/plain:text/plain\n");
}

/// Repeated object construction and destruction balance compiler heap ownership.
#[test]
fn test_fileinfo_heap_ownership() {
    if fileinfo_native::skip("test_fileinfo_heap_ownership") { return; }
    let mut failures = Vec::new();
    for (label, operation) in [
        ("construct", ""),
        ("query", "$s = $f->buffer(\"hello\");"),
        ("close", "finfo_close($f);"),
        ("query and alias", "$s = $f->buffer(\"hello\"); $g = $f; finfo_close($g); unset($g);"),
    ] {
        let source = format!(r#"<?php
for ($i = 0; $i < 10; $i++) {{
    $f = new finfo(FILEINFO_MIME_TYPE);
    {operation}
    unset($f);
}}
echo "ok\n";
"#);
        let (out, asm) = compile_and_run_with_heap_debug_and_asm(&source);
        assert!(out.success, "fileinfo program failed: {}", out.stderr);
        assert_eq!(out.stdout, "ok\n", "{label}");
        if !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            eprintln!("Fileinfo {label} assembly:\n{asm}");
            failures.push(format!("{label}: {}", out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// All eleven fileinfo constants resolve identically without enabling the native bridge.
#[test]
fn test_fileinfo_constant_catalog() {
    let out = compile_and_run(r#"<?php
echo FILEINFO_NONE, ",", FILEINFO_SYMLINK, ",", FILEINFO_DEVICES, ",", FILEINFO_MIME_TYPE, ",", FILEINFO_CONTINUE, ",", FILEINFO_PRESERVE_ATIME, ",", FILEINFO_RAW, ",", FILEINFO_MIME_ENCODING, ",", FILEINFO_MIME, ",", FILEINFO_APPLE, ",", FILEINFO_EXTENSION;
"#);
    assert_eq!(out, "0,2,8,16,32,128,256,1024,1040,2048,16777216");
}

/// Continuation, raw descriptions, Apple metadata and extension flags use the magic engine.
#[test]
fn test_fileinfo_extended_options() {
    if fileinfo_native::skip("test_fileinfo_extended_options") { return; }
    let out = compile_and_run(r#"<?php
$p = tempnam(sys_get_temp_dir(), "magic");
file_put_contents($p, "0 string ELEPHC First document\n!:apple ELPHDocu\n!:ext elph\n0 string ELEPHC Second document\n!:apple ELPHDocu\n!:ext elph\n");
$f = new finfo(FILEINFO_NONE, $p);
echo $f->buffer("ELEPHC\0data", FILEINFO_APPLE), "\n";
echo $f->buffer("ELEPHC\0data", FILEINFO_EXTENSION), "\n";
$description = $f->buffer("ELEPHC\0data", FILEINFO_CONTINUE | FILEINFO_RAW);
echo strpos($description, "First document") !== false ? "first\n" : "bad\n";
echo strpos($description, "Second document") !== false ? "second\n" : "bad\n";
unlink($p);
"#);
    assert_eq!(out, "ELPHDocu\nelph\nfirst\nsecond\n");
}

/// Native storage is freed even when a PHP subclass provides its own destructor.
#[test]
fn test_fileinfo_subclass_native_ownership() {
    if fileinfo_native::skip("test_fileinfo_subclass_native_ownership") { return; }
    let out = compile_and_run(r#"<?php
extern "elephc_fileinfo" { function elephc_fileinfo_live_handles(): int; }
class DocumentInfo extends finfo {
    public function __destruct() { echo "destroyed\n"; }
}
$before = elephc_fileinfo_live_handles();
$f = new DocumentInfo(FILEINFO_MIME_TYPE);
echo $f->buffer("hello"), "\n";
unset($f);
echo elephc_fileinfo_live_handles() === $before ? "clean\n" : "leak\n";
"#);
    assert_eq!(out, "text/plain\ndestroyed\nclean\n");
}

/// Internal restrictions survive subclass hooks and agree with reflection.
#[test]
fn test_fileinfo_subclass_clone_and_wire_restrictions() {
    if fileinfo_native::skip("test_fileinfo_subclass_clone_and_wire_restrictions") { return; }
    let out = compile_and_run(r#"<?php
class DescribedFinfo extends finfo {
    public function __clone(): void { echo "bad clone hook\n"; }
    public function __serialize(): array { echo "bad serialize hook\n"; return []; }
    public function __unserialize(array $data): void { echo "bad unserialize hook\n"; }
}
$f = new DescribedFinfo(FILEINFO_MIME_TYPE);
$r = new ReflectionClass($f);
echo $r->isCloneable() ? "bad\n" : "uncloneable\n";
try { $copy = clone $f; } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { serialize($f); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
try { unserialize('O:14:"DescribedFinfo":0:{}'); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
echo $f->buffer("hello"), "\n";
"#);
    assert_eq!(out, "uncloneable\nTrying to clone an uncloneable object of class DescribedFinfo\nSerialization of 'DescribedFinfo' is not allowed\nUnserialization of 'DescribedFinfo' is not allowed\ntext/plain\n");
}

/// Fileinfo failures use E_WARNING and allow PHP handlers to suppress or throw.
#[test]
fn test_fileinfo_warning_handlers() {
    if fileinfo_native::skip("test_fileinfo_warning_handlers") { return; }
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $errno, string $_message, string $_file, int $_line): bool {
    echo $errno, "\n";
    return true;
});
echo $f->file("/elephc-fileinfo-missing/file") === false ? "file\n" : "bad\n";
echo finfo_open(0, "/elephc-fileinfo-missing/database") === false ? "database\n" : "bad\n";
restore_error_handler();
set_error_handler(function(int $_errno, string $_message, string $_file, int $_line): bool {
    throw new RuntimeException("warning");
});
try { $f->file("/elephc-fileinfo-missing/file"); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
restore_error_handler();
echo $f->buffer("hello"), "\n";
"#);
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2\nfile\n2\n2\n2\ndatabase\nwarning\ntext/plain\n", "{}", out.stderr);
    let mut failures = Vec::new();
    if !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
        eprintln!("Fileinfo warning assembly:\n{asm}");
        failures.push(format!("fileinfo warnings: {}", out.stderr));
    }
    for (label, source) in [
        ("engine warning", "<?php __elephc_builtin_warning('control');"),
        ("user warning handler", "<?php set_error_handler(function(int $n, string $m, string $f, int $l): bool { return true; }); trigger_error('control', E_USER_WARNING); restore_error_handler();"),
        ("throwing user warning handler", "<?php set_error_handler(function(int $n, string $m, string $f, int $l): bool { throw new RuntimeException('control'); }); try { trigger_error('control', E_USER_WARNING); } catch (RuntimeException $e) {} restore_error_handler();"),
    ] {
        let control = compile_and_run_with_heap_debug(source);
        if !control.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("{label}: {}", control.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Procedural functions invoke the native implementation even if a subclass overrides methods.
#[test]
fn test_fileinfo_procedural_calls_bypass_overrides() {
    if fileinfo_native::skip("test_fileinfo_procedural_calls_bypass_overrides") { return; }
    let out = compile_and_run(r#"<?php
class OverriddenFinfo extends finfo {
    public function buffer(string $string, int $flags = 0, mixed $context = null): string|false { return "override"; }
    public function file(string $filename, int $flags = 0, mixed $context = null): string|false { return "override"; }
    public function set_flags(int $flags): bool { echo "bad flag override\n"; return true; }
}
$f = new OverriddenFinfo(FILEINFO_MIME_TYPE);
echo $f->buffer("hello"), "\n";
echo finfo_buffer($f, "hello"), "\n";
finfo_set_flags($f, FILEINFO_MIME_ENCODING);
echo finfo_buffer($f, "hello"), "\n";
echo finfo_file($f, "data://text/plain,hello"), "\n";
"#);
    assert_eq!(out, "override\ntext/plain\nus-ascii\nus-ascii\n");
}

/// PHP 8.5 deprecates finfo_close and explicitly supplied buffer contexts only.
#[test]
fn test_fileinfo_php_profile_deprecations() {
    if fileinfo_native::skip("test_fileinfo_php_profile_deprecations") { return; }
    let source = r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $errno, string $_message, string $_file, int $_line): bool {
    echo $errno, "|";
    return true;
});
echo $f->buffer("hello"), "|";
echo finfo_buffer($f, "hello"), "|";
echo $f->buffer("hello", 0, null), "|";
echo finfo_buffer($f, "hello", 0, null), "|";
echo finfo_close($f) ? "closed" : "bad";
restore_error_handler();
echo "|", (new ReflectionFunction("finfo_close"))->isDeprecated() ? "deprecated" : "current";
"#;
    assert_eq!(compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php85),
        "text/plain|text/plain|8192|text/plain|8192|text/plain|8192|closed|deprecated");
    assert_eq!(compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php84),
        "text/plain|text/plain|text/plain|text/plain|closed|current");
}

/// PCRE2 receives the original subject length, so a trailing byte cannot be dropped and turn
/// a non-matching regex rule into a match.
#[test]
fn test_fileinfo_regex_subject_length_matches_php() {
    if fileinfo_native::skip("test_fileinfo_regex_subject_length_matches_php") { return; }
    let out = compile_and_run(
        r#"<?php
file_put_contents(__DIR__ . "/slow.magic", "0 regex (a+)+$ slow\n");
$f = finfo_open(0, __DIR__ . "/slow.magic");
echo finfo_buffer($f, str_repeat("a", 30) . "b"), "\n";
echo finfo_buffer($f, "aaa"), "\n";
"#,
    );
    assert_eq!(
        out,
        "ASCII text, with no line terminators\nslow, ASCII text, with no line terminators\n"
    );
}
