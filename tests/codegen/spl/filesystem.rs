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
