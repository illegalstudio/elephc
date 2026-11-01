//! Purpose:
//! End-to-end regressions for the compiled/interpreted boundary and the builtins a large PHP
//! framework (Laravel) leans on: `JsonSerializable`, list destructuring of `ArrayAccess`, `static`
//! return overrides, dynamic `instanceof`, `htmlspecialchars()` flags and `mb_strimwidth()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_framework_boundaries` through Rust's test harness.
//!
//! Key details:
//! - Every fixture compiles PHP to native code and compares with the output `php` printed for the
//!   same source.

use crate::support::compile_and_run;

/// `Collection`, `JsonResource` and `MessageBag` are all `JsonSerializable`; the interpreter's
/// `json_encode()` printed `{}` for every one of them, so a `collect(...)` pipeline answered an
/// empty object.
#[test]
fn test_eval_json_encode_calls_json_serialize() {
    let out = compile_and_run(
        r#"<?php
eval('class Coll implements JsonSerializable, Countable {
    public function __construct(protected array $items = []) {}
    public function jsonSerialize(): array { return array_map(fn ($v) => $v instanceof JsonSerializable ? $v->jsonSerialize() : $v, $this->items); }
    public function count(): int { return count($this->items); }
}
class Plain2 { public $a = 1; protected $b = 2; }
class Plain3 { public function __construct(public $x = 5) {} }
class SelfSerial implements JsonSerializable { public $v = 7; public function jsonSerialize(): mixed { return $this; } }');
class CompiledJs implements JsonSerializable {
    public function jsonSerialize(): mixed { return ['c' => 1]; }
}
echo eval('return json_encode(["a" => new Coll([1, 2]), "b" => new Coll(["x" => new Coll([3])])]);'), "\n";
echo eval('return json_encode(new Coll([1, 2]));'), "\n";
echo eval('return json_encode([new Plain2, new Plain3(7)]);'), "\n";
echo eval('return json_encode(["k" => new CompiledJs]);'), "\n";
echo eval('return json_encode(new SelfSerial);'), "\n";
echo eval('return json_encode(["a" => new Coll([1, 2])], JSON_PRETTY_PRINT);'), "\n";
"#,
    );

    assert_eq!(
        out,
        "{\"a\":[1,2],\"b\":{\"x\":[3]}}\n[1,2]\n[{\"a\":1},{\"x\":7}]\n{\"k\":{\"c\":1}}\n{\"v\":7}\n{\n    \"a\": [\n        1,\n        2\n    ]\n}\n"
    );
}

/// `[$a, $b] = $arrayAccessObject` reads through `offsetGet()`. Blade's `ComponentAttributeBag`
/// splits its attributes with `[$appendable, $other] = (new Collection(...))->partition(...)`;
/// the interpreter warned "Cannot use object as array" and bound both to null.
#[test]
fn test_eval_list_destructuring_reads_array_access_objects() {
    let out = compile_and_run(
        r#"<?php
eval('class Pair implements ArrayAccess {
    public function __construct(private array $items) {}
    public function offsetExists(mixed $o): bool { return isset($this->items[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->items[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->items[$o] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->items[$o]); }
}
[$first, $second] = new Pair(["a", "b"]);
["x" => $x, "y" => $y] = new Pair(["x" => 1, "y" => 2]);
echo $first, $second, $x, $y;');
"#,
    );

    assert_eq!(out, "ab12");
}

/// An eval class overriding a COMPILED abstract method declared `: static` was refused: the
/// registered contract had `static` already resolved to one concrete subclass (`negated():
/// Int1`), so brick/math's `BigInteger` could never be declared and every validation rule that
/// compares numbers died.
#[test]
fn test_eval_class_may_override_a_compiled_static_return_method() {
    let out = compile_and_run(
        r#"<?php
abstract class Num {
    abstract public function negated(): static;
    public function twice(): static { return $this->negated()->negated(); }
}
final class Int1 extends Num {
    public function __construct(public int $v) {}
    public function negated(): static { return new static(-$this->v); }
}
final class Rat1 extends Num {
    public function __construct(public int $n, public int $d) {}
    public function negated(): static { return new static(-$this->n, $this->d); }
}
$a = (new Int1(3))->negated();
$b = (new Rat1(1, 2))->negated();
echo get_class($a), $a->v, get_class($b), $b->n, "\n";
eval('final class Int2 extends Num {
    public function __construct(public int $v) {}
    public function negated(): static { return new static(-$this->v); }
}
$c = (new Int2(5))->negated();
echo get_class($c), $c->v, "\n";');
"#,
    );

    assert_eq!(out, "Int1-3Rat1-1\nInt2-5\n");
}

/// A compiled `$value instanceof $class` whose target is a RUNTIME string must see classes the
/// interpreter declared: the compile-time class table has never heard of them, so the answer was
/// always false. Laravel's `alreadyInParameters()` is that shape, which dropped every route-bound
/// model and injected an empty one.
#[test]
fn test_compiled_dynamic_instanceof_sees_eval_declared_classes() {
    let out = compile_and_run(
        r#"<?php
function chk($v, $c) { return $v instanceof $c; }
function chks($v, string $c) { return $v instanceof $c; }
function chkcb(array $a, string $c) { foreach ($a as $x) { if ((fn ($value) => $value instanceof $c)($x)) { return true; } } return false; }
eval('class Q {} class R extends Q {} interface I {} class S implements I {}');
$q = eval('return new Q;');
$r = eval('return new R;');
$s = eval('return new S;');
echo var_export(chk($q, 'Q'), true), var_export(chk($r, 'Q'), true), var_export(chk($s, 'I'), true), var_export(chk($q, 'R'), true), "\n";
echo var_export(chks($q, 'Q'), true), var_export(chks($s, 'I'), true), var_export(chks($s, 'Q'), true), "\n";
echo var_export(chkcb([1, $q], 'Q'), true), var_export(chkcb([1, $q], 'R'), true), "\n";
echo var_export(chk(new ArrayObject([]), 'Countable'), true), var_export(chk(new stdClass, 'Q'), true), var_export(chk(null, 'Q'), true), "\n";
"#,
    );

    assert_eq!(out, "truetruetruefalse\ntruetruefalse\ntruefalse\ntruefalsefalse\n");
}

/// `htmlspecialchars()` honours the quote bits of `$flags`: `ENT_COMPAT` keeps `'`,
/// `ENT_NOQUOTES` keeps both quotes, and a non-HTML-4.01 doctype spells the apostrophe `&apos;`.
/// Symfony's error renderer escapes with `ENT_COMPAT`, so a 418 page printed `I&#039;m a teapot`.
#[test]
fn test_htmlspecialchars_honours_quote_flags() {
    let out = compile_and_run(
        r#"<?php
$s = "I'm a \"q\" <b>&amp;";
echo htmlspecialchars($s, ENT_COMPAT | ENT_SUBSTITUTE, 'UTF-8'), "\n";
echo htmlspecialchars($s, ENT_COMPAT, 'UTF-8'), "\n";
echo htmlspecialchars($s, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8'), "\n";
echo htmlspecialchars($s, ENT_NOQUOTES), "\n";
echo htmlspecialchars($s), "\n";
echo htmlspecialchars($s, ENT_QUOTES | ENT_HTML5), "\n";
echo htmlspecialchars($s, ENT_COMPAT | ENT_HTML5), "\n";
echo htmlspecialchars($s, ENT_COMPAT | ENT_SUBSTITUTE, 'UTF-8', false), "\n";
function esc($string) { return htmlspecialchars($string, \ENT_COMPAT | \ENT_SUBSTITUTE, 'UTF-8'); }
echo esc($s), "\n";
eval('echo htmlspecialchars("I\'m", ENT_COMPAT | ENT_SUBSTITUTE, "UTF-8"), "|", htmlspecialchars("I\'m \"x\"", ENT_NOQUOTES), "|", htmlspecialchars("I\'m", ENT_QUOTES | ENT_HTML5), "|", htmlspecialchars("I\'m"), "\n";');
"#,
    );

    assert_eq!(
        out,
        "I'm a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I'm a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I&#039;m a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I'm a \"q\" &lt;b&gt;&amp;amp;\n\
         I&#039;m a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I&apos;m a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I'm a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I'm a &quot;q&quot; &lt;b&gt;&amp;\n\
         I'm a &quot;q&quot; &lt;b&gt;&amp;amp;\n\
         I'm|I'm \"x\"|I&apos;m|I&#039;m\n"
    );
}

/// `mb_strimwidth()` is not in symfony/polyfill-mbstring, and Laravel's `Str::limit()` calls it:
/// every `limit()` in interpreted code died with "call to undefined function".
#[test]
fn test_eval_mb_strimwidth_trims_by_display_width() {
    let out = compile_and_run(
        r#"<?php
eval('$cases = [
    ["Hello World", 0, 5, ""],
    ["Hello World", 0, 8, "..."],
    ["Hello", 0, 10, "..."],
    ["The quick brown fox jumps", 0, 12, ""],
    ["日本語のテキスト", 0, 7, "…"],
    ["日本語のテキスト", 0, 8, ""],
    ["日本語のテキスト", 2, 6, ".."],
    ["Zażółć gęślą jaźń", 0, 10, "..."],
    ["abcdef", -3, 10, ""],
    ["abcdef", 0, 3, "[...]"],
    ["a😀b😀c", 0, 4, ""],
    ["", 0, 3, "..."],
];
foreach ($cases as [$s, $start, $width, $marker]) {
    echo json_encode(mb_strimwidth($s, $start, $width, $marker, "UTF-8"), JSON_UNESCAPED_UNICODE), "\n";
}
try { mb_strimwidth("abc", 5, 2); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }');
"#,
    );

    assert_eq!(
        out,
        "\"Hello\"\n\"Hello...\"\n\"Hello\"\n\"The quick br\"\n\"日本語…\"\n\"日本語の\"\n\"語の..\"\n\"Zażółć ...\"\n\"def\"\n\"[...]\"\n\"a😀b\"\n\"\"\nValueError: mb_strimwidth(): Argument #2 ($start) is out of range\n"
    );
}

/// A `foreach` inside an interpreted generator walks `Iterator`, `IteratorAggregate` and plain
/// object subjects, not only arrays. Laravel's `LazyCollection` is `foreach ($this as ...)` in a
/// generator closure; the array-only walk ended that loop before its first element, so every
/// lazy pipeline (and `collect()->lazy()`) answered an empty array.
#[test]
fn test_eval_generator_foreach_walks_iterator_objects() {
    let out = compile_and_run(
        r#"<?php
eval('class A implements IteratorAggregate {
    public $pub = "p";
    public function getIterator(): Traversable { return new ArrayIterator([1, 2, 3]); }
    public function genClosure() { return (function () { foreach ($this as $k => $v) { yield $k => $v; } })(); }
    public function genMethod() { foreach ($this as $k => $v) { yield $k => $v; } }
    public function genLocal() { $it = $this->getIterator(); foreach ($it as $k => $v) { yield $k => $v; } }
    public function genClosureSource($src) { return (function () use ($src) { foreach ($src as $k => $v) { yield $k => $v; } })(); }
}
class Chain implements IteratorAggregate { public function getIterator(): Traversable { return new A; } }
class Inner { public $a = 1; protected $b = 2; public $c = 3; }
$a = new A;
echo json_encode(iterator_to_array($a->genClosure())), "|";
echo json_encode(iterator_to_array($a->genMethod())), "|";
echo json_encode(iterator_to_array($a->genLocal())), "|";
echo json_encode(iterator_to_array($a->genClosureSource([7, 8]))), "|";
echo json_encode(iterator_to_array($a->genClosureSource($a))), "|";
echo json_encode(iterator_to_array($a->genClosureSource(new ArrayIterator([5])))), "|";
echo json_encode(iterator_to_array($a->genClosureSource(new Chain))), "|";
echo json_encode(iterator_to_array($a->genClosureSource(new Inner))), "|";
echo json_encode(iterator_to_array($a->genClosureSource((function () { yield "x" => 1; yield "y" => 2; })())));');
"#,
    );

    assert_eq!(
        out,
        "[1,2,3]|[1,2,3]|[1,2,3]|[7,8]|[1,2,3]|[5]|[1,2,3]|{\"a\":1,\"c\":3}|{\"x\":1,\"y\":2}"
    );
}

/// `f(...$traversable)` and `new C(...$traversable)` unpack a generator or iterator into
/// arguments in interpreted code. The call machinery read an object as an empty array, so
/// `new Substitutions(...Inflectible::getIrregular())` built a Doctrine inflector with no rules
/// and `Str::plural()` died on its first call.
#[test]
fn test_eval_call_spread_unpacks_traversables() {
    let out = compile_and_run(
        r#"<?php
function gen_c() { yield 1; yield 2; yield 3; }
class CC { public static function items(): iterable { yield 'a'; yield 'b'; } }
eval('function cnt(...$a) { return count($a); }
class K { public $items; public function __construct(...$items) { $this->items = $items; } public static function s(...$a) { return count($a); } public function m(...$a) { return implode(",", $a); } }
function gen_e() { yield 10; yield 20; }
echo cnt(...gen_e()), cnt(...gen_c()), cnt(...CC::items()), cnt(...new ArrayIterator([1, 2, 3, 4])), cnt(...[1, 2]), "\n";
echo count((new K(...gen_e()))->items), count((new K(...gen_c()))->items), count((new K(...CC::items()))->items), "\n";
echo K::s(...gen_e()), (new K)->m(...gen_c()), "\n";
echo json_encode([...gen_e(), ...[1, 2], ...gen_c()]), "\n";');
"#,
    );

    assert_eq!(out, "23242\n232\n21,2,3\n[10,20,1,2,1,2,3]\n");
}

/// A compiled static call with a spread argument reaches a class only an eval declares. The
/// plain route boxes one operand per argument and a spread cannot supply them, so such a call fell
/// through to `Class "Later" not found`: Laravel's `Stringable::trim()` is
/// `Str::trim(...array_merge([$this->value], func_get_args()))`.
#[test]
fn test_compiled_static_call_with_spread_reaches_eval_declared_class() {
    let out = compile_and_run(
        r##"<?php
class Caller {
    public $value = "v";
    public function plain($x = null) { return Later::join($this->value, "!"); }
    public function spread($x = null) { return Later::join(...array_merge([$this->value], func_get_args())); }
    public function spreadVar(array $args) { return Later::join(...$args); }
    public function mixed($tail) { return Later::join($this->value, ...$tail); }
}
eval('class Later { public static function join($a, $b = "", $c = "") { return $a . $b . $c; } }');
$c = new Caller;
echo $c->plain(), "|", $c->spread("#"), "|", $c->spreadVar(["a", "b"]), "|", $c->mixed(["x", "y"]), "\n";
"##,
    );

    assert_eq!(out, "v!|v#|ab|vxy\n");
}

/// A compiled runtime-name property read (`$object->{$name}`) reaches an eval-declared object's
/// own state and its `__get()`. The read went to the stdClass hash behind the eval object and
/// answered null, so every attribute of a Laravel API resource (`$this->resource->{$key}`) was
/// empty.
#[test]
fn test_compiled_runtime_name_property_read_reaches_eval_object() {
    let out = compile_and_run(
        r#"<?php
class Deleg {
    public $resource;
    public function __construct($r) { $this->resource = $r; }
    public function __get($key) { return $this->resource->{$key}; }
    public function viaVar($key) { return $this->resource->$key; }
    public function isset2($key) { return isset($this->resource->{$key}); }
}
function dyn($o, $k) { return $o->{$k}; }
eval('class Mdl { protected $attributes = ["id" => 3, "title" => "T"];
  public function __get($k) { return $this->attributes[$k] ?? null; }
  public function __isset($k) { return isset($this->attributes[$k]); } }
class Plain { public $id = 7; public $title = "P"; }
$d = new Deleg(new Mdl); $p = new Deleg(new Plain);
echo var_export($d->id, true), var_export($d->title, true), var_export($d->viaVar("id"), true), var_export($d->isset2("id"), true), var_export($d->isset2("nope"), true), "\n";
echo var_export($p->id, true), var_export($p->title, true), var_export($p->viaVar("id"), true), var_export($p->isset2("id"), true), "\n";
echo var_export(dyn(new Mdl, "id"), true), var_export(dyn(new Plain, "title"), true), "\n";');
"#,
    );

    assert_eq!(out, "3'T'3truefalse\n7'P'7true\n3'P'\n");
}

/// `$value instanceof static` inside a STATIC method tests the called class. The literal name
/// "static" matched no class, so brick/math's `BigNumber::of()` never recognised a number that
/// already had the requested class and went on to build one through an abstract method.
#[test]
fn test_instanceof_static_in_static_method_tests_called_class() {
    let out = compile_and_run(
        r#"<?php
abstract class P {
    public static function check($v) { return $v instanceof static; }
    public static function of($v) { $r = self::build(); if ($r instanceof static) { return "same"; } return "other:" . get_class($r); }
    public static function build() { return new C2(); }
}
class C2 extends P {}
class C3 extends C2 {}
echo var_export(C2::check(new C2), true), var_export(C2::check(new C3), true), var_export(C3::check(new C2), true), var_export(P::check(new C3), true), "|", C2::of(1), "|", C3::of(1), "\n";
eval('class C extends P {}
echo var_export(C::check(new C), true), var_export(C::check(new C2), true), var_export(P::check(new C), true), "\n";');
"#,
    );

    assert_eq!(out, "truetruefalsetrue|same|other:C2\ntruefalsetrue\n");
}

/// Re-throwing a caught COMPILED exception from interpreted code hands the outer handler a live
/// object. `throw $exception;` carried the variable's own cell as the thrown value, so after the
/// catch scope dropped the variable the next handler's `Throwable` parameter read freed memory
/// ("int given"): every route-model-binding miss in Laravel became a 500 instead of a 404.
#[test]
fn test_eval_rethrow_of_compiled_exception_reaches_outer_handler() {
    let out = compile_and_run(
        r#"<?php
class MissingEx extends RuntimeException { public $model = null; public function setModel($m) { $this->model = $m; return $this; } }
class Binder { public static function resolve($v) { if ($v === null) { throw (new MissingEx("not found"))->setModel("Post"); } return $v; } }
eval('class Mid { public function handle($v) { try { return Binder::resolve($v); } catch (MissingEx $exception) { if ($missing = $this->missing()) { return $missing($exception); } throw $exception; } }
  public function missing() { return null; } }
class Outer { public function run($v) { try { return (new Mid)->handle($v); } catch (\Throwable $e) { return $this->onError($v, $e); } }
  protected function onError($v, \Throwable $e) { return get_class($e) . ":" . $e->getMessage() . ":" . $e->model; } }
echo (new Outer)->run(5), "\n";
echo (new Outer)->run(null), "\n";');
"#,
    );

    assert_eq!(out, "5\nMissingEx:not found:Post\n");
}

/// A compiled function with no eval context of its own sees the relations of an eval-declared
/// class. `class_uses()` / `class_parents()` / `class_implements()` there answered from the
/// compile-time registry, which knows no eval class, so Laravel's `class_uses_recursive($model)`
/// returned `[]` and Eloquent never ran a trait initializer (`casts()` was ignored).
#[test]
fn test_compiled_class_relations_see_eval_declared_classes() {
    let out = compile_and_run(
        r#"<?php
trait TA { }
trait TB { use TA; }
interface IA { }
class BaseC implements IA { use TB; }
function trait_uses_rec($trait) {
    $traits = class_uses($trait) ?: [];
    foreach ($traits as $trait) { $traits += trait_uses_rec($trait); }
    return $traits;
}
function uses_rec($class) {
    if (is_object($class)) { $class = get_class($class); }
    $results = [];
    foreach (array_reverse(class_parents($class) ?: []) + [$class => $class] as $class) {
        $results += trait_uses_rec($class);
    }
    return array_unique($results);
}
function relations($class) {
    $implements = array_values(class_implements($class));
    sort($implements);
    return json_encode([class_uses($class), class_parents($class), $implements]);
}
echo json_encode(array_values(uses_rec(BaseC::class))), "\n";
eval('trait TC { }
interface IB { }
class SubC extends BaseC implements IB { use TC; }
class SubD extends SubC { }');
echo json_encode(array_values(uses_rec('SubC'))), "\n";
echo json_encode(array_values(uses_rec('SubD'))), "\n";
echo json_encode(array_values(uses_rec(eval('return new SubD;')))), "\n";
echo relations('SubD'), "\n";
"#,
    );

    assert_eq!(
        out,
        "[\"TB\",\"TA\"]\n[\"TB\",\"TA\",\"TC\"]\n[\"TB\",\"TA\",\"TC\"]\n[\"TB\",\"TA\",\"TC\"]\n[[],{\"SubC\":\"SubC\",\"BaseC\":\"BaseC\"},[\"IA\",\"IB\"]]\n"
    );
}

/// A compiled caller passing a VARIABLE to an interpreted closure's by-reference parameter reaches
/// the caller's variable. The invoker hands the closure a marker cell that `array_get` resolves
/// into a plain value, so the parameter bound a copy and warned `must be passed by reference,
/// value given` -- Laravel's `tap(static::getRecursiveCallStack($o), fn (&$stack) => ...)`.
#[test]
fn test_compiled_caller_reaches_eval_closure_by_ref_parameter() {
    let out = compile_and_run(
        r#"<?php
function tap_c($value, $callback) {
    $callback($value);
    return $value;
}
function call_it(callable $cb, $v) { $cb($v); return $v; }
eval('$hash = "k"; $value = "v";
echo json_encode(tap_c([], fn (&$stack) => $stack[$hash] = $value)), "\n";
echo json_encode(call_it(function (&$arr) { $arr[] = 7; }, [1])), "\n";
echo json_encode(call_it(function ($arr) { $arr[] = 7; }, [1])), "\n";');
"#,
    );

    assert_eq!(out, "{\"k\":\"v\"}\n[1,7]\n[1]\n");
}

/// A compiled `Class ...$items` variadic reached from interpreted code reads real objects. The
/// tail was handed over as boxed Mixed cells while the callee's slots hold raw object pointers, so
/// every element method call read null: Doctrine's `Patterns(Pattern ...$patterns)` compiled an
/// empty regex and `Str::plural()` failed on its first use.
#[test]
fn test_eval_calls_compiled_variadic_of_objects() {
    let out = compile_and_run(
        r#"<?php
final class Pattern { private string $pattern; public function __construct(string $pattern) { $this->pattern = $pattern; } public function getPattern(): string { return $this->pattern; } }
class Patterns {
    private $regex;
    public function __construct(Pattern ...$patterns) {
        $out = [];
        foreach ($patterns as $p) { $out[] = $p->getPattern(); }
        $this->regex = implode('|', $out);
    }
    public function get() { return $this->regex; }
    public static function make(Pattern ...$patterns) { $o = []; foreach ($patterns as $p) { $o[] = $p->getPattern(); } return implode('|', $o); }
    public function inst(Pattern ...$patterns) { $o = []; foreach ($patterns as $p) { $o[] = $p->getPattern(); } return implode('|', $o); }
}
function gen_patterns(): iterable { yield new Pattern('g1'); yield new Pattern('g2'); }
$list = [new Pattern('l1'), new Pattern('l2')];
eval('echo (new Patterns(new Pattern("a"), new Pattern("b")))->get(), "\n";
echo (new Patterns(...$list))->get(), "\n";
echo Patterns::make(new Pattern("c"), new Pattern("d")), "\n";
echo Patterns::make(...$list), "\n";
echo (new Patterns)->inst(...$list), "\n";
echo (new Patterns(...gen_patterns()))->get(), "|", (new Patterns)->get(), "\n";');
"#,
    );

    assert_eq!(out, "a|b\nl1|l2\nc|d\nl1|l2\nl1|l2\ng1|g2|\n");
}

/// An eval class extending a COMPILED class inherits its magic accessors. The class record lists
/// only the eval class's own methods, so reads, `isset()`, `empty()`, writes and `unset()` of an
/// undeclared name never reached the native `__get`/`__isset`/`__set`/`__unset`: Laravel's
/// `ColumnDefinition extends Fluent` answered `$column->type` with null and every migration died.
#[test]
fn test_eval_subclass_of_compiled_class_uses_inherited_magic_accessors() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected $attributes = [];
    public function __construct($a = []) { $this->attributes = $a; }
    public function __get($k) { return $this->attributes[$k] ?? null; }
    public function __set($k, $v) { $this->attributes[$k] = $v; }
    public function __isset($k) { return isset($this->attributes[$k]); }
    public function __unset($k) { unset($this->attributes[$k]); }
    public function dump() { return json_encode($this->attributes); }
}
eval('class Sub extends Base {}
$s = new Sub(["type" => "x", "name" => "y", "zero" => 0, "nul" => null]);
echo var_export($s->type, true), var_export($s->name, true), var_export($s->nope, true), "\n";
echo var_export(isset($s->type), true), var_export(isset($s->nope), true), var_export(isset($s->nul), true), var_export(empty($s->zero), true), var_export(empty($s->type), true), "\n";
echo var_export($s->type ?? "d", true), var_export($s->nope ?? "d", true), "\n";
$s->added = 5; $s->type = "z";
echo $s->dump(), "\n";
unset($s->name);
echo $s->dump(), "\n";
function f(Base $o) { return $o->type . "|" . var_export(isset($o->nope), true); }
echo f($s), "\n";');
"#,
    );

    assert_eq!(
        out,
        "'x''y'NULL\ntruefalsefalsetruefalse\n'x''d'\n{\"type\":\"z\",\"name\":\"y\",\"zero\":0,\"nul\":null,\"added\":5}\n{\"type\":\"z\",\"zero\":0,\"nul\":null,\"added\":5}\nz|false\n"
    );
}

/// A magic property read from compiled code on an untyped receiver reaches an eval-declared
/// object's `__get()`/`__isset()` even when some compiled class declares a property of the same
/// name. Such a name sent the read through the class-id dispatch, where the eval object -- carried
/// by a stdClass runtime object -- landed on the stdClass hash and answered null. Laravel's schema
/// builder reads `$column->name` and `$column->type` off a `Fluent` this way, so every migration
/// died on `SQLiteGrammar::type`.
#[test]
fn test_compiled_reads_magic_property_of_eval_object_named_like_a_declared_one() {
    let out = compile_and_run(
        r#"<?php
class Declares { public string $name = "own"; public string $title = "own"; public ?string $type = null; }
eval('class Fl { protected $a = ["type" => "s", "name" => "n", "title" => "t"];
  public function __get($k) { return $this->a[$k] ?? null; }
  public function __isset($k) { return isset($this->a[$k]); } }
class Cd extends Fl {}');
function show($o) {
    return var_export($o->name, true) . var_export($o->title, true) . var_export($o->type, true)
        . var_export(isset($o->name), true) . var_export($o->name ?? "d", true) . var_export($o->zip ?? "d", true);
}
echo show(eval('return new Cd;')), "\n";
echo show(new Declares), "\n";
"#,
    );

    assert_eq!(out, "'n''t''s'true'n''d'\n'own''own'NULLtrue'own''d'\n");
}

/// An autoloader that asks about the very class it is loading (`interface_exists($class)`, the
/// guard in nette/utils' `compatibility.php`) must see `false`, not re-enter itself: php refuses a
/// nested autoload of the same name. Every context kept its own in-flight set, and a closure's
/// frame opens a context of its own, so the loader recursed until the stack overflowed.
#[test]
fn test_autoload_of_a_class_is_not_reentered_by_the_loader_itself() {
    let out = compile_and_run(
        r#"<?php
$depth = 0;
spl_autoload_register(function ($class) use (&$depth) {
    $depth++;
    echo "load $class depth=$depth\n";
    if ($depth > 5) { echo "RUNAWAY\n"; return; }
    var_dump(interface_exists($class));
    var_dump(class_exists($class));
});
var_dump(interface_exists('Foo\Bar'));
var_dump(class_exists('Baz'));
echo "done\n";
"#,
    );

    assert_eq!(
        out,
        "load Foo\\Bar depth=1\nbool(false)\nbool(false)\nbool(false)\nload Baz depth=2\nbool(false)\nbool(false)\nbool(false)\ndone\n"
    );
}

/// `version_compare()` did not exist at all: interpreted library code (Laravel's database layer)
/// died with "call to undefined function version_compare()". Every row below is what php 8.5
/// printed for the same call, including its quirks (`1.0+build` sorts below `1.0`).
#[test]
fn test_eval_version_compare_matches_php() {
    let out = compile_and_run(
        r#"<?php
eval(<<<'EOT'
$pairs = [
    ['1.0', '1.0.0'], ['1.0.0', '1.0'], ['1.2.3', '1.2.4'], ['5.2', '5.10'], ['1.0-dev', '1.0'], ['1.0alpha', '1.0beta'],
    ['1.0a', '1.0alpha'], ['1.0b2', '1.0b10'], ['1.0RC1', '1.0rc1'], ['1.0rc1', '1.0'], ['1.0', '1.0pl1'], ['1.0p1', '1.0pl1'],
    ['3.10.0', '3.9.9'], ['8.5.10', '8.5.9'], ['', '1'], ['1', ''], ['', ''], ['1.0.0-beta.2', '1.0.0-beta.10'],
    ['1.0+build', '1.0'], ['1_0_0', '1.0.0'], ['v1.0', '1.0'], ['foo', 'bar'], ['1.0.0dev', '1.0.0alpha'], ['2.0', '2.0.0.0.1'],
    ['3.45.1', '3.45.1-rc'], ['10.2', '10.2.0-dev'],
];
foreach ($pairs as [$a, $b]) {
    echo json_encode([$a, $b, version_compare($a, $b), version_compare($a, $b, '<'), version_compare($a, $b, 'ge'), version_compare($a, $b, '!=')]), "\n";
}
try { version_compare('1', '2', 'bogus'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
EOT
);
"#,
    );

    let expected = [
        r#"["1.0","1.0.0",-1,true,false,true]"#,
        r#"["1.0.0","1.0",1,false,true,true]"#,
        r#"["1.2.3","1.2.4",-1,true,false,true]"#,
        r#"["5.2","5.10",-1,true,false,true]"#,
        r#"["1.0-dev","1.0",-1,true,false,true]"#,
        r#"["1.0alpha","1.0beta",-1,true,false,true]"#,
        r#"["1.0a","1.0alpha",0,false,true,false]"#,
        r#"["1.0b2","1.0b10",-1,true,false,true]"#,
        r#"["1.0RC1","1.0rc1",0,false,true,false]"#,
        r#"["1.0rc1","1.0",-1,true,false,true]"#,
        r#"["1.0","1.0pl1",-1,true,false,true]"#,
        r#"["1.0p1","1.0pl1",0,false,true,false]"#,
        r#"["3.10.0","3.9.9",1,false,true,true]"#,
        r#"["8.5.10","8.5.9",1,false,true,true]"#,
        r#"["","1",-1,true,false,true]"#,
        r#"["1","",1,false,true,true]"#,
        r#"["","",0,false,true,false]"#,
        r#"["1.0.0-beta.2","1.0.0-beta.10",-1,true,false,true]"#,
        r#"["1.0+build","1.0",-1,true,false,true]"#,
        r#"["1_0_0","1.0.0",0,false,true,false]"#,
        r#"["v1.0","1.0",-1,true,false,true]"#,
        r#"["foo","bar",-1,true,false,true]"#,
        r#"["1.0.0dev","1.0.0alpha",-1,true,false,true]"#,
        r#"["2.0","2.0.0.0.1",-1,true,false,true]"#,
        r#"["3.45.1","3.45.1-rc",1,false,true,true]"#,
        r#"["10.2","10.2.0-dev",-1,true,false,true]"#,
        "version_compare(): Argument #3 ($operator) must be a valid comparison operator",
    ]
    .join("\n")
        + "\n";
    assert_eq!(out, expected);
}

/// `new CallbackFilterIterator(...)` built by the interpreter runs through the class's
/// `__construct` bridge. Compiled `new` sites build the object inline, so no constructor body was
/// emitted and the interpreted form was a fatal; Twig's `filter` filter over a `Traversable`
/// (`form|filter(child => not child.rendered)`) died there, and Symfony's form theme silently
/// dropped the rows of every compound child (a `RepeatedType`'s two password fields).
#[test]
fn test_eval_new_callback_filter_iterator_over_an_aggregate() {
    let out = compile_and_run(
        r#"<?php
$compiled = new CallbackFilterIterator(new ArrayIterator([1]), fn($v) => true);
eval('
class EvalCfiAggregate implements IteratorAggregate {
    public array $children = ["a" => 1, "b" => 2, "c" => 3];
    public function getIterator(): Iterator { return new ArrayIterator($this->children); }
}
function eval_cfi_filter($items, $arrow) {
    return new \CallbackFilterIterator(new \IteratorIterator($items), $arrow);
}
foreach (eval_cfi_filter(new EvalCfiAggregate(), fn($v, $k) => $k !== "b") as $k => $v) {
    echo $k, "=", $v, ";";
}
');
"#,
    );
    assert_eq!(out, "a=1;c=3;");
}
