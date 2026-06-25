//! Purpose:
//! Interpreter tests for eval-declared trait adaptations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover conflict resolution, aliases, and visibility changes
//!   applied while expanding traits into eval class metadata.

use super::super::*;
use super::support::*;

/// Verifies `insteadof` keeps the selected method while `as` can alias the other method.
#[test]
fn execute_program_applies_eval_trait_insteadof_and_alias() {
    let program = parse_fragment(
        br#"trait EvalAdaptA {
    public function talk() { return "A"; }
    public function hidden() { return "secret"; }
}
trait EvalAdaptB {
    public function talk() { return "B"; }
}
class EvalAdaptBox {
    use EvalAdaptA, EvalAdaptB {
        EvalAdaptA::talk insteadof EvalAdaptB;
        EvalAdaptB::talk as talkB;
        EvalAdaptA::hidden as private;
    }
    public function read() {
        return $this->talk() . ":" . $this->talkB() . ":" . $this->hidden();
    }
}
$box = new EvalAdaptBox();
echo $box->read(); echo ":";
return $box->talk();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A:B:secret:");
    assert_eq!(values.get(result), FakeValue::String("A".to_string()));
}

/// Verifies visibility-only `as private` hides the imported method from global calls.
#[test]
fn execute_program_applies_eval_trait_visibility_adaptation() {
    let program = parse_fragment(
        br#"trait EvalAdaptHidden {
    public function hidden() { return "secret"; }
}
class EvalAdaptHiddenBox {
    use EvalAdaptHidden {
        EvalAdaptHidden::hidden as private;
    }
}
$box = new EvalAdaptHiddenBox();
return $box->hidden();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("private adapted trait method should fail from global scope");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies trait aliases that collide with class methods or no-op names follow PHP rules.
#[test]
fn execute_program_applies_eval_trait_alias_collision_rules() {
    let program = parse_fragment(
        br#"trait EvalAliasSource {
    public function source() { return "T"; }
}
class EvalAliasClassCollisionBox {
    use EvalAliasSource {
        source as target;
    }
    public function target() { return "C"; }
    public function read() { return $this->source() . $this->target(); }
}
class EvalAliasNoopBox {
    use EvalAliasSource {
        source as source;
    }
}
$box = new EvalAliasClassCollisionBox();
echo $box->read(); echo ":";
return (new EvalAliasNoopBox())->source();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "TC:");
    assert_eq!(values.get(result), FakeValue::String("T".to_string()));
}

/// Verifies same-name trait aliases that change visibility remain composition fatals.
#[test]
fn execute_program_rejects_eval_trait_same_name_visibility_alias() {
    let program = parse_fragment(
        br#"trait EvalAliasVisibilitySource {
    public function source() { return "T"; }
}
class EvalAliasVisibilityBox {
    use EvalAliasVisibilitySource {
        source as private source;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("same-name trait alias with visibility change should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies trait adaptations reject missing trait or method targets.
#[test]
fn execute_program_rejects_invalid_eval_trait_adaptation_targets() {
    let missing_method = parse_fragment(
        br#"trait EvalAdaptMissingMethod {
    public function source() { return "T"; }
}
class EvalAdaptMissingMethodBox {
    use EvalAdaptMissingMethod {
        EvalAdaptMissingMethod::missing insteadof EvalAdaptMissingMethod;
    }
}"#,
    )
    .expect("parse missing method adaptation");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&missing_method, &mut scope, &mut values)
        .expect_err("missing adaptation method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let missing_trait = parse_fragment(
        br#"trait EvalAdaptMissingTrait {
    public function source() { return "T"; }
}
class EvalAdaptMissingTraitBox {
    use EvalAdaptMissingTrait {
        EvalAdaptMissingTrait::source insteadof EvalAdaptNotUsedTrait;
    }
}"#,
    )
    .expect("parse missing trait adaptation");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&missing_trait, &mut scope, &mut values)
        .expect_err("missing adaptation trait should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let missing_unqualified_alias = parse_fragment(
        br#"trait EvalAdaptMissingAlias {
    public function source() { return "T"; }
}
class EvalAdaptMissingAliasBox {
    use EvalAdaptMissingAlias {
        missing as alias;
    }
}"#,
    )
    .expect("parse missing unqualified alias");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&missing_unqualified_alias, &mut scope, &mut values)
        .expect_err("missing unqualified alias method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies unresolved same-name trait methods remain a declaration-time fatal.
#[test]
fn execute_program_rejects_unresolved_eval_trait_method_conflict() {
    let program = parse_fragment(
        br#"trait EvalConflictA {
    public function talk() { return "A"; }
}
trait EvalConflictB {
    public function talk() { return "B"; }
}
class EvalConflictBox {
    use EvalConflictA, EvalConflictB;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unresolved trait method conflict should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies compatible same-name trait properties are deduplicated during composition.
#[test]
fn execute_program_allows_compatible_eval_trait_property_conflicts() {
    let program = parse_fragment(
        br#"trait EvalCompatibleTraitPropA {
    public int $value;
}
trait EvalCompatibleTraitPropB {
    public int $value;
}
class EvalCompatibleTraitPropBox {
    use EvalCompatibleTraitPropA, EvalCompatibleTraitPropB;
    public int $value;
    public function __construct($value) { $this->value = $value; }
}
$box = new EvalCompatibleTraitPropBox(7);
return $box->value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies incompatible same-name class and trait properties fail like PHP.
#[test]
fn execute_program_rejects_incompatible_eval_trait_property_conflicts() {
    let class_conflict = parse_fragment(
        br#"trait EvalClassTraitPropConflict {
    public int $value;
}
class EvalClassTraitPropConflictBox {
    use EvalClassTraitPropConflict;
    public string $value;
}"#,
    )
    .expect("parse class/trait property conflict");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&class_conflict, &mut scope, &mut values)
        .expect_err("incompatible class/trait property should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);

    let trait_conflict = parse_fragment(
        br#"trait EvalTraitPropConflictA {
    public int $value;
}
trait EvalTraitPropConflictB {
    public string $value;
}
class EvalTraitPropConflictBox {
    use EvalTraitPropConflictA, EvalTraitPropConflictB;
}"#,
    )
    .expect("parse trait/trait property conflict");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&trait_conflict, &mut scope, &mut values)
        .expect_err("incompatible trait/trait property should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
