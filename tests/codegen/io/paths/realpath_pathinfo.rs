//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O, paths realpath and pathinfo builtins, including realpath existing file, realpath strips redundant segments, and realpath missing returns false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Verifies `realpath()` resolves an existing file and returns its canonical path.
/// Creates a temp file with `file_put_contents`, then resolves it with `realpath()`.
#[test]
fn test_realpath_existing_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("anchor.txt");
echo $resolved !== false ? "ok" : "empty";
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `realpath()` normalizes `.` and `..` segments, producing the same result as direct resolution.
/// Fixture: `anchor.txt` vs `./anchor.txt` → both resolve to the same canonical path.
#[test]
fn test_realpath_strips_redundant_segments() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("./anchor.txt");
$direct = realpath("anchor.txt");
echo $resolved === $direct ? "match" : "differ";
"#,
    );
    assert_eq!(out, "match");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `realpath()` returns `false` when the path does not exist.
/// Fixture: `/definitely/does/not/exist/anywhere/12345` → expects `false`.
#[test]
fn test_realpath_missing_returns_false() {
    let out = compile_and_run(
        r#"<?php
$value = realpath("/definitely/does/not/exist/anywhere/12345");
echo $value === false ? "false" : "string";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies realpath cache helpers expose elephc's intentionally empty cache state.
/// Fixture: a resolved file still leaves cache_get empty and cache_size at zero.
#[test]
fn test_realpath_cache_helpers_report_empty_cache() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("anchor.txt", "");
realpath("anchor.txt");
echo function_exists("REALPATH_CACHE_GET") ? "exists" : "missing";
echo "|" . count(REALPATH_CACHE_GET());
echo "|" . RealPath_Cache_Size();
"#,
    );
    assert_eq!(out, "exists|0|0");
}

/// Verifies the success path of `realpath()` can be echoed directly without crashing.
/// Regression: codegen previously assumed union-typed `Str|Bool` results were unboxed scalars,
/// causing a crash when directly echoing the resolved path.
#[test]
fn test_realpath_direct_echo_does_not_crash() {
    // Regression: realpath used to return Union(Str, Bool) without boxing the
    // result as a Mixed cell, so directly echoing the value crashed because
    // the codegen pipeline expects boxed values for union-typed expressions.
    // This test verifies that the success path can be echoed directly.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("anchor.txt", "");
$resolved = realpath("anchor.txt");
echo $resolved !== false ? "ok" : "fail";
echo "|";
if ($resolved !== false) {
    echo $resolved;
}
"#,
    );
    assert!(out.starts_with("ok|"), "got: {}", out);
    assert!(out.ends_with("anchor.txt"), "got: {}", out);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `pathinfo()` with PATHINFO_DIRNAME returns the directory portion.
/// Fixture: `/var/log/syslog.log` with PATHINFO_DIRNAME → expects `/var/log`.
#[test]
fn test_pathinfo_dirname() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_DIRNAME);"#,
    );
    assert_eq!(out, "/var/log");
}

/// Verifies `pathinfo()` with PATHINFO_BASENAME returns the filename with extension.
/// Fixture: `/var/log/syslog.log` with PATHINFO_BASENAME → expects `syslog.log`.
#[test]
fn test_pathinfo_basename() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_BASENAME);"#,
    );
    assert_eq!(out, "syslog.log");
}

/// Verifies `pathinfo()` with PATHINFO_EXTENSION returns the last extension after the final dot.
/// Fixture: `/var/log/syslog.log` with PATHINFO_EXTENSION → expects `log`.
#[test]
fn test_pathinfo_extension() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "log");
}

/// Verifies `pathinfo()` with PATHINFO_EXTENSION returns the last extension for multiple dots.
/// Fixture: `archive.tar.gz` with PATHINFO_EXTENSION → expects `gz`.
#[test]
fn test_pathinfo_extension_multiple_dots() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "gz");
}

/// Verifies `pathinfo()` with PATHINFO_EXTENSION returns empty string when no dot is present.
/// Fixture: `/etc/hosts` with PATHINFO_EXTENSION → expects `""`.
#[test]
fn test_pathinfo_extension_no_dot() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/etc/hosts", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "");
}

/// Verifies `pathinfo()` with PATHINFO_EXTENSION on a dotfile returns the basename when no extension exists.
/// Fixture: `.bashrc` with PATHINFO_EXTENSION → expects `bashrc`.
#[test]
fn test_pathinfo_extension_dotfile() {
    let out = compile_and_run(
        r#"<?php echo pathinfo(".bashrc", PATHINFO_EXTENSION);"#,
    );
    assert_eq!(out, "bashrc");
}

/// Verifies `pathinfo()` with PATHINFO_FILENAME returns the filename without extension.
/// Fixture: `/var/log/syslog.log` with PATHINFO_FILENAME → expects `syslog`.
#[test]
fn test_pathinfo_filename() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/var/log/syslog.log", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "syslog");
}

/// Verifies `pathinfo()` with PATHINFO_FILENAME strips only the last extension for multiple dots.
/// Fixture: `archive.tar.gz` with PATHINFO_FILENAME → expects `archive.tar`.
#[test]
fn test_pathinfo_filename_multiple_dots() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("archive.tar.gz", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "archive.tar");
}

/// Verifies `pathinfo()` with PATHINFO_FILENAME on a file with no extension returns the basename.
/// Fixture: `/etc/hosts` with PATHINFO_FILENAME → expects `hosts`.
#[test]
fn test_pathinfo_filename_no_dot() {
    let out = compile_and_run(
        r#"<?php echo pathinfo("/etc/hosts", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "hosts");
}

/// Verifies `pathinfo()` with PATHINFO_FILENAME on a dotfile returns empty string (no filename before dot).
/// Fixture: `.bashrc` with PATHINFO_FILENAME → expects `""`.
#[test]
fn test_pathinfo_filename_dotfile() {
    let out = compile_and_run(
        r#"<?php echo pathinfo(".bashrc", PATHINFO_FILENAME);"#,
    );
    assert_eq!(out, "");
}

/// Verifies `pathinfo()` with no flags returns an associative array with all components.
/// Fixture: `/var/log/syslog.log` with no flag → expects `dirname|basename|extension|filename`.
#[test]
fn test_pathinfo_array_full() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

/// Verifies `pathinfo()` with PATHINFO_ALL returns the same array as the default (no flag).
/// Fixture: `/var/log/syslog.log` with PATHINFO_ALL → same as default.
#[test]
fn test_pathinfo_array_with_pathinfo_all_flag() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/var/log/syslog.log", PATHINFO_ALL);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

/// Verifies `pathinfo()` accepts a literal `15` as the all-components bitmask (same as PATHINFO_ALL).
/// Fixture: `foo.txt` with `15` → expects `.|||foo.txt|txt|foo`.
#[test]
fn test_pathinfo_array_with_literal_all_flag() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt", 15);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

/// Verifies `pathinfo()` array omits the `extension` key when no extension is present.
/// Fixture: `/etc/hosts` with no flag → `array_key_exists("extension", $info)` is false.
#[test]
fn test_pathinfo_array_no_extension_omits_key() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("/etc/hosts");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "/etc|hosts|hosts|no");
}

/// Verifies `pathinfo()` array on a dotfile includes the `extension` key (the whole name).
/// Fixture: `.bashrc` with no flag → extension key exists with value `bashrc`.
#[test]
fn test_pathinfo_array_dotfile_includes_extension() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo(".bashrc");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, ".bashrc|bashrc||yes");
}

/// Verifies `pathinfo()` array on a name with multiple dots extracts only the last extension.
/// Fixture: `archive.tar.gz` with no flag → extension=`gz`, filename=`archive.tar`.
#[test]
fn test_pathinfo_array_multiple_dots() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("archive.tar.gz");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "archive.tar.gz|gz|archive.tar");
}

/// Verifies `pathinfo()` array with a relative path uses `.` for the dirname.
/// Fixture: `foo.txt` with no flag → dirname=`.`.
#[test]
fn test_pathinfo_array_relative_path() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

/// Verifies `pathinfo()` array on an empty string omits the `dirname` key and returns empty strings for others.
/// Fixture: `""` with no flag → dirname key does not exist; basename=`""`, filename=`""`.
#[test]
fn test_pathinfo_array_empty_path_omits_dirname() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("");
echo (array_key_exists("dirname", $info) ? "yes" : "no") . "|";
echo $info["basename"] . "|" . $info["filename"] . "|";
echo pathinfo("", PATHINFO_DIRNAME);
"#,
    );
    assert_eq!(out, "no|||");
}

/// Verifies `pathinfo()` array on a trailing dot keeps an empty `extension` key.
/// Fixture: `file.` with no flag → extension=`""` but key exists; basename=`file.`.
#[test]
fn test_pathinfo_array_trailing_dot_keeps_empty_extension_key() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("file.");
echo $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . "|";
echo array_key_exists("extension", $info) ? "yes" : "no";
"#,
    );
    assert_eq!(out, "file.||file|yes");
}

/// Verifies `pathinfo()` with a multi-component bitmask returns a string for the dominant component.
/// Each flag combination returns the highest-priority component (DIRNAME=1, BASENAME=2, EXTENSION=4, FILENAME=8).
/// Fixture: DIRNAME|EXTENSION=`1|4`→`/a`, BASENAME|FILENAME=`2|8`→`b.php`, EXTENSION|FILENAME=`4|8`→`php`, `0`→`""`.
#[test]
fn test_pathinfo_bitmask_component_priority() {
    let out = compile_and_run(
        r#"<?php
echo pathinfo("/a/b.php", PATHINFO_DIRNAME | PATHINFO_EXTENSION) . "|";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . "|";
echo pathinfo("/a/b.php", PATHINFO_EXTENSION | PATHINFO_FILENAME) . "|";
echo pathinfo("/a/b.php", 0);
"#,
    );
    assert_eq!(out, "/a|b.php|php|");
}

/// Verifies `pathinfo()` with all four PATHINFO_* flags combined via `|` returns an array.
/// Fixture: `foo.txt` with DIRNAME|BASENAME|EXTENSION|FILENAME → dirname=`.`, basename=`foo.txt`, extension=`txt`, filename=`foo`.
#[test]
fn test_pathinfo_all_bitmask_expression_returns_array() {
    let out = compile_and_run(
        r#"<?php
$info = pathinfo("foo.txt", PATHINFO_DIRNAME | PATHINFO_BASENAME | PATHINFO_EXTENSION | PATHINFO_FILENAME);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

/// Verifies `pathinfo()` when the flag is a runtime variable holding PATHINFO_EXTENSION returns a string.
/// Fixture: `$flag = PATHINFO_EXTENSION; pathinfo("archive.tar.gz", $flag)` → expects `gz`.
#[test]
fn test_pathinfo_dynamic_component_flag_returns_string() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_EXTENSION;
echo pathinfo("archive.tar.gz", $flag);
"#,
    );
    assert_eq!(out, "gz");
}

/// Verifies `pathinfo()` when the flag is a runtime variable holding PATHINFO_ALL returns an array.
/// Fixture: `$flag = PATHINFO_ALL; pathinfo("/var/log/syslog.log", $flag)` → expects full array.
#[test]
fn test_pathinfo_dynamic_all_flag_returns_array() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_ALL;
$info = pathinfo("/var/log/syslog.log", $flag);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, "/var/log|syslog.log|log|syslog");
}

/// Verifies `pathinfo()` when the flag is a runtime bitmask of all components returns an array.
/// Fixture: `$flag = DIRNAME|BASENAME|EXTENSION|FILENAME; pathinfo("foo.txt", $flag)` → expects full array.
#[test]
fn test_pathinfo_dynamic_all_bitmask_returns_array() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_DIRNAME | PATHINFO_BASENAME | PATHINFO_EXTENSION | PATHINFO_FILENAME;
$info = pathinfo("foo.txt", $flag);
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"];
"#,
    );
    assert_eq!(out, ".|foo.txt|txt|foo");
}

/// Verifies `pathinfo()` shape can change at runtime when the flag variable is reassigned.
/// First uses PATHINFO_EXTENSION (string), then PATHINFO_ALL (array) — confirms type changes correctly.
#[test]
fn test_pathinfo_dynamic_runtime_shape_can_change() {
    let out = compile_and_run(
        r#"<?php
$flag = PATHINFO_EXTENSION;
$component = pathinfo("foo.txt", $flag);
echo $component . "|";
$flag = PATHINFO_ALL;
$info = pathinfo("foo.txt", $flag);
echo $info["basename"] . "|" . $info["extension"];
"#,
    );
    assert_eq!(out, "txt|foo.txt|txt");
}

/// Verifies `pathinfo()` with PATHINFO_ALL inside a user-defined function returns the array correctly.
/// Fixture: `dynamic_basename(PATHINFO_ALL)` → expects `foo.txt`.
#[test]
fn test_pathinfo_dynamic_all_inside_function_returns_array() {
    let out = compile_and_run(
        r#"<?php
function dynamic_basename(int $flag) {
    $info = pathinfo("foo.txt", $flag);
    return $info["basename"];
}
echo dynamic_basename(PATHINFO_ALL);
"#,
    );
    assert_eq!(out, "foo.txt");
}

/// Verifies `pathinfo()` with a zero flag returns an empty string.
/// Fixture: `$flag = 0; pathinfo("foo.txt", $flag)` → expects `""`.
#[test]
fn test_pathinfo_dynamic_zero_flag_returns_empty_string() {
    let out = compile_and_run(
        r#"<?php
$flag = 0;
echo "[" . pathinfo("foo.txt", $flag) . "]";
"#,
    );
    assert_eq!(out, "[]");
}

/// Verifies `dirname()` with an invalid `$levels` parameter (0) produces a compile-time error.
/// Fixture: `dirname("/usr/local/bin", 0)` → expects error about `$levels must be greater than or equal to 1`.
#[test]
fn test_dirname_dynamic_invalid_levels_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$levels = 0;
echo dirname("/usr/local/bin", $levels);
"#,
    );
    assert!(
        err.contains("dirname(): Argument #2 ($levels) must be greater than or equal to 1"),
        "unexpected stderr: {}",
        err
    );
}
