//! Purpose:
//! Checks native bucket-brigade ownership across successful and throwing PHP callbacks.
//!
//! Called from:
//! - The focused fileinfo ownership gates and the runtime GC integration suite.
//!
//! Key details:
//! - Raw and boxed callback parameters use their declared ABI on every executable target.
//! - Expanded output exceeds the former shared 64 KiB buffer and remains independently owned.

use crate::support::*;

/// Repeated conditions and updates retire first-assigned boxed and raw string owners.
#[test]
fn test_fileinfo_loop_condition_assignment_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function nextFileinfoLoopOwner(int &$remaining): mixed {
    if ($remaining-- <= 0) { return null; }
    return str_repeat("x", 32);
}
$seen = 0; $remaining = 5;
while ($whileOwner = nextFileinfoLoopOwner($remaining)) { $seen++; }
$remaining = 5;
do { $seen++; } while ($doOwner = nextFileinfoLoopOwner($remaining));
for ($remaining = 5; $forOwner = nextFileinfoLoopOwner($remaining);) { $seen++; }
for ($i = 0; $i < 5; $updateOwner = str_repeat("u", 32)) { $i++; $seen++; }
echo $seen, "\n";
unset($whileOwner, $doOwner, $forOwner, $updateOwner);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "21\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native brigade graphs retire on return and throw, including retained property getters.
#[test]
fn test_fileinfo_user_filter_brigade_owners() {
    let source = r#"<?php
class FileinfoRawBrigade extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            if (!is_bool($closing)) { echo "wrong closing flag\n"; }
            $consumed += $bucket->datalen;
            if ($consumed !== 4) { echo "bad consumed value\n"; }
            $bucket->data = str_repeat("x", 70000);
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class FileinfoBoxedBrigade extends php_user_filter {
    public function filter(mixed $in, mixed $out, mixed &$consumed, mixed $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            if (!is_bool($closing)) { echo "wrong boxed closing flag\n"; }
            $consumed += $bucket->datalen;
            if ($consumed !== 4) { echo "bad consumed value\n"; }
            $bucket->data = str_repeat("y", 70001);
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class FileinfoThrowingBrigade extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        $hadData = false;
        while ($bucket = stream_bucket_make_writeable($in)) {
            $hadData = true;
            $consumed += $bucket->datalen;
            if ($consumed !== 4) { echo "bad consumed value\n"; }
            stream_bucket_append($out, $bucket);
        }
        if ($hadData) { throw new Error("filter error"); }
        return PSFS_PASS_ON;
    }
}
class FileinfoPassBrigade extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("fileinfo.brigade.raw", "FileinfoRawBrigade");
stream_filter_register("fileinfo.brigade.boxed", "FileinfoBoxedBrigade");
stream_filter_register("fileinfo.brigade.throw", "FileinfoThrowingBrigade");
$stream = fopen("php://temp", "w+b");
echo fwrite($stream, str_repeat("s", $argc + 3)), "\n";
$bucket = stream_bucket_new($stream, str_repeat("new", 7));
echo $bucket->data, ":", $bucket->datalen, "\n";
unset($bucket); fclose($stream);
for ($i = 0; $i < 4; $i++) {
    foreach (["raw", "boxed"] as $name) {
        $stream = fopen("php://temp", "w+b"); fwrite($stream, "seed"); rewind($stream);
        $filter = stream_filter_append($stream, "fileinfo.brigade." . $name, STREAM_FILTER_READ);
        $data = $name === "raw" ? stream_get_contents($stream) : stream_get_contents($stream, 100000);
        echo strlen($data), ":", substr($data, 0, 1), "\n";
        fclose($stream); unset($data, $filter);
    }
    $stream = fopen("php://temp", "w+b"); fwrite($stream, "seed"); rewind($stream);
    $filter = stream_filter_append($stream, "fileinfo.brigade.throw", STREAM_FILTER_READ);
    try { stream_get_contents($stream, 100000); } catch (Error $e) { echo $e->getMessage(), "\n"; }
    fclose($stream); unset($filter);
}
unset($stream, $e, $name);
"#;
    let (out, _) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("4\n{}:21\n{}", "new".repeat(7), "70000:x\n70001:y\nfilter error\n".repeat(4)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Write filters retire expanded output buffers for both raw and boxed brigade signatures.
#[test]
fn test_fileinfo_write_filter_brigade_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class FileinfoWriteRawBrigade extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = str_repeat("x", 70000);
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class FileinfoWriteBoxedBrigade extends php_user_filter {
    public function filter(mixed $in, mixed $out, mixed &$consumed, mixed $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = str_repeat("y", 70001);
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("fileinfo.write.raw", "FileinfoWriteRawBrigade");
stream_filter_register("fileinfo.write.boxed", "FileinfoWriteBoxedBrigade");
foreach (["raw", "boxed"] as $name) {
    $path = tempnam(sys_get_temp_dir(), "write-filter");
    $stream = fopen($path, "w+b");
    $filter = stream_filter_append($stream, "fileinfo.write." . $name, STREAM_FILTER_WRITE);
    for ($i = 0; $i < 3; $i++) { fwrite($stream, "seed"); }
    fclose($stream);
    $data = file_get_contents($path);
    echo strlen($data), ":", substr($data, 0, 1), "\n";
    unlink($path);
    unset($path, $stream, $filter, $data);
}
unset($name);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "210000:x\n210003:y\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct reads retire raw and boxed filter output after storage, output, discard, and EOF.
#[test]
fn test_fileinfo_direct_read_filter_brigade_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class FileinfoReadRawBrigade extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            if ($this->params === "double") { $bucket->data .= $bucket->data; }
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class FileinfoReadBoxedBrigade extends php_user_filter {
    public function filter(mixed $in, mixed $out, mixed &$consumed, mixed $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            if ($this->params === "double") { $bucket->data .= $bucket->data; }
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("fileinfo.read.raw", "FileinfoReadRawBrigade");
stream_filter_register("fileinfo.read.boxed", "FileinfoReadBoxedBrigade");
for ($i = 0; $i < 3; $i++) {
    foreach (["raw", "boxed"] as $name) {
        foreach (["stored", "echo", "discarded", "characters"] as $mode) {
            $stream = fopen("php://temp", "w+b"); fwrite($stream, "seed"); rewind($stream);
            $filter = stream_filter_append($stream, "fileinfo.read." . $name, STREAM_FILTER_READ, $mode === "characters" ? "pass" : "double");
            if ($mode === "stored") { $data = fread($stream, 8); echo $data, "\n"; unset($data); }
            if ($mode === "echo") { echo fread($stream, 8), "\n"; }
            if ($mode === "discarded") { fread($stream, 8); echo "discarded\n"; }
            if ($mode === "characters") {
                $data = "";
                while (($character = fgetc($stream)) !== false) { $data .= $character; }
                echo $data, "\n"; unset($data, $character);
            }
            fclose($stream); unset($stream, $filter);
        }
    }
}
unset($name, $mode);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "SEEDSEED\nSEEDSEED\ndiscarded\nSEED\n".repeat(6), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unoptimized EIR also consumes independently owned direct read results exactly once.
#[test]
fn test_fileinfo_direct_read_filter_brigade_owners_without_ir_opt() {
    crate::support::without_ir_opt(test_fileinfo_direct_read_filter_brigade_owners);
}
