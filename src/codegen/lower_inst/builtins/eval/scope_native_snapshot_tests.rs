//! Purpose:
//! Verifies that unchanged native snapshots bypass dynamic eval reload on every ABI.
//!
//! Called from:
//! - The target-independent eval emitter regression suite in CI.
//!
//! Key details:
//! - The dirty bit is exact scope state, independent of conservative access-mask collisions.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;

/// Every target branches on the dirty bit before an unchanged snapshot can overwrite native state.
#[test]
fn native_snapshot_reload_checks_actual_scope_writes_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let function = crate::codegen::shared_helper::helper_function("scope_snapshot", PhpType::Void);
        let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                &mut shared, layout, false, false, false, None);
            scope_io::emit_branch_if_scope_entry_clean(&mut ctx, "skip_unchanged");
        }
        let assembly = emitter.output();
        let expected = if name == "linux-x86_64" { "test r10, 4" } else { "tst x10, #4" };
        assert!(assembly.contains(expected), "{name}: {assembly}");
        assert!(assembly.contains("skip_unchanged"), "{name}: {assembly}");
    }
}

/// Reload skips an already shared canonical wrapper before projecting the legacy parameter type.
#[test]
fn native_reference_reload_preserves_shared_wrapper_identity_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = Module::new(target);
        let mut function = crate::codegen::shared_helper::helper_function("scope_reference_reload", PhpType::Void);
        function.params.clear();
        function.params.push(crate::ir::FunctionParam {
            name: "value".into(), ir_type: crate::ir::IrType::I64, php_type: PhpType::Int,
            by_ref: true, variadic: false,
        });
        function.values[0].ir_type = crate::ir::IrType::I64;
        function.values[0].php_type = PhpType::Int;
        let slot = function.add_local(Some("value".into()), crate::ir::IrType::from_php(&PhpType::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        let local = EvalSyncLocal { name: "value".into(), slot, ty: PhpType::Mixed };
        let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                &mut shared, layout, false, false, false, None);
            scope_reload::store_mixed_scope_cell_to_local(&mut ctx, &local, None).unwrap();
        }
        let assembly = emitter.output();
        let shared = assembly.find("eval_ref_cell_reload_shared_wrapper").unwrap();
        let cast = assembly.find("__rt_mixed_cast_int").unwrap();
        assert!(assembly[..shared].contains("__rt_reference_cell_canonical"), "{name}: authenticate C before comparing wrapper identity");
        assert!(shared < cast, "{name}: already committed canonical writes must bypass the legacy projection");
    }
}
