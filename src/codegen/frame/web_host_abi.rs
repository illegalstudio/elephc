//! Purpose:
//! Extends web-handler frames with the complete host callee-saved register set.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_main_function()` for web handlers.
//!
//! Key details:
//! - Runtime scratch registers extend beyond the register allocator's pools.
//! - Existing save slots are reused, and extra slots follow the original frame storage.
//! - AArch64 requires only the low 64 bits of each preserved vector register.

use super::{align_to_16, FrameLayout, FRAME_FOOTER_BYTES};
use crate::codegen::platform::{Arch, Target};

/// Reserves host register saves without overlapping value, local, or activation slots.
pub(in crate::codegen) fn preserve_host_callee_saved(layout: &mut FrameLayout, target: Target) {
    let registers: &[&'static str] = match target.arch {
        Arch::X86_64 => &["rbx", "r12", "r13", "r14", "r15"],
        Arch::AArch64 => &[
            "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28",
            "d8", "d9", "d10", "d11", "d12", "d13", "d14", "d15",
        ],
    };
    let mut offset = layout.frame_size - FRAME_FOOTER_BYTES;
    for &register in registers {
        if !layout.callee_saved_offsets.iter().any(|(saved, _)| *saved == register) {
            offset += 8;
            layout.callee_saved_offsets.push((register, offset));
        }
    }
    layout.frame_size = align_to_16(offset + FRAME_FOOTER_BYTES);
}
