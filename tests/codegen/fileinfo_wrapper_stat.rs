//! Purpose:
//! Checks PHP fileinfo's stream-stat gate and user-wrapper callback ordering.
//!
//! Called from:
//! - The executable codegen integration suite on supported host targets.
//!
//! Key details:
//! - URL queries stat wrappers after filter creation and before reading bytes.
//! - Already-open mime_content_type resources retain PHP's direct magic-stream behavior.

use crate::support::*;

/// Distinguishes absent, failed, directory and regular stream stat across public URL entry points.
#[test]
fn test_fileinfo_user_wrapper_stat_results() {
    if fileinfo_native::skip("test_fileinfo_user_wrapper_stat_results") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class FileinfoNoStat {
    public $context;
    protected int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { return true; }
    public function stream_read(int $count): string { $v = substr("HELLO", $this->position, $count); $this->position += strlen($v); return $v; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_cast(int $cast_as): mixed { return false; }
}
class FileinfoFalseStat extends FileinfoNoStat {
    public function stream_stat(): array|false { return false; }
}
class FileinfoDirectoryStat extends FileinfoNoStat {
    public function stream_stat(): array { return ["mode" => 040755, "size" => 5]; }
}
class FileinfoRegularStat extends FileinfoNoStat {
    public function stream_stat(): array { return ["mode" => 0100644, "size" => 5]; }
}
class FileinfoNoModeStat extends FileinfoNoStat {
    public function stream_stat(): array { return ["size" => 5]; }
}
class FileinfoStringModeStat extends FileinfoNoStat {
    public function stream_stat(): array { return ["mode" => "16877", "size" => 5]; }
}
class FileinfoBoolStat extends FileinfoNoStat {
    public function stream_stat(): bool { return true; }
}
class FileinfoIntStat extends FileinfoNoStat {
    public function stream_stat(): int { return 42; }
}
stream_wrapper_register("nostat", "FileinfoNoStat");
stream_wrapper_register("falsestat", "FileinfoFalseStat");
stream_wrapper_register("dirstat", "FileinfoDirectoryStat");
stream_wrapper_register("regstat", "FileinfoRegularStat");
stream_wrapper_register("nomodestat", "FileinfoNoModeStat");
stream_wrapper_register("stringmodestat", "FileinfoStringModeStat");
stream_wrapper_register("boolstat", "FileinfoBoolStat");
stream_wrapper_register("intstat", "FileinfoIntStat");
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool { echo $message, "\n"; return true; });
foreach (["nostat", "falsestat", "dirstat", "regstat", "nomodestat", "stringmodestat", "boolstat", "intstat"] as $scheme) {
    $url = $scheme . "://x";
    echo var_export(finfo_file($f, $url), true), "\n";
    echo var_export($f->file($url), true), "\n";
    echo var_export(mime_content_type($url), true), "\n";
}
echo var_export(finfo_file($f, "php://filter/resource=nostat://x"), true), "\n";
echo $f->file("php://filter/read=string.rot13/resource=dirstat://x"), "\n";
$stream = fopen("dirstat://x", "rb");
echo mime_content_type($stream), "\n";
fclose($stream);
$code = $argc > 0 ? 'echo var_export($f->file("falsestat://x"), true), "\n"; echo mime_content_type("dirstat://x"), "\n";' : '';
eval($code);
restore_error_handler();
stream_wrapper_unregister("nostat"); stream_wrapper_unregister("falsestat");
stream_wrapper_unregister("dirstat"); stream_wrapper_unregister("regstat");
stream_wrapper_unregister("nomodestat"); stream_wrapper_unregister("stringmodestat");
stream_wrapper_unregister("boolstat"); stream_wrapper_unregister("intstat");
unset($f, $stream, $scheme, $url, $code);
"#);
    let mut expected = String::new();
    for name in ["finfo_file", "finfo::file", "mime_content_type"] {
        expected.push_str(&format!("{name}(): FileinfoNoStat::stream_stat is not implemented!\nfalse\n"));
    }
    expected.push_str("false\nfalse\nfalse\n'directory'\n'directory'\n'directory'\n'text/plain'\n'text/plain'\n'text/plain'\n");
    expected.push_str("'text/plain'\n'text/plain'\n'text/plain'\n'directory'\n'directory'\n'directory'\n");
    expected.push_str("false\nfalse\nfalse\nfalse\nfalse\nfalse\n");
    for name in ["resource=nostat:", "x"] {
        expected.push_str(&format!("finfo_file(): Unable to locate filter \"{name}\"\nfinfo_file(): Unable to create filter ({name})\n"));
    }
    expected.push_str("finfo_file(): FileinfoNoStat::stream_stat is not implemented!\nfalse\ndirectory\ntext/plain\nfalse\ndirectory\n");
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Filter creation precedes stat, and both ordinary and throwing stat failures close every owner.
#[test]
fn test_fileinfo_user_wrapper_stat_callback_order() {
    if fileinfo_native::skip("test_fileinfo_user_wrapper_stat_callback_order") { return; }
    let source = r#"<?php
class FileinfoStatOrder {
    public $context;
    private bool $fail = false;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { $this->fail = str_contains($path, "throw"); echo "open\n"; return true; }
    public function stream_stat(): array|false { echo "stat\n"; if ($this->fail) { throw new Error("stat failed"); } return false; }
    public function stream_read(int $count): string { echo "unexpected read\n"; return "HELLO"; }
    public function stream_eof(): bool { return false; }
    public function stream_close(): void { echo "close\n"; }
}
class FileinfoStatFilter extends php_user_filter {
    public function onCreate(): bool { echo "create\n"; return true; }
    public function onClose(): void { echo "filter-close\n"; }
    public function filter($in, $out, &$consumed, $closing): int { echo "unexpected filter\n"; return PSFS_PASS_ON; }
}
class FileinfoStatCreateFilter extends FileinfoStatFilter {
    public function onCreate(): bool { echo "create\n"; throw new Error("create failed"); }
}
class FileinfoStatCloseFilter extends FileinfoStatFilter {
    public function onClose(): void { echo "filter-close\n"; throw new Error("close failed"); }
}
stream_wrapper_register("statorder", "FileinfoStatOrder");
stream_filter_register("fileinfo.stat", "FileinfoStatFilter");
stream_filter_register("fileinfo.stat.create", "FileinfoStatCreateFilter");
stream_filter_register("fileinfo.stat.close", "FileinfoStatCloseFilter");
$f = new finfo(FILEINFO_MIME_TYPE);
echo var_export($f->file("php://filter/read=fileinfo.stat/resource=statorder://false"), true), "\n";
try { $f->file("php://filter/read=fileinfo.stat/resource=statorder://throw"); } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { $f->file("php://filter/read=fileinfo.stat.create/resource=statorder://false"); } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { $f->file("php://filter/read=fileinfo.stat.close|fileinfo.stat/resource=statorder://false"); } catch (Error $error) { echo $error->getMessage(), "\n"; }
echo var_export($f->file("php://filter/read=fileinfo.stat/resource=statorder://false"), true), "\n";
stream_wrapper_unregister("statorder"); unset($f, $error);
"#;
    let (out, _) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, "open\ncreate\nstat\nclose\nfilter-close\nfalse\nopen\ncreate\nstat\nstat failed\nopen\ncreate\ncreate failed\nopen\ncreate\ncreate\nstat\nclose\nfilter-close\nclose failed\nopen\ncreate\nstat\nclose\nfilter-close\nfalse\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct HTTP wrapper stat fails; filtering exposes socket stat and PHP's directory-bit result.
#[test]
fn test_fileinfo_http_wrapper_stat_entry_points() {
    use std::io::{Read, Write};
    if fileinfo_native::skip("test_fileinfo_http_wrapper_stat_entry_points") { return; }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while stream.read(&mut byte).unwrap_or(0) == 1 {
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") { break; }
            }
            assert!(request.starts_with(b"GET /payload HTTP/"));
            stream.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nHELLO").unwrap();
        }
    });
    let out = compile_and_run(&format!(r#"<?php
$url = "http://127.0.0.1:{port}/payload";
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $n, string $message): bool {{ echo "unexpected:", $message, "\n"; return true; }});
echo var_export($f->file($url), true), "\n";
echo var_export(finfo_file($f, $url), true), "\n";
echo var_export(mime_content_type($url), true), "\n";
echo var_export($f->file("php://filter/read=string.rot13/resource=" . $url), true), "\n";
restore_error_handler();
"#));
    server.join().unwrap();
    assert_eq!(out, "false\nfalse\nfalse\n'directory'\n");
}
