//! Purpose:
//! Regression tests for a Throwable raised by an autoloader that a class-like existence probe
//! (`class_exists()` and siblings) started.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php propagates the exception out of the probe; the eval bridge used to answer `false`.
//! - The programs call `eval()` so the bridge is linked and the probes take its entry points.

use crate::support::*;

/// symfony/config `ClassExistenceResource::throwOnRequiredClass`: an autoloader that throws while
/// a COMPILED `class_exists()` / `interface_exists()` / `trait_exists()` / `enum_exists()` probe
/// runs must unwind out of the probe, and the heap must stay clean.
#[test]
fn test_compiled_class_like_probes_propagate_autoloader_throwables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
eval('$seed = 1;');
spl_autoload_register(function (string $class): void {
    throw new ReflectionException(sprintf('Class "%s" not found while loading a parent', $class));
});
$name = $argc > 5 ? 'X' : 'Vendor\Missing\Fixture';
foreach (['class_exists', 'interface_exists', 'trait_exists', 'enum_exists'] as $probe) {
    try {
        switch ($probe) {
            case 'class_exists': $r = class_exists($name); break;
            case 'interface_exists': $r = interface_exists($name); break;
            case 'trait_exists': $r = trait_exists($name); break;
            default: $r = enum_exists($name); break;
        }
        echo $probe, ' returned ', var_export($r, true), "\n";
    } catch (ReflectionException $e) {
        echo $probe, ' caught: ', $e->getMessage(), "\n";
    }
}
var_dump(class_exists($name, false));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let expected = "class_exists caught: Class \"Vendor\\Missing\\Fixture\" not found while loading a parent\n\
interface_exists caught: Class \"Vendor\\Missing\\Fixture\" not found while loading a parent\n\
trait_exists caught: Class \"Vendor\\Missing\\Fixture\" not found while loading a parent\n\
enum_exists caught: Class \"Vendor\\Missing\\Fixture\" not found while loading a parent\n\
bool(false)\n";
    assert_eq!(out.stdout, expected);
    // The two blocks left are `spl_autoload_register()`'s own closure registration, present with
    // or without a throw; four rethrown probes must add nothing to them.
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean")
            || out.stderr.contains("leak summary: live_blocks=2 live_bytes=128"),
        "{}",
        out.stderr
    );
}

/// The same probes written inside `eval()` propagate through the interpreter.
#[test]
fn test_interpreted_class_like_probes_propagate_autoloader_throwables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
spl_autoload_register(function (string $class): void {
    throw new ReflectionException('no ' . $class);
});
try {
    eval('var_dump(class_exists("Vendor\\\\Missing\\\\Fixture"));');
} catch (ReflectionException $e) {
    echo 'caught: ', $e->getMessage(), "\n";
}
try {
    eval('var_dump(interface_exists("Vendor\\\\Missing\\\\Fixture"));');
} catch (ReflectionException $e) {
    echo 'caught: ', $e->getMessage(), "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "caught: no Vendor\\Missing\\Fixture\ncaught: no Vendor\\Missing\\Fixture\n",
        "stderr: {}",
        out.stderr
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
