//! Purpose:
//! End-to-end tests for how long an `eval()` keeps the function locals it imports alive.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_scope_lifetimes` through Rust's test harness.
//!
//! Key details:
//! - Before the fragment runs, the frame copies the locals it may read into the eval scope,
//!   and after it the native slots reload them; the scope's copies used to live until the
//!   function returned, so a destructor php runs at `unset($o)` or `$o = null` ran at return.
//! - The fixtures echo from `__destruct` and compare the order with `php -n` 8.5's output.

use crate::support::*;

/// Verifies `unset()` and reassignment after an `eval()` inside a function run the destructor
/// where php runs it, for a runtime fragment (one holding a bound closure too), a literal
/// fragment, and a local the fragment itself replaced.
#[test]
fn test_function_eval_releases_imported_locals_at_unset() {
    let out = compile_and_run(
        r#"<?php
class Noisy {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function dynamic_unset(string $code): void {
    $o = new Noisy("dynamic-unset");
    $f = function () { return $this->name; };
    $bound = Closure::bind($f, $o, Noisy::class);
    eval($code);
    unset($bound, $o);
    echo "after dynamic unset\n";
}
function dynamic_reassign(string $code): void {
    $o = new Noisy("dynamic-reassign");
    eval($code);
    $o = null;
    echo "after dynamic reassign\n";
}
function literal_unset(): void {
    $o = new Noisy("literal-unset");
    eval('echo "sees ", $o->name, "\n";');
    unset($o);
    echo "after literal unset\n";
}
function written_by_eval(string $code): void {
    $o = new Noisy("first");
    eval($code);
    echo "eval wrote ", $o->name, "\n";
    unset($o);
    echo "after written unset\n";
}
$code = $argc > 0 ? 'echo "eval sees ", isset($o) ? $o->name : "-", "\n";' : '';
dynamic_unset($code);
dynamic_reassign($code);
literal_unset();
written_by_eval($argc > 0 ? '$o = new Noisy("second");' : '');
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "eval sees dynamic-unset\ndestruct dynamic-unset\nafter dynamic unset\n\
         eval sees dynamic-reassign\ndestruct dynamic-reassign\nafter dynamic reassign\n\
         sees literal-unset\ndestruct literal-unset\nafter literal unset\n\
         destruct first\neval wrote second\ndestruct second\nafter written unset\nend\n"
    );
}
