//! Purpose:
//! Interpreter tests for scalar expressions, echo/print, object calls, and
//! runtime constructor argument handling.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases assert EvalIR expression execution against fake runtime values.

use super::super::super::*;
use super::super::support::*;

/// Verifies simple variable compound assignments read, compute, and write the scope value.
#[test]
fn execute_program_evaluates_compound_assignments() {
    let program =
        parse_fragment(br#"$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "v15");
    assert_eq!(values.get(x), FakeValue::Int(15));
}
/// Verifies division and modulo evaluate through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_division_and_modulo() {
    let program = parse_fragment(br#"$x = 20; $x /= 2; $x %= 6; echo $x; return 9 / 2;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "4");
    assert_eq!(values.get(x), FakeValue::Int(4));
    assert_eq!(values.get(result), FakeValue::Float(4.5));
}
/// Verifies exponentiation evaluates through fake runtime numeric hooks.
#[test]
fn execute_program_evaluates_exponentiation() {
    let program = parse_fragment(
        br#"$x = 2; $x **= 3; echo $x; echo ":"; echo -2 ** 2; return 2 ** 3 ** 2;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.output, "8:-4");
    assert_eq!(values.get(x), FakeValue::Float(8.0));
    assert_eq!(values.get(result), FakeValue::Float(512.0));
}
/// Verifies bitwise and shift operators evaluate through fake runtime hooks.
#[test]
fn execute_program_evaluates_bitwise_and_shift_ops() {
    let program = parse_fragment(
        br#"$x = 6; $x &= 3; echo $x; echo ":";
$x = 4; $x |= 1; echo $x; echo ":";
$x = 7; $x ^= 3; echo $x; echo ":";
$x = 1; $x <<= 5; echo $x; echo ":";
$x = 64; $x >>= 3; echo $x; echo ":";
echo ~0; echo ":"; echo -16 >> 2;
return (1 << 4) | ((16 >> 2) ^ (3 & 1));"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "2:5:4:32:8:-1:-4");
    assert_eq!(values.get(result), FakeValue::Int(21));
}
/// Verifies simple variable increment and decrement statements update the scope value.
#[test]
fn execute_program_evaluates_inc_dec_statements() {
    let program = parse_fragment(br#"$i = 1; $i++; ++$i; $i--; --$i; echo $i;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let i = scope.visible_cell("i").expect("scope should contain i");

    assert_eq!(values.output, "1");
    assert_eq!(values.get(i), FakeValue::Int(1));
}
/// Verifies echo and unset operate through runtime hooks and scope metadata.
#[test]
fn execute_program_echoes_and_unsets_scope_value() {
    let program =
        parse_fragment(br#"echo "hi" . $name; unset($name);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let name = values.string(" Ada").expect("create fake string");
    scope.set("name", name, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "hi Ada");
    assert_eq!(values.get(result), FakeValue::Null);
    assert!(scope.entry("name").expect("unset marker").flags().unset);
}
/// Verifies comma-separated echo expressions are executed in source order.
#[test]
fn execute_program_echoes_comma_list() {
    let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let b = values.string("b").expect("create fake string");
    scope.set("b", b, ScopeCellOwnership::Owned);

    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "abc");
}
/// Verifies print writes output and returns integer 1.
#[test]
fn execute_program_print_returns_one() {
    let program = parse_fragment(br#"return print "p";"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "p");
    assert_eq!(values.get(result), FakeValue::Int(1));
}
/// Verifies eval property reads and writes dispatch through runtime hooks.
#[test]
fn execute_program_reads_and_writes_object_property() {
    let program = parse_fragment(br#"$this->x = $this->x + 1; return $this->x;"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(1).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(
        values
            .property_get(object, "x")
            .map(|value| values.get(value))
            .expect("property should be readable"),
        FakeValue::Int(2)
    );
}
/// Verifies eval method calls dispatch through the runtime method hook.
#[test]
fn execute_program_calls_object_method() {
    let program = parse_fragment(br#"return $this->answer();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(42));
}
/// Verifies eval method calls forward evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_argument() {
    let program = parse_fragment(br#"return $this->add_x(5);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies eval method calls forward multiple evaluated arguments to the runtime hook.
#[test]
fn execute_program_calls_object_method_with_two_arguments() {
    let program = parse_fragment(br#"return $this->add2_x(5, 6);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}
/// Verifies eval method calls forward numerically unpacked arguments.
#[test]
fn execute_program_calls_object_method_with_spread_arguments() {
    let program =
        parse_fragment(br#"return $this->add2_x(...[5, 6]);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(7).expect("create fake int");
    let properties = vec![("x".to_string(), x)];
    let object = values.alloc(FakeValue::Object(properties));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(18));
}
/// Verifies eval object construction dispatches through runtime hooks.
#[test]
fn execute_program_constructs_named_object() {
    let program = parse_fragment(br#"return new Box();"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Object(Vec::new()));
}
/// Verifies eval object construction passes constructor arguments through runtime hooks.
#[test]
fn execute_program_constructs_named_object_with_args() {
    let program = parse_fragment(br#"return new Box(1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let FakeValue::Object(properties) = values.get(result) else {
        panic!("expected fake object");
    };
    let x = FakeOps::object_property(&properties, "x").expect("constructor should set x");

    assert_eq!(values.get(x), FakeValue::Int(1));
}

/// Verifies eval object construction binds registered AOT constructor named arguments.
#[test]
fn execute_program_constructs_named_object_with_registered_named_args() {
    let program = parse_fragment(br#"$box = new KnownClass(value: 9); return $box->read_x();"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "value"));
    assert!(context.define_native_constructor_signature("KnownClass", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered constructor named args should bind");

    assert_eq!(values.get(result), FakeValue::Int(9));
}

/// Verifies runtime/AOT constructor fallback honors by-reference parameter metadata.
#[test]
fn execute_program_rejects_runtime_constructor_by_ref_temporary_arg() {
    let program = parse_fragment(br#"$box = new KnownClass(9); return $box->read_x();"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "value"));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_constructor_signature("KnownClass", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a constructor by-reference parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies runtime/AOT constructor fallback writes coerced by-reference args back.
#[test]
fn execute_program_writes_back_runtime_constructor_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"$value = "9";
$box = new KnownClass($value);
echo $box->read_x();
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "value"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_constructor_signature("KnownClass", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered constructor by-ref coercion should bind");

    assert_eq!(values.output, "9");
    assert_eq!(values.get(result), FakeValue::Int(9));
}

/// Verifies AOT constructor by-reference writeback still runs when construction fatals.
#[test]
fn execute_program_writes_back_runtime_constructor_by_ref_before_fatal() {
    let program = parse_fragment(
        br#"$value = "9";
new KnownFailingConstructor($value);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(1);
    assert!(signature.set_param_name(0, "value"));
    assert!(signature.set_param_type(
        0,
        EvalParameterType::new(vec![EvalParameterTypeVariant::Int], false)
    ));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_constructor_signature("KnownFailingConstructor", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("failing constructor should abort after argument binding");
    let value = scope
        .entry("value")
        .expect("caller variable should remain visible")
        .cell();

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.get(value), FakeValue::Int(9));
}
