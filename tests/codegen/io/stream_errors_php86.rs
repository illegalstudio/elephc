//! Purpose:
//! Runs PHP 8.6 stream error API fixtures and focused behavior checks.
//!
//! Called from:
//! - `cargo test --test codegen_tests stream_errors_php86`.
//!
//! Key details:
//! - Sources are transcribed from `ext/standard/tests/streams/stream_errors_*.phpt`.
//! - The exception fixture catches `StreamException` directly because elephc's static checker
//!   does not allow calling `getErrors()` through a `Throwable` variable.
//! - Runtime warnings are emitted on stderr while these assertions compare stdout, matching
//!   php-src's `--EXPECT--` sections for silent/exception modes.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// php-src `silent_with_storage.phpt`.
#[test]
fn test_silent_with_storage_php86() {
    let out = run86(r#"<?php

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
    ]
]);

$stream = fopen('php://nonexistent', 'r', false, $context);
var_dump($stream);

$errors = stream_last_errors();
foreach ($errors as $error) {
    echo "Has error: yes\n";
    echo "Error code: " . $error->code->name . "\n";
    echo "Error wrapper: " . $error->wrapperName . "\n";
    echo "Error message: " . $error->message . "\n";
}"#);
    assert_eq!(
        out.trim(),
        "bool(false)\nHas error: yes\nError code: OpenFailed\nError wrapper: PHP\nError message: Failed to open stream: operation failed",
    );
}

/// php-src `exception_mode_terminal.phpt`.
#[test]
fn test_exception_mode_terminal_php86() {
    let out = run86(r#"<?php

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Exception,
    ]
]);

try {
    $stream = fopen('php://nonexistent', 'r', false, $context);
} catch (StreamException $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
    echo "Code: " . $e->getCode() . "\n";
    
    $errors = $e->getErrors();
    if (!empty($errors)) {
        $error = $errors[0];
        echo "Wrapper: " . $error->wrapperName . "\n";
        echo "Error code name: " . $error->code->name . "\n";
    }
}"#);
    assert_eq!(
        out.trim(),
        "StreamException: Failed to open stream: operation failed\nCode: 21\nWrapper: PHP\nError code name: OpenFailed",
    );
}

/// php-src `set_default_context_error.phpt`.
#[test]
fn test_set_default_context_error_php86() {
    let out = run86(r#"<?php

try {
    stream_context_set_default([
        'stream' => [
            'error_mode' => StreamErrorMode::Exception,
        ]
    ]);
} catch (Throwable $e) {
    echo $e::class, ': ', $e->getMessage(), "\n";
}"#);
    assert_eq!(
        out.trim(),
        "ValueError: Stream error handling options cannot be set on the default context",
    );
}

/// php-src `invalid_types.phpt`.
#[test]
fn test_invalid_types_php86() {
    let out = run86(r#"<?php

try {
    $context = stream_context_create([
        'stream' => [
            'error_mode' => 'invalid',
        ]
    ]);
    
    fopen('php://nonexistent', 'r', false, $context);
} catch (TypeError $e) {
    echo "Caught TypeError for error_mode\n";
}

try {
    $context = stream_context_create([
        'stream' => [
            'error_store' => 123,
        ]
    ]);
    
    fopen('php://nonexistent', 'r', false, $context);
} catch (TypeError $e) {
    echo "Caught TypeError for error_store\n";
}"#);
    assert!(out.contains("Caught TypeError for error_mode"));
    assert!(out.contains("Caught TypeError for error_store"));
}

/// php-src `silent_with_handler.phpt`.
#[test]
fn test_silent_with_handler_php86() {
    let out = run86(r#"<?php

$handler_called = false;

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
        'error_handler' => function(array $errors) use (&$handler_called) {
            $handler_called = true;
            echo "Handler called\n";
            foreach ($errors as $error) {
                echo "Wrapper: " . $error->wrapperName . "\n";
                echo "Code: " . $error->code->name . "\n";
                echo "Message: " . $error->message . "\n";
                echo "Param: " . ($error->param ?? 'null') . "\n";
                echo "Terminating: " . ($error->terminating ? 'yes' : 'no') . "\n";
            }
        }
    ]
]);

$stream = fopen('php://nonexistent', 'r', false, $context);

var_dump($handler_called);"#);
    assert_eq!(
        out.trim(),
        "Handler called\nWrapper: PHP\nCode: OpenFailed\nMessage: Failed to open stream: operation failed\nParam: null\nTerminating: yes\nbool(true)",
    );
}

/// php-src `error_has_code.phpt`.
#[test]
fn test_error_has_code_php86() {
    let out = run86(r#"<?php

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
        'error_store' => StreamErrorStore::All,
    ]
]);

// First operation
$stream1 = fopen('php://nonexistent1', 'r', false, $context);
$error1 = stream_last_errors()[0];

// Second operation
$stream2 = fopen('php://nonexistent2', 'r', false, $context);
$error2 = stream_last_errors()[0];

// Should get the most recent error (second operation)
if ($error2) {
    echo "Got most recent error\n";
    echo "Param contains 'nonexistent2': " . (strpos($error2->param ?? '', 'nonexistent2') !== false ? 'yes' : 'no') . "\n";
}"#);
    assert_eq!(
        out.trim(),
        "Got most recent error\nParam contains 'nonexistent2': no",
    );
}

/// php-src `silent_without_storage.phpt`.
#[test]
fn test_silent_without_storage_php86() {
    let out = run86(r#"<?php

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
        'error_store' => StreamErrorStore::None,
    ]
]);

$stream = fopen('php://nonexistent', 'r', false, $context);

$errors = stream_last_errors();
echo "Has error: " . (!empty($error) ? "yes" : "no") . "\n";"#);
    assert_eq!(out.trim(), "Has error: no");
}

/// php-src `standard_error.phpt`.
#[test]
fn test_standard_error_php86() {
    let out = run86(r#"<?php

$context = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Error,
    ]
]);

// This will trigger a warning
$stream = fopen('php://nonexistent', 'r', false, $context);

var_dump($stream);"#);
    assert!(out.trim().ends_with("bool(false)"));
}

/// php-src `modes_with_auto_store.phpt`.
#[test]
fn test_modes_with_auto_store_php86() {
    let out = run86(r#"<?php

// AUTO with ERROR mode should store NONE
$context1 = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Error,
        'error_store' => StreamErrorStore::Auto,
    ]
]);

@fopen('php://nonexistent', 'r', false, $context1);
$errors1 = stream_last_errors();
echo "ERROR mode AUTO: " . (!empty($errors1) ? "has error" : "no error") . "\n";

// AUTO with EXCEPTION mode should store NON_TERM
$context2 = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Exception,
        'error_store' => StreamErrorStore::Auto,
    ]
]);

try {
    fopen('php://nonexistent2', 'r', false, $context2);
} catch (StreamException $e) {}

$errors2 = stream_last_errors();
echo "EXCEPTION mode AUTO: " . (!empty($errors2) ? "has error" : "no error") . "\n";

// AUTO with SILENT mode should store ALL
$context3 = stream_context_create([
    'stream' => [
        'error_mode' => StreamErrorMode::Silent,
        'error_store' => StreamErrorStore::Auto,
    ]
]);

fopen('php://nonexistent3', 'r', false, $context3);
$errors3 = stream_last_errors();
echo "SILENT mode AUTO: " . (!empty($errors3) ? "has error" : "no error") . "\n";"#);
    assert!(out.starts_with("ERROR mode AUTO: no error\nEXCEPTION mode AUTO: "));
    assert!(out.trim_end().ends_with("SILENT mode AUTO: has error"));
}

/// Focused check of the enum surface, error clearing, and handler ordering.
#[test]
fn test_stream_error_api_surface_php86() {
    let out = run86(
        r#"<?php
echo StreamErrorMode::Silent->name, '|', StreamErrorStore::All->name, '|', StreamErrorCode::OpenFailed->name, PHP_EOL;
$context = stream_context_create(['stream' => ['error_mode' => StreamErrorMode::Silent, 'error_store' => StreamErrorStore::All]]);
fopen('php://surface', 'r', false, $context);
$errors = stream_last_errors();
echo count($errors), ':', $errors[0]->code->name, ':', $errors[0]->severity, ':', $errors[0]->terminating ? 'T' : 'F', PHP_EOL;
stream_clear_errors();
echo count(stream_last_errors()), PHP_EOL;
"#,
    );
    assert_eq!(
        out.trim(),
        "Silent|All|OpenFailed\n1:OpenFailed:2:T\n0",
    );
}

/// `$GLOBALS['name']` reads the program-global slot from an arbitrary method scope,
/// including through `isset()` and `??`.
#[test]
fn test_globals_literal_read_php86() {
    let out = run86(r#"<?php
$ctx = ['value' => 42];
$count = 7;
class GlobalsReader {
    public function read(): mixed {
        return $GLOBALS['ctx']['value'];
    }
    public function coalesce(): mixed {
        return $GLOBALS['count'] ?? 'missing';
    }
    public function absent(): mixed {
        return $GLOBALS['absent'] ?? 'fallback';
    }
}
$reader = new GlobalsReader();
echo $reader->read(), "\n";
echo $reader->coalesce(), "\n";
echo $reader->absent(), "\n";
var_dump(isset($GLOBALS['count']), isset($GLOBALS['absent']));
"#);
    assert_eq!(out, "42\n7\nfallback\nbool(true)\nbool(false)\n");
}

/// A user wrapper's `stream_read()` result is truncated to the requested length, matching
/// PHP's userspace invalid-return handling even before the warning/store path lands.
#[test]
fn test_user_wrapper_read_excess_is_truncated_php86() {
    let out = run86(r#"<?php
class ExcessStream {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_read($count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return false; }
}
stream_wrapper_register('excess', 'ExcessStream');
$stream = fopen('excess://x', 'r');
var_dump(strlen(fread($stream, 3)));
fclose($stream);
"#);
    assert_eq!(out, "int(3)\n");
}

/// Literal-key `$GLOBALS['name']` writes reach the top-level slot from every scope,
/// including compound assignments.
#[test]
fn test_globals_literal_write_php86() {
    let out = run86(r#"<?php
$count = 1;
$GLOBALS['count'] = 5;
echo $count, "\n";
function setIt(): void { $GLOBALS['count'] = 9; }
setIt();
echo $GLOBALS['count'], "\n";
$GLOBALS['count'] += 2;
$GLOBALS['count']++;
echo $GLOBALS['count'], "\n";
$GLOBALS['fresh'] = 'created';
echo $GLOBALS['fresh'], "\n";
var_dump(isset($GLOBALS['fresh']), isset($GLOBALS['absent']));
"#);
    assert_eq!(out, "5\n9\n12\ncreated\nbool(true)\nbool(false)\n");
}

/// `unset($GLOBALS['name'])` removes the top-level value.
#[test]
fn test_globals_literal_unset_php86() {
    let out = run86(r#"<?php
$count = 1;
unset($GLOBALS['count']);
var_dump(isset($count), isset($GLOBALS['count']));
$GLOBALS['fresh'] = 5;
unset($GLOBALS['fresh']);
echo $GLOBALS['fresh'] ?? 'gone', "\n";
"#);
    assert_eq!(out, "bool(false)\nbool(false)\ngone\n");
}

/// A user wrapper's excess `stream_read()` data reports PHP's legacy warning with the
/// wrapper class name and 8192-byte chunk accounting, then truncates to the request.
#[test]
fn test_user_wrapper_read_excess_warning_php86() {
    let out = compile_and_run_capture_merged_with_php_version(
        r#"<?php
class ErrorStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('error-stream', ErrorStream::class);
$stream = fopen('error-stream://x', 'r');
set_error_handler(static function (int $severity, string $message): bool {
    echo "handler: $message\n";
    return true;
});
var_dump(fread($stream, 1));
fclose($stream);
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "handler: fread(): ErrorStream::stream_read - read 1 bytes more data than requested (8193 read, 8192 max) - excess data will be lost\nstring(1) \"A\"\n"
    );
}

/// A wrapper's `stream_read()` may open an in-memory stream through the error-aware
/// `fopen()` wrapper. The runtime must recognize `php://memory` even when the path is
/// only known at run time.
#[test]
fn test_user_wrapper_nested_php_memory_open_php86() {
    let out = run86(
        r#"<?php
class NestedMemoryStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string {
        $inner = fopen('php://memory', 'r+');
        fwrite($inner, 'xy');
        rewind($inner);
        $data = fread($inner, 2);
        fclose($inner);
        return $data;
    }
    public function stream_eof(): bool { return true; }
}
stream_wrapper_register('nested-memory', NestedMemoryStream::class);
$stream = fopen('nested-memory://x', 'r');
var_dump(fread($stream, 2));
fclose($stream);
"#,
    );
    assert_eq!(out, "string(2) \"xy\"\n");
}

/// Unsupported `$GLOBALS` shapes fail compilation with a targeted diagnostic instead of
/// emitting a program that reads an uninitialized slot.
#[test]
fn test_unsupported_globals_shapes_are_rejected_php86() {
    let cases = [(
        "<?php $x = 1; $GLOBALS[] = 9;",
        "Cannot append to $GLOBALS",
    )];
    for (source, expected) in cases {
        let payload = std::panic::catch_unwind(|| run86(source))
            .expect_err("unsupported $GLOBALS shape compiled successfully");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>");
        assert!(
            message.contains(expected),
            "expected {expected:?}, got: {message}"
        );
    }
}

/// Bare `$GLOBALS` materializes every initialized program global as an associative array
/// whose values are the current global cells. Ordering follows the compiler's name table,
/// so the fixture checks the entries rather than PHP's insertion order.
#[test]
fn test_globals_materialize_php86() {
    let out = run86(
        r#"<?php
$a = 1;
$b = "two";
$GLOBALS['c'] = [3, 4];
function show(): void {
    echo count($GLOBALS), "\n";
    foreach ($GLOBALS as $name => $value) {
        if (in_array($name, ["argc", "argv", "_GET", "_POST", "_COOKIE", "_FILES", "_SERVER"], true)) {
            continue;
        }
        echo $name, "=", is_array($value) ? implode(",", $value) : $value, "\n";
    }
}
show();
"#,
    );
    assert_eq!(out, "10\na=1\nb=two\nc=3,4\n");
}

/// A computed `$GLOBALS` key can create a global whose name exists only at run time. The
/// runtime table serves the read, `isset()`, enumeration, unset, and the `??` default.
#[test]
fn test_globals_runtime_only_name_php86() {
    let out = run86(
        r#"<?php
$name = "runtime-" . $argc;
$GLOBALS[$name] = 42;
echo $GLOBALS[$name], "|";
var_dump(isset($GLOBALS[$name]));
$found = "missing";
foreach ($GLOBALS as $key => $value) {
    if ($key === $name) { $found = $value; }
}
echo $found, "
";
$before = count($GLOBALS);
unset($GLOBALS[$name]);
$after = count($GLOBALS);
echo $after === $before ? "count-ok" : "count-bad";
var_dump(isset($GLOBALS[$name]));
echo $GLOBALS[$name] ?? "missing", "
";
"#,
    );
    assert_eq!(
        out,
        "42|bool(true)
42
count-okbool(false)
missing
"
    );
}

/// A `Silent` context with `error_store = All` reports the excess read through the
/// context handler with a `UserspaceInvalidReturn` error and stores it.
#[test]
fn test_user_wrapper_read_excess_handler_php86() {
    let out = run86(
        r#"<?php
class HandlerStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('handler-stream', HandlerStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
    },
]]);
$stream = fopen('handler-stream://x', 'r', false, $context);
var_dump(strlen(fread($stream, 4)));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "handler: 1:UserspaceInvalidReturn\nint(4)\nint(1)\n"
    );
}

/// A `stream_get_contents()` operation accumulates every excess read and calls the
/// context handler once with all of them.
#[test]
fn test_user_wrapper_read_excess_accumulates_per_operation_php86() {
    let out = run86(
        r#"<?php
class AccumulatingStream {
    public $context;
    private int $reads = 0;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string {
        $this->reads++;
        return $this->reads <= 2 ? str_repeat('B', $count + 1) : '';
    }
    public function stream_eof(): bool { return $this->reads >= 3; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('accumulating-stream', AccumulatingStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), "\n";
    },
]]);
$stream = fopen('accumulating-stream://x', 'r', false, $context);
$data = stream_get_contents($stream);
var_dump(strlen($data));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(out, "handler: 2\nint(16384)\nint(2)\n");
}

/// A user wrapper without `stream_cast()` reports the two terminating cast errors
/// through the context handler when `stream_select()` probes it.
#[test]
fn test_user_wrapper_stream_select_cast_errors_php86() {
    let out = run86(
        r#"<?php
class NoCastStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count); }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('no-cast-stream', NoCastStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), "\n";
        foreach ($errors as $error) {
            echo $error->code->name, ':', $error->terminating ? 'yes' : 'no', "\n";
        }
    },
]]);
$stream = fopen('no-cast-stream://x', 'r', false, $context);
$read = [$stream];
$write = null;
$except = null;
var_dump(stream_select($read, $write, $except, 0, 0, $context));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "handler: 2\n\
NotImplemented:yes\n\
CastNotSupported:yes\n\
int(0)\n\
int(2)\n"
    );
}

/// Each wrapper stream dispatches through the context it was opened with, even when
/// another context was created later.
#[test]
fn test_user_wrapper_read_uses_its_own_context_php86() {
    let out = run86(
        r#"<?php
class ContextStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('context-stream', ContextStream::class);
$first = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_handler' => static function (array $errors): void {
        echo "first handler\n";
    },
]]);
$second = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_handler' => static function (array $errors): void {
        echo "second handler\n";
    },
]]);
$one = fopen('context-stream://one', 'r', false, $first);
$two = fopen('context-stream://two', 'r', false, $second);
fread($one, 1);
fread($two, 1);
fclose($one);
fclose($two);
"#,
    );
    assert_eq!(out, "first handler\nsecond handler\n");
}

/// A wrapper that over-reports `stream_write` yields a clamped write and a
/// `UserspaceInvalidReturn` error through the context handler.
#[test]
fn test_user_wrapper_stream_write_excess_php86() {
    let out = run86(
        r#"<?php
class WriteExcessStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_write(string $data): int { return strlen($data) + 1; }
    public function stream_read(int $count): string { return ''; }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('write-excess-stream', WriteExcessStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
        echo $errors[0]->message, "\n";
    },
]]);
$stream = fopen('write-excess-stream://x', 'w', false, $context);
var_dump(fwrite($stream, 'abc'));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "handler: 1:UserspaceInvalidReturn\n\
WriteExcessStream::stream_write wrote 1 bytes more data than requested (4 written, 3 max)\n\
int(3)\n\
int(1)\n"
    );
}

/// A wrapper without `stream_write` reports `NotImplemented` for `fwrite`.
#[test]
fn test_user_wrapper_stream_write_missing_php86() {
    let out = run86(
        r#"<?php
class WriteMissingStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return ''; }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('write-missing-stream', WriteMissingStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
        echo $errors[0]->message, "\n";
    },
]]);
$stream = fopen('write-missing-stream://x', 'w', false, $context);
var_dump(fwrite($stream, 'abc'));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "handler: 1:NotImplemented\n\
WriteMissingStream::stream_write is not implemented!\n\
int(0)\n\
int(1)\n"
    );
}

/// `file_get_contents()` dispatches a missing wrapper `stream_read` through the
/// caller's context.
#[test]
fn test_user_wrapper_file_get_contents_missing_read_php86() {
    let out = run86(
        r#"<?php
class NoReadStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_eof(): bool { return false; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('no-read-file-stream', NoReadStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
        echo $errors[0]->message, "\n";
    },
]]);
$data = file_get_contents('no-read-file-stream://x', false, $context);
var_dump($data);
var_dump(count(stream_last_errors()));
"#,
    );
    assert_eq!(
        out,
        "handler: 1:NotImplemented\n\
NoReadStream::stream_read is not implemented!\n\
string(0) \"\"\n\
int(1)\n"
    );
}

/// `stream_context_set_option()` updates the per-context map for both the
/// component form and the options-array form.
#[test]
fn test_stream_context_set_option_updates_dispatch_php86() {
    let out = run86(
        r#"<?php
class SetOptionStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_write(string $data): int { return strlen($data) + 1; }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('set-option-stream', SetOptionStream::class);
$old = static function (array $errors): void { echo "old\n"; };
$new = static function (array $errors): void { echo "new\n"; };
$first = stream_context_create(['stream' => ['error_mode' => StreamErrorMode::Silent, 'error_handler' => $old]]);
stream_context_set_option($first, 'stream', 'error_handler', $new);
$second = stream_context_create(['stream' => ['error_mode' => StreamErrorMode::Silent, 'error_handler' => $old]]);
stream_context_set_option($second, ['stream' => ['error_handler' => $new]]);
foreach ([$first, $second] as $context) {
    $stream = fopen('set-option-stream://x', 'w', false, $context);
    fwrite($stream, 'x');
    fclose($stream);
}
"#,
    );
    assert_eq!(out, "new\nnew\n");
}

/// The 8.6 context parameters are accepted by `stream_copy_to_stream()`,
/// `stream_socket_pair()`, and `stream_is_local()`.
#[test]
fn test_stream_context_parameters_php86() {
    let out = run86(
        r#"<?php
$context = stream_context_create(['stream' => ['error_mode' => StreamErrorMode::Silent]]);
$source = fopen('php://memory', 'w+');
fwrite($source, 'hello');
rewind($source);
$destination = fopen('php://memory', 'w+');
$bytes = stream_copy_to_stream($source, $destination, null, -1, $context);
rewind($destination);
$copied = stream_get_contents($destination);
$local = stream_is_local(stream: 'php://memory', context: $context);
$pair = stream_socket_pair(
    domain: STREAM_PF_UNIX,
    type: STREAM_SOCK_STREAM,
    protocol: STREAM_IPPROTO_IP,
    context: $context,
);
echo $bytes, ':', $copied, ':', $local ? 'local' : 'remote', ':', count($pair), "\n";
foreach ($pair as $stream) {
    fclose($stream);
}
fclose($source);
fclose($destination);
"#,
    );
    assert_eq!(out, "5:hello:local:2\n");
}

/// Copying from a wrapper that over-reports `stream_read()` dispatches the
/// read error once through the context supplied to `stream_copy_to_stream()`.
#[test]
fn test_stream_copy_to_stream_read_error_php86() {
    let out = run86(
        r#"<?php
class CopyErrorReadStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return false; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('copy-error-read', CopyErrorReadStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
        echo $errors[0]->message, "\n";
    },
]]);
$source = fopen('copy-error-read://x', 'r', false, $context);
$destination = fopen('php://memory', 'w+');
var_dump(stream_copy_to_stream($source, $destination, 8, -1, $context));
var_dump(count(stream_last_errors()));
fclose($source);
fclose($destination);
"#,
    );
    assert_eq!(
        out,
        "handler: 1:UserspaceInvalidReturn\n\
CopyErrorReadStream::stream_read - read 1 bytes more data than requested (8193 read, 8192 max) - excess data will be lost\n\
int(8)\n\
int(1)\n"
    );
}

/// Copying into a wrapper that over-reports `stream_write()` dispatches the
/// write error through the copy call's context.
#[test]
fn test_stream_copy_to_stream_write_error_php86() {
    let out = run86(
        r#"<?php
class CopyErrorWriteStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_write(string $data): int { return strlen($data) + 1; }
    public function stream_read(int $count): string { return ''; }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('copy-error-write', CopyErrorWriteStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
        echo $errors[0]->message, "\n";
    },
]]);
$source = fopen('php://memory', 'w+');
fwrite($source, 'abcdefgh');
rewind($source);
$destination = fopen('copy-error-write://x', 'w', false, $context);
var_dump(stream_copy_to_stream($source, $destination, 8, -1, $context));
var_dump(count(stream_last_errors()));
fclose($source);
fclose($destination);
"#,
    );
    assert_eq!(
        out,
        "handler: 1:UserspaceInvalidReturn\n\
CopyErrorWriteStream::stream_write wrote 1 bytes more data than requested (9 written, 8 max)\n\
int(8)\n\
int(1)\n"
    );
}

/// A copy that records both a read and a write error reports them in one
/// context-handler invocation, in read-then-write order.
#[test]
fn test_stream_copy_to_stream_combined_errors_php86() {
    let out = run86(
        r#"<?php
class CopyCombinedReadStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return false; }
    public function stream_stat(): array { return []; }
}
class CopyCombinedWriteStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_write(string $data): int { return strlen($data) + 1; }
    public function stream_read(int $count): string { return ''; }
    public function stream_eof(): bool { return true; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('copy-combined-read', CopyCombinedReadStream::class);
stream_wrapper_register('copy-combined-write', CopyCombinedWriteStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => static function (array $errors): void {
        echo 'handler: ', count($errors), "\n";
        foreach ($errors as $error) {
            echo $error->code->name, ': ', $error->message, "\n";
        }
    },
]]);
$source = fopen('copy-combined-read://x', 'r', false, $context);
$destination = fopen('copy-combined-write://x', 'w', false, $context);
var_dump(stream_copy_to_stream($source, $destination, 8, -1, $context));
var_dump(count(stream_last_errors()));
fclose($source);
fclose($destination);
"#,
    );
    assert_eq!(
        out,
        "handler: 2\n\
UserspaceInvalidReturn: CopyCombinedReadStream::stream_read - read 1 bytes more data than requested (8193 read, 8192 max) - excess data will be lost\n\
UserspaceInvalidReturn: CopyCombinedWriteStream::stream_write wrote 1 bytes more data than requested (9 written, 8 max)\n\
int(8)\n\
int(2)\n"
    );
}

/// `stream_context_set_options()` refreshes the per-context dispatch options used by
/// later stream-error handling.
#[test]
fn test_stream_context_set_options_refreshes_dispatch_php86() {
    let out = run86(
        r#"<?php
class SetOptionsReadStream {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_read(int $count): string { return str_repeat('A', $count + 1); }
    public function stream_eof(): bool { return false; }
    public function stream_stat(): array { return []; }
}
stream_wrapper_register('set-options-read', SetOptionsReadStream::class);
$context = stream_context_create(['stream' => [
    'error_mode' => StreamErrorMode::Error,
]]);
$handler = static function (array $errors): void {
    echo 'handler: ', count($errors), ':', $errors[0]->code->name, "\n";
};
var_dump(stream_context_set_options($context, ['stream' => [
    'error_mode' => StreamErrorMode::Silent,
    'error_store' => StreamErrorStore::All,
    'error_handler' => $handler,
]]));
$stream = fopen('set-options-read://x', 'r', false, $context);
var_dump(strlen(fread($stream, 4)));
var_dump(count(stream_last_errors()));
fclose($stream);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nhandler: 1:UserspaceInvalidReturn\nint(4)\nint(1)\n"
    );
}

/// Computed `$GLOBALS[$name]` reads and writes reach a top-level variable, including from a
/// function body, and observe `isset()`/`??`/`unset()` around it.
#[test]
fn test_globals_computed_read_write_php86() {
    let out = run86(
        r#"<?php
function setComputedGlobal(string $name, $value): void {
    $GLOBALS[$name] = $value;
}
function getComputedGlobal(string $name) {
    return $GLOBALS[$name] ?? null;
}
$counter = 1;
$key = 'counter' . ($argc > 0 ? '' : 'x');
var_dump($GLOBALS[$key]);
setComputedGlobal($key, 7);
var_dump($counter, getComputedGlobal($key), isset($GLOBALS[$key]));
unset($GLOBALS[$key]);
var_dump(isset($GLOBALS[$key]), $GLOBALS[$key] ?? 'gone');
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(7)\nint(7)\nbool(true)\nbool(false)\nstring(4) \"gone\"\n"
    );
}

/// A bare `$GLOBALS` snapshot is an array copy of the current global values: mutating a
/// global in place after taking it does not publish through the snapshot.
#[test]
fn test_globals_snapshot_isolated_from_later_mutation_php86() {
    let out = run86(
        r#"<?php
$a = [1];
$snapshot = $GLOBALS;
$a[] = 2;
echo count($snapshot["a"]), ":", count($a), "\n";
$b = "x";
$text = $GLOBALS;
$b .= "y";
echo $text["b"], ":", $b, "\n";
"#,
    );
    assert_eq!(out, "1:2\nx:xy\n");
}

/// PHP permits `foreach ($GLOBALS as &$v)` but does not publish writes through the loop
/// variable back to the original globals; elephc keeps the same snapshot behavior.
#[test]
fn test_globals_foreach_by_reference_is_a_snapshot_php86() {
    let out = run86(
        r#"<?php
$a = 1;
$seen = 0;
foreach ($GLOBALS as &$value) {
    $seen++;
    if ($value === 1) { $value = 2; }
}
unset($value);
echo $a, ":", $seen > 0 ? "iterated" : "empty";
"#,
    );
    assert_eq!(out, "1:iterated");
}

/// A computed `$GLOBALS` read whose name has no initialized value warns like php-src and
/// returns null.
#[test]
fn test_globals_computed_undefined_warning_php86() {
    let out = compile_and_run_capture_merged_with_php_version(
        r#"<?php
$key = 'notInitialized' . ($argc > 0 ? '' : 'x');
var_dump($GLOBALS[$key]);
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "Warning: Undefined global variable $notInitialized\nNULL\n"
    );
}

/// Nested `$GLOBALS[...]` writes update the top-level container, autovivify a missing
/// global, and work through a runtime outer key.
#[test]
fn test_globals_nested_write_php86() {
    let out = run86(
        r#"<?php
$data = ['a' => 1];
function setNested(): void {
    $GLOBALS['data']['b'] = 2;
    $GLOBALS['fresh']['k'] = 5;
}
setNested();
var_dump($data, $GLOBALS['fresh']);
$name = 'data' . ($argc > 0 ? '' : 'x');
$GLOBALS[$name]['c'] = 4;
var_dump($data);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  int(2)\n}\n\
array(1) {\n  [\"k\"]=>\n  int(5)\n}\n\
array(3) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  int(2)\n  [\"c\"]=>\n  int(4)\n}\n"
    );
}
