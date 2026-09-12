//! Purpose:
//! Adapts parsed hydration data to the actual native parameter ABI of `__unserialize`.
//!
//! Called from:
//! - The target-specific recursive unserialize decoders after parsing the data hash.
//!
//! Key details:
//! - The decoder owns one data slot, containing either its original hash or an acquired box.
//! - The parser context retains hydration data until all wire back-references have been decoded.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Invokes the hydration hook after transferring its typed data argument to parser-context ownership.
pub(super) fn emit_unserialize_magic_call(emitter: &mut Emitter) {
    let (object, data, method, temporary) = match emitter.target.arch {
        Arch::AArch64 => (88, 32, 40, 56),
        Arch::X86_64 => (32, 80, 72, 64),
    };
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.comment("adapt and root the unserialize data owner before calling PHP");
    abi::load_at_offset(emitter, result, object);
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::emit_symbol_address(emitter, result, "_class_unserialize_data_boxed");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x0, x10, lsl #3]");                   // select the checked data-parameter representation for this class
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rax + r10*8]");            // select the checked data-parameter representation for this class
        }
    }
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unser_magic_data_ready");
    let (lo, hi) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::load_at_offset(emitter, lo, data);
    abi::emit_load_int_immediate(emitter, hi, 0);
    abi::emit_load_int_immediate(emitter, result, 5);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, temporary);
    abi::load_at_offset(emitter, result, data);
    abi::emit_call_label(emitter, "__rt_decref_hash");
    abi::load_at_offset(emitter, result, temporary);
    abi::store_at_offset(emitter, result, data);
    emitter.label("__rt_unser_magic_data_ready");
    abi::load_at_offset(emitter, result, data);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), object);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), data);
    abi::emit_store_zero_to_local_slot(emitter, data);
    abi::load_at_offset(emitter, scratch, method);
    abi::emit_call_reg(emitter, scratch);
    emitter.comment("unserialize data remains owned until the parser context completes");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI boxes before publication and transfers data ownership before invoking PHP.
    #[test]
    fn hydration_data_adaptation_and_ownership_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_unserialize_magic_call(&mut emitter);
            let asm = emitter.output();
            let boxed = asm.find("__rt_mixed_from_value").unwrap();
            let rooted = asm.find("__rt_unserialize_defer_data").unwrap();
            let invoked = asm.find(if name == "linux-x86_64" { "call r10" } else { "blr x10" }).unwrap();
            assert!(boxed < rooted && rooted < invoked, "{name}: {asm}");
            let clear = if name == "linux-x86_64" { "mov QWORD PTR [rbp - 80], 0" } else { "stur xzr, [x29, #-32]" };
            assert!(asm[rooted..invoked].contains(clear), "{name}: {asm}");
            assert!(!asm.contains("__rt_decref_any"), "{name}: keep back-reference targets alive");
        }
    }
}
