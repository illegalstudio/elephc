//! Purpose:
//! Regression coverage for enum case property defaults and their reflected values.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Identity assertions distinguish canonical enum singletons from copies or null.

use crate::support::*;

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
