//! Purpose:
//! Interpreter tests for eval-declared method and constructor argument binding.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover named arguments and named unpacking on instance methods,
//!   static methods, and constructors declared inside eval fragments.

use super::super::*;
use super::support::*;

/// Verifies eval-declared instance, static, and constructor methods bind named args.
#[test]
fn execute_program_binds_eval_method_named_args() {
    let program = parse_fragment(
        br#"class EvalNamedMethodBox {
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function read($left, $right) {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left, $right) {
        return $left . "-" . $right;
    }
}
$box = new EvalNamedMethodBox(right: "B", left: "A");
echo $box->read(right: "D", left: "C"); echo ":";
$args = ["right" => "F", "left" => "E"];
echo $box->read(...$args); echo ":";
return EvalNamedMethodBox::join(right: "H", left: "G");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:C:D:AB:E:F:");
    assert_eq!(values.get(result), FakeValue::String("G-H".to_string()));
}

/// Verifies eval-declared methods use default values for omitted arguments.
#[test]
fn execute_program_binds_eval_method_default_args() {
    let program = parse_fragment(
        br#"class EvalDefaultMethodBox {
    public function __construct($left = "A", $right = "B") {
        $this->label = $left . $right;
    }
    public function read($left, $right = "D") {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left = "G", $right = "H") {
        return $left . "-" . $right;
    }
}
$box = new EvalDefaultMethodBox();
echo $box->read("C"); echo ":";
echo $box->read(right: "F", left: "E"); echo ":";
return EvalDefaultMethodBox::join();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "AB:C:D:AB:E:F:");
    assert_eq!(values.get(result), FakeValue::String("G-H".to_string()));
}

/// Verifies eval-declared methods materialize constant-expression parameter defaults.
#[test]
fn execute_program_binds_eval_method_constant_default_args() {
    let program = parse_fragment(
        br#"define("EVAL_METHOD_DEFAULT_GLOBAL", "G");
class EvalDefaultConstBase {
    const LABEL = "base";
}
interface EvalDefaultConstIface {
    const WORD = "iface";
}
class EvalDefaultConstDep {
    public function __construct($label = "dep") {
        $this->label = $label;
    }
    public function read() {
        return $this->label;
    }
}
class EvalDefaultConstBox extends EvalDefaultConstBase {
    const LABEL = "box";
    public function __construct($label = self::LABEL) {
        $this->label = $label;
    }
    public function read($global = EVAL_METHOD_DEFAULT_GLOBAL, $parent = parent::LABEL, $iface = EvalDefaultConstIface::WORD, $class = self::class, $parentClass = parent::class, $items = [self::LABEL => 1 + 2, "fallback" => null ?? "fallback"], $method = __METHOD__, $dep = new EvalDefaultConstDep(label: "dep"), $clone = new self("inner")) {
        return $this->label . ":" . $global . ":" . $parent . ":" . $iface . ":" . $class . ":" . $parentClass . ":" . $items[self::LABEL] . ":" . $items["fallback"] . ":" . $method . ":" . $dep->read() . ":" . $clone->label;
    }
    public static function join($label = self::LABEL, $parent = parent::LABEL) {
        return $label . "-" . $parent;
    }
}
$box = new EvalDefaultConstBox();
echo $box->read(); echo ":";
return EvalDefaultConstBox::join();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "box:G:base:iface:EvalDefaultConstBox:EvalDefaultConstBase:3:fallback:EvalDefaultConstBox::read:dep:inner:"
    );
    assert_eq!(values.get(result), FakeValue::String("box-base".to_string()));
}

/// Verifies eval-declared methods bind positional and named values into variadic arrays.
#[test]
fn execute_program_binds_eval_method_variadic_args() {
    let program = parse_fragment(
        br#"class EvalVariadicMethodBox {
    public function __construct(...$parts) {
        $this->label = $parts[0] . $parts["right"];
    }
    public function read($head, ...$tail) {
        echo count($tail); echo ":";
        return $this->label . ":" . $head . ":" . $tail[0] . ":" . $tail["named"] . ":" . $tail["tail"];
    }
    public static function join(...$items) {
        return $items[0] . $items[1];
    }
}
$box = new EvalVariadicMethodBox("A", right: "B");
echo $box->read("C", "D", named: "E", tail: "F"); echo ":";
return EvalVariadicMethodBox::join("G", "H");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:AB:C:D:E:F:");
    assert_eq!(values.get(result), FakeValue::String("GH".to_string()));
}

/// Verifies eval-declared instance, static, and constructor methods write back by-reference args.
#[test]
fn execute_program_writes_back_eval_method_by_ref_args() {
    let program = parse_fragment(
        br#"class EvalByRefMethodBox {
    public function __construct(&$value) {
        $value = $value . "-ctor";
    }
    public function change(&$value) {
        $value = $value . "-method";
    }
    public static function changeStatic(&$value) {
        $value = $value . "-static";
    }
}
$ctor = "A";
$box = new EvalByRefMethodBox($ctor);
$box->change($ctor);
EvalByRefMethodBox::changeStatic($ctor);
$named = "B";
$box->change(value: $named);
echo $ctor; echo ":";
return $named;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A-ctor-method-static:");
    assert_eq!(values.get(result), FakeValue::String("B-method".to_string()));
}

/// Verifies eval-declared by-reference method parameters reject temporary values.
#[test]
fn execute_program_rejects_eval_method_by_ref_temporary_arg() {
    let program = parse_fragment(
        br#"class EvalByRefMethodBox {
    public function change(&$value) {
        $value = "changed";
    }
}
$box = new EvalByRefMethodBox();
$box->change("literal");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a by-reference parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies typed eval-declared by-reference method params write back entry coercions.
#[test]
fn execute_program_writes_back_eval_method_by_ref_type_coercion() {
    let program = parse_fragment(
        br#"class EvalByRefTypedMethodBox {
    public function coerce(int &$value) {}
}
$value = "3";
$box = new EvalByRefTypedMethodBox();
$box->coerce($value);
return $value;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies eval-declared by-reference variadics write back mutated captured elements.
#[test]
fn execute_program_writes_back_eval_method_by_ref_variadic_elements() {
    let program = parse_fragment(
        br#"class EvalByRefVariadicMethodBox {
    public function change(&...$items) {
        $items[0] = $items[0] . "-first";
        $items["named"] = $items["named"] . "-named";
    }
    public function rebind(&...$items) {
        $items = [];
    }
}
$box = new EvalByRefVariadicMethodBox();
$first = "A";
$named = "B";
$box->change($first, named: $named);
$box->rebind($first);
echo $first; echo ":";
return $named;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "A-first:");
    assert_eq!(values.get(result), FakeValue::String("B-named".to_string()));
}

/// Verifies eval-declared by-reference method params write back array-element lvalues.
#[test]
fn execute_program_writes_back_eval_method_by_ref_array_elements() {
    let program = parse_fragment(
        br#"class EvalByRefArrayElementMethodBox {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function variadic(&...$items) {
        $items[0] = "variadic";
    }
}
$box = new EvalByRefArrayElementMethodBox();
$items = ["k" => "old"];
$box->set($items["k"], "changed");
$box->set($missing["new"], "created");
$box->variadic($items["k"]);
echo $items["k"]; echo ":";
return $missing["new"];"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "variadic:");
    assert_eq!(values.get(result), FakeValue::String("created".to_string()));
}

/// Verifies eval-declared by-reference method params write back object-property lvalues.
#[test]
fn execute_program_writes_back_eval_method_by_ref_object_properties() {
    let program = parse_fragment(
        br#"class EvalByRefPropertyChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function variadic(&...$items) {
        $items[0] = "variadic";
    }
}
class EvalByRefPublicPropertyBox {
    public string $value = "old";
}
class EvalByRefPrivatePropertyBox {
    private string $value = "private";
    public function update($changer) {
        $changer->set($this->value, "secret");
        return $this->value;
    }
}
$changer = new EvalByRefPropertyChanger();
$public = new EvalByRefPublicPropertyBox();
$changer->set($public->value, "changed");
$changer->variadic($public->value);
echo $public->value; echo ":";
$private = new EvalByRefPrivatePropertyBox();
return $private->update($changer);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "variadic:");
    assert_eq!(values.get(result), FakeValue::String("secret".to_string()));
}

/// Verifies eval-declared by-reference method params keep property access restrictions.
#[test]
fn execute_program_rejects_invalid_eval_method_by_ref_object_property_targets() {
    let private_program = parse_fragment(
        br#"class EvalByRefPrivatePropertyFailChanger {
    public function set(&$value) {
        $value = "bad";
    }
}
class EvalByRefPrivatePropertyFailBox {
    private string $value = "private";
}
$changer = new EvalByRefPrivatePropertyFailChanger();
$box = new EvalByRefPrivatePropertyFailBox();
$changer->set($box->value);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&private_program, &mut scope, &mut values)
        .expect_err("private property by-ref target should fail from global scope");
    assert_eq!(err, EvalStatus::UncaughtThrowable);

    let readonly_program = parse_fragment(
        br#"class EvalByRefReadonlyPropertyFailChanger {
    public function set(&$value) {
        $value = "bad";
    }
}
class EvalByRefReadonlyPropertyFailBox {
    public readonly string $value;
    public function __construct($changer) {
        $this->value = "old";
        $changer->set($this->value);
    }
}
new EvalByRefReadonlyPropertyFailBox(new EvalByRefReadonlyPropertyFailChanger());"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let err = execute_program(&readonly_program, &mut scope, &mut values)
        .expect_err("readonly property by-ref target should fail as an indirect modification");
    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval-declared variadic methods reject duplicate named variadic keys.
#[test]
fn execute_program_rejects_duplicate_eval_method_variadic_named_arg() {
    let program = parse_fragment(
        br#"class EvalDuplicateVariadicBox {
    public function read(...$tail) {
        return count($tail);
    }
}
$box = new EvalDuplicateVariadicBox();
return $box->read(name: "A", name: "B");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("duplicate named variadic argument should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies defaults before required eval method parameters do not make earlier slots optional.
#[test]
fn execute_program_rejects_eval_method_default_before_required_omission() {
    let program = parse_fragment(
        br#"class EvalRequiredAfterDefaultBox {
    public function read($left = "A", $right) {
        return $left . $right;
    }
}
$box = new EvalRequiredAfterDefaultBox();
return $box->read(right: "B");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("default before required parameter should remain required");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval-declared method scalar type hints coerce weak scalar arguments.
#[test]
fn execute_program_enforces_eval_method_scalar_type_hints() {
    let program = parse_fragment(
        br#"class EvalTypedScalarBox {
    public function read(int $id, string $label, bool $flag) {
        echo $id + 1; echo ":";
        echo $label; echo ":";
        return $flag ? "T" : "F";
    }
}
$box = new EvalTypedScalarBox();
return $box->read("7", 8, 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "8:8:");
    assert_eq!(values.get(result), FakeValue::String("T".to_string()));
}

/// Verifies eval-declared method scalar type hints reject non-coercible values.
#[test]
fn execute_program_rejects_eval_method_scalar_type_mismatch() {
    let program = parse_fragment(
        br#"class EvalTypedScalarFailBox {
    public function read(int $id) {
        return $id;
    }
}
$box = new EvalTypedScalarFailBox();
return $box->read("not numeric");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("non-numeric string should fail int parameter type");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies eval-declared method class/interface type hints accept matching eval objects.
#[test]
fn execute_program_enforces_eval_method_object_type_hints() {
    let program = parse_fragment(
        br#"interface EvalTypedReadable {}
class EvalTypedDep implements EvalTypedReadable {}
class EvalTypedObjectBox {
    public function read(EvalTypedReadable $dep, ?EvalTypedDep $nullable) {
        echo get_class($dep); echo ":";
        return $nullable === null ? "N" : "bad";
    }
}
$dep = new EvalTypedDep();
$box = new EvalTypedObjectBox();
return $box->read($dep, null);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "EvalTypedDep:");
    assert_eq!(values.get(result), FakeValue::String("N".to_string()));
}

/// Verifies eval-declared variadic method type hints apply to each captured argument.
#[test]
fn execute_program_enforces_eval_method_variadic_type_hints() {
    let program = parse_fragment(
        br#"class EvalTypedVariadicBox {
    public function sum(int ...$items) {
        return $items[0] + $items[1];
    }
}
$box = new EvalTypedVariadicBox();
return $box->sum("3", 4);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}

/// Verifies eval-declared methods reject unknown named arguments.
#[test]
fn execute_program_rejects_unknown_eval_method_named_arg() {
    let program = parse_fragment(
        br#"class EvalUnknownNamedMethodBox {
    public function read($left) {
        return $left;
    }
}
$box = new EvalUnknownNamedMethodBox();
return $box->read(missing: "bad");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unknown named method argument should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies runtime/AOT method fallback binds registered native method named arguments.
#[test]
fn execute_program_binds_registered_runtime_method_named_args() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
return $box->add2_x(right: 2, left: 3);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("registered runtime method named args should bind");

    assert_eq!(values.get(result), FakeValue::Int(15));
}

/// Verifies runtime/AOT method fallback honors registered by-reference parameter metadata.
#[test]
fn execute_program_rejects_runtime_method_by_ref_temporary_arg() {
    let program = parse_fragment(
        br#"$box = new KnownClass(10);
return $box->add2_x(1, 2);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut signature = NativeCallableSignature::new(2);
    assert!(signature.set_param_name(0, "left"));
    assert!(signature.set_param_name(1, "right"));
    assert!(signature.set_param_by_ref(0, true));
    assert!(context.define_native_method_signature("KnownClass", "add2_x", signature));
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("literal cannot satisfy a runtime by-reference method parameter");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}

/// Verifies runtime/AOT method fallback rejects named arguments without metadata.
#[test]
fn execute_program_rejects_unregistered_named_args_for_runtime_method_fallback() {
    let program =
        parse_fragment(br#"return $this->answer(value: 1);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let object = values.alloc(FakeValue::Object(Vec::new()));
    scope.set("this", object, ScopeCellOwnership::Borrowed);

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("unregistered runtime method fallback named args should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
}
