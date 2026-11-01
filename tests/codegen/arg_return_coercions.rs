//! Purpose:
//! Integration regressions for PHP call/return boundaries and flow-sensitive local storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures exercise runtime narrowing, representation changes, exceptions, value
//!   shape, and ownership behavior without relying on application-specific source patterns.

use crate::support::*;

/// Verifies a nullable local widened by a loop is loaded through its runtime storage type after
/// the loop, even when the final flow snapshot still describes the pre-loop null value.
#[test]
fn test_loop_widened_nullable_local_returns_runtime_value() {
    let out = compile_and_run(
        r#"<?php
abstract class SearchableValue {
    public function lastIndex(string|iterable $needle, int $offset = 0): ?int {
        if (is_string($needle)) {
            throw new TypeError('scalar search must be overridden');
        }

        $index = null;
        foreach ($needle as $value) {
            $candidate = $this->lastIndex((string) $value, $offset);
            if (null !== $candidate && $candidate >= $index) {
                $index = $offset = $candidate;
            }
        }

        return $index;
    }
}

final class ConcreteSearchableValue extends SearchableValue {
    public function lastIndex(string|iterable $needle, int $offset = 0): ?int {
        if (is_string($needle)) {
            return (int) $needle;
        }

        return parent::lastIndex($needle, $offset);
    }
}

echo (new ConcreteSearchableValue())->lastIndex(['1', '3', '2']);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies concrete arrays and Traversable objects satisfy an `iterable` return by forwarding the
/// runtime payload instead of emitting a placeholder heap coercion.
#[test]
fn test_concrete_values_return_as_iterable() {
    let out = compile_and_run(
        r#"<?php
class ReturnIterator implements Iterator {
    private int $position = 0;
    public function current(): mixed { return 5; }
    public function key(): mixed { return $this->position; }
    public function next(): void { $this->position++; }
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 1; }
}
function indexedValues(): iterable {
    return [1, 2];
}
function associativeValues(): iterable {
    return ['left' => 3, 'right' => 4];
}
function objectValues(): iterable {
    return new ReturnIterator();
}
foreach (indexedValues() as $value) {
    echo $value;
}
echo '|';
foreach (associativeValues() as $key => $value) {
    echo $key.$value;
}
echo '|';
foreach (objectValues() as $value) {
    echo $value;
}
"#,
    );
    assert_eq!(out, "12|left3right4|5");
}

/// Verifies a boxed Mixed array narrows before assignment into a declared iterable property.
///
/// Property writes use the same declared boundary as parameters and returns: a runtime array
/// carried in Mixed storage must become iterable payload storage before `PropSet`, rather than
/// reaching codegen as an incompatible boxed cell.
#[test]
fn test_mixed_array_assigns_to_declared_iterable_property() {
    let out = compile_and_run(
        r#"<?php
class IterablePropertyBox {
    private iterable $values;

    public function __construct(mixed $values) {
        $this->values = $values;
    }

    public function render(): string {
        $result = '';
        foreach ($this->values as $value) {
            $result .= $value;
        }

        return $result;
    }
}

echo (new IterablePropertyBox(['a', 'b']))->render();
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies an exception handler inside a statically infinite loop does not keep the loop's
/// synthetic exit reachable and materialize a zero-operand `iterable` return placeholder.
#[test]
fn test_infinite_loop_with_exception_handler_has_no_synthetic_iterable_fallthrough() {
    let out = compile_and_run(
        r#"<?php
function maybeThrow(bool $throw): void {
    if ($throw) {
        throw new Exception('boom');
    }
}
function values(bool $throw): iterable {
    while (true) {
        try {
            maybeThrow($throw);
            return ['ok' => 7];
        } catch (Exception $e) {
            return [];
        }
    }
}
foreach (values(false) as $key => $value) {
    echo $key.$value;
}
"#,
    );
    assert_eq!(out, "ok7");
}

/// Verifies an owning array temporary transferred to an `iterable` return remains balanced across
/// repeated calls and result cleanup.
#[test]
fn test_array_temporary_returned_as_iterable_has_balanced_ownership() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function returnedValues(int $value): iterable {
    return [$value, $value + 1];
}
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    foreach (returnedValues($i) as $value) {
        $sum += $value;
    }
}
echo $sum;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "10000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A PHP `array` return contract accepts associative storage even when the inferred values are
/// heterogeneous and therefore boxed. This is the shape produced by key-preserving
/// `array_unique(array_merge(...))` helpers.
#[test]
fn test_assoc_mixed_result_satisfies_untyped_array_return() {
    let out = compile_and_run(
        r#"<?php
function makeCandidateList(): array {
    $values = ["first" => "alpha", "number" => 7, "duplicate" => "alpha"];
    return array_unique($values);
}
$result = makeCandidateList();
echo count($result), "|", $result["first"], "|", $result["number"];
"#,
    );
    assert_eq!(out, "2|alpha|7");
}

/// Verifies a boxed object is checked against the declared parameter type and unboxed without
/// depending on the caller's source-level inference.
#[test]
fn test_mixed_object_argument_is_narrowed_at_parameter_boundary() {
    let out = compile_and_run(
        r#"<?php
final class Box {
    public function value(): int { return 7; }
}
function take(Box $box): int { return $box->value(); }
function relay(mixed $value): int { return take($value); }
echo relay(new Box());
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies a boxed value of the wrong runtime type raises a catchable `TypeError` at an object
/// parameter boundary.
#[test]
fn test_mixed_object_argument_rejects_wrong_runtime_type() {
    let out = compile_and_run(
        r#"<?php
final class Box {}
function take(Box $box): void { echo 'bad'; }
function relay(mixed $value): void { take($value); }
try {
    relay(42);
} catch (TypeError $error) {
    echo 'caught';
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies boxed unions use the same checked object boundary as a value declared directly as
/// `mixed`, including the catchable failure path for the union's null alternative.
#[test]
fn test_nullable_object_argument_is_narrowed_at_parameter_boundary() {
    let out = compile_and_run(
        r#"<?php
final class Box {
    public function value(): int { return 9; }
}
function take(Box $box): int { return $box->value(); }
function choose(bool $present): Box|null {
    return $present ? new Box() : null;
}
echo take(choose(true)), '|';
try {
    take(choose(false));
} catch (TypeError $error) {
    echo 'caught';
}
"#,
    );
    assert_eq!(out, "9|caught");
}

/// Verifies a boxed union with several nominal alternatives is checked at a nullable object
/// parameter boundary instead of being rejected statically.
#[test]
fn test_multi_object_union_argument_uses_runtime_nominal_boundary() {
    let out = compile_and_run(
        r#"<?php
interface AcceptedValue {}
interface OtherValue {}
final class AcceptedObject implements AcceptedValue {}
final class OtherObject implements OtherValue {}

function choose(int $kind): AcceptedValue|OtherValue|null {
    if ($kind === 1) { return new AcceptedObject(); }
    if ($kind === 2) { return new OtherObject(); }
    return null;
}
function take(?AcceptedValue $value): string {
    return $value === null ? 'null' : 'accepted';
}

echo take(choose(1)), '|', take(choose(0));
try {
    take(choose(2));
} catch (TypeError $error) {
    echo '|caught';
}
"#,
    );
    assert_eq!(out, "accepted|null|caught");
}

/// Verifies a boxed scalar union can cross a matching declared parameter boundary when its
/// runtime alternative satisfies that declaration.
#[test]
fn test_nullable_string_argument_uses_boxed_boundary_conversion() {
    let out = compile_and_run(
        r#"<?php
function take(string $value): void { echo $value; }
function choose(bool $present): string|null {
    return $present ? 'ok' : null;
}
take(choose(true));
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a weak call coerces an integer read from a mixed array into a nullable string
/// parameter, matching PHP's scalar argument rules at a method boundary.
#[test]
fn test_nullable_string_parameter_coerces_integer_from_array() {
    let out = compile_and_run(
        r#"<?php
class StringWidth {
    public static function of(?string $value): int {
        return strlen($value ?? '');
    }
}
class TableLike {
    public function firstWidth(array $row): int {
        return StringWidth::of($row[0]);
    }
    public function namedWidth(array $row): int {
        return StringWidth::of(value: $row[0]);
    }
}
class Displayable {
    public function __toString(): string { return 'wide'; }
}
$table = new TableLike();
echo $table->firstWidth([42]), ':', $table->namedWidth([43]), ':';
echo $table->firstWidth([false]), ':', $table->firstWidth([null]), ':';
echo $table->firstWidth([new Displayable()]);
"#,
    );
    assert_eq!(out, "2:2:0:0:4");
}

/// Verifies a gradual value keeps strict call-site semantics for a nullable string union.
#[test]
fn test_nullable_string_parameter_rejects_integer_from_array_in_strict_mode() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
class StringWidth {
    public static function of(?string $value): int {
        return strlen($value ?? '');
    }
}
class TableLike {
    public function firstWidth(array $row): string {
        try {
            StringWidth::of($row[0]);
            return 'accepted';
        } catch (TypeError $error) {
            return 'rejected';
        }
    }
}
echo (new TableLike())->firstWidth([42]);
"#,
    );
    assert_eq!(out, "rejected");
}

/// Verifies weak scalar binding does not turn an array into the string "Array".
#[test]
fn test_nullable_string_parameter_rejects_array_from_array_in_weak_mode() {
    let out = compile_and_run(
        r#"<?php
class StringWidth {
    public static function of(?string $value): int {
        return strlen($value ?? '');
    }
}
class TableLike {
    public function firstWidth(array $row): string {
        try {
            StringWidth::of($row[0]);
            return 'accepted';
        } catch (TypeError $error) {
            return 'rejected';
        }
    }
}
echo (new TableLike())->firstWidth([['not a scalar']]);
"#,
    );
    assert_eq!(out, "rejected");
}

/// Verifies an empty first argument does not freeze a declared generic array parameter to the
/// empty-literal placeholder and reject later populated arrays.
#[test]
fn test_generic_array_parameter_ignores_empty_literal_specialization() {
    let out = compile_and_run(
        r#"<?php
final class Counter {
    public function size(array $values): int { return count($values); }
}
$counter = new Counter();
echo $counter->size([]), '|', $counter->size([1, 2]);
"#,
    );
    assert_eq!(out, "0|2");
}

/// Verifies a declared bare array contract accepts indexed and associative values at every direct
/// call surface while representation normalization leaves the caller's containers readable.
#[test]
fn test_generic_array_parameter_accepts_multiple_runtime_shapes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function firstValue(array $values): mixed {
    foreach ($values as $value) { return $value; }
    return null;
}
function firstKey(array $values): mixed {
    foreach ($values as $key => $value) { return $key; }
    return null;
}
final class ArrayReader {
    public function first(array $values): mixed {
        foreach ($values as $value) { return $value; }
        return null;
    }
    public static function firstStatic(array $values): mixed {
        foreach ($values as $value) { return $value; }
        return null;
    }
    public function firstKey(array $values): mixed {
        foreach ($values as $key => $value) { return $key; }
        return null;
    }
    public static function firstStaticKey(array $values): mixed {
        foreach ($values as $key => $value) { return $key; }
        return null;
    }
}
$assoc = ['left' => 1];
$indexed = [2, 3];
$reader = new ArrayReader();
echo firstValue($assoc), firstValue($indexed), '|';
echo $reader->first($assoc), $reader->first($indexed), '|';
echo ArrayReader::firstStatic($assoc), ArrayReader::firstStatic($indexed), '|';
echo $assoc['left'], $indexed[0], '|';
echo firstKey($assoc), '|';
echo $reader->firstKey($assoc), '|';
echo ArrayReader::firstStaticKey($assoc);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "12|12|12|12|left|left|left");
}

/// Verifies boxed indexed and associative arrays satisfy a generic array return contract while
/// preserving their keys and values.
#[test]
fn test_mixed_array_return_is_narrowed_at_return_boundary() {
    let out = compile_and_run(
        r#"<?php
function relay(mixed $value): array { return $value; }
$indexed = relay([2, 3]);
$assoc = relay(['left' => 5]);
echo $indexed[0], $indexed[1], '|', $assoc['left'];
"#,
    );
    assert_eq!(out, "23|5");
}

/// Verifies a boxed non-array raises a catchable `TypeError` at a generic array return boundary.
#[test]
fn test_mixed_array_return_rejects_wrong_runtime_type() {
    let out = compile_and_run(
        r#"<?php
function relay(mixed $value): array { return $value; }
try {
    relay('not an array');
} catch (TypeError $error) {
    echo 'caught';
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies an ordinary PHP local may change representation across sequential assignments while
/// earlier and later reads continue to use the final boxed frame slot safely.
#[test]
fn test_untyped_local_can_be_reassigned_across_runtime_types() {
    let out = compile_and_run(
        r#"<?php
$value = 42;
echo $value, '|';
$value = 'text';
echo $value, '|';
$value = ['key' => 9];
echo $value['key'];
"#,
    );
    assert_eq!(out, "42|text|9");
}

/// Verifies a returned `?int` that is null is falsy in every truthiness position.
///
/// The value lives in inline tagged-scalar storage whose PHP type is the `int|null` union, so the
/// truthiness lowering must branch on the storage first: matched on the PHP type alone it took
/// the boxed-`Mixed` arm and the program died at the first `$m ? … : …`.
#[test]
fn test_nullable_int_return_truthiness_reads_tagged_storage() {
    let out = compile_and_run(
        r#"<?php
function maybe(int $i): ?int { return $i > 1 ? $i : null; }
for ($i = 0; $i < 3; $i++) {
    $m = maybe($i);
    echo $m ? "t" : "f", (bool) $m ? "T" : "F";
    if ($m) { echo "m", $m; }
    while ($m) { $m = null; echo "w"; }
    echo "|";
}
"#,
    );
    assert_eq!(out, "fF|fF|tTm2w|");
}

/// A local array returned under an `iterable` declaration reaches the caller alive. The return
/// used to count as a fresh owner while handing back the local's own storage, so the epilogue
/// released the array and a `mixed` receiver boxed the freed block as NULL (and without a boxed
/// receiver the freed array was read back by luck). Covers list, hash and object sources, with and
/// without eval, and runs under the heap validator so a leak or double release fails the test.
#[test]
fn test_iterable_return_of_a_local_container_survives_the_epilogue() {
    for with_eval in [false, true] {
        let eval = if with_eval { r#"eval("echo 1;");"# } else { "" };
        let source = format!(
            r#"<?php
class T {{
    public function fill(iterable $row): iterable {{ $n = []; foreach ($row as $c) {{ $n[] = $c; }} return $n; }}
    public function hash(): iterable {{ $n = ["k" => 1]; return $n; }}
    public function obj(): iterable {{ $o = new ArrayIterator([3, 4]); return $o; }}
}}
function v(mixed $o) {{ return [$o->fill(["a", "b"]), $o->hash(), $o->obj()]; }}
for ($i = 0; $i < 3; $i++) {{
    [$a, $h, $it] = v(new T);
    echo gettype($a), count($a), gettype($h), count($h), ($it instanceof ArrayIterator ? "AI" : "?"), count($it), "\n";
}}
{eval}
"#
        );
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "program failed (eval={with_eval}): {}", out.stderr);
        let expected = "array2array1AI2\n".repeat(3) + if with_eval { "1" } else { "" };
        assert_eq!(out.stdout, expected, "eval={with_eval}");
    }
}

/// A generic `array` built with a Mixed key is promoted to a hash at runtime while its static type
/// stays `array<mixed>`. `call_user_func_array()` must read the container's heap kind, not its
/// static type: cloning the hash as a packed array crashed the named-argument normalizer
/// (php prints `3|NULL|2.5|[1,2]` and `3|5`).
#[test]
fn test_call_user_func_array_with_runtime_promoted_hash_container() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Pt { public function __construct(public int $x = 0) {} }
function objp(Pt $p, ?string $label = null, float $scale = 1.5, array $extra = []) {
    return $p->x . "|" . var_export($label, true) . "|" . $scale . "|" . json_encode($extra);
}
function plain(int $q, int $s = 1) { return $q . "|" . $s; }
function A(...$kv) {
    $r = [];
    for ($i = 0; $i < count($kv); $i += 2) { $r[$kv[$i]] = $kv[$i + 1]; }
    return $r;
}
echo call_user_func_array('objp', A('p', new Pt(3), 'scale', 2.5, 'extra', [1, 2])), "\n";
echo call_user_func_array('plain', A('s', 5, 'q', 3)), "\n";
echo call_user_func_array('plain', A(0, 7, 1, 8)), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3|NULL|2.5|[1,2]\n3|5\n7|8\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A dynamic `new $class($array['k'], ...)` binds array elements by value. An unrelated class of
/// the program with a by-reference constructor parameter must not make the call a compile error
/// (Symfony's RedisTrait `new $sentinelClass($host, $port, $params['timeout'], ...$extra)`).
#[test]
fn test_dynamic_new_with_array_element_arguments_ignores_unrelated_by_ref_constructors() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ByRefBag { public function __construct($a, $b, &$bag) { $bag = 1; } }
class Sentinel {
    public array $seen;
    public function __construct($host, $port, $timeout = 0.0, $persistent = '', ...$rest) {
        $this->seen = [$host, $port, $timeout, $persistent, count($rest)];
    }
}
$params = ['timeout' => 1.5, 'persistent_id' => 7];
$extra = [10, 20];
$class = $argc > 5 ? 'ByRefBag' : 'Sentinel';
$s = @new $class('h', 6379, $params['timeout'], (string) $params['persistent_id'], ...$extra);
echo json_encode($s->seen), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[\"h\",6379,1.5,\"7\",2]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// String-keyed unpacks combined with other arguments bind by key at run time, and a key that hits
/// an already supplied parameter raises php's catchable `Named parameter $x overwrites previous
/// argument` Error. Omitted parameters take their literal defaults.
#[test]
fn test_string_keyed_spread_with_other_arguments_binds_and_reports_overwrites() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($a, $b = 2, $c = 3) { return "$a|$b|$c"; }
function t($label, $fn) {
    try { $r = $fn(); echo $label, ": ", $r, "\n"; } catch (Throwable $e) { echo $label, ": ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
$str = ['a' => 4];
$str2 = ['b' => 4, 'c' => 6];
$mix = ['a' => 1, 'b' => 'x'];
t("positional plus named hash", fn() => f(1, ...$str2));
t("positional overwritten", fn() => f(1, ...$str));
t("mixed values", fn() => f(...$mix));
t("mixed overwritten", fn() => f(1, ...$mix));
t("duplicate hash", fn() => f(...$str, ...$str));
t("disjoint hashes", fn() => f(...$str, ...$str2));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "positional plus named hash: 1|4|6\n\
         positional overwritten: Error: Named parameter $a overwrites previous argument\n\
         mixed values: 1|x|3\n\
         mixed overwritten: Error: Named parameter $a overwrites previous argument\n\
         duplicate hash: Error: Named parameter $a overwrites previous argument\n\
         disjoint hashes: 4|4|6\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `ReflectionProperty::setValue($e, $trace)` replaces a Throwable's private `trace`. The slot
/// holds the RAW array `__rt_throwable_capture_trace` published, while the store releases the old
/// value as the slot's declared `array` storage (a boxed Mixed): the raw array's header read as a
/// tag, so a trace of exactly 12 frames (tag 12, a global reference marker) faulted -- Symfony's
/// `ClassExistenceResource::throwOnRequiredClass()` and `ErrorHandler` hit it in
/// `debug:container --parameters`. Every other length released a wrong block silently.
#[test]
fn test_reflection_set_value_replaces_a_captured_throwable_trace_of_any_length() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function deep($n) {
    if ($n > 0) { return deep($n - 1); }
    $e = new ErrorException("m", 0, E_USER_DEPRECATED, "f.php", 3);
    $r = new ReflectionProperty(Exception::class, 'trace');
    $r->setValue($e, [['function' => 'x'], ['function' => 'y']]);
    return count($e->getTrace()) . ":" . $e->getTrace()[1]['function'];
}
foreach ([0, 1, 3, 9, 10, 11, 12, 13] as $d) { echo $d, "=", deep($d), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0=2:y\n1=2:y\n3=2:y\n9=2:y\n10=2:y\n11=2:y\n12=2:y\n13=2:y\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A by-reference parameter pushed into a persistent array (`$refs[] = &$r`) escapes the call: the
/// caller must give the argument a managed cell. Lending its frame slot let the array's marker
/// retain and release a "header" inside the caller's stack, ending in a bad refcount at exit.
#[test]
fn test_by_ref_parameter_stored_into_a_global_array_gets_a_managed_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$refs = [];
function keep(&$r) { global $refs; $refs[] = &$r; }
for ($i = 0; $i < 3; $i++) {
    $v = "value" . $i;
    keep($v);
    $v = "changed" . $i;
    echo $refs[$i], "\n";
}
echo count($refs), "\n";
$refs = [];
echo "done\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "changed0\nchanged1\nchanged2\n3\ndone\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Symfony's `AutowiringFailedException` hands `$this->message` to an anonymous class constructor
/// that stores the reference (`$this->message = &$message`) so the message can be built lazily.
/// The builtin payload slot cannot become a reference cell, so the constructor binds a copy and the
/// text is produced when the object is assigned to the slot. The final messages equal php's (php
/// runs the callback at the first `getMessage()`, this at the assignment; both run it once).
#[test]
fn test_constructor_storing_a_throwable_payload_reference_still_compiles() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class LazyFailure extends Exception {
    private ?Closure $messageCallback = null;
    public function __construct(string|Closure $message, int $code = 0, ?Throwable $previous = null) {
        if (!$message instanceof Closure) {
            parent::__construct($message, $code, $previous);
            return;
        }
        $this->messageCallback = $message;
        parent::__construct('', $code, $previous);
        $this->message = new class($this->message, $this->messageCallback) {
            private string|self $message;
            private ?Closure $messageCallback;
            public function __construct(&$message, &$messageCallback) {
                $this->message = &$message;
                $this->messageCallback = &$messageCallback;
            }
            public function __toString(): string {
                $messageCallback = $this->messageCallback;
                $this->messageCallback = null;
                try {
                    return $this->message = $messageCallback();
                } catch (Throwable $e) {
                    return $this->message = $e->getMessage();
                }
            }
        };
    }
}
$calls = 0;
try {
    throw new LazyFailure(function () use (&$calls) { $calls++; return "lazy message"; }, 3);
} catch (LazyFailure $e) {
    echo $e->getMessage(), "|", $e->getMessage(), "|", $calls, "|", $e->getCode(), "\n";
}
echo (new LazyFailure("plain"))->getMessage(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "lazy message|lazy message|1|3\nplain\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// An unpack that supplies the FORMAT as well (`sprintf(...$args)`, `printf(...$args)`,
/// `fprintf($h, ...$args)`) formats its elements: it was handed to the packed lowering as one
/// array argument ("Array to string conversion", then "too few arguments"). Symfony's
/// `TextDescriptor` prints container parameter deprecations with
/// `sprintf(...array_slice($deprecated, 2))`, which came out as `The "Array" parameter`.
#[test]
fn test_printf_family_unpack_supplying_the_format() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function source() { return ['x', 'y', '%s=%s', 'k', 'v']; }
$a = ['%s-%s', 'a', 'b'];
echo sprintf(...$a), "\n";
echo sprintf(...['%s|%s', 'c', 'd']), "\n";
echo sprintf(...array_slice(source(), 2)), "\n";
$n = printf(...['%s;%s', 'p', 'q']);
echo "|", $n, "\n";
fprintf(STDOUT, ...['%s+%s', 'm', 'n']);
echo "\n";
echo sprintf('<%s>', sprintf(...array_slice(['a', 'b', 'Since %s: %s', 'one', 'two'], 2))), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a-b\nc|d\nk=v\np;q|3\nm+n\n<Since one: two>\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `new $class($array['k'])` binds the element by reference only for a candidate whose constructor
/// declares that position as a reference, and by value for every other class, as php decides per
/// class at run time. Owned temporary arguments of the generic by-value path are retired too.
#[test]
fn test_dynamic_new_array_element_binds_by_reference_only_for_by_ref_candidates() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ByRef { public function __construct($host, &$slot) { $slot = "set-by-ByRef:" . $host; } }
class ByVal { public array $seen; public function __construct($host, $slot) { $this->seen = [$host, $slot]; $slot = "ignored"; } }
function build(string $cls, array $params) {
    $o = new $cls('h', $params['timeout']);
    return [$o, $params];
}
[$a, $p1] = build('ByRef', ['timeout' => 'orig']);
echo get_class($a), " ", $p1['timeout'], "\n";
[$b, $p2] = build('ByVal', ['timeout' => 'orig']);
echo get_class($b), " ", $p2['timeout'], " ", json_encode($b->seen), "\n";
$arr = ['k' => 1, 'z' => 2];
$cls = $argc > 5 ? 'ByVal' : 'ByRef';
$x = new $cls('q', $arr['k']);
echo json_encode($arr), "\n";
$c2 = strtoupper('byval');
$y = new $c2('w', strtoupper('v'));
echo json_encode($y->seen), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "ByRef set-by-ByRef:h\nByVal orig [\"h\",\"orig\"]\n{\"k\":\"set-by-ByRef:q\",\"z\":2}\n[\"w\",\"V\"]\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// `Closure::bind(fn &() => $this->p, $object, $object)` with an object of one known class makes
/// only THAT class's property a reference. Promoting `p` on every class declaring that name (the
/// answer for an unknown receiver) turned an unrelated class's property into an owned reference
/// cell the object walkers do not read: `(array)` gave `null`, `var_export` printed nothing, and
/// clones of a shell object shared one cell, so writes through one definition showed in all of them.
/// This is how Symfony's `KernelTrait` (`fn &() => $this->instanceof` bound to the loader) made
/// every container definition's `instanceof` conditionals alias one another.
#[test]
fn test_closure_bind_with_known_receiver_does_not_promote_unrelated_classes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class D {
    private array $instanceof = [];
    public function getI(): array { return $this->instanceof; }
    public function setI(array $i): static { $this->instanceof = $i; return $this; }
}
class L { protected array $instanceof = []; }
function main(): void {
    $l = new L();
    $ref = &\Closure::bind(fn &() => $this->instanceof, $l, $l)();
    $proto = new D();
    $proto->setI(['seed' => 1]);
    $cast = (array) $proto;
    echo count($cast), " ", json_encode(array_values($cast)), "\n";
    $ds = [];
    foreach (['A', 'B', 'C'] as $n) { $ds[$n] = clone $proto; }
    foreach ($ds as $n => $d) {
        $a = $d->getI();
        $a[$n] = 1;
        $d->setI($a);
    }
    foreach ($ds as $n => $d) { echo $n, json_encode(array_keys($d->getI())), " "; }
    echo json_encode(array_keys($proto->getI())), "\n";
    var_export($proto);
    echo "\n";
}
main();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1 [{\"seed\":1}]\nA[\"seed\",\"A\"] B[\"seed\",\"B\"] C[\"seed\",\"C\"] [\"seed\"]\n\\D::__set_state(array(\n   'instanceof' => \n  array (\n    'seed' => 1,\n  ),\n))\n"
    );
}

/// An attribute argument spelled `self::CONST` (even a private constant) is evaluated in the
/// declaring class: the attribute stays reflectable, statically and through a runtime class name.
/// It was dropped altogether (`getAttributes()` answered `[]`), so Symfony's
/// `#[AsCommand(name: 'app:add-user', help: self::HELP)]` command lost its name.
#[test]
fn test_attribute_argument_naming_a_self_constant_is_reflectable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[Attribute(Attribute::TARGET_CLASS)]
final class Cmd { public function __construct(public string $name, public ?string $help = null) {} }
class Base { const BASE_HELP = 'base help'; }
#[Cmd(name: 'x', help: self::HELP)]
final class AddUser { private const HELP = 'secret help'; }
#[Cmd(name: 'y', help: parent::BASE_HELP)]
final class Child extends Base {}
#[Cmd(name: 'z', help: 'plain')]
final class Other {}
function show(string $c): void {
    $attrs = (new ReflectionClass($c))->getAttributes(Cmd::class);
    echo $c, " ", count($attrs), " ", json_encode($attrs[0]->getArguments()), " ", $attrs[0]->newInstance()->help, "\n";
}
show(AddUser::class);
show(Child::class);
show(Other::class);
$static = (new ReflectionClass(AddUser::class))->getAttributes(Cmd::class);
echo count($static), " ", $static[0]->newInstance()->help, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "AddUser 1 {\"name\":\"x\",\"help\":\"secret help\"} secret help\n\
         Child 1 {\"name\":\"y\",\"help\":\"base help\"} base help\n\
         Other 1 {\"name\":\"z\",\"help\":\"plain\"} plain\n\
         1 secret help\n"
    );
}

/// An omitted `?int $p = null` parameter reaches the constructor through the descriptor invoker
/// (`ReflectionAttribute::newInstance()`), which pushed a lone zero for the default: the
/// `{payload, tag}` pair's tag word was stale stack memory. The property then read as a non-null
/// value of a junk tag (`??` kept it, `is_null()` said false, `isset()` said true), so Symfony's
/// `Route` attribute filed every attribute route under priority 1 and `debug:router` reordered
/// the whole route table. A dynamic `class_exists()` keeps the frame on the eval-bridge read.
#[test]
fn test_omitted_nullable_int_default_through_the_invoker_is_a_real_null() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[Attribute]
class R {
    public function __construct(public ?int $priority = null, public ?int $p5 = 5) {} }
#[R]
class C {}
function main(): void {
    $nm = $_SERVER['argv'][1] ?? 'C';
    var_dump(class_exists($nm));
    $o = (new ReflectionClass($nm))->getAttributes(R::class)[0]->newInstance();
    $v = $o->priority;
    var_dump($v === null, is_null($v), isset($v), $v);
    var_dump($o->p5 === 5);
    var_dump($o->priority ?? 'dflt', isset($o->priority));
}
main();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nbool(true)\nbool(false)\nNULL\nbool(true)\nstring(4) \"dflt\"\nbool(false)\n"
    );
}

/// `(array) $o` and `get_object_vars($o)` answered `null` for every property stored as a boxed
/// Mixed (`array`, `mixed`, `?string`, any union): the projection handed the slot's box to
/// `__rt_mixed_from_value` as tag 7, which wrapped it in a second Mixed cell no reader unwrapped.
#[test]
fn test_array_cast_and_get_object_vars_keep_typed_mixed_properties() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class B1 { public array $p = ['a' => 1]; public mixed $m = [2]; public ?string $t = 'tt'; public array $e = []; public int $n = 3; }
class B2 { public ?int $q = null; public ?int $r = 4; public $u = ['z' => 1]; }
function show(object $o): void {
    echo json_encode((array) $o), "\n", json_encode(get_object_vars($o)), "\n";
    foreach ($o as $k => $v) { echo $k, "=", json_encode($v), " "; }
    echo "\n";
}
function main(): void {
    show(new B1);
    show(new B2);
    $c = (array) new B1;
    $c['p']['b'] = 9;
    echo json_encode($c['p']), "\n";
}
main();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"p\":{\"a\":1},\"m\":[2],\"t\":\"tt\",\"e\":[],\"n\":3}\n{\"p\":{\"a\":1},\"m\":[2],\"t\":\"tt\",\"e\":[],\"n\":3}\np={\"a\":1} m=[2] t=\"tt\" e=[] n=3 \n{\"q\":null,\"r\":4,\"u\":{\"z\":1}}\n{\"q\":null,\"r\":4,\"u\":{\"z\":1}}\nq=null r=4 u={\"z\":1} \n{\"a\":1,\"b\":9}\n"
    );
}

/// Object walkers (`(array)`, `get_object_vars`, `foreach`, `var_export`, `print_r`, `var_dump`,
/// `serialize`, `json_encode`) read a property that took part in `$r = &$o->p`: its slot holds a
/// pointer to a reference cell and the class descriptor said "Mixed" for it, so each walker saw
/// garbage (`null` for the value, an empty `var_export`). The descriptor now carries the
/// declared type's tag plus a reference flag and every walker follows the cell. php's `&`
/// reference markers in `var_dump` are stripped: the compiled runtime does not print them.
#[test]
fn test_object_walkers_read_reference_bound_properties() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public array $p = ['a' => 1]; public int $n = 5; public ?int $q = null; public string $s = 'x'; public mixed $m = 'mm'; public ?string $t = null; }
function main(): void {
    $o = new P();
    $r = &$o->p; $r['b'] = 2;
    $rn = &$o->n; $rn = 6;
    $rq = &$o->q; $rq = 8;
    $rs = &$o->s; $rs = 'yy';
    $rm = &$o->m; $rm = [1];
    $rt = &$o->t; $rt = 'tt';
    echo json_encode((array) $o), "\n";
    echo json_encode(get_object_vars($o)), "\n";
    var_export($o); echo "\n";
    echo json_encode($o), "\n";
    echo serialize($o), "\n";
    foreach ($o as $k => $v) { echo $k, "=", json_encode($v), " "; }
    echo "\n";
    print_r($o);
    ob_start(); var_dump($o); echo str_replace('&', '', ob_get_clean());
    $c = clone $o;
    $c->p['z'] = 9;
    echo json_encode($o->p), json_encode($c->p), "\n";
}
main();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"p\":{\"a\":1,\"b\":2},\"n\":6,\"q\":8,\"s\":\"yy\",\"m\":[1],\"t\":\"tt\"}\n{\"p\":{\"a\":1,\"b\":2},\"n\":6,\"q\":8,\"s\":\"yy\",\"m\":[1],\"t\":\"tt\"}\n\\P::__set_state(array(\n   'p' => \n  array (\n    'a' => 1,\n    'b' => 2,\n  ),\n   'n' => 6,\n   'q' => 8,\n   's' => 'yy',\n   'm' => \n  array (\n    0 => 1,\n  ),\n   't' => 'tt',\n))\n{\"p\":{\"a\":1,\"b\":2},\"n\":6,\"q\":8,\"s\":\"yy\",\"m\":[1],\"t\":\"tt\"}\nO:1:\"P\":6:{s:1:\"p\";a:2:{s:1:\"a\";i:1;s:1:\"b\";i:2;}s:1:\"n\";i:6;s:1:\"q\";i:8;s:1:\"s\";s:2:\"yy\";s:1:\"m\";a:1:{i:0;i:1;}s:1:\"t\";s:2:\"tt\";}\np={\"a\":1,\"b\":2} n=6 q=8 s=\"yy\" m=[1] t=\"tt\" \nP Object\n(\n    [p] => Array\n        (\n            [a] => 1\n            [b] => 2\n        )\n\n    [n] => 6\n    [q] => 8\n    [s] => yy\n    [m] => Array\n        (\n            [0] => 1\n        )\n\n    [t] => tt\n)\nobject(P)#1 (6) {\n  [\"p\"]=>\n  array(2) {\n    [\"a\"]=>\n    int(1)\n    [\"b\"]=>\n    int(2)\n  }\n  [\"n\"]=>\n  int(6)\n  [\"q\"]=>\n  int(8)\n  [\"s\"]=>\n  string(2) \"yy\"\n  [\"m\"]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n  [\"t\"]=>\n  string(2) \"tt\"\n}\n{\"a\":1,\"b\":2,\"z\":9}{\"a\":1,\"b\":2,\"z\":9}\n"
    );
}
