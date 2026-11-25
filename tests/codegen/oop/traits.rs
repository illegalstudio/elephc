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

/// An abstract trait method is a requirement, not a definition: two traits may both require a
/// method, and a concrete trait method satisfies another trait's abstract one in either order,
/// whether the traits come in one `use` or in several.
#[test]
fn test_trait_abstract_method_requirements_do_not_conflict() {
    let out = compile_and_run(
        r#"<?php
trait Rows { abstract public function label(): string; public function rows(): string { return "r" . $this->label(); } }
trait Hover { abstract public function label(): string; public function hover(): string { return "h" . $this->label(); } }
final class Panel { use Rows; use Hover; public function label(): string { return "p"; } }
final class Pane { use Rows, Hover; public function label(): string { return "q"; } }

trait NeedsLabel { abstract public function label(): string; public function show(): string { return "<" . $this->label() . ">"; } }
trait GivesLabel { public function label(): string { return "x"; } }
final class First { use NeedsLabel; use GivesLabel; }
final class Second { use GivesLabel; use NeedsLabel; }
final class Both { use NeedsLabel, GivesLabel; }
final class Requirements { use Rows, Hover, GivesLabel; }

$panel = new Panel(); $pane = new Pane(); $req = new Requirements();
echo $panel->rows(), $panel->hover(), $pane->rows(), $pane->hover(), "|";
echo (new First())->show(), (new Second())->show(), (new Both())->show(), $req->rows(), $req->hover();
"#,
    );
    assert_eq!(out, "rphprqhq|<x><x><x>rxhx");
}

/// A class's own `use` list keeps an abstract trait method as a requirement even when another
/// trait in it implements the method, and a wrapper trait's alias of the abstract method stays
/// abstract: `#[\Override]` on the class method holds in both cases, as in php 8.5.12.
#[test]
fn test_trait_requirement_kept_by_class_use_list_and_abstract_alias() {
    let out = compile_and_run(
        r#"<?php
trait NeedsFoo { abstract public function foo(): string; }
trait GivesFoo { public function foo(): string { return "G"; } }
trait W { use NeedsFoo, GivesFoo { NeedsFoo::foo as bar; } }
final class Direct { use NeedsFoo, GivesFoo; #[\Override] public function foo(): string { return "D"; } }
final class Wrapped { use W; #[\Override] public function bar(): string { return "B"; } }
echo (new Direct())->foo(), (new Wrapped())->bar(), (new Wrapped())->foo(), "\n";
"#,
    );
    assert_eq!(out, "DBG\n");
}

/// `insteadof` that keeps a trait's abstract method over another trait's body leaves the
/// wrapper trait's method abstract, so a class using it implements a requirement and
/// `#[\Override]` holds, as php 8.5.12 accepts.
#[test]
fn test_trait_requirement_kept_abstract_by_insteadof_in_a_wrapper_trait() {
    let out = compile_and_run(
        r#"<?php
trait NeedsFoo { abstract public function foo(): string; }
trait GivesFoo { public function foo(): string { return "g"; } }
trait W { use NeedsFoo, GivesFoo { NeedsFoo::foo insteadof GivesFoo; } }
final class C { use W; #[\Override] public function foo(): string { return "c"; } }
echo (new C())->foo(), "\n";
"#,
    );
    assert_eq!(out, "c\n");
}

/// The same trait method reached along two `use` paths is one method, not a collision: PHP
/// compares where the method comes from, not its name. This holds for one `use A, B`, separate
/// `use` statements, a nested trait, an alias, `insteadof`, a static method, and a class or
/// parent method that overrides it. Expected output measured with php 8.5.12.
#[test]
fn test_trait_method_shared_through_two_traits_is_not_a_collision() {
    let out = compile_and_run(
        r#"<?php
trait Greets { public function hi(): string { return "hi"; } public static function make(): string { return "made"; } }
trait Loud { use Greets; }
trait Quiet { use Greets; }
trait Renames { use Greets { hi as hello; } }
trait Both { use Loud, Quiet; }
final class OneUse { use Loud, Quiet; }
final class TwoUses { use Loud; use Quiet; }
final class Direct { use Loud, Greets; }
final class Nested { use Both; }
final class Aliased { use Renames, Quiet; }
final class Chosen { use Loud, Quiet { Loud::hi insteadof Quiet; Quiet::hi as hey; } }
final class Own { use Loud, Quiet; public function hi(): string { return "own"; } }
class Base { public function hi(): string { return "base"; } }
final class Child extends Base { use Loud, Quiet; }
echo (new OneUse())->hi(), OneUse::make(), " ", (new TwoUses())->hi(), " ", (new Direct())->hi(), " ";
echo (new Nested())->hi(), " ", (new Aliased())->hi(), (new Aliased())->hello(), " ";
echo (new Chosen())->hi(), (new Chosen())->hey(), " ", (new Own())->hi(), " ", (new Child())->hi(), "\n";
"#,
    );
    assert_eq!(out, "himade hi hi hi hihi hihi own hi\n");
}

/// A concrete trait method satisfies an abstract one from a sibling trait, also when the
/// requirement comes from a nested trait, when the concrete method is private or static, when
/// `insteadof` picks it, when the class defines the method itself, and when it widens the
/// signature. Expected output measured with php 8.5.12.
#[test]
fn test_trait_requirement_satisfied_by_sibling_trait_method() {
    let out = compile_and_run(
        r#"<?php
trait Needs { abstract public function label(): string; public function show(): string { return "<" . $this->label() . ">"; } }
trait Gives { public function label(): string { return "g"; } }
trait Wraps { use Needs; }
trait Hidden { private function label(): string { return "h"; } }
trait NeedsStatic { abstract public static function tag(): string; public static function tagged(): string { return "[" . static::tag() . "]"; } }
trait GivesStatic { public static function tag(): string { return "s"; } }
trait Other { public function label(): string { return "o"; } }
trait Wide { public function label(int $x = 0): string { return "w$x"; } }
final class Sibling { use Wraps, Gives; }
final class Privy { use Needs, Hidden; }
final class Statics { use NeedsStatic, GivesStatic; }
final class Picked { use Needs, Gives, Other { Other::label insteadof Gives; Gives::label as gives; } }
final class Mine { use Needs, Gives; public function label(): string { return "m"; } }
final class Widened { use Needs, Wide; }
echo (new Sibling())->show(), (new Privy())->show(), Statics::tagged(), (new Picked())->show(), (new Picked())->gives(), (new Mine())->show(), (new Widened())->show(), "\n";
"#,
    );
    assert_eq!(out, "<g><h>[s]<o>g<m><w0>\n");
}

/// One instantiation of a generic trait reached along two `use` paths is one method, as a
/// plain shared trait is (`test_trait_method_shared_through_two_traits_is_not_a_collision`).
#[test]
fn test_generic_trait_instantiation_shared_through_two_traits() {
    let out = compile_and_run(
        r#"<?php
trait Holder<T> { public function put(T $v): string { return "put " . $v; } }
trait A { use Holder<int>; }
trait B { use Holder<int>; }
class C { use A, B; }
echo (new C())->put(7), "\n";
"#,
    );
    assert_eq!(out, "put 7\n");
}

/// A trait that implements a requirement of a trait it uses, by its own method or by another
/// used trait's, absorbs it: a class may then replace that method or alias it with any
/// signature, as php 8.5.12 accepts.
#[test]
fn test_trait_requirement_satisfied_inside_a_trait_does_not_reach_its_users() {
    let out = compile_and_run(
        r#"<?php
trait N { abstract public function foo(): int; }
trait A { use N; public function foo(): int { return 1; } }
trait G { public function foo(): int { return 2; } }
trait W { use N, G; }
final class Aliased { use A { foo as bar; } public function bar(): string { return "b"; } }
final class Replaced { use A; public function foo(): string { return "r"; } }
final class Wrapped { use W; public function foo(): string { return "w"; } }
echo (new Aliased())->bar(), (new Aliased())->foo(), (new Replaced())->foo(), (new Wrapped())->foo(), "\n";
"#,
    );
    assert_eq!(out, "b1rw\n");
}

/// A trait method that satisfies a nested requirement is checked with its `self` and `parent`
/// types naming the consuming class or enum, and an alias of a method excluded by `insteadof`
/// still satisfies a requirement, so a class may replace it, as php 8.5.12 accepts.
#[test]
fn test_trait_satisfied_requirement_relative_types_and_excluded_alias() {
    let out = compile_and_run(
        r#"<?php
class Base {}
trait N { abstract public function same(self $o): string; abstract public function up(parent $p): string; }
trait A { use N; public function same(self $o): string { return "s"; } public function up(parent $p): string { return "u"; } }
final class C extends Base { use A; }
trait EN { abstract public function same(self $o): string; }
trait EA { use EN; public function same(self $o): string { return "e"; } }
enum E { use EA; case X; }
trait Bar { abstract public function bar(): int; }
trait P { public function foo(): int { return 1; } }
trait Q { public function foo(): int { return 2; } }
trait W { use Bar, P, Q { P::foo insteadof Q; Q::foo as bar; } }
final class D { use W; public function bar(): string { return "d"; } }
final class F { use W; }
$c = new C();
echo $c->same($c), $c->up(new Base()), E::X->same(E::X), (new D())->foo(), (new D())->bar(), (new F())->bar(), "\n";
"#,
    );
    assert_eq!(out, "sue1d2\n");
}

/// A by-reference variadic that widens the declared element type of an abstract trait method's
/// by-reference variadic satisfies it. Expected output measured with php 8.5.13.
#[test]
fn test_trait_requirement_by_ref_variadic_may_widen() {
    let out = compile_and_run(
        r#"<?php
trait NeedsRefs { abstract public function take(int &...$xs): int; }
trait TakesRefs { public function take(int|string &...$xs): int { return count($xs); } }
final class Refs { use NeedsRefs, TakesRefs; }
$a = 1; $b = 2;
echo (new Refs())->take($a, $b), "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// `foo as final;` (PHP 8.3+) imports the trait method as final and `foo as final bar;` adds a
/// final alias while the original stays overridable, as reflection reports (#2352).
#[test]
fn test_trait_alias_as_final_marks_the_method_final() {
    let out = compile_and_run(
        r#"<?php
trait T {
    public function foo() { return "foo"; }
    public function baz() { return "baz"; }
}
class C {
    use T { foo as final; baz as final bar; }
}
$c = new C();
echo $c->foo(), $c->bar(), $c->baz(), "\n";
var_dump((new ReflectionMethod('C', 'foo'))->isFinal());
var_dump((new ReflectionMethod('C', 'bar'))->isFinal());
var_dump((new ReflectionMethod('C', 'baz'))->isFinal());
"#,
    );
    assert_eq!(out, "foobazbaz\nbool(true)\nbool(true)\nbool(false)\n");
}
