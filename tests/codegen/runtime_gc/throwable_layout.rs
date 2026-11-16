//! Purpose:
//! Regression tests keeping Throwable allocation, declared properties and intrinsic getters
//! coherent: `getFile()`/`getLine()` creation sites for static, dynamic and runtime-included
//! `new`, writes to the protected `$file`/`$line` fields, and `getPrevious()` after those writes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (runtime GC integration suite).
//!
//! Key details:
//! - Protected property writes must not overwrite neighboring heap blocks, so tests read
//!   message, code and previous back after relocating file and line.
//! - Class names chosen with `$argc` keep `new $class` dynamic so constant folding cannot turn
//!   it into a static `new`.

use crate::support::*;

/// Verifies a subclass with an extra declared property and a `new $class` of a built-in
/// exception both record the creating file and line, so `getFile()` equals `__FILE__` and
/// `getLine()` the `new` line. PHP outputs "file:line:file:line".
#[test]
fn test_throwable_layout_declared_and_dynamic_new_record_creation_site() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
class ExtraException extends Exception { public string $detail = 'detail'; }
$line = __LINE__ + 1;
$exception = new ExtraException('message');
echo $exception->getFile() === __FILE__ ? 'file' : 'wrong-file', ':',
    $exception->getLine() === $line ? 'line' : 'wrong-line', ':';
$class = $argc === 1 ? 'ReflectionException' : 'RuntimeException';
$line = __LINE__ + 1;
$dynamic = new $class('dynamic');
echo $dynamic->getFile() === __FILE__ ? 'file' : 'wrong-file', ':',
    $dynamic->getLine() === $line ? 'line' : 'wrong-line';
"#)],
        "main.php",
    );
    assert_eq!(out, "file:line:file:line");
}

/// Verifies a closure bound to an `Exception` subclass scope can write the protected `file` and
/// `line` of a `ReflectionException`, and the intrinsic getters then return the new values while
/// message, code and previous are intact. PHP outputs "message:7:fixture.php:149:none".
#[test]
fn test_throwable_layout_bound_closure_updates_source_fields() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
class ExceptionEditor extends Exception {}
$exception = new ReflectionException('message', 7);
$write = Closure::bind(static function ($value): void {
    $value->file = 'fixture.php';
    $value->line = 149;
}, null, ExceptionEditor::class);
$write($exception);
echo $exception->getMessage(), ':', $exception->getCode(), ':',
    $exception->getFile(), ':', $exception->getLine(), ':',
    $exception->getPrevious() === null ? 'none' : 'previous';
"#)],
        "main.php",
    );
    assert_eq!(out, "message:7:fixture.php:149:none");
}

/// Verifies a subclass method rewriting `$this->file` and `$this->line` leaves the chained
/// previous exception in place: `getPrevious()` is still the same object, whose own message and
/// code are unchanged. PHP outputs "outer:7:fixture.php:149:same:inner:3".
#[test]
fn test_throwable_layout_previous_survives_source_field_writes() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
class ExceptionEditor extends Exception {
    public function relocate(string $file, int $line): void {
        $this->file = $file;
        $this->line = $line;
    }
}
$previous = new ReflectionException('inner', 3);
$exception = new ExceptionEditor('outer', 7, $previous);
$exception->relocate('fixture.php', 149);
echo $exception->getMessage(), ':', $exception->getCode(), ':',
    $exception->getFile(), ':', $exception->getLine(), ':',
    $exception->getPrevious() === $previous ? 'same' : 'different', ':',
    $previous->getMessage(), ':', $previous->getCode();
"#)],
        "main.php",
    );
    assert_eq!(out, "outer:7:fixture.php:149:same:inner:3");
}

/// Verifies a `ReflectionException` thrown by the runtime (for a missing class) has the full
/// Throwable layout: a static subclass method can rewrite its `file` and `line`, and the
/// message and null previous read back correctly. PHP outputs
/// `Class "AbsentLayoutFixture0" does not exist:generated.php:149:none`.
#[test]
fn test_throwable_layout_generated_reflection_error_has_writable_source_fields() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
class ExceptionEditor extends Exception {
    public static function relocate(Exception $value): void {
        $value->file = 'generated.php';
        $value->line = 149;
    }
}
try {
    $missing = 'AbsentLayoutFixture'.($argc - 1);
    new ReflectionClass($missing);
} catch (ReflectionException $exception) {
    ExceptionEditor::relocate($exception);
    echo $exception->getMessage(), ':', $exception->getFile(), ':',
        $exception->getLine(), ':', $exception->getPrevious() === null ? 'none' : 'previous';
}
"#)],
        "main.php",
    );
    assert_eq!(out, "Class \"AbsentLayoutFixture0\" does not exist:generated.php:149:none");
}

/// Verifies twelve rounds of creating a chained exception with built message strings, then
/// replacing its `file` and `line`, release every superseded field and the previous exception:
/// prints "ok" with a clean heap-debug leak summary.
#[test]
fn test_throwable_layout_repeated_owned_fields_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PositionedException extends Exception {
    public function relocate(string $file, int $line): void {
        $this->file = $file;
        $this->line = $line;
    }
}
function exercise(int $count): void {
    for ($i = 0; $i < $count; $i++) {
        $previous = new Exception('inner-'.$i);
        $exception = new PositionedException('outer-'.$i, $i, $previous);
        $exception->relocate('fixture-'.$i, $i + 100);
        if ($exception->getPrevious() !== $previous) { echo 'bad'; }
        unset($exception, $previous);
    }
}
exercise(12);
echo 'ok';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies an exception created in a file included through a variable path (run by the
/// runtime evaluator) records that file and line as its creation site, matching the included
/// file's `__FILE__` and `__LINE__`. PHP outputs "file:line".
#[test]
fn test_throwable_layout_runtime_include_records_creation_site() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", "<?php $file = 'runtime'; include __DIR__.'/'.$file.'.php';"),
            ("runtime.php", r#"<?php
$line = __LINE__ + 1;
$exception = new RuntimeException('runtime');
echo $exception->getFile() === __FILE__ ? 'file' : 'wrong-file', ':',
    $exception->getLine() === $line ? 'line' : 'wrong-line';
"#),
        ],
        "main.php",
    );
    assert_eq!(out, "file:line");
}

/// Verifies `file` and `line` assigned in a custom constructor after `parent::__construct()`
/// survive a dynamic `new $class()`: the creation-site recording must not overwrite them after
/// the constructor returns. PHP outputs "custom:custom.php:88".
#[test]
fn test_throwable_layout_constructor_source_replacements_survive() {
    let out = compile_cli_files_and_run(
        &[("main.php", r#"<?php
class CustomOrigin extends Exception {
    public function __construct() {
        parent::__construct('custom');
        $this->file = 'custom.php';
        $this->line = 88;
    }
}
$class = $argc === 1 ? 'CustomOrigin' : 'Exception';
$exception = new $class();
echo $exception->getMessage(), ':', $exception->getFile(), ':', $exception->getLine();
"#)],
        "main.php",
    );
    assert_eq!(out, "custom:custom.php:88");
}
