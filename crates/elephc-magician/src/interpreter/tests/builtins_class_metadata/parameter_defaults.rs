//! Purpose:
//! Interpreter tests for reflected parameter and property default values.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Constant and magic-constant defaults are resolved in their declaring scope.

use super::super::super::*;
use super::super::support::*;

/// Registered native errors are raised before omitted slots and result allocations are skipped.
#[test]
fn reflected_class_registered_default_error_keeps_no_temporary_owners() {
    let mut context = ElephcEvalContext::new();
    assert!(context.define_class(EvalClass::new("Probe", vec![
        EvalClassProperty::new("good", Some(EvalExpr::Const(EvalConst::String("ok".into())))),
        EvalClassProperty::new("bad", None),
    ], Vec::new())));
    assert!(context.define_native_property_default_error("Probe", "bad", "missing lexical parent".into()));
    context.register_eval_reflection_class(7, "Probe");
    let mut values = FakeOps::default();
    for _ in 0..8 {
        assert_eq!(eval_reflection_class_get_default_properties_result(
            7, "getDefaultProperties", Vec::new(), &mut context, &mut values,
        ), Err(EvalStatus::UncaughtThrowable));
        let error = context.take_pending_throw().expect("owned default Error");
        assert!(!error.is_borrowed());
        eval_release_value(&mut context, &mut values, error).unwrap();
        assert_eq!(values.cell_owners.values().sum::<usize>(), 0);
    }
}

/// Failed map insertion retires the partial default map, the current value, and its key.
#[test]
fn reflected_class_partial_default_map_keeps_no_temporary_owners() {
    let mut context = ElephcEvalContext::new();
    assert!(context.define_class(EvalClass::new("Probe", vec![
        EvalClassProperty::new("good", Some(EvalExpr::Const(EvalConst::String("ok".into())))),
        EvalClassProperty::new("second", Some(EvalExpr::Const(EvalConst::Int(7)))),
    ], Vec::new())));
    context.register_eval_reflection_class(7, "Probe");
    let mut values = FakeOps::default();
    for _ in 0..8 {
        values.fail_array_set_call = Some(values.array_set_calls + 1);
        assert_eq!(eval_reflection_class_get_default_properties_result(
            7, "getDefaultProperties", Vec::new(), &mut context, &mut values,
        ), Err(EvalStatus::UnsupportedConstruct));
        assert_eq!(values.cell_owners.values().sum::<usize>(), 0);
    }
}

/// Native reflected defaults preserve their pending Error instead of becoming values or fatals.
#[test]
fn reflected_native_deferred_default_raises_an_owned_error() {
    let expression = EvalExpr::DeferredDefaultError("missing lexical parent".into());
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(eval_owned_expr(&expression, &mut context, &mut scope, &mut values),
        Err(EvalStatus::UncaughtThrowable));
    let error = context.take_pending_throw().expect("owned pending Error");
    assert!(!error.is_borrowed());
    eval_release_value(&mut context, &mut values, error).unwrap();
    assert_eq!(values.cell_owners.values().sum::<usize>(), 0);
}

/// Verifies ReflectionParameter reports eval constant-default metadata.
#[test]
fn execute_program_reflection_parameter_reports_default_constant_metadata() {
    let program = parse_fragment(
        br##"define("EVAL_REFLECT_PARAM_DEFAULT_GLOBAL", "G");
class EvalReflectParamDefaultBase {
    const BASE = "B";
}
class EvalReflectParamDefaultTarget extends EvalReflectParamDefaultBase {
    const LABEL = "L";
    public function run($required, $global = EVAL_REFLECT_PARAM_DEFAULT_GLOBAL, $self = self::LABEL, $parent = parent::BASE, $literal = 7) {}
}
$params = (new ReflectionMethod("EvalReflectParamDefaultTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName(); echo ":";
    echo $param->isDefaultValueAvailable() ? "D:" : "d:";
    if ($param->isDefaultValueAvailable()) {
        if ($param->isDefaultValueConstant()) {
            echo "C:";
            echo $param->getDefaultValueConstantName();
            echo ":";
        } else {
            echo "c:null:";
        }
        echo $param->getDefaultValue();
    }
    echo "|";
}
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "required:d:|global:D:C:EVAL_REFLECT_PARAM_DEFAULT_GLOBAL:G|self:D:C:self::LABEL:L|parent:D:C:parent::BASE:B|literal:D:c:null:7|"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionParameter default magic constants use declaring callable scopes.
#[test]
fn execute_program_reflection_parameter_resolves_default_magic_constants() {
    let program = parse_fragment(
        br##"namespace EvalReflectParamMagicNs;
function eval_reflect_param_magic($fn = __FUNCTION__, $m = __METHOD__, $c = __CLASS__, $t = __TRAIT__, $n = __NAMESPACE__) {}
interface EvalReflectParamMagicIface {
    public function read($c = __CLASS__, $m = __METHOD__, $fn = __FUNCTION__, $t = __TRAIT__, $n = __NAMESPACE__);
}
trait EvalReflectParamMagicTrait {
    public function source($c = __CLASS__, $t = __TRAIT__, $m = __METHOD__, $fn = __FUNCTION__, $n = __NAMESPACE__) {}
}
class EvalReflectParamMagicBox {
    use EvalReflectParamMagicTrait { source as aliasSource; }
    public function own($c = __CLASS__, $t = __TRAIT__, $m = __METHOD__, $fn = __FUNCTION__, $n = __NAMESPACE__) {}
}
function eval_param_magic_dump($ref) {
    foreach ($ref->getParameters() as $param) {
        echo "[" . $param->getDefaultValue() . "]";
    }
    echo ":";
}
eval_param_magic_dump(new \ReflectionFunction(__NAMESPACE__ . "\\eval_reflect_param_magic"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicBox::class, "own"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicBox::class, "aliasSource"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicIface::class, "read"));
return true;"##,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "[EvalReflectParamMagicNs\\eval_reflect_param_magic]",
            "[EvalReflectParamMagicNs\\eval_reflect_param_magic]",
            "[][][EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox]",
            "[]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox::own]",
            "[own]",
            "[EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicTrait]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicTrait::source]",
            "[source]",
            "[EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicIface]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicIface::read]",
            "[read]",
            "[]",
            "[EvalReflectParamMagicNs]:"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies ReflectionClass exposes eval property default metadata as an associative map.
#[test]
fn execute_program_reflection_class_get_default_properties_metadata() {
    let program = parse_fragment(
        br#"class EvalReflectDefaultBase {
    public int $base = 1;
    protected string $prot = "p";
    private int $shadow = 3;
    public $implicit;
    public int $typed;
    public static string $baseStatic = "bs";
}
class EvalReflectDefaultChild extends EvalReflectDefaultBase {
    public int $child = 5;
    private int $shadow = 9;
    public static int $childStatic = 7;
    public ?int $nullable = null;
}
$defaults = (new ReflectionClass("EvalReflectDefaultChild"))->getDefaultProperties();
echo $defaults["childStatic"]; echo ":";
echo $defaults["baseStatic"]; echo ":";
echo $defaults["child"]; echo ":";
echo $defaults["shadow"]; echo ":";
echo $defaults["base"]; echo ":";
echo $defaults["prot"]; echo ":";
echo array_key_exists("implicit", $defaults) && $defaults["implicit"] === null ? "I:" : "i:";
echo array_key_exists("nullable", $defaults) && $defaults["nullable"] === null ? "N:" : "n:";
echo array_key_exists("typed", $defaults) ? "T" : "t";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "7:bs:5:9:1:p:I:N:t");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
