//! Purpose:
//! Exercises generic specializations through the real Magician bridge and runtime cells.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_generics`.
//!
//! Key details:
//! - Runtime-selected source prevents literal eval from taking AOT's shortcut.
//! - Fixtures cover persistence, class storage, type failures, and portable strict syntax.

use crate::support::*;

/// Builds dynamic eval source without allowing compile-time evaluation of the string.
fn dynamic_eval(body: &str) -> String {
    format!("<?php\n$code = $argc > 1 ? $argv[1] : <<<'CODE'\n{body}\nCODE;\neval($code);\n")
}

/// PHP comparisons after instanceof coexist with fully written nested generic class targets.
#[test]
fn test_eval_generics_review_instanceof_comparisons() {
    let source = dynamic_eval(r#"
class C {} class Other {} $value = new C(); $limit = 2;
echo (int) ($value instanceof C < 1), '|', (int) ($value instanceof Other < $limit);
echo '|', (int) ($value instanceof Other < strlen('xx'));
class Box<T> { public function __construct(public T $value) {} }
$box = new Box<array<array<int>>>([[1]]);
echo '|', (int) ($box instanceof Box<array<array<int>>> < 1);
echo '|', $box instanceof Box<array<array<int>>> >> 1;
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "0|1|1|0|0");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Nested templates follow return/continue/break while top-level forward calls remain available.
#[test]
fn test_eval_generics_review_unreachable_declarations() {
    let source = dynamic_eval(r#"
echo identity(7), '|';
function identity<T>(T $value): T { return $value; }
function stop() { echo 'R'; return; class Hidden<T> {} }
stop(); stop(); echo '|';
for ($i = 0; $i < 2; $i++) { echo $i; continue; trait Skipped<T> {} }
for ($i = 0; $i < 2; $i++) { break; interface Ignored<T> {} }
function install_templates() { class Visible<T> {} function nested<T>(T $value): T { return $value; } }
install_templates(); echo '|', get_class(new Visible<int>()), '|', nested(9);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "7|RR|01|Visible<int>|9");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Runtime included files hoist forward templates across PHP tags and preserve returns and once.
#[test]
fn test_eval_generics_review_included_forward_calls() {
    let source = dynamic_eval("echo include 'piece.php'; echo '|', require_once 'piece.php';");
    let files = [
        ("main.php", source.as_str()),
        ("piece.php", "H<?php echo identity(7), '|'; return 9; function identity<T>(T $value): T { return $value; } ?>TAIL"),
    ];
    assert_eq!(compile_and_run_files(&files, "main.php"), "H7|9|1");
}

/// Nested classes recheck later trait/parent/interface dependencies before exposing variance.
#[test]
fn test_eval_generics_review_later_variance_dependencies() {
    let source = dynamic_eval(r#"
class Animal {} class Dog extends Animal {}
function install_templates() {
    class Box<out T> { use Put<T>; }
    trait Put<T> { public function put(T $value): void {} }
    class Child<out T> extends ParentBox<T> {}
    class ParentBox<T> { public function put(T $value): void {} }
    class Provider<out T> implements Consumer<T> { public function put(int $value): void {} }
    interface Consumer<T> { public function put(T $value): void; }
    class Safe<out T> { use HiddenPut<T> { HiddenPut::put as private; } }
    trait HiddenPut<T> { public function put(T $value): void {} }
}
install_templates();
try { new Box<Dog>(); echo 'unchecked'; } catch (TypeError $error) { echo 'trait'; }
try { new Child<Dog>(); echo '|unchecked'; } catch (TypeError $error) { echo '|parent'; }
try { new Provider<int>(); echo '|unchecked'; } catch (TypeError $error) { echo '|interface'; }
echo '|', get_class(new Safe<Dog>());
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "trait|parent|interface|Safe<Dog>");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Native templates persist across eval barriers and specialize scalar and nested array calls.
#[test]
fn test_eval_generics_functions_persist() {
    let source = r#"<?php
$declarations = $argc > 1 ? $argv[1] : 'function identity<T>(T $v): T { return $v; } function first<T>(array<T> $v): T { return $v[0]; }';
eval($declarations);
$use = $argc > 1 ? $argv[1] : 'echo identity(7), "|", identity<string>("ok"), "|", first([[9]])[0], "|", identity(...["v" => "spread"]); $f = identity<int>(...); echo "|", $f(8), "|", call_user_func_array("identity", ["v" => "callback"]);';
eval($use);
"#;
    assert_eq!(compile_and_run(source), "7|ok|9|spread|8|callback");
}

/// Native code can invoke generic templates declared by Magician before their first specialization.
#[test]
fn test_eval_generics_calls_from_native_code() {
    let source = r#"<?php
$code = $argc > 1 ? $argv[1] : 'function id<T>(T $v): T { return $v; } class Box<T> { public function __construct(public T $v) {} public function get(): T { return $this->v; } public static function make(T $v): Box<T> { return new Box<T>($v); } }';
eval($code);
echo id(7), '|', (new Box('native'))->get(), '|', Box::make(8)->get();
"#;
    assert_eq!(compile_and_run(source), "7|native|8");
}

/// Class and constructor-local slots, generic methods, and closures use real boxed storage.
#[test]
fn test_eval_generics_classes_methods_and_constructors() {
    let source = dynamic_eval(r#"
class Box<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
    public function map<U>(callable(T): U $f): Box<U> { return new Box<U>($f($this->value)); }
}
class Pair<T> { public function __construct<U>(public T $left, public U $right) {} }
function Box<T>(T $v): T { return $v; }
$b = new Box('hello');
$p = new Pair<int>(1, 'two');
echo Box(9), '|', $b->map(function(string $s): int { return strlen($s); })->get(), '|', $p->right, '|', get_class($p);
"#);
    assert_eq!(compile_and_run(&source), "9|5|two|Pair<int, string>");
}

/// Runtime binding failures and structural contracts use the normal catchable TypeError path.
#[test]
fn test_eval_generics_bounds_arrays_and_errors() {
    let source = dynamic_eval(r#"
class Base {} class Child extends Base {}
function bounded<T: Base>(T $v): T { return $v; }
function same<T>(T $a, T $b): T { return $a; }
function countInts(array<int> $a): int { return count($a); }
function bad(): array<int> { return ['bad']; }
echo get_class(bounded(new Child())), '|', countInts([1, 2]);
try { bounded(1); } catch (TypeError $e) { echo '|bound'; }
try { same(1, 'a'); } catch (TypeError $e) { echo '|conflict'; }
try { countInts(['a']); } catch (TypeError $e) { echo '|array'; }
try { bad(); } catch (TypeError $e) { echo '|return'; }
"#);
    assert_eq!(compile_and_run(&source), "Child|2|bound|conflict|array|return");
}

/// PHPDoc templates stay portable in strict PHP mode, including promoted constructor slots.
#[test]
fn test_eval_generics_docblocks_in_strict_php() {
    let source = dynamic_eval(r#"
/** @template T */
class Box {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
}
class Item {
    /**
     * @template U
     * @param U $value
     */
    public function __construct(public $value) {}
}
/**
 * @template V
 * @param V $value
 * @return V
 */
function identity($value) { return $value; }
echo (new Box(7))->get(), '|', identity('ok'), '|', (new Item('item'))->value;
"#);
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(&source, flags), "7|ok|item");
    }
}

/// Retained generic templates from an included file execute correctly on repeated calls.
#[test]
fn test_eval_generics_runtime_include() {
    let files = [
        ("main.php", "<?php $code = $argc > 1 ? $argv[1] : 'require_once \"templates.php\"; echo identity(7), \"|\", identity(\"ok\");'; eval($code);"),
        ("templates.php", "<?php /**\n * @template T\n * @param T $value\n * @return T\n */ function identity($value) { return $value; }"),
    ];
    assert_eq!(compile_and_run_files(&files, "main.php"), "7|ok");
}

/// Keeps the documented Magician example executable through the full compiler pipeline.
#[test]
fn test_eval_generics_example() {
    assert_eq!(compile_and_run(include_str!("../../examples/eval-generics/main.php")), "42|Magician|8\ntype checked\n");
}

/// Evaluated argument owners protect by-reference calls, COW arrays, and rejected calls.
#[test]
fn test_eval_generics_argument_ownership_and_cow() {
    let source = dynamic_eval(r#"
function swap<T>(T &$left, T &$right): void { T $held = $left; $left = $right; $right = $held; }
function change<T>(array<T> $values, T $value): array<T> { $values[0] = $value; return $values; }
function id<T>(T $v): T { return $v; }
$a = 'left'; $b = 'right'; swap($a, $b);
$original = ['old']; $changed = change($original, 'new');
echo $a, '|', $b, '|', $original[0], '|', $changed[0];
for ($i = 0; $i < 100; $i++) {
    $value = id(['payload']);
    try { id<int>(['bad']); } catch (TypeError $e) {}
    unset($value);
}
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "right|left|old|new");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Ordinary eval callables share the corrected reference-copy and array parameter boundaries.
#[test]
fn test_eval_generics_ordinary_call_ownership_regression() {
    let source = dynamic_eval(r#"
function swap(&$left, &$right): void { $held = $left; $left = $right; $right = $held; }
function change(array $values): array { $values[0] = 'new'; $values[] = 'tail'; return $values; }
$a = 'left'; $b = 'right'; swap($a, $b);
$original = ['old']; $changed = change($original);
echo $a, '|', $b, '|', $original[0], '|', $changed[0], '|', count($original), '|', count($changed);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "right|left|old|new|1|2");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Array contracts preserve indexed storage and nullable/union writes with balanced cleanup.
#[test]
fn test_eval_generics_array_local_contract_regressions() {
    let source = dynamic_eval(r#"
?array<int> $nullable = [1]; $nullable[] = 2; $nullable[0] = 3;
array<int>|null $union = [4]; $union[] = 5; $union[0] = 6;
echo $nullable[0], '|', $nullable[1], '|', $union[0], '|', $union[1];
try { $nullable[] = 'bad'; } catch (TypeError $e) { echo '|value'; }
try { $union['key'] = 7; } catch (TypeError $e) { echo '|key'; }
$union['0'] = 8; echo '|', $union[0], '|', count($union);
function length(array<int> $items): int { return count($items); }
function bad(): array<int> { return ['key' => 1]; }
try { length(['key' => 1]); } catch (TypeError $e) { echo '|argument'; }
try { bad(); } catch (TypeError $e) { echo '|return'; }
try { array<int> $items = ['key' => 1]; } catch (TypeError $e) { echo '|local'; }
array<int>|array<string> $items = [1];
try { $items[] = 'bad'; } catch (TypeError $e) { echo '|arm'; }
echo '|', count($items);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "3|2|6|5|value|key|8|2|argument|return|local|arm|1");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Constructor slots retain identities while class variance and inference see native hierarchy.
#[test]
fn test_eval_generics_constructor_class_variance_regression() {
    let body = dynamic_eval(r#"
class Box<out T: Animal> {
    public function __construct<U>(private T $value, private U $label) {}
    public function get(): T { return $this->value; }
}
function read(Box<Animal> $box): string { return get_class($box->get()); }
function relay<V>(Box<V> $box): V { return $box->get(); }
function bounded<V: Box<Animal>>(V $box): string { return read($box); }
$i = new Box(new Dog(), 1); $s = new Box<Dog>(new Dog(), 'label');
echo read($i), '|', read($s), '|', get_class(relay($i)), '|', bounded($s), '|', get_class($i), '|', get_class($s);
function packed(array<int> $items): void {}
function associative(array<string, int> $items): void {}
try { packed(new Dog()); } catch (TypeError $e) { echo '|packed'; }
try { associative(new Dog()); } catch (TypeError $e) { echo '|assoc'; }
"#);
    let source = format!("<?php\nclass Animal {{}} class Dog extends Animal {{}}\necho get_class(new Dog()), '|';\n{}", body.trim_start_matches("<?php"));
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "Dog|Dog|Dog|Dog|Dog|Box<Dog, int>|Box<Dog, string>|packed|assoc");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Same short parent names receive distinct qualified PHPDoc arguments in both PHP modes.
#[test]
fn test_eval_generics_docblock_qualified_parent_regression() {
    let source = dynamic_eval(r#"
namespace Ns;
/** @template T */
class Foo {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
}
namespace Other;
/** @template T */
interface Foo { /** @return T */ public function label(); }
namespace App;
/**
 * @extends \Ns\Foo<int>
 * @implements \Other\Foo<string>
 */
class Child extends \Ns\Foo implements \Other\Foo {
    public function label(): string { return 'label'; }
}
$child = new Child(7); echo $child->get(), '|', $child->label();
"#);
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(&source, flags), "7|label");
    }
}

/// Fully written class and constructor arguments work on a cold cache and through inheritance.
#[test]
fn test_eval_generics_written_constructor_arguments_regression() {
    let source = dynamic_eval(r#"
class Pair<T> { public function __construct<U>(public T $left, public U $right) {} }
$cold = new Pair<int, string>(1, 'cold');
$inferred = new Pair<int>(2, 'inferred');
$warm = new Pair<int, string>(3, 'warm');
echo get_class($cold), '|', get_class($inferred), '|', $warm->right;
try { new Pair<int, string>(4, []); } catch (TypeError $e) { echo '|value'; }
class ParentPair<T> { public function __construct<U>(public T $left, public U $right) {} }
class Child extends ParentPair<int, string> {}
$child = new Child(6, 'child'); echo '|', $child->left, '|', $child->right, '|', get_parent_class($child);
class Base {} class Derived extends Base {}
class DefaultPair<T> { public function __construct<U: Base, V = U>(public T $left, public U $right, public V $other) {} }
$default = new DefaultPair<int, Derived>(7, new Derived(), new Derived());
echo '|', get_class($default);
try { new DefaultPair<int, int>(7, 8, 9); } catch (TypeError $e) { echo '|bound'; }
try { new Pair<int, string, bool>(1, 'two'); } catch (TypeError $e) { echo '|arity'; }
class Shadow<T> { public function __construct<T>(public T $value) {} }
echo '|', get_class(new Shadow<int, string>('ok'));
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "Pair<int, string>|Pair<int, string>|warm|value|6|child|ParentPair<int, string>|DefaultPair<int, Derived, Derived>|bound|arity|Shadow<int, string>");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Float and bool writes validate their stored key type without changing caller keys or COW.
#[test]
fn test_eval_generics_normalized_scalar_array_keys_regression() {
    let source = dynamic_eval(r#"
array<int> $items = [1, 2]; $original = $items;
$float = 1.0; $bool = false;
$items[$float] = 3; $items[true] = 4; $items[$bool] = 5; $items[-0.0] = 6; $items[2.0] = 7;
try { $items[false] = 'bad'; } catch (TypeError $e) { echo 'value|'; }
echo $items[0], '|', $items[1], '|', $items[2], '|', count($items), '|', $original[0], '|', $original[1], '|', is_float($float), '|', is_bool($bool);
array<int, int> $map = [1, 2]; $map[true] = 8; $map[0.0] = 9;
?array<int> $nullable = [1]; $nullable[false] = 10;
array<int>|null $union = [1]; $union[0.0] = 11;
echo '|', $map[0], '|', $map[1], '|', $nullable[0], '|', $union[0];
array<string, int> $strings = ['k' => 1];
try { $strings[1.0] = 2; } catch (TypeError $e) { echo '|float'; }
try { $strings[false] = 3; } catch (TypeError $e) { echo '|bool'; }
echo '|', count($strings);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "value|6|4|7|3|1|2|1|1|9|8|10|11|float|bool|1");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Null keys keep PHP's empty-string identity across contracts, COW, and autovivification.
#[test]
fn test_eval_generics_null_array_keys_regression() {
    let source = dynamic_eval(r#"
array<string, int> $map = ['k' => 1]; $original = $map; $key = null;
$map[$key] = 2; $map[null] = 3;
echo $map[''], '|', count($map), '|', count($original), '|', is_null($key);
?array<string, int> $nullable = null; $nullable[null] = 4;
array<string, int>|null $union = null; $union[null] = 5;
echo '|', $nullable[''], '|', $union[''];
array<int> $indexed = [1];
try { $indexed[null] = 2; } catch (TypeError $e) { echo '|indexed'; }
try { $map[null] = 'bad'; } catch (TypeError $e) { echo '|value'; }
echo '|', $map[''];
for ($i = 0; $i < 20; $i++) {
    ?array<int> $failed = null;
    try { $failed['k'] = 2; } catch (TypeError $e) {}
    try { $failed[] = 'bad'; } catch (TypeError $e) {}
}
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "3|2|1|1|4|5|indexed|value|3");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Union writes preserve scalars, update string bytes, and only autovivify null or false.
#[test]
fn test_eval_generics_non_array_union_writes_regression() {
    let source = dynamic_eval(r#"
function rhs(&$calls): int { $calls++; return 9; }
$calls = 0; array<int>|string $items = 'keep'; $original = $items; $alias =& $items;
try { $items[] = rhs($calls); } catch (Error $e) { echo $e->getMessage(), '|'; }
echo $items, '|', $calls;
$items[0] = rhs($calls); $alias[-1] = 'X'; $items['01'] = 'Y';
echo '|', $items, '|', $original, '|', $alias;
try { $items['bad'] = 1; } catch (TypeError $e) { echo '|key'; }
try { $items[0] = ''; } catch (Error $e) { echo '|empty'; }
$items[6] = 'Z'; echo '|', $items;
$cases = [true, 7, 1.5];
foreach ($cases as $value) {
    array<int>|bool|int|float $scalar = $value;
    try { $scalar[] = rhs($calls); } catch (Error $e) { echo '|append'; }
    try { $scalar[0] = rhs($calls); } catch (Error $e) { echo '|index'; }
    echo '|', $scalar;
}
echo '|', $calls;
array<int>|bool|null $empty = false; $empty[] = 1; $empty = null; $empty[0] = 2;
echo '|', $empty[0];
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "[] operator not supported for strings|keep|1|9YeX|keep|9YeX|key|empty|9YeX  Z|append|index|1|append|index|7|append|index|1.5|8|2");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Ordinary scalar offset writes share PHP's conversion, diagnostics, and reference boundaries.
#[test]
fn test_eval_generics_ordinary_scalar_offsets_regression() {
    let source = dynamic_eval(r#"
class Text { public function __toString(): string { echo 'convert|'; return 'XY'; } }
$items = 'keep'; $original = $items;
$items[null] = 'N'; $items[false] = 'F'; $items[1.0] = 'G'; $items['1x'] = 'P';
$items[2] = new Text();
echo $items, '|', $original;
$keys = ['1.9', '1.9x', '1e2', '1e2x', [], new stdClass()];
foreach ($keys as $key) {
    try { $items[$key] = 'bad'; } catch (TypeError $e) { echo '|key'; }
}
$items[-99] = new Text();
echo '|', $items;
$items[' 1 '] = 'W'; $items['1efoo'] = 'V';
echo '|', $items;
$number = 7;
try { $number[] = 'x'; } catch (Error $e) { echo '|scalar'; }
echo '|', $number;
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "convert|FPXp|keep|key|key|key|key|key|key|FPXp|FVXp|scalar|7");
    assert_eq!(output.stderr.matches("String offset cast occurred").count(), 3, "{}", output.stderr);
    assert!(output.stderr.contains("Only the first byte will be assigned"), "{}", output.stderr);
    assert!(output.stderr.contains("Illegal string offset -99"), "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Live local receivers follow index and RHS side effects, including receiver-type transitions.
#[test]
fn test_eval_generics_live_offset_receivers_regression() {
    let source = dynamic_eval(r#"
function replace(&$target, $next, $result) { $target = $next; return $result; }
array<int>|string|int|null $items = 'ab'; $original = $items;
$items[replace($items, 'xy', 0)] = 'Z'; echo $items, '|', $original;
array<int>|string|int $scalar = 'ab';
try { $scalar[replace($scalar, 7, 0)] = 'Z'; } catch (Error $e) { echo '|scalar'; }
echo '|', $scalar;
array<int>|string|null $append = null;
try { $append[] = replace($append, 'ab', 'value'); } catch (Error $e) { echo '|append'; }
echo '|', $append;
array<int>|string $rhs = 'ab';
$rhs[0] = replace($rhs, 'xy', 'Z'); echo '|', $rhs;
array<int> $indexed = [0];
$indexed[replace($indexed, [1, 2], 0)] = 3; echo '|', $indexed[0], '|', $indexed[1];
array<int> $appended = [0];
$appended[] = replace($appended, [4, 5], 6); echo '|', $appended[0], '|', $appended[1], '|', $appended[2];
array<mixed, int>|string $map = 'ab';
$map[replace($map, [7], '01')] = 8; echo '|', $map[0], '|', $map['01'], '|', count($map);
$map['01'] = replace($map, 'xy', 'Z'); echo '|', $map;
function stage1(&$target, &$order) { $order .= 'i'; $target = 'xy'; return 0; }
function stage2(&$target, &$order) { $order .= 'v'; $target = 'uv'; return 'Z'; }
array<int>|string $ordered = 'ab'; $order = '';
$ordered[stage1($ordered, $order)] = stage2($ordered, $order);
echo '|', $ordered, '|', $order;
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "Zy|ab|scalar|7|append|ab|Zy|3|2|4|5|6|7|8|2|xZ|Zv|iv");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Live dispatch reaches reassigned ArrayAccess receivers and preserves operand failures and aliases.
#[test]
fn test_eval_generics_live_array_access_and_operand_failures() {
    let source = dynamic_eval(r#"
function replace(&$target, $next, $result) { $target = $next; return $result; }
function reject(&$target, $next) { $target = $next; throw new Exception('stop'); }
class Sink implements ArrayAccess {
    public function __construct(private string $label) {}
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetUnset(mixed $key): void {}
    public function offsetSet(mixed $key, mixed $value): void {
        echo $this->label, ':', $key === null ? 'null' : $key, ':', $value, '|';
    }
}
array<mixed, mixed>|string|Sink|null $object = new Sink('old');
$object[replace($object, new Sink('new'), '01')] = 'value';
$object[0] = replace($object, 'xy', 'Z'); echo $object, '|';
array<mixed, mixed>|Sink $indexed = [1];
$indexed[replace($indexed, new Sink('indexed'), 'k')] = 'V';
array<mixed, mixed>|Sink|null $append = null;
$append[] = replace($append, new Sink('append'), 'W');
$invalid = 7;
try { $invalid[replace($invalid, new stdClass(), 0)] = 'Z'; }
catch (Error $e) { echo $e->getMessage(), '|', get_class($invalid), '|'; }
array<int>|string $items = 'ab';
try { $items[reject($items, 'xy')] = 'Z'; } catch (Exception $e) { echo 'index|'; }
echo $items;
try { $items[0] = reject($items, 'uv'); } catch (Exception $e) { echo '|rhs'; }
echo '|', $items;
$live = 'ab'; $alias =& $live;
$alias[replace($live, 'xy', 0)] = 'Z'; echo '|', $live, '|', $alias;
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "new:01:value|Zy|indexed:k:V|append:null:W|Cannot use object of type stdClass as array|stdClass|index|xy|rhs|uv|Zy|Zy");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Numeric string offsets accept all PHP whitespace, including vertical-tab integer prefixes.
#[test]
fn test_eval_generics_vertical_tab_string_offsets_regression() {
    let source = dynamic_eval(r#"
array<int>|string $items = 'keep';
$items["\v1"] = 'Q'; $items["\v1x"] = 'R'; $items["1\v"] = 'S';
echo $items;
$keys = [" 0 ", "\t0\t", "\r0\r", "\n0\n", "\f0\f", "\v0\v"];
foreach ($keys as $key) { $items[$key] = 'Z'; }
echo '|', $items;
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "kSep|ZSep");
    assert_eq!(output.stderr.matches("Illegal string offset").count(), 1, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Failed construction frees receivers and preserves escaped metadata without running destructors.
#[test]
fn test_eval_generics_failed_construction_cleanup_regression() {
    let body = dynamic_eval(r#"
class Failed<T> {
    public function __construct(public T $value) { throw new Exception('body'); }
    public function __destruct() { echo 'bad-destructor|'; }
}
try { new Failed<int>(1); } catch (Exception $e) { echo $e->getMessage(), '|'; }
class Escaped<T> {
    public function __construct(public T $value, &$saved) { $saved = $this; throw new Exception('escaped'); }
    public function __destruct() { echo 'bad-escaped|'; }
}
$saved = null;
try { new Escaped<int>(2, $saved); } catch (Exception $e) { echo $e->getMessage(), '|'; }
echo get_class($saved), '|', $saved->value, '|';
unset($saved);
class Ordinary {
    public function __construct(int $value) { throw new Exception('ordinary'); }
    public function __destruct() { echo 'bad-argument|'; }
}
try { new Ordinary(3); } catch (Exception $e) { echo $e->getMessage(), '|'; }
class NativeChild<T> extends NativeParent {}
try { new NativeChild<int>(4); } catch (Exception $e) { echo $e->getMessage(); }
"#);
    let source = format!("<?php\nclass NativeParent {{ public function __construct(int $value) {{ throw new Exception('native'); }} public function __destruct() {{ echo 'bad-native|'; }} }}\n{}", body.trim_start_matches("<?php"));
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "body|escaped|Escaped<int>|2|ordinary|native");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Typed local aliases reject element, append, replacement, and parameter writes before mutation.
#[test]
fn test_eval_generics_review_reference_local_contracts() {
    let source = dynamic_eval(r#"
array<int> $a = [1]; $b =& $a;
try { $b[0] = 'bad'; } catch (TypeError $e) { echo 'index|'; }
try { $b[] = 'bad'; } catch (TypeError $e) { echo 'append|'; }
try { $b = ['bad']; } catch (TypeError $e) { echo 'replace|'; }
function change(&$v) { $v[0] = 'bad'; }
try { change($b); } catch (TypeError $e) { echo 'call|'; }
function element(&$v) { $v = 'bad'; }
$key = '0';
try { element($b[$key]); } catch (TypeError $e) { echo 'element|'; }
echo $a[0], '|', count($a), '|';
try { array<string> $a = ['bad']; } catch (TypeError $e) { echo 'declared|'; }
try { array<string> $b = ['bad']; } catch (TypeError $e) { echo 'alias|'; }
$a = [2]; $b[] = 3; echo $a[0], '|', $b[1];
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "index|append|replace|call|element|1|1|declared|alias|2|3");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Escaping storage keeps its type contract while copied arrays remain independent.
#[test]
fn test_eval_generics_review_reference_contract_lifetimes() {
    let source = dynamic_eval(r#"
array<int> $a = [1]; $b =& $a; $c =& $b; $copy = $a;
$copy[0] = 'copy'; $c[0] = 2; unset($a);
try { $c[] = 'bad'; } catch (TypeError $e) { echo 'chain|'; }
function escaped() { array<int> $local = [3]; return [&$local]; }
$refs = escaped();
try { $refs[0] = ['bad']; } catch (TypeError $e) { echo 'escape|'; }
$refs[0] = [4]; echo $b[0], '|', $copy[0], '|', $refs[0][0];
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "chain|escape|2|copy|4");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Multiple typed aliases constrain builtin writeback without constraining independent values.
#[test]
fn test_eval_generics_review_reference_contract_intersection() {
    let source = dynamic_eval(r#"
array<int>|string $wide = [1]; array<int> $narrow = [2]; $narrow =& $wide;
try { $wide = 'bad'; } catch (TypeError $e) { echo 'intersection|'; }
$copy = $wide; $copy = 'ok';
try { array_push($wide, 'bad'); } catch (TypeError $e) { echo 'builtin|'; }
$wide[] = 3; echo $wide[0], '|', $narrow[1], '|', $copy;
$f = function() use (&$wide) { $wide = ['bad']; };
try { $f(); } catch (TypeError $e) { echo '|capture'; }
"#);
    assert_eq!(compile_and_run(&source), "intersection|builtin|1|3|ok|capture");
}

/// Concrete child returns implement generic parent contracts with invariant or covariant slots.
#[test]
fn test_eval_generics_review_inherited_signature_returns() {
    let source = dynamic_eval(r#"
class Box<T> {} class Child<T> extends Box<T> {}
$parent = new Box<int>(); $child = new Child<int>();
interface Factory { public function make(): Box<int>; }
class Maker implements Factory { public function make(): Child<int> { return new Child<int>(); } }
echo get_class((new Maker())->make());
class Animal {} class Dog extends Animal {}
class Covariant<out T> {} class Sub<T> extends Covariant<T> {}
interface Animals { public function make(): Covariant<Animal>; }
class Dogs implements Animals { public function make(): Sub<Dog> { return new Sub<Dog>(); } }
echo '|', get_class((new Dogs())->make());
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "Child<int>|Sub<Dog>");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Generic traits enforce variance through nested uses, mutable properties, and shadowed methods.
#[test]
fn test_eval_generics_review_trait_variance() {
    let source = dynamic_eval(r#"
trait Put<T> { public function put(T $value): void {} }
trait Nested<U> { use Put<U>; }
trait Get<T> { public function get(): T { throw new Exception('unused'); } }
trait State<T> { public T $value; }
trait Hidden<T> { private function put(T $value): void {} public function id<T>(T $value): T { return $value; } }
try { class Bad<out V> { use Nested<V>; } } catch (TypeError $e) { echo 'nested|'; }
try { class BadGet<in V> { use Get<V>; } } catch (TypeError $e) { echo 'return|'; }
try { class BadState<out V> { use State<V>; } } catch (TypeError $e) { echo 'property|'; }
class Good<out V> { use Hidden<V>; public function __construct(V $value) {} }
class PrivateBox<out T> { use Put<T> { put as private; } }
class OverrideBox<out T> { use Put<T>; public function put(int $value): void {} }
try { class Exposed<out T> { use Hidden<T> { put as public; } } } catch (TypeError $e) { echo 'exposed|'; }
echo (new Good<int>(1))->id('ok');
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "nested|return|property|exposed|ok");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Mixed-case native declarations retain PHPDoc constructor and inherited member annotations.
#[test]
fn test_eval_generics_review_docblock_keyword_case() {
    let source = dynamic_eval(r#"
InTeRfAcE Read<T> { /** @return T */ public function get(); }
TrAiT ReadValue<T> { /** @return T */ public function get() { return $this->value; } }
CLASS Box<T> implements Read<T> {
    use ReadValue<T>;
    /** @var T */ public $value;
    /** @param T $value */ public function __construct($value) { $this->value = $value; }
}
$box = new Box(5); echo get_class($box), '|', $box->get();
$box->value = 'bad';
try { $box->get(); } catch (TypeError $e) { echo '|return'; }
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "Box<int>|5|return");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Native by-reference writeback preserves typed storage on rejection and releases staging owners.
#[test]
fn test_eval_generics_review_native_reference_writeback() {
    let eval = dynamic_eval(r#"
array<int> $a = [1]; $b =& $a;
try { native_replace($b); } catch (TypeError $e) { echo 'native|'; }
echo $a[0], '|', $b[0];
"#);
    let source = eval.replacen("<?php", "<?php\nfunction native_replace(array &$value): void { $value = ['bad']; }", 1);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "native|1|1");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// A property reference created in a function retains the local array contract after the frame exits.
#[test]
fn test_eval_generics_review_property_reference_escape() {
    let source = dynamic_eval(r#"
class Holder { public $value; }
function escaped_property() {
    array<int> $a = [3]; $holder = new Holder(); $holder->value =& $a; return $holder;
}
$holder = escaped_property();
try { $holder->value = ['bad']; } catch (TypeError $e) { echo 'property|'; }
try { $holder->value[0] = 'bad'; } catch (TypeError $e) { echo 'index|'; }
try { $holder->value[] = 'bad'; } catch (TypeError $e) { echo 'append|'; }
echo $holder->value[0], '|'; $holder->value = [4]; echo $holder->value[0];
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "property|index|append|3|4");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Contracts introduced through a global alias remain attached after the declaring frame exits.
#[test]
fn test_eval_generics_review_global_reference_contracts() {
    let source = dynamic_eval(r#"
$global = [1];
function bind_global() { global $global; array<int> $local = [2]; $local =& $global; }
bind_global();
try { $global[] = 'bad'; } catch (TypeError $e) { echo 'global|'; }
echo $global[0], '|', count($global);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "global|1|1");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Static reference storage retains typed constraints across assignments and offset writes.
#[test]
fn test_eval_generics_review_static_reference_contracts() {
    let source = dynamic_eval(r#"
class Holder { public static $value = null; }
function bind_static() { array<int> $local = [3]; Holder::$value =& $local; }
bind_static();
try { Holder::$value = ['bad']; } catch (TypeError $e) { echo 'static|'; }
try { Holder::$value[0] = 'bad'; } catch (TypeError $e) { echo 'index|'; }
try { Holder::$value[] = 'bad'; } catch (TypeError $e) { echo 'append|'; }
echo Holder::$value[0], '|'; Holder::$value = [4]; echo Holder::$value[0];
"#);
    assert_eq!(compile_and_run(&source), "static|index|append|3|4");
}

/// Qualified trait selections and visibility changes preserve the effective variance surface.
#[test]
fn test_eval_generics_review_qualified_trait_adaptations() {
    let source = dynamic_eval(r#"
trait Put<T> { public function put(T $value): void {} }
trait Hidden<T> { private function put(T $value): void {} }
trait Inner<T> { use Put<T> { Put::put as private; } }
trait Get<T> { public function value(): T { throw new Exception('unused'); } }
trait Other<T> { public function value(T $value): T { return $value; } }
class PrivateBox<out T> { use Put<T> { pUt::put as private; } }
class NestedBox<out T> { use Inner<T>; }
class SelectedBox<out T> { use Get<T>, Other<T> { Get::value insteadof Other; Get::value as copy; } }
echo get_class(new PrivateBox<int>()), '|', get_class(new NestedBox<int>()), '|', get_class(new SelectedBox<int>());
try { class Exposed<out T> { use Hidden<T> { Hidden::put as public; } } }
catch (TypeError $e) { echo '|exposed'; }
try { class Alias<out T> { use Put<T> { Put::put as private; Put::put as public exposed; } } }
catch (TypeError $e) { echo '|alias'; }
try { class SelectedAlias<out T> { use Get<T>, Other<T> { Get::value insteadof Other; Other::value as exposed; } } }
catch (TypeError $e) { echo '|selected_alias'; }
try { class NestedExposed<out T> { use Inner<T> { Inner::put as public; } } }
catch (TypeError $e) { echo '|nested'; }
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "PrivateBox<int>|NestedBox<int>|SelectedBox<int>|exposed|alias|selected_alias|nested");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Shares PHP-compatible cursor, sibling-reference, and COW checks across eval/native setters.
/// Reference-target key ownership is tracked separately in task 4136c2d0.
fn element_writeback_metadata_body() -> &'static str {
    r#"
$index = 'x'; $other = 'y';
$y = 1; $a = ['x' => 0, 'y' => &$y]; $alias =& $a; $copy = $a; next($a);
set_x($a[$index]); $y = 7;
echo $a[$other], '|', key($a), '|', $alias[$other], '|', key($alias), '|', $a[$index], '|', $copy[$index], '|', $copy[$other];
next($a); set_x($a[$index]); $y = 8;
echo '|', $a[$other], '|', key($a) === null ? 'end' : 'reset';
"#
}

/// Eval element writeback preserves sibling aliases, array cursors, and independent COW copies.
#[test]
fn test_eval_generics_review_element_writeback_metadata() {
    let source = dynamic_eval(&format!("function set_x(&$value) {{ $value = 9; }}\n{}", element_writeback_metadata_body()));
    assert_eq!(compile_and_run(&source), "7|y|7|y|9|0|7|8|end");
}

/// Native setter writeback preserves sibling aliases, cursors, and independent COW copies.
#[test]
fn test_eval_generics_review_element_writeback_metadata_native() {
    let source = dynamic_eval(element_writeback_metadata_body())
        .replacen("<?php", "<?php\nfunction set_x(mixed &$value): void { $value = 9; }", 1);
    assert_eq!(compile_and_run(&source), "7|y|7|y|9|0|7|8|end");
}
