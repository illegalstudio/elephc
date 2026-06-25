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

/// Verifies protected class constants are not readable from global eval scope.
#[test]
fn execute_program_rejects_protected_eval_class_constant_from_global_scope() {
    let program = parse_fragment(
        br#"class EvalConstProtected {
    protected const SECRET = 4;
}
return EvalConstProtected::SECRET;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values)
        .expect_err("global protected class constant access should fail");
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
