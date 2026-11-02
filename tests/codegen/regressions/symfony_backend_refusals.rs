//! Purpose:
//! Regression tests for backend refusals that kept the Symfony skeleton's closed-world build from
//! compiling a class file (each refused body handed its whole file to the runtime autoloader).
//!
//! Called from:
//! - `cargo test --test codegen_tests symfony_backend_refusals`.
//!
//! Key details:
//! - Every fixture also asserts a clean `--heap-debug` leak summary.

use crate::support::*;

/// A runtime-string callee chosen between two regex builtins (symfony/string's
/// `ByteString::replaceMatches()`): `preg_replace()` answers `mixed` through its callable wrapper,
/// whose ownership contract was ambiguous, so neither builtin had a dispatch target and the call
/// was refused ("callable_descriptor_invoke for runtime string with no descriptor targets").
#[test]
fn test_runtime_string_callee_over_preg_builtins() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class S {
    public function __construct(private string $string) {}
    public function replaceMatches(string $fromRegexp, string|callable $to): static {
        $replace = \is_array($to) || $to instanceof \Closure ? 'preg_replace_callback' : 'preg_replace';
        $r = clone $this;
        $r->string = $replace($fromRegexp, $to, $this->string);
        return $r;
    }
    public function get() { return $this->string; }
}
$s = new S("hello world");
echo $s->replaceMatches('/o/', '0')->get(), "\n";
echo $s->replaceMatches('/o/', fn($m) => strtoupper($m[0]))->get(), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "hell0 w0rld\nhellO wOrld\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
