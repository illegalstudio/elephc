//! Purpose:
//! Integration tests for php's internal-function behaviour on the stream preludes and the
//! directory family: the name and line a delegating prelude's warning carries, the `Directory`
//! object's refusals, the directory family's `TypeError` for a stream that is not a directory, and
//! the notification-callback validation of the context functions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `gzopen()` and `dir()` are preludes over `fopen()`/`opendir()`, and php names ITS internal
//!   function in their warnings, on the caller's line. Before, elephc named `fopen`/`opendir` and
//!   a line of the prelude (`1000000` for a built body, `31` for the old PHP text).
//! - Every expectation was measured on `php -n` 8.5.10.

use crate::support::*;

/// Verifies a delegating prelude's open failure names the prelude and the caller's line.
#[test]
fn test_delegating_prelude_warnings_name_the_builtin_on_the_callers_line() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(gzopen("missing.gz", "r"));
var_dump(dir("missing_dir"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
    let lines: Vec<&str> = out.located_diagnostics.lines().collect();
    assert_eq!(lines.len(), 2, "{}", out.located_diagnostics);
    assert!(
        lines[0].starts_with("Warning: gzopen(missing.gz): Failed to open stream: No such file or directory in ")
            && lines[0].ends_with(" on line 2"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("Warning: dir(missing_dir): Failed to open directory: No such file or directory in ")
            && lines[1].ends_with(" on line 3"),
        "{}",
        lines[1]
    );
}

/// Verifies `Directory` refuses cloning, serialization and use after `close()`, as php does.
#[test]
fn test_directory_objects_refuse_what_php_refuses() {
    let out = compile_and_run(
        r#"<?php
mkdir("listed");
$d = dir("listed");
try { $c = clone $d; echo "cloned\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { echo serialize($d), "\n"; } catch (Exception $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$d->close();
foreach (["read", "rewind", "close"] as $m) {
    try { $d->$m(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
rmdir("listed");
"#,
    );
    assert_eq!(
        out,
        "Error: Trying to clone an uncloneable object of class Directory\n\
Exception: Serialization of 'Directory' is not allowed\n\
Directory::read(): cannot use Directory resource after it has been closed\n\
Directory::rewind(): cannot use Directory resource after it has been closed\n\
Directory::close(): cannot use Directory resource after it has been closed\n"
    );
}

/// Verifies builtin classes php declares uncloneable refuse `clone`, naming the class held.
#[test]
fn test_uncloneable_builtin_classes_refuse_clone() {
    let out = compile_and_run(
        r#"<?php
class MyException extends Exception {}
$r = new ReflectionClass("stdClass");
try { $c = clone $r; echo "cloned\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
$x = new MyException("m");
try { $y = clone $x; echo "cloned\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
$o = new ArrayObject([1]);
$p = clone $o;
echo count($p), "\n";
"#,
    );
    assert_eq!(
        out,
        "Trying to clone an uncloneable object of class ReflectionClass\n\
Trying to clone an uncloneable object of class MyException\n\
1\n"
    );
}

/// Verifies the directory family refuses a stream that is not a directory with php's TypeError.
#[test]
fn test_directory_family_refuses_a_file_stream() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("plain.txt", "x");
$f = fopen("plain.txt", "r");
foreach (["readdir", "rewinddir", "closedir"] as $fn) {
    try { $fn($f); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
echo is_resource($f) ? "still open\n" : "closed\n";
fclose($f);
unlink("plain.txt");
"#,
    );
    assert_eq!(
        out,
        "readdir(): Argument #1 ($dir_handle) must be a valid Directory resource\n\
rewinddir(): Argument #1 ($dir_handle) must be a valid Directory resource\n\
closedir(): Argument #1 ($dir_handle) must be a valid Directory resource\n\
still open\n"
    );
}

/// Verifies an uncallable `notification` param is php's TypeError, in each shape's own words.
///
/// A STRING is accepted whatever it names: a user function nothing calls is eliminated before
/// code generation, so the run time cannot tell it from a missing one, and refusing it would throw
/// for a program php runs.
#[test]
fn test_stream_context_notification_must_be_callable() {
    let out = compile_and_run(
        r#"<?php
class Plain {}
$ctx = stream_context_create();
foreach ([42, [1, 2], [1, 2, 3], "strlen", new Plain()] as $cb) {
    try {
        var_dump(stream_context_set_params($ctx, ["notification" => $cb]));
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
try { stream_context_create([], ["notification" => 42]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump(stream_context_set_params($ctx, ["notification" => function () {}]));
"#,
    );
    let set = "stream_context_set_params(): Argument #1 ($context) must be an array with valid callbacks as values, ";
    assert_eq!(
        out,
        format!(
            "{set}no array or string given\n\
{set}first array member is not a valid class name or object\n\
{set}array callback must have exactly two members\n\
bool(true)\n\
{set}no array or string given\n\
stream_context_create(): Argument #1 ($options) must be an array with valid callbacks as values, no array or string given\n\
bool(true)\n"
        )
    );
}
