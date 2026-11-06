//! Purpose:
//! Checks fileinfo argument diagnostics and malformed runtime stream URL handling.
//!
//! Called from:
//! - The codegen integration harness on every executable supported target.
//!
//! Key details:
//! - PHP 8.4 and 8.5 differ for finfo filenames, but share database and MIME errors.
//! - Runtime-selected eval exercises the same prelude through Magician forwarding.

use crate::support::*;

/// Warning locations survive wrapper callbacks and handlers that publish another call site.
#[test]
fn test_fileinfo_diagnostic_call_site_lines() {
    if fileinfo_native::skip("test_fileinfo_diagnostic_call_site_lines") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class FileinfoExpectedLine {
    public static int $line = 0;
    public static int $wrapperLine = 0;
    public static bool $defaultOutput = false;
}
class FileinfoLineOpenFailure {
    public $context;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool {
        FileinfoExpectedLine::$wrapperLine = __LINE__ + 1;
        trigger_error("wrapper", E_USER_WARNING);
        return false;
    }
    public function stream_stat(): array { return []; }
}
function fileinfoLineCaller(finfo $f): bool {
    FileinfoExpectedLine::$line = __LINE__ + 1;
    return finfo_close($f);
}
function fileinfoLineHandlerCallback(): void {
    trigger_error("nested", E_USER_NOTICE);
}
$f = new finfo(FILEINFO_MIME_TYPE);
$context = fopen("php://memory", "w+b");
error_reporting(E_ALL & ~E_USER_NOTICE);
set_error_handler(function(int $level, string $message, string $file, int $line): bool {
    $expected = $message === "wrapper" ? FileinfoExpectedLine::$wrapperLine : FileinfoExpectedLine::$line;
    echo $file === __FILE__ && $line === $expected ? "ok\n" : "bad:" . $line . ":" . $expected . "\n";
    fileinfoLineHandlerCallback();
    return !FileinfoExpectedLine::$defaultOutput;
});
FileinfoExpectedLine::$line = __LINE__ + 1;
finfo_close($f);
fileinfoLineCaller($f);
$close = finfo_close(...);
FileinfoExpectedLine::$line = __LINE__ + 1;
$close($f);
FileinfoExpectedLine::$line = __LINE__ + 1;
$f->buffer("HELLO", 0, $context);
FileinfoExpectedLine::$line = __LINE__ + 1;
finfo_file($f, "data:text");
stream_wrapper_register("fileinfoline", "FileinfoLineOpenFailure");
FileinfoExpectedLine::$line = __LINE__ + 1;
$f->file("php://filter/resource=fileinfoline://warning");
FileinfoExpectedLine::$defaultOutput = true;
FileinfoExpectedLine::$line = __LINE__ + 1;
$f->file("fileinfolinemissing://warning");
echo "handler-default:", FileinfoExpectedLine::$line, "\n";
restore_error_handler();
error_reporting(E_ALL);
echo "callback-default:", __LINE__ + 1, "\n";
$f->file("php://filter/resource=fileinfoline://warning");
stream_wrapper_unregister("fileinfoline");
echo "default:", __LINE__ + 1, "\n";
finfo_close($f);
fclose($context); unset($f, $context, $close);
"#);
    assert!(out.success, "{}", out.stderr);
    assert!(out.stdout.starts_with(&"ok\n".repeat(10)), "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout.lines().count(), 13, "{}\n{}", out.stdout, out.stderr);
    let handler_line = out.stdout.lines().find_map(|line| line.strip_prefix("handler-default:")).unwrap();
    assert_eq!(out.stderr.matches(&format!("on line {handler_line}\n")).count(), 3, "{}", out.stderr);
    let callback_line = out.stdout.lines().find_map(|line| line.strip_prefix("callback-default:")).unwrap();
    assert!(out.stderr.contains(&format!("on line {callback_line}\n")), "{}", out.stderr);
    let line = out.stdout.lines().last().unwrap().strip_prefix("default:").unwrap();
    assert!(out.stderr.contains(&format!("on line {line}\n")), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Argument errors retain their public function names and the selected PHP exception class.
#[test]
fn test_fileinfo_php_profile_argument_errors() {
    if fileinfo_native::skip("test_fileinfo_php_profile_argument_errors") { return; }
    let source = r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
try { mime_content_type("a\0b"); } catch (TypeError $e) { echo "mime-nul:", $e->getMessage(), "\n"; }
try { mime_content_type(""); } catch (ValueError $e) { echo "mime-empty:", $e->getMessage(), "\n"; }
try { $f->file("a\0b"); } catch (TypeError $e) { echo "type\n"; } catch (ValueError $e) { echo "value\n"; }
try { finfo_file($f, "a\0b"); } catch (TypeError $e) { echo "type:", $e->getMessage(), "\n"; } catch (ValueError $e) { echo "value:", $e->getMessage(), "\n"; }
try { finfo_file($f, ""); } catch (ValueError $e) { echo "empty:", $e->getMessage(), "\n"; }
try { new finfo(0, "a\0b"); } catch (ValueError $e) { echo "database-value:", $e->getMessage(), "\n"; }
try { finfo_open(0, "a\0b"); } catch (ValueError $e) { echo "open-value:", $e->getMessage(), "\n"; }
echo $f->buffer("hello"), "\n";
"#;
    for (profile, class) in [(elephc::php_version::PhpVersion::Php84, "type"), (elephc::php_version::PhpVersion::Php85, "value")] {
        assert_eq!(compile_and_run_with_php_version(source, profile), format!(
            "mime-nul:mime_content_type(): Argument #1 ($filename) must not contain any null bytes\nmime-empty:mime_content_type(): Argument #1 ($filename) must not be empty\n{class}\n{class}:finfo_file(): Argument #2 ($filename) must not contain any null bytes\nempty:finfo_file(): Argument #2 ($filename) must not be empty\ndatabase-value:finfo::__construct(): Argument #2 ($magic_database) must not contain any null bytes\nopen-value:finfo_open(): Argument #2 ($magic_database) must not contain any null bytes\ntext/plain\n"
        ));
    }
}

/// Invalid data metadata warns and returns false without changing accepted payload decoding.
#[test]
fn test_fileinfo_data_url_metadata_validation() {
    if fileinfo_native::skip("test_fileinfo_data_url_metadata_validation") { return; }
    let out = compile_and_run(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
$warnings = 0;
set_error_handler(function(int $level, string $message) use (&$warnings): bool { if ($level === E_WARNING) { $warnings++; } return true; });
foreach (["data://foo,HELLO", "data://text/plain;BASE64,SGVsbG8=", "data://;charset=UTF-8,HELLO", "data://text/plain;broken,HELLO", "data://text/plain;base64;x=y,SGVsbG8=", "data://text/plain;,HELLO"] as $url) {
    echo finfo_file($f, $url) === false ? "rejected\n" : "bad\n";
}
echo $warnings === 6 ? "warned\n" : "bad\n";
foreach (["data://,HELLO", "data://;base64,SEVMTE8=", "data://text/plain;charset=UTF-8;base64,SEVMTE8=", "data://text/plain;token=a=b,HELLO", "data://text/plain;base64=ordinary,HELLO", "data:text/plain,HELLO", "data:,HELLO", "data:;base64,SEVMTE8="] as $url) {
    echo $f->file($url), "\n";
}
$code = $argc > 0 ? 'echo finfo_file($f, "data://foo,HELLO") === false ? "eval-rejected\n" : "bad\n"; echo $f->file("data://text/plain;BASE64,SGVsbG8=") === false ? "eval-rejected\n" : "bad\n"; echo $f->file("data://;base64,SEVMTE8="), "\n"; echo mime_content_type("data:,HELLO"), "\n";' : '';
eval($code);
restore_error_handler();
"#);
    assert_eq!(out, "rejected\n".repeat(6) + "warned\n" + &"text/plain\n".repeat(8) + "eval-rejected\neval-rejected\ntext/plain\ntext/plain\n");
}

/// Filter resource errors throw, while read and write selectors ignore ASCII case.
#[test]
fn test_fileinfo_filter_url_validation() {
    if fileinfo_native::skip("test_fileinfo_filter_url_validation") { return; }
    let out = compile_and_run(r#"<?php
$db = tempnam(sys_get_temp_dir(), "magic");
file_put_contents($db, "0 string HELLO filtered-document\n");
$f = new finfo(FILEINFO_NONE, $db);
try { finfo_file($f, "php://filter/read=string.rot13"); echo "bad\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo $f->file("php://filter/READ=string.rot13/resource=data://text/plain,URYYB"), "\n";
echo $f->file("php://filter/WrItE=string.rot13/resource=data://text/plain,HELLO"), "\n";
echo $f->file("php://filter/read=string%2Erot13/resource=data:,URYYB"), "\n";
echo $f->file("php://filter/read=string%2Erot13%00ignored/resource=data:,URYYB"), "\n";
$code = $argc > 0 ? 'try { $f->file("php://filter/read=string.rot13"); echo "bad\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; } echo finfo_file($f, "php://filter/ReAd=string.rot13/resource=data://text/plain,URYYB"), "\n";' : '';
eval($code);
unlink($db);
"#);
    assert_eq!(out, "No URL resource specified\n".to_owned() + &"filtered-document\n".repeat(4) + "No URL resource specified\nfiltered-document\n");
}

/// Context diagnostics identify the public call and reject types before deprecations.
#[test]
fn test_fileinfo_context_argument_diagnostics() {
    if fileinfo_native::skip("test_fileinfo_context_argument_diagnostics") { return; }
    let source = r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $level, string $message): bool { echo "unexpected warning\n"; return true; });
try { finfo_file($f, "x", 0, 1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, 1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { finfo_buffer($f, "x", 0, 1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->buffer("x", 0, 1); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, true); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, 1.5); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, []); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, new stdClass()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$stream = fopen("php://temp", "w+b");
try { finfo_file($f, "x", 0, $stream); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $f->file("x", 0, $stream); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
restore_error_handler();
echo @$f->buffer("hello", 0, $stream), "\n";
fclose($stream);
"#;
    let expected = "finfo_file(): Argument #4 ($context) must be of type resource or null, int given\nfinfo::file(): Argument #3 ($context) must be of type resource or null, int given\nfinfo_buffer(): Argument #4 ($context) must be of type resource or null, int given\nfinfo::buffer(): Argument #3 ($context) must be of type resource or null, int given\nfinfo::file(): Argument #3 ($context) must be of type resource or null, bool given\nfinfo::file(): Argument #3 ($context) must be of type resource or null, float given\nfinfo::file(): Argument #3 ($context) must be of type resource or null, array given\nfinfo::file(): Argument #3 ($context) must be of type resource or null, stdClass given\nfinfo_file(): supplied resource is not a valid Stream-Context resource\nfinfo::file(): supplied resource is not a valid Stream-Context resource\ntext/plain\n";
    for profile in [elephc::php_version::PhpVersion::Php84, elephc::php_version::PhpVersion::Php85] {
        assert_eq!(compile_and_run_with_php_version(source, profile), expected);
    }
}

/// Constructors throw plain exceptions while procedural opens emit their own warning names.
#[test]
fn test_fileinfo_open_error_diagnostics() {
    if fileinfo_native::skip("test_fileinfo_open_error_diagnostics") { return; }
    let out = compile_and_run(r#"<?php
try { new finfo(0, "/elephc-fileinfo-missing/magic"); } catch (Exception $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
set_error_handler(function(int $level, string $message): bool { echo $level, ":", $message, "\n"; return true; });
echo finfo_open(0, "/elephc-fileinfo-missing/magic") === false ? "database\n" : "bad\n";
$f = new finfo(FILEINFO_MIME_TYPE);
echo $f->file("/elephc-fileinfo-missing/file") === false ? "method\n" : "bad\n";
echo finfo_file($f, "/elephc-fileinfo-missing/file") === false ? "function\n" : "bad\n";
echo mime_content_type("file:///elephc-fileinfo-missing/file") === false ? "mime\n" : "bad\n";
restore_error_handler();
echo $f->buffer("hello"), "\n";
"#);
    assert_eq!(out, "Exception:finfo::__construct(/elephc-fileinfo-missing/magic): Failed to open stream: No such file or directory\n2:finfo_open(/elephc-fileinfo-missing/magic): Failed to open stream: No such file or directory\n2:finfo_open(/elephc-fileinfo-missing/magic): Failed to open stream: No such file or directory\n2:finfo_open(): Failed to load magic database at \"/elephc-fileinfo-missing/magic\"\ndatabase\n2:finfo::file(/elephc-fileinfo-missing/file): Failed to open stream: No such file or directory\nmethod\n2:finfo_file(/elephc-fileinfo-missing/file): Failed to open stream: No such file or directory\nfunction\n2:mime_content_type(file:///elephc-fileinfo-missing/file): Failed to open stream: No such file or directory\nmime\ntext/plain\n");
}

/// Directory identification requires an openable descriptor even when stat succeeds.
#[test]
fn test_fileinfo_directory_open_permissions() {
    if fileinfo_native::skip("test_fileinfo_directory_open_permissions") { return; }
    let out = compile_and_run(r#"<?php
$dir = tempnam(sys_get_temp_dir(), "finfo-directory");
unlink($dir);
mkdir($dir, 0000);
$probe = @fopen($dir, "rb");
$can_open = $probe !== false;
if ($can_open) { fclose($probe); }
$f = new finfo(FILEINFO_MIME_TYPE);
$result = @$f->file($dir);
echo $can_open ? ($result === "directory" ? "ok\n" : "bad\n") : ($result === false ? "ok\n" : "bad\n");
chmod($dir, 0700);
echo $f->file($dir), "\n";
rmdir($dir);
"#);
    assert_eq!(out, "ok\ndirectory\n");
}

/// Wrong handle types and surplus constructor arguments are runtime PHP errors.
#[test]
fn test_fileinfo_runtime_argument_errors() {
    if fileinfo_native::skip("test_fileinfo_runtime_argument_errors") { return; }
    let out = compile_and_run_with_php_version(r#"<?php
try { finfo_buffer(1, "hello"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { new finfo(0, null, 1); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { finfo_close(fopen("php://memory", "r")); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#, elephc::php_version::PhpVersion::Php84);
    assert_eq!(out, "finfo_buffer(): Argument #1 ($finfo) must be of type finfo, int given\nfinfo::__construct() expects at most 2 arguments, 3 given\nfinfo_close(): Argument #1 ($finfo) must be of type finfo, resource given\n");
}
