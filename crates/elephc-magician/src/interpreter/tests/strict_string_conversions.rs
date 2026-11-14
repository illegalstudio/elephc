//! Purpose:
//! Verifies explicit string builtin calls use shared cast adapters and balanced owners.
//!
//! Called from:
//! - Focused staged interpreter conversion tests.
//!
//! Key details:
//! - Direct, descriptor and value-invoker arguments all retain source-order leases.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Every call form retires fresh source and hook-result strings after conversion or a throw.
#[test]
fn strict_string_conversions_eval_call_surfaces_and_owners() {
    for call in ["strval(new EvalConversionText())", "$cast(new EvalConversionText())",
        "$name(new EvalConversionText())", "call_user_func('strval', new EvalConversionText())",
        "call_user_func_array('strval', [new EvalConversionText()])", "strval(value: new EvalConversionText())"] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        assert!(context.set_global_scope(&mut scope));
        let source = format!(r#"class EvalConversionText {{ public function __toString(): string {{ return 'text'; }} }}
$cast = strval(...); $name = 'strval'; echo {call}, ':', strval(str_repeat('x', 3)); return true;"#);
        let result = staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values);
        let Ok(EvalControl::Return(result)) = result else { panic!("{call}: {:?} {}", result.err(), values.output); };
        assert_eq!(values.output, "text:xxx", "{call}"); values.release(result).unwrap();
        for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{call}: {:?}", values.values[id]); }
    }
}

/// Checked boolean conversions share native handlers and transfer protected failures into catches.
#[test]
fn strict_string_conversions_eval_boolean_native_handlers_and_failure_owners() {
    for (vm, failed) in [(false, false), (true, false), (false, true)] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
        let mut scope = ElephcEvalScope::new(); let mut values = FakeOps::default();
        let source = values.bool_value(false).unwrap();
        let property = if failed { "__bool_failure" } else { "__boolean" };
        let object = values.alloc(FakeValue::Object(vec![(property.into(), source)]));
        values.release(source).unwrap();
        values.object_classes.insert(object.as_ptr() as usize, "SimpleXMLElement".into());
        scope.set("original", object, ScopeCellOwnership::Owned);
        let code = if failed {
            "try { boolval($original); echo 'X'; } catch (ValueError $error) { echo 'V'; } return true;"
        } else { "return boolval($original);" };
        // Catch statements select the walker; the scalar success path runs in both consumers.
        let code = if vm { format!("if (false) {{ function boolval($value) {{}} }} {code}") } else { code.to_string() };
        let result = staged_numeric_execution(&code, vm, &mut context, &mut scope, &mut values);
        let Ok(EvalControl::Return(result)) = result else { panic!("bool conversion vm={vm} failed={failed}: {:?}", result.err()); };
        assert_eq!(values.get(result), FakeValue::Bool(failed));
        assert_eq!(values.output, if failed { "V" } else { "" }); values.release(result).unwrap();
        for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
        for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
    }
}
