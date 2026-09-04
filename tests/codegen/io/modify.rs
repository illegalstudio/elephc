//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O modify, including function exists recognizes file modify builtins, file modify builtins are case insensitive and namespaced, and chmod existing file succeeds.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that function_exists() recognizes all file-modify builtins: touch,
/// chmod, chown, chgrp, lchown, lchgrp, umask, ftruncate, fflush, fsync, and fdatasync.
#[test]
fn test_function_exists_recognizes_file_modify_builtins() {
    let out = compile_and_run(
        r#"<?php
echo (function_exists("touch") ? "1" : "0")
   . (function_exists("ChMoD") ? "1" : "0")
   . (function_exists("chown") ? "1" : "0")
   . (function_exists("chgrp") ? "1" : "0")
   . (function_exists("LcHoWn") ? "1" : "0")
   . (function_exists("LcHgRp") ? "1" : "0")
   . (function_exists("umask") ? "1" : "0")
   . (function_exists("ftruncate") ? "1" : "0")
   . (function_exists("fflush") ? "1" : "0")
   . (function_exists("fsync") ? "1" : "0")
   . (function_exists("FdAtAsYnC") ? "1" : "0");
"#,
    );
    assert_eq!(out, "11111111111");
}

/// Verifies file-modify builtins are case-insensitive and resolve correctly
/// inside a namespace via PHP's namespace fallback rules.
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

/// Verifies chmod() succeeds on an existing file and returns true.
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

/// Verifies chmod() actually removes write permission when mode 0400 is set;
/// fileperms() confirms the mode before chmod restores it.
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

/// Verifies chmod() returns false when the path does not exist.
#[test]
fn test_chmod_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chmod("/nonexistent/xyz/abc.txt", 0o644) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

/// An unregistered `file://` wrapper stops `chmod()`, and php words it its own way.
///
/// `chmod()` carried no wrapper guard at all: it reached the syscall, which succeeded, and
/// answered TRUE while php answers false — a wrong VALUE, not a missing line. It is also the one
/// guarded operation php does not word like the others: MEASURED on `php -n` 8.5.6,
/// `unlink()` and `rename()` say "Unable to locate stream wrapper" and this says
/// "Cannot call chmod() for a non-standard stream".
#[test]
fn test_chmod_refuses_while_the_file_wrapper_is_unregistered() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("chm.txt", "x");
stream_wrapper_unregister("file");
var_dump(chmod("chm.txt", 0644));
var_dump(rename("chm.txt", "chm2.txt"));
stream_wrapper_restore("file");
unlink("chm.txt");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: chmod(): Cannot call chmod() for a non-standard stream\n\
         Warning: rename(): Unable to locate stream wrapper\n",
        "php gives these two different wordings, and neither is a paraphrase of the other"
    );
}

/// Verifies chown() returns false when the path does not exist.
#[test]
fn test_chown_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chown("/nonexistent/xyz/abc.txt", 1000) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

/// Verifies chgrp() returns false when the path does not exist.
#[test]
fn test_chgrp_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php echo chgrp("/nonexistent/xyz/abc.txt", 1000) ? "y" : "n";"#,
    );
    assert_eq!(out, "n");
}

/// Verifies lchown()/lchgrp() succeed on an existing symlink when asked to leave ownership unchanged.
/// The namespaced, mixed-case calls also verify PHP-style builtin fallback and case-insensitive lookup.
#[test]
fn test_lchown_lchgrp_symlink_noop_succeeds() {
    let out = compile_and_run(
        r#"<?php
namespace FsModifyLinks;
file_put_contents("target.txt", "x");
symlink("target.txt", "link.txt");
echo LcHoWn("link.txt", -1) ? "o" : "O";
echo LcHgRp("link.txt", -1) ? "g" : "G";
unlink("link.txt");
unlink("target.txt");
"#,
    );
    assert_eq!(out, "og");
}

/// Verifies lchown()/lchgrp() return false when the path does not exist.
#[test]
fn test_lchown_lchgrp_missing_path_returns_false() {
    let out = compile_and_run(
        r#"<?php
echo lchown("/nonexistent/xyz/abc-link.txt", 1000) ? "y" : "n";
echo lchgrp("/nonexistent/xyz/abc-link.txt", 1000) ? "y" : "n";
"#,
    );
    assert_eq!(out, "nn");
}

/// Verifies lchown()/lchgrp() return false for unknown user/group names.
#[test]
fn test_lchown_lchgrp_unknown_principal_strings_return_false() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("target.txt", "x");
symlink("target.txt", "link.txt");
echo lchown("link.txt", "elephc_user_that_should_not_exist") ? "y" : "n";
echo lchgrp("link.txt", "elephc_group_that_should_not_exist") ? "y" : "n";
unlink("link.txt");
unlink("target.txt");
"#,
    );
    assert_eq!(out, "nn");
}

/// Verifies chown() returns false when the owner string does not correspond to
/// a valid user on the host system.
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

/// Verifies chgrp() returns false when the group string does not correspond to
/// a valid group on the host system.
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

/// Verifies umask() returns the previous mask when called with an argument.
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

/// Verifies umask() with no arguments only reads the current mask without
/// modifying it by checking identity before and after a null-probed call.
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

/// Verifies ftruncate() shrinks the file to the specified byte length and that
/// filesize() reflects the truncated size.
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

/// Verifies ftruncate() extends the file with zero bytes when the offset is
/// larger than the current file size, and that filesize() reflects the new size.
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

/// Verifies fsync() succeeds on an open file and returns true.
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

/// Verifies fflush() succeeds on an open file and returns true.
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

/// Verifies fdatasync() succeeds on an open file and returns true.
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

/// Verifies touch() creates a file that did not exist and returns true.
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

/// Regression: touch() previously called libc open() with the mode in x2,
/// but Darwin ARM64 passes variadic libc args on the stack — so the
/// kernel ignored the requested 0644 and created the file with garbage
/// permissions (often 0240). Subsequent reads then failed with EACCES.
/// After the fix, file_get_contents on a freshly-touched-then-written
/// file must succeed.
#[test]
fn test_touch_creates_file_with_readable_permissions() {
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

/// Verifies touch() creates a file with PHP's default permission of 0666 when
/// umask is 0, confirming fileperms() reflects the expected mode.
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

/// Verifies touch() does not modify the content of an existing file; after
/// touch() the file must still contain its original data.
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

/// Verifies touch() with a null mtime argument uses the current system time
/// (filemtime() must return a value greater than a known past epoch).
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

/// Verifies touch() with a null variable passed as mtime uses the current
/// system time, same as the literal null case.
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

/// Verifies touch() applies the provided Unix timestamp as the modification
/// time and that filemtime() reads back the same value.
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

/// Verifies touch() with -1 as the mtime argument writes the literal -1 as
/// the modification time (not interpreted as current time).
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

/// Verifies touch() with a null atime argument defaults to the explicit mtime
/// value, and that both filemtime() and fileatime() return the same timestamp.
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

/// Verifies touch() with a null variable atime argument defaults to the
/// explicit mtime, same as the literal null case.
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

/// Verifies touch() with explicit mtime and atime arguments succeeds and that
/// mtime is preserved; atime readback is platform-dependent and not asserted.
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

/// Verifies chown()/chgrp() decide php's `string|int` principal from the VALUE, not the static type.
///
/// `fileowner()` is declared `int|false`, so `chown($p, fileowner($p))` hands over a boxed cell.
/// The principal was declared `string` in the builtin contract, which made the argument coercion
/// convert that cell to a string and sent the call down the NAME path: php answered `bool(true)`
/// for the file's own owner while elephc looked up the user named "501" and answered `bool(false)`.
/// A string principal is genuinely a name — MEASURED, `chown($p, "501")` warns
/// `Unable to find uid for 501` on `php -n` 8.5.6 — so the two paths cannot be merged; the tag
/// has to pick between them at run time.
#[test]
fn test_chown_principal_union_takes_the_uid_path_not_the_name_path() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("owner-union.txt", "x");
echo chown("owner-union.txt", fileowner("owner-union.txt")) ? "u" : "U";
echo chgrp("owner-union.txt", filegroup("owner-union.txt")) ? "g" : "G";
unlink("owner-union.txt");
"#,
    );
    assert_eq!(out, "ug");
}

/// Verifies a `mixed` chown() principal reaches php's coercive `string|int` boundary per TAG.
///
/// Each answer was MEASURED on `php -n` 8.5.6: an int is a uid, a bool and `null` coerce to
/// uid 0 and fail for an unprivileged process, a float truncates, and a string is looked up as a
/// NAME — which is why `"501"` fails where the int `501` succeeds.
#[test]
fn test_chown_mixed_principal_dispatches_on_the_runtime_tag() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("owner-tags.txt", "x");
$self = fileowner("owner-tags.txt");
foreach ([$self, true, null, "501", (float)$self] as $v) {
    echo @chown("owner-tags.txt", $v) ? "y" : "n";
}
unlink("owner-tags.txt");
"#,
    );
    assert_eq!(out, "ynnny");
}

/// Verifies a `mixed` chown() principal holding a container throws php's own TypeError.
///
/// The class name is read from the dense table `get_class()` uses, because php prints
/// `stdClass given`, not `object given`.
#[test]
fn test_chown_mixed_principal_container_throws_php_type_error() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("owner-bad.txt", "x");
foreach ([[1, 2], new stdClass()] as $v) {
    try { @chown("owner-bad.txt", $v); }
    catch (Throwable $e) { echo $e->getMessage(), "\n"; }
}
unlink("owner-bad.txt");
"#,
    );
    assert_eq!(
        out,
        "chown(): Argument #2 ($user) must be of type string|int, array given\n\
         chown(): Argument #2 ($user) must be of type string|int, stdClass given\n"
    );
}

/// Verifies lchown()/lchgrp() take the same runtime principal decision as their non-link siblings.
#[test]
fn test_lchown_mixed_principal_dispatches_on_the_runtime_tag() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("link-target.txt", "x");
symlink("link-target.txt", "link-union.txt");
echo lchown("link-union.txt", fileowner("link-target.txt")) ? "u" : "U";
echo lchgrp("link-union.txt", filegroup("link-target.txt")) ? "g" : "G";
echo @lchown("link-union.txt", "501") ? "s" : "S";
unlink("link-union.txt");
unlink("link-target.txt");
"#,
    );
    assert_eq!(out, "ugS");
}

/// Verifies a WRITTEN `null` principal is coerced, not refused.
///
/// php's ZPP takes it into `string|int` as uid/gid 0 — MEASURED on `php -n` 8.5.6, a written
/// null answers exactly what an explicit `0` answers, after a deprecation. Refusing it was doubly
/// wrong: php runs the program, and the refusal came from the BACKEND, after the checker had
/// already accepted the call.
///
/// The assertion compares the two spellings rather than naming an outcome, because whether
/// uid/gid 0 SUCCEEDS depends on who owns the test directory.
#[test]
fn test_ownership_builtins_coerce_a_written_null_principal() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("null-principal.txt", "x");
symlink("null-principal.txt", "null-principal.link");
echo @chown("null-principal.txt", null) === @chown("null-principal.txt", 0) ? "y" : "n";
echo @chgrp("null-principal.txt", null) === @chgrp("null-principal.txt", 0) ? "y" : "n";
echo @lchown("null-principal.link", null) === @lchown("null-principal.link", 0) ? "y" : "n";
echo @lchgrp("null-principal.link", null) === @lchgrp("null-principal.link", 0) ? "y" : "n";
unlink("null-principal.link");
unlink("null-principal.txt");
"#,
    );
    assert_eq!(out, "yyyy");
}
