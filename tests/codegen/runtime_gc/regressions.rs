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
