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
