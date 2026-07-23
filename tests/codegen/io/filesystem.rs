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

/// Verifies sys_get_temp_dir returns a temp-named path. The check is genuinely
/// case-insensitive and accepts both the POSIX `tmp` spelling (Linux/macOS
/// `/tmp`) and the Windows `Temp` spelling: on Windows the runtime returns the
/// real `GetTempPathW` result (e.g. `C:\Users\...\Temp`), whose lowercased form
/// contains `temp`, not `tmp`.
#[test]
fn test_sys_get_temp_dir() {
    let out = compile_and_run(
        r#"<?php
$tmp = sys_get_temp_dir();
echo $tmp;
"#,
    );
    let lower = out.to_lowercase();
    assert!(
        lower.contains("tmp") || lower.contains("temp"),
        "sys_get_temp_dir returned {:?}",
        out
    );
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
