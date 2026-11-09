//! Purpose:
//! End-to-end tests for `get_debug_type()` across scalar, object, callable, and resource
//! representations, including the closed-resource spelling the Io\Poll fixtures depend on.
//!
//! Called from:
//! - `cargo test` through the types codegen suite.
//!
//! Key details:
//! - The expected strings are real `php -r` output for the same fixtures.
//! - Resource subtype names come from the shared runtime helper, so stream filters and
//!   stream contexts report PHP's exact spelling.

use crate::support::compile_and_run;

/// Verifies scalar, array, object, enum, and closure names from static and boxed storage.
#[test]
fn test_get_debug_type_scalars_objects_and_closures() {
    let out = compile_and_run(
        r#"<?php
class DebugTypeThing {}
enum DebugTypeSuit { case Hearts; }
$values = [1, 1.5, "s", true, false, null, [1], ["a" => 1], new DebugTypeThing(), DebugTypeSuit::Hearts, function () {}];
foreach ($values as $value) {
    echo get_debug_type($value), "|";
}
echo \GET_DEBUG_TYPE(42), "|";
namespace DebugTypeNamespace {
    function name(): string { return get_debug_type(1); }
}
namespace {
    echo \DebugTypeNamespace\name();
}
"#,
    );
    assert_eq!(
        out,
        "int|float|string|bool|bool|null|array|array|DebugTypeThing|DebugTypeSuit|Closure|int|int"
    );
}

/// Verifies open stream/context/filter names and the closed-resource spelling.
#[test]
fn test_get_debug_type_resources() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://temp", "r+");
$context = stream_context_create();
$filter = stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);
echo get_debug_type($stream), "|", get_debug_type($context), "|", get_debug_type($filter), "|";
fclose($stream);
echo get_debug_type($stream), "|";
echo gettype($stream), "|", get_resource_type($stream);
"#,
    );
    assert_eq!(
        out,
        "resource (stream)|resource (stream-context)|resource (stream filter)|resource (closed)|resource (closed)|Unknown"
    );
}
