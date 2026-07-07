//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC regressions, including regression assoc value in function, regression iterate assoc in function, and regression arr equals func arr.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Regression test: associative array values accessed inside a function after the
/// array is passed as an argument. Verifies that reading multiple keys (`done`,
/// `title`, `priority`) from a passed assoc array produces correct output.
#[test]
fn test_regression_assoc_value_in_function() {
    let out = compile_and_run(
        r#"<?php
function show($todo) {
    $status = $todo["done"] === "1" ? "[x]" : "[ ]";
    $pri = $todo["priority"];
    echo $status . " " . $todo["title"] . " " . $pri;
}
$t = ["title" => "Buy milk", "done" => "0", "priority" => "high", "created" => "now"];
show($t);
"#,
    );
    assert_eq!(out, "[ ] Buy milk high");
}

/// Regression test: iterating a numerically-indexed array of assoc arrays inside a
/// function. Verifies that indexed access into a passed array of assoc arrays
/// (`$items[$i]["name"]`, `$items[$i]["value"]`) works correctly.
#[test]
fn test_regression_iterate_assoc_in_function() {
    let out = compile_and_run(
        r#"<?php
function format($items) {
    $result = "";
    for ($i = 0; $i < count($items); $i++) {
        $item = $items[$i];
        $result .= $item["name"] . ":" . $item["value"] . "\n";
    }
    return $result;
}
$data = [["name" => "a", "value" => "1"], ["name" => "b", "value" => "2"]];
echo format($data);
"#,
    );
    assert_eq!(out, "a:1\nb:2\n");
}

/// Regression test: appending to a function parameter array and returning it.
/// Verifies that `$arr[] = $val` and returning the modified array works across
/// multiple calls and that the result is correctly assigned back to the caller's
/// variable.
#[test]
fn test_regression_arr_equals_func_arr() {
    let out = compile_and_run(
        r#"<?php
function add($arr, $val) {
    $arr[] = $val;
    return $arr;
}
$nums = [1];
$nums = add($nums, 2);
$nums = add($nums, 3);
echo count($nums) . "|" . $nums[0] . "|" . $nums[2];
"#,
    );
    assert_eq!(out, "3|1|3");
}

/// Verifies that nested integerish arithmetic inside a function releases Mixed
/// temporaries cleanly. Runs 1000 iterations and asserts the heap is clean
/// (allocations == deallocations) with no leaks.
#[test]
fn test_nested_integerish_arithmetic_releases_mixed_temporaries() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
function ready(int $slot): bool {
    $offset = ($slot + 1) * 8 + 6;
    return $offset != 0;
}

for ($i = 0; $i < 1000; $i++) {
    if (ready(0)) {
        $seen = 1;
    }
}
echo "done";
"#,
    );
    assert_eq!(out.stdout, "done");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Regression: a temporary object implicitly stringified via `__toString` in `echo` must
/// be released, not leaked. 100 iterations would accumulate 100 leaked objects otherwise.
#[test]
fn test_tostring_temp_object_released_on_echo() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Greeter { public function __toString(): string { return "hi"; } }
for ($i = 0; $i < 100; $i++) { echo new Greeter(); }
echo "\n";
"#,
    );
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Regression: a temporary object stringified via `__toString` in a concatenation must be
/// released.
#[test]
fn test_tostring_temp_object_released_on_concat() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Greeter { public function __toString(): string { return "hi"; } }
for ($i = 0; $i < 100; $i++) { $s = "x" . new Greeter(); }
echo "done";
"#,
    );
    assert_eq!(out.stdout, "done");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Regression: a temporary object cast to string via `(string)` must be released.
#[test]
fn test_tostring_temp_object_released_on_string_cast() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Greeter { public function __toString(): string { return "hi"; } }
for ($i = 0; $i < 100; $i++) { $s = (string) new Greeter(); }
echo "done";
"#,
    );
    assert_eq!(out.stdout, "done");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Guard: a variable-held object stringified via `__toString` is owned by the variable
/// slot and must NOT be released by the coercion (otherwise it would be double-freed).
#[test]
fn test_tostring_variable_object_not_double_freed() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class Greeter { public function __toString(): string { return "hi"; } }
$g = new Greeter();
for ($i = 0; $i < 100; $i++) { echo $g; }
echo "\n";
"#,
    );
    assert!(out.success, "program crashed (double free?): {}", out.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs, frees, "expected clean heap, got: {}", out.stderr);
}

/// Regression test: creating assoc arrays via a factory function, pushing them
/// into a numerically-indexed array, then iterating and accessing keys. Verifies
/// that `make()` return values survive being stored and retrieved from a outer
/// array.
#[test]
fn test_regression_make_assoc_then_iterate() {
    let out = compile_and_run(
        r#"<?php
function make($name, $val) { return ["name" => $name, "val" => $val]; }
$items = [];
$items[] = make("x", "1");
$items[] = make("y", "2");
$items[] = make("z", "3");
for ($i = 0; $i < count($items); $i++) {
    $it = $items[$i];
    echo $it["name"] . "=" . $it["val"] . " ";
}
"#,
    );
    assert_eq!(out, "x=1 y=2 z=3 ");
}

/// Regression test: concatenating multiple string accesses from assoc array
/// elements inside a loop and returning the result. Verifies that repeated
/// `$content .= $items[$i]["a"] . "|" . ...` chains are handled correctly.
#[test]
fn test_regression_save_concat_chain() {
    let out = compile_and_run(
        r#"<?php
function save($items) {
    $content = "";
    for ($i = 0; $i < count($items); $i++) {
        $c = $items[$i];
        $content .= $c["a"] . "|" . $c["b"] . "|" . $c["c"] . "\n";
    }
    return $content;
}
$data = [["a" => "x", "b" => "y", "c" => "z"]];
echo save($data);
"#,
    );
    assert_eq!(out, "x|y|z\n");
}

/// Regression test: passing an object to a function and accessing its properties
/// (`$dog->name`, `$dog->breed`). Verifies that object properties are correctly
/// accessible inside function scope.
#[test]
fn test_regression_object_string_property_in_function() {
    let out = compile_and_run(
        r#"<?php
class Dog {
    public $name;
    public $breed;
    public function __construct($n, $b) { $this->name = $n; $this->breed = $b; }
}
function describe($dog) {
    return $dog->name . " (" . $dog->breed . ")";
}
$d = new Dog("Rex", "Labrador");
echo describe($d);
"#,
    );
    assert_eq!(out, "Rex (Labrador)");
}

/// Regression test: objects stored in an array are retrieved and their methods
/// called. Verifies that `$items[$i]->format()` works correctly after objects are
/// stored and fetched from a numerically-indexed array.
#[test]
fn test_regression_objects_in_array_with_methods() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
    public function format() { return $this->name . ": $" . $this->price; }
}
$items = [new Item("Apple", 1), new Item("Banana", 2)];
for ($i = 0; $i < count($items); $i++) {
    echo $items[$i]->format() . "\n";
}
"#,
    );
    assert_eq!(out, "Apple: $1\nBanana: $2\n");
}

/// Regression test: early `return` inside a `switch` within a loop. Verifies that
/// `label()` returning from within `switch` cases in a loop does not corrupt
/// control flow or stack state.
#[test]
fn test_regression_switch_return_in_loop() {
    let out = compile_and_run(
        r#"<?php
function label($n) {
    switch ($n % 3) {
        case 0: return "A";
        case 1: return "B";
        default: return "C";
    }
}
$r = "";
for ($i = 0; $i < 6; $i++) {
    $r .= label($i);
}
echo $r;
"#,
    );
    assert_eq!(out, "ABCABC");
}

/// Regression test: chained string operations (`strtolower`, `str_replace`) on a
/// function parameter. Verifies that successive string builtins that modify a
/// local variable work correctly and the result is returned.
#[test]
fn test_regression_string_ops_in_function() {
    let out = compile_and_run(
        r#"<?php
function clean($s) {
    $s = strtolower($s);
    $s = str_replace(" ", "_", $s);
    return $s;
}
echo clean("Hello World");
"#,
    );
    assert_eq!(out, "hello_world");
}

/// Verifies retaining a mutable string parameter cannot clobber a later ABI
/// argument before the callee saves it, and that the retained copy is released.
#[test]
fn test_owned_string_parameter_preserves_later_mixed_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function describe(string $label, mixed $value): string {
    $label .= "!";
    return $label . ":" . count($value);
}
$items = [1, 2, 3];
echo describe("items", $items);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "items!:3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected one balanced string-parameter retain, got: {}",
        out.stderr
    );
}

/// Verifies self-reassignment retains a borrowed string slice before freeing its source slot.
#[test]
fn test_string_self_reassignment_preserves_borrowed_builtin_slice() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function normalize(string $value): string {
    $value = trim($value);
    return $value;
}
echo normalize("  hi  ");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "hi");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after aliased self-reassignment, got: {}",
        out.stderr
    );
}

/// Regression test: `explode` result used as an array inside a function, then
/// indexed. Verifies that `$parts[0]` and `$parts[1]` access the correct exploded
/// segments after `explode` is called on a comma-separated string.
#[test]
fn test_regression_explode_in_function_use_parts() {
    let out = compile_and_run(
        r#"<?php
function parse($csv) {
    $parts = explode(",", $csv);
    return $parts[0] . "+" . $parts[1];
}
echo parse("foo,bar");
"#,
    );
    assert_eq!(out, "foo+bar");
}

/// Regression test: function returns an assoc array and the caller reads multiple
/// keys from it. Verifies that `config()["host"]`, `config()["port"]`, etc.
/// access the correct returned values after a single call.
#[test]
fn test_regression_return_assoc_read_keys() {
    let out = compile_and_run(
        r#"<?php
function config() {
    return ["host" => "localhost", "port" => "3306", "db" => "myapp"];
}
$c = config();
echo $c["host"] . ":" . $c["port"] . "/" . $c["db"];
"#,
    );
    assert_eq!(out, "localhost:3306/myapp");
}

/// Regression test: reading multiple distinct keys (`first`, `second`, `third`)
/// from a single assoc array parameter. Verifies correct access to each key
/// without interference between the reads.
#[test]
fn test_regression_multiple_hash_get_locals() {
    let out = compile_and_run(
        r#"<?php
function show($row) {
    $a = $row["first"];
    $b = $row["second"];
    $c = $row["third"];
    echo $a . "|" . $b . "|" . $c;
}
show(["first" => "x", "second" => "y", "third" => "z"]);
"#,
    );
    assert_eq!(out, "x|y|z");
}

/// Regression test: method receives a string parameter and also accesses an
/// object property (`$this->prefix`). Verifies that the property is correctly
/// available inside the method and the result is returned correctly.
#[test]
fn test_regression_method_string_param_and_prop() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public $prefix;
    public function __construct($p) { $this->prefix = $p; }
    public function greet($name) { return $this->prefix . " " . $name . "!"; }
}
$g = new Greeter("Hello");
echo $g->greet("World");
"#,
    );
    assert_eq!(out, "Hello World!");
}

/// Regression test: object property stores a string that was derived from a
/// concatenated literal (`"AB" . "CD"`). Verifies that property initialization
/// and subsequent method access (`$this->bytes`) survives constructor parameter
/// cleanup without corrupting the stored value.
#[test]
fn test_regression_string_property_survives_constructor_param_cleanup() {
    let out = compile_and_run(
        r#"<?php
class Reader {
    public $bytes;
    public function __construct(string $bytes) { $this->bytes = $bytes; }
    public function head(): string { return substr($this->bytes, 0, 4); }
}
$bytes = "AB" . "CD";
$reader = new Reader($bytes);
echo $reader->head();
"#,
    );
    assert_eq!(out, "ABCD");
}

/// Regression test: a string variable passed to a constructor is still usable
/// after the object is created. Verifies that the callee (constructor) does not
/// prematurely free the caller's string argument, leaving the original variable
/// with a valid value.
#[test]
fn test_regression_callee_does_not_free_caller_string_argument() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public $prefix;
    public function __construct($prefix) {
        $this->prefix = $prefix;
    }
}
$prefix = "IWAD";
$greeter = new Greeter($prefix);
echo $prefix;
echo "|";
echo $greeter->prefix;
"#,
    );
    assert_eq!(out, "IWAD|IWAD");
}

/// Regression test: a large heap-backed string (1 MB file) is read, sliced via
/// `substr`, stored in an object property, and the object is returned from a
/// function. Verifies that heap-allocated string slices survive across object
/// return and are not prematurely collected or corrupted.
#[test]
fn test_regression_string_property_persists_heap_slice_across_object_return() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("elephc_str_persist_{}.bin", id));
    let mut bytes = vec![b'X'; 1024 * 1024];
    bytes[..8].copy_from_slice(b"PLAYPAL\0");
    fs::write(&path, &bytes).unwrap();

    let source = format!(
        r#"<?php
class WadLike {{
    public $name;
    public function __construct() {{
        $this->name = "";
    }}
}}

class Maker {{
    public function make(): WadLike {{
        $bytes = file_get_contents("{path}");
        $name = substr($bytes, 0, 7);
        $wad = new WadLike();
        $wad->name = $name;
        return $wad;
    }}
}}

$maker = new Maker();
$wad = $maker->make();
echo $wad->name;
"#,
        path = path.display()
    );

    let out = compile_and_run_with_heap_size(&source, 67_108_864);
    let _ = fs::remove_file(&path);
    assert_eq!(out, "PLAYPAL");
}

/// Regression test: an object returned from a function carries a property built
/// via a loop that accumulates string characters. Verifies that the property built
/// inside the loop (`$name .= $ch`) is correctly preserved on the returned object.
#[test]
fn test_regression_returned_object_preserves_loop_built_string_property() {
    let out = compile_and_run(
        r#"<?php
class WadLike {
    public $kind;
    public $firstEntryName;
    public function __construct($kind) {
        $this->kind = $kind;
        $this->firstEntryName = "";
    }
}

class Maker {
    public function make(): WadLike {
        $bytes = "IWADxxxxPLAYPAL\0tail";
        $kind = substr($bytes, 0, 4);
        $raw = substr($bytes, 8, 8);
        $name = "";
        $i = 0;
        while ($i < strlen($raw)) {
            $ch = substr($raw, $i, 1);
            if (ord($ch) == 0) {
                break;
            }
            $name .= $ch;
            $i += 1;
        }
        $wad = new WadLike($kind);
        $wad->firstEntryName = $name;
        return $wad;
    }
}

$maker = new Maker();
$wad = $maker->make();
echo $wad->kind;
echo "|";
echo $wad->firstEntryName;
"#,
    );
    assert_eq!(out, "IWAD|PLAYPAL");
}

/// Regression test: calling a static method with parameters and string
/// concatenation. Verifies that `Fmt::wrap("hello", "b")` correctly concatenates
/// the tag and string and returns the wrapped result.
#[test]
fn test_regression_static_method_string() {
    let out = compile_and_run(
        r#"<?php
class Fmt {
    public static function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
}
echo Fmt::wrap("hello", "b");
"#,
    );
    assert_eq!(out, "<b>hello</b>");
}

/// Regression test: chained property access (`$o->inner->val`) where an inner
/// object is stored in an outer object property and returned. Verifies that
/// accessing a property of a nested object works correctly.
#[test]
fn test_regression_chained_property_access() {
    let out = compile_and_run(
        r#"<?php
class Inner { public $val;
    public function __construct($v) { $this->val = $v; }
}
class Outer { public $inner;
    public function __construct($i) { $this->inner = $i; }
}
$o = new Outer(new Inner(42));
echo $o->inner->val;
"#,
    );
    assert_eq!(out, "42");
}

/// Regression test: object with a float property, used in a method that
/// performs arithmetic. Verifies that float properties are correctly stored and
/// used in method computations.
#[test]
fn test_regression_float_property() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14 * $this->radius * $this->radius; }
}
$c = new Circle(10.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "314");
}

/// Regression test: `$obj->prop[] = <scalar>` boxes the scalar into a Mixed cell
/// and `__rt_array_push_refcounted` retains a reference. The codegen was keeping
/// the cell's original (heap_alloc) reference and never releasing it, leaking one
/// reference past the array's deep-free. Asserts heap is clean at exit.
#[test]
fn test_regression_property_array_push_scalar_does_not_leak() {
    // `$obj->prop[] = <scalar>` boxes the scalar into a Mixed cell, then
    // `__rt_array_push_refcounted` retains its own reference to that cell.
    // The codegen kept the cell's original (heap_alloc) reference and never
    // released it, so the boxed Mixed cell leaked one reference and survived
    // past the array's deep-free. Regression: heap must be clean at exit.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
$x->a[] = 4;
echo count($x->a);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies that assigning a Mixed indexed-array cell to a local retains an
/// independent owner and does not leave the array with a dangling cell.
#[test]
fn test_mixed_indexed_array_read_survives_local_unset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [5, "x"];
$first = $values[0];
unset($first);
echo $values[0];
unset($values);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "5");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies that assigning a Mixed associative-array cell to a local retains an
/// independent owner and does not leave the hash with a dangling cell.
#[test]
fn test_mixed_assoc_array_read_survives_local_unset() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = ["a" => 5, "b" => "x"];
$first = $values["a"];
unset($first);
echo $values["a"];
unset($values);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "5");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: pushing an owned array literal into a Mixed-element property
/// array adds a second ownership layer. The inner array is retained by the Mixed
/// box and the Mixed box is retained by `__rt_array_push_refcounted`. The property
/// push path must use the container-aware boxer and release the boxed cell after
/// the append. Asserts heap is clean at exit.
#[test]
fn test_regression_property_array_push_array_value_does_not_leak() {
    // Pushing an owned array literal into a Mixed-element property array adds
    // a second ownership layer: the inner array is retained by the Mixed box,
    // and the Mixed box is retained by `__rt_array_push_refcounted`. The
    // property push path must use the container-aware boxer (so the inner
    // array's original reference is released) and release the boxed cell
    // after the append. Regression: heap must be clean at exit.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
$x->a[] = "hello";
$x->a[] = [1, 2, 3];
echo count($x->a);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: passing an owned array literal through a `mixed` parameter
/// before storing it in a property array must release the argument expression's
/// original owner after the Mixed cell retains the payload.
#[test]
fn test_regression_mixed_arg_array_payload_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C {
    public array $a;

    public function __construct() {
        $this->a = [];
    }

    public function add(mixed $value): void {
        $this->a[] = $value;
    }
}

$x = new C();
$x->add([1, 2, 3]);
unset($x);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: a loop pushing scalars into a property array repeatedly
/// exercises the boxing + push path; each iteration must balance its refcount or
/// the leak compounds. Asserts heap is clean after 20 iterations.
#[test]
fn test_regression_property_array_push_in_loop_does_not_leak() {
    // A loop pushing scalars into a property array repeatedly exercises the
    // boxing + push path; each iteration must balance its refcount or the
    // leak compounds.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $a; }
$x = new C();
$x->a = [];
for ($i = 0; $i < 20; $i++) {
    $x->a[] = $i;
}
echo count($x->a);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "20");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: overwriting a static array must release the payloads appended
/// to the old array. The scalar is boxed into Mixed and retained by
/// `__rt_array_push_refcounted`; only the replacement static array should remain
/// live at exit. Asserts exactly one live block (the current static array).
#[test]
fn test_regression_static_property_array_push_scalar_releases_old_payload() {
    // Static storage itself is process-lifetime state, but an overwritten
    // static array must release the payloads appended to the old array. The
    // scalar is boxed into Mixed and then retained by `__rt_array_push_refcounted`;
    // only the replacement static array should remain live at exit.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public static array $a; }
C::$a = [];
C::$a[] = 4;
C::$a = [];
echo count(C::$a);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0");
    assert!(
        out.stderr
            .contains("HEAP DEBUG: leak summary: live_blocks=1"),
        "expected only the current static array to remain live, got: {}",
        out.stderr
    );
}

/// Regression test: pushing an owned array literal into a Mixed-element static
/// property array needs both the container-aware boxer and the post-push release.
/// After the static property is overwritten the old array and appended literal
/// should be gone; only the replacement static array remains live. Asserts exactly
/// one live block.
#[test]
fn test_regression_static_property_array_push_array_value_releases_old_payload() {
    // Pushing an owned array literal into a Mixed-element static property array
    // needs both the container-aware boxer and the post-push release. After the
    // static property is overwritten, the old array and appended literal should
    // be gone; only the replacement static array remains live by design.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public static array $a; }
C::$a = [];
C::$a[] = [1, 2, 3];
C::$a = [];
echo count(C::$a);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0");
    assert!(
        out.stderr
            .contains("HEAP DEBUG: leak summary: live_blocks=1"),
        "expected only the current static array to remain live, got: {}",
        out.stderr
    );
}

/// Regression test: array literals with spread build their result through
/// `__rt_array_push_refcounted` for refcounted elements. The non-spread element
/// path retained the element via `retain_borrowed_heap_arg` and again inside the
/// push helper without releasing the codegen's owning reference, leaking the
/// appended element. Asserts heap is clean at exit.
#[test]
fn test_regression_spread_array_literal_does_not_leak() {
    // Array literals with spread build their result through
    // `__rt_array_push_refcounted` for refcounted elements. The non-spread
    // element path retained the element via `retain_borrowed_heap_arg` and
    // again inside the push helper without releasing the codegen's owning
    // reference, leaking the appended element. Regression: heap clean at exit.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [[1], [2]];
$b = [...$a, [3]];
echo count($b);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: `foreach ($items as &$value)` on a non-empty array where the
/// by-ref loop variable is reassigned after the loop. Verifies that the by-ref
/// loop does not leak the local ref cell, and that reassigning `$value = 99`
/// after the loop correctly mutates the array element.
#[test]
fn test_regression_foreach_by_ref_non_empty_does_not_leak_local_ref_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$items = [1, 2, 3];
foreach ($items as &$value) {
    $value = $value + 10;
}
$value = 99;
echo $items[2];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "99");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: `foreach ($items as &$value)` on an empty array (after
/// `array_pop` empties it) should not leak the local ref cell. The loop body is
/// never entered but the by-ref binding must still be cleaned up at function exit.
#[test]
fn test_regression_foreach_by_ref_empty_releases_local_ref_cell_at_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$value = 7;
$items = [1];
array_pop($items);
foreach ($items as &$value) {
    $value = 1;
}
echo $value;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "7");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: a by-ref foreach reusing the same variable name (`$value`)
/// across two separate loops where the first array becomes empty mid-way.
/// Verifies that the prior fallback type (`string`) is released when `$value` is
/// rebound to a new by-ref cell in the second loop.
#[test]
fn test_regression_foreach_by_ref_reused_name_releases_prior_fallback_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run() {
    $value = "held";
    $first = [1];
    array_pop($first);
    foreach ($first as &$value) {
        $value = 1;
    }

    $second = [10, 20];
    foreach ($second as &$value) {
        $value += 100;
    }

    $value = 999;
    echo $second[0];
    echo "|";
    echo $second[1];
    echo "|";
    echo $value;
}

run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "110|999|999");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for the x86_64 Mixed-property string-read aliasing bug.
///
/// Reading a string-typed property off an object retrieved from a Mixed-valued
/// hash (`$arr['a']->v`) goes through `emit_property_load`'s two-word string
/// path. On x86_64 the string-pointer result register is `rax`, which also
/// serves as the object base register in the Mixed-property dispatch, so loading
/// the pointer word first clobbered the base; the length word was then read from
/// the string payload instead of the object. That garbage length drove
/// `__rt_str_persist` to copy an enormous span and exhaust the heap. The fix
/// reads the length word first when the pointer register aliases the base. ARM64
/// was always correct (its result registers never alias the base). A plain
/// object (not an enum) reproduces it, so this guards the general lowering.
#[test]
fn test_regression_mixed_hash_object_string_property() {
    let out = compile_and_run(
        r#"<?php
class Box { public string $v = 'hi'; }
$arr = ['a' => new Box(), 'b' => 1];
echo $arr['a']->v;
"#,
    );
    assert_eq!(out, "hi");
}

/// Regression test: assigning a by-value foreach element into another local should
/// release the target slot using its widened Mixed storage representation.
#[test]
fn test_regression_foreach_mixed_value_assignment_releases_old_slot_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    foreach (["x", "y", "z"] as $p) {
        $k = $p;
    }
}
echo "x";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: refcounted hidden ternary merge temps must be released when
/// reassigned across loop iterations and during function/main epilogue cleanup.
#[test]
fn test_regression_refcounted_hidden_ternary_temp_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 50; $i++) {
    $a = ["a" => "b"];
    $x = isset($a["missing"]) ? $a["missing"] : "";
}
echo "x";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: `isset($hash[$missing])` on a Mixed-valued hash should probe
/// presence/null without allocating a throwaway Mixed miss value every iteration.
#[test]
fn test_regression_mixed_hash_isset_miss_does_not_materialize_leaking_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($i = 0; $i < 50; $i++) {
    $a = ["a" => 1, "b" => "s"];
    $x = isset($a["missing"]) ? $a["missing"] : "";
}
echo "x";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for the call-argument release / alias-guard interaction: a user
/// function that accepts an array and returns it typed as `iterable`
/// (`function id(iterable $x): iterable`) aliases its argument. Releasing owned
/// call-argument temporaries must not free that shared payload, or the returned
/// value would be corrupted (read back as null/garbage). This asserts correctness
/// only — the conservative alias guard intentionally keeps the argument alive when
/// it might be the return value, which leaves a pre-existing passthrough leak that
/// is out of scope for the call-argument-release fix.
#[test]
fn test_iterable_passthrough_arg_not_freed() {
    let out = compile_and_run(
        r#"<?php
function id(iterable $x): iterable { return $x; }
$total = 0;
for ($i = 0; $i < 100; $i++) {
    $v = id([1, 2, 3]);
    foreach ($v as $x) { $total += $x; }
}
echo $total;
"#,
    );
    assert_eq!(out, "600");
}

/// Regression test for the call-argument-temporary leak: a fresh array literal passed
/// to a plain BUILTIN (`count([...])`) inside a loop is an owning temporary that must
/// be released after each call. Before the fix it leaked one array allocation per
/// iteration — an unbounded heap leak. The sibling iterable-passthrough test cannot
/// catch this because a leak does not change program output, so this heap-debug test
/// is the one that actually guards the bug.
#[test]
fn test_builtin_call_owned_array_arg_temp_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $t += count([$i, 20, 30]);
}
echo $t;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "600");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: a fresh array literal passed to a plain USER function that does
/// not return it is an owning temporary that must be released after the call, the same
/// way method and builtin calls do. Before the fix, user (and extern) calls never
/// released owned argument temporaries, leaking one array per iteration. The alias
/// guard still keeps a passthrough result alive (see
/// `test_iterable_passthrough_arg_not_freed`); here the argument is discarded, so the
/// heap must stay flat.
#[test]
fn test_user_function_owned_array_arg_temp_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function sink(array $a): int { return count($a); }
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $t += sink([$i, 20, 30]);
}
echo $t;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "600");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for the array-to-string echo fix: echoing an owned temporary array
/// stringifies to "Array" and releases the temporary, keeping GC allocs and frees balanced
/// (no leak from the discarded array, no premature/double free).
#[test]
fn test_echo_owned_temp_array_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats("<?php echo [1, 2, 3];");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Array");
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert!(
        allocs - baseline_allocs >= 1,
        "expected the temporary array to allocate at least once"
    );
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// Regression test for issue #408: releasing a string-keyed associative array
/// must free everything it owns. Reassigning a hash-typed local each iteration
/// promotes a fresh indexed array literal to hash storage via `array_to_hash`,
/// which builds the result hash from a copy of the source array; the source
/// array was leaked once per conversion. With many iterations the leak would
/// exhaust the fixed heap, so a balanced alloc/free count proves it is freed.
#[test]
fn test_regression_408_reassigned_string_keyed_array_does_not_leak() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$g = [];
for ($n = 0; $n < 500; $n++) {
    $g = [];
    $g["a"] = "x";
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert!(allocs >= 500, "expected per-iteration allocations: {allocs}");
    assert_eq!(
        allocs, frees,
        "string-keyed array release must free its source array (issue #408)"
    );
}

/// Regression test for issue #408 (heap-debug): the same reassignment loop must
/// report a clean heap with no live blocks at exit and must not trip the
/// double-free detector, confirming the conversion releases exactly one
/// reference of the source array (correct COW ownership).
#[test]
fn test_regression_408_reassigned_string_keyed_array_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = [];
for ($n = 0; $n < 500; $n++) {
    $g = [];
    $g["a"] = "x";
    $g["bb"] = "yy";
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: a local widened to boxed Mixed by loop arithmetic must be
/// released on return paths that return another value instead of that local.
#[test]
fn test_widened_loop_local_cleaned_on_alternate_return_path() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {}
class Sink {
    public array $objects;

    public function __construct() {
        $this->objects = [];
    }

    public function idx(mixed $object): int {
        $i = 0;
        $limit = count($this->objects);
        while ($i < $limit) {
            if ($this->objects[$i] === $object) {
                return $i;
            }
            $i++;
        }
        return -1;
    }
}

$box = new Box();
$sink = new Sink();
echo $sink->idx($box);
unset($sink);
unset($box);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "-1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: ordinary PHP global overwrites release the previous Mixed box
/// and the final global value is released before heap-debug leak reporting.
#[test]
fn test_ordinary_global_reassignment_releases_previous_mixed() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = 0;
function set_global(int $value): void {
    global $g;
    $g = $value;
}
for ($i = 0; $i < 200; $i++) {
    set_global($i);
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #408 (in-place promotion): a string-key write that
/// promotes a freshly built indexed array literal to hash storage must also free
/// the source array. Repeated promotion in a loop keeps GC allocs and frees
/// balanced with no leaked source arrays.
#[test]
fn test_regression_408_string_key_promotion_does_not_leak() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$g = [];
for ($n = 0; $n < 500; $n++) {
    $g = [1, 2];
    $g["a"] = "x";
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(
        allocs, frees,
        "promoting an indexed array literal to hash storage must free the source array (issue #408)"
    );
}

/// Regression test for issue #538: growing one function-local array from indexed
/// `[]` storage into a string-keyed hash must release every replaced COW generation.
#[test]
fn test_regression_538_function_local_hash_promotion_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function buildSet(int $count): array {
    $set = [];
    for ($i = 0; $i < $count; $i++) {
        $key = 'k' . $i;
        $set[$key] = true;
    }
    return $set;
}

$set = buildSet(75);
echo count($set);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "75");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap (issue #538), got: {}",
        out.stderr
    );
    let allocs = out
        .stderr
        .lines()
        .find(|line| line.starts_with("HEAP DEBUG: allocs="))
        .and_then(|line| line.split("allocs=").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing heap-debug allocation count: {}", out.stderr));
    assert!(
        allocs < 350,
        "expected in-place growth without one COW clone per write (issue #538), got {allocs} allocations: {}",
        out.stderr
    );
}

/// Verifies issue #538 without loop-flow widening: consecutive string-key writes
/// to one function-local promoted hash must replace storage without leaking.
#[test]
fn test_regression_538_function_local_direct_hash_writes_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function buildPair(): array {
    $set = [];
    $set['first'] = true;
    $set['second'] = true;
    return $set;
}

$set = buildPair();
echo count($set);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected direct hash writes to leave a clean heap (issue #538), got: {}",
        out.stderr
    );
}

/// Verifies issue #538 when a concrete load is consumed before a later write
/// widens the same function-local frame slot to boxed Mixed storage.
#[test]
fn test_regression_538_early_consumer_before_hash_promotion_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function inspectBeforePromotion(): int {
    $set = [];
    $before = count($set);
    $set['later'] = true;
    return $before + count($set);
}

echo inspectBeforePromotion();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected an early consumer of the later-widened slot to remain clean (issue #538), got: {}",
        out.stderr
    );
}

/// Verifies issue #538 for method-local storage whose final path consumes the
/// promoted hash but returns a scalar instead of transferring the hash owner.
#[test]
fn test_regression_538_method_local_hash_promotion_scalar_return_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class SetBuilder {
    public function countKeys(int $count): int {
        $set = [];
        for ($i = 0; $i < $count; $i++) {
            $key = 'k' . $i;
            $set[$key] = true;
        }
        return count($set);
    }
}

$builder = new SetBuilder();
echo $builder->countKeys(75);
unset($builder);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "75");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected method-local hash promotion to leave a clean heap (issue #538), got: {}",
        out.stderr
    );
}

/// Verifies issue #538 preserves COW aliasing while replacing a boxed local hash:
/// mutating the original after assignment must not change or free the snapshot.
#[test]
fn test_regression_538_hash_promotion_preserves_cow_alias_ownership() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function compareSnapshot(): string {
    $set = [];
    for ($i = 0; $i < 5; $i++) {
        $key = 'k' . $i;
        $set[$key] = true;
    }
    $snapshot = $set;
    $set['later'] = true;
    return count($snapshot) . ':' . count($set);
}

echo compareSnapshot();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "5:6");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected COW aliases to remain valid and heap-clean (issue #538), got: {}",
        out.stderr
    );
}

/// Verifies issue #538 when a COW snapshot is assigned before the original
/// local is later widened from indexed-array to hash-backed Mixed storage.
#[test]
fn test_regression_538_early_cow_alias_before_hash_promotion_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function compareEarlySnapshot(): string {
    $set = [];
    $snapshot = $set;
    $set['later'] = true;
    return count($snapshot) . ':' . count($set);
}

echo compareEarlySnapshot();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0:1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected an early COW alias to remain valid and clean (issue #538), got: {}",
        out.stderr
    );
}

/// Keeps the top-level equivalent of issue #538 as a clean control so the fix
/// remains scoped to boxed function/method-local replacement ownership.
#[test]
fn test_regression_538_top_level_hash_growth_remains_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$set = [];
for ($i = 0; $i < 75; $i++) {
    $key = 'k' . $i;
    $set[$key] = true;
}
echo count($set);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "75");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the top-level control to remain heap-clean (issue #538), got: {}",
        out.stderr
    );
}

/// Regression test for issue #534: a refcounted local assigned inside a NESTED
/// inner loop must not leak one block per OUTER iteration. The inner counter's
/// re-initialization (`$k = 0`) is lowered while the slot still looks like an
/// Int, but the `$k++` update widens the slot to boxed Mixed storage, so the
/// re-init store must still release the previous outer iteration's box.
#[test]
fn test_regression_534_nested_loop_local_does_not_leak_per_outer_iteration() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
for ($i = 0; $i < 1000; $i++) {
    for ($k = 0; $k < 1; $k++) {
        $s = 'x' . $i;
        $acc = $acc + strlen($s);
    }
}
echo 'acc=' . $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "acc=3890");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap (issue #534: one leaked block per outer iteration), got: {}",
        out.stderr
    );
}

/// Regression test for issue #525: a chained subscript read on a `?array`
/// receiver materializes the intermediate container through the nullable-access
/// hidden owned temp (`lower_nullable_array_access`), and the consuming read
/// must release that temp's reference after its last use. Exercises both the
/// non-null receiver (populated temp) and the null receiver (boxed-null temp)
/// on every iteration and asserts the heap is clean at exit.
#[test]
fn test_regression_525_nullable_chained_read_releases_hidden_temp() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function probe(?array $r): int {
    $x = $r[0][1];
    if ($x !== null) {
        return 1;
    }
    return 0;
}
$hits = 0;
for ($k = 0; $k < 50; $k++) {
    $a = [['a', 'b' . $k]];
    $hits = $hits + probe($a);
    $hits = $hits + probe(null);
}
echo $hits;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #534 (boxed element variant): the same nested shape
/// reading a boxed array element into a local leaked the previous outer
/// iteration's Mixed box on every `$k = 0` re-initialization.
#[test]
fn test_regression_534_nested_loop_array_element_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$arr = ['aa', 'bb'];
$acc = 0;
for ($i = 0; $i < 1000; $i++) {
    for ($k = 0; $k < 1; $k++) {
        $lc = $arr[$k];
        $acc = $acc + strlen($lc);
    }
}
echo 'acc=' . $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "acc=2000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap (issue #534: one leaked block per outer iteration), got: {}",
        out.stderr
    );
}

/// Regression test for issue #516: a chained subscript read (`$a[$t][1]`) on a
/// nested indexed array materializes the inner array as an owned +1 temporary
/// (the inner `array_get` increfs refcounted elements), and nothing ever
/// released it, leaking the inner array's blocks on every evaluation. The
/// consuming (outer) index read must release the intermediate once its result
/// is extracted. Regression: heap must be clean at exit and output unchanged.
#[test]
fn test_regression_516_chained_subscript_read_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function make(int $n): array {
    $a = [];
    for ($i = 0; $i < $n; $i++) { $a[] = ['kw', 'word' . $i]; }
    return $a;
}
$a = make(50);
$n = count($a);
$out = 0;
for ($t = 0; $t < $n; $t++) { $x = (string) $a[$t][1]; $out = $out + strlen($x); }
echo 'out=' . $out;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    // 10 five-char values (word0..word9) + 40 six-char values (word10..word49).
    assert_eq!(out.stdout, "out=290");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #534 (int-only variant): the leak was the INNER
/// LOOP COUNTER's Mixed box, not the refcounted body value, so a nested loop
/// with a purely integer body leaked identically. Also proves that scalar
/// arithmetic locals in nested loops stay leak-free after the deferred-release
/// fix.
#[test]
fn test_regression_534_nested_loop_int_counter_box_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
for ($i = 0; $i < 1000; $i++) {
    for ($k = 0; $k < 3; $k++) {
        $acc = $acc + $k;
    }
}
echo 'acc=' . $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "acc=3000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap (issue #534: leaked inner-counter Mixed boxes), got: {}",
        out.stderr
    );
}

/// Regression test for issue #525 (multiple reads): each chained read on the
/// same `?array` receiver creates its own nullable-access hidden temp, and every
/// temp's reference must be released independently — using the receiver twice
/// must not leak either intermediate container or double-release one of them.
#[test]
fn test_regression_525_nullable_receiver_read_twice_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function probe2(?array $r): int {
    $a = $r[0][0];
    $b = $r[0][1];
    if ($a !== null && $b !== null) {
        return 1;
    }
    return 0;
}
$hits = 0;
for ($k = 0; $k < 50; $k++) {
    $pair = [['a' . $k, 'b' . $k]];
    $hits = $hits + probe2($pair);
    $hits = $hits + probe2(null);
}
echo $hits;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #534 (function scope + conditional assignment):
/// inside a function, an inner-loop local conditionally reassigned across
/// outer iterations must release the carried value without double-freeing on
/// iterations that skip the assignment, and triple nesting with `while` inner
/// loops must stay balanced too.
#[test]
fn test_regression_534_nested_loop_conditional_and_triple_nesting_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(int $outer): int {
    $acc = 0;
    $s = 'seed';
    $i = 0;
    while ($i < $outer) {
        for ($j = 0; $j < 2; $j++) {
            for ($k = 0; $k < 2; $k++) {
                if ($k == 0) {
                    $s = 'x' . $i . $j;
                }
            }
        }
        $acc = $acc + strlen($s);
        $i++;
    }
    return $acc;
}
echo 'acc=' . run(300);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "acc=1390");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap for conditional nested-loop reassignment, got: {}",
        out.stderr
    );
}

/// Verifies deferred loop-slot cleanup remains balanced across `break` and `continue` paths.
#[test]
fn test_regression_534_break_continue_paths_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
$value = 0;
for ($i = 0; $i < 50; $i++) {
    for ($k = 0; $k < 5; $k++) {
        if ($k == 1) {
            continue;
        }
        if ($k == 4) {
            break;
        }
        $value = 0;
        $value++;
        $acc = $acc + $value;
    }
}
$alias =& $value;
echo $acc . '|' . $alias;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "150|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap across break/continue paths, got: {}",
        out.stderr
    );
}

/// Verifies a local already bound by reference before a loop never receives raw-slot cleanup.
#[test]
fn test_regression_534_prebound_ref_cell_loop_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$value = 0;
$alias =& $value;
for ($i = 0; $i < 100; $i++) {
    $value = 0;
    $value++;
}
echo $alias;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap for a pre-bound ref-cell slot, got: {}",
        out.stderr
    );
}

/// Verifies a syntactic promotion inside a loop reuses its runtime cell on later iterations.
#[test]
fn test_regression_534_repeated_loop_ref_promotion_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$value = 0;
for ($i = 0; $i < 5; $i++) {
    $alias =& $value;
    $alias = $i;
}
echo $value . '|' . $alias;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4|4");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected repeated runtime promotion to reuse one balanced cell, got: {}",
        out.stderr
    );
}

/// Regression for a tagged nullable parameter whose local slot widens to owned Mixed storage.
#[test]
fn test_regression_widened_tagged_parameter_releases_final_mixed_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function normalize(?int $value, int $iterations): ?int {
    for ($i = 0; $i < $iterations; $i++) {
        $value = 0;
    }
    return $value;
}
normalize(null, 0);
echo normalize(null, 1);
echo '|';
echo normalize(null, 3);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0|0");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected widened parameter storage to be released, got: {}",
        out.stderr
    );
}

/// Verifies reassignment of a borrowed Mixed parameter cannot release the caller's owner.
#[test]
fn test_regression_reassigned_mixed_parameter_retains_caller_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(mixed $value): mixed {
    $value = 0;
    return $value;
}
$caller = [1, 2, 3];
echo replace($caller) . '|';
echo count($caller);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0|3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected borrowed caller value and reassigned parameter to stay balanced, got: {}",
        out.stderr
    );
}

/// Verifies conditional ref-cell promotion cleans either each raw local owner or its cell.
#[test]
fn test_regression_conditional_parameter_ref_promotion_is_path_balanced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function maybe_bind(array $value, bool $bind): int {
    if ($bind) {
        $alias =& $value;
    }
    $value = [4, 5];
    return count($value);
}
function maybe_bind_local(bool $bind): int {
    $value = [4, 5];
    if ($bind) {
        $alias =& $value;
    }
    return count($value);
}
function conditional_realias(bool $bind): int {
    $value = 1;
    if ($bind) {
        $first =& $value;
    }
    $second =& $value;
    $second = 7;
    return $value;
}
function increment_ref(int &$value): void {
    $value = $value + 1;
}
function conditional_ref_argument(bool $bind): int {
    $value = 1;
    if ($bind) {
        $alias =& $value;
    }
    increment_ref($value);
    return $value;
}
$caller = [1, 2, 3];
echo maybe_bind($caller, false) . '|';
echo maybe_bind($caller, true) . '|';
echo maybe_bind_local(false) . '|';
echo maybe_bind_local(true) . '|';
echo conditional_realias(false) . '|';
echo conditional_realias(true) . '|';
echo conditional_ref_argument(false) . '|';
echo conditional_ref_argument(true) . '|';
echo count($caller);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2|2|2|2|7|7|2|2|3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected raw and promoted local paths to stay balanced, got: {}",
        out.stderr
    );
}

/// Regression test for issue #516 (three-level chain): every intermediate of a
/// deeper chained subscript read (`$a[$i][$j][$k]`) is an owned container
/// temporary and each must be released by its consuming read. Also guards
/// against over-eager releases: each intermediate stays alive through its
/// parent container's reference, so the extracted leaf string must stay valid.
/// Building the triple-nested structure has a small pre-existing constant leak
/// (2 blocks, unrelated to reads), so this asserts via GC counters that the
/// alloc/free gap stays constant instead of growing with read iterations.
#[test]
fn test_regression_516_three_level_chained_read_does_not_leak() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$a = [];
for ($i = 0; $i < 3; $i++) {
    $mid = [];
    for ($j = 0; $j < 3; $j++) { $mid[] = ['leaf', 'val' . $i . $j]; }
    $a[] = $mid;
}
$out = 0;
for ($r = 0; $r < 40; $r++) {
    for ($i = 0; $i < 3; $i++) {
        for ($j = 0; $j < 3; $j++) {
            $x = (string) $a[$i][$j][1];
            $out = $out + strlen($x);
        }
    }
}
echo 'out=' . $out;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "out=1800");
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    // Before the fix the leaked +1 references pinned every distinct mid/leaf
    // container past process cleanup (gap of 32 blocks on this fixture); after
    // the fix only the 2-block build residue may remain.
    assert!(
        allocs >= frees && allocs - frees < 10,
        "chained three-level reads must release their intermediates: allocs={} frees={}",
        allocs,
        frees
    );
}

/// Regression test for issue #516 (string-keyed nesting): a chained
/// associative read (`$m['x']['y']`) goes through `hash_get`, whose refcounted
/// results also carry a +1 caller reference. The chained consumer must release
/// the intermediate hash exactly like the indexed-array path. Regression: heap
/// must be clean at exit after repeated evaluation.
#[test]
fn test_regression_516_chained_string_keyed_read_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = ['x' => ['y' => 'deep', 'z' => 'other'], 'w' => ['y' => 'second']];
for ($i = 0; $i < 200; $i++) {
    $v = (string) $m['x']['y'];
}
echo $m['x']['y'];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "deep");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #516: a typed string extracted from a chained
/// read must remain valid when a later call argument releases the parent array.
#[test]
fn test_regression_516_chained_string_survives_parent_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace_parent(array &$value): string {
    $value = [];
    return str_repeat('Z', 128);
}
function emit_pair(string $left, string $right): void {
    echo $left;
    echo $right;
}
$a = [[str_repeat('L', 128)]];
emit_pair($a[0][0], replace_parent($a));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        format!("{}{}", "L".repeat(128), "Z".repeat(128))
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #527: a by-value `foreach` over an array of arrays
/// whose body string-coerces an element read (`echo $row[1] . "\n"`) must not
/// leak. The `$row[1]` read produces an owned boxed Mixed temporary; the
/// implicit Mixed→string coercion detaches its result, so the source box must
/// be released after the cast instead of leaking two heap blocks per iteration
/// (the box plus the string payload it owns).
#[test]
fn test_regression_527_foreach_value_element_string_coercion_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
for ($i = 0; $i < 4; $i++) { $a[] = ['kw', 'word' . $i]; }
foreach ($a as $row) { echo $row[1] . "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "word0\nword1\nword2\nword3\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #527 (post-loop semantics): after a by-value
/// `foreach` ends, the loop variable must still hold the LAST element like PHP,
/// and string-coercing an element of that surviving copy must release the
/// element box so the heap stays clean at exit.
#[test]
fn test_regression_527_foreach_value_var_survives_loop_and_heap_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [];
for ($i = 0; $i < 4; $i++) { $a[] = ['kw', 'word' . $i]; }
foreach ($a as $row) {}
echo $row[1] . "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "word3\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #527 (key + value form): iterating an associative
/// array of arrays with string keys binds both an owned key copy and an owned
/// value copy per iteration; string-coercing `$row[1]` inside the body must
/// release the element box each iteration and end with a clean heap.
#[test]
fn test_regression_527_foreach_assoc_key_value_element_coercion_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = [];
$m['alpha'] = ['kw', 'wordA'];
$m['beta'] = ['kw', 'wordB'];
$m['gamma'] = ['kw', 'wordC'];
foreach ($m as $k => $row) { echo $k . '=' . $row[1] . "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "alpha=wordA\nbeta=wordB\ngamma=wordC\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #527 outside `foreach`: explicit casts,
/// concatenation, and interpolation must release owned Mixed element boxes for
/// every scalar runtime tag without changing PHP-visible string conversion.
#[test]
fn test_regression_527_mixed_string_coercions_outside_foreach_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$values = [
    "string" => str_repeat("S", 3),
    "int" => 42,
    "float" => 2.5,
    "true" => true,
    "false" => false,
    "null" => null,
];
$explicit = (string) $values["string"];
$concat = "i=" . $values["int"];
$interpolated = "f={$values['float']}";
echo $explicit . "|" . $concat . "|" . $interpolated;
echo "|t=" . $values["true"];
echo "|f=" . $values["false"];
echo "|n=" . $values["null"];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "SSS|i=42|f=2.5|t=1|f=|n=");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A non-owning Mixed local load must survive repeated explicit and implicit
/// string conversions; releasing the parameter's box during the first cast
/// would make the later reads use freed storage or double-release it at exit.
#[test]
fn test_regression_527_mixed_local_survives_repeated_string_coercion() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function render_mixed(mixed $value): void {
    $explicit = (string) $value;
    echo $explicit;
    echo "|[$value]";
    echo "|again=" . $value;
    echo "|" . (string) $value;
}
render_mixed(str_repeat("alive", 1));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "alive|[alive]|again=alive|alive");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A non-null `int|string` union uses boxed Mixed codegen storage, so owned
/// function results must be released after both explicit casts and implicit
/// concatenation while the detached string result remains valid.
#[test]
fn test_regression_527_union_string_coercion_releases_owned_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function choose_union(bool $asString): int|string {
    if ($asString) {
        return str_repeat("union", 1);
    }
    return 42;
}
$left = (string) choose_union(true);
$right = "n=" . choose_union(false);
echo $left . "|" . $right;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "union|n=42");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression for #484: boxing an owned object into a Mixed cell (a `?Class` return
/// coerces through `mixed_box`) retains the payload in the runtime, so the lowering must
/// release the producer's own reference. Before the fix `function g(): ?P { return new
/// P(); }` leaked one `P` per call even though the boxed cell itself was freed on rebind.
#[test]
fn test_regression_mixed_boxed_object_return_releases_producer() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public int $v = 7; }
function g(): ?P { return new P(); }
$acc = 0;
for ($n = 0; $n < 50; $n++) {
    $c = g();
    $acc += $c->v;
}
echo $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "350");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the boxed objects to be released, got: {}",
        out.stderr
    );
}

/// Regression for #484 (array flavour): boxing an owned array into a Mixed cell retains
/// it in the runtime; the producer's reference must be released or the array leaks once
/// per boxing (e.g. a `?array`-returning function building a fresh literal).
#[test]
fn test_regression_mixed_boxed_array_return_releases_producer() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pair(int $n): ?array { return [$n, $n + 1]; }
$acc = 0;
for ($n = 0; $n < 50; $n++) {
    $p = pair($n);
    $acc += $p[1];
}
echo $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1275");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the boxed arrays to be released, got: {}",
        out.stderr
    );
}
