//! Purpose:
//! Checks ownership of concrete string and callable values extracted from hashes.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - Early loop exits and source replacement must preserve collected payloads.

use crate::support::*;

/// Extracted strings and captured callables survive continue, break, and source replacement.
#[test]
fn test_foreach_hash_scalar_owned_values_survive_early_exit() {
    let source = r#"<?php
function strings(): array<string>
{
    array<string, string> $source = [];
    $source['skip'] = str_repeat('s', 3);
    $source['keep'] = str_repeat('k', 4);
    $source['stop'] = str_repeat('t', 5);
    array<string> $kept = [];
    foreach ($source as $key => $value) {
        if ($key === 'skip') { continue; }
        if ($key === 'stop') { break; }
        $kept[] = $value;
    }
    unset($source, $value);
    return $kept;
}
function callbacks(): array<callable>
{
    $prefix = str_repeat('p', 3);
    array<string, callable> $source = [];
    $source['first'] = function() use ($prefix): string { return $prefix . 'a'; };
    $source['second'] = function() use ($prefix): string { return $prefix . 'b'; };
    array<callable> $kept = [];
    foreach ($source as $key => $value) {
        if ($key === 'first') { $kept[] = $value; }
    }
    foreach ($source as $value) {
        $kept[] = $value;
        $source = [];
        break;
    }
    unset($source, $value, $prefix);
    return $kept;
}
$strings = strings();
echo $strings[0], '|';
$callbacks = callbacks();
$callback = $callbacks[0];
$again = $callbacks[1];
unset($callbacks);
echo $callback(), '|', $again();
unset($strings, $callback, $again);
"#;
    for optimize in [false, true] {
        let out = if optimize {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout, "kkkk|pppa|pppa", "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}
