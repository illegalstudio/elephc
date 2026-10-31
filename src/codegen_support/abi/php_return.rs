//! Purpose:
//! Emits the private compiled PHP return ownership marker on every supported target.
//!
//! Called from:
//! - EIR return lowering, compiled call result boxing and method ABI adapters.
//!
//! Key details:
//! - The marker is caller-saved and must survive adapter cleanup independently of the payload.
//! - Native C intrinsics do not participate in this protocol.

use super as abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Publishes whether this exact EIR return path transfers its result owner.
pub(crate) fn emit_php_return_status(emitter: &mut Emitter, owned: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(if owned {                                      // publish the exact EIR return-path ownership status
                "mov x15, #1"
            } else {
                "mov x15, xzr"
            });
        }
        Arch::X86_64 => {
            emitter.instruction(if owned {                                      // publish the exact EIR return-path ownership status
                "mov r11d, 1"
            } else {
                "xor r11d, r11d"
            });
        }
    }
}

/// Preserves the internal marker before argument cleanup can issue another call.
pub(crate) fn emit_store_php_return_status(emitter: &mut Emitter, offset: usize) {
    let status_reg = status_reg(emitter.target.arch);
    abi::store_at_offset(emitter, status_reg, offset);
}

/// Restores a previously preserved internal marker.
pub(crate) fn emit_load_php_return_status(emitter: &mut Emitter, offset: usize) {
    let status_reg = status_reg(emitter.target.arch);
    abi::load_at_offset(emitter, status_reg, offset);
}

/// Branches when the current compiled PHP return transfers an existing owner.
pub(crate) fn emit_branch_if_php_return_owned(emitter: &mut Emitter, owned_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(                                       // transfer a result whose EIR return path already owns its storage
                "cbnz x15, {}",
                owned_label
            ));
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // inspect the per-return EIR ownership status from the callable entry
            emitter.instruction(&format!("jne {}", owned_label));               // transfer a result whose EIR return path already owns its storage
        }
    }
}

/// Returns the private caller-saved register used by the compiled PHP ABI.
fn status_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "x15",
        Arch::X86_64 => "r11",
    }
}
