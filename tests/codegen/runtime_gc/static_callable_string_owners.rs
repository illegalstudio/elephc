//! Purpose:
//! Verifies late-bound static callable string ownership across overriding methods.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Boxed array extraction keeps invocation on the runtime descriptor path.
//! - Heap-producing, parameter-returning and literal-returning overrides share one factory.

use crate::support::*;

/// Late-bound descriptors transfer owned strings and detach borrowed overrides exactly once.
#[test]
fn test_core_late_static_callbacks_balance_owned_and_borrowed_strings() {
    let source = r#"<?php
class LateStringBase {
    public static function render(string $value): string { return "base" . $value; }
    public static function callbacks(): array { return [static::render(...)]; }
}
class LateStringBorrowed extends LateStringBase {
    public static function render(string $value): string { return $value; }
}
class LateStringLiteral extends LateStringBase {
    public static function render(string $value): string { return "literal"; }
}
class LateStringOwned extends LateStringBase {
    public static function render(string $value): string { return "owned" . $value; }
}
function showLateStringCallbacks(array $callbacks): void {
    $callback = $callbacks[0];
    unset($callbacks);
    $mapped = array_map($callback, ["arg"]);
    echo $mapped[0], "|";
    unset($mapped, $callback);
}
for ($i = 0; $i < 3; $i++) {
    showLateStringCallbacks(LateStringBase::callbacks());
    showLateStringCallbacks(LateStringBorrowed::callbacks());
    showLateStringCallbacks(LateStringLiteral::callbacks());
    showLateStringCallbacks(LateStringOwned::callbacks());
}
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "basearg|arg|literal|ownedarg|".repeat(3), "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), out.stdout);
}
