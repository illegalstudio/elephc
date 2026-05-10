//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O modify, including function exists recognizes file modify builtins, file modify builtins are case insensitive and namespaced, and chmod existing file succeeds.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_function_exists_recognizes_file_modify_builtins() {
    let out = compile_and_run(
        r#"<?php
echo (function_exists("touch") ? "1" : "0")
   . (function_exists("ChMoD") ? "1" : "0")
   . (function_exists("chown") ? "1" : "0")
   . (function_exists("chgrp") ? "1" : "0")
   . (function_exists("umask") ? "1" : "0")
   . (function_exists("ftruncate") ? "1" : "0")
   . (function_exists("fflush") ? "1" : "0")
   . (function_exists("fsync") ? "1" : "0")
   . (function_exists("FdAtAsYnC") ? "1" : "0");
"#,
    );
    assert_eq!(out, "111111111");
}

#[test]
fn test_file_modify_builtins_are_case_insensitive_and_namespaced() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
namespace FsModifyCase;
ToUcH("case.txt");
$ok = ChMoD("case.txt", 0o644);
echo ($ok ? "y" : "n") . "|" . (FiLe_ExIsTs("case.txt") ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chmod_existing_file_succeeds() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("perms.txt", "");
echo chmod("perms.txt", 0o644) ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chmod_makes_file_unwritable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ro.txt", "");
chmod("ro.txt", 0o400);
$mode = sprintf("%04o", fileperms("ro.txt") & 0o777);
chmod("ro.txt", 0o644);
echo $mode;
"#,
    );
    assert_eq!(out, "0400");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chmod_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chmod("/nonexistent/xyz/abc.txt", 0o644) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_chown_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chown("/nonexistent/xyz/abc.txt", 1000) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_chgrp_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chgrp("/nonexistent/xyz/abc.txt", 1000) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

#[test]
fn test_chown_unknown_user_string_returns_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("owner.txt", "");
echo chown("owner.txt", "elephc_user_that_should_not_exist") ? "y" : "n";
"#,
    );
    assert_eq!(out, "n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chgrp_unknown_group_string_returns_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("group.txt", "");
echo chgrp("group.txt", "elephc_group_that_should_not_exist") ? "y" : "n";
"#,
    );
    assert_eq!(out, "n");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_umask_set_then_set_back() {
    let out = compile_and_run(
        r#"<?php
$prev = umask(0o027);
$set = umask($prev);
echo $set;
"#,
    );
    assert_eq!(out, format!("{}", 0o027));
}

#[test]
fn test_umask_no_args_does_not_change() {
    let out = compile_and_run(
        r#"<?php
$before = umask(0o022);
$probed = umask();
$restored = umask($before);
echo ($probed === 0o022 ? "y" : "n") . "|" . ($restored === 0o022 ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
}

#[test]
fn test_ftruncate_shrinks_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("trunc.txt", "0123456789");
$h = fopen("trunc.txt", "r+");
$ok = ftruncate($h, 4);
fclose($h);
echo ($ok ? "y" : "n") . "|" . filesize("trunc.txt");
"#,
    );
    assert_eq!(out, "y|4");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_ftruncate_extends_file_with_zeros() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ext.txt", "abc");
$h = fopen("ext.txt", "r+");
$ok = ftruncate($h, 8);
fclose($h);
echo ($ok ? "y" : "n") . "|" . filesize("ext.txt");
"#,
    );
    assert_eq!(out, "y|8");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fsync_open_file_succeeds() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sync.txt", "data");
$h = fopen("sync.txt", "r+");
$ok = fsync($h);
fclose($h);
echo $ok ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fflush_open_file_succeeds() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("flush.txt", "data");
$h = fopen("flush.txt", "r+");
$ok = fflush($h);
fclose($h);
echo $ok ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fdatasync_open_file_succeeds() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ds.txt", "data");
$h = fopen("ds.txt", "r+");
$ok = fdatasync($h);
fclose($h);
echo $ok ? "y" : "n";
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_creates_missing_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$ok = touch("freshly_touched.txt");
echo ($ok ? "y" : "n") . "|" . (file_exists("freshly_touched.txt") ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_creates_file_with_readable_permissions() {
    // Regression: touch() previously called libc open() with the mode in x2,
    // but Darwin ARM64 passes variadic libc args on the stack — so the
    // kernel ignored the requested 0644 and created the file with garbage
    // permissions (often 0240). Subsequent reads then failed with EACCES.
    // After the fix, file_get_contents on a freshly-touched-then-written
    // file must succeed.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
touch("readback.txt");
file_put_contents("readback.txt", "ok");
echo file_get_contents("readback.txt");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_creates_file_with_php_default_permissions() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$old = umask(0);
touch("mode.txt");
umask($old);
echo sprintf("%04o", fileperms("mode.txt") & 0o777);
"#,
    );
    assert_eq!(out, "0666");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_does_not_truncate_existing_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("preserved.txt", "important");
touch("preserved.txt");
echo file_get_contents("preserved.txt");
"#,
    );
    assert_eq!(out, "important");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_null_mtime_uses_current_time() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("current.txt", "");
$ok = touch("current.txt", null);
echo ($ok ? "y" : "n") . "|" . (filemtime("current.txt") > 1000000000 ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_null_mtime_variable_uses_current_time() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("current_var.txt", "");
$mtime = null;
$ok = touch("current_var.txt", $mtime);
echo ($ok ? "y" : "n") . "|" . (filemtime("current_var.txt") > 1000000000 ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_with_explicit_mtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("mtime.txt", "");
touch("mtime.txt", 1000000000);
echo filemtime("mtime.txt");
"#,
    );
    assert_eq!(out, "1000000000");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_negative_one_is_explicit_timestamp() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("negative.txt", "");
touch("negative.txt", -1);
echo filemtime("negative.txt");
"#,
    );
    assert_eq!(out, "-1");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_null_atime_defaults_to_explicit_mtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("null_atime.txt", "");
touch("null_atime.txt", 1000000000, null);
echo filemtime("null_atime.txt") . "|" . fileatime("null_atime.txt");
"#,
    );
    assert_eq!(out, "1000000000|1000000000");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_null_atime_variable_defaults_to_explicit_mtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("null_atime_var.txt", "");
$atime = null;
touch("null_atime_var.txt", 1000000000, $atime);
echo filemtime("null_atime_var.txt") . "|" . fileatime("null_atime_var.txt");
"#,
    );
    assert_eq!(out, "1000000000|1000000000");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_touch_with_explicit_mtime_and_atime() {
    // The atime cannot be reliably read back without fileatime() (introduced in
    // Phase 2 on a separate branch), so this test only verifies that
    // touch() with three args succeeds and that the mtime sticks.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("both.txt", "");
$ok = touch("both.txt", 1000000000, 900000000);
echo ($ok ? "y" : "n") . "|" . filemtime("both.txt");
"#,
    );
    assert_eq!(out, "y|1000000000");
    let _ = fs::remove_dir_all(&dir);
}
