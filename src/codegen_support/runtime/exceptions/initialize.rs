//! Purpose:
//! Initializes Error/Exception constructor fields on compact and ordinary native objects.
//!
//! Called from:
//! - EIR's typed Throwable initialization operation inside builtin constructor methods.
//!
//! Key details:
//! - The five native arguments are borrowed: receiver, message pointer/length, code, previous box.
//! - New field owners are installed before releasing displaced owners, including self aliases.
//! - Previous-slot metadata selects a raw object or a nullable Mixed cell without changing layout.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 96;
const RECEIVER: usize = 8;
const MESSAGE: usize = 16;
const MESSAGE_LEN: usize = 24;
const CODE: usize = 32;
const PREVIOUS: usize = 40;
const OLD_MESSAGE: usize = 48;
const OLD_PREVIOUS: usize = 56;
const PREVIOUS_SLOT: usize = 64;
const NEW_PREVIOUS: usize = 72;

/// Emits a layout-aware initializer for a checked Throwable receiver and normalized parameters.
pub fn emit_throwable_initialize(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let object = abi::symbol_scratch_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let (string, length) = abi::string_result_regs(emitter);

    emitter.blank();
    emitter.label_global("__rt_throwable_initialize");
    // -- preserve the borrowed constructor inputs before acquiring replacement field owners --
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [RECEIVER, MESSAGE, MESSAGE_LEN, CODE, PREVIOUS].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, RECEIVER);
    abi::emit_call_label(emitter, "__rt_throwable_previous_slot");
    abi::store_at_offset(emitter, result, PREVIOUS_SLOT);
    abi::emit_load_from_address(emitter, scratch, result, 0);
    abi::store_at_offset(emitter, scratch, OLD_PREVIOUS);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbnz x1, __rt_throwable_initialize_boxed");    // ordinary properties retain the normalized Mixed parameter
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // inspect the previous slot's physical representation
            emitter.instruction("jnz __rt_throwable_initialize_boxed");         // boxed slots must not receive a raw Throwable pointer
        }
    }
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #6");                                  // a normalized nullable Throwable is either an object or null
            emitter.instruction("csel x0, x1, xzr, eq");                        // compact storage owns the object directly or canonical zero
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 6");                                  // a normalized nullable Throwable is either an object or null
            emitter.instruction("mov eax, 0");                                  // seed the raw-null representation without changing flags
            emitter.instruction("cmove rax, rdi");                              // compact storage owns the object directly when present
        }
    }
    abi::emit_jump(emitter, "__rt_throwable_initialize_retain");
    emitter.label("__rt_throwable_initialize_boxed");
    abi::load_at_offset(emitter, result, PREVIOUS);
    emitter.label("__rt_throwable_initialize_retain");
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, NEW_PREVIOUS);

    // -- publish every replacement before a displaced previous object can run PHP code --
    abi::load_at_offset(emitter, string, MESSAGE);
    abi::load_at_offset(emitter, length, MESSAGE_LEN);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::load_at_offset(emitter, object, RECEIVER);
    abi::emit_load_from_address(emitter, scratch, object, 8);
    abi::store_at_offset(emitter, scratch, OLD_MESSAGE);
    abi::emit_store_to_address(emitter, string, object, 8);
    abi::emit_store_to_address(emitter, length, object, 16);
    abi::load_at_offset(emitter, scratch, CODE);
    abi::emit_store_to_address(emitter, scratch, object, 24);
    abi::load_at_offset(emitter, object, PREVIOUS_SLOT);
    abi::load_at_offset(emitter, result, NEW_PREVIOUS);
    abi::emit_store_to_address(emitter, result, object, 0);
    abi::load_at_offset(emitter, result, OLD_MESSAGE);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    abi::load_at_offset(emitter, result, OLD_PREVIOUS);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target stages the five ABI words and publishes replacement owners before old cleanup.
    #[test]
    fn throwable_initializer_preserves_layout_and_owner_order_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_throwable_initialize(&mut emitter);
            let asm = emitter.output();
            let previous_argument = match target.arch {
                Arch::AArch64 => "stur x4, [x29, #-40]",
                Arch::X86_64 => "mov QWORD PTR [rbp - 40], r8",
            };
            let publish_previous = match target.arch {
                Arch::AArch64 => "str x0, [x9]",
                Arch::X86_64 => "mov QWORD PTR [r11], rax",
            };
            assert!(asm.contains(previous_argument), "{name}: {asm}");
            assert!(asm.contains("__rt_throwable_previous_slot"), "{name}");
            let retain = asm.find("__rt_incref").unwrap();
            let publish = asm.find(publish_previous).unwrap();
            let release_message = asm.find("__rt_heap_free_safe").unwrap();
            let release_previous = asm.find("__rt_decref_any").unwrap();
            assert!(retain < publish && publish < release_message && release_message < release_previous, "{name}");
        }
    }
}
