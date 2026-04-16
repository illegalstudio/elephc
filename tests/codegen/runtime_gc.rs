use crate::support::*;

#[test]
fn test_gc_scope_cleanup_basic() {
    // Function locals freed on return (no leak in loop)
    let out = compile_and_run(
        r#"<?php
function process() {
    $arr = [1, 2, 3];
    $map = ["a" => "b"];
    return 42;
}
for ($i = 0; $i < 1000; $i++) { process(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_gc_return_array_survives() {
    // Returned array must not be freed by epilogue decref
    let out = compile_and_run(
        r#"<?php
function make() {
    $arr = [10, 20, 30];
    return $arr;
}
$result = make();
echo $result[0] . "|" . $result[1] . "|" . $result[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

#[test]
fn test_gc_return_array_loop() {
    // Return array in tight loop must not leak
    let out = compile_and_run(
        r#"<?php
function make() { return [1, 2, 3]; }
for ($i = 0; $i < 100000; $i++) { $x = make(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_gc_return_assoc_array() {
    let out = compile_and_run(
        r#"<?php
function config() { return ["host" => "localhost", "port" => "3306"]; }
$c = config();
echo $c["host"];
"#,
    );
    assert_eq!(out, "localhost");
}

#[test]
fn test_gc_assoc_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7, 8, 9];
$map = ["nums" => $inner];
unset($inner);
$saved = $map["nums"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_gc_assoc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4, 5, 6];
$map = ["nums" => [1]];
$map["nums"] = $inner;
unset($inner);
$saved = $map["nums"];
echo $saved[2];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_gc_return_object() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
}
function make_box($n) { return new Box($n); }
$b = make_box(42);
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_gc_explode_in_function_loop() {
    // Classic leak case: explode in function called 100K times
    let out = compile_and_run(
        r#"<?php
function parse($data) {
    $parts = explode(",", $data);
    return $parts[0];
}
for ($i = 0; $i < 1000; $i++) { $r = parse("a,b,c"); }
echo $r;
"#,
    );
    assert_eq!(out, "a");
}

#[test]
fn test_gc_multiple_locals_one_returned() {
    // Multiple array locals, only one returned — others must be freed
    let out = compile_and_run(
        r#"<?php
function work() {
    $a = [1, 2];
    $b = [3, 4];
    $c = [5, 6];
    return $b;
}
$r = work();
echo $r[0] . "|" . $r[1];
"#,
    );
    assert_eq!(out, "3|4");
}

#[test]
fn test_gc_array_reassign_in_loop() {
    // Array reassignment decrefs old value (100K iterations)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 1000; $i++) {
    $parts = explode(",", "a,b,c");
}
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

#[test]
fn test_gc_nested_function_arrays() {
    // Nested function calls all creating arrays
    let out = compile_and_run(
        r#"<?php
function inner() { return [1, 2, 3]; }
function outer() {
    $tmp = [4, 5, 6];
    $result = inner();
    return $result;
}
for ($i = 0; $i < 50000; $i++) { $x = outer(); }
echo $x[0];
"#,
    );
    assert_eq!(out, "1");
}

// === Regression tests from v0.9-v0.11 bug patterns ===

// Pattern: infer_local_type misses return types for builtins/assoc access
#[test]
fn test_regression_assoc_value_in_function() {
    // AssocArray element stored in local → must allocate 16 bytes for Str
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

// Pattern: function receives assoc, iterates, accesses string values
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

// Pattern: $arr = func($arr) where func pushes to the array
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

// Pattern: function creates assoc array from parameters, caller iterates
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

// Pattern: save function iterates assoc array with 5-field concat chain
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

// Pattern: pass object to function, access string property
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

// Pattern: objects in array, iterated with method calls
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

// Pattern: switch+return inside function called multiple times
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

// Pattern: str_replace + strtolower inside a function
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

// Pattern: explode inside function, use result
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

// Pattern: function returns assoc array, caller reads multiple keys
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

// Pattern: multiple string locals from hash_get in same function
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

// Pattern: class method with string param + string property access
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

#[test]
fn test_function_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
sum9(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_instance_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class GreeterOverflow {
    public function greet($a, $b, $c, $d, $e, $f, string $message) {
        echo $message;
    }
}
$g = new GreeterOverflow();
$g->greet(1, 2, 3, 4, 5, 6, "hello");
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_constructor_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class ConstructorOverflow {
    public $message;
    public function __construct($a, $b, $c, $d, $e, $f, string $message) {
        $this->message = $message;
    }
}
$value = new ConstructorOverflow(1, 2, 3, 4, 5, 6, "stack");
echo $value->message;
"#,
    );
    assert_eq!(out, "stack");
}

#[test]
fn test_static_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class StaticOverflow {
    public static function pick($a, $b, $c, $d, $e, $f, $g, $h) {
        echo $h;
    }
}
StaticOverflow::pick(1, 2, 3, 4, 5, 6, 7, 8);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_callable_variable_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
$fn = sum9(...);
$fn(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_by_ref_argument_supports_large_stack_offsets() {
    let mut source = String::from("<?php\nfunction bump(&$value) { $value = $value + 1; }\nfunction large_frame() {\n");
    for i in 0..520 {
        source.push_str(&format!("    $slot{} = {};\n", i, i));
    }
    source.push_str("    bump($slot519);\n    echo $slot519;\n}\nlarge_frame();\n");

    let out = compile_and_run(&source);
    assert_eq!(out, "520");
}

#[test]
fn test_float_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9f(float $a, float $b, float $c, float $d, float $e, float $f, float $g, float $h, float $i) {
    echo (int) ($a + $b + $c + $d + $e + $f + $g + $h + $i);
}
sum9f(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_call_user_func_array_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
call_user_func_array("sum9", [1, 2, 3, 4, 5, 6, 7, 8, 9]);
"#,
    );
    assert_eq!(out, "45");
}

// Pattern: static method with string params
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

// Pattern: chained property access $a->b->c
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

// Pattern: float property in class
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
