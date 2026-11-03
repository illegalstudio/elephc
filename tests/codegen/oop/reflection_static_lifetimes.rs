//! Purpose:
//! Verifies that reflector metadata does not retain the current static-property values.
//!
//! Called from:
//! - Supported-host executable reflection regressions in CI.
//!
//! Key details:
//! - Generic reflector accessors read live values and omit uninitialized properties.
//! - Constructing a reflector does not postpone a displaced object's destructor.

use crate::support::*;

/// A generic accessor refreshes its map while construction preserves ordinary static lifetimes.
#[test]
fn test_reflection_static_values_are_loaded_lazily_without_retaining_displaced_objects() {
    let source = r#"<?php
class LazyReflectionStaticSubject {
    public static mixed $value = null;
    public static int $uninitialized;
}
class LazyReflectionStaticTracked {
    public function __destruct() { echo 'old|'; }
}
function lazyReflectionMap(ReflectionClass $reflection): mixed { return $reflection->getStaticProperties(); }
function lazyReflectionValue(ReflectionClass $reflection): mixed { return $reflection->getStaticPropertyValue('value'); }
LazyReflectionStaticSubject::$value = new LazyReflectionStaticTracked();
$reflection = new ReflectionClass(LazyReflectionStaticSubject::class);
LazyReflectionStaticSubject::$value = 11;
echo 'assigned|';
$first = lazyReflectionMap($reflection);
echo $first['value'], '|', count($first), '|';
LazyReflectionStaticSubject::$value = 12;
echo lazyReflectionValue($reflection), '|';
$second = lazyReflectionMap($reflection);
echo $second['value'];
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "status={} stdout={} stderr={}", output.status, output.stdout, output.stderr);
    assert_eq!(output.stdout, "old|assigned|11|1|12|12");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
