//! Purpose:
//! Verifies that eval access-mask probes retire owned snapshots before every branch.
//!
//! Called from:
//! - The all-target eval emitter regression suite in CI.
//!
//! Key details:
//! - A local probe owns its canonical or raw snapshot; a global probe borrows its box.
//! - Cleanup preserves the unboxed scalar tag and restores the stack before branching.

use super::emit_branch_if_mixed_numeric_scalar;
use crate::codegen_support::{emit::Emitter, platform::Target};

/// Every target balances a local snapshot without changing borrowed global probe ownership.
#[test]
fn numeric_scope_probe_retires_owned_snapshot_before_all_branches_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for owned in [false, true] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_branch_if_mixed_numeric_scalar(&mut emitter, "numeric_source", owned);
            let assembly = emitter.output();
            let unbox = assembly.find("__rt_mixed_unbox").unwrap();
            let branch = assembly.find(if name == "linux-x86_64" {
                "cmp rax, 0"
            } else { "cmp x0, #0" }).unwrap();
            assert_eq!(assembly.matches("__rt_decref_mixed").count(), usize::from(owned),
                "{name}: {assembly}");
            if owned {
                let release = assembly.find("__rt_decref_mixed").unwrap();
                let restore = assembly.rfind(if name == "linux-x86_64" {
                    "add rsp, 16"
                } else { "add sp, sp, #16" }).unwrap();
                assert!(unbox < release && release < restore && restore < branch,
                    "{name}: {assembly}");
            }
            assert_eq!(assembly.matches("numeric_source").count(), 3, "{name}: {assembly}");
        }
    }
}

/// Every supported target accepts only integer, float, and boolean Mixed tags.
#[test]
fn eval_scope_scalar_proof_covers_all_supported_targets() {
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let mut emitter = Emitter::new(Target::parse(target).unwrap());
        emit_branch_if_mixed_numeric_scalar(&mut emitter, "scalar_ok", false);
        let asm = emitter.output();
        assert!(asm.contains("__rt_mixed_unbox"), "{target}: {asm}");
        for tag in [0, 2, 3] {
            let compare = if target == "linux-x86_64" {
                format!("cmp rax, {tag}")
            } else {
                format!("cmp x0, #{tag}")
            };
            assert!(asm.contains(&compare), "{target}: {asm}");
        }
        assert_eq!(asm.matches("scalar_ok").count(), 3, "{target}: {asm}");
    }
}

/// A missing binding never reaches the snapshot retain or unbox used by the numeric fast path.
#[test]
fn eval_scope_numeric_probe_checks_binding_presence_on_every_target() {
    use crate::codegen::{context::FunctionContext, data_section::DataSection, shared_state::SharedCodegenState};
    use crate::{ir::{Builder, Function, IrType, LocalKind, Module, Terminator}, types::PhpType};
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut function = Function::new("scope_presence".into(), IrType::Void, PhpType::Void);
        function.flags.tracks_local_binding_state = true;
        let slot = function.add_local(Some("value".into()), IrType::from_php(&PhpType::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry); builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
        let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
        let presence = layout.binding_state_offsets[&slot];
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                &mut shared, layout, false, false, false, None);
            let local = super::EvalSyncLocal { name: "value".into(), slot, ty: PhpType::Mixed };
            super::promote_unsafe_eval_scope_access_to_full(&mut ctx, &[local], &[]).unwrap();
        }
        let assembly = emitter.output();
        let address = if name == "linux-x86_64" { format!("[rbp - {presence}]") }
            else { format!("[x29, #-{presence}]") };
        let check = assembly.find(&address).unwrap_or_else(|| panic!("{name}: {assembly}"));
        let retain = assembly.find("__rt_incref").unwrap();
        let unbox = assembly.find("__rt_mixed_unbox").unwrap();
        assert!(check < retain && retain < unbox, "{name}: {assembly}");
        let guard = &assembly[check..retain];
        assert!(guard.contains("eval_scope_access_local_type_safe"), "{name}: {assembly}");
    }
}
