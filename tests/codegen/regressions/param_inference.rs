//! Purpose:
//! Regression tests for call-site parameter type inference of untyped parameters.
//! Ensures a parameter called with heterogeneous argument types is inferred as
//! `Mixed` (boxed), not collapsed to a single type that mis-tags scalar arguments.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The bug surfaced via PDO `bindValue` with mixed `?`/`:name` placeholders: an
//!   int argument passed to a param that another call site passed a string to was
//!   coerced to a string, so `is_int` returned false and values were corrupted.

use crate::support::*;

/// A free function with an untyped parameter called with both string and int
/// arguments infers the parameter as `Mixed`, so `is_int` on the int argument is
/// true (the int is boxed, not coerced to string).
#[test]
fn test_untyped_param_heterogeneous_calls_infer_mixed() {
    let out = compile_and_run(
        r#"<?php
function tag($a) { return is_int($a) ? "I" : "N"; }
echo tag("x") . tag(1) . tag(2.5) . tag(7);
"#,
    );
    assert_eq!(out, "NINI");
}

/// The same inference applies to instance-method parameters: params called with
/// both int and string are `Mixed`, so `is_int` and the round-tripped argument
/// values are correct regardless of call order.
#[test]
fn test_untyped_method_param_heterogeneous_calls_infer_mixed() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function put($a, $b, int $c) {
        return (is_int($a) ? "I" : "N") . $a . ":" . $b . ":" . $c;
    }
}
$o = new Box();
echo $o->put(1, "x", 5) . "|" . $o->put("y", 2, 6);
"#,
    );
    assert_eq!(out, "I1:x:5|Ny:2:6");
}

/// A parameter that is genuinely homogeneous (only int call sites) stays a
/// concrete int and is not over-widened to `Mixed`.
#[test]
fn test_untyped_param_homogeneous_int_stays_int() {
    let out = compile_and_run(
        r#"<?php
function only_int($a) { return is_int($a) ? "I" : "N"; }
echo only_int(1) . only_int(2) . only_int(3);
"#,
    );
    assert_eq!(out, "III");
}

/// An integer argument passed to a declared `float` parameter is converted with int→float
/// (`IToF`) before the call, not reinterpreted as a raw 64-bit bit-pattern. A single int argument
/// to a single float parameter previously produced garbage (the int bits read as a double).
#[test]
fn test_int_arg_to_float_param_single() {
    let out = compile_and_run(
        r#"<?php
function f(float $g): float { return $g; }
echo f(2), "|", f(7);
"#,
    );
    assert_eq!(out, "2|7");
}

/// When a float parameter receiving an int argument sits next to another float argument, the
/// int→float conversion must target the correct slot. Without the conversion the unconverted
/// argument slot was overwritten by the neighbouring float argument, so both parameters read the
/// same value regardless of argument order.
#[test]
fn test_int_arg_to_float_param_beside_float_arg() {
    let out = compile_and_run(
        r#"<?php
function f(float $g, float $h): string { return $g . "," . $h; }
echo f(90.5, 2), "|", f(2, 90.5);
"#,
    );
    assert_eq!(out, "90.5,2|2,90.5");
}


/// A `?int` reaching a declared `int` return is PHP's RUNTIME TypeError, not a compile error.
///
/// The backend has answered this since `declared_int_return_boundary` landed: it boxes the compact
/// nullable scalar and emits `ReturnBoundaryMixedToInt`, whose null arm raises PHP's own message.
/// Only the checker still refused the shape, and it was the single error standing between
/// Symfony's 165-file `--web` preload and a compiled binary — `KernelEvent` stores
/// `private ?int $requestType` and returns it from `getRequestType(): int`, which reference PHP
/// runs happily because the value is never actually null there.
///
/// Both halves matter: the non-null call must answer, and the null call must raise the SAME text
/// `php -n` 8.5 prints, or accepting the shape would trade a loud refusal for a silent zero. A
/// declared `float` or `string` return deliberately keeps refusing — those boundaries CONVERT
/// rather than check, so the same admission there would answer `0.0` / `""` where PHP throws.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_nullable_int_return_raises_php_type_error_only_when_null() {
    let out = compile_and_run(
        r#"<?php
class A {
    public function __construct(private ?int $n) {}
    public function get(): int { return $this->n; }
}
echo (new A(5))->get(), "\n";
try {
    echo (new A(null))->get(), "\n";
} catch (TypeError $e) {
    echo 'caught: ', $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "5\ncaught: A::get(): Return value must be of type int, null returned\n"
    );
}


/// An overriding method's UNTYPED parameter stays gradual; it does not adopt the ancestor's type.
///
/// `adopt_ancestor_parameter_storage` exists so an override with an undeclared parameter uses the
/// ancestor's parameter STORAGE — a caller reaches it through the ancestor's ABI, so a boxed
/// gradual cell on one side and a raw slot on the other would not agree. It was applied to
/// `__construct` as well, and a constructor is not an override: PHP exempts it from signature
/// compatibility entirely, and nothing ever enters one through an ancestor's entry point —
/// `new C(...)` calls C's, `parent::__construct(...)` calls the parent's.
///
/// The cost was seventeen refused calls in one Symfony build. `Twig\Node::__construct(array
/// $nodes = [], array $attributes = [], int $lineno = 0)` stamped `array` onto
/// `ConstantExpression::__construct($value, int $lineno)`, so every `new ConstantExpression('x',
/// 1)` read as "parameter $value expects Array(Mixed), got Str" — about a parameter PHP reads as
/// `mixed`.
///
/// The five values below are the five storage shapes the parameter has to carry, and they travel
/// through the parent's declared `array $attributes` on the way, which is the half a
/// type-check-only test would not reach.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_an_overriding_constructor_keeps_its_untyped_parameter_gradual() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public array $attrs;
    public function __construct(array $nodes = [], array $attributes = [], int $lineno = 0) {
        $this->attrs = $attributes;
    }
    public function shown(): string { return var_export($this->attrs['value'] ?? null, true); }
}
class ConstantNode extends Node {
    public function __construct($value, int $lineno) {
        parent::__construct([], ['value' => $value], $lineno);
    }
}
echo (new ConstantNode("x", 2))->shown(), "\n";
echo (new ConstantNode(7, 3))->shown(), "\n";
echo (new ConstantNode(true, 4))->shown(), "\n";
echo (new ConstantNode(null, 5))->shown(), "\n";
echo (new ConstantNode([1, 2], 6))->shown(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "'x'\n",
            "7\n",
            "true\n",
            "NULL\n",
            "array (\n  0 => 1,\n  1 => 2,\n)\n",
        )
    );
}


/// A function nobody calls must not export a fabricated `int` into what it calls.
///
/// An undeclared parameter is seeded `PhpType::Int` as a gradual starting point that the FIRST
/// call site discards and replaces with the real argument type. A function nothing calls never
/// gets that call site, so the seed stops being a starting point and becomes the answer — and
/// `int` is not what PHP reads an undeclared parameter as.
///
/// It did not stay contained: the uncalled body is still type-checked, and every call it makes
/// hands the fabricated `int` to the callee, which adopts it as its own contract and locks it
/// (the adoption is once-only, so no later caller can widen it back). Twig's deprecated
/// `twig_array_filter(Environment $env, $array, $arrow)` is called by nothing and passes its two
/// locals to `CoreExtension::filter()`; that froze `$array` and `$arrow` to `int` for the whole
/// build, so `$arrow($v, $k)` read as "Cannot call $arrow — not a callable (got Int)" and
/// `new \IteratorIterator($array)` as "expects Object(Traversable), got Int" — four refused calls
/// across `filter`, `map`, `find` and `reduce`, on code PHP runs.
///
/// The methods equivalent was already right: `method_body_param_type` answers `mixed` for an
/// undeclared parameter no call site has specialized.
///
/// The fixture reproduces the shape exactly: the only STATIC caller of each method is an uncalled
/// wrapper, and the real calls arrive through a callable array, which is how Twig reaches them.
/// Both symptoms are present — the callable one and the constructor one — because they are one
/// defect, and asserting stdout rather than "it compiles" keeps the parameters honest at run time
/// as well.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_an_uncalled_function_does_not_freeze_its_callees_parameters() {
    let out = compile_and_run(
        r#"<?php
class Ext {
    public static function filter($array, $arrow)
    {
        $out = [];
        foreach ($array as $k => $v) {
            if ($arrow($v, $k)) {
                $out[$k] = $v;
            }
        }
        return $out;
    }

    public static function wrap($array)
    {
        return new ArrayIterator($array);
    }

    public static function entries(): array
    {
        return [[self::class, 'filter'], [self::class, 'wrap']];
    }
}

function legacy_filter($array, $arrow)
{
    return Ext::filter($array, $arrow);
}

function legacy_wrap($array)
{
    return Ext::wrap($array);
}

$entries = Ext::entries();
$filter = $entries[0];
$wrap = $entries[1];
echo implode(',', $filter([1, 2, 3, 4], fn ($v, $k) => $v % 2 === 0)), "\n";
echo implode(',', iterator_to_array($wrap(['a', 'b']))), "\n";
"#,
    );
    assert_eq!(out, "2,4\na,b\n");
}

/// A CONDITIONALLY declared function nothing calls must not export a fabricated `int` either.
///
/// The fix above covered a plain declaration. `if (!function_exists('f')) { function f($x) {…} }`
/// takes two other routes, and both still fabricated the type:
///
/// - `function_exists('f')` with a literal name force-resolved `f` by checking a call that passed
///   `0` for every parameter, so the guard itself typed `$x` as `int`;
/// - the declaration is a function-variant group, and the end-of-pass resolution of an uncalled
///   group kept the `int` seed the plain-function path widens to `mixed`.
///
/// This is Laravel's `Illuminate/Reflection/helpers.php`: `lazy($class, …)` is guarded, called by
/// nothing, and hands `$class` to an anonymous class whose trait method is declared
/// `Closure $closure` — "parameter $closure expects Callable, got Int" refused the build. The
/// called polyfill beside it shows a guarded function still binds and runs with a string.
///
/// Oracle: php 8.5.10 prints the asserted lines.
#[test]
fn test_an_uncalled_conditional_function_does_not_freeze_its_callees_parameters() {
    let out = compile_and_run(
        r#"<?php
trait ReflectsClosures {
    protected function firstParameterType(Closure $closure): string {
        $params = (new ReflectionFunction($closure))->getParameters();
        return $params === [] ? 'none' : (string) $params[0]->getType();
    }
}
if (!function_exists('lazy_type')) {
    function lazy_type($class, $callback = 0) {
        static $reflector;
        $reflector ??= new class {
            use ReflectsClosures;
            public function typeOf($callback) { return $this->firstParameterType($callback); }
        };
        [$class, $callback] = is_string($class) ? [$class, $callback] : [$reflector->typeOf($class), $class];
        return $class;
    }
}
if (!function_exists('shout')) {
    function shout($s) { return strtoupper($s) . '!'; }
}
echo function_exists('lazy_type') ? "declared\n" : "missing\n";
echo shout('hi'), "\n";
"#,
    );
    assert_eq!(out, "declared\nHI!\n");
}


/// A call may OMIT a by-reference parameter that has a default.
///
/// PHP allows `function f($a, ?array &$out = null)` and a caller that passes only `$a`: the callee
/// writes through `&$out` and the write goes nowhere the caller can see. elephc had nowhere to put
/// it — the default-argument path materializes a VALUE, which is not a cell — so the call reached
/// the backend one operand short of the callee's ABI:
///
///     method call to ContainerBuilder::resolveEnvPlaceholders with 3 operands for 4 ABI params
///
/// The omitted position now gets a synthetic local seeded with the parameter's default, passed
/// positionally, after which the ordinary by-reference machinery applies unchanged. It had to be
/// wired into BOTH operand-assembly paths — `lower_method_call` and
/// `lower_method_call_with_receiver` do not share one, and fixing only the second left the direct
/// call still short.
///
/// The fixture calls the same method both ways — omitting the parameter and supplying it — so the
/// synthetic place cannot be standing in for a real one.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_an_omitted_by_ref_parameter_gets_a_place() {
    let out = compile_and_run(
        r#"<?php
class Resolver {
    public function resolve(mixed $value, $format = null, ?array &$used = null): string
    {
        $used[] = 'seen';
        return $value . ':' . var_export($format, true) . ':' . count($used);
    }
}
$r = new Resolver();
echo $r->resolve('a', true), "\n";
$collected = [];
echo $r->resolve('b', false, $collected), "\n";
echo implode(',', $collected), "\n";
"#,
    );
    assert_eq!(out, "a:true:1\nb:false:1\nseen\n");
}


/// A GRADUAL receiver may omit a by-reference parameter, and no caller-side padding can fix it.
///
/// `pad_omitted_by_ref_args` handles the nominal case by synthesising the missing argument in the
/// IR — but that only works when the call has ONE signature to pad to. For a gradual receiver
/// `lower_mixed_method_candidate` builds `param_types`/`ref_params` from EACH CANDIDATE's own
/// signature at dispatch time, so one candidate's third parameter is another's second and the
/// padding would be wrong for whichever one it was not written for. The backend refused:
///
///     receiver-register method call with missing non-value parameter
///
/// The omitted position now gets a DISCARDED CELL, the same thing an argument with no caller
/// variable already got: planned by `plan_ref_arg_temp_cells` with no source operand, seeded to
/// null, its address pushed, and released with the rest of the cell block after the call. The
/// accounting that had to change with it is `arg_temp_bytes` — the fill loop used to push without
/// advancing it because nothing read it afterwards, and a by-reference address is
/// `arg_temp_bytes + cell_offset`.
///
/// The two classes deliberately put `&$seen` at DIFFERENT positions, which is what makes this
/// unreachable by IR padding; verified by making the new helper refuse, which fails this fixture
/// and not the single-candidate one above. Symfony reaches it at
/// `HtmlErrorRenderer::getAndCleanOutputBuffer`, `$request->headers->get('X-Php-Ob-Level', -1)`
/// over a gradual `headers`.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_a_gradual_receiver_may_omit_a_by_ref_parameter() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public function get(string $key, $default = null, ?array &$seen = null): string
    {
        $seen[] = $key;
        return 'bag:' . $key . '=' . var_export($default, true) . '#' . count($seen);
    }
}
class Store {
    public function get(string $key, ?array &$seen = null): string
    {
        $seen[] = $key;
        return 'store:' . $key . '#' . count($seen);
    }
}
function fetch(mixed $o): string { return $o->get('lvl'); }
echo fetch(new Bag()), "\n";
echo fetch(new Store()), "\n";
$kept = [];
echo (new Store())->get('kept', $kept), "\n";
echo implode(',', $kept), "\n";
"#,
    );
    assert_eq!(out, "bag:lvl=NULL#1\nstore:lvl#1\nstore:kept#1\nkept\n");
}

/// Verifies `is_iterable()` narrows a gradual value to `array|Traversable`, so a following
/// `is_array()` guard can leave the Traversable behind.
///
/// Without the narrowing the fall-through kept whatever the parameter arrived as, and the first
/// thing that needs a real object refused it: `twig/twig`'s `CoreExtension::filter()` throws unless
/// `is_iterable($array)`, returns early when `is_array($array)`, and hands what is left to
/// `new \IteratorIterator($array)` -- which the backend answered with
/// `unsupported EIR backend feature: IteratorIterator source PHP type Mixed`.
#[test]
fn test_is_iterable_narrows_a_gradual_value_to_array_or_traversable() {
    let out = compile_and_run(
        r#"<?php
function pick(int $which)
{
    if (0 === $which) {
        return [1, 2, 3, 4];
    }

    return (function () { yield 1; yield 2; yield 3; yield 4; })();
}

function myFilter($array, $arrow)
{
    if (!is_iterable($array)) {
        throw new RuntimeException('expects a sequence');
    }

    if (\is_array($array)) {
        return array_filter($array, $arrow);
    }

    return new \CallbackFilterIterator(new \IteratorIterator($array), $arrow);
}

$even = static fn ($v) => 0 === $v % 2;

$out = [];
foreach (myFilter(pick(0), $even) as $v) {
    $out[] = $v;
}
echo implode(',', $out), ';';

$out2 = [];
foreach (myFilter(pick(1), $even) as $v) {
    $out2[] = $v;
}
echo implode(',', $out2);
"#,
    );
    assert_eq!(out, "2,4;2,4");
}

/// A parameter first called with a string and then with a closure is `mixed`. Callable arguments
/// were left out of the widening, so it stayed `string` and the closure was read as one: `gettype()`
/// answered "string", `$cb('x')` and a forwarded `preg_replace_callback()` fataled.
#[test]
fn test_untyped_param_called_with_string_then_closure_is_mixed() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
function kind($cb) { return gettype($cb); }
function call($cb) { return $cb('x'); }
function forward($cb) { return preg_replace_callback('/P/', $cb, 'APPLE'); }
class K { public static function up($m) { return strtoupper($m[0]); } public function low($m) { return strtolower($m[0]); } }
$c = fn() => 1;
var_dump(kind('x'), kind($c));
var_dump(call('strtoupper'), call(fn($s) => $s . '!'));
var_dump(forward('K::up'), forward(fn($m) => '#'));
var_dump(forward([new K, 'low']), forward(fn($m) => '#'));
"#,
    );
    assert_eq!(out.stdout, r#"string(6) "string"
string(6) "object"
string(1) "X"
string(2) "x!"
string(5) "APPLE"
string(5) "A##LE"
string(5) "AppLE"
string(5) "A##LE"
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// The method half of the rule above: a callable argument widens an untyped METHOD parameter too.
/// One site passing an int (through a `Bag|Some` receiver) and another a closure left `$callable`
/// at `int`, so `$callable(...)` in the body was refused at compile time; phpoption's
/// `Some::filter()` was deferred to the interpreter for it. Instance and static sites both.
#[test]
fn test_untyped_method_param_called_with_int_then_closure_is_mixed() {
    let out = compile_and_run(
        r#"<?php
class Some {
    public function __construct(private $value) {}
    public function filter($callable) { return is_int($callable) ? "int" : ($callable($this->value) ? "yes" : "no"); }
    public static function check($callable) { return is_int($callable) ? "int" : $callable(3); }
}
class Bag {
    public function filter($key) { return "bag:" . $key; }
}
function pick(int $n) { return $n === 1 ? new Bag() : new Some(5); }
$m = pick($argc);
echo $m->filter(42), " ", (new Some(5))->filter(fn($v) => $v === 5), " ", (new Some(5))->filter(7), " ";
echo Some::check(1), " ", Some::check(fn($v) => $v * 2), "\n";
"#,
    );
    assert_eq!(out, "bag:42 yes int int 6\n");
}

/// A method merely NAMED like the stream-filter contract (`filter`, `onCreate`) is an ordinary
/// method when the program can never register a filter. The contract keeps the runtime's raw
/// argument ABI and so the `int` seed of an untyped parameter; a `filter($callback)` only reached
/// dynamically kept it and `array_filter($items, $callback)` was refused at compile time.
#[test]
fn test_filter_named_method_is_ordinary_without_stream_filters() {
    let out = compile_and_run(
        r#"<?php
class Pipeline {
    public function __construct(private array $items) {}
    public function filter($callback) { return array_values(array_filter($this->items, $callback)); }
    public function onCreate($label) { return "created:" . $label; }
}
$p = new Pipeline([1, 2, 3, 4]);
$method = $argc > 5 ? 'nothing' : 'filter';
echo json_encode($p->$method(fn($v) => $v % 2 === 0)), " ";
$hook = 'onCreate';
echo $p->$hook("x"), "\n";
"#,
    );
    assert_eq!(out, "[2,4] created:x\n");
}

/// A recursive self-call widens its own parameters. The call cannot re-specialize the function
/// it sits in, so the body kept the first call's `string` for `$key` and the recursive call
/// converted its array to "Array": Laravel's `data_get()`/`data_forget()` walked the wrong keys.
#[test]
fn test_recursive_self_call_widens_the_parameter_it_passes() {
    let out = compile_and_run(
        r#"<?php
function walk($key) {
    $segments = is_array($key) ? $key : explode('.', $key);
    $segment = array_shift($segments);
    return $segments ? $segment . '>' . walk($segments) : $segment;
}
function forget(&$target, $key) {
    $segments = is_array($key) ? $key : explode('.', $key);
    $segment = array_shift($segments);
    if ($segments && array_key_exists($segment, $target)) {
        forget($target[$segment], $segments);
    } else {
        unset($target[$segment]);
    }
    return $target;
}
$a = ['x' => ['y' => 1, 'z' => 2]];
forget($a, 'x.y');
echo walk('a.b.c'), " ", json_encode($a), "\n";
"#,
    );
    assert_eq!(out, "a>b>c {\"x\":{\"z\":2}}\n");
}

/// A recursive untyped function called first with an array and then, from its own body, with the
/// array's elements (strings, ints, nested arrays) must receive those elements as they are. The
/// inferred parameter type was a container, and the recursive call narrowed its `mixed` argument
/// to that container at the call site, so the first string element threw "Value must be of type
/// array, string given" (a `var_dump`-style walker over `preg_match_all()`'s matches). Oracle: php.
#[test]
fn test_recursive_untyped_walker_receives_mixed_elements_unnarrowed() {
    let out = compile_and_run(
        r#"<?php
function walk($v) {
    if (is_array($v)) {
        $parts = [];
        foreach ($v as $k => $item) {
            $parts[] = $k . '=' . walk($item);
        }
        return '[' . implode(',', $parts) . ']';
    }
    return (string)$v;
}
echo walk(['a' => [1, 'x', [2.5, 'deep']], 'b' => 's', 3 => true]), "\n";
$n = preg_match_all('/([0-9]+)([a-z])/', 'a1b22c333d', $m);
echo $n, ':', walk($m), "\n";
"#,
    );
    assert_eq!(
        out,
        "[a=[0=1,1=x,2=[0=2.5,1=deep]],b=s,3=1]\n3:[0=[0=1b,1=22c,2=333d],1=[0=1,1=22,2=333],2=[0=b,1=c,2=d]]\n"
    );
}
