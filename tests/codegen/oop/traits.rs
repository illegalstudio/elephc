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

/// A used trait's abstract methods are requirements on the class, and an INHERITED method
/// satisfies one — even a final one, an instance one or a static one, and through a nested trait
/// use. The method that implements a requirement may carry `#[\Override]`, as PHP 8.3 accepts.
#[test]
fn test_trait_requirements_satisfied_by_inherited_methods_and_override() {
    let out = compile_and_run(
        r#"<?php
abstract class Widget {
    final public function invalidate(): string { return "inv"; }
    public static function make(): string { return "made"; }
}
trait Rows {
    abstract public function invalidate(): string;
    abstract public static function make(): string;
    abstract protected function rowPath(int $i): ?string;
    public function rows(): string { return $this->invalidate() . ":" . static::make() . ":" . $this->rowPath(2); }
}
trait Nested { use Rows; }
final class Panel extends Widget {
    use Nested;
    #[\Override]
    protected function rowPath(int $i): ?string { return "r$i"; }
}
echo (new Panel())->rows(), "\n";
"#,
    );
    assert_eq!(out, "inv:made:r2\n");
}

/// An `as` alias of an abstract trait method is a requirement too, so an inherited final method
/// of the alias name satisfies it, directly and through a nested trait's own `use` block. An
/// inherited protected method satisfies a public requirement and stays protected, as in PHP.
#[test]
fn test_trait_requirement_aliases_and_visibility_satisfied_by_inherited_methods() {
    let out = compile_and_run(
        r#"<?php
class P {
    final public function foo(): int { return 1; }
    final public function bar(): int { return 2; }
    protected function hidden(): int { return 3; }
}
trait T {
    abstract public function foo(): int;
    abstract public function hidden(): int;
    public function viaT(): int { return $this->hidden(); }
}
trait U { use T { foo as bar; } }
final class C extends P { use T { foo as bar; } }
final class D extends P { use U; }
echo (new C())->bar(), (new D())->bar(), (new C())->viaT(), "\n";
"#,
    );
    assert_eq!(out, "223\n");
}

/// Thirty levels of traits where each level's two traits both use the previous level's two
/// traits: the class-level requirement walk reaches the bottom traits along 2^30 `use` paths,
/// so each trait's requirement set must be memoized for this to compile at all. The abstract
/// method of a sibling trait is still found as a requirement, so `#[\Override]` on its
/// implementation holds.
#[test]
fn test_trait_requirements_of_shared_nested_traits_are_walked_once() {
    let mut source = String::from("<?php\ntrait T0a {}\ntrait T0b {}\n");
    for level in 1..=30 {
        for side in ["a", "b"] {
            source.push_str(&format!("trait T{level}{side} {{ use T{prev}a, T{prev}b; }}\n", prev = level - 1));
        }
    }
    source.push_str(
        "trait Needs { abstract public function need(): string; }\n\
         final class C { use T30a, T30b, Needs; #[\\Override] public function need(): string { return 'need'; } }\n\
         echo (new C())->need(), \"\\n\";\n",
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "need\n");
}

/// A trait that implements a method a used trait declares abstract satisfies it for its users:
/// aliasing that method gives a concrete alias. An alias taken inside the trait's own `use`
/// block, from the abstract method itself, stays a requirement, so a class method of that name
/// may carry `#[\Override]`.
#[test]
fn test_trait_aliases_follow_the_abstractness_of_the_selected_method() {
    let out = compile_and_run(
        r#"<?php
trait N { abstract public function foo(): string; }
trait A { use N; public function foo(): string { return "A::foo"; } }
trait B { use N { foo as bar; } public function foo(): string { return "B::foo"; } }
final class C { use A { foo as bar; } }
final class D { use B; #[\Override] public function bar(): string { return "D::bar"; } }
echo (new C())->bar(), (new C())->foo(), "|", (new D())->bar(), (new D())->foo(), "\n";
"#,
    );
    assert_eq!(out, "A::fooA::foo|D::barB::foo\n");
}
