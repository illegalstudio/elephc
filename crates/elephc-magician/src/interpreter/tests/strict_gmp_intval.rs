//! Purpose:
//! Verifies checked GMP builtin helper selection through eval call surfaces.
//!
//! Called from:
//! - Focused walker/VM public builtin dispatch tests.
//!
//! Key details:
//! - Compiled helper test doubles preserve the public bridge and binding boundaries.

use super::super::*;
use super::{support::*, strict_semantics::staged_numeric_execution};
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Registers one compiled helper test double with the real hidden strict/num parameter ABI.
fn register(context: &mut ElephcEvalContext, name: &str, result: RuntimeCellHandle) {
    let mut function = NativeFunction::new(result.as_ptr().cast(), fake_native_return_descriptor, 2);
    assert!(function.set_param_name(0, "strict")); assert!(function.set_param_name(1, "num"));
    assert!(context.define_native_function(name, function).is_ok());
}

/// Direct, named and callable builtin dispatch selects the checked helper without changing other GMP calls.
#[test]
fn strict_gmp_intval_eval_selects_public_profile_helper() {
    for strict in [false, true] { for vm in [false, true] {
        for source in ["return gMp_InTvAl(7);", "return gmp_intval(num: 7);",
            "$cast = gmp_intval(...); return $cast(7);", "return call_user_func('gmp_intval', 7);", "return gmp_sign(7);"] {
            if vm && source != "return gMp_InTvAl(7);" { continue; }
            let checked_call = strict && source != "return gmp_sign(7);";
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, strict)).unwrap();
            let mut values = FakeOps::default(); let mut scope = ElephcEvalScope::new();
            let ordinary = values.int(17).unwrap(); let checked = values.int(29).unwrap();
            register(&mut context, "__elephc_gmp_strval", ordinary);
            register(&mut context, "__elephc_gmp_intval", ordinary);
            register(&mut context, "__elephc_gmp_sign", ordinary);
            register(&mut context, "__elephc_gmp_checked_intval", checked);
            let source = if vm { format!("if (false) {{ function gmp_intval($num) {{}} }} {source}") } else { source.to_string() };
            let Ok(EvalControl::Return(result)) = staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values)
                else { panic!("{source} strict={strict} vm={vm}"); };
            assert_eq!(values.get(result), FakeValue::Int(if checked_call { 29 } else { 17 }));
            values.release(result).unwrap();
            for owner in scope.drain_owned_cells() { values.release(owner).unwrap(); }
        }
    } }
}

/// A checked host gap cannot silently fall back to the truncating permissive GMP helper.
#[test]
fn strict_gmp_intval_eval_missing_checked_helper_fails_closed() {
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Casts, true)).unwrap();
    let mut values = FakeOps::default(); let mut scope = ElephcEvalScope::new();
    let ordinary = values.int(17).unwrap();
    register(&mut context, "__elephc_gmp_strval", ordinary);
    register(&mut context, "__elephc_gmp_intval", ordinary);
    assert!(matches!(staged_numeric_execution("return gmp_intval(7);", false, &mut context, &mut scope, &mut values),
        Err(EvalStatus::UncaughtThrowable)));
    let error = context.take_pending_throw().unwrap(); assert!(values.object_is_a(error, "Error", false).unwrap());
    values.release(error).unwrap(); values.release(ordinary).unwrap();
    for (id, owners) in &values.cell_owners { assert_eq!(*owners, 0, "{:?}", values.values[id]); }
}
