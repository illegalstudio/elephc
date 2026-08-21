//! Purpose:
//! End-to-end tests for SPL file and directory iterator classes.
//! Covers Phase 8 metadata, file info/object behavior, directory snapshots, glob iteration, and recursive wrappers.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Fixtures create and remove files under isolated codegen temp directories.
//! - Directory tests avoid relying on libc directory-entry ordering.

use crate::support::*;

/// Verifies that Phase 8 SPL classes are declared and implement expected contracts.
#[test]
fn test_filesystem_spl_classes_are_declared_and_implement_contracts() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function has_name(array $names, string $target): bool {
    foreach ($names as $name) {
        if ($name === $target) {
            return true;
        }
    }
    return false;
}

file_put_contents("meta.txt", "one\n");
$names = spl_classes();

var_dump(class_exists("SplFileInfo"));
var_dump(class_exists("SplFileObject"));
var_dump(class_exists("SplTempFileObject"));
var_dump(class_exists("DirectoryIterator"));
var_dump(class_exists("FilesystemIterator"));
var_dump(class_exists("GlobIterator"));
var_dump(class_exists("RecursiveDirectoryIterator"));
var_dump(class_exists("RecursiveCachingIterator"));
var_dump(has_name($names, "SplFileInfo"));
var_dump(has_name($names, "RecursiveCachingIterator"));

$info = new SplFileInfo("meta.txt");
var_dump($info instanceof Stringable);
$file = new SplFileObject("meta.txt");
var_dump($file instanceof SplFileInfo);
var_dump($file instanceof RecursiveIterator);
var_dump($file instanceof SeekableIterator);
var_dump(new SplTempFileObject() instanceof SplFileObject);
var_dump(new DirectoryIterator(".") instanceof Iterator);
var_dump(new FilesystemIterator(".") instanceof DirectoryIterator);
var_dump(new GlobIterator("*.txt") instanceof Countable);
var_dump(new RecursiveDirectoryIterator(".") instanceof RecursiveIterator);
var_dump(new RecursiveCachingIterator(new RecursiveArrayIterator([])) instanceof CachingIterator);
var_dump(SplFileObject::DROP_NEW_LINE);
var_dump(SplFileObject::READ_CSV);
var_dump(FilesystemIterator::CURRENT_AS_PATHNAME);
var_dump(FilesystemIterator::KEY_AS_FILENAME);
var_dump(FilesystemIterator::SKIP_DOTS);
var_dump(RecursiveDirectoryIterator::FOLLOW_SYMLINKS);
unlink("meta.txt");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(1)\n",
            "int(8)\n",
            "int(32)\n",
            "int(256)\n",
            "int(4096)\n",
            "int(16384)\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplFileInfo path/stat helpers and SplFileObject line iteration.
#[test]
fn test_spl_file_info_and_file_object_behavior() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("docs");
file_put_contents("docs/a.txt", "one\ntwo\n");

$info = new SplFileInfo("docs/a.txt");
echo $info->getFilename();
echo "|";
echo $info->getExtension();
echo "|";
echo $info->getBasename(".txt");
echo "|";
echo $info->getPath();
echo "|";
echo $info->isFile() ? "file" : "no";
echo "|";
echo $info->getSize();
echo "\n";

$file = $info->openFile();
foreach ($file as $line => $text) {
    echo $line;
    echo ":";
    echo trim($text);
    echo ";";
}
echo "\n";

$file->seek(1);
echo trim($file->current());
echo "|";
$file->rewind();
echo trim($file->fgets());
echo "|";
echo $file->key();
echo "\n";

$csv = new SplFileObject("docs/a.txt");
$csv->setFlags(SplFileObject::READ_CSV);
$csv->setCsvControl("n");
$row = $csv->current();
echo count($row);
echo ":";
echo $row[0];
echo ":";
echo trim($row[1]);
echo "\n";

$tmp = new SplTempFileObject();
$tmp->fwrite("temp\nline\n");
$tmp->rewind();
echo trim($tmp->fgets());
echo "|";
echo trim($tmp->fgets());
echo "\n";

unlink("docs/a.txt");
rmdir("docs");
"#,
    );
    assert_eq!(
        out,
        "a.txt|txt|a|docs|file|8\n0:one;1:two;\ntwo|one|1\n2:o:e\ntemp|line\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplFileInfo factories honor explicit and stored class-string overrides.
#[test]
fn test_spl_file_info_factory_class_overrides() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class MyInfo extends SplFileInfo {}
class MyFile extends SplFileObject {}

mkdir("docs");
file_put_contents("docs/a.txt", "one\n");

$info = new SplFileInfo("docs/a.txt");
$direct = $info->getFileInfo(MyInfo::class);
var_dump($direct instanceof MyInfo);
var_dump($direct->getFilename());

$info->setInfoClass(MyInfo::class);
$fileInfo = $info->getFileInfo();
$pathInfo = $info->getPathInfo();
var_dump($fileInfo instanceof MyInfo);
var_dump($pathInfo instanceof MyInfo);
var_dump($pathInfo->getPathname());

$info->setFileClass(MyFile::class);
$file = $info->openFile("r");
var_dump($file instanceof MyFile);
echo trim($file->fgets());

unlink("docs/a.txt");
rmdir("docs");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "string(5) \"a.txt\"\n",
            "bool(true)\n",
            "bool(true)\n",
            "string(4) \"docs\"\n",
            "bool(true)\n",
            "one",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies directory, filesystem, and glob iterators over real files.
#[test]
fn test_directory_filesystem_and_glob_iterators() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("tree");
mkdir("tree/sub");
file_put_contents("tree/a.txt", "a");
file_put_contents("tree/b.log", "b");
file_put_contents("tree/sub/c.txt", "c");

$dot = false;
$file = false;
$dir = new DirectoryIterator("tree");
foreach ($dir as $_) {
    if ($dir->isDot()) {
        $dot = true;
    }
    if ($dir->getFilename() === "a.txt" && $dir->isFile()) {
        $file = true;
    }
}
echo $dot ? "dot" : "nodot";
echo "|";
echo $file ? "file" : "nofile";
echo "\n";

$fs = new FilesystemIterator(
    "tree",
    FilesystemIterator::KEY_AS_FILENAME |
    FilesystemIterator::CURRENT_AS_PATHNAME |
    FilesystemIterator::SKIP_DOTS
);
$seenA = false;
$seenS = false;
foreach ($fs as $key => $path) {
    if ($key === "a.txt") {
        $seenA = $path === "tree/a.txt";
    }
    if ($key === "sub") {
        $seenS = $path === "tree/sub";
    }
}
echo $seenA ? "A" : "!";
echo $seenS ? "S" : "!";
echo "\n";

$glob = new GlobIterator(
    "tree/*.txt",
    FilesystemIterator::KEY_AS_FILENAME | FilesystemIterator::CURRENT_AS_PATHNAME
);
echo count($glob);
foreach ($glob as $key => $path) {
    echo "|";
    echo $key;
    echo "=";
    echo $path;
}
echo "\n";

unlink("tree/sub/c.txt");
rmdir("tree/sub");
unlink("tree/a.txt");
unlink("tree/b.log");
rmdir("tree");
"#,
    );
    assert_eq!(out, "dot|file\nAS\n1|a.txt=tree/a.txt\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplFileObject stream methods use byte offsets and preserve file position.
#[test]
fn test_spl_file_object_stream_position_methods() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("stream.txt", "abcdef\nsecond\n");
$file = new SplFileObject("stream.txt", "r+");
echo $file->fread(3);
echo "|";
echo $file->ftell();
$file->fseek(4);
echo "|";
echo $file->fread(2);
$file->fseek(0);
$file->fwrite("XY");
$file->fseek(0);
echo "|";
echo $file->fread(6);
$file->ftruncate(4);
$file->fseek(0);
echo "|";
echo $file->fread(10);
unlink("stream.txt");
"#,
    );
    assert_eq!(out, "abc|3|ef|XYcdef|XYcd");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplTempFileObject uses a writable stream for basic read/write cycles.
#[test]
fn test_spl_temp_file_object_stream_read_write() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = new SplTempFileObject(5);
echo $tmp->getPathname();
echo "|";
$tmp->fwrite("temp\nline\n");
$tmp->rewind();
echo trim($tmp->fgets());
echo "|";
echo trim($tmp->fgets());
echo "|";
$memory = new SplTempFileObject(-1);
echo $memory->getPathname();
"#,
    );
    assert_eq!(out, "php://temp/maxmemory:5|temp|line|php://memory");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplTempFileObject keeps small contents in memory with seek/read/write state.
#[test]
fn test_spl_temp_file_object_memory_buffer_before_spill() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = new SplTempFileObject(10);
echo $tmp->getPathname();
echo "|";
echo $tmp->ftell();
echo "|";
echo $tmp->fwrite("abc");
echo "|";
echo $tmp->ftell();
$tmp->fseek(1);
$tmp->fwrite("Z");
$tmp->rewind();
echo "|";
echo $tmp->fread(3);
$stat = $tmp->fstat();
echo "|";
echo $stat["size"];
echo "|";
echo count($stat);
"#,
    );
    assert_eq!(out, "php://temp/maxmemory:10|0|3|3|aZc|3|26");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies SplTempFileObject spills after maxMemory while preserving stream position.
#[test]
fn test_spl_temp_file_object_spills_after_threshold() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = new SplTempFileObject(3);
$tmp->fwrite("abc");
echo $tmp->ftell();
echo "|";
$tmp->fwrite("d");
echo $tmp->ftell();
$tmp->fseek(1);
$tmp->fwrite("YY");
$tmp->rewind();
echo "|";
echo $tmp->fread(4);
$tmp->ftruncate(2);
$tmp->rewind();
echo "|";
echo $tmp->fread(10);
"#,
    );
    assert_eq!(out, "3|4|aYYd|aY");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies negative maxMemory uses php://memory and never needs spill for large writes.
#[test]
fn test_spl_temp_file_object_negative_memory_uses_memory_stream() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = new SplTempFileObject(-1);
echo $tmp->getPathname();
$tmp->fwrite("first\nsecond\n");
$tmp->rewind();
echo "|";
echo trim($tmp->fgets());
echo "|";
echo trim($tmp->fgets());
echo "|";
echo $tmp->eof() ? "eof" : "more";
"#,
    );
    assert_eq!(out, "php://memory|first|second|eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies DirectoryIterator foreach values stay typed objects for method dispatch.
#[test]
fn test_directory_iterator_foreach_value_supports_direct_methods() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("tree");
file_put_contents("tree/a.txt", "a");

$seen = false;
foreach (new DirectoryIterator("tree") as $entry) {
    if (!$entry->isDot() && $entry->getFilename() === "a.txt" && $entry->isFile()) {
        $seen = true;
    }
}
echo $seen ? "entry" : "missing";

unlink("tree/a.txt");
rmdir("tree");
"#,
    );
    assert_eq!(out, "entry");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies FilesystemIterator foreach values support direct file-info methods in default mode.
#[test]
fn test_filesystem_iterator_foreach_value_supports_direct_methods() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("tree");
file_put_contents("tree/a.txt", "a");

$seen = false;
foreach (new FilesystemIterator("tree") as $entry) {
    if ($entry->getFilename() === "a.txt" && $entry->isFile()) {
        $seen = true;
    }
}
echo $seen ? "entry" : "missing";

unlink("tree/a.txt");
rmdir("tree");
"#,
    );
    assert_eq!(out, "entry");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies RecursiveDirectoryIterator honors FOLLOW_SYMLINKS for child detection.
#[test]
fn test_recursive_directory_iterator_follow_symlinks_flag() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("root");
mkdir("root/child");
file_put_contents("root/child/leaf.txt", "leaf");
symlink("child", "root/linkchild");

$plain = new RecursiveDirectoryIterator(
    "root",
    FilesystemIterator::KEY_AS_FILENAME | FilesystemIterator::SKIP_DOTS
);
$plainLinkHasChildren = false;
foreach ($plain as $key => $entry) {
    if ($key === "linkchild") {
        $plainLinkHasChildren = $plain->hasChildren();
    }
}

$follow = new RecursiveDirectoryIterator(
    "root",
    FilesystemIterator::KEY_AS_FILENAME |
    FilesystemIterator::SKIP_DOTS |
    RecursiveDirectoryIterator::FOLLOW_SYMLINKS
);
$followLinkHasChildren = false;
foreach ($follow as $key => $entry) {
    if ($key === "linkchild") {
        $followLinkHasChildren = $follow->hasChildren();
    }
}

echo $plainLinkHasChildren ? "plain" : "plain-no";
echo "|";
echo $followLinkHasChildren ? "follow" : "follow-no";

unlink("root/linkchild");
unlink("root/child/leaf.txt");
rmdir("root/child");
rmdir("root");
"#,
    );
    assert_eq!(out, "plain-no|follow");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies RecursiveDirectoryIterator and RecursiveCachingIterator child wrapping.
#[test]
fn test_recursive_directory_and_recursive_caching_iterators() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("root");
mkdir("root/child");
file_put_contents("root/child/leaf.txt", "leaf");
file_put_contents("root/top.txt", "top");

$it = new RecursiveDirectoryIterator(
    "root",
    FilesystemIterator::KEY_AS_FILENAME |
    FilesystemIterator::CURRENT_AS_PATHNAME |
    FilesystemIterator::SKIP_DOTS
);
foreach ($it as $key => $path) {
    if ($key === "child" && $it->hasChildren()) {
        echo "child:";
        $child = $it->getChildren();
        echo $child instanceof RecursiveDirectoryIterator ? "wrapped" : "missing";
        $child->rewind();
        echo ":";
        echo $child->key();
        echo "=";
        echo $child->current();
    }
}
echo "\n";

$cache = new RecursiveCachingIterator(new RecursiveArrayIterator(["keep" => ["leaf" => 7]]));
$cache->rewind();
echo $cache->hasChildren() ? "has" : "none";
$wrapped = $cache->getChildren();
$wrapped->rewind();
echo "|";
echo $wrapped->key();
echo "=";
echo $wrapped->current();
echo "\n";

unlink("root/child/leaf.txt");
rmdir("root/child");
unlink("root/top.txt");
rmdir("root");
"#,
    );
    assert_eq!(out, "child:wrapped:leaf.txt=root/child/leaf.txt\nhas|leaf=7\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `SplFileObject::getCsvControl()` returns the controls instead of faulting.
///
/// The method was declared on the class and left out of `is_supported_builtin_spl_method()`,
/// the list that decides which prelude bodies are LOWERED. A declared-but-unlowered method
/// keeps a null vtable slot, so calling it branched to address 0 — a segfault at the call site
/// with nothing wrong at compile time. Removing the name from that list reproduces it exactly.
#[test]
fn test_spl_file_object_get_csv_control_is_lowered() {
    let (out, dir) = compile_and_run_in_dir(
        r##"<?php
file_put_contents("ctl2.csv", "a,b\n");
$f = new SplFileObject("ctl2.csv", "r");
echo json_encode($f->getCsvControl()), "|";
$f->setCsvControl(";", "'", "#");
echo json_encode($f->getCsvControl()), "\n";
unset($f);
unlink("ctl2.csv");
"##,
    );
    assert_eq!(out, "[\",\",\"\\\"\",\"\\\\\"]|[\";\",\"'\",\"#\"]\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies READ_CSV iteration reads CSV RECORDS rather than exploding the raw line.
///
/// `current()` used to answer `explode($delimiter, $line)`, which is not CSV: an enclosure was
/// ordinary text, so `a,"b,c",d` came back as `["a", "\"b", "c\"", "d\n"]` — four fields, quotes
/// attached, the terminator glued to the last one. A quoted field holding a newline was cut in
/// half across two iterations, and a blank line answered `["\n"]` where php answers `[null]`.
/// Every expectation below is `php -n` 8.5.6 on the same file, including the final `[null]`
/// php yields because it reads until a read fails rather than until the lines run out.
#[test]
fn test_spl_file_object_read_csv_parses_records_not_exploded_lines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rec.csv", "a,\"b,c\",d\n\n\"x\ny\",z\n");
$f = new SplFileObject("rec.csv");
$f->setFlags(SplFileObject::READ_CSV);
foreach ($f as $i => $row) {
    echo $i, "=", json_encode($row), ";";
}
echo "\n";
unset($f);
unlink("rec.csv");
"#,
    );
    assert_eq!(
        out,
        "0=[\"a\",\"b,c\",\"d\"];1=[null];2=[\"x\\ny\",\"z\"];3=[null];\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies READ_CSV honors the flags it is combined with, as php does.
///
/// SKIP_EMPTY turns the end-of-input record into `false` instead of `[null]`, and — only when
/// DROP_NEW_LINE is set too, which is php's own rule — steps OVER a blank record without
/// renumbering the ones after it: the keys run 0, 2, 3, not 0, 1, 2. A record spanning three
/// physical lines counts as ONE key, so the key is a record index and not a line index.
#[test]
fn test_spl_file_object_read_csv_flag_combinations() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function walk(string $content, int $flags): void {
    file_put_contents("flags.csv", $content);
    $f = new SplFileObject("flags.csv");
    $f->setFlags($flags);
    foreach ($f as $i => $row) {
        echo $i, "=", json_encode($row), ";";
    }
    echo "\n";
    unset($f);
    unlink("flags.csv");
}
$c = "a,\"b,c\",d\n\n\"x\ny\",z\n";
walk($c, SplFileObject::READ_CSV | SplFileObject::SKIP_EMPTY);
walk($c, SplFileObject::READ_CSV | SplFileObject::SKIP_EMPTY | SplFileObject::DROP_NEW_LINE);
walk("\"a\nb\nc\",z\nq,r\n", SplFileObject::READ_CSV);
walk("a,b\nc,d", SplFileObject::READ_CSV);
walk("", SplFileObject::READ_CSV);
"#,
    );
    assert_eq!(
        out,
        "0=[\"a\",\"b,c\",\"d\"];1=[null];2=[\"x\\ny\",\"z\"];3=false;\n\
         0=[\"a\",\"b,c\",\"d\"];2=[\"x\\ny\",\"z\"];3=false;\n\
         0=[\"a\\nb\\nc\",\"z\"];1=[\"q\",\"r\"];2=[null];\n\
         0=[\"a\",\"b\"];1=[\"c\",\"d\"];\n\
         0=[null];\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `SplFileObject::fputcsv()` forwards its `$eol` instead of dropping it.
///
/// The method declared the parameter and then called `fputcsv()` with five arguments, so the
/// sixth never left the prelude: every row ended in `"\n"` whatever the caller asked for, and
/// the return count reported the newline it did not write. Measured on `php -n` 8.5.6, the
/// three rows below leave `a,b\nc,de,f|EOL|` and answer 4, 3, 8.
#[test]
fn test_spl_file_object_fputcsv_forwards_its_eol() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$w = new SplFileObject("eol.csv", "w");
echo $w->fputcsv(["a", "b"]), "|";
echo $w->fputcsv(["c", "d"], ",", "\"", "\\", ""), "|";
echo $w->fputcsv(["e", "f"], ",", "\"", "\\", "|EOL|"), "\n";
unset($w);
echo bin2hex(file_get_contents("eol.csv")), "\n";
unlink("eol.csv");
"#,
    );
    assert_eq!(out, "4|3|8\n612c620a632c64652c667c454f4c7c\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an omitted CSV control falls back on `setCsvControl()` state, not on a literal.
///
/// php resolves `$separator`, `$enclosure` and `$escape` against the object when the call
/// leaves them out — that is what `setCsvControl()` is for, and what the 8.4 deprecation text
/// points at. elephc spelled `","` as the parameter default, so the state was ignored and
/// `$f->setCsvControl(";"); $f->fgetcsv()` came back as one field.
#[test]
fn test_spl_file_object_csv_controls_fall_back_on_set_csv_control() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ctl.csv", "a;b;c\n");
$f = new SplFileObject("ctl.csv", "r");
$f->setCsvControl(";", "\"", "\\");
echo json_encode($f->fgetcsv()), "|";
$g = new SplFileObject("ctl.csv", "r");
echo json_encode($g->fgetcsv(";", "\"", "\\")), "|";
$h = new SplFileObject("ctl.csv", "r");
echo json_encode($h->fgetcsv(",", "\"", "\\")), "\n";
unlink("ctl.csv");
"#,
    );
    assert_eq!(out, "[\"a\",\"b\",\"c\"]|[\"a\",\"b\",\"c\"]|[\"a;b;c\"]\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `SplFileObject::fscanf()` scans one line through php's scanf engine.
///
/// The method did not exist: `$f->fscanf('%d %s')` was `Undefined method`. It reads a line the
/// way `fgets()` does and hands it to the shared engine, so a `%d` field comes back as an INT.
///
/// The LINE NUMBER rule is php's own and is NOT `fgets()`'s: measured on `php -n` 8.5.6, the
/// FIRST `fscanf()` of a fresh object leaves `key()` where it was and only later reads advance
/// it, so on a three-line file the keys run 0, 1, 2 where `fgets()` gives 1, 2, 3. Mixing the
/// two shows it is the first READ that is special rather than the method.
#[test]
fn test_spl_file_object_fscanf_scans_one_line_per_call() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("scan.txt", "1 a\n2 b\n3 c\n");
$f = new SplFileObject("scan.txt", "r");
echo json_encode($f->fscanf("%d %s")), " key=", $f->key(), "\n";
echo json_encode($f->fscanf("%d %s")), " key=", $f->key(), "\n";
echo json_encode($f->fscanf("%d %s")), " key=", $f->key(), "\n";
echo json_encode($f->fscanf("%d %s")), " key=", $f->key(), "\n";
$g = new SplFileObject("scan.txt", "r");
$g->fgets();
echo "after fgets key=", $g->key(), "\n";
$g->fscanf("%d %s");
echo "after fscanf key=", $g->key(), "\n";
unlink("scan.txt");
"#,
    );
    assert_eq!(
        out,
        "[1,\"a\"] key=0\n[2,\"b\"] key=1\n[3,\"c\"] key=2\nnull key=3\n\
         after fgets key=1\nafter fscanf key=2\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a file object at end of file ANSWERS an empty line once, then refuses.
///
/// `SplFileObject::fgets()` never returns `false` in php: measured on `php -n` 8.5.6 over
/// `"a\nbb\n"`, it answers `'a\n'`, `'bb\n'`, then `''` — `key()` advancing each time, `eof()`
/// true after the empty one — and only the call AFTER that throws
/// `RuntimeException: Cannot read from file <path>`. elephc answered `false` for ever and
/// stopped counting, so a `!== false` loop terminated where php's throws.
///
/// The empty-line step exists because `feof()` only becomes true once a read has hit the end,
/// which is also why the guard fires on the following call rather than this one.
#[test]
fn test_spl_file_object_read_past_eof_answers_empty_then_throws() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("eof.txt", "a\nbb\n");
$f = new SplFileObject("eof.txt", "r");
for ($i = 0; $i < 4; $i++) {
    try {
        $line = $f->fgets();
    } catch (\RuntimeException $e) {
        echo $i, ": THROW ", $e->getMessage(), "\n";
        continue;
    }
    echo $i, ": ", json_encode($line), " key=", $f->key(), " eof=", json_encode($f->eof()), "\n";
}
unlink("eof.txt");
"#,
    );
    assert_eq!(
        out,
        "0: \"a\\n\" key=1 eof=false\n\
         1: \"bb\\n\" key=2 eof=false\n\
         2: \"\" key=3 eof=true\n\
         3: THROW Cannot read from file eof.txt\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `getCurrentLine()` is php's ALIAS of `fgets()` and therefore CONSUMES the line.
///
/// elephc answered the cached current line and left the stream where it was, so
/// `getCurrentLine()` followed by `fgetc()` read the FIRST line's bytes twice. Measured on
/// `php -n` 8.5.6 over `"aa\nbb\n"`: the line comes back, `key()` advances, the next read
/// starts at the second line, and once `feof()` holds the call throws like `fgets()` does.
#[test]
fn test_spl_file_object_get_current_line_is_an_alias_of_fgets() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("cur.txt", "aa\nbb\n");
$f = new SplFileObject("cur.txt", "r");
echo json_encode($f->getCurrentLine()), " key=", $f->key(), "\n";
echo json_encode($f->fgetc()), $f->fgetc(), "\n";
$g = new SplFileObject("cur.txt", "r");
$g->getCurrentLine();
$g->getCurrentLine();
echo json_encode($g->getCurrentLine()), " key=", $g->key(), "\n";
try {
    $g->getCurrentLine();
} catch (\RuntimeException $e) {
    echo "THROW ", $e->getMessage(), "\n";
}
unlink("cur.txt");
"#,
    );
    assert_eq!(
        out,
        "\"aa\\n\" key=1\n\"b\"b\n\"\" key=3\nTHROW Cannot read from file cur.txt\n"
    );
    let _ = fs::remove_dir_all(&dir);
}
