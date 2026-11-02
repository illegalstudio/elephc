//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables state and variadics, including global read, global write, and global read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Global variables ---

/// Verifies that a `global $var` declaration inside a function reads the correct global value.
#[test]
fn test_global_read() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
}
test();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a `global $var` declaration inside a function can write to a global variable.
#[test]
fn test_global_write() {
    let out = compile_and_run(
        r#"<?php
$y = 5;
function modify() {
    global $y;
    $y = 99;
}
modify();
echo $y;
"#,
    );
    assert_eq!(out, "99");
}

/// Verifies that a `global $var` declaration allows both reading and writing the global variable.
#[test]
fn test_global_read_write() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
    $x = 20;
}
test();
echo $x;
"#,
    );
    assert_eq!(out, "1020");
}

/// Verifies that multiple comma-separated global variables can be declared in one statement.
#[test]
fn test_global_multiple_vars() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = 2;
function sum() {
    global $a, $b;
    echo $a + $b;
}
sum();
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that global variables persist and are correctly mutated across multiple function calls.
#[test]
fn test_global_increment() {
    let out = compile_and_run(
        r#"<?php
$counter = 0;
function inc() {
    global $counter;
    $counter++;
}
inc();
inc();
inc();
echo $counter;
"#,
    );
    assert_eq!(out, "3");
}

// --- Static variables ---

/// Verifies that a static variable inside a function increments across multiple invocations.
#[test]
fn test_static_counter() {
    let out = compile_and_run(
        r#"<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    assert_eq!(out, "123");
}

/// Verifies that a static variable declared without an initializer defaults to null.
#[test]
fn test_static_without_initializer_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
function f() {
    static $x;
    var_dump($x);
}
f();
f();
"#,
    );
    assert_eq!(out, "NULL\nNULL\n");
}

/// Verifies that `static $x;` behaves identically to the explicit `static $x = null;` form.
#[test]
fn test_static_without_initializer_matches_explicit_null_form() {
    let implicit = compile_and_run(
        r#"<?php
function counter() {
    static $n;
    if ($n === null) {
        $n = 0;
    }
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    let explicit = compile_and_run(
        r#"<?php
function counter() {
    static $n = null;
    if ($n === null) {
        $n = 0;
    }
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    assert_eq!(implicit, explicit);
}

/// Verifies untyped static locals preserve null and later heap-backed value representations.
#[test]
fn test_static_local_boxes_values_that_change_representation() {
    let out = compile_and_run(
        r#"<?php
class StaticBox {
    public string $value;

    public function __construct(string $value) {
        $this->value = $value;
    }
}

function cachedCallable(): callable {
    static $callback;
    return $callback ??= static fn (): string => 'called';
}

function cachedObject(): StaticBox {
    static $object = null;
    if ($object === null) {
        $object = new StaticBox('object');
    }
    return $object;
}

function cachedArray(): mixed {
    static $values = [];
    if (!$values) {
        $values = [1, 'two'];
    }
    return $values;
}

$values = cachedArray();
echo (cachedCallable())(), ':', cachedObject()->value, ':', $values[0], ':', $values[1];
"#,
    );
    assert_eq!(out, "called:object:1:two");
}

/// Verifies a persistent associative static slot accepts a more precise string-keyed initializer
/// while retaining the generic associative-array storage representation across calls.
#[test]
fn test_static_local_accepts_precise_assoc_initializer() {
    let out = compile_and_run(
        r#"<?php
function offsetFor(string $name): int {
    static $offsets = ["left" => 3, "right" => 7];
    return $offsets[$name];
}

echo offsetFor("left"), ":", offsetFor("right"), ":", offsetFor("left");
"#,
    );
    assert_eq!(out, "3:7:3");
}

/// Verifies that a static variable inside a closure links and persists across calls.
#[test]
fn test_closure_static_local_preserves_value_across_calls() {
    let out = compile_and_run(
        r#"<?php
$f = function () {
    static $x = 0;
    echo ++$x;
};
$f();
$f();
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that a static variable's value is preserved and updated correctly across calls.
#[test]
fn test_static_preserves_value() {
    let out = compile_and_run(
        r#"<?php
function acc() {
    static $total = 0;
    $total = $total + 10;
    return $total;
}
echo acc();
echo acc();
echo acc();
"#,
    );
    assert_eq!(out, "102030");
}

/// Verifies that two functions can each declare a static variable with the same name without interference.
#[test]
fn test_static_separate_functions() {
    let out = compile_and_run(
        r#"<?php
function a() {
    static $x = 0;
    $x++;
    echo $x;
}
function b() {
    static $x = 0;
    $x = $x + 10;
    echo $x;
}
a();
b();
a();
b();
"#,
    );
    assert_eq!(out, "110220");
}

// --- Pass by reference ---

/// Verifies that a `&$var` parameter increments the caller's variable in place.
#[test]
fn test_ref_increment() {
    let out = compile_and_run(
        r#"<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x;
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies that a `&$var` parameter can be assigned a new value and the caller sees the change.
#[test]
fn test_ref_assign() {
    let out = compile_and_run(
        r#"<?php
function set_value(&$v, $new_val) {
    $v = $new_val;
}
$x = 1;
set_value($x, 42);
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies direct reference assignment aliases reads from the source variable.
#[test]
fn test_reference_assignment_alias_reads_source() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
echo $b;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies writes through a directly aliased variable update the original source.
#[test]
fn test_reference_assignment_alias_writes_through() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$b = 42;
echo $a;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies writes to the original source remain visible through the alias.
#[test]
fn test_reference_assignment_source_write_visible_through_alias() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$a = 2;
echo $b;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies that a two-argument `&$a, &$b` swap function correctly swaps the caller's values.
#[test]
fn test_ref_swap() {
    let out = compile_and_run(
        r#"<?php
function swap(&$a, &$b) {
    $tmp = $a;
    $a = $b;
    $b = $tmp;
}
$p = 1;
$q = 2;
swap($p, $q);
echo $p . $q;
"#,
    );
    assert_eq!(out, "21");
}

/// Verifies that a `&$target` parameter with a regular by-value parameter works correctly.
#[test]
fn test_ref_mixed_params() {
    let out = compile_and_run(
        r#"<?php
function add_to(&$target, $amount) {
    $target = $target + $amount;
}
$x = 10;
add_to($x, 5);
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies by-reference variadic function and method element assignments mutate caller variables.
#[test]
fn test_by_ref_variadic_function_and_method_element_writeback() {
    let out = compile_and_run(
        r#"<?php
function f(&...$items) {
    $items[0] = $items[0] . "-f";
    $items[1] = $items[1] . "-g";
}
class C {
    public function m(&...$items) {
        $items[0] = $items[0] . "-m";
        $items[1] = $items[1] . "-n";
    }
}
$a = "A";
$b = "B";
f($a, $b);
echo $a . ":" . $b . "|";
$c = "C";
$d = "D";
(new C())->m($c, $d);
echo $c . ":" . $d;
"#,
    );
    assert_eq!(out, "A-f:B-g|C-m:D-n");
}

/// By-reference variadic markers widen scalar caller locals before a positional or named write.
#[test]
fn test_by_ref_variadic_scalar_and_named_tail_writeback_use_boxed_storage() {
    let out = compile_and_run(
        r#"<?php
function replace(&...$items): void {
    $items[0] = "position";
    $items["named"] = "name";
}
function replaceWithCount(&...$items): void {
    func_num_args();
    $items[0] = "counted";
}
function replaceBoth(&...$items): void {
    $items[0] = "first";
    $items[1] = "second";
}
$position = 1;
$named = 2;
replace($position, named: $named);
echo $position . ":" . $named . "|";
$callable = replace(...);
$callablePosition = 3;
$callableNamed = 4;
$callable($callablePosition, named: $callableNamed);
echo $callablePosition . ":" . $callableNamed . "|";
$cuf = 5;
call_user_func($callable, $cuf);
echo $cuf . "|";
$counted = 6;
replaceWithCount($counted);
echo $counted . "|";
$repeated = 7;
replaceBoth($repeated, $repeated);
echo $repeated;
"#,
    );
    assert_eq!(out, "position:name|position:name|position|counted|second");
}

// --- Issue #1062: a by-reference variadic may retype the caller's variable -----------------
//
// `f(&...$items)` is the one by-reference form PHP's checker does not constrain: the callee
// writes through `$items[n]` with no declared type, so it may store a value of ANY type into
// a caller local that currently holds something else. The marker the caller pushes into the
// invoker argument array therefore has to point at storage wide enough for whatever comes
// back. Pointing it at the local's concrete storage is silent corruption rather than a
// diagnostic: a string written over an `I64` slot reads back as the string's pointer.
//
// The existing `test_by_ref_variadic_function_and_method_element_writeback` above covers the
// same-type case, which never exercises the widening. Each test here changes the type.

/// The archetypal shape from the issue: an `int` local replaced by a string.
#[test]
fn test_by_ref_variadic_write_retypes_an_int_local_to_a_string() {
    let out = compile_and_run(
        r#"<?php
function replace(&...$items): void { $items[0] = "position"; }
$p = 1;
replace($p);
var_dump($p);
"#,
    );
    assert_eq!(out, "string(8) \"position\"\n");
}

/// Every scalar and the array case in one call, so a fix that only widens far enough for
/// pointers still fails on `float` and `bool`, which are stored inline.
#[test]
fn test_by_ref_variadic_write_retypes_int_locals_to_every_value_kind() {
    let out = compile_and_run(
        r#"<?php
function retype(&...$items): void {
    $items[0] = "s";
    $items[1] = 2.5;
    $items[2] = true;
    $items[3] = [7, 8];
}
$a = 1;
$b = 1;
$c = 1;
$d = 1;
retype($a, $b, $c, $d);
var_dump($a, $b, $c, $d);
"#,
    );
    assert_eq!(
        out,
        "string(1) \"s\"\nfloat(2.5)\nbool(true)\narray(2) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(8)\n}\n"
    );
}

/// The other direction: a string local narrowed to an `int`. Widening the slot is not enough
/// on its own -- the read back out of it has to go through the box too, or the integer is
/// returned as an empty string.
#[test]
fn test_by_ref_variadic_write_retypes_a_string_local_to_an_int() {
    let out = compile_and_run(
        r#"<?php
function narrow(&...$items): void { $items[0] = 42; }
$s = "text";
narrow($s);
var_dump($s);
"#,
    );
    assert_eq!(out, "int(42)\n");
}

/// A method and a closure reach the same marker through different callers.
#[test]
fn test_by_ref_variadic_retype_through_a_method_and_a_closure() {
    let out = compile_and_run(
        r#"<?php
class Store { public function put(&...$items): void { $items[0] = "kept"; } }
$p = 7;
(new Store())->put($p);
var_dump($p);
$q = 3;
$make = function (&...$items): void { $items[0] = "closed"; };
$make($q);
var_dump($q);
"#,
    );
    assert_eq!(out, "string(4) \"kept\"\nstring(6) \"closed\"\n");
}

/// The first-class-callable form, and the issue's own loop: fifty calls that each retype a
/// fresh `int` local, so a fix that works once but corrupts the slot for the next iteration
/// reports a count below fifty rather than passing.
#[test]
fn test_by_ref_variadic_retype_through_a_first_class_callable_and_a_loop() {
    let out = compile_and_run(
        r#"<?php
function replace(&...$items): void { $items[0] = "position"; }
$c = replace(...);
$p = 1;
$c($p);
var_dump($p);
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $v = $i;
    replace($v);
    if ($v === "position") { $n++; }
}
echo $n, "\n";
"#,
    );
    assert_eq!(out, "string(8) \"position\"\n50\n");
}

// --- Variadic functions ---

// --- Variadic functions ---

/// Verifies a variadic function collects exactly three positional arguments into the rest array.
#[test]
fn test_variadic_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies a variadic function collects exactly five positional arguments into the rest array.
#[test]
fn test_variadic_five_args() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3, 4, 5);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies that a variadic function can be called multiple times with different argument counts without interference.
#[test]
fn test_variadic_multiple_calls_same_function() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
echo ":";
echo sum(10, 20, 30, 40, 50);
"#,
    );
    assert_eq!(out, "6:150");
}

/// Verifies that a variadic function called with no arguments receives an empty rest array.
#[test]
fn test_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that a variadic parameter follows regular positional parameters and collects remaining arguments.
#[test]
fn test_variadic_with_regular_params() {
    let out = compile_and_run(
        r#"<?php
function greet($greeting, ...$names) {
    foreach ($names as $name) {
        echo $greeting . " " . $name . "\n";
    }
}
greet("Hello", "Alice", "Bob");
"#,
    );
    assert_eq!(out, "Hello Alice\nHello Bob\n");
}

/// Verifies that `count()` works correctly on a variadic rest array with four elements.
#[test]
fn test_variadic_count() {
    let out = compile_and_run(
        r#"<?php
function num_args(...$args) {
    return count($args);
}
echo num_args(10, 20, 30, 40);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that a variadic function returning the rest array allows accessing the single wrapped element.
#[test]
fn test_variadic_single_arg() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    return $items;
}
$arr = wrap(42);
echo $arr[0];
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a nested array passed to a variadic function preserves its element tag through json_encode.
#[test]
fn test_variadic_array_arg_preserves_runtime_element_tag() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    echo json_encode($items);
}
wrap([1, 2]);
"#,
    );
    assert_eq!(out, "[[1,2]]");
}

// --- Spread operator ---

/// Verifies that an array spread `...$args` in a function call unpacks correctly into a variadic callee.
#[test]
fn test_spread_in_function_call() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
$args = [10, 20, 30];
echo sum(...$args);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies that an array spread into a function with regular and variadic params fills regular params first and collects the remainder into the rest array.
#[test]
fn test_spread_in_variadic_function_fills_regular_params_first() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $value) {
        echo $value . ";";
    }
}
show(...[1, 2, 3]);
"#,
    );
    assert_eq!(out, "head=1;2;3;");
}

/// Verifies that two spread arrays in an array literal `[...$a, ...$b]` produce a flattened array of four elements.
#[test]
fn test_spread_in_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
echo count($c);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that two spread arrays in an array literal produce a flattened array whose elements iterate in correct order.
#[test]
fn test_spread_array_values() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "1234");
}

/// Verifies that array spreads can be interleaved with literal elements in an array literal.
#[test]
fn test_spread_mixed_with_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [5, 6];
$c = [...$a, 3, 4, ...$b];
echo count($c);
echo " ";
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "6 123456");
}

/// Verifies that a single-array spread `[...$a]` produces an array equal in length to the source.
#[test]
fn test_spread_single_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = [...$a];
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies gradual indexed-array values are checked and unboxed before argument unpacking.
#[test]
fn test_gradual_array_spread_into_dynamic_method_call() {
    let out = compile_and_run(
        r#"<?php
function gradual_pair(bool $present): mixed {
    return $present ? ['left', 'right'] : null;
}

class GradualSpreadReceiver {
    public function join(string $left, string $right): string {
        return $left . ':' . $right;
    }
}

$receiver = new GradualSpreadReceiver();
$method = 'join';
echo $receiver->{$method}(...gradual_pair(true));
"#,
    );
    assert_eq!(out, "left:right");
}

/// Verifies a gradual argument container preserves its runtime keys for a known method call.
#[test]
fn test_gradual_array_spread_into_known_method_call() {
    let out = compile_and_run(
        r#"<?php
function gradual_named_pair(bool $present): mixed {
    return $present ? ['right' => 'R', 'left' => 'L'] : null;
}

class KnownSpreadReceiver {
    public function join(string $left, string $right): string {
        return $left . ':' . $right;
    }
}

$receiver = new KnownSpreadReceiver();
echo $receiver->join(...gradual_named_pair(true));
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies a fixed-ABI parent call expands the caller's runtime argument array.
#[test]
fn test_func_get_args_spread_into_parent_method_call() {
    let out = compile_and_run(
        r#"<?php
class SpreadParent {
    public function join(string $first, ?string $second = null, bool $upper = false): string {
        $value = $first . ':' . ($second ?? 'none');
        return $upper ? strtoupper($value) : $value;
    }
}

class SpreadChild extends SpreadParent {
    public function join(string $first, ?string $second = null, bool $upper = false): string {
        $args = func_get_args();
        return parent::join(...$args);
    }
}

echo (new SpreadChild())->join('left', 'right', true);
"#,
    );
    assert_eq!(out, "LEFT:RIGHT");
}

/// Verifies that a parent import reached through `func_get_args()` retains a private inherited
/// directory property when the same loader is held by a typed configurator property.
#[test]
fn test_parent_spread_import_retains_inherited_private_loader_directory() {
    let out = compile_and_run(
        r#"<?php
class BaseLoader {
    private ?string $currentDir = null;

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        return $this->doImport($resource, $type, $ignoreErrors, $source, $exclude);
    }

    private function doImport(mixed $resource, ?string $type, bool|string $ignoreErrors, ?string $source, mixed $exclude): string {
        return $this->currentDir . ':' . $resource;
    }
}

class ChildLoader extends BaseLoader {
    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }
}

class Configurator {
    public function __construct(private ChildLoader $loader, private string $path) {}

    public function import(string $resource): string {
        $this->loader->setCurrentDir(dirname($this->path));

        return $this->loader->import($resource, null, false, $this->path);
    }
}

echo (new Configurator(new ChildLoader(), '/project/Kernel/Bundle.php'))->import('Resources/config/services.php');
"#,
    );
    assert_eq!(out, "/project/Kernel:Resources/config/services.php");
}

/// Verifies that interface resolution and an `instanceof` narrowing preserve the selected
/// loader instance before a typed configurator forwards a parent spread import.
#[test]
fn test_resolved_loader_identity_survives_instanceof_before_parent_spread_import() {
    let out = compile_and_run(
        r#"<?php
interface ResolvedLoaderInterface {}

interface ResolvedLoaderResolverInterface {
    public function resolve(string $file): ResolvedLoaderInterface;
}

class ResolvedBaseLoader {
    private ?string $currentDir = null;

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        return $this->currentDir . ':' . $resource;
    }
}

class ResolvedChildLoader extends ResolvedBaseLoader implements ResolvedLoaderInterface {
    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }
}

class ResolvedLoaderResolver implements ResolvedLoaderResolverInterface {
    public function __construct(private ResolvedChildLoader $loader) {}

    public function resolve(string $file): ResolvedLoaderInterface {
        return $this->loader;
    }
}

class ResolvedConfigurator {
    public function __construct(private ResolvedChildLoader $loader, private string $path) {}

    public function import(string $resource): string {
        $this->loader->setCurrentDir(dirname($this->path));

        return $this->loader->import($resource, null, false, $this->path);
    }
}

class ResolvedBootstrap {
    public function __construct(private ResolvedLoaderResolverInterface $resolver) {}

    public function load(string $file): string {
        $loader = $this->resolver->resolve($file);
        if (!$loader instanceof ResolvedChildLoader) {
            return 'unexpected-loader';
        }
        $loader->setCurrentDir(dirname($file));

        return (new ResolvedConfigurator($loader, $file))->import('Resources/config/services.php');
    }
}

$loader = new ResolvedChildLoader();
echo (new ResolvedBootstrap(new ResolvedLoaderResolver($loader)))->load('/project/Kernel/Bundle.php');
"#,
    );
    assert_eq!(out, "/project/Kernel:Resources/config/services.php");
}

/// Verifies a closure argument receives the newly configured loader object without substituting
/// another resolver-owned instance before it forwards a parent spread import.
#[test]
fn test_closure_callback_preserves_resolved_loader_for_parent_spread_import() {
    let out = compile_and_run(
        r#"<?php
interface CallbackLoaderInterface {}

interface CallbackLoaderResolverInterface {
    public function resolve(string $file): CallbackLoaderInterface;
}

class CallbackBaseLoader {
    private ?string $currentDir = null;

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        return $this->currentDir . ':' . $resource;
    }
}

class CallbackChildLoader extends CallbackBaseLoader implements CallbackLoaderInterface {
    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }
}

class CallbackLoaderResolver implements CallbackLoaderResolverInterface {
    public function __construct(private CallbackChildLoader $loader) {}

    public function resolve(string $file): CallbackLoaderInterface {
        return $this->loader;
    }
}

class CallbackConfigurator {
    public function __construct(private CallbackChildLoader $loader, private string $path) {}

    public function import(string $resource): string {
        $this->loader->setCurrentDir(dirname($this->path));

        return $this->loader->import($resource, null, false, $this->path);
    }
}

class CallbackBootstrap {
    public function __construct(private CallbackLoaderResolverInterface $resolver) {}

    public function load(Closure $callback, string $file): string {
        $loader = $this->resolver->resolve($file);
        if (!$loader instanceof CallbackChildLoader) {
            return 'unexpected-loader';
        }
        $loader->setCurrentDir(dirname($file));

        return $callback(new CallbackConfigurator($loader, $file));
    }
}

$loader = new CallbackChildLoader();
$callback = static fn (CallbackConfigurator $configurator): string => $configurator->import('Resources/config/services.php');
echo (new CallbackBootstrap(new CallbackLoaderResolver($loader)))->load($callback, '/project/Kernel/Bundle.php');
"#,
    );
    assert_eq!(out, "/project/Kernel:Resources/config/services.php");
}

/// Verifies captured closure invocation preserves its freshly constructed configurator argument
/// when the closure also captures an array, an object, and its owning object instance.
#[test]
fn test_captured_closure_keeps_configured_loader_for_parent_spread_import() {
    let out = compile_and_run(
        r#"<?php
interface CapturedLoaderInterface {}

interface CapturedLoaderResolverInterface {
    public function resolve(string $file): CapturedLoaderInterface;
}

class CapturedContainer {}

class CapturedBaseLoader {
    private ?string $currentDir = null;

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        return $this->currentDir . ':' . $resource;
    }
}

class CapturedChildLoader extends CapturedBaseLoader implements CapturedLoaderInterface {
    protected array $instanceof = [];

    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $source = null, mixed $exclude = null): string {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }
}

class CapturedLoaderResolver implements CapturedLoaderResolverInterface {
    public function __construct(private CapturedChildLoader $loader) {}

    public function resolve(string $file): CapturedLoaderInterface {
        return $this->loader;
    }
}

class CapturedConfigurator {
    private array $instanceof;

    public function __construct(
        private CapturedContainer $container,
        private CapturedChildLoader $loader,
        array &$instanceof,
        private string $path,
        private string $file,
        private ?string $env = null,
    ) {
        $this->instanceof = &$instanceof;
    }

    public function import(string $resource): string {
        $this->loader->setCurrentDir(dirname($this->path));

        return $this->loader->import($resource, null, false, $this->file);
    }
}

interface CapturedSubjectInterface {
    public function loadExtension(array $config, CapturedConfigurator $configurator, CapturedContainer $container): void;
}

class CapturedSubject implements CapturedSubjectInterface {
    public function loadExtension(array $config, CapturedConfigurator $configurator, CapturedContainer $container): void {
        echo $configurator->import('Resources/config/services.php');
    }
}

class CapturedBootstrap {
    private array $instanceof = [];

    public function __construct(
        private CapturedLoaderResolverInterface $resolver,
        private CapturedSubjectInterface $subject,
    ) {}

    public function load(string $file): void {
        $config = ['environment' => 'dev'];
        $container = new CapturedContainer();
        $callback = function (CapturedConfigurator $configurator) use ($config, $container): void {
            $this->subject->loadExtension($config, $configurator, $container);
        };

        $this->execute($callback, $container, $file);
    }

    private function execute(Closure $callback, CapturedContainer $container, string $file): void {
        $loader = $this->resolver->resolve($file);
        if (!$loader instanceof CapturedChildLoader) {
            echo 'unexpected-loader';

            return;
        }
        $loader->setCurrentDir(dirname($file));
        $instanceof = &\Closure::bind(fn &() => $this->instanceof, $loader, $loader)();

        try {
            $callback(new CapturedConfigurator($container, $loader, $instanceof, $file, $file));
        } finally {
            $instanceof = [];
        }
    }
}

$first = new CapturedBootstrap(new CapturedLoaderResolver(new CapturedChildLoader()), new CapturedSubject());
$second = new CapturedBootstrap(new CapturedLoaderResolver(new CapturedChildLoader()), new CapturedSubject());
$first->load('/first/Kernel/Bundle.php');
echo '|';
$second->load('/second/Kernel/Bundle.php');
"#,
    );
    assert_eq!(
        out,
        "/first/Kernel:Resources/config/services.php|/second/Kernel:Resources/config/services.php"
    );
}

/// Verifies that `instanceof self` recognises a child loader returned through an interface before
/// a parent spread import applies its configured relative-resource directory.
#[test]
fn test_parent_import_instanceof_self_accepts_interface_resolved_child_loader() {
    let out = compile_and_run(
        r#"<?php
interface InterfaceResolvedLoader {}

interface InterfaceResolvedLoaderResolver {
    public function resolve(mixed $resource): InterfaceResolvedLoader;
}

class InterfaceResolvedParentLoader {
    private ?string $currentDir = null;
    private InterfaceResolvedLoaderResolver $resolver;

    public function setResolver(InterfaceResolvedLoaderResolver $resolver): void {
        $this->resolver = $resolver;
    }

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function resolve(mixed $resource, ?string $type = null): InterfaceResolvedLoader {
        return $this->resolver->resolve($resource);
    }

    public function import(mixed $resource, ?string $type = null, bool $ignoreErrors = false, ?string $sourceResource = null, string|array|null $exclude = null): string {
        return $this->doImport($resource, $type);
    }

    private function doImport(mixed $resource, ?string $type): string {
        $loader = $this->resolve($resource, $type);
        if (!$loader instanceof self) {
            return 'not-self';
        }
        if (null !== $this->currentDir) {
            $resource = $this->currentDir . '/' . $resource;
        }

        return $loader->load($resource, $type);
    }

    public function load(mixed $resource, ?string $type = null): string {
        return $resource;
    }
}

class InterfaceResolvedIntermediateLoader extends InterfaceResolvedParentLoader {}

class InterfaceResolvedChildLoader extends InterfaceResolvedIntermediateLoader implements InterfaceResolvedLoader {
    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $sourceResource = null, $exclude = null): string {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }
}

class InterfaceResolvedResolver implements InterfaceResolvedLoaderResolver {
    public function __construct(private InterfaceResolvedLoader $loader) {}

    public function resolve(mixed $resource): InterfaceResolvedLoader {
        return $this->loader;
    }
}

$loader = new InterfaceResolvedChildLoader();
$loader->setResolver(new InterfaceResolvedResolver($loader));
$loader->setCurrentDir('/project/Kernel');
echo $loader->import('Resources/config/services.php', null, false, '/project/Kernel/Bundle.php', null);
"#,
    );
    assert_eq!(out, "/project/Kernel/Resources/config/services.php");
}

/// Verifies interface dispatch selects `supports()` rather than `load()` while a parent private
/// import helper resolves a descendant loader and forwards its complete runtime argument list.
#[test]
fn test_interface_loader_supports_dispatch_keeps_parent_relative_import_path() {
    let out = compile_and_run(
        r#"<?php
interface DispatchLoaderInterface {
    public function load(mixed $resource, ?string $type = null): mixed;
    public function supports(mixed $resource, ?string $type = null): bool;
}

class DispatchLoaderResolver {
    public function __construct(private DispatchLoaderInterface $loader) {}

    public function resolve(mixed $resource, ?string $type = null): DispatchLoaderInterface|false {
        if ($this->loader->supports($resource, $type)) {
            return $this->loader;
        }

        return false;
    }
}

abstract class DispatchParentLoader implements DispatchLoaderInterface {
    private DispatchLoaderResolver $resolver;
    private string $currentDir = '';

    public function setResolver(DispatchLoaderResolver $resolver): void {
        $this->resolver = $resolver;
    }

    public function setCurrentDir(string $directory): void {
        $this->currentDir = $directory;
    }

    public function resolve(mixed $resource, ?string $type = null): DispatchLoaderInterface|false {
        if ($this->supports($resource, $type)) {
            return $this;
        }

        return $this->resolver->resolve($resource, $type);
    }

    public function import(mixed $resource, ?string $type = null, bool $ignoreErrors = false, ?string $sourceResource = null, string|array|null $exclude = null): mixed {
        return $this->doImport($resource, $type, $ignoreErrors, $sourceResource, $exclude);
    }

    private function doImport(mixed $resource, ?string $type, bool $ignoreErrors, ?string $sourceResource, string|array|null $exclude): mixed {
        $loader = $this->resolve($resource, $type);
        if (false === $loader) {
            return 'not-resolved';
        }
        if ('' !== $this->currentDir) {
            $resource = $this->currentDir . '/' . $resource;
        }

        return $loader->load($resource, $type);
    }
}

class DispatchChildLoader extends DispatchParentLoader {
    public function import(mixed $resource, ?string $type = null, bool|string $ignoreErrors = false, ?string $sourceResource = null, $exclude = null): mixed {
        $arguments = func_get_args();

        return parent::import(...$arguments);
    }

    public function load(mixed $resource, ?string $type = null): mixed {
        echo 'load:' . $resource . ';';

        return 'loaded';
    }

    public function supports(mixed $resource, ?string $type = null): bool {
        echo 'supports:' . $resource . ';';

        return true;
    }
}

$loader = new DispatchChildLoader();
$loader->setResolver(new DispatchLoaderResolver($loader));
$loader->setCurrentDir('/project/Kernel');
echo $loader->import('Resources/config/services.php', null, false, '/project/Kernel/Bundle.php', null);
"#,
    );
    assert_eq!(
        out,
        "supports:Resources/config/services.php;load:/project/Kernel/Resources/config/services.php;loaded"
    );
}

/// Regression for #354: spread of an associative array into a new array literal flattens its
/// string-keyed entries instead of inserting the source as a single nested value.
#[test]
fn test_spread_assoc_array() {
    let out = compile_and_run(r#"<?php
$a = ['x' => 1];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[x:1]");
}

/// Regression for #354: spread reindexes integer-keyed source entries to fresh sequential keys
/// (matching PHP) while preserving string keys.
#[test]
fn test_spread_mixed_keys() {
    let out = compile_and_run(r#"<?php
$a = [10 => 'a', 'x' => 'b'];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:a][x:b]");
}

/// Verifies array unpack accepts Traversable sources, reindexing integer keys and preserving strings.
#[test]
fn test_spread_iterable_mixed_keys() {
    let out = compile_and_run(r#"<?php
function values(): iterable {
    yield 7 => 'a';
    yield 'x' => 'b';
    yield 3 => 'c';
}
$spread = ['z', ...values()];
foreach ($spread as $key => $value) { echo '[' . $key . ':' . $value . ']'; }
"#);
    assert_eq!(out, "[0:z][1:a][x:b][2:c]");
}

/// Regression for #354: later spread operands overwrite earlier ones on string-key collision.
#[test]
fn test_spread_overwrite() {
    let out = compile_and_run(r#"<?php
$a = ['x' => 1, 'y' => 2];
$b = ['y' => 3, 'z' => 4];
$c = [...$a, ...$b];
foreach ($c as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[x:1][y:3][z:4]");
}

/// Regression for #354: spread of an indexed array into a new array literal stays on the indexed
/// storage path and preserves sequential integer keys.
#[test]
fn test_spread_indexed_array() {
    let out = compile_and_run(r#"<?php
$a = [1, 2, 3];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:1][1:2][2:3]");
}

/// Regression for #354: a literal element before and after an associative spread continues the
/// automatic integer key sequence across the reindexed spread entries.
#[test]
fn test_spread_literal_interleaved_with_assoc() {
    let out = compile_and_run(r#"<?php
$a = ['y' => 1];
$b = ['w', ...$a, 'x'];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:w][y:1][1:x]");
}

/// Regression for #354: an indexed spread followed by an associative spread continues the
/// reindex counter across operands.
#[test]
fn test_spread_indexed_then_assoc() {
    let out = compile_and_run(r#"<?php
$a = [10, 20];
$b = ['x' => 1];
$c = [...$a, ...$b];
foreach ($c as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:10][1:20][x:1]");
}

/// Verifies that a variadic function with a preceding regular parameter receives zero rest elements when called with exactly one argument.
#[test]
fn test_variadic_with_regular_and_no_extra() {
    let out = compile_and_run(
        r#"<?php
function prefix($pre, ...$items) {
    echo count($items);
}
prefix("x");
"#,
    );
    assert_eq!(out, "0");
}

// --- Typed variadics ---

/// Verifies a typed `int ...$nums` free-function variadic sums its arguments; the element type
/// is inferred from the passed integers.
#[test]
fn test_typed_variadic_int_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$nums): int { return array_sum($nums); }
echo sum(1, 2, 3, 4);
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a typed method variadic preserves its integer element type through `foreach`.
#[test]
fn test_typed_method_variadic_foreach_keeps_element_type() {
    let out = compile_and_run(
        r#"<?php
class VariadicCodePointProbe {
    public static function fold(int ...$codes): int {
        $sum = 0;
        foreach ($codes as $code) {
            $code %= 100;
            $sum += $code;
        }
        return $sum;
    }
}
echo VariadicCodePointProbe::fold(101, 202, 303);
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies a typed `string ...$parts` variadic joins its string arguments.
#[test]
fn test_typed_variadic_string_join() {
    let out = compile_and_run(
        r#"<?php
function join_em(string ...$parts): string { return implode("-", $parts); }
echo join_em("a", "b", "c");
"#,
    );
    assert_eq!(out, "a-b-c");
}

/// Verifies a typed variadic following a regular parameter collects only the trailing arguments.
#[test]
fn test_typed_variadic_after_regular_param() {
    let out = compile_and_run(
        r#"<?php
function tag(string $t, string ...$items): string { return $t . ":" . implode(",", $items); }
echo tag("x", "a", "b");
"#,
    );
    assert_eq!(out, "x:a,b");
}

/// Verifies a typed variadic accepts zero trailing arguments.
#[test]
fn test_typed_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$nums): int { return array_sum($nums); }
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a typed variadic on an instance method collects its arguments (counted to avoid the
/// pre-existing array_sum-over-mixed-array backend gap that affects all method/closure variadics).
#[test]
fn test_typed_variadic_method() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public function count_args(int ...$ns): int { return count($ns); }
}
echo (new Calc())->count_args(10, 20, 30);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies an explicitly `mixed` method variadic remains heterogeneous across call sites
/// instead of being permanently specialized to the first call's element type.
#[test]
fn test_mixed_variadic_method_does_not_specialize_between_calls() {
    let out = compile_and_run(
        r#"<?php
class MixedVariadicCollector {
    public function count_args(mixed ...$args): int { return count($args); }
}
$collector = new MixedVariadicCollector();
echo $collector->count_args("first") . ":" . $collector->count_args([1, 2], 3.5);
"#,
    );
    assert_eq!(out, "1:2");
}

/// Verifies associative-array COW cloning retains receiver-bound callable
/// descriptors. Later insertions must not free descriptors already stored under
/// earlier keys when their source locals leave scope.
#[test]
fn test_assoc_array_cow_clone_retains_callable_descriptors() {
    let out = compile_and_run(
        r#"<?php
class CallableHashTarget {
    public function twice($value) { return $value * 2; }
    public function triple($value) { return $value * 3; }
    public function quadruple($value) { return $value * 4; }
}

$target = new CallableHashTarget();
$twice = $target->twice(...);
$callbacks = ["twice" => $twice];
$triple = $target->triple(...);
$callbacks["triple"] = $triple;
$quadruple = $target->quadruple(...);
$callbacks["quadruple"] = $quadruple;
unset($twice, $triple, $quadruple);
echo call_user_func_array($callbacks["twice"], [5]);
echo ":" . call_user_func_array($callbacks["triple"], [5]);
echo ":" . call_user_func_array($callbacks["quadruple"], [5]);
"#,
    );
    assert_eq!(out, "10:15:20");
}

/// Verifies a typed variadic on a closure collects its arguments.
#[test]
fn test_typed_variadic_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function (int ...$xs): int { return count($xs); };
echo $f(5, 6, 7);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that array unpacking into a typed variadic works (`sum(...$a)`).
#[test]
fn test_typed_variadic_spread_argument() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$n): int { return array_sum($n); }
$a = [1, 2, 3];
echo sum(...$a);
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies that a typed variadic collects correctly-typed positional arguments and runs
/// end-to-end, confirming the declared element type does not interfere with valid calls.
#[test]
fn test_typed_variadic_positional_arguments_run() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$n): int { return array_sum($n); }
echo sum(4, 5, 6);
"#,
    );
    assert_eq!(out, "15");
}

// --- First-class callables over registry builtins with variadic/optional signatures ---

/// Verifies `var_dump(...)` as a first-class callable exposes the registry's variadic
/// signature (`value, ...values`): the wrapper accepts multiple arguments and dumps
/// each independently in source order.
#[test]
fn test_first_class_callable_var_dump_variadic() {
    let out = compile_and_run(
        r#"<?php
$dump = var_dump(...);
$dump(1, "a");
"#,
    );
    assert_eq!(out, "int(1)\nstring(1) \"a\"\n");
}

/// Verifies `print_r(...)` as a first-class callable exposes the optional `$return`
/// flag from the registry signature. Through the wrapper the flag is a runtime
/// parameter, so the call takes the runtime-selected mode path and returns the
/// rendered string (boxed Mixed) without echoing.
#[test]
fn test_first_class_callable_print_r_return_flag() {
    let out = compile_and_run(
        r#"<?php
$render = print_r(...);
$r = $render("hi", true);
echo "|$r";
"#,
    );
    assert_eq!(out, "|hi");
}

/// Verifies `print_r(...)` as a first-class callable defaults the `$return` flag to
/// `false` when called with one argument: the value is echoed and the wrapper
/// returns true.
#[test]
fn test_first_class_callable_print_r_echo_default() {
    let out = compile_and_run(
        r#"<?php
$render = print_r(...);
$ok = $render(42);
echo "|";
echo $ok ? "yes" : "no";
"#,
    );
    assert_eq!(out, "42|yes");
}

/// An untyped method variadic that a first-class callable promotes to the mixed collector stays
/// mixed when a direct call later passes only strings. The direct string call used to narrow the
/// collector back to `array<string>` while the callable path still stored mixed values into it,
/// and the program aborted with "heap memory exhausted".
#[test]
fn test_promoted_method_variadic_is_not_narrowed_back_by_a_string_call() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public function is(...$patterns) {
        foreach ($patterns as $p) { echo gettype($p), ":", is_array($p) ? json_encode($p) : $p, " "; }
        return count($patterns);
    }
}
class Mw {
    public function check(Req $r) {
        $f = $r->is(...);
        return $f("api/x", [1, 2], 3);
    }
}
echo (new Req())->is('api/*'), "\n";
echo (new Mw())->check(new Req()), "\n";
"#,
    );
    assert_eq!(out, "string:api/* 1\nstring:api/x array:[1,2] integer:3 3\n");
}

/// A by-reference variadic slot holds a marker to the caller's variable, and only a value
/// boxed by the write itself was checked for one. A Mixed operand (a call's `mixed` result)
/// went straight to the runtime setter and overwrote the marker, so the caller never saw the
/// write: `$flag` stayed `false` and `$found` stayed `null`. This is the shape hosted
/// extensions use to write their optional by-reference parameters back.
#[test]
fn test_mixed_result_written_into_a_by_ref_variadic_reaches_the_caller() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function rebuild(int $n): mixed {
    return $n > 100;
}
function inc(int $n, mixed &...$rest): int {
    if (array_key_exists(0, $rest)) {
        $rest[0] = rebuild($n);
    }
    return $n + 1;
}
$flag = false;
echo inc(41, $flag), " ", var_export($flag, true), "\n";
echo inc(500, $flag), " ", var_export($flag, true), "\n";
$found = null;
echo inc(500, $found), " ", var_export($found, true), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "42 false\n501 true\n501 true\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Named arguments to a by-reference variadic travel in a hash, and the call hands the
/// callee an `array<mixed>` reference cell for it. The cell only retained a container of
/// the very same shape, so the hash went in unretained: releasing the cell and then the
/// argument freed it twice on every `f(name: $v)`, whether or not the callee wrote back.
#[test]
fn test_named_argument_into_a_by_ref_variadic_releases_its_container_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function rebuild(int $n): mixed {
    return $n > 100;
}
function inc(int $n, mixed &...$rest): int {
    if (array_key_exists("success", $rest)) {
        $rest["success"] = rebuild($n);
    }
    return $n + 1;
}
function touch_only(mixed &...$rest): int {
    return count($rest);
}
$total = 0;
for ($i = 0; $i < 20; $i++) {
    $named = false;
    $total += inc(500, success: $named) + ($named ? 1 : 0);
    $other = 1;
    $total += touch_only(x: $other);
}
echo $total, "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "10060\n");
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Verifies a callable can be narrowed against an unrelated interface without backend refusal.
#[test]
fn test_callable_parameter_supports_object_interface_narrowing() {
    let out = compile_and_run(
        r#"<?php
interface HandlerMarker {}

function selectHandler(callable $candidate): ?HandlerMarker {
    return $candidate instanceof HandlerMarker ? $candidate : null;
}

$selected = selectHandler(static function (): void {});
echo $selected === null ? "none" : "handler";
"#,
    );
    assert_eq!(out, "none");
}

/// By-value parameters that receive reference markers (`call_user_func_array($f, [&$a])`) get the
/// referenced value: a raw string, an array, a boxed object, and named entries. The conversion
/// is the shared `__rt_invoker_ref_cell_to_mixed` helper rather than per-argument inline code.
#[test]
fn test_invoker_by_value_parameters_read_reference_markers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function twice(int $x): int { return $x * 2; }
function label(string $s, $m): string { return $s . ':' . (is_array($m) ? count($m) : $m); }
function label2(string $s, $m): string { return $s . "=" . $m; }
$a = 5;
$s = "abc" . mt_rand(1, 1);
$m = [1, 2, 3];
$o = new stdClass();
$o->v = 7;
for ($i = 0; $i < 2; $i++) {
    echo call_user_func_array('twice', [&$a]), "|";
    echo call_user_func_array('label', [&$s, &$m]), "|";
    $mixed = $i ? $o : 'x';
    echo call_user_func_array(fn($v) => is_object($v) ? $v->v : $v, [&$mixed]), "|";
    echo call_user_func_array('label2', ['m' => &$a, 's' => &$s]), "\n";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "10|abc1:3|x|abc1=5\n10|abc1:3|7|abc1=5\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `mixed` value bound to a `callable` parameter is NORMALIZED, so every callable spelling it
/// can hold reaches the callee as a working descriptor, and the normalized descriptor is released
/// after each call. A bare unbox read a boxed `null` as a descriptor and segfaulted, and leaked
/// one block per call for a boxed closure.
#[test]
fn test_mixed_argument_bound_to_a_callable_parameter_is_normalized_and_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class K { function m($x) { return "m$x"; } }
function call(callable $c, $a) { return $c($a); }
function forward(mixed $v, $a) { return call($v, $a); }
echo forward("strtoupper", "ab"), "|", forward([new K, "m"], 1), "|", forward(fn($x) => $x * 2, 4), "\n";
for ($i = 0; $i < 20; $i++) { forward(fn($x) => $x, 1); forward("strrev", "xy"); }
echo "ok";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "AB|m1|8\nok");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
