//! Purpose:
//! Verifies undefined eval calls use catchable PHP errors across the native boundary.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime-selected strings force Magician instead of literal eval compilation.
//! - Tests cover eval-local and native catches, namespace fallback, and argument effects.

use crate::support::*;

/// Missing direct functions throw before arguments run and survive both eval and native catches.
#[test]
fn test_eval_undefined_functions_are_catchable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
    $n = 0;
    function bump() { global $n; $n++; return $n; }
    try { IsSeEt(bump()); } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
    echo $n, "\n";
' : '';
eval($source);
$source = $argc > 0 ? 'namespace ReplCheck; IsSeEt();' : '';
try { eval($source); } catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$source = $argc > 0 ? 'namespace ReplCheck; echo StRlEn("ok"), "\n";' : '';
eval($source);
echo "alive\n";
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "Error: Call to undefined function IsSeEt()\n",
        "0\n",
        "Error: Call to undefined function ReplCheck\\IsSeEt()\n",
        "2\nalive\n",
    ));
}

/// Source-preserving direct calls still find native callbacks and native namespace fallbacks.
#[test]
fn test_eval_mixed_case_direct_native_calls() {
    let output = compile_and_run_with_heap_debug(r#"<?php
namespace NativeRepl {
    function MixedName(int $value): int { return $value + 1; }
}
namespace {
    function GlobalReplTarget(int $value): int { return $value + 2; }
    $source = $argc > 0 ? '
        namespace NativeRepl;
        echo mIxEdNaMe(40), ":", GlObAlRePlTaRgEt(40), ":", \\NATIVEREPL\\MIXEDNAME(42);
    ' : '';
    eval($source);
}
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "41:42:43");
}

/// First-class callables retain mixed-case native lookup, namespace fallback, and error spelling.
#[test]
fn test_eval_mixed_case_native_function_callables() {
    let output = compile_and_run_with_heap_debug(r#"<?php
namespace NativeRepl {
    function MixedName(int $value): int { return $value + 1; }
}
namespace {
    function GlobalReplTarget(int $value): int { return $value + 2; }
    $source = $argc > 0 ? '
        namespace NativeRepl;
        use function NativeRepl\\MixedName as Imported;
        $local = mIxEdNaMe(...);
        $fallback = GlObAlRePlTaRgEt(...);
        $qualified = \\NATIVEREPL\\MIXEDNAME(...);
        $imported = iMpOrTeD(...);
        echo $local(40), ":", $fallback(40), ":", $qualified(42), ":", $imported(43), "\n";
        try { StrLenX(...); } catch (\\Error $e) { echo $e->getMessage(), "\n"; }
    ' : '';
    eval($source);
    $source = $argc > 0 ? 'namespace NativeRepl; StrLenX(...);' : '';
    try { eval($source); } catch (Error $e) { echo $e->getMessage(), "\n"; }
    echo "alive\n";
}
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "41:42:43:44\n",
        "Call to undefined function NativeRepl\\StrLenX()\n",
        "Call to undefined function NativeRepl\\StrLenX()\n",
        "alive\n",
    ));
}
