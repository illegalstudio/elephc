//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP traits, including trait basic method import, trait class method override wins, and trait insteadof and alias.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that a trait's public method is imported into a class that uses the trait.
#[test]
fn test_trait_basic_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "hello"; }
}
class Person {
    use Greeter;
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies that when a class defines the same method as an imported trait, the class method takes precedence.
#[test]
fn test_trait_class_method_override_wins() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "trait"; }
}
class Person {
    use Greeter;
    public function greet() { return "class"; }
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "class");
}

/// Verifies trait conflict resolution via insteadof and aliasing: one method is selected via insteadof, the other is aliased to a new name.
#[test]
fn test_trait_insteadof_and_alias() {
    let out = compile_and_run(
        r#"<?php
trait A {
    public function label() { return "A"; }
}
trait B {
    public function label() { return "B"; }
}
class Box {
    use A, B {
        A::label insteadof B;
        B::label as bLabel;
    }
}
$b = new Box();
echo $b->label();
echo ":";
echo $b->bLabel();
"#,
    );
    assert_eq!(out, "A:B");
}

/// Verifies that a trait property with a default value is accessible via a trait method when used by a class.
#[test]
fn test_trait_property_default_and_method_access() {
    let out = compile_and_run(
        r#"<?php
trait Counter {
    public $value = 7;
    public function read() { return $this->value; }
}
class Box {
    use Counter;
}
$b = new Box();
echo $b->read();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that a trait can use another trait; the outer trait's method can call the inner trait's method via $this.
#[test]
fn test_trait_can_use_another_trait() {
    let out = compile_and_run(
        r#"<?php
trait BaseGreeter {
    public function greet() { return "A"; }
}
trait FancyGreeter {
    use BaseGreeter;
    public function greetTwice() { return $this->greet() . "B"; }
}
class Person {
    use FancyGreeter;
}
$p = new Person();
echo $p->greetTwice();
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies that a static method from a trait is imported and callable on the class directly.
#[test]
fn test_trait_static_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Numbers {
    public static function one() { return 1; }
}
class Box {
    use Numbers;
}
echo Box::one();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that a trait method aliased as protected is callable from within the class but not from outside.
#[test]
fn test_trait_protected_alias_is_callable_inside_class() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() {
        return "hello";
    }
}

class Demo {
    use Greeter {
        Greeter::greet as protected innerGreet;
    }

    public function reveal() {
        return $this->innerGreet();
    }
}

$demo = new Demo();
echo $demo->reveal();
"#,
    );
    assert_eq!(out, "hello");
}

/// Trait adaptations accept a semi-reserved keyword wherever they name a method: the bare target
/// (`match as …`), the `Trait::list` target and the alias (`as print`), with or without a
/// visibility change — as tempest/highlight's `use IsPattern { match as matchContent; }` does.
#[test]
fn test_trait_adaptation_keyword_method_names() {
    let out = compile_and_run(
        r#"<?php
trait Patterns {
    public function match(string $s): string { return "m:" . $s; }
    public function list(): string { return "l"; }
}

final class Highlighter {
    use Patterns {
        match as protected matchContent;
        Patterns::list as print;
    }

    public function match(string $s): string { return "[" . $this->matchContent($s) . "]"; }
}

$h = new Highlighter();
echo $h->match("x"), "|", $h->print(), "|", $h->list();
"#,
    );
    assert_eq!(out, "[m:x]|l|l");
}

/// Keyword method names also resolve conflicts: `A::print insteadof B` and `B::list insteadof A`
/// pick each trait's keyword method, `B::print as function` keeps the excluded one under a keyword
/// alias, and after a visibility the alias may itself be a modifier (`as protected static`), as in
/// PHP's `member_modifier identifier` rule.
#[test]
fn test_trait_insteadof_and_modifier_alias_with_keyword_names() {
    let out = compile_and_run(
        r#"<?php
trait A {
    public function print(): string { return "A"; }
    public function list(): string { return "Al"; }
}
trait B {
    public function print(): string { return "B"; }
    public function list(): string { return "Bl"; }
}

class C {
    use A, B {
        A::print insteadof B;
        B::list insteadof A;
        B::print as function;
        A::list as protected static;
    }

    public function viaStatic(): string { return $this->static(); }
}

$c = new C();
echo $c->print(), "|", $c->list(), "|", $c->function(), "|", $c->viaStatic();
"#,
    );
    assert_eq!(out, "A|Bl|B|Al");
}

/// Verifies that a child class can satisfy an abstract property declaration from a trait it inherits via an abstract class.
#[test]
fn test_abstract_trait_property_can_be_satisfied_by_concrete_child() {
    let out = compile_and_run(
        r#"<?php
trait RequiresValue {
    abstract public int $value { get; set; }
}

abstract class Base {
    use RequiresValue;
}

class Box extends Base {
    public int $value = 9;
}

$box = new Box();
echo $box->value;
"#,
    );
    assert_eq!(out, "9");
}
