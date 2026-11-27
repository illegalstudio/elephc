//! Purpose:
//! End-to-end coverage for the PHP 8.6 `ZipArchive` surface and its `elephc_zip`
//! bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext zip`.
//!
//! Key details:
//! - The two-entry fixture archive is embedded as base64 so the compiled program
//!   needs no external path; `crates/elephc-zip/tests/fixtures/sample.zip` is
//!   the byte-identical copy used by the bridge's Rust round-trip tests.
//! - Programs run inside the test's isolated current directory, so relative
//!   extraction paths cannot collide across parallel tests.
//! - `openString()`/`closeString()` require the PHP 8.6 profile.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Base64 of the committed two-entry ZIP fixture (deflated + stored).
const SAMPLE_ZIP: &str = "UEsDBBQAAAAIAAAAIVwMXYaPDgAAAAwAAAAJAAAAaGVsbG8udHh080jNycnXUYjyDFDkAgBQSwMEFAAAAAAAAAAhXOKcU6UHAAAABwAAAAoAAABzdG9yZWQudHh0c3RvcmVkClBLAQIUAxQAAAAIAAAAIVwMXYaPDgAAAAwAAAAJAAAAAAAAAAAAAACAAQAAAABoZWxsby50eHRQSwECFAMUAAAAAAAAACFc4pxTpQcAAAAHAAAACgAAAAAAAAAAAAAAgAE1AAAAc3RvcmVkLnR4dFBLBQYAAAAAAgACAG8AAABkAAAAAAA=";

/// Reading an in-memory archive preserves names, payloads, metadata and bytes.
#[test]
fn test_ziparchive_open_string_reads_entries() {
    let source = r#"<?php
$data = base64_decode("__SAMPLE_ZIP__");
if (!is_string($data)) {
    echo "decode failed\n";
    exit(1);
}
$zip = new ZipArchive();
$opened = $zip->openString($data);
echo $opened === true ? "open" : "fail", "\n";
echo $zip->numFiles, ":", count($zip), "\n";
echo $zip->getNameIndex(0), ":", $zip->getNameIndex(1), "\n";
echo $zip->getFromName("hello.txt");
echo $zip->getFromIndex(1);
$missing = $zip->locateName("missing");
echo $zip->locateName("stored.txt"), ":", $missing === false ? "none" : "found", "\n";
$stat = $zip->statName("hello.txt");
echo $stat["name"], ":", $stat["size"], ":", $stat["comp_method"], "\n";
$unchanged = $zip->closeString();
echo $unchanged === $data ? "same" : "different", "\n";
"#
    .replace("__SAMPLE_ZIP__", SAMPLE_ZIP);
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(
        out,
        "open\n2:2\nhello.txt:stored.txt\nHello, ZIP!\nstored\n1:none\nhello.txt:12:8\nsame\n"
    );
}

/// Creating an archive, editing it, reserializing it and extracting it round-trips.
#[test]
fn test_ziparchive_create_edit_and_extract() {
    let source = r#"<?php
$zip = new ZipArchive();
$opened = $zip->openString();
echo $opened === true ? "create" : "fail", "\n";
$zip->addFromString("hello.txt", "Hello\n");
$zip->addEmptyDir("dir");
$zip->addFromString("dir/a.txt", "A");
$zip->setArchiveComment("arch");
$zip->setCompressionName("hello.txt", ZipArchive::CM_STORE);
$zip->setMtimeName("hello.txt", 1700000000);
$zip->setCommentName("hello.txt", "note");
echo $zip->numFiles, ":", $zip->lastId, "\n";
$bytes = $zip->closeString();
if (!is_string($bytes)) {
    echo "close failed\n";
    exit(1);
}
echo "bytes\n";

$copy = new ZipArchive();
$copy->openString($bytes);
echo $copy->getFromName("hello.txt"), $copy->getFromName("dir/a.txt"), "\n";
echo $copy->getArchiveComment(), ":", $copy->getCommentName("hello.txt"), "\n";
$stat = $copy->statName("hello.txt");
echo $stat["comp_method"], ":", $stat["mtime"], "\n";
$copy->renameName("hello.txt", "renamed.txt");
$copy->deleteName("dir/a.txt");
echo $copy->numFiles, ":", $copy->getNameIndex(0), "\n";
$extracted = $copy->extractTo("zip_out");
echo $extracted ? "extracted" : "failed", "\n";
echo file_get_contents("zip_out/renamed.txt");
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(
        out,
        "create\n3:2\nbytes\nHello\nA\narch:note\n0:1700000000\n2:renamed.txt\nextracted\nHello\n"
    );
}

/// PHP 8.6 errors, closed-handle guards and serialization stay observable.
#[test]
fn test_ziparchive_errors_and_serialization() {
    let source = r#"<?php
$zip = new ZipArchive();
echo $zip->openString("not a zip"), ":", $zip->getStatusString(), "\n";
echo $zip->openString("", ZipArchive::EXCL), ":", $zip->getStatusString(), "\n";
$zip->openString();
try {
    serialize($zip);
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}
try {
    $zip->extractTo("out", [1]);
} catch (Throwable $error) {
    echo get_class($error), "\n";
}
try {
    $zip->addGlob("*.txt", 0, ["remove_all_path" => 1]);
} catch (Throwable $error) {
    echo get_class($error), "\n";
}
try {
    $zip->addPattern("*", ".", ["comp_method" => []]);
} catch (Throwable $error) {
    echo get_class($error), "\n";
}
$zip->close();
echo $zip->getFromName("missing") === false ? "closed" : "open", "\n";

class RoundTripZip extends ZipArchive
{
    public function __serialize(): array
    {
        $data = $this->closeString();
        return ["data" => is_string($data) ? $data : ""];
    }

    public function __unserialize(array $data): void
    {
        $this->openString($data["data"]);
    }
}

$round = new RoundTripZip();
$round->openString();
$round->addFromString("a.txt", "A");
$restored = unserialize(serialize($round));
echo $restored->getFromName("a.txt"), "\n";
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(
        out,
        "19:Not a zip archive\n10:File already exists\n\
Exception:Serialization of 'ZipArchive' is not allowed\n\
TypeError\nTypeError\nTypeError\nclosed\nA\n"
    );
}

/// `addGlob()` stores real files and `extractTo()` restores them.
#[test]
fn test_ziparchive_add_glob_and_extract_round_trip() {
    let source = r#"<?php
@mkdir("globdir");
file_put_contents("globdir/a.txt", "A");
file_put_contents("globdir/b.md", "B");
$zip = new ZipArchive();
$zip->openString();
$added = $zip->addGlob("globdir/*.txt", 0, [
    "remove_all_path" => true,
    "comp_method" => ZipArchive::CM_STORE,
]);
if (!is_array($added)) {
    echo "glob failed\n";
    exit(1);
}
$first = $added[0];
echo count($added), ":", $first, "\n";
$bytes = $zip->closeString();
if (!is_string($bytes)) {
    echo "close failed\n";
    exit(1);
}
$copy = new ZipArchive();
$copy->openString($bytes);
echo $copy->getFromName("a.txt"), ":", $copy->locateName("b.md") === false ? "one" : "two", "\n";
echo $copy->statName("a.txt")["comp_method"], "\n";
$copy->extractTo("glob_out");
echo file_get_contents("glob_out/a.txt"), "\n";
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(out, "1:a.txt\nA:one\n0\nA\n");
}

/// File-backed `open()` commits on `close()` and `RDONLY` rejects mutations.
#[test]
fn test_ziparchive_file_open_commits_and_readonly() {
    let source = r#"<?php
$file = new ZipArchive();
$created = $file->open("persisted.zip", ZipArchive::CREATE);
echo $created === true ? "created" : "fail", "\n";
$file->addFromString("persist.txt", "P");
$file->close();

$read = new ZipArchive();
$read->open("persisted.zip");
echo $read->getFromName("persist.txt"), "\n";
$read->close();

$readonly = new ZipArchive();
$readonly->open("persisted.zip", ZipArchive::RDONLY);
$added = $readonly->addFromString("no.txt", "N");
echo $added === false ? "readonly" : "writable", ":", $readonly->status, "\n";
$readonly->close();

$missing = new ZipArchive();
echo $missing->open("does-not-exist.zip"), "\n";
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(out, "created\nP\nreadonly:25\n9\n");
}
