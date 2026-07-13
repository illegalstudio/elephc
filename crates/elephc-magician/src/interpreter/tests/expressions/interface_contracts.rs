//! Purpose:
//! Interpreter tests for eval interface method presence, variance, staticness,
//! by-reference parameters, and variadics.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Concrete and deferred abstract implementations are checked independently.

use super::super::super::*;
use super::super::support::*;

/// Verifies eval rejects classes missing methods required by eval interfaces.
#[test]
fn execute_program_rejects_missing_dynamic_interface_method() {
    let program = parse_fragment(
        br#"interface EvalNeedsRead {
    function read($n);
}
class EvalMissingRead implements EvalNeedsRead {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("missing interface method should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval accepts covariant return types for interface method contracts.
#[test]
fn execute_program_accepts_covariant_interface_method_return_type() {
    let program = parse_fragment(
        br#"interface EvalReturnReadable {
    function read(): int|string;
}
class EvalReturnReader implements EvalReturnReadable {
    public function read(): int {
        return 7;
    }
}
interface EvalReturnRootSelf {
    function linked(): self;
}
interface EvalReturnChildSelf extends EvalReturnRootSelf {}
class EvalReturnSelfImpl implements EvalReturnChildSelf {
    public function linked(): EvalReturnRootSelf {
        return $this;
    }
}
$reader = new EvalReturnReader();
return $reader->read();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval rejects missing or wider return types for interface method contracts.
#[test]
fn execute_program_rejects_incompatible_interface_method_return_type() {
    let missing_return = parse_fragment(
        br#"interface EvalNeedsReturn {
    function read(): string;
}
class EvalMissingReturnImpl implements EvalNeedsReturn {
    public function read() { return "bad"; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&missing_return, &mut scope, &mut values)
        .expect_err("missing interface return type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let wider_return = parse_fragment(
        br#"interface EvalNeedsStringReturn {
    function read(): string;
}
class EvalWiderReturnImpl implements EvalNeedsStringReturn {
    public function read(): int|string { return "bad"; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&wider_return, &mut scope, &mut values)
        .expect_err("wider interface return type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract eval classes must keep declared interface method signatures compatible.
#[test]
fn execute_program_rejects_incompatible_abstract_interface_method_declarations() {
    let bad_abstract_param = parse_fragment(
        br#"interface EvalAbstractIfaceParam {
    function read(int $value);
}
abstract class EvalAbstractIfaceParamBase implements EvalAbstractIfaceParam {
    abstract public function read(string $value);
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_param, &mut scope, &mut values)
        .expect_err("abstract interface method parameter type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_abstract_return = parse_fragment(
        br#"interface EvalAbstractIfaceReturn {
    function read(): int;
}
abstract class EvalAbstractIfaceReturnBase implements EvalAbstractIfaceReturn {
    abstract public function read(): string;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_abstract_return, &mut scope, &mut values)
        .expect_err("abstract interface method return type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_inherited_method = parse_fragment(
        br#"interface EvalInheritedIfaceMethod {
    function read(int $value);
}
abstract class EvalInheritedIfaceMethodBase {
    public function read(string $value) {}
}
abstract class EvalInheritedIfaceMethodChild extends EvalInheritedIfaceMethodBase implements EvalInheritedIfaceMethod {}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_inherited_method, &mut scope, &mut values)
        .expect_err("inherited incompatible interface method should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies abstract eval classes may defer missing compatible interface methods.
#[test]
fn execute_program_accepts_deferred_abstract_interface_method_declarations() {
    let program = parse_fragment(
        br#"interface EvalAbstractIfaceDeferred {
    function read(int $value): int;
}
abstract class EvalAbstractIfaceDeferredBase implements EvalAbstractIfaceDeferred {}
abstract class EvalAbstractIfaceDeferredTyped implements EvalAbstractIfaceDeferred {
    abstract public function read(mixed $value): int;
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval accepts PHP-contravariant parameter types for interface contracts.
#[test]
fn execute_program_accepts_contravariant_interface_method_parameter_types() {
    let program = parse_fragment(
        br#"interface EvalParamContract {
    function read(int $value);
}
class EvalParamContractReader implements EvalParamContract {
    public function read(mixed $value) {
        return $value . ":ok";
    }
}
$reader = new EvalParamContractReader();
return $reader->read(8);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("8:ok".to_string()));
}

/// Verifies eval rejects interface implementations with incompatible parameter types.
#[test]
fn execute_program_rejects_incompatible_interface_method_parameter_types() {
    let incompatible_type = parse_fragment(
        br#"interface EvalParamStringContract {
    function read(int $value);
}
class EvalParamStringReader implements EvalParamStringContract {
    public function read(string $value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&incompatible_type, &mut scope, &mut values)
        .expect_err("incompatible interface parameter type should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let untyped_to_typed = parse_fragment(
        br#"interface EvalParamUntypedContract {
    function read($value);
}
class EvalParamTypedReader implements EvalParamUntypedContract {
    public function read(int $value) { return $value; }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&untyped_to_typed, &mut scope, &mut values)
        .expect_err("typed parameter implementation of untyped contract should fail");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval static interface method contracts are satisfied by public static methods.
#[test]
fn execute_program_accepts_static_dynamic_interface_method() {
    let program = parse_fragment(
        br#"interface EvalNeedsStaticRead {
    public static function read($n);
}
class EvalStaticReader implements EvalNeedsStaticRead {
    public static function read($n) {
        return $n . "!";
    }
}
return EvalStaticReader::read("ok");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("ok!".to_string()));
}

/// Verifies eval rejects instance methods for static interface method contracts.
#[test]
fn execute_program_rejects_instance_method_for_static_dynamic_interface_method() {
    let program = parse_fragment(
        br#"interface EvalNeedsStaticRead {
    public static function read();
}
class EvalInstanceReader implements EvalNeedsStaticRead {
    public function read() {}
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("instance method should not satisfy static interface method");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval interface method contracts require matching by-reference parameters.
#[test]
fn execute_program_validates_interface_method_by_ref_parameters() {
    let program = parse_fragment(
        br#"interface EvalRefReadable {
    function read(&$value);
}
class EvalRefReader implements EvalRefReadable {
    public function read(&$value) {
        $value = "ok";
    }
}
$value = "bad";
$reader = new EvalRefReader();
$reader->read($value);
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("ok".to_string()));

    let bad_value_impl = parse_fragment(
        br#"interface EvalNeedsByRef {
    function read(&$value);
}
class EvalByValueReader implements EvalNeedsByRef {
    public function read($value) {}
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_value_impl, &mut scope, &mut values)
        .expect_err("by-value implementation must not satisfy by-reference contract");
    assert_eq!(err, EvalStatus::RuntimeFatal);

    let bad_ref_impl = parse_fragment(
        br#"interface EvalNeedsByValue {
    function read($value);
}
class EvalByRefReader implements EvalNeedsByValue {
    public function read(&$value) {}
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&bad_ref_impl, &mut scope, &mut values)
        .expect_err("by-reference implementation must not satisfy by-value contract");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies variadic eval methods can satisfy fixed-arity interface contracts.
#[test]
fn execute_program_accepts_variadic_method_for_fixed_interface_contract() {
    let program = parse_fragment(
        br#"interface EvalFixedReadable {
    function read($left, $right);
}
class EvalVariadicReadable implements EvalFixedReadable {
    public function read($left, ...$tail) {
        return $left . $tail[0];
    }
}
$box = new EvalVariadicReadable();
return $box->read("A", "B");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("AB".to_string()));
}

/// Verifies non-variadic eval methods cannot satisfy variadic interface contracts.
#[test]
fn execute_program_rejects_non_variadic_method_for_variadic_interface_contract() {
    let program = parse_fragment(
        br#"interface EvalVariadicReadable {
    function read($left, ...$tail);
}
class EvalFixedReadable implements EvalVariadicReadable {
    public function read($left, $tail = null) {
        return $left;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("non-variadic implementation should not satisfy variadic contract");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
