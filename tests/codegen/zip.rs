//! Purpose:
//! Focused end-to-end coverage for the `ext/zip` surface: archives, stats, mutations,
//! encryption, extraction, globbing, and the deprecated procedural API.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Each fixture runs in an isolated temporary directory, so archive side effects stay local.
//! - These tests are deliberately smaller than the migrated php-src corpus in `zip_php_src.rs`.

use crate::support::*;

/// Creates, closes, reopens, and reads a small archive.
#[test]
fn test_zip_create_read_roundtrip() {
    if skip_without_zip_native("test_zip_create_read_roundtrip") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/roundtrip.zip';
$zip = new ZipArchive();
var_dump($zip->open($file, ZipArchive::CREATE));
var_dump($zip->addFromString('hello.txt', 'hello world'));
var_dump($zip->addEmptyDir('dir'));
var_dump($zip->numFiles, $zip->lastId);
var_dump($zip->close());

$zip = new ZipArchive();
var_dump($zip->open($file));
var_dump($zip->numFiles);
var_dump($zip->getNameIndex(0));
var_dump($zip->getFromName('hello.txt'));
var_dump($zip->statName('hello.txt')['size']);
var_dump($zip->close());
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nbool(true)\nint(2)\nint(1)\nbool(true)\nbool(true)\nint(2)\nstring(9) \"hello.txt\"\nstring(11) \"hello world\"\nint(11)\nbool(true)");
}

/// Exercises comments, mtimes, rename/delete, and read-only properties.
#[test]
fn test_zip_mutations_and_properties() {
    if skip_without_zip_native("test_zip_mutations_and_properties") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/mutate.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('one.txt', '1');
$zip->addFromString('two.txt', '2');
var_dump($zip->renameIndex(0, 'renamed.txt'));
var_dump($zip->setCommentIndex(0, 'note'));
var_dump($zip->setMtimeIndex(0, 1600000000));
var_dump($zip->deleteName('two.txt'));
var_dump($zip->setArchiveComment('archive note'));
$zip->close();

$zip = new ZipArchive();
$zip->open($file, ZipArchive::RDONLY);
var_dump($zip->getNameIndex(0));
var_dump($zip->getCommentIndex(0));
var_dump($zip->statIndex(0)['mtime']);
var_dump($zip->getArchiveComment());
var_dump($zip->addEmptyDir('nope'));
var_dump($zip->status === ZipArchive::ER_RDONLY);
try {
    $zip->status = 1;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
var_dump($zip->close());
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nstring(11) \"renamed.txt\"\nstring(4) \"note\"\nint(1600000000)\nstring(12) \"archive note\"\nbool(false)\nbool(true)\nCannot write read-only property ZipArchive::$status\nbool(true)");
}

/// Verifies AES-256 encryption requires the password and succeeds with it.
#[test]
fn test_zip_encryption() {
    if skip_without_zip_native("test_zip_encryption") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/encrypted.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('clear.txt', 'clear');
$zip->addFromString('secret.txt', 'secret');
var_dump($zip->setEncryptionName('secret.txt', ZipArchive::EM_AES_256, 'hunter2'));
$zip->close();

$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->getFromName('clear.txt'));
var_dump($zip->getFromName('secret.txt'));
$zip->setPassword('hunter2');
var_dump($zip->getFromName('secret.txt'));
var_dump($zip->statName('secret.txt')['encryption_method'] === ZipArchive::EM_AES_256);
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nstring(5) \"clear\"\nbool(false)\nstring(6) \"secret\"\nbool(true)");
}

/// Extracts nested empty directories and file payloads.
#[test]
fn test_zip_extract_to() {
    if skip_without_zip_native("test_zip_extract_to") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/extract.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addEmptyDir('outer/inner');
$zip->addFromString('outer/file.txt', 'payload');
$zip->close();

$destination = __DIR__ . '/out';
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo($destination));
var_dump(is_dir($destination . '/outer/inner'));
var_dump(file_get_contents($destination . '/outer/file.txt'));
$zip->close();
@unlink($file);
@unlink($destination . '/outer/file.txt');
@rmdir($destination . '/outer/inner');
@rmdir($destination . '/outer');
@rmdir($destination);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nstring(7) \"payload\"");
}

/// Adds files with `addGlob`, including path rewriting options.
#[test]
fn test_zip_add_glob() {
    if skip_without_zip_native("test_zip_add_glob") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__ . '/glob';
@mkdir($dir);
file_put_contents($dir . '/one.txt', '1');
file_put_contents($dir . '/two.txt', '2');
$file = __DIR__ . '/glob.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$added = $zip->addGlob($dir . '/*.txt', 0, ['add_path' => 'bundle/', 'remove_all_path' => true]);
var_dump(is_array($added), count($added));
var_dump($zip->getNameIndex(0));
var_dump($zip->getNameIndex(1));
$zip->close();
@unlink($file);
@unlink($dir . '/one.txt');
@unlink($dir . '/two.txt');
@rmdir($dir);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nint(2)\nstring(14) \"bundle/one.txt\"\nstring(14) \"bundle/two.txt\"");
}

/// Verifies addGlob() reports a failed entry encryption instead of success.
#[test]
fn test_zip_add_glob_reports_encryption_failure() {
    if skip_without_zip_native("test_zip_add_glob_reports_encryption_failure") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__ . '/glob-enc';
@mkdir($dir);
file_put_contents($dir . '/one.txt', '1');
$file = __DIR__ . '/glob-enc.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
var_dump($zip->addGlob($dir . '/*.txt', 0, ['enc_method' => ZipArchive::EM_AES_256, 'enc_password' => "bad\0password"]));
$zip->close();
@unlink($file);
@unlink($dir . '/one.txt');
@rmdir($dir);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(false)");
}

/// Adds files with `addPattern` and verifies invalid patterns fail cleanly.
#[test]
fn test_zip_add_pattern() {
    if skip_without_zip_native("test_zip_add_pattern") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__ . '/pattern';
@mkdir($dir);
file_put_contents($dir . '/data.txt', 'data');
file_put_contents($dir . '/skip.dat', 'skip');
$file = __DIR__ . '/pattern.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$added = $zip->addPattern('/^data\.txt$/', $dir);
var_dump(is_array($added), count($added));
var_dump(basename($zip->getNameIndex(0)));
var_dump($zip->addPattern('/^DATA\.TXT$/i', $dir) === false);
var_dump($zip->addPattern('/invalid'));
var_dump($zip->getStatusString());
$zip->close();
@unlink($file);
@unlink($dir . '/data.txt');
@unlink($dir . '/skip.dat');
@rmdir($dir);
"#);
    let normalized = out.replace("\r\n", "\n");
    let out = normalized.trim_end();
    assert_eq!(out, "bool(true)\nint(1)\nstring(8) \"data.txt\"\nbool(false)\nbool(false)\nstring(8) \"No error\"");
}

/// Replays the deprecated procedural API over an archive.
#[test]
fn test_zip_procedural_api() {
    if skip_without_zip_native("test_zip_procedural_api") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/procedural.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('first.txt', 'hello');
$zip->addFromString('second.txt', 'world');
$zip->close();

$directory = zip_open($file);
$entry = zip_read($directory);
var_dump(zip_entry_name($entry));
var_dump(zip_entry_open($directory, $entry, 'rb'));
var_dump(zip_entry_read($entry, 5));
var_dump(zip_entry_close($entry));
var_dump(get_resource_type($entry));
zip_close($directory);
var_dump(get_resource_type($directory));
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(9) \"first.txt\"\nbool(true)\nstring(5) \"hello\"\nbool(true)\nstring(7) \"Unknown\"\nstring(7) \"Unknown\"");
}

/// Verifies the deprecated procedural `zip_open()` keeps libzip's `ER_NOZIP` result for a
/// pre-existing zero-byte file, unlike `ZipArchive::open()`'s empty-file compatibility path.
#[test]
fn test_zip_procedural_open_empty_file_returns_nozip() {
    if skip_without_zip_native("test_zip_procedural_open_empty_file_returns_nozip") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/empty-open.zip';
file_put_contents($file, '');
var_dump(zip_open($file));
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "int(19)");
}

/// Verifies string probes for the optional extension discover the injected surface.
#[test]
fn test_zip_extension_probes() {
    if skip_without_zip_native("test_zip_extension_probes") { return; }
    let out = compile_and_run(r#"<?php
var_dump(extension_loaded('zip'));
var_dump(class_exists('ZipArchive'));
var_dump(function_exists('zip_open'));
var_dump(ZipArchive::isEncryptionMethodSupported(ZipArchive::EM_AES_256));
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nbool(true)\nbool(true)");
}

/// Verifies the `zip://` fopen wrapper and its PHP-shaped stream metadata.
#[test]
fn test_zip_stream_wrapper_metadata() {
    if skip_without_zip_native("test_zip_stream_wrapper_metadata") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/stream.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('foo', 'value');
$zip->close();

$stream = fopen('zip://' . $file . '#foo', 'rb');
$meta = stream_get_meta_data($stream);
var_dump($meta['stream_type'], $meta['wrapper_type'], substr($meta['uri'], -4));
var_dump(stream_get_contents($stream));
fclose($stream);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(3) \"zip\"\nstring(11) \"zip wrapper\"\nstring(4) \"#foo\"\nstring(5) \"value\"");
}

/// Verifies the `zip://` wrapper surfaces a CRC mismatch instead of silently returning data.
#[test]
fn test_zip_stream_wrapper_reports_crc_error() {
    if skip_without_zip_native("test_zip_stream_wrapper_reports_crc_error") { return; }
    let out = compile_and_run_capture(r#"<?php
$file = __DIR__ . '/crc.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', str_repeat('A', 2000));
$zip->close();
$bytes = file_get_contents($file);
$position = strpos($bytes, "PK\x01\x02");
$byte = ord(substr($bytes, $position - 10, 1));
$corrupt = substr($bytes, 0, $position - 10) . chr($byte ^ 0xFF) . substr($bytes, $position - 9);
file_put_contents($file, $corrupt);
$data = file_get_contents('zip://' . $file . '#a.txt');
var_dump(strlen($data) < 2000);
@unlink($file);
"#);
    assert!(out.success, "fixture failed: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "bool(true)");
    assert!(
        out.stderr.contains("Zip stream error: CRC error"),
        "expected a CRC warning, got: {}",
        out.stderr
    );
}

/// Verifies two getStream() resources for the same entry keep independent state.
#[test]
fn test_zip_getstream_instances_are_independent() {
    if skip_without_zip_native("test_zip_getstream_instances_are_independent") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/streams.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'abcdef');
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
$first = $zip->getStream('a.txt');
$second = $zip->getStream('a.txt');
if (is_resource($first)) {
    if (is_resource($second)) {
        var_dump(fread($first, 3));
        var_dump(fread($second, 2));
        var_dump(fstat($second)['size']);
    }
}
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(3) \"abc\"\nstring(2) \"ab\"\nint(6)");
}

/// Verifies a subclass getStream() override keeps its declared string return type.
#[test]
fn test_zip_subclass_getstream_override_keeps_return_type() {
    if skip_without_zip_native("test_zip_subclass_getstream_override_keeps_return_type") { return; }
    let out = compile_and_run(r#"<?php
class CustomZip extends ZipArchive {
    public function getStream(string $name): string { return 'custom:' . $name; }
}
function takesString(string $value): void { echo $value; }
$zip = new CustomZip();
takesString($zip->getStream('a.txt'));
"#);
    assert_eq!(out, "custom:a.txt");
}

/// Verifies progress callbacks run during the actual archive write.
#[test]
fn test_zip_progress_callback_during_write() {
    if skip_without_zip_native("test_zip_progress_callback_during_write") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/progress.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
for ($i = 0; $i < 128; $i = $i + 1) { $zip->addFromString('f' . $i, str_repeat('A', 100000)); }
$intermediate = 0;
$zip->registerProgressCallback(0.01, function (float $progress) use (&$intermediate): void {
    if ($progress > 0.0 && $progress < 1.0) { $intermediate = $intermediate + 1; }
});
var_dump($zip->close());
var_dump($intermediate > 0);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)");
}

/// Verifies a cancel callback can stop a write after libzip has started it.
#[test]
fn test_zip_cancel_callback_during_write() {
    if skip_without_zip_native("test_zip_cancel_callback_during_write") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/cancel.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
for ($i = 0; $i < 128; $i = $i + 1) { $zip->addFromString('f' . $i, str_repeat('A', 100000)); }
$calls = 0;
$zip->registerCancelCallback(function () use (&$calls): int {
    $calls = $calls + 1;
    return $calls > 3 ? 1 : 0;
});
var_dump($zip->close());
var_dump($calls > 3);
var_dump($zip->status === ZipArchive::ER_CANCELLED);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(false)\nbool(true)\nbool(true)");
}

/// Verifies the `zip://` wrapper accepts only read modes, like php-src.
#[test]
fn test_zip_stream_wrapper_rejects_write_modes() {
    if skip_without_zip_native("test_zip_stream_wrapper_rejects_write_modes") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/modes.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
foreach (['r', 'rb', 'r+', 'w', 'wb', 'a', 'ab', 'x', 'c'] as $mode) {
    $stream = @fopen('zip://' . $file . '#a.txt', $mode);
    echo $mode, ': ', is_resource($stream) ? 'open' : 'closed', "\n";
    if (is_resource($stream)) { fclose($stream); }
}
$zip->close();
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "r: open\nrb: open\nr+: open\nw: closed\nwb: closed\na: closed\nab: closed\nx: closed\nc: closed"
    );
}

/// Verifies `zip://` scheme matching is case-insensitive, like php-src's URL parser.
#[test]
fn test_zip_stream_wrapper_scheme_is_case_insensitive() {
    if skip_without_zip_native("test_zip_stream_wrapper_scheme_is_case_insensitive") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/upper.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('foo', 'value');
$zip->close();

$stream = fopen('ZIP://' . $file . '#foo', 'rb');
$meta = stream_get_meta_data($stream);
var_dump($meta['stream_type'], $meta['wrapper_type']);
var_dump(stream_get_contents($stream));
fclose($stream);
var_dump(file_get_contents('ZIP://' . $file . '#foo'));
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(3) \"zip\"\nstring(11) \"zip wrapper\"\nstring(5) \"value\"\nstring(5) \"value\"");
}



/// Verifies addFile() defaults to FL_OVERWRITE, so a duplicate entry replaces like php-src.
#[test]
fn test_zip_addfile_default_overwrites_duplicate_entry() {
    if skip_without_zip_native("test_zip_addfile_default_overwrites_duplicate_entry") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/duplicate.zip';
file_put_contents(__DIR__ . '/a.txt', 'A');
file_put_contents(__DIR__ . '/b.txt', 'B');
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
var_dump($zip->addFile(__DIR__ . '/a.txt', 'same.txt'));
var_dump($zip->addFile(__DIR__ . '/b.txt', 'same.txt'));
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->getFromName('same.txt'));
$zip->close();
@unlink(__DIR__ . '/a.txt');
@unlink(__DIR__ . '/b.txt');
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nstring(1) \"B\"");
}

/// Verifies getComment* returns the empty string for an entry without a comment.
#[test]
fn test_zip_getcomment_without_comment_returns_empty_string() {
    if skip_without_zip_native("test_zip_getcomment_without_comment_returns_empty_string") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/comment.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->getCommentIndex(0));
var_dump($zip->getCommentName('a.txt'));
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(0) \"\"\nstring(0) \"\"");
}

/// Verifies removing an archive comment clears the saved comment property.
#[test]
fn test_zip_removing_archive_comment_clears_property() {
    if skip_without_zip_native("test_zip_removing_archive_comment_clears_property") { return; }
    let out = compile_and_run(r#"<?php
$zip = new ZipArchive();
$zip->open(__DIR__ . '/archive-comment.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
var_dump($zip->setArchiveComment('old comment'));
var_dump($zip->comment);
var_dump($zip->setArchiveComment(''));
var_dump($zip->comment);
$zip->close();
@unlink(__DIR__ . '/archive-comment.zip');
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nstring(11) \"old comment\"\nbool(true)\nstring(0) \"\"");
}

/// Verifies open_basedir is enforced by open(), not only by the glob/pattern helpers.
#[test]
fn test_zip_open_honors_open_basedir() {
    if skip_without_zip_native("test_zip_open_honors_open_basedir") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__;
mkdir($dir . '/allowed');
mkdir($dir . '/outside');
$make = new ZipArchive();
$make->open($dir . '/allowed/inside.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
$make->addFromString('a.txt', 'x');
$make->close();
$make->open($dir . '/outside/escape.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
$make->addFromString('a.txt', 'x');
$make->close();
symlink($dir . '/outside', $dir . '/allowed/link');
symlink($dir . '/allowed', $dir . '/allowed-link');
$zip = new ZipArchive();
ini_set('open_basedir', $dir . '/allowed');
var_dump($zip->open($dir . '/outside/escape.zip'));
var_dump(zip_open($dir . '/outside/escape.zip'));
var_dump($zip->open($dir . '/allowed/./inside.zip'));
var_dump($zip->open($dir . '/allowed/link/../outside/escape.zip'));
chdir($dir);
var_dump($zip->open('allowed/inside.zip'));
ini_set('open_basedir', $dir . '/allowed-link');
var_dump($zip->open($dir . '/allowed/inside.zip'));
ini_set('open_basedir', $dir . '/other:' . $dir . '/allowed');
var_dump($zip->open($dir . '/allowed/inside.zip'));
chdir($dir);
ini_set('open_basedir', 'allowed');
var_dump($zip->open($dir . '/allowed/inside.zip'));
$zip->close();
@unlink($dir . '/allowed-link');
@unlink($dir . '/allowed/link');
@unlink($dir . '/allowed/inside.zip');
@unlink($dir . '/outside/escape.zip');
@rmdir($dir . '/allowed');
@rmdir($dir . '/outside');
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(false)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(true)"
    );
}

/// Verifies extractTo() restores the entry mtime on every supported target.
#[test]
fn test_zip_extract_restores_mtime() {
    if skip_without_zip_native("test_zip_extract_restores_mtime") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/mtime.zip';
$dir = __DIR__ . '/mtime-out';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
$zip->setMtimeName('a.txt', 1700000000);
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo($dir));
$zip->close();
var_dump(filemtime($dir . '/a.txt') === 1700000000);
@unlink($dir . '/a.txt');
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)");
}

/// Verifies extractTo() rejects a corrupt payload instead of installing its bytes.
#[test]
fn test_zip_extract_to_rejects_crc_corrupt_payload() {
    if skip_without_zip_native("test_zip_extract_to_rejects_crc_corrupt_payload") { return; }
    let out = compile_and_run_capture(r#"<?php
$file = __DIR__ . '/extract-crc.zip';
$dir = __DIR__ . '/extract-crc-out';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'ABCDEF');
$zip->close();
$bytes = file_get_contents($file);
$position = strpos($bytes, 'ABCDEF');
$bytes = substr_replace($bytes, 'X', $position, 1);
file_put_contents($file, $bytes);
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo($dir));
var_dump(file_exists($dir . '/a.txt'));
$zip->close();
@unlink($dir . '/a.txt');
@rmdir($dir);
@unlink($file);
"#);
    assert!(out.success, "fixture failed: {}", out.stderr);
    assert_eq!(out.stdout.replace("\r\n", "\n").trim_end(), "bool(false)\nbool(false)");
    assert!(
        out.stderr.contains("Warning: ZipArchive::extractTo(): Cannot extract \"a.txt\": \"CRC error\""),
        "expected the failing entry name and libzip reason, got: {}",
        out.stderr
    );
}

/// Verifies `extractTo()` iterates array values, so filtered lists keep their keys.
///
/// PHP 8.5.11+ (GH-23647, `oo_extract_array_keys.phpt`) walks the values instead of
/// positions `0..count-1`; `array_filter()` preserves keys and is therefore a valid
/// selection even though it is not a packed list.
#[test]
fn test_zip_extract_to_accepts_non_sequential_file_keys() {
    if skip_without_zip_native("test_zip_extract_to_accepts_non_sequential_file_keys") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/extract-keys.zip';
$dir = __DIR__ . '/extract-keys-out';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('file0.txt', 'zero');
$zip->addFromString('file1.txt', 'one');
$zip->close();
$files = array_filter(['file0.txt', 'file1.txt'], fn($name) => $name === 'file1.txt');
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo($dir, $files));
var_dump(file_exists($dir . '/file1.txt'));
var_dump(file_exists($dir . '/file0.txt'));
$zip->close();
@unlink($dir . '/file1.txt');
@rmdir($dir);
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(true)\nbool(true)\nbool(false)"
    );
}

/// Verifies `close()` and mutators throw PHP's "Already being closed" from a callback.
#[test]
fn test_zip_close_rejects_reentrant_calls_from_progress_callback() {
    if skip_without_zip_native("test_zip_close_rejects_reentrant_calls_from_progress_callback") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/reentry.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', str_repeat('a', 100000));
$zip->addFromString('b.txt', str_repeat('b', 100000));
$zip->registerProgressCallback(0.0, function ($rate) use ($zip) {
    static $done = false;
    if ($done || $rate <= 0) { return; }
    $done = true;
    foreach ([
        'close' => fn() => $zip->close(),
        'delete' => fn() => $zip->deleteIndex(0),
        'add' => fn() => $zip->addFromString('c.txt', 'c'),
        'password' => fn() => $zip->setPassword('secret'),
        'register' => fn() => $zip->registerCancelCallback(fn() => 0),
    ] as $name => $operation) {
        try { $operation(); }
        catch (\Error $error) { echo $name, ': ', $error::class, ': ', $error->getMessage(), "\n"; }
    }
});
var_dump($zip->close());
$zip = new ZipArchive();
var_dump($zip->open($file, ZipArchive::CHECKCONS));
var_dump($zip->numFiles);
var_dump($zip->getFromName('a.txt') === str_repeat('a', 100000));
$zip->close();
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "close: Error: Already being closed\ndelete: Error: Already being closed\nadd: Error: Already being closed\npassword: Error: Already being closed\nregister: Error: Already being closed\nbool(true)\nbool(true)\nint(2)\nbool(true)"
    );
}

/// Verifies an exception from an intermediate progress callback is re-raised by `close()`.
#[test]
fn test_zip_close_propagates_intermediate_progress_exception() {
    if skip_without_zip_native("test_zip_close_propagates_intermediate_progress_exception") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/progress-throw.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', str_repeat('a', 100000));
$zip->addFromString('b.txt', str_repeat('b', 100000));
$zip->registerProgressCallback(0.0, function ($rate) {
    static $done = false;
    if ($done || $rate <= 0) { return; }
    $done = true;
    throw new \Exception('boo');
});
try { $zip->close(); }
catch (\Throwable $error) { echo $error::class, ': ', $error->getMessage(), "\n"; }
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "Exception: boo");
}

/// Verifies traditional PKWARE (ZipCrypto) writing and password reads.
#[test]
fn test_zip_zipcrypto_write() {
    if skip_without_zip_native("test_zip_zipcrypto_write") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/zipcrypto.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('legacy.txt', 'legacy payload');
var_dump($zip->setEncryptionName('legacy.txt', ZipArchive::EM_TRAD_PKWARE, 'secret'));
$zip->close();

$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->statName('legacy.txt')['encryption_method'] === ZipArchive::EM_TRAD_PKWARE);
var_dump($zip->getFromName('legacy.txt'));
$zip->setPassword('secret');
var_dump($zip->getFromName('legacy.txt'));
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nbool(false)\nstring(14) \"legacy payload\"");
}

/// Verifies setEncryptionName() without a password uses the archive default.
#[test]
fn test_zip_encryption_uses_archive_password() {
    if skip_without_zip_native("test_zip_encryption_uses_archive_password") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/default-password.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('secret.txt', 'classified');
var_dump($zip->setPassword('hunter2'));
var_dump($zip->setEncryptionName('secret.txt', ZipArchive::EM_AES_256));
$zip->close();

$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->getFromName('secret.txt'));
$zip->setPassword('hunter2');
var_dump($zip->getFromName('secret.txt'));
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(true)\nbool(false)\nstring(10) \"classified\"");
}

/// Verifies failed opens return PHP's integer error code and leave `status` at zero.
#[test]
fn test_zip_open_failure_returns_error_code() {
    if skip_without_zip_native("test_zip_open_failure_returns_error_code") { return; }
    let out = compile_and_run(r#"<?php
$missing = new ZipArchive();
var_dump($missing->open(__DIR__ . '/missing.zip'));
var_dump($missing->status);
$directory = new ZipArchive();
var_dump($directory->open(__DIR__));
var_dump($directory->status);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "int(9)\nint(0)\nint(28)\nint(0)");
}

/// Verifies reopening an archive commits pending changes instead of discarding them.
#[test]
fn test_zip_reopen_commits_pending_changes() {
    if skip_without_zip_native("test_zip_reopen_commits_pending_changes") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/reopen.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
$zip->addFromString('b.txt', 'B');
var_dump($zip->open($file));
var_dump($zip->numFiles);
$zip->close();
$check = new ZipArchive();
$check->open($file);
var_dump($check->numFiles);
var_dump($check->getFromName('b.txt'));
$check->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nint(2)\nint(2)\nstring(1) \"B\"");
}

/// Verifies closed archives throw PHP's ValueError for count() and getArchiveFlag().
#[test]
fn test_zip_closed_archive_value_errors() {
    if skip_without_zip_native("test_zip_closed_archive_value_errors") { return; }
    let out = compile_and_run(r#"<?php
$zip = new ZipArchive();
try { $zip->count(); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { $zip->getArchiveFlag(0); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "Invalid or uninitialized Zip object\nInvalid or uninitialized Zip object"
    );
}

/// Verifies extractTo() selection semantics and file:// destination handling match PHP.
#[test]
fn test_zip_extract_selection_and_file_uri() {
    if skip_without_zip_native("test_zip_extract_selection_and_file_uri") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/selection.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'A');
$zip->addFromString('dir/b.txt', 'B');
$zip->close();

$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo(__DIR__ . '/assoc', ['k' => 'a.txt']));
var_dump(file_exists(__DIR__ . '/assoc/a.txt'));
var_dump($zip->extractTo(__DIR__ . '/mixed', ['a.txt', 'k' => 'dir/b.txt']));
var_dump(file_exists(__DIR__ . '/mixed/a.txt'));
var_dump(file_exists(__DIR__ . '/mixed/dir/b.txt'));
var_dump($zip->extractTo(__DIR__ . '/missing', ['nope.txt']));
var_dump($zip->extractTo(__DIR__ . '/empty', []));
var_dump($zip->extractTo(__DIR__ . '/basename', ['b.txt']));
var_dump(file_exists(__DIR__ . '/basename/dir/b.txt'));
var_dump($zip->extractTo('file://' . __DIR__ . '/fileuri'));
var_dump(file_exists(__DIR__ . '/fileuri/a.txt'));
$zip->close();
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)"
    );
}

/// Verifies a ZipArchive subclass inherits the stream-getter resource shape.
#[test]
fn test_zip_subclass_getstream_typing() {
    if skip_without_zip_native("test_zip_subclass_getstream_typing") { return; }
    let out = compile_and_run(r#"<?php
class ZipChild extends ZipArchive {}
$file = __DIR__ . '/child.zip';
$zip = new ZipChild();
$zip->open($file, ZipArchive::CREATE);
$zip->addFromString('x.txt', 'v');
$zip->close();
$zip = new ZipChild();
$zip->open($file);
$stream = $zip->getStream('x.txt');
var_dump(is_resource($stream));
if (is_resource($stream)) { var_dump(fread($stream, 1)); fclose($stream); }
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nstring(1) \"v\"");
}

/// Verifies a `zip://` literal alone injects the wrapper without any ZipArchive reference.
#[test]
fn test_zip_literal_stream_wrapper_without_class() {
    if skip_without_zip_native("test_zip_literal_stream_wrapper_without_class") { return; }
    // Minimal stored ZIP containing `x.txt` => `hello`.
    let out = compile_and_run(r#"<?php
$archive = __DIR__ . '/literal.zip';
$bytes = base64_decode('UEsDBBQAAAAAADFhYV2GphA2BQAAAAUAAAAFAAAAeC50eHRoZWxsb1BLAQIUAxQAAAAAADFhYV2GphA2BQAAAAUAAAAFAAAAAAAAAAAAAACAAQAAAAB4LnR4dFBLBQYAAAAAAQABADMAAAAoAAAAAAA=');
file_put_contents($archive, $bytes);
$stream = fopen('ZIP://' . $archive . '#x.txt', 'rb');
var_dump(is_resource($stream));
if ($stream !== false) { echo stream_get_contents($stream), "\n"; fclose($stream); }
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nhello");
}

/// Verifies `file_get_contents()` honors a zip stream context password.
#[test]
fn test_zip_file_get_contents_context_password() {
    if skip_without_zip_native("test_zip_file_get_contents_context_password") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/context.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('bar.txt', 'bar');
$zip->setEncryptionName('bar.txt', ZipArchive::EM_AES_256, 'secret');
$zip->close();
var_dump(file_get_contents('zip://' . $file . '#bar.txt'));
$context = stream_context_create(['zip' => ['password' => 'secret']]);
var_dump(file_get_contents('zip://' . $file . '#bar.txt', false, $context));
@unlink($file);
"#);
    let normalized = out.replace("\r\n", "\n");
    assert!(normalized.contains("bool(false)"), "{normalized}");
    assert!(normalized.contains("string(3) \"bar\""), "{normalized}");
}

/// Verifies pending memory entries expose PHP's uncompressed CM_STORE stat shape.
#[test]
fn test_zip_pending_stat_shape() {
    if skip_without_zip_native("test_zip_pending_stat_shape") { return; }
    let out = compile_and_run(r#"<?php
$zip = new ZipArchive();
$zip->open(__DIR__ . '/pending.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'hello');
$stat = $zip->statName('a.txt');
var_dump($stat['crc'], $stat['comp_size'], $stat['comp_method']);
$zip->close();
@unlink(__DIR__ . '/pending.zip');
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "int(0)\nint(5)\nint(0)");
}

/// Verifies deflated getStream() descriptors refuse seeks while stored ones allow them.
#[test]
fn test_zip_getstream_seekability() {
    if skip_without_zip_native("test_zip_getstream_seekability") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/seek.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('store.txt', 'x');
$zip->addFromString('deflate.txt', str_repeat('A', 5000));
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
$stored = $zip->getStream('store.txt');
var_dump(fseek($stored, 0));
$deflated = $zip->getStream('deflate.txt');
var_dump(fseek($deflated, 0));
var_dump($zip->statName('deflate.txt')['comp_method']);
$zip->close();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "int(0)\nint(-1)\nint(8)");
}

/// Verifies CREATE leaves no file behind until an entry is committed.
#[test]
fn test_zip_create_defers_file_creation() {
    if skip_without_zip_native("test_zip_create_defers_file_creation") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/deferred.zip';
$zip = new ZipArchive();
var_dump($zip->open($file, ZipArchive::CREATE));
var_dump(file_exists($file));
var_dump($zip->close());
var_dump(file_exists($file));
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(false)\nbool(true)\nbool(false)");
}

/// Verifies zip diagnostics use PHP's engine levels, not user-triggered ones.
///
/// `set_error_handler()` receives `E_WARNING`/`E_DEPRECATED` from php-src's
/// `php_error_docref()` calls. A `trigger_error(..., E_USER_WARNING)` would report `512`
/// instead of `2`, and `E_USER_DEPRECATED` would report `16384` instead of `8192`.
#[test]
fn test_zip_diagnostics_use_engine_error_levels() {
    if skip_without_zip_native("test_zip_diagnostics_use_engine_error_levels") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/levels.zip';
set_error_handler(function ($level, $message) {
    echo $level, "\n";
    return true;
});
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFile(__DIR__ . '/missing-file.txt', 'missing.txt');
$zip->close();
$handle = zip_open($file);
if (is_resource($handle)) { zip_close($handle); }
restore_error_handler();
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "2\n8192");
}

/// Verifies setPassword("") is refused and does not replace the previous password.
#[test]
fn test_zip_empty_password_is_refused() {
    if skip_without_zip_native("test_zip_empty_password_is_refused") { return; }
    let out = compile_and_run(r#"<?php
$zip = new ZipArchive();
$zip->open(__DIR__ . '/password.zip', ZipArchive::CREATE | ZipArchive::OVERWRITE);
var_dump($zip->setPassword('secret'));
var_dump($zip->setPassword(''));
$zip->close();
@unlink(__DIR__ . '/password.zip');
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nbool(false)");
}

/// Verifies a header-only rewrite preserves a CRC-corrupt payload instead of emptying it.
#[test]
fn test_zip_crc_corrupt_payload_survives_close() {
    if skip_without_zip_native("test_zip_crc_corrupt_payload_survives_close") { return; }
    let out = compile_and_run_capture(r#"<?php
$file = __DIR__ . '/crc.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'hello world');
$zip->close();

$bytes = file_get_contents($file);
$central = strpos($bytes, "PK\x01\x02");
$bytes = substr_replace($bytes, chr(ord($bytes[$central + 16]) ^ 0xFF), $central + 16, 1);
file_put_contents($file, $bytes);

$zip = new ZipArchive();
var_dump($zip->open($file));
var_dump($zip->setCommentName('a.txt', 'comment'));
var_dump($zip->close());
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->getFromName('a.txt'));
var_dump($zip->getCommentName('a.txt'));
$zip->close();
@unlink($file);
"#);
    assert!(out.success, "fixture failed: {}", out.stderr);
    assert_eq!(
        out.stdout.replace("\r\n", "\n").trim_end(),
        "bool(true)\nbool(true)\nbool(true)\nbool(false)\nstring(7) \"comment\""
    );
}

/// Verifies CHECKCONS rejects a central entry whose local-header offset is broken.
#[test]
fn test_zip_checkcons_rejects_broken_local_offset() {
    if skip_without_zip_native("test_zip_checkcons_rejects_broken_local_offset") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/checkcons.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'hello');
$zip->close();

$bytes = file_get_contents($file);
$central = strpos($bytes, "PK\x01\x02");
$bytes = substr_replace($bytes, "\xff\xff\xff\xff", $central + 42, 4);
file_put_contents($file, $bytes);

$zip = new ZipArchive();
var_dump($zip->open($file, ZipArchive::CHECKCONS));
var_dump($zip->status);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "int(35)\nint(0)");
}

/// Verifies empty entry-name arguments raise PHP's `ValueError`, not `false`.
#[test]
fn test_zip_empty_name_arguments_throw_value_errors() {
    if skip_without_zip_native("test_zip_empty_name_arguments_throw_value_errors") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/empty-names.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'x');
$zip->close();
$zip->open($file);
$calls = [
    'addFile' => fn() => $zip->addFile(''),
    'replaceFile' => fn() => $zip->replaceFile('', 0),
    'renameIndex' => fn() => $zip->renameIndex(0, ''),
    'renameName' => fn() => $zip->renameName('a.txt', ''),
    'statName' => fn() => $zip->statName(''),
    'getFromName' => fn() => $zip->getFromName(''),
    'getStreamName' => fn() => $zip->getStreamName(''),
    'setCommentName' => fn() => $zip->setCommentName('', 'c'),
    'setMtimeName' => fn() => $zip->setMtimeName('', 0),
    'setCompressionName' => fn() => $zip->setCompressionName('', 0),
    'setEncryptionName' => fn() => $zip->setEncryptionName('', 0),
    'setExternalAttributesName' => fn() => $zip->setExternalAttributesName('', 0, 0),
];
foreach ($calls as $name => $call) {
    try { $call(); echo $name, ": no error\n"; }
    catch (\ValueError $error) { echo $name, ': ', $error->getMessage(), "\n"; }
}
$zip->close();
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "addFile: ZipArchive::addFile(): Argument #1 ($filepath) must not be empty\n\
replaceFile: ZipArchive::replaceFile(): Argument #1 ($filepath) must not be empty\n\
renameIndex: ZipArchive::renameIndex(): Argument #2 ($new_name) must not be empty\n\
renameName: ZipArchive::renameName(): Argument #2 ($new_name) must not be empty\n\
statName: ZipArchive::statName(): Argument #1 ($name) must not be empty\n\
getFromName: ZipArchive::getFromName(): Argument #1 ($name) must not be empty\n\
getStreamName: ZipArchive::getStreamName(): Argument #1 ($name) must not be empty\n\
setCommentName: ZipArchive::setCommentName(): Argument #1 ($name) must not be empty\n\
setMtimeName: ZipArchive::setMtimeName(): Argument #1 ($name) must not be empty\n\
setCompressionName: ZipArchive::setCompressionName(): Argument #1 ($name) must not be empty\n\
setEncryptionName: ZipArchive::setEncryptionName(): Argument #1 ($name) must not be empty\n\
setExternalAttributesName: ZipArchive::setExternalAttributesName(): Argument #1 ($name) must not be empty"
    );
}

/// Verifies an empty archive path in `zip://#entry` fails the open instead of throwing.
#[test]
fn test_zip_stream_wrapper_empty_archive_path_returns_false() {
    if skip_without_zip_native("test_zip_stream_wrapper_empty_archive_path_returns_false") { return; }
    let out = compile_and_run(r#"<?php
var_dump(fopen('zip://#entry', 'r'));
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(false)");
}

/// Verifies the procedural entry-size helpers match php-src's `int|false` shape.
#[test]
fn test_zip_entry_size_helpers_return_false_and_type_error() {
    if skip_without_zip_native("test_zip_entry_size_helpers_return_false_and_type_error") { return; }
    let out = compile_and_run(r#"<?php
set_error_handler(function () { return true; });
$file = __DIR__ . '/entry-size.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'hello');
$zip->close();
$resource = zip_open($file);
$entry = zip_read($resource);
var_dump(zip_entry_filesize($entry));
var_dump(zip_entry_compressedsize($entry));
zip_entry_close($entry);
try { zip_entry_filesize($entry); }
catch (\TypeError $error) { echo $error->getMessage(), "\n"; }
try { zip_entry_compressedsize($entry); }
catch (\TypeError $error) { echo $error->getMessage(), "\n"; }
zip_close($resource);
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "int(5)\nint(5)\nzip_entry_filesize(): supplied resource is not a valid Zip Entry resource\nzip_entry_compressedsize(): supplied resource is not a valid Zip Entry resource"
    );
}

/// Verifies progress callbacks can read the archive while `close()` is writing.
#[test]
fn test_zip_progress_callback_reads_archive_during_close() {
    if skip_without_zip_native("test_zip_progress_callback_reads_archive_during_close") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/progress-read.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
for ($i = 0; $i < 40; $i++) {
    $zip->addFromString("f$i.txt", str_repeat('x', 2000));
}
$zip->registerProgressCallback(0.5, function (float $rate) use ($zip) {
    if ($rate <= 0.0) {
        echo 'start num=', count($zip), ' stat=', $zip->statIndex(0) === false ? 'false' : 'ok', "\n";
    }
    if ($rate >= 1.0) {
        echo 'end num=', count($zip), ' name=', $zip->getNameIndex(0), "\n";
    }
});
var_dump($zip->close());
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "start num=40 stat=ok\nend num=40 name=f0.txt\nbool(true)"
    );
}

/// Verifies a no-op close does not invoke a registered cancel callback.
#[test]
fn test_zip_noop_close_skips_cancel_callback() {
    if skip_without_zip_native("test_zip_noop_close_skips_cancel_callback") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/noop-cancel.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'x');
$zip->close();
$zip->open($file);
$calls = 0;
$zip->registerCancelCallback(function () use (&$calls) { $calls++; return 1; });
var_dump($zip->close(), $calls, $zip->status);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "bool(true)\nint(0)\nint(0)");
}

/// Verifies extract-all uses `ZIP_FL_UNCHANGED` names, like php-src.
#[test]
fn test_zip_extract_all_uses_unchanged_names() {
    if skip_without_zip_native("test_zip_extract_all_uses_unchanged_names") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__ . '/unchanged';
mkdir($dir);
$file = $dir . '/a.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('old.txt', 'data');
$zip->close();

$zip->open($file);
$zip->addFromString('x.txt', 'data');
var_dump($zip->extractTo($dir));
var_dump(file_exists($dir . '/x.txt'));
$zip->close();

$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('old.txt', 'data');
$zip->close();
$zip->open($file);
$zip->renameName('old.txt', 'new.txt');
var_dump($zip->extractTo($dir));
var_dump(file_exists($dir . '/old.txt'), file_exists($dir . '/new.txt'));
$zip->close();

@unlink($dir . '/old.txt');
@unlink($dir . '/new.txt');
@unlink($file);
@rmdir($dir);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)"
    );
}

/// Verifies `..` is resolved and a symlinked parent cannot create outside directories.
#[test]
fn test_zip_extract_resolves_dotdot_and_rejects_symlinked_parent() {
    if skip_without_zip_native("test_zip_extract_resolves_dotdot_and_rejects_symlinked_parent") { return; }
    let out = compile_and_run(r#"<?php
$dest = __DIR__ . '/dotdot';
$file = __DIR__ . '/dotdot.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('sub/../../escape.txt', 'escaped');
$zip->close();
$zip->open($file);
var_dump($zip->extractTo($dest));
var_dump(file_exists($dest . '/escape.txt'));
var_dump(file_exists($dest . '/sub/escape.txt'));
$zip->close();
@unlink($dest . '/escape.txt');
@rmdir($dest);

$outside = __DIR__ . '/outside';
$dest = __DIR__ . '/symlink-parent';
mkdir($outside);
mkdir($dest);
symlink($outside, $dest . '/sub');
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('sub/deep/x.txt', 'x');
$zip->close();
$zip = new ZipArchive();
$zip->open($file);
var_dump($zip->extractTo($dest));
var_dump(is_dir($outside . '/deep'));
$zip->close();
@unlink($file);
@unlink($dest . '/sub');
@rmdir($dest);
@rmdir($outside);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)"
    );
}

/// Verifies `getCommentName()` locates with flags 0, so `FL_NOCASE` does not match.
#[test]
fn test_zip_get_comment_name_ignores_lookup_flags() {
    if skip_without_zip_native("test_zip_get_comment_name_ignores_lookup_flags") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/comment-flags.zip';
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'x');
$zip->setCommentName('a.txt', 'note');
$zip->close();
$zip->open($file);
var_dump($zip->getCommentName('A.TXT', ZipArchive::FL_NOCASE));
var_dump($zip->getCommentName('a.txt'));
$zip->close();
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(false)\nstring(4) \"note\""
    );
}

/// Verifies a `zip://` mention that is not a path argument does not inject the bridge.
#[test]
fn test_zip_scheme_text_does_not_inject_bridge() {
    let out = compile_and_run(r#"<?php
echo "see zip://docs for details\n";
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "see zip://docs for details"
    );
}

/// Verifies the procedural deprecation text follows the `--php-version` profile.
#[test]
fn test_zip_procedural_deprecation_follows_php_version() {
    if skip_without_zip_native("test_zip_procedural_deprecation_follows_php_version") { return; }
    let source = r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $message, "\n";
    return true;
});
zip_open(__DIR__ . '/missing-deprecation.zip');
"#;
    use elephc::php_version::PhpVersion;
    for version in [PhpVersion::Php82, PhpVersion::Php83] {
        let out = compile_and_run_with_php_version(source, version);
        assert_eq!(
            out.replace("\r\n", "\n").trim_end(),
            "Function zip_open() is deprecated",
            "--php-version {version}"
        );
    }
    for version in [PhpVersion::Php84, PhpVersion::Php85] {
        let out = compile_and_run_with_php_version(source, version);
        assert_eq!(
            out.replace("\r\n", "\n").trim_end(),
            "Function zip_open() is deprecated since 8.0, use ZipArchive::open() instead",
            "--php-version {version}"
        );
    }
}

/// Verifies extractTo() refuses to overwrite a pre-planted final-component symlink.
#[test]
fn test_zip_extract_rejects_symlinked_file_target() {
    if skip_without_zip_native("test_zip_extract_rejects_symlinked_file_target") { return; }
    let out = compile_and_run(r#"<?php
$dir = __DIR__ . '/symlink-file';
$file = __DIR__ . '/symlink-file.zip';
mkdir($dir);
mkdir($dir . '/outside');
file_put_contents($dir . '/outside/target.txt', 'untouched');
symlink($dir . '/outside/target.txt', $dir . '/a.txt');
$zip = new ZipArchive();
$zip->open($file, ZipArchive::CREATE | ZipArchive::OVERWRITE);
$zip->addFromString('a.txt', 'archive');
$zip->close();
$zip->open($file);
var_dump($zip->extractTo($dir));
$zip->close();
var_dump(file_get_contents($dir . '/outside/target.txt'));
@unlink($file);
@unlink($dir . '/a.txt');
@unlink($dir . '/outside/target.txt');
@rmdir($dir . '/outside');
@rmdir($dir);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(false)\nstring(9) \"untouched\""
    );
}

/// Verifies a `zip://` literal stored in a variable still injects the prelude.
#[test]
fn test_zip_scheme_literal_in_variable_injects_bridge() {
    if skip_without_zip_native("test_zip_scheme_literal_in_variable_injects_bridge") { return; }
    let out = compile_and_run(r#"<?php
$file = __DIR__ . '/auto-detect.zip';
file_put_contents($file, base64_decode('UEsDBAoAAAAAAHiBBDPps6IEBAAAAAQAAAADABUAYmFyVVQJAAMkIvJCTiLyQlV4BAD0AWQAYmFyClBLAwQKAAAAAACOgQQzAAAAAAAAAAAAAAAABwAVAGZvb2Jhci9VVAkAA0wi8kKCIvJCVXgEAPQBZABQSwMEFAAAAAgAjoEEM1g+xR0YAAAAGwAAAAoAFQBmb29iYXIvYmF6VVQJAANMIvJCTiLyQlV4BAD0AWQAS8pJTMpJVACSqUUKRYlJYLr48J6cKi4AUEsDBBQAAAAIAKGh5jTbZAz0CgAAAAgAAAAKAAAAZW50cnkxLnR4dEvNKymqVFA2BABQSwECFwMKAAAAAAB4gQQz6bOiBAQAAAAEAAAAAwANAAAAAAABAAAAtIEAAAAAYmFyVVQFAAMkIvJCVXgAAFBLAQIXAwoAAAAAAI6BBDMAAAAAAAAAAAAAAAAHAA0AAAAAAAAAEAD9QToAAABmb29iYXIvVVQFAANMIvJCVXgAAFBLAQIXAxQAAAAIAI6BBDNYPsUdGAAAABsAAAAKAA0AAAAAAAEAAAC0gXQAAABmb29iYXIvYmF6VVQFAANMIvJCVXgAAFBLAQIAABQAAAAIAKGh5jTbZAz0CgAAAAgAAAAKAAAAAAAAAAAAAAAAAMkAAABlbnRyeTEudHh0UEsFBgAAAAAEAAQA/QAAAPsAAAAAAA=='));
$path = 'zip://' . $file . '#entry1.txt';
var_dump(file_get_contents($path));
@unlink($file);
"#);
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "string(8) \"entry #1\""
    );
}
