//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of static class features, including class class named, class class namespaced, and class class self inside method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies class-like existence probes accept runtime strings and search the
/// appropriate closed-world metadata table case-insensitively.
#[test]
fn test_dynamic_class_like_existence_probes() {
    let out = compile_and_run(
        r#"<?php
class RuntimeNamedClass {}
interface RuntimeNamedInterface {}
trait RuntimeNamedTrait {}
enum RuntimeNamedEnum { case Ready; }

function hasClass(string $name): bool { return class_exists($name, false); }
function hasInterface(string $name): bool { return interface_exists($name, false); }
function hasTrait(string $name): bool { return trait_exists($name, false); }
function hasEnum(string $name): bool { return enum_exists($name, false); }

echo hasClass('runtimenamedclass') ? '1' : '0';
echo hasInterface('RUNTIMENAMEDINTERFACE') ? '1' : '0';
echo hasTrait('RuntimeNamedTrait') ? '1' : '0';
echo hasEnum('runtimenamedenum') ? '1' : '0';
echo hasClass('MissingClass') ? '1' : '0';
echo hasInterface('MissingInterface') ? '1' : '0';
echo hasTrait('MissingTrait') ? '1' : '0';
echo hasEnum('MissingEnum') ? '1' : '0';
"#,
    );
    assert_eq!(out, "11110000");
}

// --- ::class magic constant ---

/// Verifies `ClassName::class` resolves to the unqualified class name `C`.
#[test]
fn test_class_class_named() {
    let out = compile_and_run(
        "<?php class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "C");
}

/// Verifies `ClassName::class` inside a namespace resolves to the fully-qualified name `App\C`.
#[test]
fn test_class_class_namespaced() {
    let out = compile_and_run(
        "<?php namespace App; class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "App\\C");
}

/// Verifies `self::class` inside a method resolves to the lexical (defining) class `C`, not the runtime-called subclass.
#[test]
fn test_class_class_self_inside_method() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public static function name() { return self::class; }\n}\necho C::name();\n",
    );
    assert_eq!(out, "C");
}

/// Verifies `parent::class` inside a child class resolves to the parent class `Base`.
#[test]
fn test_class_class_parent_inside_child() {
    let out = compile_and_run(
        "<?php\nclass Base { public int $x = 0; }\nclass Child extends Base {\n    public static function parent_name() { return parent::class; }\n}\necho Child::parent_name();\n",
    );
    assert_eq!(out, "Base");
}

/// Verifies `static::class` uses late static binding — resolves to the runtime class `Child` even when called from base method.
#[test]
fn test_class_class_static_uses_late_static_binding() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function name() { return static::class; }\n}\nclass Child extends Base {}\necho Child::name();\n",
    );
    assert_eq!(out, "Child");
}

/// Verifies `::class` can be used in string concatenation expressions.
#[test]
fn test_class_class_concat_in_message() {
    let out = compile_and_run(
        "<?php class Logger { public int $x = 0; } echo \"From: \" . Logger::class;",
    );
    assert_eq!(out, "From: Logger");
}

/// Verifies `$object::class` returns the object's fully-qualified runtime class name.
#[test]
fn test_object_class_name_returns_runtime_fqn() {
    let out = compile_and_run(
        "<?php namespace App; class Pluto {} $pippo = new Pluto(); echo $pippo::class;",
    );
    assert_eq!(out, "App\\Pluto");
}

/// Verifies `$object::class` reads the concrete subclass even when the expression is typed as its base class.
#[test]
fn test_object_class_name_preserves_concrete_subclass() {
    let out = compile_and_run(
        "<?php class Base {} class Child extends Base {} function pick(bool $base): Base { return $base ? new Base() : new Child(); } echo pick(false)::class;",
    );
    assert_eq!(out, "Child");
}

/// Verifies `::class` accepts object-only unions and dispatches using the selected runtime object.
#[test]
fn test_object_class_name_accepts_object_union() {
    let out = compile_and_run(
        "<?php class Left {} class Right {} function pick(bool $left): Left|Right { return $left ? new Left() : new Right(); } echo pick(false)::class;",
    );
    assert_eq!(out, "Right");
}

/// Verifies a gradual nullable receiver resolves `::class` from its runtime object payload.
#[test]
fn test_object_class_name_accepts_gradual_nullable_object() {
    let out = compile_and_run(
        "<?php class GradualClassName {} $value = $argc > 0 ? new GradualClassName() : null; echo $value::class;",
    );
    assert_eq!(out, "GradualClassName");
}

/// Verifies an object-valued expression before `::class` is evaluated exactly once.
#[test]
fn test_object_class_name_evaluates_receiver_once() {
    let out = compile_and_run(
        "<?php class Probe {} function make_probe(): Probe { echo 'once|'; return new Probe(); } echo make_probe()::class;",
    );
    assert_eq!(out, "once|Probe");
}

// --- new self() / new static() / new parent() ---

/// Verifies `new self()` inside a static method returns an instance of the lexical (defining) class `Box` and that fields are accessible.
#[test]
fn test_new_self_returns_instance_of_lexical_class() {
    let out = compile_and_run(
        "<?php\nclass Box {\n    public string $label = \"hello\";\n    public static function make(): Box { return new self(); }\n}\n$b = Box::make();\necho $b->label;\n",
    );
    assert_eq!(out, "hello");
}

/// Verifies `new static()` uses late static binding — returns an instance of the runtime-called class `Child` when called via `Child::make()`.
#[test]
fn test_new_static_returns_instance_of_called_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function make(): Base { return new static(); }\n    public function name(): string { return self::class; }\n}\nclass Child extends Base {\n    public function name(): string { return self::class; }\n}\n$b = Child::make();\necho $b->name();\n",
    );
    assert_eq!(out, "Child");
}

/// Verifies an abstract factory may construct the concrete runtime-called subclass through
/// `new static()` without treating the lexical abstract base as an instantiation candidate.
#[test]
fn test_new_static_from_abstract_base_constructs_concrete_child() {
    let out = compile_and_run(
        r#"<?php
abstract class AbstractFactory {
    public function __construct(public string $value) {}

    public static function make(string $value): static {
        return new static($value);
    }
}

final class ConcreteFactory extends AbstractFactory {}

echo ConcreteFactory::make("ready")->value;
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies `new static()` pads omitted constructor defaults before the late-static candidate
/// dispatch, including when an inherited factory constructs the runtime-called child class.
#[test]
fn test_new_static_with_omitted_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
class Handler {
    public function __construct(
        public ?string $logger = null,
        public bool $debug = false,
    ) {}

    public static function register(?self $handler = null): self {
        if (null === $handler) {
            $handler = new static();
        }
        return $handler;
    }
}

class ChildHandler extends Handler {}

$handler = ChildHandler::register();
echo $handler::class, "|", null === $handler->logger ? "null" : $handler->logger, "|", $handler->debug ? "debug" : "quiet";
"#,
    );
    assert_eq!(out, "ChildHandler|null|quiet");
}

/// Verifies a static child override may narrow a parent-class return to `static`.
#[test]
fn test_static_override_covariant_self_return() {
    let out = compile_and_run(
        "<?php class Base { public static function make(): Base { return new static(); } } class Child extends Base { public static function make(): static { return new static(); } } echo Child::make() instanceof Child ? 'ok' : 'no';",
    );
    assert_eq!(out, "ok");
}

/// Verifies `new parent()` inside a child class returns an instance of the parent class `Base`.
#[test]
fn test_new_parent_returns_instance_of_parent_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public string $tag = \"base\";\n}\nclass Child extends Base {\n    public static function makeBase(): Base { return new parent(); }\n}\n$b = Child::makeBase();\necho $b->tag;\n",
    );
    assert_eq!(out, "base");
}

/// Verifies `new self`, `new static`, and `new parent` work without constructor parentheses.
#[test]
fn test_new_relative_receivers_without_constructor_parentheses() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public function who(): string { return \"base\"; }\n    public static function makeStatic() { return new static; }\n}\nclass Child extends Base {\n    public function who(): string { return \"child\"; }\n    public static function makeSelf() { return new self; }\n    public static function makeParent() { return new parent; }\n}\necho Child::makeSelf()->who(), \"|\", Child::makeStatic()->who(), \"|\", Child::makeParent()->who();\n",
    );
    assert_eq!(out, "child|child|base");
}

/// Verifies `new self()` passes constructor arguments correctly.
#[test]
fn test_new_self_with_constructor_args() {
    let out = compile_and_run(
        "<?php\nclass Greeter {\n    public string $name;\n    public function __construct(string $n) { $this->name = $n; }\n    public static function make(string $n): Greeter { return new self($n); }\n}\n$g = Greeter::make(\"Alice\");\necho $g->name;\n",
    );
    assert_eq!(out, "Alice");
}

// --- Static closures ---

/// Verifies static anonymous functions (closures) can be created and invoked with positional arguments.
#[test]
fn test_static_closure_runs() {
    let out = compile_and_run(
        "<?php $f = static function($a, $b) { return $a + $b; }; echo $f(3, 4);",
    );
    assert_eq!(out, "7");
}

/// Verifies static arrow functions (fn) can be created and invoked.
#[test]
fn test_static_arrow_function_runs() {
    let out = compile_and_run("<?php $g = static fn($x) => $x * 2; echo $g(5);");
    assert_eq!(out, "10");
}

/// Verifies a protected read in an impossible `$this instanceof` sibling branch is accepted.
#[test]
fn test_this_instanceof_incompatible_class_protected_read_is_dead() {
    let out = compile_and_run(
        r#"<?php
class AlternativeNode {
    public function __construct(protected ?AlternativeNode $parent = null) {}
}
class ActiveNode {
    use ReadsParent;
    public function __construct(private ?ActiveNode $parent = null) {}
}
trait ReadsParent {
    public function describe(): string {
        return $this instanceof AlternativeNode
            ? ($this->parent === null ? "unexpected" : "parent")
            : "active";
    }
}
echo (new ActiveNode())->describe();
"#,
    );
    assert_eq!(out, "active");
}

/// `new static(...)` pads omitted constructor defaults and gathers variadic arguments, as
/// `new self()` and a named `new Child()` already did: one default omitted, several omitted,
/// a variadic constructor with and without arguments, and a named argument. It failed with
/// `dynamic object construction for Tagged without EIR-lowered candidates`, because the
/// padding thunks were only generated for `new $class()` sites. Regression for #797.
#[test]
fn test_new_static_pads_omitted_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
class Tagged {
    public function __construct(private int $tag = 5) {}
    public static function make(): static { return new static(); }
    public function tag(): int { return $this->tag; }
}
class SubTagged extends Tagged {}
echo SubTagged::make()->tag(), "\n";
echo Tagged::make()->tag(), "\n";

class Pair {
    public function __construct(public int $a = 1, public int $b = 2) {}
    public static function one(int $a): static { return new static($a); }
    public static function none(): static { return new static(); }
}
class SubPair extends Pair {}
$p = SubPair::one(9);
echo get_class($p), " ", $p->a, ",", $p->b, "\n";
$q = Pair::none();
echo $q->a, ",", $q->b, "\n";

class Bag {
    public array $items;
    public function __construct(int ...$items) { $this->items = $items; }
    public static function of(int $x): static { return new static($x, $x + 1, $x + 2); }
    public static function empty(): static { return new static(); }
}
class SubBag extends Bag {}
echo count(SubBag::of(3)->items), " ", array_sum(SubBag::of(3)->items), "\n";
echo count(SubBag::empty()->items), "\n";

class Named {
    public function __construct(public string $label = "default", public int $n = 0) {}
    public static function labelled(): static { return new static(label: "x"); }
}
class SubNamed extends Named {}
$n = SubNamed::labelled();
echo $n->label, " ", $n->n, "\n";
for ($i = 0; $i < 20; $i++) { $t = SubTagged::make(); }
echo $t->tag(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "5\n",
            "5\n",
            "SubPair 9,2\n",
            "1,2\n",
            "3 12\n",
            "0\n",
            "x 0\n",
            "5\n",
        )
    );
}

/// A `new static('', ...)` factory whose descendant narrows the constructor to numeric
/// parameters must still compile: the descendant is simply not a candidate at that site.
///
/// Laravel's `Limit::perSecond()` is `new static('', $maxAttempts, $decaySeconds)` while
/// `GlobalLimit::__construct(int $maxAttempts, int $decaySeconds)` takes the attempts first. php
/// raises a `TypeError` only for a program that calls `GlobalLimit::perSecond()`; the checker used
/// to refuse the whole program ("parameter $maxAttempts expects Int, got Str"). The base class
/// still builds normally.
#[test]
fn test_new_static_with_literal_string_skips_numeric_descendant_constructor() {
    let out = compile_and_run(
        r#"<?php
class Limit {
    public function __construct(public $key = '', public int $max = 60, public int $decay = 60) {}
    public static function perSecond($max, $decay = 1) { return new static('', $max, $decay); }
}
class GlobalLimit extends Limit {
    public function __construct(int $max, int $decay = 60) { parent::__construct('', $max, $decay); }
}
$l = Limit::perSecond(5);
echo $l->max, ":", $l->decay, ":", get_class($l), "\n";
$g = new GlobalLimit(7);
echo $g->max, ":", $g->decay, ":", get_class($g), "\n";
"#,
    );
    assert_eq!(out, "5:1:Limit\n7:60:GlobalLimit\n");
}

/// Laravel's `EnumeratesValues::wrap()` is `new static($value, ...$args)` with `$value` narrowed to
/// an `Enumerable`, while `SortedMiddleware extends Collection` takes `array $priorityMap` first.
/// The spread hid the descendant from the candidate filter, so the checker refused `Collection`
/// ("parameter $priorityMap expects Array, got Object"), the class was deferred and failed to
/// declare at run time. An object can never reach an array parameter, so that descendant is not a
/// candidate at this call site; the array-taking branch still builds it.
#[test]
fn test_new_static_with_spread_skips_descendant_whose_array_param_gets_an_object() {
    let out = compile_and_run(
        r#"<?php
interface Enumerable {}
class Coll implements Enumerable {
    public array $items;
    public function __construct($items = []) { $this->items = $items instanceof Enumerable ? $items->items : (array) $items; }
    public static function wrap($value, ...$args) {
        return $value instanceof Enumerable
            ? new static($value, ...$args)
            : new static((array) $value, ...$args);
    }
}
class Sorted extends Coll {
    public function __construct(array $priorityMap, $middlewares) { parent::__construct($middlewares); }
}
$c = Coll::wrap(new Coll([1, 2]));
echo get_class($c), count($c->items), "\n";
$d = Coll::wrap([3]);
echo get_class($d), count($d->items), "\n";
$s = Sorted::wrap([1], [7, 8, 9]);
echo get_class($s), count($s->items), "\n";
"#,
    );
    assert_eq!(out, "Coll2\nColl1\nSorted3\n");
}

/// `static::$singletons[$key] ??= new static($locale)` whose constructor unsets another key of
/// the same static array (Carbon's `AbstractTranslator::get()`). php evaluates the default
/// before fetching the static property for write; separating the container first stored the
/// new instance into a stale copy, so the following read found no "en" key.
#[test]
fn test_static_array_element_write_sees_writes_made_by_its_right_hand_side() {
    let out = compile_and_run(
        r#"<?php
abstract class AbstractT {
    protected static array $singletons = [];
    public string $locale = '';
    public static function get(?string $locale = null): static {
        $locale = $locale ?: 'en';
        $key = static::class . '|' . $locale;
        static::$singletons[$key] ??= new static($locale);
        return static::$singletons[$key];
    }
    public static function put(string $key): void {
        static::$singletons[$key] = static::fill($key);
    }
    private static function fill(string $key): string {
        static::$singletons['side'] = 'effect';
        return strtoupper($key);
    }
    public function __construct($locale) { $this->setLocale($locale); }
    public function setLocale($locale): void {
        unset(static::$singletons[$this->locale]);
        $this->locale = $locale;
    }
    public static function keys(): string { return implode(',', array_keys(static::$singletons)); }
}
class Plain extends AbstractT {}
$a = Plain::get('en');
$b = Plain::get('en');
echo $a->locale, ' ', var_export($a === $b, true), "\n";
Plain::put('k');
echo AbstractT::keys(), "\n";
"#,
    );
    assert_eq!(out, "en true\nPlain|en,side,k\n");
}

/// Carbon's `Mixin` trait, shared by `Carbon` and `CarbonInterval`: a closure a STATIC method
/// builds tests `isset($this)` (its null receiver was captured as a raw word and the read
/// crashed), and calls `self::copyStep()`, which only one user of the trait declares (the other
/// user's copy was refused at compile time, where php raises only if that line runs).
#[test]
fn test_trait_closure_from_static_method_and_missing_self_method() {
    let out = compile_and_run(
        r#"<?php
trait Mixin {
    private static array $macros = [];
    private static function load(): void {
        static::$macros[static::class] = function (bool $deep) {
            $down = isset($this) ? $this : new static();
            if ($deep) {
                return self::copyStep($down);
            }
            return get_class($down) . ':shallow';
        };
    }
    public static function run(bool $deep): string {
        self::load();
        return (static::$macros[static::class])($deep);
    }
}
class Interval { use Mixin; private static function copyStep(self $a): string { return 'copied'; } }
class Moment { use Mixin; }
echo Interval::run(true), "|", Moment::run(false), "|";
try {
    Moment::run(true);
} catch (Error $e) {
    echo get_class($e), ":", $e->getMessage();
}
echo "\n";
"#,
    );
    assert_eq!(
        out,
        "copied|Moment:shallow|Error:Call to undefined method Moment::copyStep()\n"
    );
}

/// `static $result;` holds what an earlier call stored, so `if (null !== $result) return
/// $result;` in a `bool` method is the lazy-cache idiom (Symfony `Process::isPtySupported()`),
/// not a return of null. Typing the static as its null initializer refused the method.
#[test]
fn test_null_initialized_static_local_is_a_lazy_cache() {
    let out = compile_and_run(
        r#"<?php
class P {
    public static function supported(): bool {
        static $result;
        if (null !== $result) {
            return $result;
        }
        return $result = strlen('x') === 1;
    }
}
function counter(): int {
    static $n;
    $n = ($n ?? 0) + 1;
    return $n;
}
var_dump(P::supported(), P::supported());
echo counter(), counter(), counter(), "\n";
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n123\n");
}
