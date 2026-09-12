//! Purpose:
//! Completes one frame-local cleanup while preserving the exception already being unwound.
//!
//! Called from:
//! - Exceptional PHP frame callbacks through the shared typed ABI cleanup helpers.
//!
//! Key details:
//! - C arguments are a unary release entry and its owned payload.
//! - Destructor exceptions join the active chain and never escape this cleanup call.
//! - The integer result reports whether this cleanup caught a new throw.

use crate::codegen_support::{abi, emit::Emitter};

const FRAME: usize = 64;
const ENTRY: usize = 8;
const PAYLOAD: usize = 16;
const PENDING: usize = 24;
const CAUGHT: usize = 32;

/// Emits a bounded release that republishes the accumulated Throwable and returns its caught flag.
pub fn emit_cleanup_preserve_exception(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_cleanup_preserve_exception");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, PAYLOAD].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), ENTRY);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), PAYLOAD);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI republishes the pending chain and returns the independent newly-caught flag.
    #[test]
    fn frame_cleanup_preserves_the_pending_chain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_cleanup_preserve_exception(&mut emitter);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let invoke = asm.find("__rt_cleanup_invoke").unwrap();
            let publish = asm.rfind("_exc_value").unwrap();
            let (caught_store, caught_reload) = match arch {
                crate::codegen_support::platform::Arch::AArch64 => (
                    "stur x0, [x29, #-32]",
                    "ldur x0, [x29, #-32]",
                ),
                crate::codegen_support::platform::Arch::X86_64 => {
                    (
                        "mov QWORD PTR [rbp - 32], rax",
                        "mov rax, QWORD PTR [rbp - 32]",
                    )
                }
            };
            let save = asm.find(caught_store).unwrap();
            let reload = asm.rfind(caught_reload).unwrap();
            assert!(asm.find("_exc_value").unwrap() < invoke, "{name}");
            assert!(invoke < save && save < publish && publish < reload, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
