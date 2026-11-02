//! Purpose:
//! End-to-end tests for the payload contract of a by-reference return whose source is an object
//! property: a runtime-dispatched receiver whose class stores the property with an incompatible
//! representation raises a catchable `Error` instead of publishing a cell the caller would read
//! through the wrong shape, while every compatible class keeps working through the same function.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The guard is per candidate class, not per function: one `mixed` parameter reaches both a
//!   compatible and an incompatible holder in these fixtures, and only the second one raises.
//! - The raised `Error` is a compiler-subset condition. Reference PHP has untyped references and
//!   no equivalent failure, so these fixtures make no PHP-equivalence claim.
//! - A refused return publishes no pointer at all, so a same-function `catch` observes the
//!   borrowed aliases and the properties exactly as they were.
//! - The `Closure::bind` by-reference specialization is exercised through the immediate-invoke
//!   form, a stored binding, a plain value call and the runtime descriptor routes
//!   (`call_user_func`, a callable argument), which must all agree on the bound receiver.
//! - The bound descriptor is the sole owner of its boxed receiver, so the receiver's destructor
//!   position in the output is the ownership assertion; the heap-debug fixtures additionally
//!   require a clean leak summary.

use crate::support::*;

/// A `mixed` by-reference return serves a compatible holder and refuses an incompatible one.
///
/// `GoodPayloadHolder::$value` is declared `mixed`, so its cell holds the boxed payload a `mixed`
/// result dereferences. `BadPayloadHolder::$value` is a raw `int`, and handing that cell to the
/// same caller would make it read the integer as a boxed pointer. Both are reference properties
/// (each is aliased by a local first), so both reach the runtime class dispatch and the decision
/// is genuinely made per candidate.
#[test]
fn test_dynamic_property_reference_guards_each_candidate_class() {
    let out = compile_and_run(
        r#"<?php
class GoodPayloadHolder { public mixed $value = 7; }
class BadPayloadHolder { public int $value = 9; }
function &dynamicPropertyReference(mixed $holder): mixed { return $holder->value; }
function readDynamicReferences(): void {
    $good = new GoodPayloadHolder();
    $bad = new BadPayloadHolder();
    $goodAlias = &$good->value;
    $badAlias = &$bad->value;
    $fromGood = &dynamicPropertyReference($good);
    echo $fromGood, '|';
    try {
        $fromBad = &dynamicPropertyReference($bad);
        echo 'unguarded|', $fromBad;
    } catch (Error $error) {
        echo 'caught|', $bad->value, '|', $badAlias;
    }
}
readDynamicReferences();
"#,
    );
    assert_eq!(out, "7|caught|9|9");
}

/// The refused candidate leaves the aliases it did not publish completely untouched.
///
/// The compatible holder is still writable through the reference after the incompatible one has
/// raised, which is what proves the guard rejected one candidate rather than poisoning the
/// shared lowering.
#[test]
fn test_dynamic_property_reference_write_through_survives_a_refused_candidate() {
    let out = compile_and_run(
        r#"<?php
class WritableHolder { public mixed $value = 1; }
class RawIntHolder { public int $value = 2; }
function &dynamicSlot(mixed $holder): mixed { return $holder->value; }
function writeThroughDynamicSlots(): void {
    $writable = new WritableHolder();
    $raw = new RawIntHolder();
    $writableAlias = &$writable->value;
    $rawAlias = &$raw->value;
    try {
        $refused = &dynamicSlot($raw);
        echo 'unguarded|';
    } catch (Error $error) {
        echo 'caught|';
    }
    $accepted = &dynamicSlot($writable);
    $accepted = 42;
    echo $writable->value, '|', $writableAlias, '|', $rawAlias;
}
writeThroughDynamicSlots();
"#,
    );
    assert_eq!(out, "caught|42|42|2");
}

/// A by-reference `Closure::bind` over a `string` property works immediately, when stored, and
/// when the stored binding is called for its VALUE rather than for its reference.
///
/// The three routes must agree on the bound receiver and on the transported payload: the direct
/// call carries the property's cell pointer, and the plain value call copies the pointee. Before
/// the bound closure's body was typed from the bound property, the immediate and stored forms
/// published a `Mixed` payload claim for a `string` slot.
#[test]
fn test_bound_by_reference_string_closure_agrees_across_its_call_forms() {
    let out = compile_and_run(
        r#"<?php
class BoundStringHolder { public string $text = 'init'; }
$holder = new BoundStringHolder();
$immediate = &\Closure::bind(fn &() => $this->text, $holder, $holder)();
$immediate = 'first';
echo $holder->text, '|';
$bound = \Closure::bind(fn &() => $this->text, $holder, $holder);
$stored = &$bound();
$stored = 'second';
echo $holder->text, '|';
echo $bound(), '|';
$holder->text = 'viaprop';
echo $immediate, '|', $stored;
"#,
    );
    assert_eq!(out, "first|second|second|viaprop|viaprop");
}

/// The same bound by-reference closure over an `array` property, written through both aliases.
///
/// The array form is the Symfony-shaped case the specialization was built for; keeping it next to
/// the string form pins that the contextual result type did not narrow the accepted shapes.
#[test]
fn test_bound_by_reference_array_closure_writes_through_both_forms() {
    let out = compile_and_run(
        r#"<?php
class BoundArrayHolder { public array $items = []; }
$holder = new BoundArrayHolder();
$immediate = &\Closure::bind(fn &() => $this->items, $holder, $holder)();
$immediate[] = 'a';
$bound = \Closure::bind(fn &() => $this->items, $holder, $holder);
$stored = &$bound();
$stored[] = 'b';
echo implode(',', $holder->items), '|';
$stored = [];
echo count($holder->items), '|', count($immediate);
"#,
    );
    assert_eq!(out, "a,b|0|0");
}

/// A base-typed by-reference result accepts a property that holds a SUBCLASS instance.
///
/// Object storage is one pointer whatever the class is, so the payload guard must not turn an
/// ordinary covariant property into a refusal. Writing a different subclass through the alias is
/// observed by the property, proving the transferred cell really is the shared one.
#[test]
fn test_by_reference_return_of_a_subclass_object_property() {
    let out = compile_and_run(
        r#"<?php
class PayloadNode { public int $id = 4; }
class PayloadLeaf extends PayloadNode { }
class NodeHolder {
    public PayloadLeaf $node;
    public function __construct() { $this->node = new PayloadLeaf(); }
}
function &nodeSlot(NodeHolder $holder): PayloadNode { return $holder->node; }
$holder = new NodeHolder();
$alias = &nodeSlot($holder);
echo $alias->id, '|', $holder->node->id;
"#,
    );
    assert_eq!(out, "4|4");
}

/// An OWNED temporary receiver is retired exactly once when the payload guard throws.
///
/// `makeOwnedBad()->value` evaluates a receiver the returning frame owns, so the guard runs with a
/// fresh object live only in that frame. Rooting it in the operand-owner chain is what lets the
/// unwind release it: left only in SSA it would be stranded, and released on both the normal and
/// the unwinding path it would be freed twice. The heap-debug leak summary separates the two
/// outcomes, and the caller's own borrowed holder has to survive either way.
#[test]
fn test_refused_dynamic_reference_retires_its_owned_receiver_once() {
    let source = r#"<?php
class OwnedBadHolder { public int $value = 4; }
function makeOwnedBad(): mixed { return new OwnedBadHolder(); }
function &ownedReceiverSlot(): mixed { return makeOwnedBad()->value; }
function consumeOwnedReceiverSlot(): void {
    $probe = new OwnedBadHolder();
    $probeAlias = &$probe->value;
    try {
        $refused = &ownedReceiverSlot();
        echo 'unguarded|';
    } catch (Error $error) {
        echo 'caught|';
    }
    echo $probe->value, '|', $probeAlias;
}
consumeOwnedReceiverSlot();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|4|4", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A STORED bound by-reference closure reaches the same receiver through every call form.
///
/// The direct-call path writes through the property's cell, and the descriptor routes
/// (`call_user_func`, a callable argument, a first-class callable) dereference the same cell
/// through the runtime invoker. Storing the unbound descriptor made those routes dispatch
/// against a null receiver, so the point of the fixture is that all four answers agree.
#[test]
fn test_stored_bound_closure_agrees_between_direct_and_descriptor_calls() {
    let out = compile_and_run(
        r#"<?php
class DescriptorHolder { public string $text = 'init'; }
function callThrough(callable $callback): string { return $callback(); }
$holder = new DescriptorHolder();
$bound = \Closure::bind(fn &() => $this->text, $holder, $holder);
$alias = &$bound();
$alias = 'written';
echo $bound(), '|', call_user_func($bound), '|', callThrough($bound), '|';
$holder->text = 'again';
echo $bound(), '|', call_user_func($bound);
"#,
    );
    assert_eq!(out, "written|written|written|again|again");
}

/// The bound descriptor is the SOLE owner of the receiver box it hands the direct call.
///
/// The receiver is an owned temporary, so nothing but the bound closure keeps it alive: it has to
/// survive the bind's own input cleanup (its destructor must not run while the result is still
/// being produced) and it has to be released when the descriptor retires, not held by a hidden
/// second box until the frame exits. The destructor's position in the output pins both halves,
/// and the heap-debug summary pins that no box was left behind.
#[test]
fn test_bound_closure_owns_its_receiver_until_the_descriptor_retires() {
    let source = r#"<?php
class NoisyReceiver {
    public string $text = 'kept';
    public function __destruct() { echo 'dtor|'; }
}
function makeNoisy(): NoisyReceiver { return new NoisyReceiver(); }
function boundOwnership(): void {
    $bound = \Closure::bind(fn &() => $this->text, makeNoisy(), null);
    echo 'bound|', $bound(), '|';
    unset($bound);
    echo 'after|';
}
boundOwnership();
echo 'end';
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bound|kept|dtor|after|end", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Repeated immediate binds leak no receiver box, and the shared receiver outlives all of them.
///
/// Each `Closure::bind(...)()` builds a bound descriptor, borrows its boxed receiver for the
/// direct call and retires it again. A box the descriptor did not own would accumulate one leaked
/// `Mixed` cell (and one leaked receiver reference) per bind, which the leak summary would report
/// and the receiver's destructor position would betray.
#[test]
fn test_repeated_immediate_binds_leave_no_receiver_box_behind() {
    let source = r#"<?php
class CountedReceiver {
    public string $text = 'v';
    public function __destruct() { echo 'gone|'; }
}
function repeatedBinds(): void {
    $holder = new CountedReceiver();
    echo \Closure::bind(fn &() => $this->text, $holder, $holder)(), '|';
    echo \Closure::bind(fn &() => $this->text, $holder, $holder)(), '|';
    $stored = \Closure::bind(fn &() => $this->text, $holder, $holder);
    unset($stored);
    echo 'unset|';
    unset($holder);
    echo 'released|';
}
repeatedBinds();
echo 'end';
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout, "v|v|unset|gone|released|end",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A receiver expression that THROWS retires the already-lowered closure literal.
///
/// The closure literal is evaluated first, in source order, and is a live descriptor while
/// `$newThis` is evaluated. A same-frame catch therefore has to find it through the operand-owner
/// chain, or that descriptor is stranded. The bind that follows the catch proves the failed one
/// left no pending owner record behind.
#[test]
fn test_throwing_receiver_expression_retires_the_lowered_closure_literal() {
    let source = r#"<?php
class LateHolder { public string $text = 'ready'; }
function refuseReceiver(): LateHolder { throw new Exception('no receiver'); }
function bindWithThrowingReceiver(): void {
    try {
        $bound = \Closure::bind(fn &() => $this->text, refuseReceiver(), null);
        echo 'unguarded|';
    } catch (Exception $error) {
        echo 'caught|', $error->getMessage(), '|';
    }
    $holder = new LateHolder();
    $good = \Closure::bind(fn &() => $this->text, $holder, $holder);
    echo $good();
}
bindWithThrowingReceiver();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|no receiver|ready", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A bind written inside a method, to an instance of ANOTHER class, reads the bound class's slot.
///
/// Both classes declare a compatible `int` property, so the only thing that can go wrong is the
/// representation: capturing the enclosing `$this` at its own class would compile `$this->count`
/// against the ENCLOSING slot and write the bound receiver through the wrong offset. The
/// enclosing object's own property must be untouched.
#[test]
fn test_in_method_bind_to_another_class_writes_the_bound_receivers_slot() {
    let out = compile_and_run(
        r#"<?php
class OtherCounter { public int $count = 3; }
class CounterBinder {
    public int $count = 99;
    public function bumpOther(OtherCounter $other): string {
        $bound = \Closure::bind(fn &() => $this->count, $other, OtherCounter::class);
        $alias = &$bound();
        $alias = 11;
        return $other->count . '|' . $this->count . '|' . $bound();
    }
}
$binder = new CounterBinder();
echo $binder->bumpOther(new OtherCounter());
"#,
    );
    assert_eq!(out, "11|99|11");
}

/// Bound-closure specialization evaluates the scope expression after the receiver, exactly once.
#[test]
fn test_bound_closure_evaluates_scope_in_source_order_and_unwinds_its_receiver() {
    let source = r#"<?php
class ScopedReceiver {
    public string $text = 'ok';
    public function __destruct() { echo 'drop|'; }
}
function scopedReceiver(): ScopedReceiver { echo 'receiver|'; return new ScopedReceiver(); }
function selectedScope(bool $fail): string {
    echo 'scope|';
    if ($fail) { throw new RuntimeException('scope'); }
    return ScopedReceiver::class;
}
function exerciseScope(): void {
    try {
        $bad = \Closure::bind(fn &() => $this->text, scopedReceiver(), selectedScope(true));
        echo 'unreached|';
    } catch (RuntimeException $error) { echo 'caught|'; unset($error); }
    $good = \Closure::bind(fn &() => $this->text, scopedReceiver(), selectedScope(false));
    echo $good(), '|';
    unset($good);
    echo 'done';
}
exerciseScope();
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "receiver|scope|drop|caught|receiver|scope|ok|drop|done");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$this->p = &$param` on a by-reference parameter binds the property to the CALLER's variable,
/// and a property passed to a by-reference parameter is passed as its own cell.
///
/// The parser spells the binding as a copy into the property plus `$param = &$this->p`, which
/// re-pointed the parameter at a fresh cell: the caller's variable was cut off (it even read back
/// `null`), and a property handed on to another constructor (`new Keeper($this->p)`) arrived as a
/// copy. Symfony's `ContainerConfigurator` -> `ServicesConfigurator` -> `ServiceConfigurator`
/// chain shares the loader's `$instanceof` exactly this way, and every container-building console
/// command died on "Cannot assign null to property ServiceConfigurator::$instanceof". Expected
/// output is `php -n` 8.5's.
#[test]
fn test_property_bound_to_by_ref_parameter_shares_the_callers_variable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function fill(array &$p) { $p = ['f' => 1]; }
class Keeper {
    private array $p;
    public function __construct(array &$p) { $this->p = &$p; $p['seen'] = true; }
    public function get(): array { return $this->p; }
}
class Holder {
    private array $p;
    public array $own = ['x' => 1];
    public function __construct(array &$p) { $this->p = &$p; }
    public function touch(): array { $r = &$this->p; $r['t'] = 1; return $this->p; }
    public function keeper(): Keeper { return new Keeper($this->p); }
    public function fillOwn(): void { fill($this->own); }
}
function forward(array &$z): Keeper { return new Keeper($z); }
$b = ['b' => 1];
$h = new Holder($b);
echo json_encode($h->touch()), json_encode($b), "\n";
$k = $h->keeper();
$b['late'] = 2;
echo json_encode($k->get()), json_encode($b), "\n";
$h->fillOwn();
echo json_encode($h->own), "\n";
$w = ['w' => 1];
$k2 = forward($w);
$w['after'] = 3;
echo json_encode($k2->get()), json_encode($w), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "{\"b\":1,\"t\":1}{\"b\":1,\"t\":1}\n\
         {\"b\":1,\"t\":1,\"seen\":true,\"late\":2}{\"b\":1,\"t\":1,\"seen\":true,\"late\":2}\n\
         {\"f\":1}\n\
         {\"w\":1,\"seen\":true,\"after\":3}{\"w\":1,\"seen\":true,\"after\":3}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference parameter bound to a property keeps the property's declared representation,
/// including when the argument is OMITTED and its default seeds the cell.
///
/// Recording such a parameter as escaping must not widen it to `mixed` the way a closure capture
/// is widened: the `int` property then read the box pointer as an integer, and a caller's `int`
/// local was boxed into a temporary the property could not keep (Symfony's
/// `ContainerConfigurator::__construct(..., int &$anonymousCount = 0)`). Expected output is
/// `php -n` 8.5's.
#[test]
fn test_property_bound_by_ref_parameter_with_omitted_default_keeps_its_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Counted {
    private array $a; private int $n;
    public function __construct(array &$a, int &$n = 0) { $this->a = &$a; $this->n = &$n; }
    function bump() { $this->n++; $this->a[] = $this->n; return $this->a; }
}
$x = [];
$c = new Counted($x);
echo json_encode($c->bump()), json_encode($x), "\n";
$m = 5;
$d = new Counted($x, $m);
$d->bump();
echo $m, json_encode($x), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[1][1]\n6[1,6]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Only a call that reaches a property-bound by-reference parameter promotes the caller's local.
///
/// Keyed by the parameter's NAME instead, `array_splice($tail, ...)` matched a user method's
/// `array &$array` that binds a property, the local became a reference cell that could no longer
/// be re-typed, and `$tail = '/' . implode('/', $tail); strlen($tail)` stopped compiling
/// (Symfony's `DebugClassLoader::checkCase`). Expected output is `php -n` 8.5's.
#[test]
fn test_builtin_by_ref_argument_named_like_a_property_bound_parameter_stays_a_plain_local() {
    let out = compile_and_run(
        r#"<?php
class Binder { private array $x = []; function bind(array &$array) { $this->x = &$array; } }
function tailOf(string $p) {
    $tail = explode('/', $p);
    array_splice($tail, 0, 1);
    if (!$tail) { return null; }
    $tail = '/' . implode('/', $tail);
    return strlen($tail);
}
echo tailOf('a/b/c'), "\n";
"#,
    );
    assert_eq!(out, "4\n");
}
