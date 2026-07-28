//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O filesystem, including mkdir rmdir, copy unlink, and rename file.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies mkdir, rmdir, and is_dir by creating a directory, confirming it
/// exists, removing it, and confirming it no longer exists.
#[test]
fn test_fread_inside_user_function_does_not_overwrite_other_locals() {
    // Regression for a frame-layout bug: when fread() was used inside a user
    // function and its result was assigned to a local variable, the codegen
    // inference fell back to PhpType::Mixed (8-byte slot) instead of Str
    // (16-byte). The store path still wrote the string as a 16-byte (ptr+len)
    // pair, so the second 8 bytes clobbered the adjacent local — typically
    // the just-opened $f resource — and the next fclose($f) crashed because
    // it tried to mixed-unbox an integer length.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("readfn.txt", "elephc");
function read_back() {
    $f = fopen("readfn.txt", "r");
    $r = fread($f, 64);
    fclose($f);
    return $r;
}
echo read_back();
unlink("readfn.txt");
"#,
    );
    assert_eq!(out, "elephc");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for mkdir rmdir.
#[test]
fn test_mkdir_rmdir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("testdir");
if (is_dir("testdir")) { echo "made"; }
rmdir("testdir");
if (!is_dir("testdir")) { echo "gone"; }
"#,
    );
    assert_eq!(out, "madegone");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies copy, unlink, and file existence by creating a file, copying it,
/// reading through the copy, deleting both files, and confirming removal.
#[test]
fn test_copy_unlink() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "content");
copy("orig.txt", "dup.txt");
echo file_get_contents("dup.txt");
unlink("dup.txt");
if (!file_exists("dup.txt")) { echo "|gone"; }
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "content|gone");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies rename by creating a file, renaming it, confirming the new name
/// holds the data, confirming the old name is gone, and cleaning up.
#[test]
fn test_rename_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("old.txt", "data");
rename("old.txt", "new.txt");
echo file_get_contents("new.txt");
if (!file_exists("old.txt")) { echo "|moved"; }
unlink("new.txt");
"#,
    );
    assert_eq!(out, "data|moved");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies getcwd returns a non-empty string (platform-independent check).
#[test]
fn test_getcwd() {
    let out = compile_and_run(
        r#"<?php
$cwd = getcwd();
if (strlen($cwd) > 0) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies sys_get_temp_dir names a directory that exists.
///
/// The path is not necessarily spelled "tmp" or "temp": php resolves TMPDIR, and
/// macOS points it at a per-user `/var/folders/<hash>/T` directory whose name
/// contains neither. That spelling only held while the lowering hardcoded `/tmp`.
/// On Windows the runtime returns the `GetTempPathW` result, which the host cannot
/// stat when the binary runs under Wine, so the name check is kept there.
#[test]
fn test_sys_get_temp_dir() {
    let out = compile_and_run(
        r#"<?php
$tmp = sys_get_temp_dir();
echo $tmp;
"#,
    );
    assert!(!out.is_empty(), "sys_get_temp_dir returned an empty path");
    if cfg!(windows) {
        assert!(
            out.to_lowercase().contains("temp"),
            "sys_get_temp_dir returned {:?}",
            out
        );
    } else {
        assert!(
            std::path::Path::new(&out).is_dir(),
            "sys_get_temp_dir returned {:?}, which is not a directory",
            out
        );
    }
}

/// Verifies chdir changes the working directory and getcwd reflects the new
/// path, confirming the change by checking path length increased after chdir.
#[test]
fn test_chdir_getcwd() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("subdir");
$before = getcwd();
chdir("subdir");
$after = getcwd();
if (strlen($after) > strlen($before)) { echo "changed"; }
chdir("..");
rmdir("subdir");
"#,
    );
    assert_eq!(out, "changed");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies scandir by creating two files, confirming all four entries (. .. a.txt b.txt)
/// appear in the result, and cleaning up the directory.
#[test]
fn test_scandir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("sd");
file_put_contents("sd/a.txt", "a");
file_put_contents("sd/b.txt", "b");
$files = scandir("sd");
if (
    count($files) == 4 &&
    in_array(".", $files) &&
    in_array("..", $files) &&
    in_array("a.txt", $files) &&
    in_array("b.txt", $files)
) {
    echo "ok";
}
unlink("sd/a.txt");
unlink("sd/b.txt");
rmdir("sd");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies glob by creating two files matching a pattern, confirming both
/// are returned with their full paths, and cleaning up.
#[test]
fn test_glob_fn() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gd");
file_put_contents("gd/g1.txt", "a");
file_put_contents("gd/g2.txt", "b");
$matches = glob("gd/*.txt");
if (
    count($matches) == 2 &&
    in_array("gd/g1.txt", $matches) &&
    in_array("gd/g2.txt", $matches)
) {
    echo "ok";
}
unlink("gd/g1.txt");
unlink("gd/g2.txt");
rmdir("gd");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies tempnam creates a unique file in the given directory and that it
/// exists immediately, then cleans up the temporary file.
#[test]
fn test_glob_stream_wrapper_iterates_matches() {
    // Phase 6: opendir("glob://pattern") returns a synthetic directory
    // resource backed by libc glob; readdir iterates the matches, closedir
    // releases the gl_pathv, rewinddir restarts the iteration. libc glob
    // returns the matches in sorted order on every target we support.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gw");
file_put_contents("gw/a.txt", "1");
file_put_contents("gw/b.txt", "2");
$h = opendir("glob://gw/*.txt");
$first = readdir($h);
$second = readdir($h);
$end = readdir($h);
rewinddir($h);
$first_again = readdir($h);
closedir($h);
echo $first . "|" . $second . "|" . ($end === false ? "end" : "x") . "|" . $first_again;
unlink("gw/a.txt");
unlink("gw/b.txt");
rmdir("gw");
"#,
    );
    assert_eq!(out, "gw/a.txt|gw/b.txt|end|gw/a.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for tempnam.
#[test]
fn test_tempnam() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = tempnam(".", "test");
if (file_exists($tmp)) { echo "ok"; }
unlink($tmp);
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `tempnam` keeps only the basename of a long prefix and applies PHP's 63-byte limit.
#[test]
fn test_tempnam_normalizes_and_limits_prefix() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = tempnam(".", "ignored/path/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
if ($tmp === false) {
    echo "fail";
} else {
    echo strlen(basename($tmp));
    unlink($tmp);
}
"#,
    );
    assert_eq!(out, "69");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies Windows retries `tempnam()` in the system directory after an explicit-dir failure.
#[test]
fn test_tempnam_windows_falls_back_to_system_directory() {
    if target().platform != Platform::Windows {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$tmp = @tempnam("Z:\\elephc\\missing\\directory", "fallback");
echo is_string($tmp) && file_exists($tmp) ? "ok" : "fail";
if (is_string($tmp)) {
    unlink($tmp);
}
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for disk space positive and ordered.
#[test]
fn test_disk_space_positive_and_ordered() {
    let out = compile_and_run(
        r#"<?php
$free = disk_free_space("/");
$total = disk_total_space("/");
echo $free > 0 ? "f" : "F";
echo $total > 0 ? "t" : "T";
echo $total >= $free ? "o" : "O";
"#,
    );
    assert_eq!(out, "fto");
}

/// Verifies compiled PHP output for disk free space invalid path returns zero.
#[test]
fn test_disk_free_space_invalid_path_returns_zero() {
    let out = compile_and_run(r#"<?php var_dump(disk_free_space("/no/such/path/xyz123"));"#);
    assert_eq!(out, "float(0)\n");
}

/// Verifies `sys_get_temp_dir()` reads TMPDIR and drops one trailing slash.
#[test]
fn test_sys_get_temp_dir_reads_tmpdir() {
    // php_get_temporary_directory reads TMPDIR before any fallback and strips
    // exactly one trailing slash, so "/var/tmp/elephc-probe/" yields
    // "/var/tmp/elephc-probe" and a bare "/" yields the empty string
    // (main/php_open_temporary_file.c). The lowering returned a hardcoded "/tmp",
    // which is wrong on every host that sets TMPDIR -- macOS always does.
    // Each case is its own program because php resolves the directory once and
    // caches it, so a later putenv() in the same run would not be observable.
    //
    // TMPDIR is the POSIX half of that function. Windows takes the `#ifdef PHP_WIN32`
    // branch instead, which asks GetTempPath -- TMP, then TEMP, then USERPROFILE,
    // then the Windows directory -- and strips the trailing separator the API always
    // appends. TMPDIR has no meaning there, so the assertion is the part of the
    // contract that does hold: an absolute path that does not end in a separator.
    if target().platform == Platform::Windows {
        let resolved = compile_and_run(
            r#"<?php
$dir = sys_get_temp_dir();
echo (strlen($dir) > 3 && $dir[1] === ":" && substr($dir, -1) !== "\\" && is_dir($dir))
    ? "absolute-unsuffixed" : "[" . $dir . "]";
"#,
        );
        assert_eq!(resolved, "absolute-unsuffixed");
        return;
    }

    let plain = compile_and_run(
        r#"<?php
putenv("TMPDIR=/var/tmp/elephc-probe");
echo sys_get_temp_dir();
"#,
    );
    assert_eq!(plain, "/var/tmp/elephc-probe");

    let trailing = compile_and_run(
        r#"<?php
putenv("TMPDIR=/var/tmp/elephc-probe/");
echo sys_get_temp_dir();
"#,
    );
    assert_eq!(trailing, "/var/tmp/elephc-probe");

    // Only one slash comes off: "///" keeps the other two.
    let many = compile_and_run(
        r#"<?php
putenv("TMPDIR=/var/tmp/elephc-probe///");
echo sys_get_temp_dir();
"#,
    );
    assert_eq!(many, "/var/tmp/elephc-probe//");

    // A bare "/" strips to nothing rather than falling back.
    let root = compile_and_run(
        r#"<?php
putenv("TMPDIR=/");
echo "[" . sys_get_temp_dir() . "]";
"#,
    );
    assert_eq!(root, "[]");
}

/// Verifies `tmpfile()` creates its file in php's temporary directory.
#[test]
fn test_tmpfile_resolves_its_directory_like_php() {
    // php opens the file through php_open_temporary_file, which resolves the
    // directory with php_get_temporary_directory -- TMPDIR first. elephc copied a
    // fixed "/tmp/elephc-XXXXXX" template, so it succeeded against /tmp where php
    // reports false because the configured directory does not exist, and it wrote
    // outside any directory the host had confined the process to.
    // One putenv per program: php resolves the directory once and caches it, so a
    // second call in the same run would not see a later putenv.
    //
    // Windows resolves the directory through GetTempPath, which reads the process
    // environment block rather than TMPDIR, so neither case below can be staged
    // there: elephc's putenv writes the CRT environment and GetTempPath would not
    // observe it. What is checkable is that the handle tmpfile() returns is backed
    // by a real file in the resolved directory.
    if target().platform == Platform::Windows {
        let roundtrip = compile_and_run(
            r#"<?php
$f = tmpfile();
fwrite($f, "hello");
rewind($f);
echo fread($f, 5);
"#,
        );
        assert_eq!(roundtrip, "hello");
        return;
    }

    let unreachable = compile_and_run(
        r#"<?php
putenv("TMPDIR=/nonexistent-elephc-tmpfile");
$f = tmpfile();
echo ($f === false) ? "false" : "resource";
"#,
    );
    assert_eq!(unreachable, "false");

    let reachable = compile_and_run(
        r#"<?php
putenv("TMPDIR=/var/tmp");
$f = tmpfile();
fwrite($f, "hello");
rewind($f);
echo fread($f, 5);
"#,
    );
    assert_eq!(reachable, "hello");
}

/// Verifies `tempnam()` falls back when the requested directory cannot hold the file.
#[test]
fn test_tempnam_falls_back_to_the_system_temporary_directory() {
    // php retries in the system temporary directory and emits a notice when the
    // requested directory is unusable. elephc returned a path inside the unusable
    // directory for a file it never created, so file_exists() on the result was
    // false and the next fopen() failed. The AArch64 arm could not even see the
    // failure: it compared mkstemp's C int result as a 64-bit value, where -1 reads
    // as 0xffffffff and looks like a valid descriptor.
    let absent = compile_and_run(
        r#"<?php
$path = @tempnam("/nonexistent-elephc-tempnam", "pfx");
echo ($path === false) ? "false" : (file_exists($path) ? "exists" : "missing");
echo "|";
echo (substr($path, 0, 12) === "/nonexistent") ? "requested" : "fallback";
@unlink($path);
"#,
    );
    assert_eq!(absent, "exists|fallback");

    // A usable directory is still honoured rather than redirected.
    let usable = compile_and_run(
        r#"<?php
$dir = sys_get_temp_dir();
$path = tempnam($dir, "pfx");
echo file_exists($path) ? "exists" : "missing";
echo "|";
echo (dirname($path) === $dir) ? "requested" : "fallback";
unlink($path);
"#,
    );
    assert_eq!(usable, "exists|requested");
}

/// Verifies `filemtime()` reports false rather than stack contents for a missing path.
#[test]
fn test_filemtime_reports_false_for_an_unstattable_path() {
    // The AArch64 helper read st_mtime out of the stat buffer without checking
    // whether stat() had written it, so a missing path returned whatever the stack
    // held -- a value that changed with the path. The x86_64 helper checked but
    // reported 0. php returns false, and the builtin declared Int, which discarded
    // the false the boxing layer produced.
    let out = compile_and_run(
        r#"<?php
$missing = @filemtime("/nonexistent-elephc-filemtime");
echo ($missing === false) ? "false" : "int:" . var_export($missing, true);
echo "|";
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-filemtime-probe.txt";
file_put_contents($path, "x");
echo is_int(filemtime($path)) && filemtime($path) > 1000000000 ? "stamped" : "bad";
unlink($path);
"#,
    );
    assert_eq!(out, "false|stamped");
}

/// Verifies a read larger than the concat arena does not destroy its surroundings.
#[test]
fn test_fread_beyond_the_concat_arena_keeps_its_surroundings() {
    // The arena is a flat 64 KiB buffer with an offset and no capacity of its own, so
    // a request larger than what is left of it was written straight past the end.
    // Nothing reported it: the returned bytes were correct -- read back from the same
    // overflowed region -- while whatever followed the buffer was destroyed. Here the
    // casualty is the stream itself, which fclose() then rejected as "unknown given".
    //
    // The victim depends on the program's layout, which is what made this look
    // intermittent: an earlier call that moved the arena offset moved the damage
    // somewhere harmless. So the fixture pins both a neighbour and the handle.
    //
    // The payload is written in chunks rather than with str_repeat(200000), because
    // building a string that size would route it through the very arena under test.
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-fread-arena-probe.bin";
$sink = fopen($path, "w");
$chunk = str_repeat("D", 4096);
for ($i = 0; $i < 50; $i++) { fwrite($sink, $chunk); }
fclose($sink);

$canary = "intact";
$source = fopen($path, "r");
$data = fread($source, 204800);
$closed = fclose($source);
unlink($path);

echo strlen($data), "|", $canary, "|", ($closed ? "closed" : "close-false");
"#,
    );
    assert_eq!(out, "204800|intact|closed");
}

/// Verifies a length far beyond the file yields only the bytes that exist.
#[test]
fn test_fread_with_a_length_beyond_the_file_returns_only_what_exists() {
    // fread() reads *up to* the requested length, so asking for more than the file
    // holds is ordinary PHP -- `fread($f, 100000000)` on a 13-byte file is 13 bytes,
    // not an error. Sending the oversized case to its own heap block therefore may
    // not size that block from the request: elephc's heap is 8 MiB by default, so a
    // 100 MB request emptied it in one call and the program died with
    // "heap memory exhausted" on a read PHP answers in full.
    //
    // The companion test above reads a file whose size matches the request exactly,
    // which is why it never saw this. Both sizes here exceed the arena, so both take
    // the oversized path: the first has almost nothing to give, the second has more
    // than the arena holds and must still come back whole.
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-fread-huge-probe.bin";

$sink = fopen($path, "w");
fwrite($sink, "hello, elephc");
fclose($sink);
$canary = "intact";
$source = fopen($path, "r");
$small = fread($source, 100000000);
$closed_small = fclose($source);

$sink = fopen($path, "w");
$chunk = str_repeat("F", 1000);
for ($i = 0; $i < 100; $i++) { fwrite($sink, $chunk); }
fclose($sink);
$source = fopen($path, "r");
$big = fread($source, 100000000);
$closed_big = fclose($source);
unlink($path);

echo strlen($small), "/", $small, "/", ($closed_small ? "closed" : "close-false"), " ";
echo strlen($big), "/", $canary, "/", ($closed_big ? "closed" : "close-false");
"#,
    );
    assert_eq!(out, "13/hello, elephc/closed 100000/intact/closed");
}

/// Verifies an oversized read on a stream that cannot be measured still behaves.
#[test]
fn test_fread_with_an_oversized_length_on_a_socket_returns_what_arrived() {
    // Sizing an oversized read means asking the stream how much it holds, and a socket
    // cannot answer: the seek probe fails on it, exactly as it does on a pipe. That
    // branch is the one the file-based fixtures never reach, and getting it wrong is
    // silent -- clamping to the failed probe's result would truncate every socket read
    // to nothing while still returning a perfectly valid empty string.
    //
    // The filler runs the arena cursor most of the way to its 64 KiB end first, so the
    // clamped read no longer fits in what is left and has to take an owned block. Both
    // the filler and the payload are checked, since the block is what protects them.
    //
    // AF_UNIX socket pairs do not exist on Windows -- php itself has no
    // STREAM_PF_UNIX there -- and elephc hands back a null stream, so the fixture
    // would be testing the absence of the transport rather than the read. The branch
    // still gets its x86_64 coverage from the linux-x86_64 shards.
    if target().platform == Platform::Windows {
        return;
    }

    let out = compile_and_run(
        r#"<?php
$filler = str_repeat("Z", 60000);
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($pair[0], "payload-from-the-socket");
$got = fread($pair[1], 1000000);
$canary = "intact";
echo strlen($filler), "/", strlen($got), "/", $got, "/", $canary;
"#,
    );
    assert_eq!(out, "60000/23/payload-from-the-socket/intact");
}

/// Verifies an oversized read at end of stream reports EOF without consuming the heap.
#[test]
fn test_fread_with_an_oversized_length_at_eof_reports_the_stream_exhausted() {
    // The oversized path runs before the read, so it also runs when there is nothing
    // left to read. Taking a block per attempt there would let a `while (!feof())`
    // loop with a large chunk size exhaust the heap through its final, empty read --
    // the one iteration that exists only to observe EOF.
    //
    // The chunk is deliberately larger than half the 8 MiB default heap, so sizing a
    // block from the request rather than from the stream cannot survive two rounds.
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-fread-eof-probe.bin";
$sink = fopen($path, "w");
$chunk = str_repeat("G", 1000);
for ($i = 0; $i < 70; $i++) { fwrite($sink, $chunk); }
fclose($sink);

$source = fopen($path, "r");
$rounds = 0;
$total = 0;
while (!feof($source) && $rounds < 50) {
    $total += strlen(fread($source, 5000000));
    $rounds++;
}
$closed = fclose($source);
unlink($path);

echo $total, "/", ($rounds < 50 ? "terminated" : "runaway"), "/", ($closed ? "closed" : "close-false");
"#,
    );
    assert_eq!(out, "70000/terminated/closed");
}

/// Verifies reading a whole stream past the concat arena keeps its surroundings.
#[test]
fn test_stream_get_contents_beyond_the_concat_arena_keeps_its_surroundings() {
    // stream_get_contents accumulates through its own copy loop, so bounding fread
    // alone did not cover it: it kept copying 4096-byte chunks past the arena's
    // 64 KiB end once the total outgrew it. Same signature as the fread case -- the
    // returned string was correct, read back from the overflowed region, while the
    // stream itself was destroyed and fclose() rejected it as "unknown given".
    //
    // The sizes straddle the arena: 65000 still fits, 205000 does not, and both must
    // behave. The payload is written in chunks because building a string that size
    // would route it through the very arena under test.
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-sgc-arena-probe.bin";
$report = "";
foreach ([65000, 205000] as $size) {
    $sink = fopen($path, "w");
    $chunk = str_repeat("E", 1000);
    for ($i = 0; $i < $size / 1000; $i++) { fwrite($sink, $chunk); }
    fclose($sink);

    $canary = "intact";
    $source = fopen($path, "r");
    $data = stream_get_contents($source);
    $closed = fclose($source);
    $report .= strlen($data) . ":" . $canary . ":" . ($closed ? "closed" : "close-false") . " ";
}
unlink($path);
echo trim($report);
"#,
    );
    assert_eq!(out, "65000:intact:closed 205000:intact:closed");
}

/// Verifies `filesize()` reports false rather than zero for an unstattable path.
#[test]
fn test_filesize_reports_false_for_an_unstattable_path() {
    // php returns int|false. elephc declared Int, which discarded the false, and the
    // AArch64 helper read st_size out of the stat buffer without checking whether
    // stat() had written it -- the same fault as filemtime. That it usually looked
    // like 0 was luck, not correctness, and `filesize($f) === false` never fired.
    let out = compile_and_run(
        r#"<?php
$missing = @filesize("/nonexistent-elephc-filesize");
echo ($missing === false) ? "false" : "int:" . var_export($missing, true);
echo "|";
$path = sys_get_temp_dir() . DIRECTORY_SEPARATOR . "elephc-filesize-probe.txt";
file_put_contents($path, "hello");
echo filesize($path) === 5 ? "sized" : "bad";
echo "|";
echo is_file($path) ? "file" : "bad";
unlink($path);
"#,
    );
    assert_eq!(out, "false|sized|file");
}
