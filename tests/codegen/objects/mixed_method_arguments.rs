//! Purpose:
//! Tests source argument binding for methods reached through an opaque Mixed receiver.
//!
//! Called from:
//! - `crate::codegen::objects` in the codegen integration suite.
//!
//! Key details:
//! - Runtime method selection must preserve defaults, variadic binding and evaluation order.
//! - Descriptor invocation must not leak its receiver, callback or argument-container owners.

use super::*;

/// A null receiver fails before evaluating arguments and retires its temporary box.
#[test]
fn mixed_method_arguments_null_receiver_is_lazy_and_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Totals { public function collect(int ...$values): int { return array_sum($values); } }
class OtherTotals { public function collect(int $value): int { return $value; } }
function opaque(mixed $value): mixed { return $value; }
function argument(): int { echo "arg|"; return 1; }
opaque(new OtherTotals())->collect(0);
$receiver = opaque(new Totals());
echo $receiver->collect(2), "|";
unset($receiver);
try { opaque(null)->collect(argument()); }
catch (Error $error) { echo $error->getMessage(); unset($error); }
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2|Call to a member function collect() on null", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic method binding uses source arguments rather than guessing physical ABI arity.
#[test]
fn mixed_method_arguments_preserve_defaults_variadics_spread_and_names() {
    let out = compile_and_run(r#"<?php
class Totals {
    public function collect(int $head = 10, int ...$tail): int {
        return $head + array_sum($tail);
    }
}
class OtherTotals { public function collect(int $value): int { return $value; } }
function opaque(mixed $value): mixed { return $value; }
opaque(new OtherTotals())->collect(0);
$receiver = opaque(new Totals());
echo $receiver->collect(), "|";
echo $receiver->collect(2, 3, 4), "|";
echo $receiver->collect(...[2, 3, 4]), "|";
echo $receiver->collect(head: 5, last: 6);
"#);
    assert_eq!(out, "10|9|9|11");
}

/// Binding evaluates positional arguments once in source order and releases temporary owners.
#[test]
fn mixed_method_arguments_evaluate_once_and_keep_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Totals {
    public function collect(int $head, int ...$tail): int {
        return $head + array_sum($tail);
    }
}
class OtherTotals { public function collect(int $value): int { return $value; } }
function opaque(mixed $value): mixed { return $value; }
function argument(int $value): int { echo $value; return $value; }
opaque(new OtherTotals())->collect(0);
$receiver = opaque(new Totals());
echo $receiver->collect(argument(1), argument(2), argument(3));
unset($receiver);
"#);
    assert!(out.success, "{} {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1236", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
