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
    assert_eq!(out, "bool(true)\nint(1)\nstring(8) \"data.txt\"\nbool(false)\nstring(8) \"No error\"");
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
zip_close($directory);
@unlink($file);
"#);
    assert_eq!(out.replace("\r\n", "\n").trim_end(), "string(9) \"first.txt\"\nbool(true)\nstring(5) \"hello\"\nbool(true)");
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
        "bool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nbool(true)"
    );
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
$stream = fopen('zip://' . $archive . '#x.txt', 'rb');
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
    let out = compile_and_run(r#"<?php
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
    assert_eq!(
        out.replace("\r\n", "\n").trim_end(),
        "bool(true)\nbool(true)\nbool(true)\nstring(11) \"hello world\"\nstring(7) \"comment\""
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
