//! Purpose:
//! Regression tests for dynamic method dispatch on receivers whose static type
//! does not name a single class (a `Mixed` value, or a union of object classes).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Before the fix, a method call on such a receiver emitted no dispatch and
//!   left a garbage value in the result register. Dispatch now reads the
//!   receiver's runtime class id and selects the matching class's method, so
//!   these fixtures assert PHP-equivalent stdout.

use super::*;

/// Verifies a method call on a `: mixed`-returning value dispatches on the runtime
/// class id (the value is an object), both via a local and chained directly.
#[test]
fn test_mixed_receiver_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class S {
    public int $v;
    public function __construct(int $x) { $this->v = $x; }
    public function doubled(): int { return $this->v * 2; }
}
class C {
    public function make(int $x): mixed {
        if ($x < 0) { return false; }
        return new S($x);
    }
}
$c = new C();
$s = $c->make(5);
echo $s->doubled() . ";" . $c->make(7)->doubled();
"#,
    );
    assert_eq!(out, "10;14");
}

/// Verifies a method call on an `object|false` union receiver dispatches correctly.
#[test]
fn test_object_or_false_union_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class S {
    public int $v;
    public function __construct(int $x) { $this->v = $x; }
    public function doubled(): int { return $this->v * 2; }
}
class C {
    public function make(int $x): S|bool {
        if ($x < 0) { return false; }
        return new S($x);
    }
}
$c = new C();
$s = $c->make(5);
echo ($s === false) ? "F" : $s->doubled();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a dynamic-receiver method call passes its arguments correctly (the
/// receiver and arguments are evaluated once and dispatched together).
#[test]
fn test_mixed_receiver_method_with_arguments() {
    let out = compile_and_run(
        r#"<?php
class S {
    public function add(int $a, int $b): int { return $a + $b; }
}
class C {
    public function make(): mixed { return new S(); }
}
$c = new C();
echo $c->make()->add(3, 4);
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies a boxed-Mixed object argument is checked and unboxed after dynamic receiver dispatch.
///
/// The receiver's runtime class selects `Consumer::accept()`, whose declared
/// parameter uses the raw object ABI. The argument remains Mixed until that
/// selection, so the candidate-specific call path must validate and expose its
/// object payload instead of passing the Mixed cell as an object pointer.
#[test]
fn test_mixed_receiver_unboxes_object_argument_for_typed_method_parameter() {
    let out = compile_and_run(
        r#"<?php
class Dependency {
    public function label(): string { return 'ok'; }
}
class Consumer {
    public function accept(Dependency $dependency): string { return $dependency->label(); }
}
function receiver(): mixed { return new Consumer(); }
function dependency(): mixed { return new Dependency(); }
echo receiver()->accept(dependency());
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies a dynamic receiver preserves PHP's nominal object parameter TypeError.
///
/// Candidate dispatch cannot validate its argument in IR because the receiver
/// class is only selected at runtime. Its concrete candidate path must reject a
/// boxed non-object before exposing an invalid payload through the object ABI.
#[test]
fn test_mixed_receiver_rejects_non_object_for_typed_method_parameter() {
    let out = compile_and_run_capture(
        r#"<?php
class Dependency {}
class Consumer {
    public function accept(Dependency $dependency): void {}
}
function receiver(): mixed { return new Consumer(); }
function invalidDependency(): mixed { return 'not an object'; }
receiver()->accept(invalidDependency());
"#,
    );
    assert!(!out.success);
    // php reports an uncaught error on stdout (display_errors), and so does the runtime.
    assert!(
        out.stdout.contains("TypeError: ")
            && out.stdout.contains("must be of type Dependency, string given"),
        "unexpected stdout: {}",
        out.stdout
    );
}

/// A same-name dynamic candidate with a nullable-int parameter must not reject a string branch.
#[test]
fn test_mixed_method_dispatch_with_string_and_nullable_int_candidates() {
    let out = compile_and_run(
        r#"<?php
class StringTarget {
    public function run(string $value): string { return 's:'.$value; }
}
class IntTarget {
    public function run(?int $value): string { return 'i:'.$value; }
}
function target(bool $string): mixed {
    return $string ? new StringTarget() : new IntTarget();
}
echo target(true)->run('text'), '|', target(false)->run('12');
"#,
    );
    assert_eq!(out, "s:text|i:12");
}

/// Verifies dynamic dispatch selects the correct class at runtime when several
/// classes define the same method.
#[test]
fn test_mixed_receiver_multiple_candidate_classes() {
    let out = compile_and_run(
        r#"<?php
class Dog { public function speak(): string { return "woof"; } }
class Cat { public function speak(): string { return "meow"; } }
function animal(int $n): mixed { return ($n == 0) ? new Dog() : new Cat(); }
echo animal(0)->speak() . ":" . animal(1)->speak();
"#,
    );
    assert_eq!(out, "woof:meow");
}

/// Verifies an object-union call dispatches to the member that declares the method even when
/// another possible member does not declare it.
#[test]
fn test_object_union_method_supported_by_one_member() {
    let out = compile_and_run(
        r#"<?php
class ReadyTarget { public function value(): string { return "ready"; } }
class OtherTarget {}
function target(bool $ready): ReadyTarget|OtherTarget {
    return $ready ? new ReadyTarget() : new OtherTarget();
}
echo target(true)->value();
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies the same partial object-union dispatch raises a runtime error when the selected
/// member does not declare the method.
#[test]
fn test_object_union_missing_runtime_method_fatals() {
    let out = compile_and_run_capture(
        r#"<?php
class ReadyTarget { public function value(): string { return "ready"; } }
class OtherTarget {}
function target(bool $ready): ReadyTarget|OtherTarget {
    return $ready ? new ReadyTarget() : new OtherTarget();
}
echo target(false)->value();
"#,
    );
    assert!(!out.success);
    // php: `Uncaught Error: Call to undefined method OtherTarget::value()`, written to stdout.
    assert!(
        out.stdout.contains("Call to undefined method OtherTarget::value()"),
        "unexpected stdout: {}",
        out.stdout
    );
}

/// Verifies a dynamic-receiver method call returning a string works end to end.
#[test]
fn test_mixed_receiver_string_return() {
    let out = compile_and_run(
        r#"<?php
class S {
    public string $n;
    public function __construct(string $n) { $this->n = $n; }
    public function greet(): string { return "hi " . $this->n; }
}
class C {
    public function make(string $n): mixed { return new S($n); }
}
$c = new C();
echo $c->make("ada")->greet();
"#,
    );
    assert_eq!(out, "hi ada");
}

/// Verifies a dynamic-receiver method call on a non-object runtime value fatals
/// (PHP "Call to a member function ... on a non-object") instead of miscompiling.
#[test]
fn test_mixed_receiver_non_object_fatals() {
    let out = compile_and_run_capture(
        r#"<?php
class S { public function d(): int { return 1; } }
function make(int $x): mixed { if ($x < 0) { return false; } return new S(); }
$v = make(-1);
echo $v->d();
"#,
    );
    assert!(!out.success);
    // STDOUT: the uncaught report follows PHP's stream.
    assert!(out.stdout.contains("Call to a member function d()"), "{}", out.stdout);
}

/// Regression: a user method whose name collides with a builtin method of a different arity (here
/// `add`, which `DateTime::add(DateInterval)` also defines) must still dispatch correctly for a
/// mixed receiver. The dispatch marshals arguments once with the first candidate's signature, so
/// candidates are filtered by argument arity; otherwise `DateTime::add` could be selected for this
/// 2-argument call depending on (nondeterministic) class-id ordering, corrupting the result.
#[test]
fn test_mixed_receiver_method_name_collides_with_builtin_arity() {
    let out = compile_and_run(
        r#"<?php
class Money { public function add(int $a, int $b): int { return $a + $b; } }
function make(): mixed { return new Money(); }
echo make()->add(40, 2);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies a lone same-named method cannot pin a gradual receiver's nominal object return type.
#[test]
fn test_mixed_receiver_nominal_object_return_stays_gradual_for_chained_dispatch() {
    let out = compile_and_run(
        r#"<?php
class ConcreteConnection {}
class UnrelatedProvider {
    public function getConnection(): ConcreteConnection { return new ConcreteConnection(); }
}
function optional_adapter(string $class): void {
    $client = new $class();
    $client->getConnection()->setSentinelTimeout(1);
}
echo "linked";
"#,
    );
    assert_eq!(out, "linked");
}

/// A local declared through one interface can be narrowed to an unrelated capability interface.
/// The guarded call must use the proven interface metadata directly instead of depending on
/// closed-world discovery of a concrete implementation through the original interface.
#[test]
fn test_cross_interface_instanceof_narrows_local_for_dispatch() {
    let out = compile_and_run(
        r#"<?php
interface BaseHandler { public function open(): bool; }
interface TimestampHandler { public function validateId(string $id): bool; }
final class Handler implements BaseHandler, TimestampHandler {
    public function open(): bool { return true; }
    public function validateId(string $id): bool { return $id === "ok"; }
}
function validate(BaseHandler $handler, string $id): bool {
    if ($handler instanceof TimestampHandler) {
        return $handler->validateId($id);
    }
    return false;
}
echo validate(new Handler(), "ok") ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}


/// `instanceof` a target the receiver's DECLARED class already satisfies needs no lookup.
///
/// `PhpType::Object(name)` is an upper bound -- the value is `name` or a subclass -- and PHP's
/// hierarchy only grows downward, so every value that can arrive satisfies the target too. The
/// answer is `receiver != null` rather than a constant `true`, because PHP's `null instanceof X`
/// is false and a declared object type still reads null from an uninitialized slot.
///
/// Pinned together with the cases the fold must NOT take: a downcast (`Base instanceof Native`)
/// and an unrelated interface both still have to be decided at run time.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_instanceof_folds_when_the_declared_class_already_satisfies_the_target() {
    let out = compile_and_run(
        r#"<?php
interface Marker {}
interface Other {}
class Base implements Marker {}
class Native extends Base {}
class Deeper extends Native {}
function inspect(Base $b): string {
    return implode('|', [
        var_export($b instanceof Base, true),
        var_export($b instanceof Marker, true),
        var_export($b instanceof Native, true),
        var_export($b instanceof Deeper, true),
        var_export($b instanceof Other, true),
    ]);
}
function inspectNullable(?Base $b): string {
    return var_export($b instanceof Base, true).'|'.var_export($b instanceof Marker, true);
}
echo inspect(new Base()), "\n";
echo inspect(new Native()), "\n";
echo inspect(new Deeper()), "\n";
echo inspectNullable(null), "\n";
echo inspectNullable(new Native());
"#,
    );
    assert_eq!(
        out,
        "true|true|false|false|false\n\
         true|true|true|false|false\n\
         true|true|true|true|false\n\
         false|false\n\
         true|true"
    );
}


/// A class DECLARED AT RUNTIME that extends a compiled one keeps its protected access.
///
/// The eval property bridge compares the interpreter's class scope against an allow-list baked
/// into the user assembly, and `related_class_scope_names` builds that list from `module.class_infos`
/// -- the AOT classes and nothing else. An interpreted subclass is in no list, so every read and
/// write it makes to a protected property of a compiled object was refused, with no description:
/// `Fatal error: eval() runtime failed: unsupported PropertyGet expression`.
///
/// The interpreter already knows both hierarchies, so it now performs the access inside the
/// DECLARING class's scope once `validate_eval_member_access` has allowed it. The declaring class
/// is in every allow-list by construction, private and protected alike.
///
/// This is what killed the 192-file Symfony `--web` preload: every `var/cache` container fragment
/// declares `class getXService extends App_KernelProdContainer` and writes
/// `$container->services[$id]`, declared `protected array $services` on
/// `Symfony\Component\DependencyInjection\Container`.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_runtime_declared_subclass_reaches_a_compiled_protected_property() {
    let out = compile_and_run(
        r#"<?php
class Stack { public function __construct(public string $name) {} }
class BaseContainer { protected array $services = []; }
class AppContainer extends BaseContainer {
    public function step(string $fragment, string $method): string {
        return (string) $fragment::$method($this);
    }
}
$name = 'frag';
file_put_contents($name.'.php', '<?php class FragRunner extends AppContainer {'
    ."\n".'    public static function read($c) { return count($c->services); }'
    ."\n".'    public static function make($c) { return ($c->services[\'rs\'] ??= new Stack(\'made\'))->name; }'
    ."\n".'}');
require './'.$name.'.php';
$c = new AppContainer();
echo $c->step('FragRunner', 'read'), '/', $c->step('FragRunner', 'make'), '/', $c->step('FragRunner', 'read');
unlink($name.'.php');
"#,
    );
    assert_eq!(out, "0/made/1");
}


/// Interpreted code reads and writes a COMPILED object's `\Closure`-typed property.
///
/// That slot holds a callable DESCRIPTOR -- `emit_mixed_callable_for_property_store` unboxes,
/// demands runtime tag 10 and stores the descriptor pointer, so compiled code already treats it
/// that way. The interpreter hands over a BOXED callback instead, and the eval property bridge had
/// no `Callable` arm at all: `property_type_supported` excluded it, so the property got no slot and
/// both directions were refused with `unsupported Assign expression`.
///
/// The conversion is the one a `callable` PARAMETER already gets
/// (`emit_*_cast_eval_callable_arg`). Its dynamic fallback needs the active eval context, which is
/// why `__elephc_eval_value_property_set` grew a trailing `context` argument.
///
/// This is what 404ed every Symfony route: the generated container writes
/// `$container->getService ??= $container->getService(...)` against
/// `protected \Closure $getService;`, so the routing loader service died and the router got no
/// routes while the error page stayed byte-perfect.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_eval_bridge_reads_and_writes_a_compiled_closure_typed_property() {
    let out = compile_and_run(
        r#"<?php
class AppContainer {
    protected array $services = ['alpha' => 'A', 'beta' => 'B'];
    protected \Closure $factory;
    final protected function getService(string $id): string { return $this->services[$id] ?? '?'; }
    public function fill(): void { $this->factory = $this->getService(...); }
    public function callOwn(string $id): string { return 'own:'.($this->factory)($id); }
    public function step(string $fragment, string $method): string {
        return (string) $fragment::$method($this);
    }
}
$name = 'closfrag';
file_put_contents($name.'.php', '<?php class ClosFrag extends AppContainer {'
    ."\n".'    public static function readTyped($c) { $f = $c->factory; return "read:".$f("alpha"); }'
    ."\n".'    public static function writeTyped($c) { $c->factory = $c->getService(...); return "write:".($c->factory)("beta"); }'
    ."\n".'}');
require './'.$name.'.php';
$c = new AppContainer();
$c->fill();
echo $c->callOwn('alpha'), '|', $c->step('ClosFrag', 'readTyped'), '|', $c->step('ClosFrag', 'writeTyped');
unlink($name.'.php');
"#,
    );
    assert_eq!(out, "own:A|read:A|write:B");
}


/// A class satisfies a builtin interface whose return type PHP declares TENTATIVE.
///
/// PHP 8.1 gave its own interfaces return types but kept them tentative: an implementer that
/// declares none stays legal, PHP raises a deprecation rather than a fatal, and
/// `#[\ReturnTypeWillChange]` silences even that. elephc refused it outright.
///
/// The attribute is NOT what makes it legal, which is why the rule is keyed on the declaring
/// interface instead: `php -n` fatals with `Declaration of Child::f() must be compatible with
/// Base::f(): int` on the same shape against a USERLAND parent whether or not the attribute is
/// written, so a class overriding a declared userland return type must still declare one.
///
/// This is what stopped `Twig\Node\Node` (`#[\ReturnTypeWillChange] public function count()`)
/// from registering at all, and with it every Twig node class that extends it -- Nodes,
/// NameExpression, AssignNameExpression, ConstantExpression, ArrayExpression, AbstractUnary and
/// Symfony's FormThemeNode each then reported `Undefined class` against its own file.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_tentative_interface_return_type_accepts_an_undeclared_implementation() {
    let out = compile_and_run(
        r#"<?php
class Bag implements \Countable, \IteratorAggregate {
    private array $items = ['a', 'b', 'c'];
    /** @return int */
    #[\ReturnTypeWillChange]
    public function count() { return \count($this->items); }
    public function getIterator(): \Traversable { return new \ArrayIterator($this->items); }
}
class Sub extends Bag {
    #[\ReturnTypeWillChange]
    public function count() { return 42; }
}
$b = new Bag();
echo count($b), '|', ($b instanceof Countable ? 'yes' : 'no'), '|';
foreach ($b as $v) { echo $v; }
echo '|', count(new Sub());
"#,
    );
    assert_eq!(out, "3|yes|abc|42");
}


/// A `method_exists()` test in the same body admits the call it guards.
///
/// `infer_lenient_subtype_method_call` accepts a method the nominal receiver does not declare when
/// SOME compatible class does — PHP dispatches on the runtime class and the backend mirrors that
/// with a class-id switch — but refused when NOTHING in the closed world declares it. Twig ships
/// exactly that shape so a 4.0 interface method can be adopted early
/// (`ExpressionParsers::getOperatorTokensFor()`, 3.24): no bundled parser implements
/// `getOperatorTokens()` yet, `php -n` never enters the branch, and elephc refused the file.
///
/// The fixture is that shape reduced: no class declares `getOperatorTokens`, so the guarded branch
/// is dead and the program must print what `php -n` prints. The narrow admission is pinned by
/// `test_undefined_method_without_a_guard_is_still_refused` and
/// `test_method_exists_guard_does_not_admit_a_different_receiver` in the error suite.
///
/// Oracle: `php -n` prints `plain`.
#[test]
fn test_method_exists_guard_admits_a_call_no_class_declares() {
    let out = compile_and_run(
        r#"<?php
interface ParserInterface { public function getName(): string; }
class Plain implements ParserInterface { public function getName(): string { return 'plain'; } }
function tokensFor(ParserInterface $parser): array {
    if (method_exists($parser, 'getOperatorTokens')) { return $parser->getOperatorTokens(); }
    return [$parser->getName()];
}
echo implode(',', tokensFor(new Plain()));
"#,
    );
    assert_eq!(out, "plain");
}


/// `$f->__invoke(...)` and `$f?->__invoke(...)` are the closure call written the long way.
///
/// There was no instance dispatch for a `PhpType::Callable` receiver at all: the plain form died
/// in the backend with `unsupported EIR backend feature: method call receiver for PHP type
/// Callable`, and the nullsafe form was refused by the checker as "Nullsafe method call requires
/// an object or null, got callable|null" — a `?\Closure` is null OR an object, which is exactly
/// what `?->` is for. Both now lower through the path `$f($x)` already used.
///
/// Symfony's `TraceableAdapter` writes `$this->disabled?->__invoke()` on thirteen operations, and
/// the receiver there is a promoted `?\Closure` constructor property, so one program reaches both
/// the null and the non-null arm.
///
/// The `?->` form is the interesting one twice over: written after a property read it is not a
/// bare nullsafe call but a postfix CHAIN, lowered in a different module, and fixing only the
/// bare form left it fataling `Call to a member function __invoke() on null` on a receiver that
/// was not null.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_invoke_on_a_closure_receiver_is_the_closure_call() {
    let out = compile_and_run(
        r#"<?php
class Traced {
    public function __construct(private readonly ?\Closure $disabled = null) {}
    public function chained(): string { return $this->disabled?->__invoke() ? "off" : "on"; }
}
class Held {
    public function __construct(private \Closure $fn) {}
    public function plain(string $s): string { return $this->fn->__invoke($s); }
}
echo (new Traced())->chained(), "\n";
echo (new Traced(fn () => true))->chained(), "\n";
echo (new Traced(fn () => false))->chained(), "\n";
echo (new Held(fn ($s) => $s . "!"))->plain("hi"), "\n";
$bare = fn (int $n): int => $n * 3;
echo $bare->__invoke(4), "\n";
$maybe = null;
var_dump($maybe?->__invoke());
"#,
    );
    assert_eq!(out, "on\noff\non\nhi!\n12\nNULL\n");
}


/// A gradual receiver's call matches a candidate whose extra parameter is OPTIONAL.
///
/// A call on a `mixed` (or bare `object`) receiver dispatches on the runtime class id over the
/// classes that declare the method, and the static result is built from those candidates. The
/// filter that picked them compared `params.len()` against the argument count — the TOTAL
/// parameter count, not the range the method can be called with. A method with an optional
/// parameter was therefore excluded from its own call, and what remained were the unrelated
/// zero-parameter methods that merely share the name.
///
/// Symfony's `ProxyAdapter::clear(): bool` is the case: it reads `$this->pool`, declared
/// `private object $pool` in `ProxyTrait`, so fully gradual. `$this->pool->clear()` skipped every
/// `clear(string $prefix = '')` in the program and matched `HeaderBag`-shaped `clear(): void`
/// methods instead, so the call came back `void` against its own declared `: bool` and the build
/// stopped.
///
/// `Unrelated::clear(): void` is in the fixture deliberately: it is what poisoned the answer, and
/// without it the fixture passes even unfixed.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_a_gradual_receiver_call_matches_an_optional_parameter() {
    let out = compile_and_run(
        r#"<?php
interface PoolInterface { public function clear(): bool; }
interface AdapterInterface extends PoolInterface { public function clear(string $prefix = ''): bool; }

class Unrelated { public function clear(): void { echo "unrelated\n"; } }

class Inner implements AdapterInterface {
    public function clear(string $prefix = ''): bool { return $prefix === ''; }
}

class Proxy implements AdapterInterface {
    private object $pool;
    public function __construct(PoolInterface $pool) { $this->pool = $pool; }

    public function clear(string $prefix = ''): bool
    {
        if ($this->pool instanceof AdapterInterface) {
            return $this->pool->clear($prefix);
        }
        return $this->pool->clear();
    }
}

var_dump((new Proxy(new Inner()))->clear());
var_dump((new Proxy(new Inner()))->clear('ns'));
(new Unrelated())->clear();
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nunrelated\n");
}
