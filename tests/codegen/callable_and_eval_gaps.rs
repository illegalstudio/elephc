//! Purpose:
//! Callable and eval-bridge gaps closed together: calling a first-class callable where it is
//! made, every `Closure::fromCallable()` form, request-wide eval statics, and `$v = &call()`
//! over a callee that returns a value.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Expected stdout is `php -n` 8.5's own; php's notices go to stderr here, as elephc's do.

use crate::support::*;

/// `$o->m(...)()` and `f(...)(x)` call the Closure they just built; the parser stopped at the
/// second `(`. `$o->me()->m(...)` over an untyped `return $this` also named the class "".
#[test]
fn test_a_first_class_callable_is_called_where_it_is_made() {
    let out = compile_and_run(r#"<?php
class K {
    public $n = 3;
    function m($x = 1) { return "m" . ($x + $this->n); }
    static function s($x) { return "s" . $x; }
    function me() { return $this; }
}
$o = new K;
echo $o->m(...)(), "\n";
echo $o->m(...)(5), "\n";
echo K::s(...)("a"), "\n";
echo strtoupper(...)("abc"), "\n";
echo $o->me()->m(...)(2), "\n";
echo $o->m(...)(...)(7), "\n";
"#);
    assert_eq!(out, r#"m4
m8
sa
ABC
m5
m10
"#);
}

/// Only a function-name string was rewritten to a first-class callable; every other literal
/// reached the eval bridge, which leaked its argument and refused a compiled Closure outright.
#[test]
fn test_closure_from_callable_accepts_every_literal_callable() {
    let out = compile_and_run(r#"<?php
class K {
    public $n = 3;
    function m($x = 1) { return "m" . ($x + $this->n); }
    static function s($x = "d") { return "s" . $x; }
}
$o = new K;
echo Closure::fromCallable([$o, 'm'])(2), "\n";
echo Closure::fromCallable('K::s')("a"), "\n";
echo Closure::fromCallable(['K', 's'])(), "\n";
echo Closure::fromCallable('strtoupper')("x"), "\n";
$c = Closure::fromCallable(function ($v) { return $v * 2; });
echo $c(21), "\n";
echo get_class(Closure::fromCallable([new K, 'm'])), "\n";
$f = Closure::fromCallable(fn($v) => $v + 1);
var_dump($f instanceof Closure, $f(1));
$g = function () { return "same"; };
var_dump(Closure::fromCallable($g) === $g);
echo Closure::fromCallable($g)(), "\n";
$h = $g;
var_dump($g === $h, $g === $c, $g !== $c);
"#);
    assert_eq!(out, r#"m5
sa
sd
X
42
Closure
bool(true)
int(2)
bool(true)
same
bool(true)
bool(false)
bool(true)
"#);
}

/// A `mixed` callback still goes through the bridge, which answered a compiled Closure with
/// `function "" not found`: Twig's `ReflectionCallable` does exactly this for a closure filter.
#[test]
fn test_closure_from_callable_over_a_runtime_value() {
    let out = compile_and_run(r#"<?php
class K {
    public $n = 3;
    function m($x = 1) { return "m" . ($x + $this->n); }
    static function s($x = "d") { return "s" . $x; }
}
function mk(mixed $callable): Closure {
    try {
        $closure = \Closure::fromCallable($callable);
    } catch (\TypeError $e) {
        throw new \LogicException("not callable", 0, $e);
    }
    return $closure;
}
$g = fn($v) => $v * 2;
echo mk($g)(21), "\n";
var_dump(mk($g) === $g);
echo mk('strtoupper')("x"), "\n";
echo mk([new K, 'm'])(2), "\n";
echo mk('K::s')("a"), "\n";
echo mk(['K', 's'])(), "\n";
try { mk('nope_fn'); } catch (LogicException $e) { echo get_class($e->getPrevious()), "\n"; }
"#);
    assert_eq!(out, r#"42
bool(true)
X
m5
sa
sd
TypeError
"#);
}

/// Each compiled activation builds its own eval context, and statics lived in the context:
/// `static $c` and `self::$shared` restarted every time the calling function returned.
#[test]
fn test_eval_class_statics_outlive_the_calling_function() {
    let out = compile_and_run(r#"<?php
class EvalPrelude {}
eval('
class Z {
    static $shared = [];
    function counter() { static $c = 0; $c++; return $c; }
    static function remember($v) { self::$shared[] = $v; return count(self::$shared); }
}
');
function run() {
    $z = new Z;
    echo $z->counter(), $z->counter(), " ", Z::remember(1), Z::remember(2), "\n";
}
run();
run();
"#);
    assert_eq!(out, r#"12 12
34 34
"#);
}

/// `$v = &f()` over a by-value callee bound the returned VALUE as a reference cell and
/// faulted on the next access. php assigns it (with a notice).
///
/// A by-reference method DECLARED INSIDE eval() and called from compiled code is not covered:
/// binding the caller to a reference into the interpreter's property store is the same missing
/// cross-boundary reference as `$r = &$evalObject->prop`, which the backend refuses
/// (`load_prop_ref_cell for dynamic or missing property`).
#[test]
fn test_reference_bind_of_a_by_value_call_takes_the_value() {
    let out = compile_and_run(r#"<?php
class N { public $items = ["a"]; function &ref() { return $this->items; } function val() { return $this->items; } }
function g() { return [1]; }
function run() {
    $n = new N;
    $r = &$n->ref();
    $r[] = "b";
    echo implode(",", $n->items), "\n";
    $v = &$n->val();
    $v[] = "c";
    echo implode(",", $n->items), " ", implode(",", $v), "\n";
    $w = &g();
    $w[] = 2;
    echo implode(",", $w), "\n";
}
run();
"#);
    assert_eq!(out, r#"a,b
a,b a,b,c
1,2
"#);
}

/// A constant-true guard folds to a plain block before checking, and the return fold skipped
/// such blocks: the guarded `return false` vanished from the inferred type, which became the
/// other exit's alone. An array exit failed codegen; a string exit silently answered `""`.
/// (`opcache_get_status()` has exactly this shape when the build disables the cache.)
#[test]
fn test_a_return_behind_a_constant_guard_counts_toward_the_return_type() {
    let out = compile_and_run(r#"<?php
function gate_false() { if (false === false) { return false; } $status = ["x" => 1]; return $status; }
function gate_true() { if (true) { return 5; } return ["x" => 1]; }
function gate_string() { if (false === false) { return false; } $s = "x"; return $s; }
var_dump(gate_false(), gate_true(), gate_string());
"#);
    assert_eq!(out, r#"bool(false)
int(5)
bool(false)
"#);
}

/// A dynamic method call whose only argument is a spread binds the spread's string keys to the
/// parameters NAMED by them, as php does: `$this->{$m}(...['data' => [1], 'view' => 'v'])` reaches
/// `named('v', [1])`. The call used to be desugared to `call_user_func`, which passes every entry
/// positionally. Laravel's `ViewController::callAction` is exactly this shape. Output is php 8.5's.
#[test]
fn test_dynamic_method_call_with_a_lone_spread_binds_string_keys_by_name() {
    let out = compile_and_run(r#"<?php
class K {
    public function collect(...$args) { return json_encode($args); }
    public function named($view, $data = []) { return json_encode([$view, $data]); }
    public function viaName($m, $p) { return $this->{$m}(...$p); }
    public function viaPositional($m, $a, $b) { return $this->{$m}($a, $b); }
}
$k = new K();
echo $k->viaName('named', ['data' => [1], 'view' => 'v']), "\n";
echo $k->viaName('named', ['v', [2]]), "\n";
echo $k->viaPositional('named', 'w', [3]), "\n";
echo $k->viaName('collect', ['a', 'b']), "\n";
"#);
    assert_eq!(out, "[\"v\",[1]]\n[\"v\",[2]]\n[\"w\",[3]]\n[\"a\",\"b\"]\n");
}

/// A compiled user function whose name begins with an underscore (`__()` and `_e()`, the
/// translation helpers frameworks define) is an ordinary function that interpreted code can call,
/// find with `function_exists()` and use as a callable. eval hid every compiled function whose
/// name started with `_` to keep the compiler's synthetic functions private, so a Laravel view
/// calling `__('Not Found')` died with "call to undefined function __()". Output is php 8.5's.
#[test]
fn test_eval_sees_compiled_functions_whose_names_start_with_an_underscore() {
    let out = compile_and_run(r#"<?php
function __($key = null) { return 'tr:' . $key; }
function _e($key) { return 'e:' . $key; }
if (! function_exists('_cond')) {
    function _cond($key) { return 'c:' . $key; }
}
echo __('a'), _e('a'), _cond('a'), "\n";
$code = 'echo __("b"), _e("b"), _cond("b"), "\n";
echo call_user_func("__", "c"), call_user_func("_e", "c"), call_user_func("_cond", "c"), "\n";
var_dump(function_exists("__"), function_exists("_e"), function_exists("_cond"));';
eval($code);
"#);
    assert_eq!(
        out,
        "tr:ae:ac:a\ntr:be:bc:b\ntr:ce:cc:c\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// `get_defined_vars()` inside interpreted code answers the running scope's variables: the ones
/// the compiled caller exposes plus those the fragment bound, without an `unset` one and without
/// `$this`. It was "call to undefined function get_defined_vars()", which killed every Laravel
/// page whose Blade view uses `@include` (it compiles to
/// `array_diff_key(get_defined_vars(), ['__data' => 1, '__path' => 1])`). The ORDER php reports
/// is the lexical slot order of the enclosing function, which an interpreter cannot know, so the
/// fixture sorts the keys; and a compiled local that was never assigned is exposed to the
/// fragment as null, so the fixture keeps only non-null entries. Output is php 8.5's.
#[test]
fn test_eval_get_defined_vars_reports_the_running_scope() {
    let out = compile_and_run(r#"<?php
function render($__path, $__data) {
    extract($__data);
    $local = 5;
    $code = '$more = 1; $zap = 2; unset($zap); $vars = array_filter(array_diff_key(get_defined_vars(), ["__data" => 1, "__path" => 1, "code" => 1]), fn($v) => $v !== null); ksort($vars); echo json_encode($vars), "\n";';
    eval($code);
}
render('p', ['a' => 1, 'b' => [2, 3]]);
class T {
    public $p = 1;
    function run() { $q = 2; $code = '$names = array_keys(array_filter(get_defined_vars(), fn($v) => $v !== null)); sort($names); echo json_encode($names), "\n";'; eval($code); }
}
(new T)->run();
"#);
    assert_eq!(
        out,
        "{\"a\":1,\"b\":[2,3],\"local\":5,\"more\":1}\n[\"code\",\"q\"]\n"
    );
}

/// A constant the compiled program `define()`s is visible to interpreted code: `defined()` is
/// true and a bare name reads the value, as in php's single constant namespace. The compiled
/// `define()` kept its value in a program global the interpreter could not read, so Laravel's
/// `health-up` view (`@if (defined('LARAVEL_START'))`) silently dropped its timing line. The value
/// is the runtime one (`$n * 2`), not a folded literal, and an undefined name stays undefined.
/// Output is php 8.5's.
#[test]
fn test_eval_sees_constants_the_compiled_program_defined() {
    let out = compile_and_run(r#"<?php
$n = (int) "21";
define('LV_INT', $n * 2);
define('LV_STR', 'a' . 'b' . $n);
define('LV_FLOAT', $n / 4);
define('LV_BOOL', $n > 5);
$code = 'var_dump(defined("LV_INT"), LV_INT, LV_STR, LV_FLOAT, LV_BOOL, constant("LV_STR"), defined("LV_NOPE"));';
eval($code);
"#);
    assert_eq!(
        out,
        "bool(true)\nint(42)\nstring(4) \"ab21\"\nfloat(5.25)\nbool(true)\nstring(4) \"ab21\"\nbool(false)\n"
    );
}

/// A static call on a name an SPL autoloader only makes known as a `class_alias()` dispatches under
/// the REAL class: `static::class` is the target's name and `static::$prop` reads the target's
/// slot. The interpreter resolved the receiver before the autoloader ran, so the late-static class
/// of the call stayed the alias spelling the compiled side cannot map to a class, and Laravel's
/// facade root (`static::getFacadeRoot()`, read through that class) came back as garbage on the
/// first `Route::has()` of a Blade view. Output is php 8.5's.
#[test]
fn test_static_call_through_an_autoloaded_class_alias_uses_the_real_class() {
    let out = compile_and_run(r#"<?php
class Router { public function has($name) { return $name === "login"; } }
abstract class Facade {
    protected static $app;
    protected static $resolvedInstance;
    protected static function getFacadeAccessor() { throw new RuntimeException('no accessor'); }
    public static function setApp($app) { static::$app = $app; }
    public static function getFacadeRoot() { return static::resolveFacadeInstance(static::getFacadeAccessor()); }
    protected static function resolveFacadeInstance($name) {
        if (isset(static::$resolvedInstance[$name])) { return static::$resolvedInstance[$name]; }
        if (static::$app) { return static::$resolvedInstance[$name] = static::$app[$name]; }
    }
    public static function __callStatic($method, $args)
    {
        $instance = static::getFacadeRoot();
        echo static::class, ' ', get_debug_type($instance), "\n";
        return $instance->$method(...$args);
    }
}
class RouteFacade extends Facade { protected static function getFacadeAccessor() { return 'router'; } }
$loaderCode = 'class Loader {
    protected $aliases = ["Route" => "RouteFacade"];
    public function load($alias) {
        if (isset($this->aliases[$alias])) { return class_alias($this->aliases[$alias], $alias); }
    }
    public function register() { spl_autoload_register([$this, "load"], true, true); }
}';
eval($loaderCode);
(new Loader)->register();
Facade::setApp(['router' => new Router()]);
$code = 'var_dump(Route::has("login")); var_dump(Route::has("other"));';
eval($code);
"#);
    assert_eq!(
        out,
        "RouteFacade Router\nbool(true)\nRouteFacade Router\nbool(false)\n"
    );
}

/// A Throwable raised by a method reached through `call_user_func_array([$mixed, 'm'], ...)`
/// reaches the caller's `catch`. Instance-method descriptors are bounded for the interpreter: the
/// invoker catches the throw and returns null with it pending, and the owned-argument boundary
/// AOT code calls them through restored the previous exception over it, so the call yielded null.
/// With eval in the program every method carries the hidden `func_get_args()` collector, which
/// also stopped counting as a declared variadic: `$mixed->run(...)` routed through that same
/// callable path instead of the class-id dispatch.
#[test]
fn test_a_throw_through_a_mixed_receiver_method_reaches_the_catch() {
    let out = compile_and_run(r#"<?php
class A {
    public function run(?object $x): string {
        if ($x instanceof ArrayObject) { throw new LogicException('A cannot run'); }
        return 'A ok';
    }
}
class B extends A {
    public function run(?object $x): string { return 'B:' . parent::run($x); }
}
function viaCallable($r) {
    try {
        echo call_user_func_array([$r, 'run'], [new ArrayObject([])]), "\n";
    } catch (LogicException $e) {
        echo 'callable ', $e->getMessage(), "\n";
    }
}
function viaMethod($r) {
    try {
        echo $r->run(new ArrayObject([])), "\n";
    } catch (LogicException $e) {
        echo 'method ', $e->getMessage(), "\n";
    }
    echo $r->run(null), "\n";
}
foreach ([new A(), new B()] as $r) {
    viaCallable($r);
    viaMethod($r);
}
echo eval('return "e";'), "\n";
"#);
    assert_eq!(
        out,
        "callable A cannot run\nmethod A cannot run\nA ok\ncallable A cannot run\nmethod A cannot run\nB:A ok\ne\n"
    );
}

/// A Fiber whose body is an instance method rethrows that method's Throwable out of `start()`.
/// The Fiber start wrapper calls the descriptor's invoker directly, and an instance-method
/// invoker is bounded: it returned null with the throw pending, `start()` returned normally, and
/// the Throwable resurfaced as an uncaught fatal at exit.
#[test]
fn test_a_fiber_running_a_throwing_method_rethrows_from_start() {
    let out = compile_and_run(r#"<?php
class A {
    public function run($x) { throw new LogicException('A ' . $x); }
}
function f($r) {
    $fiber = new Fiber([$r, 'run']);
    try {
        $fiber->start(1);
        echo "no throw\n";
    } catch (LogicException $e) {
        echo 'caught ', $e->getMessage(), "\n";
    }
    $g = new Fiber($r->run(...));
    try {
        $g->start(2);
        echo "no throw\n";
    } catch (LogicException $e) {
        echo 'caught ', $e->getMessage(), "\n";
    }
}
f(new A());
echo eval('return "e";'), "\n";
"#);
    assert_eq!(out, "caught A 1\ncaught A 2\ne\n");
}

/// A builtin's NAME registers as a handler: `set_exception_handler('var_dump')` is how Symfony's
/// `DebugHandlersListener` probes the active handler on every request. The registration resolves
/// the string to a callable descriptor at once, and runtime string dispatch had no case for
/// `var_dump` or `print_r`, so it fataled "Call to undefined function <dynamic>()".
#[test]
fn test_builtin_names_register_as_exception_and_error_handlers() {
    let out = compile_and_run(r#"<?php
class Listener {
    public $earlyHandler;
    public function __construct()
    {
        $handler = set_exception_handler('var_dump');
        $this->earlyHandler = \is_array($handler) ? $handler[0] : null;
        restore_exception_handler();
    }
}
$l = new Listener();
var_dump($l->earlyHandler);
var_dump(set_exception_handler('var_dump'));
var_dump(set_exception_handler('print_r'));
restore_exception_handler();
restore_exception_handler();
var_dump(set_error_handler('var_dump'));
restore_error_handler();
echo "done\n";
"#);
    assert_eq!(out, "NULL\nNULL\nstring(8) \"var_dump\"\nNULL\ndone\n");
}

/// `clone $this` in a program that links the eval bridge returns the clone, for a subclass
/// receiver too (Symfony's `ServiceLocator::withContext()`).
///
/// Two interpreter callbacks shared ONE runtime slot: the clone operation a generated `clone`
/// offers each object to beforehand (six arguments, a status answer), and the adoption callback
/// that registers an eval-declared clone afterwards (two arguments, no answer). The adoption one
/// was installed last, so the operation call read its leftover return register as "handled" and
/// took a null clone; the next property write through it segfaulted.
#[test]
fn test_clone_in_an_eval_program_returns_the_clone_for_a_subclass() {
    let out = compile_and_run(r#"<?php
class Container { public string $name = 'c'; }
trait LocatorTrait {
    private array $loading = [];
    private array $providedTypes;
    public function __construct(private array $factories) {}
    public function has(string $id): bool { return isset($this->factories[$id]); }
}
class Locator {
    use LocatorTrait;
    private ?string $externalId = null;
    private ?Container $container = null;
    public function withContext(string $externalId, Container $container): static
    {
        $locator = clone $this;
        $locator->externalId = $externalId;
        $locator->container = $container;
        return $locator;
    }
    public function describe(): string { return ($this->externalId ?? '-') . ':' . ($this->container?->name ?? '-') . ':' . (int) $this->has('a'); }
}
class SubLocator extends Locator {}
$l = new Locator(['a' => fn () => 1]);
echo $l->withContext('x', new Container())->describe(), ' ', $l->describe(), "\n";
$s = new SubLocator(['b' => fn () => 2]);
echo $s->withContext('y', new Container())->describe(), "\n";
eval('class Dyn extends Locator { public $extra = "x"; public function __clone() { $this->extra .= "!"; } }');
$name = 'Dyn';
$d = new $name(['a' => 1]);
$c = $d->withContext('z', new Container());
echo get_class($c), ' ', $c->describe(), ' ', $c->extra, ' ', $d->extra, "\n";
"#);
    assert_eq!(out, "x:c:1 -:-:1\ny:c:0\nDyn z:c:1 x! x\n");
}

/// An instance-method error handler that THROWS (Symfony's `ErrorHandler::handleError()` turning
/// a warning into `ErrorException`) unwinds into the caller's `catch`, for `trigger_error()` and
/// for warnings the runtime raises itself.
///
/// Such a handler runs through a bounded invoker, which returns null with the Throwable left
/// pending; `__rt_error_handler_invoke` handed that null back and the warning path dereferenced it
/// as the handler's boxed answer, so every worker segfaulted on its first warning.
#[test]
fn test_throwing_instance_method_error_handler_reaches_the_catch() {
    let out = compile_and_run(r#"<?php
class EH {
    public int $calls = 0;
    public function handleError(int $type, string $message, string $file = '', int $line = 0): bool
    {
        $this->calls++;
        if (!str_contains($message, 'quiet')) {
            throw new ErrorException('handled: ' . $message, 0, $type);
        }
        return true;
    }
}
$h = new EH();
set_error_handler([$h, 'handleError']);
function read(array $a, string $k) { return $a[$k]; }
try {
    trigger_error('boom', E_USER_WARNING);
    echo "not reached\n";
} catch (ErrorException $e) {
    echo 'caught ', $e->getMessage(), "\n";
}
trigger_error('quiet', E_USER_NOTICE);
try {
    $x = read([], 'missing');
    echo "not reached\n";
} catch (ErrorException $e) {
    echo 'caught ', $e->getMessage(), "\n";
}
echo 'calls=', $h->calls, "\n";
restore_error_handler();
echo eval('return "e";'), "\n";
"#);
    assert_eq!(
        out,
        "caught handled: boom\ncaught handled: Undefined array key \"missing\"\ncalls=3\ne\n"
    );
}

/// `getTrace()` of a Throwable whose class declares properties of its own (`ErrorException`, or a
/// user subclass adding one) is an array that flows into an `array` parameter.
///
/// Such a class runs its property initializer, which stores the `trace` default boxed, and the
/// getter read that box as a raw array: it returned the box's header words as elements, and passed
/// on, the boxed result arrived as `null` -- Symfony's `ErrorHandler::cleanTrace()` threw
/// "Argument must be of type array, null given" on every warning. php also records the calling
/// frame here; that part is a separate gap, so only the shape is asserted.
#[test]
fn test_get_trace_of_a_throwable_with_declared_properties_is_an_array() {
    let out = compile_and_run(r#"<?php
class MyEx2 extends Exception { protected int $severity = 1; public function __construct(string $m = '') { parent::__construct($m); } }
class H {
    private function clean(array $backtrace, int $type, string &$file, int &$line, bool $throw): array { return $backtrace; }
    public function a(int $type, string $file, int $line): string {
        $bt = (new \ErrorException('m', 0, $type, $file, $line))->getTrace();
        return get_debug_type($this->clean($bt, $type, $file, $line, false));
    }
    public function b(int $type): string {
        $file = 'f'; $line = 1;
        $bt = (new MyEx2('m'))->getTrace();
        return get_debug_type($this->clean($bt, $type, $file, $line, false));
    }
}
$h = new H();
echo 'a=', $h->a(2, 'f', 1), ' b=', $h->b(2), "\n";
foreach ([new ErrorException('m'), new MyEx2('m'), new RuntimeException('m')] as $e) {
    $t = $e->getTrace();
    echo get_debug_type($t), ':', is_array($t) && array_is_list($t) ? 'list' : 'other', ' ';
}
echo "\n", eval('return "e";'), "\n";
"#);
    assert_eq!(out, "a=array b=array\narray:list array:list array:list \ne\n");
}

/// A compiled `new $class(...)` that reaches the eval bridge before the program's first `eval()`
/// runs the constructor with its arguments (Symfony's router building `CompiledUrlMatcher`).
///
/// The call passes a null eval handle, and the runtime's fallback context synced the module's
/// AOT metadata snapshot -- which only the registration helper publishes, and only once a
/// compiled body built its own eval context. Nothing had, so the context knew no constructor
/// signature, never bound the hidden `func_get_args` collector, and the constructor bridge
/// refused the one-argument call on arity: "eval() runtime failed" for every class.
#[test]
fn test_dynamic_new_before_the_first_eval_runs_the_constructor() {
    let out = compile_and_run(r#"<?php
class NInt { public int $v = 0; public function __construct(int $x) { $this->v = $x; } }
class NArr { public int $v = 0; public function __construct(array $x) { $this->v = count($x); } }
class Matcher {
    private bool $matchHost = false;
    private array $staticRoutes = [];
    private ?\Closure $checkCondition;
    public function __construct(array $compiledRoutes)
    {
        [$this->matchHost, $this->staticRoutes, $this->checkCondition] = $compiledRoutes;
    }
    public function n(): int { return count($this->staticRoutes); }
}
$c = $argc > 5 ? 'X' : 'NInt';
$o = new $c(3);
echo $c, ' ', $o->v, "\n";
$c = $argc > 5 ? 'X' : 'NArr';
$o = new $c([1, 2]);
echo $c, ' ', $o->v, "\n";
$c = $argc > 5 ? 'X' : 'Matcher';
$o = new $c([false, ['/' => 1, '/x' => 2], null]);
echo $c, ' ', $o->n(), "\n";
echo eval('return "e";'), "\n";
"#);
    assert_eq!(out, "NInt 3\nNArr 2\nMatcher 2\ne\n");
}

/// A `callable` parameter given `[$object, 'method']` still holds that array: `is_array()` is
/// true, it indexes, and it spreads into a builtin and a constructor -- from the parameter and
/// after it is stored in an untyped property (Symfony's `ControllerEvent::setController()`).
///
/// Two defects stood between. The argument was lowered to a first-class callable for its target,
/// which is a Closure, so `is_array()` answered false; and a builtin's iterator-based unpack
/// walked the stored descriptor as a non-iterable, warning "foreach() argument must be of type
/// array|object" and raising "Too few arguments for unpacked call".
#[test]
fn test_callable_array_parameter_stays_an_array_through_storage_and_spreads() {
    let out = compile_and_run(r#"<?php
class HC { public function home(): string { return 'h'; } }
class Holder {
    private $c;
    public function set(callable $c): void { $this->c = $c; }
    public function isArr(): bool { return \is_array($this->c); }
    public function exists(): bool { return method_exists(...$this->c); }
    public function first(): string { return get_class($this->c[0]) . ' ' . $this->c[1]; }
    public function refl(): string { $m = new \ReflectionMethod(...$this->c); return $m->class . '::' . $m->name; }
    public function reflClass(): string { return (new \ReflectionClass($this->c[0]))->getName(); }
}
function existsParam(callable $c): bool { return \is_array($c) && method_exists(...$c); }
$o = new HC();
echo var_export(existsParam([$o, 'home']), true), "\n";
$h = new Holder();
$h->set([$o, 'home']);
echo var_export($h->isArr(), true), "\n";
echo $h->first(), "\n";
echo var_export($h->exists(), true), "\n";
echo $h->refl(), "\n";
echo $h->reflClass(), "\n";
echo eval('return "e";'), "\n";
"#);
    assert_eq!(out, "true\ntrue\nHC home\ntrue\nHC::home\nHC\ne\n");
}

/// A named-argument call to a class the compiler never saw reaches the autoloader like a
/// positional call does (Laravel's `Str::ascii()` -> `ASCII::to_ascii(..., replace_single_chars_only:
/// false)`): it was compiled into a `Class "..." not found` fatal. Expected output verified
/// against php 8.5.
#[test]
fn test_named_argument_static_call_on_runtime_declared_class_autoloads() {
    let out = compile_and_run(
        r#"<?php
function viaStatic($v, $language = 'en') { return Dyn2::go((string) $v, $language, replace_single_chars_only: false); }
spl_autoload_register(function ($c) { echo "[autoload $c]"; eval('class Dyn2 { public static function go(string $s, $lang = "en", bool $replace_single_chars_only = false) { return $s . "-" . $lang . "-" . var_export($replace_single_chars_only, true); } }'); });
echo viaStatic('b'), "\n";
echo viaStatic('c', 'fr'), "\n";
"#,
    );
    assert_eq!(out, "[autoload Dyn2]b-en-false\nc-fr-false\n");
}

/// Interpreted `Foo::$prop` reads and writes give the SPL autoloaders their chance at a class
/// nobody has declared yet (Laravel's `Serializers\\Native::$transformUseVariables = ...` under
/// artisan died with `Class "..." not found`). Expected output verified against php 8.5.
#[test]
fn test_interpreted_static_property_write_autoloads_the_class() {
    let out = compile_and_run(
        r#"<?php
spl_autoload_register(function ($c) { echo "[autoload $c]"; eval('class Late2 { public static $hook = null; public static $n = 0; }'); });
$code = 'function setIt($f) { Late2::$hook = $f; Late2::$n = 7; return [Late2::$hook === null ? "null" : "set", Late2::$n]; }';
eval($code);
echo json_encode(setIt(fn () => 1)), "\n";
"#,
    );
    assert_eq!(out, "[autoload Late2][\"set\",7]\n");
}

/// A `mixed &$param` kept by a closure stays bound to the caller's variable when the receiver is
/// only known as a union (`S|false`). Such a call used to go through the eval bridge, which hands
/// the callee a frame-address marker instead of the caller's durable cell, so the closure read
/// back an empty value (`PDOStatement::bindParam()` on `prepare()`'s `PDOStatement|false`).
/// Expected output verified against php 8.5.
#[test]
fn test_by_ref_mixed_parameter_kept_by_a_closure_survives_a_union_receiver() {
    let out = compile_and_run(r#"<?php
class S {
    private array $getters = [];
    public function bind($slot, mixed &$variable): bool {
        $this->getters[] = function () use (&$variable): mixed { return $variable; };
        return $slot > 0;
    }
    public function read(): void { foreach ($this->getters as $getter) echo $getter(), "\n"; }
}
function make(): S|false { return new S(); }
$n = 42;
$s = make();
$s->bind(1, $n);
$n = 43;
$s->read();
"#);
    assert_eq!(out, "43\n");
}

/// A first-class callable of an interpreted class's private or protected method keeps the scope
/// it was created in: compiled code may call it directly, through `call_user_func*`, and through
/// `array_map`, while a plain `[$o, 'prot']` array is still refused from outside the class.
/// Symfony's dumped container hands `$this->getService(...)` to a compiled `ServiceLocator`,
/// whose `($this->factory)(...)` failed with "Call to protected method ... from global scope".
#[test]
fn test_interpreted_method_closure_keeps_its_creation_scope() {
    let out = compile_and_run(r#"<?php
$o = eval('class K {
    private function secret($x) { return "s" . $x; }
    protected function prot($x) { return "p" . $x; }
    public function closures(): array { return [$this->secret(...), $this->prot(...), [$this, "prot"]]; }
} return new K();');
[$s, $p, $arr] = $o->closures();
echo $s(1), " ", $p(2), "\n";
echo call_user_func($s, 3), " ", call_user_func_array($p, [4]), "\n";
echo implode(",", array_map($p, [5, 6])), "\n";
try { call_user_func($arr, 7); } catch (TypeError $e) { echo str_contains($e->getMessage(), "cannot access protected method K::prot()") ? "refused" : $e->getMessage(), "\n"; }
"#);
    assert_eq!(out, "s1 p2\ns3 p4\np5,p6\nrefused\n");
}

