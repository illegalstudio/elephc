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
    let cases = [
        (
            "<?php $x = 1; var_dump(count($GLOBALS));",
            "The $GLOBALS array itself is not supported yet",
        ),
        (
            "<?php $x = 1; $k = 'x'; var_dump($GLOBALS[$k]);",
            "Computed $GLOBALS keys are not supported yet",
        ),
        (
            "<?php $x = 1; $GLOBALS['y'] = 9;",
            "Writes through $GLOBALS are not supported yet",
        ),
        (
            "<?php $x = 1; $GLOBALS[] = 9;",
            "Writes through $GLOBALS are not supported yet",
        ),
        (
            "<?php $x = 1; $GLOBALS['a']['b'] = 9;",
            "Writes through $GLOBALS are not supported yet",
        ),
    ];
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
    assert_eq!(out, "handler: 2\nint(8192)\nint(2)\n");
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
