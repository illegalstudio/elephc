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
