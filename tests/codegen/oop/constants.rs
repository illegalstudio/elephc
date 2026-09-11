//! Purpose:
//! End-to-end codegen tests for class, interface, trait, and enum constants.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Constant values are inlined by codegen rather than looked up at runtime.
//! - Inheritance and visibility cases cover schema/codegen agreement.

use super::*;

/// Verifies keyword-named class and interface constants preserve PHP's case-sensitive identity,
/// including declarations whose spellings differ only by case.
#[test]
fn test_keyword_named_class_and_interface_constants_preserve_case() {
    let out = compile_and_run(
        "<?php
        class KeywordConstants {
            const Match = 1;
            const MATCH = 2;
            const Default = 3;
        }
        interface KeywordInterface {
            const Print = 4;
            const PRINT = 5;
        }
        echo KeywordConstants::Match, KeywordConstants::MATCH,
             KeywordConstants::Default, KeywordInterface::Print, KeywordInterface::PRINT;
        ",
    );
    assert_eq!(out, "12345");
}

/// Verifies class constant int.
#[test]
fn test_class_constant_int() {
    //! Verifies integer class constant is inlined and accessible via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
class Math {
    const PI = 314;
}
echo Math::PI;
"#,
    );
    assert_eq!(out, "314");
}

/// Verifies class constant string.
#[test]
fn test_class_constant_string() {
    //! Verifies string class constant is inlined and accessible via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
class Greet {
    const HELLO = "hi";
}
echo Greet::HELLO;
"#,
    );
    assert_eq!(out, "hi");
}

/// Verifies class constant inherited from parent.
#[test]
fn test_class_constant_inherited_from_parent() {
    //! Verifies child class inherits parent constants via ClassName::CONST lookup.
    let out = compile_and_run(
        r#"<?php
class Base {
    const VERSION = 7;
}
class Child extends Base {}
echo Child::VERSION;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies class constant expression can reference self constant.
#[test]
fn test_class_constant_expression_can_reference_self_constant() {
    //! Verifies constant expressions can use self:: to reference other constants in the same class.
    let out = compile_and_run(
        r#"<?php
class Box {
    const A = 1;
    const B = self::A + 2;
}
echo Box::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies inherited class constant expression keeps lexical self.
#[test]
fn test_inherited_class_constant_expression_keeps_lexical_self() {
    //! Verifies self:: in a parent constant expression refers to the defining class, not the runtime subclass.
    //! Regression: lexical self must not be replaced with runtime dynamic dispatch.
    let out = compile_and_run(
        r#"<?php
class Base {
    const A = 1;
    const B = self::A + 2;
}
class Child extends Base {
    const A = 10;
}
echo Child::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies class constant expression can reference parent constant.
#[test]
fn test_class_constant_expression_can_reference_parent_constant() {
    //! Verifies constant expressions can use parent:: to access inherited constants.
    let out = compile_and_run(
        r#"<?php
class Base {
    const A = 1;
}
class Child extends Base {
    const B = parent::A + 2;
}
echo Child::B;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies class constant expression can use self class.
#[test]
fn test_class_constant_expression_can_use_self_class() {
    //! Verifies self::class magic constant works inside a constant expression.
    let out = compile_and_run(
        r#"<?php
class Box {
    const NAME = self::class;
}
echo Box::NAME;
"#,
    );
    assert_eq!(out, "Box");
}

/// Verifies class constant self access inside method.
#[test]
fn test_class_constant_self_access_inside_method() {
    //! Verifies self::CONST inside an instance method resolves to the defining class constant.
    let out = compile_and_run(
        r#"<?php
class Box {
    const SIZE = 42;
    public function describe(): int { return self::SIZE; }
}
$b = new Box();
echo $b->describe();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies interface constant.
#[test]
fn test_interface_constant() {
    //! Verifies interface constants are accessible through implementing class and via ClassName::CONST.
    let out = compile_and_run(
        r#"<?php
interface Limits {
    const MAX = 100;
}
class Bound implements Limits {
    public function get(): int { return Limits::MAX; }
}
$b = new Bound();
echo $b->get();
"#,
    );
    assert_eq!(out, "100");
}

/// Verifies final class constants cannot be redeclared by subclasses.
#[test]
fn test_final_class_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
class Base {
    final public const LIMIT = 1;
}
class Child extends Base {
    public const LIMIT = 2;
}
"#,
    );
    assert!(err.contains("cannot override final constant"), "{err}");
}

/// Verifies final interface constants cannot be redeclared by child interfaces or implementors.
#[test]
fn test_final_interface_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
interface Limits {
    final public const MAX = 100;
}
class Bound implements Limits {
    public const MAX = 200;
}
"#,
    );
    assert!(
        err.contains("cannot override final interface constant"),
        "{err}"
    );
}

/// Verifies child interfaces cannot redeclare final parent interface constants.
#[test]
fn test_final_parent_interface_constant_override_fails() {
    let err = compile_expect_type_error(
        r#"<?php
interface Limits {
    final public const MAX = 100;
}
interface ChildLimits extends Limits {
    public const MAX = 200;
}
"#,
    );
    assert!(
        err.contains("cannot override final interface constant"),
        "{err}"
    );
}

/// Verifies private class constants cannot be final.
#[test]
fn test_final_private_class_constant_fails() {
    let err = compile_expect_type_error(
        r#"<?php
class Hidden {
    final private const SECRET = 1;
}
"#,
    );
    assert!(
        err.contains("Private constant Hidden::SECRET cannot be final"),
        "{err}"
    );
}

/// Verifies class constant with attribute compiles.
#[test]
fn test_class_constant_with_attribute_compiles() {
    //! Verifies constants with PHP attributes compile without error; attribute is discarded.
    let out = compile_and_run(
        r#"<?php
class Cfg {
    #[Documented]
    const TIMEOUT = 30;
}
echo Cfg::TIMEOUT;
"#,
    );
    assert_eq!(out, "30");
}

/// Verifies `static::CONST` uses late static binding to resolve the constant
/// from the actual runtime class (not the declaring class).
#[test]
fn test_static_constant_late_static_binding() {
    let out = compile_and_run(
        r#"<?php
class A { const X = 'A'; public static function show() { echo static::X; } }
class B extends A { const X = 'B'; }
B::show();
"#,
    );
    assert_eq!(out, "B");
}

/// Verifies `static::CONST` falls back to the declaring-class value when the
/// runtime class does not override the constant.
#[test]
fn test_static_constant_late_static_binding_fallback() {
    let out = compile_and_run(
        r#"<?php
class A { const X = 'A'; public static function show() { echo static::X . "\n"; } }
class B extends A { const X = 'B'; }
class C extends A { }
A::show();
B::show();
C::show();
"#,
    );
    assert_eq!(out, "A\nB\nA\n");
}

/// Verifies `static::CONST` works with integer constants and multiple overrides.
#[test]
fn test_static_constant_integer_override() {
    let out = compile_and_run(
        r#"<?php
class Base { const VAL = 10; public static function get() { return static::VAL; } }
class Derived extends Base { const VAL = 20; }
echo Base::get() . "\n";
echo Derived::get() . "\n";
"#,
    );
    assert_eq!(out, "10\n20\n");
}

/// PHP 8.3 typed class constants: `const TYPE NAME = ...` parses in class-like bodies and the
/// constants read back byte-identically to PHP 8.5.
#[test]
fn test_typed_class_constants_parse_and_read() {
    let out = compile_and_run(
        "<?php final class C { public const string NAME = 'n'; private const int LIMIT = 3; public static function d(): string { return self::NAME . ':' . (string) self::LIMIT; } } echo C::d();",
    );
    assert_eq!(out, "n:3");
}

/// Verifies typed constant overrides may narrow a parent union and retain the
/// declared type when the constant is used in another typed expression.
#[test]
fn test_typed_class_constant_covariant_override() {
    let out = compile_and_run(
        r#"<?php
class BaseLimit { public const int|string VALUE = 1; }
class IntLimit extends BaseLimit { public const int VALUE = 2; }
function read_limit(): int { return IntLimit::VALUE; }
echo read_limit();
"#,
    );
    assert_eq!(out, "2");
}

/// Issue #752: `defined('C::K')` must return true for a declared class constant.
#[test]
fn test_defined_literal_class_constant_issue_752() {
    let out = compile_and_run(
        r#"<?php
class K { const KEY = 42; }
var_dump(defined('K::KEY'));
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// Verifies `defined('Class::CONST')` case rules, missing members, and missing classes.
///
/// Class-like names are case-insensitive; constant names are case-sensitive.
/// A missing class or member is `false`, not a compile error. String names are
/// global, so a leading `\` is accepted and namespace/`use` is not applied.
#[test]
fn test_defined_literal_class_constant_case_and_missing() {
    let out = compile_and_run(
        r#"<?php
class KeywordConstants {
    const Match = 1;
    const MATCH = 2;
}
echo defined('KeywordConstants::Match') ? '1' : '0';
echo defined('keywordconstants::MATCH') ? '1' : '0';
echo defined('\\KeywordConstants::Match') ? '1' : '0';
echo defined('KeywordConstants::match') ? '1' : '0';
echo defined('KeywordConstants::MISSING') ? '1' : '0';
echo defined('MissingClass::KEY') ? '1' : '0';
"#,
    );
    assert_eq!(out, "111000");
}

/// Verifies `defined()` sees inherited class constants and implemented-interface constants.
#[test]
fn test_defined_literal_inherited_and_interface_constants() {
    let out = compile_and_run(
        r#"<?php
interface Limits {
    const MAX = 100;
}
interface ChildLimits extends Limits {}
class Base {
    const VERSION = 7;
}
class Bound extends Base implements Limits {}
echo defined('Base::VERSION') ? '1' : '0';
echo defined('Bound::VERSION') ? '1' : '0';
echo defined('Limits::MAX') ? '1' : '0';
echo defined('ChildLimits::MAX') ? '1' : '0';
echo defined('Bound::MAX') ? '1' : '0';
echo defined('Bound::MISSING') ? '1' : '0';
"#,
    );
    assert_eq!(out, "111110");
}

/// Verifies `defined()` reports enum cases and extra enum class constants.
///
/// Case names are case-sensitive; the enum type name is case-insensitive.
/// Extra enum constants follow class-constant visibility (`HIDDEN` is private).
#[test]
fn test_defined_literal_enum_cases_and_constants() {
    let out = compile_and_run(
        r#"<?php
enum Suit {
    case Hearts;
    case Spades;
    const COUNT = 2;
    private const HIDDEN = 1;
    public static function inside(): string {
        return defined('Suit::HIDDEN') ? '1' : '0';
    }
}
echo defined('Suit::Hearts') ? '1' : '0';
echo defined('suit::Spades') ? '1' : '0';
echo defined('Suit::COUNT') ? '1' : '0';
echo defined('Suit::hearts') ? '1' : '0';
echo defined('Suit::Clubs') ? '1' : '0';
echo defined('Suit::HIDDEN') ? '1' : '0';
echo Suit::inside();
"#,
    );
    assert_eq!(out, "1110001");
}

/// Verifies `defined('Class::CONST')` strings are not namespace- or use-resolved.
///
/// PHP looks up the string as a global class-like name, so an unqualified
/// `'K::KEY'` inside `namespace App` does not mean `App\K::KEY`, and a `use`
/// alias is not applied.
#[test]
fn test_defined_literal_class_constant_ignores_namespace_and_use() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    class K { const KEY = 42; }
    echo defined('K::KEY') ? '1' : '0';
    echo defined('App\\K::KEY') ? '1' : '0';
    echo defined('\\App\\K::KEY') ? '1' : '0';
}
namespace {
    use Real as Alias;
    class Real { const KEY = 1; }
    echo defined('Alias::KEY') ? '1' : '0';
    echo defined('Real::KEY') ? '1' : '0';
}
"#,
    );
    assert_eq!(out, "01101");
}

/// Verifies `defined('Class::CONST')` visibility from global scope and related classes.
///
/// Public is visible everywhere. Private is only visible from the declaring class.
/// Protected is visible from the declaring class's inheritance family, so a parent
/// method can see a child's protected constant (`A::probe()` / `B::BP`). `self::`
/// and `parent::` use the lexical class; an inaccessible first declaration does not
/// fall through. Expected stdout was checked against PHP 8.5.
#[test]
fn test_defined_literal_class_constant_visibility_and_self_parent() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public const PUB = 1;
    protected const PROT = 2;
    private const PRIV = 3;
    public static function inside(): string {
        return (defined('Base::PUB') ? '1' : '0')
            . (defined('Base::PROT') ? '1' : '0')
            . (defined('Base::PRIV') ? '1' : '0')
            . (defined('Child::PUB') ? '1' : '0')
            . (defined('Child::PROT') ? '1' : '0')
            . (defined('Child::PRIV') ? '1' : '0')
            . (defined('self::PRIV') ? '1' : '0')
            . (defined('self::PROT') ? '1' : '0');
    }
}
class Child extends Base {
    public static function inside(): string {
        return (defined('Base::PUB') ? '1' : '0')
            . (defined('Base::PROT') ? '1' : '0')
            . (defined('Base::PRIV') ? '1' : '0')
            . (defined('Child::PROT') ? '1' : '0')
            . (defined('Child::PRIV') ? '1' : '0')
            . (defined('self::PROT') ? '1' : '0')
            . (defined('parent::PROT') ? '1' : '0')
            . (defined('parent::PRIV') ? '1' : '0')
            . (defined('self::PRIV') ? '1' : '0');
    }
    public static function relative(): string {
        return (defined('SELF::PROT') ? '1' : '0')
            . (defined('Parent::PROT') ? '1' : '0');
    }
}
class Unrelated {
    public static function inside(): string {
        return (defined('Base::PUB') ? '1' : '0')
            . (defined('Base::PROT') ? '1' : '0')
            . (defined('Base::PRIV') ? '1' : '0');
    }
}
class A {
    protected const AP = 4;
    public static function probe(): string {
        return (defined('B::BP') ? '1' : '0')
            . (defined('C::CP') ? '1' : '0')
            . (defined('B::AP') ? '1' : '0');
    }
}
class B extends A {
    protected const BP = 2;
}
class C extends A {
    protected const CP = 3;
    public static function probe(): string {
        return (defined('B::BP') ? '1' : '0')
            . (defined('C::AP') ? '1' : '0');
    }
}
class D {
    private const X = 1;
    public static function inside(): string {
        return (defined('D::X') ? '1' : '0')
            . (defined('E::X') ? '1' : '0');
    }
}
class E extends D {
    public const X = 2;
    public static function inside(): string {
        return (defined('D::X') ? '1' : '0')
            . (defined('E::X') ? '1' : '0')
            . (defined('parent::X') ? '1' : '0');
    }
}
echo defined('Base::PUB') ? '1' : '0';
echo defined('Base::PROT') ? '1' : '0';
echo defined('Base::PRIV') ? '1' : '0';
echo defined('Child::PUB') ? '1' : '0';
echo defined('Child::PROT') ? '1' : '0';
echo defined('Child::PRIV') ? '1' : '0';
echo '|';
echo Child::inside();
echo '|';
echo Base::inside();
echo '|';
echo Unrelated::inside();
echo '|';
echo A::probe();
echo '|';
echo C::probe();
echo '|';
echo defined('D::X') ? '1' : '0';
echo defined('E::X') ? '1' : '0';
echo '|';
echo D::inside();
echo '|';
echo E::inside();
echo '|';
echo Child::relative();
"#,
    );
    assert_eq!(out, "100100|110101100|11111011|100|111|01|01|11|010|11");
}

/// Verifies a nested closure keeps the enclosing method's class scope for `defined()`.
#[test]
fn test_defined_literal_class_constant_visibility_in_closure() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private const PRIV = 3;
    public static function inside(): string {
        $fn = function () {
            return defined('Base::PRIV') ? '1' : '0';
        };
        return $fn();
    }
}
echo Base::inside();
echo defined('Base::PRIV') ? '1' : '0';
"#,
    );
    assert_eq!(out, "10");
}
