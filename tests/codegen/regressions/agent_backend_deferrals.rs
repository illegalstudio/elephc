//! Purpose:
//! Regression tests for framework files the backend used to refuse ("the backend cannot lower"),
//! which handed them to the runtime interpreter instead of compiling them natively.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test names the vendor file and the refusal it fixes, uses only standalone PHP, and
//!   asserts php's exact output (and a clean heap where values are allocated).

use crate::support::*;

/// doctrine/dbal `Schema/Table.php::_addUniqueConstraint`:
/// `array_merge((array) $this->getName(), $columns)` over a `string` getter was refused with
/// "array cast for PHP type Str".
#[test]
fn test_array_cast_of_a_scalar_is_a_one_element_list() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class N { public function getName(): string { return "nm"; } }
function h(N $n, array $cols) { return array_merge((array) $n->getName(), $cols); }
echo implode(",", h(new N, ['a', 'b'])), "\n";
function i(int $v) { return (array) $v; }
function f(float $v) { return (array) $v; }
function b(bool $v) { return (array) $v; }
echo json_encode([i(3), f(1.5), b(true), b(false)]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "nm,a,b\n[[3],[1.5],[true],[false]]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// nette/utils `Utils/Validators.php::isCallable`: `is_callable($value, syntax_only: true)` named
/// the second parameter and the backend refused "is_callable expected 1 args, got 3".
#[test]
fn test_is_callable_accepts_the_syntax_only_named_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function ic(mixed $value): bool { return $value && is_callable($value, syntax_only: true); }
var_dump(ic('strlen'), ic(0), ic('nonexistent_fn'), ic([1, 2]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\nbool(true)\nbool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/queue `QueueRoutes.php::getRoute`: a four-operand `array_merge()` over gradual
/// operands (`class_parents($q) ?: []`) had no fixed-arity helper and was refused with
/// "array_merge for PHP type Mixed".
#[test]
fn test_array_merge_of_four_gradual_operands_keeps_php_key_rules() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function m4($a, $b, $c, $d) { return array_merge($a, $b, $c, $d); }
echo implode(",", m4(['a'], ['b'], ['c'], ['d'])), "\n";
echo json_encode(m4(['a' => 1], [5 => 'b'], ['a' => 2], ['d'])), "\n";
function m7($a, $b, $c, $d, $e, $f, $g) { return array_merge($a, $b, $c, $d, $e, $f, $g); }
echo json_encode(m7([1], [2], [3], ['k' => 4], [5], [6], ['k' => 7])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "a,b,c,d\n{\"a\":2,\"0\":\"b\",\"1\":\"d\"}\n{\"0\":1,\"1\":2,\"2\":3,\"k\":7,\"3\":5,\"4\":6}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine-bundle `EntityListenerPass.php::process` and psysh `FuzzyMatcher.php::filter`:
/// `usort()` over a list of rows (`array<array<string, mixed>>`) was refused with
/// "usort indexed-array element PHP type AssocArray".
#[test]
fn test_usort_over_a_list_of_rows() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$tags = [];
$tags[] = ['name' => 'a', 'attributes' => ['priority' => 1]];
$tags[] = ['name' => 'b', 'attributes' => ['priority' => 5]];
$tags[] = ['name' => 'c', 'attributes' => []];
usort($tags, static fn (array $a, array $b) => ($b['attributes']['priority'] ?? 0) <=> ($a['attributes']['priority'] ?? 0));
foreach ($tags as $t) { echo $t['name']; }
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bac\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/routing `Route.php::parentOfParameter` and laravel/prompts `SelectPrompt.php`:
/// `array_search($parameter, array_keys($this->parameters))` with an untyped needle was refused
/// with "array_search needle PHP type Mixed".
#[test]
fn test_array_search_with_a_gradual_needle_over_array_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class R {
    public array $parameters = ['a' => 1, 'b' => 2];
    function p($parameter) { return array_search($parameter, array_keys($this->parameters)); }
}
$r = new R;
var_dump($r->p('b'), $r->p('zz'));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(1)\nbool(false)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/support `MessageBag.php::count`: `count($this->messages, COUNT_RECURSIVE)` over an
/// untyped property holding lists was refused ("COUNT_RECURSIVE needs a statically known element
/// type").
#[test]
fn test_count_recursive_over_nested_arrays() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class M {
    protected $messages = ['a' => ['x', 'y'], 'b' => ['z']];
    function count(): int { return count($this->messages, COUNT_RECURSIVE) - count($this->messages); }
}
echo (new M)->count(), "\n";
function deep(array $v) { return count($v, COUNT_RECURSIVE); }
echo deep([1, [2, [3, 4]], 5]), "\n";
echo deep([]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3\n7\n0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine/dbal `Platforms/AbstractPlatform.php::getEnumDeclarationSQL`:
/// `max(...array_map(mb_strlen(...), $values))` spread a boxed list into `max()` and was refused
/// with "max for PHP type Array(Mixed)".
#[test]
fn test_max_and_min_over_a_spread_list() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function longest(array $values): int {
    return max(...array_map(fn($s) => strlen($s), $values));
}
function shortest(array $values): int {
    return min(...array_map(fn($s) => strlen($s), $values));
}
echo longest(['ab', 'abcd', 'c']), shortest(['ab', 'abcd', 'c']), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "41\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// nette/utils `Utils/Strings.php::ord` and brick/math `BigDecimal.php::fromFloatExact`:
/// `unpack('N', $bytes)[1]` and `unpack('J', ...)` (an unnamed field, and a 64-bit directive)
/// were refused by the literal-format `unpack()` lowering.
#[test]
fn test_unpack_unnamed_field_and_64_bit_directives() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo unpack('N', "\x00\x00\x01\x02")[1], "\n";
echo unpack('J', "\x00\x00\x00\x00\x00\x00\x01\x02")[1], "\n";
echo unpack('P', "\x02\x01\x00\x00\x00\x00\x00\x00")[1], "\n";
echo unpack('J', "\xff\xff\xff\xff\xff\xff\xff\xff")[1], "\n";
echo unpack('v', "\x02\x01")[1], "\n";
echo unpack('C', "A")[1], "\n";
$a = unpack('Nhi/Vlo', "\x00\x00\x00\x07\x08\x00\x00\x00");
echo $a['hi'], ",", $a['lo'], "\n";
function ord8(string $s): int { return unpack('N', $s . "\0\0\0")[1] ?? 0; }
echo ord8("\x01"), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "258\n258\n258\n-1\n258\n65\n7,8\n16777216\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine/dbal `Schema/IndexEditor.php::setColumns`:
/// `[$first, ...array_values($others)]` over a variadic predicted an associative literal for the
/// list-shaped spread and failed validation (`OperandTypeMismatch` Heap(Array) vs Heap(Hash)).
#[test]
fn test_array_literal_spreading_array_values_of_a_variadic() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Col { public function __construct(public string $n) {} }
class IndexEditor {
    private array $columns = [];
    public function setColumns(Col $first, Col ...$others): self
    {
        $this->columns = [$first, ...array_values($others)];
        return $this;
    }
    public function names(): string { return implode(",", array_map(fn(Col $c) => $c->n, $this->columns)); }
}
$cols = [new Col('a'), new Col('b'), new Col('c')];
$e = new IndexEditor;
$e->setColumns(...$cols);
echo $e->names(), "\n";
function mk(string $first, array $others): array { return [$first, ...array_values($others)]; }
echo implode(",", mk('a', ['b', 'c'])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a,b,c\na,b,c\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/cache `ArrayLock.php::forceRelease`, illuminate/http `DelegatesToResource::__unset`
/// and illuminate/queue `DatabaseUuidFailedJobProvider::all`: `unset()` of an element of, or a
/// property of, a boxed (`mixed`) receiver was refused with "unset target shape with 1 lowered
/// operands".
#[test]
fn test_unset_through_a_boxed_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Store { public $locks = []; }
class Lock {
    protected $store;
    protected $name;
    public function __construct($store, $name) { $this->store = $store; $this->name = $name; }
    public function forceRelease() { unset($this->store->locks[$this->name]); }
}
$s = new Store; $s->locks = ['a' => 1, 'b' => 2];
(new Lock($s, 'a'))->forceRelease();
echo json_encode($s->locks), "\n";
class Res {
    public $resource;
    public function __unset($key) { unset($this->resource->{$key}); }
}
$r = new Res; $r->resource = new stdClass; $r->resource->x = 1; $r->resource->y = 2;
unset($r->x);
echo json_encode($r->resource), "\n";
function strip($records) {
    return array_map(function ($record) { $record->id = $record->uuid; unset($record->uuid); return $record; }, $records);
}
$row = new stdClass; $row->uuid = 'u1';
echo json_encode(strip([$row])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"b\":2}\n{\"y\":2}\n[{\"id\":\"u1\"}]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/queue `DeferredQueue.php::push`: `fn () => parent::push(...)` in a subclass forwards
/// `$this` to the parent's instance method, but the closure did not capture the receiver and the
/// backend refused "lexical instance method static call without this".
#[test]
fn test_closure_calling_a_parent_instance_method_captures_this() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function push($j) { return "P:$j"; } protected static function sp() { return "sp"; } }
class C extends P {
    public function push($j) { return (fn () => parent::push($j))(); }
    public function viaClosure($j) { $f = function () use ($j) { return parent::push($j) . self::sp(); }; return $f(); }
}
echo (new C)->push('a'), "\n";
echo (new C)->viaClosure('b'), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "P:a\nP:bsp\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/console `Scheduling/Event.php::execute`: `fwrite($type === 'out' ? STDOUT : STDERR,
/// $line)` merged two stream constants into an `int` and the backend refused "fwrite stream
/// argument PHP type Int".
#[test]
fn test_ternary_over_two_stream_constants_is_a_stream_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$cb = fn ($type, $line) => fwrite($type === 'out' ? STDOUT : STDERR, $line);
$cb('out', "to-stdout\n");
$n = $cb('out', "again\n");
echo $n, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "to-stdout\nagain\n6\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// illuminate/image `ImageManager.php::fromUrl` and three other Laravel files: a zero-argument
/// `->throw()` on a boxed receiver (`$http->get($url)->throw()->body()`) took the `Fiber::throw`
/// arm of the mixed dispatch and was refused "Fiber::throw without exactly one EIR argument".
#[test]
fn test_zero_argument_throw_on_a_boxed_receiver_is_not_the_fibers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Response {
    public function throw(): static { return $this; }
    public function body(): string { return "body!"; }
}
class Http { public function get(string $u): mixed { return new Response; } }
class Factory { public function make(string $c): mixed { return new Http; } }
class Mgr {
    public function __construct(private Factory $container) {}
    public function fromUrl(string $url): \Closure
    {
        return fn () => $this->container->make('http')->get($url)->throw()->body();
    }
}
echo (new Mgr(new Factory))->fromUrl('x')(), "\n";
$fiber = new Fiber(function (): string { $got = Fiber::suspend('s'); return "got " . $got; });
echo $fiber->start(), "\n";
$fiber->resume('r');
echo $fiber->getReturn(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "body!\ns\ngot r\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A callable array `[$object, 'method']` called with a spread of a declared `array` parameter must
/// bind by name and raise php's `Error` / `ArgumentCountError`, not warn and read null. A gradual
/// source takes the descriptor walk (`instance_callable_args_need_descriptor_binder`).
#[test]
fn test_callable_array_spread_of_a_boxed_array_binds_by_name_and_validates() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public function sub(int $first, int $second): int { return $first - $second; } }
function viaCallable(array $v): int { $cb = [new T(), 'sub']; return $cb(...$v); }
foreach ([['second' => 1, 'first' => 5], [7, 2], [10, 'second' => 3], ['first' => 1, 'nope' => 2], [1]] as $args) {
    try { echo viaCallable($args), "\n"; } catch (Error $e) { echo get_class($e), "\n"; }
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "4\n5\n7\nError\nArgumentCountError\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `str_replace()` with the by-reference `$count` writes a caller variable, so the optimizer must
/// not treat that form as pure (it substitutes into pure calls and drops their unused results).
#[test]
fn test_str_replace_count_form_is_not_optimized_away() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$c = 0;
str_replace("a", "b", "aaa" . $argc, $c);
echo $c, "\n";
function f(string $s) { $n = 0; str_replace("a", "b", $s, $n); return $n; }
echo f("banana" . $argc), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3\n3\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A local whose type changes inside a `foreach` that then `break`s must keep the value the break
/// carried out: the exit used to read the (widened, boxed) slot at its pre-loop `int` type, so
/// polyfill-deepclone's `$n = count($m); foreach ($m as $v) { if (0 !== $v) { $n = $m; break; } }`
/// kept the count and every deep-cloned object came back as the first object's class.
#[test]
fn test_foreach_break_after_a_retyping_store_keeps_the_stored_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function a() {
    $m = [0, 1];
    $n = count($m);
    foreach ($m as $v) { if ($v !== 7) { $n = $m; break; } }
    return json_encode($n);
}
echo a(), "\n";
function b() {
    $m = [0, 1];
    $n = count($m);
    foreach ($m as $v) { if ($v !== 7) { $n = $m; break; } }
    return json_encode([$n]);
}
echo b(), "\n";
function d(array $c) {
    $m = [];
    foreach ($c as $id => $e) { $m[$id] = $e[0]; }
    $n = count($m);
    foreach ($m as $v) { if (0 !== $v) { $n = $m; break; } }
    return json_encode($n);
}
echo d([[0, 0], [1, 0], [1, 0]]), "\n";
echo d([[0, 0], [0, 0]]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[0,1]\n[[0,1]]\n[0,1,1]\n2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An implementor's covariant return type must not replace the ABSTRACT parent's declared one:
/// reflection reports the declaration (`Mgr`), and an interpreted subclass whose return is another
/// covariant narrowing is accepted against it (doctrine/dbal's `createSchemaManager()`).
#[test]
fn test_abstract_method_keeps_its_declared_return_type() {
    let out = compile_and_run(
        r#"<?php
abstract class Mgr {}
final class MgrA extends Mgr {}
final class MgrB extends Mgr {}
abstract class Platform {
    abstract public function create(int $c): Mgr;
    public function name(): string { return static::class; }
}
final class PlatformA extends Platform {
    public function create(int $c): MgrA { return new MgrA(); }
}
$p = new PlatformA();
echo get_class($p->create(1)), "\n";
echo (new ReflectionMethod(Platform::class, 'create'))->getReturnType(), "\n";
echo (new ReflectionMethod(PlatformA::class, 'create'))->getReturnType(), "\n";
eval('final class PlatformB extends Platform { public function create(int $c): MgrB { return new MgrB(); } }');
$b = new PlatformB();
echo get_class($b->create(2)), ' ', $b->name(), "\n";
"#,
    );
    assert_eq!(out, "MgrA\nMgr\nMgrA\nMgrB PlatformB\n");
}

/// `clone($o, $withProperties)` applies each entry through ONE shared driver that dispatches to
/// the clone class's per-entry applicator. The invocation scope still selects the applicator
/// body: readonly re-initialisation works from the class scope and is refused from global scope
/// (direct site and callable alike), a private slot is only reachable from its own scope.
#[test]
fn test_clone_with_properties_through_the_shared_driver_keeps_scope_semantics() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public function __construct(public readonly int $r = 1, protected int $p = 2, private int $q = 3, public int $a = 4) {}
  public function dump(): string { return "$this->r/$this->p/$this->q/$this->a"; }
  public function inScope(array $w): static { return clone($this, $w); }
  public function viaCallable(array $w): static { $f = clone(...); return $f($this, $w); } }
class C extends P {
  private int $q = 30;
  public function childScope(P $o, array $w): P { return clone($o, $w); }
  public function dumpC(): string { return $this->dump() . "|" . $this->q; } }
function gl(object $o, array $w): mixed { return clone($o, $w); }
function t(callable $f) { try { echo $f(), "\n"; } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; } }
$p = new P(); $c = new C();
t(fn() => gl($p, ['a' => 9])->dump());
t(fn() => gl($p, ['r' => 9])->dump());
t(fn() => $p->inScope(['r' => 9, 'p' => 8, 'q' => 7])->dump());
t(fn() => $p->viaCallable(['r' => 5])->dump());
t(fn() => (clone(...))($p, ['r' => 5])->dump());
t(fn() => $c->childScope($p, ['p' => 6, 'a' => 1])->dump());
t(fn() => $c->childScope($p, ['q' => 6])->dump());
t(fn() => $c->childScope($c, ['q' => 6, 'a' => 2])->dumpC());
t(fn() => gl($c, ['q' => 6])->dumpC());
t(fn() => $c->inScope(['q' => 11, 'r' => 12])->dumpC());
t(fn() => gl($p, ["\0x" => 1])->dump());
$v = 5; $arr = ['a' => &$v];
t(fn() => gl($p, $arr)->dump());
t(fn() => gl($p, [])->dump());
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1/2/3/9\n\
         Error: Cannot modify protected(set) readonly property P::$r from global scope\n\
         9/8/7/4\n\
         5/2/3/4\n\
         Error: Cannot modify protected(set) readonly property P::$r from global scope\n\
         1/6/3/1\n\
         Error: Cannot access private property P::$q\n\
         1/2/3/2|6\n\
         Error: Cannot access private property C::$q\n\
         12/2/11/4|30\n\
         Error: Cannot access property starting with \"\\0\"\n\
         Error: Cannot assign by reference when cloning with updated properties\n\
         1/2/3/4\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new $class(...)` and `newInstanceWithoutConstructor()` over a runtime class string run through
/// shared per-module helpers (`outlined_dynamic_new`). The helper holds the non-Throwable classes;
/// the site keeps Throwable creation (it records the site's line), the arity refusal and the miss.
#[test]
fn test_dynamic_new_through_shared_helpers_keeps_throwables_refusals_and_misses() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public function __construct(public string $s = 'a', public int $n = 1) {} }
class B { public $v = 'b'; public function __construct(public string $x, public int $y) {} }
class C { public $v = 'c'; public function __construct(public string $p = 'p', public int $q = 0) {} }
class D { public $v = 'd'; }
class MyEx extends Exception {}
function mk(string $c) { return new $c('q'); }
function mk2(string $c, string $s) { return new $c($s, 2); }
function bare(string $c) { return (new ReflectionClass($c))->newInstanceWithoutConstructor(); }
foreach (['A', 'a', 'B', 'C', 'D', 'MyEx', 'RuntimeException'] as $c) {
    try {
        $o = mk2($c, 'q');
        echo get_class($o), ' ', $o instanceof Throwable ? $o->getMessage() . '@' . $o->getLine() : json_encode($o), "\n";
    } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
foreach (['A', 'D', 'MyEx', 'Nope'] as $c) {
    try { echo get_class(bare($c)), "\n"; } catch (Throwable $e) { echo get_class($e), "\n"; }
}
try { echo get_class(mk('B')), "\n"; } catch (Throwable $e) { echo get_class($e), "\n"; }
echo json_encode(mk('A')), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "A {\"s\":\"q\",\"n\":2}\nA {\"s\":\"q\",\"n\":2}\nB {\"v\":\"b\",\"x\":\"q\",\"y\":2}\n\
         C {\"v\":\"c\",\"p\":\"q\",\"q\":2}\nD {\"v\":\"d\"}\nMyEx q@8\nRuntimeException q@8\n\
         A\nD\nMyEx\nReflectionException\nArgumentCountError\n{\"s\":\"q\",\"n\":1}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `static` hydrator closure bound with `Closure::bindTo(null, $class)` writes private and
/// protected properties of that class and of its parent by RUNTIME name, one scope per class
/// (polyfill-deepclone's `getHydrator()`). Each name resolves to its declared slot -- none is
/// created as a dynamic property, so no creation deprecation reaches stderr.
#[test]
fn test_scope_bound_hydrator_closure_writes_declared_slots_by_runtime_name() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Base { private mixed $bp = 'bp'; protected mixed $bq = 'bq'; public mixed $pub = 'pub'; }
class Child extends Base { private mixed $cp = 'cp'; protected mixed $cq = 'cq'; private ?array $typed = null; }
class Other { private mixed $value = null; private ?int $identifier = null; private int $type = 0; }
eval('
final class H {
    public static function get(string $class): \Closure {
        $base = static function ($properties, $objects) {
            foreach ($properties as $name => $values) {
                foreach ($values as $i => $v) {
                    $objects[$i]->$name = $v;
                }
            }
        };
        return $base->bindTo(null, $class);
    }
}');
$c = new Child();
$o = new Other();
H::get(Child::class)(['cp' => [0 => 1], 'cq' => [0 => 2], 'bq' => [0 => 3], 'pub' => [0 => 4], 'typed' => [0 => [7]]], [$c, $o]);
H::get(Base::class)(['bp' => [5]], [$c]);
H::get(Other::class)(['value' => [1 => "v"], 'identifier' => [1 => 3], 'type' => [1 => 9]], [$c, $o]);
print_r($c);
print_r($o);
"#,
    );
    assert_eq!(
        out.stdout,
        "Child Object\n(\n    [bp:Base:private] => 5\n    [bq:protected] => 3\n    [pub] => 4\n    [cp:Child:private] => 1\n    [cq:protected] => 2\n    [typed:Child:private] => Array\n        (\n            [0] => 7\n        )\n\n)\nOther Object\n(\n    [value:Other:private] => v\n    [identifier:Other:private] => 3\n    [type:Other:private] => 9\n)\n"
    );
    assert!(!out.stderr.contains("dynamic property"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An interpreted property write through a receiver that is not a plain variable
/// (`$a[0]->p = v`, `$o->inner->p = v`, `f()->p = v`, `$a[0]->p += v`) owns that receiver
/// reference and must release it; it used to stay live, with everything it reaches, until exit.
#[test]
fn test_eval_property_write_through_a_temporary_receiver_releases_it() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Other { public mixed $value = null; }
class Holder { public $o; function __construct($o) { $this->o = $o; } function get() { return $this->o; } }
$o = new Other();
$h = new Holder($o);
eval('$h->get()->value = 3; $h->o->value += 1; $a = [$o]; $a[0]->value += 2; $b = ["k" => $o]; $b["k"]->value = $b["k"]->value . "x";');
echo $o->value, "\n";
"#,
    );
    assert_eq!(out.stdout, "6x\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// twig/extra-bundle `Extensions.php::getClasses`: `array_column(self::EXTENSIONS, 'class', 'name')`
/// in interpreted code was refused ("unsupported Return statement"): the interpreter's
/// `array_column` only knew the two-argument form.
#[test]
fn test_eval_array_column_with_an_index_key() {
    let out = compile_and_run(
        r#"<?php
eval('
class Ext {
    private const E = [
        ["class" => "A", "name" => "a", "x" => [1]],
        ["class" => "B", "name" => "b", "x" => []],
        ["class" => "C", "id" => 7],
        ["class" => "D", "name" => "a"],
    ];
    public static function getClasses(): array { return array_column(self::E, "class", "name"); }
    public static function whole(): array { return array_column(self::E, null, "class"); }
    public static function nullIndex(): array { return array_column(self::E, "class", null); }
}
echo json_encode(Ext::getClasses()), "\n";
echo json_encode(array_keys(Ext::whole())), "\n";
echo json_encode(Ext::nullIndex()), "\n";
echo json_encode(array_column([[5 => "x", "k" => 9], [5 => "y", "k" => 8]], 5, "k")), "\n";
echo json_encode(array_column([["a" => 1, "i" => 4], ["a" => 2, "i" => 4]], "a", "i")), "\n";
');
"#,
    );
    assert_eq!(
        out,
        "{\"a\":\"D\",\"b\":\"B\",\"0\":\"C\"}\n[\"A\",\"B\",\"C\",\"D\"]\n[\"A\",\"B\",\"C\",\"D\"]\n{\"9\":\"x\",\"8\":\"y\"}\n{\"4\":2}\n"
    );
}

/// monolog `LogRecord` stamps `new \DateTimeImmutable('now', $tz)` and prints `.u`: php's current
/// time carries microseconds, while elephc's constructor kept only whole seconds, so every
/// record read `.000000`. A string that sets a time of day (a bare date, `tomorrow`, `10:00`,
/// `@ts`) still has none.
#[test]
fn test_current_time_carries_microseconds() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$utc = new DateTimeZone('UTC');
$micro = function (DateTimeInterface $d): string { return $d->format('u') === '000000' ? 'zero' : 'micro'; };
// A single zero reading is possible once in a million; ten of them never.
$plain = $now = $tz = $mutable = $relative = $ago = $nowrel = 0;
for ($i = 0; $i < 10; $i++) {
    $plain += $micro(new DateTimeImmutable()) === 'micro' ? 1 : 0;
    $now += $micro(new DateTime('now')) === 'micro' ? 1 : 0;
    $tz += $micro(new DateTimeImmutable('now', $utc)) === 'micro' ? 1 : 0;
    $mutable += $micro(date_create('now')) === 'micro' ? 1 : 0;
    $relative += $micro(new DateTimeImmutable('+1 day')) === 'micro' ? 1 : 0;
    $ago += $micro(new DateTimeImmutable('1 hour ago', $utc)) === 'micro' ? 1 : 0;
    $nowrel += $micro(new DateTimeImmutable('last day of next month')) === 'micro' ? 1 : 0;
    usleep(137);
}
echo 'plain ', $plain > 0 ? 'micro' : 'zero', "\n";
echo 'now ', $now > 0 ? 'micro' : 'zero', "\n";
echo 'tz ', $tz > 0 ? 'micro' : 'zero', "\n";
echo 'mutable ', $mutable > 0 ? 'micro' : 'zero', "\n";
echo 'relative ', $relative > 0 ? 'micro' : 'zero', "\n";
echo 'ago ', $ago > 0 ? 'micro' : 'zero', "\n";
echo 'nowrel ', $nowrel > 0 ? 'micro' : 'zero', "\n";
foreach (['tomorrow', 'today', 'midnight', 'noon', 'next monday', '@1700000000', '2020-01-02', '10:00', 'yesterday 10:00', '2020-01-02 03:04:05'] as $s) {
    echo str_pad($s, 22), $micro(new DateTimeImmutable($s)), ' ', $micro(new DateTimeImmutable($s, $utc)), "\n";
}
$a = new DateTimeImmutable('now', $utc);
$b = new DateTimeImmutable('now', $utc);
echo $a <= $b ? "ordered\n" : "reversed\n";
echo abs((float)$a->format('U.u') - microtime(true)) < 2.0 ? "close\n" : "far\n";
echo (new DateTimeImmutable('2020-01-02 03:04:05.250000'))->format('u'), "\n";
echo (new DateTime())->setTimestamp(1700000000)->format('u'), ' ', (new DateTimeImmutable())->setTimestamp(1700000000)->format('u'), "\n";
"#,
    );
    let mut expected = String::new();
    for k in ["plain", "now", "tz", "mutable", "relative", "ago", "nowrel"] {
        expected.push_str(&format!("{k} micro\n"));
    }
    for s in [
        "tomorrow", "today", "midnight", "noon", "next monday", "@1700000000", "2020-01-02", "10:00",
        "yesterday 10:00", "2020-01-02 03:04:05",
    ] {
        expected.push_str(&format!("{s:<22}zero zero\n"));
    }
    expected.push_str("ordered\nclose\n250000\n000000 000000\n");
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// twig `Environment::parse()` calls `$this->parser->parse($stream)` through an untyped property,
/// and symfony's polyfill `NumberFormatter::parse(string, int $type = 3, &$offset = null)` was the
/// only compiled `parse` taking a reference. Its by-reference SHAPE is borrowed for the mixed
/// receiver, but the shape kept that class's default values and padded them onto the call, so
/// `Twig\Parser::parse($stream, $test = null)` received `3` and the parser then fataled with
/// "mixed value is not callable".
#[test]
fn test_mixed_receiver_call_does_not_inherit_defaults_from_a_by_ref_candidate() {
    let out = compile_and_run(
        r#"<?php
class Tok { public function __construct(public string $v) {} public function test($x): bool { return $this->v === $x; } }
class Parser {
    public function parse(Tok $stream, $test = null, bool $dropNeedle = false): string {
        return $this->sub($test, $dropNeedle);
    }
    public function sub($test, bool $dropNeedle = false): string {
        if (null !== $test && $test(new Tok('x'))) { return 'called'; }
        return $dropNeedle ? 'drop' : 'null-ok';
    }
}
class NumFmt { public function parse(Tok $value, int $type = 3, &$offset = null): string { $offset = 7; return 'nf' . $type; } }
class Magic { public function __call($n, $a) { return 'magic:' . $n; } }
class Env {
    private $parser;
    public function parse(Tok $stream) {
        if (null === $this->parser) { $this->parser = new Parser(); }
        return $this->parser->parse($stream);
    }
    public function parseWith(Tok $stream, $holder) {
        return $holder->parse($stream);
    }
}
$e = new Env();
echo $e->parse(new Tok('a')), "\n";
echo (new Magic)->whatever(), "\n";
$off = 0;
echo (new NumFmt)->parse(new Tok('a'), 1, $off), ' ', $off, "\n";
echo $e->parseWith(new Tok('a'), new Parser()), "\n";
echo $e->parseWith(new Tok('a'), new NumFmt()), "\n";
"#,
    );
    assert_eq!(out, "null-ok\nmagic:whatever\nnf1 7\nnull-ok\nnf3\n");
}

/// A compiled `is_callable($this)` inside a base-class method answered false when `$this` was an
/// instance of an interpreter-declared subclass defining `__invoke` (Symfony's Command
/// constructor relies on it): the native object table cannot see the subclass.
#[test]
fn test_is_callable_this_sees_an_interpreted_subclass_invoke() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function check() { return is_callable($this); }
    public static function checkObj($o) { return is_callable($o); }
}
class Plain3 extends Base {}
eval('
class Cmd extends Base { public function __invoke(string $u = "x"): int { return 0; } }
class Other extends Base {}
$c = new Cmd;
echo var_export(is_callable($c), true), " ", var_export($c->check(), true), " ", var_export(Base::checkObj($c), true), "\n";
$o = new Other;
echo var_export($o->check(), true), " ", var_export(Base::checkObj($o), true), "\n";
');
echo var_export((new Plain3)->check(), true), "\n";
"#,
    );
    assert_eq!(out, "true true true\nfalse false\nfalse\n");
}

/// `new ReflectionFunction($obj(...))` and the other method first-class callables report the
/// method's own short name, not `Class::method` / `object::method`.
#[test]
fn test_reflection_function_of_method_first_class_callable_reports_the_method_name() {
    let out = compile_and_run(
        r#"<?php
class Cmd { public function __invoke($a = 1) { return $a; } public function m($x) { return $x; } public static function s($x) { return $x; } }
function show($f) {
    $r = new ReflectionFunction($f);
    echo $r->getName(), "|", $r->getShortName(), "|", var_export($r->isClosure(), true), "|", $r->getNumberOfParameters(), "\n";
}
$o = new Cmd;
show($o(...));
show($o->m(...));
show(Cmd::s(...));
show(strlen(...));
"#,
    );
    assert_eq!(out, "__invoke|__invoke|true|1\nm|m|true|1\ns|s|true|1\nstrlen|strlen|true|1\n");
}

/// Reading a concrete-typed property through the eval bridge handed back an owned Mixed cell that
/// EIR (which treats the read as borrowed) never released: three blocks per `echo $p->name` after
/// a reflection write. The slope test requires 1 and 6 iterations to both end clean.
#[test]
fn test_eval_bridge_property_read_does_not_leak_per_iteration() {
    let source = |n: u32| {
        format!(
            r#"<?php
class E {{ public $name = 'n'; }}
for ($i = 0; $i < {n}; $i++) {{
    $rp = new ReflectionProperty(E::class, 'name');
    $p = new E();
    $rp->setRawValue($p, 'proxied' . $i);
    echo $p->name, "\n";
}}
"#
        )
    };
    for n in [1u32, 6] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        let expected: String = (0..n).map(|i| format!("proxied{}\n", i)).collect();
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `ReflectionProperty::setRawValue($object, new Z())` lowered the raw store without retiring the
/// retain its owning receiver temporary carries, so every hydrated object (and the value it held)
/// outlived its last reference and its destructor never ran. The slope test requires 1 and 6
/// iterations to both end clean, and each destructor to run before the next iteration.
#[test]
fn test_reflection_set_raw_value_of_an_object_does_not_leak_the_receiver() {
    let source = |n: u32| {
        format!(
            r#"<?php
class Z {{ public $v = 1; function __destruct() {{ echo "dz\n"; }} }}
class E {{ public ?Z $o = null; public $u = null; function __destruct() {{ echo "de\n"; }} }}
for ($i = 0; $i < {n}; $i++) {{
    $p = new E();
    (new ReflectionProperty(E::class, 'o'))->setRawValue($p, new Z());
    $p = null;
    echo "end\n";
}}
"#
        )
    };
    for n in [1u32, 6] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        let expected = "de\ndz\nend\n".repeat(n as usize);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `ReflectionProperty::setValue($object, new Z())` lowered the store without retiring the retain
/// its owning receiver temporary carries, so the hydrated object and the value it held were never
/// destructed. Both a typed and an untyped property are checked, at 1 and 6 iterations.
#[test]
fn test_reflection_set_value_of_an_object_does_not_leak_the_receiver() {
    let source = |n: u32| {
        format!(
            r#"<?php
class Z {{ public $v = 1; function __destruct() {{ echo "dz\n"; }} }}
class E {{ public ?Z $o = null; public $u = null; function __destruct() {{ echo "de\n"; }} }}
for ($i = 0; $i < {n}; $i++) {{
    $p = new E();
    $rp = new ReflectionProperty(E::class, 'o');
    $rp->setValue($p, new Z());
    $p = null;
    echo "end\n";
}}
"#
        )
    };
    for n in [1u32, 6] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        let expected = "de\ndz\nend\n".repeat(n as usize);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `$storage[$key] = v` released the value but not a non-string KEY: a loop variable another store
/// had widened to boxed Mixed storage is an owned unbox, and an inline `new P($i)` key is a fresh
/// temporary; `offsetSet()` borrows both. One 48-byte block leaked per write.
#[test]
fn test_object_offset_set_releases_a_non_string_key() {
    let source = |n: u32| {
        format!(
            r#"<?php
final class P {{ public function __construct(public int $n) {{}} }}
$s = new SplObjectStorage();
$objs = [];
for ($i = 0; $i < {n}; $i++) {{ $objs[] = new P($i); }}
foreach ($objs as $q) {{ $s[$q] = 1; }}
$q = json_decode('1');
$t = new SplObjectStorage();
for ($i = 0; $i < {n}; $i++) {{ $t[new P($i)] = $i; }}
foreach ($t as $idx => $k) {{ echo $idx, ':', $t[$k], ' '; }}
echo count($s), "\n";
"#
        )
    };
    for n in [1u32, 4] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        let expected: String = (0..n).map(|i| format!("{i}:{i} ")).collect::<String>() + &format!("{n}\n");
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `$this->map[$k] = v` on a NULLABLE object property (`?\WeakMap`, `?ArrayAccess`) read the
/// property as an owned unbox and handed it to `offsetSet()`, which only borrows it; the receiver
/// cell, a fresh key and the value were never dropped. Twig's `ExpressionParsers` leaked 14 blocks
/// per request this way. The slope test requires 1 and 4 constructions to end clean.
#[test]
fn test_array_access_write_through_a_nullable_property_does_not_leak() {
    let source = |n: u32| {
        format!(
            r#"<?php
interface P {{ public function name(): string; }}
final class A implements P {{ public function name(): string {{ return 'a'; }} }}
final class B implements P {{ public function name(): string {{ return 'b'; }} }}
final class Bag implements ArrayAccess {{
    private array $d = [];
    public function offsetExists(mixed $o): bool {{ return isset($this->d[$o]); }}
    public function offsetGet(mixed $o): mixed {{ return $this->d[$o]; }}
    public function offsetSet(mixed $o, mixed $v): void {{ $this->d[$o] = $v; }}
    public function offsetUnset(mixed $o): void {{ unset($this->d[$o]); }}
}}
final class Reg {{
    private ?\WeakMap $changes = null;
    public ?Bag $bag = null;
    public function __construct(private array $parsers) {{}}
    public function get(): \WeakMap {{
        if (null === $this->changes) {{
            $this->changes = new \WeakMap();
            foreach ($this->parsers as $p) {{
                $this->changes[$p] = array_values(array_filter($this->parsers, fn ($o) => $o !== $p));
            }}
        }}
        return $this->changes;
    }}
    public function fill(string $k): void {{
        $this->bag = new Bag();
        $this->bag['x' . $k] = [$k];
        $this->bag[$k] = 5;
    }}
}}
for ($i = 0; $i < {n}; $i++) {{
    $a = new A(); $b = new B();
    $r = new Reg([$a, $b]);
    foreach ($r->get() as $ep => $list) {{ echo $ep->name(), count($list), ' '; }}
    $r->fill('k' . $i);
    echo $r->bag['k' . $i] + 1, "\n";
}}
"#
        )
    };
    for n in [1u32, 4] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, "a1 b1 6\n".repeat(n as usize));
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `$this->p[] = $v` on a NULLABLE `ArrayAccess` property (`?ArrayObject`, `?Bag`) took the boxed
/// Mixed array append, which wrote into a detached copy: the push vanished (`count()` stayed 0).
/// php runs it as `offsetSet(null, $v)` on the object. The same shapes on an untyped property and
/// a user `ArrayAccess` class must agree, and end with a clean heap at 1 and 4 repetitions.
#[test]
fn test_push_through_a_nullable_array_access_property_reaches_the_object() {
    let source = |n: u32| {
        format!(
            r#"<?php
final class Bag implements ArrayAccess {{
    public array $d = [];
    public function offsetExists(mixed $o): bool {{ return isset($this->d[$o]); }}
    public function offsetGet(mixed $o): mixed {{ return $this->d[$o]; }}
    public function offsetSet(mixed $o, mixed $v): void {{
        if ($o === null) {{ $this->d[] = $v; }} else {{ $this->d[$o] = $v; }}
    }}
    public function offsetUnset(mixed $o): void {{ unset($this->d[$o]); }}
}}
final class H {{
    public ?ArrayObject $ao = null;
    public ?Bag $bag = null;
    public $loose = null;
    public ArrayObject $plain;
    public function run(): string {{
        $this->ao = new ArrayObject();
        $this->ao[] = 5;
        $this->ao[] = [1, 2];
        $this->ao['k'] = 6;
        $this->bag = new Bag();
        $this->bag[] = 'x';
        $this->bag[] = [7];
        $this->bag['k'] = 'y';
        $this->loose = new Bag();
        $this->loose[] = 'u';
        $this->loose['k'] = 'v';
        $this->plain = new ArrayObject();
        $this->plain[] = 1;
        return count($this->ao) . count($this->bag->d) . count($this->loose->d) . count($this->plain);
    }}
}}
for ($i = 0; $i < {n}; $i++) {{
    echo (new H())->run(), "\n";
}}
"#
        )
    };
    for n in [1u32, 4] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, "3321\n".repeat(n as usize));
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// Frames larger than the pair-store and unscaled-offset ranges (hundreds of owned string locals)
/// must assemble and unwind: the prologue zeroes the cleanup slots in runs, the activation record
/// is published by a shared helper, and an exception thrown below releases every local and keeps
/// its trace lines. 1 and 5 repetitions both end clean.
#[test]
fn test_large_frames_assemble_unwind_and_keep_trace_lines() {
    let mut body = String::new();
    for index in 0..90 {
        body.push_str(&format!("    $v{index} = $s . \"{index}\";\n"));
    }
    let sum: String = (0..90).map(|i| format!("strlen($v{i})")).collect::<Vec<_>>().join(" + ");
    let source = |n: u32| {
        format!(
            r#"<?php
function wide(string $s, bool $fail): int {{
{body}    if ($fail) {{ throw new RuntimeException("wide failed"); }}
    return {sum};
}}
function outer(string $s, bool $fail): int {{ return wide($s, $fail); }}
for ($i = 0; $i < {n}; $i++) {{
    echo outer("ab", false), "\n";
    try {{ outer("ab", true); }} catch (RuntimeException $e) {{
        $t = $e->getTrace();
        echo $e->getMessage(), ":", $e->getLine(), ":", $t[0]["function"], ":", $t[0]["line"], ":", $t[1]["function"], ":", $t[1]["line"], "\n";
    }}
}}
"#
        )
    };
    for n in [1u32, 5] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        let expected = "350\nwide failed:93:wide:96:outer:99\n".repeat(n as usize);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// An untyped property declared `= null` whose only assignments are objects was typed as that
/// object class, so `$this->p === null` folded to false and the lazy-init branch never ran
/// ("Call to a member function parse() on null").
#[test]
fn test_lazy_init_of_an_untyped_null_default_property_runs_the_null_branch() {
    let out = compile_and_run(
        r#"<?php
class Parser { function parse() { return 'ok'; } }
class A { private $p = null; function f() { if ($this->p === null) { $this->p = new Parser(); } return $this->p->parse(); } }
class B { private $p = null; function f() { if ($this->p !== null) { return "has"; } else { $this->p = new Parser(); return "made"; } } }
class C { private $p = null; function f() { $this->p ??= new Parser(); return $this->p->parse(); } }
class D { private $p = null; function f() { if (!$this->p) { $this->p = new Parser(); } return $this->p->parse(); } }
class E { private $p = null; function f() { if (!isset($this->p)) { $this->p = new Parser(); } return $this->p->parse(); } }
class F { private $p = null; function f() { if (!$this->p instanceof Parser) { $this->p = new Parser(); } return $this->p->parse(); } }
class G { private ?Parser $p = null; function f() { if ($this->p === null) { $this->p = new Parser(); } return $this->p->parse(); } }
class H { private static $p = null; static function f() { if (self::$p === null) { self::$p = new Parser(); } return self::$p->parse(); } }
class I { private $p = null; function f() { if (null == $this->p) { $this->p = new Parser(); } return $this->p->parse(); } }
class J { private $p = null; function f() { return $this->p === null ? "null" : "set"; } function g() { $this->p = new Parser(); } }
class K { private $p = null; function f() { if (is_null($this->p)) { $this->p = new Parser(); } return $this->p->parse(); } }
foreach ([new A, new B, new C, new D, new E, new F, new G, new I, new K] as $o) { echo get_class($o), ":", $o->f(), "\n"; if ($o instanceof B) echo $o->f(), "\n"; }
echo H::f(), H::f(), "\n";
$j = new J; echo $j->f(), "\n"; $j->g(); echo $j->f(), "\n";
"#,
    );
    assert_eq!(out, "A:ok\nB:made\nhas\nC:ok\nD:ok\nE:ok\nF:ok\nG:ok\nI:ok\nK:ok\nokok\nnull\nset\n");
}

/// Interpreter-declared receivers behind the shared owner-call helper: a compiled caller invoking
/// non-final methods of an object whose class only the interpreter declared reaches the bridge
/// through ONE helper per (method, argument representation, result type). Every result type,
/// the miss back to native dispatch (a method the interpreted subclass does not redeclare), a
/// protected call that needs the caller's class scope, a Throwable out of the interpreter and a
/// by-reference parameter (which keeps the inline path) must behave as the inline path did.
#[test]
fn test_owned_receiver_calls_through_the_shared_helper_match_php() {
    let out = compile_and_run(
        r####"<?php
class Base {
    protected int $n = 7;
    public function geti(int $a, int $b = 1): int { return $a + $b + $this->n; }
    public function gets(string $p): string { return $p . "-base"; }
    public function getf(float $x): float { return $x * 2; }
    public function getb(bool $f): bool { return !$f; }
    public function geto(Base $o): Base { return $o; }
    public function geta(array $a): array { return array_merge($a, ['base']); }
    public function getn(?int $v): ?int { return $v; }
    public function getm($v) { return $v; }
    public function setv(int $v): void { $this->n = $v; }
    public function other(): string { return "native-other"; }
    protected function prot(int $x): string { return "base-prot" . $x; }
    public function viaProt(Base $o, int $x): string { return $o->prot($x); }
    public function boom(): int { return 1; }
    public function refv(int &$x): void { $x += 1; }
    public function describe(): string { return static::class; }
}
final class User {
    public function drive(Base $b): string {
        $x = 5;
        $s = "str";
        $r = $b->geti($x, 2) . "|" . $b->gets($s) . "|" . $b->getf(1.5) . "|" . var_export($b->getb(true), true);
        $r .= "|" . get_class($b->geto($b)) . "|" . implode(",", $b->geta(['a'])) . "|" . var_export($b->getn(null), true) . "|" . var_export($b->getn($x), true);
        $r .= "|" . $b->getm($s);
        $b->setv(40);
        $r .= "|" . $b->geti(1);
        $r .= "|" . $b->other();
        $r .= "|" . $b->viaProt($b, 3);
        $y = 10;
        $b->refv($y);
        $r .= "|y=" . $y;
        $r .= "|" . $b->describe();
        return $r;
    }
}
eval('
class Child extends Base {
    public function geti(int $a, int $b = 1): int { return ($a + $b) * 100; }
    public function gets(string $p): string { return $p . "-child"; }
    public function getf(float $x): float { return $x * 3; }
    public function getb(bool $f): bool { return $f; }
    public function geto(Base $o): Base { return new Child(); }
    public function geta(array $a): array { $a[] = "child"; return $a; }
    public function getn(?int $v): ?int { return $v === null ? 99 : $v + 1; }
    public function getm($v) { return $v . "m"; }
    public function setv(int $v): void { $this->n = $v + 1; }
    protected function prot(int $x): string { return "child-prot" . $x; }
    public function boom(): int { throw new RuntimeException("child boom"); }
    public function refv(int &$x): void { $x += 100; }
}
');
$u = new User();
echo $u->drive(new Base()), "\n";
echo $u->drive(new Child()), "\n";
try { echo (new Child())->boom(); } catch (RuntimeException $e) { echo "caught ", $e->getMessage(), "\n"; }
function callBoom(Base $b) { try { return $b->boom(); } catch (RuntimeException $e) { return "caught " . $e->getMessage(); } }
echo callBoom(new Child()), "\n";
echo callBoom(new Base()), "\n";
"####,
    );
    assert_eq!(out, "14|str-base|3|false|Base|a,base|NULL|5|str|42|native-other|base-prot3|y=11|Base\n700|str-child|4.5|true|Child|a,child|99|6|strm|200|native-other|child-prot3|y=110|Child\ncaught child boom\ncaught child boom\n1\n");
}

/// The shared owner-call helper boxes the receiver and every argument and converts the owned
/// result exactly as the inline path did, so it strands nothing: the only live blocks at exit are
/// the two the interpreter keeps for its own class declaration (the same two a program that never
/// calls through the helper ends with).
#[test]
fn test_owned_receiver_calls_through_the_shared_helper_leak_nothing() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Base {
    protected int $n = 7;
    public function geti(int $a, int $b = 1): int { return $a + $b + $this->n; }
    public function gets(string $p): string { return $p . "-base"; }
    public function getf(float $x): float { return $x * 2; }
    public function getb(bool $f): bool { return !$f; }
    public function geto(Base $o): Base { return $o; }
    public function geta(array $a): array { return array_merge($a, ['base']); }
    public function getn(?int $v): ?int { return $v; }
    public function getm($v) { return $v; }
    public function setv(int $v): void { $this->n = $v; }
    public function other(): string { return "native-other"; }
    protected function prot(int $x): string { return "base-prot" . $x; }
    public function viaProt(Base $o, int $x): string { return $o->prot($x); }
    public function boom(): int { return 1; }
    public function refv(int &$x): void { $x += 1; }
    public function describe(): string { return static::class; }
}
final class User {
    public function drive(Base $b): string {
        $x = 5; $s = "str";
        $r = $b->geti($x, 2) . "|" . $b->gets($s) . "|" . $b->getf(1.5) . "|" . get_class($b->geto($b));
        $r .= "|" . implode(",", $b->geta(['a'])) . "|" . $b->getm($s) . "|" . $b->other() . "|" . $b->viaProt($b, 3);
        return $r . "|" . $b->describe();
    }
}
eval('
class Child extends Base {
    public function geti(int $a, int $b = 1): int { return ($a + $b) * 100; }
    public function gets(string $p): string { return $p . "-child"; }
    public function getf(float $x): float { return $x * 3; }
    public function getb(bool $f): bool { return $f; }
    public function geto(Base $o): Base { return new Child(); }
    public function geta(array $a): array { $a[] = "child"; return $a; }
    public function getn(?int $v): ?int { return $v === null ? 99 : $v + 1; }
    public function getm($v) { return $v . "m"; }
    public function setv(int $v): void { $this->n = $v + 1; }
    protected function prot(int $x): string { return "child-prot" . $x; }
    public function boom(): int { throw new RuntimeException("child boom"); }
    public function refv(int &$x): void { $x += 100; }
}
');
$u = new User();
echo $u->drive(new Base()), "\n";
echo $u->drive(new Child()), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "14|str-base|3|Base|a,base|str|native-other|base-prot3|Base\n700|str-child|4.5|Child|a,child|strm|native-other|child-prot3|Child\n"
    );
    let live_blocks: usize = out
        .stderr
        .split("leak summary: live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    assert!(live_blocks <= 2, "{}", out.stderr);
}

/// php names the DECLARING class in a binding `TypeError`: an inherited method called on a
/// subclass is `P::m()`, whatever the receiver's class, for instance, static and `parent::`/`self::`
/// calls alike. The constructor already did.
#[test]
fn test_definite_argument_type_error_names_the_declaring_class() {
    let out = compile_and_run(
        r####"<?php
class Foo {}
class P {
    public function __construct(Foo $c) {}
    public function m(Foo $x) { return 1; }
    public static function s(Foo $x) { return 1; }
}
class Q extends P {
    public function viaThis() { return $this->m(5); }
    public function viaParent() { return parent::m(5); }
    public function viaSelf() { return self::s(5); }
    public function viaStatic() { return static::s(5); }
}
class R extends Q {}
function t($f) {
    try { $f(); echo "no error\n"; } catch (TypeError $e) {
        echo substr($e->getMessage(), 0, strpos($e->getMessage(), ', called in')), "\n";
    }
}
$q = new Q(new Foo);
$r = new R(new Foo);
t(fn() => $q->m(5));
t(fn() => $r->m([]));
t(fn() => Q::s(5));
t(fn() => R::s(5));
t(fn() => new Q(5));
t(fn() => new R(5));
t(fn() => $q->viaThis());
t(fn() => $r->viaParent());
t(fn() => $r->viaSelf());
t(fn() => $r->viaStatic());
t(fn() => (new R(new Foo))->m(5));
"####,
    );
    assert_eq!(out, "P::m(): Argument #1 ($x) must be of type Foo, int given\nP::m(): Argument #1 ($x) must be of type Foo, array given\nP::s(): Argument #1 ($x) must be of type Foo, int given\nP::s(): Argument #1 ($x) must be of type Foo, int given\nP::__construct(): Argument #1 ($c) must be of type Foo, int given\nP::__construct(): Argument #1 ($c) must be of type Foo, int given\nP::m(): Argument #1 ($x) must be of type Foo, int given\nP::m(): Argument #1 ($x) must be of type Foo, int given\nP::s(): Argument #1 ($x) must be of type Foo, int given\nP::s(): Argument #1 ($x) must be of type Foo, int given\nP::m(): Argument #1 ($x) must be of type Foo, int given\n");
}

/// A spread combined with named arguments (`f(...src($x), c: 9)`) hoists a call source into one temp: the call used to run once per parameter slot and the first slot read the wrong key, dropping the spread's first element. Function-return, literal, variable and string-keyed sources, plain functions, methods, static methods and constructors all match php, each source is evaluated once, and the heap ends clean.
#[test]
fn test_spread_with_named_arguments_evaluates_the_source_once_and_binds_every_slot() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
function src(mixed $p): array { echo "[src]"; return [$p]; }
function pair(mixed $p, mixed $q): array { echo "[pair]"; return [$p, $q]; }
function named(): array { echo "[named]"; return ['b' => 20, 'a' => 10]; }
function named2(): array { echo "[named2]"; return ['y' => 'Y', 'x' => 'X']; }
function f($a, $b = 5, $c = 6) { echo $a, "|", $b, "|", $c, "\n"; }
class K {
    public function m($a, $b = 5, $c = 6) { echo "m:", $a, "|", $b, "|", $c, "\n"; }
    public static function s($a, $b = 5, $c = 6) { echo "s:", $a, "|", $b, "|", $c, "\n"; }
    public function __construct(public $x = 'dx', public $y = 'dy', public $z = 'dz') { echo "k:", $x, "|", $y, "|", $z, "\n"; }
}
function run(mixed $x): void {
    $k = new K();
    $v = [$x, $x . '2'];
    f(...src($x), c: 9);
    f(...pair($x, 7), c: 9);
    f(...src($x), b: 8);
    f(...[$x], c: 9);
    f(...$v, c: 9);
    f(...$v, c: 8);
    f(...named(), c: 9);
    f(...['a' => $x, 'c' => 4]);
    f(...['a' => $x], c: 3);
    $k->m(...src($x), c: 9);
    $k->m(...named(), c: 9);
    K::s(...src($x), c: 9);
    K::s(...pair($x, 'q'), c: 9);
    K::s(...named(), c: 9);
    new K(...src($x), z: 'zz');
    new K(...named2(), z: 'zz');
}
run(1);
run("s");
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "k:dx|dy|dz\n[src]1|5|9\n[pair]1|7|9\n[src]1|8|6\n1|5|9\n1|12|9\n1|12|8\n[named]10|20|9\n1|5|4\n1|5|3\n[src]m:1|5|9\n[named]m:10|20|9\n[src]s:1|5|9\n[pair]s:1|q|9\n[named]s:10|20|9\n[src]k:1|dy|zz\n[named2]k:X|Y|zz\nk:dx|dy|dz\n[src]s|5|9\n[pair]s|7|9\n[src]s|8|6\ns|5|9\ns|s2|9\ns|s2|8\n[named]10|20|9\ns|5|4\ns|5|3\n[src]m:s|5|9\n[named]m:10|20|9\n[src]s:s|5|9\n[pair]s:s|q|9\n[named]s:10|20|9\n[src]k:s|dy|zz\n[named2]k:X|Y|zz\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$a = &\Closure::bind(fn &() => $this->prop, $obj, $obj)()` (immediate invoke) and the stored form bind the property's reference cell: writes through the alias reach the property and back, for a string and an int property, matching php, with a clean heap. A bound receiver whose class is not statically known is still refused at compile time (the descriptor invoker cannot return a cell); pre-merge silently printed garbage for it.
#[test]
fn test_bound_closure_reference_assignment_aliases_the_property_at_runtime() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class TwinHolder { public string $text = 'init'; }
class CounterHolder { public int $n = 0; }
for ($i = 0; $i < 3; $i++) {
    $holder = new TwinHolder();
    $first = &\Closure::bind(fn &() => $this->text, $holder, $holder)();
    $bound = \Closure::bind(fn &() => $this->text, $holder, $holder);
    $second = &$bound();
    $first = "a" . $i;
    echo $first, "|", $second, "|", $holder->text, "\n";
    $second = "b" . $i;
    echo $first, "|", $second, "|", $holder->text, "\n";
    unset($first, $second);
    $counter = new CounterHolder();
    $n = &\Closure::bind(fn &() => $this->n, $counter, $counter)();
    $n += 5;
    $n++;
    echo $counter->n, "\n";
    unset($n);
}
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a0|a0|a0\nb0|b0|b0\n6\na1|a1|a1\nb1|b1|b1\n6\na2|a2|a2\nb2|b2|b2\n6\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference argument that is a NESTED array element (`keep($outer[0]["k"])`, `inc($m[1][0])`, a three-level chain, two nested places in one call) binds the element's own reference cell instead of a copy written back after the call: a callee that keeps the reference (`$this->r = &$x`) sees later writes to the element and writes back into it, as php does. The flat form already worked; the nested form used a hidden copy so the alias was lost. Output matches php and the heap ends clean.
#[test]
fn test_nested_array_element_by_reference_argument_aliases_what_the_callee_keeps() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Holder {
    public $r;
    public function keep(&$x) { $this->r = &$x; }
}
function inc(&$v) { $v++; }
function pair(&$a, &$b) { $a = 'A'; $b = 'B'; }
for ($i = 0; $i < 3; $i++) {
    $h = new Holder();
    $flat = ["k" => 1, "j" => 2];
    $h->keep($flat["k"]);
    $flat["k"] = 5 + $i;
    echo $h->r, "\n";
    $h->r = 99;
    echo json_encode($flat), "\n";
    $outer = [["k" => 1, "j" => 2], ["k" => 3]];
    $h2 = new Holder();
    $h2->keep($outer[0]["k"]);
    $outer[0]["k"] = 10 + $i;
    echo $h2->r, "\n";
    $h2->r = 77;
    echo json_encode($outer), "\n";
    $h2->keep($outer[1]["k"]);
    $outer[1]["k"] .= "x";
    echo $h2->r, "|", json_encode($outer), "\n";
    $m = [[1, 2], [3, 4]];
    inc($m[1][0]);
    inc($m[0][1]);
    foreach ($m as &$row) { inc($row[1]); }
    unset($row);
    echo json_encode($m), "\n";
    $deep = ['a' => ['b' => ['c' => 1]]];
    inc($deep['a']['b']['c']);
    pair($deep['a']['b']['x'], $deep['a']['b']['y']);
    echo json_encode($deep), "\n";
}
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "5\n{\"k\":99,\"j\":2}\n10\n[{\"k\":77,\"j\":2},{\"k\":3}]\n3x|[{\"k\":77,\"j\":2},{\"k\":\"3x\"}]\n[[1,4],[4,5]]\n{\"a\":{\"b\":{\"c\":2,\"x\":\"A\",\"y\":\"B\"}}}\n6\n{\"k\":99,\"j\":2}\n11\n[{\"k\":77,\"j\":2},{\"k\":3}]\n3x|[{\"k\":77,\"j\":2},{\"k\":\"3x\"}]\n[[1,4],[4,5]]\n{\"a\":{\"b\":{\"c\":2,\"x\":\"A\",\"y\":\"B\"}}}\n7\n{\"k\":99,\"j\":2}\n12\n[{\"k\":77,\"j\":2},{\"k\":3}]\n3x|[{\"k\":77,\"j\":2},{\"k\":\"3x\"}]\n[[1,4],[4,5]]\n{\"a\":{\"b\":{\"c\":2,\"x\":\"A\",\"y\":\"B\"}}}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An omitted `array &$left = []` default whose cell a returned closure binds with `use (&$left)` is a managed cell: the closure retains its own lease, the caller retires its, and the last release frees the cell (function, method and static method). It used to be a persistent heap cell that was never freed (3 blocks per call).
#[test]
fn test_omitted_by_ref_default_kept_by_a_closure_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
function viaClosure(array &$left = [], int $value = 0): callable {
    $left[] = $value;
    return function() use (&$left): int { $left[] = 1; return count($left); };
}
class Maker {
    public function make(array &$left = [], int $value = 0): callable {
        $left[] = $value;
        return function() use (&$left): int { $left[] = 2; return count($left); };
    }
    public static function build(array &$left = [], int $value = 0): callable {
        $left[] = $value;
        return function() use (&$left): int { $left[] = 3; return count($left); };
    }
}
for ($i = 0; $i < 3; $i++) {
    $cb = viaClosure(value: $i);
    echo $cb(), $cb(), "\n";
    unset($cb);
    $m = new Maker();
    $cb = $m->make(value: $i);
    echo $cb(), $cb(), "\n";
    unset($cb);
    $cb = Maker::build(value: $i);
    echo $cb(), $cb(), "\n";
    unset($cb);
}
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "23\n23\n23\n23\n23\n23\n23\n23\n23\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A spread that already binds a parameter a named argument then names throws php's catchable `Named parameter $x overwrites previous argument` Error (function, method, static method; literal, variable and function-return sources). It used to be an uncatchable fatal, and a literal spread silently let the named value win.
#[test]
fn test_spread_over_an_already_named_parameter_throws_a_catchable_error() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
function f($a, $b = 2, $c = 3) { return "$a|$b|$c"; }
function src() { return [1, 2]; }
class K {
    public function m($a, $b = 2, $c = 3) { return "$a|$b|$c"; }
    public static function s($a, $b = 2, $c = 3) { return "$a|$b|$c"; }
}
$arr = [1, 2];
$one = [1];
$tests = [
    fn() => f(...$arr, b: 9),
    fn() => f(...[1, 2], a: 9),
    fn() => f(...[1, 2], b: 9),
    fn() => f(...src(), b: 9),
    fn() => f(...src(), c: 9),
    fn() => (new K)->m(...$arr, a: 5),
    fn() => (new K)->m(...$one, c: 5),
    fn() => K::s(...$arr, b: 5),
    fn() => K::s(...[1, 2], a: 5),
    fn() => f(...$one, b: 7),
];
for ($round = 0; $round < 2; $round++) {
    foreach ($tests as $i => $t) {
        try { echo $i, ": ", $t(), "\n"; }
        catch (Error $e) { echo $i, ": ", get_class($e), ": ", $e->getMessage(), "\n"; }
    }
}
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0: 0: Error: Named parameter $b overwrites previous argument\n1: 1: Error: Named parameter $a overwrites previous argument\n2: 2: Error: Named parameter $b overwrites previous argument\n3: 3: Error: Named parameter $b overwrites previous argument\n4: 1|2|9\n5: 5: Error: Named parameter $a overwrites previous argument\n6: 1|2|5\n7: 7: Error: Named parameter $b overwrites previous argument\n8: 8: Error: Named parameter $a overwrites previous argument\n9: 1|7|3\n0: 0: Error: Named parameter $b overwrites previous argument\n1: 1: Error: Named parameter $a overwrites previous argument\n2: 2: Error: Named parameter $b overwrites previous argument\n3: 3: Error: Named parameter $b overwrites previous argument\n4: 1|2|9\n5: 5: Error: Named parameter $a overwrites previous argument\n6: 1|2|5\n7: 7: Error: Named parameter $b overwrites previous argument\n8: 8: Error: Named parameter $a overwrites previous argument\n9: 1|7|3\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
