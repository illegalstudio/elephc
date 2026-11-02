//! Purpose:
//! Interpreter tests pinning every shape PHP allows after `new` when the class name is not a
//! literal -- a property, a property element, an array element, a static property reached
//! through an object, a parenthesised expression, and the relative keywords.
//! `new $required->class()` is `dependency-injection`'s `KernelTrait`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::new_class_expressions`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - The refusal came from a guard meant for a DIFFERENT operator. `parse_variable_class_name_target`
//!   refuses a `(` after a property read so `instanceof $obj->method()` -- which PHP's grammar
//!   does not allow -- cannot be read as a class name, and the same guard was rejecting the
//!   CONSTRUCTOR's own parentheses. The two positions now say which one they are.
//! - Each shape passes a distinct constructor argument, so a lowering that built the right class
//!   with the wrong arguments cannot pass.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse new class expression fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute new class expression fragment");
    values.output.clone()
}

/// Verifies a class name held in a PROPERTY, a property ELEMENT and an array element.
///
/// `php -n` 8.5.6 prints `d;m;a`. The first is `KernelTrait`'s
/// `new $required->class()`, whose parentheses are the constructor's and were being read as a
/// method call.
#[test]
fn a_property_an_element_and_an_array_element_name_the_class() {
    assert_eq!(
        out(
            br#"class Made { public $tag; public function __construct($tag = "d") { $this->tag = $tag; } }
class Holder { public $cls = "Made"; public $map = ["k" => "Made"]; }
$h = new Holder();
echo (new $h->cls())->tag, ";";
echo (new $h->map['k']("m"))->tag, ";";
$arr = ["k" => "Made"];
echo (new $arr['k']("a"))->tag;"#
        ),
        "d;m;a",
    );
}

/// Verifies a STATIC property reached through an object names the class.
///
/// `php -n` 8.5.6 prints `s`. `$h::$stat` reads the static property of `$h`'s class, which the
/// dynamic class-name walker had no rule for at all.
#[test]
fn a_static_property_through_an_object_names_the_class() {
    assert_eq!(
        out(
            br#"class Made { public $tag; public function __construct($tag = "d") { $this->tag = $tag; } }
class Holder { public static $stat = "Made"; }
$h = new Holder();
echo (new $h::$stat("s"))->tag;"#
        ),
        "s",
    );
}

/// Verifies a PARENTHESISED expression names the class, including a computed one.
///
/// `php -n` 8.5.6 prints `p;c`. The second builds the name by concatenation, so the expression
/// really is evaluated rather than matched as a name.
#[test]
fn a_parenthesised_expression_names_the_class() {
    assert_eq!(
        out(
            br#"class Made { public $tag; public function __construct($tag = "d") { $this->tag = $tag; } }
$name = "Made";
echo (new ($name)("p"))->tag, ";";
echo (new ("Ma" . "de")("c"))->tag;"#
        ),
        "p;c",
    );
}

/// Verifies `new static`, `new self` and `new parent` resolve as PHP resolves them.
///
/// `php -n` 8.5.6 prints `Child;Base;Base`. `static` follows the CALLED class and `self` follows
/// the declaring one, which is the whole distinction between them; `parent` is pinned beside
/// them because it is the third relative keyword and shares the path.
#[test]
fn the_relative_class_keywords_resolve_as_php_resolves_them() {
    assert_eq!(
        out(
            br#"class Base {
    public static function makeStatic(): static { return new static(); }
    public function makeSelf(): self { return new self(); }
}
class Child extends Base {
    public function makeParent(): Base { return new parent(); }
}
echo get_class(Child::makeStatic()), ";";
echo get_class((new Child())->makeSelf()), ";";
echo get_class((new Child())->makeParent());"#
        ),
        "Child;Base;Base",
    );
}
