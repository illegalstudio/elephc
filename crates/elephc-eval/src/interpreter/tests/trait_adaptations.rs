//! Purpose:
//! Interpreter tests for eval-declared trait adaptations.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
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
