use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

pub(crate) fn emit_ensure_unique_arg(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Array(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("bl __rt_array_ensure_unique");         // split shared indexed arrays before a mutating builtin runs
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the candidate indexed-array pointer into the first x86_64 runtime argument register
                    abi::emit_call_label(emitter, "__rt_array_ensure_unique");  // split shared indexed arrays before a mutating builtin runs
                }
            }
        }
        PhpType::AssocArray { .. } => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("bl __rt_hash_ensure_unique");          // split shared associative arrays before a mutating builtin runs
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the candidate associative-array pointer into the first x86_64 runtime argument register
                    abi::emit_call_label(emitter, "__rt_hash_ensure_unique");   // split shared associative arrays before a mutating builtin runs
                }
            }
        }
        _ => {}
    }
}
