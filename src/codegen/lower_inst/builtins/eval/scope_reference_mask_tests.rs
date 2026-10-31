//! Purpose:
//! Verifies dynamic eval uses the original parsed name mask for live native aliases.
//!
//! Called from:
//! - The target-independent eval emitter regression suite.
//!
//! Key details:
//! - Ordinary locals keep the conservative mask used for heap synchronization.
//! - By-reference storage must not reload a stale snapshot when the name was unaccessed.

use super::*;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Target;
use crate::codegen::shared_state::SharedCodegenState;

/// Alias guards read the preserved mask while ordinary locals read the working mask on every ABI.
#[test]
fn native_alias_scope_guards_preserve_the_parsed_mask_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for reference in [false, true] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut function = crate::codegen::shared_helper::helper_function("scope_ref_mask", PhpType::Void);
            function.params[0].by_ref = reference;
            let slot = function.add_local(Some("value".into()),
                crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed), PhpType::Mixed, LocalKind::PhpLocal);
            let local = EvalSyncLocal { name: "value".into(), slot, ty: PhpType::Mixed };
            let layout = crate::codegen::frame::layout_for_function(&function, target, false, false, false);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let mut shared = SharedCodegenState::default();
            {
                let mut ctx = FunctionContext::new(&module, &function, &mut emitter, &mut data,
                    &mut shared, layout, false, false, false, None);
                scope_context::emit_branch_if_eval_local_unaccessed(&mut ctx, &local,
                    EVAL_SCOPE_ACCESS_MASK_OFFSET, "unaccessed").unwrap();
            }
            let offset = if reference { EVAL_REFERENCE_ACCESS_MASK_OFFSET } else { EVAL_SCOPE_ACCESS_MASK_OFFSET };
            let expected = if name == "linux-x86_64" { format!("[rsp + {offset}]") }
                else { format!("[sp, #{offset}]") };
            let assembly = emitter.output();
            assert!(assembly.contains(&expected), "{name}, reference={reference}: {assembly}");
            assert!(assembly.contains("unaccessed"), "{name}, reference={reference}: {assembly}");
        }
    }
}
