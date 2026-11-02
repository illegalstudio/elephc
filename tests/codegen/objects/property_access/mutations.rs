//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property mutations, including class array of objects property access, class property array push, and class property array assign.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// A fallback replacing the local receiver still writes through the originally pinned object.
#[test]
fn test_asymmetric_followup_readonly_coalesce_replaced_local() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box {
    public readonly int $value;
    public function __destruct() { echo 'D'; }
}
function fallback(&$box): int { echo 'F'; $box = new Box(); return 9; }
$box = new Box();
try { $box->value ??= fallback($box); echo 'bad'; }
catch (Error $error) { echo 'E'; }
unset($box);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "FDED");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies element writes into a property the compiler never saw declared. A stdClass built by
/// `(object) [...]` carries keys no class schema records, so `$state->vars[] = $v` and
/// `$state->routes[$k] = $v` had no declared slot to name and were refused with "Undefined
/// property: stdClass::vars" — even though the plain `$state->mark = 7` beside them was accepted.
/// Symfony's `CompiledUrlMatcherDumper` builds its entire working state exactly this way.
/// PHP outputs "7|a,b|3|n1,n2|v|5".
#[test]
fn test_stdclass_dynamic_property_element_writes() {
    let out = compile_and_run(
        r#"<?php
$state = (object) [
    'mark' => 0,
    'vars' => [],
    'routes' => [],
];

$state->vars[] = 'a';
$state->vars[] = 'b';
$state->routes['x'] = 1;
$state->routes['y'] = 2;
$state->mark = 7;

$state->fresh[] = 'n1';
$state->fresh[] = 'n2';
$state->keyed['k'] = 'v';

echo $state->mark, '|';
echo implode(',', $state->vars), '|';
echo $state->routes['x'] + $state->routes['y'], '|';
echo implode(',', $state->fresh), '|';
echo $state->keyed['k'], '|';
echo count((array) $state);
"#,
    );
    assert_eq!(out, "7|a,b|3|n1,n2|v|5");
}

/// Verifies the same element writes through `#[\AllowDynamicProperties]`, where an undeclared
/// property lives in the per-object side table while the declared one keeps its slot.
/// PHP outputs "p,q|n|1".
#[test]
fn test_allow_dynamic_properties_element_writes() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Loose {
    public int $declared = 1;
}

$loose = new Loose();
$loose->bag[] = 'p';
$loose->bag[] = 'q';
$loose->map['m'] = 'n';
echo implode(',', $loose->bag), '|', $loose->map['m'], '|', $loose->declared;
"#,
    );
    assert_eq!(out, "p,q|n|1");
}

/// Verifies prefix increment on a property returns the updated value while mutating the slot.
#[test]
fn test_prefix_increment_property_expression_returns_updated_value() {
    let out = compile_and_run(
        r#"<?php
class Counter { public int $value = 0; }
$counter = new Counter();
if (++$counter->value === 1) { echo 'yes'; }
echo ':' . $counter->value;
"#,
    );
    assert_eq!(out, "yes:1");
}

/// A prefix increment in an assignment RHS is not misclassified as a discarded postfix statement.
#[test]
fn test_prefix_increment_this_property_after_concat_returns_updated_value() {
    let out = compile_and_run(
        r#"<?php
class InlineIdCounter {
    public string $currentId = 'service';
    public int $counter = 0;
    public function nextId(): string {
        $id = '.autowire_inline.'.$this->currentId.'.'.++$this->counter;
        return $id;
    }
}
echo (new InlineIdCounter())->nextId();
"#,
    );
    assert_eq!(out, ".autowire_inline.service.1");
}

/// Verifies coercive typed-property assignment invokes `__toString()` and stores
/// the resulting string rather than the source object representation.
#[test]
fn test_string_property_assignment_accepts_stringable_object() {
    let out = compile_and_run(
        r#"<?php
class PropertyText {
    public function __toString(): string {
        return "converted";
    }
}

class PropertyTextHolder {
    public string $value = "";

    public function fill(): void {
        $this->value = new PropertyText();
    }
}

$holder = new PropertyTextHolder();
$holder->fill();
echo $holder->value;
"#,
    );
    assert_eq!(out, "converted");
}

/// Verifies an interface-typed source is checked against a narrower nominal
/// property at runtime, accepting matching objects and throwing for siblings.
#[test]
fn test_nominal_property_assignment_guards_interface_source() {
    let out = compile_and_run(
        r#"<?php
interface PropertyPass {}
class ExpectedPropertyPass implements PropertyPass {}
class OtherPropertyPass implements PropertyPass {}

class NominalPropertyHolder {
    private ExpectedPropertyPass $pass;

    public function set(PropertyPass $pass): void {
        $this->pass = $pass;
    }

    public function valid(): bool {
        return $this->pass instanceof ExpectedPropertyPass;
    }
}

$holder = new NominalPropertyHolder();
$holder->set(new ExpectedPropertyPass());
echo $holder->valid() ? "ok" : "bad";
try {
    $holder->set(new OtherPropertyPass());
} catch (TypeError $error) {
    echo ":type";
}
"#,
    );
    assert_eq!(out, "ok:type");
}

/// Verifies a nominal property guard retains the borrowed interface parameter it forwards.
///
/// The source variable is released before the property is read, so heap debug catches a
/// guard that labels its result owned without materializing the matching object reference.
#[test]
fn test_nominal_property_guard_retains_borrowed_object_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface RetainedPropertyService {}

final class RetainedPropertyServiceImplementation implements RetainedPropertyService {
    public function label(): string {
        return 'retained';
    }
}

final class RetainedPropertyHolder {
    private RetainedPropertyServiceImplementation $service;

    public function set(RetainedPropertyService $service): void {
        $this->service = $service;
    }

    public function label(): string {
        return $this->service->label();
    }
}

$service = new RetainedPropertyServiceImplementation();
$holder = new RetainedPropertyHolder();
$holder->set($service);
unset($service);
echo $holder->label();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "retained");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies null coalescing probes a statically undeclared property without a
/// compile error or runtime read, while still evaluating the receiver once.
#[test]
fn test_null_coalesce_accepts_missing_declared_property() {
    let out = compile_and_run(
        r#"<?php
class OptionalPropertyOwner {
    public static int $constructed = 0;

    public function __construct() {
        self::$constructed++;
    }
}

echo (new OptionalPropertyOwner())->missing ?? "fallback";
echo ":", OptionalPropertyOwner::$constructed;
"#,
    );
    assert_eq!(out, "fallback:1");
}

/// Verifies `??` and `??=` treat an uninitialized typed property as null before reading it.
#[test]
fn test_null_coalesce_initializes_uninitialized_typed_property() {
    let out = compile_and_run(
        r#"<?php
class LazyTypedProperty {
    private readonly stdClass $value;

    public function peek(): ?object {
        return $this->value ?? null;
    }

    public function resolve(): object {
        return $this->value ??= new stdClass();
    }
}

$box = new LazyTypedProperty();
var_dump($box->peek());
echo get_class($box->resolve()), ':', get_class($box->resolve());
"#,
    );
    assert_eq!(out, "NULL\nstdClass:stdClass");
}

/// Verifies an uninitialized named property keeps its concrete object representation through a
/// fluent `??=` result, rather than exposing the temporary boxed-null merge cell to the call.
#[test]
fn test_null_coalesce_initializes_named_property_for_fluent_call() {
    let out = compile_and_run(
        r#"<?php
interface LazyBuilderParent {}

final class LazyBuilder {
    private array $mapping;

    public function __construct() {
        $this->mapping = ['builder' => self::class];
    }

    public function attach(LazyBuilderParent $parent): self {
        return $this;
    }

    public function label(): string {
        return 'ready';
    }
}

final class LazyBuilderHolder implements LazyBuilderParent {
    private LazyBuilder $builder;
    private LazyBuilder $statementBuilder;
    private LazyBuilder $initializedStatementBuilder;

    public function __construct() {
        $this->initializedStatementBuilder = clone new LazyBuilder();
    }

    public function resolve(): string {
        return ($this->builder ??= new LazyBuilder())->attach($this)->label();
    }

    public function resolveStatement(): string {
        $this->statementBuilder ??= new LazyBuilder();

        return $this->statementBuilder->attach($this)->label();
    }

    public function resolveInitializedStatement(): string {
        $this->initializedStatementBuilder ??= new LazyBuilder();

        return $this->initializedStatementBuilder->attach($this)->label();
    }
}

$holder = new LazyBuilderHolder();
echo $holder->resolve(), ':', $holder->resolveStatement(), ':', $holder->resolveInitializedStatement();
"#,
    );
    assert_eq!(out, "ready:ready:ready");
}

/// Verifies null-coalescing element assignment autovivifies an uninitialized typed array property.
#[test]
fn test_null_coalesce_element_initializes_uninitialized_typed_array_property() {
    let out = compile_and_run(
        r#"<?php
class LazyTypedArrayProperty {
    private array $items;
    private array $direct;
    private array $list;
    private int $created = 0;

    private function create(): stdClass {
        ++$this->created;
        return new stdClass();
    }

    public function has(string $key): bool {
        return isset($this->items[$key]);
    }

    public function resolve(string $key): object {
        return $this->items[$key] ??= $this->create();
    }

    public function created(): int {
        return $this->created;
    }

    public function writeDirect(string $key, object $value): int {
        $this->direct[$key] = $value;
        $this->direct[] = $value;
        return count($this->direct);
    }

    public function append(object $value): int {
        $this->list[] = $value;
        return count($this->list);
    }
}

$box = new LazyTypedArrayProperty();
var_dump($box->has("service"));
$first = $box->resolve("service");
$second = $box->resolve("service");
echo get_class($first), ":", $first === $second ? "same" : "different", ":", $box->created(), ":",
     $box->writeDirect("named", new stdClass()), ":", $box->append(new stdClass());
"#,
    );
    assert_eq!(out, "bool(false)\nstdClass:same:1:2:1");
}

/// Verifies `??=` keeps working when a nullable object parameter uses the gradual receiver path.
#[test]
fn test_null_coalesce_assignment_on_nullable_object_receiver() {
    let out = compile_and_run(
        r#"<?php
class GradualCoalesceBox {
    public ?string $name = null;
}

function resolveGradualName(?GradualCoalesceBox $box): string {
    return $box->name ??= 'fallback';
}

$box = new GradualCoalesceBox();
echo resolveGradualName($box), ':', resolveGradualName($box);
"#,
    );
    assert_eq!(out, "fallback:fallback");
}

/// Verifies a gradual union crossing a nullable nominal property is checked at runtime.
#[test]
fn test_guarded_union_assignment_to_nullable_nominal_property() {
    let out = compile_and_run(
        r#"<?php
class NominalBase {}
class NominalChild extends NominalBase {}
class NominalHolder {
    public ?NominalChild $value = null;
    public function set(NominalBase|string $candidate): void {
        $isChild = $candidate instanceof NominalChild;
        $this->value = $isChild ? $candidate : null;
    }
}
$holder = new NominalHolder();
$holder->set(new NominalChild());
echo get_class($holder->value);
"#,
    );
    assert_eq!(out, "NominalChild");
}

/// Verifies `foreach ($this)` iterates ordinary object properties visible in class scope.
#[test]
fn test_foreach_plain_object_properties_inside_class() {
    let out = compile_and_run(
        r#"<?php
class PropertyIterationProbe {
    public int $publicValue = 1;
    protected int $protectedValue = 2;
    private int $privateValue = 3;

    public function dump(): void {
        foreach ($this as $name => $value) {
            echo $name, '=', $value, ';';
        }
    }
}
(new PropertyIterationProbe())->dump();
"#,
    );
    assert_eq!(out, "publicValue=1;protectedValue=2;privateValue=3;");
}

/// Compiles a loop over an array of class instances, reading the `price` field
/// of each `Item` object via `$items[$i]->price` and accumulating the sum.
#[test]
fn test_class_array_of_objects_property_access() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
}
$items = [];
$items[] = new Item("Apple", 1);
$items[] = new Item("Banana", 2);
$total = 0;
for ($i = 0; $i < count($items); $i++) {
    $total = $total + $items[$i]->price;
}
echo $total;
"#,
    );
    assert_eq!(out, "3");
}

/// Exercises `$this->items[] = $value` (push operator) on a class property
/// that holds an array, verifying the pushed element is retrievable at the
/// correct index.
#[test]
fn test_class_property_array_push() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2];
    }

    public function add($value) {
        $this->items[] = $value;
    }

    public function last(): int {
        return $this->items[2];
    }
}

$bucket = new Bucket();
$bucket->add(7);
echo $bucket->last();
"#,
    );
    assert_eq!(out, "7");
}

/// Exercises indexed write `$this->items[0] = $value` on a class property
/// that holds an array, verifying the replaced element is retrieved correctly.
#[test]
fn test_class_property_array_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2, 3];
    }

    public function replaceFirst($value) {
        $this->items[0] = $value;
    }

    public function first(): int {
        return $this->items[0];
    }
}

$bucket = new Bucket();
$bucket->replaceFirst(9);
echo $bucket->first();
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies assigning an untyped function parameter into a typed object property.
#[test]
fn test_typed_int_property_accepts_untyped_function_param_assignment() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
}

function set_n(Box $box, $value): void {
    $box->n = $value;
}

$box = new Box();
set_n($box, 7);
echo $box->n;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that a typed `public array $headers` property (initialized to `[]`)
/// accepts a string-keyed assignment (`$this->headers["Host"] = ...`) and the
/// value is retrievable via the same key.
#[test]
fn test_typed_array_property_accepts_string_key_assignment() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public array $headers;

    public function __construct() {
        $this->headers = [];
        $this->headers["Host"] = "example.com";
    }
}

$r = new Req();
echo $r->headers["Host"];
"#,
    );
    assert_eq!(out, "example.com");
}

/// Verifies dynamic property writes through a `mixed` receiver can assign
/// method-computed names and values into declared mixed object properties.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_from_method_values() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        return "Ada";
    }

    private function column(int $i): string {
        if ($i == 0) {
            return "id";
        }
        return "name";
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies dynamic property writes through a `mixed` receiver preserve mixed
/// string values built by repeated concatenation before assignment.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_with_concat_built_string() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        if ($i == 0) {
            return "id";
        }
        return "name";
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies dynamic property writes accept runtime-built property names and
/// runtime-built mixed string values when hydrating a declared object.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_with_runtime_name_and_value() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        $_name = "";
        if ($i == 0) {
            $_name = $_name . chr(105);
            $_name = $_name . chr(100);
            return $_name;
        }
        $_name = $_name . chr(110);
        $_name = $_name . chr(97);
        $_name = $_name . chr(109);
        $_name = $_name . chr(101);
        return $_name;
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies a prelude-style hydrator can instantiate from a mixed class-string
/// parameter and then assign runtime dynamic property names into the object.
#[test]
fn test_dynamic_property_set_after_mixed_dynamic_instantiation() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        $_name = "";
        if ($i == 0) {
            $_name = $_name . chr(105);
            $_name = $_name . chr(100);
            return $_name;
        }
        $_name = $_name . chr(110);
        $_name = $_name . chr(97);
        $_name = $_name . chr(109);
        $_name = $_name . chr(101);
        return $_name;
    }

    private function assign(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }

    public function fetch(mixed $classOrObject = null): mixed {
        return $this->assign(new $classOrObject());
    }
}

$row = (new Hydrator())->fetch(Row::class);
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies runtime-named reads and writes dispatch across concrete classes hidden by `object`.
#[test]
fn test_dynamic_property_read_write_through_generic_object_parameter() {
    let out = compile_and_run(
        r#"<?php
class FirstDynamicPropertyTarget {
    public int $value = 1;
}

class SecondDynamicPropertyTarget {
    public int $value = 2;
}

function replaceDynamicProperty(object $object, string $name, int $value): int {
    $object->{$name} = $value;
    return $object->{$name};
}

echo replaceDynamicProperty(new FirstDynamicPropertyTarget(), "value", 3);
echo ":";
echo replaceDynamicProperty(new SecondDynamicPropertyTarget(), "value", 4);
"#,
    );
    assert_eq!(out, "3:4");
}

/// Verifies that an untyped `public $headers = []` property (array default)
/// accepts a string-keyed assignment (`$r->headers["Host"] = ...`) and the
/// value is retrievable via the same key.
#[test]
fn test_empty_array_property_default_accepts_string_key_assignment() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public $headers = [];
}

$r = new Req();
$r->headers["Host"] = "example.com";
echo $r->headers["Host"];
"#,
    );
    assert_eq!(out, "example.com");
}

/// Verifies a string-key-refined property uses hash storage even before its first keyed write.
#[test]
fn test_empty_array_property_default_iterates_before_first_string_key_write() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    private array $items = [];

    public function set(string $key, mixed $value): void {
        $this->items[$key] = $value;
    }

    public function sizeByIteration(): int {
        $size = 0;
        foreach ($this->items as $value) {
            $size++;
        }
        return $size;
    }
}

class ChildRegistry extends Registry {}

$registry = new ChildRegistry();
echo $registry->sizeByIteration();
$registry->set('item', 1);
echo ':', $registry->sizeByIteration();
"#,
    );
    assert_eq!(out, "0:1");
}

/// Exercises `+=` and `*=` compound assignment on a `public $value` property,
/// verifying the result is `10 + 5 = 15`, then `15 * 3 = 45`.
#[test]
fn test_class_property_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

$counter = new Counter();
$counter->value += 5;
$counter->value *= 3;
echo $counter->value;
"#,
    );
    assert_eq!(out, "45");
}

/// Regression test: when the receiver of a compound property assignment is a
/// function call (`passthrough($counter)->value += 5`), the function must be
/// evaluated exactly once, not twice. Verifies output is `"r:15"` (not `"rr:15"`).
#[test]
fn test_class_property_compound_assign_evaluates_receiver_once() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

function passthrough($counter) {
    echo "r";
    return $counter;
}

$counter = new Counter();
passthrough($counter)->value += 5;
echo ":" . $counter->value;
"#,
    );
    assert_eq!(out, "r:15");
}

/// Exercises `+=` and `>>=` compound assignment on an indexed class property
/// (`$bucket->items[1] += 6` and `$bucket->items[2] >>= 1`), verifying the
/// results are `4 + 6 = 10` and `8 >> 1 = 4`.
#[test]
fn test_class_property_array_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

$bucket = new Bucket();
$bucket->items[1] += 6;
$bucket->items[2] >>= 1;
echo $bucket->items[1] . "|" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "10|4");
}

/// Regression test: when the receiver of an indexed compound property assignment
/// is a function call (`passthrough($bucket)->items[idx()] -= 3`), both the
/// receiver and the index expression must be evaluated exactly once each.
/// Verifies output is `"ri:5"` (not `"riri:5"` or similar).
#[test]
fn test_class_property_array_compound_assign_evaluates_receiver_and_index_once() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

function passthrough($bucket) {
    echo "r";
    return $bucket;
}

function idx() {
    echo "i";
    return 2;
}

$bucket = new Bucket();
passthrough($bucket)->items[idx()] -= 3;
echo ":" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "ri:5");
}

/// Verifies that `??=` on a `readonly` property that has already been initialized
/// does not invoke the fallback expression and preserves the existing value (`7`).
#[test]
fn test_readonly_property_null_coalesce_assignment_keeps_initialized_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public readonly int $value;

    public function __construct() {
        $this->value = 7;
    }
}

function fallback() {
    echo "fallback";
    return 9;
}

$box = new Box();
$box->value ??= fallback();
echo $box->value;
"#,
    );
    assert_eq!(out, "7");
}

/// Direct readonly overwrites evaluate owned operands and remain catchable with implicit set access.
#[test]
fn test_asymmetric_review_readonly_direct_write_is_catchable() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box {
    public readonly string $value;
    public function __construct() { $this->value = 'kept'; }
}
function receiver(): Box { echo 'receiver|'; return new Box(); }
function replacement(): string { echo 'rhs|'; return str_repeat('x', 9); }
try { receiver()->value = replacement(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "receiver|rhs|Cannot modify readonly property Box::$value");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Readonly coalescing keeps one receiver alive through its lazy fallback and caught Error.
#[test]
fn test_asymmetric_followup_readonly_coalesce_receiver_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box {
    public readonly int $value;
    public function __destruct() { echo 'D'; }
}
function receiver(): Box { echo 'R'; return new Box(); }
function fallback(): int { echo 'F'; return 9; }
try { receiver()->value ??= fallback(); echo 'bad'; }
catch (Error $error) { echo 'E'; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "RFDE");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The keep branch releases its captured receiver once without executing the fallback.
#[test]
fn test_asymmetric_followup_readonly_coalesce_keep_receiver() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box {
    public readonly int $value;
    public function __construct() { $this->value = 7; }
    public function __destruct() { echo 'D'; }
}
function receiver(): Box { echo 'R'; return new Box(); }
function fallback(): int { echo 'F'; return 9; }
receiver()->value ??= fallback();
echo 'K';
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "RDK");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An uninitialized readonly fallback runs before the conditional write raises Error.
#[test]
fn test_asymmetric_review_readonly_null_coalesce_fallback_still_throws() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public readonly int $value; }
function fallback() { echo "fallback:"; return 9; }
$box = new Box();
try {
    $box->value ??= fallback();
    echo "unexpected";
} catch (Error $e) {
    echo "error:", isset($box->value) ? "set" : "unset";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "fallback:error:unset", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullable readonly values also keep non-null values and throw only on a fallback write.
#[test]
fn test_asymmetric_review_nullable_readonly_coalesce_is_conditional() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box {
    public readonly ?int $value;
    public function __construct(?int $value) { $this->value = $value; }
}
function fallback(): int { echo "fallback:"; return 9; }
$none = new Box(null);
try { $none->value ??= fallback(); } catch (Error $e) { echo "error:"; }
echo $none->value === null ? "null|" : "bad|";
$set = new Box(7);
$set->value ??= fallback();
echo $set->value;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "fallback:error:null|7", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies `unset($obj->prop)` on a declared (typed) property.
///
/// PHP leaves the property UNINITIALIZED rather than nulled: `isset()` answers false,
/// `print_r` omits it, reading it raises `Error: Typed property … must not be accessed
/// before initialization`, and assigning again brings it back. `unset($a, $b)` clears
/// both targets.
#[test]
fn test_unset_declared_typed_property_leaves_it_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 3; public string $s = "x"; public array $a = [1, 2]; }
$t = new T();
unset($t->n, $t->s);
var_dump(isset($t->n), isset($t->s), isset($t->a));
print_r($t);
try { echo $t->n; } catch (\Error $e) { echo "ERR:", $e->getMessage(), "\n"; }
$t->n = 9;
var_dump(isset($t->n), $t->n);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(true)\n\
         T Object\n(\n    [a] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n)\n\
         ERR:Typed property T::$n must not be accessed before initialization\n\
         bool(true)\nint(9)\n"
    );
}

/// Verifies `unset()` on a property the caller cannot see still routes to `__unset`.
///
/// PHP calls `__unset` only for an INACCESSIBLE (or absent) property; a property the
/// caller can see is removed directly and `__unset` is never consulted.
#[test]
fn test_unset_inaccessible_property_calls_magic_unset() {
    let out = compile_and_run(
        r#"<?php
class Pv {
    private $secret = 1;
    public int $open = 2;
    public function __unset($k) { echo "magic:$k\n"; }
}
$p = new Pv();
unset($p->secret);
unset($p->open);
var_dump(isset($p->open));
"#,
    );
    assert_eq!(out, "magic:secret\nbool(false)\n");
}

/// Verifies `unset($std->prop)` on a `stdClass` really REMOVES the dynamic property.
///
/// Every `stdClass` property is a hash entry, so PHP's removal semantics are exact here:
/// `isset()` answers false, `json_encode()` stops listing the key, unsetting the same key
/// again and unsetting a key that was never set are both no-ops, a later write re-appends
/// the key at the END of the property order, `unset($o->b, $o->c)` removes both, and a read
/// of the removed name answers null (observed through `??`, so the fixture does not depend
/// on the undefined-property warning elephc does not yet emit for `stdClass`).
///
/// Expected output is `LC_ALL=C php 8.4.20` verbatim. The fixture deliberately avoids
/// `var_dump($o)`/`print_r($o)`: elephc renders a `stdClass` body as empty regardless of
/// `unset()`, a separate pre-existing gap.
#[test]
fn test_unset_stdclass_dynamic_property_removes_it() {
    let out = compile_and_run(
        r#"<?php
$o = new stdClass();
$o->a = 1;
$o->b = "two";
$o->c = 3;
unset($o->a);
var_dump(isset($o->a), isset($o->b));
echo json_encode($o), "\n";
unset($o->a);
unset($o->never);
echo json_encode($o), "\n";
$o->a = 9;
echo json_encode($o), "\n";
echo $o->a, "|", $o->b, "\n";
unset($o->b, $o->c);
echo json_encode($o), "\n";
var_dump($o->b ?? "gone");
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\n\
         {\"b\":\"two\",\"c\":3}\n\
         {\"b\":\"two\",\"c\":3}\n\
         {\"b\":\"two\",\"c\":3,\"a\":9}\n\
         9|two\n\
         {\"a\":9}\n\
         string(4) \"gone\"\n"
    );
}

/// Verifies `unset()` of an UNDECLARED name on an `#[AllowDynamicProperties]` class removes
/// the hash entry while leaving the class's fixed slots untouched.
///
/// The receiver mixes both storage shapes: `$fixed` is a declared typed slot and `$x`/`$y`
/// are dynamic hash entries. Unsetting the dynamic names must not disturb `$fixed`, and
/// repeat/absent unsets stay no-ops. Expected output is `LC_ALL=C php 8.4.20` verbatim.
#[test]
fn test_unset_dynamic_property_on_allow_dynamic_class() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Bag { public int $fixed = 7; }
$b = new Bag();
$b->x = 1;
$b->y = "two";
unset($b->x);
var_dump(isset($b->x), isset($b->y), isset($b->fixed));
unset($b->x);
unset($b->missing);
$b->x = 5;
var_dump(isset($b->x));
echo $b->x, "|", $b->y, "|", $b->fixed, "\n";
unset($b->x, $b->y);
var_dump(isset($b->x), isset($b->y), isset($b->fixed));
echo $b->fixed, "\n";
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nbool(true)\n\
         bool(true)\n\
         5|two|7\n\
         bool(false)\nbool(false)\nbool(true)\n\
         7\n"
    );
}

/// Regression: repeatedly reading the SAME dynamic property must keep answering its value.
///
/// `__rt_hash_get` only borrows the stored `Mixed` cell, but the dynamic-property read
/// hands its result to a caller that releases it, so a missing retain made every read drop
/// a reference the program never took. After enough reads the live hash entry was freed and
/// further reads answered `NULL` — a use-after-free of the property's storage.
/// Expected output is `LC_ALL=C php 8.4.20` verbatim.
#[test]
fn test_repeated_dynamic_property_reads_keep_the_value_alive() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Slot {}
$s = new Slot();
$s->v = "kept";
echo $s->v, $s->v, $s->v, "\n";
var_dump($s->v, $s->v);
var_dump($s->v);
"#,
    );
    assert_eq!(
        out,
        "keptkeptkept\nstring(4) \"kept\"\nstring(4) \"kept\"\nstring(4) \"kept\"\n"
    );
}

/// Verifies an untyped fixed slot carries observable removed state across rendering and cloning.
///
/// A normal read warns and answers null, reassignment clears the marker, and the untouched
/// property proves renderers skip only the removed slot.
#[test]
fn test_unset_untyped_declared_property_warns_null_and_can_be_reassigned() {
    let out = compile_and_run_capture(
        r#"<?php
class M { public $foo = 1; public $keep = 2; }
$m = new M();
unset($m->foo);
var_dump(isset($m->foo));
print_r($m);
var_dump($m->foo);
$copy = clone $m;
var_dump(isset($copy->foo));
$copy->foo = "again";
var_dump(isset($copy->foo), $copy->foo);
"#,
    );
    assert_eq!(
        out.stdout,
        "bool(false)\nM Object\n(\n    [keep] => 2\n)\nNULL\n\
         bool(false)\nbool(true)\nstring(5) \"again\"\n"
    );
    assert_eq!(out.stderr.matches("Undefined property: M::$foo").count(), 1);
}

/// Verifies a missing untyped boxed slot can converge into a nullable scalar result safely.
///
/// This catches stale tag registers when the absent branch materializes null while the present
/// branch would otherwise materialize a payload and a separate scalar tag.
#[test]
fn test_unset_untyped_nullable_slot_materializes_tagged_null() {
    let out = compile_and_run_capture(
        r#"<?php
class Pair { public $value = null; }
$pair = new Pair();
$pair->value = 7;
$name = "value";
unset($pair->{$name});
var_dump(isset($pair->{$name}));
var_dump($pair->{$name});
$pair->{$name} = null;
var_dump(isset($pair->{$name}), $pair->{$name});
"#,
    );
    assert_eq!(
        out.stdout,
        "bool(false)\nNULL\nbool(false)\nNULL\n"
    );
    assert_eq!(
        out.stderr.matches("Undefined property: Pair::$value").count(),
        1
    );
}

/// Verifies fixed-slot removal is visible before the displaced value's destructor runs.
///
/// The destructor reads the absent property, recreates it, or recursively removes it. The outer
/// unset must not release the old owner twice or overwrite a value installed during reentry.
#[test]
fn test_unset_untyped_fixed_slot_commits_before_reentrant_destructor() {
    let out = compile_and_run_capture(
        r#"<?php
class ReentrantFixedOwner { public $value = null; }
class ReentrantFixedValue {
    public static ReentrantFixedOwner $owner;
    public static int $mode = 0;
    public function __destruct() {
        $owner = self::$owner;
        echo isset($owner->value) ? "present:" : "absent:";
        if (self::$mode === 1) {
            var_dump($owner->value);
        } elseif (self::$mode === 2) {
            $owner->value = 17;
        } else {
            unset($owner->value);
            echo isset($owner->value) ? "present" : "absent";
        }
    }
}
$owner = new ReentrantFixedOwner();
ReentrantFixedValue::$owner = $owner;
ReentrantFixedValue::$mode = 1;
$owner->value = new ReentrantFixedValue();
echo "r:";
unset($owner->value);
echo "|w:";
ReentrantFixedValue::$mode = 2;
$owner->value = new ReentrantFixedValue();
unset($owner->value);
echo $owner->value, "|u:";
ReentrantFixedValue::$mode = 3;
$owner->value = new ReentrantFixedValue();
unset($owner->value);
echo "|";
"#,
    );
    assert_eq!(
        out.stdout,
        "r:absent:NULL\n|w:absent:17|u:absent:absent|"
    );
    assert_eq!(
        out.stderr
            .matches("Undefined property: ReentrantFixedOwner::$value")
            .count(),
        1,
        "{}",
        out.stderr
    );
}

/// Verifies `unset()` of a BY-REFERENCE property is refused rather than silently skipped.
///
/// The slot holds an object-owned ref-cell pointer that the destructor still frees and that
/// a later write would write THROUGH, reviving the alias PHP's `unset()` just broke. The
/// backend used to skip the shape quietly, which left `isset()` answering `true` after an
/// `unset()` where PHP answers `false`.
#[test]
fn test_unset_by_reference_property_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
class R { public function __construct(public int &$p) {} }
$v = 3;
$r = new R($v);
unset($r->p);
"#,
    );
    assert!(
        error.contains("unset() of by-reference property R::$p"),
        "the diagnostic must name the by-reference property, got: {}",
        error
    );
}

/// Verifies `unset()` never stamps the removed marker into packed native field storage.
#[test]
fn test_unset_packed_property_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
packed class Cell { public int $value; }
buffer<Cell> $cells = buffer_new<Cell>(1);
unset($cells[0]->value);
"#,
    );
    assert!(
        error.contains("unset() of packed class field Cell::$value")
            || error.contains("unset target shape"),
        "the diagnostic must preserve the packed-field refusal, got: {}",
        error
    );
}

/// Verifies `unset()` of a dynamic name on a class that declares `__unset()` is refused.
///
/// PHP consults `__unset()` only when the dynamic property is ABSENT at the unset site and
/// removes the entry silently when it is present — a choice that depends on runtime state.
/// elephc picks the lowering statically, so it declines rather than guessing one of the two
/// behaviors.
#[test]
fn test_unset_dynamic_property_with_magic_unset_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
#[AllowDynamicProperties]
class Hooked { public function __unset($n) { echo "magic:$n\n"; } }
$h = new Hooked();
$h->a = 1;
unset($h->a);
"#,
    );
    assert!(
        error.contains("unset target shape"),
        "the runtime-dependent __unset shape must be refused, got: {}",
        error
    );
}

/// Verifies `isset()` on a never-initialized typed property answers false instead of
/// raising the uninitialized-read error, matching PHP.
#[test]
fn test_isset_on_uninitialized_typed_property_is_false() {
    let out = compile_and_run(
        r#"<?php
class U { public ?int $v; public int $w = 1; }
$u = new U();
var_dump(isset($u->v), isset($u->w));
$u->v = 5;
var_dump(isset($u->v), $u->v);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\nint(5)\n");
}

/// Verifies an inherited method sees its private typed slot as uninitialized on a child object.
#[test]
fn test_isset_on_inherited_private_uninitialized_typed_property_is_false() {
    let out = compile_and_run(
        r#"<?php
class PrivateTypedBase {
    private string $path;
    public function path(): string {
        if (!isset($this->path)) {
            $this->path = "initialized";
        }
        return $this->path;
    }
}
class PrivateTypedMiddle extends PrivateTypedBase {
    public string $middle = "middle";
}
final class PrivateTypedChild extends PrivateTypedMiddle {
    public string $child = "child";
}
echo (new PrivateTypedChild())->path();
"#,
    );
    assert_eq!(out, "initialized");
}

/// Verifies a bare `object` receiver selects the runtime class when property names collide.
#[test]
fn test_generic_object_property_write_dispatches_by_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class TextPropertyOwner {
    public string $value = '';
}
class ObjectPropertyOwner {
    public object $value;
}
function assignObjectProperty(object $owner, object $value): void {
    $owner->value = $value;
}
$owner = new ObjectPropertyOwner();
assignObjectProperty($owner, new stdClass());
echo $owner->value instanceof stdClass ? 'ok' : 'bad';
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies an array write through a bare `object` receiver dispatches an anonymous class property
/// before applying ArrayAccess offset assignment semantics.
#[test]
fn test_generic_object_anonymous_property_supports_array_writes() {
    let out = compile_and_run(
        r#"<?php
class AnonymousArrayPropertyFactory {
    public function fill(object $result, ArrayObject $storage): void {
        $result->values = $storage;
        $result->values[0] = 'first';
    }
}

$storage = new ArrayObject();
$value = new class {
    public $values;
};
(new AnonymousArrayPropertyFactory())->fill($value, $storage);
echo $storage[0];
"#,
    );
    assert_eq!(out, "first");
}

/// Verifies a nullable attribute instance keeps its runtime class when a same-named property on
/// another class would otherwise make the static slot guess incompatible with the assigned value.
#[test]
fn test_nullable_attribute_property_write_falls_back_to_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class RequestContextCollision {
    public string $method = '';
}

#[Attribute(Attribute::TARGET_METHOD)]
class MethodMarker {
    private ReflectionMethod $method;

    public static function from(ReflectionMethod $method): ?self {
        /** @var self|null $self */
        if (!$self = ($method->getAttributes(self::class)[0] ?? null)?->newInstance()) {
            return null;
        }
        $self->method = $method;
        return $self;
    }

    public function name(): string {
        return $this->method->getName();
    }
}

class MarkedHandler {
    #[MethodMarker]
    public function run(): void {}
}

$marker = MethodMarker::from(new ReflectionMethod(MarkedHandler::class, 'run'));
echo $marker?->name();
"#,
    );
    assert_eq!(out, "run");
}

/// Verifies append on a nullable array property mutates its boxed property cell in place and
/// autovivifies the initial null value before retaining subsequent elements.
#[test]
fn test_nullable_array_property_push_autovivifies_mixed_cell() {
    let out = compile_and_run(
        r#"<?php
class NullableRows {
    private ?array $rows = null;

    public function push(string $value): void {
        $this->rows[] = $value;
    }

    public function joined(): string {
        return implode(',', $this->rows);
    }
}

$rows = new NullableRows();
$rows->push('first');
$rows->push('second');
echo $rows->joined();
"#,
    );
    assert_eq!(out, "first,second");
}

/// Verifies that assigning `[]` to an associative instance property preserves hash storage.
#[test]
fn test_empty_array_resets_associative_instance_property() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public array $items = ['seed' => 1];

    public function reset(): void {
        $this->items = [];
    }
}
$registry = new Registry();
$registry->reset();
$registry->items['next'] = 2;
echo count($registry->items), ':', $registry->items['next'];
"#,
    );
    assert_eq!(out, "1:2");
}

/// Verifies resetting a private array property clears prior child-resolution state between calls.
#[test]
fn test_private_array_property_reset_clears_prior_search_path() {
    let out = compile_and_run(
        r#"<?php
class ChildPathResolver {
    private array $currentPath;

    public function resolve(string $parent): string {
        $this->currentPath = [];
        $searchKey = array_search($parent, $this->currentPath);
        $this->currentPath[] = $parent;

        return false !== $searchKey ? 'circular' : 'ok';
    }
}

echo eval('$resolver = new ChildPathResolver(); return $resolver->resolve("cache.system") . $resolver->resolve("cache.adapter.system");');
"#,
    );
    assert_eq!(out, "okok");
}

/// Verifies nested associative writes preserve dynamic keys and sibling adapter values.
#[test]
fn test_nested_associative_pool_configuration_preserves_each_adapter() {
    let out = compile_and_run(
        r#"<?php
$config = ['app' => 'cache.adapter.filesystem', 'system' => 'cache.adapter.system', 'pools' => []];
foreach (['app', 'system'] as $name) {
    $config['pools']['cache.'.$name] = ['adapters' => [$config[$name]], 'public' => true];
}
echo $config['pools']['cache.app']['adapters'][0], ':';
echo $config['pools']['cache.system']['adapters'][0], ':';
echo count($config['pools']);
"#,
    );
    assert_eq!(out, "cache.adapter.filesystem:cache.adapter.system:2");
}

/// Verifies an inherited-definition path does not confuse distinct service ids with a common prefix.
#[test]
fn test_array_search_distinguishes_child_definition_parent_service_ids() {
    let out = compile_and_run(
        r#"<?php
$currentPath = [];
$currentPath[] = 'cache.system';
$searchKey = array_search('cache.adapter.system', $currentPath);
$currentPath[] = 'cache.adapter.system';
echo false !== $searchKey ? 'circular' : implode(' -> ', $currentPath);
"#,
    );
    assert_eq!(out, "cache.system -> cache.adapter.system");
}

/// Verifies a private string property returned by a getter retains exact array-search semantics.
#[test]
fn test_private_string_getter_preserves_child_definition_parent_search() {
    let out = compile_and_run(
        r#"<?php
class ChildDefinitionLike {
    public function __construct(private string $parent) {}

    public function getParent(): string {
        return $this->parent;
    }
}

$currentPath = ['cache.system'];
$parent = (new ChildDefinitionLike('cache.adapter.system'))->getParent();
$searchKey = array_search($parent, $currentPath);
echo false !== $searchKey ? 'circular' : $parent;
"#,
    );
    assert_eq!(out, "cache.adapter.system");
}

/// Verifies an eval-created child keeps its private string slot after a broad inherited layout.
#[test]
fn test_eval_child_private_string_after_broad_inherited_layout() {
    let out = compile_and_run(
        r#"<?php
class DefinitionLayoutLike {
    private ?string $class = null;
    private ?string $file = null;
    private string|array|null $factory = null;
    private bool $shared = true;
    private array $deprecation = [];
    private array $properties = [];
    private array $calls = [];
    private array $instanceof = [];
    private bool $autoconfigured = false;
    private string|array|null $configurator = null;
    private array $tags = [];
    private bool $public = false;
    private bool $synthetic = false;
    private bool $abstract = false;
    private bool $lazy = false;
    private ?array $decoratedService = null;
    private bool $autowired = false;
    private array $changes = [];
    private array $bindings = [];
    private array $errors = [];
    protected array $arguments = [];
    public ?string $innerServiceId = null;
    public ?int $decorationOnInvalid = null;
    public ?int $decorationPriority = null;
}
class ChildDefinitionLayoutLike extends DefinitionLayoutLike {
    public function __construct(private string $parent) {}
    public function getParent(): string { return $this->parent; }
}
echo eval('$definition = new ChildDefinitionLayoutLike("cache.adapter.system"); return array_search($definition->getParent(), ["cache.system"]) === false ? $definition->getParent() : "circular";');
"#,
    );
    assert_eq!(out, "cache.adapter.system");
}

/// Verifies nested pool iteration forwards each adapter to its child-definition constructor.
#[test]
fn test_nested_pool_iteration_preserves_child_definition_parent() {
    let out = compile_and_run(
        r#"<?php
class ChildDefinitionLike {
    public function __construct(private string $parent) {}
    public function getParent(): string { return $this->parent; }
}
$config = [
    'app' => 'cache.adapter.filesystem',
    'system' => 'cache.adapter.system',
    'pools' => [],
];
foreach (['app', 'system'] as $name) {
    $config['pools']['cache.'.$name] = ['adapters' => [$config[$name]], 'public' => true];
}
$definitions = [];
foreach ($config['pools'] as $name => $pool) {
    foreach ($pool['adapters'] as $provider => $adapter) {
    }
    $definitions[$name] = new ChildDefinitionLike($adapter);
}
echo $definitions['cache.app']->getParent(), ':';
echo $definitions['cache.system']->getParent();
"#,
    );
    assert_eq!(out, "cache.adapter.filesystem:cache.adapter.system");
}

/// Verifies a populated service-definition hash distinguishes prefix-sharing service ids.
#[test]
fn test_large_service_definition_hash_distinguishes_prefixed_ids() {
    let out = compile_and_run(
        r#"<?php
$definitions = [];
for ($i = 0; $i < 128; ++$i) {
    $definitions['service.'.$i] = $i;
}
$definitions['cache.adapter.system'] = 'parent';
$definitions['cache.system'] = 'child';
echo $definitions['cache.system'], ':';
echo $definitions['cache.adapter.system'], ':';
echo count($definitions);
"#,
    );
    assert_eq!(out, "child:parent:130");
}

/// Verifies that indexed values assigned to an associative property are promoted to hash storage.
#[test]
fn test_indexed_array_replaces_associative_instance_property() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public array $items = ['seed' => 1];

    public function replace(): void {
        $this->items = [2, 3];
    }
}
$registry = new Registry();
$registry->replace();
echo count($registry->items), ':', $registry->items[0], ':', $registry->items[1];
"#,
    );
    assert_eq!(out, "2:2:3");
}

/// Verifies an array property refined by string-key writes can later receive indexed storage;
/// numeric keys must survive the representation change alongside subsequent string keys.
#[test]
fn test_indexed_array_replaces_string_key_refined_property() {
    let out = compile_and_run(
        r#"<?php
class RefinedRegistry {
    private array $items = [];

    public function set(string $key, mixed $value): void {
        $this->items[$key] = $value;
    }

    public function replace(array $items): void {
        $this->items = $items;
    }

    public function values(): array {
        return $this->items;
    }
}
$registry = new RefinedRegistry();
$registry->set('seed', 1);
$registry->replace([2, 3]);
$registry->set('tail', 4);
$values = $registry->values();
echo count($values), ':', $values[0], ':', $values[1], ':', $values['tail'];
"#,
    );
    assert_eq!(out, "3:2:3:4");
}

/// Verifies runtime `mixed` property names use PHP string coercion for both declared-property
/// writes and reads instead of reaching the backend as an unmaterializable boxed name.
#[test]
fn test_dynamic_property_access_coerces_mixed_string_name() {
    let out = compile_and_run(
        r#"<?php
class DynamicNameRow {
    public mixed $value = null;

    public function write(mixed $name, mixed $value): void {
        $this->{$name} = $value;
    }

    public function read(mixed $name): mixed {
        return $this->{$name};
    }
}

$row = new DynamicNameRow();
$row->write('value', 'ok');
echo $row->read('value');
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies runtime-named unset dispatch for generic objects, Mixed receivers, and stdClass.
#[test]
fn test_dynamic_property_unset_dispatches_and_marks_declared_slots_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class DynamicUnsetBox {
    public string $value = 'live';
}

function clearObject(object $object, string $name): void {
    unset($object->$name);
}

function clearMixed(mixed $object, string $name): void {
    unset($object->$name);
}

$first = new DynamicUnsetBox();
clearObject($first, 'value');
echo isset($first->value) ? '1' : '0';

$second = new DynamicUnsetBox();
clearMixed($second, 'value');
echo isset($second->value) ? '1' : '0';

$dynamic = new stdClass();
$dynamic->value = 'live';
clearObject($dynamic, 'value');
echo isset($dynamic->value) ? '1' : '0';
"#,
    );
    assert_eq!(out, "000");
}

/// Verifies a Mixed source is runtime-checked and widened before entering a declared array slot.
#[test]
fn test_mixed_array_values_replace_declared_instance_properties() {
    let out = compile_and_run(
        r#"<?php
class RuntimeArrayHolder {
    public array $generic = [];
    public array $associative = ['seed' => 0];

    public function replaceGeneric(mixed $value): void {
        $this->generic = $value;
    }

    public function replaceAssociative(mixed $value): void {
        $this->associative = $value;
    }
}

$holder = new RuntimeArrayHolder();
$holder->replaceGeneric([1, 'two']);
$holder->replaceAssociative(['name' => 'value', 'count' => 2]);
echo $holder->generic[0], ':', $holder->generic[1], '|';
echo $holder->associative['name'], ':', $holder->associative['count'];
try {
    $holder->replaceGeneric('not an array');
} catch (TypeError $error) {
    echo '|type';
}
"#,
    );
    assert_eq!(out, "1:two|value:2|type");
}

/// Verifies lossless numeric widening and array-to-iterable assignment for typed properties.
#[test]
fn test_declared_property_accepts_int_float_widening_and_array_iterable() {
    let out = compile_and_run(
        r#"<?php
class WidenedPropertyHolder {
    public float $ratio = 0.0;
    public iterable $values = [];

    public function fill(int $ratio, array $values): void {
        $this->ratio = $ratio;
        $this->values = $values;
    }
}

$holder = new WidenedPropertyHolder();
$holder->fill(7, ['left' => 2, 'right' => 3]);
echo $holder->ratio, ':', count($holder->values);
foreach ($holder->values as $key => $value) {
    if ($key === 'right') {
        echo ':', $value;
    }
}
"#,
    );
    assert_eq!(out, "7:2:3");
}

/// Pins the `??=` container targets that reach the BORROWED-write scope, and the one that does
/// not.
///
/// `??=` writes its result temporary into the container and also hands it to the expression's
/// consumer, so the write only borrows it. A reviewer argued the borrow was unsound for a
/// target whose base is a temporary, and could not be settled by reading the code: the question
/// is which targets reach that scope at all. These three do — a fresh temporary base, a base
/// aliasing a live object, and a right-hand side that is itself an owned string — and each
/// matches `php -n`.
///
/// The refusal below is the other half. `mkArr()[2] ??= 5` is accepted by PHP and refused here,
/// so the accept set is bounded by a compile error rather than by an ownership argument. It is
/// pinned so that widening the set has to come through this test instead of silently opening
/// the borrowed-write scope to a shape nobody measured.
#[test]
fn test_coalesce_assign_borrowed_write_targets() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $a = [1, 2];
}
$shared = new Bag();
function mkShared(): Bag {
    global $shared;
    return $shared;
}
function mk(): Bag {
    return new Bag();
}
mkShared()->a[2] ??= 5;
mkShared()->a[3] ??= "o" . "wned";
mkShared()->a[1] ??= 9;
mk()->a[2] ??= 5;
echo implode(",", $shared->a);
"#,
    );
    assert_eq!(out, "1,2,5,owned");
}

/// The bound on that accept set, lifted: an element write through a call result writes a
/// temporary copy and is otherwise a no-op, which `php -n` 8.5.6 accepts and prints nothing but
/// `unreachable` for. It used to be refused by the EIR backend ("array set with receiver PHP
/// type"); the merged Laravel work places the receiver, so the program now runs as php does.
#[test]
fn test_coalesce_assign_on_a_returned_array_runs_as_php_does() {
    let out = compile_and_run(
        r#"<?php
function mkArr(): array {
    return [1, 2];
}
mkArr()[2] ??= 5;
echo "unreachable";
"#,
    );
    assert_eq!(out, "unreachable");
}

/// An indexed write auto-initializes an uninitialized typed property, nullable and nested too.
///
/// PHP auto-initializes the CONTAINER of an indexed write rather than raising, and `php -n`
/// 8.5.6 treats `array`, `?array` and an untyped property alike. The nullable case represents
/// as `Mixed` (every union does), and the nested case reaches the property through a different
/// lowering than the one-level case, so all three are asserted together: keying the
/// auto-initialization on the representation missed the first, and the nested parent chain
/// missed the second.
#[test]
fn test_indexed_write_auto_initializes_uninitialized_typed_property() {
    let out = compile_and_run(
        r#"<?php
class Bag { public array $a; public ?array $na; public $u; }
$one = new Bag(); $one->a['k'] = 1;      echo count($one->a), ';';
$two = new Bag(); $two->a[] = 2;         echo count($two->a), ';';
$three = new Bag(); $three->na['k'] = 3; echo count($three->na), ';';
$four = new Bag(); $four->u['k'] = 4;    echo count($four->u), ';';
$five = new Bag(); $five->a['k']['j'] = 5; echo $five->a['k']['j'], ';';
$six = new Bag(); $six->a['k'] = 6;      echo isset($six->a) ? 'set' : 'unset';
"#,
    );
    assert_eq!(out, "1;1;1;1;5;set");
}

/// Verifies a runtime property name on a receiver of unknown class reaches the right class's slot.
///
/// A read, write or unset through `$object->$name` on a `mixed` or `object` receiver dispatches
/// over every declared property of the program: the receiver's class is found by a binary search
/// over class ids, then the name is compared within that class. Ten classes, inherited and
/// redeclared properties, a property name several classes share, a name no class declares, and a
/// non-object element cover each side of that search; `Req` writes through a cached
/// `ReflectionProperty::setRawValue()`, which is how Symfony's `Request` sets its properties.
#[test]
fn test_runtime_property_name_dispatch_finds_the_receiver_class() {
    let out = compile_and_run(
        r#"<?php
class A0 { public $a = 'a0'; public $shared = 's0'; }
class A1 { public $b = 'b1'; public $shared = 's1'; public $z = 'z1'; }
class A2 extends A1 { public $c = 'c2'; }
class A3 { public int $n = 3; public $shared = 's3'; }
class A4 { public $d = 'd4'; }
class A5 extends A4 { public $e = 'e5'; public $shared = 's5'; }
class A6 { public $f = 'f6'; }
class A7 { public $g = 'g7'; public $shared = 's7'; }
class A8 { public $h = 'h8'; }
class A9 extends A8 { public $i = 'i9'; public $shared = 's9'; }
final class Req {
    public $query = null;
    public $request = null;
    public $attributes = null;

    public static function set(self $r, string $name, mixed $value): void
    {
        static $cache;
        $p = $cache[$name] ??= new \ReflectionProperty(self::class, $name);
        $p->setRawValue($r, $value);
    }
}
function objects(): array {
    return [new A0(), new A1(), new A2(), new A3(), new A4(), new A5(), new A6(), new A7(), new A8(), new A9(), 42];
}
function drop(object $o, string $n): void { unset($o->$n); }

$names = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'n', 'z', 'shared', 'missing'];
foreach (objects() as $index => $object) {
    $row = [];
    foreach ($names as $name) {
        if (is_object($object) && property_exists($object, $name)) {
            $row[] = $name . '=' . $object->$name;
        }
    }
    echo $index, ':', implode(',', $row), "\n";
}
foreach (objects() as $index => $object) {
    if (!is_object($object)) {
        continue;
    }
    $row = [];
    foreach ($names as $name) {
        if (property_exists($object, $name)) {
            $object->$name = $name === 'n' ? 30 + $index : $name . $index;
            $row[] = $object->$name;
        }
    }
    echo implode(',', $row), "\n";
}
$u = new A9();
drop($u, $argc > 5 ? 'h' : 'i');
echo isset($u->i) ? 'set' : 'unset', ' ', $u->h, "\n";
$r = new Req();
foreach (['query' => 'q', 'request' => 'r', 'attributes' => 'a'] as $k => $v) {
    for ($i = 0; $i < 2; $i++) {
        Req::set($r, $k, $v . $i);
    }
}
echo $r->query, $r->request, $r->attributes, "\n";
"#,
    );
    assert_eq!(out, "0:a=a0,shared=s0\n1:b=b1,z=z1,shared=s1\n2:b=b1,c=c2,z=z1,shared=s1\n3:n=3,shared=s3\n4:d=d4\n5:d=d4,e=e5,shared=s5\n6:f=f6\n7:g=g7,shared=s7\n8:h=h8\n9:h=h8,i=i9,shared=s9\n10:\na0,shared0\nb1,z1,shared1\nb2,c2,z2,shared2\n33,shared3\nd4\nd5,e5,shared5\nf6\ng7,shared7\nh8\nh9,i9,shared9\nunset h8\nq1r1a1\n");
}

/// A declared property written through a `mixed`-typed receiver actually reaches the object.
///
/// The Mixed receiver path used to hand EVERY static-name write to the stdClass helper, which
/// understands stdClass alone and silently dropped the rest. The write was not diagnosed, not
/// applied, and not visible: the reads below returned the class defaults. Compiled PHP now
/// dispatches on the receiver's runtime class id and stores into the declared slot, so this
/// prints what reference PHP prints.
///
/// The fixture covers both slot representations that can be reached this way (an `int` slot and
/// a `string` slot, which owns its payload), a second class declaring the SAME property name so
/// the dispatch has to discriminate on the class id rather than on the name alone, and an
/// unrelated stdClass receiver so the dynamic-property fallback is still exercised.
#[test]
fn test_declared_property_write_through_a_mixed_receiver_reaches_the_object() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $k = 1;
    public string $s = "a";
}
class Crate {
    public int $k = 100;
}
function pick(int $which): mixed {
    if ($which === 0) { return new Box(); }
    if ($which === 1) { return new Crate(); }
    return new stdClass();
}
$b = pick(0);
$b->k = 4;
$b->s = "z";
$c = pick(1);
$c->k = 7;
$d = pick(2);
$d->k = 9;
echo $b->k . "|" . $b->s . "|" . $c->k . "|" . $d->k;
"#,
    );
    assert_eq!(out, "4|z|7|9");
}

/// Writes through a mixed receiver must still enforce private and protected visibility.
#[test]
fn test_mixed_receiver_write_to_inaccessible_declared_properties_throws() {
    let out = compile_and_run(
        r#"<?php
class HiddenSlots {
    private int $secret = 1;
    protected int $guard = 2;
    public function readSecret(): int { return $this->secret; }
    public function readGuard(): int { return $this->guard; }
}
function writeSecret(mixed $object): void { $object->secret = 9; }
function writeGuard(mixed $object): void { $object->guard = 8; }
$object = new HiddenSlots();
try { writeSecret($object); echo "no throw;"; }
catch (Error $error) { echo $error->getMessage() . ";"; }
echo $object->readSecret() . ";";
try { writeGuard($object); echo "no throw;"; }
catch (Error $error) { echo $error->getMessage() . ";"; }
echo $object->readGuard();
"#,
    );
    assert_eq!(
        out,
        "Cannot access private property HiddenSlots::$secret;1;Cannot access protected property HiddenSlots::$guard;2"
    );
}

/// Verifies an ordinary runtime-name property access whose name collides with a STRICT ancestor's
/// private property addresses a DYNAMIC property, while the declaring scope keeps its own slot.
///
/// php 7.4 removed shadow properties, so `P`'s `private int $n` is not in `C`'s by-name table:
/// measured against php 8.5.10, this fixture prints `false;1/5/1;true;1/6;3` and deprecates the
/// creation of `C::$zz` and `C::$n` exactly once each. Both values exist side by side afterwards,
/// the outside runtime-name read answers 5 from the hash while `P::readN()` and the PARENT-scope
/// runtime-name read `$this->{$key}` both still answer 1 from the private slot.
///
/// The backend's runtime-name ladder used to be built from the physical slot table with no scope
/// input, so `isset()` reported the ancestor's slot, the read returned its value and the write
/// overwrote it. The `#[AllowDynamicProperties]` child and the stdClass receiver are the controls
/// for the two storage shapes that were already name-addressable.
///
/// The parent-scope probe ECHOES instead of returning on purpose. An inherited method with an
/// undeclared return type whose inferred type is `mixed` is double-boxed at a SUBCLASS call site,
/// which prints a raw pointer; that defect is unrelated to property identity (an assoc-array
/// element read reproduces it), and a declared return type here would hide it rather than leave
/// it visible for its own fix.
#[test]
fn test_runtime_name_write_colliding_with_an_ancestor_private_creates_a_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
class P {
    private int $n = 1;
    public function readN(): int { return $this->n; }
    public function showDyn(string $key): void { echo $this->{$key}; }
}
class C extends P {}
#[AllowDynamicProperties]
class Open extends P {}
$c = new C();
$key = "n";
clone($c, ["zz" => 0]);
echo var_export(isset($c->{$key}), true) . ";";
$c->{$key} = 5;
echo $c->readN() . "/" . $c->{$key} . "/";
$c->showDyn("n");
echo ";" . var_export(isset($c->{$key}), true) . ";";
$open = new Open();
$open->{$key} = 6;
echo $open->readN() . "/" . $open->{$key} . ";";
$plain = new stdClass();
$plain->{$key} = 3;
echo $plain->{$key};
"#,
    );
    assert_eq!(out.stdout, "false;1/5/1;true;1/6;3");
    assert_eq!(
        out.stderr
            .matches("Creation of dynamic property C::$n is deprecated")
            .count(),
        1,
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("Open::$n") && !out.stderr.contains("stdClass::$n"),
        "an opted-in class and stdClass must not deprecate: {}",
        out.stderr
    );
}

/// Verifies a runtime-name write php REFUSES raises the catchable `Error` instead of storing.
///
/// A strict ancestor's private name is invisible and becomes a dynamic property, but a private
/// property declared by the receiver's OWN class, and a protected one reached from an unrelated
/// scope, are different answers: php raises `Error` and leaves the storage alone. The runtime-name
/// ladder used to carry no visibility check at all, so both wrote the physical slot, which is a
/// private-storage escape that a literal name is refused at compile time for.
///
/// Both messages are php 8.5.10's verbatim wording, with no scope suffix, and both readers below
/// prove the slot still holds its default.
#[test]
fn test_runtime_name_write_to_an_inaccessible_property_raises_the_php_error() {
    let out = compile_and_run(
        r#"<?php
class D { private int $n = 1; public function readN(): int { return $this->n; } }
class Prot { protected int $p = 1; public function readP(): int { return $this->p; } }
class Outsider { public function poke(Prot $o, string $key): void { $o->{$key} = 9; } }
$d = new D();
$key = "n";
try { $d->{$key} = 5; echo "no throw;"; } catch (Error $e) { echo $e->getMessage() . ";"; }
echo $d->readN() . ";";
$p = new Prot();
try { (new Outsider())->poke($p, "p"); echo "no throw;"; } catch (Error $e) { echo $e->getMessage() . ";"; }
echo $p->readP();
"#,
    );
    assert_eq!(
        out,
        "Cannot access private property D::$n;1;Cannot access protected property Prot::$p;1"
    );
}

/// Verifies a runtime-name write selects the private slot the INVOCATION SCOPE owns, not the
/// receiver's shadowing one.
///
/// `Base` and `Child` both declare `private int $p`, so the name addresses two different pieces of
/// storage. The receiver's own by-name table resolves `p` to the CHILD's slot, so a ladder built
/// from it wrote the wrong property whenever the scope was `Base`. php selects the scope's slot:
/// measured against php 8.5.10, this prints `10:2;10:20`.
#[test]
fn test_runtime_name_write_selects_the_scope_private_slot_of_a_shadowed_property() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private int $p = 1;
    public function readP(): int { return $this->p; }
    public function pokeChild(Child $o, string $key): void { $o->{$key} = 10; }
}
class Child extends Base {
    private int $p = 2;
    public function readC(): int { return $this->p; }
    public function pokeSelf(Child $o, string $key): void { $o->{$key} = 20; }
}
$c = new Child();
(new Base())->pokeChild($c, "p");
echo $c->readP() . ":" . $c->readC() . ";";
(new Child())->pokeSelf($c, "p");
echo $c->readP() . ":" . $c->readC();
"#,
    );
    assert_eq!(out, "10:2;10:20");
}

/// A property name computed for one access (`$o->{(string) $mixed}`) is a
/// temporary string the access only reads. Neither the write nor the read
/// released it, so each access leaked one block.
#[test]
fn test_computed_dynamic_property_name_is_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function name_of(int $i): mixed {
    return $i % 2 === 0 ? "alpha" : "beta";
}
$total = 0;
for ($i = 0; $i < 50; $i++) {
    $o = new stdClass();
    $o->{(string) name_of($i)} = $i;
    $o->{(string) name_of($i + 1)} = 1;
    $total += $o->{(string) name_of($i)};
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "1225\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// A runtime-name `unset($object->$name)` compiles even though SOME class in the program declares
/// an untyped property whose slot cannot hold the removed state.
///
/// The name is only known at run time, and the closure is bound to a scope that sees protected
/// members, so the removal ladder has an arm for `Error::$code`, an untyped slot that cannot hold
/// the removed state. That arm used to refuse the whole closure at compile time ("unset() of
/// untyped property slot Failure::$code"), which handed Symfony's DeepClone hydrators -- and every
/// file depending on them -- to the interpreter. The arm now raises only if it actually runs; this
/// program never reaches it. Expected output is `php -n` 8.5's.
#[test]
fn test_runtime_name_unset_compiles_beside_an_untyped_error_slot() {
    let out = compile_and_run(
        r#"<?php
class Failure extends \Error {}
class Item { public ?string $label = 'x'; public ?int $count = 3; }
function hydrator(string $class): \Closure {
    return \Closure::bind(static function ($object, array $values, array $unsetOnNull): void {
        foreach ($values as $name => $value) {
            if (null === $value && isset($unsetOnNull[$name])) {
                unset($object->$name);
                continue;
            }
            $object->$name = $value;
        }
    }, null, $class);
}
$item = new Item();
hydrator(Item::class)($item, ['label' => null, 'count' => 7], ['label' => true]);
var_dump(isset($item->label), $item->count);
$e = new Failure('m');
hydrator(Failure::class)($e, ['message' => 'n'], []);
echo get_class($e), " ", $e->getMessage(), "\n";
"#,
    );
    assert_eq!(out, "bool(false)\nint(7)\nFailure n\n");
}

/// An element write into a property declared `array|SomeObject` goes through the boxed cell's
/// run-time dispatch, like a `mixed` property (symfony/http-kernel's `DataCollector::$data`).
#[test]
fn test_element_write_into_an_array_or_object_union_property() {
    let out = compile_and_run(
        r#"<?php
class Data { }
class Coll {
    protected array|Data $data = [];
    public function collect(bool $r): void {
        if ($r) { $this->data["redirect"] = true; $this->data["url"] = "/x"; }
        $this->data["n"] = 3;
    }
    public function get(): array { return $this->data; }
}
$c = new Coll();
$c->collect(true);
echo json_encode($c->get()), "\n";
"#,
    );
    assert_eq!(out, "{\"redirect\":true,\"url\":\"\\/x\",\"n\":3}\n");
}
