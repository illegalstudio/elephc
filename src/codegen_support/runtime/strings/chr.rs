//! Purpose:
//! Emits the runtime conversion of a byte-valued integer to a PHP string.
//!
//! Called from:
//! - Direct PHP lowering and the eval builtin dispatcher.
//!
//! Key details:
//! - Memoir reserves and publishes the result storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::strings::staged_concat::emit_reserve_byte_for_integer;

/// Converts the low byte of the incoming integer and returns a one-byte string.
pub fn emit_chr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label_global("__rt_chr");

    emit_reserve_byte_for_integer(emitter, "__rt_chr_reserved");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("strb w0, [x1]");                               // store the low byte as the string
            emitter.instruction("mov x2, #1");                                  // return the initialized length
        }
        Arch::X86_64 => {
            emitter.instruction("mov BYTE PTR [rax], dil");                     // store the low byte as the string
            emitter.instruction("mov edx, 1");                                  // return the initialized length
        }
    }
    abi::emit_jump(emitter, "__rt_concat_publish");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target bounds the byte store with the shared reservation and publishes the result.
    #[test]
    fn chr_reserves_before_writing_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_chr(&mut emitter);
            let output = emitter.output();
            let reserve = output.find("__rt_concat_reserve").unwrap();
            let write = output.find(if name == "linux-x86_64" {
                "mov BYTE PTR [rax], dil"
            } else {
                "strb w0, [x1]"
            }).unwrap();
            let publish = output.find("__rt_concat_publish").unwrap();
            assert!(reserve < write && write < publish, "{name}");
        }
    }
}
