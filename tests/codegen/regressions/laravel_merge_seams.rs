//! Purpose:
//! Regression tests for defects found when the Laravel `--web` line was merged with this branch:
//! each one only appeared once both sides' changes met.

use super::*;

/// A hash property returned under a bare `array` return type is handed back with its own
/// reference. The return used to fall to the generic runtime conversion, which gave the caller the
/// property's storage without one: the caller's release freed the property, and the next call read
/// freed memory (Symfony's `ResponseHeaderBag::all()` in Laravel's `sendHeaders()`).
#[test]
fn test_hash_property_returned_as_array_keeps_its_own_reference() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Bag {
    protected array $headers = [];
    public function set(string $key, string $value): void {
        $this->headers[strtolower($key)] = [$value];
    }
    public function all(?string $key = null): array {
        if (null !== $key) {
            return $this->headers[$key] ?? [];
        }
        return $this->headers;
    }
}
class RBag extends Bag {
    protected array $cookies = [];
    public function all(?string $key = null): array {
        $headers = parent::all();
        foreach ($this->cookies as $cookie) {
            $headers['set-cookie'][] = (string) $cookie;
        }
        return $headers;
    }
}
function run(): void {
    $h = new RBag();
    $h->set('Content-Type', 'application/json');
    for ($i = 0; $i < 3; $i++) {
        echo json_encode($h->all()), "\n";
    }
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"content-type\":[\"application\\/json\"]}\n".repeat(3));
}

/// A callback that declares fewer parameters than `array_map()` passes ignores the surplus, as in
/// php. The literal `['K', 's']` became a direct static binding, whose call carried the element as
/// an operand the method has no slot for, and the build failed.
#[test]
fn test_array_map_with_a_static_callback_taking_fewer_parameters() {
    let out = compile_and_run(
        r#"<?php
class K { static function s() { return "x" . mt_rand(1, 1); } }
echo implode(",", array_map(['K', 's'], [1, 2])), "\n";
echo implode(",", array_map('K::s', [3])), "\n";
"#,
    );
    assert_eq!(out, "x1,x1\nx1\n");
}

/// An unqualified constant nothing defines is reported under the name the source wrote: php's
/// `Error` names `App\BARE_UNKNOWN` even though the lookup falls back to the global constant.
#[test]
fn test_undefined_namespaced_constant_error_names_the_written_form() {
    let out = compile_and_run(
        r#"<?php
namespace App;
try { echo BARE_UNKNOWN; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
try { echo \FOURTH_UNKNOWN; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "Undefined constant \"App\\BARE_UNKNOWN\"\nUndefined constant \"FOURTH_UNKNOWN\"\n");
}

/// Runtime `constant()` misses caught twice at the top level keep their `Error` intact. With the
/// eval interpreter linked into every program (an `isset($mixed[$k])` in the session prelude
/// asked `offsetExists()` through it), the second catch tripped the heap validator.
#[test]
fn test_runtime_constant_misses_are_caught_twice_at_top_level() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = $argc > 5 ? "x" : "Y";
try { echo constant("FOURTH" . $n); } catch (\Error $e) { echo $e->getMessage(), "\n"; }
try { echo constant("FIFTH" . $n); } catch (\Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Undefined constant \"FOURTHY\"\nUndefined constant \"FIFTHY\"\n");
}

/// `iterator_to_array()` and `foreach` over an interpreter-declared `IteratorAggregate` reach its
/// `getIterator()`. The bridge answers "definitely not" (2) when asked whether such an object is an
/// `Iterator`, and the compiled probe read any non-zero answer as a yes: it called `rewind()` on
/// the aggregate, and Laravel died on dotenv's `PhpOption\Some` at boot.
#[test]
fn test_iterator_to_array_over_an_interpreted_aggregate() {
    let out = compile_and_run(
        r#"<?php
function grab($o) { return iterator_to_array($o); }
function loop($o) { $r = []; foreach ($o as $k => $v) { $r[] = "$k=$v"; } return implode(",", $r); }
eval('final class Opt implements IteratorAggregate {
    public function __construct(private $v) {}
    public function getIterator(): Traversable { return new ArrayIterator([$this->v]); }
}
final class Gen implements IteratorAggregate {
    public function getIterator(): Traversable { yield "a" => 1; yield "b" => 2; }
}');
$o = eval('return new Opt("x");');
$g = eval('return new Gen();');
echo json_encode(grab($o)), json_encode(grab($g)), "\n";
echo loop($o), "|", loop($g), "\n";
"#,
    );
    assert_eq!(out, "[\"x\"]{\"a\":1,\"b\":2}\n0=x|a=1,b=2\n");
}

/// An interpreter-declared subclass of a compiled abstract `IteratorAggregate` reaches its own
/// `getIterator()` from compiled `iterator_to_array()` and `foreach`. Its object carries the
/// compiled ancestor's header, so the interface scan matched and then called the ancestor's empty
/// abstract slot; phpoption's `Some` under a compiled `Option` killed Laravel's worker that way.
#[test]
fn test_interface_dispatch_on_interpreted_subclass_of_compiled_abstract_aggregate() {
    let out = compile_and_run(
        r#"<?php
abstract class Opt implements IteratorAggregate {
    public function all(): array { return iterator_to_array($this); }
}
function take(Opt $o): array { return iterator_to_array($o); }
eval('final class Just extends Opt {
    public function __construct(private $v) {}
    public function getIterator(): ArrayIterator { return new ArrayIterator([$this->v]); }
}');
$name = "Just";
$o = new $name(42);
echo json_encode(take($o)), json_encode($o->all());
foreach ($o as $k => $v) { echo " $k=$v"; }
"#,
    );
    assert_eq!(out, "[42][42] 0=42");
}

/// A boxed receiver's arguments reach an interpreted object as written, and a compiled one's
/// `func_num_args()` sees the real count. The site was lowered against the only compiled
/// `bootstrap()`, a parameterless one whose hidden collector swallowed `$this`, so Laravel's
/// eval-loaded bootstrappers received `bootstrap(array)`; and a boxed call supplying every
/// optional parameter handed the compiled body a collector without its count prefix.
#[test]
fn test_boxed_receiver_passes_arguments_as_written_to_interpreted_and_compiled_bodies() {
    let out = compile_and_run(
        r#"<?php
interface App { public function name(): string; }
class Application implements App {
    public function name(): string { return "laravel"; }
    public function make($abstract) { return new $abstract(); }
    public function bootstrapWith(array $bootstrappers): void {
        foreach ($bootstrappers as $bootstrapper) { $this->make($bootstrapper)->bootstrap($this); }
    }
}
class Kernel {
    public function bootstrap() { echo "kernel:", func_num_args(), " "; }
    public function pair($a, $b = "B") { return "kernel:$a$b"; }
}
class Other {
    public function pair($a, $b = "B") { return "other:$a$b:" . func_num_args() . json_encode(func_get_args()); }
}
eval('class LoadEnv { public function bootstrap(App $app) { echo "env:", $app->name(), " "; } }');
function pick(int $n) { return $n === 1 ? new Kernel() : new Other(); }
(new Kernel())->bootstrap();
(new Application())->bootstrapWith(['LoadEnv']);
$o = pick($argc + 1);
echo $o->pair("a", "b"), " ", $o->pair("a"), " ", count(debug_backtrace()), "\n";
"#,
    );
    assert_eq!(out, "kernel:0 env:laravel other:ab:2[\"a\",\"b\"] other:aB:1[\"a\"] 0\n");
}

/// An interpreted class constant evaluated on its first read stays readable. The lazily evaluated
/// cell went into the constant cache AND to the reader as an owned value; the reader's release
/// freed the cached cell and the next read returned whatever reused it. Laravel's `DateFactory`
/// read `static::DEFAULT_CLASS_NAME` back as `"parse"` and Carbon's boot died.
#[test]
fn test_lazily_evaluated_interpreted_class_constant_survives_its_first_read() {
    let out = compile_and_run_files(
        &[
            (
                "DateFactory.php",
                r#"<?php
namespace Support;
class DateFactory {
    const DEFAULT_CLASS_NAME = Carbon::class;
    protected static $dateClass;
    public function __call($method, $parameters) {
        $defaultClassName = static::DEFAULT_CLASS_NAME;
        $dateClass = static::$dateClass ?: $defaultClassName;
        return $dateClass . '::' . $method . '(' . implode(',', $parameters) . ')';
    }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
$file = __DIR__ . '/DateFactory' . ($argc > 5 ? 'x' : '') . '.php';
require $file;
function make(string $class) { return new $class(); }
$f = make('Support\DateFactory');
echo $f->now(), "|", $f->parse('2020'), "|", $f->create(), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "Support\\Carbon::now()|Support\\Carbon::parse(2020)|Support\\Carbon::create()\n"
    );
}

/// Named constructor arguments reach a class only the runtime autoloader supplies in php's
/// parameter order. With no compiled constructor signature to place them, they were passed as
/// written: Monolog's `new LogRecord(datetime: ..., channel: ..., message: ..., level: ...)`
/// handed its message to `Level $level` and Laravel's logger threw a TypeError.
#[test]
fn test_named_arguments_construct_a_runtime_loaded_class_in_parameter_order() {
    let out = compile_and_run_files(
        &[
            (
                "Lib/Rec.php",
                r#"<?php
namespace Lib;
class Rec {
    public function __construct(public string $a, public int $b, public string $c = "C", public array $d = []) {}
    public function show() { return $this->a . "|" . $this->b . "|" . $this->c . "|" . json_encode($this->d); }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function (string $class): void {
    $file = __DIR__ . '/' . str_replace('\\', '/', $class) . '.php';
    if (is_file($file)) { require $file; }
});
use Lib\Rec;
echo (new Rec(b: 2, a: 'A'))->show(), " ", (new Rec('A', d: [1], b: 3))->show(), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "A|2|C|[] A|3|C|[1]\n");
}

/// `new self(...$args)` with a keyed array compiles once `debug_backtrace()` arms the hidden
/// collector, and the constructor still sees php's argument count. The keyed-spread lowering
/// refused any variadic, the collector included, and Monolog's `LogRecord::with()` failed with
/// "constructor call ... with 1 args for 8 params", leaving the class to the interpreter.
#[test]
fn test_keyed_spread_construction_binds_through_the_hidden_collector() {
    let out = compile_and_run(
        r#"<?php
class P {
    public array $seen;
    public function __construct(public string $a, public int $b = 2, public string $c = 'C') {
        $this->seen = [func_num_args(), func_get_args()];
    }
    public function with(mixed ...$args): self {
        foreach (['a', 'b', 'c'] as $prop) { $args[$prop] ??= $this->{$prop}; }
        return new self(...$args);
    }
}
function show(P $p): string { return $p->a . $p->b . $p->c . ':' . json_encode($p->seen); }
$one = ['a' => 'x'];
$gap = ['a' => 'z', 'c' => 'Q'];
echo show(new P(...$one)), " ", show(new P(...$gap)), " ", show((new P('k'))->with(b: 9)), " ";
echo count(debug_backtrace()), "\n";
"#,
    );
    assert_eq!(
        out,
        "x2C:[1,[\"x\"]] z2Q:[3,[\"z\",2,\"Q\"]] k9C:[3,[\"k\",9,\"C\"]] 0\n"
    );
}

/// `chmod()` accepts a mode that is only possibly an int and converts it at run time. A property
/// read is not narrowed by its null check, so Monolog's `@chmod($url, $this->filePermission)`
/// reached the checker as `?int` and was refused, leaving `StreamHandler` to the interpreter.
#[test]
fn test_chmod_accepts_a_nullable_int_mode_behind_a_null_check() {
    let out = compile_and_run(
        r#"<?php
class H {
    protected int|null $filePermission;
    public function __construct(?int $perm) { $this->filePermission = $perm; }
    public function apply(string $url): string {
        $ok = 'skipped';
        if ($this->filePermission !== null) { $ok = @chmod($url, $this->filePermission) ? 'ok' : 'failed'; }
        clearstatcache();
        return $ok . ':' . decoct(fileperms($url) & 0777);
    }
}
$file = sys_get_temp_dir() . '/elephc_chmod_nullable_' . getmypid();
touch($file);
echo (new H(0600))->apply($file), " ", (new H(null))->apply($file), " ", (new H(0644))->apply($file), "\n";
unlink($file);
"#,
    );
    assert_eq!(out, "ok:600 skipped:600 ok:644\n");
}

/// A boxed receiver in a program that loads code at run time is not planned against the
/// signature the COMPILED classes agree on. Laravel's `Route` is interpreted, so the only compiled
/// `withTrashed` was `PendingResourceRegistration::withTrashed(array $methods = [])`: the route
/// cache's `->withTrashed($attributes['withTrashed'] ?? false)` raised "must be of type array,
/// false given", and `->withTrashed()` received that class's `[]` default instead of `true`.
#[test]
fn test_boxed_receiver_is_not_planned_against_compiled_only_signatures() {
    let out = compile_and_run(
        r#"<?php
class PendingResourceRegistration {
    public array $trashed = [];
    public function withTrashed(array $methods = []) { $this->trashed = $methods; return $this; }
}
eval('class Route {
    public $trashed = null;
    public function withTrashed($withTrashed = true) { $this->trashed = $withTrashed; return $this; }
}');
function make(string $class) { return new $class(); }
function run(): void {
    $route = make('Route');
    $route->withTrashed(false);
    echo var_export($route->trashed, true), " ";
    $route->withTrashed();
    echo var_export($route->trashed, true), " ";
    $pending = make('PendingResourceRegistration');
    $pending->withTrashed(['show']);
    echo json_encode($pending->trashed), " ";
    $pending->withTrashed();
    echo json_encode($pending->trashed), "\n";
}
run();
"#,
    );
    assert_eq!(out, "false true [\"show\"] []\n");
}

/// A typed property accepts an object of the declared class built by the interpreter. Symfony's
/// `Route::compile()` stores `$class::compile($this)` into `?CompiledRoute $compiled`; with
/// `RouteCompiler` and `CompiledRoute` loaded only at run time the object carries the bridge's
/// generic header, the class-id check refused it, and Laravel answered "Cannot assign stdClass to
/// property ... of type ?CompiledRoute" on every route.
#[test]
fn test_typed_property_accepts_an_interpreter_built_object_of_its_class() {
    let out = compile_and_run_files(
        &[
            (
                "R/CompiledRoute.php",
                r#"<?php
namespace R;
class CompiledRoute {
    public function __construct(private string $regex) {}
    public function getRegex(): string { return $this->regex; }
}
"#,
            ),
            (
                "R/RouteCompiler.php",
                r##"<?php
namespace R;
class RouteCompiler {
    public static function compile(Route $route): CompiledRoute { return new CompiledRoute("#^" . $route->getPath() . "$#"); }
}
"##,
            ),
            (
                "main.php",
                r#"<?php
namespace R;
spl_autoload_register(function (string $class): void {
    $file = __DIR__ . '/' . str_replace('\\', '/', $class) . '.php';
    if (is_file($file)) { require $file; }
});
class Route {
    private ?CompiledRoute $compiled = null;
    private array $options = ['compiler_class' => 'R\\RouteCompiler'];
    public function __construct(private string $path) {}
    public function getPath(): string { return $this->path; }
    public function compile(): CompiledRoute {
        if (null !== $this->compiled) { return $this->compiled; }
        $class = $this->options['compiler_class'];
        return $this->compiled = $class::compile($this);
    }
}
$r = new Route('/posts');
echo $r->compile()->getRegex(), " ", $r->compile()->getRegex(), "\n";
eval('$unused = 1;');
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "#^/posts$# #^/posts$#\n");
}

/// A boxed closure is truthy, in compiled code and in the interpreter alike. The runtime's Mixed
/// truthiness had no arm for the callable-descriptor tag and answered false, so Laravel's
/// `$this->routeResolver ?: function () {}` replaced the router's resolver with the empty fallback:
/// `$request->route()` returned null and every route failed on `parameters()`.
#[test]
fn test_boxed_closure_is_truthy() {
    let out = compile_and_run(
        r#"<?php
class Route { public function __construct(public string $uri) {} }
eval('class Request {
    protected $routeResolver;
    public function setRouteResolver(Closure $callback) { $this->routeResolver = $callback; return $this; }
    public function getRouteResolver() { return $this->routeResolver ?: function () {}; }
    public function route() { return call_user_func($this->getRouteResolver()); }
}');
function make(string $class) { return new $class(); }
function run(Route $route): string {
    $request = make('Request');
    $request->setRouteResolver(fn () => $route);
    $found = $request->route();
    return $found === null ? 'null' : $found->uri;
}
$boxed = $argc > 0 ? fn () => 1 : null;
echo run(new Route('/posts')), " ", var_export((bool) $boxed, true), " ", ($boxed ?: 'none') === $boxed ? 'kept' : 'fallback', "\n";
"#,
    );
    assert_eq!(out, "/posts true kept\n");
}

/// A boxed object's `__toString()` called through the string ladder retires the temporaries the
/// call staged. Once `debug_backtrace()` arms the hidden collector, the call links a cleanup
/// record for its empty collector array; the ladder left it linked on stack the next call reused,
/// and the next unwind ran garbage: Laravel crashed in `ResponseHeaderBag::all()` on
/// `(string) $cookie` for every web route.
#[test]
fn test_boxed_tostring_call_retires_its_collector_record() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Cookie {
    public function __construct(private string $name) {}
    public function __toString(): string { return $this->name . '=x'; }
}
function viaMixed($c): string { return (string) $c; }
function collect(array $cookies): array {
    $headers = [];
    foreach ($cookies as $cookie) { $headers['set-cookie'][] = (string) $cookie; }
    return $headers;
}
for ($i = 0; $i < 3; $i++) {
    echo viaMixed(new Cookie('a')), " ", json_encode(collect([new Cookie('b'), new Cookie('c')])), "\n";
}
echo count(debug_backtrace()), "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a=x {\"set-cookie\":[\"b=x\",\"c=x\"]}\n".repeat(3) + "0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A gradual array spread into a typed variadic keeps its own slot layout through the tail slice.
/// A declared `: array` return types as `array<mixed>` after the merge, and slicing it straight
/// into `Item ...$items` asked the backend to narrow boxed slots in place ("array_slice result
/// element PHP type Object for source element PHP type Mixed"); Doctrine's inflector rules stopped
/// compiling and Laravel's `Str::plural()` failed.
#[test]
fn test_gradual_array_spread_into_a_typed_variadic_compiles() {
    let out = compile_and_run(
        r#"<?php
final class Item {
    public function __construct(private string $name) {}
    public function name(): string { return $this->name; }
}
final class Items {
    private $items;
    public function __construct(Item ...$items) { $this->items = $items; }
    public function names(): string {
        $out = [];
        foreach ($this->items as $item) { $out[] = $item->name(); }
        return implode(',', $out);
    }
}
final class Source {
    public static function list(): array { return [new Item('x'), new Item('y')]; }
}
echo (new Items(...Source::list()))->names(), " ", (new Items(new Item('p')))->names(), "|", (new Items())->names(), "\n";
"#,
    );
    assert_eq!(out, "x,y p|\n");
}

/// A declared override of an untyped method hands its value back in the storage the base's own
/// callers read. Monolog's `LineFormatter::normalizeException(): string` overrides an untyped
/// `NormalizerFormatter::normalizeException()` inferred as an array; `$this->normalizeException()`
/// inside the base stored the string as an array, and formatting a logged exception crashed.
#[test]
fn test_declared_override_of_an_untyped_method_returns_in_the_base_storage() {
    let out = compile_and_run(
        r#"<?php
class NormBase {
    public function normalize($data) {
        if ($data instanceof Throwable) { return $this->normalizeException($data); }
        return $data;
    }
    protected function normalizeException(Throwable $e) {
        return ['class' => get_class($e), 'message' => $e->getMessage()];
    }
}
class Line extends NormBase {
    protected function normalizeException(Throwable $e): string {
        return '[object] (' . get_class($e) . ': ' . $e->getMessage() . ')';
    }
}
function show($v): string { return is_array($v) ? 'array:' . json_encode($v) : 'string:' . $v; }
$e = new RuntimeException('boom');
echo show((new NormBase())->normalize($e)), "|", show((new Line())->normalize($e)), "|";
foreach (['exception' => (new Line())->normalize($e)] as $k => $v) { echo $k, '=', $v, "\n"; }
"#,
    );
    assert_eq!(
        out,
        "array:{\"class\":\"RuntimeException\",\"message\":\"boom\"}|string:[object] (RuntimeException: boom)|exception=[object] (RuntimeException: boom)\n"
    );
}

/// Verifies a closure created by one eval context can throw, and be caught, under another.
///
/// The closure runs on its owner's context. Two things stayed on the caller's: the runtime hooks
/// (so building the exception could not write the protected `Exception::$file`, and the `throw`
/// died as "unsupported Throw statement"), and the pending-throw slot the caller's `try` reads (so
/// once the exception was built, nothing caught it). Laravel's `$request->validate()` macro is
/// this shape: a failed validation killed the worker instead of answering 422.
#[test]
fn test_foreign_context_closure_throw_reaches_the_caller_catch() {
    let out = compile_and_run_files(
        &[
            (
                "body.php",
                r#"<?php
final class Failure extends RuntimeException {
    public function __construct(string $message, public array $errors = []) {
        parent::__construct($message);
    }
}
class Box {
    public static array $fns = [];
    public function viaStatic(): string { $f = static::$fns['x']; return $f(); }
    public function viaLocal(): string {
        $f = function () { throw new Failure('local'); };
        return $f();
    }
}
Box::$fns['x'] = function () { throw new Failure('static', ['title' => ['required']]); };
class Runner {
    public function run(string $which): string {
        try {
            return (new Box())->{$which}();
        } catch (Failure $e) {
            return 'caught ' . get_class($e) . ' ' . $e->getMessage() . ' '
                . json_encode($e->errors) . ' ' . basename($e->getFile());
        }
    }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
function load_body(string $path): void { include $path; }
load_body(__DIR__ . '/body' . ($argc > 5 ? 'x' : '') . '.php');
echo (new Runner())->run($argc > 7 ? 'x' : 'viaLocal'), "\n";
echo (new Runner())->run($argc > 7 ? 'x' : 'viaStatic'), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "caught Failure local [] body.php\ncaught Failure static {\"title\":[\"required\"]} body.php\n"
    );
}

/// Verifies interpreted code unpacks a generator into user-function and constructor arguments.
///
/// The merge kept main's user-call argument evaluator, which only understood arrays; ours turned a
/// Traversable into the array its iteration yields first. Doctrine's inflector builds its rules
/// with `new Transformations(...Inflectible::getPlural())`, so `Str::plural()` met an empty
/// variadic and Laravel's `/strings` page died on "foreach() argument must be of type
/// array|object, null given".
#[test]
fn test_interpreted_generator_spread_reaches_variadic_parameters() {
    let out = compile_and_run_files(
        &[
            (
                "body.php",
                r#"<?php
final class Item { public function __construct(public string $name) {} }
final class Bag {
    private array $names = [];
    public function __construct(Item ...$items) {
        foreach ($items as $item) { $this->names[] = $item->name; }
    }
    public function names(): string { return implode(',', $this->names); }
}
final class Source {
    public static function get(): iterable {
        yield new Item('a');
        yield new Item('b');
        yield from self::more();
    }
    private static function more(): iterable { yield new Item('c'); }
}
function untyped(...$items): string {
    $out = [];
    foreach ($items as $item) { $out[] = $item->name; }
    return implode(',', $out);
}
function keyed(): iterable { yield 3 => 1; yield 5 => 2; }
function ints(int $x = 0, int ...$rest): string { return $x . '|' . json_encode($rest); }
echo (new Bag(...Source::get()))->names(), "\n";
echo untyped(...Source::get()), "\n";
echo ints(...keyed()), "\n";
"#,
            ),
            (
                "main.php",
                r#"<?php
function load_body(string $path): void { include $path; }
load_body(__DIR__ . '/body' . ($argc > 5 ? 'x' : '') . '.php');
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "a,b,c\na,b,c\n1|[2]\n");
}

/// A Closure `eval()` returned reaches a compiled `Closure` parameter as a callable descriptor.
///
/// The `mixed` argument is narrowed to the parameter's descriptor storage by `mixed_unbox`, which
/// took the box's payload as the descriptor: for an interpreter-made Closure that payload is an
/// object, and calling through it jumped into its bytes (SIGBUS here; in Laravel,
/// `Application::detectEnvironment()` fed `LoadConfiguration`'s arrow function silently returned
/// 0, so `$app->environment()` was "0"). The unbox now builds a bridge descriptor for anything
/// that is not already one, as the local-load conversion does.
#[test]
fn test_eval_closure_reaches_a_compiled_closure_parameter_through_mixed() {
    let out = compile_and_run(
        r#"<?php
final class Detector
{
    public function detect(Closure $callback, $consoleArgs = null)
    {
        if ($consoleArgs) {
            return $this->detectConsoleEnvironment($callback, $consoleArgs);
        }
        return $this->detectWebEnvironment($callback);
    }

    protected function detectWebEnvironment(Closure $callback)
    {
        return $callback();
    }

    protected function detectConsoleEnvironment(Closure $callback, array $args)
    {
        if (! is_null($value = $this->getEnvironmentArgument($args))) {
            return $value;
        }
        return $this->detectWebEnvironment($callback);
    }

    protected function getEnvironmentArgument(array $args)
    {
        foreach ($args as $i => $value) {
            if ($value === '--env') {
                return $args[$i + 1] ?? null;
            }
            if (str_starts_with($value, '--env=')) {
                return array_slice(explode('=', $value), 1)[0] ?? null;
            }
        }
    }
}

var_dump((new Detector())->detect(fn () => 'from-compiled', null));
var_dump(eval('return (new Detector())->detect(fn () => "from-eval", null);'));
var_dump((new Detector())->detect(eval('return fn () => "eval-closure";'), null));
var_dump((new Detector())->detect(fn () => 'console', ['x', '--env=staging']));
"#,
    );
    assert_eq!(
        out,
        "string(13) \"from-compiled\"\nstring(9) \"from-eval\"\nstring(12) \"eval-closure\"\nstring(7) \"staging\"\n"
    );
}

/// Verifies builtin container mutators work on an element of a local hash.
///
/// Our side binds such an element to its managed reference cell, so that a user function given
/// the same element twice sees one storage. The merged builtin lowering expects the array value
/// and a writable receiver place instead, and the cell made the backend refuse "sort for PHP type
/// Pointer" (Symfony's `Console\Helper\Table` hit the same refusal with `array_pop`). Builtins now
/// take the place rewrite; user functions keep the shared cell.
#[test]
fn test_builtin_mutators_on_a_local_hash_element() {
    let out = compile_and_run(
        r#"<?php
$m = ['k' => [3, 1, 2], 'r' => ['b' => 2, 'a' => 1]];
$copy = $m['k'];
sort($m['k']);
rsort($copy);
ksort($m['r']);
echo json_encode($m), ' ', json_encode($copy), "\n";
$rows = ['row' => [1, 2, 3]];
$last = array_pop($rows['row']);
array_push($rows['row'], 9, 10);
array_unshift($rows['row'], 0);
$first = array_shift($rows['row']);
echo $last, ' ', $first, ' ', json_encode($rows), "\n";
function alias(&$a, &$b) { $a[] = 'x'; $b[] = 'y'; }
$h = ['k' => []];
alias($h['k'], $h['k']);
echo json_encode($h), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"k\":[1,2,3],\"r\":{\"a\":1,\"b\":2}} [3,2,1]\n3 0 {\"row\":[1,2,9,10]}\n{\"k\":[\"x\",\"y\"]}\n"
    );
}

/// A closure argument to a method a subclass inherits must reach the body in the representation
/// the body was specialized with. The call specialized the declaring class's signature to
/// `callable`, but a call on the subclass receiver planned against the subclass's own copy, which
/// had been published as `mixed`: the closure arrived boxed and the body read the box as a
/// descriptor. Laravel's `Application extends Container` lost `$app['env'] = ...` (environment
/// `0`, log channel "0") and crashed on the next `make()`.
#[test]
fn test_inherited_method_receives_a_closure_as_its_body_expects() {
    let out = compile_and_run(
        r#"<?php
class Container {
    protected array $bindings = [];
    public function bind($abstract, $concrete = null, $shared = false) {
        $this->bindings[$abstract] = compact('concrete', 'shared');
    }
    public function make($abstract) {
        $concrete = $this->bindings[$abstract]['concrete'];
        return $concrete($this, []);
    }
    public function put($key, $value): void {
        $this->bind($key, $value instanceof Closure ? $value : fn () => $value);
    }
    public static function wrap($value) { return fn () => $value; }
}
class Application extends Container {}
$app = new Application();
$app->bind('direct', fn () => 'direct');
$app->put('env', 'production');
$app->put('n', 42);
echo $app->make('direct'), ' ', $app->make('env'), ' ', $app->make('n'), "\n";
$w = Application::wrap('static');
echo $w(), "\n";
"#,
    );
    assert_eq!(out, "direct production 42\nstatic\n");
}

/// A ternary arm reading an element of a container the lowering cannot type (a call's result)
/// must merge as boxed `mixed`. The syntactic fallback answered `int`, so Laravel's `head()`
/// (`empty($a) ? false : array_values($a)[0]`) returned `int(0)` for a string element and for
/// `false`, and `--env=staging` detected environment 0.
#[test]
fn test_ternary_over_an_element_of_a_call_result_keeps_the_element() {
    let out = compile_and_run(
        r#"<?php
function head($array) { return empty($array) ? false : array_values($array)[0]; }
function env_arg(array $args) {
    foreach ($args as $i => $value) {
        if (str_starts_with($value, '--env=')) {
            return head(array_slice(explode('=', $value), 1));
        }
    }
}
var_dump(head(['staging']), head([]), head([7]), head([1 => 'x']));
var_dump(env_arg(['artisan', '--env=staging']), env_arg(['artisan']));
"#,
    );
    assert_eq!(
        out,
        "string(7) \"staging\"\nbool(false)\nint(7)\nstring(1) \"x\"\nstring(7) \"staging\"\nNULL\n"
    );
}

/// A two-level write into a declared `array` property must not copy the property per write. The
/// nested-parent path read and acquired the property, then declined the (now boxed) slot and let
/// the fallback write while that reference stayed live: every write copied the whole property and
/// leaked the old copy. 300 writes grew the heap by ~5.8 MB (Symfony's
/// `HtmlSanitizerConfig::allowSafeElements()` exhausted 256 MB); php needs ~17 KiB.
#[test]
fn test_nested_property_array_writes_do_not_copy_the_property() {
    let out = compile_and_run(
        r#"<?php
final class Config {
    private array $allowed = [];
    public function allow(string $element, array $attributes): static {
        $clone = clone $this;
        $clone->allowed[$element] = [];
        foreach ($attributes as $attr) {
            $clone->allowed[$element][$attr] = true;
        }
        return $clone;
    }
    public function total(): int {
        $n = 0;
        foreach ($this->allowed as $attrs) { $n += count($attrs); }
        return $n;
    }
}
$attributes = [];
for ($i = 0; $i < 300; $i++) { $attributes[] = 'a' . $i; }
$c = new Config();
$before = memory_get_usage();
for ($i = 0; $i < 20; $i++) { $c = $c->allow('el' . $i, $attributes); }
echo $c->total(), "\n";
echo memory_get_usage() - $before < 2 * 1024 * 1024 ? "bounded" : "grew", "\n";
"#,
    );
    assert_eq!(out, "6000\nbounded\n");
}

/// A method's callable parameter must not inherit the signature another body recorded for a
/// same-named closure. Callable records are keyed by variable name; functions isolate their
/// parameters, methods did not, so `return $callback();` in Laravel's
/// `EnvironmentDetector::detectWebEnvironment(Closure $callback)` was typed as some other body's
/// int closure and the interpreted caller's string came back as `int(0)` -- every Laravel request
/// ran in environment 0.
#[test]
fn test_method_callable_parameter_does_not_inherit_another_bodys_closure_signature() {
    let out = compile_and_run(
        r#"<?php
function counter(): int
{
    $callback = fn () => 41;
    return $callback() + 1;
}
class Detector
{
    public function detect(Closure $callback) { return $this->detectWeb($callback); }
    protected function detectWeb(Closure $callback) { return $callback(); }
}
echo counter(), "\n";
var_dump(eval('return (new Detector())->detect(fn () => "production");'));
echo json_encode(eval('return (new Detector())->detect(fn () => [1, 2]);')), "\n";
"#,
    );
    assert_eq!(out, "42\nstring(10) \"production\"\n[1,2]\n");
}

/// Spreading a generator unpacks what it yields, into an array literal and into a variadic call
/// or constructor. Doctrine's inflector builds its rulesets as
/// `new Patterns(...Uninflected::getSingular())`; the spread read the generator object as an array
/// and the lowering was refused, which sent Laravel back to a second compile.
#[test]
fn test_spreading_a_generator_into_variadics_and_literals() {
    let out = compile_and_run(
        r#"<?php
final class P { public function __construct(public string $p) {} }
final class Ps {
    private array $ps;
    public function __construct(P ...$ps) { $this->ps = array_map(fn (P $p) => $p->p, $ps); }
    public function all(): array { return $this->ps; }
}
final class U { public static function get(): iterable { yield new P("a"); yield new P("b"); } }
function f(P ...$ps): int { return count($ps); }
$a = [...U::get()];
echo json_encode((new Ps(...U::get()))->all()), f(...U::get()), count($a), $a[1]->p, "\n";
"#,
    );
    assert_eq!(out, "[\"a\",\"b\"]22b\n");
}

/// `array_pad()` over a list the native helper cannot copy (strings, a hash) or with a pad value
/// of another type goes through the built prelude. Laravel's `Kernel::parseMiddleware()` is
/// `[$name, $parameters] = array_pad(explode(':', $middleware, 2), 2, [])`, which was refused.
#[test]
fn test_array_pad_over_strings_hashes_and_foreign_pad_values() {
    let out = compile_and_run(
        r#"<?php
function mw(string $m): string {
    [$name, $parameters] = array_pad(explode(":", $m, 2), 2, []);
    return $name . "|" . json_encode($parameters);
}
echo mw("auth:web"), " ", mw("guest"), "\n";
echo json_encode(array_pad(["a" => 1, 5 => 2], 4, "x")), json_encode(array_pad(["b"], -3, null)),
    json_encode(array_pad([1, 2], 1, 0)), json_encode(array_pad([1], 3, 0)), "\n";
"#,
    );
    assert_eq!(
        out,
        "auth|\"web\" guest|[]\n{\"a\":1,\"0\":2,\"1\":\"x\",\"2\":\"x\"}[null,null,\"b\"][1,2][1,0,0]\n"
    );
}

/// `self::instanceMethod()` from a static method throws when it RUNS, as in php; the class still
/// compiles. Carbon's shared `Mixin` trait calls `self::copyStep()` on a branch only
/// CarbonInterval takes, and the compile-time refusal sent all of Carbon to the interpreter.
#[test]
fn test_self_instance_call_from_a_static_method_throws_at_run_time() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static function run(bool $go) { return $go ? self::value() : "skipped"; }
    public function value() { return 1; }
}
echo Box::run(false), "\n";
try { Box::run(true); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "skipped\nError: Non-static method Box::value() cannot be called statically\n");
}

/// The `0` flag on `%s` pads with zeros in interpreted code too. Carbon's `create()` builds its
/// date with `sprintf('%s:%02s:%02s', ...)`; the interpreter padded with spaces, so
/// `createFromFormat()` rejected "12: 0:00" and every `Carbon::create()` threw.
#[test]
fn test_interpreted_sprintf_zero_pads_strings() {
    let out = compile_and_run(
        r#"<?php
eval('echo sprintf("%-05s|%05s|%02s|%02s|%5s", "ab", "ab", 0, "7", "x"), "\n";');
"#,
    );
    assert_eq!(out, "ab000|000ab|00|07|    x\n");
}

/// `DateInterval` has php 8.2's `__serialize()`/`__unserialize()`, compiled and interpreted.
/// Carbon tests `method_exists(parent::class, '__unserialize')` to pick its php 8.2 path; without
/// the method it wrote removed properties (`weekday`, ...) and every interval raised deprecations.
#[test]
fn test_date_interval_serialize_round_trip_and_unserialize_hook() {
    let out = compile_and_run(
        r#"<?php
$i = new DateInterval("P1Y2DT3H");
echo serialize($i), "\n";
$u = unserialize(serialize($i));
echo get_class($u), " ", $u->y, " ", $u->d, " ", $u->h, "\n";
eval('echo var_export(method_exists("DateInterval", "__unserialize"), true), "\n";
$s = serialize($i); $v = unserialize($s); echo $v->d, " ", strlen($s), "\n";
$i->__unserialize(["y" => 3, "m" => 0, "d" => 4, "h" => 0, "i" => 0, "s" => 0, "f" => 0.0,
    "invert" => 0, "days" => 9, "from_string" => false]); echo $i->y, $i->d, $i->h, $i->days, "\n";');
"#,
    );
    assert_eq!(
        out,
        "O:12:\"DateInterval\":10:{s:1:\"y\";i:1;s:1:\"m\";i:0;s:1:\"d\";i:2;s:1:\"h\";i:3;s:1:\"i\";i:0;s:1:\"s\";i:0;s:1:\"f\";d:0;s:6:\"invert\";i:0;s:4:\"days\";b:0;s:11:\"from_string\";b:0;}\nDateInterval 1 2 3\ntrue\n2 164\n3409\n"
    );
}

/// An interpreted class with a NATIVE parent still finds its interfaces' constants: the parent
/// walk stopped at `DateTime` and returned before the interfaces were consulted, so Carbon's
/// `static::YEARS_PER_MILLENNIUM` (from `UnitValue`, through `CarbonInterface`) was a fatal.
#[test]
fn test_eval_class_with_native_parent_reads_interface_constants() {
    let out = compile_and_run(
        r#"<?php
eval('interface UnitValue { public const YEARS = 1000; }
interface CI extends DateTimeInterface, JsonSerializable, UnitValue {}
class A extends DateTime implements CI {
    public function f() { return [static::YEARS, self::YEARS, static::ATOM]; }
    public function jsonSerialize(): mixed { return 1; }
}
class B extends A {}
echo json_encode((new B())->f()), " ", B::YEARS, "\n";');
"#,
    );
    assert_eq!(out, "[1000,1000,\"Y-m-d\\\\TH:i:sP\"] 1000\n");
}

/// `f()[] = $v` appends through the returned object's `offsetSet()` in interpreted code, as an
/// expression and as a statement; an array result is a temporary. Laravel's `defer()` helper is
/// `fn ($d) => app(DeferredCallbackCollection::class)[] = $d`, which the parser refused, so
/// `Illuminate/Support/functions.php` failed to load.
#[test]
fn test_eval_append_to_a_call_result() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess {
    public array $items = [];
    public function offsetExists($o): bool { return isset($this->items[$o]); }
    public function offsetGet($o): mixed { return $this->items[$o]; }
    public function offsetSet($o, $v): void { $this->items[] = $v; }
    public function offsetUnset($o): void {}
}
$bag = new Bag();
function bag(): Bag { global $bag; return $bag; }
function arr(): array { return []; }
eval('$r = (fn ($v) => bag()[] = $v)("a"); bag()[] = "b"; $x = arr()[] = 3;
echo $r, " ", $x, " ", implode(",", bag()->items), "\n";');
"#,
    );
    assert_eq!(out, "a 3 a,b\n");
}

/// Interpreted comparisons order two date objects by their instant, as php's
/// `date_object_compare` does, whatever their class or time zone. The runtime ranked every pair
/// of objects as equal, so Carbon's `lt()`/`gt()` (`$this < $date`) were false for any two dates.
#[test]
fn test_eval_date_objects_compare_by_instant() {
    let out = compile_and_run(
        r#"<?php
eval('class C extends DateTime { public $extra = 1; }
$c = new C("2020-01-02"); $d = new C("2020-01-01");
echo json_encode([$c > $d, $c < $d, $c == $d, $d <= $c, $c <=> $d]), "\n";
$u = new DateTimeImmutable("2020-01-01 10:00:00", new DateTimeZone("UTC"));
$v = new DateTime("2020-01-01 11:00:00", new DateTimeZone("Europe/Paris"));
echo json_encode([$u == $v, $u <=> $v, $u > new C("2019-12-31")]), "\n";');
"#,
    );
    assert_eq!(out, "[true,false,false,true,1]\n[true,0,true]\n");
}

/// `new static(...$assoc)` builds an interpreted late-static class too: string keys are named
/// arguments, which the constrained dynamic new could not carry, so it was a fatal. Laravel's
/// `Component::resolve()` does this for every anonymous Blade component.
#[test]
fn test_new_static_with_named_spread_builds_an_interpreted_subclass() {
    let out = compile_and_run(
        r#"<?php
abstract class Component {
    public static function resolve(array $data) {
        return new static(...array_intersect_key($data, array_flip(['title', 'size'])));
    }
}
class Card extends Component {
    public function __construct(public string $title = "", public int $size = 1) {}
}
eval('class Badge extends Component { public function __construct(public string $title = "", public int $size = 1) {} }');
$c = Card::resolve(['size' => 3, 'title' => 'x', 'other' => 9]);
$b = Badge::resolve(['size' => 4, 'title' => 'y']);
echo get_class($c), $c->title, $c->size, " ", get_class($b), $b->title, $b->size, "\n";
"#,
    );
    assert_eq!(out, "Cardx3 Badgey4\n");
}

/// `$map[$k] ?? null` is the element type OR null, for scalars and callables alike. The null was
/// dropped: a missing int read as 0, a string as "", and a callable as a truthy null descriptor
/// that crashed when called -- Laravel's `Connection::getResolver()`
/// (`static::$resolvers[$driver] ?? null`) took down every route that touched the database.
#[test]
fn test_null_coalesce_to_null_over_typed_elements_stays_nullable() {
    let out = compile_and_run(
        r#"<?php
class R {
    protected static $ints = [];
    protected static $strs = [];
    protected static $cbs = [];
    public static function set() { static::$ints['a'] = 1; static::$strs['a'] = "s"; static::$cbs['a'] = fn ($c) => "resolved:" . $c; }
    public static function i($k) { return static::$ints[$k] ?? null; }
    public static function s($k) { return static::$strs[$k] ?? null; }
    public static function c($k) { return static::$cbs[$k] ?? null; }
}
R::set();
var_dump(R::i('a'), R::i('b'), R::s('a'), R::s('b'), R::c('b'));
foreach (['a', 'b'] as $k) {
    if ($resolver = R::c($k)) { echo $resolver("pdo"), "\n"; } else { echo "plain:", $k, "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "int(1)\nNULL\nstring(1) \"s\"\nNULL\nNULL\nresolved:pdo\nplain:b\n"
    );
}

/// A free function returning a method's result gets that method's settled return type. Functions
/// are resolved at their first call, before any method body: `typed(Fluent $f)` returning
/// `$f->name` (through `__get`) kept the placeholder `int` and printed a boxed pointer, and
/// `count(byName(...))` over an array-returning getter was refused.
#[test]
fn test_function_return_inferred_after_method_types_settle() {
    let out = compile_and_run(
        r#"<?php
class Fluent {
    protected $attributes = [];
    public function __construct($attributes = []) { $this->attributes = $attributes; }
    public function __get($key) { return $this->attributes[$key] ?? null; }
}
class Blueprint {
    protected $commands = [];
    public function add($n) { $this->commands[] = new Fluent(['name' => $n]); }
    public function getCommands() { return $this->commands; }
}
function typed(Fluent $f) { return $f->name; }
function byName(Blueprint $b, $name) {
    return array_filter($b->getCommands(), function ($value) use ($name) { return $value->name == $name; });
}
$b = new Blueprint();
$b->add('create'); $b->add('index'); $b->add('create');
echo typed(new Fluent(['name' => 'a'])), " ", count(byName($b, 'create')), " ", json_encode(array_keys(byName($b, 'index'))), "\n";
"#,
    );
    assert_eq!(out, "a 2 [1]\n");
}

/// Interpreted code can `bindTo()` a COMPILED closure. Laravel's `Macroable::__call` rebinds every
/// macro a compiled service provider registered (`Request::macro('hasValidRelativeSignature',
/// ...)`), and the call on the callable descriptor was a fatal.
#[test]
fn test_eval_binds_a_compiled_closure_to_a_new_this() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static $macros = [];
    public static function macro($name, $macro) { static::$macros[$name] = $macro; }
    public function path() { return "/p"; }
}
class Provider {
    public function register() {
        Base::macro('sig', function () { return "sig:" . $this->path() . ":" . get_class($this); });
        Base::macro('plain', fn ($x) => $x * 2);
    }
}
(new Provider())->register();
$code = 'class Req extends Base {
    public function __call($method, $parameters) {
        $macro = static::$macros[$method];
        if ($macro instanceof Closure) { $macro = $macro->bindTo($this, static::class) ?? $macro; }
        return $macro(...$parameters);
    }
}
$r = new Req();
echo $r->sig(), " ", $r->plain(4), "\n";';
eval($argc > 0 ? $code : '');
"#,
    );
    assert_eq!(out, "sig:/p:Req 8\n");
}

/// A `mixed` read of a name a class does not declare goes through its `__get` even when the class
/// keeps a dynamic-property table. The table's arm answered alone and warned on a miss: Laravel's
/// `Fluent` read `$command->name` as null with "Undefined property", and every schema operation
/// (and so every route touching a fresh database) failed.
#[test]
fn test_mixed_read_reaches_get_on_a_class_with_a_dynamic_property_table() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Fluent {
    protected $attributes = [];
    public function __construct($attributes = []) { $this->attributes = $attributes; }
    public function __get($key) { return $this->attributes[$key] ?? null; }
    public function __isset($key) { return isset($this->attributes[$key]); }
}
class Other { public $name = "other"; }
function pick(int $n) { return $n > 1 ? new Other() : new Fluent(['name' => 'create']); }
foreach ([pick(0), pick(2)] as $item) { echo $item->name, " ", $item->name ?? "dflt", " "; }
echo "\n";
"#,
    );
    assert_eq!(out, "create create other other \n");
}

/// `$this->{$cond ? 'a' : 'b'}[] = $v` appends to the named property the condition picks.
/// Laravel's `Query\Builder::orderBy()` is written this way, and the refused statement sent the
/// whole query builder to the interpreter.
#[test]
fn test_append_through_a_ternary_property_name() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public $orders = [];
    public $unionOrders = [];
    public $unions = [];
    public function orderBy($column, $direction = 'asc') {
        $this->{$this->unions ? 'unionOrders' : 'orders'}[] = ['column' => $column, 'direction' => $direction];
        $this->{'orders'}[] = 'x';
        return $this;
    }
}
$b = (new Builder())->orderBy('id');
$b->unions = [1];
$b->orderBy('t', 'desc');
echo json_encode([$b->orders, $b->unionOrders]), "\n";
"#,
    );
    assert_eq!(
        out,
        "[[{\"column\":\"id\",\"direction\":\"asc\"},\"x\",\"x\"],[{\"column\":\"t\",\"direction\":\"desc\"}]]\n"
    );
}

/// An untyped parameter that one call site specializes to `callable` still accepts null from
/// another, and a null `callable` boxes back as php null. Laravel's `Arr::sort($array)` forwards
/// its default null callback to `Collection::sortBy()`: the normalizer fataled "mixed value is
/// not callable", and once past it `is_array()` dereferenced the null sentinel as a descriptor.
#[test]
fn test_null_reaches_a_parameter_specialized_to_callable() {
    let out = compile_and_run(
        r#"<?php
class Coll {
    public function __construct(public array $items) {}
    protected function valueRetriever($value) { return is_callable($value) ? $value : fn ($item) => $item; }
    public function sortBy($callback) {
        if (is_array($callback) && ! is_callable($callback)) { return "many"; }
        $callback = $this->valueRetriever($callback);
        $results = [];
        foreach ($this->items as $k => $v) { $results[$k] = $callback($v, $k); }
        asort($results);
        foreach (array_keys($results) as $k) { $results[$k] = $this->items[$k]; }
        return array_values($results);
    }
}
function sortArr($array, $callback = null) { return (new Coll($array))->sortBy($callback); }
function sortNullable($array, ?callable $callback = null) { return (new Coll($array))->sortBy($callback); }
echo json_encode((new Coll([5, 4]))->sortBy(fn ($v) => $v)), json_encode(sortArr([3, 1, 2])), "\n";
echo json_encode(sortNullable([3, 1, 2])), json_encode(sortNullable([3, 1, 2], fn ($v) => -$v)), "\n";
"#,
    );
    assert_eq!(out, "[4,5][1,2,3]\n[1,2,3][3,2,1]\n");
}

/// A dynamic call that omits a parameter whose default is an ENUM CASE materializes the case
/// singleton. Laravel's `Query\Builder::orderBy($column, $direction = SortDirection::Ascending)`
/// is reached through Eloquent's `__call` forwarding (`$query->{$method}(...$parameters)`), and
/// the descriptor invoker aborted: "cannot materialize this default value" killed `/api/posts`.
#[test]
fn test_dynamic_call_materializes_an_enum_case_default() {
    let out = compile_and_run(
        r#"<?php
enum SortDir { case Ascending; case Descending; }
enum Mode: string { case Native = 'native'; case Strict = 'strict'; }
class Query {
    public array $orders = [];
    public function orderBy($column, $direction = SortDir::Ascending, Mode $mode = Mode::Native) {
        $this->orders[] = $column . ':' . ($direction === SortDir::Ascending ? 'asc' : 'desc') . ':' . $mode->value;
        return $this;
    }
}
class Eloquent {
    public function __construct(private $query) {}
    public function __call($method, $parameters) {
        $this->query->{$method}(...$parameters);
        return $this;
    }
}
$q = new Query();
$e = new Eloquent($argc > 5 ? null : $q);
$e->orderBy('id');
$e->orderBy('name', SortDir::Descending);
$e->orderBy('x', SortDir::Ascending, Mode::Strict);
$m = 'orderBy';
$q->{$m}('dyn');
echo json_encode($q->orders), "\n";
"#,
    );
    assert_eq!(
        out,
        "[\"id:asc:native\",\"name:desc:native\",\"x:asc:strict\",\"dyn:asc:native\"]\n"
    );
}

/// A class-like or function declared under a `PHP_VERSION_ID` guard the target decides is a
/// file-scope declaration. Folding turned the taken branch into a block nothing collected
/// declarations from, so symfony/polyfill-php86's `if (\PHP_VERSION_ID < 80600) { enum
/// SortDirection {...} }` vanished: `Class "SortDirection" not found`, and Laravel's
/// `orderBy($column, $direction = SortDirection::Ascending)` default could not be materialized.
#[test]
fn test_target_decided_guard_keeps_its_declarations() {
    let out = compile_and_run(
        r#"<?php
if (\PHP_VERSION_ID >= 80100) {
    enum SortDir { case Ascending; case Descending; }
    class Version { const ID = 'guarded'; }
    function guarded_fn() { return 'fn'; }
}
class Query {
    public function orderBy($column, $direction = SortDir::Ascending) {
        return $column . ':' . ($direction === SortDir::Ascending ? 'asc' : 'desc');
    }
}
$q = new Query();
$m = $argc > 5 ? 'nope' : 'orderBy';
echo $q->{$m}('id'), ' ', $q->orderBy('x', SortDir::Descending), ' ', Version::ID, ' ', guarded_fn(), "\n";
"#,
    );
    assert_eq!(out, "id:asc x:desc guarded fn\n");
}

/// The same guard in a `require_once`d stub file, which is how symfony/polyfill-php86 ships
/// `SortDirection`: the include machinery lifts a file's unconditional declarations before
/// folding, so the enum the guard exposes afterwards stayed inside the include body.
#[test]
fn test_required_polyfill_stub_declares_its_guarded_enum() {
    let out = compile_and_run_files(
        &[
            (
                "SortDir.php",
                r#"<?php
if (\PHP_VERSION_ID < 80600 && \PHP_VERSION_ID >= 80100) {
    enum SortDir { case Ascending; case Descending; }
}
"#,
            ),
            (
                "main.php",
                r#"<?php
require_once __DIR__ . '/SortDir.php';
class Query {
    public function orderBy($c, $d = SortDir::Ascending) {
        return $c . ':' . ($d === SortDir::Ascending ? 'asc' : 'desc');
    }
}
$q = new Query();
$m = $argc > 5 ? 'x' : 'orderBy';
echo $q->{$m}('id'), ' ', $q->orderBy('x', SortDir::Descending), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "id:asc x:desc\n");
}

/// An omitted `array $x = []` parameter stored as a boxed cell still makes its class a candidate
/// of a call on an untyped receiver. Laravel's `ApplicationBuilder::withCommands(array $commands =
/// [])` was dropped from the dispatch of `->withCommands()` in `Application::configure()`, and
/// every artisan command died on "Call to a member function withCommands() on null".
#[test]
fn test_mixed_receiver_call_stages_an_empty_array_default() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    protected array $cmds = [];
    public function __construct(public App $app) {}
    public function withKernels() { return $this; }
    public function withCommands(array $commands = [])
    {
        if (empty($commands)) {
            $commands = ['default'];
        }
        $this->cmds = [...$this->cmds, ...$commands];
        return $this;
    }
    public function count(): int { return count($this->cmds); }
}
class Other {
    public function withCommands(string $path = 'cmd') { return $path; }
}
class App {
    protected static $builder = Builder::class;
    public function __construct(public string $base) {}
    public static function useBuilder(string $c) { static::$builder = $c; }
    public static function configure(?string $base = null)
    {
        $base = match (true) { is_string($base) => $base, default => 'x' };
        return (new static::$builder(new static($base)))
            ->withKernels()
            ->withCommands();
    }
}
App::useBuilder($argc > 5 ? 'Nope' : Builder::class);
$b = App::configure(base: 'p');
echo get_debug_type($b), " ", $b->count(), "\n";
echo App::configure('q')->withCommands(['a', 'b'])->count(), "\n";
echo (new Other())->withCommands(), "\n";
"#,
    );
    assert_eq!(out, "Builder 1\n3\ncmd\n");
}

/// An interpreted subclass may redeclare a compiled parent's `?Closure` property. The checker
/// lowers the `Closure` hint to `callable`, which the eval metadata published as the parent's
/// property type, so php's invariant-type check refused `?Closure` and Laravel Prompts'
/// `TextareaPrompt` "could not be declared" -- every artisan command died on it.
#[test]
fn test_interpreted_subclass_redeclares_a_compiled_closure_property() {
    let out = compile_and_run(
        r#"<?php
abstract class Prompt
{
    public string $state = 'initial';
    public bool|string $required;
    public ?Closure $transform = null;
    public function show(): string { return $this->state . ':' . var_export($this->required, true); }
}
class Plain extends Prompt { public function __construct() { $this->required = false; } }
$code = <<<'PHP'
class TextareaPrompt extends Prompt
{
    public function __construct(
        public string $label,
        public bool|string $required = false,
        public ?Closure $transform = null,
    ) {}
}
PHP;
eval($argc > 0 ? $code : '');
$c = 'TextareaPrompt';
$p = new $c('lbl', required: 'yes', transform: fn ($v) => strtoupper($v));
echo $p->show(), " ", $p->label, " ", ($p->transform)('x'), " ", (new Plain())->show(), "\n";
"#,
    );
    assert_eq!(out, "initial:'yes' lbl X initial:false\n");
}

/// `new $c` resolves a class name written with a leading `\` like php. Laravel's console component
/// factory builds `'\Illuminate\Console\View\Components\' . ucfirst($method)`; the dynamic-new
/// ladder compared it against bare names, matched nothing, and every `$this->components->info()`
/// failed ("Call to a member function render() on null").
#[test]
fn test_dynamic_new_accepts_a_leading_namespace_separator() {
    let out = compile_and_run(
        r#"<?php
interface Out { const VERBOSITY_NORMAL = 32; }
abstract class Component { public function __construct(protected $output) {} }
class Info extends Component {
    public function render($string, $verbosity = Out::VERBOSITY_NORMAL) { return "info:$string:$verbosity"; }
}
class Line extends Component {
    public function render($style, $string, $verbosity = Out::VERBOSITY_NORMAL) { return "line:$style:$string:$verbosity"; }
}
class Factory {
    public function __construct(protected $output) {}
    public function __call($method, $parameters)
    {
        $component = '\\' . ucfirst($method);
        if (! class_exists($component)) { throw new InvalidArgumentException($method); }
        return (new $component($this->output))->render(...$parameters);
    }
}
$f = new Factory('o');
echo $f->info('hello'), "\n";
echo $f->line('s', 'x'), "\n";
echo $f->info('v', 64), "\n";
$c = $argc > 5 ? 'x' : '\Info';
echo get_class(new $c(1)), "\n";
"#,
    );
    assert_eq!(out, "info:hello:32\nline:s:x:32\ninfo:v:64\nInfo\n");
}

/// A spread method call on a boxed receiver reaches an object the interpreter declared, and a
/// runtime `class_exists()` accepts a leading `\`. Laravel's console component factory does
/// `class_exists('\Illuminate\...\' . ucfirst($method))` and then
/// `(new $component($this->output))->render(...$parameters)` on interpreted components: the probe
/// answered false ("Console component [info] not found"), and once it answered, the spread call
/// found no compiled candidate and died as "Call to a member function render() on null".
#[test]
fn test_spread_call_reaches_an_interpreted_component() {
    let out = compile_and_run(
        r#"<?php
namespace Illuminate\Console\View\Components;

class Out { public function write($s) { echo $s; } }

class Factory {
    public function __construct(protected $output) {}
    public function __call($method, $parameters)
    {
        $component = '\Illuminate\Console\View\Components\\' . ucfirst($method);
        if (! class_exists($component)) {
            throw new \InvalidArgumentException($method);
        }
        return (new $component($this->output))->render(...$parameters);
    }
}

$code = <<<'PHP'
namespace Illuminate\Console\View\Components;
abstract class Component {
    public function __construct(protected $output) {}
}
class Info extends Component {
    public function render($string, $verbosity = 32) { $this->output->write("info:$string:$verbosity\n"); }
}
PHP;
eval($argc > 0 ? $code : '');
$f = new Factory(new Out());
$f->info('hello');
$f->info('v', 64);
echo "done\n";
"#,
    );
    assert_eq!(out, "info:hello:32\ninfo:v:64\ndone\n");
}

/// php always defines `ZEND_THREAD_SAFE` and `ZEND_DEBUG_BUILD` (false in a release NTS build).
/// Symfony's `Process` pins its working directory only when `\defined('ZEND_THREAD_SAFE')`; without
/// the constant the cwd stayed null and every process failed "The provided cwd "" does not exist".
#[test]
fn test_zend_build_constants_are_defined() {
    let out = compile_and_run(
        r#"<?php
var_dump(defined('ZEND_THREAD_SAFE'), ZEND_THREAD_SAFE, defined('ZEND_DEBUG_BUILD'), ZEND_DEBUG_BUILD);
$cwd = null;
if (null === $cwd && (\defined('ZEND_THREAD_SAFE') || '\\' === \DIRECTORY_SEPARATOR)) {
    $cwd = getcwd();
}
var_dump(is_string($cwd));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\n");
}

/// Splicing file-scope `Synthetic` blocks (so a folded `if` polyfill's enum is a file-scope
/// declaration) must leave the parser's own `Synthetic` groups intact. A nested append is one:
/// spliced, `C::$items[0][] = 3` at file scope wrote into a detached copy of a `false` element
/// and dropped the write, and a local's nested append failed to compile. The expected output is
/// php 8.5's own (its deprecation goes to stderr here).
#[test]
fn test_file_scope_splice_keeps_parser_append_groups() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public static array $items = [false]; }
if (PHP_VERSION_ID >= 80100) {
    enum Dir: string { case Up = "up"; }
    $rows = [[]];
    $rows[0][] = Dir::Up->value;
}
C::$items[0][] = 3;
$rows[0][] = "b";
$grid = ["k" => false];
@$grid["k"][] = 1;
echo json_encode(C::$items), json_encode($rows), json_encode($grid), "\n";
C::$items = []; unset($rows, $grid);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[[3]][[\"up\",\"b\"]]{\"k\":[1]}\n");
    assert!(
        out.stderr.contains("Automatic conversion of false to array is deprecated"),
        "missing deprecation: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "file-scope nested appends leaked: {}",
        out.stderr
    );
}

/// A user `function main()` shares its name with the EIR entry function, and three name-keyed
/// tables let the entry stand in for it: effect refinement summarized the call with the entry's
/// own (pure) summary and deleted it, tail-call elimination turned the file's trailing `main();`
/// into the entry looping on itself, and the inliner resolved the callee to the entry body. Each
/// left a program that printed nothing. Each fixture below has no other file-scope effect, which
/// is what kept the CLI build (whose entry also seeds `$_SERVER`) from showing it. The expected
/// outputs are php 8.5's own.
#[test]
fn test_user_function_named_main_is_not_the_entry() {
    assert_eq!(
        compile_and_run("<?php\nfunction main(): void { echo \"x\\n\"; }\nmain();\n"),
        "x\n"
    );
    assert_eq!(
        compile_and_run("<?php\nfunction main(): int { return 41; }\necho main() + 1, \"\\n\";\n"),
        "42\n"
    );
    assert_eq!(
        compile_and_run(
            "<?php\nfunction step(int $x): int { return $x * 2; }\n\
             function main(int $n): void { if ($n === 0) { echo \"done\\n\"; return; } \
             echo step($n), \" \"; main($n - 1); }\nmain(3);\n"
        ),
        "6 4 2 done\n"
    );
    // Called from ANOTHER function, the inliner's name table resolved `main()` to the entry
    // body and spliced the file's statements into `f()`.
    assert_eq!(
        compile_and_run(
            "<?php\nfunction main(): int { return 41; }\n\
             function f(): int { return main() + 1; }\necho \"start\\n\";\necho f(), \"\\n\";\n"
        ),
        "start\n42\n"
    );
}

/// The same programs without the EIR optimizer, which never had the defect: the guard that the
/// three passes now agree with the unoptimized build.
#[test]
fn test_user_function_named_main_is_not_the_entry_no_ir_opt() {
    without_ir_opt(test_user_function_named_main_is_not_the_entry);
}

/// Omitting a by-reference VARIADIC passes zero arguments. The omitted-by-reference padding gave
/// the tail one `null` cell, so `int &...$tail` failed its own guard (`Argument #2 must be of type
/// int, null given`) and `count($tail)` was 1. An omitted by-reference REGULAR parameter still
/// gets its default. The expected output is php 8.5's own.
#[test]
fn test_omitted_by_reference_variadic_receives_no_arguments() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class K {
    public function f(int &...$tail): int { return count($tail); }
    public function g(int $a, int &$b = 5, int &...$t): string { return $b . ":" . count($t); }
}
$k = new K(); echo $k->f(), " ", $k->g(1), "\n";
$x = 3; $y = 4; echo $k->f($x, $y), " ", $k->g(1, $x), "\n";
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0 5:0\n2 3:0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `$mixed->has($v)` ladder merges arms whose code is identical, and ONLY those: a class whose
/// parameter is an `int` (a different conversion), one with a defaulted extra parameter, a
/// nullable one, an inherited method and an int conversion still get their own behaviour. The
/// expected output is php 8.5's own.
#[test]
fn test_merged_ladder_arms_keep_each_class_signature() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class A { public function has(string $k): string { return "A:" . $k; } }
class B { public function has(int $k): string { return "B:" . ($k + 1); } }
class C { public function has(string $k, int $d = 5): string { return "C:" . $k . $d; } }
class D { public function has(string $k): string { return "D:" . strtoupper($k); } }
class E extends A {}
class F { public function has(?string $k): string { return "F:" . var_export($k, true); } }
class G { public function other(): string { return "G"; } }
function probe(mixed $o, mixed $v): string { return $o->has($v); }
foreach ([new A(), new B(), new C(), new D(), new E(), new F()] as $o) { echo probe($o, "7"), " "; }
echo "\n";
echo probe(new B(), "41"), "\n";
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "A:7 B:8 C:75 D:7 A:7 F:'7' \nB:42\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
