//! Purpose:
//! End-to-end regressions for general runtime gaps the Symfony demo (`examples/symfony-demo`)
//! exposed under `--web`: each program is ordinary PHP whose compiled or interpreted behavior
//! diverged from php.
//!
//! Called from:
//! - `cargo test --test codegen_tests web_app_runtime_gaps` through Rust's test harness.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.10's output for the same program.

use crate::support::*;

/// `extension_loaded('session')` inside eval answers what the compiled half answers.
///
/// The session surface is a compiled prelude the host registers into eval as native callbacks;
/// eval's extension list only knew the catalog, so code the runtime loads (Symfony's
/// `NativeSessionStorage`) threw "PHP extension "session" is required" while `session_start()` was
/// callable from that very code.
#[test]
fn test_eval_reports_the_session_extension_the_program_links() {
    let out = compile_cli_file_and_run(r###"<?php
echo json_encode(extension_loaded('session')), "\n";
eval('echo json_encode(extension_loaded("session")), "\n"; echo json_encode(in_array("session", get_loaded_extensions(), true)), "\n";');
"###, &[]);
    assert_eq!(out, "true\ntrue\ntrue\n");
}

/// `fopen()` supports php's `x` (exclusive create) and `c` (create, no truncate) modes, and a
/// `+` anywhere in the mode.
///
/// Only `r`, `w` and `a` were recognised, so `fopen($tmp, 'x')` -- Symfony's cache
/// `FilesystemCommonTrait::write()` -- failed and every cache save died on `fwrite(false)`.
#[test]
fn test_fopen_accepts_exclusive_and_create_modes() {
    let out = compile_and_run(r###"<?php
$f = 'elephc-fopen-modes.txt';
@unlink($f);
$h = fopen($f, 'x');
var_dump(is_resource($h));
fwrite($h, "hello");
fclose($h);
var_dump(@fopen($f, 'x'));
$c = fopen($f, 'c+');
echo fread($c, 3), "\n";
fclose($c);
$r = fopen($f, 'rb+');
fwrite($r, "J");
fclose($r);
echo file_get_contents($f), "\n";
unlink($f);
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nhel\nJello\n");
}

/// A `set_error_handler()` callback that throws keeps receiving the later diagnostics.
///
/// The runtime marks a dispatch in progress around the call into the handler and cleared the mark
/// only when the call returned; a throwing handler never returns there, so every later diagnostic
/// of the process bypassed the handler. Symfony's cache adapters install exactly such a handler
/// around each `include` of a cache file.
#[test]
fn test_error_handler_keeps_receiving_diagnostics_after_it_throws() {
    let out = compile_cli_file_and_run(r###"<?php
set_error_handler(static function (int $type, string $message) {
    throw new ErrorException($message, 0, $type);
});
$a = [];
foreach (['first', 'second', 'third'] as $key) {
    try {
        $v = $a[$key];
        echo "no warning for $key\n";
    } catch (ErrorException $e) {
        echo "caught: ", $e->getMessage(), "\n";
    }
}
"###, &[]);
    assert_eq!(out, "caught: Undefined array key \"first\"\ncaught: Undefined array key \"second\"\ncaught: Undefined array key \"third\"\n");
}

/// A call through an interface reaches an implementation that WIDENS the interface's parameter.
///
/// `CacheItemPoolInterface::getItem(string $key)` is implemented by
/// `AbstractAdapterTrait::getItem(mixed $key)`: the interface table was called with the string's
/// registers while the implementation read a boxed cell, and the key arrived empty.
#[test]
fn test_interface_typed_call_reaches_an_implementation_declaring_mixed() {
    let out = compile_and_run(r###"<?php
interface PoolI { public function getItem(string $key): string; }
trait AdapterT { public function getItem(mixed $key): string { return "item(" . var_export($key, true) . ")"; } }
abstract class AbstractPool implements PoolI { use AdapterT; }
class FilesPool extends AbstractPool {}
class TracedPool extends FilesPool {
    public function getItem(mixed $key): string { return "traced:" . parent::getItem($key); }
}
function viaInterface(PoolI $pool, string $key): string { return $pool->getItem($key); }
echo viaInterface(new FilesPool(), "k1"), "\n";
echo viaInterface(new TracedPool(), "k2"), "\n";
echo (new TracedPool())->getItem("k3"), "\n";
"###);
    assert_eq!(out, "item('k1')\ntraced:item('k2')\ntraced:item('k3')\n");
}

/// A closure invoked out of an untyped slot receives a closure argument it can call.
///
/// The descriptor invoker passed the boxed cell to a `callable` parameter instead of the
/// descriptor it carries, so `$callback(...)` jumped into the cell's bytes -- Symfony's cache
/// `ContractsTrait` callback wrapper on every cache miss.
#[test]
fn test_closure_from_an_untyped_slot_receives_a_callable_argument() {
    let out = compile_and_run(r###"<?php
class P {
    private $w;
    public function __construct() { $this->w = static fn (callable $callback, $item) => $callback($item); }
    public function viaLocal(callable $cb) { $w = $this->w; return $w($cb, 'L'); }
    public function viaProp(callable $cb) { return ($this->w)($cb, 'P'); }
    public function literal() { $w = $this->w; return $w(fn ($i) => 'lit-' . $i, 'A'); }
}
$p = new P();
echo $p->viaLocal(fn ($i) => 'local-' . $i), "\n";
echo $p->viaProp(fn ($i) => 'prop-' . $i), "\n";
echo $p->literal(), "\n";
"###);
    assert_eq!(out, "local-L\nprop-P\nlit-A\n");
}

/// An interpreted `$object[$key] = $value` on an ArrayAccess object is one `offsetSet()` call.
///
/// The interpreter read the element through `offsetGet()` first, which runs code php does not run
/// and made the write throw when `offsetGet()` throws for an unknown key -- Symfony Intl's
/// `RingBuffer`, so every locale bundle read failed.
#[test]
fn test_eval_array_access_assignment_calls_only_offset_set() {
    let out = compile_and_run(r###"<?php
eval('
class RB implements ArrayAccess {
    private array $d = [];
    public function offsetExists(mixed $k): bool { echo "exists($k)\n"; return isset($this->d[$k]); }
    public function offsetGet(mixed $k): mixed {
        echo "get($k)\n";
        if (!isset($this->d[$k])) { throw new OutOfBoundsException("no $k"); }
        return $this->d[$k];
    }
    public function offsetSet(mixed $k, mixed $v): void { echo "set($k)\n"; $this->d[$k] = $v; }
    public function offsetUnset(mixed $k): void {}
}
class H {
    private RB $b;
    public function __construct() { $this->b = new RB(); }
    public function put(string $k) {
        if (!isset($this->b[$k])) { $this->b[$k] = strtoupper($k); }
        return $this->b[$k];
    }
}
echo (new H())->put("a"), "\n";
$r = new RB();
$r["x"] = 1;
$r["x"] .= "2";
echo $r["x"], "\n";
');
"###);
    assert_eq!(out, "exists(a)\nset(a)\nget(a)\nA\nset(x)\nget(x)\nset(x)\nget(x)\n12\n");
}

/// Declaring a class autoloads the classes its override signatures need to be proven compatible.
///
/// php loads `IdleConnectionDriver` to learn that `wrap(): IdleConnectionDriver` satisfies
/// `Middleware::wrap(): Driver`, and `DatePoint` for `convertToPHPValue(): ?DatePoint` over
/// `?DateTimeImmutable`; eval checked the unloaded names, proved nothing and refused the class.
#[test]
fn test_eval_declaration_autoloads_classes_its_covariant_signatures_name() {
    let out = compile_and_run(r###"<?php
class Loader {
    public function loadClass($class) {
        echo "autoload($class)\n";
        if ($class === 'Impl') {
            eval('final class Impl extends AbstractImpl {}');
        } elseif ($class === 'AbstractImpl') {
            eval('abstract class AbstractImpl implements Driver {}');
        } elseif ($class === 'Point') {
            eval('final class Point extends \DateTimeImmutable {}');
        }
    }
}
spl_autoload_register([new Loader(), 'loadClass']);
eval('
interface Driver {}
interface Mw { public function wrap(Driver $d): Driver; }
class M implements Mw { public function wrap(Driver $d): Impl { return new Impl(); } }
echo get_class((new M())->wrap(new Impl())), "\n";
abstract class BaseType { public function toPhp(mixed $v): mixed { return $v; } }
class ImmType extends BaseType { public function toPhp(mixed $v): ?\DateTimeImmutable { return $v === null ? null : new \DateTimeImmutable($v); } }
final class PointType extends ImmType { public function toPhp(mixed $v): ?Point { return $v === null ? null : new Point($v); } }
echo get_class((new PointType())->toPhp("2020-01-02")), "\n";
');
"###);
    assert_eq!(out, "autoload(Impl)\nautoload(AbstractImpl)\nImpl\nautoload(Point)\nPoint\n");
}

/// Trailing commas are accepted in `unset(...)` (PHP 7.3) and in a closure's `use (...)` (PHP 8.0)
/// by the interpreter too; Doctrine's `UnitOfWork` could not be loaded at run time.
#[test]
fn test_trailing_commas_in_unset_and_closure_use_lists() {
    let out = compile_and_run(r###"<?php
$a = ['x' => 1, 'y' => 2, 'z' => 3];
unset(
    $a['x'],
    $a['y'],
);
echo implode(',', array_keys($a)), "\n";
eval('
$b = [1, 2, 3];
unset($b[0], $b[1],);
echo implode(",", $b), "\n";
$c = 5;
$h = function ($x,) use ($c, $b,) { return $x . $c . count($b); };
echo $h(4,), "\n";
');
"###);
    assert_eq!(out, "z\n3\n451\n");
}

/// PHP 8.3 typed class constants and comma-grouped constant declarations, compiled and interpreted.
///
/// The compiler declared one constant per `const` and refused `const int N = 3, M = 4;`; the
/// interpreter refused the type in `public const array DEFAULT_SCHEME_MAP` (Doctrine's
/// `ConnectionFactory`).
#[test]
fn test_typed_and_grouped_class_constants() {
    let out = compile_and_run(r###"<?php
class TcA { public const array MAP = ['a' => 1]; const int N = 3, M = 4; }
interface TcI { const string LABEL = 'iface', OTHER = 'o'; }
echo TcA::MAP['a'], TcA::N, TcA::M, TcI::LABEL, TcI::OTHER, "\n";
eval('
class TcB {
    public const array MAP = ["pdo_sqlite" => "sqlite", "db2" => "ibm_db2"];
    final protected const ?string NAME = null;
    const int|string MIXED = 5, OTHER = "x";
    public static function name() { return var_export(self::NAME, true); }
}
echo TcB::MAP["db2"], " ", TcB::name(), " ", TcB::MIXED, TcB::OTHER, "\n";
');
"###);
    assert_eq!(out, "134ifaceo\nibm_db2 NULL 5x\n");
}

/// The DateTime factories declare php's parameter types, so a subclass may override them.
///
/// `createFromMutable()` was declared over `DateTimeInterface` and `createFromTimestamp()` over
/// `mixed`, so `symfony/clock`'s `DatePoint` overrides looked like narrowed parameters and the
/// class could not be declared at run time.
#[test]
fn test_datetime_factories_declare_phps_parameter_types() {
    let out = compile_and_run(r###"<?php
eval('
final class Dp extends \DateTimeImmutable
{
    public static function createFromMutable(\DateTime $object): static { return parent::createFromMutable($object); }
    public static function createFromTimestamp(int|float $timestamp): static { return parent::createFromTimestamp($timestamp); }
}
echo "declared\n";
echo (new Dp("2020-01-02 03:04:05"))->format("Y-m-d H:i"), "\n";
');
echo DateTimeImmutable::createFromMutable(new DateTime("@86400"))->getTimestamp(), "\n";
echo DateTimeImmutable::createFromTimestamp(1700000000.5)->format("U.u"), "\n";
echo DateTime::createFromImmutable(new DateTimeImmutable("@172800"))->getTimestamp(), "\n";
"###);
    assert_eq!(out, "declared\n2020-01-02 03:04\n86400\n1700000000.500000\n172800\n");
}

/// `fn ($class) => new $class()` returns the object it builds.
///
/// The closure's return type fell to the syntactic `int` default, so every object was converted to
/// a number on the way out -- Doctrine DBAL's `Type::createTypeRegistry()` maps its type classes
/// exactly so.
#[test]
fn test_closure_returning_a_dynamic_new_returns_the_object() {
    let out = compile_and_run(r###"<?php
abstract class T {}
class A extends T {}
class B extends T {}
$m = ['a' => A::class, 'b' => B::class];
foreach (array_map(static fn ($c) => new $c(), $m) as $k => $v) { echo $k, '=', get_debug_type($v), "\n"; }
foreach (array_map(static fn ($c) => new $c(), [A::class]) as $v) { echo get_class($v), "\n"; }
"###);
    assert_eq!(out, "a=A\nb=B\nA\n");
}

/// `return self::$x ??= make();` in interpreted code keeps the object the class holds alive.
///
/// The static slot consumes the reference it is given, and a later non-null read handed the slot's
/// own cell back unretained; the caller's release destroyed the registry -- Doctrine DBAL's
/// `Type::getTypeRegistry()` read back as a string after its first use.
#[test]
fn test_eval_static_property_coalesce_assignment_keeps_its_object() {
    let out = compile_and_run(r###"<?php
eval('
final class Reg {
    public function __construct(public string $tag) {}
    public function has(string $n): bool { return $n === "x"; }
}
abstract class Ty
{
    private static ?Reg $registry = null;
    final public static function registry(): Reg { return self::$registry ??= self::create(); }
    private static function create(): Reg { return new Reg("static"); }
    public static function has(string $n): bool { return self::registry()->has($n); }
}
var_dump(Ty::has("x"));
var_dump(Ty::has("y"));
var_dump(spl_object_id(Ty::registry()) === spl_object_id(Ty::registry()));
echo Ty::registry()->tag, "\n";
');
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nstatic\n");
}

/// An interpreted class can extend `ReflectionProperty`, construct through it and override it.
///
/// The owner was declared final (php's is not), the subclass instance kept an empty reflector, and
/// the reflection fast paths bypassed its overrides -- Doctrine's `RuntimeReflectionProperty`,
/// which backs every mapped entity field.
#[test]
fn test_eval_class_extends_reflection_property() {
    let out = compile_and_run(r###"<?php
class Post { public string $title = 'hello'; private int $views = 3; }
eval('
class Sub extends \ReflectionProperty {}
$obj = new Post();
$s = new Sub(Post::class, "title");
echo $s->getName(), "|", $s->getValue($obj), "\n";
class RuntimeProp extends \ReflectionProperty
{
    public function getValue(?object $object = null): mixed { return "wrapped:" . parent::getValue($object); }
    public function setValue(mixed $object, mixed $value = null): void { parent::setValue($object, $value); }
}
$rp = new RuntimeProp(Post::class, "title");
echo $rp->getValue($obj), "\n";
$rp->setValue($obj, "changed");
echo $obj->title, "\n";
$v = new RuntimeProp(Post::class, "views");
echo $v->isPrivate() ? "private" : "public", "\n";
');
"###);
    assert_eq!(out, "title|hello\nwrapped:hello\nchanged\nprivate\n");
}

/// Interpreted `unset($container[$a][$b])` removes the nested key, and an absent link is a no-op.
///
/// Only the ArrayAccess form was supported: Doctrine's
/// `unset($this->initializedHashMapping[$event][$hash])` failed as an unsupported construct.
#[test]
fn test_eval_nested_unset() {
    let out = compile_and_run(r###"<?php
eval('
class M {
    private array $map = ["a" => ["x" => 1, "y" => 2], "b" => ["z" => 3]];
    public function drop(string $e, string $h): void { unset($this->map[$e][$h]); }
    public function dump(): string { return json_encode($this->map); }
}
$m = new M();
$m->drop("a", "x");
$m->drop("missing", "q");
$m->drop("b", "nope");
echo $m->dump(), "\n";
$arr = ["k" => ["i" => ["j" => 1, "l" => 2]]];
unset($arr["k"]["i"]["j"]);
unset($arr["none"]["i"]);
echo json_encode($arr), "\n";
');
"###);
    assert_eq!(out, "{\"a\":{\"y\":2},\"b\":{\"z\":3}}\n{\"k\":{\"i\":{\"l\":2}}}\n");
}

/// A Reflection method called through `[$reflector, 'method']` is answered like a direct call.
///
/// The callable path invoked the compiled method body, which knows nothing of a reflector the
/// interpreter materialized: Doctrine's Instantiator builds every entity through
/// `[$reflectionClass, 'newInstanceWithoutConstructor']` and got `null`.
#[test]
fn test_reflector_methods_called_through_array_callables() {
    let out = compile_and_run(r###"<?php
class Post { public string $title = 'hello'; public function __construct() { $this->title = 'constructed'; } }
eval('
$rc = new \ReflectionClass(Post::class);
$factory = [$rc, "newInstanceWithoutConstructor"];
$p = $factory();
echo get_class($p), " ", $p->title, "\n";
echo call_user_func([$rc, "getShortName"]), "\n";
echo call_user_func_array([$rc, "getName"], []), "\n";
');
"###);
    assert_eq!(out, "Post hello\nPost\nPost\n");
}

/// `class_parents()` / `class_implements()` in compiled code see classes the interpreter declared.
///
/// The compiled relation registry answered `false` for them, and
/// `[$c => $c] + class_parents($c) + class_implements($c)` threw "Value must be of type array,
/// false given" -- Symfony's `ControllerAttributesListener`, on every controller attribute.
#[test]
fn test_class_relations_see_interpreted_classes() {
    let out = compile_and_run(r###"<?php
interface CI {}
class CBase implements CI {}
eval('interface EI {} class EBase extends CBase implements EI {} class EChild extends EBase {}');
function hierarchy(string $class): array {
    return [$class => $class] + class_parents($class) + class_implements($class);
}
foreach (['CBase', 'EChild'] as $n) {
    $h = hierarchy($n);
    ksort($h);
    echo $n, ': ', implode(',', $h), "\n";
}
$o = eval('return new EChild();');
$parents = class_parents($o);
ksort($parents);
echo implode(',', $parents), "\n";
"###);
    assert_eq!(out, "CBase: CBase,CI\nEChild: CBase,CI,EBase,EChild,EI\nCBase,EBase\n");
}

/// An undeclared `__toString()` carries php's implicit `string` return type.
///
/// The interpreter compared the missing type against `Stringable::__toString(): string` and refused
/// the class -- Doctrine's `ArrayCollection`, which every entity relation holds.
#[test]
fn test_undeclared_to_string_satisfies_a_compiled_stringable_contract() {
    let out = compile_and_run(r###"<?php
interface Named extends Stringable { public function name(): string; }
function show(Named $n): string { return $n->name() . ':' . $n; }
eval('
class N implements Named {
    public function name(): string { return "n"; }
    public function __toString() { return "N!"; }
}
echo show(new N()), "\n";
echo (new ReflectionMethod("N", "__toString"))->getReturnType(), "\n";
');
"###);
    assert_eq!(out, "n:N!\nstring\n");
}

/// An array filled in one arm of an `if` inside a loop is returned with the element type the
/// caller was promised.
///
/// The `if` join met `array<string>` (the arm that pushed) and `array<never>` (the arm that did
/// not) and boxed the array as `array<mixed>`, while the checker typed the function's return
/// `array<string>`: the caller read each boxed cell pointer as a string -- empty output here, a
/// multi-gigabyte allocation request in Symfony's `ControllerAttributesListener`.
#[test]
fn test_array_filled_in_one_branch_of_a_loop_returns_its_strings() {
    let out = compile_and_run(r###"<?php
function names(object $o, array $with): array
{
    $names = [];
    $class = $o::class;
    foreach ([$class => $class, 'Countable' => 'Countable'] as $c) {
        if (isset($with[$c])) {
            $names[] = 'ev.' . $c;
        }
    }
    return $names;
}
foreach (names(new ArrayObject(), ['ArrayObject' => true, 'Countable' => true]) as $n) {
    echo $n, "\n";
}
$words = [];
for ($i = 0; $i < 4; $i++) {
    if ($i % 2 === 0) {
        $words[] = "w$i";
    }
}
echo implode(',', $words), "\n";
"###);
    assert_eq!(out, "ev.ArrayObject\nev.Countable\nw0,w2\n");
}

/// `class_implements()` and `ReflectionClass::getInterfaceNames()` list interfaces in php's order.
///
/// php-src links a class that names no interface of its own by copying its parent's list from the
/// end, so `class D extends P {}` reports P's interfaces reversed; a class declaring `__toString`
/// names `Stringable` after its own interfaces, and a subclass inherits it in that position.
/// Symfony orders attribute listeners by `class_implements()`.
#[test]
fn test_class_implements_follows_php_interface_order() {
    let out = compile_and_run(r###"<?php
interface CI {}
interface CJ extends CI {}
class CBase implements CJ {}
interface EI {}
interface EK {}
class EBase extends CBase implements EI, EK {}
class D1 extends EBase {}
class D2 extends D1 {}
class D3 extends D2 implements Countable { public function count(): int { return 0; } }
class St { public function __toString(): string { return 's'; } }
class St2 extends St {}
interface Z {}
class St3 extends St implements Z {}
foreach (['EBase', 'D1', 'D2', 'D3', 'St', 'St2', 'St3'] as $c) {
    echo $c, ': ', implode(',', class_implements($c)), ' / ', implode(',', (new ReflectionClass($c))->getInterfaceNames()), "\n";
}
eval('class X1 extends EBase {} class X2 extends X1 {} class X3 extends CBase {} class X4 { public function __toString(): string { return "x"; } } class X5 extends X4 {}');
foreach (['X1', 'X2', 'X3', 'X4', 'X5'] as $c) {
    echo $c, ': ', implode(',', class_implements($c)), ' / ', implode(',', (new ReflectionClass($c))->getInterfaceNames()), "\n";
}
echo implode(',', class_implements(eval('return new X1();'))), "\n";
"###);
    assert_eq!(out, "EBase: CJ,CI,EI,EK / CJ,CI,EI,EK\nD1: EK,EI,CI,CJ / EK,EI,CI,CJ\nD2: CJ,CI,EI,EK / CJ,CI,EI,EK\nD3: CJ,CI,EI,EK,Countable / CJ,CI,EI,EK,Countable\nSt: Stringable / Stringable\nSt2: Stringable / Stringable\nSt3: Stringable,Z / Stringable,Z\nX1: EK,EI,CI,CJ / EK,EI,CI,CJ\nX2: CJ,CI,EI,EK / CJ,CI,EI,EK\nX3: CI,CJ / CI,CJ\nX4: Stringable / Stringable\nX5: Stringable / Stringable\nEK,EI,CI,CJ\n");
}

/// `defined()` and `constant()` resolve `Class::CONST` and enum-case names, literal or computed.
///
/// The compiled constant registry lists global constants only, so a runtime
/// `defined('N\\TT::T_' . $word)` answered `false` and a literal `constant('N\\K::C')` was a
/// compile error. Doctrine's DQL lexer classifies every keyword that way: `SELECT` came back as a
/// plain identifier and every query failed to parse.
#[test]
fn test_defined_and_constant_resolve_class_constants_by_name() {
    let out = compile_and_run(r###"<?php
namespace N;
enum TT: int { case T_NONE = 1; case T_SELECT = 200; const ALIAS = self::T_SELECT; }
class K { const C = 5; const S = 'five'; }
define('MY_C', 3);
eval('class EK { const D = 7; }');
function type(string $value): TT
{
    $name = 'N\TT::T_' . strtoupper($value);
    if (defined($name)) {
        $type = constant($name);
        if ($type->value > 100) {
            return $type;
        }
    }
    return TT::T_NONE;
}
echo type('select')->name, ' ', type('nope')->name, ' ', type('none')->name, "\n";
foreach (['N\TT::T_SELECT', 'N\TT::T_NOPE', 'N\TT::ALIAS', 'N\K::C', '\N\TT::T_SELECT', 'EK::D', 'MY_C', 'Nope\X::Y'] as $name) {
    $d = defined($name);
    echo $name, ': ', var_export($d, true);
    if ($d) {
        $v = constant($name);
        echo ' = ', $v instanceof TT ? 'TT::' . $v->name : var_export($v, true);
    }
    echo "\n";
}
try {
    $missing = 'N\K::' . strtoupper('nope');
    constant($missing);
} catch (\Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
var_dump(constant('N\K::C'), constant('\N\K::S'), constant('N\TT::T_SELECT') === TT::T_SELECT);
"###);
    assert_eq!(out, "T_SELECT T_NONE T_NONE\nN\\TT::T_SELECT: true = TT::T_SELECT\nN\\TT::T_NOPE: false\nN\\TT::ALIAS: true = TT::T_SELECT\nN\\K::C: true = 5\n\\N\\TT::T_SELECT: true = TT::T_SELECT\nEK::D: true = 7\nMY_C: true = 3\nNope\\X::Y: false\nError: Undefined constant N\\K::NOPE\nint(5)\nstring(4) \"five\"\nbool(true)\n");
}

/// `constant('Enum::CASE')` in interpreted code hands its caller an owned reference.
///
/// The class-constant fetch returns the case singleton borrowed; the builtin returned it as is,
/// the method's local released it on return, and the singleton was freed: the next fetch of the
/// same case read reused memory. Doctrine's DQL lexer typed the second `JOIN` of a query as an
/// identifier, and every blog page's query failed to parse.
#[test]
fn test_interpreted_constant_keeps_the_enum_case_singleton_alive() {
    let out = compile_and_run(r###"<?php
eval('
namespace X;
enum TT: int { case T_NONE = 1; case T_IDENTIFIER = 102; case T_JOIN = 230; case T_LEFT = 232; }
final class Tok { public $type; public function __construct(public string $value, $type) { $this->type = $type; } }
class Lex {
    public array $tokens = [];
    protected function getType(string &$value): TT {
        $type = TT::T_NONE;
        $name = "X\\\\TT::T_" . strtoupper($value);
        if (defined($name)) {
            $type = constant($name);
            if ($type->value > 100) { return $type; }
        }
        return TT::T_IDENTIFIER;
    }
    public function scan(array $words): void {
        foreach ($words as $w) { $first = $w; $type = $this->getType($first); $this->tokens[] = new Tok($first, $type); }
    }
}
');
$l = new X\Lex();
$l->scan(['JOIN', 'a', 'JOIN', 'LEFT', 'b', 'LEFT', 'JOIN']);
foreach ($l->tokens as $t) {
    echo $t->value, ' ', $t->type->name, "\n";
}
"###);
    assert_eq!(out, "JOIN T_JOIN\na T_IDENTIFIER\nJOIN T_JOIN\nLEFT T_LEFT\nb T_IDENTIFIER\nLEFT T_LEFT\nJOIN T_JOIN\n");
}

/// `implode()` over a mixed array joins each object through its `__toString()`.
///
/// The runtime joiner cast every boxed element with the scalar cast, which has no object arm:
/// objects joined as empty strings. Doctrine's `QueryBuilder` joins its `Expr\\Select` and
/// `Expr\\OrderBy` parts that way, so every query went out as `SELECT ,  FROM ... ORDER BY`.
#[test]
fn test_implode_joins_objects_through_to_string() {
    let out = compile_and_run(r###"<?php
class A { public function __construct(private string $s) {} public function __toString(): string { return 'A:' . $this->s; } }
eval('class E { public function __construct(private string $s) {} public function __toString(): string { return "E:" . $this->s; } }');
$e = [eval('return new E("p");'), eval('return new E("q");')];
echo implode(', ', $e), "\n";
$m = [new A('m'), eval('return new E("n");'), 'str', 5, 1.5, true, null];
echo implode(' | ', $m), "\n";
echo join('-', $m), "\n";
$parts = ['select' => [eval('return new E("s");')]];
echo 'SELECT ' . implode(', ', $parts['select']), "\n";
function parts(): array { return [new A('f'), 'g']; }
echo implode('+', parts()), "\n";
eval('
$parts = ["select" => [new E("x"), new A("y")], "empty" => []];
echo "SELECT " . implode(", ", $parts["select"]), "\n";
echo "[" . implode(", ", $parts["empty"]) . "]", "\n";
echo join([new E("j"), "k"]), "\n";
');
"###);
    assert_eq!(out, "E:p, E:q\nA:m | E:n | str | 5 | 1.5 | 1 | \nA:m-E:n-str-5-1.5-1-\nSELECT E:s\nA:f+g\nSELECT E:x, A:y\n[]\nE:jk\n");
}

/// A `match` arm that yields an enum case (or any shared cell) hands the caller its own reference.
///
/// The arm value was returned as the enum's cached singleton cell, while every consumer treats a
/// `match` result as a fresh temporary: `$t = APT::toElement($t)` then released the singleton on
/// the next reassignment, freed it, and the following `PT::INTEGER` read reused memory. Doctrine's
/// `ArrayParameterType::toElementParameterType()` is that shape, and every paginated query then
/// died on `Unhandled match case`.
#[test]
fn test_interpreted_match_arm_hands_out_an_owned_enum_case() {
    let out = compile_and_run(r###"<?php
eval('
enum PT { case NUL; case INTEGER; case STR; }
enum APT {
    case INTEGER; case STR;
    public static function toElement(self $type): PT {
        return match ($type) {
            self::INTEGER => PT::INTEGER,
            self::STR => PT::STR,
        };
    }
}
function name(PT $p): string {
    return match ($p) { PT::NUL => "null", PT::INTEGER => "int", PT::STR => "str" };
}
function pick(int $i, array $pool): mixed { return match ($i % 2) { 0 => $pool[0], default => $pool[1] }; }
for ($i = 0; $i < 6; $i++) {
    $t = APT::INTEGER;
    $t = APT::toElement($t);
    echo name($t);
}
$pool = [PT::STR, "s"];
for ($i = 0; $i < 6; $i++) {
    $p = pick($i, $pool);
    $p = null;
}
echo " ", name(PT::INTEGER), " ", name($pool[0]), "\n";
');
"###);
    assert_eq!(out, "intintintintintint int str\n");
}

/// `(array) $object` in interpreted code sees a compiled object's declared properties.
///
/// The interpreter walked only a stdClass dynamic-property hash, so every property a compiled
/// class declares was missing and the cast came back empty. Doctrine reads each entity field as
/// `((array) $entity)["\\0Class\\0name"]` (`RawValuePropertyAccessor::getValue()`), so every
/// hydrated entity looked blank.
#[test]
fn test_interpreted_array_cast_of_a_compiled_object_mangles_its_properties() {
    let out = compile_and_run(r###"<?php
class Base { private string $secret = 's'; protected int $level = 1; }
class Post extends Base { private ?int $id = null; protected string $title = ''; public array $tags = []; public int $unset; public function id(): ?int { return $this->id; } public function title(): string { return $this->title; } }
eval('
$o = (new ReflectionClass(Post::class))->newInstanceWithoutConstructor();
$id = new ReflectionProperty(Post::class, "id");
$title = new ReflectionProperty(Post::class, "title");
$tags = new ReflectionProperty(Post::class, "tags");
$id->setRawValueWithoutLazyInitialization($o, 42);
$title->setRawValue($o, "hello");
$tags->setRawValueWithoutLazyInitialization($o, ["a"]);
echo $o->id(), " ", $o->title(), " ", implode(",", $o->tags), "\n";
$arr = (array) $o;
ksort($arr);
foreach ($arr as $k => $v) { echo str_replace("\0", "~", $k), "=", json_encode($v), " "; }
echo "\n", $arr["\0Post\0id"], " ", $arr["\0*\0title"], " ", count((array) new stdClass()), "\n";
$s = new stdClass(); $s->a = 1; $s->b = [2];
echo json_encode((array) $s), "\n";
');
"###);
    assert_eq!(out, "42 hello a\n~*~level=1 ~*~title=\"hello\" ~Base~secret=\"s\" ~Post~id=42 tags=[\"a\"] \n42 hello 0\n{\"a\":1,\"b\":[2]}\n");
}

/// Reflection may initialize a readonly property once, from any scope; a second write throws.
///
/// The interpreter refused the FIRST write with an uncatchable fatal, while php allows it and
/// only rejects a write to an initialized readonly property with a catchable `Error`. Doctrine
/// hydrates `private readonly string $name` that way (the demo's `Tag`).
#[test]
fn test_reflection_initializes_an_uninitialized_readonly_property() {
    let out = compile_and_run(r###"<?php
class TagC { public function __construct(private readonly string $name = 'x') {} public function name(): string { return $this->name; } }
eval('
class Tag { private ?int $id = null; private readonly string $name; public readonly int $n; public function name(): string { return $this->name; } }
$t = (new ReflectionClass(Tag::class))->newInstanceWithoutConstructor();
(new ReflectionProperty(Tag::class, "name"))->setRawValueWithoutLazyInitialization($t, "php");
(new ReflectionProperty(Tag::class, "n"))->setValue($t, 3);
echo $t->name(), " ", $t->n, "\n";
try { (new ReflectionProperty(Tag::class, "name"))->setRawValueWithoutLazyInitialization($t, "again"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { (new ReflectionProperty(Tag::class, "n"))->setValue($t, 4); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$c = (new ReflectionClass(TagC::class))->newInstanceWithoutConstructor();
(new ReflectionProperty(TagC::class, "name"))->setRawValueWithoutLazyInitialization($c, "compiled");
echo $c->name(), "\n";
');
"###);
    assert_eq!(out, "php 3\nError: Cannot modify readonly property Tag::$name\nError: Cannot modify readonly property Tag::$n\ncompiled\n");
}

/// A user `ArrayAccess` class whose methods declare no return type compiles, and `$o[] = $v` runs
/// `offsetSet(null, $v)`.
///
/// The checker compared its own INFERENCE of an undeclared return type against the interface's
/// tentative one (`offsetUnset(): void` against an inferred `int`) and refused the class, and the
/// append form dispatched `append()`, which only the SPL containers define. Doctrine's
/// `ArrayCollection` hit both, and twenty classes built on it went to the interpreter.
#[test]
fn test_untyped_array_access_implementation_compiles_and_appends_through_offset_set() {
    let out = compile_and_run(r###"<?php
class Coll implements ArrayAccess, IteratorAggregate, Countable
{
    private array $elements = [];
    public function remove($key) { $removed = $this->elements[$key] ?? null; unset($this->elements[$key]); return $removed; }
    public function set($key, $value) { $this->elements[$key] = $value; }
    #[\ReturnTypeWillChange]
    public function offsetExists(mixed $offset) { return isset($this->elements[$offset]); }
    #[\ReturnTypeWillChange]
    public function offsetGet(mixed $offset) { return $this->elements[$offset] ?? null; }
    #[\ReturnTypeWillChange]
    public function offsetSet(mixed $offset, mixed $value)
    {
        if ($offset === null) {
            $this->elements[] = $value;
            return;
        }
        $this->set($offset, $value);
    }
    #[\ReturnTypeWillChange]
    public function offsetUnset(mixed $offset) { $this->remove($offset); }
    #[\ReturnTypeWillChange]
    public function getIterator() { return new ArrayIterator($this->elements); }
    #[\ReturnTypeWillChange]
    public function count() { return count($this->elements); }
}
function addTo(ArrayAccess $a, $v): void { $a[] = $v; }
$c = new Coll();
$c['a'] = 1;
$c[] = 2;
unset($c['a']);
echo count($c), " ", isset($c[0]) ? 'y' : 'n', " ", $c[0], "\n";
foreach ($c as $k => $v) { echo $k, '=', $v, "\n"; }
addTo($c, 7);
echo count($c), " ", $c[1], "\n";
$o = new ArrayObject([1]);
$o[] = 2;
echo count($o), "\n";
"###);
    assert_eq!(out, "1 y 2\n0=2\n2 7\n2\n");
}

/// `array_map()` with two or more arrays walks them in parallel by position.
///
/// The arity check refused a third argument, so Doctrine's `MappingException`, DoctrineBundle's
/// `MiddlewaresPass` and the Live Component runtime went to the interpreter. The call now goes to a
/// built prelude helper: keys are dropped, shorter arrays pad with `null`, and a `null` callback
/// zips.
#[test]
fn test_array_map_walks_several_arrays_in_parallel() {
    let out = compile_and_run(r###"<?php
$a = [1, 2, 3];
$b = ['x' => 'a', 'y' => 'b'];
print_r(array_map(fn($n, $s) => $n . ($s ?? '-'), $a, $b));
print_r(array_map(null, $a, $b));
print_r(array_map(function ($x, $y, $z) { return [$x, $y, $z]; }, [1], [2, 3], ['k' => 4]));
function bigger($a, $b) { return max($a, $b); }
$cb = 'bigger';
print_r(array_map($cb, [1, 9], [5, 2]));
print_r(array_map(fn($v) => $v * 2, ['p' => 1, 'q' => 2]));
eval('print_r(array_map(fn($a, $b) => $a + $b, [1, 2], [10, 20, 30]));');
"###);
    assert_eq!(out, "Array\n(\n    [0] => 1a\n    [1] => 2b\n    [2] => 3-\n)\nArray\n(\n    [0] => Array\n        (\n            [0] => 1\n            [1] => a\n        )\n\n    [1] => Array\n        (\n            [0] => 2\n            [1] => b\n        )\n\n    [2] => Array\n        (\n            [0] => 3\n            [1] => \n        )\n\n)\nArray\n(\n    [0] => Array\n        (\n            [0] => 1\n            [1] => 2\n            [2] => 4\n        )\n\n    [1] => Array\n        (\n            [0] => \n            [1] => 3\n            [2] => \n        )\n\n)\nArray\n(\n    [0] => 5\n    [1] => 9\n)\nArray\n(\n    [p] => 2\n    [q] => 4\n)\nArray\n(\n    [0] => 11\n    [1] => 22\n    [2] => 30\n)\n");
}

/// A call through an interface may pass more arguments than the interface declares, when an
/// implementation declares them.
///
/// php dispatches on the runtime class and binds what it declares; the checker judged the call
/// against the interface alone and refused `PasswordHasherInterface::hash($plain, $salt)`, which
/// every Symfony password listener writes.
#[test]
fn test_interface_call_may_pass_what_an_implementation_declares() {
    let out = compile_and_run(r###"<?php
interface Hasher { public function hash(string $plain): string; public function verify(string $hashed, string $plain): bool; }
class Legacy implements Hasher {
    public function hash(string $plain, ?string $salt = null): string { return $plain . ':' . ($salt ?? 'none'); }
    public function verify(string $hashed, string $plain, ?string $salt = null): bool { return $hashed === $this->hash($plain, $salt); }
}
class Modern implements Hasher {
    public function hash(string $plain): string { return 'm' . $plain; }
    public function verify(string $hashed, string $plain): bool { return $hashed === 'm' . $plain; }
}
function run(Hasher $h): string {
    $hashed = $h->hash('x', 'NaCl');
    return $hashed . ' ' . var_export($h->verify($hashed, 'x', 'NaCl'), true);
}
echo run(new Legacy()), "\n", run(new Modern()), "\n";
"###);
    assert_eq!(out, "x:NaCl true\nmx true\n");
}

/// `$this->prop[] = $v` on an `ArrayAccess` property runs `offsetSet(null, $v)`, and
/// `$list[$i]->prop[] = $v` on a gradual receiver is resolved at run time.
///
/// Both were refused by the checker (league/commonmark's block parsers hold an `ArrayCollection`;
/// Doctrine's `InsertBatch` pushes through an array element), and the append form dispatched an
/// `append()` method only the SPL containers define.
#[test]
fn test_property_push_reaches_array_access_and_gradual_receivers() {
    let out = compile_and_run(r###"<?php
class Coll implements ArrayAccess, Countable
{
    private array $items = [];
    public function offsetExists(mixed $offset): bool { return isset($this->items[$offset]); }
    public function offsetGet(mixed $offset): mixed { return $this->items[$offset]; }
    public function offsetSet(mixed $offset, mixed $value): void { if ($offset === null) { $this->items[] = $value; } else { $this->items[$offset] = $value; } }
    public function offsetUnset(mixed $offset): void { unset($this->items[$offset]); }
    public function count(): int { return count($this->items); }
    public function all(): array { return $this->items; }
}
class Parser
{
    private Coll $strings;
    public function __construct() { $this->strings = new Coll(); }
    public function add(string $line): void { $this->strings[] = $line; }
    public function lines(): array { return $this->strings->all(); }
}
class Batch { public array $entities = []; public function __construct(public string $type) {} }
function batch(array $entities): array
{
    $batches = [];
    $batchIndex = -1;
    foreach ($entities as $entity) {
        if ($batchIndex < 0 || $batches[$batchIndex]->type !== $entity[0]) {
            $batches[] = new Batch($entity[0]);
            $batchIndex++;
            $batches[$batchIndex]->entities[] = $entity[1];
            continue;
        }
        $batches[$batchIndex]->entities[] = $entity[1];
    }
    return $batches;
}
$p = new Parser();
$p->add('a');
$p->add('b');
echo implode(',', $p->lines()), "\n";
foreach (batch([['A', 1], ['A', 2], ['B', 3]]) as $b) {
    echo $b->type, ':', implode(',', $b->entities), "\n";
}
"###);
    assert_eq!(out, "a,b\nA:1,2\nB:3\n");
}

/// What a `continue`d iteration wrote reaches the code after the loop.
///
/// The checker joined `break` environments into the loop exit but dropped `continue` ones, so a
/// function whose only push sits in a branch that `continue`s returned an array typed empty, and
/// the caller refused `$b->type` on its elements (Doctrine's `InsertBatch::batchByEntityType()`).
#[test]
fn test_continue_carries_its_branch_types_out_of_the_loop() {
    let out = compile_and_run(r###"<?php
class Batch { public array $entities = []; public function __construct(public string $type) {} }
function batch(array $entities): array
{
    $batches = [];
    $batchIndex = -1;
    foreach ($entities as $entity) {
        if ($batchIndex < 0 || $batches[$batchIndex]->type !== $entity[0]) {
            $batches[] = new Batch($entity[0]);
            $batchIndex++;
            $batches[$batchIndex]->entities[] = $entity[1];
            continue;
        }
        $batches[$batchIndex]->entities[] = $entity[1];
    }
    return $batches;
}
$r = batch([['A', 1], ['A', 2], ['B', 3]]);
foreach ($r as $b) {
    echo $b->type, ':', implode(',', $b->entities), "\n";
}
"###);
    assert_eq!(out, "A:1,2\nB:3\n");
}

/// `array_map()` over an untyped `array` property hands back the callback's objects.
///
/// The builtin keeps a Mixed result slot for an untyped source and widens every entry to a Mixed
/// cell in key-preserving hash storage, while the checker proved `array<Ed>`: the typed caller read
/// each cell pointer as the object (`get_class()` named an unrelated class) and `foreach` walked
/// the hash as packed slots (Doctrine's `Table::getIndexes()` editors, "read property on null").
#[test]
fn test_array_map_over_an_untyped_property_yields_the_callback_objects() {
    let out = compile_and_run(r###"<?php
final class Ed { public function __construct(public array $cols) {} }
final class Index {
    public function __construct(private array $cols) {}
    public function edit(): Ed { return new Ed($this->cols); }
}
final class Table {
    public function __construct(private array $indexes) {}
    public function editors(): array { return array_map(static fn (Index $i): Ed => $i->edit(), $this->indexes); }
    public function keyed(): array { return array_map(static fn (Index $i): Ed => $i->edit(), ['p' => $this->indexes[0]]); }
}
$t = new Table([new Index(['a', 'b']), new Index(['c'])]);
$eds = $t->editors();
echo count($eds), ' ', get_class($eds[1]), "\n";
foreach ($eds as $k => $e) { echo $k, ':', implode(',', $e->cols), "\n"; }
foreach ($t->editors() as $e) { echo implode(',', $e->cols), ' '; }
echo "\n";
foreach ($t->keyed() as $k => $e) { echo $k, '=', implode(',', $e->cols), "\n"; }
"###);
    assert_eq!(out, "2 Ed\n0:a,b\n1:c\na,b c \np=a,b\n");
}

/// A call through a nullable interface-typed receiver supplies an implementation's extra defaults.
///
/// `?ContainerInterface $resolvers` holds its object in a Mixed cell, and a call on it went through
/// the interface table with the interface's one argument: Symfony's
/// `Container::get(string $id, int $invalidBehavior = ...)` read its second parameter from a stale
/// register. The non-null branch now unboxes the receiver so the override ladder pads it.
#[test]
fn test_a_nullable_interface_receiver_pads_the_implementations_defaults() {
    let out = compile_and_run(r###"<?php
interface PsrLike { public function get(string $id); public function has(string $id): bool; }
interface DiLike extends PsrLike {
    public const EXCEPTION = 1;
    public function get(string $id, int $invalidBehavior = self::EXCEPTION): ?object;
}
class Container implements DiLike {
    public function get(string $id, int $invalidBehavior = self::EXCEPTION): ?object {
        $o = new stdClass();
        $o->id = $id . ':' . $invalidBehavior;
        return $o;
    }
    public function has(string $id): bool { return $id !== ''; }
}
class Plain implements PsrLike {
    public function get(string $id) { return 'plain:' . $id; }
    public function has(string $id): bool { return true; }
}
final class Resolver {
    public function __construct(private ?PsrLike $named = null) {}
    public function resolve(string $name): mixed {
        if (!$this->named->has($name)) {
            return null;
        }
        return $this->named->get($name);
    }
}
echo (new Resolver(new Container()))->resolve('b')->id, "\n";
echo (new Resolver(new Plain()))->resolve('c'), "\n";
function pick(int $n): ?PsrLike { return $n === 1 ? new Container() : new Plain(); }
$c = pick(1);
echo $c->get('local')->id, "\n";
$total = 0;
for ($i = 0; $i < 6; $i++) {
    $p = pick(1 + ($i % 2));
    $r = $p->get('k');
    $total += is_object($r) ? strlen($r->id) : strlen($r);
}
echo $total, "\n";
"###);
    assert_eq!(out, "b:1\nplain:c\nlocal:1\n30\n");
}

/// `+` over two lists of different element types keeps each side's values.
///
/// The union was typed with the left list's element type, so a string list unioned with a longer
/// int/float/bool list printed the right side's surplus as strings of garbage (Monolog's
/// `Level::NAMES + Level::VALUES`-style constant tables).
#[test]
fn test_array_union_of_differently_typed_lists_keeps_both_element_types() {
    let out = compile_and_run(r###"<?php
enum Level: int {
    case Debug = 100; case Info = 200; case Error = 400;
    public const NAMES = ['DEBUG', 'INFO', 'ERROR'];
    public const VALUES = [100, 200, 400, 500];
}
echo implode(', ', Level::NAMES + Level::VALUES), "\n";
$a = ['x', 'y'];
$b = [1, 2, 3.5, true];
var_dump($a + $b);
function names(array $l, array $r): string { return implode('|', $l + $r); }
echo names([1], ['a', 'b']), "\n";
"###);
    assert_eq!(out, "DEBUG, INFO, ERROR, 500\narray(4) {\n  [0]=>\n  string(1) \"x\"\n  [1]=>\n  string(1) \"y\"\n  [2]=>\n  float(3.5)\n  [3]=>\n  bool(true)\n}\n1|b\n");
}

/// `$this->$name = null` on a declared non-nullable property throws php's `TypeError`.
///
/// A runtime-named write stored into the typed slot without checking it, so Doctrine's
/// `FieldMapping` (an `ArrayAccess` over its own properties) nulled a `string` property that later
/// reads returned as an empty value instead of refusing the write.
#[test]
fn test_a_dynamic_property_write_honours_the_declared_property_type() {
    let out = compile_and_run(r###"<?php
class FieldMapping implements ArrayAccess {
    public string $type = 'string';
    public ?int $length = null;
    public bool $nullable = false;
    public function offsetExists(mixed $offset): bool { return isset($this->$offset); }
    public function offsetGet(mixed $offset): mixed { return $this->$offset; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->$offset = $value; }
    public function offsetUnset(mixed $offset): void { $this->$offset = null; }
}
$m = new FieldMapping();
unset($m['length']);
var_dump($m['length']);
try {
    unset($m['type']);
} catch (TypeError $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
echo $m['type'], "\n";
"###);
    assert_eq!(out, "NULL\nTypeError: Cannot assign null to property FieldMapping::$type of type string\nstring\n");
}

/// A method called with a spread argument list returns its object to the caller.
///
/// The descriptor invoker that runs a spread call stored an object result as the Mixed box
/// itself, so `$e->setColumns(...cols())` returned a pointer that was not `$e`.
#[test]
fn test_a_spread_call_returning_self_hands_back_the_object() {
    let out = compile_and_run(r###"<?php
final class IndexEditor {
    public array $cols = [];
    public function setColumns(string ...$c): self { $this->cols = $c; return $this; }
    public function count(string ...$c): int { return count($c); }
}
function cols(array $a): array { return $a; }
$e = new IndexEditor();
echo $e->count(...cols(['a', 'b'])), "\n";
$r = $e->setColumns(...cols(['a', 'b']));
var_dump($r === $e, $r->cols);
"###);
    assert_eq!(out, "2\nbool(true)\narray(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n");
}

/// A call through an interface reaches implementations that declare more, and fewer, parameters.
///
/// `Psr\Container\ContainerInterface::get($id)` is implemented by Symfony's
/// `Container::get($id, $invalidBehavior = 1)`, by `ContainerBag::get($name)` and by anonymous
/// locators. A site lowered against the widest implementation passed the default along: the
/// nullable-receiver path refused the whole method ("3 operands for 2 ABI params"), the class-id
/// dispatch dropped the one-parameter implementations ("Call to undefined method
/// ServiceLocator::get()"), and the wide one read its default from a stale register.
#[test]
fn test_a_narrowed_interface_call_reaches_every_implementation_with_its_own_arity() {
    let out = compile_and_run(r###"<?php
interface PsrLike { public function get(string $id); public function has(string $id): bool; }
interface DiLike extends PsrLike {
    public const EXCEPTION = 1;
    public function get(string $id, int $invalidBehavior = self::EXCEPTION): ?object;
}
interface BagLike extends PsrLike { public function all(): array; }
class Resetter { public function reset(): void { echo "reset\n"; } }
class Container implements DiLike {
    public function get(string $id, int $invalidBehavior = self::EXCEPTION): ?object { echo "get $id $invalidBehavior\n"; return new Resetter(); }
    public function has(string $id): bool { return true; }
}
class Builder extends Container {
    public function get(string $id, int $invalidBehavior = DiLike::EXCEPTION): ?object { echo "builder $id $invalidBehavior\n"; return new Resetter(); }
}
class Bag implements BagLike {
    public function get(string $name): array|bool|string|int|float|null { return 'bag:' . $name; }
    public function has(string $name): bool { return true; }
    public function all(): array { return []; }
}
final class App {
    public function __construct(private ?PsrLike $container = null) {}
    public function reset(): void
    {
        if ($this->container?->has('services_resetter')) {
            $r = $this->container->get('services_resetter');
            if (is_object($r)) { $r->reset(); } else { echo $r, "\n"; }
        }
    }
    public function ids(): mixed
    {
        return !$this->container instanceof DiLike ? ($this->container->has('ids') ? $this->container->get('ids') : []) : 'di';
    }
}
function fetch(PsrLike $c): mixed { return $c->get('f'); }
$anon = new class() implements PsrLike {
    public function get(string $id): string { return 'anon:' . $id; }
    public function has(string $id): bool { return true; }
};
foreach ([new Container(), new Builder(), new Bag(), $anon] as $c) {
    (new App($c))->reset();
    $x = (new App($c))->ids();
    echo is_string($x) ? $x : get_debug_type($x), "\n";
    $f = fetch($c);
    echo is_string($f) ? $f : get_class($f), "\n";
}
(new App())->reset();
"###);
    assert_eq!(out, "get services_resetter 1\nreset\ndi\nget f 1\nResetter\nbuilder services_resetter 1\nreset\ndi\nbuilder f 1\nResetter\nbag:services_resetter\nbag:ids\nbag:f\nanon:services_resetter\nanon:ids\nanon:f\n");
}

/// `array_keys($array, $filter_value[, $strict])` lists only the keys whose value matches.
///
/// The builtin took exactly one argument, so Doctrine's
/// `BasicEntityPersister::getSelectConditionStatementSQL()` (`array_keys($value, null, true)`) kept
/// the entity persister -- every query goes through it -- in the interpreter. The search form is a
/// prelude helper now.
#[test]
fn test_array_keys_search_form_lists_only_matching_keys() {
    let out = compile_and_run(r###"<?php
$value = ['a' => 1, 'b' => null, 'c' => '1', 'd' => null, 5 => 0];
$nullKeys = array_keys($value, null, true);
var_dump($nullKeys);
var_dump(array_keys($value, 1));
var_dump(array_keys($value, '1', true));
var_dump(array_keys([3, 1, 3], 3));
var_dump(array_diff_key($value, array_flip($nullKeys)));
"###);
    assert_eq!(out, "array(2) {\n  [0]=>\n  string(1) \"b\"\n  [1]=>\n  string(1) \"d\"\n}\narray(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"c\"\n}\narray(1) {\n  [0]=>\n  string(1) \"c\"\n}\narray(2) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(2)\n}\narray(3) {\n  [\"a\"]=>\n  int(1)\n  [\"c\"]=>\n  string(1) \"1\"\n  [5]=>\n  int(0)\n}\n");
}

/// `array_sum()` over a gradual operand and `array_map('spl_object_id', ...)` over untyped storage.
///
/// `array_map()` over an untyped array keeps a Mixed result slot, which `array_sum()` had no
/// lowering for (Doctrine's `Paginator::count()`), and `spl_object_id` was not a runtime string
/// callback although it accepts any object (Doctrine's `PersistentCollection::getDeleteDiff()`).
#[test]
fn test_gradual_array_aggregates_and_object_id_callbacks() {
    let out = compile_and_run(r###"<?php
final class Row { public function __construct(public int $n) {} }
final class Coll {
    private array $snapshot = [];
    private $rows = [];
    public function take(array $items): void { $this->snapshot = $items; $this->rows = [['n' => 3], ['n' => '4.5']]; }
    public function ids(): array { return array_map('spl_object_id', $this->snapshot); }
    public function total(): int|float { return array_sum(array_map(fn ($r) => $r['n'], $this->rows)); }
    public function product(): int|float { return array_product(array_map(fn ($r) => $r['n'], $this->rows)); }
}
$rows = [new Row(1), new Row(2)];
$c = new Coll();
$c->take($rows);
var_dump(count($c->ids()) === 2, $c->ids()[1] === spl_object_id($rows[1]));
var_dump($c->total(), $c->product());
"###);
    assert_eq!(out, "bool(true)\nbool(true)\nfloat(7.5)\nfloat(13.5)\n");
}

/// `$a->b->list[] = $v` compiles when `$a->b` is a union of object types.
///
/// A property chain keeps its declared union after an `instanceof` test the checker does not
/// narrow through, and the push was refused ("Array push requires an object or typed pointer"):
/// Doctrine's `WhereInWalker` -- the paginator -- went to the interpreter.
#[test]
fn test_a_push_through_an_object_union_receiver_is_a_runtime_dispatch() {
    let out = compile_and_run(r###"<?php
class Primary { public function __construct(public string $n = 'p') {} }
class Term { public array $factors = []; }
class Where { public function __construct(public Term|Primary $cond) {} }
class Select { public ?Where $where = null; }
$s = new Select();
$s->where = new Where(new Term());
if ($s->where->cond instanceof Term) {
    $s->where->cond->factors[] = new Primary('a');
    $s->where->cond->factors[] = new Primary('b');
} elseif ($s->where->cond instanceof Primary) {
    echo "primary\n";
}
$c = $s->where->cond;
if ($c instanceof Term) {
    echo count($c->factors), ' ', $c->factors[1]->n, "\n";
}
"###);
    assert_eq!(out, "2 b\n");
}

/// `f($GLOBALS['x'])` inside eval passes the global's value.
///
/// Call arguments that are array elements are evaluated as places (a by-reference parameter may
/// bind them), and that path looked `$GLOBALS` up as a LOCAL named `GLOBALS`: the argument arrived
/// as null while `$v = $GLOBALS['x']` in the same eval read the value.
#[test]
fn test_eval_passes_a_globals_element_as_a_call_argument() {
    let out = compile_and_run(r###"<?php
final class Box { public function __construct(public string $v) {} }
$GLOBALS['loc'] = new Box('a');
$GLOBALS['n'] = 5;
eval('function f1($c) { return get_debug_type($c); } echo f1($GLOBALS["n"]), "\n";');
eval('echo f1($GLOBALS["loc"]), "\n";');
function g($c) { return get_debug_type($c); }
eval('echo g($GLOBALS["loc"]), " ", strlen($GLOBALS["loc"]->v), "\n";');
"###);
    assert_eq!(out, "int\nBox\nBox 1\n");
}

/// `fn ($r) => $r['n']` returns the element as it is, float or string included.
///
/// The closure's return type came from the syntactic default for a direct `return $r[...]`, which
/// is `int`, and the body converted the element on its way out: `array_map(fn ($r) => $r['n'],
/// $rows)` over `[['n' => 3], ['n' => 4.5]]` answered `[3, 4]`.
#[test]
fn test_a_closure_returning_an_element_of_an_untyped_parameter_keeps_its_type() {
    let out = compile_and_run(r###"<?php
$rows = [['n' => 3], ['n' => 4.5], ['n' => 'x']];
var_dump(array_map(fn ($r) => $r['n'], $rows));
$g = function ($r) { return $r['n']; };
var_dump($g(['n' => 2.5]));
$k = function (mixed $r) { return $r[0]; };
var_dump($k(['a', 'b']), $k('zq'));
$m = function (array $r) { return $r['n']; };
var_dump($m(['n' => null]));
"###);
    assert_eq!(out, "array(3) {\n  [0]=>\n  int(3)\n  [1]=>\n  float(4.5)\n  [2]=>\n  string(1) \"x\"\n}\nfloat(2.5)\nstring(1) \"a\"\nstring(1) \"z\"\nNULL\n");
}

/// A call through an `object` receiver reaches a method whose body reads `func_get_args()`.
///
/// Such a body receives a hidden variadic parameter, and the runtime class dispatch required a
/// default for every omitted parameter: the variadic tail has none, so the method left the
/// candidate list. Symfony's `ServiceLocator::get()` reads `func_get_args()`, and
/// `Container::getEnv()` died "Call to undefined method ServiceLocator::get()" on every request.
#[test]
fn test_runtime_dispatch_reaches_a_method_that_reads_func_get_args() {
    let out = compile_and_run(r###"<?php
class Locator {
    public function get(string $id): mixed { return $id . ':' . count(func_get_args()); }
    public function has(string $id): bool { return true; }
}
class Other {
    public function get(string $id): string { return 'other:' . $id; }
    public function has(string $id): bool { return false; }
}
class Pool {
    public function all(string ...$names): string { return implode(',', $names) . '#' . count($names); }
}
function pick(int $i): ?object { return [new Locator(), new Other(), new Pool()][$i]; }
function fetch(?object $o, string $id): mixed { return $o->has($id) ? $o->get($id) : $o->get($id . '!'); }
echo fetch(pick(0), 'a'), ' ', fetch(pick(1), 'b'), "\n";
$m = [new Locator(), new Other()];
foreach ($m as $x) { echo $x->get('m'), ' '; }
echo "\n";
$p = pick(2);
echo $p->all(), ' ', $p->all('x', 'y'), "\n";
"###);
    assert_eq!(out, "a:1 other:b!\nm:1 other:m \n#0 x,y#2\n");
}

/// A call through an interface passes the argument array a `func_get_args()` implementation needs.
///
/// An implementation that reads `func_get_args()` gives the whole method family a hidden variadic
/// tail, the interface's table signature included. A nullable interface receiver lowered its call
/// without a signature (the tail counted as a required parameter, and the implementors' return
/// types disagreed), and the backend refused two operands for three ABI parameters: Symfony's
/// `ArgumentResolver`, `SessionListener` and console `Application` went to the interpreter.
#[test]
fn test_an_interface_call_packs_the_hidden_tail_an_implementation_reads() {
    let out = compile_and_run(r###"<?php
interface PsrLike { public function get(string $id); public function has(string $id): bool; }
class Locator implements PsrLike {
    public function get(string $id): mixed { return $id . ':' . count(func_get_args()); }
    public function has(string $id): bool { return true; }
}
class Plain implements PsrLike {
    public function get(string $id): string { return 'plain:' . $id; }
    public function has(string $id): bool { return true; }
}
final class Resolver {
    public function __construct(private ?PsrLike $named = null) {}
    public function resolve(string $name): mixed {
        if (!$this->named->has($name)) {
            return null;
        }
        return $this->named->get($name);
    }
}
echo (new Resolver(new Locator()))->resolve('a'), ' ', (new Resolver(new Plain()))->resolve('b'), "\n";
function via(PsrLike $c): mixed { return $c->get('v'); }
echo via(new Locator()), ' ', via(new Plain()), "\n";
"###);
    assert_eq!(out, "a:1 plain:b\nv:1 plain:v\n");
}

/// `json_encode($data, $flags)` with a `?int` flags local, and `array_map()` through a `?Closure`.
///
/// Monolog's `Utils::jsonEncode()` defaults a `?int $encodeFlags` before encoding, and the backend
/// refused tagged-scalar flags; its `LineFormatter` maps through the `?Closure` property
/// `$this->stacktracesParser`, which the native map runtime cannot bind. Both classes went to the
/// interpreter, and a compiled caller of `Utils` then died "Class Monolog\Utils not found".
#[test]
fn test_json_flags_from_a_nullable_int_and_a_runtime_selected_map_callback() {
    let out = compile_and_run(r###"<?php
final class Utils {
    public const DEFAULT_JSON_FLAGS = JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_PRESERVE_ZERO_FRACTION;
    public static function jsonEncode($data, ?int $encodeFlags = null, bool $ignoreErrors = false): string
    {
        if (null === $encodeFlags) {
            $encodeFlags = self::DEFAULT_JSON_FLAGS;
        }
        if ($ignoreErrors) {
            $json = @json_encode($data, $encodeFlags);
            return false === $json ? 'null' : $json;
        }
        $json = json_encode($data, $encodeFlags);
        return false === $json ? 'err' : $json;
    }
}
final class Formatter {
    private ?Closure $parser = null;
    private $untyped = null;
    public function withParser(?Closure $p): self { $this->parser = $p; $this->untyped = $p; return $this; }
    public function parse(string $trace): string {
        return implode('|', array_filter(array_map($this->parser, explode("\n", $trace)), fn ($l) => is_string($l) && trim($l) !== ''));
    }
    public function keyed(array $rows): array { return array_map($this->untyped, $rows); }
}
echo Utils::jsonEncode(['a/b' => 1.0, 'é' => null]), "\n";
echo Utils::jsonEncode(['x' => 'y/z'], JSON_PRETTY_PRINT), "\n";
echo Utils::jsonEncode([1, 2], null, true), "\n";
$f = (new Formatter())->withParser(fn ($line) => $line === 'skip' ? null : strtoupper($line));
echo $f->parse("a\nskip\nb"), "\n";
var_dump($f->keyed(['k' => 'v', 3 => 'w']));
var_dump((new Formatter())->keyed(['k' => 'v']));
"###);
    assert_eq!(out, "{\"a/b\":1.0,\"é\":null}\n{\n    \"x\": \"y\\/z\"\n}\n[1,2]\nA|B\narray(2) {\n  [\"k\"]=>\n  string(1) \"V\"\n  [3]=>\n  string(1) \"W\"\n}\narray(1) {\n  [\"k\"]=>\n  string(1) \"v\"\n}\n");
}

/// A runtime-dispatched call packs the arguments past a `func_num_args()` method's parameters.
///
/// Twig's `Node::getAttribute($name)` reads an optional second argument through
/// `func_num_args()`/`func_get_arg(1)`. Called through a gradual receiver with that argument, the
/// class dispatch passed the raw `false` where the hidden variadic array belongs (`count()` then
/// failed on it), and without it the dispatch refused to stage the missing tail at all.
#[test]
fn test_runtime_dispatch_packs_surplus_arguments_into_the_hidden_tail() {
    let out = compile_and_run(r###"<?php
class Node {
    protected array $attributes = [];
    public function __construct(array $nodes = [], array $attributes = [], int $lineno = 0)
    {
        if (\func_num_args() > 3) { echo "tag given\n"; }
        $this->attributes = $attributes;
    }
    public function getAttribute(string $name)
    {
        $triggerDeprecation = \func_num_args() > 1 ? func_get_arg(1) : true;
        return ($this->attributes[$name] ?? 'none') . ($triggerDeprecation ? '' : '(quiet)');
    }
}
class NameNode extends Node {}
class Other { public function getAttribute(string $name): string { return 'other:' . $name; } }
function pick(int $i): mixed { return [new NameNode([], ['name' => 'x', 'global' => true]), new Other()][$i]; }
foreach ([0, 1] as $i) {
    $ref = pick($i);
    echo $ref->getAttribute('name'), ' ', $ref->getAttribute('name', false), "\n";
}
$n = new NameNode([], ['name' => 'y'], 3, 'tag');
echo $n->getAttribute('name'), "\n";
"###);
    assert_eq!(out, "x x(quiet)\nother:name other:name\ntag given\ny\n");
}

/// `array_filter($list)` inside a function whose `array` parameter received hash storage.
///
/// A gradual value passed to an `array` parameter becomes a hash that keeps its keys, and the
/// callee's `array<mixed>` code must dispatch on the storage kind. `array_filter()` without a
/// callback walked the hash as packed Mixed slots: Symfony's `LocaleListener::__construct()`,
/// given the container parameter `kernel.enabled_locales`, crashed every request.
#[test]
fn test_array_filter_over_a_hash_backed_array_parameter() {
    let out = compile_and_run(r###"<?php
class LocaleListener {
    public function __construct(
        private string $defaultLocale = 'en',
        private bool $useAcceptLanguageHeader = false,
        private array $enabledLocales = [],
    ) {
        $this->enabledLocales = $enabledLocales ? array_values(array_unique(array_merge([$defaultLocale], array_filter($enabledLocales)))) : [];
    }
    public function locales(): array { return $this->enabledLocales; }
}
class Container {
    protected array $parameters = [];
    public function __construct() { $this->parameters = $this->getDefaultParameters(); }
    protected function getDefaultParameters(): array
    {
        return [
            'kernel.enabled_locales' => ['ar', 'en', 'fr', '', 'de'],
            'kernel.keyed' => ['a' => 'x', 'b' => '', 'c' => 0, 'd' => 'y'],
            'kernel.debug' => false,
        ];
    }
    public function make(): LocaleListener
    {
        return new LocaleListener('en', false, $this->parameters['kernel.enabled_locales']);
    }
    public function keyed(): array { return self::filtered($this->parameters['kernel.keyed']); }
    private static function filtered(array $values): array { return array_filter($values); }
}
$c = new Container();
echo implode(',', $c->make()->locales()), "\n";
var_dump($c->keyed());
"###);
    assert_eq!(out, "en,ar,fr,de\narray(2) {\n  [\"a\"]=>\n  string(1) \"x\"\n  [\"d\"]=>\n  string(1) \"y\"\n}\n");
}

/// A gradual value holding a list reaches an `array` parameter as a list every builtin can read.
///
/// The gradual boundary converts the value to hash storage, and a hash whose keys are 0..n-1 was
/// handed over as it was: `in_array()`, `max()`/`min()` and friends read its buckets as packed
/// slots and crashed. It is packed at the call now, which is the same PHP value.
#[test]
fn test_a_list_shaped_hash_reaches_an_array_parameter_as_a_list() {
    let out = compile_and_run(r###"<?php
function probe(array $v): void
{
    var_dump(in_array('fr', $v, true), in_array('xx', $v), array_search('de', $v));
    echo max($v), ' ', min($v), ' ', count($v), "\n";
    echo implode(',', array_map(fn ($x) => strtoupper($x), $v)), ' ', implode(',', array_slice($v, 1, 2)), "\n";
    echo implode(',', array_unique($v)), ' ', implode(',', array_reverse($v)), ' ', json_encode($v), "\n";
    foreach ($v as $k => $x) { echo $k, '=', $x, ';'; }
    echo "\n";
    $w = $v; sort($w); echo implode(',', $w), ' ', implode(',', array_merge($v, ['zz'])), "\n";
    var_dump(array_key_first($v), array_key_last($v), array_is_list($v));
}
class Box {
    protected array $parameters = [];
    public function __construct() { $this->parameters = ['list' => ['ar', 'en', 'fr', 'de'], 'map' => ['x' => 'en', 'y' => 'fr']]; }
    public function run(): void { probe($this->parameters['list']); }
    public function keys(): array { return array_keys($this->parameters['map']); }
}
(new Box())->run();
echo implode(',', (new Box())->keys()), "\n";
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nint(3)\nfr ar 4\nAR,EN,FR,DE en,fr\nar,en,fr,de de,fr,en,ar [\"ar\",\"en\",\"fr\",\"de\"]\n0=ar;1=en;2=fr;3=de;\nar,de,en,fr ar,en,fr,de,zz\nint(0)\nint(3)\nbool(true)\nx,y\n");
}

/// An override that declares MORE fixed parameters than an ancestor whose body reads
/// `func_get_arg()` is called through the ancestor's type with its own parameters bound.
///
/// `func_get_args()` gives the whole method family a hidden variadic tail, at position 1 in
/// `Node::getAttribute(string $name)` and position 2 in the override `getAttribute($name,
/// $default = null)`. The override adopted the tail's array storage for `$default`, and a call
/// typed against the ancestor packed its surplus where the override read `$default` (Twig's
/// `SupportDefinedTestDeprecationTrait`).
#[test]
fn test_override_wider_than_a_func_get_args_ancestor_binds_its_parameters() {
    let out = compile_and_run(r###"<?php
class Node {
    protected array $attributes = [];
    public function __construct(array $a) { $this->attributes = $a; }
    public function getAttribute(string $name) {
        $flag = func_num_args() > 1 ? func_get_arg(1) : true;
        return $this->attributes[$name] . ($flag ? "+" : "-");
    }
}
trait Deprecating {
    public function getAttribute($name, $default = null) {
        if ('x' === $name) { return 'x!' . var_export($default, true); }
        return parent::getAttribute($name, $default);
    }
}
abstract class Expr extends Node {}
class Constant extends Expr {}
class Name extends Expr { use Deprecating; }
function show(Node $n): void {
    echo $n->getAttribute('value'), " ", $n->getAttribute('value', false), " ", $n->getAttribute('x', 7), " | ";
    echo $n->getAttribute('x'), "\n";
}
foreach ([new Constant(['value' => 'c', 'x' => 'cx']), new Name(['value' => 'n'])] as $node) {
    show($node);
}
$name = new Name(['value' => 'n2']);
echo $name->getAttribute('value'), $name->getAttribute('value', 0), $name->getAttribute('x', 'd'), "\n";
"###);
    assert_eq!(out, "c+ c- cx+ | cx+\nn- n- x!7 | x!NULL\nn2-n2-x!'d'\n");
}

/// A variadic override receives the arguments of a call typed against its narrower ancestor.
///
/// The class-id ladder that pads a wider override's defaults skipped every variadic signature,
/// so the override read its packed tail from whatever the register held.
#[test]
fn test_variadic_override_receives_arguments_through_the_base_type() {
    let out = compile_and_run(r###"<?php
class Base {
    public function bar($a) { return "base:$a"; }
    public function baz(string $a, ...$rest) { return "base:$a:" . count($rest); }
}
class Wide extends Base {
    public function bar($a, ...$rest) { return "wide:$a:" . count($rest) . ":" . implode(",", $rest); }
    public function baz(string $a, $b = 'B', ...$rest) { return "wide:$a:$b:" . count($rest); }
}
class Narrow extends Base {
    public function baz(string $a, ...$more) { return "narrow:$a:" . implode(",", $more); }
}
function run(Base $o): void {
    echo $o->bar('x'), " ", $o->bar('x', 1, 2), "\n";
    echo $o->baz('y'), " ", $o->baz('y', 'z'), " ", $o->baz('y', 'z', 3, 4), "\n";
}
foreach ([new Base(), new Wide(), new Narrow()] as $o) {
    run($o);
}
"###);
    assert_eq!(out, "base:x base:x\nbase:y:0 base:y:1 base:y:3\nwide:x:0: wide:x:2:1,2\nwide:y:B:0 wide:y:z:0 wide:y:z:2\nbase:x base:x\nnarrow:y: narrow:y:z narrow:y:z,3,4\n");
}

/// `headers_sent($file, $line)` works inside eval and writes its by-reference outputs.
///
/// Only the no-argument form was joined to the shared runtime builtin; the two-argument shape
/// Symfony's `NativeSessionStorage::start()` uses was refused as an unsupported builtin.
#[test]
fn test_eval_headers_sent_binds_its_output_parameters() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
$file = 'unset';
$line = -1;
$sent = headers_sent($file, $line);
$sentToo = headers_sent($other);
$plain = headers_sent();
var_dump($sent, $file, $line, $sentToo, $other, $plain);
PHP);
"###);
    assert_eq!(out, "bool(false)\nstring(0) \"\"\nint(0)\nbool(false)\nstring(0) \"\"\nbool(false)\n");
}

/// An interpreted class may implement `ArrayAccess` without declaring return types.
///
/// A PHP-internal interface's return types are tentative, but the check that runs against the
/// host program's interface metadata demanded them, so dflydev's `Data` class could not be
/// declared at all.
#[test]
fn test_eval_class_implements_arrayaccess_without_return_types() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
class Data implements ArrayAccess
{
    protected $data = [];
    #[\ReturnTypeWillChange]
    public function offsetExists($key) { return isset($this->data[$key]); }
    #[\ReturnTypeWillChange]
    public function offsetGet($key) { return $this->data[$key] ?? null; }
    #[\ReturnTypeWillChange]
    public function offsetSet($key, $value) { $this->data[$key] = $value; }
    #[\ReturnTypeWillChange]
    public function offsetUnset($key) { unset($this->data[$key]); }
}
$d = new Data();
$d['a'] = 1;
var_dump(isset($d['a']), isset($d['b']), $d['a']);
unset($d['a']);
var_dump(isset($d['a']));
PHP);
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nint(1)\nbool(false)\n");
}

/// An interpreted `foreach` may bind its value to an appended element (`$list[]`).
///
/// The parser stopped in front of the empty `[]` and refused the statement, which is how
/// Symfony's `IsGrantedAttributeListener` collects attributes.
#[test]
fn test_eval_foreach_value_target_may_append() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
function attributes() { yield 'A' => 1; yield 'B' => 2; yield 'C' => 3; }
$attributes = [];
foreach (attributes() as $class => $attributes[]) {
    if ($class === 'B') {
        array_pop($attributes);
    }
}
var_dump($attributes);
$box = ['list' => ['x']];
foreach (['y', 'z'] as $box['list'][]) {}
echo implode(',', $box['list']), "\n";
PHP);
"###);
    assert_eq!(out, "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(3)\n}\nx,y,z\n");
}

/// A value read from a property, or placed into an array literal from a variable, is a COPY.
///
/// Interpreted stores shared the source's cell, so a later write through the copy landed in the
/// property or variable it came from: Doctrine's hydrator appended every row's id to its id
/// template and returned each fetch-joined entity once per row.
#[test]
fn test_eval_value_read_from_a_property_or_variable_is_copied_on_write() {
    let out = compile_and_run(r###"<?php
$code = <<<'PHP'
function g(array &$a) { $a['x'] .= 'y'; }
function h(array &$a) { $a['x'] = 'z'; }
function k(array &$a) { $a['n']++; }
function m(array &$a) { $a[] = 'app'; }
function byval(array $a) { $a["x"] = "bv"; return $a; }
$t = ['x' => '', 'n' => 1];
$c = $t; g($c); echo json_encode($t), json_encode($c), "\n";
$c = $t; h($c); echo json_encode($t), json_encode($c), "\n";
$c = $t; k($c); echo json_encode($t), json_encode($c), "\n";
$c = $t; m($c); echo json_encode($t), json_encode($c), "\n";
class P {
    private array $tpl = ['x' => '', 'n' => 1];
    public function run(): void {
        $c = $this->tpl; g($c); echo json_encode($this->tpl), json_encode($c), "\n";
        $c = $this->tpl; h($c); echo json_encode($this->tpl), json_encode($c), "\n";
        $c = $this->tpl; k($c); echo json_encode($this->tpl), json_encode($c), "\n";
        $c = $this->tpl; m($c); echo json_encode($this->tpl), json_encode($c), "\n";
        $d = $this->tpl; $d['x'] .= 'q'; echo json_encode($this->tpl), json_encode($d), "\n";
        byval($this->tpl); echo json_encode($this->tpl), "\n";
        $e = ["k" => $this->tpl]; $e["k"]["x"] = "w"; echo json_encode($this->tpl), json_encode($e), "\n";
    }
}
(new P())->run();
PHP;
eval($code);
"###);
    assert_eq!(out, "{\"x\":\"\",\"n\":1}{\"x\":\"y\",\"n\":1}\n{\"x\":\"\",\"n\":1}{\"x\":\"z\",\"n\":1}\n{\"x\":\"\",\"n\":1}{\"x\":\"\",\"n\":2}\n{\"x\":\"\",\"n\":1}{\"x\":\"\",\"n\":1,\"0\":\"app\"}\n{\"x\":\"\",\"n\":1}{\"x\":\"y\",\"n\":1}\n{\"x\":\"\",\"n\":1}{\"x\":\"z\",\"n\":1}\n{\"x\":\"\",\"n\":1}{\"x\":\"\",\"n\":2}\n{\"x\":\"\",\"n\":1}{\"x\":\"\",\"n\":1,\"0\":\"app\"}\n{\"x\":\"\",\"n\":1}{\"x\":\"q\",\"n\":1}\n{\"x\":\"\",\"n\":1}\n{\"x\":\"\",\"n\":1}{\"k\":{\"x\":\"w\",\"n\":1}}\n");
}

/// An `ArrayIterator` built by interpreted code is a real one for compiled code too.
///
/// The interpreter kept its own state on the side and handed back an uninitialized compiled
/// object, so `count()`, `foreach` and `iterator_to_array()` on Doctrine's paginator results
/// failed ("Typed property ArrayIterator::$values must not be accessed before initialization").
#[test]
fn test_eval_built_array_iterator_is_readable_by_compiled_code() {
    let out = compile_and_run(r###"<?php
$it = eval('return new ArrayIterator([3, 4]);');
echo get_class($it), "\n";
echo count($it), "\n";
foreach ($it as $k => $v) { echo "$k=$v "; }
echo "\n";
echo json_encode(iterator_to_array($it)), "\n";
$ao = eval('return new ArrayObject(["a" => 1]);');
echo count($ao), json_encode($ao->getArrayCopy()), "\n";
"###);
    assert_eq!(out, "ArrayIterator\n2\n0=3 1=4 \n[3,4]\n1{\"a\":1}\n");
}

/// `isset()`/`empty()` from compiled code on an interpreted object's private or protected
/// property answer "absent" instead of raising.
///
/// The bridge raised "Cannot access private property" even in the quiet fetch, and compiled
/// `empty($o->p)` did not enter the quiet fetch at all.
#[test]
fn test_isset_and_empty_on_an_inaccessible_eval_property_answer_quietly() {
    let out = compile_and_run(r###"<?php
$code = <<<'PHP'
class Post {
    private ?string $title = 'T';
    protected int $n = 1;
    public ?string $pub = 'P';
    public ?string $nul = null;
}
return new Post();
PHP;
$p = eval($code);
echo isset($p->title) ? "set" : "unset", "\n";
echo isset($p->n) ? "set" : "unset", "\n";
echo isset($p->pub) ? "set" : "unset", "\n";
echo isset($p->nul) ? "set" : "unset", "\n";
echo isset($p->nope) ? "set" : "unset", "\n";
$item = 'title';
echo isset($p->$item) ? "set" : "unset", "\n";
echo empty($p->title) ? "empty" : "nonempty", "\n";
$code2 = <<<'PHP'
$item = 'title';
echo isset($p->title) ? "set" : "unset", "|", isset($p->$item) ? "set" : "unset", "|", empty($p->n) ? "e" : "ne", "\n";
PHP;
eval($code2);
"###);
    assert_eq!(out, "unset\nunset\nset\nunset\nunset\nunset\nempty\nunset|unset|e\n");
}

/// A program that names `property_exists` only as a callable still links.
///
/// Its builtin wrapper is emitted during codegen and probes `__rt_member_exists`, but the
/// runtime feature scan only looked at direct calls, so the helper was never emitted
/// ("Undefined symbols: __rt_member_exists") -- Twig's `CoreExtension::getPropertyChecker()`.
#[test]
fn test_builtin_member_probe_named_as_a_callable_links() {
    let out = compile_and_run(r###"<?php
final class Core
{
    public static function methods(object $object): array
    {
        $methods = get_class_methods($object);
        sort($methods);
        return array_map('strtolower', $methods);
    }

    public static function checker(string $class, string $property): \Closure
    {
        static $propertyExists;
        if ('name' !== $property) {
            return $propertyExists ??= \Closure::fromCallable('property_exists');
        }
        return static fn () => true;
    }
}
class Item { public $name = 'n'; public function getName() {} public function isOk() {} }
eval('return 1;');
echo implode(',', Core::methods(new Item())), "\n";
$exists = Core::checker(Item::class, 'nope');
var_dump($exists(new Item(), 'name'), $exists(new Item(), 'nope'), $exists('Item', 'name'));
"###);
    assert_eq!(out, "getname,isok\nbool(true)\nbool(false)\nbool(true)\n");
}

/// A `static $cache = []` declared after a `goto` label memoizes on every path.
///
/// The label's region is lowered once per incoming path, and the copies without flow facts read
/// the static's hash as a Mixed cell: the memo never hit and the method table was rebuilt on
/// every call (Twig's `CoreExtension::getAttribute()` jumps to `methodCheck:`).
#[test]
fn test_static_memo_declared_after_a_goto_label_keeps_its_hash() {
    let out = compile_and_run(r###"<?php
final class Resolver
{
    public static function resolve(object $object, string $item, bool $skipProperty): string
    {
        if (!$skipProperty) {
            if (isset($object->$item)) {
                return 'property:' . $object->$item;
            }
            if ($item === 'jump') {
                goto methodCheck;
            }
        }

        methodCheck:

        static $cache = [];
        $class = $object::class;
        if (!isset($cache[$class])) {
            $methods = get_class_methods($object);
            sort($methods);
            $classCache = [];
            foreach ($methods as $method) {
                $classCache[$method] = $method;
                $classCache[strtolower($method)] = $method;
            }
            $cache[$class] = $classCache;
        }
        if (isset($cache[$class][$item])) {
            return 'method:' . $object->{$cache[$class][$item]}();
        }
        return 'missing:' . count($cache) . ':' . count($cache[$class]);
    }
}
final class Post
{
    public string $pub = 'P';
    public function getTitle(): string { return 'T'; }
    public function jump(): string { return 'J'; }
}
eval("return 1;");
$p = new Post();
echo Resolver::resolve($p, 'pub', false), "\n";
echo Resolver::resolve($p, 'getTitle', true), "\n";
echo Resolver::resolve($p, 'gettitle', false), "\n";
echo Resolver::resolve($p, 'jump', false), "\n";
echo Resolver::resolve($p, 'none', true), "\n";
echo Resolver::resolve(new ArrayObject([]), 'count', true), "\n";
"###);
    assert_eq!(out, "property:P\nmethod:T\nmethod:T\nmethod:J\nmissing:1:3\nmethod:0\n");
}

/// A `static $m = []` written with string keys reads back what the same call just wrote.
///
/// The slot stayed typed as an empty packed array over the hash its first string-key write
/// promoted it to: the read on the same call missed, and a later call duplicated keys.
#[test]
fn test_string_keyed_static_array_reads_back_on_the_same_call() {
    let out = compile_and_run(r###"<?php
function g1(string $class, string $item)
{
    static $cache = [];
    $cache[$class] = ['a' => 'A', 'b' => 'B'];
    return $cache[$class][$item] ?? 'miss';
}
function g2(string $class, string $item)
{
    static $cache = [];
    if (!isset($cache[$class])) {
        $cache[$class] = ['a' => 'A', 'b' => 'B'];
    }
    return $cache[$class][$item];
}
function g3(string $class, string $item)
{
    static $cache = [];
    if (!isset($cache[$class])) {
        $cache[$class] = ['a' => 'A', 'b' => 'B'];
    }
    return isset($cache[$class][$item]) ? 'set' : 'unset';
}
function g4(string $class)
{
    static $cache = [];
    $cache[$class] = 1;
    return count($cache);
}
echo g1('X', 'a'), g1('X', 'b'), "\n";
echo g2('X', 'a'), g2('X', 'b'), "\n";
echo g3('X', 'a'), g3('X', 'b'), "\n";
echo g4('X'), g4('Y'), g4('X'), "\n";
"###);
    assert_eq!(out, "AB\nAB\nsetset\n122\n");
}

/// Compiled `is_iterable()` and `foreach` accept an interpreted `IteratorAggregate`.
///
/// The object wraps the generic native header, so the compiled Traversable matcher answered
/// false and the loop ran zero times: every Doctrine collection a compiled Twig helper touched
/// looked empty.
#[test]
fn test_compiled_code_iterates_an_interpreted_traversable() {
    let out = compile_and_run(r###"<?php
$code = <<<'PHP'
class Coll implements \Countable, \IteratorAggregate {
    private array $elements;
    public function __construct(array $e) { $this->elements = $e; }
    public function count(): int { return count($this->elements); }
    public function getIterator(): \Traversable { return new \ArrayIterator($this->elements); }
}
return new Coll(['a' => 1, 'b' => 2]);
PHP;
$c = eval($code);
var_dump(is_iterable($c), is_countable($c), $c instanceof \Traversable, $c instanceof \Countable, $c instanceof \IteratorAggregate);
function ensure_traversable($seq) { if (is_iterable($seq)) { return $seq; } return []; }
foreach (ensure_traversable($c) as $k => $v) { echo "$k=$v "; }
echo "\n";
"###);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\na=1 b=2 \n");
}

/// An interpreted generator can `yield from` a compiled generator more than once.
///
/// A dead interpreted generator's frame stayed registered under its freed address, and the next
/// compiled generator allocated there was answered for as that finished generator: every Twig
/// block after the first rendered empty.
#[test]
fn test_interpreted_generator_delegates_twice_to_compiled_generators() {
    let out = compile_and_run(r###"<?php
function deleg($obj, string $m): iterable
{
    yield from $obj->$m();
}
function deleg2(array $blocks, string $name): iterable
{
    $template = $blocks[$name][0];
    $block = $blocks[$name][1];
    yield from $template->$block();
}
$code = <<<'PHP'
class E {
    public function a(): iterable { yield "A"; yield from []; }
    public function b(): iterable { yield "B"; yield from []; }
    public function run(): iterable {
        yield "[";
        yield from deleg($this, 'a');
        yield "|";
        yield from deleg($this, 'b');
        yield "]";
    }
    public function run2(): iterable {
        $blocks = ['a' => [$this, 'a'], 'b' => [$this, 'b']];
        yield "[";
        yield from deleg2($blocks, 'a');
        yield "|";
        yield from deleg2($blocks, 'b');
        yield "]";
    }
}
return new E();
PHP;
$e = eval($code);
$out = '';
foreach ($e->run() as $s) { $out .= $s; }
echo $out, "\n";
$out = '';
foreach ($e->run2() as $s) { $out .= $s; }
echo $out, "\n";
$out = '';
foreach (deleg($e, 'a') as $s) { $out .= $s; }
foreach (deleg($e, 'b') as $s) { $out .= $s; }
echo $out, "\n";
"###);
    assert_eq!(out, "[A|B]\n[A|B]\nAB\n");
}

/// A `foreach` inside an interpreted generator walks objects, not only arrays.
///
/// Twig compiles every `{% for %}` into a generator's `foreach`, and one over a paginator or a
/// Doctrine collection took its `{% else %}` branch.
#[test]
fn test_interpreted_generator_foreach_walks_traversable_objects() {
    let out = compile_and_run(r###"<?php
function make_iter(): ArrayIterator { return new ArrayIterator([10 => 'a', 11 => 'b']); }
$code = <<<'PHP'
class Agg implements IteratorAggregate {
    public function getIterator(): Iterator { return new ArrayIterator(['x' => 1, 'y' => 2]); }
}
function dup() { yield 'k' => 1; yield 'k' => 2; }
function g(array $context) {
    $context['_seq'] = $context['page'];
    $context['_iterated'] = false;
    foreach ($context['_seq'] as $context["_key"] => $context["p"]) {
        yield $context["_key"] . '=' . $context["p"];
        $context['_iterated'] = true;
    }
    if (!$context['_iterated']) { yield "NONE"; }
}
foreach ([make_iter(), new Agg(), dup(), [], new ArrayIterator([])] as $subject) {
    $out = [];
    foreach (g(['page' => $subject]) as $piece) { $out[] = $piece; }
    echo implode(',', $out), "\n";
}
PHP;
eval($code);
"###);
    assert_eq!(out, "10=a,11=b\nx=1,y=2\nk=1,k=2\nNONE\nNONE\n");
}

/// `strtr()` with a non-string replacement value leaves the rest of the subject intact.
///
/// The coerced replacement was spelled into the concat scratch the result had reserved but not
/// claimed, and overwrote the result's first bytes (`file://` came back as `1ile://`).
#[test]
fn test_strtr_with_an_integer_replacement_keeps_the_subject() {
    let out = compile_and_run(r###"<?php
function fmt(array|false $fmt, string $file, int $line): string
{
    return strtr($fmt[0], ['%f' => $file, '%l' => $line]);
}
echo fmt(['file://%f#L%l'], '/tmp/x.twig', 1), "\n";
echo strtr('file://%f#L%l', ['%f' => '/a', '%l' => 7]), "\n";
$line = 1;
echo strtr('file://%f#L%l', ['%f' => '/b', '%l' => $line]), "\n";
$code = <<<'PHP'
$fmt = ['file://%f#L%l'];
echo strtr($fmt[0], ['%f' => '/c', '%l' => 3]), "\n";
PHP;
eval($code);
"###);
    assert_eq!(out, "file:///tmp/x.twig#L1\nfile:///a#L7\nfile:///b#L1\nfile:///c#L3\n");
}

/// A runtime-named property read through an `object` parameter returns the property's value.
///
/// The bare `object` type names no class, and the read fell back to the checker's `int`, so a
/// string property came back as `1`.
#[test]
fn test_dynamic_property_read_through_an_object_parameter() {
    let out = compile_and_run(r###"<?php
final class Post { public string $pub = 'P'; public int $n = 7; }
function a(object $object, string $item) {
    if (isset($object->$item)) {
        return 'property:' . $object->$item;
    }
    return 'none';
}
function b(object $object, string $item) {
    return $object->$item;
}
function c($object, $item) {
    if (isset($object->$item)) {
        return $object->$item;
    }
    return 'none';
}
$p = new Post();
echo a($p, 'pub'), ' ', a($p, 'n'), ' ', b($p, 'pub'), ' ', c($p, 'pub'), ' ', c($p, 'n'), "\n";
"###);
    assert_eq!(out, "property:P property:7 P P 7\n");
}

/// `get_class_methods()` from a compiled static method sees an interpreted class.
///
/// Compiled code asks through the bridge's shared context, which never declared the classes
/// other eval contexts did, so Twig listed no methods for a Doctrine entity and every
/// `post.title` rendered empty.
#[test]
fn test_compiled_code_reads_methods_of_a_class_another_context_declared() {
    let out = compile_and_run(r###"<?php
final class Ext
{
    public static function getAttribute($object, $item, int $type = 0)
    {
        if (1 !== $type) {
            static $propertyCheckers = [];
            if (isset($object->$item)
                || ($propertyCheckers[$object::class][$item] ??= self::getPropertyChecker($object::class, $item))($object, $item)
            ) {
                return $object->$item;
            }
            if (\defined($object::class.'::'.$item)) {
                return \constant($object::class.'::'.$item);
            }
        }
        static $cache = [];
        $class = $object::class;
        if (!isset($cache[$class])) {
            $methods = get_class_methods($object);
            sort($methods);
            $lcMethods = array_map('strtolower', $methods);
            $classCache = [];
            foreach ($methods as $i => $method) {
                $classCache[$method] = $method;
                $classCache[$lcName = $lcMethods[$i]] = $method;
                if ('g' === $lcName[0] && str_starts_with($lcName, 'get')) {
                    $name = substr($method, 3);
                    $lcName = substr($lcName, 3);
                } elseif ('i' === $lcName[0] && str_starts_with($lcName, 'is')) {
                    $name = substr($method, 2);
                    $lcName = substr($lcName, 2);
                } else {
                    continue;
                }
                if ($name) {
                    if (!isset($classCache[$name])) {
                        $classCache[$name] = $method;
                    }
                    if (!isset($classCache[$lcName])) {
                        $classCache[$lcName] = $method;
                    }
                }
            }
            $cache[$class] = $classCache;
        }
        if (isset($cache[$class][$item])) {
            $method = $cache[$class][$item];
        } elseif (isset($cache[$class][$lcItem = strtolower($item)])) {
            $method = $cache[$class][$lcItem];
        } else {
            return 'NOMETHOD';
        }
        return $object->$method();
    }

    private static function getPropertyChecker(string $class, string $property): \Closure
    {
        static $classReflectors = [];
        $class = $classReflectors[$class] ??= new \ReflectionClass($class);
        if (!$class->hasProperty($property)) {
            static $propertyExists;
            return $propertyExists ??= \Closure::fromCallable('property_exists');
        }
        $property = $class->getProperty($property);
        if (!$property->isPublic() || $property->isStatic()) {
            static $false;
            return $false ??= static fn () => false;
        }
        return static fn ($object) => $property->isInitialized($object);
    }
}
final class Local { private string $title = 'LT'; public function getTitle(): string { return $this->title; } }
$code = <<<'PHP'
namespace App\Entity;
class Post {
    private ?int $id = 3;
    private ?string $title = 'T';
    private string $slug = 's';
    public function getId(): ?int { return $this->id; }
    public function getTitle(): ?string { return $this->title; }
    public function getSlug(): string { return $this->slug; }
}
return new Post();
PHP;
$p = eval($code);
echo Ext::getAttribute(new Local(), 'title'), "\n";
echo Ext::getAttribute($p, 'title'), "|", Ext::getAttribute($p, 'slug'), "|", Ext::getAttribute($p, 'id'), "\n";
"###);
    assert_eq!(out, "LT\nT|s|3\n");
}

/// A string callable naming a function that only a runtime-included file declared is called, and
/// `function_exists()` sees that function.
///
/// The compiled candidate tables cannot name such a function (a polyfill's
/// `if (!function_exists('f')) { function f() {} }` that Composer includes through a computed
/// path), so `$name(...)` aborted with `Call to undefined function <dynamic>()` and
/// `function_exists('f')` folded to false: symfony/string's `UnicodeString::split()` calls
/// `grapheme_strpos` exactly that way.
#[test]
fn test_runtime_string_callable_reaches_a_function_a_runtime_include_declared() {
    let out = compile_cli_files_and_run(&[("main.php", r###"<?php
$p = __DIR__ . "/inc" . ($argc > 5 ? "x" : "") . ".php";
require $p;
class S
{
    public bool $ci = false;
    public string $s = "ab\ncd";
    public function idx(string $d)
    {
        $f = $this->ci ? "my_fnx" : "my_fn";
        return $f($this->s, $d);
    }
}
$f = $argc > 5 ? "nope" : "my_fn";
var_dump($f("abc", "c"));
var_dump((new S())->idx("\n"));
$names = ["my_fn"];
var_dump($names[0]("xyz", "z"));
var_dump(call_user_func($names[0], "abc", "b"));
var_dump(call_user_func_array($f, ["abc", "a"]));
var_dump(array_map(fn ($x) => $f($x, "b"), ["ab", "cb"]));
var_dump(function_exists("my_fn"), function_exists($f), function_exists("my_fnx"));
"###), ("inc.php", r###"<?php
if (!function_exists("my_fn")) {
    function my_fn(?string $h, ?string $n, ?int $o = 0): int|false
    {
        return strpos((string) $h, (string) $n, (int) $o);
    }
}
"###)], "main.php");
    assert_eq!(out, "int(2)\nint(2)\nint(2)\nint(1)\nint(0)\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(1)\n}\nbool(true)\nbool(true)\nbool(false)\n");
}

/// A function declared inside `if (!function_exists(...))` is callable from eval by its public name.
///
/// A conditional declaration compiles to a private variant behind a public dispatcher, and only
/// the private symbol was registered with eval: `u()` from symfony/string was an undefined
/// function in every interpreted caller.
#[test]
fn test_conditionally_declared_namespaced_function_is_callable_from_eval() {
    let out = compile_cli_file_and_run(r###"<?php
namespace Lib\Str {
    final class Wrap
    {
        public function __construct(public string $s = '') {}
        public function upper(): string { return strtoupper($this->s); }
    }
    if (!\function_exists(w::class)) {
        function w(?string $s = ''): Wrap
        {
            return new Wrap($s ?? '');
        }
    }
}
namespace {
    $direct = \Lib\Str\w('abc')->upper();
    echo $direct, "\n";
    echo eval('namespace X; use function Lib\Str\w; return w("def")->upper();'), "\n";
    echo eval('return \Lib\Str\w("ghi")->upper();'), "\n";
    echo function_exists('Lib\Str\w') ? 'exists' : 'missing', "\n";
    echo call_user_func('Lib\Str\w', 'jkl')->upper(), "\n";
}
"###, &[]);
    assert_eq!(out, "ABC\nDEF\nGHI\nexists\nJKL\n");
}

/// `preg_split()`'s limit counts the pieces php KEEPS between separators: an empty piece dropped by
/// `PREG_SPLIT_NO_EMPTY` and a delimiter added by `PREG_SPLIT_DELIM_CAPTURE` leave it untouched.
///
/// Counting every separator (compiled) or every output piece (eval) stopped the split early, so
/// the grapheme polyfill's `grapheme_extract()` returned three spaces for four and symfony/string's
/// `startsWith('    ')` answered false.
#[test]
fn test_preg_split_limit_counts_only_kept_segments() {
    let out = compile_and_run_with_regex(r###"<?php
$size = $argc > 5 ? 9 : 4;
echo json_encode(preg_split('/(\X)/u', "\r\n    a", $size + 1, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE)), "\n";
echo json_encode(preg_split('/(,)/', "a,b,,c,d", 3, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE)), "\n";
echo json_encode(preg_split('/,/', "a,,b,,c,d", 3, PREG_SPLIT_NO_EMPTY)), "\n";
echo json_encode(preg_split('/(x)/', "xaxbx", 2, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE)), "\n";
echo json_encode(preg_split('/,/', "a,b,c", 2)), "\n";
echo eval('return json_encode(preg_split("/(,)/", "a,b,,c,d", 3, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE));'), "\n";
echo eval('return json_encode(preg_split("/,/", "a,,b,,c,d", 3, PREG_SPLIT_NO_EMPTY));'), "\n";
echo eval('return json_encode(preg_split("/(x)/", "xaxbx", 2, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE));'), "\n";
"###);
    assert_eq!(out, "[\"\\r\\n\",\" \",\" \",\" \",\" \",\"a\"]\n[\"a\",\",\",\"b\",\",\",\",c,d\"]\n[\"a\",\"b\",\",c,d\"]\n[\"x\",\"a\",\"x\",\"bx\"]\n[\"a\",\"b,c\"]\n[\"a\",\",\",\"b\",\",\",\",c,d\"]\n[\"a\",\"b\",\",c,d\"]\n[\"x\",\"a\",\"x\",\"bx\"]\n");
}

/// `ReflectionMethod::getFileName()`, `getStartLine()` and `getEndLine()` name the method's own
/// declaration, whoever built the reflector.
///
/// A reflector eval built never ran the compiled constructor, so compiled code read `""` and `0`
/// from its slots; a method of an interpreted class reported the REFLECTING code's file; and the
/// end line was the start line. The Symfony demo's "show controller code" printed lines of its
/// own Twig extension.
#[test]
fn test_reflection_method_reports_its_own_file_and_line_range() {
    let out = compile_cli_files_and_run(&[("main.php", r###"<?php
require __DIR__ . "/ctl.php";
$p = __DIR__ . "/ext_" . ($argc > 5 ? "x" : "rt") . ".php";
require $p;
$q = __DIR__ . "/ctl2_" . ($argc > 5 ? "x" : "rt") . ".php";
require $q;
$m = new ReflectionMethod("Ctl", "index");
echo basename($m->getFileName()), " ", $m->getStartLine(), " ", $m->getEndLine(), "\n";
$m = new ReflectionMethod(new Ctl(), "index");
echo basename($m->getFileName()), " ", $m->getStartLine(), " ", $m->getEndLine(), "\n";
$e = new Ext();
$e->setController([new Ctl2(), "index"]);
echo $e->info(), "\n";
$e->setController([new Ctl(), "index"]);
echo $e->info(), "\n";
$m = new ReflectionMethod(new Ctl2(), "index");
echo basename($m->getFileName()), " ", $m->getStartLine(), " ", $m->getEndLine(), "\n";
"###), ("ctl.php", r###"<?php
class Ctl
{
    public function index(): string
    {
        return "x";
    }
}
"###), ("ctl2_rt.php", r###"<?php
final class Ctl2
{
    public function index(): string
    {
        return "x";
    }
}
"###), ("ext_rt.php", r###"<?php
final class Ext
{
    private $controller;
    public function setController(?callable $c): void { $this->controller = $c; }
    public function info(): string
    {
        $m = $this->reflector($this->controller);
        return basename($m->getFileName()) . " " . $m->getStartLine() . " " . $m->getEndLine();
    }
    private function reflector(callable $callable): \ReflectionFunctionAbstract
    {
        if (\is_array($callable)) {
            return new \ReflectionMethod($callable[0], $callable[1]);
        }
        return new \ReflectionFunction($callable);
    }
}
"###)], "main.php");
    assert_eq!(out, "ctl.php 4 7\nctl.php 4 7\nctl2_rt.php 4 7\nctl.php 4 7\nctl2_rt.php 4 7\n");
}

/// A row built by mixed-key writes into `[]` keeps its hash shape once pushed into a list.
///
/// The row is statically an `array<mixed>` list promoted to a hash at runtime; stored under the
/// list tag, every reader walked list indices through the hash (`[0,5,1]`). `preg_match_all()`
/// with `PREG_SET_ORDER` builds its rows this way, which broke Symfony's `HeaderUtils::split()`.
#[test]
fn test_rows_promoted_to_hashes_keep_their_shape_in_a_list() {
    let out = compile_and_run(r###"<?php
function build(array $plain, array $ms, int $flags): array
{
    $keys = array_keys($plain);
    $withOffset = ($flags & 256) !== 0;
    $asNull = ($flags & 512) !== 0;
    $trimTrailing = ($flags & 2) !== 0 && !$asNull;
    $sets = [];
    foreach ($ms as $m) {
        $set = [];
        foreach ($keys as $k) {
            if ($trimTrailing && !array_key_exists($k, $m)) {
                continue;
            }
            if (array_key_exists($k, $m) && $m[$k][1] !== -1) {
                $pair = $m[$k];
            } else {
                $pair = [$asNull ? null : '', -1];
            }
            if ($withOffset) {
                $set[$k] = $pair;
            } else {
                $set[$k] = $pair[0];
            }
        }
        $sets[] = $set;
    }
    return $sets;
}
$plain = [0 => [], 'sep' => [], 1 => []];
$ms = [
    [0 => [',', 0], 'sep' => [',', 0], 1 => [',', 0]],
    [0 => ['a', 1]],
];
$x = build($plain, $ms, 2); echo json_encode($x), "\n"; echo json_encode($x[0]), "\n"; var_dump($x[0]); $y = $x; $y[] = 1; echo json_encode($y), "\n";
$ms2 = [
    [0 => ['a', 0]],
    [0 => [',', 1], 'sep' => [',', 1], 1 => [',', 1]],
    [0 => ['b', 2]],
];
echo json_encode(build($plain, $ms2, 2)), "\n";
echo json_encode(build($plain, $ms2, 0)), "\n";
"###);
    assert_eq!(out, "[{\"0\":\",\",\"sep\":\",\",\"1\":\",\"},[\"a\"]]\n{\"0\":\",\",\"sep\":\",\",\"1\":\",\"}\narray(3) {\n  [0]=>\n  string(1) \",\"\n  [\"sep\"]=>\n  string(1) \",\"\n  [1]=>\n  string(1) \",\"\n}\n[{\"0\":\",\",\"sep\":\",\",\"1\":\",\"},[\"a\"],1]\n[[\"a\"],{\"0\":\",\",\"sep\":\",\",\"1\":\",\"},[\"b\"]]\n[{\"0\":\"a\",\"sep\":\"\",\"1\":\"\"},{\"0\":\",\",\"sep\":\",\",\"1\":\",\"},{\"0\":\"b\",\"sep\":\"\",\"1\":\"\"}]\n");
}

/// `isset(static::$map[$k]) ? static::$map[$k][0] : null` returns the element, not a pointer.
///
/// The nested read's type fell back to the syntactic guess `int`, so the ternary stored the inner
/// string's pointer in an integer slot: Symfony's `Request::getMimeType()` answered `"4396328192"`
/// and every cached response went out with that Content-Type.
#[test]
fn test_nested_read_through_a_static_array_property_under_isset() {
    let out = compile_and_run(r###"<?php
class A
{
    protected static ?array $f = null;
    protected static array $g = [];
    protected ?array $h = null;

    public static function init(): void
    {
        static::$f = ['html' => ['text/html', 'x']];
        static::$g = ['html' => ['text/html', 'x']];
    }
    public function inst(): void
    {
        $this->h = ['html' => ['text/html', 'x']];
    }
    public function a(string $k): ?string { return isset(static::$f[$k]) ? static::$f[$k][0] : null; }
    public function b(string $k): ?string { return static::$f[$k][0]; }
    public function c(string $k): ?string { return isset(static::$g[$k]) ? static::$g[$k][0] : null; }
    public function d(string $k): ?string { return isset($this->h[$k]) ? $this->h[$k][0] : null; }
    public function e(string $k): ?string { $x = static::$f[$k][0]; return $x; }
    public function f(string $k): mixed { return isset(static::$f[$k]) ? static::$f[$k][0] : null; }
}
A::init();
$a = new A();
$a->inst();
var_dump($a->a('html'), $a->b('html'), $a->c('html'), $a->d('html'), $a->e('html'), $a->f('html'));
"###);
    assert_eq!(out, "string(9) \"text/html\"\nstring(9) \"text/html\"\nstring(9) \"text/html\"\nstring(9) \"text/html\"\nstring(9) \"text/html\"\nstring(9) \"text/html\"\n");
}

/// An instance of an interpreted class is not `instanceof \stdClass`, from compiled or eval code.
///
/// Such an object carries the generic `stdClass` native header, and both the compiled matcher and
/// the interpreter's native fallback answered from it: league/config casts `stdClass` values to
/// arrays and turned every configured service object into one.
#[test]
fn test_eval_class_instance_is_not_a_stdclass() {
    let out = compile_and_run(r###"<?php
$o = eval('class Foo { public $a = 1; } return new Foo();');
var_dump($o instanceof stdClass);
var_dump(eval('return $o instanceof \stdClass;'));
var_dump(is_a($o, 'stdClass'));
var_dump(eval('return is_a($o, "stdClass");'));
var_dump(get_class($o));
$f = function ($data) {
    if ($data instanceof \stdClass) {
        $data = (array) $data;
    }
    return $data;
};
var_dump(is_object($f($o)));
var_dump(eval('return (function ($d) { if ($d instanceof \stdClass) { $d = (array) $d; } return is_object($d); })($o);'));
$s = new stdClass();
var_dump($s instanceof stdClass, eval('return $s instanceof \stdClass;'));
"###);
    assert_eq!(out, "bool(false)\nbool(false)\nbool(false)\nbool(false)\nstring(3) \"Foo\"\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n");
}

/// Eval code reads a compiled class's array constants, nested arrays and enum cases included.
///
/// Only scalar and enum-case constants were exposed to eval, so
/// `W3CReference::CONTEXTS_MAP[$element] ?? W3CReference::CONTEXT_BODY` threw "Undefined
/// constant" in an interpreted `HtmlSanitizer`.
#[test]
fn test_eval_reads_array_constants_of_a_compiled_class() {
    let out = compile_and_run(r###"<?php
enum Suit: string { case Hearts = 'H'; case Spades = 'S'; }
final class Ref
{
    public const A = "a";
    public const MAP = ["x" => self::A, "y" => "b", "7" => 7, 8 => true];
    public const LIST = [1, 2.5, null, "three"];
    public const NESTED = ["head" => ["meta", "title"], "body" => ["p" => [1, 2]]];
    public const ENUMS = [Suit::Hearts, Suit::Spades];
}
echo Ref::MAP["x"], "\n";
echo eval('return Ref::MAP["x"] ?? "none";'), "\n";
echo eval('return Ref::MAP["missing"] ?? "none";'), "\n";
echo eval('return count(Ref::MAP) . count(Ref::LIST);'), "\n";
echo eval('return json_encode(Ref::MAP);'), "\n";
echo eval('return json_encode(Ref::LIST);'), "\n";
echo eval('return json_encode(Ref::NESTED);'), "\n";
echo eval('return Ref::NESTED["body"]["p"][1];'), "\n";
echo eval('return Ref::ENUMS[1]->name;'), "\n";
echo eval('$m = Ref::MAP; $m["x"] = "changed"; return $m["x"] . Ref::MAP["x"];'), "\n";
var_dump(eval('return array_is_list(Ref::LIST);'));
var_dump(eval('return array_keys(Ref::MAP);'));
"###);
    assert_eq!(out, "a\na\nnone\n44\n{\"x\":\"a\",\"y\":\"b\",\"7\":7,\"8\":true}\n[1,2.5,null,\"three\"]\n{\"head\":[\"meta\",\"title\"],\"body\":{\"p\":[1,2]}}\n2\nSpades\nchangeda\nbool(true)\narray(4) {\n  [0]=>\n  string(1) \"x\"\n  [1]=>\n  string(1) \"y\"\n  [2]=>\n  int(7)\n  [3]=>\n  int(8)\n}\n");
}

/// `$cv =& $cv[$k]` walking down an array in eval writes through to the original array.
///
/// The rebinding kept the alias on the outer variable, so writes landed in a detached copy
/// (Dflydev's `Data::set()`, which league/config builds on).
#[test]
fn test_eval_reference_walk_into_nested_array_writes_through() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
$data = ['a' => ['b' => 1]];
$cv =& $data;
foreach (['x', 'y'] as $k) {
    if (!isset($cv[$k])) {
        $cv[$k] = [];
    }
    $cv =& $cv[$k];
}
$cv['z'] = 3;
echo json_encode($data), "\n";
class D {
    public $data = [];
    public function set(array $path, $value): void {
        $cv =& $this->data;
        $end = array_pop($path);
        foreach ($path as $k) {
            if (!isset($cv[$k])) {
                $cv[$k] = [];
            }
            $cv =& $cv[$k];
        }
        $cv[$end] = $value;
    }
}
$d = new D();
$d->set(['x', 'y', 'z'], 3);
$d->set(['x', 'w'], 4);
echo json_encode($d->data), "\n";
PHP);
"###);
    assert_eq!(out, "{\"a\":{\"b\":1},\"x\":{\"y\":{\"z\":3}}}\n{\"x\":{\"y\":{\"z\":3},\"w\":4}}\n");
}

/// A reference rebound into a sub-array that was just created still writes through.
#[test]
fn test_eval_reference_rebound_into_a_fresh_sub_array() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
$data = [];
$cv =& $data;
$cv['x'] = [];
echo json_encode($data), "\n";
$cv =& $cv['x'];
$cv['y'] = 1;
echo json_encode($data), " ", json_encode($cv), "\n";
$other = ['p' => []];
$r =& $other['p'];
$r['q'] = 2;
echo json_encode($other), "\n";
PHP);
"###);
    assert_eq!(out, "{\"x\":[]}\n{\"x\":{\"y\":1}} {\"y\":1}\n{\"p\":{\"q\":2}}\n");
}

/// A bag that keeps `&$session[$key]` in a property writes through to `$_SESSION`.
///
/// Symfony's `NativeSessionStorage::loadSession()` hands each bag a reference into `$_SESSION`;
/// the write-back never reached the session, so flash messages and attributes vanished.
#[test]
fn test_eval_reference_to_a_session_slot_writes_through() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
class Bag {
    protected array $attributes = [];
    public function __construct(private string $key) {}
    public function getStorageKey(): string { return $this->key; }
    public function initialize(array &$attributes): void { $this->attributes = &$attributes; }
    public function set(string $n, mixed $v): void { $this->attributes[$n] = $v; }
    public function get(string $n): mixed { return $this->attributes[$n] ?? null; }
}
class Storage {
    private array $bags = [];
    public function register(Bag $bag): void { $this->bags[$bag->getStorageKey()] = $bag; }
    public function load(?array &$session = null): void {
        if (null === $session) {
            $session = &$_SESSION;
        }
        foreach ($this->bags as $bag) {
            $key = $bag->getStorageKey();
            $session[$key] = isset($session[$key]) && \is_array($session[$key]) ? $session[$key] : [];
            $bag->initialize($session[$key]);
        }
    }
}
$_SESSION = ['_attrs' => ['old' => 1]];
$bag = new Bag('_attrs');
$meta = new Bag('_meta');
$st = new Storage();
$st->register($bag);
$st->register($meta);
$st->load();
$bag->set('new', 2);
$meta->set('t', 3);
echo json_encode($_SESSION), ' ', $bag->get('old'), "\n";
PHP);
"###);
    assert_eq!(out, "{\"_attrs\":{\"old\":1,\"new\":2},\"_meta\":{\"t\":3}} 1\n");
}

/// A method storing `&$attributes` (an array element passed by reference) in a property writes
/// through to that element.
#[test]
fn test_eval_property_reference_to_an_array_element_argument() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHP'
class Bag {
    protected array $attributes = [];
    public function initialize(array &$attributes): void { $this->attributes = &$attributes; }
    public function set(string $n, mixed $v): void { $this->attributes[$n] = $v; }
    public function get(string $n): mixed { return $this->attributes[$n] ?? null; }
}
$session = ['k' => []];
$bag = new Bag();
$bag->initialize($session['k']);
$bag->set('a', 1);
echo $bag->get('a'), "\n";
echo json_encode($session), "\n";
function start(array &$session, Bag $bag): void {
    $key = 'z';
    $session[$key] = isset($session[$key]) && \is_array($session[$key]) ? $session[$key] : [];
    $bag->initialize($session[$key]);
}
$s2 = [];
$bag2 = new Bag();
start($s2, $bag2);
$bag2->set('b', 2);
echo json_encode($s2), "\n";
PHP);
"###);
    assert_eq!(out, "1\n{\"k\":{\"a\":1}}\n{\"z\":{\"b\":2}}\n");
}

/// `return $c->privates['Sub'] = new Sub($c->privates['Ext'] ?? self::getExt($c));` evaluates the
/// right side before fetching the property for writing, as php's delayed fetch does.
///
/// Fetching first wrote into a stale copy that the nested `getExt()` write then replaced, so the
/// Symfony container built a private service twice.
#[test]
fn test_eval_container_assignment_evaluates_its_value_first() {
    let out = compile_and_run(r###"<?php
class BaseContainer
{
    protected array $services = [];
    protected array $privates = [];
    public static int $made = 0;

    public function has(string $id): string
    {
        return isset($this->privates[$id]) ? 'set' : 'unset';
    }
}
$code = <<<'PHP'
class Ext { public $n; public function __construct() { $this->n = ++BaseContainer::$made; } }
class Sub { public function __construct(public Ext $ext) {} }
class AppContainer extends BaseContainer
{
    protected static function getExtService($container)
    {
        return $container->privates['Ext'] = new Ext();
    }
    protected static function getSubService($container)
    {
        return $container->privates['Sub'] = new Sub(($container->privates['Ext'] ?? self::getExtService($container)));
    }
    public function sub() { return self::getSubService($this); }
    public function make() { return self::getExtService($this); }
}
return new AppContainer();
PHP;
$c = eval($code);
$c->sub();
echo $c->has('Ext'), ' ', $c->has('Sub'), "\n";
$d = eval('return new AppContainer();');
$d->make();
echo $d->has('Ext'), "\n";
"###);
    assert_eq!(out, "set set\nset\n");
}

/// A service locator's `??=` through the container's private array reuses the instance a factory
/// already stored.
#[test]
fn test_eval_locator_reuses_a_private_service() {
    let out = compile_and_run(r###"<?php
class BaseContainer
{
    protected array $services = [];
    protected array $privates = [];
    public static int $made = 0;

    final protected function getService(string|false $registry, string $id, ?string $method, string|bool $load): mixed
    {
        if (null === $method) {
            return $this->privates[$id] ?? null;
        }
        return $this->privates[$id] ??= $this->{$method}($this);
    }

    public function has(string $id): string
    {
        return isset($this->privates[$id]) ? 'set' : 'unset';
    }
}
final class Locator
{
    public function __construct(private \Closure $factory, private array $map) {}
    public function get(string $id): mixed
    {
        return ($this->factory)(...$this->map[$id]);
    }
}
$code = <<<'PHP'
class Ext { public $n; public function __construct() { $this->n = ++BaseContainer::$made; } }
class Sub { public function __construct(public Ext $ext) {} }
class AppContainer extends BaseContainer
{
    protected \Closure $getService;
    public function __construct() { $this->services = $this->privates = []; }
    protected static function getExtService($container)
    {
        return $container->privates['Ext'] = new Ext();
    }
    protected static function getSubService($container)
    {
        return $container->privates['Sub'] = new Sub(($container->privates['Ext'] ?? self::getExtService($container)));
    }
    public function sub() { return self::getSubService($this); }
    public function locator(): Locator
    {
        return new Locator($this->getService ??= $this->getService(...), ['Ext' => ['privates', 'Ext', 'getExtService', false]]);
    }
}
return new AppContainer();
PHP;
$c = eval($code);
$sub = $c->sub();
echo $c->has('Ext'), "\n";
$ext = $c->locator()->get('Ext');
echo $sub->ext->n, ' ', $ext->n, ' ', ($sub->ext === $ext ? 'same' : 'diff'), ' made=', BaseContainer::$made, "\n";
"###);
    assert_eq!(out, "set\n1 1 same made=1\n");
}

/// `ReflectionAttribute::newInstance()` autoloads the attribute class, and throws php's
/// `Attribute class "X" not found` when nothing declares it.
///
/// An attribute is often the ONLY mention of its class, so nothing had loaded it; eval answered
/// null instead of autoloading, and Symfony UX's `LiveComponentMetadataFactory` then passed null
/// for every `#[LiveProp]` (the demo's search page died on it).
#[test]
fn test_attribute_new_instance_autoloads_its_class() {
    let out = compile_cli_files_and_run(&[("main.php", r###"<?php
final class Loader
{
    public function __construct(private string $dir, private string $suffix)
    {
    }

    public function load(string $class): void
    {
        $file = $this->dir . '/' . str_replace('\\', '/', $class) . $this->suffix . '.php';
        if (is_file($file)) {
            require $file;
        }
    }
}
spl_autoload_register([new Loader(__DIR__, $argc > 5 ? '.x' : ''), 'load']);
final class Comp
{
    #[\Lib\Prop(writable: true)]
    public string $query = '';

    #[\Nope\Missing]
    public int $page = 1;
}
$attrs = (new ReflectionProperty('Comp', 'query'))->getAttributes();
echo $attrs[0]->getName(), "\n";
var_dump(class_exists('Lib\Prop', false));
$i = $attrs[0]->newInstance();
echo get_debug_type($i), ' ', var_export($i->writable, true), "\n";
try {
    (new ReflectionProperty('Comp', 'page'))->getAttributes()[0]->newInstance();
    echo "no error\n";
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
echo eval('$a = (new \ReflectionProperty("Comp", "query"))->getAttributes()[0]; return get_debug_type($a->newInstance());'), "\n";
"###), ("Lib/Prop.php", r###"<?php
namespace Lib;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
final class Prop
{
    public function __construct(public bool $writable = false)
    {
    }
}
"###)], "main.php");
    assert_eq!(out, "Lib\\Prop\nbool(false)\nLib\\Prop true\nError: Attribute class \"Nope\\Missing\" not found\nLib\\Prop\n");
}

/// `new Foo($arg)` on a class with no constructor anywhere evaluates the arguments and ignores
/// them, as php does.
///
/// Eval refused it outright; Symfony's compiled container passes a factory to
/// `ReflectionTypeResolver`, which no longer declares a constructor, and the demo's search page
/// died building it.
#[test]
fn test_eval_new_without_constructor_ignores_its_arguments() {
    let out = compile_and_run(r###"<?php
final class A {}
function side() { echo "side "; return 1; }
var_dump(eval('class B {} return get_class(new B(side(), [1]));'));
var_dump(eval('return get_class(new A(side()));'));
class P { public function __construct(public int $x = 5) {} }
class C extends P {}
var_dump(eval('return (new C(7))->x;'));
var_dump(eval('class D { public function __construct() {} } return get_class(new D(1, 2));'));
"###);
    assert_eq!(out, "side string(1) \"B\"\nside string(1) \"A\"\nint(7)\nstring(1) \"D\"\n");
}

/// `headers_list()` answers an empty array outside `--web`, from compiled code and from `eval()`,
/// and `header_remove()` in `eval()` is a no-op there, as the php CLI does.
///
/// Neither was defined: `headers_list()` did not compile and `header_remove()` in `eval()` was an
/// undefined function.
#[test]
fn test_headers_list_is_empty_outside_web_in_compiled_and_eval_code() {
    let out = compile_and_run(r###"<?php
header('X-A: 1');
$a = headers_list();
$b = eval('header("X-B: 2"); return headers_list();');
eval('header_remove("X-A");');
eval('header_remove();');
header_remove();
$c = headers_list();
echo json_encode([$a, $b, $c]), "\n";
"###);
    assert_eq!(out, "[[],[],[]]\n");
}

/// `http_build_query()` reads an object -- as `$data` or as any value inside it -- as its public
/// properties, and an object without any contributes nothing, `Stringable` or not.
///
/// The helper cast every non-array value to a string, so Symfony's `UrlGenerator`, which passes
/// the request attributes it does not use as route variables (authenticator objects among them),
/// died with "could not be converted to string" on every login redirect.
#[test]
fn test_http_build_query_reads_objects_as_their_public_properties() {
    let out = compile_and_run(r###"<?php
final class Auth
{
    private string $secret = 's';
    protected int $level = 2;
}

final class Pub
{
    public int $x = 1;
    public ?string $n = null;
    public array $list = [2, 3];
    private string $hidden = 'h';
}

final class Str implements Stringable
{
    public function __toString(): string
    {
        return 'str';
    }
}

$data = [
    'auth' => new Auth(),
    'list' => [new Auth(), new Auth()],
    'b' => 1,
    'pub' => new Pub(),
    'std' => (object) ['k' => 'v', 'e' => []],
    'str' => new Str(),
    'empty' => new stdClass(),
    't' => true,
    'f' => false,
    'nul' => null,
    'fl' => 1.5,
];
echo http_build_query($data), "\n";
echo http_build_query($data, 'p_', '&amp;', PHP_QUERY_RFC3986), "\n";
echo http_build_query(new Pub()), "\n";
echo http_build_query([new Pub(), 'x' => new Auth()], 'n'), "\n";
echo eval('return http_build_query($data, "", "&", PHP_QUERY_RFC3986);'), "\n";
echo eval('return http_build_query(new Pub());'), "\n";
"###);
    assert_eq!(out, "b=1&pub%5Bx%5D=1&pub%5Blist%5D%5B0%5D=2&pub%5Blist%5D%5B1%5D=3&std%5Bk%5D=v&t=1&f=0&fl=1.5\nb=1&amp;pub%5Bx%5D=1&amp;pub%5Blist%5D%5B0%5D=2&amp;pub%5Blist%5D%5B1%5D=3&amp;std%5Bk%5D=v&amp;t=1&amp;f=0&amp;fl=1.5\nx=1&list%5B0%5D=2&list%5B1%5D=3\nn0%5Bx%5D=1&n0%5Blist%5D%5B0%5D=2&n0%5Blist%5D%5B1%5D=3\nb=1&pub%5Bx%5D=1&pub%5Blist%5D%5B0%5D=2&pub%5Blist%5D%5B1%5D=3&std%5Bk%5D=v&t=1&f=0&fl=1.5\nx=1&list%5B0%5D=2&list%5B1%5D=3\n");
}

/// `sprintf()` in `eval()` supports php's `%n$` argument numbers -- which leave the sequential
/// cursor alone -- and the `'c` custom padding flag, and refuses `%0$` with php's ValueError.
///
/// The spec parser knew neither, so `RedirectResponse::setTargetUrl()`'s `%1$s` template failed
/// and every redirect Symfony built in interpreted code (the login entry point among them)
/// killed the worker.
#[test]
fn test_eval_sprintf_numbers_its_arguments_and_takes_a_custom_padding() {
    let out = compile_and_run(r###"<?php
$url = 'http://127.0.0.1/en/login?a=1&b=\'2\'';
echo eval('return \sprintf(\'<a href="%1$s">%1$s</a>\', htmlspecialchars($url, \ENT_QUOTES, \'UTF-8\'));'), "\n";
echo eval('return sprintf(\'%2$s-%1$s-%2$s|%s|%s\', "x", "y");'), "\n";
echo eval('return sprintf("[%\'*8s|%-\'#6s|%\'010d|%2\$\'.5d]", "ab", "cd", -42, 7);'), "\n";
echo eval('return vsprintf(\'%2$s/%1$s\', ["a", "b"]);'), "\n";
eval('printf("%1\$s=%1\$.1f\n", 3.25);');
try {
    eval('return sprintf(\'%0$s\', "x");');
} catch (ValueError $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
"###);
    assert_eq!(out, "<a href=\"http://127.0.0.1/en/login?a=1&amp;b=&#039;2&#039;\">http://127.0.0.1/en/login?a=1&amp;b=&#039;2&#039;</a>\ny-x-y|x|y\n[******ab|cd####|-000000042|....0]\nb/a\n3.25=3.2\nValueError: Argument number specifier must be greater than zero and less than 2147483647\n");
}

/// `version_compare()` answers as php 8.5.10 does -- canonical forms, special forms, the `#N#`
/// sentinel, the dropped trailing dot, every operator spelling and the invalid-operator ValueError
/// -- from compiled code, from a namespaced caller and from `eval()`.
///
/// It did not exist on either side: Symfony's `SodiumPasswordHasher::isSupported()` asks it from
/// interpreted code, so every login POST died on an undefined function.
#[test]
fn test_version_compare_matches_php_in_compiled_and_eval_code() {
    let out = compile_and_run(r###"<?php
namespace App;

$pairs = [
    ['1.0.14', '1.0.22'], ['5.10', '5.2'], ['1.0', '1.0.0'], ['1.0-dev', '1.0'], ['1.0alpha', '1.0beta'],
    ['1.0RC1', '1.0rc2'], ['1.0pl1', '1.0'], ['1', '1.'], ['', '1'], ['', ''], ['1', '1.a'], ['.1', '1'],
    ['1.0.0-beta.2', '1.0.0-beta.10'], ['v1.2', '1.2'], ['1_2+3', '1.2.3'], ['99999999999999999999', '1'],
    ['#N#', '1'], ['1.0#', '1.0'], ['8.5.10', '8.5.9'], ["1.2\0zz", '1.2'],
];
$ops = [null, '<', 'lt', '<=', 'le', '>', 'gt', '>=', 'ge', '==', '=', 'eq', '!=', '<>', 'ne'];
$out = [];
foreach ($pairs as [$a, $b]) {
    $row = [version_compare($a, $b)];
    foreach ($ops as $op) {
        if ($op !== null) {
            $row[] = version_compare($a, $b, $op);
        }
    }
    $out[] = $row;
}
echo json_encode($out), "\n";
$evaluated = eval('namespace Lib; $o = []; foreach ($pairs as [$a, $b]) { $r = [version_compare($a, $b)]; foreach ($ops as $op) { if ($op !== null) { $r[] = \version_compare($a, $b, $op); } } $o[] = $r; } return $o;');
echo json_encode($evaluated === $out), "\n";
foreach (['x', ''] as $bad) {
    try {
        version_compare('1', '2', $bad);
    } catch (\ValueError $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
    try {
        eval('version_compare("1", "2", $bad);');
    } catch (\ValueError $e) {
        echo 'eval ', get_class($e), ': ', $e->getMessage(), "\n";
    }
}
var_dump(version_compare('1.0.14', '1.0.14', null));
"###);
    assert_eq!(out, "[[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[1,false,false,false,false,true,true,true,true,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[1,false,false,false,false,true,true,true,true,false,false,false,true,true,true],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false],[1,false,false,false,false,true,true,true,true,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[-1,true,true,true,true,false,false,false,false,false,false,false,true,true,true],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false],[1,false,false,false,false,true,true,true,true,false,false,false,true,true,true],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false],[1,false,false,false,false,true,true,true,true,false,false,false,true,true,true],[0,false,false,true,true,false,false,true,true,true,true,true,false,false,false]]\ntrue\nValueError: version_compare(): Argument #3 ($operator) must be a valid comparison operator\neval ValueError: version_compare(): Argument #3 ($operator) must be a valid comparison operator\nValueError: version_compare(): Argument #3 ($operator) must be a valid comparison operator\neval ValueError: version_compare(): Argument #3 ($operator) must be a valid comparison operator\nint(0)\n");
}

/// `hash_pbkdf2()` derives php's keys -- hex and raw, default and odd lengths, case-insensitive
/// algorithm names -- and raises php's three ValueErrors, from compiled code and from `eval()`.
///
/// It did not exist on either side, and `hash()` refused an upper-case algorithm name:
/// Symfony's `Pbkdf2PasswordHasher` derives a key in its constructor, so every login POST died.
#[test]
fn test_hash_pbkdf2_matches_php_in_compiled_and_eval_code() {
    let out = compile_and_run(r###"<?php
echo hash_pbkdf2('sha256', 'password', 'salt', 1000), "\n";
echo hash_pbkdf2('sha512', '', 'salt', 1000, 40, false), "\n";
echo bin2hex(hash_pbkdf2('SHA1', 'password', 'salt', 2, 20, true)), "\n";
echo hash_pbkdf2('sha1', 'password', 'salt', 1, 7), "\n";
echo base64_encode(hash_pbkdf2('sha512', '', 'salt', 1000, 40, true)), "\n";
echo hash('SHA256', 'abc'), "\n";
foreach ([['crc32b', 1, 0], ['nope', 1, 0], ['sha1', 0, 0], ['sha1', 1, -1]] as [$algo, $iterations, $length]) {
    try {
        hash_pbkdf2($algo, 'p', 's', $iterations, $length);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo eval('return hash_pbkdf2("sha256", "password", "salt", 1000);'), "\n";
echo eval('return base64_encode(hash_pbkdf2("sha512", "", "salt", 1000, 40, true));'), "\n";
echo eval('return hash_pbkdf2("sha1", "password", "salt", 1, 7);'), "\n";
echo eval('return hash("SHA256", "abc");'), "\n";
foreach ([['crc32b', 1, 0], ['sha1', 0, 0], ['sha1', 1, -1]] as [$algo, $iterations, $length]) {
    try {
        eval('hash_pbkdf2($algo, "p", "s", $iterations, $length);');
    } catch (ValueError $e) {
        echo 'eval ', $e->getMessage(), "\n";
    }
}
"###);
    assert_eq!(out, "632c2812e46d4604102ba7618e9d6d7d2f8128f6266b4a03264d2a0460b7dcb3\nf930bd1aa48d2e83b0202406030ed64b16161c4e\nea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957\n0c60c80\n+TC9GqSNLoOwICQGAw7WSxYWHE4lKwjRm1RTHnKxfcOA66mou8G8/g==\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\nhash_pbkdf2(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm\nhash_pbkdf2(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm\nhash_pbkdf2(): Argument #4 ($iterations) must be greater than 0\nhash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0\n632c2812e46d4604102ba7618e9d6d7d2f8128f6266b4a03264d2a0460b7dcb3\n+TC9GqSNLoOwICQGAw7WSxYWHE4lKwjRm1RTHnKxfcOA66mou8G8/g==\n0c60c80\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\neval hash_pbkdf2(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm\neval hash_pbkdf2(): Argument #4 ($iterations) must be greater than 0\neval hash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0\n");
}

/// The `password_*` API answers in `eval()` as php does, and both sides resolve `$algo` as php's
/// `php_password_algo_find_zval` does: `0` is the default, and an unknown algorithm never prompts
/// `password_needs_rehash()` (it threw a ValueError in compiled code).
///
/// None of the five existed in `eval()`: Symfony's `NativePasswordHasher` verifies every login
/// password from interpreted code, so every login POST died on an undefined function.
#[test]
fn test_password_api_matches_php_in_eval_and_legacy_algorithm_ids() {
    let out = compile_and_run(r###"<?php
$hash = '$2y$13$3EJxNVreeSXjCJooqMEDsu8jjTtCWVQWKs5QaCRZ2VcrXK1ATq0E.';
var_dump(eval('return password_verify("kitten", $hash);'));
var_dump(eval('return password_verify("kitten!", $hash);'));
var_dump(eval('return password_verify("kitten", "not a hash");'));
echo json_encode(eval('return password_get_info($hash);')), "\n";
echo json_encode(eval('return password_get_info("nope");')), "\n";
var_dump(eval('return password_needs_rehash($hash, PASSWORD_BCRYPT, ["cost" => 13]);'));
var_dump(eval('return password_needs_rehash($hash, null);'));
var_dump(eval('return password_needs_rehash("nope", "2y");'));
var_dump(eval('return password_needs_rehash("nope", 7);'), password_needs_rehash('nope', 7));
var_dump(eval('return password_needs_rehash($hash, 0, ["cost" => 13]);'), password_needs_rehash($hash, 0, ['cost' => 13]));
$new = eval('return password_hash("secret", PASSWORD_DEFAULT, ["cost" => 4]);');
var_dump(strlen($new), substr($new, 0, 7), password_verify('secret', $new), eval('return password_verify("secret", $new);'));
var_dump(strlen(password_hash('secret', 0, ['cost' => 4])), strlen(eval('return password_hash("secret", 0, ["cost" => 4]);')));
foreach (['password_hash("x", "foo");', 'password_hash("x", null, ["cost" => 3]);'] as $code) {
    try {
        eval($code);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\n{\"algo\":\"2y\",\"algoName\":\"bcrypt\",\"options\":{\"cost\":13}}\n{\"algo\":null,\"algoName\":\"unknown\",\"options\":[]}\nbool(false)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nint(60)\nstring(7) \"$2y$04$\"\nbool(true)\nbool(true)\nint(60)\nint(60)\npassword_hash(): Argument #2 ($algo) must be a valid password hashing algorithm\nInvalid bcrypt cost parameter specified: 3\n");
}

/// `serialize()` in `eval()` writes what `__serialize()` returns -- for a compiled object and for
/// an interpreted one -- and `unserialize()` in `eval()` hands the payload, integer keys and all,
/// to `__unserialize()`.
///
/// Eval ignored both hooks: a compiled object serialized as `O:5:"Token":0:{}`, an interpreted one
/// as its raw private properties, and an `i:0;` payload key was a parse error. Symfony stores its
/// security token in the session through exactly these calls, so no login survived its redirect.
#[test]
fn test_eval_serialize_honours_serialize_magic_on_compiled_and_eval_objects() {
    let out = compile_and_run(r###"<?php
abstract class BaseToken
{
    private array $roleNames = ['ROLE_ADMIN'];
    private array $attributes = [];

    public function __construct(private ?User $user = null) {}

    public function __serialize(): array
    {
        return [$this->user, true, null, $this->attributes, $this->roleNames];
    }

    public function __unserialize(array $data): void
    {
        [$this->user, , , $this->attributes, $this->roleNames] = $data;
    }

    public function getUser(): ?User
    {
        return $this->user;
    }
}

final class Token extends BaseToken
{
    public function __construct(?User $user, private string $firewallName)
    {
        parent::__construct($user);
    }

    public function __serialize(): array
    {
        return [null, $this->firewallName, parent::__serialize()];
    }

    public function __unserialize(array $data): void
    {
        [, $this->firewallName, $parentData] = $data;
        parent::__unserialize($parentData);
    }

    public function firewall(): string
    {
        return $this->firewallName;
    }
}

final class User
{
    private string $email = 'jane@example.com';

    public function __construct(private int $id, private string $name) {}

    public function __serialize(): array
    {
        return [$this->id, $this->name];
    }

    public function __unserialize(array $data): void
    {
        [$this->id, $this->name] = $data;
    }

    public function name(): string
    {
        return $this->name;
    }
}

$token = new Token(new User(1, 'jane'), 'main');
$aot = serialize($token);
echo $aot, "\n";
$fromEval = eval('return serialize($token);');
echo $fromEval, "\n";
$back = unserialize($aot);
echo get_class($back), ' ', $back->firewall(), ' ', $back->getUser()->name(), "\n";
$back2 = eval('return unserialize($fromEval);');
echo get_class($back2), ' ', $back2->firewall(), ' ', $back2->getUser()->name(), "\n";
eval('final class EvalUser { public function __construct(private int $id = 3) {} public function __serialize(): array { return ["id" => $this->id * 2]; } public function __unserialize(array $d): void { $this->id = $d["id"] + 1; } public function id(): int { return $this->id; } }');
$e = eval('return serialize(new EvalUser());');
echo $e, "\n";
echo serialize(eval('return new EvalUser();')), "\n";
echo eval('return unserialize($e)->id();'), ' ', unserialize($e)->id(), "\n";
"###);
    assert_eq!(out, "O:5:\"Token\":3:{i:0;N;i:1;s:4:\"main\";i:2;a:5:{i:0;O:4:\"User\":2:{i:0;i:1;i:1;s:4:\"jane\";}i:1;b:1;i:2;N;i:3;a:0:{}i:4;a:1:{i:0;s:10:\"ROLE_ADMIN\";}}}\nO:5:\"Token\":3:{i:0;N;i:1;s:4:\"main\";i:2;a:5:{i:0;O:4:\"User\":2:{i:0;i:1;i:1;s:4:\"jane\";}i:1;b:1;i:2;N;i:3;a:0:{}i:4;a:1:{i:0;s:10:\"ROLE_ADMIN\";}}}\nToken main jane\nToken main jane\nO:8:\"EvalUser\":1:{s:2:\"id\";i:6;}\nO:8:\"EvalUser\":1:{s:2:\"id\";i:6;}\n7 7\n");
}

/// `unserialize()` in `eval()` rebuilds a compiled class through its `__unserialize()` and writes no
/// property from the payload itself, and `serialize()` in `eval()` reads a compiled object's
/// private and protected properties with php's mangled keys.
#[test]
fn test_eval_unserialize_calls_compiled_unserialize_hook() {
    let out = compile_and_run(r###"<?php
final class User
{
    public function __construct(private int $id = 0, private string $name = '') {}

    public function __serialize(): array
    {
        return [$this->id, $this->name];
    }

    public function __unserialize(array $data): void
    {
        [$this->id, $this->name] = $data;
    }

    public function name(): string
    {
        return $this->name;
    }
}

final class Plain
{
    private int $a = 1;
    protected string $b = 'x';
    public array $c = [1];
}

$u = eval('return unserialize(\'O:4:"User":2:{i:0;i:1;i:1;s:4:"jane";}\');');
var_dump($u instanceof User);
echo $u->name(), "\n";
echo eval('return serialize(new Plain());') === serialize(new Plain()) ? "same\n" : "diff\n";
echo str_replace("\0", '~', eval('return serialize(new Plain());')), "\n";
eval('final class Box { public function __construct(public array $items = []) {} public function __serialize(): array { return ["items" => $this->items, 7 => "seven"]; } public function __unserialize(array $d): void { $this->items = [$d["items"], $d[7]]; } }');
$s = eval('return serialize(new Box([1, 2]));');
echo $s, "\n";
echo json_encode(eval('return unserialize($s)->items;')), "\n";
echo json_encode(eval('return get_object_vars(unserialize($s));')), "\n";
"###);
    assert_eq!(out, "bool(true)\njane\nsame\nO:5:\"Plain\":3:{s:8:\"~Plain~a\";i:1;s:4:\"~*~b\";s:1:\"x\";s:1:\"c\";a:1:{i:0;i:1;}}\nO:3:\"Box\":2:{s:5:\"items\";a:2:{i:0;i:1;i:1;i:2;}i:7;s:5:\"seven\";}\n[[1,2],\"seven\"]\n{\"items\":[[1,2],\"seven\"]}\n");
}

/// `serialize()` in `eval()` keeps only the properties `__sleep()` names, in its order, resolving
/// each as a public, then private, then protected property -- for a compiled class and an
/// interpreted one.
#[test]
fn test_eval_serialize_honours_sleep_on_compiled_and_eval_objects() {
    let out = compile_and_run(r###"<?php
class Conn
{
    public string $dsn = 'sqlite::memory:';
    protected int $timeout = 5;
    private string $secret = 'pw';
    private $handle = 'live';

    public function __sleep(): array
    {
        return ['dsn', 'timeout', 'secret'];
    }
}

echo str_replace("\0", '~', eval('return serialize(new Conn());')), "\n";
eval('class EvalConn { public $a = 1; protected $b = 2; private $c = 3; public function __sleep() { return ["c", "a"]; } }');
echo str_replace("\0", '~', eval('return serialize(new EvalConn());')), "\n";
"###);
    assert_eq!(out, "O:4:\"Conn\":3:{s:3:\"dsn\";s:15:\"sqlite::memory:\";s:10:\"~*~timeout\";i:5;s:12:\"~Conn~secret\";s:2:\"pw\";}\nO:8:\"EvalConn\":2:{s:11:\"~EvalConn~c\";i:3;s:1:\"a\";i:1;}\n");
}

/// An `eval()` class that extends a native class and implements a builtin interface is declared
/// when the interface's methods come from the native parent, as php allows.
///
/// Symfony's `InheritDataAwareIterator extends \IteratorIterator implements \RecursiveIterator`
/// declares only `getChildren()`/`hasChildren()`; the declaration was refused (`class ... could not
/// be declared`) and every form with a nested type died.
#[test]
fn test_eval_class_inherits_builtin_interface_methods_from_native_parent() {
    let out = compile_and_run(r###"<?php
eval('namespace Lib;
class NodeIterator extends \IteratorIterator implements \RecursiveIterator
{
    public function getChildren(): static
    {
        return new static(new \ArrayIterator([]));
    }

    public function hasChildren(): bool
    {
        return false;
    }
}');
echo json_encode(eval('return [class_exists("Lib\\\\NodeIterator"), is_subclass_of("Lib\\\\NodeIterator", "RecursiveIterator"), is_subclass_of("Lib\\\\NodeIterator", "IteratorIterator"), (new \ReflectionClass("Lib\\\\NodeIterator"))->hasMethod("valid")];')), "\n";
"###);
    assert_eq!(out, "[true,true,true,true]\n");
}

/// `array_change_key_case()` folds string keys to ASCII lower or upper case (any non-zero `$case`
/// is upper), keeps integer keys, and lets a later folded key overwrite an earlier one in place --
/// from compiled code, a namespaced caller and `eval()`.
///
/// It did not exist on either side, and Symfony's `RedirectResponse` calls it for every redirect.
#[test]
fn test_array_change_key_case_matches_php_in_compiled_and_eval_code() {
    let out = compile_and_run(r###"<?php
namespace App;

$headers = ['Cache-Control' => 'no-cache', 'X-Two' => 'a', 'x-two' => 'b', 5 => 'five', 'Ünicode' => 'u', 'Nested' => ['In' => 1]];
echo json_encode(array_change_key_case($headers)), "\n";
echo json_encode(array_change_key_case($headers, CASE_UPPER)), "\n";
echo json_encode(array_change_key_case($headers, 7)), "\n";
echo json_encode(array_change_key_case([])), "\n";
var_dump(\array_key_exists('cache-control', array_change_key_case($headers, \CASE_LOWER)));
echo json_encode(eval('return array_change_key_case($headers);')), "\n";
echo json_encode(eval('return \array_change_key_case($headers, \CASE_UPPER);')), "\n";
echo json_encode(eval('return array_change_key_case(["A" => 1, "a" => 2, 3 => 3]);')), "\n";
"###);
    assert_eq!(out, "{\"cache-control\":\"no-cache\",\"x-two\":\"b\",\"5\":\"five\",\"\\u00dcnicode\":\"u\",\"nested\":{\"In\":1}}\n{\"CACHE-CONTROL\":\"no-cache\",\"X-TWO\":\"b\",\"5\":\"five\",\"\\u00dcNICODE\":\"u\",\"NESTED\":{\"In\":1}}\n{\"CACHE-CONTROL\":\"no-cache\",\"X-TWO\":\"b\",\"5\":\"five\",\"\\u00dcNICODE\":\"u\",\"NESTED\":{\"In\":1}}\n[]\nbool(true)\n{\"cache-control\":\"no-cache\",\"x-two\":\"b\",\"5\":\"five\",\"\\u00dcnicode\":\"u\",\"nested\":{\"In\":1}}\n{\"CACHE-CONTROL\":\"no-cache\",\"X-TWO\":\"b\",\"5\":\"five\",\"\\u00dcNICODE\":\"u\",\"NESTED\":{\"In\":1}}\n{\"a\":2,\"3\":3}\n");
}

/// `preg_match()`'s `$matches` keeps working when the function also writes into it, which types
/// the destination `array<mixed>`: the runtime's string list is widened to boxed slots on the way
/// in.
///
/// It was stored as it was built, with 16-byte string slots under a static 8-byte Mixed layout,
/// so `isset($matches[3])` read half a string slot as a box pointer and crashed. Symfony's
/// `Request::getLanguageComponents()` is that function, and `/` crashed on every request without
/// an `Accept-Language` header.
#[test]
fn test_preg_match_matches_widen_for_a_written_mixed_destination() {
    let out = compile_and_run_with_regex(r###"<?php
final class Req
{
    private static function getLanguageComponents(string $locale): array
    {
        $locale = str_replace('_', '-', strtolower($locale));
        $pattern = '/^([a-zA-Z]{2,3}|i-[a-zA-Z]{5,})(?:-([a-zA-Z]{4}))?(?:-([a-zA-Z]{2}))?(?:-(.+))?$/';
        if (!preg_match($pattern, $locale, $matches)) {
            return [$locale, null, null];
        }
        if (str_starts_with($matches[1], 'i-')) {
            $matches[1] = substr($matches[1], 2);
        }

        return [
            $matches[1],
            isset($matches[2]) ? ucfirst(strtolower($matches[2])) : null,
            isset($matches[3]) ? strtoupper($matches[3]) : null,
        ];
    }

    public static function format(string $locale): string
    {
        [$language, $script, $region] = self::getLanguageComponents($locale);

        return implode('_', array_filter([$language, $script, $region]));
    }
}

foreach (['ar', 'bg', 'pt_BR', 'sr_Cyrl', 'sr_Latn', 'zh_CN', 'i-cherokee', 'en', 'x', 'de-DE-1996', 'sr-Latn-RS'] as $locale) {
    echo $locale, ' => ', Req::format($locale), "\n";
}
"###);
    assert_eq!(out, "ar => ar\nbg => bg\npt_BR => pt_BR\nsr_Cyrl => sr_Cyrl\nsr_Latn => sr_Latn\nzh_CN => zh_CN\ni-cherokee => cherokee\nen => en\nx => x\nde-DE-1996 => de_DE\nsr-Latn-RS => sr_Latn_RS\n");
}

/// A user method named `getProperties()` (or `getMethods()`, `getReflectionConstants()`) on an
/// `eval()` object is called like any other method, whatever its arguments.
///
/// The ReflectionClass intercepts read the call's arguments as a member filter before checking
/// that the receiver is a reflector, so Symfony UX's `ComponentProperties::getProperties(object,
/// bool)` failed and every page rendering a Twig component (the blog search) died.
#[test]
fn test_eval_user_method_named_like_reflection_member_api_is_dispatched() {
    let out = compile_and_run(r###"<?php
final class SearchComponent
{
    public string $query = 'lorem';
}

eval('namespace Ux;
final class I
{
    public function getProperties(object $component, bool $publicProps = false): array
    {
        return ["x" => 1];
    }

    public function getProps(object $component, bool $publicProps = false): array
    {
        return ["y" => 2];
    }
}
');

$o = eval('return new \Ux\I();');
echo json_encode($o->getProps(new SearchComponent(), true)), "\n";
echo json_encode(eval('return $o->getProperties(new \SearchComponent(), true);')), "\n";
echo json_encode($o->getProperties(new SearchComponent(), true)), "\n";
"###);
    assert_eq!(out, "{\"y\":2}\n{\"x\":1}\n{\"x\":1}\n");
}

/// The Symfony UX shape end to end: an interpreted `getProperties()` drains a private generator
/// that yields a compiled component's public properties and getter values.
#[test]
fn test_eval_generator_backed_properties_method_on_compiled_component() {
    let out = compile_and_run(r###"<?php
final class SearchComponent
{
    public string $query = 'lorem';
    public int $page = 2;
    private string $hidden = 'h';

    public function getQuery(): string
    {
        return $this->query;
    }
}

eval('namespace Ux;
final class Props
{
    private array $classMetadata = [];

    public function getProperties(object $component, bool $publicProps = false): array
    {
        return iterator_to_array($this->extractProperties($component, $publicProps));
    }

    private function extractProperties(object $component, bool $publicProps): \Generator
    {
        yield from $publicProps ? get_object_vars($component) : [];

        $metadata = $this->classMetadata[$component::class] ??= $this->loadClassMetadata($component::class);

        foreach ($metadata["properties"] as $propertyName => $property) {
            yield $propertyName => $property["getter"] ? $component->{$property["getter"]}() : null;
        }
    }

    private function loadClassMetadata(string $class): array
    {
        return ["properties" => ["query" => ["getter" => "getQuery"]]];
    }
}');

$props = eval('return new \Ux\Props();');
echo json_encode($props->getProperties(new SearchComponent(), true)), "\n";
echo json_encode(eval('return $props->getProperties(new \SearchComponent(), false);')), "\n";
"###);
    assert_eq!(out, "{\"query\":\"lorem\",\"page\":2}\n{\"query\":\"lorem\"}\n");
}

/// A generator method's body, resumed from anywhere, appears in `debug_backtrace()` as that
/// method with its `object`, as php lists it.
///
/// Resumed eval generator bodies pushed no call frame, so Symfony UX's
/// `DeterministicTwigIdCalculator`, which looks for the Twig template object in the backtrace
/// (templates render through generator methods), found none and the blog search page failed.
#[test]
fn test_eval_generator_body_appears_in_debug_backtrace_with_its_object() {
    let out = compile_and_run(r###"<?php
abstract class Template
{
    public function display(): string
    {
        $out = '';
        foreach ($this->doDisplay() as $chunk) {
            $out .= $chunk;
        }
        return $out;
    }

    abstract protected function doDisplay(): iterable;
}

eval('final class EvalGuesser { public function guess(): string { $names = []; foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS | DEBUG_BACKTRACE_PROVIDE_OBJECT) as $t) { $names[] = ($t["class"] ?? "") . "::" . $t["function"] . (isset($t["object"]) ? "(obj)" : ""); if (isset($t["object"]) && $t["object"] instanceof \Template) { return get_class($t["object"]) . ":" . $t["function"]; } } return "none " . implode(",", $names); } }
final class EvalTemplate extends \Template { protected function doDisplay(): iterable { yield "a"; yield (new EvalGuesser())->guess(); } }');
$t = eval('return new EvalTemplate();');
echo $t->display(), "\n";
echo eval('return $t->display();'), "\n";
"###);
    assert_eq!(out, "aEvalTemplate:doDisplay\naEvalTemplate:doDisplay\n");
}

/// `new ReflectionObject($o)` in a compiled body reflects an object whose class was declared at
/// runtime.
///
/// The compiled `ReflectionObject` constructor looked the object's runtime class up among the
/// compiled candidates only, so a class declared by an included runtime file died with
/// "ReflectionObject runtime class is not available" -- Symfony UX's
/// `DeterministicTwigIdCalculator` reflects the template object this way.
#[test]
fn test_reflection_object_of_a_runtime_declared_object_from_compiled_code() {
    let out = compile_and_run(r###"<?php
final class Describer
{
    public function describe(object $o): string
    {
        $r = new ReflectionObject($o);
        return $r->getName() . ' ' . ($r->hasMethod('hello') ? 'has hello' : 'no hello') . ' ' . count($r->getProperties()) . ' ' . basename((string) $r->getFileName());
    }
}

$file = __DIR__ . '/runtime_thing.php';
file_put_contents($file, '<?php
class RuntimeThing
{
    public $a = 1;
    public $b = 2;
    public function hello() {}
}
');
require $file;
$c = new RuntimeThing();
echo (new Describer())->describe($c), "\n";
unlink($file);
"###);
    assert_eq!(out, "RuntimeThing has hello 2 runtime_thing.php\n");
}

/// A compiled exception's `getTrace()` lists the interpreted calls that reached it, and stays `[]`
/// where nothing interpreted is live.
///
/// The compiled `getTrace()` was a constant empty array, so Twig's `Error` could never find the
/// template frame it needs to map a line; the eval-to-compiled method call pushed no frame either,
/// so even a recorded trace lacked the call that entered compiled code.
#[test]
fn test_compiled_exception_trace_holds_the_frames_of_the_interpreted_callers() {
    let out = compile_and_run(r###"<?php
class Maker
{
    public function make(): array
    {
        $out = [];
        foreach ((new LogicException('x'))->getTrace() as $t) {
            $out[] = ($t['class'] ?? '') . ($t['type'] ?? '') . $t['function'];
        }
        return $out;
    }

    public static function build(): array
    {
        $out = [];
        foreach ((new LogicException('x'))->getTrace() as $t) {
            $out[] = ($t['class'] ?? '') . ($t['type'] ?? '') . $t['function'];
        }
        return $out;
    }
}

eval('function evalCaller(Maker $m) { return [$m->make(), Maker::build()]; }');
echo json_encode(evalCaller(new Maker())), "\n";
var_dump((new Exception('top'))->getTrace());
"###);
    assert_eq!(out, "[[\"Maker->make\",\"evalCaller\"],[\"Maker::build\",\"evalCaller\"]]\narray(0) {\n}\n");
}

/// A generator method declared in a runtime-included file reads that file as `__FILE__` while a
/// compiled `foreach` resumes it.
///
/// The resumption restored the class scopes of the generator but not its source file, so the body
/// saw the file of whoever called `next()` -- Twig's compiled templates are generators resumed
/// from compiled code.
#[test]
fn test_generator_resumed_from_compiled_code_runs_in_its_own_file() {
    let out = compile_and_run(r###"<?php
$file = __DIR__ . '/gen_file_tpl.php';
file_put_contents($file, '<?php
class TplGen
{
    public function rows(): iterable
    {
        yield basename(__FILE__);
        yield basename(__FILE__) . ":" . __LINE__;
        yield "after:" . basename(__FILE__);
    }
}
');
require $file;
$g = new TplGen();
foreach ($g->rows() as $v) {
    echo $v, "\n";
}
unlink($file);
"###);
    assert_eq!(out, "gen_file_tpl.php\ngen_file_tpl.php:7\nafter:gen_file_tpl.php\n");
}

/// A compiled method reached from a runtime-declared generator body sees a `debug_backtrace()`
/// frame naming the call that entered it, positioned in the runtime file.
///
/// The interpreter pushed no frame for a call into compiled code, and a resumed generator body ran
/// under the resumer's file, so the frame whose `file` is the template and whose `line` is the call
/// never existed -- the lookup Symfony UX uses to derive a live component's deterministic id.
#[test]
fn test_backtrace_frame_of_a_compiled_call_carries_the_interpreted_file_and_line() {
    let out = compile_and_run(r###"<?php
abstract class Template
{
    public function display(): string
    {
        $out = '';
        foreach ($this->doDisplay() as $chunk) {
            $out .= $chunk;
        }
        return $out;
    }

    abstract protected function doDisplay(): iterable;
}

final class Calc
{
    public function id(): string
    {
        $template = null;
        foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS | DEBUG_BACKTRACE_PROVIDE_OBJECT) as $t) {
            if (isset($t['object']) && $t['object'] instanceof Template) {
                $template = $t['object'];
                break;
            }
        }
        if (null === $template) {
            return 'no template';
        }
        $file = (new ReflectionObject($template))->getFileName();
        $e = new LogicException('');
        $found = [];
        foreach ($e->getTrace() as $trace) {
            if (isset($trace['file'], $trace['line']) && $trace['file'] === $file) {
                $found[] = $trace['line'] . ':' . $trace['function'];
            }
        }
        return basename($file) . ' ' . implode(',', $found);
    }
}

final class Runtime
{
    public function render(): string
    {
        return (new Calc())->id();
    }
}

$file = __DIR__ . '/live_tpl.php';
file_put_contents($file, '<?php
final class EvalTemplate extends \Template
{
    public function __construct(private \Runtime $runtime) {}

    protected function doDisplay(): iterable
    {
        yield "a";
        yield $this->runtime->render();
    }
}
');
require $file;
$t = new EvalTemplate(new Runtime());
echo $t->display(), "\n";
unlink($file);
"###);
    assert_eq!(out, "alive_tpl.php 9:render\n");
}

/// A class extending `IteratorIterator` constructs in compiled code and in an `eval()`'d
/// declaration, and so does `IteratorIterator` itself from interpreted code.
///
/// The compiler lowers `new IteratorIterator($x)` inline and never emitted the constructor body,
/// so a subclass without its own constructor was refused at build time, `parent::__construct()`
/// found no body to call, and the interpreter's native-constructor bridge had no slot to
/// dispatch to ("unsupported NewObject expression") -- Symfony Form's `InheritDataAwareIterator`.
#[test]
fn test_iterator_iterator_subclasses_construct_in_compiled_and_interpreted_code() {
    let out = compile_and_run(r###"<?php
class Plain extends IteratorIterator
{
}

class Wrapping extends IteratorIterator
{
    public function __construct(array $items)
    {
        parent::__construct(new ArrayIterator($items));
    }
}

foreach (new Plain(new ArrayIterator([3, 4])) as $v) {
    echo $v, ",";
}
echo "\n";
foreach (new Wrapping([1, 2]) as $v) {
    echo $v, ",";
}
echo "\n";
$code = <<<'CODE'
class RuntimeWrapping extends \IteratorIterator
{
    public function current(): mixed
    {
        return parent::current() * 10;
    }
}
foreach (new \IteratorIterator(new \ArrayIterator([5, 6])) as $v) {
    echo $v, ",";
}
echo "\n";
foreach (new RuntimeWrapping(new \ArrayIterator([7, 8])) as $k => $v) {
    echo $k, "=", $v, ",";
}
echo "\n";
CODE;
eval($code);
"###);
    assert_eq!(out, "3,4,\n1,2,\n5,6,\n0=70,1=80,\n");
}

/// `$value instanceof \Closure` is true for a compiled closure inside a function that also
/// builds a Reflection object.
///
/// Such a function owns an eval context, and the eval-aware `instanceof` only answered for object
/// cells; a compiled closure is a callable descriptor, so the guard was false and
/// `OptionsResolver::setDefault()` never registered a lazy option -- every Symfony form's
/// `compound` default then resolved wrong.
#[test]
fn test_closure_instanceof_holds_in_a_function_that_builds_a_reflection() {
    let out = compile_and_run(r###"<?php
function describeCallable(mixed $value): string
{
    if ($value instanceof \Closure) {
        $reflection = new \ReflectionFunction($value);
        return 'closure with ' . count($reflection->getParameters()) . ' param(s), first named $' . $reflection->getParameters()[0]->getName();
    }
    return 'not a closure: ' . get_debug_type($value);
}

final class Registry
{
    private array $lazy = [];

    public function set(string $name, mixed $value): void
    {
        if ($value instanceof \Closure && (new \ReflectionFunction($value))->getNumberOfParameters() > 0) {
            $this->lazy[] = $name;
        }
    }

    public function lazyNames(): string
    {
        return implode(',', $this->lazy);
    }
}

echo describeCallable(static fn (int $count, string $label) => $count), "\n";
echo describeCallable('strlen'), "\n";
echo describeCallable(new ArrayObject()), "\n";
$registry = new Registry();
$registry->set('lazy', static fn (array $options) => 1);
$registry->set('plain', 5);
$registry->set('noargs', static fn () => 1);
echo $registry->lazyNames(), "\n";
"###);
    assert_eq!(out, "closure with 2 param(s), first named $count\nnot a closure: string\nnot a closure: ArrayObject\nlazy\n");
}

/// An `Error` created by interpreted code carries the frames live at its `new`, so code that maps
/// the frame of a generator-driven template back to a template line finds it.
///
/// Symfony UX's `DeterministicTwigIdCalculator` is itself interpreted: it builds
/// `new Twig\Error\Error('')` from eval, walks `getTrace()` for the frame whose file is the
/// template's and maps its line through `getDebugInfo()`. The interpreted `new` allocated through
/// the generated factory and never recorded a trace, so the lookup ended in "Could not find line
/// number in template" on every page rendering a live component.
#[test]
fn test_template_line_is_found_through_the_trace_of_an_interpreted_throwable() {
    let out = compile_and_run(r###"<?php
class TError extends \Exception
{
    private $lineno;
    private string $phpFile;
    private int $phpLine;

    public function __construct(string $message, int $lineno = -1, ?\Throwable $previous = null)
    {
        parent::__construct('', 0, $previous);
        $this->phpFile = $this->getFile();
        $this->phpLine = $this->getLine();
        $this->lineno = $lineno;
    }
}

abstract class Template
{
    protected array $blocks = [];

    public function display(array $context, array $blocks = []): void
    {
        foreach ($this->yield($context, $blocks) as $data) {
            echo $data;
        }
    }

    public function yield(array $context, array $blocks = []): iterable
    {
        $blocks = array_merge($this->blocks, $blocks);
        try {
            yield from $this->doDisplay($context, $blocks);
        } catch (TError $e) {
            throw $e;
        }
    }

    public function yieldBlock($name, array $context, array $blocks = []): iterable
    {
        $template = $this->blocks[$name][0];
        $block = $this->blocks[$name][1];
        try {
            yield from $template->$block($context, $blocks);
        } catch (TError $e) {
            throw $e;
        }
    }

    abstract public function getTemplateName(): string;

    abstract public function getDebugInfo(): array;

    abstract protected function doDisplay(array $context, array $blocks = []): iterable;
}

eval(<<<'EVALCODE'
class Calc
{
    public function calculate(): string
    {
        $template = null;
        foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS | DEBUG_BACKTRACE_PROVIDE_OBJECT) as $trace) {
            if (isset($trace['object']) && $trace['object'] instanceof Template) {
                $template = $trace['object'];
                break;
            }
        }
        if (null === $template) { return 'none'; }
        $name = $template->getTemplateName();
        $r = new \ReflectionObject($template);
        $file = $r->getFileName();
        $exceptions = [new TError('')];
        while ($e = array_pop($exceptions)) {
            $traces = $e->getTrace();
            array_unshift($traces, ['file' => $e->getFile(), 'line' => $e->getLine()]);
            while ($trace = array_shift($traces)) {
                if (!isset($trace['file']) || !isset($trace['line']) || $file != $trace['file']) {
                    continue;
                }
                foreach ($template->getDebugInfo() as $codeLine => $templateLine) {
                    if ($codeLine <= $trace['line']) {
                        return $name . '-' . $templateLine . ' (php line ' . $trace['line'] . ')';
                    }
                }
            }
        }
        return 'not found';
    }
}

final class Runtime
{
    public function render(string $n, array $a): string
    {
        return (new Calc())->calculate();
    }
}

final class Env
{
    public function getRuntime(string $c): Runtime
    {
        return new Runtime();
    }
}


EVALCODE
);

$file = __DIR__ . '/live_block_tpl.php';
file_put_contents($file, '<?php
class TwigTemplateX extends \Template
{
    private $env;

    public function __construct(\Env $env)
    {
        $this->env = $env;
        $this->blocks = [
            "main" => [$this, "block_main"],
        ];
    }

    protected function doDisplay(array $context, array $blocks = []): iterable
    {
        $macros = [];
        yield from $this->yieldBlock("main", $context, $blocks);
    }

    // line 5
    public function block_main(array $context, array $blocks = []): iterable
    {
        $macros = [];
        // line 6
        yield "    ";
        // line 10
        yield "    ";
        yield $this->env->getRuntime(\'Runtime\')->render("blog_search", ["query" =>         // line 11
($context["query"] ?? null)]);
        // line 12
        yield "
";
        yield from [];
    }

    public function getTemplateName(): string
    {
        return "blog/search.html.twig";
    }

    public function getDebugInfo(): array
    {
        return array (  31 => 12,  29 => 11,  27 => 10,  25 => 6,  18 => 5,  9 => 1,);
    }
}
');
require $file;
$t = new TwigTemplateX(new Env());
$t->display([]);
echo "\n";
unlink($file);
"###);
    assert_eq!(out, "        blog/search.html.twig-10 (php line 28)\n\n");
}

/// A direct by-reference builtin (`sort`, `ksort`, `array_pop`, `array_push`, `settype`) applied
/// to a by-reference PARAMETER of an interpreted function writes through to the caller, and a
/// by-reference `foreach` over such a parameter does too.
///
/// The capture of the argument resolved straight to the caller's variable, so the write released
/// the cell the function's own parameter entry still held; the function then read freed storage
/// and wrote it back at return, leaving the caller's variable an integer. Symfony UX's
/// `LiveComponentHydrator::recursiveKeySort(array &$data)` (`foreach (&) ... ksort($data)`) died
/// on it, which took the live-component search page down.
#[test]
fn test_direct_builtin_on_a_by_reference_parameter_updates_the_caller() {
    let out = compile_and_run(r###"<?php
eval('
function sortIt(array &$data): void { sort($data); }
function popIt(array &$data): void { array_pop($data); }
function pushIt(array &$data): void { array_push($data, 7); }
function typeIt(int &$n): void { settype($n, "string"); }
function keySortAll(array &$data): void {
    foreach ($data as &$value) {
        if (is_array($value)) { keySortAll($value); }
    }
    ksort($data);
}
final class Sorter {
    public function sortAll(array &$data): void {
        foreach ($data as &$value) {
            if (is_array($value)) { $this->sortAll($value); }
        }
        ksort($data);
    }
    public function grow(array &$data): void {
        foreach ($data as &$value) {
            if (is_array($value)) { $value[] = 1; }
        }
    }
    public function run(): void {
        $e = [3, 1, 2]; sortIt($e); echo "sort ", serialize($e), "\n";
        $e = [3, 1, 2]; popIt($e); echo "pop ", serialize($e), "\n";
        $e = [3, 1, 2]; pushIt($e); echo "push ", serialize($e), "\n";
        $n = 5; typeIt($n); echo "settype ", serialize($n), "\n";
        $tree = ["b" => ["z" => 1, "a" => 2], "a" => 5, "c" => ["y" => ["q" => 1, "b" => 2]]];
        $t = $tree; $this->sortAll($t); echo "sortAll ", serialize($t), "\n";
        $t = $tree; keySortAll($t); echo "keySortAll ", serialize($t), "\n";
        $t = ["b" => [3, 2], "a" => 5]; $this->grow($t); echo "grow ", serialize($t), "\n";
    }
}
(new Sorter())->run();
');
"###);
    assert_eq!(out, "sort a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}\npop a:2:{i:0;i:3;i:1;i:1;}\npush a:4:{i:0;i:3;i:1;i:1;i:2;i:2;i:3;i:7;}\nsettype s:1:\"5\";\nsortAll a:3:{s:1:\"a\";i:5;s:1:\"b\";a:2:{s:1:\"a\";i:2;s:1:\"z\";i:1;}s:1:\"c\";a:1:{s:1:\"y\";a:2:{s:1:\"b\";i:2;s:1:\"q\";i:1;}}}\nkeySortAll a:3:{s:1:\"a\";i:5;s:1:\"b\";a:2:{s:1:\"a\";i:2;s:1:\"z\";i:1;}s:1:\"c\";a:1:{s:1:\"y\";a:2:{s:1:\"b\";i:2;s:1:\"q\";i:1;}}}\ngrow a:2:{s:1:\"b\";a:3:{i:0;i:3;i:1;i:2;i:2;i:1;}s:1:\"a\";i:5;}\n");
}

/// A closure whose result is a ternary between a string and an untyped parameter returns the
/// parameter's own value, not a string conversion of it.
///
/// The closure's return type came from the syntactic join, where a string arm absorbs the other,
/// so `fn ($v) => $v instanceof Closure ? 'closure' : $v` returned the byte `"` for `true` and
/// `5`. Symfony's OptionsResolver reads its options through such a closure, which turned every
/// `compound` option false and left each form type a simple form.
#[test]
fn test_closure_ternary_over_an_untyped_parameter_keeps_the_value_type() {
    let out = compile_and_run(r###"<?php
function f1($v) { return $v instanceof Closure ? 'closure' : $v; }
$fn = fn ($v) => $v instanceof Closure ? 'closure' : $v;
$fn2 = function ($v) { return $v === 1 ? 'one' : $v; };
$fn3 = fn ($v) => $v ?: 'empty';
foreach ([true, null, 5, "s", 1.5, [1], 1, 0] as $x) {
    echo json_encode(f1($x)), json_encode($fn($x)), json_encode($fn2($x)), json_encode($fn3($x)), "\n";
}
$options = ["compound" => true, "limit" => 5, "name" => "x", "lazy" => static fn () => 1];
echo json_encode(array_map(fn ($v) => $v instanceof Closure ? 'closure' : $v, $options)), "\n";
"###);
    assert_eq!(out, "truetruetruetrue\nnullnullnull\"empty\"\n5555\n\"s\"\"s\"\"s\"\"s\"\n1.51.51.51.5\n[1][1][1][1]\n11\"one\"1\n000\"empty\"\n{\"compound\":true,\"limit\":5,\"name\":\"x\",\"lazy\":\"closure\"}\n");
}

/// A nested write through an object's array property (`$this->items[$k][] = $v`,
/// `$this->items[$k][$j] = $v`) changes only that object, even when the property array was copied
/// out first or the object was cloned.
///
/// The buckets are boxed cells, and a copied or cloned container shares them. The write reached the
/// shared cell, whose payload sat at refcount 1, so there was nothing to split and the other holder
/// changed too. `OptionsResolver` clones its parent resolver and then registers lazy options and
/// normalizers with exactly these writes, which leaked every child type's options into its parent.
#[test]
fn test_nested_property_writes_do_not_leak_into_a_copy_or_a_clone() {
    let out = compile_and_run(r###"<?php
class Bag {
    public array $items = [];
    public function add(string $k, int $v): void { $this->items[$k][] = $v; }
    public function put(string $k, string $k2, int $v): void { $this->items[$k][$k2] = $v; }
}

// A property array copied out before a nested append / nested set.
$x = new Bag(); $x->items = ["k" => [1], "m" => ["a" => 1]];
$copy = $x->items;
$x->add("k", 2);
$x->put("m", "b", 3);
echo json_encode($copy), json_encode($x->items), "\n";

// Two objects sharing one property array through clone.
$y = clone $x;
$y->add("k", 9);
$y->put("m", "c", 4);
echo json_encode($x->items), json_encode($y->items), "\n";

// The same through an array stored in an option-like registry, as OptionsResolver clones its parent.
$parent = new Bag();
$parent->put("compound", "value", 1);
$parent->add("lazy", 1);
$child = clone $parent;
$child->put("compound", "value", 0);
$child->add("lazy", 2);
echo json_encode($parent->items), json_encode($child->items), "\n";
"###);
    assert_eq!(out, "{\"k\":[1],\"m\":{\"a\":1}}{\"k\":[1,2],\"m\":{\"a\":1,\"b\":3}}\n{\"k\":[1,2],\"m\":{\"a\":1,\"b\":3}}{\"k\":[1,2,9],\"m\":{\"a\":1,\"b\":3,\"c\":4}}\n{\"compound\":{\"value\":1},\"lazy\":[1]}{\"compound\":{\"value\":0},\"lazy\":[1,2]}\n");
}

/// A by-reference `foreach` over `$this->listeners[$event]`, and over each priority bucket in it,
/// writes through to the property.
///
/// Symfony's `EventDispatcher::optimizeListeners()` rewrites every listener this way. It relies on
/// the loop's elements being the property's own cells, so a container split that copied those cells
/// (to keep a shared cell from being written through) left the rewritten listeners in a copy and the
/// dispatcher calling the originals.
#[test]
fn test_by_reference_foreach_over_a_nested_property_array_writes_through() {
    let out = compile_and_run(r###"<?php
class Reg {
    private array $listeners = [];
    public function add(string $event, array $listener, int $priority): void {
        $this->listeners[$event][$priority][] = $listener;
    }
    public function dump(string $event): string {
        return json_encode($this->listeners[$event]);
    }
    public function sorted(string $event): string {
        krsort($this->listeners[$event]);
        return json_encode($this->listeners[$event]);
    }
    public function refloop(string $event): string {
        $n = 0;
        foreach ($this->listeners[$event] as &$listeners) {
            foreach ($listeners as &$listener) {
                $n += count($listener);
            }
        }
        return (string) $n;
    }
    public function mutate(string $event): string {
        foreach ($this->listeners[$event] as &$listeners) {
            foreach ($listeners as &$listener) {
                $listener[0] = "changed";
            }
        }
        return json_encode($this->listeners[$event]);
    }
}
$r = new Reg();
$r->add("e", ["a", "h"], -200);
$r->add("e", ["b", "h"], 0);
$r->add("e", ["c", "h"], 200);
echo $r->dump("e"), "\n";
echo $r->sorted("e"), "\n";
echo $r->refloop("e"), "\n";
echo $r->mutate("e"), "\n";
echo $r->dump("e"), "\n";
"###);
    assert_eq!(out, "{\"-200\":[[\"a\",\"h\"]],\"0\":[[\"b\",\"h\"]],\"200\":[[\"c\",\"h\"]]}\n{\"200\":[[\"c\",\"h\"]],\"0\":[[\"b\",\"h\"]],\"-200\":[[\"a\",\"h\"]]}\n6\n{\"200\":[[\"changed\",\"h\"]],\"0\":[[\"changed\",\"h\"]],\"-200\":[[\"changed\",\"h\"]]}\n{\"200\":[[\"changed\",\"h\"]],\"0\":[[\"changed\",\"h\"]],\"-200\":[[\"changed\",\"h\"]]}\n");
}

/// `Dom\HTMLDocument::createFromString()` builds the tree php builds, and the node classes answer
/// php's questions about it.
///
/// elephc had no `Dom\` classes, so `symfony/html-sanitizer`'s `NativeParser` died on the missing
/// class and every page that renders a sanitized post or comment (the blog post, the admin
/// preview) ended in a RuntimeFatal. The surface is an HTML5 tokenizer and tree builder written
/// as a prelude; its trees were compared with php's own over several thousand generated inputs.
#[test]
fn test_dom_html_document_parses_html5_like_php() {
    let out = compile_and_run(r###"<?php
// Dom\HTMLDocument::createFromString(): the tree php builds for a handful of inputs.
function dumpNode(mixed $n, int $depth = 0): string
{
    $out = str_repeat(' ', $depth) . $n->nodeType . ' ' . $n->nodeName;
    if ($n->nodeType === 1) {
        foreach ($n->attributes as $a) {
            $out .= ' [' . $a->name . '=' . $a->value . ']';
        }
    }
    if ($n->nodeType === 3 || $n->nodeType === 8) {
        $out .= ' ' . json_encode($n->data);
    }
    $out .= "\n";
    foreach ($n->childNodes as $c) {
        $out .= dumpNode($c, $depth + 1);
    }
    return $out;
}

$inputs = [
    '<!DOCTYPE html><body><p class="a b" ID=x>Hello <b>wor<i>ld</b></i> &amp; &nbsp;&copy; &bogus; &#65;<br>y<!-- c --></p></body>',
    '<!DOCTYPE html><body><ul><li>a<li>b</ul><table><tr><td>1<td>2</table></body>',
    '<!DOCTYPE html><body><table>text<tr><td>x</td></tr></table></body>',
    '<!DOCTYPE html><body><a href=x>1<a href=y>2</a></a><p>one<p>two</body>',
    '<!DOCTYPE html><body><svg viewbox="0 0 1 1"><circle r=1 /><p>out</p></svg><math><mi>x</mi></math></body>',
    '<!DOCTYPE html><body><script>var a = "<b>";</script><textarea><b>x</b></textarea><pre>' . "\n" . 'line</pre></body>',
];
foreach ($inputs as $html) {
    echo dumpNode(@Dom\HTMLDocument::createFromString($html)), "--\n";
}

$d = @Dom\HTMLDocument::createFromString('<!DOCTYPE html><body><p id="a" class="b c">Hi <b>there</b></p><p>two</p><img src=x></body>');
$ps = $d->getElementsByTagName('p');
echo $ps->length, ' ', $ps->item(0)->textContent, ' ', $ps->item(1)->id, ' ', $d->body->textContent, ' ', $d->getElementById('a')->className, "\n";
echo $d->doctype->name, ' ', $d->documentElement->nodeName, ' ', $d->head->nodeName, ' ', $d->body->childElementCount, "\n";
$p = $ps->item(0);
echo json_encode([$p->getAttribute('class'), $p->hasAttribute('id'), $p->getAttribute('nope'), $p->hasChildNodes(), $p->firstChild->nodeName, $p->lastChild->nodeName]), "\n";
echo json_encode($p->getAttributeNames()), ' ', $p->attributes->length, ' ', $p->attributes->getNamedItem('class')->value, "\n";
echo $p->parentNode->nodeName, ' ', $p->nextElementSibling->nodeName, ' ', $p->nextSibling->nodeName, ' ', $p->previousSibling === null ? 'none' : 'some', "\n";
$q = $d->createElement('section');
$q->setAttribute('data-k', 'v');
$d->body->appendChild($q);
echo $d->body->childElementCount, ' ', $d->body->lastChild->nodeName, ' ', $q->getAttribute('data-k'), ' ', count($d->body->childNodes), "\n";
$d->body->removeChild($q);
echo $d->body->childElementCount, ' ', $q->parentNode === null ? 'detached' : 'attached', "\n";
"###);
    assert_eq!(out, "13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   1 P [class=a b] [id=x]\n    3 #text \"Hello \"\n    1 B\n     3 #text \"wor\"\n     1 I\n      3 #text \"ld\"\n    3 #text \" & \\u00a0\\u00a9 &bogus; A\"\n    1 BR\n    3 #text \"y\"\n    8 #comment \" c \"\n--\n13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   1 UL\n    1 LI\n     3 #text \"a\"\n    1 LI\n     3 #text \"b\"\n   1 TABLE\n    1 TBODY\n     1 TR\n      1 TD\n       3 #text \"1\"\n      1 TD\n       3 #text \"2\"\n--\n13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   3 #text \"text\"\n   1 TABLE\n    1 TBODY\n     1 TR\n      1 TD\n       3 #text \"x\"\n--\n13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   1 A [href=x]\n    3 #text \"1\"\n   1 A [href=y]\n    3 #text \"2\"\n   1 P\n    3 #text \"one\"\n   1 P\n    3 #text \"two\"\n--\n13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   1 svg [viewBox=0 0 1 1]\n    1 circle [r=1]\n   1 P\n    3 #text \"out\"\n   1 math\n    1 mi\n     3 #text \"x\"\n--\n13 #document\n 10 html\n 1 HTML\n  1 HEAD\n  1 BODY\n   1 SCRIPT\n    3 #text \"var a = \\\"<b>\\\";\"\n   1 TEXTAREA\n    3 #text \"<b>x<\\/b>\"\n   1 PRE\n    3 #text \"line\"\n--\n2 Hi there  Hi theretwo b c\nhtml HTML HEAD 3\n[\"b c\",true,null,true,\"#text\",\"B\"]\n[\"id\",\"class\"] 2 b c\nBODY P P none\n4 SECTION v 4\n3 detached\n");
}

/// `count()` of a `Countable` declared by `eval()`, from compiled code, calls its `count()`.
///
/// The object has no compiled class id, so the runtime's `_class_count_ptrs` table found no
/// `count` and the call ended in "must be of type Countable|array, stdClass given". Twig's
/// `|length` filter hit it on a Doctrine `PersistentCollection` (`post.comments|length`).
#[test]
fn test_count_of_a_countable_the_interpreter_declared() {
    let out = compile_and_run(r###"<?php
function aotLength(mixed $thing): int
{
    if (is_countable($thing)) {
        return count($thing);
    }
    return -1;
}
function aotLength2(mixed $thing): int
{
    return $thing instanceof Countable ? count($thing) : -2;
}
eval('
final class EvalBag implements Countable, IteratorAggregate {
    public function __construct(private array $items) {}
    public function count(): int { return count($this->items); }
    public function getIterator(): Iterator { return new ArrayIterator($this->items); }
}
function mkBag() { return new EvalBag([1, 2, 3]); }
');
$bag = mkBag();
echo aotLength($bag), " ", aotLength2($bag), " ", count($bag), "\n";
echo aotLength([1, 2]), " ", aotLength("x"), "\n";
"###);
    assert_eq!(out, "3 3 3\n2 -1\n");
}

/// `count()` of a value typed as the `Countable` interface (a parameter, a narrowed `mixed`)
/// dispatches through the interface tables.
///
/// The lowering sent an interface-typed receiver to the "any other object" arm, which raises
/// "Countable given" for the interface itself.
#[test]
fn test_count_of_an_interface_typed_countable() {
    let out = compile_and_run(r###"<?php
final class Bag implements Countable {
    public function __construct(private array $items) {}
    public function count(): int { return count($this->items); }
}
function viaInterface(Countable $c): int { return count($c); }
function viaNarrow(mixed $x): int { return $x instanceof Countable ? count($x) : -2; }
function viaUnion(Countable|array $c): int { return count($c); }
$b = new Bag([1, 2, 3]);
echo viaInterface($b), " ", viaNarrow($b), " ", viaUnion($b), " ", viaUnion([1]), " ", viaNarrow(5), "\n";
"###);
    assert_eq!(out, "3 3 3 1 -2\n");
}

/// `new IteratorIterator($aggregate)` / `new RecursiveIteratorIterator($aggregate)` inside `eval()`
/// iterate what the aggregate's `getIterator()` returns, when the aggregate is a class the
/// interpreter declared.
///
/// The compiled constructor recognizes an aggregate by its compiled class id, which such a class
/// does not have, so it ended in "Cannot assign object to property of type Iterator".
/// `Form::setData()` wraps its children (an `OrderedHashMap`) exactly this way, which made every
/// form the demo renders a 500.
#[test]
fn test_iterator_iterator_unwraps_an_aggregate_the_interpreter_declared() {
    let out = compile_and_run(r###"<?php
eval('
final class NativeAgg implements IteratorAggregate {
    public function __construct(private array $items) {}
    public function getIterator(): Traversable { return new ArrayIterator($this->items); }
}
final class EvalIter implements Iterator {
    private int $i = 0;
    public function __construct(private array $items) {}
    public function current(): mixed { return array_values($this->items)[$this->i]; }
    public function key(): mixed { return array_keys($this->items)[$this->i]; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < count($this->items); }
}
final class EvalAgg implements IteratorAggregate {
    public function __construct(private array $items) {}
    public function getIterator(): Traversable { return new EvalIter($this->items); }
}
final class EvalRecIter extends IteratorIterator implements RecursiveIterator {
    public function getChildren(): ?RecursiveIterator { return null; }
    public function hasChildren(): bool { return false; }
}
final class EvalAggRec implements IteratorAggregate {
    public function __construct(private array $items) {}
    public function getIterator(): Traversable { return new EvalRecIter(new ArrayIterator($this->items)); }
}
function run(string $label, callable $make) {
    try {
        $out = [];
        foreach ($make() as $k => $v) { $out[] = $k . "=" . $v; }
        echo $label, ": ", implode(",", $out), "\n";
    } catch (Throwable $e) {
        echo $label, ": ", get_class($e), " ", $e->getMessage(), "\n";
    }
}
run("native agg", fn () => new IteratorIterator(new NativeAgg(["a" => 1])));
run("eval iter", fn () => new IteratorIterator(new EvalIter(["a" => 1])));
run("eval agg", fn () => new IteratorIterator(new EvalAgg(["a" => 1])));
run("recursive", fn () => new RecursiveIteratorIterator(new EvalAggRec(["a" => 1])));
');
"###);
    assert_eq!(out, "native agg: a=1\neval iter: a=1\neval agg: a=1\nrecursive: a=1\n");
}

/// `clone $o` of an object whose class `eval()` declared gave the clone the SAME cell for every
/// array property, so `$clone->p[] = ...` and `$clone->p[$k][] = ...` wrote the original's array.
/// Symfony's `OptionsResolver` clones itself per child type; the child's defaults leaked into
/// the parent and every form after the first lost its `compound` option.
#[test]
fn test_eval_clone_does_not_share_array_properties() {
    let out = compile_and_run(r###"<?php
eval('
class N {
    public array $b = [];
    public function app2(string $k, int $v): void { $this->b[$k][] = $v; }
}
function mk(): array { $a = [1]; return ["k" => $a]; }
// 1: literal assigned from outside, clone, append in clone
$x = new N(); $x->b = ["k" => [1]];
$y = clone $x; $y->app2("k", 2);
echo "1 ", json_encode([$x->b, $y->b]), "\\n";
// 2: built dynamically
$x = new N(); $x->b = mk();
$y = clone $x; $y->app2("k", 2);
echo "2 ", json_encode([$x->b, $y->b]), "\\n";
// 3: built by appends through the method
$x = new N(); $x->app2("k", 1);
$y = clone $x; $y->app2("k", 2);
echo "3 ", json_encode([$x->b, $y->b]), "\\n";
// 4: copy of the property without clone
$x = new N(); $x->b = ["k" => [1]];
$copy = $x->b; $x->app2("k", 2);
echo "4 ", json_encode([$copy, $x->b]), "\\n";
// 5: assignment of one object\'s array property to another
$x = new N(); $x->b = ["k" => [1]];
$y = new N(); $y->b = $x->b; $y->app2("k", 2);
echo "5 ", json_encode([$x->b, $y->b]), "\\n";
// 6: clone then replace the whole array element
$x = new N(); $x->b = ["k" => [1], "j" => [7]];
$y = clone $x; $y->app2("j", 8);
echo "6 ", json_encode([$x->b, $y->b]), "\\n";
');
"###);
    assert_eq!(out, "1 [{\"k\":[1]},{\"k\":[1,2]}]\n2 [{\"k\":[1]},{\"k\":[1,2]}]\n3 [{\"k\":[1]},{\"k\":[1,2]}]\n4 [{\"k\":[1]},{\"k\":[1,2]}]\n5 [{\"k\":[1]},{\"k\":[1,2]}]\n6 [{\"k\":[1],\"j\":[7]},{\"k\":[1],\"j\":[7,8]}]\n");
}

/// A nested write through a property array that a clone (or a copy) shares: the inner array's own
/// count only reflects its container, so it must be copied before it is written through.
#[test]
fn test_eval_clone_nested_array_write_is_copy_on_write() {
    let out = compile_and_run(r###"<?php
eval('
class N { public array $b = [];
 function s1($k,$v){ $this->b[$k] = $v; }
 function s2($k,$v){ $this->b[$k][0] = $v; }
 function a1($v){ $this->b[] = $v; }
 function a2($k,$v){ $this->b[$k][] = $v; } }
$x = new N(); $x->b = ["k" => [1]];
$y = clone $x; $y->s1("z", 5); echo json_encode([$x->b,$y->b]), "\n";
$x = new N(); $x->b = ["k" => [1]];
$y = clone $x; $y->s2("k", 5); echo json_encode([$x->b,$y->b]), "\n";
$x = new N(); $x->b = ["k" => [1]];
$y = clone $x; $y->a1(5); echo json_encode([$x->b,$y->b]), "\n";
$x = new N(); $x->b = ["k" => [1]];
$y = clone $x; $y->a2("k", 5); echo json_encode([$x->b,$y->b]), "\n";
');
"###);
    assert_eq!(out, "[{\"k\":[1]},{\"k\":[1],\"z\":5}]\n[{\"k\":[1]},{\"k\":[5]}]\n[{\"k\":[1]},{\"k\":[1],\"0\":5}]\n[{\"k\":[1]},{\"k\":[1,5]}]\n");
}

/// A chained assignment of an array (`$a = $b = []`) gives each target its own array, in eval.
///
/// The inner assignment returns the cell it stored, and the outer one stored that same cell, so
/// every property of the chain aliased one array: Doctrine's `UnitOfWork::clear()` assigns
/// `$this->entityIdentifiers = $this->entityStates = ... = []`, after which writing the identifier
/// map also wrote the state map and the original-data map, and loading an entity died in
/// `implode()` on a `DateTimeImmutable`.
#[test]
fn test_eval_chained_array_assignment_gives_each_target_its_own_array() {
    let out = compile_and_run(r###"<?php
$code = '
class Chain {
    private array $a = [1];
    private array $b = [2];
    private array $c = [3];
    private array $d = [4];
    public function clear(): void {
        $this->a =
        $this->b =
        $this->c =
        $this->d = [];
    }
    public function put(int $k): void {
        $this->a[$k] = "a";
        $this->b[$k] = "b";
        $this->c[$k] = "c";
    }
    public function dump(): string { return json_encode([$this->a, $this->b, $this->c, $this->d]); }
}
$x = new Chain();
$x->clear();
$x->put(5);
echo $x->dump(), "\n";
$x->put(6);
echo $x->dump(), "\n";
$l1 = $l2 = [];
$l1[] = 1;
echo json_encode([$l1, $l2]), "\n";
';
eval($code);
"###);
    assert_eq!(out, "[{\"5\":\"a\"},{\"5\":\"b\"},{\"5\":\"c\"},[]]\n[{\"5\":\"a\",\"6\":\"a\"},{\"5\":\"b\",\"6\":\"b\"},{\"5\":\"c\",\"6\":\"c\"},[]]\n[[1],[]]\n");
}

/// `(array) $object` of an `eval()`-declared object mangles protected and private keys.
///
/// The interpreter's cast walked public properties only, so `$item[" * key"]` was null.
/// Symfony's `ArrayAdapter::save()` reads a `CacheItem` exactly this way and then called
/// `getItem(null)`: every validator-metadata cache save died with "Cache key must be string,
/// "null" given", which is what turned the admin edit and new-post pages into HTTP 500.
#[test]
fn test_eval_array_cast_of_an_eval_object_mangles_protected_and_private_keys() {
    let out = compile_and_run(r###"<?php
class CI {
    protected string $key;
    protected mixed $value = null;
    protected ?float $expiry = null;
    private array $tags = [];
    public $pub = 1;
    public function __construct(string $k) { $this->key = $k; }
    public function set(mixed $v): static { $this->value = $v; return $this; }
}
class Pool {
    public function save(object $item): bool {
        if (!$item instanceof CI) { return false; }
        $item = (array) $item;
        echo json_encode(array_map('strlen', array_keys($item))), "\n";
        $key = $item["\0*\0key"];
        $value = $item["\0*\0value"];
        $expiry = $item["\0*\0expiry"];
        echo var_export($key, true), var_export($value, true), var_export($expiry, true), "\n";
        return true;
    }
}
$p = new Pool();
$i = new CI('a.b');
$p->save($i->set([1, 2]));
$code = '
class CIE { protected string $key; protected mixed $value = null; protected ?float $expiry = null; public function __construct(string $k) { $this->key = $k; } public function set(mixed $v): static { $this->value = $v; return $this; } }
$i = new CIE("e.k");
$a = (array) $i->set("v");
echo json_encode(array_map("strlen", array_keys($a))), "\n";
echo var_export($a["\0*\0key"], true), "\n";
';
eval($code);
"###);
    assert_eq!(out, "[6,8,9,8,3]\n'a.b'array (\n  0 => 1,\n  1 => 2,\n)NULL\n[6,8,9]\n'e.k'\n");
}

/// `$x?->m($arg)` on an interface-typed receiver reaches an implementation that stores the
/// parameter differently from the interface.
///
/// The nullsafe call was lowered straight through the interface table with the INTERFACE's
/// argument storage (`string $key`), so an implementation declaring `mixed $key` (or an untyped
/// one) read a string's registers as a box and saw null or "". A plain `->` call on the same
/// receiver already went through the class-id ladder that lowers against each implementation's own
/// signature; the nullsafe call, whether standalone or inside a `?->` chain, now does too.
/// Symfony's validator metadata factory writes `$this->cache?->getItem($key)` against
/// `CacheItemPoolInterface`, and every `ArrayAdapter` lookup died "Cache key must be string,
/// "null" given".
#[test]
fn test_nullsafe_call_on_an_interface_receiver_binds_each_implementations_parameters() {
    let out = compile_and_run(r###"<?php
class Item { public function __construct(public mixed $key) {} }
interface PoolI { public function getItem(string $key): Item; }
class PoolA implements PoolI {
    public function getItem(mixed $key): Item {
        if (!is_string($key)) { throw new InvalidArgumentException('A: ' . get_debug_type($key)); }
        return new Item($key);
    }
}
class PoolB implements PoolI {
    public function getItem($key): Item { return new Item('B:' . $key); }
}
class PoolC implements PoolI {
    public function getItem(string $key): Item { return new Item('C:' . $key); }
}
class Holder {
    public function __construct(protected ?PoolI $cache = null) {}
    public function viaNullsafe(string $k): string {
        $item = $this->cache?->getItem($this->esc($k));
        return var_export($item?->key, true);
    }
    public function viaPlain(string $k): string {
        $item = $this->cache->getItem($this->esc($k));
        return var_export($item->key, true);
    }
    public function viaNullsafeLocal(string $k): string {
        $key = $this->esc($k);
        $item = $this->cache?->getItem($key);
        return var_export($item?->key, true);
    }
    private function esc(string $c): string { return str_replace('\\', '.', $c); }
}
foreach ([new PoolA(), new PoolB(), new PoolC()] as $pool) {
    $h = new Holder($pool);
    foreach (['viaNullsafe', 'viaPlain', 'viaNullsafeLocal'] as $m) {
        try {
            echo get_class($pool), ' ', $m, ' ', $h->$m('A\\B'), "\n";
        } catch (Throwable $e) {
            echo get_class($pool), ' ', $m, ' threw ', $e->getMessage(), "\n";
        }
    }
}
"###);
    assert_eq!(out, "PoolA viaNullsafe 'A.B'\nPoolA viaPlain 'A.B'\nPoolA viaNullsafeLocal 'A.B'\nPoolB viaNullsafe 'B:A.B'\nPoolB viaPlain 'B:A.B'\nPoolB viaNullsafeLocal 'B:A.B'\nPoolC viaNullsafe 'C:A.B'\nPoolC viaPlain 'C:A.B'\nPoolC viaNullsafeLocal 'C:A.B'\n");
}

/// `array_replace_recursive()` exists in `eval()`.
///
/// It was on the interpreter's pending list, so eval code calling it died with "call to undefined
/// function array_replace_recursive()". Symfony's form option merging reaches it on the admin
/// edit and new-post pages: every matching key is overwritten in place or merged one level down
/// when both sides are arrays, new keys are appended, int keys are matched not renumbered, and
/// the result's list shape follows its final keys.
#[test]
fn test_eval_array_replace_recursive_merges_nested_arrays() {
    let out = compile_and_run(r###"<?php
$code = '
echo ser(array_replace_recursive(["a" => 1, "b" => ["x" => 1, "y" => 2], "c" => 3], ["b" => ["y" => 9, "z" => 8], "d" => 4])), "\n";
echo ser(array_replace_recursive([1, 2, [3, 4]], [9, 8, [7]])), "\n";
echo ser(array_replace_recursive(["a" => ["b" => ["c" => 1]]], ["a" => ["b" => ["d" => 2]]], ["a" => ["e" => 3]])), "\n";
echo ser(array_replace_recursive(["a" => [1, 2, 3]], ["a" => "scalar"])), "\n";
echo ser(array_replace_recursive(["a" => "scalar"], ["a" => [1]])), "\n";
echo ser(array_replace_recursive([5 => "a", 9 => "b"], [9 => "c", 2 => "d"])), "\n";
echo json_encode(array_replace_recursive([1, 2], [3])), json_encode(array_replace_recursive(["a" => 1], [])), "\n";
$base = ["k" => ["n" => 1]];
$out = array_replace_recursive($base, ["k" => ["n" => 2]]);
echo ser($base), " ", ser($out), "\n";
try { array_replace_recursive([1], 5); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
';
eval('function ser($a) {
    $s = "";
    foreach ($a as $k => $v) {
        $s .= (is_int($k) ? "i" : "s") . ":" . $k . "=";
        if (is_array($v)) { $s .= "[" . ser($v) . "]"; }
        elseif (is_null($v)) { $s .= "NULL"; }
        elseif (is_bool($v)) { $s .= $v ? "true" : "false"; }
        elseif (is_string($v)) { $s .= "\'" . $v . "\'"; }
        else { $s .= $v; }
        $s .= ";";
    }
    return $s;
}' . $code);
"###);
    assert_eq!(out, "s:a=1;s:b=[s:x=1;s:y=9;s:z=8;];s:c=3;s:d=4;\ni:0=9;i:1=8;i:2=[i:0=7;i:1=4;];\ns:a=[s:b=[s:c=1;s:d=2;];s:e=3;];\ns:a='scalar';\ns:a=[i:0=1;];\ni:5='a';i:9='c';i:2='d';\n[3,2]{\"a\":1}\ns:k=[s:n=1;]; s:k=[s:n=2;];\narray_replace_recursive(): Argument #2 must be of type array, int given\n");
}

/// `new Holder($this->items)` against `__construct(private array &$data)` keeps the property and
/// the holder's array one array.
///
/// A native call copied the property into a temporary, so `$holder->push()` wrote a copy and the
/// owner never saw it (php: `[1,2,3]`, compiled: `[1]`). Both classes come from the autoloader,
/// so the compiler hands the pair to the runtime autoloader, where references are implemented.
/// Symfony's `OrderedHashMap::getIterator()` builds its `OrderedHashMapIterator` this way, which
/// killed every page that iterates a form.
#[test]
fn test_by_reference_constructor_argument_from_a_property_stays_linked() {
    let out = compile_cli_files_and_run(&[("composer.json", r###"{"autoload":{"psr-4":{"Acme\\":"src/"}}}
"###), ("main.php", r###"<?php
class Loader {
    public static function load(string $class): void {
        $file = __DIR__ . '/src/' . substr($class, 5) . '.php';
        if (is_file($file)) { require $file; }
    }
}
spl_autoload_register(['Loader', 'load']);
$o = new Acme\Owner();
$h = $o->make();
$h->push(2);
$h->push(3);
echo json_encode($o->items), "\n";
"###), ("src/Holder.php", r###"<?php
namespace Acme;

class Holder {
    private int $cursor = 0;
    public function __construct(private array &$data) {}
    public function push(int $v): void { $this->data[] = $v; }
}
"###), ("src/Owner.php", r###"<?php
namespace Acme;

class Owner {
    public array $items = [1];
    public function make(): Holder { return new Holder($this->items); }
}
"###)], "main.php");
    assert_eq!(out, "[1,2,3]\n");
}

/// `new IteratorIterator($aggregate)` accepts an IteratorAggregate source.
///
/// The omitted optional `?string $class` argument reaches the constructor as a boxed null; the
/// lowering took every boxed value for an invalid downcast class and threw "Class to downcast to
/// not found or not base class or does not implement Traversable" for every aggregate (an
/// iterator source was unaffected). Twig's `|filter` wraps its input this way, so every form theme
/// block that filters a `FormView` died.
#[test]
fn test_iterator_iterator_wraps_an_aggregate_given_the_default_downcast() {
    let out = compile_and_run(r###"<?php
class AotAgg implements IteratorAggregate {
    public array $children = ['a' => 1, 'b' => 2];
    public function getIterator(): ArrayIterator { return new ArrayIterator($this->children); }
}
function wrapMixed($array) {
    $o = '';
    foreach (new IteratorIterator($array) as $k => $v) { $o .= "$k=$v,"; }
    return $o;
}
function wrapTyped(Traversable $array) {
    $o = '';
    foreach (new IteratorIterator($array) as $k => $v) { $o .= "$k=$v,"; }
    return $o;
}
function wrapAgg(IteratorAggregate $array) {
    $o = '';
    foreach (new IteratorIterator($array) as $k => $v) { $o .= "$k=$v,"; }
    return $o;
}
echo wrapTyped(new AotAgg()), "\n";
echo wrapAgg(new AotAgg()), "\n";
echo wrapMixed(new AotAgg()), "\n";
echo wrapMixed(new ArrayIterator([1, 2])), "\n";
"###);
    assert_eq!(out, "a=1,b=2,\na=1,b=2,\na=1,b=2,\n0=1,1=2,\n");
}

/// Compiled `new IteratorIterator($x)` and `CallbackFilterIterator` accept a source that `eval()`
/// declared: an Iterator, an IteratorAggregate, a generator-backed aggregate.
///
/// The compiled constructor recognizes interfaces by compiled class id, which an interpreter-
/// declared class does not have, so every such source ended in "foreach over iterable with
/// unsupported kind". The aggregate is now unwrapped through the bridge (repeatedly, as php does
/// when `getIterator()` hands back another aggregate) and an interpreter-owned Iterator is kept as
/// the inner iterator. Twig's `|filter` over a form view built by the interpreter hit this.
#[test]
fn test_iterator_iterator_wraps_sources_the_interpreter_declared() {
    let out = compile_and_run(r###"<?php
eval('
class EvIter implements Iterator {
    private int $i = 0;
    public function __construct(private array $a) {}
    public function current(): mixed { return $this->a[$this->i]; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < count($this->a); }
}
class EvAgg implements IteratorAggregate, Countable {
    public function __construct(private array $a) {}
    public function getIterator(): Traversable { return new EvIter($this->a); }
    public function count(): int { return count($this->a); }
}
class EvAggGen implements IteratorAggregate {
    public function getIterator(): Traversable { yield "x" => 1; yield "y" => 2; }
}
class EvAggAI implements IteratorAggregate {
    public function getIterator(): Traversable { return new ArrayIterator(["p" => 1, "q" => 2]); }
}
');
function wrapFilter($array, callable $f) {
    return new CallbackFilterIterator(new IteratorIterator($array), $f);
}
function wrapOnly($array) {
    $o = '';
    foreach (new IteratorIterator($array) as $k => $v) { $o .= "$k=$v,"; }
    return $o;
}
$names = ['EvIter' => [[5, 6]], 'EvAgg' => [[7, 8]], 'EvAggGen' => [], 'EvAggAI' => []];
foreach ($names as $cls => $args) {
    $obj = new $cls(...$args);
    try { echo $cls, ' only: ', wrapOnly($obj), "\n"; } catch (Throwable $e) { echo $cls, ' only threw ', $e->getMessage(), "\n"; }
    try {
        $o = '';
        foreach (wrapFilter($obj, fn ($v) => $v > 0) as $k => $v) { $o .= "$k=$v,"; }
        echo $cls, ' filter: ', $o, "\n";
    } catch (Throwable $e) { echo $cls, ' filter threw ', $e->getMessage(), "\n"; }
}
"###);
    assert_eq!(out, "EvIter only: 0=5,1=6,\nEvIter filter: 0=5,1=6,\nEvAgg only: 0=7,1=8,\nEvAgg filter: 0=7,1=8,\nEvAggGen only: x=1,y=2,\nEvAggGen filter: x=1,y=2,\nEvAggAI only: p=1,q=2,\nEvAggAI filter: p=1,q=2,\n");
}

/// `json_encode()` inside `eval()` honours `JsonSerializable`.
///
/// The interpreter's encoder walked an object's properties and never asked for `jsonSerialize()`,
/// so a list of `Tag` entities encoded as `[{},{}]` where php answers `["lorem","ipsum"]`. An
/// object that returns itself from `jsonSerialize()` encodes as a plain object, and the result may
/// itself contain serializable objects.
#[test]
fn test_eval_json_encode_honours_json_serializable() {
    let out = compile_and_run(r###"<?php
eval('
class TagE implements JsonSerializable {
    public function __construct(private string $name) {}
    public function jsonSerialize(): string { return $this->name; }
}
class SelfE implements JsonSerializable {
    public $a = 1;
    public function jsonSerialize(): mixed { return $this; }
}
class ArrE implements JsonSerializable {
    public function jsonSerialize(): array { return ["k" => new TagE("nested"), "n" => [1, 2]]; }
}
class PlainE { public $a = 1; protected $b = 2; }
$tags = [new TagE("lorem"), new TagE("ipsum")];
echo json_encode($tags), "\n";
echo json_encode(new TagE("x")), "\n";
echo json_encode(["k" => new TagE("y"), "p" => new PlainE(), "s" => new SelfE()]), "\n";
echo json_encode(new ArrE()), "\n";
echo json_encode($tags, JSON_PRETTY_PRINT), "\n";
');
"###);
    assert_eq!(out, "[\"lorem\",\"ipsum\"]\n\"x\"\n{\"k\":\"y\",\"p\":{\"a\":1},\"s\":{\"a\":1}}\n{\"k\":\"nested\",\"n\":[1,2]}\n[\n    \"lorem\",\n    \"ipsum\"\n]\n");
}

/// An eval-declared `IteratorAggregate::getIterator()` may narrow its return type to a native
/// Traversable class other than `ArrayIterator`.
///
/// Only `ArrayIterator` was known to be a `Traversable` when the interpreter validated the
/// declaration, so `getIterator(): \EmptyIterator` (Symfony's `Button`) was a fatal "class could
/// not be declared" and every form with a submit button died.
#[test]
fn test_eval_get_iterator_may_narrow_to_a_native_traversable_class() {
    let out = compile_and_run(r###"<?php
$code = '
class C1 implements IteratorAggregate { public function getIterator(): \EmptyIterator { return new \EmptyIterator(); } }
class C2 implements IteratorAggregate { public function getIterator(): \ArrayIterator { return new \ArrayIterator([1]); } }
class C3 implements IteratorAggregate { public function getIterator(): \Generator { yield 1; } }
class C4 implements IteratorAggregate { public function getIterator(): \Iterator { return new \ArrayIterator([2]); } }
class C5 implements Countable, ArrayAccess, IteratorAggregate {
    public function count(): int { return 0; }
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetSet(mixed $o, mixed $v): void {}
    public function offsetUnset(mixed $o): void {}
    public function getIterator(): \EmptyIterator { return new \EmptyIterator(); }
}
foreach (["C1", "C2", "C3", "C4", "C5"] as $c) { $o = new $c(); echo $c, ":", get_class($o), ":", ($o instanceof Traversable ? "T" : "-"), " "; }
echo "\n";
';
eval($code);
"###);
    assert_eq!(out, "C1:C1:T C2:C2:T C3:C3:T C4:C4:T C5:C5:T \n");
}

/// Compiled `json_encode()` of an object `eval()` declared honours `JsonSerializable`.
///
/// An interpreter-owned object carries the generic stdClass wrapper header, so the compiled
/// encoder took it for a stdClass and printed `{}` (or looked past its descriptor table). It now
/// asks the bridge what to encode. Twig's `tags|json_encode` over the blog post's `Tag` entities
/// printed `[{},{},...]` instead of the tag names.
#[test]
fn test_json_encode_of_an_interpreter_declared_json_serializable() {
    let out = compile_and_run(r###"<?php
eval('
class TagE implements JsonSerializable {
    public function __construct(private string $name) {}
    public function jsonSerialize(): string { return $this->name; }
}
');
function mk(): mixed { return new TagE('m'); }
$o = new TagE('x');
echo json_encode($o), "\n";
echo json_encode(mk()), "\n";
echo json_encode([$o]), "\n";
echo json_encode(['k' => $o]), "\n";
function arr(array $a) { return json_encode($a); }
function obj(object $a) { return json_encode($a); }
function mix($a) { return json_encode($a); }
echo arr([$o, $o]), "\n";
echo obj($o), "\n";
echo mix($o), "\n";
echo mix([$o]), "\n";
echo mix(['k' => $o]), "\n";
"###);
    assert_eq!(out, "\"x\"\n\"m\"\n[\"x\"]\n{\"k\":\"x\"}\n[\"x\",\"x\"]\n\"x\"\n\"x\"\n[\"x\"]\n{\"k\":\"x\"}\n");
}

/// A closure `eval()` created can be passed to a compiled function whose `callable` parameter the
/// compiler inferred from compiled callers that passed compiled closures.
///
/// A `callable` slot holds a descriptor pointer; the interpreter's closure is an object, so the
/// compiled callee read its invoke slot out of the object's bytes and jumped to garbage (SIGBUS).
/// The interpreter's call boundary now adapts such a closure into a descriptor that dispatches
/// back through the interpreter. Twig's `|filter` / `|map` arrow functions hit it: a compiled
/// `CallbackFilterIterator` wrapper called with an arrow function a template block created.
#[test]
fn test_eval_closure_reaches_a_callable_parameter_other_callers_fed_compiled_closures() {
    let out = compile_and_run(r###"<?php
function run($cb) {
    return $cb(5);
}
echo run(fn ($x) => $x + 1), "\n";
eval('
echo run(fn ($c) => $c * 2), "\n";
$k = 3;
echo run(function ($c) use ($k) { return $c + $k; }), "\n";
');
function keep($cb) {
    return new CallbackFilterIterator(new ArrayIterator([1, 2, 3]), $cb);
}
foreach (keep(fn ($x) => $x > 0) as $v) { echo $v; }
echo "\n";
eval('
foreach (keep(fn ($c) => $c > 1) as $v) { echo $v; }
echo "\n";
');
"###);
    assert_eq!(out, "6\n10\n8\n123\n23\n");
}

/// `new IteratorIterator($this->source)` wraps what an interpreter-declared aggregate's
/// `getIterator()` returns, and keeps an interpreter-declared Iterator as it is.
///
/// The constructor's source is not a plain variable here (a typed property), so the AST rewrite
/// that unwraps variables does not apply and the compiled constructor, which recognizes
/// interfaces by compiled class id, saw neither interface on the interpreter's class. The
/// interpreter is now asked for the aggregate's iterator, nested aggregates included. The second
/// half chains `CallbackFilterIterator` over the wrapper with a closure the interpreter made.
#[test]
fn test_iterator_iterator_unwraps_an_interpreter_aggregate_it_reads_from_a_typed_slot() {
    let out = compile_and_run(r###"<?php
class Holder {
    public function __construct(public Traversable $t) {}
    public function it() { return new IteratorIterator($this->t); }
}
$h = new Holder(new ArrayIterator([7, 8]));
foreach ($h->it() as $k => $c) { echo $k, "=", $c, "\n"; }
function wrap($array, $cb) {
    return new CallbackFilterIterator(new IteratorIterator($array), $cb);
}
$g = wrap(new ArrayIterator([1, 2]), fn ($x) => true);
echo count(iterator_to_array($g, false)), "\n";
eval('
class V implements IteratorAggregate {
    public array $children = ["a" => 1, "b" => 2, "c" => 3];
    public function getIterator(): ArrayIterator { return new ArrayIterator($this->children); }
}
class Outer implements IteratorAggregate {
    public function getIterator(): Traversable { return new V(); }
}
class W implements Iterator {
    private $i = 0;
    public function current(): mixed { return $this->i * 10; }
    public function key(): mixed { return "w" . $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 3; }
}
foreach ((new Holder(new V()))->it() as $k => $c) { echo $k, "=", $c, "\n"; }
foreach ((new Holder(new Outer()))->it() as $k => $c) { echo $k, "=", $c, "\n"; }
foreach ((new Holder(new W()))->it() as $k => $c) { echo $k, "=", $c, "\n"; }
foreach (wrap(new V(), fn ($c) => $c > 1) as $k => $c) { echo $k, "=", $c, "\n"; }
');
"###);
    assert_eq!(out, "0=7\n1=8\n2\na=1\nb=2\nc=3\na=1\nb=2\nc=3\nw0=0\nw1=10\nw2=20\nb=2\nc=3\n");
}

/// `eval()` can construct and call the SPL iterators, heaps and storage a program never names
/// outside the interpreted text.
///
/// Only the allocation of a handful of SPL classes was reachable from the interpreter, so
/// `new LimitIterator(...)` ended in "unsupported NewObject expression", and a heap's `insert()` in
/// "unsupported MethodCall ->insert()" (the constructor and the surface the bridge needs were never
/// compiled in). The bridge now builds every SPL class the program registers, with the methods
/// interpreted code can call by name.
#[test]
fn test_eval_constructs_spl_classes_the_compiled_code_never_names() {
    let out = compile_and_run(r###"<?php
eval('
$it = new LimitIterator(new ArrayIterator([1, 2, 3, 4]), 1, 2);
foreach ($it as $k => $v) { echo $k, "=", $v, "\\n"; }
echo implode(",", iterator_to_array(new CachingIterator(new ArrayIterator(["a", "b"])), false)), "\\n";
foreach (new LimitIterator(new InfiniteIterator(new ArrayIterator([1, 2])), 0, 5) as $v) { echo $v; }
echo "\\n";
$st = new SplObjectStorage();
$a = new stdClass; $b = new stdClass;
$st[$a] = null; $st[$b] = "data";
echo count($st), " ", json_encode(isset($st[$a])), " ", $st[$b], "\\n";
unset($st[$a]);
echo count($st), "\\n";
$h = new SplMaxHeap();
foreach ([3, 9, 1] as $x) { $h->insert($x); }
echo $h->top(), " ", count($h), "\\n";
foreach ($h as $v) { echo $v; }
echo "\\n";
$ap = new AppendIterator();
$ap->append(new ArrayIterator([1, 2]));
$ap->append(new ArrayIterator([3]));
echo implode(",", iterator_to_array($ap, false)), "\\n";
$nr = new NoRewindIterator(new ArrayIterator([1, 2, 3]));
foreach ($nr as $v) { echo $v; break; }
foreach ($nr as $v) { echo $v; }
echo "\\n";
$pq = new SplPriorityQueue();
$pq->insert("lo", 1); $pq->insert("hi", 5);
echo $pq->extract(), count($pq), "\\n";
');
"###);
    assert_eq!(out, "1=2\n2=3\na,b\n12121\n2 true data\n1\n9 3\n931\n1,2,3\n1123\nhi1\n");
}

/// An `SplStack` / `SplQueue` driven from `eval()` stores the values it is given and hands them
/// back.
///
/// The list runtime helpers CONSUME their boxed arguments (the value cell of `push()`, the offset
/// of the ArrayAccess operations), and the eval bridge passed the interpreter's own cell without
/// the reference the helper takes over. The stack then held freed cells: `top()` and `pop()`
/// returned empty arrays and the loop printed garbage.
#[test]
fn test_eval_list_methods_keep_the_values_they_store_and_return() {
    let out = compile_and_run(r###"<?php
eval('
$s = new SplStack(); $s->push(4); $s->push(5);
var_dump($s->top());
var_dump($s->pop());
var_dump(count($s));
var_dump($s->isEmpty());
$q = new SplQueue(); $q->enqueue(1); $q->enqueue(2); var_dump($q->dequeue());
foreach ($s as $v) { var_dump($v); }
$f = new SplFixedArray(2); $f[0] = "x"; $f[1] = [1, 2];
var_dump($f[0], count($f[1]));
');
"###);
    assert_eq!(out, "int(5)\nint(5)\nint(1)\nbool(false)\nint(1)\nint(4)\nstring(1) \"x\"\nint(2)\n");
}

/// A write through a nested array element reaches the variable an element of that array references.
///
/// An element array about to be written through is copied first (so `clone` cannot share it), but
/// the interpreter keeps an array's reference slots in a side table keyed by the payload, and the
/// copy did not inherit them: `$aliases["nested"][0] = 99` after `$aliases["nested"][0] =& $item`
/// wrote into a copy and `$items[0]` stayed 11. Arrays that hold references are written in place.
#[test]
fn test_eval_nested_element_write_keeps_the_references_the_array_holds() {
    let out = compile_and_run(r###"<?php
eval('$items = [1, 2];
foreach ($items as $key => &$item) {
    $item = $item + $key + 10;
}
unset($item);
$aliases = [];
foreach ($items as $key => &$item) {
    $aliases["nested"][$key] =& $item;
}
$aliases["nested"][0] = 99;
echo $items[0] . ":" . $items[1] . ":" . $item . "\\n";
class Holder { public array $data = ["k" => [1, 2]]; }
$a = new Holder();
$b = clone $a;
$b->data["k"][0] = 7;
echo $a->data["k"][0], $b->data["k"][0], "\\n";');
"###);
    assert_eq!(out, "99:13:13\n17\n");
}

/// A closure that captured `use (&$listener)` from a by-reference `foreach` can write an element of
/// that variable after the method that bound it has returned.
///
/// Writing `$listener[0] = ...` forwards the new array to the storage the variable references; the
/// nested target (`foreach ($this->listeners[$e] as &$listeners)` then `as &$listener`) is rooted
/// in the method's activation scope, which is gone by then. The plain variable and element targets
/// treat that as "nothing left to update"; the nested one read the dead scope and raised
/// "unsupported ArraySetVar statement". Symfony's `EventDispatcher::optimizeListeners()` lazy
/// listener wrapper is exactly this closure.
#[test]
fn test_eval_write_through_a_by_reference_capture_outliving_its_method() {
    let out = compile_and_run(r###"<?php
eval(<<<'PHPCODE'
class Wrapped
{
    public function __invoke(string $tag): string
    {
        return "W(" . $tag . ")";
    }
}

class Dispatcher
{
    private array $listeners = [];
    private array $optimized = [];

    public function add(string $event, int $priority, callable $listener): void
    {
        $this->listeners[$event][$priority][] = $listener;
        unset($this->optimized[$event]);
    }

    private function optimizeListeners(string $eventName): array
    {
        krsort($this->listeners[$eventName]);
        $this->optimized[$eventName] = [];

        foreach ($this->listeners[$eventName] as &$listeners) {
            foreach ($listeners as &$listener) {
                $closure = &$this->optimized[$eventName][];
                if (\is_array($listener) && isset($listener[0]) && $listener[0] instanceof \Closure && 2 >= \count($listener)) {
                    $closure = static function (...$args) use (&$listener, &$closure) {
                        if ($listener[0] instanceof \Closure) {
                            $listener[0] = $listener[0]();
                            $listener[1] ??= '__invoke';
                        }
                        ($closure = $listener(...))(...$args);
                    };
                } else {
                    $closure = $listener instanceof Wrapped ? $listener : $listener(...);
                }
            }
        }

        return $this->optimized[$eventName];
    }

    public function call(string $eventName, string $tag): string
    {
        $out = [];
        foreach ($this->optimized[$eventName] ?? $this->optimizeListeners($eventName) as $listener) {
            $out[] = $listener($tag);
        }
        return implode("|", $out);
    }
}

$d = new Dispatcher();
$d->add("evt", 10, static fn (string $t): string => "high:" . $t);
$d->add("evt", 5, new Wrapped());
$d->add("evt", 1, [static fn (): object => new Wrapped(), "__invoke"]);

echo $d->call("evt", "x"), "\n";
echo $d->call("evt", "y"), "\n";
echo "p4done\n";
PHPCODE
);
"###);
    assert_eq!(out, "high:x|W(x)|\nhigh:y|W(y)|W(y)\np4done\n");
}

/// Four spellings the compiler used to refuse now behave like php: `[1] + ["right", "side"]` (the
/// union keeps the left operand's keys whatever the element types), `constant("K::A")`,
/// `array_keys($a, $search)` and the multi-array form of `array_map()`.
///
/// The error tests that asserted the refusals ("Array union requires compatible indexed array
/// element types", "constant() class constants are not supported", the one-argument arity messages)
/// described limits the compiler no longer has; this pins the php-verified positive behavior.
#[test]
fn test_array_union_constant_class_names_array_keys_search_and_multi_array_map_match_php() {
    let out = compile_and_run(r###"<?php
$result = [1] + ["right", "side"];
echo json_encode($result), "\n";
class K { const A = 1; }
echo constant("K::A"), "\n";
echo json_encode(array_keys(["a" => 1, "b" => 2, "c" => 1], 1)), "\n";
echo json_encode(array_map(fn ($a, $b) => $a . $b, ["x", "y"], ["1", "2"])), "\n";
"###);
    assert_eq!(out, "[1,\"side\"]\n1\n[\"a\",\"c\"]\n[\"x1\",\"y2\"]\n");
}
