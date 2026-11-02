//! Purpose: retain argument-reader cells until a native eval call has finished.
//! Called from: generated constructor, instance-method and static-method bridges.
//! Key details: zeroed frame slots survive argument staging and native unwinding.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use crate::types::PhpType;

// AArch64 helpers place their handler at FP + 32; x86_64 helpers reserve
// at most 96 bytes above it. Owner slots must not overlap either handler.
const AARCH64_OWNER_OFFSET: usize = 32 + TRY_HANDLER_SLOT_SIZE;
const X86_64_OWNER_PREFIX: usize = 96 + TRY_HANDLER_SLOT_SIZE;

pub(super) fn frame_size(emitter: &Emitter, base_size: usize, count: usize) -> usize {
    let owner_bytes = count * 16;
    match emitter.target.arch {
        Arch::AArch64 => base_size + owner_bytes,
        Arch::X86_64 => X86_64_OWNER_PREFIX + owner_bytes,
    }
}

/// Clears slots before any early dispatch failure can reach the cleanup tail.
pub(super) fn initialize(emitter: &mut Emitter, count: usize) {
    for index in 0..count * 2 {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("str xzr, [x29, #{}]", AARCH64_OWNER_OFFSET + index * 8)), // initialize an unread argument owner to null
            Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", X86_64_OWNER_PREFIX + (index + 1) * 8)), // initialize an unread argument owner to null
        }
    }
}

/// Records the owned reader result before coercion can fail or overwrite it.
pub(super) fn record(emitter: &mut Emitter, index: usize) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("str x0, [x29, #{}]", AARCH64_OWNER_OFFSET + index * 16)), // retain the reader owner until call completion and reference writeback
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", X86_64_OWNER_PREFIX + index * 16 + 8)), // retain the reader owner until call completion and reference writeback
    }
}

/// Tracks a by-value string conversion borrowed by the native callee.
/// Reference argument conversions and builtin property initialization have their
/// own transfer rules and must not register their payload here.
pub(super) fn record_conversion(emitter: &mut Emitter, index: usize, ty: &PhpType) {
    if ty.codegen_repr() != PhpType::Str {
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("str x1, [x29, #{}]", AARCH64_OWNER_OFFSET + index * 16 + 8)), // preserve the separately allocated native string argument for post-call cleanup
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", X86_64_OWNER_PREFIX + (index + 1) * 16)), // preserve the separately allocated native string argument for post-call cleanup
    }
}

/// Constructor casts retain raw heap payloads; ordinary method casts borrow them.
/// Strings use the same independently allocated conversion in both bridges.
pub(super) fn record_constructor_conversion(emitter: &mut Emitter, index: usize, ty: &PhpType) {
    if ty.codegen_repr() == PhpType::Str {
        record_conversion(emitter, index, ty);
    } else if matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("str x0, [x29, #{}]", AARCH64_OWNER_OFFSET + index * 16 + 8)), // preserve the constructor cast's retained raw heap owner for post-call cleanup
            Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", X86_64_OWNER_PREFIX + (index + 1) * 16)), // preserve the constructor cast's retained raw heap owner for post-call cleanup
        }
    }
}

/// Releases reader owners after writeback, preserving the separately owned result.
pub(super) fn release(emitter: &mut Emitter, count: usize) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    for index in 0..count * 2 {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("ldr x0, [x29, #{}]", AARCH64_OWNER_OFFSET + index * 8)), // load the argument reader owner or an untouched null slot
            Arch::X86_64 => emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", X86_64_OWNER_PREFIX + (index + 1) * 8)), // load the argument reader owner or an untouched null slot
        }
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    abi::emit_pop_reg(emitter, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    #[test]
    fn argument_owners_fit_all_supported_target_frames() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for count in [0, 1, 3, 19] {
                let mut emitter = Emitter::new(target);
                for (base_size, fp_offset) in [(80 + TRY_HANDLER_SLOT_SIZE, 48), (96 + TRY_HANDLER_SLOT_SIZE, 64)] {
                    let size = frame_size(&emitter, base_size, count);
                    assert_eq!(size % 16, 0, "{name}");
                    match target.arch {
                        Arch::AArch64 => assert!(fp_offset + AARCH64_OWNER_OFFSET + count * 16 <= size, "{name}"),
                        Arch::X86_64 => assert!(X86_64_OWNER_PREFIX + count * 16 <= size, "{name}"),
                    }
                }
                initialize(&mut emitter, count);
                for index in 0..count {
                    record(&mut emitter, index);
                    record_conversion(&mut emitter, index, &PhpType::Str);
                }
                release(&mut emitter, count);
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_decref_any").count(), count * 2, "{name}: {asm}");
                if count > 0 {
                    match target.arch {
                        Arch::AArch64 => assert!(asm.contains(&format!("ldr x0, [x29, #{}]", AARCH64_OWNER_OFFSET + count * 16 - 8)), "{name}: {asm}"),
                        Arch::X86_64 => assert!(asm.contains(&format!("mov rax, QWORD PTR [rbp - {}]", X86_64_OWNER_PREFIX + count * 16)), "{name}: {asm}"),
                    }
                }
            }
        }
    }
}
