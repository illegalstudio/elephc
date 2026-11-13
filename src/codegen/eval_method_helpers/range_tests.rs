//! Purpose:
//! Verifies wide transfers between eval dispatch checks and distant method bodies.
//!
//! Called from:
//! - The eval method helper unit tests on every supported target.
//!
//! Key details:
//! - The receiver, name, scope and arity checks share the production emitters.
//! - ARM64 conditional edges stay local even when the dispatch body grows beyond one MiB.

use super::*;
use crate::codegen::platform::Target;

/// Builds a scalar method case without requiring a PHP program or linked implementation.
fn instance_slot(name: &str, visibility: Visibility) -> EvalMethodSlot {
    EvalMethodSlot {
        class_id: 1,
        class_name: "WideReceiver".into(),
        method: name.into(),
        impl_class: "WideReceiver".into(),
        visibility,
        allowed_scopes: vec!["WideReceiver".into()],
        params: vec![PhpType::Int],
        ref_params: vec![false],
        return_ty: PhpType::Int,
        by_ref_return: false,
        return_is_owned: false,
        return_uses_marker: false,
        is_hidden_shadow: false,
        entry_symbol: method_symbol("WideReceiver", name),
        runtime_helper: None,
    }
}

/// Builds the corresponding static method case with the same scope and argument checks.
fn static_slot(name: &str, visibility: Visibility) -> EvalStaticMethodSlot {
    let instance = instance_slot(name, visibility);
    EvalStaticMethodSlot {
        class_id: instance.class_id,
        class_name: instance.class_name,
        method: instance.method,
        impl_class: instance.impl_class,
        visibility: instance.visibility,
        allowed_scopes: instance.allowed_scopes,
        params: instance.params,
        ref_params: instance.ref_params,
        return_ty: instance.return_ty,
        by_ref_return: false,
        return_is_owned: false,
        return_uses_marker: false,
        entry_symbol: static_method_symbol("WideReceiver", name),
    }
}

/// Both dispatchers use wide ARM64 fail/body edges while retaining x86_64 conditional transfers.
#[test]
fn eval_method_dispatch_uses_wide_edges_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let support = crate::codegen::eval_callable_helpers::emit_eval_callable_descriptor_support(
            &module, &mut emitter, &mut data, false,
        );
        let methods = [instance_slot("publichit", Visibility::Public),
            instance_slot("privatehit", Visibility::Private)];
        let statics = [static_slot("publicstatic", Visibility::Public),
            static_slot("protectedstatic", Visibility::Protected)];
        emit_method_call_helper(&module, &mut emitter, &mut data, &methods, &[], &support);
        emit_static_method_call_helper(&module, &mut emitter, &mut data, &statics, &support);
        let assembly = emitter.output();
        let lines: Vec<_> = assembly.lines().map(str::trim).collect();
        if target.arch == Arch::AArch64 {
            let mut far = vec!["__elephc_eval_value_method_call_fail".to_string(),
                "__elephc_eval_value_static_method_call_fail".to_string()];
            far.extend(methods.iter().map(|slot| method_body_label(&module, slot)));
            far.extend(statics.iter().map(|slot| static_method_body_label(&module, slot)));
            for label in far {
                assert!(lines.iter().any(|line| *line == format!("b {label}")), "{name}: {label}\n{assembly}");
                assert!(!lines.iter().any(|line| {
                    (line.starts_with("b.") || line.starts_with("cbz ") || line.starts_with("cbnz "))
                        && line.split_whitespace().last() == Some(label.as_str())
                }), "{name}: narrow transfer to {label}\n{assembly}");
            }
        } else {
            assert!(assembly.contains("jz __elephc_eval_value_method_call_fail_x"), "{name}");
            assert!(assembly.contains("jne __elephc_eval_value_static_method_call_fail_x"), "{name}");
            assert!(assembly.contains(&format!("je {}", method_body_label(&module, &methods[0]))), "{name}");
            assert!(assembly.contains(&format!("je {}", static_method_body_label(&module, &statics[0]))), "{name}");
        }
    }
}
