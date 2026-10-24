//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array suites, including nested array create access, nested array count, and nested array push.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Keeps a class constant's integer storage type when it initializes an indexed array.
#[test]
fn test_indexed_array_class_constant_element_type() {
    let out = compile_and_run(
        r#"<?php
class Message { public const NUMBER = 42; }
$values = [Message::NUMBER];
echo is_int($values[0]) ? "int" : "other";
"#,
    );
    assert_eq!(out, "int");
}

/// Stores a null class constant as a nullable array element.
#[test]
fn test_indexed_array_null_class_constant_element_type() {
    let out = compile_and_run(
        r#"<?php
class EmptyValue { public const NOTHING = null; }
$values = [EmptyValue::NOTHING];
echo count($values), ":", is_null($values[0]) ? "null" : "other";
"#,
    );
    assert_eq!(out, "1:null");
}

/// Resolves nested class constants before stamping the outer array element storage.
#[test]
fn test_indexed_array_nested_class_constant_element_type() {
    let out = compile_and_run(
        r#"<?php
class NumberValue { public const N = 5; }
class NumberList { public const ITEMS = [NumberValue::N]; }
$values = [NumberList::ITEMS];
echo is_array($values[0]) ? "array" : "other", ":", $values[0][0];
"#,
    );
    assert_eq!(out, "array:5");
}

/// Keeps a nested scoped constant's integer element type through an untyped arrow return.
#[test]
fn test_indexed_array_nested_class_constant_in_untyped_arrow_return() {
    let out = compile_and_run(
        r#"<?php
class NumberValue { public const N = 5; }
class NumberList { public const ITEMS = [NumberValue::N]; }
$make = fn() => [NumberList::ITEMS];
$values = $make();
echo gettype($values[0][0]), ":", $values[0][0];
"#,
    );
    assert_eq!(out, "integer:5");
}

/// Keeps a nested scoped constant's integer element type through an untyped closure return.
#[test]
fn test_indexed_array_nested_class_constant_in_untyped_closure_return() {
    let out = compile_and_run(
        r#"<?php
class NumberValue { public const N = 5; }
class NumberList { public const ITEMS = [NumberValue::N]; }
$make = function () { return [NumberList::ITEMS]; };
$values = $make();
echo gettype($values[0][0]), ":", $values[0][0];
"#,
    );
    assert_eq!(out, "integer:5");
}

/// Preserves a null class constant in an untyped arrow's returned array.
#[test]
fn test_scoped_constant_closure_null_arrow_return() {
    let out = compile_and_run(
        r#"<?php
class EmptyValue { public const NOTHING = null; }
$make = fn() => [EmptyValue::NOTHING];
$values = $make();
echo count($values), ":", gettype($values[0]);
"#,
    );
    assert_eq!(out, "1:NULL");
}

/// Preserves a null class constant in an untyped closure's returned array.
#[test]
fn test_scoped_constant_closure_null_return() {
    let out = compile_and_run(
        r#"<?php
class EmptyValue { public const NOTHING = null; }
$make = function () { return [EmptyValue::NOTHING]; };
$values = $make();
echo count($values), ":", gettype($values[0]);
"#,
    );
    assert_eq!(out, "1:NULL");
}

/// Keeps a null class constant boxed when returned directly by untyped callables.
#[test]
fn test_scoped_constant_closure_direct_null_return() {
    let out = compile_and_run(
        r#"<?php
class EmptyValue { public const NOTHING = null; }
$arrow = fn() => EmptyValue::NOTHING;
$closure = function () { return EmptyValue::NOTHING; };
echo gettype($arrow()), ":", gettype($closure());
"#,
    );
    assert_eq!(out, "NULL:NULL");
}

/// Resolves inherited class and interface constants in an untyped arrow return.
#[test]
fn test_scoped_constant_closure_inherited_return() {
    let out = compile_and_run(
        r#"<?php
interface Origin { public const FLAG = 9; }
interface ExtendedOrigin extends Origin {}
class BaseNumber { public const N = 5; }
class DerivedNumber extends BaseNumber implements ExtendedOrigin {}
$make = fn() => [DerivedNumber::N, DerivedNumber::FLAG];
$values = $make();
echo gettype($values[0]), ":", $values[0], ",", gettype($values[1]), ":", $values[1];
"#,
    );
    assert_eq!(out, "integer:5,integer:9");
}

/// Resolves self, parent, and static receivers inside method-defined closures.
#[test]
fn test_scoped_constant_closure_relative_receivers() {
    let out = compile_and_run(
        r#"<?php
class BaseNumber { public const N = 5; }
class DerivedNumber extends BaseNumber {
    public function selfMaker() { return fn() => [self::N]; }
    public function parentMaker() { return function () { return [parent::N]; }; }
    public function staticMaker() { return fn() => [static::N]; }
}
$object = new DerivedNumber();
$self = $object->selfMaker();
$parent = $object->parentMaker();
$static = $object->staticMaker();
$a = $self(); $b = $parent(); $c = $static();
echo gettype($a[0]), ":", $a[0], ",", gettype($b[0]), ":", $b[0], ",", gettype($c[0]), ":", $c[0];
"#,
    );
    assert_eq!(out, "integer:5,integer:5,integer:5");
}

/// Keeps a late-static constant override's runtime type in a returned closure array.
#[test]
fn test_scoped_constant_closure_late_static_override_return() {
    let out = compile_and_run(
        r#"<?php
class BaseValue {
    public const ITEM = 5;
    public function maker() { return fn() => [static::ITEM]; }
}
class DerivedValue extends BaseValue { public const ITEM = "nine"; }
$object = new DerivedValue();
$make = $object->maker();
$values = $make();
echo gettype($values[0]), ":", $values[0];
"#,
    );
    assert_eq!(out, "string:nine");
}

/// Preserves an enum case returned through an untyped arrow's array slot.
#[test]
fn test_scoped_constant_closure_enum_case_return() {
    let out = compile_and_run(
        r#"<?php
enum Suit { case Hearts; }
$make = fn() => [Suit::Hearts];
$values = $make();
echo $values[0] instanceof Suit ? "case" : "other";
"#,
    );
    assert_eq!(out, "case");
}

/// Resolves nested constants in a mixed-key class-constant array through a closure return.
#[test]
fn test_scoped_constant_closure_mixed_key_array_return() {
    let out = compile_and_run(
        r#"<?php
class NumberValue { public const N = 5; }
class NumberList { public const ITEMS = [[NumberValue::N]]; }
class Bag { public const ITEMS = [...NumberList::ITEMS, "name" => [NumberValue::N]]; }
$make = fn() => [Bag::ITEMS];
$values = $make();
echo gettype($values[0][0][0]), ":", $values[0][0][0], ",", gettype($values[0]["name"][0]), ":", $values[0]["name"][0];
"#,
    );
    assert_eq!(out, "integer:5,integer:5");
}

// --- Phase 14: Multi-dimensional arrays ---

/// Compiles a 2D numeric array literal and verifies indexed access to all four elements.
#[test]
fn test_nested_array_create_access() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2], [3, 4]];
echo $a[0][0] . " " . $a[0][1] . " " . $a[1][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "1 2 3 4");
}

/// Verifies `count()` on a 2D array returns the outer element count and the inner sub-array length.
#[test]
fn test_nested_array_count() {
    let out = compile_and_run(
        r#"<?php
$a = [[10, 20], [30, 40], [50, 60]];
echo count($a) . " " . count($a[0]);
"#,
    );
    assert_eq!(out, "3 2");
}

/// Appends a new sub-array to a 2D array via `[]` and confirms the outer count incremented and the new sub-array is accessible.
#[test]
fn test_nested_array_push() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2]];
$a[] = [3, 4];
echo count($a) . " " . $a[1][0];
"#,
    );
    assert_eq!(out, "2 3");
}

/// Exercises nested `foreach` iteration over a 2D array, confirming each scalar element is visited in row-major order.
#[test]
fn test_nested_array_foreach() {
    let out = compile_and_run(
        r#"<?php
$matrix = [[1, 2], [3, 4]];
foreach ($matrix as $row) {
    foreach ($row as $v) {
        echo $v . " ";
    }
}
"#,
    );
    assert_eq!(out, "1 2 3 4 ");
}

/// Tests three levels of array nesting (`[[[1]]]`), verifying that evaluation order correctly traverses to the innermost scalar.
#[test]
fn test_nested_array_3_levels() {
    let out = compile_and_run(
        r#"<?php
$a = [[[1]]];
echo $a[0][0][0];
"#,
    );
    assert_eq!(out, "1");
}

/// Constructs a 2D array of string values and asserts that indexed access and string concatenation produce the expected output.
#[test]
fn test_nested_array_string_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [["hello", "world"], ["foo", "bar"]];
echo $a[0][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "hello bar");
}

/// Tests `array_column()` extracting a named string key from an array of associative rows, confirming the result count is correct.
#[test]
fn test_array_column() {
    let out = compile_and_run(
        r#"<?php
$users = [
    ["name" => "Alice", "age" => "30"],
    ["name" => "Bob", "age" => "25"],
    ["name" => "Charlie", "age" => "35"],
];
$names = array_column($users, "name");
echo count($names);
"#,
    );
    assert_eq!(out, "3");
}

/// Exercises `array_column()` on rows containing mixed (string and int) values, then iterates both result arrays to confirm ordering and values are preserved.
#[test]
fn test_array_column_mixed_row_values() {
    let out = compile_and_run(
        r#"<?php
$users = [
    ["name" => "Ada", "score" => 10],
    ["name" => "Linus", "score" => 12],
    ["name" => "Grace", "score" => 8],
];
$names = array_column($users, "name");
$scores = array_column($users, "score");
foreach ($names as $name) {
    echo $name . " ";
}
echo "|";
foreach ($scores as $score) {
    echo $score . " ";
}
"#,
    );
    assert_eq!(out, "Ada Linus Grace |10 12 8 ");
}

/// Regression test for GC balance: after `array_column()` on mixed-type rows with all three arrays subsequently `unset`, allocations must equal frees.
#[test]
fn test_array_column_mixed_row_values_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$users = [
    ["name" => "Ada", "score" => 10],
    ["name" => "Linus", "score" => 12],
    ["name" => "Grace", "score" => 8],
];
$names = array_column($users, "name");
$scores = array_column($users, "score");
unset($names);
unset($scores);
unset($users);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// An assoc literal nested inside an assoc literal keeps its own element type.
///
/// The outer literal stamps a value type on its hash, and for a nested literal it used to take
/// the context-free syntactic guess, which types every variable `int`. The outer hash therefore
/// claimed to hold `array<string, int>` while the inner hash really held `array<string>`, and a
/// read through both levels returned the inner array POINTER read back as an integer -- the
/// `int(...)` symptom in issue #984. `$v` has to come from a local: a literal spelled out in
/// place is the one case the syntactic guess gets right on its own.
#[test]
fn test_nested_assoc_literal_keeps_a_local_arrays_element_type() {
    let out = compile_and_run(
        r#"<?php
$v = ["p", "q"];
$a = ["outer" => ["inner" => $v]];
var_dump($a["outer"]["inner"][1]);
"#,
    );
    assert_eq!(out, "string(1) \"q\"\n");
}

/// The same nesting, reached through an intermediate local instead of one chained read.
///
/// Reading `$a["outer"]` into its own local first goes through a different read path than the
/// chained `$a["outer"]["inner"]` above, but both consume the same fabricated stamp, so both
/// answered the pointer-as-integer before the fix. The inner array is string-valued on purpose:
/// the fabricated stamp was `int`, so only a non-int element type tells the two apart.
#[test]
fn test_nested_assoc_literal_element_type_survives_an_intermediate_local() {
    let out = compile_and_run(
        r#"<?php
$s = ["p", "q"];
$a = ["outer" => ["inner" => $s]];
$mid = $a["outer"];
echo $mid["inner"][1], "|", count($mid["inner"]);
"#,
    );
    assert_eq!(out, "q|2");
}

/// Regression for issue #984's ORIGINAL shape: a superglobal-backed array ASSIGNED into a cell.
///
/// #1068 fixed the nested-LITERAL stamp, which is a different path. This is the one the issue
/// was filed from: `$n["nested"] = $argv`, then an element read through the boxed `mixed` cell.
/// `$argv` is the only array the RUNTIME builds rather than the program, and on x86_64 it was
/// built with a bare `malloc` that never wrote the packed kind word `__rt_array_new` puts at
/// `[array - 8]`. Reading an element through the box then used the wrong slot stride and
/// answered the string's pointer as `int(140724801510533)`, with `foreach` iterating zero times.
///
/// The program is run without arguments, so `$argv` holds exactly the binary path; the values
/// are compared against `$argv` itself rather than spelled out, which is what makes the same
/// fixture match `php -n` on the script.
#[test]
fn test_regression_984_argv_assigned_into_an_assoc_cell_reads_back_as_strings() {
    let out = compile_and_run(
        r#"<?php
$n = ["a" => 1];
$n["nested"] = $argv;
echo count($n["nested"]), ";";
echo gettype($n["nested"][0]), ";";
echo $n["nested"][0] === $argv[0] ? "same" : "differs", ";";
$seen = 0;
foreach ($n["nested"] as $i => $v) { $seen++; echo $v === $argv[$i] ? "" : "MISMATCH"; }
echo $seen, ";";
$s = ["b" => 2];
$s["argv"] = $_SERVER["argv"];
echo gettype($s["argv"][0]), ";";
echo $s["argv"][0] === $argv[0] ? "same" : "differs", ";";
echo count($s["argv"]);
"#,
    );
    assert_eq!(out, "1;string;same;1;string;same;1");
}

/// An INDEXED outer literal was always correct, and has to stay that way.
///
/// `array_literal_element_type_for_ir` already carried the nested-literal arms that the
/// associative sibling was missing, which is exactly why `[["k" => $v]]` worked while
/// `["j" => ["k" => $v]]` did not. This pins the working half against a later change that
/// unifies the two.
#[test]
fn test_indexed_outer_literal_still_types_a_nested_assoc_literal() {
    let out = compile_and_run(
        r#"<?php
$v = ["p", "q"];
$g = [["inner" => $v]];
echo $g[0]["inner"][1], "|", count($g[0]["inner"]);
"#,
    );
    assert_eq!(out, "q|2");
}

/// A nested literal holding a loop-grown array, iterated rather than indexed.
///
/// `foreach` builds its iterator from the same stamped element type, so a fabricated `int`
/// value type made the loop read scalars out of an array pointer. The loop-grown source also
/// covers the case where the element type is only known from the local's inferred storage.
#[test]
fn test_nested_assoc_literal_array_iterates_its_real_elements() {
    let out = compile_and_run(
        r#"<?php
$items = [];
for ($i = 0; $i < 3; $i++) { $items[] = "e" . $i; }
$doc = ["body" => ["items" => $items]];
$seen = "";
foreach ($doc["body"]["items"] as $it) { $seen .= $it . ","; }
echo $seen, "|", count($doc["body"]["items"]);
"#,
    );
    assert_eq!(out, "e0,e1,e2,|3");
}

/// Verifies that `array_column()` creating copied sub-arrays survives `unset` of the source rows, and that individual nested elements are still accessible.
#[test]
fn test_gc_array_column_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    ["nums" => [4, 5]],
    ["nums" => [6, 7]],
];
$cols = array_column($rows, "nums");
unset($rows);
$first = $cols[0];
$second = $cols[1];
echo $first[1] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "5|6");
}

/// A nested element write whose root is a scalar raises PHP's catchable `Error` instead of
/// panicking in the compiler. Before this regression the emitted parent fetch passed the
/// scalar to the array fetch-for-write helper, which also segfaulted at runtime (#1985).
#[test]
fn test_nested_write_on_scalar_root_raises_the_php_error() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $m): string {
    $m["a"]["b"] = ($m = 42) ? 5 : 6;
    return "after";
}
try { echo f(["a" => ["x" => 0]]), "\n"; } catch (Error $e) { echo "caught:", $e->getMessage(), "\n"; }
function g(): string {
    $x = 42;
    $x["a"]["b"] = 5;
    return "after";
}
try { echo g(), "\n"; } catch (Error $e) { echo "caught:", $e->getMessage(), "\n"; }
function h(): string {
    $a = 42;
    $a[][] = 1;
    return "after";
}
try { echo h(), "\n"; } catch (Error $e) { echo "caught:", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "caught:Cannot use a scalar value as an array\ncaught:Cannot use a scalar value as an array\ncaught:Cannot use a scalar value as an array\n"
    );
}

/// Computed keys and the RHS run in PHP order before rejecting a scalar write parent.
#[test]
fn test_nested_write_scalar_root_preserves_operand_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function scalar_key(string $label): string { echo $label, '|'; return $label; }
function scalar_rhs(): int { echo 'rhs|'; return 9; }
function first(): void {
    $root = 42;
    try { $root[scalar_key('outer')][scalar_key('inner')] = scalar_rhs(); }
    catch (Error $e) { echo $e->getMessage(), "\n"; unset($e); }
}
function second(mixed $root): void {
    try { $root[scalar_key('outer')][scalar_key('inner')] = ($root = scalar_rhs()) ? 5 : 6; }
    catch (Error $e) { echo $e->getMessage(), "\n"; unset($e); }
}
function scalar_string_rhs(): string { echo 'owned-rhs|'; return str_repeat('x', 32); }
function fourth(): void {
    $root = 42;
    try { $root[] = scalar_string_rhs(); }
    catch (Error $e) { echo $e->getMessage(), "\n"; unset($e); }
}
first(); second(['outer' => ['before' => 1]]); fourth();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, concat!(
        "outer|inner|rhs|Cannot use a scalar value as an array\n",
        "outer|inner|rhs|Cannot use a scalar value as an array\n",
        "owned-rhs|Cannot use a scalar value as an array\n",
    ));
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
