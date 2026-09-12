//! Purpose:
//! Verifies the descriptor variadic STORAGE contract end to end: a collector any callable
//! descriptor can fill reads a positional tail, a named tail, and a mixed tail through one
//! container, for every callable kind that can be handed out as a descriptor.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - The contract is one decision with two halves. The collector's STORAGE is promoted to
//!   `array<mixed>` (`crate::types::signatures::descriptor_variadic_container`) so the invoker may
//!   fill the same slot with either an indexed block or a hash, while the collector's SOURCE
//!   element type stays in the signature's declaration metadata and keeps governing direct calls.
//!   The type-level half is asserted in `tests/error_tests/type_system.rs`; this file asserts the
//!   storage half, which only a running program can show.
//! - `call_user_func_array` with a VARIABLE container is the route, because it hands the array to
//!   the descriptor invoker untouched. That is the entry the two argument builders disagree on: a
//!   container with only integer keys takes the indexed builder, one with a string key takes the
//!   hash builder, and both fill the same callee slot.
//! - Every fixture asserts `is_string($key)` per entry, not just the values. A callee compiled for
//!   indexed storage read a tail hash's header as an indexed array (the entry count became the
//!   length and the insertion-order head slot became element 0), which prints plausible-looking
//!   output while being the exact ABI mismatch these tests exist to catch.
//! - Tail values are STRINGS, and every fixture repeats and runs under heap debug. An indexed
//!   release over hash storage never frees the persisted string keys, so the leak summary is the
//!   assertion that pins ownership; a per-call imbalance is invisible in a single pass.
//! - The mixed fixtures pin ORDER too: the positional tail entries keep their renumbered integer
//!   keys and the named entries follow in container order, so a builder that appended the two
//!   groups the other way round cannot pass.

use crate::support::*;

/// A free function's collector reads a positional, a named, and a mixed tail through one container.
#[test]
fn test_core_descriptor_variadic_container_serves_a_free_function() {
    let source = r#"<?php
function collectFreeTail(string $head, ...$rest): string {
    $out = $head . '#' . count($rest);
    foreach ($rest as $key => $value) {
        $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
}
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$positional = ['lead', 'first tail', 'second tail'];
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
$mixed = ['lead', 'first tail', 'beta' => 'named beta'];
echo bindTail(collectFreeTail(...), $positional), '|', bindTail(collectFreeTail(...), $named);
echo '|', bindTail(collectFreeTail(...), $mixed);
echo ':', bindTail(collectFreeTail(...), $positional), '|', bindTail(collectFreeTail(...), $named);
echo '|', bindTail(collectFreeTail(...), $mixed);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let positional = "lead#2;i0=first tail;i1=second tail";
    let named = "lead#1;salpha=named alpha";
    let mixed = "lead#2;i0=first tail;sbeta=named beta";
    assert_eq!(
        out.stdout,
        format!("{positional}|{named}|{mixed}:{positional}|{named}|{mixed}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An INSTANCE method's collector serves the same three tails, through both ways of naming it.
///
/// The callable array and the first-class callable are separate materialization points in the
/// checker, and only one of them used to promote the method's stored signature. Both appear here
/// because the method frame is compiled once: if the two disagreed about the container, whichever
/// came second would be reading the other one's storage.
#[test]
fn test_core_descriptor_variadic_container_serves_an_instance_method() {
    let source = r#"<?php
class TailReceiver {
    public function collect(string $head, ...$rest): string {
        $out = $head . '#' . count($rest);
        foreach ($rest as $key => $value) {
            $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
        }
        return $out;
    }
}
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$receiver = new TailReceiver();
$viaArray = [$receiver, 'collect'];
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
$mixed = ['lead', 'first tail', 'beta' => 'named beta'];
$positional = ['lead', 'first tail', 'second tail'];
echo bindTail($viaArray, $positional), '|', bindTail($viaArray, $named), '|', bindTail($viaArray, $mixed);
echo ':', bindTail($receiver->collect(...), $positional), '|', bindTail($receiver->collect(...), $named);
echo '|', bindTail($receiver->collect(...), $mixed);
echo ':', bindTail($viaArray, $named), '|', bindTail($receiver->collect(...), $mixed);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let positional = "lead#2;i0=first tail;i1=second tail";
    let named = "lead#1;salpha=named alpha";
    let mixed = "lead#2;i0=first tail;sbeta=named beta";
    assert_eq!(
        out.stdout,
        format!("{positional}|{named}|{mixed}:{positional}|{named}|{mixed}:{named}|{mixed}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A STATIC method's collector serves the same three tails, named as an array and as syntax.
///
/// The static table is a separate map from the instance one, so it is a separate promotion target;
/// a contract that only moved instance collectors compiles this fixture's frame for indexed storage
/// and reads the named tail's hash header as a length.
#[test]
fn test_core_descriptor_variadic_container_serves_a_static_method() {
    let source = r#"<?php
class StaticTailReceiver {
    public static function collect(string $head, ...$rest): string {
        $out = $head . '#' . count($rest);
        foreach ($rest as $key => $value) {
            $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
        }
        return $out;
    }
}
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$viaArray = ['StaticTailReceiver', 'collect'];
$positional = ['lead', 'first tail', 'second tail'];
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
$mixed = ['lead', 'first tail', 'beta' => 'named beta'];
echo bindTail($viaArray, $positional), '|', bindTail($viaArray, $named), '|', bindTail($viaArray, $mixed);
echo ':', bindTail(StaticTailReceiver::collect(...), $positional);
echo '|', bindTail(StaticTailReceiver::collect(...), $named);
echo '|', bindTail(StaticTailReceiver::collect(...), $mixed);
echo ':', bindTail($viaArray, $mixed), '|', bindTail(StaticTailReceiver::collect(...), $named);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let positional = "lead#2;i0=first tail;i1=second tail";
    let named = "lead#1;salpha=named alpha";
    let mixed = "lead#2;i0=first tail;sbeta=named beta";
    assert_eq!(
        out.stdout,
        format!("{positional}|{named}|{mixed}:{positional}|{named}|{mixed}:{mixed}|{named}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A CLOSURE's collector serves the same three tails, and its captures survive every shape.
///
/// A closure value is always a descriptor, so its collector never had a reachability question to
/// answer; what it had was two answers, because the body environment and the published signature
/// were seeded from different storage. The capture is echoed back so a frame that read the
/// collector's slot as raw storage cannot pass by returning a plausible count alone.
#[test]
fn test_core_descriptor_variadic_container_serves_a_closure() {
    let source = r#"<?php
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$label = 'closure';
$collect = function (string $head, ...$rest) use ($label): string {
    $out = $label . '/' . $head . '#' . count($rest);
    foreach ($rest as $key => $value) {
        $out .= ';' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
};
$positional = ['lead', 'first tail', 'second tail'];
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
$mixed = ['lead', 'first tail', 'beta' => 'named beta'];
echo bindTail($collect, $positional), '|', bindTail($collect, $named), '|', bindTail($collect, $mixed);
echo ':', bindTail($collect, $positional), '|', bindTail($collect, $named), '|', bindTail($collect, $mixed);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let positional = "closure/lead#2;i0=first tail;i1=second tail";
    let named = "closure/lead#1;salpha=named alpha";
    let mixed = "closure/lead#2;i0=first tail;sbeta=named beta";
    assert_eq!(
        out.stdout,
        format!("{positional}|{named}|{mixed}:{positional}|{named}|{mixed}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A DECLARED `int ...$rest` takes a named tail entry too, because PHP lets a name reach it.
///
/// This is the case a "declared collectors keep their declared storage" rule got wrong: the
/// declaration constrains the ELEMENTS, not the container, so `int ...$rest` is exactly as
/// reachable by a named argument as `...$rest` and needs the same hash-capable storage. The
/// element contract it keeps is asserted where it belongs, as a rejected direct call in
/// `tests/error_tests/type_system.rs`; here the point is that the promoted storage still holds
/// ints, reads a string key, and sums correctly through all three shapes.
#[test]
fn test_core_descriptor_variadic_container_serves_a_declared_typed_collector() {
    let source = r#"<?php
function sumTypedTail(int $head, int ...$rest): string {
    $out = $head . '#' . count($rest) . '=' . array_sum($rest);
    foreach ($rest as $key => $value) {
        $out .= ';' . (is_string($key) ? 's' : 'i') . $key;
    }
    return $out;
}
function bindTail(callable $callback, array $arguments): string {
    return call_user_func_array($callback, $arguments);
}
$positional = [1, 20, 300];
$named = ['head' => 1, 'alpha' => 20];
$mixed = [1, 20, 'beta' => 300];
echo bindTail(sumTypedTail(...), $positional), '|', bindTail(sumTypedTail(...), $named);
echo '|', bindTail(sumTypedTail(...), $mixed);
echo ':', bindTail(sumTypedTail(...), $positional), '|', bindTail(sumTypedTail(...), $named);
echo '|', bindTail(sumTypedTail(...), $mixed);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let positional = "1#2=320;i0;i1";
    let named = "1#1=20;salpha";
    let mixed = "1#2=320;i0;sbeta";
    assert_eq!(
        out.stdout,
        format!("{positional}|{named}|{mixed}:{positional}|{named}|{mixed}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A callee with no collector still refuses an unknown name, so promotion did not widen binding.
///
/// The gate that admits a named tail asks the collector's STORAGE whether a string key fits, and a
/// signature with no collector has nothing to ask. That refusal is the fail-closed half of the
/// contract: a callee whose collector could not be promoted keeps indexed storage and produces
/// this same catchable diagnostic instead of being handed a hash its frame cannot read.
#[test]
fn test_core_descriptor_variadic_container_still_refuses_an_unknown_name() {
    let source = r#"<?php
function noCollector(string $head): string { return $head; }
function bindTail(callable $callback, array $arguments): string {
    try {
        return call_user_func_array($callback, $arguments);
    } catch (Error $error) { return $error->getMessage(); }
}
$named = ['head' => 'lead', 'alpha' => 'named alpha'];
echo bindTail(noCollector(...), $named), ':', bindTail(noCollector(...), $named);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let refused = "Unknown named parameter $alpha";
    assert_eq!(out.stdout, format!("{refused}:{refused}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
