//! Purpose:
//! Covers `__get`/`__isset`/`__set` on compiled classes in a program that links eval: reads
//! through a `Mixed` receiver (every local is `mixed` after an `eval()`), and eval code reading,
//! probing and writing a compiled object's magic properties.
//!
//! Called from:
//! - `cargo test --test codegen_tests magic_accessors_with_eval_`.
//!
//! Key details:
//! - Linking eval gives classes a per-instance property hash. A name the hash lacks is still
//!   php's `__get()` to answer, before any `Undefined property` warning.

use crate::support::compile_and_run_capture;

/// A `Mixed` receiver reads an undeclared name through `__get` even when the class keeps a
/// per-instance hash for eval, and reads a dynamic property the hash holds without calling it.
#[test]
fn magic_accessors_with_eval_mixed_reads_call_get() {
    let out = compile_and_run_capture(
        r#"<?php
class M {
    private array $d = [];
    public function __get(string $n): mixed { echo "get $n\n"; return $this->d[$n] ?? null; }
    public function __set(string $n, mixed $v): void { echo "set $n\n"; $this->d[$n] = $v; }
}
function readA(mixed $m): mixed { return $m->a; }
$m = new M;
$m->a = 1;
var_dump(readA($m));
if ($argc > 9) { eval('1;'); }
"#,
    );
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "set a\nget a\nint(1)\n");
}

/// Eval code reads and probes a compiled object's undeclared names through `__get` and
/// `__isset`, and writes them through `__set`, as php does.
#[test]
fn magic_accessors_with_eval_eval_code_calls_accessors() {
    let out = compile_and_run_capture(
        r#"<?php
class M {
    private array $d = [];
    public function __get(string $n): mixed { echo "get $n\n"; return $this->d[$n] ?? null; }
    public function __isset(string $n): bool { return isset($this->d[$n]); }
    public function __set(string $n, mixed $v): void { echo "set $n\n"; $this->d[$n] = $v; }
}
$m = new M;
eval('$m->b = 2;');
var_dump(eval('return $m->b;'));
var_dump(eval('return isset($m->b);'));
var_dump(eval('return isset($m->c);'));
var_dump($m->b);
"#,
    );
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "set b\nget b\nint(2)\nbool(true)\nbool(false)\nget b\nint(2)\n");
}

/// An eval class's `__get` and `__isset` run at most once per object and property at a time, as
/// php's guards do: the same access inside the hook reaches the property itself instead of
/// recursing until the stack runs out.
#[test]
fn magic_accessors_with_eval_hooks_do_not_recurse() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class M { public function __get($n) { echo "get $n\n"; return isset($this->$n) ? "set" : "unset"; } public function __isset($n) { echo "isset $n\n"; return isset($this->$n); } }');
eval('$m = new M; var_dump($m->x); var_dump(isset($m->y));');
"#,
    );
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "get x\nisset x\nstring(5) \"unset\"\nisset y\nbool(false)\n");
}
