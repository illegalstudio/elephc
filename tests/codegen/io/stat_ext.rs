//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O filesystem stat builtins, including fileperms known file, fileowner returns uid, and filegroup returns gid.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `fileperms()` extracts the regular file type bits (0x8000) from a known file.
/// Uses a temp directory to create `perms.txt` and asserts the type code equals "regular".
#[test]
fn test_fileperms_known_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("perms.txt", "hi");
$perms = fileperms("perms.txt");
echo ($perms & 0xF000) === 0x8000 ? "regular" : "other";
"#,
    );
    assert_eq!(out, "regular");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fileowner()` returns a non-negative UID for an existing file.
/// Uses a temp directory to create `ownr.txt` and asserts output is "ok".
#[test]
fn test_fileowner_returns_uid() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ownr.txt", "");
$uid = fileowner("ownr.txt");
echo $uid >= 0 ? "ok" : "neg";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filegroup()` returns a non-negative GID for an existing file.
/// Uses a temp directory to create `grp.txt` and asserts output is "ok".
#[test]
fn test_filegroup_returns_gid() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("grp.txt", "");
$gid = filegroup("grp.txt");
echo $gid >= 0 ? "ok" : "neg";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fileinode()` returns a value greater than zero for an existing file.
/// Uses a temp directory to create `ino.txt` and asserts output is "ok".
#[test]
fn test_fileinode_nonzero() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ino.txt", "");
echo fileinode("ino.txt") > 0 ? "ok" : "zero";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fileatime()` returns a timestamp greater than zero for a recently accessed file.
/// Uses a temp directory to create `atime.txt` and asserts output is "ok".
#[test]
fn test_fileatime_nonzero() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("atime.txt", "");
echo fileatime("atime.txt") > 0 ? "ok" : "zero";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filectime()` returns a timestamp greater than zero for a file with metadata changes.
/// Uses a temp directory to create `ctime.txt` and asserts output is "ok".
#[test]
fn test_filectime_nonzero() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ctime.txt", "");
echo filectime("ctime.txt") > 0 ? "ok" : "zero";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filetype()` returns "file" for a regular file.
/// Uses a temp directory to create `ft.txt` and asserts output is "file".
#[test]
fn test_filetype_regular_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ft.txt", "");
echo filetype("ft.txt");
"#,
    );
    assert_eq!(out, "file");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filetype()` returns "dir" for a directory.
/// Uses a temp directory to create then remove `mydir/` and asserts output is "dir".
#[test]
fn test_filetype_directory() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("mydir");
echo filetype("mydir");
rmdir("mydir");
"#,
    );
    assert_eq!(out, "dir");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filetype()` returns the string `"false"` when called on a nonexistent path.
/// Asserts strict `=== false` comparison (not a falsy string) so PHP semantics are preserved.
#[test]
fn test_filetype_missing_is_strict_false() {
    let out = compile_and_run(
        r#"<?php echo filetype("/nonexistent/path/xyz") === false ? "false" : "string";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies the scalar stat getters return strict `false` when the target file does not exist.
///
/// THE MEMBERSHIP OF THIS LIST IS THE POINT. It used to name six — `fileatime`, `filectime`,
/// `fileperms`, `fileowner`, `filegroup`, `fileinode` — and the two it left out, `filemtime`
/// and `filesize`, are exactly the two that stayed broken: `filemtime` answered with whatever
/// the stack held where `st_mtime` would have been, and `filesize` answered `0`. A family test
/// that enumerates its members defines the family for every later reader, so an omission does
/// not read as a gap, it reads as coverage.
///
/// All eight are named here now, `filesize` included. Its lowering goes through a
/// wrapper-dispatch composer shared with `is_file()`, so both arms of that composer had to start
/// carrying an int|false flag beside the payload; `is_file()` reads the payload register only, so
/// the flag is inert for it. Concatenated results must be "acpogimts".
#[test]
fn test_scalar_stat_getters_missing_are_strict_false() {
    let out = compile_and_run(
        r#"<?php
echo fileatime("missing.txt") === false ? "a" : "!";
echo filectime("missing.txt") === false ? "c" : "!";
echo fileperms("missing.txt") === false ? "p" : "!";
echo fileowner("missing.txt") === false ? "o" : "!";
echo filegroup("missing.txt") === false ? "g" : "!";
echo fileinode("missing.txt") === false ? "i" : "!";
echo filemtime("missing.txt") === false ? "m" : "!";
echo filetype("missing.txt") === false ? "t" : "!";
echo filesize("missing.txt") === false ? "s" : "!";
"#,
    );
    assert_eq!(out, "acpogimts");
}

/// Verifies `filesize()` still answers `int(0)` for a file that genuinely holds zero bytes.
///
/// This is the half of the change that can silently go wrong. `filesize()` used to report every
/// failure as `0`, so the fix has to separate "could not be measured" from "measured, and it is
/// zero" — and an empty file is exactly the case where those two answers used to be the same
/// value. A fix that returned `false` here would be no better than the bug.
#[test]
fn test_filesize_of_an_empty_file_is_zero_not_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("empty.txt", "");
$s = filesize("empty.txt");
echo $s === 0 ? "zero" : "!";
echo is_int($s) ? "-int" : "-notint";
echo $s === false ? "-false" : "-notfalse";
"#,
    );
    drop(dir);
    assert_eq!(out, "zero-int-notfalse");
}

/// Verifies a successful `filesize()` still behaves as an integer after the return type widened.
///
/// Declaring `int|false` changes how the value is CARRIED, not just what it can be. Arithmetic,
/// `is_int()`, and string concatenation are the three places a boxed result diverges from a raw
/// one, so a success-side regression would show up here rather than in the failure assertions.
#[test]
fn test_filesize_success_still_behaves_as_an_integer() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seven2.txt", "1234567");
$s = filesize("seven2.txt");
echo $s + 1;
echo ":";
echo is_int($s) ? "int" : "notint";
echo ":";
echo "size=" . $s;
"#,
    );
    drop(dir);
    assert_eq!(out, "8:int:size=7");
}

/// Verifies `is_executable()` returns true for `/bin/sh`, which is executable on every
/// POSIX target the compiler ships for. Regression guard for target-specific path handling.
#[test]
fn test_is_executable_true_for_self() {
    // /bin/sh is executable on every POSIX target we ship for.
    let out = compile_and_run(
        r#"<?php echo is_executable("/bin/sh") ? "y" : "n";"#,
    );
    assert_eq!(out, "y");
}

/// Verifies `is_executable()` returns false for a plain text file with no execute bit.
/// Uses a temp directory to create `plain.txt` and asserts output is "n".
#[test]
fn test_is_executable_false_for_text() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("plain.txt", "data");
echo is_executable("plain.txt") ? "y" : "n";
"#,
    );
    assert_eq!(out, "n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_link()` returns false for a regular file.
/// Uses a temp directory to create `plain.txt` and asserts output is "n".
#[test]
fn test_is_link_false_for_regular_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("plain.txt", "");
echo is_link("plain.txt") ? "y" : "n";
"#,
    );
    assert_eq!(out, "n");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
/// Verifies `filetype()` returns "link" and `is_link()` returns true for a symlink.
/// Uses a temp directory with a `target.txt` file and a `link.txt` symlink pointing to it.
/// Asserts output is "link|y". Platform-restricted to unix targets due to `symlink` usage.
#[test]
fn test_filetype_and_is_link_for_symlink() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let source = r#"<?php
echo filetype("link.txt") . "|";
echo is_link("link.txt") ? "y" : "n";
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    fs::write(dir.join("target.txt"), "payload").unwrap();
    std::os::unix::fs::symlink("target.txt", dir.join("link.txt")).unwrap();

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "link|y");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_writeable()` (PHP alias for `is_writable`) works correctly.
/// Uses a temp directory to create `wr.txt` and asserts output is "y".
#[test]
fn test_is_writeable_alias_of_is_writable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("wr.txt", "");
echo is_writeable("wr.txt") ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `clearstatcache()` with no arguments is a no-op and prints "ok".
#[test]
fn test_clearstatcache_no_op_no_args() {
    let out = compile_and_run(r#"<?php clearstatcache(); echo "ok";"#);
    assert_eq!(out, "ok");
}

/// Verifies `clearstatcache()` with arguments (bool and path) is a no-op and prints "ok".
/// Arguments are accepted and discarded; this guards against argument handling bugs.
#[test]
fn test_clearstatcache_no_op_with_args() {
    let out = compile_and_run(
        r#"<?php clearstatcache(true, "foo.txt"); echo "ok";"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `clearstatcache()` evaluates its arguments before discarding them.
/// A user-defined function `marker()` is called and must echo "arg|" before "ok" appears,
/// confirming argument evaluation order is preserved.
#[test]
fn test_clearstatcache_evaluates_arguments() {
    let out = compile_and_run(
        r#"<?php
function marker(): bool {
    echo "arg|";
    return true;
}
clearstatcache(marker(), "foo.txt");
echo "ok";
"#,
    );
    assert_eq!(out, "arg|ok");
}

/// Verifies `stat()` returns an array with expected string keys ("size", "mode") and
/// numeric key 7 equal to "size". Uses a temp directory to create `metadata.txt`
/// and checks that mode bits equal 0x8000 (regular file) and key 7 matches size.
#[test]
fn test_stat_array_has_expected_keys() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("metadata.txt", "hello");
$info = stat("metadata.txt");
echo $info["size"] . "|" . ($info["mode"] & 0xF000) . "|" . ($info[7] === $info["size"] ? "match" : "differ");
"#,
    );
    assert_eq!(out, format!("5|{}|match", 0x8000));
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stat`, `lstat`, and `fopen`-derived `fstat` all return strict `false`
/// when given a nonexistent path or a false resource handle. Each result checked
/// individually and concatenated must be "slf".
#[test]
fn test_stat_lstat_fstat_failures_are_strict_false() {
    let out = compile_and_run(
        r#"<?php
echo stat("missing.txt") === false ? "s" : "!";
echo lstat("missing.txt") === false ? "l" : "!";
$f = @fopen("missing.txt", "r");
echo $f === false ? "f" : "!";
"#,
    );
    assert_eq!(out, "slf");
}

/// Verifies `fstat()` rejects a false handle (from a failed `fopen`) with an uncaught TypeError.
///
/// The report is asserted on the DIAGNOSTIC stream, which is where php puts it: MEASURED on this
/// exact program, `php -n` 8.5.6 writes `Fatal error: Uncaught TypeError: fstat(): Argument #1
/// ($stream) must be of type resource, false given` to STDOUT, leaves stderr empty and exits 255.
/// This used to read `out.stderr`, from when elephc routed an uncaught report around the output
/// buffer instead of through it; the assert found an empty string once that was aligned with php.
/// The property is unchanged — the TypeError is raised, names the argument and the given type,
/// and the program fails.
#[test]
fn test_fstat_rejects_fopen_false_runtime_handle() {
    let out = compile_and_run_capture(
        r#"<?php
$f = @fopen("missing.txt", "r");
fstat($f);
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert!(
        out.diagnostics.contains("TypeError: fstat()")
            && out.diagnostics.contains("false given"),
        "expected fstat TypeError, got diagnostics={} stderr={}",
        out.diagnostics,
        out.stderr
    );
}

/// Verifies that a failed `stat()` result still evaluates its key argument.
/// A user function `stat_key()` is called as the array access key and must echo
/// "key|" even though `stat("missing.txt")` returns false, confirming that
/// the key expression is evaluated before the array access short-circuits.
#[test]
fn test_failed_stat_array_access_still_evaluates_key() {
    let out = compile_and_run(
        r#"<?php
function stat_key() {
    echo "key|";
    return "size";
}
stat("missing.txt")[stat_key()];
echo "done";
"#,
    );
    assert_eq!(out, "key|done");
}

/// Verifies `stat()` array "size" field equals `filesize()` for a 7-byte file.
/// Uses a temp directory to create `seven.txt` containing "1234567".
#[test]
fn test_stat_array_size_matches_filesize() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seven.txt", "1234567");
$info = stat("seven.txt");
echo $info["size"] === filesize("seven.txt") ? "ok" : "differ";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stat()` array "mtime" field equals `filemtime()` for an existing file.
/// Uses a temp directory to create `mt.txt` and asserts both functions agree.
#[test]
fn test_stat_array_mtime_matches_filemtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("mt.txt", "");
$info = stat("mt.txt");
echo $info["mtime"] === filemtime("mt.txt") ? "ok" : "differ";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `lstat()` array for a regular file has the same "size" field as `stat()`.
/// Uses a temp directory to create `plain.txt` and asserts both arrays agree on size.
#[test]
fn test_lstat_array_for_regular_file_matches_stat() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("plain.txt", "abc");
$st = stat("plain.txt");
$lst = lstat("plain.txt");
echo $st["size"] === $lst["size"] ? "ok" : "differ";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fstat()` array "size" field reflects actual file contents (10 bytes).
/// Uses a temp directory to create `fd.txt` with "abcdefghij", opens it with `fopen`,
/// calls `fstat`, then `fclose`, and asserts size is "10".
#[test]
fn test_fstat_array_size_matches_file_contents() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("fd.txt", "abcdefghij");
$h = fopen("fd.txt", "r");
$info = fstat($h);
fclose($h);
echo $info["size"];
"#,
    );
    assert_eq!(out, "10");
    let _ = fs::remove_dir_all(&dir);
}

/// `filemtime()` answers `false` for a path it cannot stat, like its seven siblings and like php.
///
/// It used to answer with whatever the stack held where `st_mtime` would have been: the AArch64
/// helper read the buffer without checking that `stat()` had written it, and the x86_64 helper
/// checked but reported `0` — a legitimate timestamp, indistinguishable from success. Three
/// authorities had to agree for the failure to survive the trip: the runtime helper raises the
/// int|false flag its siblings already use, the lowering goes through the shared
/// `int_or_false` composer instead of reading a plain integer, and the declared return type is
/// no longer a bare `Int` — that declaration is what discarded the `false`.
///
/// The success half is asserted in the same fixture on purpose: a fix that reports failure
/// correctly and breaks the ordinary read would pass a failure-only test.
#[test]
fn test_filemtime_returns_false_for_an_unstatable_path() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("mtime.txt", "x");
$ok = filemtime("mtime.txt");
$bad = @filemtime("/nonexistent/elephc/probe");
echo var_export($bad, true), "|", ($ok > 1000000000 ? "ok" : "bad"), "|";
echo var_export(@filemtime("also/missing"), true);
"#,
    );
    assert_eq!(out, "false|ok|false");
    let _ = fs::remove_dir_all(&dir);
}

/// The same read, twice in one process, from two different stack depths.
///
/// This is the witness for the LEAK rather than for the value: an uninitialised read tracks a
/// stack address, so it answers differently depending on how deep the frame sits — and it
/// changed between runs of one binary, which no single-call assertion can see. Both calls must
/// now agree, and agree with php.
#[test]
fn test_filemtime_failure_does_not_depend_on_stack_depth() {
    let out = compile_and_run(
        r#"<?php
function deep(int $n): string {
    if ($n > 0) {
        return deep($n - 1);
    }
    return var_export(@filemtime("/nonexistent/elephc/probe"), true);
}
$shallow = var_export(@filemtime("/nonexistent/elephc/probe"), true);
echo $shallow, "|", deep(12);
"#,
    );
    assert_eq!(out, "false|false");
}
