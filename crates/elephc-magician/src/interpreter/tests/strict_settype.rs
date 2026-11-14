//! Purpose:
//! Verifies checked settype conversions precede reference writeback on every eval call surface.
//!
//! Called from:
//! - Focused staged interpreter semantic-profile regressions.
//!
//! Key details:
//! - Ref-like calls use the existing walker fallback because the VM excludes reference parameters.
//! - Failed conversions preserve the receiver and retire all argument and Throwable owners.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Releases the complete test activation, including every cell retained by a reference alias.
fn clean(scope: &mut ElephcEvalScope, values: &mut FakeOps) {
    for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
}

/// Invalid runtime targets fail before alias or container mutation and retire all call owners.
#[test]
fn strict_settype_eval_invalid_targets_preserve_receivers_and_owners() {
    for call in ["settype($saved, $target);", "settype(type: $target, var: $saved);",
        "$cast = settype(...); $cast($saved, $target);",
        "$cast = 'settype'; $cast($saved, $target);",
        "call_user_func_array('settype', [&$saved, $target]);",
        "settype($items['k' . ''], $target);"] {
        for target in ["unset", "resource", "int ", "int\0x"] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
            let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
            let target_cell = values.alloc(FakeValue::String(target.into()));
            scope.set("target", target_cell, ScopeCellOwnership::Owned);
            let source = format!("$saved = 'kept'; $alias =& $saved; $items = ['k' => 'kept']; {call}");
            assert!(matches!(staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values),
                Err(EvalStatus::UncaughtThrowable)), "{call} {target:?}");
            assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::String("kept".into()));
            assert_eq!(values.get(scope.visible_cell("alias").unwrap()), FakeValue::String("kept".into()));
            let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, "ValueError", false).unwrap());
            values.release(error).unwrap(); clean(&mut scope, &mut values);
        }
    }
}

/// Checked target errors are opt-in and the existing permissive false result remains observable.
#[test]
fn strict_settype_eval_invalid_target_permissive_control() {
    let mut context = ElephcEvalContext::new(); let mut values = FakeOps::default(); let mut scope = ElephcEvalScope::new();
    let Ok(EvalControl::Return(result)) = staged_numeric_execution(
        "$value = 'kept'; return settype($value, 'unset');", false, &mut context, &mut scope, &mut values)
        else { panic!("permissive settype target control"); };
    assert_eq!(values.get(result), FakeValue::Bool(false));
    assert_eq!(values.get(scope.visible_cell("value").unwrap()), FakeValue::String("kept".into()));
    values.release(result).unwrap(); clean(&mut scope, &mut values);
}

/// Direct, named, dynamic and callable failures preserve the source value and its aliases.
#[test]
fn strict_settype_eval_failed_conversions_preserve_reference_values() {
    for call in ["settype($saved, 'INTEGER');", "settype(type: 'double', var: $saved);",
        "$convert = 'settype'; $convert($saved, 'int');",
        "$convert = settype(...); $convert($saved, 'float');",
        "call_user_func_array('settype', [&$saved, 'int']);",
        "call_user_func('settype', $saved, 'int');"] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = format!("$saved = 'junk'; $alias =& $saved; {call}");
        assert!(matches!(staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable)), "{call}");
        assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::String("junk".into()), "{call}");
        assert_eq!(values.get(scope.visible_cell("alias").unwrap()), FakeValue::String("junk".into()), "{call}");
        let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, "ValueError", false).unwrap());
        values.release(error).unwrap(); clean(&mut scope, &mut values);
    }
}

/// Exact numeric values publish after conversion while array and property failures leave storage intact.
#[test]
fn strict_settype_eval_exact_results_properties_arrays_and_cleanup() {
    let source = r#"
$value = '9223372036854775807.9'; $alias =& $value;
settype($value, 'int'); echo $alias, '|';
settype($value, 'double'); echo is_float($alias) ? 'F|' : 'X|';
$items = ['k' => 'junk']; $copy = $items;
try { settype($items['k'], 'int'); echo 'X'; } catch (ValueError $error) { echo $items['k'], ':', $copy['k'], '|'; }
$items = ['k' => '7.9']; $copy = $items; $dynamic = 'settype';
$dynamic($items['k'], 'integer'); echo $items['k'], ':', $copy['k'], '|';
$nested = ['inner' => ['value' => 'junk']]; $convert = settype(...);
try { $convert($nested['inner']['value'], 'float'); echo 'X'; } catch (ValueError $error) { echo $nested['inner']['value'], '|'; }
class StrictSettypeBox { public mixed $value = 'junk'; public static mixed $shared = 'junk'; }
$box = new StrictSettypeBox(); $convert = settype(...);
try { $convert($box->value, 'float'); echo 'X'; } catch (ValueError $error) { echo $box->value, '|'; }
$name = 'int';
try { settype(StrictSettypeBox::$shared, $name); echo 'X'; } catch (ValueError $error) { echo StrictSettypeBox::$shared, '|'; }
$value = 1e999;
try { settype($value, 'integer'); echo 'X'; } catch (ValueError $error) { echo is_float($alias) ? 'I' : 'X'; }
return true;
"#;
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
    let Ok(EvalControl::Return(result)) = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
        else { panic!("checked settype activation failed: {}", values.output); };
    assert_eq!(values.output, "9223372036854775807|F|junk:junk|7:7.9|junk|junk|junk|I");
    values.release(result).unwrap();
    for entry in context.take_persistent_cell_owners() { values.release(entry).unwrap(); }
    clean(&mut scope, &mut values);
}

/// Checked and permissive scalar conversions retain independent policies and returned owners.
#[test]
fn strict_settype_eval_kernel_controls_and_value_argument_retirement() {
    for checked in [false, true] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, checked)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = "$value = 'junk'; try { settype($value, 'int'); echo $value; } catch (ValueError $e) { echo $value; } return true;";
        let Ok(EvalControl::Return(result)) = staged_numeric_execution(source, false, &mut context, &mut scope, &mut values)
            else { panic!("settype control"); };
        assert_eq!(values.output, if checked { "junk" } else { "0" });
        values.release(result).unwrap(); clean(&mut scope, &mut values);
    }
}
