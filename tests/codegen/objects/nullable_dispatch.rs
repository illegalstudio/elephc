//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of nullable object dispatch and
//! nullable scalar values that cross typed calls or array-mediated control flow.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Tests that a method call on a `?Holder` parameter dispatches correctly
/// when the receiver is non-null. Verifies the codegen unboxes the boxed
/// mixed cell to recover the concrete object pointer before reading the
/// class id and vtable.
/// Regression: nullable parameters were previously treated as opaque mixed
/// without unboxing to the concrete type.
#[test]
fn test_method_call_on_nullable_object_parameter() {
    // Calling a method through a `?Foo` parameter must dispatch to the
    // declared class — the runtime representation is a boxed mixed cell,
    // and the codegen now unboxes it to recover the concrete object
    // pointer before reading the class id.
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg;
    public function __construct(string $m) { $this->msg = $m; }
    public function getMsg(): string { return $this->msg; }
}
function deliver(?Holder $h): void {
    if ($h instanceof Holder) {
        echo $h->getMsg();
    }
}
deliver(new Holder("ok"));
"#,
    );
    assert_eq!(out, "ok");
}

/// Tests that a property read on a `?Holder` parameter returns the correct
/// value when the receiver is non-null. Verifies unboxing and property load
/// through a nullable parameter type.
#[test]
fn test_property_access_on_nullable_object_parameter() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg = "default";
    public function __construct(string $m) { $this->msg = $m; }
}
function read(?Holder $h): void {
    if ($h instanceof Holder) {
        echo $h->msg;
    }
}
read(new Holder("hi"));
"#,
    );
    assert_eq!(out, "hi");
}

/// Tests storing a `?Holder` through a setter and reading it back via a
/// typed `?Holder` field. Verifies both the write path (which must not
/// unbox when storing) and the load path (which must unbox and rebox the
/// mixed cell before method calls can land on the concrete object).
#[test]
fn test_nullable_object_property_round_trip_through_typed_field() {
    // Storing a nullable object into a typed field and reading it back
    // exercises both write and load paths through the boxed
    // representation — the unbox must run on read so subsequent method
    // calls land on the real object.
    let out = compile_and_run(
        r#"<?php
class Holder {
    public string $msg;
    public function __construct(string $m) { $this->msg = $m; }
    public function getMsg(): string { return $this->msg; }
}
class Box {
    public ?Holder $h = null;
    public function setIt(?Holder $h): void { $this->h = $h; }
}
$b = new Box();
$b->setIt(new Holder("via-box"));
if ($b->h instanceof Holder) {
    echo $b->h->getMsg();
}
"#,
    );
    assert_eq!(out, "via-box");
}

/// Verifies a nullable string declared by an ancestor is unboxed before it crosses a typed
/// instance-method argument boundary on a concrete descendant.
#[test]
fn test_inherited_nullable_string_property_passes_as_typed_method_argument() {
    let out = compile_and_run(
        r#"<?php
class PathLocator {
    public function locate(string $resource, ?string $currentDirectory = null): string {
        return ($currentDirectory ?? 'missing') . '/' . $resource;
    }
}

class PathLoaderBase {
    private ?string $currentDirectory = null;

    public function setCurrentDirectory(string $currentDirectory): void {
        $this->currentDirectory = $currentDirectory;
    }

    private function importRelative(PathLocator $locator, string $resource): string {
        return $locator->locate($resource, $this->currentDirectory);
    }

    public function import(PathLocator $locator, string $resource): string {
        return $this->importRelative($locator, $resource);
    }
}

class PathLoader extends PathLoaderBase {}

$loader = new PathLoader();
$loader->setCurrentDirectory('/fixture/config');
echo $loader->import(new PathLocator(), 'services.php');
"#,
    );
    assert_eq!(out, "/fixture/config/services.php");
}

/// Verifies an inherited nullable string retains its Mixed layout when inserted into an initially
/// empty `array<mixed>` and later read by `foreach`.
///
/// The array's static element type alone does not establish the physical runtime layout of a
/// default empty property. `array_unshift()` must therefore normalize the array before writing a
/// boxed nullable value, or the loop will interpret its pointer as an integer string.
#[test]
fn test_inherited_nullable_string_survives_array_unshift_and_foreach() {
    let out = compile_and_run(
        r#"<?php
class ArrayPathLocator {
    private array $paths = [];

    public function locate(string $resource, ?string $currentDirectory = null): string {
        $paths = $this->paths;
        if (null !== $currentDirectory) {
            array_unshift($paths, $currentDirectory);
        }

        foreach ($paths as $path) {
            return $path . '/' . $resource;
        }

        return 'missing';
    }
}

class ArrayPathLoaderBase {
    private ?string $currentDirectory = null;

    public function setCurrentDirectory(string $currentDirectory): void {
        $this->currentDirectory = $currentDirectory;
    }

    private function importRelative(ArrayPathLocator $locator, string $resource): string {
        return $locator->locate($resource, $this->currentDirectory);
    }

    public function import(ArrayPathLocator $locator, string $resource): string {
        return $this->importRelative($locator, $resource);
    }
}

class ArrayPathLoader extends ArrayPathLoaderBase {}

$loader = new ArrayPathLoader();
$loader->setCurrentDirectory('/fixture/config');
echo $loader->import(new ArrayPathLocator(), 'services.php');
"#,
    );
    assert_eq!(out, "/fixture/config/services.php");
}

/// Verifies a nullable object parameter is materialized correctly through a `??=` merge.
///
/// The parameter arrives in boxed union storage. Once the non-null branch has won, the merge
/// result is a concrete object and must not retain the outer Mixed cell as though it were the
/// object's pointer. A later typed call must receive the original object instance.
#[test]
fn test_nullable_named_object_param_null_coalesce_merge_materializes_for_typed_call() {
    let out = compile_and_run(
        r#"<?php
class CoalesceState {
    public function value(): string { return 'ok'; }
}

class CoalesceConsumer {
    public static function read(?CoalesceState $state): string {
        return $state->value();
    }

    public static function resolve(?CoalesceState $state): string {
        $state ??= new CoalesceState();

        return self::read($state);
    }
}

echo CoalesceConsumer::resolve(new CoalesceState()), ':', CoalesceConsumer::resolve(null);
"#,
    );
    assert_eq!(out, "ok:ok");
}

/// Verifies a nullable nominal parameter remains borrowed while a child constructor delegates it.
///
/// The parent constructor retains the nullable value in its typed property. The child-side
/// nominal guard must therefore not release the original Mixed cell after the call: a later
/// property replacement would otherwise free already-reused storage.
#[test]
fn test_nullable_nominal_parent_constructor_parameter_keeps_its_boxed_owner() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface ParentLink {}

class Root implements ParentLink {}

class BaseNode {
    protected ?ParentLink $parent;

    public function __construct(?ParentLink $parent = null) {
        $this->parent = $parent;
    }
}

class ChildNode extends BaseNode {
    public function __construct(?ParentLink $parent = null) {
        parent::__construct($parent);
    }

    public function setParent(ParentLink $parent): void {
        $this->parent = $parent;
    }

    public function hasParent(): bool {
        return $this->parent instanceof ParentLink;
    }
}

$child = new ChildNode();
$child->setParent(new Root());
echo $child->hasParent() ? 'ok' : 'bad';
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
}

/// Tests `?->{expr}?->prop` nullsafe dynamic property chain with a declared
/// property. Verifies that the dynamic property name expression is evaluated
/// even when the receiver is non-null and the chain short-circuits to null
/// only when a link in the chain is null.
#[test]
fn test_nullsafe_dynamic_property_chain_reads_declared_property() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public ?Box $next = null;
    public int $v;
    public function __construct(int $v) { $this->v = $v; }
}
$name = "next";
$a = new Box(1);
$a->next = new Box(2);
echo $a?->{$name}?->v;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests that a nullsafe chain with a null receiver skips evaluation of
/// the property name expression. Verifies short-circuit semantics: only the
/// receiver is checked for null before the chain terminates, and the
/// dynamic property name expression must not be evaluated.
#[test]
fn test_nullsafe_dynamic_property_skips_name_expression_on_null_receiver() {
    let out = compile_and_run(
        r#"<?php
function property_name(): string {
    echo "name-evaluated";
    return "next";
}
class Box {
    public ?Box $next = null;
    public int $v = 1;
}
$a = null;
echo $a?->{property_name()}?->v ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

/// Regression test: method call on a `?Foo` receiver previously returned the
/// boxed-mixed tag word instead of the method's return value. Uses strlen
/// on the returned string to verify payload bytes are correct and not
/// corrupted by a missing unbox.
#[test]
fn test_nullable_object_method_call_returns_correct_string_length() {
    // Regression for the bug where a method call on a `?Foo` receiver
    // returned the boxed-mixed tag word instead of the method's return
    // value. Asserting strlen of the returned string verifies that the
    // payload bytes match exactly.
    let out = compile_and_run(
        r#"<?php
class Tag {
    public string $name;
    public function __construct(string $n) { $this->name = $n; }
    public function getName(): string { return $this->name; }
}
function show(?Tag $t): void {
    if ($t instanceof Tag) {
        $name = $t->getName();
        echo strlen($name);
        echo ":";
        echo $name;
    }
}
show(new Tag("Europe/Paris"));
"#,
    );
    assert_eq!(out, "12:Europe/Paris");
}

/// Tests two chained method calls on a `?Inner` value returned from a
/// method on `?Outer`. Verifies that both calls unbox correctly and that
/// the chain dispatches to the concrete types at each step.
#[test]
fn test_nullable_object_chain_method_calls() {
    // Two chained method calls on a `?Foo` value — both must unbox.
    let out = compile_and_run(
        r#"<?php
class Inner {
    public string $value;
    public function __construct(string $v) { $this->value = $v; }
    public function get(): string { return $this->value; }
}
class Outer {
    public ?Inner $inner = null;
    public function __construct(?Inner $i) { $this->inner = $i; }
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer(new Inner("nested"));
echo $o->getInner()->get();
"#,
    );
    assert_eq!(out, "nested");
}

/// Tests that a chain of method calls where an intermediate receiver is
/// null produces a fatal "Call to a member function on null" error and
/// does not panic or produce wrong output.
#[test]
fn test_nullable_object_chain_method_call_on_null_receiver_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Inner {
    public function get(): string { return "never"; }
}
class Outer {
    public ?Inner $inner = null;
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer();
echo $o->getInner()->get();
"#,
    );
    assert!(
        err.contains("Call to a member function get() on null"),
        "{err}"
    );
}

/// Tests that when a chain method call receiver is null, argument evaluation
/// is skipped entirely. Verifies the compiler does not evaluate arguments
/// before the null check fatal — PHP semantics require arguments are not
/// evaluated when the receiver is null.
#[test]
fn test_nullable_object_null_receiver_fatal_skips_arguments() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function noisy(): string {
    file_get_contents("missing-nullable-method-arg.txt");
    return "unused";
}
class Inner {
    public function get(string $unused): string { return "never"; }
}
class Outer {
    public ?Inner $inner = null;
    public function getInner(): ?Inner { return $this->inner; }
}
$o = new Outer();
echo $o->getInner()->get(noisy());
"#,
    );
    assert!(
        err.contains("Call to a member function get() on null"),
        "{err}"
    );
    assert!(
        !err.contains("file_get_contents"),
        "argument side effect ran before nullable receiver fatal: {err}"
    );
}

/// Reading a property on a null `?Holder` receiver under `??` yields the fallback and emits
/// NOTHING. `??` suppresses the "Attempt to read property on null" warning — `php -n` prints
/// `fallback` on stdout and leaves stderr empty for this exact program.
///
/// This test used to assert the warning WAS emitted, which is what elephc did and what PHP does
/// not. It was written from the implementation rather than from the language, so the fix that
/// made the two agree turned it red. The warning-on-a-plain-read case is the test below, which
/// is where that assertion belongs.
#[test]
fn test_nullable_object_property_access_on_null_receiver_coalesces_without_warning() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public string $msg = "unused";
}
function read(?Holder $h): void {
    echo $h->msg ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "", "`??` must suppress the property-on-null warning");
}

/// The companion, and the one that keeps the warning honest: a PLAIN read of a property on a
/// null receiver — no `??` anywhere — warns and yields null, exactly as `php -n` does.
#[test]
fn test_nullable_object_property_access_on_null_receiver_warns_without_coalesce() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public string $msg = "unused";
}
function read(?Holder $h): void {
    echo "[", $h->msg, "]";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[]");
    assert!(
        out.stderr.contains("Warning: Attempt to read property \"msg\" on null"),
        "{}",
        out.stderr
    );
}

/// Tests that the error-control @ operator on a null property access
/// suppresses the warning. Verifies that `@$h->msg` on a null receiver
/// produces no warning and falls through to the ?? fallback.
#[test]
fn test_error_control_suppresses_nullable_property_warning() {
    let out = compile_and_run_capture(
        r#"<?php
class Holder {
    public string $msg = "unused";
}
function read(?Holder $h): void {
    echo @$h->msg ?? "fallback";
}
read(null);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "fallback");
    assert_eq!(out.stderr, "");
}

/// Tests that assigning a property on a non-null `?Holder` receiver executes
/// the write and returns the new value. Verifies both that the rhs function
/// is called (output "rhs|") and that the written value "updated" is echoed.
#[test]
fn test_nullable_object_property_assign_writes_non_null_receiver() {
    let out = compile_and_run(
        r#"<?php
function rhs(): string {
    echo "rhs|";
    return "updated";
}
class Holder {
    public string $msg = "initial";
}
function write(?Holder $h): void {
    $h->msg = rhs();
    echo $h->msg;
}
write(new Holder());
"#,
    );
    assert_eq!(out, "rhs|updated");
}

/// Tests that assigning a property on a null receiver produces a fatal
/// error after the RHS has been evaluated. Verifies: receiver function
/// runs, rhs function runs (but its return is unused), and the uncaught
/// Error is reported after those effects through the common throwable path.
#[test]
fn test_nullable_object_property_assign_on_null_receiver_fatals_after_rhs() {
    let out = compile_and_run_capture(
        r#"<?php
function receiver(): ?Holder {
    echo "receiver|";
    return null;
}
function rhs(): string {
    echo "rhs|";
    return "unused";
}
class Holder {
    public string $msg = "initial";
}
receiver()->msg = rhs();
echo "after";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert_eq!(out.stdout.split_once("\nFatal error:").unwrap().0, "receiver|rhs|");
    assert!(out.stdout.contains("Uncaught Error: Attempt to assign property \"msg\" on null"),
        "{}", out.stdout);
    assert!(!out.stdout.contains("after"), "the rejected write must terminate execution");
    assert_eq!(out.stderr, "");
}

/// A nullable parameter write raises a catchable Error and retires its unused string RHS.
#[test]
fn test_nullable_object_property_assign_on_null_receiver_is_catchable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Holder { public string $msg = ''; }
function write(?Holder $holder): void { $holder->msg = str_repeat('x', 24); }
for ($i = 0; $i < 8; $i++) {
    try { write(null); echo 'bad'; }
    catch (Error $error) { echo $error->getMessage(), '|'; }
}
echo 'after';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout,
        format!("{}after", "Attempt to assign property \"msg\" on null|".repeat(8)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Tests that a spread argument reaches a method called on a boxed (retyped, conditionally
/// assigned) receiver as the arguments it holds, not as one array operand.
///
/// `$object` is re-bound to another class inside the `if`, so its slot is a boxed cell and the
/// call goes through the boxed-receiver path. `$object->deprecate($k, ...$rest)` counts two
/// arguments against a three-parameter method, which dropped the signature and handed the spread
/// array to the dispatch as a single plain operand; the call then died with "Call to a member
/// function deprecate() on null". Symfony's `debug:container --parameters` is this shape.
#[test]
fn test_spread_argument_reaches_method_on_boxed_receiver_in_loop() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag {
    public array $deprecated = [];
    public function deprecate(string $name, string $package, string $version, string $message = 'm'): void
    {
        $this->deprecated[$name] = [$package, $version, $message];
    }
}
class Builder {
    public function nothing(): int { return 0; }
}
function run(int $mode, array $source): string {
    $object = new Builder();
    if ($mode > 0) {
        $object = new Bag();
        foreach ($source as $k => $rest) {
            $object->deprecate($k, ...$rest);
        }
        $object->deprecate('lit', ...['pkg', '9.9']);
    }
    return json_encode($object instanceof Bag ? $object->deprecated : []);
}
echo run($argc, ['a' => ['p1', '1.0'], 'b' => ['p2', '2.0', 'custom']]), "\n";
echo run($argc - 1, []), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "{\"a\":[\"p1\",\"1.0\",\"m\"],\"b\":[\"p2\",\"2.0\",\"custom\"],\"lit\":[\"pkg\",\"9.9\",\"m\"]}\n[]\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
