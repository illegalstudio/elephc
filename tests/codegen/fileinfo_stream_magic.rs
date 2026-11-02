//! Purpose:
//! Checks fileinfo's magic_stream callbacks, cast results and independent stat phase.
//!
//! Called from:
//! - The executable codegen integration suite on every supported host target.
//!
//! Key details:
//! - Fixtures omit stream_cast as well as declaring scalar and resource return types.
//! - Successful read and write filters suppress casts, while failed filters do not.
//! - Throwing callbacks retain the original exception and skip later close callbacks.

use crate::support::*;

/// Validates cast warnings, native return representations, resource bytes and throwing handlers.
#[test]
fn test_fileinfo_stream_cast_callbacks() {
    if fileinfo_native::skip("test_fileinfo_stream_cast_callbacks") { return; }
    let source = r#"<?php
class FileinfoCastMissing {
    public $context;
    protected int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { return true; }
    public function stream_stat(): array { return ["mode" => 0100644, "size" => 5]; }
    public function stream_read(int $count): string { $s = substr("HELLO", $this->position, $count); $this->position += strlen($s); return $s; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_close(): void { echo "close\n"; }
}
class FileinfoCastFalse extends FileinfoCastMissing {
    public function stream_cast(int $kind): mixed { echo "cast ", $kind, "\n"; return false; }
}
class FileinfoCastInt extends FileinfoCastMissing {
    public function stream_cast(int $kind): int { echo "cast ", $kind, "\n"; return 1; }
}
class FileinfoCastResource extends FileinfoCastMissing {
    public function stream_cast(int $kind): mixed {
        echo "cast ", $kind, "\n";
        $stream = fopen("php://temp", "w+b"); fwrite($stream, "%PDF-1.7\n"); rewind($stream); return $stream;
    }
}
class FileinfoCastEmptyArray extends FileinfoCastMissing {
    public function stream_cast(int $kind): array { echo "cast ", $kind, "\n"; return []; }
}
class FileinfoCastZeroString extends FileinfoCastMissing {
    public function stream_cast(int $kind): string { echo "cast ", $kind, "\n"; $s = str_repeat("0", $this->position + 1); return substr($s, $this->position); }
}
class FileinfoCastZeroFloat extends FileinfoCastMissing {
    public function stream_cast(int $kind): float { echo "cast ", $kind, "\n"; return 0.0; }
}
class FileinfoCastTrueString extends FileinfoCastMissing {
    public function stream_cast(int $kind): string { echo "cast ", $kind, "\n"; return str_repeat("b", 3); }
}
class FileinfoCastTrueFloat extends FileinfoCastMissing {
    public function stream_cast(int $kind): float { echo "cast ", $kind, "\n"; return 1.5; }
}
class FileinfoCastTrueArray extends FileinfoCastMissing {
    public function stream_cast(int $kind): array { echo "cast ", $kind, "\n"; return ["bad"]; }
}
class FileinfoCastSelf extends FileinfoCastMissing {
    public static $stream;
    public function stream_cast(int $kind): mixed { echo "cast ", $kind, "\n"; return FileinfoCastSelf::$stream; }
}
stream_wrapper_register("castmissing", "FileinfoCastMissing");
stream_wrapper_register("castfalse", "FileinfoCastFalse");
stream_wrapper_register("castint", "FileinfoCastInt");
stream_wrapper_register("castresource", "FileinfoCastResource");
stream_wrapper_register("castemptyarray", "FileinfoCastEmptyArray");
stream_wrapper_register("castzerostring", "FileinfoCastZeroString");
stream_wrapper_register("castzerofloat", "FileinfoCastZeroFloat");
stream_wrapper_register("casttruestring", "FileinfoCastTrueString");
stream_wrapper_register("casttruefloat", "FileinfoCastTrueFloat");
stream_wrapper_register("casttruearray", "FileinfoCastTrueArray");
stream_wrapper_register("castself", "FileinfoCastSelf");
$f = new finfo(FILEINFO_MIME_TYPE);
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
echo $f->file("castmissing://x"), "\n";
echo finfo_file($f, "castmissing://x"), "\n";
echo mime_content_type("castmissing://x"), "\n";
$stream = fopen("castmissing://x", "rb"); echo mime_content_type($stream), "\n"; fclose($stream);
foreach (["castfalse", "castint", "castresource", "castemptyarray", "castzerostring", "castzerofloat", "casttruestring", "casttruefloat", "casttruearray"] as $scheme) {
    try { echo $f->file($scheme . "://x"), "\n"; } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
$code = $argc > 0 ? 'echo $f->file("castfalse://x"), "\n";' : '';
eval($code);
$self = fopen("castself://x", "rb"); FileinfoCastSelf::$stream = $self;
echo mime_content_type($self), "\n"; FileinfoCastSelf::$stream = null; fclose($self);
restore_error_handler();
try { $f->file("castint://x"); } catch (Throwable $e) { echo $e->getMessage(), "\n"; }
set_error_handler(function(int $level, string $message): bool { echo "handler\n"; throw new Error("cast warning"); });
try { $f->file("castmissing://x"); } catch (Throwable $e) { echo $e->getMessage(), "\n"; }
restore_error_handler();
foreach (["castmissing", "castfalse", "castint", "castresource", "castemptyarray", "castzerostring", "castzerofloat", "casttruestring", "casttruefloat", "casttruearray", "castself"] as $scheme) { stream_wrapper_unregister($scheme); }
unset($f, $stream, $self, $scheme, $code, $e);
"#;
    let (out, _) = compile_and_run_with_heap_debug_and_asm(source);
    let mut expected = String::new();
    for function in ["finfo::file", "finfo_file", "mime_content_type"] {
        expected.push_str(&format!("{function}(): FileinfoCastMissing::stream_cast is not implemented!\nclose\ntext/plain\n"));
    }
    expected.push_str("mime_content_type(): FileinfoCastMissing::stream_cast is not implemented!\ntext/plain\nclose\n");
    for (_class, invalid) in [("FileinfoCastFalse", false), ("FileinfoCastInt", true), ("FileinfoCastResource", false),
        ("FileinfoCastEmptyArray", false), ("FileinfoCastZeroString", false), ("FileinfoCastZeroFloat", false),
        ("FileinfoCastTrueString", true), ("FileinfoCastTrueFloat", true), ("FileinfoCastTrueArray", true)] {
        expected.push_str("cast 0\n");
        if invalid {
            expected.push_str("TypeError: finfo::file(): supplied argument is not a valid stream resource\n");
        } else { expected.push_str("close\ntext/plain\n"); }
    }
    expected.push_str("cast 0\nclose\ntext/plain\ncast 0\nmime_content_type(): FileinfoCastSelf::stream_cast must not return itself\ntext/plain\nclose\ncast 0\nfinfo::file(): supplied argument is not a valid stream resource\nhandler\ncast warning\n");
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("finfo::file(): FileinfoCastInt::stream_cast must return a stream resource"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both filter directions attach before stat and suppress casts only after successful creation.
#[test]
fn test_fileinfo_stream_cast_filter_directions() {
    if fileinfo_native::skip("test_fileinfo_stream_cast_filter_directions") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class FileinfoFilterCast {
    public $context;
    private int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { return true; }
    public function stream_stat(): array { echo "stat\n"; return ["mode" => 0100644, "size" => 5]; }
    public function stream_read(int $count): string { echo "read\n"; $s = substr("HELLO", $this->position, $count); $this->position += strlen($s); return $s; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_close(): void { echo "wrapper-close\n"; }
}
class FileinfoWriteCastFilter extends php_user_filter {
    public function onCreate(): bool { echo "create\n"; return true; }
    public function onClose(): void { echo "filter-close\n"; }
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) { $consumed += $bucket->datalen; stream_bucket_append($out, $bucket); }
        return PSFS_PASS_ON;
    }
}
stream_wrapper_register("filtercast", "FileinfoFilterCast");
stream_filter_register("fileinfo.writecast", "FileinfoWriteCastFilter");
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
$f = new finfo(FILEINFO_MIME_TYPE);
foreach (["read=string.toupper", "write=string.toupper", "write=fileinfo.writecast", "read=missing.cast", "write=missing.cast"] as $spec) {
    echo $f->file("php://filter/" . $spec . "/resource=filtercast://x"), "\n";
}
restore_error_handler(); stream_wrapper_unregister("filtercast"); unset($f, $spec);
"#);
    let normal = "stat\nstat\nread\nwrapper-close\ntext/plain\n";
    let unknown = "finfo::file(): Unable to locate filter \"missing.cast\"\nfinfo::file(): Unable to create filter (missing.cast)\nstat\nstat\nread\nfinfo::file(): FileinfoFilterCast::stream_cast is not implemented!\nwrapper-close\ntext/plain\n";
    let expected = format!("{normal}{normal}create\nstat\nstat\nread\nwrapper-close\nfilter-close\ntext/plain\n{unknown}{unknown}");
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The magic stat runs independently of the URL gate and honors active temporary or stored flags.
#[test]
fn test_fileinfo_magic_stream_stat_phase() {
    if fileinfo_native::skip("test_fileinfo_magic_stream_stat_phase") { return; }
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class FileinfoMagicStat {
    public $context;
    private int $position = 0;
    private int $stats = 0;
    private string $kind = "";
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { $this->kind = substr($path, strrpos($path, "/") + 1); return true; }
    public function stream_stat(): array|false {
        echo "stat ", ++$this->stats, "\n";
        if ($this->stats == 2) {
            if ($this->kind == "throw") { throw new Error("second stat"); }
            if ($this->kind == "directory") { return ["mode" => 040755, "size" => 5]; }
            return false;
        }
        return ["mode" => 0100644, "size" => 5];
    }
    public function stream_read(int $count): string { echo "read\n"; $s = substr("HELLO", $this->position, $count); $this->position += strlen($s); return $s; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_cast(int $kind): mixed { echo "cast ", $kind, "\n"; return false; }
    public function stream_close(): void { echo "close\n"; }
}
class FileinfoMagicMissing {
    public $context;
    private int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { return true; }
    public function stream_read(int $count): string { $s = substr("HELLO", $this->position, $count); $this->position += strlen($s); return $s; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_close(): void { echo "close\n"; }
}
stream_wrapper_register("magicstat", "FileinfoMagicStat"); stream_wrapper_register("magicmissing", "FileinfoMagicMissing");
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
$f = new finfo(FILEINFO_MIME_TYPE);
echo $f->file("magicstat://false"), "\n";
echo $f->file("magicstat://directory"), "\n";
echo var_export($f->file("magicstat://false", FILEINFO_MIME_TYPE | 512), true), "\n";
echo $f->file("magicstat://false"), "\n";
finfo_set_flags($f, FILEINFO_MIME_TYPE | 512);
echo var_export(finfo_file($f, "magicstat://false"), true), "\n";
$other = finfo_open(FILEINFO_MIME_TYPE | 512);
echo var_export($other->file("magicstat://false"), true), "\n";
$f->set_flags(FILEINFO_MIME_TYPE);
try { $f->file("magicstat://throw"); } catch (Throwable $e) { echo $e->getMessage(), "\n"; }
$stream = fopen("magicmissing://x", "rb"); echo mime_content_type($stream), "\n"; fclose($stream);
restore_error_handler();
try { $f->file("magicstat://throw"); } catch (Throwable $e) { echo $e->getMessage(), "\n"; }
stream_wrapper_unregister("magicstat"); stream_wrapper_unregister("magicmissing");
unset($f, $other, $stream, $e);
"#);
    let normal = "stat 1\nstat 2\nread\ncast 0\nclose\ntext/plain\n";
    let failure = "stat 1\nstat 2\nfinfo::file(): Failed identify data 0:cannot stat `(null)'\nclose\nfalse\n";
    let procedural = "stat 1\nstat 2\nfinfo_file(): Failed identify data 0:cannot stat `(null)'\nclose\nfalse\n";
    let missing = "mime_content_type(): FileinfoMagicMissing::stream_stat is not implemented!\nmime_content_type(): FileinfoMagicMissing::stream_cast is not implemented!\ntext/plain\nclose\n";
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, format!("{normal}{normal}{failure}{normal}{procedural}{failure}stat 1\nstat 2\nsecond stat\n{missing}stat 1\nstat 2\nsecond stat\n"));
    assert!(out.stderr.contains("finfo::file(): Failed identify data 0:cannot read `(null)'"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Exception warnings honor handler masks and precede destruction of the invalid cast owner.
#[test]
fn test_fileinfo_magic_stream_exception_diagnostics() {
    if fileinfo_native::skip("test_fileinfo_magic_stream_exception_diagnostics") { return; }
    let source = r#"<?php
class FileinfoExceptionValue {
    private bool $fail = false;
    public function __construct(bool $fail) { $this->fail = $fail; }
    public function __destruct() { echo "value-dtor\n"; error_reporting(0); if ($this->fail) { throw new Error("dtor error"); } }
}
class FileinfoExceptionWrapper {
    public $context;
    private string $kind = "";
    private int $position = 0;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { $this->kind = substr($path, strrpos($path, "/") + 1); return true; }
    public function stream_stat(): array { return ["mode" => 0100644, "size" => 5]; }
    public function stream_read(int $count): string {
        echo "read\n";
        if ($this->kind == "read") { throw new Error("read error"); }
        $s = substr("HELLO", $this->position, $count); $this->position += strlen($s); return $s;
    }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function stream_tell(): int { return $this->position; }
    public function stream_seek(int $offset, int $whence): bool { $this->position = $offset; return true; }
    public function stream_cast(int $kind): mixed {
        echo "cast ", $kind, "\n";
        if ($this->kind == "value") { return new FileinfoExceptionValue(false); }
        if ($this->kind == "throwing") { return new FileinfoExceptionValue(true); }
        return 1;
    }
    public function stream_close(): void { echo "close\n"; }
}
class FileinfoExceptionFilter extends php_user_filter {
    public function onCreate(): bool { echo "create\n"; return true; }
    public function onClose(): void { echo "unexpected filter close\n"; }
    public function filter($in, $out, &$consumed, bool $closing): int {
        echo "filter\n";
        while ($bucket = stream_bucket_make_writeable($in)) { $consumed += $bucket->datalen; stream_bucket_append($out, $bucket); }
        throw new Error("filter error");
    }
}
function fileinfoExceptionQuery(finfo $f, string $path): void {
    try { $f->file($path); } catch (Throwable $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
        if (str_ends_with($path, "throwing")) { $p = $e->getPrevious(); echo $p === null ? "no previous\n" : get_class($p) . ": " . $p->getMessage() . "\n"; }
    }
}
stream_wrapper_register("exceptionfileinfo", "FileinfoExceptionWrapper");
stream_filter_register("fileinfo.exception", "FileinfoExceptionFilter");
$f = new finfo(FILEINFO_MIME_TYPE);
fileinfoExceptionQuery($f, "exceptionfileinfo://read");
foreach ([E_WARNING, E_USER_WARNING] as $mask) {
    set_error_handler(function(int $level, string $message): bool { echo "unexpected handler\n"; return true; }, $mask);
    fileinfoExceptionQuery($f, "exceptionfileinfo://read");
    fileinfoExceptionQuery($f, "exceptionfileinfo://int");
    restore_error_handler();
}
fileinfoExceptionQuery($f, "exceptionfileinfo://value"); error_reporting(E_ALL);
fileinfoExceptionQuery($f, "exceptionfileinfo://throwing"); error_reporting(E_ALL);
$stream = fopen("exceptionfileinfo://read", "rb");
try { mime_content_type($stream); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
fclose($stream);
fileinfoExceptionQuery($f, "php://filter/read=fileinfo.exception/resource=exceptionfileinfo://normal");
stream_wrapper_unregister("exceptionfileinfo"); unset($f, $stream, $mask, $e);
"#;
    let (out, _) = compile_and_run_with_heap_debug_and_asm(source);
    let read_error = "read\nError: read error\n";
    let invalid = "read\ncast 0\nTypeError: finfo::file(): supplied argument is not a valid stream resource\n";
    let expected = format!("{read_error}{read_error}{invalid}{read_error}{invalid}read\ncast 0\nvalue-dtor\nTypeError: finfo::file(): supplied argument is not a valid stream resource\nread\ncast 0\nvalue-dtor\nError: dtor error\nTypeError: finfo::file(): supplied argument is not a valid stream resource\n{read_error}close\ncreate\nread\nfilter\nError: filter error\n");
    assert!(out.success, "fileinfo program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert_eq!(out.stderr.matches("finfo::file(): Failed identify data 0:cannot read `(null)'").count(), 3, "{}", out.stderr);
    assert_eq!(out.stderr.matches("mime_content_type(): Failed identify data 0:cannot read `(null)'").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("FileinfoExceptionWrapper::stream_cast must return a stream resource").count(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
