//! Purpose:
//! Retires an independently owned helper payload when the host mode flag is set.
//!
//! Called from:
//! - Compiler container-write failure adapters before their pending error is propagated.
//!
//! Key details:
//! - The host supplies physical slot offsets and the extra-owner flag bit.
//! - Cleanup preserves the enclosing exception and leaves the original storage owner intact.

use crate::{abi, Arch, Emitter, FrameOffset, RecordOffset};

/// Host layout of the mode word and independently owned payload on both native ABIs.
#[derive(Clone, Copy)]
pub struct Layout {
    /// AArch64 mode-word offset from the current stack pointer.
    pub aarch64_mode: RecordOffset,
    /// AArch64 owned-payload offset from the current stack pointer.
    pub aarch64_payload: RecordOffset,
    /// x86_64 mode-word offset below the frame pointer.
    pub x86_64_mode: FrameOffset,
    /// x86_64 owned-payload offset below the frame pointer.
    pub x86_64_payload: FrameOffset,
    /// Bit selecting independent ownership, restricted to 0..=30 for both target encodings.
    pub owner_bit: u8,
}

/// Converts a host ownership flag into an immediate supported by both native encodings.
fn owner_mask(bit: u8) -> u32 {
    assert!(bit < 31, "owner flag must fit a positive x86_64 test immediate");
    1_u32 << bit
}

/// Emits conditional retirement while preserving the host's already pending exception.
pub fn emit(emitter: &mut impl Emitter, layout: Layout) {
    let mask = owner_mask(layout.owner_bit);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{}]", layout.aarch64_mode.bytes())); // recover the host mode flags
            emitter.instruction(&format!("tbz x9, #{}, 986f", layout.owner_bit)); // keep the original storage owner when no extra owner was acquired
            let payload = if layout.aarch64_payload.bytes() == 0 { "[sp]".to_owned() }
                else { format!("[sp, #{}]", layout.aarch64_payload.bytes()) };
            emitter.instruction(&format!("ldr x0, {payload}"));                 // retire only the separately acquired payload owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test QWORD PTR [rbp - {}], {mask}", layout.x86_64_mode.bytes())); // inspect independent ownership without changing other mode bits
            emitter.instruction("jz 986f");                                     // leave the caller's surviving owner untouched
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", layout.x86_64_payload.bytes())); // load the independently owned payload
        }
    }
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_any", abi::int_result_reg(emitter));
    emitter.label("986");
}

#[cfg(test)]
mod tests {
    use super::owner_mask;

    /// The common and highest supported ownership bits retain their exact positive mask.
    #[test]
    fn conditional_owner_mask_preserves_supported_bits() {
        assert_eq!(owner_mask(0), 1);
        assert_eq!(owner_mask(1), 2);
        assert_eq!(owner_mask(30), 1 << 30);
    }

    /// A flag outside the shared immediate range is rejected before emitting invalid x86_64 code.
    #[test]
    #[should_panic(expected = "owner flag must fit a positive x86_64 test immediate")]
    fn conditional_owner_mask_rejects_unencodable_bit() {
        let _ = owner_mask(31);
    }
}
