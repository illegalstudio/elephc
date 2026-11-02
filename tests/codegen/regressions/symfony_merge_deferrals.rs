//! Purpose:
//! Regression tests for the Symfony class files the merged compiler handed to the runtime
//! autoloader, or compiled wrong, that the branch before the merge compiled.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture is the smallest shape of one vendor method, with the vendor file named.
//! - Every expected string is real `php -n` 8.5 output.

use super::*;

/// An `eval()` (or a runtime include) widens the frame's locals to `mixed`, but `$this` cannot be
/// rebound. Widened, `$this->getFile($id)` dispatched over every class declaring `getFile`, met
/// `Throwable::getFile()`, and the backend refused the method
/// (`Symfony\Component\Cache\Adapter\PhpFilesAdapter::doFetch`).
#[test]
fn test_eval_barrier_keeps_this_typed() {
    let out = compile_and_run(
        r#"<?php
class Adapter {
    private array $files = [];
    private function getFile(string $id, bool $mkdir = false): string { return "/d/$id" . ($mkdir ? '!' : ''); }
    public function fetch(array $ids): array {
        $values = [];
        foreach ($ids as $id) {
            $values[$id] = eval('return 1;');
            $values[$id] .= $this->files[$id] ??= $this->getFile($id);
        }
        return $values;
    }
}
try { throw new RuntimeException('x'); } catch (RuntimeException $e) { echo basename($e->getFile()) !== '' ? "ok\n" : "no\n"; }
echo json_encode((new Adapter)->fetch(['a', 'b'])), "\n";
"#,
    );
    assert_eq!(out, "ok\n{\"a\":\"1\\/d\\/a\",\"b\":\"1\\/d\\/b\"}\n");
}

/// `array_flip()` over a `mixed` source whose result the checker typed as an array (the source
/// was proved an array by `is_array()`) takes the gradual hash path instead of being refused
/// (`Symfony\Polyfill\DeepClone\DeepClone::prepare`).
#[test]
fn test_array_flip_of_a_proven_mixed_array() {
    let out = compile_and_run(
        r#"<?php
class Sleeper { public $a = 1; public $b = 2; public function __sleep() { return ['b', 'a']; } }
function flipSleep(object $value): array {
    if (!\is_array($sleep = $value->__sleep())) {
        return [];
    }
    $sleep = array_flip($sleep);
    return $sleep;
}
echo json_encode(flipSleep(new Sleeper())), "\n";
"#,
    );
    assert_eq!(out, "{\"b\":0,\"a\":1}\n");
}

/// A `static` local whose slot is `mixed` already holds any array, so an `if` join over it converts
/// nothing. Narrowed by `null === $setTrace`, it kept an array type over that storage and the join
/// emitted `array_to_mixed` on a `mixed` operand (`DeepClone::getClassReflector`).
#[test]
fn test_static_local_array_join_over_boxed_storage() {
    let out = compile_and_run(
        r#"<?php
class DC {
    public static function reflect($proto): void
    {
        if ($proto instanceof \Throwable) {
            static $setTrace;
            if (null === $setTrace) {
                $setTrace = [new \ArrayObject([1]), new \ArrayObject([2, 3])];
            }
            echo count($setTrace), " ", count($setTrace[1]), "\n";
        }
    }
}
DC::reflect(new \Exception('x'));
DC::reflect(new \Error('y'));
DC::reflect(1);
"#,
    );
    assert_eq!(out, "2 2\n2 2\n");
}

/// The write path of `$a[$k] = <value>` is chosen from the container's type before the value is
/// lowered, and an `eval()` inside the value widens the container. The path chosen first was the
/// indexed writer, handed the boxed container
/// (`Symfony\Component\Routing\Matcher\Dumper\CompiledUrlMatcherDumper::getCompiledRoutes`).
#[test]
fn test_array_write_whose_value_runs_eval() {
    let out = compile_and_run(
        r#"<?php
function compile(bool $forDump): array {
    $compiledRoutes = [false, null, null, null, null];
    $code = 'static function ($c) { return $c * 2; }';
    $compiledRoutes[4] = $forDump ? $code : eval('return ' . $code . ';');
    return $compiledRoutes;
}
$r = compile(true);
echo gettype($r[4]), " ", strlen($r[4]), "\n";
$r = compile(false);
echo gettype($r[4]), " ", ($r[4])(21), "\n";
"#,
    );
    assert_eq!(out, "string 39\nobject 42\n");
}

/// An untyped method parameter defaulting to `null`, refined only by a call passing `false`, keeps
/// the branches its `false !== $x` guard leaves open: seeded from the `null` default as `null`, the
/// other call site's forwarded argument was absorbed and the parameter became plain `false`, so
/// `new DateTimeZone($timezone)` was refused (`Twig\Extension\CoreExtension::convertDate`).
#[test]
fn test_null_default_parameter_is_not_narrowed_by_one_false_caller() {
    let out = compile_and_run(
        r#"<?php
class Ext {
    public function formatDate($date, $format = null, $timezone = null): string
    {
        return $this->convertDate($date, $timezone)->format($format ?? 'e');
    }
    public function modifyDate($date, $modifier)
    {
        return $this->convertDate($date, false)->modify($modifier);
    }
    public function convertDate($date = null, $timezone = null)
    {
        if (false !== $timezone) {
            if (null === $timezone) {
                $timezone = new \DateTimeZone('UTC');
            } elseif (!$timezone instanceof \DateTimeZone) {
                $timezone = new \DateTimeZone($timezone);
            }
        }
        $date = new \DateTime('@0');
        if (false !== $timezone) {
            $date = $date->setTimezone($timezone);
        }
        return $date;
    }
}
echo (new Ext())->modifyDate(null, '+1 day')->format('Y-m-d'), "\n";
"#,
    );
    assert_eq!(out, "1970-01-02\n");
}

/// `preg_match_all()` with flags goes to a prelude helper whose `&$matches` is gradual. A
/// `$matches = [];` written first kept its `array<never>` type through that call, so every later
/// `$match[2][0]` read the rows as nothing and `RouteCompiler::compilePattern()` built
/// `{^(?P<>[^/]++)/greet/\{name\}$}` for `/greet/{name}`.
#[test]
fn test_preg_match_all_flags_into_an_initialized_local() {
    let out = compile_and_run_with_regex(
        r#"<?php
function variables(string $pattern): array {
    $out = [];
    $matches = [];
    preg_match_all('#\{(!)?(\w+)\}#', $pattern, $matches, \PREG_OFFSET_CAPTURE | \PREG_SET_ORDER);
    foreach ($matches as $match) {
        $out[] = [$match[2][0], $match[0][1], $match[1][1] >= 0];
    }
    return $out;
}
echo json_encode(variables('/a/{b}/c/{!d}')), "\n";
"#,
    );
    assert_eq!(out, "[[\"b\",3,false],[\"d\",9,true]]\n");
}

/// A gradual receiver's call whose same-named candidates all return `void` is not a `void` value:
/// the object that answers may be of a class the closed world never saw. Twig's
/// `CoreExtension::include()` returns `$loaded->render(...)`, every one-argument `render()` the
/// demo's world declared was `: void`, and all of `CoreExtension` went to the interpreter.
#[test]
fn test_gradual_receiver_call_with_only_void_candidates_is_a_value() {
    let out = compile_and_run(
        r#"<?php
class Table { public function render(array $rows = []): void { echo count($rows); } }
function inc($loaded, array $vars): string { return $loaded->render($vars); }
eval('class Tpl { public function render(array $c = []): string { return "tpl" . count($c); } }');
$t = "Tpl";
echo inc(new $t, [1, 2]), "\n";
"#,
    );
    assert_eq!(out, "tpl2\n");
}

/// A standalone `null` return type parses to the same storage as `void`; its `return null;` is
/// valid php (`Symfony\Component\ExpressionLanguage\Node\NullCoalescedNameNode::evaluate`).
#[test]
fn test_null_return_type_returns_null() {
    let out = compile_and_run(
        r#"<?php
class N { public function evaluate(array $functions, array $values): null { return null; } }
var_dump((new N)->evaluate([], []));
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// A `static` local filled in a `foreach` from `$x = []`: its storage is a boxed cell, so the loop
/// has no array representation to convert, and the conversion the backend was asked for refused
/// league/uri's `Idna\Option::cases()` (and sent the class back to a second compile).
#[test]
fn test_static_local_filled_in_a_foreach_from_an_empty_array() {
    let out = compile_and_run(
        r#"<?php
final class Option
{
    private const A = 0x1;
    private const B = 0x2;
    private const C = 0x20;
    private static function cases(): array
    {
        static $assoc;
        if (null === $assoc) {
            $assoc = [];
            $fooClass = new ReflectionClass(self::class);
            foreach ($fooClass->getConstants(ReflectionClassConstant::IS_PRIVATE) as $name => $value) {
                $assoc[$name] = $value;
            }
        }
        return $assoc;
    }
    public static function all(): int
    {
        return array_reduce(self::cases(), fn (int $value, int $option) => $value | $option, 0);
    }
}
echo Option::all(), "\n";
"#,
    );
    assert_eq!(out, "35\n");
}

/// The interpreter declares a class that takes an ABSTRACT method from one trait and the concrete
/// method of the same name from another, as php does (symfony/http-client's `CurlResponse`:
/// `CommonResponseTrait::close()` is abstract, `TransportResponseTrait::close()` is not).
#[test]
fn test_eval_class_combines_abstract_and_concrete_trait_methods() {
    let out = compile_and_run(
        r#"<?php
eval('trait CommonT {
    public function getContent(): string { $this->close(); return "content"; }
    abstract protected function close(): void;
}
trait TransportT {
    protected function close(): void { echo "closed "; }
}
trait LateAbstract { abstract protected function close(): void; }
final class Resp { use CommonT; use TransportT; }
final class Resp2 { use TransportT; use LateAbstract; public function go() { $this->close(); } }');
echo (new Resp)->getContent(), "\n";
(new Resp2)->go();
echo "\n";
"#,
    );
    assert_eq!(out, "closed content\nclosed \n");
}

/// The interpreter's `foreach` destructures by reference: `foreach ($a as $i => [$m, &$r])` binds
/// `$r` to the element (symfony/http-client's `TransportResponseTrait::stream()`).
#[test]
fn test_eval_foreach_destructures_by_reference() {
    let out = compile_and_run(
        r#"<?php
$running = [['m1', ['a']], ['m2', []]];
eval('foreach ($running as $i => [$multi, &$responses]) { $responses[] = $multi . $i; } unset($responses);');
echo json_encode($running), "\n";
"#,
    );
    assert_eq!(out, "[[\"m1\",[\"a\",\"m10\"]],[\"m2\",[\"m21\"]]]\n");
}

/// An untyped parameter the body tests with `instanceof` keeps `mixed` instead of the type of its
/// call sites: Carbon's `Creator::make($var)` is only called with strings in Laravel, and its
/// `instanceof DateTimeInterface` branch then refused the whole class.
#[test]
fn test_instanceof_tested_untyped_param_is_not_specialized_to_a_scalar() {
    let out = compile_and_run(
        r#"<?php
class D {
    public static function instance(DateTimeInterface $d): string { return "I:" . $d->format("Y"); }
    public static function make($var): ?string {
        if ($var instanceof DateTimeInterface) {
            return static::instance($var);
        }
        if (is_string($var)) {
            return "S:" . $var;
        }
        return null;
    }
}
echo D::make("2024"), "\n";
"#,
    );
    assert_eq!(out, "S:2024\n");
}

/// A constructor default that is an EXPRESSION over `self::` constants resolves in the declaring
/// class, not the caller's: Symfony's `ReflectionExtractor` defaults `$magicMethodsFlags` to
/// `self::ALLOW_MAGIC_GET | self::ALLOW_MAGIC_SET`, and `new ReflectionExtractor()` in a container
/// class threw `Undefined constant getPropertyAccessorService::ALLOW_MAGIC_GET`.
#[test]
fn test_constructor_default_expression_over_self_constants_resolves_in_declaring_class() {
    let out = compile_and_run(
        r#"<?php
namespace Lib {
    class Extractor {
        public const ALLOW_PUBLIC = 4;
        public const ALLOW_MAGIC_GET = 1 << 0;
        public const ALLOW_MAGIC_SET = 1 << 1;
        public function __construct(
            int $access = self::ALLOW_PUBLIC,
            private int $flags = self::ALLOW_MAGIC_GET | self::ALLOW_MAGIC_SET,
            private array $list = [self::ALLOW_PUBLIC, -self::ALLOW_MAGIC_SET],
        ) {}
        public function show(): string { return $this->flags . ":" . implode(",", $this->list); }
    }
}
namespace App {
    abstract class Base {}
    final class getService extends Base {
        public static function make(): \Lib\Extractor { return new \Lib\Extractor(); }
    }
    echo getService::make()->show(), "\n";
}
"#,
    );
    assert_eq!(out, "3:4,-2\n");
}

/// `ReflectionProperty::setValue()` reaches the Throwable payload's private `previous` through a
/// reflector whose class is only known at run time. Symfony's `ErrorListener` chains exceptions
/// with `new ReflectionProperty($wrapper instanceof \Exception ? \Exception::class : \Error::class,
/// 'previous')`; the write landed in a dynamic property and the request died on
/// `Cannot access private property Error::$previous`.
#[test]
fn test_reflection_property_writes_throwable_previous_through_a_runtime_class_name() {
    let out = compile_and_run(
        r#"<?php
$inner = new LogicException("inner");
$e = new RuntimeException("outer");
$cls = $argc > 5 ? Error::class : Exception::class;
$rp = new ReflectionProperty($cls, 'previous');
$rp->setValue($e, $inner);
echo $e->getPrevious()->getMessage(), " ", get_class($rp->getValue($e)), "\n";
$t = new TypeError("t");
$cls2 = $argc > 5 ? Exception::class : Error::class;
(new ReflectionProperty($cls2, 'previous'))->setValue($t, $inner);
echo $t->getPrevious()->getMessage(), "\n";
"#,
    );
    assert_eq!(out, "inner LogicException\ninner\n");
}

/// A method named like a Fiber state query (`isStarted()`, `isRunning()`, ...) on a `mixed`
/// receiver that is NOT a Fiber dispatches normally. The name alone routed it to the Fiber
/// predicate, whose non-Fiber arm is a fatal: Symfony's session factory closure keeps the session
/// in `static $sess`, and every symfony/demo page died on `Call to a member function isStarted()
/// on null` inside `AbstractSessionListener`.
#[test]
fn test_fiber_state_method_names_on_a_mixed_non_fiber_receiver() {
    let out = compile_and_run(
        r#"<?php
class Sess { public function isStarted(): bool { return false; } public function isRunning() { return "r"; } }
class Listener {
    protected function getSession(): ?Sess { return new Sess(); }
    public function factory(): Closure {
        return function () {
            static $sess;
            if (!$sess) {
                $sess = $this->getSession();
                if ($sess && !$sess->isStarted()) { echo "fresh ", $sess->isRunning(), "\n"; }
            }
            return $sess;
        };
    }
}
$f = (new Listener())->factory();
echo get_class($f()), " ", get_class($f()), "\n";
"#,
    );
    assert_eq!(out, "fresh r\nSess Sess\n");
}

/// A union with a `string` member returns or binds an object it names AS IS; only an object no
/// member accepts goes through the `Stringable` coercion. Doctrine's
/// `ParameterTypeInferer::inferType(): ParameterType|ArrayParameterType|int|string` cast its enum
/// return to string and died on `could not be converted to string`.
#[test]
fn test_union_with_string_member_keeps_an_object_it_names() {
    let out = compile_and_run(
        r#"<?php
enum ParameterType: int { case STRING = 2; }
enum ArrayParameterType { case INTEGER; case STRING; }
final class Inferer {
    public static function inferType(mixed $value): ParameterType|ArrayParameterType|int|string {
        if (is_int($value)) { return 'integer'; }
        if (is_array($value)) { return is_int(current($value)) ? ArrayParameterType::INTEGER : ArrayParameterType::STRING; }
        return ParameterType::STRING;
    }
}
class Name implements Stringable { public function __toString(): string { return "nm"; } }
class Other {}
function take(Other|string $v): string { return is_string($v) ? "s:" . $v : "o"; }
function keep(Name|int|string $v): string { return is_object($v) ? "obj" : "s:" . $v; }
var_dump(Inferer::inferType([1, 2]) === ArrayParameterType::INTEGER, Inferer::inferType(3));
echo Inferer::inferType(1.5)->name, " ", take(new Name()), " ", take(new Other()), " ", keep(new Name()), "\n";
"#,
    );
    assert_eq!(out, "bool(true)\nstring(7) \"integer\"\nSTRING s:nm o obj\n");
}

/// `ReflectionProperty::isInitialized()` answers for a typed property without a default through a
/// reflector only known at run time, as php does, instead of throwing. Doctrine's
/// `TypedNoDefaultReflectionProperty::getValue()` asks it for every entity id.
#[test]
fn test_reflection_is_initialized_on_runtime_reflector_for_typed_no_default() {
    let out = compile_and_run(
        r#"<?php
class E { public int $id; protected ?string $title; private array $tags; public $loose; public int $n = 0;
  function set() { $this->title = null; } }
$e = new E();
$e->set();
$e->id = 3;
foreach (['id', 'title', 'tags', 'loose', 'n'] as $p) {
    $cls = $argc > 5 ? 'X' : E::class;
    echo $p, ":", var_export((new ReflectionProperty($cls, $p))->isInitialized($e), true), " ";
}
echo count((array) new E()), "\n";
"#,
    );
    assert_eq!(out, "id:true title:true tags:false loose:true n:true 2\n");
}

/// A `: void` override of an untyped abstract method falls off its end as php does. It keeps the
/// ancestor's `mixed` return ABI, and that made it throw "Return value must be of type mixed,
/// none returned": Doctrine's `PersistentCollection::doInitialize(): void`, every symfony/demo
/// post page.
#[test]
fn test_void_override_of_untyped_abstract_falls_off_its_end() {
    let out = compile_and_run(
        r#"<?php
abstract class Lazy {
    public function init(): void { $this->doInitialize(); }
    abstract protected function doInitialize();
}
class Persistent extends Lazy {
    public array $items = [];
    protected function doInitialize(): void { if (count($this->items) > 5) { return; } $this->items[] = "loaded"; }
}
class Other extends Lazy { protected function doInitialize() { return 5; } }
$p = new Persistent();
$p->init();
(new Other())->init();
echo implode(",", $p->items), "\n";
"#,
    );
    assert_eq!(out, "loaded\n");
}

/// Interpreted code calling a compiled by-reference method reads the returned VALUE: the bridge
/// boxed the returned storage address, so `$v = &$session->getUsageIndex()` in Symfony's
/// interpreted `ContextListener` held a pointer as an integer.
#[test]
fn test_eval_reads_a_compiled_by_reference_method_result() {
    let out = compile_and_run(
        r#"<?php
class Session {
    private int $usageIndex = 4;
    private string $name = "sess";
    public function &getUsageIndex(): int { return $this->usageIndex; }
    public function &getName(): string { return $this->name; }
}
$s = new Session();
$code = 'echo $s->getUsageIndex(), " ", $s->getName(), " ", $s->getUsageIndex() + 1, "\n";';
eval($argc > 0 ? $code : '');
"#,
    );
    assert_eq!(out, "4 sess 5\n");
}

/// A by-reference argument to a method on a `mixed` receiver binds by reference when every class
/// declaring the method agrees on it. It was passed by value: Symfony's
/// `NativeSessionStorage::loadSession()` hands `$session[$key]` to each bag's
/// `initialize(array &$a)` over a mixed list, and the session data written through the bags never
/// reached `$_SESSION` (the demo's login page died on the unbound reference).
#[test]
fn test_mixed_receiver_method_binds_by_reference_argument() {
    let out = compile_and_run(
        r#"<?php
class AttributeBag {
    protected array $attributes = [];
    public function getStorageKey(): string { return '_sf2_attributes'; }
    public function initialize(array &$attributes): void { $this->attributes = &$attributes; }
    public function set(string $n, $v): void { $this->attributes[$n] = $v; }
}
class FlashBag {
    private array $flashes = [];
    public function getStorageKey(): string { return '_sf2_flashes'; }
    public function initialize(array &$flashes): void { $this->flashes = &$flashes; }
}
class Storage {
    public function __construct(private array $bags) {}
    public function loadSession(?array &$session = null): void {
        if (null === $session) { $session = &$_SESSION; }
        foreach (array_merge($this->bags, [new FlashBag()]) as $bag) {
            $key = $bag->getStorageKey();
            $session[$key] = isset($session[$key]) && \is_array($session[$key]) ? $session[$key] : [];
            $bag->initialize($session[$key]);
        }
    }
}
$a = new AttributeBag();
$s = new Storage([$a]);
$_SESSION = ['_sf2_attributes' => ['u' => 1]];
$s->loadSession();
$a->set('v', 2);
echo json_encode($_SESSION), "\n";
"#,
    );
    assert_eq!(out, "{\"_sf2_attributes\":{\"u\":1,\"v\":2},\"_sf2_flashes\":[]}\n");
}

/// A method call on a `mixed` receiver borrows only the by-reference SHAPE of a same-named
/// method, never its parameter types. Symfony's `Translator::trans()` calls
/// `$catalogue->get($id, $domain)` on a mixed catalogue while the cache contracts declare
/// `get(string $key, callable $callback, ..., ?array &$metadata)`: `$domain` was normalized as a
/// callable and every translated page died with "Call to undefined function".
#[test]
fn test_mixed_receiver_by_ref_shape_does_not_impose_parameter_types() {
    let out = compile_and_run(
        r#"<?php
class Cache {
    public function get(string $key, callable $callback, ?float $beta = null, ?array &$metadata = null): mixed {
        $metadata = ['k' => $key];
        return $callback($key);
    }
}
class Catalogue {
    public function __construct(private array $messages) {}
    public function get(string $id, string $domain = 'messages'): string {
        return $this->messages[$domain][$id] ?? $id;
    }
}
function pick(array $all, int $i): mixed { return $all[$i]; }
$all = [new Catalogue(['messages' => ['hi' => 'Bonjour']]), new Cache()];
$catalogue = pick($all, $argc > 5 ? 1 : 0);
echo $catalogue->get('hi', 'messages'), "\n";
$cache = pick($all, $argc > 5 ? 0 : 1);
$meta = null;
echo $cache->get('k', fn ($k) => strtoupper($k), null, $meta), json_encode($meta), "\n";
"#,
    );
    assert_eq!(out, "Bonjour\nK{\"k\":\"k\"}\n");
}

/// `array_count_values()` accepts a gradual array: a bare `array` parameter or property, or an
/// untyped one, tallied from whatever shape it holds at run time. Doctrine's `ClassMetadata`
/// tallies field names this way, and the refusal ("argument must be array") left the class to
/// the interpreter, where its property accessors were never built.
#[test]
fn test_array_count_values_on_a_gradual_array() {
    let out = compile_and_run(
        r#"<?php
class M {
    public array $fieldNames = [];
    public function dupes(array $names): array {
        return array_keys(array_filter(array_count_values($names), fn ($c) => $c > 1));
    }
    public function all() { return array_count_values($this->fieldNames); }
}
$m = new M();
$m->fieldNames = ["a" => "x", "b" => "y", "c" => "x", 3, 3];
echo json_encode($m->dupes(["a", "b", "a", 1, 1, 2])), json_encode($m->all()), "\n";
function f($x) { return array_count_values($x); }
echo json_encode(f($argc > 5 ? [] : ["q", "q", 7])), json_encode(f($argc > 5 ? [] : [4, 4, 5])), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",1]{\"x\":2,\"y\":1,\"3\":2}\n{\"q\":2,\"7\":1}{\"4\":2,\"5\":1}\n");
}

/// Interpreted reflection over a COMPILED method sees its parameters' attributes. Symfony's
/// argument resolver reads `#[CurrentUser]` off a compiled controller's `ReflectionParameter`;
/// the list came back empty, `$user` could not be resolved, and every admin page of the demo
/// failed instead of redirecting an anonymous visitor to the login.
#[test]
fn test_interpreted_reflection_reads_compiled_parameter_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_PARAMETER)] class CurrentUser { public function __construct(public ?string $expr = null) {} }
#[Attribute(Attribute::TARGET_PARAMETER)] class MapQuery { public function __construct(public string $key = 'q') {} }
class User {}
final class BlogController {
    public function index(#[CurrentUser] User $user, #[MapQuery('page')] int $page = 1, string $plain = ''): string { return "i"; }
}
$code = <<<'PHP'
final class Factory {
    public function create(\ReflectionFunctionAbstract $reflector): array {
        $out = [$reflector instanceof \ReflectionMethod ? $reflector->class . '::' . $reflector->name : $reflector->name];
        foreach ($reflector->getParameters() as $param) {
            $attrs = [];
            foreach ($param->getAttributes() as $ra) {
                $attrs[] = $ra->getName() . json_encode($ra->getArguments());
            }
            $out[] = $param->getName() . ':' . implode(',', $attrs);
        }
        return $out;
    }
}
PHP;
eval($argc > 0 ? $code : '');
$f = 'Factory';
echo json_encode((new $f())->create(new \ReflectionMethod(new BlogController(), 'index'))), "\n";
"#,
    );
    assert_eq!(
        out,
        "[\"BlogController::index\",\"user:CurrentUser[]\",\"page:MapQuery[\\\"page\\\"]\",\"plain:\"]\n"
    );
}

/// A method that no member of an object union declares is PHP's run-time "Call to undefined
/// method", not a compile error: the call is often guarded by a test the checker does not track.
/// Symfony's `ContextListener` calls `$refreshedUser->getSalt()` on `User|InMemoryUser` behind
/// an `instanceof` test on another variable, and the refusal sent the firewall's context listener
/// to the interpreter.
#[test]
fn test_union_call_no_member_declares_faults_at_run_time() {
    let out = compile_and_run(
        r#"<?php
interface Legacy { public function getSalt(): ?string; }
class User { public function id() { return 1; } }
class Mem { public function id() { return 2; } }
class LUser extends User implements Legacy { public function getSalt(): ?string { return "s"; } }
function refresh(User|Mem $original, User|Mem $refreshed): string {
    if ($original instanceof Legacy && $original->getSalt() !== $refreshed->getSalt()) { return "changed"; }
    return "same " . $refreshed->id();
}
echo refresh(new User(), new Mem()), "\n";
echo refresh(new LUser(), new LUser()), "\n";
try { echo refresh(new LUser(), new Mem()), "\n"; } catch (\Error $e) { echo get_class($e), "\n"; }
"#,
    );
    assert_eq!(out, "same 2\nsame 1\nError\n");
}

/// A property ELEMENT passed by reference to a user callee that KEEPS the reference is bound to
/// the element's own cell, through a typed or a mixed receiver. Symfony's
/// `MockArraySessionStorage::loadSession()` hands `$this->data[$key]` to each bag's
/// `initialize(array &$a) { $this->a = &$a; }`; the copy-in/write-back temporary had no cell
/// ("Cannot return a reference to storage that has no independent reference cell"), and the
/// demo's login redirect killed the worker.
#[test]
fn test_property_element_by_reference_is_kept_by_the_callee() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    private array $attributes = [];
    public function getStorageKey(): string { return '_sf2_attributes'; }
    public function initialize(array &$attributes): void { $this->attributes = &$attributes; }
    public function set(string $n, $v): void { $this->attributes[$n] = $v; }
}
class Flash {
    private array $flashes = [];
    public function getStorageKey(): string { return '_sf2_flashes'; }
    public function initialize(array &$flashes): void { $this->flashes = &$flashes; }
}
class Storage {
    protected array $data = [];
    protected array $bags = [];
    public function register($bag): void { $this->bags[] = $bag; }
    public function load(): void {
        foreach ($this->bags as $bag) {
            $key = $bag->getStorageKey();
            $this->data[$key] ??= [];
            $bag->initialize($this->data[$key]);
        }
    }
    public function typed(Bag $bag): void {
        $this->data['t'] ??= [];
        $bag->initialize($this->data['t']);
    }
    public function data(): array { return $this->data; }
}
$s = new Storage();
$b = new Bag();
$s->register($b);
$s->register(new Flash());
$s->load();
$b->set('x', 1);
echo json_encode($s->data()), "\n";
$b2 = new Bag();
$s->typed($b2);
$b2->set('y', 2);
echo json_encode($s->data()), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"_sf2_attributes\":{\"x\":1},\"_sf2_flashes\":[]}\n{\"_sf2_attributes\":{\"x\":1},\"_sf2_flashes\":[],\"t\":{\"y\":2}}\n"
    );
}

/// Symfony's `UrlGenerator::doGenerate()` casts the extra query parameters with a by-reference
/// closure that captures itself: `get_object_vars($v)` there is typed `mixed` and must be a boxed
/// array, and `$v instanceof \Stringable` narrows a by-reference parameter whose cell holds a box,
/// so `(string) $v` must still dispatch on the real object. The raw hash read as a box made
/// `count()` say 16, and the narrowed read took the box's tag word for a class id ("Object of
/// class ArrayIterator could not be converted to string"): the demo's login redirect segfaulted.
#[test]
fn test_url_generator_parameter_caster() {
    let out = compile_and_run(
        r#"<?php
class Str implements Stringable { public function __toString(): string { return 'S'; } }
class P { public $x = 7; public function m() { return 'm'; } }
function gen(array $extra): string {
    array_walk_recursive($extra, $caster = static function (&$v) use (&$caster) {
        if (\is_object($v)) {
            if ($vars = get_object_vars($v)) {
                array_walk_recursive($vars, $caster);
                $v = $vars;
            } elseif ($v instanceof \Stringable) {
                $v = (string) $v;
            }
        }
    });
    return http_build_query($extra, '', '&', \PHP_QUERY_RFC3986);
}
function narrow(&$v) { if ($v instanceof P) { $v = $v->m() . $v->x; } }
echo gen(['a' => 1, 's' => new Str(), 'p' => new P(), 'n' => ['q' => new Str()]]), "\n";
$o = new P();
narrow($o);
var_dump($o);
"#,
    );
    assert_eq!(out, "a=1&s=S&p%5Bx%5D=7&n%5Bq%5D=S\nstring(2) \"m7\"\n");
}

/// A ternary arm that is itself `x ?? null` merges as what the coalesce yields. It was typed by
/// the syntactic fallback (`int`), so the branch temp became a `?int` scalar and the string the
/// coalesce produced was stored through it as "". Symfony's
/// `ParameterBagUtils::getRequestParameterValue()` reads the login form's CSRF token this way:
/// every demo login failed "Invalid double-submit CSRF token".
#[test]
fn test_ternary_arm_null_coalesce_keeps_its_value() {
    let out = compile_and_run(
        r#"<?php
function method(): string { return $GLOBALS['argc'] > 5 ? 'GET' : 'POST'; }
function all(): array { return ['_csrf_token' => 'csrf-token', 'n' => 3]; }
function token(string $path): mixed {
    $get = static fn ($path) => (!\in_array(method(), ['GET', 'HEAD'], true) ? all()[$path] ?? null : null);
    return $get($path);
}
var_dump(token('_csrf_token'), token('n'), token('missing'));
"#,
    );
    assert_eq!(out, "string(10) \"csrf-token\"\nint(3)\nNULL\n");
}

/// A surplus argument passed through an INTERFACE-typed receiver to an interpreted
/// implementation arrives as itself, not as the collector array the interface's compiled
/// signature packs it into. Symfony's `CheckCredentialsListener` calls
/// `PasswordHasherInterface::verify($hash, $plain, $salt)` (two declared parameters) on an
/// interpreted `MigratingPasswordHasher`: `$salt` was an array and every demo login died.
#[test]
fn test_interface_surplus_argument_reaches_an_interpreted_implementation() {
    let out = compile_and_run(
        r#"<?php
interface PasswordAuth { public function getPassword(): ?string; }
interface LegacyAuth extends PasswordAuth { public function getSalt(): ?string; }
interface HasherIface { public function verify(string $hashed, string $plain): bool; }
class User implements PasswordAuth { public function getPassword(): ?string { return 'h'; } }
class OldUser implements LegacyAuth { public function getPassword(): ?string { return 'h'; } public function getSalt(): ?string { return 's'; } }
$code = <<<'PHP'
class EvalHasher implements HasherIface {
    public function verify(string $hashed, string $plain, ?string $salt = null): bool { echo "salt=", var_export($salt, true), "\n"; return true; }
}
PHP;
eval($argc > 0 ? $code : '');
function hasher(): HasherIface { $c = 'EvalHasher'; return new $c(); }
function check(PasswordAuth $user): bool {
    return hasher()->verify($user->getPassword(), 'kitten', $user instanceof LegacyAuth ? $user->getSalt() : null);
}
var_dump(check(new User()), check(new OldUser()));
"#,
    );
    assert_eq!(out, "salt=NULL\nsalt='s'\nbool(true)\nbool(true)\n");
}

/// A surplus argument passed through an interface-typed TEMPORARY receiver reaches a compiled
/// implementation's extra defaulted parameter. The override ladder skipped owning temporaries,
/// so `$factory->getPasswordHasher($user)->verify($hash, $plain, $salt)` entered
/// `MigratingPasswordHasher::verify(..., ?string $salt = null)` with the interface's surplus
/// collector array in `$salt` and garbage in its own tail: the demo's login crashed.
#[test]
fn test_temporary_interface_receiver_binds_a_wider_override() {
    let out = compile_and_run(
        r#"<?php
interface H { public function verify(string $a, string $b): bool; }
final class M implements H {
    public function verify(string $a, string $b, ?string $salt = null): bool {
        echo "M:", $a, "/", $b, "/", var_export($salt, true), "\n";
        return $a === $b;
    }
}
final class N implements H {
    public function verify(string $a, string $b): bool { echo "N:", $a, "/", $b, " ", count(func_get_args()), "\n"; return true; }
}
final class F {
    public function get(int $i): H { return $i > 1 ? new M() : new N(); }
}
$f = new F();
var_dump($f->get($argc + 1)->verify("x", "x", $argc > 5 ? "s" : null));
var_dump($f->get($argc + 1)->verify("x", "y", "salt"));
var_dump($f->get($argc - 1)->verify("p", "q", "salt"));
"#,
    );
    assert_eq!(
        out,
        "M:x/x/NULL\nbool(true)\nM:x/y/'salt'\nbool(false)\nN:p/q 3\nbool(true)\n"
    );
}

/// An object typed by one interface keeps its identity when passed to a union naming ANOTHER
/// interface beside `string`. Symfony's `PasswordMigratingListener` passes a `UserInterface` to
/// `getPasswordHasher(string|PasswordAuthenticatedUserInterface|PasswordHasherAwareInterface)`; the
/// `string` arm cast the entity, and every demo login died on "Object of class App\Entity\User
/// could not be converted to string".
#[test]
fn test_interface_typed_object_keeps_identity_through_a_string_union() {
    let out = compile_and_run(
        r#"<?php
interface UserIface { public function id(): string; }
interface PasswordIface { public function password(): string; }
final class User implements UserIface, PasswordIface {
    public function id(): string { return 'jane'; }
    public function password(): string { return 'hash'; }
}
final class Label { public function __toString(): string { return 'label'; } }
final class Factory {
    public function hasherFor(string|PasswordIface $user): string {
        return is_string($user) ? "string:$user" : 'object:' . $user->password();
    }
}
function check(Factory $f, UserIface $user): string {
    if (!$user instanceof PasswordIface || '' === $user->password()) {
        return 'no';
    }
    return $f->hasherFor($user);
}
$f = new Factory();
echo check($f, new User()), "\n";
echo $f->hasherFor('name'), "\n";
echo $f->hasherFor(new Label()), "\n";
"#,
    );
    assert_eq!(out, "object:hash\nstring:name\nstring:label\n");
}

/// `json_encode()` boxes a typed `jsonSerialize()` return instead of reading it as a Mixed cell.
/// The demo's `Tag::jsonSerialize(): string` rendered `data-tags="[null,null,...]"` in the post
/// form: the encoder took the raw string pair for a boxed value.
#[test]
fn test_json_encode_boxes_a_typed_json_serialize_return() {
    let out = compile_and_run(
        r#"<?php
class Tag implements JsonSerializable {
    public function __construct(private string $name) {}
    public function jsonSerialize(): string { return $this->name; }
}
class Count implements JsonSerializable {
    public function __construct(private int $n) {}
    public function jsonSerialize(): int { return $this->n; }
}
class Pair implements JsonSerializable {
    public function jsonSerialize(): array { return ['a' => 1, 'b' => [true, 1.5]]; }
}
$tags = [new Tag('lorem'), new Tag('ipsum')];
echo json_encode($tags), "\n";
echo json_encode(['n' => new Count(3), 'p' => new Pair()]), "\n";
echo json_encode(new Tag('x')), "\n";
"#,
    );
    assert_eq!(out, "[\"lorem\",\"ipsum\"]\n{\"n\":3,\"p\":{\"a\":1,\"b\":[true,1.5]}}\n\"x\"\n");
}

/// `$mixed[$k]` reaches an `ArrayAccess::offsetGet()` declared with a narrowed return. Symfony's
/// `FormView::offsetGet(): self` was left out of the runtime's offsetGet table (it only listed
/// `mixed` returns), so Twig's `getAttribute()` raised "Cannot use object of type FormView as
/// array" on every demo form page.
#[test]
fn test_mixed_index_reaches_a_narrowed_offset_get() {
    let out = compile_and_run(
        r#"<?php
class View implements ArrayAccess {
    public array $children = [];
    public array $vars = ['name' => ''];
    public function offsetGet(mixed $name): self { return $this->children[$name]; }
    public function offsetExists(mixed $name): bool { return isset($this->children[$name]); }
    public function offsetSet(mixed $name, mixed $value): void { throw new BadMethodCallException('no'); }
    public function offsetUnset(mixed $name): void { unset($this->children[$name]); }
}
class Labels implements ArrayAccess {
    public function offsetGet(mixed $name): string { return "label-$name"; }
    public function offsetExists(mixed $name): bool { return true; }
    public function offsetSet(mixed $name, mixed $value): void {}
    public function offsetUnset(mixed $name): void {}
}
function read($object, $item) { return $object[$item]; }
function label($object, $item) { return $object[$item]; }
$v = new View();
$child = new View();
$child->vars['name'] = 'title';
$v->children['title'] = $child;
echo get_class(read($v, 'title')), " ", read($v, 'title')->vars['name'], "\n";
echo label(new Labels(), 'x'), "\n";
"#,
    );
    assert_eq!(out, "View title\nlabel-x\n");
}

/// doctrine/orm `UnitOfWork::executeUpdates()` builds `new PreUpdateEventArgs($entity, $em,
/// $this->getEntityChangeSet($entity))` over `array &$changeSet`: php binds the call result to a
/// temporary, notices it, and the callee's writes never reach the source. The compile refusal
/// handed the whole of UnitOfWork to the interpreter.
#[test]
fn test_call_result_passed_by_reference_binds_a_noticed_temporary() {
    let out = compile_and_run_capture(
        r#"<?php
class Args {
    public array $changeSet;
    public function __construct(public object $entity, array &$changeSet) {
        $changeSet['seen'] = true;
        $this->changeSet = &$changeSet;
    }
}
class Uow {
    private array $sets = ['a' => [1, 2]];
    public function getEntityChangeSet(object $e): array { return $this->sets['a']; }
    public function run(object $e): string {
        $args = new Args($e, $this->getEntityChangeSet($e));
        return json_encode($args->changeSet) . ' ' . json_encode($this->sets);
    }
}
function touch_it(array &$a): int { $a[] = 9; return count($a); }
function make(): array { return [1]; }
echo (new Uow())->run(new stdClass()), "\n";
echo touch_it(make()), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":1,\"1\":2,\"seen\":true} {\"a\":[1,2]}\n2\n");
    let notices = out.stderr.matches("Notice: Only variables should be passed by reference").count()
        + out.stdout.matches("Notice: Only variables should be passed by reference").count();
    assert_eq!(notices, 2, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
}

/// symfony/validator's `ConstraintViolationBuilderInterface::setParameter(string $key, string
/// $value)` receives ints and floats from ten validators (`CountValidator`, `ImageValidator`...).
/// Without `strict_types` php coerces them like any class method's; a call dispatched through the
/// INTERFACE was refused, and each validator file was handed to the interpreter.
#[test]
fn test_interface_dispatched_call_coerces_scalars_without_strict_types() {
    let out = compile_and_run(
        r#"<?php
interface Builder { public function setParameter(string $key, string $value): static; }
class B implements Builder {
    public array $p = [];
    public function setParameter(string $key, string $value): static { $this->p[$key] = $value; return $this; }
}
class V {
    public function __construct(private Builder $b) {}
    public function run(int $limit, int|float $size): void {
        $this->b->setParameter('{{ limit }}', $limit)->setParameter('{{ size }}', $size);
    }
}
$b = new B();
(new V($b))->run(5, 1.5);
echo json_encode($b->p), "\n";
"#,
    );
    assert_eq!(out, "{\"{{ limit }}\":\"5\",\"{{ size }}\":\"1.5\"}\n");
}

/// doctrine/orm `AbstractHydrator::gatherRowData()` writes `$this->rsm->newObject[$i]` through a
/// `ResultSetMapping|null $rsm`. The element write was refused ("Array index assignment requires
/// an object or typed pointer"), which interpreted AbstractHydrator and the six hydrators that
/// extend it; a null receiver is php's catchable "Attempt to modify property ... on null".
#[test]
fn test_nullable_receiver_property_element_write() {
    let out = compile_and_run(
        r#"<?php
class Rsm {
    public array $newObjectMappings = [];
    public array $newObject = [];
}
abstract class AbstractHydrator {
    protected Rsm|null $rsm = null;
    public function __construct(?Rsm $rsm) { $this->rsm = $rsm; }
    public function gather(): string {
        foreach ($this->rsm->newObjectMappings as $mapping) {
            if (! array_key_exists($mapping['objIndex'], $this->rsm->newObject)) {
                $this->rsm->newObject[$mapping['objIndex']] = $mapping['className'];
            }
        }
        return json_encode($this->rsm->newObject);
    }
    public function writeNull(): string {
        try { $this->rsm->newObject['x'] = 'y'; return 'no'; } catch (Error $e) { return $e->getMessage(); }
    }
}
final class ObjectHydrator extends AbstractHydrator {}
$r = new Rsm();
$r->newObjectMappings = [['objIndex' => 2, 'className' => 'A'], ['objIndex' => 2, 'className' => 'B'], ['objIndex' => 5, 'className' => 'C']];
echo (new ObjectHydrator($r))->gather(), "\n";
echo (new ObjectHydrator(null))->writeNull(), "\n";
"#,
    );
    assert_eq!(out, "{\"2\":\"A\",\"5\":\"C\"}\nAttempt to modify property \"newObject\" on null\n");
}

/// symfony/form's `BirthdayType` writes `min($options['years'])` and `DefaultChoiceListFactory`
/// `min($groupViewsOrder)` over gradual operands. The one-argument form was refused for a `mixed`
/// operand ("must be of type array, mixed given"), which interpreted both files; php checks it at
/// run time.
#[test]
fn test_min_max_over_a_gradual_operand() {
    let out = compile_and_run(
        r#"<?php
function years(array $options): string {
    return \sprintf('%d-01-01', min($options['years'])) . ' ' . max($options['years']);
}
echo years(['years' => [2001, 1999, 2010]]), "\n";
function pick(array $o): mixed { return min($o['v']); }
var_dump(pick(['v' => ['b', 'a', 'c']]));
try { var_dump(pick(['v' => 5])); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { var_dump(pick(['v' => []])); } catch (ValueError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "1999-01-01 2010\nstring(1) \"a\"\nTypeError: min(): Argument #1 ($value) must be of type array, int given\nValueError: min(): Argument #1 ($value) must contain at least one element\n"
    );
}

/// Doctrine's `MultiTableUpdateExecutor` / `MultiTableDeleteExecutor` push onto the inherited
/// `protected array|string $sqlStatements`. The push was refused ("Array push requires an array
/// property, got array<mixed>|array<mixed, mixed>|string"), which interpreted both executors.
#[test]
fn test_push_onto_an_array_or_string_property() {
    let out = compile_and_run(
        r#"<?php
abstract class A { protected array|string $s; public function get(): array|string { return $this->s; } }
final class B extends A {
    public function __construct(int $n) {
        $this->s = [];
        for ($i = 0; $i < $n; $i++) { $this->s[] = 'q' . $i; }
        foreach ($this->s as $k => $v) { echo $k, '=', $v, "\n"; }
    }
}
final class C extends A { public function __construct() { $this->s = 'one'; } }
echo implode(',', (new B(3))->get()), "\n";
echo (new C())->get(), "\n";
"#,
    );
    assert_eq!(out, "0=q0\n1=q1\n2=q2\nq0,q1,q2\none\n");
}

/// symfony/validator's `->setParameter('{{ limit }}', $constraint->max)` passes a `?int` once a
/// call between the `null !==` test and the argument drops the narrowing. php converts the int and
/// throws a `TypeError` for a null; the call was refused ("expects Str, got Union([Int, Void])"),
/// which interpreted six validators.
#[test]
fn test_nullable_int_into_a_string_parameter() {
    let out = compile_and_run(
        r#"<?php
function s(string $v): string { return "[$v]"; }
function pick(int $n): ?int { return $n > 0 ? $n : null; }
echo s(pick(4)), "\n";
try { echo s(pick(0)), "\n"; } catch (TypeError $e) { echo get_class($e), "\n"; }
"#,
    );
    assert_eq!(out, "[4]\nTypeError\n");
}

/// symfony/form's `NumberToLocalizedStringTransformer::round()` and symfony/validator's
/// `FileValidator::moreDecimalsThan()` pass a `string` to `round()` / `ceil()`. php converts a
/// numeric string to the number it spells and throws a `TypeError` for any other; code
/// generation refused the operand ("round for PHP type Str"), which interpreted both files.
#[test]
fn test_math_builtins_over_numeric_strings() {
    let out = compile_and_run(
        r#"<?php
function r(string $n, int $p): float { return round($n, $p); }
function c(string $n): float { return ceil($n); }
function f(string $n): float { return floor($n); }
function a(string $n): int|float { return abs($n); }
var_dump(r('3.14159', 2), c('2.1'), f('-2.1'), a('-7'), a('-7.5'));
try { var_dump(c('12abc')); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "float(3.14)\nfloat(3)\nfloat(-3)\nint(7)\nfloat(7.5)\nceil(): Argument #1 ($num) must be of type int|float, string given\n"
    );
}

/// Doctrine's `ObjectHydrator::onClear()` fills `array_keys($this->identifierMap)`, an array
/// whose keys are known only at run time. Code generation refused it ("array_fill_keys key
/// element PHP type Mixed"), which interpreted the hydrator. php keeps an int key and converts
/// any other key to a string, so a float key is `"1.5"`, not the truncated `1`.
#[test]
fn test_array_fill_keys_over_gradual_keys() {
    let out = compile_and_run(
        r#"<?php
final class H {
    private array $identifierMap = [];
    public function add(string $alias): void { $this->identifierMap[$alias] = ['x' => 1]; }
    public function clear(): void {
        $aliases = array_keys($this->identifierMap);
        $this->identifierMap = array_fill_keys($aliases, []);
    }
    public function count(): int { return count($this->identifierMap['u']) + count($this->identifierMap['p']); }
}
$h = new H(); $h->add('u'); $h->add('p'); $h->clear(); echo $h->count(), "\n";
function fk(array $keys, mixed $v): array { return array_fill_keys($keys, $v); }
echo json_encode(fk([1, 'a', 1.5, true, '7'], 0)), "\n";
"#,
    );
    assert_eq!(out, "0\n{\"1\":0,\"a\":0,\"1.5\":0,\"7\":0}\n");
}

/// league/commonmark's `InlineParserEngine` keeps the text of preg offset pairs with
/// `array_column($match, 0)`, and symfony/security-bundle reads `array_column($factories, 1)` off
/// list rows. The native reader takes a string column key and associative rows only, so code
/// generation refused both ("array_column key PHP type Int"), which interpreted the files.
#[test]
fn test_array_column_with_an_int_key_over_list_rows() {
    let out = compile_and_run(
        r#"<?php
function cols(array $matches): string {
    $out = [];
    foreach ($matches as $match) {
        $offset = \intval($match[0][1]);
        $m = \array_column($match, 0);
        $out[$offset][] = implode('|', $m);
    }
    return json_encode($out);
}
echo cols([[['ab', 1], ['b', 2]], [['a', 4]]]), "\n";
function sorted(array $factories): array {
    usort($factories, static fn ($a, $b) => $b[0] <=> $a[0] ?: $a[2] <=> $b[2]);
    return array_column($factories, 1);
}
echo implode(',', sorted([[1, 'x', 0], [5, 'y', 1], [1, 'z', 2]])), "\n";
"#,
    );
    assert_eq!(out, "{\"1\":[\"ab|b\"],\"4\":[\"a\"]}\ny,x,z\n");
}

/// Twig's `Environment::getExtension()` returns `$this->extensionSet->getExtension($class)` through
/// an untyped property, so the checker reads the result off every same-named method that can take
/// one argument. The hidden `func_get_args()` collector counted as a REQUIRED parameter, so every
/// method carrying it looked one argument short; in the demo console, where all of them carry it,
/// only the translation dumpers' parameterless `getExtension(): string` remained, and
/// `Environment` was refused for returning a string -- Twig's whole environment was interpreted.
#[test]
fn test_mixed_receiver_candidates_ignore_the_hidden_args_collector() {
    let out = compile_and_run(
        r#"<?php
interface ExtensionInterface { public function name(): string; }
final class Ext implements ExtensionInterface { public function name(): string { return 'ext'; } }
final class ExtensionSet {
    private $extensions = [];
    public function add(ExtensionInterface $e): void { $this->extensions[get_class($e)] = $e; }
    public function getExtension(string $class): ExtensionInterface {
        if (count(func_get_args()) > 1) { throw new LogicException('extra'); }
        return $this->extensions[$class];
    }
}
final class Dumper {
    public function getExtension(): string { return 'csv' . count(func_get_args()); }
}
class Environment {
    private $extensionSet;
    public function __construct() { $this->extensionSet = new ExtensionSet(); $this->extensionSet->add(new Ext()); }
    public function getExtension(string $class): ExtensionInterface { func_get_args(); return $this->extensionSet->getExtension($class); }
}
echo (new Environment())->getExtension('Ext')->name(), "\n";
echo (new Dumper())->getExtension(), "\n";
"#,
    );
    assert_eq!(out, "ext\ncsv0\n");
}

/// symfony/clock's `DatePoint::__construct()` ends with `$this->__unserialize((array) $now)`,
/// php 8.2's `DateTimeImmutable::__unserialize()`. The builtin had no such method, so the class
/// was interpreted. Here `(array)` yields the backing properties, which the method takes as well
/// as php's `date`/`timezone_type`/`timezone` shape that `__serialize()` returns.
#[test]
fn test_datetime_serialize_and_unserialize_methods() {
    let out = compile_and_run(
        r#"<?php
final class DatePoint extends \DateTimeImmutable {
    public function __construct(string $datetime = 'now', ?\DateTimeZone $timezone = null) {
        $now = new \DateTimeImmutable('2024-03-05 10:20:30.250000', $timezone);
        if ('now' !== $datetime) {
            $now = $now->modify($datetime);
        }
        $this->__unserialize((array) $now);
    }
}
$p = new DatePoint('+1 day', new \DateTimeZone('Europe/Paris'));
echo get_class($p), ' ', $p->format('Y-m-d H:i:s.u e'), "\n";
$d = new \DateTimeImmutable('2024-01-01 00:00:00', new \DateTimeZone('UTC'));
echo json_encode($d->__serialize()), "\n";
$e = new \DateTime('2000-01-01');
$e->__unserialize(['date' => '2021-06-07 08:09:10.000000', 'timezone_type' => 3, 'timezone' => 'Europe/Paris']);
echo $e->format(DATE_ATOM), "\n";
"#,
    );
    assert_eq!(
        out,
        "DatePoint 2024-03-06 10:20:30.250000 Europe/Paris\n{\"date\":\"2024-01-01 00:00:00.000000\",\"timezone_type\":3,\"timezone\":\"UTC\"}\n2021-06-07T08:09:10+02:00\n"
    );
}

/// The hash writer takes its own reference to a string value, so an owned string a literal
/// inserts -- a call's result -- has to be released after the insert. Neither keyed literal path
/// did: `['date' => $d->format('Y'), 'n' => 1]` leaked one string per evaluation, and so did
/// every `DateTimeImmutable::__serialize()`.
#[test]
fn test_keyed_literal_releases_an_inserted_call_string() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class U { public function f(string $x): string { return str_repeat($x, 3); } }
function g(string $x): string { return str_repeat($x, 2); }
$u = new U();
for ($i = 0; $i < 3; $i++) {
    $a = ['date' => $u->f('Y'), 'z' => 1];
    $b = ['only' => g('Z')];
    $c = ['k' => g('Q'), ...['m' => 2]];
    echo $a['date'], $b['only'], $c['k'], "\n";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "YYYZZQQ\nYYYZZQQ\nYYYZZQQ\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Doctrine's `UnitOfWork::eagerLoadCollections(array $collections)` calls
/// `array_chunk($collections, $size, true)`. A declared `array` is the list-or-hash union, and the
/// checker accepted it only with a non-literal flag, although the lowering sends every
/// three-argument gradual call to the same keyed helper -- so the literal `true` interpreted the
/// whole unit of work.
#[test]
fn test_array_chunk_with_a_literal_flag_over_a_declared_array() {
    let out = compile_and_run(
        r#"<?php
final class Coll { public function __construct(public string $n) {} }
final class UoW {
    private array $pending = [];
    public function add(string $k, Coll $c): void { $this->pending[$k][spl_object_id($c)] = $c; }
    public function run(int $size): void {
        foreach ($this->pending as $group) { $this->load($group, $size); }
    }
    private function load(array $collections, int $size): void {
        foreach (array_chunk($collections, $size, true) as $batch) {
            echo count($batch), ':';
            foreach ($batch as $c) { echo $c->n; }
            echo "\n";
        }
    }
}
$u = new UoW();
$u->add('a', new Coll('x')); $u->add('a', new Coll('y')); $u->add('a', new Coll('z'));
$u->run(2);
"#,
    );
    assert_eq!(out, "2:xy\n1:z\n");
}

/// Doctrine's `Parser::getAST()` reads `$AST->selectClause` after
/// `assert($AST instanceof AST\SelectStatement)`, on a union with `DeleteStatement`, which has no
/// such property: "Undefined property: DeleteStatement::selectClause" interpreted the DQL parser.
/// elephc's `assert()` always throws on a false assertion, so the statements after it see the
/// asserted class, as after `if (!$x instanceof C) throw`.
#[test]
fn test_assert_instanceof_narrows_the_following_statements() {
    let out = compile_and_run(
        r#"<?php
namespace App\Query;
final class SelectClause { public array $selectExpressions = ['a', 'b', 'c']; }
final class SelectStatement { public SelectClause $selectClause; public function __construct() { $this->selectClause = new SelectClause(); } }
final class DeleteStatement { public string $deleteClause = 'd'; }
final class Parser {
    public function getAST(bool $select): SelectStatement|DeleteStatement {
        return $select ? new SelectStatement() : new DeleteStatement();
    }
    public function fix(): string {
        $AST = $this->getAST(true);
        assert($AST instanceof SelectStatement);
        $key = array_search('b', $AST->selectClause->selectExpressions, true);
        unset($AST->selectClause->selectExpressions[$key]);
        return implode(',', $AST->selectClause->selectExpressions);
    }
}
echo (new Parser())->fix(), "\n";
"#,
    );
    assert_eq!(out, "a,c\n");
}

/// Doctrine's `EntityManager::__construct()` calls `$conn->getEventManager()` behind
/// `method_exists($conn, 'getEventManager')`, a method DBAL 4's `Connection` and every class below
/// it lack. Code generation refused the call ("method call to unknown method"), which interpreted
/// the entity manager; php throws only if the call runs, and here it never does.
#[test]
fn test_guarded_call_to_a_method_no_class_declares_compiles() {
    let out = compile_and_run(
        r#"<?php
class EventManager { public string $name = 'default'; }
class Connection { public function query(): string { return 'q'; } }
final class EntityManager {
    public EventManager $eventManager;
    public function __construct(Connection $conn, ?EventManager $eventManager = null) {
        $this->eventManager = $eventManager
            ?? (method_exists($conn, 'getEventManager') ? $conn->getEventManager() : new EventManager());
    }
}
echo (new EntityManager(new Connection()))->eventManager->name, "\n";
"#,
    );
    assert_eq!(out, "default\n");
}

/// Doctrine's DQL parser stores `Token::$value` (`string|int`) into its
/// `string|null &$fieldAlias` parameter. php checks a parameter's type only when binding it, and
/// both unions are the same boxed cell here, but the store was refused ("cannot reassign
/// $fieldAlias from string|null to string|int|null"), which interpreted the parser.
#[test]
fn test_wider_union_stored_into_a_union_by_ref_parameter() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
final class Token { public function __construct(public string|int $value) {} }
final class Parser {
    public int $n = 0;
    public function arg(string|null &$fieldAlias = null): string {
        $fieldAlias = null;
        $token = new Token($this->n++ % 2 ? 7 : 'alias');
        $fieldAlias = $token->value;
        return 'expr';
    }
    public function args(): string {
        $fieldAlias = null;
        $e = $this->arg($fieldAlias);
        return get_debug_type($fieldAlias) . ':' . $e;
    }
}
$p = new Parser();
echo $p->args(), ' ', $p->args(), "\n";
"#,
    );
    assert_eq!(out, "string:expr int:expr\n");
}

/// A computed key of a keyed array literal (`[$alias ?? 'none' => $v]`) is copied into the hash,
/// so an owned key -- a box, a call's string -- is the literal's to release. It never was, and
/// every evaluation leaked it.
#[test]
fn test_keyed_literal_releases_a_computed_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function k(string $s): string { return str_repeat($s, 2); }
function build(string|int|null $alias, string $s): array {
    return [$alias ?? 'none' => 1, k($s) => 2, 'c' . $s => 3];
}
for ($i = 0; $i < 3; $i++) {
    echo implode(',', array_keys(build($i % 2 ? null : 'x' . $i, 'ab'))), "\n";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "x0,abab,cab\nnone,abab,cab\nx2,abab,cab\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An object of an INTERPRETED class has no compiled class id, so the nominal check of a compiled
/// `?Interface` parameter said no for it whatever it implemented: symfony/http-client's
/// interpreted `NativeHttpClient` handed to `?HttpClientInterface $client` was "Value must be of
/// type HttpClientInterface, object given", and every importmap command died on it. The check now
/// asks the eval bridge before raising; an unrelated interpreted class is still a TypeError.
#[test]
fn test_interpreted_object_passes_a_compiled_interface_parameter() {
    let out = compile_and_run(
        r#"<?php
interface HCI { public function req(): string; }
eval('class NativeHC implements HCI { public function req(): string { return "native"; } } class Other { }');
function make(string $cls): object { return new $cls(); }
final class Cmd {
    private HCI $client;
    public function __construct(?HCI $client = null) { $this->client = $client ?? make('NativeHC'); }
    public function go(): string { return $this->client->req(); }
}
echo (new Cmd(make('NativeHC')))->go(), "\n";
try { new Cmd(make('Other')); } catch (TypeError $e) { echo get_class($e), "\n"; }
"#,
    );
    assert_eq!(out, "native\nTypeError\n");
}

/// An element write, push or unset through a property CHAIN (`$a->b->items[$k] = $v`) acquires
/// the intermediate object and never released it, so that object -- and everything it held --
/// outlived its owner by one reference per statement.
#[test]
fn test_chained_property_element_writes_release_the_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Clause { public array $items = ['a', 'b', 'c']; public $untyped = []; }
final class Stmt { public Clause $clause; public function __construct() { $this->clause = new Clause(); } }
final class Holder {
    public Stmt $stmt;
    public function __construct() { $this->stmt = new Stmt(); }
    public function run(): string {
        $this->stmt->clause->items[1] = 'B';
        $this->stmt->clause->items[] = 'D';
        $this->stmt->clause->untyped['k'] = 'v';
        $this->stmt->clause->untyped[] = 'w';
        unset($this->stmt->clause->items[0]);
        return implode(',', $this->stmt->clause->items) . '|' . implode(',', $this->stmt->clause->untyped);
    }
}
for ($i = 0; $i < 2; $i++) {
    $s = new Stmt();
    $s->clause->items[1] = 'B';
    $s->clause->items[] = 'D';
    unset($s->clause->items[0]);
    echo implode(',', $s->clause->items), ' ', (new Holder())->run(), "\n";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "B,c,D B,c,D|v,w\nB,c,D B,c,D|v,w\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new ReflectionFunction('<unknown>')` is php's catchable ReflectionException -- VarDumper's
/// ExceptionCaster reflects every trace frame's function name (`{closure}` among them) and catches
/// it -- but the bridge reported an unsupported construct, a FATAL that killed every demo console
/// command whose error path dumped a trace. Each hand-built interpreter throw also leaked its
/// message and code operands (one string and two boxes per throw).
#[test]
fn test_reflection_function_of_unknown_name_throws_and_releases_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function known(int $a): int { return $a; }
$names = [$argc > 5 ? "x" : "nope_fn", "known", "{closure}", "strlen"];
for ($i = 0; $i < 2; $i++) {
    foreach ($names as $n) {
        try { $r = new ReflectionFunction($n); echo $n, " ok ", $r->getNumberOfParameters(), "\n"; }
        catch (ReflectionException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
    }
    try { new ReflectionClass($argc > 5 ? "x" : "Nope"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    let once = "ReflectionException: Function nope_fn() does not exist\nknown ok 1\n\
ReflectionException: Function {closure}() does not exist\nstrlen ok 1\nClass \"Nope\" does not exist\n";
    assert_eq!(out.stdout, once.repeat(2));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An attribute argument naming an ENUM's plain constant (Symfony's `Requirement::POSITIVE_INT`
/// in `#[Route(requirements: ...)]`) was judged unmaterializable -- only enum CASES counted -- so
/// the whole attribute list became "unsupported" and the demo's router silently lost every route
/// carrying such a requirement. Reflecting the member also leaked each nested argument array (the
/// box retained it) and `ReflectionFunction` stored its short name twice.
#[test]
fn test_attribute_enum_constant_arguments_materialize_without_leaks() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[Attribute(Attribute::TARGET_METHOD | Attribute::IS_REPEATABLE)]
final class R { public function __construct(public string $path, public ?string $name = null, public array $requirements = []) {} }
enum Req { case Unused; public const POSITIVE_INT = '[1-9][0-9]*'; }
final class Ctl {
    #[R('/a', name: 'a', requirements: ['page' => Req::POSITIVE_INT])]
    #[R('/b', 'b', [Req::POSITIVE_INT])]
    public function index(): void {}
}
for ($i = 0; $i < 3; $i++) {
    foreach ((new ReflectionMethod(Ctl::class, 'index'))->getAttributes() as $a) {
        $r = $a->newInstance();
        echo $r->name, ' ', $r->path, ' ', json_encode($r->requirements), ' ', json_encode($a->getArguments()), "\n";
    }
    $f = new ReflectionFunction('strlen');
    echo $f->getShortName(), "\n";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    let once = "a /a {\"page\":\"[1-9][0-9]*\"} {\"0\":\"\\/a\",\"name\":\"a\",\"requirements\":{\"page\":\"[1-9][0-9]*\"}}\n\
b /b [\"[1-9][0-9]*\"] [\"\\/b\",\"b\",[\"[1-9][0-9]*\"]]\nstrlen\n";
    assert_eq!(out.stdout, once.repeat(3));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `settype()` changes its variable's type: on an untyped parameter it read back the string it
/// was given (`settype($v, 'int')` left `"12"`), and a run-time type name (Nette's
/// `Helpers::getCastStrategy()` closure) was refused by the backend. Both go through a prelude
/// function with a by-reference `mixed` parameter now, with php's case-insensitive names and its
/// two `ValueError`s.
#[test]
fn test_settype_retypes_the_variable_for_literal_and_runtime_types() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g($value) { settype($value, 'int'); return $value; }
final class Helpers {
    public static function getCastStrategy(string $type): \Closure {
        return static function ($value) use ($type) { settype($value, $type); return $value; };
    }
}
var_dump(g('12'));
foreach (['INT', 'Bool', 'double', 'string', 'array', 'NULL', 'resource', 'foo'] as $t) {
    try {
        echo $t, ' => ', var_export(Helpers::getCastStrategy($t)('12'), true), "\n";
    } catch (ValueError $e) {
        echo $t, ' => ', $e->getMessage(), "\n";
    }
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "int(12)\nINT => 12\nBool => true\ndouble => 12.0\nstring => '12'\narray => array (\n  0 => '12',\n)\nNULL => NULL\n\
resource => resource => Cannot convert to resource type\nfoo => foo => settype(): Argument #2 ($type) must be a valid type\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A `match` whose subject is an owning temporary (`match (strtolower($t))`) released it on no
/// path: one leaked string per evaluation.
#[test]
fn test_match_releases_an_owning_temporary_subject() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function kind(string $type): string {
    return match (strtolower($type)) { 'int', 'integer' => 'number', 'string' => 'text', default => 'other' };
}
$out = [];
for ($i = 0; $i < 6; $i++) { $out[] = kind(['INT', 'String', 'x'][$i % 3]); }
echo implode(',', $out), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "number,text,other,number,text,other\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `unpack($be ? 'N1' : 'V1', $bytes)` (Symfony's `MoFileLoader::readLong`) is split into two
/// literal-format calls; the backend refused the ternary format and the whole demo recompiled.
#[test]
fn test_unpack_with_a_ternary_of_literal_formats_compiles() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Mo {
    public function readLong($stream, bool $isBigEndian): int {
        $result = unpack($isBigEndian ? 'N1' : 'V1', fread($stream, 4));
        $result = current($result);
        return (int) substr($result, -8);
    }
}
$f = fopen('php://memory', 'w+');
fwrite($f, pack('V', 258) . pack('N', 258));
rewind($f);
$mo = new Mo();
echo $mo->readLong($f, false), ' ', $mo->readLong($f, true), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "258 258\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `ReflectionProperty::setRawValue()` with a property known only at run time writes private and
/// protected slots in the declaring class's scope (Doctrine's `RawValuePropertyAccessor`). The
/// inline dynamic write it used carried no scope: it stored nothing into a private property and
/// refused a protected one; and on a receiver narrowed to an INTERFACE the backend refused it.
#[test]
fn test_reflection_set_raw_value_writes_private_and_protected_slots() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
interface Proxy { public function __isInitialized(): bool; }
interface InternalProxy extends Proxy { public function __setInitialized(bool $v): void; }
class Entity { public $name = 'n'; private $hidden = 'h'; protected $prot = 'p'; }
class EntityProxy extends Entity implements InternalProxy {
    private bool $init = false;
    public function __isInitialized(): bool { return $this->init; }
    public function __setInitialized(bool $v): void { $this->init = $v; }
}
function setRaw(ReflectionProperty $rp, object $o, mixed $v): void { $rp->setRawValue($o, $v); }
function narrowed(ReflectionProperty $rp, object $o, mixed $v): void {
    if ($o instanceof InternalProxy) {
        $o->__setInitialized(true);
        $rp->setRawValue($o, $v);
        $o->__setInitialized(false);
    }
}
$e = new EntityProxy();
foreach (['name', 'hidden', 'prot'] as $p) {
    $rp = new ReflectionProperty(Entity::class, $p);
    setRaw($rp, $e, "raw-$p");
    echo $p, '=', $rp->getValue($e), ' ';
    narrowed($rp, $e, "proxy-$p");
    echo $rp->getValue($e), "\n";
}
echo count(get_object_vars($e)), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "name=raw-name proxy-name\nhidden=raw-hidden proxy-hidden\nprot=raw-prot proxy-prot\n1\n"
    );
}

/// The `clone` callable refuses a non-public `__clone` with php's message naming the RUNTIME
/// scope, read from the class-name table: one static message per possible scope made each such
/// class carry an arm per class of the program (9.9 M instructions in the Symfony demo).
#[test]
fn test_clone_callable_names_the_runtime_scope_of_a_hook_refusal() {
    let out = compile_and_run(
        r#"<?php
class Priv { public $v = 1; private function __clone() {} public static function inside(object $o): object { return array_map(clone(...), [$o])[0]; } }
class Prot { public $v = 2; protected function __clone() {} }
class ProtChild extends Prot { public static function inside(object $o): object { return array_map(clone(...), [$o])[0]; } }
class Other { public static function try(object $o): void {
    try { array_map(clone(...), [$o]); echo "cloned\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
} }
foreach ([new Priv, new Prot, new ProtChild] as $o) {
    try { array_map(clone(...), [$o]); echo "cloned\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
    Other::try($o);
}
echo get_class(Priv::inside(new Priv)), "\n";
echo get_class(ProtChild::inside(new ProtChild)), "\n";
echo get_class(ProtChild::inside(new Prot)), "\n";
"#,
    );
    assert_eq!(
        out,
        "Error: Call to private method Priv::__clone() from global scope\n\
Error: Call to private method Priv::__clone() from scope Other\n\
Error: Call to protected method Prot::__clone() from global scope\n\
Error: Call to protected method Prot::__clone() from scope Other\n\
Error: Call to protected method Prot::__clone() from global scope\n\
Error: Call to protected method Prot::__clone() from scope Other\n\
Priv\nProtChild\nProt\n"
    );
}

/// A literal-name read from an untyped receiver answers a class's dynamic property through the
/// `_class_dyn_hash_rows` row instead of one arm per class: the Symfony demo carried ~3 300 such
/// arms in each of its 415 named-read helpers (40 M instructions). Declared slots, `__get`,
/// a strict ancestor's private name, stdClass and a class without dynamic storage keep php's
/// answers, for a value read, `isset()` and `??`.
#[test]
fn test_mixed_named_read_answers_dynamic_properties_through_the_row_table() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
#[AllowDynamicProperties] class A { public $declared = 'decl-A'; }
#[AllowDynamicProperties] class B { private $hidden = 'h'; }
#[AllowDynamicProperties] class C extends B {}
#[AllowDynamicProperties] class D { public function __get($n) { return "magic-$n"; } }
class E { public $other = 1; }
#[AllowDynamicProperties] class F { public $dyn = 'declared-in-F'; }
function make(int $i): mixed {
    return match ($i % 7) { 0 => new A, 1 => new B, 2 => new C, 3 => new D, 4 => new E, 5 => new F, default => new stdClass };
}
for ($i = 0; $i < 7; $i++) {
    $o = make($i);
    if (!($o instanceof E) && !($o instanceof D)) { $o->dyn = "v$i"; }
    echo get_class($o), ' read=', var_export(@$o->dyn, true), ' isset=', var_export(isset($o->dyn), true),
        ' coalesce=', $o->dyn ?? 'none', ' missing=', var_export(@$o->nothere, true), "\n";
    echo get_class($o), ' hidden=', var_export(isset($o->hidden), true), ' declared=', var_export(@$o->declared, true), "\n";
}
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "A read='v0' isset=true coalesce=v0 missing=NULL\nA hidden=false declared='decl-A'\n\
B read='v1' isset=true coalesce=v1 missing=NULL\nB hidden=false declared=NULL\n\
C read='v2' isset=true coalesce=v2 missing=NULL\nC hidden=false declared=NULL\n\
D read='magic-dyn' isset=false coalesce=magic-dyn missing='magic-nothere'\nD hidden=false declared='magic-declared'\n\
E read=NULL isset=false coalesce=none missing=NULL\nE hidden=false declared=NULL\n\
F read='v5' isset=true coalesce=v5 missing=NULL\nF hidden=false declared=NULL\n\
stdClass read='v6' isset=true coalesce=v6 missing=NULL\nstdClass hidden=false declared=NULL\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new ReflectionFunction($command->__invoke(...))` reflects the METHOD, parameter attributes
/// included: Symfony's InvokableCommand builds every `#[Option]`/`#[Argument]` from them, and the
/// demo's `app:list-users --help` listed no option because the compiled closure's native record
/// carried none.
#[test]
fn test_reflection_function_of_a_method_closure_keeps_parameter_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_PARAMETER)]
final class Opt { public function __construct(public string $description = '', public ?string $name = null) {} }
final class Cmd {
    public function __invoke(object $in, #[Opt('Send to', 'send-to')] ?string $email = null, #[Opt('Limit')] int $maxResults = 50): int { return 0; }
}
function describe(callable $code): void {
    foreach ((new ReflectionFunction($code))->getParameters() as $p) {
        $attrs = $p->getAttributes(Opt::class, ReflectionAttribute::IS_INSTANCEOF);
        echo $p->getName(), ' ', count($attrs);
        foreach ($attrs as $a) { $o = $a->newInstance(); echo ' ', $o->description, ' ', var_export($o->name, true); }
        echo "\n";
    }
}
$cmd = new Cmd();
describe($cmd->__invoke(...));
describe(Closure::fromCallable([$cmd, '__invoke']));
"#,
    );
    let once = "in 0\nemail 1 Send to 'send-to'\nmaxResults 1 Limit NULL\n";
    assert_eq!(out, once.repeat(2));
}

/// A weak-mode string argument to an `int` parameter converts when the call runs, as php
/// does: symfony's ICU polyfill passes a string timestamp to `int $timestamp`, and refusing it
/// deferred the file to the interpreter and cost the demo a recompile. A non-numeric string is a
/// catchable `TypeError` naming the call site, and neither path leaks.
#[test]
fn test_weak_mode_string_argument_to_an_int_parameter_converts_at_run_time() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function twice(int $n): int { return $n * 2; }
final class Fmt { public function at(int $ts): string { return "ts=" . $ts; } }
foreach (['12', ' 12', 'abc', '', '12abc', '1e3', '99999999999999999999999'] as $s) {
    $s = $s . '';
    try { echo twice($s), "\n"; } catch (TypeError $e) { echo strstr($e->getMessage(), ', called', true), "\n"; }
}
$f = new Fmt();
echo $f->at(substr("x42", 1)), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "24\n24\n\
twice(): Argument #1 ($n) must be of type int, string given\n\
twice(): Argument #1 ($n) must be of type int, string given\n\
twice(): Argument #1 ($n) must be of type int, string given\n\
2000\n\
twice(): Argument #1 ($n) must be of type int, string given\n\
ts=42\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `foreach ($weakMap as $object => $value)` yields the stored OBJECTS as keys and their values as
/// values; the WeakMap model copied `SplObjectStorage`'s positions-and-objects iteration, so Twig's
/// `Parser::checkPrecedenceDeprecations()` handed a parser object to `in_array()` as its haystack
/// and `lint:twig` died.
#[test]
fn test_weak_map_iterates_objects_as_keys_and_values_as_values() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class A { public function __construct(public string $n) {} }
$a = new A('a'); $b = new A('b');
$m = new WeakMap();
$m[$a] = [1, 2];
$m[$b] = [3];
foreach ($m as $k => $v) { echo get_debug_type($k), ' ', $k->n, ' ', count($v), "\n"; }
function walk(WeakMap $w, A $needle): void {
    foreach ($w as $k => $v) { echo $k === $needle ? 'hit' : 'miss', ' ', in_array(3, $v, true) ? 'in' : 'out', "\n"; }
}
walk($m, $b);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A a 2\nA b 1\nmiss out\nhit in\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `$o[$k] = [ ... ]` and `$o[] = new C()` on an `ArrayAccess` object hand a fresh temporary to
/// `offsetSet()`, which only borrows it; the write released strings alone and leaked one array or
/// object per write -- every `SplObjectStorage`/`WeakMap` entry stored with an array payload.
#[test]
fn test_array_access_write_releases_a_temporary_value_handed_to_offset_set() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Box implements ArrayAccess {
    private array $d = [];
    public function offsetExists(mixed $o): bool { return isset($this->d[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->d[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { if ($o === null) { $this->d[] = $v; } else { $this->d[$o] = $v; } }
    public function offsetUnset(mixed $o): void { unset($this->d[$o]); }
}
final class P { public function __construct(public int $n) {} }
function make(int $i): array { return ['i' => $i, 's' => str_repeat('x', $i)]; }
$b = new Box();
for ($i = 0; $i < 5; $i++) {
    $b['k' . $i] = [1, $i];
    $b['m' . $i] = make($i);
    $b['o' . $i] = new P($i);
    $b[] = [$i, 'z' . $i];
    $b[] = new P($i * 10);
}
echo count($b['k3']), ' ', $b['m4']['s'], ' ', $b['o2']->n, ' ', $b[0][1], ' ', $b[1]->n, "\n";
$s = new SplObjectStorage();
$p = new P(7);
$s[$p] = [7, 14];
echo $s[$p][1], "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2 xxxx 2 z0 0\n14\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
