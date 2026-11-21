//! Purpose:
//! Exercises native descriptors and delayed eval callbacks across the Rust/C boundary.
//!
//! Called from:
//! - The focused eval codegen test module.
//!
//! Key details:
//! - Runtime-selected source prevents literal-eval AOT lowering from hiding the bridge.
//! - Native allocations and exceptions must use the caller arena, not Rust's scratch register.

use crate::support::*;

/// Keeps allocating AOT functions and native throws inside the captured eval arena.
#[test]
fn test_eval_foreign_context_native_descriptor_allocates_and_catches() {
    let out = compile_and_run(r#"<?php
function nativeContextReply(string $label): string {
    $parts = [$label, strtoupper($label)];
    return implode("/", $parts);
}
function nativeContextThrow(): void { throw new RuntimeException("native-arena"); }
$code = $argc > 1 ? $argv[1] : '
    $reply = nativeContextReply("arena");
    try { nativeContextThrow(); } catch (RuntimeException $error) {
        return $reply . "|" . $error->getMessage();
    }
';
echo eval($code), "|alive";
"#);
    assert_eq!(out, "arena/ARENA|native-arena|alive");
}

/// Invokes an eval-owned closure after the original execute activation has returned.
#[test]
fn test_eval_foreign_context_delayed_callback_allocates() {
    let out = compile_and_run(r#"<?php
$code = $argc > 1 ? $argv[1] : 'return function (string $label): string {
    return strtoupper($label) . "/later";
};';
$callback = eval($code);
echo call_user_func($callback, "arena"), "|alive";
"#);
    assert_eq!(out, "ARENA/later|alive");
}

/// Probes and rebinds an eval closure after execute returns, then reads a native receiver property.
#[test]
fn test_eval_foreign_context_post_barrier_probe_and_bind() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ArenaBindReceiver { public string $label = "bound-arena"; }
$code = $argc > 1 ? $argv[1] : 'return function (): string { return $this->label; };';
$callback = eval($code);
echo is_callable($callback) ? "callable|" : "invalid|";
$receiver = new ArenaBindReceiver();
$bound = Closure::bind($callback, $receiver, ArenaBindReceiver::class);
echo call_user_func($bound), "|alive";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "callable|bound-arena|alive");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
