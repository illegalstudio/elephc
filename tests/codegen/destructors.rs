//! Purpose:
//! End-to-end tests for PHP `__destruct`: the compiler invokes a class's
//! destructor when an object's refcount reaches zero, before its storage is
//! released, across scope exit, overwrite, `unset`, program end, inheritance,
//! container release, and the self-reference re-entrancy guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Destructors run inside the object-free path, so these fixtures assert the
//!   exact interleaving of destructor output with surrounding `echo`s. Ordering
//!   among siblings released together reflects the codegen cleanup order and is
//!   asserted as produced.

use crate::support::*;

/// Objects that only a reference cycle keeps alive are destructed at program end, after
/// the acyclic ones, as PHP's shutdown destructs every object still alive.
#[test]
fn test_destruct_cycle_at_program_end() {
    let out = compile_and_run(
        r#"<?php
final class Node {
    public ?Node $next = null;
    public function __construct(private string $name) {}
    public function __destruct() { echo "destruct ", $this->name, "\n"; }
}
$a = new Node('a');
$b = new Node('b');
$a->next = $b;
$b->next = $a;
$c = new Node('c');
echo "end of script\n";
"#,
    );
    assert_eq!(out, "end of script\ndestruct c\ndestruct a\ndestruct b\n");
}

/// A destructor fires at function scope exit, before control returns to the
/// caller's following statement.
#[test]
fn test_destruct_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() {
        echo "destroyed\n";
    }
}
function run() {
    $x = new Logger();
    echo "inside\n";
}
run();
echo "after\n";
"#,
    );
    assert_eq!(out, "inside\ndestroyed\nafter\n");
}

/// A top-level object's destructor runs at program end, after main's body.
#[test]
fn test_destruct_at_program_end() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() { echo "bye\n"; }
}
$x = new Logger();
echo "main\n";
"#,
    );
    assert_eq!(out, "main\nbye\n");
}

/// The object a narrowed receiver's call returns is destructed like any other: a method the
/// receiver's static interface does not declare dispatches through a boxed `Mixed` result,
/// and that box owns the returned object, whether the result is chained, bound or discarded.
#[test]
fn test_destruct_result_of_narrowed_receiver_call() {
    let out = compile_and_run(
        r#"<?php
interface Response {}
final class Doc implements Response {
    public function __construct(private string $name) {}
    public function __destruct() { echo "destruct ", $this->name, "\n"; }
    public function with(string $step): self { return new self($this->name . '+' . $step); }
    public function name(): string { return $this->name; }
}
function chained(Response $response): string {
    if ($response instanceof Doc) { return $response->with('chained')->name(); }
    return '';
}
function bound(Response $response): string {
    if ($response instanceof Doc) { $next = $response->with('bound'); return $next->name(); }
    return '';
}
function discarded(Response $response): void {
    if ($response instanceof Doc) { $response->with('discarded'); }
    echo "after discard\n";
}
echo chained(new Doc('a')), "\n";
echo bound(new Doc('b')), "\n";
discarded(new Doc('c'));
echo "end\n";
"#,
    );
    // `bound` releases its locals as codegen orders them; PHP destroys `$response` before `$next`.
    assert_eq!(
        out,
        "destruct a+chained\ndestruct a\na+chained\ndestruct b+bound\ndestruct b\nb+bound\n\
         destruct c+discarded\nafter discard\ndestruct c\nend\n"
    );
}

/// `unset($x)` releasing the last reference runs the destructor immediately.
#[test]
fn test_destruct_on_unset() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() { echo "gone\n"; }
}
$x = new Logger();
echo "a\n";
unset($x);
echo "b\n";
"#,
    );
    assert_eq!(out, "a\ngone\nb\n");
}

/// Overwriting a variable releases the previous object (running its destructor),
/// and the destructor can read `$this`'s properties.
#[test]
fn test_destruct_on_overwrite_reads_this() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    private string $tag;
    public function __construct(string $t) { $this->tag = $t; }
    public function __destruct() { echo "drop:" . $this->tag . "\n"; }
}
$x = new Logger("first");
$x = new Logger("second");
echo "end\n";
"#,
    );
    assert_eq!(out, "drop:first\nend\ndrop:second\n");
}

/// The same overwrite still releases the previous object when a LATER store widens the
/// variable's frame slot to boxed Mixed.
///
/// `test_destruct_on_overwrite_reads_this` above passes without the trailing `$x = null`
/// because the slot stays a concrete object for the whole function. Adding that store
/// makes codegen lay the slot out as Mixed, so the overwrite's occupant load — lowered
/// earlier, when the slot still looked concrete — becomes an unbox PLUS a retain, and its
/// paired release only gave that retain back: the box owning `first` was overwritten
/// without ever being released, so `drop:first` never printed (php prints it at the second
/// store). `retype_stale_local_load_release_ops` types that release by the slot's FINAL
/// storage instead.
#[test]
fn test_destruct_on_overwrite_when_a_later_store_boxes_the_slot() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    private string $tag;
    public function __construct(string $t) { $this->tag = $t; }
    public function __destruct() { echo "drop:" . $this->tag . "\n"; }
}
$x = new Logger("first");
$x = new Logger("second");
$x = null;
echo "end\n";
"#,
    );
    assert_eq!(out, "drop:first\ndrop:second\nend\n");
}

/// A fluent chain of overwrites inside a function loses EVERY superseded object when a
/// conditional null store boxes the slot — the shape Symfony's `ServiceConfigurator` uses,
/// where each `->set()` supersedes the previous configurator and the destructor is what
/// registers the service definition.
#[test]
fn test_destruct_on_repeated_overwrite_with_a_conditional_null_store() {
    let out = compile_and_run(
        r#"<?php
class Cfg {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo "register {$this->id}\n"; }
}
function build(bool $reset): void {
    $c = new Cfg('a');
    $c = new Cfg('b');
    $c = new Cfg('c');
    if ($reset) { $c = null; }
    echo "built\n";
}
build(true);
echo "after\n";
"#,
    );
    assert_eq!(out, "register a\nregister b\nregister c\nbuilt\nafter\n");
}

/// The boxed-slot overwrite leaves a clean heap: before the fix the superseded object's
/// Mixed box stayed live for the whole process, so the value assertions above alone would
/// not have pinned the ownership half of the fix.
#[test]
fn test_overwrite_of_a_boxed_object_slot_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Item {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo "d{$this->id}"; }
}
function run(): void {
    $x = new Item('1');
    $x = new Item('2');
    $x = null;
}
run();
echo "|end";
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "d1d2|end");
}

/// A temporary passed to a NULLABLE object parameter is destroyed when the call returns, the
/// same as one passed to a non-nullable parameter.
///
/// `?C` has the Mixed ABI, so the argument is boxed and then run through the nullable nominal
/// guard, which FORWARDS the same cell. The forwarded value is borrowed, so the post-call
/// cleanup — which asks only about the argument itself — released nothing and the box kept the
/// object alive forever. The non-nullable and untyped arms are the controls: neither emits the
/// guard, and both were already correct.
#[test]
fn test_destruct_of_a_temporary_passed_to_a_nullable_object_parameter() {
    let out = compile_and_run(
        r#"<?php
class C {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __destruct() { echo "destruct {$this->id}\n"; }
}
function nn(C $c): void { echo "nn\n"; }
function nu(?C $c): void { echo "nu\n"; }
function un($c): void { echo "un\n"; }
nn(new C('nn'));
nu(new C('nu'));
un(new C('un'));
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        "nn\ndestruct nn\nnu\ndestruct nu\nun\ndestruct un\nafter\n"
    );
}

/// The nullable-parameter call leaves a clean heap: the leaked box kept the object live for the
/// whole process, so the ordering assertion alone would not pin the ownership half.
#[test]
fn test_nullable_object_parameter_call_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public function __destruct() { echo "d"; } }
function nu(?C $c): void { echo "n"; }
nu(new C());
echo "|end";
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "nd|end");
}

/// A subclass with no destructor inherits its parent's `__destruct`, dispatched
/// to the implementing ancestor's method.
#[test]
fn test_destruct_inherited_from_parent() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __destruct() { echo "base-dtor\n"; }
}
class Derived extends Base {
}
function run() { $d = new Derived(); }
run();
echo "done\n";
"#,
    );
    assert_eq!(out, "base-dtor\ndone\n");
}

/// An overriding subclass destructor runs instead of the parent's.
#[test]
fn test_destruct_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __destruct() { echo "base\n"; }
}
class Derived extends Base {
    public function __destruct() { echo "derived\n"; }
}
function run() { $d = new Derived(); }
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "derived\nend\n");
}

/// Objects held in an array are destructed when the array is released.
#[test]
fn test_destruct_objects_in_array() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public function __destruct() { echo "free "; }
}
function run() {
    $arr = [new Item(), new Item()];
    echo "built ";
}
run();
echo "done";
"#,
    );
    assert_eq!(out, "built free free done");
}

/// A class without `__destruct` is unaffected (no spurious calls, no crash), even
/// alongside a class that defines one.
#[test]
fn test_no_destruct_is_noop() {
    let out = compile_and_run(
        r#"<?php
class Plain {
    public int $v;
    public function __construct() { $this->v = 7; }
}
class Loud {
    public function __destruct() { echo "loud "; }
}
function run() {
    $p = new Plain();
    $l = new Loud();
    echo $p->v . " ";
}
run();
echo "end";
"#,
    );
    assert_eq!(out, "7 loud end");
}

/// The re-entrancy guard: a destructor that takes a local copy of `$this` (a
/// balanced incref then scope-exit decref) must not re-enter the free path or
/// double-run. The destructor runs exactly once.
#[test]
fn test_destruct_self_reference_guard() {
    let out = compile_and_run(
        r#"<?php
class Tricky {
    public function __destruct() {
        $tmp = $this;
        echo "x";
    }
}
function run() { $t = new Tricky(); }
run();
echo "|ok";
"#,
    );
    assert_eq!(out, "x|ok");
}

/// A destructor that releases a heap-backed property (a string) before the
/// object's own storage is freed runs cleanly.
#[test]
fn test_destruct_with_heap_property() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    private array $items;
    public function __construct() { $this->items = ["a", "b", "c"]; }
    public function __destruct() { echo "count:" . count($this->items); }
}
function run() { $h = new Holder(); }
run();
echo "|fin";
"#,
    );
    assert_eq!(out, "count:3|fin");
}

/// A method only the narrowed class declares (`if ($r instanceof Doc) { $r->take($x) }`)
/// dispatches on the runtime class, as for a `mixed` receiver, and its arguments are released
/// after the call. They were handed over without the caller's release, so an object passed
/// straight in (a `new` expression, a property read) was never destructed: in a `--web`
/// handler a page's blocks and the connection they held outlived every request.
#[test]
fn test_narrowed_receiver_call_releases_its_arguments() {
    let out = compile_and_run(
        r#"<?php
final class Conn { public function __construct(public int $n) {} public function __destruct() { echo "close {$this->n}\n"; } }
interface Response { public function done(): string; }
final class Doc implements Response {
    public function take(Conn $c): self { return new self(); }
    public function done(): string { return "doc"; }
}
final class Handler {
    public function __construct(private Conn $conn) {}
    public function handle(Response $r, int $n): string {
        if ($r instanceof Doc) {
            return $r->take($this->conn)->done() . " " . $r->take(new Conn($n + 10))->done();
        }
        return $r->done();
    }
    public function direct(Response $r, int $n): string {
        if ($r instanceof Doc) {
            return $r->take(new Conn($n + 20))->done();
        }
        return $r->done();
    }
}
function request(int $n): void {
    $h = new Handler(new Conn($n));
    echo $h->handle(new Doc(), $n), "\n";
    echo $h->direct(new Doc(), $n), "\n";
}
request(1);
request(2);
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "close 11\ndoc doc\nclose 21\ndoc\nclose 1\nclose 12\ndoc doc\nclose 22\ndoc\nclose 2\nend\n"
    );
}

/// A dynamically required temporary runs its destructor and can publish into an AOT receiver.
#[test]
fn test_dynamic_required_temporary_destructor_mutates_aot_receiver() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
class PublicationLog {
    public string $value = '';

    public function publish(string $value): void {
        $this->value = $value;
    }
}

$log = new PublicationLog();
$path = __DIR__ . '/dynamic.php';
require $path;
stage_publication($log);
echo $log->value;
"#,
            ),
            (
                "dynamic.php",
                r#"<?php
class DeferredPublication {
    private PublicationLog $log;

    public function __construct(PublicationLog $log) {
        $this->log = $log;
    }

    public function __destruct() {
        $this->log->publish('released');
    }
}

function stage_publication(PublicationLog $log): void {
    new DeferredPublication($log);
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "released");
}

/// A dynamic include releases an AOT temporary and runs its AOT destructor before returning.
#[test]
fn test_dynamic_required_aot_temporary_runs_aot_destructor() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
class AotPublicationLog {
    public string $value = '';
}

class AotDeferredPublication {
    private AotPublicationLog $log;

    public function __construct(AotPublicationLog $log) {
        $this->log = $log;
    }

    public function __destruct() {
        $this->log->value = 'released';
    }
}

$log = new AotPublicationLog();
$path = __DIR__ . '/dynamic.php';
require $path;
stage_aot_publication($log);
echo $log->value;
"#,
            ),
            (
                "dynamic.php",
                r#"<?php
function stage_aot_publication(AotPublicationLog $log): void {
    new AotDeferredPublication($log);
}
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(out, "released");
}

/// Storing a fresh object into a NULLABLE object property still releases the producer's
/// reference, so the object's refcount can reach zero and its destructor runs.
///
/// A `?C` slot has the Mixed representation, so the assignment goes through the runtime
/// nominal-class guard. That guard borrows its operand and hands back its own reference —
/// its codegen increfs the payload precisely because its EIR result is declared owned — so
/// the producer's reference is dead once the guard has run. It was not being released, which
/// left every such object one reference above zero: `__destruct` never ran and the payload
/// leaked. A non-nullable `C` slot never took that path and was always balanced.
#[test]
fn test_destruct_after_store_into_a_nullable_object_property() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public function __construct(private string $id) {}
    public function __destruct() { echo "drop:" . $this->id . "\n"; }
}
class Nullable { public ?Item $p = null; }
class Plain { public Item $p; }

function nulled(): void {
    $h = new Nullable();
    $h->p = new Item("nulled");
    $h->p = null;
}
function replaced(): void {
    $h = new Nullable();
    $h->p = new Item("first");
    $h->p = new Item("second");
}
function plain(): void {
    $h = new Plain();
    $h->p = new Item("plain");
}
nulled();
echo "|";
replaced();
echo "|";
plain();
echo "|end";
"#,
    );
    assert_eq!(
        out,
        "drop:nulled\n|drop:first\ndrop:second\n|drop:plain\n|end"
    );
}

/// The same store leaves a clean heap: the leaked reference above kept the payload alive for
/// the whole process, so a value assertion alone would not have pinned the ownership fix.
#[test]
fn test_nullable_object_property_store_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Item { public function __destruct() { echo "d"; } }
class Nullable { public ?Item $p = null; }
function run(): void {
    $h = new Nullable();
    $h->p = new Item();
}
run();
echo "|end";
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "d|end");
}

/// A discarded fluent result destructs at the end of its statement even when the method's
/// declared return type forces the receiver to be boxed on the way out.
///
/// `return $this` hands the caller a borrowed receiver, so the return lowering acquires it to
/// balance the caller's release. A declared return type that cannot carry a bare object pointer
/// — a nullable object, `mixed`, or an object union — makes the return coercion box the receiver
/// into a freshly allocated Mixed cell first. That cell already owns its reference and is itself
/// handed over as owned, so the extra acquire landed on the box: the caller's single release
/// could not free it, the object never reached zero, and `__destruct` never ran. Only the
/// object-only return type (`: Cfg`) was ever balanced, which is why a fluent DSL lost exactly
/// the trailing call of a chain — the one whose result nothing else consumes.
#[test]
fn test_destruct_of_a_discarded_fluent_result_boxed_by_its_return_type() {
    let out = compile_and_run(
        r#"<?php
class Other {}
class Cfg {
    public function __construct(private string $id) {}
    public function plain(): Cfg { return $this; }
    public function nullable(): ?Cfg { return $this; }
    public function anything(): mixed { return $this; }
    public function united(): Cfg|Other { return $this; }
    public function __destruct() { echo "drop:" . $this->id . "\n"; }
}
(new Cfg("plain"))->plain();
echo "|";
(new Cfg("nullable"))->nullable();
echo "|";
(new Cfg("anything"))->anything();
echo "|";
(new Cfg("united"))->united();
echo "|end";
"#,
    );
    assert_eq!(
        out,
        "drop:plain\n|drop:nullable\n|drop:anything\n|drop:united\n|end"
    );
}

/// A boxed `return $this` still keeps the receiver alive for the binding that owns it: the box
/// holds its own reference, so dropping the extra acquire must not destruct a live object.
#[test]
fn test_boxed_returned_this_keeps_a_live_receiver_alive() {
    let out = compile_and_run(
        r#"<?php
class Cfg {
    public function __construct(private string $id) {}
    public function nullable(): ?Cfg { return $this; }
    public function anything(): mixed { return $this; }
    public function id(): string { return $this->id; }
    public function __destruct() { echo "drop:" . $this->id . "\n"; }
}
$c = new Cfg("kept");
$c->nullable();
echo "after-nullable:" . $c->id() . "|";
$c->anything();
echo "after-anything:" . $c->id() . "|";
$held = $c->nullable();
unset($c);
echo "held:" . $held->id() . "|";
unset($held);
echo "end";
"#,
    );
    assert_eq!(
        out,
        "after-nullable:kept|after-anything:kept|held:kept|drop:kept\nend"
    );
}

/// A boxed fluent chain destructs in statement order: every link is the same receiver, so the
/// object drops once, when the statement's trailing result is released.
#[test]
fn test_destruct_order_of_a_boxed_fluent_chain() {
    let out = compile_and_run(
        r#"<?php
class Cfg {
    public function __construct(private string $id) {}
    public function step(): ?Cfg { return $this; }
    public function __destruct() { echo "drop:" . $this->id . "\n"; }
}
function make(string $id): Cfg { return new Cfg($id); }
make("a")->step()->step();
echo "|";
make("b")->step();
echo "|end";
"#,
    );
    assert_eq!(out, "drop:a\n|drop:b\n|end");
}

/// The discarded boxed result leaves a clean heap: before the fix the object stayed one
/// reference above zero for the life of the process, so a value assertion alone would not
/// have pinned the ownership balance.
#[test]
fn test_discarded_boxed_fluent_result_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Other {}
class Cfg {
    public function nullable(): ?Cfg { return $this; }
    public function anything(): mixed { return $this; }
    public function united(): Cfg|Other { return $this; }
    public function __destruct() { echo "d"; }
}
function run(): void {
    (new Cfg())->nullable();
    (new Cfg())->anything();
    (new Cfg())->united();
}
run();
echo "|end";
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
    assert_eq!(out.stdout, "ddd|end");
}

/// An eval context in the enclosing function must not change when a discarded chain is destroyed.
///
/// A function that owns an eval context has every statically typed method call preceded by an
/// eval-bridge probe, and the probe boxes its receiver into a Mixed cell. The bridge only borrows
/// that cell, so the probe still owns it on both exits; when the miss path dropped it the receiver
/// stayed one reference above zero and the destructor never ran at all. `php -n` prints `destruct`
/// before `after chain`, so the ordering — not merely the presence of the line — is the assertion.
#[test]
fn test_destruct_of_discarded_chain_under_an_eval_context() {
    let out = compile_and_run(
        r#"<?php
class Cfg {
    public array $data = [];
    public function __destruct() { echo "destruct\n"; }
    public function args(array $a): static { $this->data = $a; return $this; }
    public function tag(string $t): static { $this->data[] = $t; return $this; }
}
function run(): void {
    eval('$seed = 1;');
    (new Cfg())->args([1, 2])->tag('x');
    echo "after chain\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "destruct\nafter chain\nend\n");
}

/// Releasing probe-boxed operands must not touch a cell the bridge handed back as the result.
///
/// An interpreted body may return the very cell it was passed, in which case that cell's
/// reference moved to the result and is no longer the caller's to drop. Releasing it anyway
/// would free `$local` while `$back` still names it, so this fixture pins the surviving object:
/// `$local` is read after the call and its destructor runs once, at scope exit.
#[test]
fn test_eval_call_returning_its_argument_keeps_the_returned_cell() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
}
function run(): void {
    eval('function echo_back($v) { return $v; }');
    $local = new T('local');
    $back = echo_back($local);
    echo "back is " . $back->n . "\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "back is local\nscope end\ndestruct local\nend\n");
}

/// Exactly one side compensates when an interpreted body returns the cell it was passed.
///
/// `statements::dispatch` retains a returned value whose activation scope owns nothing to
/// transfer -- a by-value parameter is bound `Borrowed` on purpose, so without the retain a
/// discarded `f($x);` released the CALLER's `$x`. The AOT frame therefore always releases the
/// cell it boxed: skipping that release when the published result happened to BE the operand
/// (which the bridge used to do, comparing the two pointers at runtime) counted the same
/// reference twice and left the object one reference above zero.
///
/// Both halves are pinned here because fixing either one alone breaks the other: `$kept` is
/// passed through a discarded call AND a used one, and `$other` through a call whose result does
/// not alias it. Each destructor must run exactly once, at scope exit.
///
/// Oracle: `php -n` 8.5.10 prints the asserted lines.
#[test]
fn test_eval_call_returning_its_argument_compensates_on_exactly_one_side() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
}
function run(): void {
    eval('function echo_back($v) { return $v; }
function ignore_it($v) { return "plain"; }');
    $kept = new T('kept');
    echo_back($kept);
    echo "after discarded\n";
    $used = echo_back($kept);
    echo "used is " . $used->n . "\n";
    $other = new T('other');
    echo ignore_it($other), "\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "after discarded\nused is kept\nplain\nscope end\ndestruct kept\ndestruct other\nend\n",
    );
}

/// A `static::` call inside an eval-context function releases the operands its override probe boxed.
///
/// The late-bound static path runs the same borrow-and-drop probe as instance dispatch, so a
/// receiver reaching it through a discarded chain has the same destructor to lose.
#[test]
fn test_destruct_of_discarded_chain_calling_a_late_bound_static() {
    let out = compile_and_run(
        r#"<?php
abstract class Base {
    public static function processValue(mixed $v, bool $flag = false): mixed { return $v; }
}
class Cfg extends Base {
    public mixed $data = null;
    public function __destruct() { echo "destruct\n"; }
    public function args(array $a): static {
        $this->data = static::processValue($a, true);
        return $this;
    }
    public function tag(string $t): static {
        $this->data = static::processValue($t, true);
        return $this;
    }
}
function run(): void {
    eval('$seed = 1;');
    (new Cfg())->args([1, 2])->tag('x');
    echo "after chain\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "destruct\nafter chain\nend\n");
}

/// A property read under an eval context must release the receiver cell its bridge call boxed.
///
/// `lower_eval_property_get` boxes a statically typed receiver into a Mixed cell with
/// `__rt_mixed_from_value`, which retains the object, and `__elephc_eval_property_get` only
/// borrows that cell. Nothing released it, so every `$obj->prop` read inside a function that
/// owns an eval context left the receiver one reference above zero and its `__destruct` never
/// ran. `php -n` prints `destruct made` after `scope end`, so the interleaving is the assertion.
#[test]
fn test_property_read_under_an_eval_context_releases_its_boxed_receiver() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
    public function keep(): T { return new T('made'); }
}
function run(): void {
    eval('$seed = 1;');
    $keeper = new T('keeper');
    $held = $keeper->keep();
    echo "held is " . $held->n . "\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "held is made\nscope end\ndestruct keeper\ndestruct made\nend\n"
    );
}

/// The stranded reference belongs to whichever object the property was read from.
///
/// Reading a property off a call argument rather than off the call result pins the leak to the
/// receiver of the read: `$local` is the object whose destructor the unreleased box swallowed,
/// while the call result and the receiver of the call both stayed correct.
#[test]
fn test_property_read_of_an_untouched_local_under_an_eval_context_still_destructs() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
    public function keep(T $o): T { return new T('made'); }
}
function run(): void {
    eval('$seed = 1;');
    $local = new T('local');
    $keeper = new T('keeper');
    $held = $keeper->keep($local);
    echo "local is " . $local->n . "\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "local is local\nscope end\ndestruct local\ndestruct keeper\ndestruct made\nend\n"
    );
}

/// A configurator that publishes itself from `__destruct` needs the property read to be balanced.
///
/// `$this->definition->setArguments(...)` reads a property off `$this` before dispatching, so the
/// unreleased receiver box kept the configurator alive for the whole program and the destructor
/// that does the publishing never ran. This is the shape a fluent configurator chain has.
#[test]
fn test_property_chain_receiver_under_an_eval_context_destructs_at_scope_exit() {
    let out = compile_and_run(
        r#"<?php
class Definition {
    public array $args = [];
    public function __destruct() { echo "destruct definition\n"; }
    public function setArguments(array $a): void { $this->args = $a; }
}
class Cfg {
    public Definition $definition;
    public function __construct() { $this->definition = new Definition(); }
    public function __destruct() { echo "destruct cfg\n"; }
    public function args(array $a): static {
        $this->definition->setArguments($a);
        return $this;
    }
}
function run(): void {
    eval('$seed = 1;');
    (new Cfg())->args([1, 2]);
    echo "after chain\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "destruct cfg\ndestruct definition\nafter chain\nend\n"
    );
}

/// `instanceof` under an eval context must release the receiver cell its bridge probe boxed.
///
/// The relation predicate answers through `__elephc_eval_object_is_a`, which borrows the boxed
/// receiver the same way the property read does. Neither the answered path nor the non-object
/// shortcut released it, so testing an object's class was enough to outlive its own destructor.
#[test]
fn test_instanceof_under_an_eval_context_releases_its_boxed_receiver() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
    public function keep(): int { return 1; }
}
function run(): void {
    eval('$seed = 1;');
    $keeper = new T('keeper');
    $keeper->keep();
    $local = new T('local');
    echo ($local instanceof T) ? "yes\n" : "no\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "yes\nscope end\ndestruct keeper\ndestruct local\nend\n"
    );
}

/// `is_callable()` under an eval context must release the callback cell its bridge probe boxed.
///
/// `lower_eval_is_callable` boxes a statically typed callback into a Mixed cell and
/// `__elephc_eval_is_callable` only borrows it, so asking whether an object is callable was by
/// itself enough to strand a reference on that object and swallow its `__destruct`. The probe
/// answers with a plain integer in the register the release helper reuses, so the answer is
/// parked in the unused scratch result slot across the decref. `php -n` prints both destructors
/// after `scope end` and in declaration order, so the interleaving is the assertion.
#[test]
fn test_is_callable_under_an_eval_context_releases_its_boxed_callback() {
    let out = compile_and_run(
        r#"<?php
interface Marker {}
class T implements Marker {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
    public function __invoke(): int { return 7; }
}
class U extends T {}
function run(): void {
    eval('$seed = 1;');
    $keeper = new T('keeper');
    $local = new U('local');
    var_dump(is_callable($local));
    var_dump(is_callable($keeper));
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nscope end\ndestruct keeper\ndestruct local\nend\n"
    );
}

/// `method_exists()` and `property_exists()` must release the two cells their bridge probe boxed.
///
/// `lower_eval_member_exists` boxes both the target and the member name for
/// `__elephc_eval_member_exists`, which borrows them. Neither was released, so a single
/// membership question outlived the object it asked about. Both builtins share the emitter, so
/// one asks about a method and the other about a property.
#[test]
fn test_member_exists_under_an_eval_context_releases_its_boxed_operands() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
    public function keep(): int { return 1; }
}
function run(): void {
    eval('$seed = 1;');
    $keeper = new T('keeper');
    $local = new T('local');
    var_dump(method_exists($local, 'keep'));
    var_dump(property_exists($keeper, 'n'));
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nscope end\ndestruct keeper\ndestruct local\nend\n"
    );
}

/// `class_implements()` and `class_parents()` must release the target cell their bridge boxed.
///
/// `lower_eval_class_relation` boxes the target for `__elephc_eval_class_relation`, which
/// borrows it and publishes a fresh relation map as its result. The box was never released, so
/// reading an object's interfaces or parents kept that object alive past its scope. The release
/// compares against the published cell first, because an interpreted body may hand back the very
/// cell it was passed.
#[test]
fn test_class_relation_under_an_eval_context_releases_its_boxed_target() {
    let out = compile_and_run(
        r#"<?php
interface Marker {}
class T implements Marker {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
}
class U extends T {}
function run(): void {
    eval('$seed = 1;');
    $keeper = new U('keeper');
    $local = new U('local');
    echo count(class_implements($local)), "\n";
    echo count(class_parents($keeper)), "\n";
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "1\n1\nscope end\ndestruct keeper\ndestruct local\nend\n");
}

/// A spread call under an eval context must not keep the container it spread.
///
/// `f(...$args)` after an eval barrier cannot use the fixed native argument ABI, so it is lowered
/// to `EvalFunctionCallArray` and the container is boxed for the bridge by
/// `coerce_eval_function_arg_array`. That box is a fresh owning temporary and the bridge only
/// borrows it, but nothing released it: the box held a reference on the array for the rest of the
/// program. An array literal spread straight into the call leaked twice over, because the raw
/// `MixedBox` also kept the producer's reference to the temporary it boxed.
///
/// Two shapes, one release each. `$args` is a borrowed local, so only the box is this frame's;
/// `[2]` is an owning temporary, so `box_value_as_mixed` also hands the array's own reference to
/// the box. Both are proven by the block count rather than by output, because an int element has
/// no destructor to observe: this program ended at `live_blocks=12` (880 bytes) before the release
/// and ends at `live_blocks=8` (496 bytes) after it — four blocks, two per leaked container.
///
/// Six further blocks used to remain -- the eval context, scope, and module registration an
/// `eval()` frame allocated once -- and were pinned as `live_blocks=6`. The count was eight while
/// `append_unpacked_call_arg_values` still leaked the Mixed cell `__rt_mixed_array_get` freshly
/// boxes for each spread element, and six once that element debt was paid after the call. The
/// eval frame now gives those six back as well, so the program ends with a clean heap.
#[test]
fn test_spread_call_under_an_eval_context_releases_its_boxed_container() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T {
    public function __construct(public string $n) {}
    public function __destruct() { echo "destruct {$this->n}\n"; }
}
function takesInt(int $x): int { return 1; }
function run(): void {
    eval('$seed = 1;');
    $local = new T('local');
    $args = [1];
    takesInt(...$args);
    takesInt(...[2]);
    echo "scope end\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out.stdout, "scope end\ndestruct local\nend\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the two spread containers and their elements to be released, got: {}",
        out.stderr
    );
}

/// A spread element read out of its container is released once the callee has been handed it.
///
/// `append_unpacked_call_arg_values` reads each element with `array_get`, which
/// `src/codegen_support/runtime/objects/mixed_array_get.rs` documents as returning an OWNED cell on
/// every path: it increfs a stored boxed cell, or freshly boxes a typed slot through
/// `__rt_mixed_from_value`, which retains an object payload. Every consumer of the evaluated
/// argument treated that cell as borrowed, so the reference was never given back and the object
/// spread into the call outlived the program: php prints `destruct local`, elephc printed nothing.
///
/// Releasing it at the read is not the fix — on a typed slot the cell is fresh and the callee would
/// be handed freed storage — so `EvaluatedCallArg::owned` carries the debt to the release that
/// follows the call.
#[test]
fn test_object_spread_under_an_eval_context_destructs_its_element() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $value): void { echo "takes {$value->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $local = new T('local');
    $args = [$local];
    takes(...$args);
    echo "after spread\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "takes local\nafter spread\ndestruct local\nend\n");
}

/// Spreading an array literal releases the element the literal put in it, not just the container.
///
/// `takes(...[$local])` spreads an owning temporary: the container box is released by
/// `release_owned_call_arg_temporaries` on the AOT side, and the element cell `array_get` handed
/// the interpreter is released after the call. Before the element half, php printed
/// `destruct local` here and elephc printed nothing.
#[test]
fn test_object_array_literal_spread_under_an_eval_context_destructs_its_element() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $value): void { echo "takes {$value->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $local = new T('local');
    takes(...[$local]);
    echo "after spread\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "takes local\nafter spread\ndestruct local\nend\n");
}

/// Every element of a multi-element spread is released, not only the first.
///
/// The debt is per argument, so a two-object spread owes two releases. Both objects outlived the
/// program before the fix; the order asserted here is the one `php -n` prints.
#[test]
fn test_two_object_spread_under_an_eval_context_destructs_both_elements() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $first, T $second): void { echo "takes {$first->name} {$second->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $one = new T('one');
    $two = new T('two');
    $args = [$one, $two];
    takes(...$args);
    echo "after spread\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "takes one two\nafter spread\ndestruct one\ndestruct two\nend\n"
    );
}

/// A string key spreads as a named argument and still owes the same release.
///
/// `append_unpacked_call_arg_values` reads the value with `array_get` on the named branch too, and
/// that branch binds through `bind_native_function_named_arg` rather than the positional one, so it
/// is asserted separately.
#[test]
fn test_named_key_object_spread_under_an_eval_context_destructs_its_element() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $value): void { echo "takes {$value->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $args = ['value' => new T('named')];
    takes(...$args);
    echo "after spread\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "takes named\nafter spread\ndestruct named\nend\n");
}

/// A spread that follows a positional argument leaves both arguments balanced.
///
/// The positional argument is a borrowed local and owes nothing; the spread element owes one
/// release. Mixing them proves the debt is tracked per argument and not per call.
#[test]
fn test_object_spread_after_a_positional_argument_destructs_both() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $first, T $second): void { echo "takes {$first->name} {$second->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $one = new T('one');
    $two = new T('two');
    $args = [$two];
    takes($one, ...$args);
    echo "after spread\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "takes one two\nafter spread\ndestruct one\ndestruct two\nend\n"
    );
}

/// `call_user_func_array()` unpacks through the same reader and stays balanced.
///
/// `eval_array_call_arg_values` calls `append_unpacked_call_arg_values` directly, so the argument
/// array it walks produces the same owned element cells. This shape was already balanced before the
/// fix because `call_user_func_array` releases an array-literal argument of its own accord; it is
/// asserted so the new release cannot turn that into a double release.
#[test]
fn test_call_user_func_array_with_an_object_element_destructs_it() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(T $value): void { echo "takes {$value->name}\n"; }
function run(): void {
    eval('$seed = 1;');
    $local = new T('local');
    call_user_func_array('takes', [$local]);
    echo "after call\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "takes local\nafter call\ndestruct local\nend\n");
}

/// Spreading arrays of objects into a variadic builtin keeps the objects balanced.
///
/// `array_merge(...$arrays)` binds each spread element into the builtin's variadic array, where the
/// debt is settled at the store rather than after the call: `__elephc_eval_value_array_set` increfs
/// the value before the setter consumes it, so the variadic array already holds its own reference.
/// Releasing again after the call would free a live element, which is why this shape is asserted.
#[test]
fn test_object_arrays_spread_into_a_builtin_stay_balanced() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function run(): void {
    eval('$seed = 1;');
    $arrays = [[new T('one')], [new T('two')]];
    $merged = array_merge(...$arrays);
    echo "merged " . count($merged) . "\n";
    unset($merged);
    echo "after merge\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "merged 2\nafter merge\ndestruct one\ndestruct two\nend\n"
    );
}

/// A spread into a by-reference parameter binds the element itself and releases it like any other.
///
/// php's unpack makes `$args[0]` the by-reference argument (also from a function whose `eval`
/// gives it the hidden `func_get_args()` collector): the callee's write replaces the element and
/// destructs the original there, and the replacement dies with `$args`. Every object is
/// destructed exactly once.
#[test]
fn test_object_spread_into_a_by_ref_parameter_destructs_its_element() {
    let out = compile_and_run_capture(
        r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function takes(&$value): void { echo "takes {$value->name}\n"; $value = new T('replaced'); }
function run(): void {
    eval('$seed = 1;');
    $args = [new T('local')];
    takes(...$args);
    echo "after spread {$args[0]->name}\n";
}
run();
echo "end\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "takes local\ndestruct local\nafter spread replaced\ndestruct replaced\nend\n"
    );
}

/// An array literal built inside interpreted code releases the elements it stored.
///
/// `eval_indexed_array` stores each element with `values.array_set`, and
/// `__elephc_eval_value_array_set` increfs the value before the setter consumes it. An element
/// expression that ALLOCATED its cell — here `new T('literal')` — therefore left a second reference
/// with the literal builder that nothing paid: `unset($a)` freed the array and the object survived
/// it, so php printed `destruct literal` and elephc printed nothing. The element is built by the
/// interpreter rather than by AOT code only when the literal itself is interpreted, which is why
/// the fixture puts it in a required file's closure.
#[test]
fn test_array_literal_element_built_by_interpreted_code_is_released_with_its_container() {
    let out = compile_cli_files_and_run(
        &[
            (
                "entry.php",
                r#"<?php
class T {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
}
function run(): void {
    $closure = require __DIR__ . '/dynamic.php';
    $closure();
    echo "run end\n";
}
run();
echo "end\n";
"#,
            ),
            (
                "dynamic.php",
                r#"<?php
return function (): void {
    $a = [new T('literal')];
    echo "built " . count($a) . "\n";
    unset($a);
    echo "closure end\n";
};
"#,
            ),
        ],
        "entry.php",
    );
    assert_eq!(
        out,
        "built 1\ndestruct literal\nclosure end\nrun end\nend\n"
    );
}

/// A `foreach` subject the loop ALLOCATED is destroyed when the loop is done with it.
///
/// The value never reaches a variable, so nothing but the loop can release it. Note where the
/// destructor lands: at the end of the `foreach` STATEMENT, before the `echo` on the next line.
/// Measured with `php -n` 8.5.6.
#[test]
fn test_destruct_of_a_foreach_subject_the_loop_allocated() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public $a = 1;
    public $b = 2;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct {$this->tag}\n"; }
}
foreach (new Probe('new') as $k => $v) { echo "$k=$v,"; }
echo "\nafter\n";
"#,
    );
    assert_eq!(out, "a=1,b=2,tag=new,destruct new\n\nafter\n");
}

/// A `foreach` subject read from a variable outlives the loop.
///
/// The mirror of the case above: the loop borrows the caller's reference, so the object dies
/// with the variable and NOT with the loop. Measured with `php -n` 8.5.6.
#[test]
fn test_no_destruct_of_a_foreach_subject_held_in_a_variable() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public $a = 1;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct {$this->tag}\n"; }
}
$held = new Probe('held');
foreach ($held as $k => $v) { echo "$k=$v,"; }
echo "\nafterloop\n";
unset($held);
echo "end\n";
"#,
    );
    assert_eq!(out, "a=1,tag=held,\nafterloop\ndestruct held\nend\n");
}

/// Breaking out of a `foreach` over an allocated subject still destroys it.
///
/// The release has to sit on every exit edge, not only on the loop running out of values.
/// Measured with `php -n` 8.5.6.
#[test]
fn test_destruct_of_a_foreach_subject_broken_out_of() {
    let out = compile_and_run(
        r#"<?php
class Probe {
    public $a = 1;
    public $b = 2;
    public $tag;
    public function __construct($t) { $this->tag = $t; }
    public function __destruct() { echo "destruct {$this->tag}\n"; }
}
foreach (new Probe('broken') as $k => $v) { echo "$k=$v,"; break; }
echo "\nafter\n";
"#,
    );
    assert_eq!(out, "a=1,destruct broken\n\nafter\n");
}

/// An overwrite publishes the new value before it releases the old one, as php's assignment does:
/// the old value's destructor may write the same place, and its write must survive. Each store
/// used to retire first, so the resumed store clobbered the destructor's value (a static property
/// read `2` for php's `99`, with a bad refcount), and a destructor that grew the same array freed
/// the buffer the resumed store then wrote back into its property (garbage strings, leaks). The
/// expectation is host PHP 8.5.
#[test]
fn test_overwrite_survives_a_destructor_that_writes_the_same_place() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Holder { public static $shared; public array $a = []; public array $l = []; }
class Dropper {
    public function __construct(public Holder $h, public string $mode) {}
    public function __destruct() {
        if ($this->mode === 'static') { Holder::$shared = 'from-destructor'; }
        if ($this->mode === 'key') { $this->h->a['k'] = 30; $this->h->a['s'] = str_repeat('x', 3); }
        if ($this->mode === 'grow') {
            $this->h->l[0] = 5;
            for ($i = 1; $i < 20; $i++) { $this->h->l[] = str_repeat('y', $i); }
            for ($i = 0; $i < 20; $i++) { $this->h->a["k$i"] = str_repeat('z', $i + 1); }
        }
    }
}
$h = new Holder();
Holder::$shared = new Dropper($h, 'static');
Holder::$shared = 'plain';
echo Holder::$shared, "\n";
$h->a['k'] = new Dropper($h, 'key');
$h->a['k'] = 'plain';
echo $h->a['k'], ' ', $h->a['s'], "\n";
$g = new Holder();
$g->l[0] = new Dropper($g, 'grow');
$g->l[0] = 'plain';
echo count($g->l), ' ', $g->l[0], ' ', $g->l[19], ' ', count($g->a), ' ', $g->a['k19'], "\n";
$k = new Holder();
$k->a['x'] = new Dropper($k, 'grow');
$k->a['x'] = 'plain';
echo count($k->a), ' ', $k->a['x'], ' ', $k->a['k19'], ' ', count($k->l), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "from-destructor\n",
            "30 xxx\n",
            "20 5 yyyyyyyyyyyyyyyyyyy 20 zzzzzzzzzzzzzzzzzzzz\n",
            "21 plain zzzzzzzzzzzzzzzzzzzz 20\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
