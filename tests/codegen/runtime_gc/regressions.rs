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

/// Verifies method callback adapters transfer a returned Mixed argument box and release all peers.
#[test]
fn test_method_callback_mixed_return_boxes_balance_gc_stats() {
    let out = compile_and_run_with_gc_stats(
        r#"<?php
class MixedIdentity {
    public function first(mixed $value): mixed { return $value; }
}
$identity = new MixedIdentity();
for ($i = 0; $i < 100; $i++) {
    $mapped = array_map([$identity, "first"], [1, 2, 3]);
    echo $mapped[0];
}
unset($mapped);
unset($identity);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
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
    // The path is embedded in a PHP double-quoted literal, where Windows
    // backslashes could otherwise become escapes such as `\r` or `\t`.
    let php_path = path.to_string_lossy().replace('\\', "/");

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
        path = php_path
    );

    let out = compile_and_run_with_heap_size(&source, 67_108_864);
    let _ = fs::remove_file(&path);
    assert_eq!(out, "PLAYPAL");
}

/// Regression test for issue #540: each successful `file_get_contents()` call
/// returns an owned Mixed box containing an owned string. A retaining string cast
/// must release that source value instead of leaking two blocks per call.
#[test]
fn test_file_get_contents_owned_success_result_is_released_after_string_cast() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("elephc_fgc_owned_success_{}.txt", id));
    fs::write(&path, b"payload").unwrap();
    let php_path = path.to_string_lossy().replace('\\', "/");

    let source = format!(
        r#"<?php
for ($i = 0; $i < 100; $i++) {{
    $contents = (string) file_get_contents("{path}");
}}
echo $contents;
"#,
        path = php_path
    );
    let out = compile_and_run_with_heap_debug(&source);
    let _ = fs::remove_file(&path);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "payload");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #540: the failure branch of `file_get_contents()`
/// returns a fresh boxed `false`. Casting repeated failures to string must release
/// each source box while preserving PHP's empty-string result.
#[test]
fn test_file_get_contents_owned_false_result_is_released_after_string_cast() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("elephc_fgc_owned_missing_{}.txt", id));
    let _ = fs::remove_file(&path);

    let source = format!(
        r#"<?php
for ($i = 0; $i < 100; $i++) {{
    $contents = (string) @file_get_contents("{path}");
}}
echo strlen($contents);
"#,
        path = path.display()
    );
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
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

/// Regression test: overwriting a Mixed/nullable-object static property with a
/// freshly-constructed object must release the previous object's boxed owner. A
/// non-Mixed value assigned to a Mixed slot is boxed with `__rt_mixed_from_value`,
/// which takes its own retained reference to the object; the owning `new C()`
/// temporary is a separate reference that must be released after the store, or
/// each overwrite leaks one object. Twenty iterations must stay bounded — only the
/// final boxed value remains live in the process-lifetime static slot (one block),
/// never a per-iteration accumulation of twenty.
#[test]
fn test_regression_static_property_object_overwrite_releases_old_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public int $v = 0; }
class H { public static ?C $h = null; }
for ($i = 0; $i < 20; $i++) {
    H::$h = new C();
}
H::$h = null;
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr
            .contains("HEAP DEBUG: leak summary: live_blocks=1"),
        "expected only the final boxed static value to remain live (bounded), got: {}",
        out.stderr
    );
}

/// Verifies repeated Mixed-to-object static stores retain only the current object.
///
/// The untyped setter receives a boxed Mixed argument, while inference gives the
/// static property a concrete object slot. Each store must retain the unboxed
/// object independently, release the replaced object, and leave no call-argument
/// or assignment-result owners behind.
#[test]
fn test_regression_untyped_static_setter_object_overwrite_stays_bounded() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C {
    protected static $instance;

    public static function set($instance) {
        return static::$instance = $instance;
    }
}

for ($i = 0; $i < 20; $i++) {
    C::set(new C());
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr
            .contains("HEAP DEBUG: leak summary: live_blocks=1"),
        "expected only the current static object to remain live, got: {}",
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
/// value would be corrupted (read back as null/garbage). The retained temporary
/// owner transfers to the function result, so normal result cleanup must also leave
/// the heap balanced.
#[test]
fn test_iterable_passthrough_arg_not_freed() {
    let out = compile_and_run_with_heap_debug(
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
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "600");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: returning a borrowed Mixed parameter must not make the call
/// result an owning temporary. Releasing that alias would invalidate the source
/// local before its next use.
#[test]
fn test_borrowed_mixed_user_call_result_does_not_free_source_local() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function identity(mixed $value): mixed { return $value; }
$values = [1];
$value = array_pop($values);
echo identity($value), "|", $value;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
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
/// guard still transfers a passthrough owner to the result (see
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

/// Regression test for issue #540 root cause 5: an Array-returning method whose
/// result is fresh must not keep an unrelated inline Array argument alive. The
/// old type-only alias guard treated every `Array -> Array` call as a possible
/// passthrough and leaked the argument plus its two boxed string elements on
/// every invocation.
#[test]
fn test_regression_540_fresh_method_array_result_releases_inline_array_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CallAliasScanner {
    public function scan(array $delimiters): array {
        $tokens = [];
        $count = count($delimiters);
        for ($i = 0; $i < $count; $i++) {
            $tokens[] = $delimiters[$i];
        }
        return $tokens;
    }
}
$scanner = new CallAliasScanner();
for ($i = 0; $i < 40; $i++) {
    $tokens = $scanner->scan(['//', '#']);
}
$copy = $tokens;
$copy[] = 'tail';
echo $tokens[0] . ':' . count($tokens) . ':' . count($copy);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "//:2:3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the precise call-result summary keeps only the argument a method
/// actually returns. The unrelated first temporary must be released, while the
/// second temporary transfers its owner to the result and remains COW-safe.
#[test]
fn test_regression_540_method_passthrough_preserves_only_aliased_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CallAliasChooser {
    public function choose(array $discard, array $keep): array {
        return $keep;
    }
}
$chooser = new CallAliasChooser();
for ($i = 0; $i < 40; $i++) {
    $value = $chooser->choose(['drop'], ['keep']);
}
$copy = $value;
$copy[] = 'tail';
echo $value[0] . ':' . count($value) . ':' . count($copy);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "keep:1:2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a non-final receiver summary includes descendant overrides. The base
/// implementation returns fresh storage, but the child returns its argument; a
/// call through the base type must retain that possible alias without leaking.
#[test]
fn test_regression_540_method_alias_summary_includes_overrides() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class CallAliasBase {
    public function choose(array $value): array { return []; }
}
class CallAliasChild extends CallAliasBase {
    public function choose(array $value): array { return $value; }
}
function chooseThroughBase(CallAliasBase $chooser): array {
    return $chooser->choose(['child']);
}
for ($i = 0; $i < 40; $i++) {
    $value = chooseThroughBase(new CallAliasChild());
}
echo $value[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "child");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the same precise cleanup applies to a source function that returns
/// a locally-created array containing a value retained from its argument.
#[test]
fn test_regression_540_fresh_function_result_releases_inline_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function copyFirst(array $source): array {
    $copy = [];
    $copy[] = $source[0];
    return $copy;
}
for ($i = 0; $i < 40; $i++) {
    $functionResult = copyFirst(['function']);
}
echo $functionResult[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "function");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #540: a fresh object passed directly to a constructor
/// is an owning argument temporary. `ObjectNew` must release the caller's reference
/// after the constructor returns, even when the constructor ignores the argument.
#[test]
fn test_object_new_releases_owned_nested_object_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ObjectNewNestedPayload {}
class ObjectNewNestedSink {
    public function __construct(ObjectNewNestedPayload $payload) {}
}
for ($i = 0; $i < 100; $i++) {
    $sink = new ObjectNewNestedSink(new ObjectNewNestedPayload());
}
echo "ok";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #540: an inline array passed to a constructor owns
/// its array payload and boxed Mixed elements. All of those temporary allocations
/// must be released after the constructor has consumed the argument.
#[test]
fn test_object_new_releases_owned_inline_array_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ObjectNewArraySink {
    public function __construct(array $items) {}
}
for ($i = 0; $i < 100; $i++) {
    $sink = new ObjectNewArraySink([$i, $i + 1, $i + 2]);
}
echo "ok";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test for issue #540: a Mixed container read is an owned temporary
/// before it is materialized for a typed constructor parameter.
/// `ObjectNew` must release that temporary after the constructor returns.
#[test]
fn test_object_new_releases_owned_mixed_container_read_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ObjectNewStringSink {
    public function __construct(string $value) {}
}
$source = ["value" => "payload", "other" => 1];
for ($i = 0; $i < 100; $i++) {
    $sink = new ObjectNewStringSink((string) $source["value"]);
}
unset($sink);
echo "ok";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies that constructor argument cleanup does not release borrowed by-reference
/// array storage. The constructor must still mutate the caller's array, and both the
/// caller and object lifetimes must finish with a clean heap.
#[test]
fn test_object_new_argument_cleanup_preserves_by_ref_array_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ObjectNewArrayMutator {
    public function __construct(array &$items) { $items[] = 3; }
}
$items = [1, 2];
$mutator = new ObjectNewArrayMutator($items);
echo count($items) . ":" . $items[2];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3:3");
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

/// Regression for #448: a caught exception object must be released once the catch
/// handler is done with it. Before the fix the catch binding moved the throwable into
/// the variable's slot without ever scheduling a release, leaking one ~48-byte block
/// per catch. A throw/catch loop must leave the heap clean.
#[test]
fn test_regression_caught_exception_released_per_catch() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $e) {}
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after caught exceptions, got: {}",
        out.stderr
    );
}

/// Regression for #448: a catch clause without a variable must still consume and
/// release the in-flight exception instead of dropping the reference.
#[test]
fn test_regression_unbound_catch_releases_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try { throw new RuntimeException("x"); } catch (\Throwable) {}
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after unbound catches, got: {}",
        out.stderr
    );
}

/// Regression for #448: rethrowing the caught variable (`throw $e`) hands the same
/// object to an outer handler. With catch slots owned and released, the rethrow must
/// retain the borrowed local so inner and outer bindings each own a reference —
/// neither a leak nor a double free.
#[test]
fn test_regression_rethrow_chain_balances_references() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try {
        try { throw new LogicException("x"); } catch (\Throwable $e) { throw $e; }
    } catch (\Throwable $outer) {}
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after rethrow chains, got: {}",
        out.stderr
    );
}

/// Regression for #448: aliasing the caught exception (`$x = $e`) acquires its own
/// reference; both locals release at scope exit and the heap stays clean.
#[test]
fn test_regression_caught_exception_alias_balances_references() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $e) { $x = $e; }
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after aliased catches, got: {}",
        out.stderr
    );
}

/// Regression for #448: a function that throws and catches internally must release
/// the caught exception in its epilogue like any owned object local.
#[test]
fn test_regression_function_scoped_catch_releases_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function poke(int $n): int {
    try { throw new RuntimeException("r$n"); } catch (\Throwable $e) { return $n; }
}
$acc = 0;
for ($n = 0; $n < 50; $n++) { $acc += poke($n); }
echo $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1225");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after function-scoped catches, got: {}",
        out.stderr
    );
}

/// Regression for #448: a caught exception that legitimately escapes the catch (pushed
/// into an array) is retained by the container and NOT freed by the catch slot's own
/// cleanup — exactly the escaped objects stay live, nothing more (no double free, no
/// extra leak on top of the retention). `return $e` from inside a catch is a separate,
/// pre-existing unsupported shape, so the array escape is the coverable transfer path.
#[test]
fn test_regression_escaped_caught_exception_retained_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$keep = [];
for ($n = 0; $n < 5; $n++) {
    try { throw new LogicException("boom"); } catch (\Throwable $e) { $keep[] = $e; }
}
echo count($keep), "|", strlen($keep[4]->getMessage());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "5|4");
    // Match the no-try control that pushes `new LogicException("boom")` into the same
    // array: main's epilogue currently leaves the escaped throwables (and their
    // message payloads) live rather than deep-freeing array object elements. Catch
    // cleanup must not free them a second time — a double free would abort with a
    // bad-refcount fatal before this assertion — and must not add blocks beyond that
    // baseline (10 = 5 exceptions + 5 message strings under the current allocator).
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: live_blocks=10"),
        "expected the no-try escape baseline (10 live blocks), got: {}",
        out.stderr
    );
}

/// Regression for #448: `finally` on both paths — a caught exception with a finally
/// block, and a propagating exception whose finally runs before the outer catch —
/// must keep the heap flat like the plain catch shapes.
#[test]
fn test_regression_finally_paths_release_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g(): int {
    try { throw new RuntimeException("inner"); } finally { }
}
$hits = 0;
for ($n = 0; $n < 50; $n++) {
    try { throw new TypeError("x"); } catch (\Throwable $e) { $hits++; } finally { }
    try { g(); } catch (\Throwable $e) { $hits++; }
}
echo $hits;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "100");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap across finally paths, got: {}",
        out.stderr
    );
}

/// Regression for #448: a multi-type catch clause (`catch (A | B $e)`) types the slot
/// as Throwable; both matching arms must release through the same owned-slot lifecycle.
#[test]
fn test_regression_multi_type_catch_releases_exception() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$hits = 0;
for ($n = 0; $n < 50; $n++) {
    try {
        if ($n % 2 == 0) { throw new LogicException("a"); }
        throw new RuntimeException("b");
    } catch (LogicException | RuntimeException $e) {
        $hits++;
    }
}
echo $hits;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap for multi-type catches, got: {}",
        out.stderr
    );
}

/// Regression for #448: a catch variable declared through `global` must replace the
/// shared global value, release prior iterations, and remain readable after the function.
#[test]
fn test_regression_catch_binding_updates_global_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$caught = null;
function catchGlobally(): int {
    global $caught;
    $hits = 0;
    for ($n = 0; $n < 50; $n++) {
        try { throw new RuntimeException("g"); }
        catch (\Throwable $caught) {
            if ($caught instanceof RuntimeException) { $hits++; }
        }
    }
    return $hits;
}
echo catchGlobally(), "|", get_class($caught);
$caught = null;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50|RuntimeException");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected global catch rebinding to keep a clean heap, got: {}",
        out.stderr
    );
}

/// Regression for #448: binding a catch into a referenced local must write through
/// the shared ref cell instead of replacing the frame slot's cell pointer.
#[test]
fn test_regression_catch_binding_updates_reference_cell_storage() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$value = new LogicException("old");
$target =& $value;
$hits = 0;
for ($n = 0; $n < 50; $n++) {
    try { throw new LogicException("r"); }
    catch (\Throwable $target) {
        if ($value instanceof LogicException) { $hits++; }
    }
}
echo $hits, "|", get_class($value);
$value = null;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50|LogicException");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected reference-cell catch rebinding to keep a clean heap, got: {}",
        out.stderr
    );
}

/// Regression for #448: releasing a closure that captured a caught throwable must
/// dispatch heap kind 6 through `__rt_decref_any` on every supported architecture.
#[test]
fn test_regression_caught_throwable_capture_uses_uniform_decref_dispatch() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$hits = 0;
for ($n = 0; $n < 50; $n++) {
    try { throw new RuntimeException("captured"); }
    catch (\Throwable $e) {
        $callback = function () use ($e) { return $e instanceof RuntimeException; };
        if ($callback() === true) { $hits++; }
        $callback = null;
    }
}
echo $hits;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "50");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected throwable captures to release through decref_any, got: {}",
        out.stderr
    );
}

/// Regression for #448: expression-form rethrow (`true ? throw $e : 0`) must retain
/// the catch local the same way statement-form `throw $e` does. Without the retain,
/// inner and outer catch slots share one reference and epilogue/rebind double-frees.
#[test]
fn test_regression_expression_rethrow_balances_references() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 50; $n++) {
    try {
        try { throw new LogicException("x"); } catch (\Throwable $e) { $x = true ? throw $e : 0; }
    } catch (\Throwable $outer) {}
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap after expression-form rethrows, got: {}",
        out.stderr
    );
}

/// Regression for #448: `Fiber::throw($caught)` must retain a separate in-flight
/// reference before the suspended fiber consumes it, while both catch bindings
/// continue to own and release their local references.
#[test]
fn test_regression_fiber_throw_balances_caught_exception_references() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
for ($n = 0; $n < 20; $n++) {
    $fiber = new Fiber(function (): void {
        try { Fiber::suspend(); } catch (\Throwable $inside) {}
    });
    $fiber->start();
    try { throw new LogicException("fiber"); }
    catch (\Throwable $caught) { $fiber->throw($caught); }
    $fiber = null;
}
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected Fiber::throw catch transfers to keep a clean heap, got: {}",
        out.stderr
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

/// Regression for issue #540 root cause 4: narrowing a Mixed-backed local to an
/// object makes each property receiver load retain the unboxed object. Named,
/// dynamic, and nullsafe reads must all release that temporary receiver retain.
#[test]
fn test_regression_540_property_reads_release_owned_mixed_local_receivers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Paste {
    public string $named = "named";
    public string $dynamic = "dynamic";
    public string $safe = "safe";
    public int $count = 1;
}

$named_result = "";
$dynamic_result = null;
$safe_result = null;
$safe_dynamic_result = null;
$property = "dynamic";
$safe_property = "safe";
$paste = null;
for ($i = 0; $i < 40; $i++) {
    $paste = new Paste();
    $named_result = $paste->named;
    $dynamic_result = $paste->{$property};
    $safe_result = $paste?->safe;
    $safe_dynamic_result = $paste?->{$safe_property};
}
echo $named_result;
echo ":";
echo $dynamic_result;
echo ":";
echo $safe_result;
echo ":";
echo $safe_dynamic_result;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "named:dynamic:safe:safe");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected property receiver retains to be released, got: {}",
        out.stderr
    );
}

/// Verifies issue #540's receiver cleanup preserves typed property payloads from
/// temporary objects. String and nested-object reads must not become dangling,
/// while an extracted array must remain independently mutable and heap-clean.
#[test]
fn test_regression_540_temporary_property_results_survive_receiver_cleanup() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Leaf {
    public string $name = "leaf";
}

class Holder {
    public Leaf $leaf;
    public array $items = ["one"];

    public function __construct() {
        $this->leaf = new Leaf();
    }
}

function make_holder(): Holder {
    return new Holder();
}

$name = make_holder()->leaf->name;
$items = make_holder()->items;
$items[] = "two";
echo $name;
echo ":";
echo count($items);
echo ":";
echo $items[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "leaf:2:one");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected temporary property results to remain valid and clean, got: {}",
        out.stderr
    );
}

/// Ensures issue #540's cleanup does not release an ordinary borrowed receiver.
/// Reading an array property into a local must retain normal PHP COW behavior:
/// mutating the copy leaves the object's property unchanged.
#[test]
fn test_regression_540_borrowed_property_receiver_preserves_cow() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag {
    public array $items = ["original"];
}

$bag = new Bag();
$copy = $bag->items;
$copy[] = "copy";
echo count($bag->items);
echo ":";
echo count($copy);
echo ":";
echo $bag->items[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:2:original");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected borrowed receiver and COW ownership to stay balanced, got: {}",
        out.stderr
    );
}

/// Verifies a nullsafe property read releases an owning nullable call result on
/// both branches. In particular, a boxed null receiver must not leak when `?->`
/// short-circuits before the property read.
#[test]
fn test_regression_540_nullsafe_property_releases_null_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class MaybeBox {
    public string $value = "present";
}

function maybe_box(bool $present): ?MaybeBox {
    if ($present) {
        return new MaybeBox();
    }
    return null;
}

$missing = null;
for ($i = 0; $i < 40; $i++) {
    $missing = maybe_box(false)?->value;
}
$present = maybe_box(true)?->value;
if (is_null($missing)) {
    echo "null";
}
echo ":";
echo $present;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "null:present");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected both nullsafe receiver branches to be clean, got: {}",
        out.stderr
    );
}

/// Regression for issue #540's post-fix residual: scalar casts and builtins with
/// independent results must release inline-read stabilization owners even when a
/// concrete receiver's provisional release is later pruned.
#[test]
fn test_regression_540_inline_array_reads_do_not_keep_stabilization_owners() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $fields = ["bench", "php", "1720000000", "0"];
    $expiresAt = (int) $fields[3];
    $title = rawurldecode((string) $fields[0]);
    $createdAt = (int) $fields[2];
    $sum += $expiresAt + $createdAt + strlen($title);
}
echo $sum;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "172000000500");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected inline Array reads to be heap-clean, got: {}",
        out.stderr
    );
}

/// Regression for issue #540's post-fix residual: nullable-object property reads
/// reach a typed constructor as Mixed operands. EIR-created Mixed-to-string copies
/// are caller-owned temporaries and must be released after the constructor returns.
#[test]
fn test_regression_540_constructor_releases_mixed_string_conversion_temporaries() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ResidualMetadata {
    public function __construct(public string $id, public string $title) {}
}
function loadResidualMetadata(int $n): ?ResidualMetadata {
    return new ResidualMetadata("id" . $n, "title");
}
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $metadata = loadResidualMetadata($i);
    if ($metadata instanceof ResidualMetadata) {
        $metadata = new ResidualMetadata($metadata->id, $metadata->title);
        $sum += strlen($metadata->id) + strlen($metadata->title);
    }
}
echo $sum;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "890");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected constructor conversion temporaries to be released, got: {}",
        out.stderr
    );
}

/// Regression for issue #540's post-fix residual: a method returning `implode()`
/// storage cannot alias its string parameters. Property-read argument owners from a
/// Mixed-backed object local must therefore be released after the call.
#[test]
fn test_regression_540_implode_return_summary_releases_property_arguments() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ResidualPair {
    public function __construct(public string $left, public string $right) {}
}
class ResidualJoiner {
    public function join(string $left, string $right): string {
        $parts = [$left, $right];
        return implode('', $parts);
    }
}
function maybeResidualPair(): ?ResidualPair {
    return new ResidualPair("left", "right");
}
$joiner = new ResidualJoiner();
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $pair = maybeResidualPair();
    if ($pair instanceof ResidualPair) {
        $pair = new ResidualPair("left", "right");
        $joined = $joiner->join($pair->left, $pair->right);
        $sum += strlen($joined);
    }
}
echo $sum;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "900");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected property arguments to be released, got: {}",
        out.stderr
    );
}

/// Verifies Mixed-to-string EIR cleanup transfers rather than frees a temporary
/// when a typed passthrough method returns the exact argument storage.
#[test]
fn test_regression_540_mixed_string_conversion_preserves_passthrough_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ResidualStringBox {
    public function __construct(public string $value) {}
}
class ResidualStringIdentity {
    public function keep(string $value): string {
        return $value;
    }
}
function maybeResidualStringBox(int $n): ?ResidualStringBox {
    return new ResidualStringBox("value" . $n);
}
$identity = new ResidualStringIdentity();
$last = "";
for ($i = 0; $i < 100; $i++) {
    $box = maybeResidualStringBox($i);
    if ($box instanceof ResidualStringBox) {
        $box = new ResidualStringBox("value" . $i);
        $last = $identity->keep(value: $box->value);
    }
}
echo $last;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "value99");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected passthrough conversion ownership to stay balanced, got: {}",
        out.stderr
    );
}

/// Verifies read-stabilization cleanup does not release a borrowed dynamic string
/// after a retaining array write has copied it into another container.
#[test]
fn test_regression_540_read_stabilization_cleanup_preserves_borrowed_owner() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$seed = "value" . $argc;
$source = [$seed];
$copy = [$source[0]];
echo $source[0] . "|" . $copy[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "value1|value1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected both dynamic string owners to remain balanced, got: {}",
        out.stderr
    );
}

/// Verifies a wrapper returning independent HTML-escape storage releases a
/// stabilized property argument instead of treating the result as a passthrough.
#[test]
fn test_regression_540_html_escape_summary_releases_property_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ResidualEscaper {
    public function escape(string $value): string {
        return htmlspecialchars($value);
    }
}
class ResidualLabel {
    public function __construct(public string $lang) {}
}
function maybeResidualLabel(): ?ResidualLabel {
    return new ResidualLabel("<php>");
}
$escaper = new ResidualEscaper();
$last = "";
for ($i = 0; $i < 100; $i++) {
    $label = maybeResidualLabel();
    if ($label instanceof ResidualLabel) {
        $label = new ResidualLabel("<php>");
        $last = $escaper->escape($label->lang);
    }
}
echo $last;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "&lt;php&gt;");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected HTML escape argument ownership to stay balanced, got: {}",
        out.stderr
    );
}

/// Verifies a stabilized first argument survives a later by-reference argument
/// that replaces the same receiver before the callee consumes the read value.
#[test]
fn test_regression_540_read_stabilization_survives_later_receiver_mutation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replaceResidualValues(array &$values): int {
    $values = ["new"];
    return 0;
}
function consumeResidualValue(string $value, int $ignored): int {
    return strlen($value);
}
$values = ["old"];
$length = consumeResidualValue($values[0], replaceResidualValues($values));
echo $length . "|" . $values[0];
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3|new");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the stabilized argument and replaced receiver to be clean, got: {}",
        out.stderr
    );
}

/// Verifies constructor ABI materialization releases a temporary boxed-Mixed
/// argument after the constructor has retained it in promoted property storage.
#[test]
fn test_regression_540_constructor_releases_scalar_to_mixed_abi_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ResidualMixedSink {
    public function __construct(public mixed $value) {}
}
$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $sink = new ResidualMixedSink($i);
    $sum += (int) $sink->value;
}
echo $sum;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4950");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected constructor ABI boxes to be released, got: {}",
        out.stderr
    );
}

/// Canonical issue #500 repro: an integer expression whose binary operator has a
/// parenthesized compound operand (`($i * 7 + 1) & 0xFFFF`) must not leak the
/// checked-arithmetic Mixed box per evaluation. Both the split (`comboVar`) and
/// direct (`comboParen`) variants must complete with a clean heap.
#[test]
fn test_regression_500_paren_compound_operand_combo() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function comboParen(): int { $acc = 0; for ($i = 0; $i < 3000; $i++) { $acc = ($i * 7 + 1) & 0xFFFF; } return $acc; }
function comboVar(): int   { $acc = 0; for ($i = 0; $i < 3000; $i++) { $v = $i * 7 + 1; $acc = $v & 0xFFFF; } return $acc; }
for ($k = 0; $k < 600; $k++) { comboVar(); }  echo "comboVar ok\n";
for ($k = 0; $k < 600; $k++) { comboParen(); } echo "comboParen ok\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "comboVar ok\ncomboParen ok\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected checked-arithmetic operand boxes to be released, got: {}",
        out.stderr
    );
}

/// Issue #500: every bitwise/shift operator consuming a parenthesized compound
/// operand must release the checked-arithmetic Mixed box it coerces to int.
#[test]
fn test_regression_500_bitops_release_checked_operand_boxes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = 0; $b = 0; $c = 0; $d = 0; $e = 0;
for ($i = 0; $i < 2000; $i++) {
    $a = ($i * 7 + 1) & 0xFFFF;
    $b = ($i * 3 + 1) | 16;
    $c = ($i * 5 + 1) ^ 255;
    $d = ($i + 1) << 2;
    $e = ($i + 3) >> 1;
}
echo $a, "|", $b, "|", $c, "|", $d, "|", $e;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "13994|6014|10227|8000|1001");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected bitop int coercion to release owned Mixed operands, got: {}",
        out.stderr
    );
}

/// Issue #500: `%` coerces both operands to int; a parenthesized compound left
/// operand must not leak its checked-arithmetic Mixed box.
#[test]
fn test_regression_500_mod_releases_checked_operand_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = 0;
for ($i = 0; $i < 3000; $i++) { $m = ($i * 7 + 1) % 65536; }
echo $m;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "20994");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected modulo int coercion to release owned Mixed operands, got: {}",
        out.stderr
    );
}

/// Issue #500 (exponentiation flavour): `**` consumes its operands through the
/// float-coercion path, so a checked-arithmetic Mixed box used as either the
/// base or the exponent must be released like the bitwise/modulo consumers.
/// Each of these loops leaked one box per evaluation before the fix.
#[test]
fn test_regression_500_pow_releases_checked_operand_boxes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
for ($i = 0; $i < 2000; $i++) { $acc = ($i % 7 + 1) ** 2; }
echo $acc, "\n";
$f = 0.0;
for ($i = 0; $i < 2000; $i++) { $f = ($i * 7 + 1) ** 0.5; }
echo ($f > 118.29 && $f < 118.30) ? "sqrt ok" : "sqrt bad", "\n";
$r = 0;
for ($i = 0; $i < 2000; $i++) { $r = 2 ** ($i % 3 + 1); }
echo $r, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "25\nsqrt ok\n4\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected exponentiation to release checked-arithmetic operand boxes (issue #500), got: {}",
        out.stderr
    );
}

/// Issue #500: an int-coerced array index built from a parenthesized compound
/// expression (`$SIN[($i * 7 + 5) & 1023]`) must release the inner Mixed box.
#[test]
fn test_regression_500_coerced_array_index_releases_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$SIN = [];
for ($j = 0; $j < 1024; $j++) { $SIN[] = $j * 2; }
$s = 0;
for ($i = 0; $i < 2000; $i++) { $s = $SIN[($i * 7 + 5) & 1023]; }
echo $s;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1372");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected coerced array index boxes to be released, got: {}",
        out.stderr
    );
}

/// Issue #500: a computed index consumed directly by an array read
/// (`$B[$i + 1]`) goes through the mixed-key read path; the owned boxed index
/// must be released after the read.
#[test]
fn test_regression_500_mixed_key_index_releases_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$B = [];
for ($j = 0; $j < 2002; $j++) { $B[] = $j * 3; }
$x = 0;
for ($i = 0; $i < 2000; $i++) { $x = $B[$i + 1]; }
echo $x;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "6000");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected mixed-key index boxes to be released after the read, got: {}",
        out.stderr
    );
}

/// Issue #500: a comparison with a parenthesized compound operand
/// (`($i * 7 + 1) < 500`) must release the original Mixed box, not just the
/// rebound post-coercion scalar.
#[test]
fn test_regression_500_comparison_releases_checked_operand_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$cnt = 0;
for ($i = 0; $i < 2000; $i++) {
    if (($i * 7 + 1) < 500) { $cnt++; }
}
echo $cnt;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "72");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected comparison int coercion to release owned Mixed operands, got: {}",
        out.stderr
    );
}

/// Issue #500: unary minus over a compound operand (`-($i * 7 + 1)`) lowers
/// through the mixed numeric sub helper and must release the owned operand box.
#[test]
fn test_regression_500_unary_minus_releases_checked_operand_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 0;
for ($i = 0; $i < 3000; $i++) { $n = -($i * 7 + 1); }
echo $n;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "-20994");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected unary minus to release its owned Mixed operand, got: {}",
        out.stderr
    );
}

/// Issue #500: nested masking (`(($i + 1) & 0xFF) & 0xF`) creates exactly one
/// checked-arithmetic box per iteration; it must be released exactly once.
#[test]
fn test_regression_500_nested_masking_releases_box_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$h = 0;
for ($i = 0; $i < 2000; $i++) { $h = (($i + 1) & 0xFF) & 0xF; }
echo $h;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected exactly one release per nested-mask iteration, got: {}",
        out.stderr
    );
}

/// Issue #500: single checked ops (`+`, `-`, `*`) as direct bitand operands
/// must each release their Mixed result box.
#[test]
fn test_regression_500_single_checked_ops_release_boxes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = 0; $q = 0; $r = 0;
for ($i = 0; $i < 2000; $i++) {
    $p = ($i + 1) & 1;
    $q = ($i - 1) & 1;
    $r = ($i * 2) & 3;
}
echo $p, "|", $q, "|", $r;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0|0|2");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected single checked-op operand boxes to be released, got: {}",
        out.stderr
    );
}

/// Issue #500 control: splitting the compound operand into a local
/// (`$v = $i * 7 + 1; $acc = $v & 0xFFFF`) routes the box through slot
/// ownership. The borrowed local load must NOT be over-released.
#[test]
fn test_regression_500_split_statement_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
for ($i = 0; $i < 3000; $i++) { $v = $i * 7 + 1; $acc = $v & 0xFFFF; }
echo $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "20994");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected borrowed local operands to stay balanced, got: {}",
        out.stderr
    );
}

/// Issue #500 control: plain scalar shapes (`$i & 0xFFFF`, `$i * 7 + 1` stored
/// straight into a local) must stay clean and produce unchanged results.
#[test]
fn test_regression_500_plain_scalar_shapes_stay_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = 0; $b = 0;
for ($i = 0; $i < 3000; $i++) { $a = $i & 0xFFFF; $b = $i * 7 + 1; }
echo $a, "|", $b;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2999|20994");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected plain scalar arithmetic to stay clean, got: {}",
        out.stderr
    );
}

/// Issue #500 control: a chained mixed addition (`($i + 1) + ($i + 2)`) already
/// releases its operand boxes; the fix must not double-release them.
#[test]
fn test_regression_500_chained_mixed_add_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$l = 0;
for ($i = 0; $i < 2000; $i++) { $l = ($i + 1) + ($i + 2); }
echo $l;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4001");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected chained mixed adds to release each operand exactly once, got: {}",
        out.stderr
    );
}

/// Issue #500 control: a chained sum of array-read values must stay clean and
/// correct (operand releases at the mixed binop sites must not double up).
#[test]
fn test_regression_500_chained_array_read_sum_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$A = [10, 20, 30];
$v = 0;
for ($i = 0; $i < 2000; $i++) { $v = $A[0] + $A[1] + $A[2]; }
echo $v;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "60");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected chained array-read sums to stay balanced, got: {}",
        out.stderr
    );
}

/// Issue #500 control: a checked-arithmetic result passed as a call argument
/// (`intdiv($i * 7 + 1, 3)`) is released by the call-argument path; the fix
/// must not add a second release.
#[test]
fn test_regression_500_call_argument_box_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$t = 0;
for ($i = 0; $i < 2000; $i++) { $t = intdiv($i * 7 + 1, 3); }
echo $t;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4664");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected call-argument boxes to be released exactly once, got: {}",
        out.stderr
    );
}

/// Issue #500 control: a checked-arithmetic result flowing into a string
/// concatenation is released by the issue-#527 string coercion path; it must
/// not be double-released.
#[test]
fn test_regression_500_string_coercion_box_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$str = "";
for ($i = 0; $i < 1000; $i++) { $str = ($i * 7 + 1) . "x"; }
echo $str;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "6994x");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected stringified boxes to be released exactly once, got: {}",
        out.stderr
    );
}

/// Issue #500 control: array literal elements built from checked-arithmetic
/// expressions exercise the element storage coercion path; the caller-side
/// release there must not double up with the coercer-internal release.
#[test]
fn test_regression_500_array_literal_element_box_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$last = 0;
for ($i = 0; $i < 1000; $i++) {
    $pair = [$i + 1, $i * 2];
    $last = $pair[0] + $pair[1];
}
echo $last;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2998");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected array literal element boxes to be released exactly once, got: {}",
        out.stderr
    );
}

/// Issue #500 control: a loop-carried accumulator (`$acc = ($acc + $i) & 0xFFFF`)
/// feeds the next iteration through its slot; the coercion release must not
/// free the live local.
#[test]
fn test_regression_500_loop_carried_accumulator_stays_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$acc = 0;
for ($i = 0; $i < 2000; $i++) { $acc = ($acc + $i) & 0xFFFF; }
echo $acc;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "32920");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the loop-carried accumulator to survive operand releases, got: {}",
        out.stderr
    );
}

/// Issue #500 control (issue #369 parity): runtime-overflowing int arithmetic
/// with non-constant operands must still promote to float. A relational
/// consumer of the overflowed box (`($a * 2) > 0`) is deliberately not
/// asserted here: the comparison path truncates the float payload through
/// `__rt_mixed_cast_int`, whose float→int conversion is architecture-divergent
/// on overflow (ARM64 `fcvtzs` saturates to INT64_MAX, x86_64 `cvttsd2si`
/// yields INT64_MIN) — a pre-existing parity gap independent of this leak fix.
/// Release-through-comparison is covered by the non-overflowing
/// `test_regression_500_comparison_releases_checked_operand_box`.
#[test]
fn test_regression_500_overflow_to_float_parity_preserved() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = $argc > 0 ? PHP_INT_MAX : 0;
$b = $argc > 0 ? 2 : 0;
$r = $a * $b;
echo gettype($r), "\n";
$s = $a + 1;
echo gettype($s), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "double\ndouble\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected overflow-promoted boxes to be read then released cleanly, got: {}",
        out.stderr
    );
}

/// Regression test: a function returning its Mixed-boxed static local must hand the
/// caller an owned reference. The checked `++` widens the static slot to a boxed
/// `int|float`, and the caller releases call results after consuming them, so an
/// unretained `return $next_id` drops the slot's own box to refcount zero — every
/// later call increments freed memory and reads garbage once the block is reused.
#[test]
fn test_returned_static_local_mixed_box_survives_caller_release() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function next_n() {
    static $n = 0;
    $n++;
    return $n;
}
echo "N: " . next_n() . "\n";
echo "N: " . next_n() . "\n";
echo "N: " . next_n() . "\n";
var_dump(next_n());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "N: 1\nN: 2\nN: 3\nint(4)\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the returned static box to stay owned by its slot, got: {}",
        out.stderr
    );
}

/// Regression test: the freed-block reuse form of the same bug. A checked-arithmetic
/// store into a global allocates between calls, reusing the static's freed box, so
/// without the return retain the second `make_id()` concat prints an empty string
/// instead of the counter value (the shape of `examples/advanced-functions`).
#[test]
fn test_returned_static_survives_interleaved_global_mixed_store() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
function add_to_total($amount) {
    global $total;
    $total = $total + $amount;
}
function make_id() {
    static $next_id = 0;
    $next_id++;
    return $next_id;
}
echo "ID: " . make_id() . "\n";
add_to_total(10);
echo "ID: " . make_id() . "\n";
echo "ID: " . make_id() . "\n";
echo "Total: " . $total . "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ID: 1\nID: 2\nID: 3\nTotal: 10\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected static-return and global checked-add stores to stay balanced, got: {}",
        out.stderr
    );
}
