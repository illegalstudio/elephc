//! Purpose:
//! Materializes the current value behind a private invoker reference argument marker.
//!
//! Called from:
//! - Descriptor argument binding and the AOT backtrace named-argument reader.
//!
//! Key details:
//! - The marker supplies a storage address and a source representation tag, not a PHP value.
//! - Boxed sources are retained; raw sources are boxed with the shared target-aware ABI.
//! - The caller owns the returned Mixed cell and must clone it for an independent PHP snapshot.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, value_boxing};
use crate::types::PhpType;

/// Returns one owned Mixed value read from a marker's borrowed source storage.
pub(crate) fn emit_materialize_ref_cell_value_as_mixed(
    ref_cell_reg: &str,
    source_tag_reg: &str,
    emitter: &mut Emitter,
    next_label: &mut dyn FnMut(&str) -> String,
) {
    let result_reg = abi::int_result_reg(emitter);
    let lo_reg = abi::tertiary_scratch_reg(emitter);
    let hi_reg = match emitter.target.arch {
        Arch::AArch64 => "x12",
        Arch::X86_64 => "rdx",
    };
    let mixed_label = next_label("invoker_ref_mixed");
    let string_hi_label = next_label("invoker_ref_string_hi");
    let box_label = next_label("invoker_ref_box");
    let done_label = next_label("invoker_ref_materialized");

    abi::emit_load_from_address(emitter, lo_reg, ref_cell_reg, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #7", source_tag_reg));        // detect an already-boxed Mixed value in the reference cell
            emitter.instruction(&format!("b.eq {}", mixed_label));              // retain the existing Mixed cell instead of nesting it
            abi::emit_load_int_immediate(emitter, hi_reg, 0);
            emitter.instruction(&format!("cmp {}, #1", source_tag_reg));        // is the referenced value a string (runtime tag 1)?
            emitter.instruction(&format!("b.eq {}", string_hi_label));          // strings also need the high length word loaded
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 7", source_tag_reg));         // detect an already-boxed Mixed value in the reference cell
            emitter.instruction(&format!("je {}", mixed_label));                // retain the existing Mixed cell instead of nesting it
            abi::emit_load_int_immediate(emitter, hi_reg, 0);
            emitter.instruction(&format!("cmp {}, 1", source_tag_reg));         // is the referenced value a string (runtime tag 1)?
            emitter.instruction(&format!("je {}", string_hi_label));            // strings also need the high length word loaded
        }
    }
    abi::emit_jump(emitter, &box_label);
    emitter.label(&string_hi_label);
    abi::emit_load_from_address(emitter, hi_reg, ref_cell_reg, 8);
    emitter.label(&box_label);
    value_boxing::emit_box_runtime_payload_as_mixed(emitter, source_tag_reg, lo_reg, hi_reg);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&mixed_label);
    emitter.instruction(&format!("mov {}, {}", result_reg, lo_reg));            // reuse the referenced Mixed cell without adding a wrapper layer
    abi::emit_incref_if_refcounted(emitter, &PhpType::Mixed);
    emitter.label(&done_label);
}
