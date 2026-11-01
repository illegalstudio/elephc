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

/// Verifies `isset()` distinguishes present and missing keys in a class constant hash.
#[test]
fn test_isset_class_constant_hash_key_presence() {
    let out = compile_and_run(
        r#"<?php
class ClassConstantIssetRegistry {
    private const INTERNAL_TYPES = [
        "int" => true,
        "string" => true,
    ];

    public static function has(string $name): bool {
        return isset(self::INTERNAL_TYPES[$name]);
    }
}

echo ClassConstantIssetRegistry::has("int") ? "hit" : "miss";
echo ":";
echo ClassConstantIssetRegistry::has("Symfony\\Component\\HttpFoundation\\Request") ? "hit" : "miss";
"#,
    );
    assert_eq!(out, "hit:miss");
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

/// Verifies a missing class constant makes a literal `defined()` guard select its fallback
/// before the unreachable access is type-checked.
#[test]
fn test_defined_missing_class_constant_prunes_unreachable_access() {
    let out = compile_and_run(
        r#"<?php
class FeatureFlags {
    const PRESENT = 7;
}
if (!defined('FeatureFlags::MISSING')) {
    echo 'fallback';
} else {
    echo FeatureFlags::MISSING;
}
"#,
    );
    assert_eq!(out, "fallback");
}

/// Verifies a declared class constant makes a literal `defined()` guard select the branch that
/// reads it while discarding an invalid fallback branch.
#[test]
fn test_defined_present_class_constant_prunes_unreachable_fallback() {
    let out = compile_and_run(
        r#"<?php
class FeatureFlags {
    const PRESENT = 7;
}
if (!defined('FeatureFlags::PRESENT')) {
    echo FeatureFlags::MISSING;
} else {
    echo FeatureFlags::PRESENT;
}
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies literal `defined()` probes follow closed-world class inheritance for both present
/// and absent constants.
#[test]
fn test_defined_class_constant_follows_declared_parent() {
    let out = compile_and_run(
        r#"<?php
class ParentFlags {
    const PRESENT = 9;
}
class ChildFlags extends ParentFlags {}
echo defined('ChildFlags::PRESENT') ? 'Y' : 'N';
echo defined('ChildFlags::MISSING') ? 'Y' : 'N';
"#,
    );
    assert_eq!(out, "YN");
}

/// Verifies a certain right-hand boolean still evaluates the left operand before pruning the
/// unreachable branch of a class-constant guard.
#[test]
fn test_defined_class_constant_guard_preserves_left_evaluation() {
    let out = compile_and_run(
        r#"<?php
class FeatureFlags {
    const PRESENT = 7;
}
function probe(): bool {
    echo 'P';
    return false;
}
if (probe() || !defined('FeatureFlags::MISSING')) {
    echo 'Y';
} else {
    echo FeatureFlags::MISSING;
}
"#,
    );
    assert_eq!(out, "PY");
}

/// Verifies a scoped constant on an unavailable class remains legal in a runtime-skipped branch,
/// matching PHP's lazy class resolution instead of rejecting the whole source during checking.
#[test]
fn test_unknown_class_constant_in_skipped_branch_compiles() {
    let out = compile_and_run(
        r#"<?php
if ($argc < 0) {
    echo OptionalConstantProvider::VALUE;
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies executing the same unresolved scoped constant terminates through a class-not-found
/// runtime fatal rather than producing an undefined backend feature diagnostic.
#[test]
fn test_unknown_class_constant_access_fails_at_runtime() {
    let err = compile_and_run_expect_failure("<?php echo OptionalConstantProvider::VALUE;");
    assert!(
        err.contains("Class \"OptionalConstantProvider\" not found"),
        "unexpected stderr: {err:?}",
    );
}

/// Verifies a runtime class-string resolves a declared class constant through the name registry.
#[test]
fn test_dynamic_class_string_constant_access() {
    let out = compile_and_run(
        r#"<?php
class DynamicFlags {
    const VALUE = 'dynamic';
}
$class = DynamicFlags::class;
echo $class::VALUE;
"#,
    );
    assert_eq!(out, "dynamic");
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

/// Verifies an object-typed receiver resolves its class constant without runtime string dispatch.
#[test]
fn test_class_constant_access_through_typed_object() {
    let out = compile_and_run(
        r#"<?php
class Container {
    public const IGNORE = 7;
}
function read(Container $container): int {
    return $container::IGNORE;
}
echo read(new Container());
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies a `mixed` receiver holding an object reads its own class's constant, and that the
/// read leaves the local's reference alone while a method call on the same local is in flight.
#[test]
fn test_class_constant_through_a_mixed_object_receiver() {
    let out = compile_and_run(
        r#"<?php
final class Meta {
    public const K = 2;
    public function __construct(private string $n) {}
    public function getName(): string { return $this->n; }
}
function items(): mixed { return $GLOBALS['argc'] > 0 ? [new Meta('x'), new Meta('y')] : null; }
$names = [];
foreach (items() as $m) {
    $names[] = $m->getName() . ':' . $m::K;
}
echo implode(',', $names);
"#,
    );
    assert_eq!(out, "x:2,y:2");
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

/// Verifies nested trait composition imports constants and binds `__CLASS__` to the consumer.
#[test]
fn test_trait_constants_are_flattened_into_consuming_class() {
    let out = compile_and_run(
        r#"<?php
trait InnerConstants {
    private const VALUES = ['ready'];
    protected const OWNER = __CLASS__;
}
trait OuterConstants { use InnerConstants; }
class ConstantReader {
    use OuterConstants;
    public static function render(): string {
        return self::VALUES[0].':'.self::OWNER;
    }
}
echo ConstantReader::render();
"#,
    );
    assert_eq!(out, "ready:ConstantReader");
}

/// Verifies identical trait and class constant declarations compose without a conflict.
#[test]
fn test_compatible_trait_constant_declarations_compose() {
    let out = compile_and_run(
        r#"<?php
trait FirstConstants { public const VALUE = 9; }
trait SecondConstants { public const VALUE = 9; }
class CompatibleConstants {
    use FirstConstants, SecondConstants;
    public const VALUE = 9;
}
echo CompatibleConstants::VALUE;
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies an incompatible class declaration cannot replace an imported trait constant.
#[test]
fn test_incompatible_trait_constant_declaration_fails() {
    let err = compile_expect_type_error(
        r#"<?php
trait ImportedConstants { public const VALUE = 9; }
class IncompatibleConstants {
    use ImportedConstants;
    public const VALUE = 10;
}
"#,
    );
    assert!(err.contains("incompatible duplicate constant 'VALUE'"), "{err}");
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

/// Verifies an object-valued conditional receiver keeps its checked class for `::CONST`.
#[test]
fn test_dynamic_scoped_constant_uses_lowered_receiver_type() {
    let out = compile_and_run(
        r#"<?php
class ConditionalConstantReceiver {
    public const VALUE = 'ready';
}

$left = new ConditionalConstantReceiver();
$right = new ConditionalConstantReceiver();
echo ($argc > 0 ? $left : $right)::VALUE;
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies a mixed local narrowed by `instanceof` retains its nominal type for `::CONST`.
#[test]
fn test_dynamic_scoped_constant_uses_instanceof_narrowed_local() {
    let out = compile_and_run(
        r#"<?php
class NarrowedConstantReceiver {
    public const VALUE = 'ready';

    public static function read(mixed $value): string {
        if ($value instanceof self) {
            return $value::VALUE;
        }
        return 'other';
    }

    public static function readTernary(mixed $value): string {
        return $value instanceof self ? $value::VALUE : 'other';
    }
}

class NarrowedConstantChild extends NarrowedConstantReceiver {
    public const VALUE = 'child';
}

echo NarrowedConstantReceiver::read(new NarrowedConstantChild()), ':';
echo NarrowedConstantReceiver::readTernary(new NarrowedConstantChild());
"#,
    );
    assert_eq!(out, "child:child");
}

/// Property defaults that are constant EXPRESSIONS or ENUM CASES initialize a `new` object.
///
/// `self::A | self::B` (`symfony/validator`'s `NoSuspiciousCharacters::$checks`), a class
/// constant built from global flags (`monolog`'s `Utils::DEFAULT_JSON_FLAGS`), `~self::A`, and an
/// enum case (`doctrine/dbal`'s `QueryBuilder::$type = QueryType::SELECT`) all failed the backend
/// with "object_new for default value of property". The enum case must also be the very same
/// singleton `QueryType::SELECT` reads. Expected output is php 8.5's.
#[test]
fn test_constant_expression_and_enum_case_property_defaults() {
    let out = compile_and_run(
        r#"<?php
enum QueryType { case SELECT; case DELETE; }
enum Level: int { case Debug = 100; case Info = 200; }
class Utils { const FLAGS = JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE; }
class Q {
    const A = 1; const B = 4;
    private QueryType $type = QueryType::SELECT;
    public int $flags = self::A | self::B | 16;
    protected Level $level = Level::Debug;
    public int $json = Utils::FLAGS;
    public int $neg = ~self::A;
    public function show() { var_dump($this->type === QueryType::SELECT, $this->flags, $this->level->value, $this->json, $this->neg); }
}
$q = new Q; $q->show(); $r = new Q; var_dump($r == $q, QueryType::SELECT === (fn() => $this->type)->call($r));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nint(21)\nint(100)\nint(320)\nint(-2)\nbool(true)\nbool(true)\n"
    );
}

/// A property default with no literal form — an array whose values are enum cases, keyed by
/// interface constants (`symfony/monolog-bridge`'s `ConsoleHandler::$verbosityLevelMap`) — is
/// assigned by the class's property-initializer thunk after allocation, on the static `new`, the
/// dynamic `new $c` and the constructor-less paths alike. The backend used to refuse every body
/// that could allocate such a class, including the generated `ReflectionClass::newLazyGhost`, so
/// the whole build failed. Expected output is php 8.5's.
#[test]
fn test_non_literal_property_default_is_assigned_by_the_init_thunk() {
    let out = compile_and_run(
        r#"<?php
enum Level: int { case Error = 400; case Debug = 100; }
interface Out { const VERBOSITY_QUIET = 16; const VERBOSITY_NORMAL = 32; }
class Handler {
    private array $map = [Out::VERBOSITY_QUIET => Level::Error, Out::VERBOSITY_NORMAL => Level::Debug];
    public $label = "h";
    public function show() { foreach ($this->map as $k => $v) { echo $k, "=", $v->name, ";"; } echo $this->label, "\n"; }
}
(new Handler)->show();
$c = $argc > 0 ? "Handler" : "X"; $h = new $c(); $h->show();
$r = (new ReflectionClass("Handler"))->newInstanceWithoutConstructor(); $r->show();
"#,
    );
    assert_eq!(out, "16=Error;32=Debug;h\n16=Error;32=Debug;h\n16=Error;32=Debug;h\n");
}

/// Verifies `Enum::Case->value` inside an array literal keeps the enum's backing type.
///
/// The element type was guessed syntactically as int, so a string-backed value was stored as
/// its string POINTER: Symfony's `ReflectionExtractor::MAP_TYPES = ['integer' =>
/// TypeIdentifier::INT->value, ...]` printed `4321534512`. Expected output is php 8.5.10's.
#[test]
fn test_enum_case_value_in_an_array_constant_keeps_its_backing_type() {
    let out = compile_and_run(
        r#"<?php
enum Kind: string { case Int = 'int'; case Bool = 'bool'; }
enum Num: int { case One = 1; }
class Map {
    const TYPES = ['integer' => Kind::Int->value, 'one' => Num::One->value];
    const LIST = [Kind::Int->value, Kind::Bool->name];
}
var_dump(Map::TYPES, Map::LIST);
$x = [Kind::Bool->value, 'k' => Num::One->value];
echo $x[0], $x['k'], Map::TYPES['integer'], "\n";
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [\"integer\"]=>\n  string(3) \"int\"\n  [\"one\"]=>\n  int(1)\n}\n\
         array(2) {\n  [0]=>\n  string(3) \"int\"\n  [1]=>\n  string(4) \"Bool\"\n}\nbool1int\n"
    );
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

/// Verifies invalid relative `defined()` receivers raise PHP-compatible catchable `Error`s.
#[test]
fn test_defined_literal_invalid_relative_scope_throws_error() {
    let out = compile_and_run(
        r#"<?php
try {
    echo defined('SELF::X') ? 'bad' : 'bad';
} catch (\Error $e) {
    echo $e->getMessage(), "|";
}
try {
    defined('Parent::X');
} catch (\Error $e) {
    echo $e->getMessage(), "|";
}
try {
    defined('STATIC::X');
} catch (\Error $e) {
    echo $e->getMessage(), "|";
}
class Root {
    public static function probe(): string {
        try {
            defined('PARENT::X');
        } catch (\Error $e) {
            return $e->getMessage();
        }
        return 'bad';
    }
}
echo Root::probe();
"#,
    );
    assert_eq!(
        out,
        "Cannot access \"self\" when no class scope is active|\
Cannot access \"parent\" when no class scope is active|\
Cannot access \"static\" when no class scope is active|\
Cannot access \"parent\" when current class scope has no parent"
    );
}

/// Verifies `defined('static::CONST')` dispatches against the runtime called class.
#[test]
fn test_defined_literal_late_static_class_constants() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public const BASE = 1;
    protected const BASE_PROT = 2;
    private const BASE_PRIV = 3;

    public static function probe(): string {
        return (defined('STATIC::BASE') ? '1' : '0')
            . (defined('static::CHILD') ? '1' : '0')
            . (defined('static::CHILD_PROT') ? '1' : '0')
            . (defined('static::CHILD_PRIV') ? '1' : '0')
            . (defined('static::BASE_PROT') ? '1' : '0')
            . (defined('static::BASE_PRIV') ? '1' : '0');
    }
    public function instanceProbe(): string {
        return defined('static::CHILD') ? '1' : '0';
    }
    public function instanceClosureProbe(): string {
        $probe = static function (): string {
            return defined('static::CHILD') ? '1' : '0';
        };
        return $probe();
    }
    public static function closureProbe(): string {
        $probe = static function (): string {
            return defined('static::CHILD') ? '1' : '0';
        };
        return $probe();
    }
    public static function namedProbe(): string {
        return defined(constant_name: 'static::CHILD') ? '1' : '0';
    }
}
class Child extends Base {
    public const CHILD = 4;
    protected const CHILD_PROT = 5;
    private const CHILD_PRIV = 6;
}
class Grand extends Child {}
echo Base::probe(), "|", Child::probe(), "|", Grand::probe();
echo "|", (new Base())->instanceProbe(), (new Child())->instanceProbe();
echo "|", Base::closureProbe(), Child::closureProbe();
echo "|", (new Base())->instanceClosureProbe(), (new Child())->instanceClosureProbe();
echo "|", Base::namedProbe(), Child::namedProbe();
"#,
    );
    assert_eq!(out, "100011|111010|111010|01|01|01|01");
}

/// Verifies trait constants are exposed through using classes, but not the trait receiver itself.
#[test]
fn test_defined_literal_trait_imported_constants() {
    let out = compile_and_run(
        r#"<?php
trait T {
    public const PUB = 1;
    protected const PROT = 2;
    private const PRIV = 3;
}
trait Outer {
    use T;
    public const OWN = 4;
}
class UsesT {
    use T;
    public static function inside(): string {
        return (defined('self::PUB') ? '1' : '0')
            . (defined('self::PROT') ? '1' : '0')
            . (defined('self::PRIV') ? '1' : '0');
    }
}
class ChildOfUsesT extends UsesT {
    public static function inside(): string {
        return (defined('self::PUB') ? '1' : '0')
            . (defined('self::PROT') ? '1' : '0')
            . (defined('self::PRIV') ? '1' : '0');
    }
}
class UsesOuter {
    use Outer;
}
trait SameLeft {
    public const SAME = 3 + 4;
}
trait SameRight {
    public const SAME = 7;
}
class UsesCompatibleConstants {
    use SameLeft, SameRight;
}
echo defined('T::PUB') ? '1' : '0';
echo defined('UsesT::PUB') ? '1' : '0';
echo defined('UsesT::PROT') ? '1' : '0';
echo defined('UsesT::PRIV') ? '1' : '0';
echo "|", UsesT::inside(), "|", ChildOfUsesT::inside(), "|";
echo defined('UsesOuter::PUB') ? '1' : '0';
echo defined('UsesOuter::OWN') ? '1' : '0';
echo "|", UsesT::PUB, UsesOuter::PUB, UsesOuter::OWN;
echo "|", defined('UsesCompatibleConstants::SAME') ? '1' : '0';
echo UsesCompatibleConstants::SAME;
"#,
    );
    assert_eq!(out, "0100|111|110|11|114|17");
}
