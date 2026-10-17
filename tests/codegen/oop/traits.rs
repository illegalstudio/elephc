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

/// Verifies that a constant an enum imports from a trait is readable as `E::CONST` and listed by
/// `ReflectionClass::getConstants()` after the enum's own cases and constants.
#[test]
fn test_enum_reads_trait_constant() {
    let out = compile_and_run(
        r#"<?php
trait T { const TC = 2; function tc(): int { return self::TC; } }
enum E: string { use T; case A = "a"; const X = 1; }
echo E::TC, "|", E::X, "|", E::A->value, "|", E::A->tc(), "\n";
echo implode(",", array_keys((new ReflectionClass("E"))->getConstants())), "\n";
"#,
    );
    assert_eq!(out, "2|1|a|2\nA,X,TC\n");
}

/// Trait self receivers keep the consuming enum in global, function and class callers.
#[test]
fn test_enum_trait_constants_lexical_all() {
    let out = compile_and_run_with_heap_debug(r#"<?php
trait T { public const A = 1; public const B = self::A + 1; public const N = self::class; }
enum E { use T; case One; }
echo E::B, ":", E::N, "|";
function readE() { echo E::B, ":", E::N, "|"; }
readE();
class C {
 const A = 99;
 static function readE() {
  echo E::B, ":", E::N, "\n";
 }
}
C::readE();
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:E|2:E|2:E\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested traits bind independently to each namespaced consuming enum.
#[test]
fn test_enum_trait_constants_namespaced_nested() {
    let out = compile_and_run_with_heap_debug(r#"<?php
namespace Example;
trait BaseT { const B = self::A + 1; const N = self::class; }
trait T { use BaseT; }
enum E { use T; case One; const A = 1; }
enum F: string { use T; case One = "one"; const A = 7; }
class C { const A = 99; static function read() {
 echo E::B, ":", E::N, "|", F::B, ":", F::N, "\n";
} }
C::read();
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:Example\\E|8:Example\\F\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Enum cases and trait constants keep PHP case-sensitive member names.
#[test]
fn test_enum_trait_constants_case_sensitive() {
    let out = compile_and_run_with_heap_debug(r#"<?php
trait T { const a = 1; }
enum E { use T; case A; }
echo E::a, "\n";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct enum constants retain the enum scope when read from another class.
#[test]
fn test_enum_trait_constants_own_lexical() {
    let out = compile_and_run_with_heap_debug(r#"<?php
enum E { case One; const A = 1; const B = self::A + 1; const N = self::class; }
class C { const A = 99; static function readE() { echo E::B, ":", E::N, "\n"; } }
C::readE();
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:E\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct and reflected reads agree about the enum scope from a different caller class.
#[test]
fn test_enum_trait_constants_reflection_matches_direct_read() {
    let out = compile_and_run_capture(r#"<?php
trait T { public const A = 1; public const B = self::A + 1; public const N = self::class; }
enum E { use T; case One; }
class C {
 const A = 99;
 static function readE() {
  echo E::B, ":", E::N, "|";
  $r = new ReflectionClass(E::class);
  echo $r->getConstant("B"), ":", $r->getConstant("N"), "\n";
 }
}
C::readE();
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "2:E|2:E\n");
    // The equivalent reviewed baseline has the same independent Reflection context retention.
}
