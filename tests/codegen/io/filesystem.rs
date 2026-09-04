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

/// `mkdir()` honours `$permissions` and `$recursive`, the parameters php documents.
///
/// The contract stopped at one parameter, so `mkdir($p, 0755, true)` — the call every
/// "create this directory tree" snippet makes — was a COMPILE ERROR: "mkdir() takes exactly 1
/// argument". The runtime helper matched, hard-coding mode 0755 and having nowhere to put a
/// recursive flag. Measured against php 8.5.6, which prints exactly the output below, including
/// `false` for a second create over an existing directory.
#[test]
fn test_mkdir_honours_permissions_and_recursive() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
var_dump(mkdir("tree/a/b", 0755, true));
var_dump(is_dir("tree/a/b"));
printf("%o\n", fileperms("tree/a/b") & 0777);
var_dump(mkdir("plain", 0700));
printf("%o\n", fileperms("plain") & 0777);
var_dump(@mkdir("plain", 0700));
var_dump(rmdir("plain"));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\n755\nbool(true)\n700\nbool(false)\nbool(true)\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `mkdir()` without `$recursive` must NOT create missing parents.
///
/// The recursive walk only runs when asked; php returns false here because the parent is absent.
#[test]
fn test_mkdir_without_recursive_refuses_a_missing_parent() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
var_dump(@mkdir("absent/child", 0777));
var_dump(is_dir("absent/child"));
var_dump(is_dir("absent"));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(false)\n");
    let _ = fs::remove_dir_all(&dir);
}

/// `rmdir()` and `unlink()` accept the trailing `$context` php documents.
///
/// Both contracts stopped at the path, so passing a context was a compile error. elephc has no
/// context plumbing on the path-op route, so the argument is accepted and IGNORED — but accepted,
/// because refusing a documented signature is the worse answer.
#[test]
fn test_rmdir_and_unlink_accept_a_context_argument() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$ctx = stream_context_create([]);
mkdir("ctxdir");
file_put_contents("ctxfile.txt", "x");
var_dump(rmdir("ctxdir", $ctx));
var_dump(unlink("ctxfile.txt", $ctx));
var_dump(is_dir("ctxdir"));
var_dump(file_exists("ctxfile.txt"));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(false)\nbool(false)\n");
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

/// `is_dir()` works in a program that names nothing else.
///
/// It SEGFAULTED. `__rt_is_dir` is the wrapper guard and `__rt_is_dir_core` the body, declared as
/// a second global a few instructions later, and the guard branches over the refusal into it. On
/// macOS every global starts its own SUBSECTION, and that branch — resolved by the assembler
/// inside one section — carries no relocation: the linker saw nothing referencing the core,
/// stripped it, and the branch landed in whatever took its place.
///
/// MEASURED: the same program with a `glob("*.txt", GLOB_ONLYDIR)` call added — the core's only
/// other caller — ran correctly. That is what kept this out of every suite: real test programs
/// name enough other things to keep the core alive.
///
/// The test is deliberately minimal for the same reason. Adding another filesystem call to it
/// would hide the defect again.
#[test]
fn test_is_dir_alone_in_a_program() {
    let out = compile_and_run(
        r#"<?php
echo is_dir(".") ? "dir" : "not", "|";
echo is_dir("no_such_directory_here") ? "dir" : "not";
"#,
    );
    assert_eq!(out, "dir|not");
}

/// php refuses to copy a file onto ITSELF, and knows which file that is by (st_dev, st_ino).
///
/// MEASURED on `php -n` 8.5.6: the same path, `./` before the same path, a hard link to the source
/// and a symlink to it all answer `false`, say nothing, and leave the file alone; a destination
/// that does not exist yet is an ordinary copy. elephc answered `true` to all four. Comparing the
/// path STRINGS would answer three of the five correctly and is not the rule php implements.
#[test]
fn test_copy_refuses_a_destination_that_is_the_same_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sf.txt", "AB");
link("sf.txt", "sf_hard.txt");
symlink("sf.txt", "sf_soft.txt");
var_dump(copy("sf.txt", "sf.txt"));
var_dump(copy("sf.txt", "./sf.txt"));
var_dump(copy("sf.txt", "sf_hard.txt"));
var_dump(copy("sf.txt", "sf_soft.txt"));
var_dump(file_get_contents("sf.txt"));
var_dump(copy("sf.txt", "sf_new.txt"));
var_dump(file_get_contents("sf_new.txt"));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nstring(2) \"AB\"\n\
         bool(true)\nstring(2) \"AB\"\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `copy()` truncates its destination whatever the last `file_put_contents()` was told.
///
/// The one-shot writer reads `$flags` from a register and tests FILE_APPEND. `copy()`, the phar
/// finalizer and the lowered-source copy all called it without setting that register, so the last
/// lowering to leave something there decided whether the destination was TRUNCATED or EXTENDED.
/// MEASURED: with an appending write to an unrelated file in between, `copy()` appended its source
/// to a destination php replaces — and the same program without that write copied correctly, which
/// is what made it invisible.
#[test]
fn test_copy_truncates_after_an_appending_write_elsewhere() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("cf_src.txt", "NEW");
file_put_contents("cf_dst.txt", "OLDOLD");
file_put_contents("cf_other.txt", "x", FILE_APPEND);
var_dump(copy("cf_src.txt", "cf_dst.txt"));
var_dump(file_get_contents("cf_dst.txt"));
"#,
    );
    assert_eq!(out, "bool(true)\nstring(3) \"NEW\"\n");
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

/// Verifies sys_get_temp_dir returns a usable absolute directory.
///
/// This used to require the answer to CONTAIN "tmp", which is not true of php: on macOS it
/// hands out a private per-user directory such as `/var/folders/xc/…/T`, with no "tmp" in it
/// anywhere. The assertion only held because elephc answered a hardcoded `/tmp`, so the test
/// pinned the divergence rather than the behaviour.
///
/// What the answer must satisfy on every platform is checked instead; the relationship to
/// `TMPDIR` is pinned separately by `test_sys_get_temp_dir_follows_tmpdir`.
#[test]
fn test_sys_get_temp_dir() {
    let out = compile_and_run(
        r#"<?php
$tmp = sys_get_temp_dir();
echo var_export($tmp !== "" && is_dir($tmp), true);
"#,
    );
    assert_eq!(out, "true");
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

/// Verifies `scandir()` on a missing directory answers FALSE, as php does.
///
/// Three eras of this test: AArch64 first handed `opendir()`'s NULL straight to `readdir()`
/// and crashed; then the empty listing papered over the crash while diverging from php's
/// `false`; now the union is real. `=== false` is the manual's own failure test, and
/// `count()` on the false raises php's TypeError — both asserted, because the empty-array era
/// made exactly those two observations impossible.
#[test]
fn test_scandir_on_a_missing_directory_answers_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$entries = @scandir("no_such_directory_here");
var_dump($entries === false);
try {
    count($entries);
    echo "uncaught";
} catch (TypeError $e) {
    echo "count: ", $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "bool(true)\ncount: count(): Argument #1 ($value) must be of type Countable|array, false given"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the array-taking builtin TypeErrors answer the same through `eval()` as compiled.
///
/// The compiled side already threw — the lowering wraps every `array|false` argument site —
/// while `eval()` SILENTLY ACCEPTED the same `false`: `in_array("x", @scandir(...))` answered
/// without throwing, so a failed `scandir()` read as an empty haystack and the caller's
/// `catch (TypeError)` never ran. Both halves are asserted, because a test that only ran one
/// of them could not see the two sides disagree.
///
/// `array_merge()` earns its place beside `in_array()`: it is FULLY variadic, so php's message
/// carries no `($name)` segment at all, where `in_array()` names `$haystack` and `sort()` names
/// `$array`. `sort()` adds the third shape — a BY-REFERENCE receiver, which has to throw before
/// it writes anything back.
#[test]
fn test_array_builtin_type_errors_match_between_compiled_and_eval() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$d = @scandir("no_such_directory_here");
try { in_array("x", $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_merge([], $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { $s = $d; sort($s); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "\n";
eval('
$d = @scandir("no_such_directory_here");
try { in_array("x", $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { array_merge([], $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { $s = $d; sort($s); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
');
"#,
    );
    let expected = concat!(
        "in_array(): Argument #2 ($haystack) must be of type array, false given",
        "|array_merge(): Argument #2 must be of type array, false given",
        "|sort(): Argument #1 ($array) must be of type array, false given",
    );
    let (compiled, evaluated) = out
        .split_once('\n')
        .expect("compiled half, then the eval half");
    assert_eq!(compiled, expected);
    assert_eq!(evaluated, expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `count()` answers the same ERROR CLASS through `eval()` as compiled, on both slots.
///
/// `count()` is the one builtin of this family that reaches all three shapes in one name, and
/// the two backends started out disagreeing on the middle one, measured against `php -n` 8.5.6:
///
/// - Argument #1 is a `TypeError` naming the union `Countable|array` — both sides already
///   threw it, so it is the control that proves the harness sees a real throw.
/// - Argument #2 out of range is a `ValueError` naming the two accepted CONSTANTS rather than
///   the offending value. The compiled side already raised it while `eval()` answered an
///   UNCATCHABLE `RuntimeFatal`, so the `catch (ValueError)` block never ran there. A wrong
///   error CLASS is what this half pins: catching `ValueError` and not `TypeError` is the
///   assertion, because an eval that threw the wrong class would still print something.
/// - `array_search()` rides along as the third slot shape, an argument #2 `TypeError`.
///
/// Asserting both halves is the point — a test that ran only one could not see them disagree.
///
/// Two members of this family are NOT here because the COMPILED side still diverges, measured:
/// `array_key_exists("x", false)` and `implode(",", false)` both answer without throwing where
/// php throws. Neither is reachable by adding a row to `ARRAY_OR_FALSE_ARG_SITES`: both declare
/// their parameter `Mixed`, so the lowered value is a boxed cell and `array_or_false_member()`
/// never matches. Fixing them means widening the wrap, which is deliberately left undone here
/// rather than papered over by a test that asserts today's wrong answer.
#[test]
fn test_count_argument_errors_match_between_compiled_and_eval() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$d = @scandir("no_such_directory_here");
try { count($d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { count([1], 99); echo "uncaught"; } catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
try { array_search("x", $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "\n";
eval('
$d = @scandir("no_such_directory_here");
try { count($d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
echo "|";
try { count([1], 99); echo "uncaught"; } catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
try { array_search("x", $d); echo "uncaught"; } catch (TypeError $e) { echo $e->getMessage(); }
');
"#,
    );
    let expected = concat!(
        "count(): Argument #1 ($value) must be of type Countable|array, false given",
        "|count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE",
        "|array_search(): Argument #2 ($haystack) must be of type array, false given",
    );
    let (compiled, evaluated) = out
        .split_once('\n')
        .expect("compiled half, then the eval half");
    assert_eq!(compiled, expected);
    assert_eq!(evaluated, expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `scandir()` reports an unopenable directory the way php does — and stays SILENT
/// when the directory opens.
///
/// php-src writes TWO lines for one failure, the second naming the error number, and elephc
/// wrote neither: the failure was completely mute, so a typo'd path produced an empty listing
/// and no clue. Neither line needed a composer of its own, because `__rt_errno_warning` already
/// appends `strerror` and the newline and so serves as the tail of both.
///
/// The successful call at the end is not padding. The failure block was first placed after the
/// `closedir` that ends the read loop, so the SUCCESS path fell straight into it and every
/// working `scandir()` printed a warning carrying a stale errno. Only a probe that exercised a
/// directory which opens could catch that, and the first one did.
///
/// `@` suppression and the 3000-iteration loop are asserted together: the error number is
/// rendered by `__rt_itoa`, which formats into the shared 64 KiB concat arena and advances its
/// cursor, so a loop over unreadable paths would eat the buffer if the diagnostic did not hand
/// its scratch back.
#[test]
fn test_scandir_reports_an_unopenable_directory_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
scandir("/pas/la");
scandir("/etc/hosts");
@scandir("/pas/la");
for ($i = 0; $i < 3000; $i++) {
    @scandir("/pas/la/deep/path/number/$i");
}
$here = scandir(".");
echo "opened=", (count($here) > 0 ? "yes" : "no"), "\n";
"#,
    );
    assert!(out.success, "the diagnostics must not disturb the program");
    assert_eq!(out.stdout, "opened=yes\n");
    assert_eq!(
        out.diagnostics,
        "Warning: scandir(/pas/la): Failed to open directory: No such file or directory\n\
         Warning: scandir(): (errno 2): No such file or directory\n\
         Warning: scandir(/etc/hosts): Failed to open directory: Not a directory\n\
         Warning: scandir(): (errno 20): Not a directory\n",
        "both lines, both error numbers, nothing from the suppressed calls, \
         and nothing at all from the directory that opened"
    );
}

/// Verifies `file_put_contents()` on an unopenable path warns and answers false — and writes
/// the payload NOWHERE.
///
/// The open result was never checked. On macOS a failed open answers the ERRNO with the carry
/// set, so the payload was written through descriptor 2 — the caller's own stderr — and the
/// byte count reported SUCCESS: `file_put_contents("/no/such/dir/x", $secret)` leaked the
/// secret to the terminal and returned int(7). php warns and answers false, which is also why
/// the declaration is now `int|false` rather than `Int`: with `Int`, the manual's own
/// `=== false` failure test could never fire.
///
/// The stdout assertion is exact so a payload leaking to EITHER stream fails the test.
#[test]
fn test_file_put_contents_on_an_unopenable_path_answers_false() {
    let out = compile_and_run_capture(
        r#"<?php
$n = file_put_contents("/no/such/dir/leak.txt", "SECRET-PAYLOAD");
var_dump($n);
var_dump($n === false);
var_dump(@file_put_contents("/no/such/dir/leak.txt", "SECRET-PAYLOAD"));
"#,
    );
    assert!(out.success, "a failed write is not a crash");
    assert_eq!(out.stdout, "bool(false)\nbool(true)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: file_put_contents(/no/such/dir/leak.txt): Failed to open stream: \
         No such file or directory\n",
        "one warning in php's wording; the @-suppressed call prints nothing, \
         and the payload appears on neither stream"
    );
}

/// Verifies `scandir()` sorts like php and its `array|false` union flows through the family.
///
/// php sorts ascending by default, descending for SCANDIR_SORT_DESCENDING, and keeps readdir
/// order only for SCANDIR_SORT_NONE — elephc answered readdir order for every call, which is
/// filesystem-dependent. The file names are created in REVERSE alphabetical order so an
/// unsorted listing cannot pass by accident.
///
/// The consumers each pin one leg of the union machinery: `in_array`/`array_values`/
/// `array_map`/`array_filter`/`array_search` go through the argument unbox (which must borrow,
/// not own — an owned unbox freed the listing UNDER the box and a later `sort($d)` sorted
/// freed memory), and `sort($d)` goes through the in-place path where the box must remain the
/// listing's sole owner or the copy-on-write split sorts a copy.
#[test]
fn test_scandir_sorts_like_php_and_the_union_flows_through_the_family() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("sd");
file_put_contents("sd/z.txt", "1");
file_put_contents("sd/a.txt", "2");
echo implode(",", scandir("sd")), "\n";
echo implode(",", scandir("sd", SCANDIR_SORT_DESCENDING)), "\n";
$none = scandir("sd", SCANDIR_SORT_NONE);
sort($none);
echo implode(",", $none), "\n";
$d = scandir("sd");
echo "in=", var_export(in_array("a.txt", $d), true), "\n";
sort($d);
echo "s0=", $d[2], "\n";
echo "vals=", count(array_values(scandir("sd"))), "\n";
$up = array_map(fn($f) => strtoupper($f), scandir("sd"));
echo "map=", $up[2], "\n";
echo "search=", var_export(array_search("z.txt", scandir("sd")), true), "\n";
echo "filter=", count(array_filter(scandir("sd"), fn($f) => $f !== ".")), "\n";
unlink("sd/z.txt"); unlink("sd/a.txt"); rmdir("sd");
"#,
    );
    assert_eq!(
        out,
        ".,..,a.txt,z.txt\nz.txt,a.txt,..,.\n.,..,a.txt,z.txt\nin=true\ns0=a.txt\nvals=4\nmap=A.TXT\nsearch=3\nfilter=3\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a runtime `false` flowing into an array-taking builtin raises php's TypeError.
///
/// The message is composed at compile time from php's own parameter naming — measured, not
/// derived — and the throw happens at the argument, before the consumer's lowering ever sees
/// the value. `sort($d)` exercises the by-reference spelling of the same contract, and the
/// variadic tail is asserted through `array_merge`'s SECOND argument, which php words with no
/// parameter name at all. `array_merge`'s FIRST argument is nameless too — the builtin is
/// fully variadic — where `array_values` names its `$array`.
#[test]
fn test_an_array_or_false_union_argument_throws_phps_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$d = @scandir("/no/such/dir");
try {
    in_array("x", $d);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    sort($d);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    array_merge([], $d);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    array_merge($d, []);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo "alive\n";
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "in_array(): Argument #2 ($haystack) must be of type array, false given\n\
         sort(): Argument #1 ($array) must be of type array, false given\n\
         array_merge(): Argument #2 must be of type array, false given\n\
         array_merge(): Argument #1 must be of type array, false given\nalive\n"
    );
}

/// Verifies the argument TypeError stays INSIDE its `try` for a pure array-returning builtin.
///
/// Two optimizer decisions used to break this: claiming purity for `array_values` let the
/// try-prefix hoist move `$y = array_values($d)` ABOVE the handler push — the bytes were
/// right, but the TypeError reported itself uncaught — and let dead-code elimination drop an
/// unused `array_merge([], $d)` call together with the throw php still performs. Both are
/// pinned here: the assignment's throw must land in ITS catch, and the discarded call must
/// still raise.
#[test]
fn test_the_union_type_error_is_catchable_and_survives_a_discarded_result() {
    let out = compile_and_run_capture(
        r#"<?php
$d = @scandir("/no/such/dir");
try {
    $y = array_values($d);
    var_dump($y);
} catch (TypeError $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
try {
    array_diff([], $d);
} catch (TypeError $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
echo "alive\n";
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "caught: array_values(): Argument #1 ($array) must be of type array, false given\n\
         caught: array_diff(): Argument #2 must be of type array, false given\nalive\n"
    );
}

/// Verifies `array_reverse()` on a STRING array — literal and through the scandir union.
///
/// String slots are 16-byte (ptr, len) descriptors, and the shared 8-byte gate refused them
/// since it existed: `array_reverse(["a","b"])` failed to compile on plain literals. The
/// string variant re-persists each element into the new array, so the result owns its bytes
/// and the source's lifetime asks no aliasing questions.
#[test]
fn test_array_reverse_on_a_string_array_matches_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo implode(",", array_reverse(["a", "b", "c"])), "|";
mkdir("rvd");
file_put_contents("rvd/a.txt", "1");
file_put_contents("rvd/z.txt", "2");
echo implode(",", array_reverse(scandir("rvd")));
unlink("rvd/a.txt"); unlink("rvd/z.txt"); rmdir("rvd");
"#,
    );
    assert_eq!(out, "c,b,a|z.txt,a.txt,..,.");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_diff()` and `array_intersect()` on STRING arrays.
///
/// Both refused string arrays at the shared 8-byte gate — plain literals included — which is
/// what kept `array_diff(scandir($d), [".", ".."])`, the most ordinary directory idiom in
/// PHP, from compiling. One parameterised string helper serves both operations (the loop is
/// identical, only the keep-on-match sense differs), comparing through `__rt_str_eq` and
/// re-persisting survivors.
///
/// The survivors also keep their SOURCE keys, as php does, so the KEYS are asserted beside the
/// values: `implode()` alone cannot tell `{0:"a", 2:"c"}` from the reindexed `["a", "c"]` that
/// this family used to return, and the keys are the half that was wrong.
#[test]
fn test_string_array_set_operations_keep_the_right_values()
{
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo implode(",", array_diff(["a", "b", "c"], ["b"])), "|";
echo implode(",", array_intersect(["a", "b", "c"], ["b", "c", "z"])), "|";
echo json_encode(array_diff(["a", "b", "c"], ["b"])), "|";
echo json_encode(array_intersect(["a", "b", "c"], ["b", "c", "z"])), "|";
mkdir("sod");
file_put_contents("sod/a.txt", "1");
file_put_contents("sod/b.txt", "2");
echo implode(",", array_diff(scandir("sod"), [".", ".."])), "|";
echo json_encode(array_diff(scandir("sod"), [".", ".."]));
unlink("sod/a.txt"); unlink("sod/b.txt"); rmdir("sod");
"#,
    );
    assert_eq!(
        out,
        "a,c|b,c|{\"0\":\"a\",\"2\":\"c\"}|{\"1\":\"b\",\"2\":\"c\"}|a.txt,b.txt|{\"2\":\"a.txt\",\"3\":\"b.txt\"}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_merge()` on STRING arrays, empty-literal mixes included.
///
/// php reindexes list keys on merge, which is exactly what two append loops produce — unlike
/// the set operations, there is no key divergence here. An empty literal carries a
/// `Never`-element type whose declared slot size is moot at length zero, so it rides along
/// with a string side rather than failing the one-common-layout rule.
#[test]
fn test_array_merge_on_string_arrays_matches_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo implode(",", array_merge(["a", "b"], ["c"])), "|";
echo implode(",", array_merge([], ["x", "y"])), "|";
mkdir("mgd");
file_put_contents("mgd/f.txt", "1");
echo implode(",", array_merge(scandir("mgd"), ["extra"]));
unlink("mgd/f.txt"); rmdir("mgd");
"#,
    );
    assert_eq!(out, "a,b,c|x,y|.,..,f.txt,extra");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_filter($array)` with NO callback, which php defines as "keep the truthy".
///
/// php-src declares `array_filter(array $array, ?callable $callback = null, int $mode = 0)`,
/// so one argument is valid — but the registry carried `min_args: 2`, reproducing a legacy
/// check arm that refused `array_filter($rows)` outright at compile time. The implicit
/// predicate carries the callback wrapper's own ABI, so the existing filter loops drive it
/// unchanged rather than a second loop being grown alongside them.
///
/// The string cases are the ones worth pinning: php's ONLY falsy strings are `""` and `"0"`,
/// so `"00"` and `"0.0"` survive. An explicit `null` callback is asserted too — php accepts it
/// identically to omitting the argument.
#[test]
fn test_array_filter_without_a_callback_keeps_phps_truthy_values() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo implode(",", array_filter(["a", "", "c", "0", "00", "0.0"])), "|";
echo implode(",", array_filter([1, 0, 2, -1])), "|";
echo implode(",", array_filter(["x", "y"], null)), "|";
echo count(array_filter([])), "|";
mkdir("fld");
file_put_contents("fld/f.txt", "1");
echo implode(",", array_filter(scandir("fld")));
unlink("fld/f.txt"); rmdir("fld");
"#,
    );
    assert_eq!(out, "a,c,00,0.0|1,2,-1|x,y|0|.,..,f.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `natsort()` and `natcasesort()` on STRING arrays against php's natural order.
///
/// The comparator is a from-scratch `strnatcmp_ex`: whitespace skipped before a field
/// ("a 3" sits between "a2" and "a10"), digit runs compared as integers ("img2" before
/// "img10"), a leading zero turning the field fractional ("a002" before "a01" before "a1",
/// "1.002" < "1.010" < "1.02"), and case folded UP for the case-insensitive spelling —
/// `strnatcasecmp("_", "x")` is +1 in php, which only toupper reproduces. Every line is
/// php's measured output.
///
/// Values only, through `implode()`, which reads php's ITERATION order — the half of the
/// answer the comparator decides. These receivers start as indexed arrays and are now
/// promoted to int-keyed hashes so the permuted KEYS survive too; that half is pinned by
/// `tests/codegen/arrays/key_sort.rs`, which is also why these expectations did not move
/// when the promotion landed.
#[test]
fn test_natsort_on_string_arrays_matches_php() {
    let out = compile_and_run_capture(
        r#"<?php
$a = ["a01", "a1", "a2", "a002", "a 3", "a10"];
natsort($a);
echo implode(",", $a), "\n";
$b = ["1.002", "1.02", "1.1", "1.010"];
natsort($b);
echo implode(",", $b), "\n";
$c = ["B", "a", "C", "b", "A_x", "Ax"];
natcasesort($c);
echo implode(",", $c), "\n";
$d = ["x", "", " ", "x1y10", "x1y9", "x01y2"];
natsort($d);
echo implode(",", $d), "\n";
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "a002,a01,a1,a2,a 3,a10\n1.002,1.010,1.02,1.1\na,Ax,A_x,B,b,C\n, ,x,x01y2,x1y9,x1y10\n"
    );
}

/// Verifies `shuffle()` on a STRING array permutes whole descriptors.
///
/// The 8-byte swap would tear each 16-byte `(ptr, len)` slot in half, pairing one string's
/// pointer with another's length. Randomness cannot be pinned, so the assertion is the
/// invariant a torn descriptor breaks: after shuffling, sorting must recover exactly the
/// original elements.
#[test]
fn test_shuffle_on_a_string_array_keeps_every_descriptor() {
    let out = compile_and_run_capture(
        r#"<?php
$s = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
shuffle($s);
echo count($s), "\n";
sort($s);
echo implode(",", $s), "\n";
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "6\nalpha,bravo,charlie,delta,echo,foxtrot\n"
    );
}

/// Verifies `array_slice()` on STRING arrays across php's whole window grammar.
///
/// The window arithmetic is the shared `emit_slice_bounds` prologue every slice helper
/// inlines, so the cases pin the string variant against the same negative-offset,
/// negative-length, omitted-length, and past-the-end rules the scalar helpers honour —
/// measured against php: `b,c|b,c|b,c,d|b,c|` for the literal spellings.
#[test]
fn test_array_slice_on_a_string_array_matches_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$a = ["a", "b", "c", "d"];
echo implode(",", array_slice($a, 1, 2)), "|";
echo implode(",", array_slice($a, -3, 2)), "|";
echo implode(",", array_slice($a, 1)), "|";
echo implode(",", array_slice($a, 1, -1)), "|";
echo implode(",", array_slice($a, 10)), "|";
mkdir("sld");
file_put_contents("sld/f.txt", "1");
echo implode(",", array_slice(scandir("sld"), 2));
unlink("sld/f.txt"); rmdir("sld");
"#,
    );
    assert_eq!(out, "b,c|b,c|b,c,d|b,c||f.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_unique()` on STRING arrays keeps the first occurrences, in order.
///
/// The inner loop compares the candidate against the source's own `0..i` prefix, which keeps
/// php's FIRST occurrence. Each survivor also keeps its SOURCE key, as php does, so the KEYS are
/// asserted beside the values: `implode()` alone cannot tell `{0:"a", 1:"b", 3:"c"}` from the
/// reindexed `["a", "b", "c"]` this returned before, and the keys are the half that was wrong.
#[test]
fn test_array_unique_on_a_string_array_keeps_first_occurrences() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo implode(",", array_unique(["a", "b", "a", "c", "b"])), "|";
echo json_encode(array_unique(["a", "b", "a", "c", "b"])), "|";
mkdir("uqd");
file_put_contents("uqd/f.txt", "1");
echo implode(",", array_unique(array_merge(scandir("uqd"), scandir("uqd")))), "|";
echo json_encode(array_unique(array_merge(scandir("uqd"), scandir("uqd"))));
unlink("uqd/f.txt"); rmdir("uqd");
"#,
    );
    assert_eq!(
        out,
        // The merged scandir case keeps keys 0..2, contiguous from zero, so `json_encode` renders
        // it as a JSON array — the deduplication dropped only the second copy's keys 3..5.
        "a,b,c|{\"0\":\"a\",\"1\":\"b\",\"3\":\"c\"}|.,..,f.txt|[\".\",\"..\",\"f.txt\"]"
    );
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

/// Verifies every `glob()` flag php exposes, against the behaviour php was measured to have.
///
/// php 8.5 ships its own glob, so `GLOB_*` are php's numbers and the bits of NO libc: php's
/// `GLOB_NOESCAPE` is 4096, which macOS's own glob.h defines as `GLOB_LIMIT`, and glibc agrees
/// with php on `GLOB_NOCHECK` alone. `__rt_glob` translates each one to the platform's bit, so a
/// flag that reached libc untranslated would show up here as the WRONG behaviour rather than as
/// an error — which is exactly why each flag is exercised for its own visible effect.
#[test]
fn test_glob_flags_match_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gf");
mkdir("gf/sub");
file_put_contents("gf/a.txt", "a");
file_put_contents("gf/c.log", "c");

// GLOB_MARK appends a slash to directories, and to nothing else.
echo implode(",", glob("gf/*", GLOB_MARK)), "\n";
// GLOB_ONLYDIR keeps directories only. It is php's private 1 << 30 and no libc bit.
echo implode(",", glob("gf/*", GLOB_ONLYDIR)), "\n";
// It applies AFTER the rest: the pattern GLOB_NOCHECK invents is filtered out too.
echo implode(",", glob("gf/*", GLOB_MARK | GLOB_ONLYDIR)), "\n";
echo implode(",", glob("gf/zz*", GLOB_NOCHECK)), "\n";
echo count(glob("gf/zz*", GLOB_NOCHECK | GLOB_ONLYDIR)), "\n";
// GLOB_BRACE expands the alternatives; without it the braces are literal.
echo implode(",", glob("gf/{a.txt,c.log}", GLOB_BRACE)), "\n";
echo count(glob("gf/{a.txt,c.log}")), "\n";
// GLOB_NOSORT still finds everything; only the order is the filesystem's.
$unsorted = glob("gf/*", GLOB_NOSORT);
sort($unsorted);
echo implode(",", $unsorted), "\n";
// A flag held in a variable travels the same path as a constant.
$flags = GLOB_ONLYDIR;
echo implode(",", glob("gf/*", $flags)), "\n";

unlink("gf/a.txt");
unlink("gf/c.log");
rmdir("gf/sub");
rmdir("gf");
"#,
    );
    assert_eq!(
        out,
        "gf/a.txt,gf/c.log,gf/sub/\n\
         gf/sub\n\
         gf/sub/\n\
         gf/zz*\n\
         0\n\
         gf/a.txt,gf/c.log\n\
         0\n\
         gf/a.txt,gf/c.log,gf/sub\n\
         gf/sub\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `glob()` refuses a flag php does not expose: a warning, and `false`.
///
/// The values here are the point. 1024 is glibc's own `GLOB_BRACE` and 8192 is macOS's own
/// `GLOB_NOESCAPE`; php answers `false` to both on every platform, because neither is one of
/// php's bits. A runtime that forwarded `$flags` to libc unchanged would ACCEPT them and do
/// something — silently, and differently on each target.
#[test]
fn test_glob_refuses_a_flag_php_does_not_expose() {
    let out = compile_and_run_capture(
        r#"<?php
mkdir("gr");
file_put_contents("gr/a.txt", "a");
var_dump(glob("gr/*", 64));
var_dump(glob("gr/*", 1024));
var_dump(glob("gr/*", 8192));
var_dump(glob("gr/*", -1));
var_dump(@glob("gr/*", 64));
var_dump(count(glob("gr/*", GLOB_AVAILABLE_FLAGS)));
unlink("gr/a.txt");
rmdir("gr");
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nint(0)\n",
        "every refused call answers php's false, and GLOB_AVAILABLE_FLAGS is accepted"
    );
    let expected = "Warning: glob(): At least one of the passed flags is invalid or not \
                    supported on this platform\n";
    assert_eq!(
        out.diagnostics,
        expected.repeat(4),
        "four warnings: the fifth call is suppressed with @, the sixth is a valid flag set"
    );
}

/// Verifies `glob()` keeps working while the plain-files wrapper is unregistered — with a flag.
///
/// php routes `glob()` through no stream wrapper, so `stream_wrapper_unregister("file")` does not
/// stop it. That is why the `GLOB_ONLYDIR` filter calls `__rt_is_dir_core` and not `__rt_is_dir`:
/// the latter carries the refusal, and reusing it would have made every entry test as "not a
/// directory" and quietly emptied the listing.
#[test]
fn test_glob_onlydir_survives_the_unregistered_file_wrapper() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gu");
mkdir("gu/sub");
file_put_contents("gu/a.txt", "a");
stream_wrapper_unregister("file");
echo implode(",", glob("gu/*", GLOB_ONLYDIR)), "|";
echo implode(",", glob("gu/*")), "|";
stream_wrapper_restore("file");
echo is_dir("gu/sub") ? "restored" : "still blocked";
unlink("gu/a.txt");
rmdir("gu/sub");
rmdir("gu");
"#,
    );
    assert_eq!(out, "gu/sub|gu/a.txt,gu/sub|restored");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies tempnam creates a unique file in the given directory and that it
/// exists immediately, then cleans up the temporary file.
#[test]
fn test_glob_stream_wrapper_iterates_matches() {
    // opendir("glob://pattern") returns a synthetic directory resource backed by libc glob;
    // readdir iterates the matches, closedir releases the gl_pathv, rewinddir restarts the
    // iteration. libc glob returns the matches in sorted order on every target we support.
    //
    // The NAME, not the path the pattern matched: this pinned `gw/a.txt` and php answers
    // `a.txt` — MEASURED on `php -n` 8.5.6 with this very program. The directory the pattern
    // named is the caller's already, so carrying it back made `dirname . "/" . readdir()` build
    // `gw/gw/a.txt`.
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
    assert_eq!(out, "a.txt|b.txt|end|a.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// `scandir()` lists a `glob://` directory, because php's `scandir()` IS opendir + readdir.
///
/// MEASURED on `php -n` 8.5.6 with this program: `array(2) { "a.txt", "b.txt" }` for the default
/// order and the reverse for `SCANDIR_SORT_DESCENDING`. elephc went straight to the filesystem
/// and answered `Warning: scandir(glob://g/*.txt): Failed to open directory` then `false` —
/// `opendir()` had had a `glob://` arm since it was written and `scandir()` never grew one.
#[test]
fn test_glob_stream_wrapper_lists_through_scandir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gs");
file_put_contents("gs/b.txt", "1");
file_put_contents("gs/a.txt", "2");
file_put_contents("gs/c.log", "3");
echo implode(",", scandir("glob://gs/*.txt")), "|";
echo implode(",", scandir("glob://gs/*.txt", SCANDIR_SORT_DESCENDING)), "|";
$none = scandir("glob://gs/*.txt", SCANDIR_SORT_NONE);
sort($none);
echo implode(",", $none);
unlink("gs/a.txt");
unlink("gs/b.txt");
unlink("gs/c.log");
rmdir("gs");
"#,
    );
    assert_eq!(out, "a.txt,b.txt|b.txt,a.txt|a.txt,b.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// A `glob://` pattern that matches nothing OPENS; it is an empty directory, not a failure.
///
/// MEASURED on `php -n` 8.5.6: `opendir("glob://ge/*.nope")` answers a handle whose first
/// `readdir()` is false, `scandir()` of it is `array(0)`, and both hold for a pattern naming a
/// directory that does not exist either. elephc answered `false` to all of them — libc `glob()`
/// reports `GLOB_NOMATCH` and the opener bailed on any non-zero return, while `__rt_glob` (the
/// `glob()` builtin) had always read the same return as an empty list.
#[test]
fn test_glob_stream_wrapper_opens_a_pattern_that_matches_nothing() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("ge");
file_put_contents("ge/a.txt", "1");
$h = opendir("glob://ge/*.nope");
echo ($h === false ? "false" : "open"), "|";
echo (readdir($h) === false ? "end" : "x"), "|";
closedir($h);
echo count(scandir("glob://ge/*.nope")), "|";
echo count(scandir("glob://nosuchdir/*.txt")), "|";
$m = opendir("glob://nosuchdir/*.txt");
echo ($m === false ? "false" : "open");
closedir($m);
unlink("ge/a.txt");
rmdir("ge");
"#,
    );
    assert_eq!(out, "open|end|0|0|open");
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

/// A path that cannot be stat'd answers `false`, not a reading of zero.
///
/// The old expectation named the defect: `float(0)` is a legitimate reading for a full filesystem,
/// so `disk_free_space($d) === false` never fired and arithmetic on the result silently used zero.
/// PHP returns `float|false` and this is the `false`.
#[test]
fn test_disk_free_space_invalid_path_is_false() {
    let out = compile_and_run(r#"<?php var_dump(disk_free_space("/no/such/path/xyz123"));"#);
    assert_eq!(out, "bool(false)\n");
}

/// `dir()` and the `Directory` it returns were both ABSENT; a program calling either failed to
/// compile with "Undefined function"/"Undefined class".
///
/// php's `dir(string $directory, $context = null): Directory|false` is the only way to obtain a
/// `Directory`, whose surface is `$path`, `$handle`, `read(): string|false`, `rewind(): void` and
/// `close(): void`. Measured on php 8.5.6, including the three shapes a naive implementation gets
/// wrong: the listing is `readdir()`'s own order and NOT sorted, `rewind()`/`close()` answer null
/// rather than a boolean, and the class refuses direct construction with `Error: Cannot directly
/// construct Directory, use dir() instead` — a wording a private constructor cannot produce.
///
/// `$context` is accepted and ignored, as `mkdir()`/`opendir()` already accept it.
#[test]
fn test_dir_returns_a_directory_object_matching_php() {
    let out = compile_and_run(
        r#"<?php
$base = "elephc_dir_surface";
@mkdir($base);
file_put_contents("$base/a.txt", "x");

$d = dir($base);
var_dump($d instanceof Directory);
var_dump(get_class($d));
echo "path=", $d->path, "\n";
var_dump(is_resource($d->handle));

$viaObj = [];
while (($e = $d->read()) !== false) { $viaObj[] = $e; }
$d->close();
$h = opendir($base);
$viaFn = [];
while (($e = readdir($h)) !== false) { $viaFn[] = $e; }
closedir($h);
var_dump($viaObj === $viaFn);
var_dump(count($viaObj));

$d = dir($base);
$first = $d->read();
$d->read();
var_dump($d->rewind());
var_dump($d->read() === $first);
var_dump($d->close());

var_dump(@dir("$base/nope"));

try { new Directory(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }

$ctx = stream_context_create([]);
$d = dir($base, $ctx);
var_dump($d instanceof Directory);
$d->close();
$d = dir($base, null);
var_dump($d instanceof Directory);
$d->close();

unlink("$base/a.txt");
rmdir($base);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nstring(9) \"Directory\"\npath=elephc_dir_surface\nbool(true)\n\
         bool(true)\nint(3)\n\
         NULL\nbool(true)\nNULL\n\
         bool(false)\n\
         Error: Cannot directly construct Directory, use dir() instead\n\
         bool(true)\nbool(true)\n",
        "the whole Directory surface, readdir-ordered and with php's void returns"
    );
}

/// Verifies invalid disk-space paths return strict PHP `false`, not a successful `0.0` byte count.
#[test]
fn test_disk_space_invalid_path_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
var_dump(disk_free_space("/no/such/path/xyz123"));
var_dump(disk_total_space("/no/such/path/xyz123"));
echo disk_free_space("/no/such/path/xyz123") === false ? "strict" : "!";
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nstrict");
}

/// Verifies disk-space failures keep PHP boolean ordering against negative numbers.
/// Casting the boxed `false` to an integer would make both relational and spaceship
/// results point in the opposite direction because PHP compares a boolean operand by truthiness.
#[test]
fn test_disk_space_invalid_path_uses_false_ordering_rules() {
    let out = compile_and_run(
        r#"<?php
$free = disk_free_space("/no/such/path/xyz123");
$total = disk_total_space("/no/such/path/xyz123");
var_dump($free > -1);
var_dump($free < -1);
var_dump($free <=> -1);
var_dump(-1 <=> $free);
var_dump($total > -1);
var_dump($total <=> -1);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nint(-1)\nint(1)\nbool(false)\nint(-1)\n"
    );
}

/// A program that owns the names itself keeps them — a DELIBERATE divergence, pinned as one.
///
/// php refuses: `Fatal error: Cannot redeclare function dir()`, because `dir` and `Directory` are
/// its own. elephc accepts, because the prelude is pay-for-use and injecting it unconditionally is
/// the only way to reproduce php's fatal — which would also break every program that today owns
/// these very ordinary names and compiles fine. Over-acceptance was judged the safer half of the
/// trade, and this test exists so the choice is visible rather than incidental: if the prelude ever
/// becomes unconditional, this test fails and the divergence note has to be revisited.
#[test]
fn test_user_declared_dir_and_directory_keep_their_own_definitions() {
    let out = compile_and_run(
        r#"<?php
class Directory {
    public string $label;
    public function __construct(string $label) { $this->label = $label; }
    public function describe(): string { return "user:" . $this->label; }
}
function dir(string $path): string { return "mine:" . $path; }
echo dir("/tmp"), "\n";
$d = new Directory("own");
echo $d->describe(), "\n";
var_dump(function_exists("dir"), class_exists("Directory"));
"#,
    );
    assert_eq!(
        out,
        "mine:/tmp\nuser:own\nbool(true)\nbool(true)\n",
        "the user's own dir()/Directory win; php would have refused the program outright"
    );
}

/// `opendir()`, `copy()` and `scandir()` refused php's `$context` argument outright.
///
/// All three document one, and passing it was a COMPILE error on a signature php accepts — the
/// same trap `unlink()`/`mkdir()`/`rmdir()` were already fixed for. The argument is accepted and
/// ignored: elephc has no context plumbing on these routes, and a null context behaves the same.
/// Measured on php 8.5.6.
#[test]
fn test_opendir_copy_and_scandir_accept_phps_context_argument() {
    let out = compile_and_run(
        r#"<?php
$base = "elephc_ctxprobe";
@mkdir($base);
file_put_contents("$base/a.txt", "x");
$ctx = stream_context_create(["http" => ["method" => "GET"]]);
$h = opendir($base, $ctx);
var_dump(is_resource($h));
closedir($h);
$h = opendir($base, null);
var_dump(is_resource($h));
closedir($h);
var_dump(copy("$base/a.txt", "$base/b.txt", $ctx));
var_dump(copy("$base/a.txt", "$base/c.txt", null));
var_dump(file_exists("$base/b.txt"), file_exists("$base/c.txt"));
$list = scandir($base, SCANDIR_SORT_ASCENDING, $ctx);
echo implode(",", $list), "\n";
$list = scandir($base, SCANDIR_SORT_ASCENDING, null);
echo implode(",", $list), "\n";
unlink("$base/a.txt"); unlink("$base/b.txt"); unlink("$base/c.txt");
rmdir($base);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n\
         .,..,a.txt,b.txt,c.txt\n.,..,a.txt,b.txt,c.txt\n",
        "all three accept the context php documents, including a null one"
    );
}

/// `readdir()` answers php's `string|false`, not `string|bool`.
///
/// The registered union was the wider `Str|Bool`, so a function DECLARED with php's own
/// `string|false` return could not hand back what `readdir()` gave it —
/// `Method 'D::read' return type expects Union([Str, False]), got Union([Str, Bool])` — on a
/// signature php accepts. `readlink()` and `ob_get_clean()` already used the narrow form. This is
/// what `Directory::read()` needs, so the prelude pins it too.
#[test]
fn test_readdir_returns_string_or_false_not_string_or_bool() {
    let out = compile_and_run(
        r#"<?php
$base = "elephc_readdir_type";
@mkdir($base);
function first_entry($handle): string|false {
    return readdir($handle);
}
$h = opendir($base);
$e = first_entry($h);
closedir($h);
var_dump(is_string($e));
rmdir($base);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// `readdir()` / `rewinddir()` / `closedir()` refused php's OPTIONAL `$dir_handle`.
///
/// php's signature is `readdir(?resource $dir_handle = null)`: with no argument — or with an
/// explicit `null` — it operates on the LAST directory stream opened by `opendir()` (or by
/// `dir()`, which is built on it). `fopen()` does not participate. elephc required the argument,
/// so this very ordinary shape failed to COMPILE.
///
/// MEASURED on `php -n` 8.5.6, stdout with the notices suppressed:
///
/// ```text
/// bool(true)          the handle-less listing IS the handle's own listing
/// int(3)
/// bool(true)          readdir() after rewinddir(), both handle-less
/// bool(true)          an intervening fopen() does NOT move the directory slot
/// TypeError: No resource supplied      after the slot's stream was closed
/// bool(true)          dir() feeds the same slot
/// TypeError: No resource supplied      rewinddir(), slot closed by Directory::close()
/// TypeError: No resource supplied      closedir(), same
/// bool(true)          an explicit null takes the same route
/// ```
///
/// Two wordings are pinned, both verbatim from php-src. The notice is `Deprecated: <fn>():
/// Passing null is deprecated, instead the last opened directory stream should be provided`, and
/// it fires when the argument is OMITTED just as much as when `null` is passed explicitly — php
/// fills the default in before the deprecation check, so the two calls are indistinguishable. The
/// refusal carries NO function prefix at all: it is the bare `No resource supplied`, which is why
/// it cannot reuse the `<fn>(): Argument #1 ($stream) must be an open stream resource` text every
/// other closed-handle guard emits.
///
/// `@` suppresses the notice, so the loop stays readable; one unsuppressed call at the end pins
/// the wording.
#[test]
fn test_readdir_family_accepts_phps_optional_last_opened_directory_handle() {
    let out = compile_and_run_capture(
        r#"<?php
$base = "elephc_lastdir";
@mkdir($base);
file_put_contents("$base/a.txt", "x");

$d = opendir($base);
$viaSlot = [];
while (($e = @readdir()) !== false) { $viaSlot[] = $e; }
@rewinddir();
$viaHandle = [];
while (($e = readdir($d)) !== false) { $viaHandle[] = $e; }
var_dump($viaSlot === $viaHandle);
var_dump(count($viaSlot));

@rewinddir();
var_dump(@readdir() !== false);
$f = fopen("$base/a.txt", "r");
var_dump(@readdir() !== false);
fclose($f);
@closedir();
try { @readdir(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }

$o = dir($base);
var_dump(@readdir() !== false);
$o->close();
try { @rewinddir(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }
try { @closedir(); } catch (Throwable $t) { echo get_class($t), ": ", $t->getMessage(), "\n"; }

$d2 = opendir($base);
var_dump(readdir(null) !== false);
closedir($d2);

unlink("$base/a.txt");
rmdir($base);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "bool(true)\n",
            "int(3)\n",
            "bool(true)\n",
            "bool(true)\n",
            "TypeError: No resource supplied\n",
            "bool(true)\n",
            "TypeError: No resource supplied\n",
            "TypeError: No resource supplied\n",
            "bool(true)\n",
        ),
        "the handle-less family follows the last opendir()/dir(), and refuses once it closed"
    );
    assert_eq!(
        out.diagnostics,
        "Deprecated: readdir(): Passing null is deprecated, instead the last opened \
         directory stream should be provided\n",
        "an explicit null is deprecated with php's own wording, and `@` silences the rest"
    );
}

/// Verifies a successful `disk_free_space()` still behaves as a float after the return type
/// widened to `float|false`.
///
/// Declaring a union changes how a SUCCESSFUL value is carried, not just what it can be, so the
/// success side needs its own guard: `is_float()`, an arithmetic use, and the ordering against
/// `disk_total_space()` that any real caller depends on.
#[test]
fn test_disk_space_success_still_behaves_as_a_float() {
    let out = compile_and_run(
        r#"<?php
$free = disk_free_space(".");
$total = disk_total_space(".");
echo is_float($free) ? "f" : "!";
echo is_float($total) ? "t" : "!";
echo $free > 0 ? "p" : "!";
echo $total >= $free ? "o" : "!";
echo ($free + 1.0) > $free ? "a" : "!";
"#,
    );
    assert_eq!(out, "ftpoa");
}
