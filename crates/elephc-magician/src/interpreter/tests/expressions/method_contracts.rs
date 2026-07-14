//! Purpose:
//! Interpreter tests for method override visibility, arity, variance, and return
//! type enforcement.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Both declaration compatibility and runtime return values are validated.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval rejects overriding a public method with lower visibility.
#[test]
fn execute_program_rejects_method_override_with_reduced_visibility() {
    let program = parse_fragment(
        br#"class EvalVisibleBase {
    public function read() { return 1; }
}
class EvalVisibleChild extends EvalVisibleBase {
    protected function read() { return 2; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("reduced method visibility should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval rejects parent method overrides that require more arguments.
#[test]
fn execute_program_rejects_method_override_with_narrower_arity() {
    let program = parse_fragment(
        br#"class EvalArityBase {
    public function read($value = "base") { return $value; }
}
class EvalArityChild extends EvalArityBase {
    public function read($value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("narrower method override arity should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval accepts PHP-contravariant method parameter type overrides.
#[test]
fn execute_program_accepts_contravariant_method_parameter_type_overrides() {
    let program = parse_fragment(
        br#"class EvalParamBase {
    public function anyInt(int $value) { return $value; }
    public function maybeInt(int $value) { return $value; }
    public function untypedInt(int $value) { return $value; }
}
class EvalParamChild extends EvalParamBase {
    public function anyInt(mixed $value) { return $value . ":mixed"; }
    public function maybeInt(?int $value) { return $value; }
    public function untypedInt($value) { return $value; }
}
$child = new EvalParamChild();
echo $child->anyInt(7); echo ":";
echo $child->untypedInt("ok");
return $child->maybeInt(null) === null;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:mixed:ok");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval rejects method parameter overrides that narrow PHP's accepted type set.
#[test]
fn execute_program_rejects_incompatible_method_parameter_type_overrides() {
    let incompatible_type = parse_fragment(
        br#"class EvalParamTypeBase {
    public function read(int $value) { return $value; }
}
class EvalParamStringChild extends EvalParamTypeBase {
    public function read(string $value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&incompatible_type, &mut scope, &mut values)
        .expect_err("incompatible parameter override type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let narrower_nullable = parse_fragment(
        br#"class EvalParamNullableBase {
    public function maybe(?int $value) { return $value; }
}
class EvalParamNonNullChild extends EvalParamNullableBase {
    public function maybe(int $value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&narrower_nullable, &mut scope, &mut values)
        .expect_err("narrower nullable parameter override type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let untyped_to_typed = parse_fragment(
        br#"class EvalParamUntypedBase {
    public function read($value) { return $value; }
}
class EvalParamTypedChild extends EvalParamUntypedBase {
    public function read(int $value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&untyped_to_typed, &mut scope, &mut values)
        .expect_err("typed parameter override of untyped parent should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval accepts covariant method return type overrides.
#[test]
fn execute_program_accepts_covariant_method_return_type_overrides() {
    let program = parse_fragment(
        br#"class EvalReturnBase {
    public function id(): ?int { return 1; }
    public function make(): EvalReturnBase { return $this; }
    public function selfType(): self { return $this; }
}
class EvalReturnChild extends EvalReturnBase {
    public function id(): int { return 2; }
    public function make(): EvalReturnChild { return $this; }
    public function selfType(): static { return $this; }
}
class EvalReturnParentRoot {}
class EvalReturnParentBase extends EvalReturnParentRoot {
    public function parentKeyword(): EvalReturnParentRoot { return new EvalReturnParentRoot(); }
}
class EvalReturnParentChild extends EvalReturnParentBase {
    public function parentKeyword(): parent { return new EvalReturnParentBase(); }
}
class EvalReturnMixedBase {
    public function maybe(): mixed { return null; }
}
class EvalReturnMixedChild extends EvalReturnMixedBase {
    public function maybe(): ?int { return null; }
}
$child = new EvalReturnChild();
return $child->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
}

/// Verifies eval rejects method overrides that widen declared return types.
#[test]
fn execute_program_rejects_incompatible_method_return_type_overrides() {
    let wider_nullable = parse_fragment(
        br#"class EvalReturnNarrowBase {
    public function id(): int { return 1; }
}
class EvalReturnWiderNullable extends EvalReturnNarrowBase {
    public function id(): ?int { return 2; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&wider_nullable, &mut scope, &mut values)
        .expect_err("wider nullable return type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let missing_return = parse_fragment(
        br#"class EvalReturnRequiredBase {
    public function label(): string { return "base"; }
}
class EvalReturnMissingChild extends EvalReturnRequiredBase {
    public function label() { return "child"; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&missing_return, &mut scope, &mut values)
        .expect_err("missing return type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let static_to_self = parse_fragment(
        br#"class EvalReturnStaticBase {
    public function make(): static { return $this; }
}
class EvalReturnSelfChild extends EvalReturnStaticBase {
    public function make(): self { return $this; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&static_to_self, &mut scope, &mut values)
        .expect_err("static return type should not widen to self");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let nullable_to_mixed = parse_fragment(
        br#"class EvalReturnNullableBase {
    public function maybe(): ?int { return null; }
}
class EvalReturnMixedChildBad extends EvalReturnNullableBase {
    public function maybe(): mixed { return null; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&nullable_to_mixed, &mut scope, &mut values)
        .expect_err("mixed return type should widen nullable int");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval enforces declared method return values at runtime.
#[test]
fn execute_program_enforces_eval_method_return_type_values() {
    let program = parse_fragment(
        br#"class EvalReturnRuntimeBase {
    public function id(): int { return "12"; }
    public function makeSelf(): self { return new EvalReturnRuntimeBase(); }
    public function done(): void { return; }
}
class EvalReturnRuntimeChild extends EvalReturnRuntimeBase {}
$child = new EvalReturnRuntimeChild();
echo $child->id(); echo ":";
echo get_class($child->makeSelf()); echo ":";
$child->done();
return 3;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "12:EvalReturnRuntimeBase:");
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies eval rejects method return values that do not satisfy declarations.
#[test]
fn execute_program_rejects_invalid_eval_method_return_type_values() {
    let bad_scalar = parse_fragment(
        br#"class EvalReturnBadScalar {
    public function id(): int { return "nope"; }
}
$box = new EvalReturnBadScalar();
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_scalar, &mut scope, &mut values)
        .expect_err("non-numeric string should fail int return type");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_void = parse_fragment(
        br#"class EvalReturnBadVoid {
    public function done(): void { return null; }
}
$box = new EvalReturnBadVoid();
return $box->done();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_void, &mut scope, &mut values)
        .expect_err("explicit value should fail void return type");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_static = parse_fragment(
        br#"class EvalReturnStaticRuntimeBase {
    public function make(): static { return new EvalReturnStaticRuntimeBase(); }
}
class EvalReturnStaticRuntimeChild extends EvalReturnStaticRuntimeBase {}
$child = new EvalReturnStaticRuntimeChild();
return $child->make();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_static, &mut scope, &mut values)
        .expect_err("base instance should fail inherited static return type");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let implicit_return = parse_fragment(
        br#"class EvalReturnImplicitBad {
    public function id(): ?int {}
}
$box = new EvalReturnImplicitBad();
return $box->id();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&implicit_return, &mut scope, &mut values)
        .expect_err("implicit return should fail non-void return type");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}
