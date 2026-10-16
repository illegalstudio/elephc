//! Purpose:
//! Regressions for readonly storage widening and reflected retained abstract setters.
//!
//! Called from:
//! - The OOP codegen integration harness.
//!
//! Key details:
//! - Eval declarations retain the original setter type even when backing storage is broader.
//! - Reflection setValue dispatches hooks; eval's setRawValue keeps its physical-storage bypass.

use crate::support::*;

/// Builds equivalent native, eval-only and native-parent programs without changing their body.
fn source(native_parent: Option<bool>, body: &str) -> String {
    let parent = "abstract class Base { abstract public int $x { set; } }";
    let child = "abstract class Child extends Base { public public(set) readonly int|string $x; } class Grandchild extends Child {}";
    match native_parent {
        None => format!("<?php {parent} {child} {body}"),
        Some(false) => format!("<?php eval('{parent} {child} {body}');"),
        Some(true) => format!("<?php {parent} echo (new ReflectionClass(Base::class))->isAbstract() ? '' : 'bad'; eval('{child} {body}');"),
    }
}

/// Broader backing storage keeps the inherited setter's narrower argument contract.
fn verify_storage(native_parent: Option<bool>) {
    let body = r#"$object = new Grandchild();
$object->x = 4;
echo isset($object->x) ? "bad" : "U";
try { $object->x = "bad"; }
catch (TypeError $error) { echo "|", $error->getMessage(); }"#;
    assert_eq!(compile_and_run(&source(native_parent, body)),
        "U|Base::$x::set(): Argument #1 ($value) must be of type int, string given");
}

/// Reflection dispatches retained setters and preserves eval's supported raw-storage bypass.
fn verify_reflection(native_parent: Option<bool>) {
    let mut body = r#"$object = new Grandchild();
$property = new ReflectionProperty(Grandchild::class, "x");
$property->setValue($object, 4);
echo $property->isInitialized($object) ? "bad" : "U";
try { $property->setValue($object, "bad"); }
catch (TypeError $error) { echo "|", $error->getMessage(); }"#.to_string();
    let mut expected = "U|Base::$x::set(): Argument #1 ($value) must be of type int, string given".to_string();
    if native_parent.is_some() {
        // The raw API is currently eval-only; do not introduce a new native method here.
        body.push_str(r#"class MutableHook {
    public int $value { get { return $this->value; } set { echo "bad"; $this->value = $value; } }
}
$raw = new MutableHook();
$raw_property = new ReflectionProperty(MutableHook::class, "value");
$raw_property->setRawValue($raw, 7); echo "|", $raw_property->getRawValue($raw);"#);
        expected.push_str("|7");
    }
    assert_eq!(compile_and_run(&source(native_parent, &body)), expected);
}

/// Native storage is the legal widening control for the eval declaration checks.
#[test]
fn test_readonly_setter_review_storage_native() { verify_storage(None); }

/// Eval-only inheritance accepts legal setter-supertype storage.
#[test]
fn test_readonly_setter_review_storage_eval() { verify_storage(Some(false)); }

/// Native parent metadata grants eval children the same storage variance.
#[test]
fn test_readonly_setter_review_storage_native_parent() { verify_storage(Some(true)); }

/// Statically resolved ReflectionProperty writes must not bypass the retained setter.
#[test]
fn test_readonly_setter_review_reflection_native() { verify_reflection(None); }

/// Eval Reflection writes preserve setter dispatch and the explicit raw bypass.
#[test]
fn test_readonly_setter_review_reflection_eval() { verify_reflection(Some(false)); }

/// Native-parent eval Reflection keeps the original setter owner and type.
#[test]
fn test_readonly_setter_review_reflection_native_parent() { verify_reflection(Some(true)); }

/// Reflected no-op writes add no owners beyond normally released reflector construction.
#[test]
fn test_readonly_setter_review_reflection_owners() {
    let mut baseline = None;
    for iterations in [1, 8] {
        let run = |function: &str| compile_and_run_with_heap_debug(&source(None, &format!(r#"
function reflect(Grandchild $object): void {{
    try {{ (new ReflectionProperty(Grandchild::class, "x"))->setValue($object, str_repeat("b", 2)); }}
    catch (TypeError $error) {{ echo "T"; unset($error); }}
    (new ReflectionProperty(Grandchild::class, "x"))->setValue($object, 4);
    echo (new ReflectionProperty(Grandchild::class, "x"))->isInitialized($object) ? "bad" : "U";
}}
function control(Grandchild $object): void {{
    new ReflectionProperty(Grandchild::class, "x");
    new ReflectionProperty(Grandchild::class, "x");
    new ReflectionProperty(Grandchild::class, "x");
    echo "TU";
}}
for ($i = 0; $i < {iterations}; $i++) {{ $object = new Grandchild(); {function}($object); unset($object); }}
"#)));
        let control = run("control");
        let result = run("reflect");
        assert!(control.success && result.success, "control: {}\nresult: {}\n{}", control.stderr, result.stdout, result.stderr);
        assert_eq!(control.stdout, "TU".repeat(iterations));
        assert_eq!(result.stdout, control.stdout, "{}", result.stderr);
        let retained = |stderr: &str| ["live_blocks=", "live_bytes="].map(|metric| {
            stderr.lines().next().unwrap().split_whitespace()
                .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap()
        });
        let control_retained = retained(&control.stderr);
        let result_retained = retained(&result.stderr);
        assert_eq!(result_retained[0], control_retained[0], "control: {}\nresult: {}", control.stderr, result.stderr);
        if let Some((control_baseline, result_baseline)) = baseline {
            let growth = |current: [usize; 2], original: [usize; 2]| [current[0] - original[0], current[1] - original[1]];
            let result_growth = growth(result_retained, result_baseline);
            let control_growth = growth(control_retained, control_baseline);
            assert_eq!(result_growth[0], control_growth[0]);
            // Free-list reuse can change retained metadata block capacities across binaries.
            assert!(result_growth[1] <= control_growth[1],
                "argument owners must not add to the pre-existing metadata baseline: {result_growth:?} > {control_growth:?}");
        } else { baseline = Some((control_retained, result_retained)); }
    }
}

/// A reflected heap result outlives both an inline reflector and the source object temporary.
#[test]
fn test_readonly_setter_review_reflected_result_owner() {
    assert_eq!(compile_and_run(r#"<?php
class ResultBox {
    public array $data;
    public function __construct() { $this->data = [str_repeat("x", 2)]; }
    public function __destruct() { echo "D"; }
}
function read(ResultBox $box): mixed {
    return (new ReflectionProperty(ResultBox::class, "data"))->getValue($box);
}
$result = read(new ResultBox()); echo $result[0];
"#), "Dxx");
}

/// Retained setter guards reject null after nullable backing-storage materialization.
#[test]
fn test_readonly_setter_tenth_review_nullable_argument_guard() {
    let source = r#"<?php
abstract class Base { abstract public int $x { set; } }
abstract class Child extends Base { public public(set) readonly ?int $x; }
class Grandchild extends Child {}
$object = new Grandchild();
try { $object->x = null; }
catch (TypeError $error) { echo $error->getMessage(), "|"; }
try { (new ReflectionProperty(Grandchild::class, "x"))->setValue($object, null); }
catch (TypeError $error) { echo $error->getMessage(), "|"; }
echo (new ReflectionProperty(Grandchild::class, "x"))->isInitialized($object) ? "bad" : "U";
"#;
    assert_eq!(
        compile_and_run(source),
        "Base::$x::set(): Argument #1 ($value) must be of type int, null given|".repeat(2) + "U",
    );
}

/// Abstract setter storage accepts class supertypes declared later in schema order.
#[test]
fn test_readonly_setter_tenth_review_aot_class_supertype_order() {
    assert_eq!(compile_and_run(r#"<?php
abstract class AContract { abstract public ZValue $x { set; } }
abstract class BStorage extends AContract { public public(set) readonly YBase $x; }
class CConcrete extends BStorage {}
class YBase {}
class ZValue extends YBase {}
$object = new CConcrete();
$object->x = new ZValue();
echo isset($object->x) ? "bad" : "U";
"#), "U");
}

/// Lexical `self` setter and storage types resolve against their own declaring classes.
#[test]
fn test_readonly_setter_tenth_review_eval_self_owner() {
    let out = compile_and_run_capture(r#"<?php
echo "before";
eval('abstract class EvalSelfBase { abstract public self $x { set; } }
abstract class EvalSelfChild extends EvalSelfBase {
    public public(set) readonly self $x;
}
echo "bad";');
"#);
    assert!(!out.success, "{}", out.stdout);
    assert_eq!(out.stdout, "before", "{}", out.stderr);
    assert!(out.stderr.contains("Fatal error: eval() runtime failed"), "{}", out.stderr);
}

/// Each IR-off control is separately scheduled under the existing CI timeout.
macro_rules! without_opt_test {
    ($name:ident, $case:ident) => {
        /// Preserves this readonly-setter regression without EIR optimization.
        #[test]
        fn $name() { without_ir_opt($case); }
    };
}

without_opt_test!(test_readonly_setter_review_storage_native_no_ir_opt, test_readonly_setter_review_storage_native);
without_opt_test!(test_readonly_setter_review_storage_eval_no_ir_opt, test_readonly_setter_review_storage_eval);
without_opt_test!(test_readonly_setter_review_storage_native_parent_no_ir_opt, test_readonly_setter_review_storage_native_parent);
without_opt_test!(test_readonly_setter_review_reflection_native_no_ir_opt, test_readonly_setter_review_reflection_native);
without_opt_test!(test_readonly_setter_review_reflection_eval_no_ir_opt, test_readonly_setter_review_reflection_eval);
without_opt_test!(test_readonly_setter_review_reflection_native_parent_no_ir_opt, test_readonly_setter_review_reflection_native_parent);
without_opt_test!(test_readonly_setter_review_reflection_owners_no_ir_opt, test_readonly_setter_review_reflection_owners);
without_opt_test!(test_readonly_setter_review_reflected_result_owner_no_ir_opt, test_readonly_setter_review_reflected_result_owner);
without_opt_test!(test_readonly_setter_tenth_review_nullable_argument_guard_no_ir_opt, test_readonly_setter_tenth_review_nullable_argument_guard);
without_opt_test!(test_readonly_setter_tenth_review_aot_class_supertype_order_no_ir_opt, test_readonly_setter_tenth_review_aot_class_supertype_order);
without_opt_test!(test_readonly_setter_tenth_review_eval_self_owner_no_ir_opt, test_readonly_setter_tenth_review_eval_self_owner);
