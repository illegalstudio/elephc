//! Purpose:
//! Regression coverage for enum case property defaults and their reflected values.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Identity assertions distinguish canonical enum singletons from copies or null.

use crate::support::*;

/// Runtime eval materializes projected defaults in source order before returning the case.
#[test]
fn test_enum_property_defaults_review_runtime_eval_order() {
    let out = compile_and_run(r#"<?php
enum EvalLevel { case Low; case High; }
class EvalHolder { const ALL = [EvalLevel::Low, EvalLevel::High]; const DEFAULT = self::ALL[1]; }
class EvalConfig { public EvalLevel $value = EvalHolder::DEFAULT; }
$code = $argc > 0 ? '$defaults = (new ReflectionClass("EvalConfig"))->getDefaultProperties();
var_dump($defaults["value"] === EvalLevel::High);
var_dump(spl_object_id(EvalLevel::Low) < spl_object_id(EvalLevel::High));' : '';
eval($code);
"#);
    assert_eq!(out, "bool(true)\n".repeat(2));
}

/// Enum projections and selectors preserve evaluated cases and leave untaken cases lazy.
#[test]
fn test_enum_property_defaults_review_materialization_order() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class H { const ALL = [E::A, E::B]; const DEFAULT = self::ALL[1]; }
class C { public E $value = H::DEFAULT; }
$c = new C();
var_dump($c->value === E::B, spl_object_id(E::A) < spl_object_id(E::B));
enum T { case A; case B; case Untaken; }
class TH { const DEFAULT = T::A ? T::B : T::Untaken; }
class TC { public static T $value = TH::DEFAULT; }
var_dump(TC::$value === T::B, spl_object_id(T::A) < spl_object_id(T::B));
$fresh = new stdClass();
var_dump(spl_object_id($fresh) < spl_object_id(T::Untaken));
"#);
    assert_eq!(out, "bool(true)\n".repeat(5));
}

/// Interface self aliases keep the original declaration scope through overriding interfaces.
#[test]
fn test_enum_property_defaults_review_interface_scope() {
    let out = compile_and_run(r#"<?php
enum E { case A; }
enum F { case B; }
interface Origin { const ITEM = E::A; const DEFAULT = self::ITEM; }
interface Alias extends Origin { const ITEM = F::B; }
class Config {
    const ITEM = F::B;
    public E $value = Alias::DEFAULT;
    public static ?E $shared = Alias::DEFAULT;
}
$c = new Config();
var_dump($c->value === E::A, Config::$shared === E::A);
$defaults = (new ReflectionClass(Config::class))->getDefaultProperties();
var_dump($defaults['value'] === E::A);
"#);
    assert_eq!(out, "bool(true)\n".repeat(3));
}

/// Enum-valued constant expressions resolve before instance and static slot selection.
#[test]
fn test_enum_property_defaults_review_constant_expressions() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; case High; }
class Holder {
    const ALL = [Level::Low, 'last' => Level::High];
    const FIRST = self::ALL[0];
    const NESTED = ['value' => [Level::High]];
    const DEFAULT = self::NESTED['value'][0];
}
class Config {
    public Level $value = Holder::FIRST;
    public static ?Level $shared = Holder::DEFAULT;
    public mixed $other = Holder::ALL['last'];
}
$c = new Config();
var_dump($c->value === Level::Low, Config::$shared === Level::High, $c->other === Level::High);
$defaults = (new ReflectionClass(Config::class))->getDefaultProperties();
var_dump($defaults['value'] === Level::Low, $defaults['shared'] === Level::High);
"#);
    assert_eq!(out, "bool(true)\n".repeat(5));
}

/// Nullable, union and mixed slots retain the enum singleton on every allocation path.
#[test]
fn test_enum_property_defaults_boxed_slots() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; case High; }
class Holder { const NUMBER = 7; }
class Config {
    public ?Level $nullable = Level::Low;
    public Level|string $union = Level::Low;
    public mixed $mixed = Level::Low;
    public static ?Level $shared = Level::Low;
    public static Level|string $choice = Level::Low;
    public static mixed $any = Level::Low;
}
class ScalarConfig {
    public mixed $number = Holder::NUMBER;
    public int|string $scalar = Holder::NUMBER;
}
$class = $argc > 0 ? Config::class : '';
$c = new $class();
var_dump($c->nullable === Level::Low, $c->union === Level::Low, $c->mixed === Level::Low);
var_dump(Config::$shared === Level::Low, Config::$choice === Level::Low, Config::$any === Level::Low);
$s = new ScalarConfig();
var_dump($s->number, $s->scalar);
"#);
    assert_eq!(out, format!("{}int(7)\nint(7)\n", "bool(true)\n".repeat(6)));
}

/// Relative receivers bind to the declaring class, including inherited defaults.
#[test]
fn test_enum_property_defaults_relative_receivers() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; case High; }
class Base { const LOW = Level::Low; }
class Config extends Base {
    const HIGH = Level::High;
    public Level $low = parent::LOW;
    public Level $high = self::HIGH;
    public static Level $shared = self::HIGH;
}
class Child extends Config { const HIGH = Level::Low; }
$c = new Child();
var_dump($c->low === Level::Low, $c->high === Level::High, Child::$shared === Level::High);
"#);
    assert_eq!(out, "bool(true)\n".repeat(3));
}

/// Class and inherited interface aliases resolve through multiple hops before allocation.
#[test]
fn test_enum_property_defaults_constant_aliases() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; }
interface Origin { const LOW = Level::Low; }
interface Alias extends Origin { const DEFAULT = Origin::LOW; }
class Holder implements Alias { const VALUE = Alias::DEFAULT; }
class Config {
    public Level $value = Holder::VALUE;
    public static Level $shared = Alias::DEFAULT;
    public ?Level $nullable = Holder::DEFAULT;
    public static mixed $any = Holder::VALUE;
}
$c = new Config();
var_dump($c->value === Level::Low, Config::$shared === Level::Low,
    $c->nullable === Level::Low, Config::$any === Level::Low);
"#);
    assert_eq!(out, "bool(true)\n".repeat(4));
}

/// Shadowing a private parent property uses semantic enum default validation.
#[test]
fn test_enum_property_defaults_private_parent_shadow() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; }
class Base {
    private int $value = 7;
    public function original(): int { return $this->value; }
}
class Config extends Base { public Level $value = Level::Low; }
$c = new Config();
var_dump($c->value === Level::Low, $c->original());
"#);
    assert_eq!(out, "bool(true)\nint(7)\n");
}

/// Reflection and eval expose pure and backed enum defaults with singleton identity.
#[test]
fn test_enum_property_defaults_reflection_and_eval() {
    let out = compile_and_run(r#"<?php
enum Level { case Low; case High; }
enum State: string { case Ready = 'ready'; }
interface Defaults { const LEVEL = Level::Low; }
class Config {
    const DEFAULT = Defaults::LEVEL;
    public Level $level = Level::Low;
    public static State $state = State::Ready;
    public ?Level $alias = self::DEFAULT;
}
class Child extends Config { const DEFAULT = Level::High; }
$defaults = (new ReflectionClass(Child::class))->getDefaultProperties();
var_dump($defaults['level'] === Level::Low, $defaults['state'] === State::Ready);
var_dump((new ReflectionProperty(Child::class, 'level'))->getDefaultValue() === Level::Low);
var_dump($defaults['alias'] === Level::Low);
eval('$defaults = (new ReflectionClass(Child::class))->getDefaultProperties();
var_dump($defaults["level"] === Level::Low, $defaults["state"] === State::Ready);
var_dump((new ReflectionProperty(Child::class, "level"))->getDefaultValue() === Level::Low);
var_dump($defaults["alias"] === Level::Low);');
"#);
    assert_eq!(out, "bool(true)\n".repeat(8));
}

/// An enum-branch class constant initializes an instance slot through the EIR thunk.
#[test]
fn test_enum_property_defaults_review_branch_constant_instance_slot() {
    let out = compile_and_run(r#"<?php
enum T { case A; case B; case Untaken; }
class C {
    const DEFAULT = T::A ? T::B : T::Untaken;
    public T $p = self::DEFAULT;
    public T|int $u = self::DEFAULT;
    public mixed $m = self::DEFAULT;
}
$c = new C();
var_dump($c->p === T::B, $c->u === T::B, $c->m === T::B);
function pick(bool $flag): T { $value = $flag ? T::B : T::Untaken; return $value; }
var_dump(pick(true) === T::B, pick(false) === T::Untaken);
"#);
    assert_eq!(out, "bool(true)\n".repeat(5));
}

/// Enum-dependent offsets and keys keep their singleton reads and resolve in every slot.
#[test]
fn test_enum_property_defaults_review_enum_offsets_and_keys() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class K {
    const ALL = [E::A, E::B];
    const PICK = self::ALL[E::A ? 1 : 0];
    const KEYED = [E::A ? 'x' : 'y' => E::B];
}
class Holder {
    public E $i = K::PICK;
    public E $k = K::KEYED['x'];
    public static E $s = K::PICK;
}
$h = new Holder();
var_dump($h->i === E::B, $h->k === E::B, Holder::$s === E::B);
$defaults = (new ReflectionClass('Holder'))->getDefaultProperties();
var_dump($defaults['i'] === E::B, $defaults['k'] === E::B, $defaults['s'] === E::B);
"#);
    assert_eq!(out, "bool(true)\n".repeat(6));
}

/// Projections and branches written directly on a property type-check as their selected case.
#[test]
fn test_enum_property_defaults_review_direct_projection_and_branch() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class Holder {
    const ALL = [E::A, E::B];
    public E $direct = Holder::ALL[0];
    public E $branch = E::A ? E::B : E::A;
    public static E $shared = self::ALL[1];
    public static E $selected = E::A ? E::B : E::A;
}
$h = new Holder();
var_dump($h->direct === E::A, $h->branch === E::B);
var_dump(Holder::$shared === E::B, Holder::$selected === E::B);
$defaults = (new ReflectionClass('Holder'))->getDefaultProperties();
var_dump($defaults['direct'] === E::A, $defaults['branch'] === E::B);
"#);
    assert_eq!(out, "bool(true)\n".repeat(6));
}

/// A scalar selected by an enum condition is initialized and reflected in both slot kinds.
#[test]
fn test_enum_property_defaults_review_enum_condition_scalar() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class C { public int $n = E::A ? 1 : 2; public static int $s = E::A ? 3 : 4; public float $f = E::B ? 5 : 6; }
var_dump((new C())->n, C::$s, (new C())->f);
$defaults = (new ReflectionClass('C'))->getDefaultProperties();
var_dump($defaults['n'], $defaults['s'], $defaults['f']);
var_dump((new ReflectionProperty('C', 'n'))->getDefaultValue());
"#);
    assert_eq!(out, "int(1)\nint(3)\nfloat(5)\nint(1)\nint(3)\nfloat(5)\nint(1)\n");
}

/// A static enum default keeps its enum's runtime metadata when nothing else names the enum.
#[test]
fn test_enum_property_defaults_review_static_only_enum_metadata() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class H { public static E $s = E::B; public static int $n = E::A ? 1 : 2; }
var_dump(H::$s);
echo get_class(H::$s), "\n";
"#);
    assert_eq!(out, "enum(E::B)\nE\n");
}

/// ReflectionClass constant metadata folds projections, branches and coalescing like PHP.
#[test]
fn test_enum_property_defaults_review_reflection_constant_projections() {
    let out = compile_and_run(r#"<?php
enum E { case A; case B; }
class K {
    const ALL = [E::A, E::B];
    const ZERO = self::ALL[0];
    const PICK = self::ALL[E::A ? 1 : 0];
    const FLAG = true ? 1 : 2;
    const NAMES = ['x' => 'one', 'y' => 'two'];
    const NAME = self::NAMES['y'] ?? 'none';
    const BOTH = E::A && false;
}
$constants = (new ReflectionClass('K'))->getConstants();
var_dump($constants['ZERO'] === E::A, $constants['PICK'] === E::B);
var_dump($constants['FLAG'], $constants['NAME'], $constants['BOTH']);
var_dump((new ReflectionClassConstant('K', 'PICK'))->getValue() === E::B);
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nint(1)\nstring(3) \"two\"\nbool(false)\nbool(true)\n");
}
