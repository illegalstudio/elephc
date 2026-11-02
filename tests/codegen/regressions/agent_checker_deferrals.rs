//! Purpose:
//! Regression tests for framework files the type checker used to refuse, which handed them to
//! the runtime interpreter instead of compiling them natively.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test names the vendor file and the refusal it fixes, uses only standalone PHP, and
//!   asserts php's exact output (and a clean heap where values are allocated).

use crate::support::*;

/// symfony/ux-twig-component `ComputedPropertiesProxy.php`: a property declared `object`, read as
/// `$this->component[$name]` after `instanceof \ArrayAccess && isset(...)`, was refused with
/// "Cannot index non-array".
#[test]
fn test_bare_object_property_indexed_after_array_access_guard() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P {
    public function __construct(private object $component) {}
    public function get(string $name) {
        if ($this->component instanceof \ArrayAccess && isset($this->component[$name])) {
            return $this->component[$name];
        }
        return "none";
    }
}
echo (new P(new ArrayObject(["a" => "A"])))->get("a"), (new P(new ArrayObject([])))->get("z"), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Anone\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/monolog-bundle `AddProcessorsPass.php`: a callback written as an assignment,
/// `array_any($tags, $closure = function (...) {...})`, and reused on the next line was refused
/// with "Undefined variable: $closure".
#[test]
fn test_closure_assigned_inside_callback_argument_stays_bound() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$tags = [[1], [], [2]];
if (array_any($tags, $closure = function (array $tag) { return (bool) $tag; })) {
    $tags = array_values(array_filter($tags, $closure));
}
echo count($tags), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/form `ChoiceType.php`: `$unknownValues = []` outside, `use (&$unknownValues)` and
/// `$unknownValues = $valueMap` (an `array_flip()` result) inside was refused with
/// "cannot reassign $unknownValues from array<never> to array".
#[test]
fn test_by_ref_captured_empty_array_takes_a_php_array_result() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function h() {
    $u = [];
    $f = static function ($data) use (&$u) {
        $vm = array_flip(array_filter($data, static fn($v) => null !== $v));
        $u = $vm;
        return count($u);
    };
    return $f(["a", "b", null]) . ":" . count($u);
}
echo h(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "2:2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// league/uri `UriTemplate/Template.php::expand`: `if (!$variables instanceof VariableBag)
/// { $variables = new VariableBag($variables); }` followed by a call taking a `VariableBag` was
/// refused with "parameter $variables expects Object(VariableBag), got Iterable".
#[test]
fn test_iterable_parameter_replaced_by_object_in_instanceof_guard() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class VB implements IteratorAggregate {
    private array $d;
    public function __construct(iterable $v = []) { $this->d = is_array($v) ? $v : iterator_to_array($v); }
    public function getIterator(): Iterator { return new ArrayIterator($this->d); }
}
function expandAll(VB $b): string { return implode(",", iterator_to_array($b)); }
function expand(iterable $variables = []): string {
    if (!$variables instanceof VB) {
        $variables = new VB($variables);
    }
    return expandAll($variables);
}
echo expand(["a", "b"]), "\n";
echo expand(new VB(["c"])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a,b\nc\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// league/uri `Template.php::expand` (second shape): re-binding a by-value `iterable` parameter to
/// an object that is itself iterable and then reading it through a builtin leaked the caller's
/// array, because the boxed parameter slot's read retained a payload nothing released.
#[test]
fn test_iterable_parameter_rebound_to_iterable_object_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class VB implements IteratorAggregate {
    private array $d;
    public function __construct(array $v = []) { $this->d = $v; }
    public function getIterator(): Iterator { return new ArrayIterator($this->d); }
}
function expand(iterable $variables): string {
    $variables = new VB($variables);
    return implode(",", iterator_to_array($variables));
}
echo expand(["a", "b"]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a,b\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Shared fixture for the object-key tests: a user `ArrayAccess` keyed by object identity.
const OBJECT_KEY_FIXTURE: &str = r#"<?php
final class IdMap implements ArrayAccess {
    private array $items = [];
    public function offsetExists(mixed $o): bool { return isset($this->items[spl_object_id($o)]); }
    public function offsetGet(mixed $o): mixed { return $this->items[spl_object_id($o)]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->items[spl_object_id($o)] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->items[spl_object_id($o)]); }
}
function mk(int $kind): mixed {
    if ($kind === 0) { return new WeakMap(); }
    if ($kind === 1) { return new SplObjectStorage(); }
    return new IdMap();
}
function put(mixed $m, object $k, mixed $v): void { $m[$k] = $v; }
function fetch(mixed $m, object $k): mixed { return $m[$k]; }
function has(mixed $m, object $k): bool { return isset($m[$k]); }
function orElse(mixed $m, object $k): mixed { return $m[$k] ?? 'none'; }
function drop(mixed $m, object $k): void { unset($m[$k]); }
"#;

/// Laravel `Foundation/Exceptions/Handler.php`: `$this->reportedExceptionMap[$e] = true` and
/// `$this->reportedExceptionMap[$e] ?? false` over a `mixed` container. Reads returned null (a
/// silent miscompile) and a statically object-typed key was refused at hash-key materialization.
#[test]
fn test_object_keys_on_a_runtime_container_dispatch_to_array_access() {
    let body = r#"
$a = new Exception('a');
$b = new Exception('b');
foreach ([0, 1, 2] as $kind) {
    $m = mk($kind);
    put($m, $a, 'va');
    echo $kind, ':', fetch($m, $a), ',', var_export(has($m, $a), true), ',', var_export(has($m, $b), true),
        ',', orElse($m, $a), ',', orElse($m, $b);
    drop($m, $a);
    echo ',', var_export(has($m, $a), true), "\n";
}
"#;
    let out = compile_and_run_with_heap_debug(&format!("{}{}", OBJECT_KEY_FIXTURE, body));
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0:va,true,false,va,none,false\n1:va,true,false,va,none,false\n2:va,true,false,va,none,false\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The Laravel shape itself: an UNTYPED property initialised to `new WeakMap`, written and read
/// with a `Throwable`-typed key.
#[test]
fn test_untyped_property_weakmap_with_throwable_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Handler {
    protected $reported;
    protected $withoutDuplicates = true;
    public function __construct() { $this->reported = new WeakMap(); }
    public function report(Throwable $e): void { $this->reported[$e] = true; }
    public function seen(Throwable $e): bool {
        return $this->withoutDuplicates && ($this->reported[$e] ?? false);
    }
    public function has(Throwable $e): bool { return isset($this->reported[$e]); }
    public function forget(Throwable $e): void { unset($this->reported[$e]); }
}
$h = new Handler();
$a = new RuntimeException('a');
$b = new LogicException('b');
$h->report($a);
echo var_export($h->seen($a), true), var_export($h->seen($b), true), var_export($h->has($a), true), "\n";
$h->forget($a);
echo var_export($h->seen($a), true), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "truefalsetrue\nfalse\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An object key into a container that turns out to be a plain array is php's TypeError.
#[test]
fn test_object_key_on_array_container_raises_type_error() {
    let out = compile_and_run(
        r#"<?php
class T {}
function put(mixed $m, T $k): void { $m[$k] = 1; }
function fetch(mixed $m, T $k): mixed { return $m[$k]; }
try { put([], new T()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { fetch([], new T()); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Cannot access offset of type T on array\nCannot access offset of type T on array\n"
    );
}

/// Symfony `debug:router`: `#[Route]` attributes whose named arguments are out of declaration
/// order lost their arguments in `ReflectionAttribute::newInstance()` when the attribute object
/// was materialized at run time (a run-time method name, a filtered `getAttributes()`): the
/// spread of the retained argument hash was listed through `array_values()` before the name
/// probes ran, so `'n'` landed in the `requirements` array parameter -- a TypeError.
#[test]
fn test_new_instance_binds_named_arguments_for_runtime_materialized_attributes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[Attribute(Attribute::TARGET_ALL)]
final class R {
    public function __construct(
        public string $path,
        public array $requirements = [],
        public ?string $name = null,
        public array $methods = [],
    ) {}
}
#[R('/cls', name: 'n', requirements: ['p' => 'lit'], methods: ['GET'])]
final class Ctl {
    #[R('/prop', methods: ['POST'], name: 'pn')]
    public int $prop = 0;
    #[R(path: '/skip', methods: ['PUT'])]
    public function skipped(#[R('/param', name: 'arg')] int $x = 0): void {}
    #[R('/m', name: 'n', requirements: ['p' => 'lit'], methods: ['GET'])]
    public function m(): void {}
}
function show(R $r): string { return json_encode([$r->path, $r->requirements, $r->name, $r->methods], JSON_UNESCAPED_SLASHES); }
$member = $argc > 5 ? 'zz' : 'm';
$skipped = $argc > 5 ? 'zz' : 'skipped';
$prop = $argc > 5 ? 'zz' : 'prop';
$class = $argc > 5 ? 'zz' : Ctl::class;
echo show((new ReflectionMethod(Ctl::class, $member))->getAttributes()[0]->newInstance()), "\n";
echo show((new ReflectionMethod(Ctl::class, 'm'))->getAttributes(R::class)[0]->newInstance()), "\n";
echo show((new ReflectionMethod(Ctl::class, $skipped))->getAttributes()[0]->newInstance()), "\n";
echo show((new ReflectionProperty(Ctl::class, $prop))->getAttributes()[0]->newInstance()), "\n";
echo show((new ReflectionClass($class))->getAttributes(R::class)[0]->newInstance()), "\n";
echo show((new ReflectionMethod(Ctl::class, $skipped))->getParameters()[0]->getAttributes()[0]->newInstance()), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "[\"/m\",{\"p\":\"lit\"},\"n\",[\"GET\"]]\n",
            "[\"/m\",{\"p\":\"lit\"},\"n\",[\"GET\"]]\n",
            "[\"/skip\",[],null,[\"PUT\"]]\n",
            "[\"/prop\",[],\"pn\",[\"POST\"]]\n",
            "[\"/cls\",{\"p\":\"lit\"},\"n\",[\"GET\"]]\n",
            "[\"/param\",[],\"arg\",[]]\n",
        )
    );
    // Heap cleanliness is not asserted: constructing any Reflection* object (`new ReflectionClass`,
    // a run-time-named `ReflectionMethod`) leaks on its own, with or without attributes.
}

/// Laravel `DatabaseUserProvider::getGenericUser`, `TableGuesser::guess`,
/// `ProviderRepository::loadManifest`, `Onceable::tryFromTrace`,
/// `AbstractRouteCollection::getRouteForMethods`: a function with NO declared return type whose
/// end is reachable after `return <value>;` inferred a non-null type, and its default return had
/// no value of that type to produce (the backend refused the file). php answers null there.
#[test]
fn test_implicit_fall_off_return_is_null_for_every_inferred_type() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class X { public int $v = 1; }
function fo(int $a) { if ($a > 0) { return new X; } }
function fa(int $a) { if ($a > 0) { return [1, 2]; } }
function fs(int $a) { if ($a > 0) { return "s"; } }
function fi(int $a) { if ($a > 0) { return 7; } }
function fl(int $a) { foreach ([1, 2] as $n) { if ($n === $a) { return new X; } } }
function fx(int $a) { if ($a > 0) { return new X; } throw new Exception("no"); }
function fy(int $a) { if ($a > 0) { return new X; } else { return new X; } }
function fz(int $a) { while (true) { if ($a > 0) { return "z"; } $a++; } }
class K {
    public function mo(int $a) { if ($a > 0) { return new X; } }
    public static function ma(int $a) { if ($a > 0) { return [3]; } }
}
$cl = function (int $a) { if ($a > 0) { return new X; } };
foreach ([1, 0] as $n) {
    echo var_export(fo($n) === null, true), var_export(fa($n) === null, true), var_export(fs($n) === null, true),
        var_export(fi($n) === null, true), var_export(fl($n) === null, true),
        var_export((new K)->mo($n) === null, true), var_export(K::ma($n) === null, true),
        var_export($cl($n) === null, true), "\n";
}
var_dump(fo(0), fi(0));
echo fx(1)->v, fy(2)->v, fz(0), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "falsefalsefalsefalsefalsefalsefalsefalse\ntruetruetruetruetruetruetruetrue\nNULL\nNULL\n11z\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Doctrine ORM `Tools/Pagination/Paginator.php`: `array_map('current', $rows)` /
/// `array_map('key', $rows)` was refused ("current() argument must be array") and, once accepted,
/// died at run time because the internal-pointer builtins were not runtime-callable strings.
/// php warns for `reset`/`end` (by-reference parameter, value given) and still answers the same
/// values; the warning is not reproduced.
#[test]
fn test_internal_pointer_builtins_are_string_callables() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [['a', 'b'], [3, 4], []];
echo json_encode(array_map('current', $rows)), "\n";
echo json_encode(array_map('key', $rows)), "\n";
echo json_encode(array_map('reset', $rows)), "\n";
echo json_encode(array_map('end', $rows)), "\n";
$one = ['x' => 5, 'y' => 6];
echo json_encode(call_user_func('current', $one)), json_encode(call_user_func('key', $one)), "\n";
echo json_encode(call_user_func('reset', $one)), json_encode(call_user_func('end', $one)), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "[\"a\",3,false]\n[0,0,null]\n[\"a\",3,false]\n[\"b\",4,false]\n5\"x\"\n56\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The long forms of `fputcsv()` (`$escape`, `$eol`) and `fgetcsv()` (`$enclosure`, `$escape`),
/// including `$escape = ''`, and `fputcsv()` over fields that are not all strings. The native
/// runtimes knew only the default dialect and refused any other argument count. php 8.4+ also
/// prints a deprecation when `$escape` is omitted; that notice is not reproduced (expected
/// output is php's with it silenced).
#[test]
fn test_csv_long_forms_follow_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$h = fopen('php://memory', 'w+');
$rows = [['a b', 'c"d', 'e\\"f', "g\nh", 'plain', ''], ['x,y', "t\\", 3, 1.5, true, null]];
$n = 0;
foreach ($rows as $r) { $n += fputcsv($h, $r); }
foreach ($rows as $r) { $n += fputcsv($h, $r, ';', "'", "\\"); }
foreach ($rows as $r) { $n += fputcsv($h, $r, ',', '"', ''); }
foreach ($rows as $r) { $n += fputcsv($h, $r, ',', '"', '\\', "\r\n"); }
foreach ($rows as $r) { $n += fputcsv($h, $r, '|', '"', '', "\n--\n"); }
echo $n, "\n";
rewind($h);
echo json_encode(stream_get_contents($h)), "\n";
rewind($h); ftruncate($h, 0);
fwrite($h, "a,\"b\\\"c\",d\n\"x\\\"y\",z\n\"q\"\"r\",s\n\"multi\nline\",t\r\n\n");
rewind($h);
while (($row = fgetcsv($h, null, ',', '"', '')) !== false) { echo json_encode($row), "\n"; }
rewind($h);
while (($row = fgetcsv($h, null, ',', '"', '\\')) !== false) { echo json_encode($row), "\n"; }
rewind($h);
while (($row = fgetcsv($h, null, ',', "'")) !== false) { echo json_encode($row), "\n"; }
echo json_encode(str_getcsv("a,\"b\\\"c\",d", ',', '"', '')), json_encode(str_getcsv("a,\"b\\\"c\",d", ',', '"', '\\')), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "264\n",
            "\"\\\"a b\\\",\\\"c\\\"\\\"d\\\",\\\"e\\\\\\\"f\\\",\\\"g\\nh\\\",plain,\\n\\\"x,y\\\",\\\"t\\\\\\\",3,1.5,1,\\n'a b';c\\\"d;'e\\\\\\\"f';'g\\nh';plain;\\nx,y;'t\\\\';3;1.5;1;\\n\\\"a b\\\",\\\"c\\\"\\\"d\\\",\\\"e\\\\\\\"\\\"f\\\",\\\"g\\nh\\\",plain,\\n\\\"x,y\\\",t\\\\,3,1.5,1,\\n\\\"a b\\\",\\\"c\\\"\\\"d\\\",\\\"e\\\\\\\"f\\\",\\\"g\\nh\\\",plain,\\r\\n\\\"x,y\\\",\\\"t\\\\\\\",3,1.5,1,\\r\\n\\\"a b\\\"|\\\"c\\\"\\\"d\\\"|\\\"e\\\\\\\"\\\"f\\\"|\\\"g\\nh\\\"|plain|\\n--\\nx,y|t\\\\|3|1.5|1|\\n--\\n\"\n",
            "[\"a\",\"b\\\\c\\\"\",\"d\"]\n",
            "[\"x\\\\y\\\"\",\"z\"]\n",
            "[\"q\\\"r\",\"s\"]\n",
            "[\"multi\\nline\",\"t\"]\n",
            "[null]\n",
            "[\"a\",\"b\\\\\\\"c\",\"d\"]\n",
            "[\"x\\\\\\\"y\",\"z\"]\n",
            "[\"q\\\"r\",\"s\"]\n",
            "[\"multi\\nline\",\"t\"]\n",
            "[null]\n",
            "[\"a\",\"\\\"b\\\\\\\"c\\\"\",\"d\"]\n",
            "[\"\\\"x\\\\\\\"y\\\"\",\"z\"]\n",
            "[\"\\\"q\\\"\\\"r\\\"\",\"s\"]\n",
            "[\"\\\"multi\"]\n",
            "[\"line\\\"\",\"t\"]\n",
            "[null]\n",
            "[\"a\",\"b\\\\c\\\"\",\"d\"][\"a\",\"b\\\\\\\"c\",\"d\"]\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine/orm `Cache/DefaultEntityHydrator.php`, `TimestampQueryCacheValidator.php` and symfony/ux-twig-component `AsTwigComponent.php`: a property read through a (nullable) interface that declares no storage (`$entry->class` over a marker `CacheEntry`, `$this->template->method` after `instanceof FromMethod`) was refused by the backend with "unknown class CacheEntry", and typed `int` once a ternary arm hid the flow fact.
#[test]
fn test_property_read_through_a_nullable_interface_dispatches_to_implementers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface Entry {}
final class EA implements Entry { public function __construct(public string $class, public int $time = 5) {} }
final class EB implements Entry { public function __construct(public string $class, public int $time = 9) {} }
function cls(?Entry $e): ?string { return $e === null ? null : $e->class; }
function after(Entry $entry, ?Entry $ts): bool { return $ts && $ts->time > $entry->time; }
echo cls(new EA('A')), cls(new EB('B')), var_export(cls(null), true), "\n";
var_dump(after(new EA('x'), new EB('y')), after(new EB('x'), new EA('y')), after(new EA('x'), null));
interface Cfg {}
final class FromMethod implements Cfg { public function __construct(public string $method) {} }
final class Cmp { public function __construct(public string|Cfg|null $template = null) {}
  public function conf(): array { $c = []; if ($this->template instanceof FromMethod) { $c['m'] = $this->template->method; } else { $c['m'] = 'none'; } return $c; } }
echo (new Cmp(new FromMethod('mm')))->conf()['m'], (new Cmp('t'))->conf()['m'], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "ABNULL\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
            "mmnone\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine/persistence `EnumReflectionProperty.php`: `$enum->value` over a `BackedEnum|array` parameter (heap cleanliness is not asserted: enum case singletons stay live at exit, with or without this shape).
#[test]
fn test_backed_enum_union_parameter_reads_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
enum S: string { case A = "a"; case B = "b"; }
function en(BackedEnum|array $e) { if (is_array($e)) { return array_map(static fn (BackedEnum $x) => $x->value, $e); } return $e->value; }
echo en(S::A), json_encode(en([S::A, S::B])), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "a[\"a\",\"b\"]\n",
        )
    );
}

/// symfony/ux-live-component `LiveComponentHydrator.php`: `new \LogicException(throw new \LogicException(...))` was refused with "parameter $message expects Str, got Void"; a `throw` expression is the bottom type.
#[test]
fn test_throw_expression_is_accepted_as_a_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function t1(bool $b) { if ($b) { throw new LogicException(throw new RuntimeException("inner")); } return 1; }
try { t1(true); } catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
echo t1(false), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "RuntimeException:inner\n",
            "1\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/form `BirthdayType.php` (`range(date('Y') - 120, date('Y'))`), symfony/validator `WeekValidator.php` (`mktime(..., $year)` over an exploded string), symfony/translation `MoFileLoader.php` (`unpack('V1', ...)`) and the callable-array `count()` of ux-live-component `LiveComponentSubscriber.php`.
#[test]
fn test_numeric_string_endpoints_reach_integer_builtins() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo json_encode(range(date('Y') - 120 > 0 ? 1 : 2, date('Y') > 0 ? 3 : 4)), "\n";
$s = (string)(2 + (int)(date('Y') > 0)); // "3"
echo json_encode(range(1, $s)), json_encode(range($s, 5)), "\n";
$year = explode('-W', '2024-W05', 2)[0];
echo date('W', mktime(0, 0, 0, 12, 28, $year)), "\n";
$m = unpack('V1', "\x01\x02\x03\x04");
echo json_encode($m), dechex(current($m)), "\n";
echo json_encode(unpack('V2a/n', "\x01\x00\x00\x00\x02\x00\x00\x00\x00\x05")), "\n";
echo json_encode(unpack('C2x/C', "\x01\x02\x03")), json_encode(unpack('V1v', "\x01\x00\x00\x00")), "\n";
function cnt($c) { return !is_array($c) || 2 !== count($c); }
echo var_export(cnt([1, 2]), true), var_export(cnt('x'), true), "\n";
function rnd(mixed $value, int $precision) { return round($value, $precision, PHP_ROUND_HALF_EVEN); }
echo rnd(2.5, 0), rnd(3.14159, 2), rnd(7, 0), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "[1,2,3]\n",
            "[1,2,3][3,4,5]\n",
            "52\n",
            "{\"1\":67305985}4030201\n",
            "{\"a1\":1,\"a2\":2,\"1\":5}\n",
            "{\"x1\":1,\"x2\":2,\"1\":3}{\"v\":1}\n",
            "falsetrue\n",
            "23.147\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/polyfill-intl-icu `NumberFormatter.php`: `round($value, $precision)` with a `?int` precision (a tagged scalar) was refused by the backend.
#[test]
fn test_round_precision_accepts_a_nullable_integer_and_callable_arrays_count() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class NF {
    private static array $map = [1 => PHP_ROUND_HALF_UP, 2 => PHP_ROUND_HALF_EVEN];
    private function uninit($value, $precision) { return null === $precision ? (is_int($value) ? 0 : 3) : $precision; }
    public function round($value, $precision, int $mode) {
        $precision = $this->uninit($value, $precision);
        if (isset(self::$map[$mode])) { $value = round($value, $precision, self::$map[$mode]); }
        return $value . "|" . round($value, $precision);
    }
}
$n = new NF;
echo $n->round(2.34567, null, 1), $n->round(2.5, 0, 2), $n->round(7, null, 2), "\n";
function maybe(): ?int { return $GLOBALS['argc'] > 5 ? 1 : null; }
function r2($v) { $p = maybe(); return round($v, $p ?? 1) . round($v, maybe() ?? 0); }
echo r2(1.25), "\n";
function r3($v) { $p = maybe(); if ($p === null) { $p = 2; } return round($v, $p); }
echo r3(1.2345), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "2.346|2.3462|27|7\n",
            "1.31\n",
            "1.23\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// doctrine/orm `Query/Parser.php` and `SqlWalker.php`, symfony/type-info `StringTypeResolver.php`: a `Node` / interface argument for a parameter declared as a union of its subclasses is php's run-time check (TypeError for the other classes), not a compile-time refusal.
#[test]
fn test_object_argument_of_a_supertype_narrows_to_a_class_union_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
abstract class Node {}
final class Lit extends Node { public function __construct(public string $v) {} }
final class Param extends Node {}
final class Other extends Node {}
interface Cond {}
final class CT extends Node implements Cond {}
final class CP extends Node implements Cond {}
final class Like { public function __construct(public Lit|Param $p) {} }
function mk(Node $n): string { try { return get_class((new Like($n))->p); } catch (TypeError $e) { return 'TE'; } }
echo mk(new Lit('a')), mk(new Param), mk(new Other), "\n";
final class W { public function walk(CT|CP $c): string { return get_class($c); }
  public function go(Cond $c): string { try { return $this->walk($c); } catch (TypeError $e) { return 'TE'; } } }
echo (new W)->go(new CT), (new W)->go(new CP), "\n";
function arr(array $a): string { return 'arr'; }
function viaMixed(mixed $d): string { try { return arr($d); } catch (TypeError $e) { return 'TE'; } }
echo viaMixed([1]), viaMixed('s'), "\n";
function nul(?Lit $l): string { return $l === null ? 'null' : 'lit'; }
function viaInt(mixed $i): string { try { return nul($i); } catch (TypeError $e) { return 'TE'; } }
echo viaInt(null), viaInt(new Lit('x')), viaInt(5), "\n";
final class Tok { public function __construct(public string|DateTimeInterface $d) {} }
function dt(string|false $s) { try { return (new Tok($s))->d instanceof DateTimeInterface ? 'dt' : 's'; } catch (TypeError $e) { return 'TE'; } }
echo dt('2020-01-01'), dt(false), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "LitParamTE\n",
            "CTCP\n",
            "arrTE\n",
            "nulllitTE\n",
            "ss\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/mime `CharacterStream.php`: `$this->charCount += ($strlen - $ignoredL) / $this->fixedWidth` stores an int division (a float) into an `int` property; php coerces it.
#[test]
fn test_float_stored_into_an_int_property_is_coerced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface DI { public function pos(): int; }
final class D implements DI { public function __construct(private int $p) {} public function pos(): int { return $this->p; } }
final class Stack {
    private static function getIndex(?DI $d): int { return $d === null ? -1 : $d->pos(); }
    public function find(int|DI|null $bottom): int { return \is_int($bottom) ? $bottom : self::getIndex($bottom); }
}
$s = new Stack; echo $s->find(3), $s->find(new D(9)), $s->find(null), "\n";

final class CS {
    private int $charCount = 0;
    private int $fixedWidth = 2;
    public function add(string $chars): int {
        $strlen = \strlen($chars);
        $ignoredL = $strlen % $this->fixedWidth;
        $this->charCount += ($strlen - $ignoredL) / $this->fixedWidth;
        return $this->charCount;
    }
}
$c = new CS; echo $c->add("abcdef"), $c->add("abcde"), "\n";

class Prov { public function __construct(private array $providers = []) {} public function go(iterable $providers = []) {
  if (!\is_array($providers)) { $providers = iterator_to_array($providers, false); }
  return implode(',', $providers); } }
echo (new Prov)->go(['a', 'b']), (new Prov)->go(new ArrayIterator(['z'])), "\n";

final class V { public function validate($value) {
    $checker = new ArrayObject(['x' => 1]);
    if (!$checker->offsetExists('x')) { return 'none'; }
    $ok = $this->probe($value, $errorCode);
    return $ok ? 'ok' : 'err' . $errorCode; }
  private function probe($v, &$code) { $code = 7; return $v > 1; } }
echo (new V)->validate(0), (new V)->validate(5), "\n";
function fm($paramFirst) { $a = [3, 4]; $x = current($a); $k = key($a); return $x . $k . current($paramFirst) . ($paramFirst ? 'y' : 'n'); }
echo fm([8, 9]), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "39-1\n",
            "35\n",
            "a,bz\n",
            "err7ok\n",
            "308y\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/form `ResizeFormListener.php`: `iterator_to_array($event->getForm())` where `FormInterface extends Traversable` was refused ("must be a statically known array or Traversable").
#[test]
fn test_iterator_to_array_over_an_interface_extending_traversable() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface FI extends \ArrayAccess, \Traversable, \Countable {}
final class F implements \IteratorAggregate, FI {
    private array $c = ['a' => 1, 'b' => 2];
    public function getIterator(): Traversable { return new ArrayIterator($this->c); }
    public function offsetExists(mixed $o): bool { return isset($this->c[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->c[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->c[$o] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->c[$o]); }
    public function count(): int { return count($this->c); }
}
final class Ev { public function __construct(private FI $f) {} public function getForm(): FI { return $this->f; } }
final class L { private array $stack = [];
  public function pre(Ev $e): void { $this->stack[] = iterator_to_array($e->getForm()); }
  public function dump(): string { return json_encode($this->stack); } }
$l = new L; $l->pre(new Ev(new F)); echo $l->dump(), "\n";
function rows(): mixed { return [['x', 'y'], ['z']]; }
echo array_sum(array_map('current', array_map(fn($r) => [count($r)], rows()))), json_encode(array_map('current', rows())), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "[{\"a\":1,\"b\":2}]\n",
            "3[\"x\",\"z\"]\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony/console-style option reflection: `new ReflectionFunction($cmd->__invoke(...))` then
/// parameter attributes through `getAttributes(Class, IS_INSTANCEOF)` and `newInstance()`. Each
/// pass leaked 24 blocks (unreleased attribute objects and array-store owners, plus the
/// `__invoke(...)` descriptor); the slope test compiles the loop at 1 and 12 iterations and
/// requires both to be clean, so any per-iteration growth fails.
#[test]
fn test_reflection_function_parameter_attribute_loop_does_not_leak_per_iteration() {
    let source = |n: u32| {
        format!(
            r#"<?php
#[Attribute(Attribute::TARGET_PARAMETER)]
final class Opt {{ public function __construct(public string $description = '', public ?string $name = null) {{}} }}
final class Cmd {{
    public function __invoke(object $in, #[Opt('Send to', 'send-to')] ?string $email = null, #[Opt('Limit')] int $maxResults = 50): int {{ return 0; }}
}}
function describe(callable $code): void {{
    $r = new ReflectionFunction($code);
    foreach ($r->getParameters() as $p) {{
        $attrs = $p->getAttributes(Opt::class, ReflectionAttribute::IS_INSTANCEOF);
        echo $p->getName(), ' ', count($attrs);
        foreach ($attrs as $a) {{ $o = $a->newInstance(); echo ' ', $o->description, ' ', var_export($o->name, true); }}
        echo "\n";
    }}
}}
$cmd = new Cmd();
for ($i = 0; $i < {n}; $i++) {{ describe($cmd->__invoke(...)); }}
"#
        )
    };
    let one = "in 0\nemail 1 Send to 'send-to'\nmaxResults 1 Limit NULL\n";
    for n in [1u32, 12] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, one.repeat(n as usize));
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// `$o->__invoke(...)` builds a fresh bound descriptor that owns its receiver; a discarded or
/// reassigned one used to keep a 96-byte block and a receiver reference per evaluation.
#[test]
fn test_invoke_first_class_callable_is_released() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Cmd { public function __invoke(int $a = 1): int { return $a + 1; } }
function d(callable $c): int { return $c(2); }
$cmd = new Cmd();
$t = 0;
for ($i = 0; $i < 6; $i++) { $t += d($cmd->__invoke(...)); $x = $cmd->__invoke(...); $t += $x(); }
echo $t, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "30\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// One shared helper spells the "X given" word of every parameter wrong-tag `TypeError`:
/// `true`/`false` by value, the class name for an object, `Closure` for a closure. The
/// inlined ladders it replaced were ~60 instructions and eleven throw records per guard.
#[test]
fn test_wrong_tag_parameter_type_error_names_the_given_value_like_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
namespace App\Models;
class Foo {}
function f(array $a): int { return count($a); }
function g(?array $a): int { return $a === null ? 0 : count($a); }
function pick(int $k): mixed {
    return match ($k) {
        0 => 7, 1 => 'str', 2 => 1.5, 3 => true, 4 => false, 5 => null, 6 => new Foo(), 7 => [1, 2], 8 => static fn() => 1,
    };
}
for ($k = 0; $k <= 8; $k++) {
    try { echo f(pick($k)), "\n"; }
    catch (\TypeError $e) { echo substr(strrchr(explode(', called', $e->getMessage())[0], ','), 2), "\n"; }
    try { echo g(pick($k)), "\n"; }
    catch (\TypeError $e) { echo substr(strrchr(explode(', called', $e->getMessage())[0], ','), 2), "\n"; }
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "int given\nint given\nstring given\nstring given\nfloat given\nfloat given\n",
            "true given\ntrue given\nfalse given\nfalse given\nnull given\n0\n",
            "App\\Models\\Foo given\nApp\\Models\\Foo given\n2\n2\nClosure given\nClosure given\n",
        )
    );
}

/// `$r = new ReflectionMethod(..); $ps = $r->getParameters();` inside a top-level loop leaked ~176
/// blocks per iteration. The `if` join made `$ps` a boxed `mixed` for the rest of the function, but
/// the edge that converted the array (`array<ReflectionParameter>` to boxed elements) was lowered
/// while the slot still looked like a raw array, so its store never released the previous box.
/// The slope test requires 1 and 6 iterations to leave the same (clean) heap.
#[test]
fn test_array_local_converted_at_a_join_releases_the_previous_box_in_a_loop() {
    let source = |n: u32| {
        format!(
            r#"<?php
final class Cmd {{
    public function __invoke(object $in, ?string $email = null, int $max = 50): int {{ return 0; }}
}}
$mode = $argc > 5 ? 'b' : 'a';
for ($i = 0; $i < {n}; $i++) {{
    if ($mode === 'a') {{ $r = new ReflectionMethod(Cmd::class, '__invoke'); $q = $r->getParameters(); }}
    if ($mode === 'b') {{ $r = new ReflectionMethod(Cmd::class, '__invoke'); $q = $r->getParameters(); }}
}}
echo count($q), ' ', $q[1]->getName(), "\n";
"#
        )
    };
    for n in [1u32, 6] {
        let out = compile_and_run_with_heap_debug(&source(n));
        assert!(out.success, "program failed: {}", out.stderr);
        assert_eq!(out.stdout, "3 email\n");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "n={}: {}", n, out.stderr);
    }
}

/// symfony/form `CollectionType::finishView`: `array_splice($entryView->vars['block_prefixes'], ...)`
/// where `$entryView` comes out of an iteration and the receiver is an element of a property read
/// through a boxed object. The splice now goes through a temporary and writes the place back.
#[test]
fn test_array_splice_on_a_nested_property_element_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class V {
    public array $vars = ['bp' => ['a', 'b', 'c'], 'n' => 5];
    public array $kids = [];
}
function run(array $views): void {
    foreach ($views as $v) {
        $removed = array_splice($v->vars['bp'], 1, 1, ['X', 'Y']);
        echo implode(',', $removed), '|', implode(',', $v->vars['bp']), "\n";
        array_splice($v->vars['bp'], -1);
        echo implode(',', $v->vars['bp']), "\n";
    }
}
$a = new V();
$b = new V();
run([$a, $b]);
$a->kids[] = new V();
function deep(V $v): void {
    array_splice($v->kids[0]->vars['bp'], 0, 0, 'first');
}
deep($a);
echo implode(',', $a->kids[0]->vars['bp']), "\n";
echo implode(',', $b->vars['bp']), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "b|a,X,Y,c\na,X,Y\nb|a,X,Y,c\na,X,Y\nfirst,a,b,c\na,X,Y\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// symfony `ExpressionLanguage::__construct`: an `iterable` parameter narrowed by
/// `if (!is_array($p)) { $p = iterator_to_array($p, false); }` and then prepended to.
#[test]
fn test_array_unshift_after_an_iterator_to_array_guard_on_an_iterable_parameter() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Prov { public function __construct(public ?string $n = null) {} }
function make(iterable $providers = []): array {
    if (!\is_array($providers)) {
        $providers = iterator_to_array($providers, false);
    }
    array_unshift($providers, new Prov('default'));
    return $providers;
}
function gen() { yield new Prov('a'); yield new Prov('b'); }
echo count(make([new Prov('x')])), count(make(gen())), count(make()), "\n";
foreach (make(gen()) as $p) { echo $p->n, ","; }
echo "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "231\ndefault,a,b,\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A user function may reuse a PHP alias name (`pos` is `current`'s alias): the runtime string
/// callable must keep naming the program's function.
#[test]
fn test_runtime_string_callable_prefers_a_program_function_over_a_builtin_alias() {
    let out = compile_and_run(
        r#"<?php
function pos($x) { return $x > 0; }
function walk(array $array, $callback): bool {
    foreach ($array as $key => $value) {
        if (!$callback($value, $key)) { return false; }
    }
    return true;
}
echo walk([1, 2, 3], "pos") ? "y" : "n", walk([1, -2], "pos") ? "y" : "n", "\n";
"#,
    );
    assert_eq!(out, "yn\n");
}

/// Every by-reference array builtin applied to `$boxed->vars['k']` must write the result back into the property element (it used to be refused or silently dropped).
#[test]
fn test_by_reference_array_builtins_write_back_through_a_nested_boxed_place() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
final class V {
    public array $vars = ['l' => [3, 1, 2], 'k' => ['b' => 2, 'a' => 1, 'c' => 3], 'w' => [1, 2, 3]];
}
function run(array $views): void {
    foreach ($views as $v) {
        array_push($v->vars['l'], 9, 8);
        echo implode(',', $v->vars['l']), "\n";
        echo array_pop($v->vars['l']), '|', implode(',', $v->vars['l']), "\n";
        echo array_shift($v->vars['l']), '|', implode(',', $v->vars['l']), "\n";
        echo array_unshift($v->vars['l'], 7), '|', implode(',', $v->vars['l']), "\n";
        sort($v->vars['l']);
        echo implode(',', $v->vars['l']), "\n";
        rsort($v->vars['l']);
        echo implode(',', $v->vars['l']), "\n";
        usort($v->vars['l'], fn($a, $b) => $a <=> $b);
        echo implode(',', $v->vars['l']), "\n";
        ksort($v->vars['k']);
        echo implode(',', array_keys($v->vars['k'])), "\n";
        arsort($v->vars['k']);
        echo implode(',', array_keys($v->vars['k'])), "\n";
        asort($v->vars['k']);
        echo implode(',', array_keys($v->vars['k'])), "\n";
        krsort($v->vars['k']);
        echo implode(',', array_keys($v->vars['k'])), "\n";
        uasort($v->vars['k'], fn($a, $b) => $b <=> $a);
        echo implode(',', array_keys($v->vars['k'])), "\n";
        uksort($v->vars['k'], fn($a, $b) => strcmp($a, $b));
        echo implode(',', array_keys($v->vars['k'])), "\n";
        array_walk($v->vars['w'], function (&$x, $i) { $x = $x * 10 + $i; });
        echo implode(',', $v->vars['w']), "\n";
        reset($v->vars['w']);
        $v->vars['w'][] = 5;
    }
}
$a = new V();
$b = new V();
run([$a, $b]);
echo implode(',', $a->vars['l']), ' ', implode(',', $b->vars['w']), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "3,1,2,9,8\n8|3,1,2,9\n3|1,2,9\n4|7,1,2,9\n1,2,7,9\n9,7,2,1\n1,2,7,9\na,b,c\nc,b,a\na,b,c\nc,b,a\nc,b,a\na,b,c\n10,21,32\n3,1,2,9,8\n8|3,1,2,9\n3|1,2,9\n4|7,1,2,9\n1,2,7,9\n9,7,2,1\n1,2,7,9\na,b,c\nc,b,a\na,b,c\nc,b,a\nc,b,a\na,b,c\n10,21,32\n1,2,7,9 10,21,32,5\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A user function taking `array &$a` given `$boxed->vars['k']` mutated a transient copy and the write was dropped.
#[test]
fn test_user_function_by_reference_parameter_writes_back_a_nested_boxed_place() {
    let out = compile_and_run(
        r####"<?php
final class V {
    public array $vars = ['l' => [3, 1, 2]];
}
function srt(array &$a): void { sort($a); }
function srtm(mixed &$a): void { sort($a); }
function run(array $views): void {
    foreach ($views as $v) {
        srt($v->vars['l']);
        echo implode(',', $v->vars['l']), "\n";
        $v->vars['l'] = [9, 4, 6];
        srtm($v->vars['l']);
        echo implode(',', $v->vars['l']), "\n";
        $v->vars['l'] = [9, 4, 6];
        sort($v->vars['l']);
        echo implode(',', $v->vars['l']), "\n";
    }
}
run([new V()]);
"####,
    );
    assert_eq!(out, "1,2,3\n4,6,9\n4,6,9\n");
}

/// symfony/form `BaseDateTimeTransformer::$formats = [\IntlDateFormatter::NONE, ...]`: the default
/// names a class only the run time defines, so it is evaluated at the property's first use (php's
/// semantics) instead of in the prologue, which could not resolve the constant and handed the
/// declaring file back to the autoloader.
#[test]
fn test_static_default_naming_a_runtime_defined_class_is_evaluated_at_first_use() {
    let out = compile_and_run(
        r####"<?php
abstract class Base {
    protected static array $formats = [
        Later::NONE,
        Later::FULL,
        Later::LONG,
    ];
    public static int $count = 0;
    public static function formats(): array { return static::$formats; }
    public static function first(): int { return self::$formats[0]; }
    public static function add(int $n): void { static::$formats[] = $n; }
}
final class D extends Base {}
echo "before\n";
eval('class Later { const NONE = -1; const FULL = 0; const LONG = 1; }');
echo implode(',', D::formats()), "\n";
D::add(7);
echo implode(',', Base::formats()), "|", Base::first(), "|", Base::$count, "\n";
"####,
    );
    assert_eq!(out, "before\n-1,0,1\n-1,0,1,7|-1|0\n");
}

/// A statically definite argument/parameter mismatch is never a compile refusal in php: the call
/// evaluates its arguments, then throws `TypeError` with php's exact wording. Covers a strict_types
/// scalar mismatch (function, method, static method, constructor) and a definite class, array and
/// null mismatch in strict mode; the source is the same under weak mode below.
#[test]
fn test_definite_argument_mismatch_throws_type_error_at_the_call() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
declare(strict_types=1);
namespace App;
class Foo {}
class Bar {}
function side(string $s): string { echo "eval:$s\n"; return $s; }
function f(int $n, string $label = 'x'): int { return $n; }
function g(Foo $x): string { return 'ok'; }
function h(array $a): int { return count($a); }
class K {
    public function __construct(public int $v) {}
    public function m(float $x): float { return $x; }
    public static function s(string $q): string { return $q; }
}
function show(callable $c): void {
    try { var_dump($c()); }
    catch (\TypeError $e) { echo preg_replace('~, called in .* on line (\d+)~', ', called in FILE on line $1', $e->getMessage()), "\n"; }
}
show(fn() => f(side("8"), 'a'));
show(fn() => f(1.5));
show(fn() => g(new Bar()));
show(fn() => g([1]));
show(fn() => h(new Foo()));
show(fn() => h("s"));
show(fn() => (new K(7))->m("2"));
show(fn() => new K("7"));
show(fn() => K::s(5));
show(fn() => g(null));
show(fn() => f(true));
echo "end\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "eval:8\nApp\\f(): Argument #1 ($n) must be of type int, string given, called in FILE on line 19\nApp\\f(): Argument #1 ($n) must be of type int, float given, called in FILE on line 20\nApp\\g(): Argument #1 ($x) must be of type App\\Foo, App\\Bar given, called in FILE on line 21\nApp\\g(): Argument #1 ($x) must be of type App\\Foo, array given, called in FILE on line 22\nApp\\h(): Argument #1 ($a) must be of type array, App\\Foo given, called in FILE on line 23\nApp\\h(): Argument #1 ($a) must be of type array, string given, called in FILE on line 24\nApp\\K::m(): Argument #1 ($x) must be of type float, string given, called in FILE on line 25\nApp\\K::__construct(): Argument #1 ($v) must be of type int, string given, called in FILE on line 26\nApp\\K::s(): Argument #1 ($q) must be of type string, int given, called in FILE on line 27\nApp\\g(): Argument #1 ($x) must be of type App\\Foo, null given, called in FILE on line 28\nApp\\f(): Argument #1 ($n) must be of type int, true given, called in FILE on line 29\nend\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Weak mode: a class, array or null argument that no coercion can bind throws the same
/// `TypeError`; coercible scalars (`"7"` into `int`) keep binding.
#[test]
fn test_definite_weak_mode_argument_mismatch_throws_type_error_at_the_call() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Foo {}
class Bar {}
function g(Foo $x): string { return 'ok'; }
function h(array $a): int { return count($a); }
function f(int $n): int { return $n; }
function show(callable $c): void {
    try { var_dump($c()); }
    catch (\TypeError $e) { echo preg_replace('~, called in .* on line (\d+)~', ', called in FILE on line $1', $e->getMessage()), "\n"; }
}
show(fn() => g(new Bar()));
show(fn() => g([1]));
show(fn() => g(3));
show(fn() => h(new Foo()));
show(fn() => h("s"));
show(fn() => h(null));
show(fn() => f("7"));
echo "end\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "g(): Argument #1 ($x) must be of type Foo, Bar given, called in FILE on line 11\ng(): Argument #1 ($x) must be of type Foo, array given, called in FILE on line 12\ng(): Argument #1 ($x) must be of type Foo, int given, called in FILE on line 13\nh(): Argument #1 ($a) must be of type array, Foo given, called in FILE on line 14\nh(): Argument #1 ($a) must be of type array, string given, called in FILE on line 15\nh(): Argument #1 ($a) must be of type array, null given, called in FILE on line 16\nint(7)\nend\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
