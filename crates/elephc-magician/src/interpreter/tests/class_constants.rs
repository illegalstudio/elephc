//! Purpose:
//! Interpreter tests for eval-declared class constants.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover inherited lookup, scoped receivers, visibility, and dynamic storage.

use super::super::*;
use super::support::*;

/// Verifies class constants can be fetched directly and through scoped receivers.
#[test]
fn execute_program_reads_eval_class_constants() {
    let program = parse_fragment(
        br#"class EvalConstBase {
    public const SEED = 2;
    protected const HIDDEN = 5;
    public static function read() {
        return self::SEED + static::SEED;
    }
    public static function hidden() {
        return self::HIDDEN;
    }
}
class EvalConstChild extends EvalConstBase {
    public const SEED = 7;
    public static function readParent() {
        return parent::SEED;
    }
}
echo EvalConstBase::SEED; echo ":";
echo EvalConstChild::SEED; echo ":";
echo EvalConstChild::read(); echo ":";
echo EvalConstChild::readParent(); echo ":";
return EvalConstChild::hidden();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:7:9:2:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies comma-separated class-like constants are registered and fetchable.
#[test]
fn execute_program_reads_comma_separated_eval_class_like_constants() {
    let program = parse_fragment(
        br#"class EvalMultiConstClass {
    public const A = 1, B = 2;
}
interface EvalMultiConstIface {
    public const C = 3, D = 4;
}
trait EvalMultiConstTrait {
    public const E = 5, F = 6;
}
class EvalMultiConstTraitBox {
    use EvalMultiConstTrait;
}
enum EvalMultiConstEnum {
    public const G = 7, H = 8;
    case Ready;
}
echo EvalMultiConstClass::A; echo EvalMultiConstClass::B; echo ":";
echo EvalMultiConstIface::C; echo EvalMultiConstIface::D; echo ":";
echo EvalMultiConstTraitBox::E; echo EvalMultiConstTraitBox::F; echo ":";
return EvalMultiConstEnum::G + EvalMultiConstEnum::H;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "12:34:56:");
    assert_eq!(values.get(result), FakeValue::Int(15));
}

/// Verifies protected class constant access from global eval scope throws Error.
#[test]
fn execute_program_protected_eval_class_constant_from_global_scope_throws_error() {
    let program = parse_fragment(
        br#"class EvalConstProtected {
    protected const SECRET = 4;
}
try {
    echo EvalConstProtected::SECRET;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Cannot access protected constant EvalConstProtected::SECRET"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies duplicate class constants in one eval class are rejected.
#[test]
fn execute_program_rejects_duplicate_eval_class_constant() {
    let program = parse_fragment(
        br#"class EvalConstDuplicate {
    public const SEED = 1;
    public const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("duplicate class constant should fail");
}

/// Verifies final eval class constants are readable and reject child redeclarations.
#[test]
fn execute_program_rejects_overriding_final_eval_class_constant() {
    let program = parse_fragment(
        br#"class EvalFinalConstBase {
    final public const SEED = 1;
}
class EvalFinalConstChild extends EvalFinalConstBase {
    public const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("overriding final class constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval accepts class constant redeclarations that keep compatible visibility.
#[test]
fn execute_program_accepts_compatible_eval_class_constant_redeclaration() {
    let program = parse_fragment(
        br#"class EvalConstVisibilityBase {
    protected const SEED = 2;
}
class EvalConstVisibilityChild extends EvalConstVisibilityBase {
    public const SEED = 7;
}
interface EvalConstVisibilityIface {
    public const TOKEN = 3;
}
class EvalConstVisibilityImpl implements EvalConstVisibilityIface {
    public const TOKEN = 5;
}
echo EvalConstVisibilityChild::SEED; echo ":";
return EvalConstVisibilityImpl::TOKEN;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies eval rejects inherited class constant redeclarations with reduced visibility.
#[test]
fn execute_program_rejects_reduced_eval_class_constant_visibility() {
    let reduced_parent_visibility = parse_fragment(
        br#"class EvalConstPublicBase {
    public const SEED = 1;
}
class EvalConstProtectedChild extends EvalConstPublicBase {
    protected const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&reduced_parent_visibility, &mut scope, &mut values)
        .expect_err("reduced class constant visibility should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let reduced_protected_parent_visibility = parse_fragment(
        br#"class EvalConstProtectedBase {
    protected const SEED = 1;
}
class EvalConstPrivateChild extends EvalConstProtectedBase {
    private const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&reduced_protected_parent_visibility, &mut scope, &mut values)
        .expect_err("private class constant redeclaration should fail protected parent");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let reduced_interface_visibility = parse_fragment(
        br#"interface EvalConstPublicContract {
    public const SEED = 1;
}
class EvalConstProtectedImpl implements EvalConstPublicContract {
    protected const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&reduced_interface_visibility, &mut scope, &mut values)
        .expect_err("reduced interface constant visibility should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval rejects non-public interface constants like PHP.
#[test]
fn execute_program_rejects_non_public_eval_interface_constants() {
    parse_fragment(
        br#"interface EvalConstProtectedIface {
    protected const SEED = 1;
}"#,
    )
    .expect_err("protected interface constant should fail while parsing");

    parse_fragment(
        br#"interface EvalConstPrivateIface {
    private const SEED = 1;
}"#,
    )
    .expect_err("private interface constant should fail while parsing");
}

/// Verifies private eval constants cannot be declared final.
#[test]
fn execute_program_rejects_final_private_eval_class_constant() {
    let program = parse_fragment(
        br#"class EvalFinalPrivateConst {
    final private const SEED = 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("final private class constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies class-name literals resolve class-like receiver spelling.
#[test]
fn execute_program_reads_eval_class_name_literals() {
    let program = parse_fragment(
        br#"class EvalClassNameBase {
    public static function selfName() { return self::class; }
    public static function staticName() { return static::class; }
}
class EvalClassNameChild extends EvalClassNameBase {}
interface EvalClassNameIface {}
trait EvalClassNameTrait {}
echo EvalClassNameChild::class; echo ":";
echo EvalClassNameIface::class; echo ":";
echo EvalClassNameTrait::class; echo ":";
echo EvalClassNameChild::selfName(); echo ":";
return EvalClassNameChild::staticName();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalClassNameChild:EvalClassNameIface:EvalClassNameTrait:EvalClassNameBase:"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("EvalClassNameChild".to_string())
    );
}

/// Verifies interface constants are readable directly, through inheritance, and through classes.
#[test]
fn execute_program_reads_eval_interface_constants() {
    let program = parse_fragment(
        br#"interface EvalConstParentIface {
    public const BASE = 2;
}
interface EvalConstChildIface extends EvalConstParentIface {
    public const LOCAL = 3;
}
class EvalConstIfaceImpl implements EvalConstChildIface {}
echo EvalConstParentIface::BASE; echo ":";
echo EvalConstChildIface::BASE; echo ":";
echo EvalConstChildIface::LOCAL; echo ":";
return EvalConstIfaceImpl::BASE + EvalConstIfaceImpl::LOCAL;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:2:3:");
    assert_eq!(values.get(result), FakeValue::Int(5));
}

/// Verifies final eval interface constants cannot be redeclared by children or implementors.
#[test]
fn execute_program_rejects_overriding_final_eval_interface_constant() {
    let program = parse_fragment(
        br#"interface EvalFinalConstIface {
    final public const SEED = 1;
}
interface EvalFinalConstChildIface extends EvalFinalConstIface {
    public const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("overriding final interface constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let program = parse_fragment(
        br#"interface EvalFinalImplConstIface {
    final public const SEED = 1;
}
class EvalFinalImplConstBox implements EvalFinalImplConstIface {
    public const SEED = 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("class overriding final interface constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies trait constants are readable directly and from classes using the trait.
#[test]
fn execute_program_reads_eval_trait_constants() {
    let program = parse_fragment(
        br#"trait EvalConstReusableTrait {
    public const SEED = 6;
    public static function readTraitSeed() {
        return self::SEED;
    }
}
class EvalConstTraitBox {
    use EvalConstReusableTrait;
}
echo EvalConstReusableTrait::SEED; echo ":";
echo EvalConstTraitBox::SEED; echo ":";
return EvalConstTraitBox::readTraitSeed();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:6:");
    assert_eq!(values.get(result), FakeValue::Int(6));
}

/// Verifies `self::CONST`, `parent::CONST`, and another class's constant compose inside a
/// class-like constant initializer.
///
/// This is the exact shape that blocked the Symfony `--web` campaign:
/// `symfony/http-foundation/Request.php` keys a private const array with
/// `self::HEADER_X_FORWARDED_FOR => 'for'`, and `self`/`parent` resolution for a constant
/// initializer only ever consulted `current_class_scope()`, which is the METHOD call-frame
/// stack (`push_class_scope`/`pop_class_scope` from method dispatch) -- never populated while
/// a constant/property default is being evaluated during class declaration. `self::A * 3`
/// mirrors the campaign's `cc1.php` reducer (php answers 6).
#[test]
fn execute_program_reads_self_and_parent_reference_in_eval_constant_initializer() {
    let program = parse_fragment(
        br#"class EvalConstSelfBase {
    public const A = 2;
    public const B = self::A * 3;
}
class EvalConstSelfChild extends EvalConstSelfBase {
    public const C = parent::A + 1;
}
class EvalConstOther {
    public const X = 10;
}
class EvalConstCross {
    public const Y = EvalConstOther::X + 1;
}
echo EvalConstSelfBase::B; echo ":";
echo EvalConstSelfChild::C; echo ":";
return EvalConstCross::Y;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:3:");
    assert_eq!(values.get(result), FakeValue::Int(11));
}

/// Verifies `self::class` and `parent::class` resolve to the DECLARING class inside a constant
/// initializer, matching php: the literal is fixed at the point the constant is written, so a
/// subclass reading the inherited constant still sees the base class's name.
#[test]
fn execute_program_reads_self_and_parent_class_name_in_eval_constant_initializer() {
    let program = parse_fragment(
        br#"class EvalConstClassSelfBase {
    public const NAME = self::class;
}
class EvalConstClassSelfChild extends EvalConstClassSelfBase {}
class EvalConstClassParentBase {}
class EvalConstClassParentChild extends EvalConstClassParentBase {
    public const PARENT_NAME = parent::class;
}
echo EvalConstClassSelfBase::NAME; echo ":";
echo EvalConstClassSelfChild::NAME; echo ":";
return EvalConstClassParentChild::PARENT_NAME;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "EvalConstClassSelfBase:EvalConstClassSelfBase:"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("EvalConstClassParentBase".to_string())
    );
}

/// Verifies `self::CONST` resolves inside both instance and static property defaults.
///
/// Property defaults share the same `eval_class_like_member_default` path as constant
/// initializers, so the same missing `push_class_scope` broke both.
#[test]
fn execute_program_reads_self_reference_in_eval_property_defaults() {
    let program = parse_fragment(
        br#"class EvalPropSelfBase {
    public const A = 5;
    public $instanceProp = self::A + 1;
    public static $staticProp = self::A + 2;
}
$o = new EvalPropSelfBase();
echo $o->instanceProp; echo ":";
return EvalPropSelfBase::$staticProp;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:");
    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies `self::CONST` resolves as a backed enum case's value expression.
#[test]
fn execute_program_reads_self_reference_in_eval_enum_case_value() {
    let program = parse_fragment(
        br#"enum EvalEnumSelfRef: int {
    const BASE = 10;
    case A = self::BASE;
    case B = self::BASE + 1;
}
echo EvalEnumSelfRef::A->value; echo ":";
return EvalEnumSelfRef::B->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "10:");
    assert_eq!(values.get(result), FakeValue::Int(11));
}

/// Verifies `static::` is refused inside compile-time-constant positions exactly like php:
/// `"static::" is not allowed in compile-time constants` is a zend COMPILE error, uncatchable,
/// so accepting it silently (treating `static` as `self`, which falling back to
/// `current_class_scope()` would do once self-resolution works) would be a silent wrong-value
/// hole rather than a visible refusal.
#[test]
fn execute_program_rejects_late_static_binding_in_eval_compile_time_constants() {
    let const_case = parse_fragment(
        br#"class EvalConstStaticBase {
    public const A = 4;
    public const B = static::A + 1;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&const_case, &mut scope, &mut values).expect_err(
        "static:: in a constant initializer should fail like php's compile-time refusal",
    );
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let prop_case = parse_fragment(
        br#"class EvalPropStaticBase {
    public const A = 4;
    public $p = static::A + 1;
}
new EvalPropStaticBase();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&prop_case, &mut scope, &mut values)
        .expect_err("static:: in a property default should fail like php's compile-time refusal");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies a self-referencing constant cycle raises a catchable `Error`, matching php's
/// `Cannot declare self-referencing constant self::X`, instead of recursing without limit.
///
/// This shape only became reachable once `self::` resolution started working inside constant
/// initializers: the eager evaluation in `initialize_eval_declared_constants` and the
/// cache-on-miss in `eval_class_like_constant_cell` would otherwise recurse forever between
/// `A`'s and `B`'s uncached cells.
///
/// php evaluates class constants LAZILY on first read, so its own version of this reducer
/// (`p_cyclic_catch.php` in the campaign scratchpad) wraps the *usage* in try/catch. Elephc's
/// `initialize_eval_declared_constants` evaluates every constant EAGERLY at the class-declaration
/// statement instead (a pre-existing divergence, unrelated to this fix, documented below rather
/// than fixed here), so the throw surfaces at the `class` statement -- this test wraps the
/// declaration to catch it where it actually happens.
#[test]
fn execute_program_rejects_cyclic_eval_class_constant_initializers_as_catchable_error() {
    let program = parse_fragment(
        br#"try {
    class EvalConstCyclic {
        public const A = self::B;
        public const B = self::A;
    }
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . (str_contains($e->getMessage(), "self-referencing constant") ? "1" : "0");
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Error:1");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies compatible same-name trait constants are deduplicated during composition.
#[test]
fn execute_program_allows_compatible_eval_trait_constant_conflicts() {
    let program = parse_fragment(
        br#"trait EvalConstCompatibleA {
    public const SEED = 6;
}
trait EvalConstCompatibleB {
    public const SEED = 6;
}
class EvalConstCompatibleTraitBox {
    use EvalConstCompatibleA, EvalConstCompatibleB;
}
class EvalConstCompatibleClassBox {
    use EvalConstCompatibleA;
    public const SEED = 6;
}
echo EvalConstCompatibleTraitBox::SEED; echo ":";
return EvalConstCompatibleClassBox::SEED;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "6:");
    assert_eq!(values.get(result), FakeValue::Int(6));
}

/// Verifies incompatible same-name class and trait constants fail like PHP.
#[test]
fn execute_program_rejects_incompatible_eval_trait_constant_conflicts() {
    let class_conflict = parse_fragment(
        br#"trait EvalConstClassTraitConflict {
    public const SEED = 6;
}
class EvalConstClassTraitConflictBox {
    use EvalConstClassTraitConflict;
    public const SEED = 7;
}"#,
    )
    .expect("parse class/trait constant conflict");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&class_conflict, &mut scope, &mut values)
        .expect_err("incompatible class/trait constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let trait_conflict = parse_fragment(
        br#"trait EvalConstTraitConflictA {
    public const SEED = 6;
}
trait EvalConstTraitConflictB {
    public const SEED = 7;
}
class EvalConstTraitConflictBox {
    use EvalConstTraitConflictA, EvalConstTraitConflictB;
}"#,
    )
    .expect("parse trait/trait constant conflict");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&trait_conflict, &mut scope, &mut values)
        .expect_err("incompatible trait/trait constant should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
