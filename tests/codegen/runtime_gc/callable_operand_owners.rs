//! Purpose:
//! Verifies ownership of callable operands across throwing argument evaluation and cleanup:
//! statically resolved builtin callables, descriptor callbacks, immediately invoked closures,
//! statically lowered `array_map()` results, and by-value reference-call results.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Every fixture runs under heap debug, so a stranded operand shows up as a leak summary.
//! - Destructor output pins WHEN a payload is destroyed, which a leak summary alone cannot:
//!   an owner retired only at process exit would still read as clean in some shapes.
//! - The `try` sits INSIDE the function whose operand records are under test, so the unwind that
//!   reaches the catch is the operand-record chain alone rather than a whole activation teardown.
//! - Fixtures repeat and loop, because a per-call imbalance only shows up in a leak summary.

use crate::support::*;

/// An earlier owned builtin-callable argument is released when a later argument throws.
#[test]
fn test_core_static_builtin_callable_releases_earlier_arguments_when_a_later_one_throws() {
    let source = r#"<?php
function failingReplacement(): string { throw new RuntimeException('stop'); }
function replaceThroughCallable(string $left, string $right): string {
    return call_user_func('str_replace', implode('', [$left, $right]), failingReplacement(), 'a-b');
}
try { echo replaceThroughCallable('a', 'b'); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "caught");
}

/// A statically resolved builtin callable still releases its arguments on the successful path.
#[test]
fn test_core_static_builtin_callable_releases_arguments_after_a_successful_call() {
    let source = r#"<?php
function replaceThroughCallable(string $left, string $right): string {
    return call_user_func('str_replace', implode('', [$left, $right]), '-', 'xaby');
}
function replaceThroughFirstClassCallable(string $left, string $right): string {
    $callback = str_replace(...);
    return $callback(implode('', [$left, $right]), '-', 'xaby');
}
echo replaceThroughCallable('a', 'b'), ':', replaceThroughFirstClassCallable('a', 'b');
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "x-y:x-y", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "x-y:x-y");
}

/// Replacement callables release owned search, replacement and subject strings in every branch.
#[test]
fn test_core_string_replacement_callables_retire_all_owned_string_arguments() {
    let source = r#"<?php
function renderReplacements(string $left, string $right): void {
    echo str_replace(implode('', [$left, $right]), implode('', ['-', '!']), implode('', ['x', $left, $right, 'y'])), ':';
    echo call_user_func('str_ireplace', implode('', [$left, $right]), implode('', ['-', '!']), implode('', ['x', 'A', 'B', 'y'])), ':';
    $callback = str_ireplace(...);
    echo $callback(implode('', [$left, $right]), implode('', ['-', '!']), implode('', ['x', 'A', 'B', 'y'])), ':';
    echo call_user_func('str_replace', '', '-', implode('', [$left, $right])), ':';
    echo $callback(implode('', ['z', 'z']), '-', implode('', [$left, $right]));
}
renderReplacements('a', 'b');
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "x-!y:x-!y:x-!y:ab:ab", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "x-!y:x-!y:x-!y:ab:ab");
}

/// A freshly built descriptor callback is released when an argument expression throws.
///
/// The handler destructor must run while the exception propagates, not at process exit: the
/// callable array is the only owner of that object once `call_user_func()` has evaluated it.
#[test]
fn test_core_descriptor_callback_is_released_when_an_argument_throws() {
    let source = r#"<?php
class CallbackHandler {
    public function handle(int $value): int { return $value; }
    public function __destruct() { echo 'handler|'; }
}
function failingArgument(): int { throw new RuntimeException('stop'); }
function invokeFreshCallable(): int {
    return call_user_func([new CallbackHandler(), 'handle'], failingArgument());
}
try { echo invokeFreshCallable(); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "handler|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "handler|caught");
}

/// A descriptor callback with named and spread arguments is released on the successful path.
#[test]
fn test_core_descriptor_callback_owners_survive_named_and_spread_arguments() {
    let source = r#"<?php
class ArgumentHandler {
    public function handle(int $first, int $second): int { return $first * 10 + $second; }
    public function __destruct() { echo 'handler|'; }
}
function invokeWithNamedArgument(): int {
    return call_user_func([new ArgumentHandler(), 'handle'], second: 2, first: 1);
}
function invokeWithSpreadArguments(array $values): int {
    return call_user_func([new ArgumentHandler(), 'handle'], ...$values);
}
$named = invokeWithNamedArgument();
echo $named, '|';
$spread = invokeWithSpreadArguments([3, 4]);
echo $spread;
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "handler|12|handler|34", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "handler|12|handler|34");
}

/// An immediately invoked closure literal releases the descriptor its direct call does not use.
///
/// The captured payload is destroyed by `unset($items)` only if the closure descriptor released
/// its own retained copy of the capture first, so the output pins that retirement.
#[test]
fn test_core_immediately_invoked_closure_releases_its_descriptor() {
    let source = r#"<?php
class CapturedPayload {
    public function __destruct() { echo 'payload|'; }
}
function invokeClosureLiteral(): int {
    $items = [new CapturedPayload()];
    $count = (function () use ($items): int { return count($items); })();
    unset($items);
    return $count;
}
function invokeClosuresInLoop(int $rounds): int {
    $total = 0;
    for ($i = 0; $i < $rounds; $i++) {
        $items = [new CapturedPayload()];
        $total += (function () use ($items): int { return count($items); })();
        unset($items);
    }
    return $total;
}
$single = invokeClosureLiteral();
echo $single, '|';
$looped = invokeClosuresInLoop(3);
echo $looped;
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "payload|1|payload|payload|payload|3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "payload|1|payload|payload|payload|3");
}

/// A by-reference capture keeps the descriptor path, and a throwing argument still retires it.
#[test]
fn test_core_immediately_invoked_closure_owners_cover_by_ref_captures_and_throws() {
    let source = r#"<?php
class CapturedPayload {
    public function __destruct() { echo 'payload|'; }
}
function failingArgument(): int { throw new RuntimeException('stop'); }
function invokeByRefClosure(): int {
    $counter = 0;
    (function () use (&$counter): void { $counter = $counter + 1; })();
    return $counter;
}
function invokeClosureWithFailingArgument(): int {
    $items = [new CapturedPayload()];
    return (function (int $value) use ($items): int { return $value + count($items); })(
        failingArgument()
    );
}
echo invokeByRefClosure(), '|';
try { echo invokeClosureWithFailingArgument(); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|payload|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1|payload|caught");
}

/// The statically lowered `array_map()` result is released when a callback throws.
#[test]
fn test_core_static_array_map_releases_its_partial_result_when_a_callback_throws() {
    let source = r#"<?php
function upperItem(string $text): string { return strtoupper($text); }
function failingUpper(string $text): string {
    if ($text === 'b') { throw new RuntimeException('stop'); }
    return strtoupper($text);
}
function mapFailingItems(): array { return array_map('failingUpper', ['a', 'b']); }
echo implode(',', array_map('upperItem', ['a', 'b'])), '|';
try { echo implode(',', mapFailingItems()); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "A,B|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "A,B|caught");
}

/// A by-value reference-call result survives argument cleanup whose destructor throws.
///
/// The lease is the only owner of the callee's payload while the caller retires the argument
/// temporary, so the payload must be destroyed while the exception propagates, before the
/// catch runs, not at process exit.
#[test]
fn test_core_by_value_reference_lease_survives_throwing_argument_cleanup() {
    let source = r#"<?php
class ReferenceValuePayload {
    public function __destruct() { echo 'payload|'; }
}
class LeaseSource {
    public array $items = [];
    public function __construct() { $this->items = [new ReferenceValuePayload()]; }
}
class ThrowingArgument {
    public function __destruct() { echo 'argument|'; throw new RuntimeException('cleanup'); }
}
function &leaseFromLocalSource(ThrowingArgument $marker): array {
    $source = new LeaseSource();
    return $source->items;
}
function copyLease(): int {
    $copy = leaseFromLocalSource(new ThrowingArgument());
    return count($copy);
}
try { echo copyLease(); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\nGenerated user assembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(out.stdout, "argument|payload|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{}", out.stderr, asm);
    assert_eq!(compile_and_run_tagged(source), "argument|payload|caught");
}

/// A by-value reference-call result survives receiver cleanup whose destructor throws.
#[test]
fn test_core_by_value_reference_lease_survives_throwing_receiver_cleanup() {
    let source = r#"<?php
class ReferenceValuePayload {
    public function __destruct() { echo 'payload|'; }
}
class LeaseReceiver {
    public array $items = [];
    public function __construct() { $this->items = [new ReferenceValuePayload()]; }
    public function &reference(): array { return $this->items; }
    public function __destruct() { echo 'receiver|'; throw new RuntimeException('cleanup'); }
}
function copyMethodLease(): int {
    $copy = (new LeaseReceiver())->reference();
    return count($copy);
}
try { echo copyMethodLease(); }
catch (RuntimeException $error) { echo 'caught'; unset($error); }
"#;
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\nGenerated user assembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(out.stdout, "receiver|payload|caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{}", out.stderr, asm);
    assert_eq!(compile_and_run_tagged(source), "receiver|payload|caught");
}

/// An ordinary by-value use of a reference-returning call still copies the referenced payload.
#[test]
fn test_core_by_value_reference_lease_copies_its_payload() {
    let source = r#"<?php
class PlainLeaseSource { public array $items = [1]; }
function &plainLease(PlainLeaseSource $source): array { return $source->items; }
class PlainLeaseHolder {
    public array $items = [1];
    public function &reference(): array { return $this->items; }
}
function copyPlainLeases(): string {
    $source = new PlainLeaseSource();
    $copy = plainLease($source);
    array_push($source->items, 2);
    $holder = new PlainLeaseHolder();
    $methodCopy = $holder->reference();
    array_push($holder->items, 3);
    return implode(',', $copy) . ':' . implode(',', $source->items)
        . '|' . implode(',', $methodCopy) . ':' . implode(',', $holder->items);
}
echo copyPlainLeases();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1:1,2|1:1,3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "1:1,2|1:1,3");
}

/// A typed positional descriptor container releases its partial contents into a SAME-frame catch.
///
/// The `try` surrounds the call inside the very function whose operand records are under test, so
/// the destructor output pins that the published container is retired by the unwind BEFORE the
/// catch body runs, not by the caller's frame or by process exit. The fixture repeats to prove
/// the retirement is per call rather than a one-off.
#[test]
fn test_core_typed_positional_container_releases_partial_contents_before_a_same_frame_catch() {
    let source = r#"<?php
class Marker {
    public string $tag = '';
    public function __construct(string $tag) { $this->tag = $tag; }
    public function __destruct() { echo $this->tag, '|'; }
}
class Joiner {
    public function join(Marker $left, Marker $right): string { return $left->tag . $right->tag; }
}
function failingMarker(): Marker { throw new RuntimeException('stop'); }
function joinWithSameFrameCatch(): string {
    try {
        return call_user_func([new Joiner(), 'join'], new Marker('first'), failingMarker());
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
echo joinWithSameFrameCatch(), ':', joinWithSameFrameCatch();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first|caught|ok:first|caught|ok", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "first|caught|ok:first|caught|ok");
}

/// A spread followed by a named argument builds one container that a same-frame catch retires.
///
/// PHP requires unpacking to precede explicit named arguments, so this is the legal spelling of
/// the shape that used to decline AFTER the callback had been evaluated. The spread source is a
/// fresh array, so the container is the only owner of the spread element by the time the named
/// argument throws, and its destructor pins the retirement before the catch.
#[test]
fn test_core_named_and_spread_container_releases_its_spread_element_before_a_same_frame_catch() {
    let source = r#"<?php
class Marker {
    public string $tag = '';
    public function __construct(string $tag) { $this->tag = $tag; }
    public function __destruct() { echo $this->tag, '|'; }
}
class Joiner {
    public function join(Marker $left, Marker $right): string { return $left->tag . $right->tag; }
}
class Adder {
    public function add(int $left, int $right): int { return $left * 10 + $right; }
}
function leadingMarkers(): array { return [new Marker('left')]; }
function leadingInts(): array { return [1]; }
function failingMarker(): Marker { throw new RuntimeException('stop'); }
function joinNamedSpreadWithSameFrameCatch(): string {
    try {
        return call_user_func([new Joiner(), 'join'], ...leadingMarkers(), right: failingMarker());
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
function addNamedSpread(): int {
    return call_user_func([new Adder(), 'add'], ...leadingInts(), right: 2);
}
echo joinNamedSpreadWithSameFrameCatch(), ':', joinNamedSpreadWithSameFrameCatch();
echo ':', addNamedSpread(), ':', addNamedSpread();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "left|caught|ok:left|caught|ok:12:12",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "left|caught|ok:left|caught|ok:12:12",
    );
}

/// A `call_user_func_array()` reference-marker container releases its partial contents in frame.
///
/// A published CUFA container protects earlier items even when its inferred callback signature
/// has no by-reference parameters. Rooting cannot depend on whether reference markers are needed.
#[test]
fn test_core_call_user_func_array_container_releases_partial_contents_before_a_same_frame_catch() {
    let source = r#"<?php
class Marker {
    public string $tag = '';
    public function __construct(string $tag) { $this->tag = $tag; }
    public function __destruct() { echo $this->tag, '|'; }
}
function failingMarker(): Marker { throw new RuntimeException('stop'); }
function applyWithSameFrameCatch(callable $callback): string {
    $count = 0;
    try {
        call_user_func_array($callback, [$count, new Marker('first'), failingMarker()]);
        return 'unreached';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok' . $count;
    }
}
$record = function ($slot, Marker $first, Marker $second): int { return 1; };
echo applyWithSameFrameCatch($record), ':', applyWithSameFrameCatch($record);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "first|caught|ok0:first|caught|ok0", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "first|caught|ok0:first|caught|ok0");
}

/// A descriptor call's owned result survives a throwing destructor run by callback retirement.
///
/// The closure's captured array is the LAST owner of the capture payload by the time the call
/// runs, because the argument expression clears the outer holder through a reference. Retiring
/// the callback after the call therefore destroys that payload, and its destructor throws. The
/// result object's own destructor pins that the result was still rooted when it did: without the
/// result staging it is stranded and the heap summary is dirty.
#[test]
fn test_core_descriptor_result_survives_a_throwing_capture_destructor_before_a_same_frame_catch() {
    let source = r#"<?php
class ThrowingCapture {
    public function __destruct() { echo 'capture|'; throw new RuntimeException('capture'); }
}
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
function dropHolder(array &$slot): int { $slot = []; return 1; }
function invokeWithLastOwnedCapture(): string {
    $held = [new ThrowingCapture()];
    try {
        call_user_func(
            function (int $ignored) use ($held): ResultPayload { return new ResultPayload(); },
            dropHolder($held)
        );
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
echo invokeWithLastOwnedCapture(), ':', invokeWithLastOwnedCapture();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "capture|result|caught|ok:capture|result|caught|ok",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "capture|result|caught|ok:capture|result|caught|ok",
    );
}

/// An immediately invoked closure's result survives the throwing destructor of its own capture.
///
/// The direct call does not pass the descriptor, so the descriptor is retired separately after
/// the call. It is the last owner of the captured array here, so that retirement runs a throwing
/// destructor while the call's result is only an SSA temporary unless it is staged first.
#[test]
fn test_core_immediately_invoked_closure_result_survives_a_throwing_capture_destructor() {
    let source = r#"<?php
class ThrowingCapture {
    public function __destruct() { echo 'capture|'; throw new RuntimeException('capture'); }
}
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
function dropHolder(array &$slot): int { $slot = []; return 1; }
function invokeIifeWithLastOwnedCapture(): string {
    $held = [new ThrowingCapture()];
    try {
        (function (int $ignored) use ($held): ResultPayload { return new ResultPayload(); })(
            dropHolder($held)
        );
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
echo invokeIifeWithLastOwnedCapture(), ':', invokeIifeWithLastOwnedCapture();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "capture|result|caught|ok:capture|result|caught|ok",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "capture|result|caught|ok:capture|result|caught|ok",
    );
}

/// A direct user call's fresh owned result survives a throwing argument destructor, in frame.
///
/// The argument temporary is rooted across the call and released after it, and that release runs
/// a destructor that throws. The result object was produced before it, so its own destructor
/// output pins that the result is retired exactly once by the unwind rather than stranded.
#[test]
fn test_core_direct_user_call_result_survives_throwing_argument_cleanup_before_a_same_frame_catch() {
    let source = r#"<?php
class ResultPayload {
    public function __destruct() { echo 'result|'; }
}
class ThrowingArgument {
    public function __destruct() { echo 'argument|'; throw new RuntimeException('cleanup'); }
}
class ResultFactory {
    public function build(ThrowingArgument $marker): ResultPayload { return new ResultPayload(); }
}
function buildResult(ThrowingArgument $marker): ResultPayload { return new ResultPayload(); }
function buildWithSameFrameCatch(): string {
    try {
        buildResult(new ThrowingArgument());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'fn';
    }
}
function buildThroughMethodWithSameFrameCatch(): string {
    $factory = new ResultFactory();
    try {
        $factory->build(new ThrowingArgument());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'method';
    }
}
echo buildWithSameFrameCatch(), ':', buildThroughMethodWithSameFrameCatch();
"#;
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\nassembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(
        out.stdout,
        "argument|result|caught|fn:argument|result|caught|method",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(
        compile_and_run_tagged(source),
        "argument|result|caught|fn:argument|result|caught|method",
    );
}

/// `Closure::call()` releases the descriptor it binds, on the successful and the throwing path.
///
/// The rebound descriptor is a fresh owner of the closure environment and of the new receiver,
/// and nothing but this call site ever owns it. The loop turns a per-call leak into a heap
/// summary a structural assertion cannot produce, and the throwing argument covers the unwind.
#[test]
fn test_core_closure_call_releases_its_bound_descriptor_on_success_and_on_a_throw() {
    let source = r#"<?php
class Holder {
    public int $value = 7;
    public function __destruct() { echo 'holder|'; }
}
function failingInt(): int { throw new RuntimeException('stop'); }
function callBound($extra) {
    $closure = function ($bonus) { return $this->value + $bonus; };
    return $closure->call(new Holder(), $extra);
}
function callBoundInLoop(int $rounds): int {
    $total = 0;
    for ($i = 0; $i < $rounds; $i++) {
        $total += callBound(1);
    }
    return $total;
}
function callBoundWithSameFrameCatch(): string {
    $closure = function ($bonus) { return $this->value + $bonus; };
    try {
        $closure->call(new Holder(), failingInt());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
echo callBoundInLoop(32), ':', callBoundWithSameFrameCatch(), ':', callBoundWithSameFrameCatch();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = format!("{}256:holder|caught|ok:holder|caught|ok", "holder|".repeat(32));
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Static extern, first-class-callable and pass-through callable calls leak nothing in a loop.
///
/// A structural comparison of the lowered releases proves the release EXISTS; only running the
/// program many times with heap debug proves it BALANCES. Each round builds a fresh string with
/// `implode()` and hands it to a callable form whose callee consumes it differently: an extern
/// call that keeps nothing, a builtin that allocates a new string, and a user function whose
/// result is its own argument, which is the alias case the release must not free twice.
#[test]
fn test_core_static_callable_forms_balance_fresh_string_arguments_across_repeated_calls() {
    let source = r#"<?php
extern function atoi(string $value): int;
function passthrough(string $value): string { return $value; }
function parseThroughExternCallable(string $left, string $right): int {
    return call_user_func('atoi', implode('', [$left, $right]));
}
function parseThroughExternFirstClassCallable(string $left, string $right): int {
    $callback = atoi(...);
    return $callback(implode('', [$left, $right]));
}
function upperThroughFirstClassCallable(string $left, string $right): string {
    $callback = strtoupper(...);
    return $callback(implode('', [$left, $right]));
}
function aliasThroughCallable(string $left, string $right): string {
    return call_user_func('passthrough', implode('', [$left, $right]));
}
function runRounds(int $rounds): string {
    $total = 0;
    $last = '';
    for ($i = 0; $i < $rounds; $i++) {
        $total += parseThroughExternCallable('4', '2');
        $total += parseThroughExternFirstClassCallable('4', '2');
        $last = upperThroughFirstClassCallable('a', 'b') . aliasThroughCallable('c', 'd');
    }
    return $total . ':' . $last;
}
echo runRounds(64);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "5376:ABcd", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "5376:ABcd");
}

/// The statically lowered `array_map()` retires its partial result into a SAME-frame catch, and
/// never reorders an element whose evaluation is observable.
///
/// PHP evaluates the whole source array before calling the callback once per element. The fast
/// path interleaves the two, so it may only accept elements whose evaluation cannot be seen. The
/// second fixture makes both the element expression and the callback print, which is exactly the
/// interleaving a wrongly applied fast path would expose.
#[test]
fn test_core_static_array_map_retires_its_partial_result_and_preserves_evaluation_order() {
    let source = r#"<?php
function failingUpper(string $text): string {
    if ($text === 'b') { throw new RuntimeException('stop'); }
    return strtoupper($text);
}
function shoutItem(string $text): string { echo strtoupper($text), '|'; return strtoupper($text); }
function noteItem(string $tag): string { echo $tag, '|'; return $tag; }
function mapWithSameFrameCatch(): string {
    try {
        return implode(',', array_map('failingUpper', ['a', 'b']));
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'ok';
    }
}
function mapObservedItems(): string {
    return implode(',', array_map('shoutItem', [noteItem('x'), noteItem('y')]));
}
echo mapWithSameFrameCatch(), ':', mapWithSameFrameCatch(), ':', mapObservedItems();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|ok:caught|ok:x|y|X|Y|X,Y", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "caught|ok:caught|ok:x|y|X|Y|X,Y");
}

/// A by-value reference-return copy survives throwing argument and receiver cleanup, in frame.
///
/// The existing fixtures catch in the CALLER of the function whose records are under test, which
/// unwinds a whole activation before the catch. These catch inside that same function, so the
/// payload destructor output pins the retirement against the operand-record chain alone.
#[test]
fn test_core_by_value_reference_lease_survives_cleanup_before_a_same_frame_catch() {
    let source = r#"<?php
class ReferenceValuePayload {
    public function __destruct() { echo 'payload|'; }
}
class LeaseSource {
    public array $items = [];
    public function __construct() { $this->items = [new ReferenceValuePayload()]; }
}
class LeaseReceiver {
    public array $items = [];
    public function __construct() { $this->items = [new ReferenceValuePayload()]; }
    public function &reference(): array { return $this->items; }
    public function __destruct() { echo 'receiver|'; throw new RuntimeException('cleanup'); }
}
class ThrowingArgument {
    public function __destruct() { echo 'argument|'; throw new RuntimeException('cleanup'); }
}
function &leaseFromLocalSource(ThrowingArgument $marker): array {
    $source = new LeaseSource();
    return $source->items;
}
function copyLeaseWithSameFrameCatch(): string {
    try {
        $copy = leaseFromLocalSource(new ThrowingArgument());
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'arg';
    }
}
function copyMethodLeaseWithSameFrameCatch(): string {
    try {
        $copy = (new LeaseReceiver())->reference();
        echo 'unreached|';
        return 'no';
    } catch (RuntimeException $error) {
        echo 'caught|';
        return 'receiver';
    }
}
echo copyLeaseWithSameFrameCatch(), ':', copyMethodLeaseWithSameFrameCatch();
"#;
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\nGenerated user assembly:\n{}", out.stdout, out.stderr, asm);
    assert_eq!(
        out.stdout,
        "argument|payload|caught|arg:receiver|payload|caught|receiver",
        "{}", out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{}", out.stderr, asm);
    assert_eq!(
        compile_and_run_tagged(source),
        "argument|payload|caught|arg:receiver|payload|caught|receiver",
    );
}

/// A by-value use of a reference-returning call DETACHES a boxed pointee instead of aliasing it.
///
/// The lease keeps the callee's own mutable box, and a later write through the same reference
/// mutates that box in place. A by-value use must not observe that write, so the copy clones the
/// box the way an ordinary by-value `return` of a reference-cell read does.
///
/// The property is declared `array`, not `mixed`: a declared PHP `array` is a `Union` whose
/// codegen representation is already the boxed `Mixed` cell this exercises, while `mixed` is
/// rejected by the array-push property checker before the lowering under test ever runs. The
/// in-place write goes through a reference alias for the same reason; a direct `$o->prop[] = …`
/// is refused for a declared-`array` property too.
#[test]
fn test_core_by_value_reference_lease_detaches_a_boxed_pointee() {
    let source = r#"<?php
class MixedBox {
    public array $value = [1, 'two'];
    public function &current(): array { return $this->value; }
}
function copyMixedLease(): string {
    $source = new MixedBox();
    $copy = $source->current();
    $alias = &$source->value;
    $alias[] = 3;
    return count($copy) . ':' . count($source->value);
}
echo copyMixedLease();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:3", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), "2:3");
}
