//! Purpose:
//! Verifies PHP argument-unpacking key semantics and ownership for signature-unknown callable
//! descriptors: sparse and associative sources, duplicate names, ordering rules, invalid sources
//! and keys, and the three named-argument binding errors, each under heap debug.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Sources are declared-`array` returns, whose physical storage is a boxed Mixed cell. That is
//!   the representation the indexed unpack walk could not read.
//! - Fixtures pass callbacks through callable parameters to avoid first-class specialization.
//! - Destructor output pins WHEN a payload dies, which a leak summary alone cannot: a duplicate
//!   name must not destroy the entry it would have replaced.
//! - Every fixture repeats, because a per-call imbalance only shows up in a leak summary.
//! - A key that spells a number is the pivot: an ordinary PHP array normalizes `'12'` to the
//!   integer 12 at construction, so it stays POSITIONAL, while a Traversable hands the walk the
//!   raw string, which is the NAME `$12`. Both spellings are asserted, in the same file, so a
//!   change that collapses them cannot pass.
//! - `is_string($key)` is what makes the two distinguishable in output: a string key `'12'` and
//!   an integer key 12 print identically.
//! - Every refusal asserts `Error::getMessage()` verbatim rather than a local marker string. The
//!   rules differ only in which message they produce, so a marker would let two of them swap.

use crate::support::*;

/// Integer keys renumber positionally and string keys bind by name, through both call forms.
#[test]
fn test_core_descriptor_unpack_binds_sparse_and_named_keys() {
    let source = r#"<?php
class UnpackAdder {
    public function add(int $first, int $second): int { return $first * 10 + $second; }
}
function sparseLeading(): array { return [7 => 1]; }
function reorderedNames(): array { return ['second' => 2, 'first' => 1]; }
function throughCallUserFunc(callable $callback): mixed {
    return call_user_func($callback, ...sparseLeading(), second: 2);
}
function throughDescriptorValue(callable $callback): mixed {
    return $callback(...reorderedNames());
}
function throughTypedSource(callable $callback): mixed {
    $leading = [1];
    return call_user_func($callback, ...$leading, second: 2);
}
$callback = [new UnpackAdder(), 'add'];
echo throughCallUserFunc($callback), ':', throughDescriptorValue($callback), ':', throughTypedSource($callback);
echo ':', throughCallUserFunc($callback), ':', throughDescriptorValue($callback);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12:12:12:12:12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "12:12:12:12:12");
}

/// A duplicate name is rejected before the entry it would replace is overwritten.
///
/// A replacement that is never bound must retire before the earlier container entry, and
/// both must retire before a catch in this same frame. A post-write guard reverses that order.
#[test]
fn test_core_descriptor_unpack_rejects_a_duplicate_name_without_overwriting() {
    let source = r#"<?php
class Marker {
    public string $tag = '';
    public function __construct(string $tag) { $this->tag = $tag; }
    public function __destruct() { echo $this->tag, '|'; }
}
class Joiner {
    public function join(Marker $left, Marker $right): string { return $left->tag . $right->tag; }
}
function namedMarker(): array { return ['left' => new Marker('first')]; }
function duplicateNameInFrame(callable $callback): string {
    try {
        return call_user_func($callback, ...namedMarker(), left: new Marker('second'));
    } catch (Error $error) {
        echo 'caught|';
        return $error->getMessage();
    }
}
$callback = [new Joiner(), 'join'];
echo duplicateNameInFrame($callback), ':', duplicateNameInFrame($callback);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let refused = "second|first|caught|Named parameter $left overwrites previous argument";
    assert_eq!(out.stdout, format!("{refused}:{refused}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A positional key after a name, and an unusable source, both raise catchable errors.
#[test]
fn test_core_descriptor_unpack_rejects_bad_order_and_bad_sources() {
    let source = r#"<?php
class UnpackAdder {
    public function add(int $first, int $second): int { return $first * 10 + $second; }
}
function namedFirst(): array { return ['first' => 1]; }
function trailingPositional(): array { return [2]; }
function scalarSource(): mixed { return 7; }
function positionalAfterName(callable $callback): string {
    try {
        call_user_func($callback, ...namedFirst(), ...trailingPositional());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
function invalidSource(callable $callback): string {
    try {
        call_user_func($callback, ...scalarSource(), second: 2);
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
$callback = [new UnpackAdder(), 'add'];
echo positionalAfterName($callback), ':', invalidSource($callback);
echo ':', positionalAfterName($callback), ':', invalidSource($callback);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let ordered = "Cannot use positional argument after named argument during unpacking";
    let rejected = "Only arrays and Traversables can be unpacked";
    assert_eq!(
        out.stdout,
        format!("{ordered}:{rejected}:{ordered}:{rejected}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An unpacked source expression is evaluated exactly once, in source order.
#[test]
fn test_core_descriptor_unpack_evaluates_its_source_once() {
    let source = r#"<?php
class UnpackAdder {
    public function add(int $first, int $second): int { return $first * 10 + $second; }
}

function countedLeading(): array { echo 'source|'; return [1]; }
function countedName(): int { echo 'name|'; return 2; }
function orderedUnpack(callable $callback): mixed {
    return call_user_func($callback, ...countedLeading(), second: countedName());
}
echo orderedUnpack([new UnpackAdder(), 'add']);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "source|name|12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A sole spread still normalizes sparse keys, while iterator keys follow iteration order.
#[test]
fn test_core_descriptor_sole_spread_and_iterator_keys_are_positional() {
    let source = r#"<?php
class UnpackCounter implements Iterator {
    private int $i = 0;
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return 7 + $this->i * 3; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function unpackPair(int $first, int $second): int { return $first * 10 + $second; }
function boxedSparsePair(): array { return [17 => 1, 5 => 2]; }
function directSoleUnpack(callable $callback): mixed { return $callback(...boxedSparsePair()); }
function cufSoleUnpack(callable $callback): mixed { return call_user_func($callback, ...boxedSparsePair()); }
function iteratorUnpack(callable $callback): mixed { return $callback(...new UnpackCounter()); }
echo directSoleUnpack(unpackPair(...)), ':', cufSoleUnpack(unpackPair(...)), ':', iteratorUnpack(unpackPair(...));
echo ':', directSoleUnpack(unpackPair(...)), ':', cufSoleUnpack(unpackPair(...)), ':', iteratorUnpack(unpackPair(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12:12:12:12:12:12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "12:12:12:12:12:12");
}

/// A null-valued name is occupied, and rejecting another spread retires its current entry.
#[test]
fn test_core_descriptor_duplicate_null_name_retires_the_unbound_entry_in_frame() {
    let source = r#"<?php
class DuplicatePayload { public function __destruct() { echo 'unbound|'; } }
function firstNullName(): array { return ['value' => null]; }
function duplicatePayloadName(): array { return ['value' => new DuplicatePayload()]; }
function neverBindValue(mixed $value): void { echo 'called|'; }
function duplicateNullInFrame(callable $callback): string {
    try {
        $callback(...firstNullName(), ...duplicatePayloadName());
        return 'bound';
    } catch (Error $error) {
        echo 'caught|';
        return $error->getMessage();
    }
}
echo duplicateNullInFrame(neverBindValue(...)), ':', duplicateNullInFrame(neverBindValue(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let refused = "unbound|caught|Named parameter $value overwrites previous argument";
    assert_eq!(out.stdout, format!("{refused}:{refused}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Named callable arguments retain their descriptor through the destination hash's lifetime.
#[test]
fn test_core_descriptor_named_callable_argument_keeps_its_bound_receiver() {
    let source = r#"<?php
class NamedCallableToken {
    public function render(): string { return 'ready'; }
    public function __destruct() { echo 'retired|'; }
}
function makeNamedCallable(): callable {
    $token = new NamedCallableToken();
    return $token->render(...);
}
function consumeNamedCallable(callable $inner, string $suffix): string { return $inner() . $suffix; }
function invokeNamedCallable(callable $target): mixed {
    return call_user_func($target, inner: makeNamedCallable(), suffix: '!');
}
echo invokeNamedCallable(consumeNamedCallable(...)), ':', invokeNamedCallable(consumeNamedCallable(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "retired|ready!:retired|ready!", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A statically known non-Traversable object still gets a catchable unpack error.
#[test]
fn test_core_descriptor_rejects_a_typed_non_traversable_source_in_frame() {
    let source = r#"<?php
class NotUnpackable { public function __destruct() { echo 'released|'; } }
function neverUnpack(int $value): void { echo 'called|'; }
function rejectObjectSource(callable $callback): string {
    try {
        $callback(...new NotUnpackable());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
echo rejectObjectSource(neverUnpack(...)), ':', rejectObjectSource(neverUnpack(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let caught = "released|Only arrays and Traversables can be unpacked";
    assert_eq!(out.stdout, format!("{caught}:{caught}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A Traversable numeric-string key is a NAME, so a variadic callee keeps it as a string key.
///
/// `"12"` is what PHP's array-key normalization turns into the integer 12, and that is exactly
/// wrong here: a descriptor container keys parameter names, so the caller asked for `$12`.
#[test]
fn test_core_descriptor_iterator_numeric_string_keys_stay_named_in_a_variadic() {
    let source = r#"<?php
class NumericNameIterator implements Iterator {
    private array $keys = ['12', '13'];
    private int $i = 0;
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return $this->keys[$this->i]; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function collectNamedTail(...$rest): string {
    $out = '';
    foreach ($rest as $key => $value) {
        $out .= (is_string($key) ? 's' : 'i') . $key . '=' . $value . ';';
    }
    return $out;
}
function unpackNumericNames(callable $callback): mixed { return $callback(...new NumericNameIterator()); }
echo unpackNumericNames(collectNamedTail(...)), ':', unpackNumericNames(collectNamedTail(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "s12=1;s13=2;:s12=1;s13=2;", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same Traversable against a non-variadic callee is a catchable unknown-name Error.
#[test]
fn test_core_descriptor_iterator_numeric_string_keys_reject_a_nonvariadic_callee() {
    let source = r#"<?php
class NumericNameIterator implements Iterator {
    private array $keys = ['12', '13'];
    private int $i = 0;
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return $this->keys[$this->i]; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function needsTwoPositions(int $first, int $second): int { return $first * 10 + $second; }
function unpackStrict(callable $callback): string {
    try {
        $callback(...new NumericNameIterator());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
echo unpackStrict(needsTwoPositions(...)), ':', unpackStrict(needsTwoPositions(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "Unknown named parameter $12:Unknown named parameter $12",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An ordinary PHP array keeps PHP key normalization, so those same keys stay positional.
///
/// `['12' => 1]` is an INTEGER-keyed array in PHP, and the descriptor walk renumbers integer
/// keys onto the container's next position. Negative integer keys take the same path: they are
/// positions to renumber, never names.
#[test]
fn test_core_descriptor_array_numeric_and_negative_keys_stay_positional() {
    let source = r#"<?php
function needsTwoPositions(int $first, int $second): int { return $first * 10 + $second; }
function numericStringKeyedArray(): array { return ['12' => 1, '13' => 2]; }
function negativeKeyedArray(): array { return [-3 => 1, -2 => 2]; }
function unpackNumericStrings(callable $callback): mixed { return $callback(...numericStringKeyedArray()); }
function unpackNegatives(callable $callback): mixed { return $callback(...negativeKeyedArray()); }
echo unpackNumericStrings(needsTwoPositions(...)), ':', unpackNegatives(needsTwoPositions(...));
echo ':', unpackNumericStrings(needsTwoPositions(...)), ':', unpackNegatives(needsTwoPositions(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12:12:12:12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "12:12:12:12");
}

/// A Traversable key stays a name even when it spells a negative number or a bare zero.
///
/// `"-1"` and `"0"` are the two shapes `__rt_hash_normalize_key` treats specially for arrays.
/// Neither is special here, because a descriptor container never normalizes a name.
#[test]
fn test_core_descriptor_iterator_signed_string_keys_stay_named() {
    let source = r#"<?php
class SignedNameIterator implements Iterator {
    private array $keys = ['-1', '0'];
    private int $i = 0;
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return $this->keys[$this->i]; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function collectNamedTail(...$rest): string {
    $out = '';
    foreach ($rest as $key => $value) {
        $out .= (is_string($key) ? 's' : 'i') . $key . '=' . $value . ';';
    }
    return $out;
}
function unpackSignedNames(callable $callback): mixed { return $callback(...new SignedNameIterator()); }
echo unpackSignedNames(collectNamedTail(...)), ':', unpackSignedNames(collectNamedTail(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "s-1=1;s0=2;:s-1=1;s0=2;", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A name that also arrives at its own position is rejected, from an array and a Traversable.
///
/// Binding takes the name and would otherwise drop position 0 without a word, which is the one
/// outcome PHP refuses. The message names the parameter so the two candidates are distinguishable.
#[test]
fn test_core_descriptor_named_positional_alias_is_rejected_in_frame() {
    let source = r#"<?php
class AliasIterator implements Iterator {
    private int $i = 0;
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return $this->i === 0 ? 0 : 'a'; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function aliasTarget(int $a, int $b = 0): int { return $a * 10 + $b; }
function aliasedArray(): array { return [0 => 1, 'a' => 2]; }
function unpackAliasArray(callable $callback): string {
    try {
        $callback(...aliasedArray());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
function unpackAliasIterator(callable $callback): string {
    try {
        $callback(...new AliasIterator());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
echo unpackAliasArray(aliasTarget(...)), ':', unpackAliasIterator(aliasTarget(...));
echo ':', unpackAliasArray(aliasTarget(...)), ':', unpackAliasIterator(aliasTarget(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = "Named parameter $a overwrites previous argument";
    assert_eq!(
        out.stdout,
        format!("{expected}:{expected}:{expected}:{expected}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A variadic callee keeps a name no fixed parameter declares, instead of rejecting it.
///
/// This is the boundary of the unknown-name rule: `$rest` is what makes `z` bindable, so the
/// rejection must not fire, and the entry must survive with its own key rather than a position.
#[test]
fn test_core_descriptor_unmatched_name_reaches_the_variadic_tail() {
    let source = r#"<?php
function keepsSpareName(int $a, ...$rest): string {
    $out = (string) $a;
    foreach ($rest as $key => $value) {
        $out .= '|' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
}
function spareNamedArray(): array { return [0 => 1, 'z' => 9]; }
function unpackSpareName(callable $callback): mixed { return $callback(...spareNamedArray()); }
echo unpackSpareName(keepsSpareName(...)), ':', unpackSpareName(keepsSpareName(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|sz=9:1|sz=9", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A Traversable key that is neither an int nor a string is a catchable Error, naming the rule.
///
/// Only a Traversable can reach this: an ordinary PHP array already normalized every key into an
/// int or a string when it was built, so the guard is unreachable through an array source.
#[test]
fn test_core_descriptor_unpack_rejects_an_invalid_key_type_in_frame() {
    let source = r#"<?php
class InvalidKeyIterator implements Iterator {
    private int $i = 0;
    public function __destruct() { echo 'source|'; }
    public function current(): mixed { return $this->i + 1; }
    public function key(): mixed { return 1.5; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function neverBindPair(int $first, int $second): int { return $first * 10 + $second; }
function unpackInvalidKey(callable $callback): string {
    try {
        $callback(...new InvalidKeyIterator());
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
echo unpackInvalidKey(neverBindPair(...)), ':', unpackInvalidKey(neverBindPair(...));
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let refused = "source|Keys must be of type int|string during argument unpacking";
    assert_eq!(out.stdout, format!("{refused}:{refused}"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The invoker classifies a raw associative container in ITS order, not in parameter order.
///
/// Both containers are unbindable twice over, and the assertion is about which refusal PHP
/// reports. `['z' => 1, 0 => 2, 'a' => 3]` against `f($a)` carries an unknown name AND a name
/// aliasing its own position AND a position after a name; `z` comes first, so `z` is the answer.
/// `['a' => 1, 0 => 2]` against `f($a, $b)` has only the alias and the ordering violation, and
/// the position arrives after the name, so the ordering rule is the one that applies.
///
/// `call_user_func_array` with a variable container is the route: it hands the array to the
/// descriptor invoker untouched, which is the associative entry these rules guard.
#[test]
fn test_core_invoker_reports_the_first_unbindable_container_entry() {
    let source = r#"<?php
function takesOne(int $a): int { return $a; }
function takesTwo(int $a, int $b): int { return $a * 10 + $b; }
function bindContainer(callable $callback, array $arguments): string {
    try {
        call_user_func_array($callback, $arguments);
        return 'bound';
    } catch (Error $error) { return $error->getMessage(); }
}
$unknownFirst = ['z' => 1, 0 => 2, 'a' => 3];
$positionAfterName = ['a' => 1, 0 => 2];
echo bindContainer(takesOne(...), $unknownFirst), ':', bindContainer(takesTwo(...), $positionAfterName);
echo ':', bindContainer(takesOne(...), $unknownFirst), ':', bindContainer(takesTwo(...), $positionAfterName);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let unknown = "Unknown named parameter $z";
    let ordered = "Cannot use positional argument after named argument";
    assert_eq!(
        out.stdout,
        format!("{unknown}:{ordered}:{unknown}:{ordered}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The collision rule still applies at a parameter index far past any single-word bitset.
///
/// The earlier implementation remembered arriving positional keys in one 64-bit word and stopped
/// detecting the name/position collision after visible parameter index 63, which made a legal
/// signature length into a silent semantic cliff. This fixture sits on the far side of it: 65
/// visible parameters, and every assertion is about `$p64`.
///
/// Three containers, all built from the same two entries, pin the whole precedence order at that
/// index: position 64 before the name is the collision, position 0 before the name is not (a
/// different position must not answer for `$p64`), and the name before position 64 is the
/// ordering refusal, because a positional key that arrives LATER is never a collision.
#[test]
fn test_core_invoker_detects_a_name_position_collision_past_index_63() {
    let params = (0..65).map(|index| format!("int $p{index} = 0")).collect::<Vec<_>>().join(", ");
    let source = format!(
        r#"<?php
function wideSignature({params}): string {{ return $p0 . '/' . $p64; }}
function bindContainer(callable $callback, array $arguments): string {{
    try {{
        return call_user_func_array($callback, $arguments);
    }} catch (Error $error) {{ return $error->getMessage(); }}
}}
$collidingTail = [64 => 1, 'p64' => 2];
$unrelatedLead = [0 => 1, 'p64' => 2];
$positionAfterName = ['p64' => 2, 64 => 1];
echo bindContainer(wideSignature(...), $collidingTail);
echo ':', bindContainer(wideSignature(...), $unrelatedLead);
echo ':', bindContainer(wideSignature(...), $positionAfterName);
echo ':', bindContainer(wideSignature(...), $collidingTail);
echo ':', bindContainer(wideSignature(...), $unrelatedLead);
echo ':', bindContainer(wideSignature(...), $positionAfterName);
"#
    );
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let collided = "Named parameter $p64 overwrites previous argument";
    let ordered = "Cannot use positional argument after named argument";
    assert_eq!(
        out.stdout,
        format!("{collided}:1/2:{ordered}:{collided}:1/2:{ordered}"),
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
