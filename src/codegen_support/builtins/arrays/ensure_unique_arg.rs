//! Purpose:
//! Emits copy-on-write uniqueness checks for array arguments before in-place mutation.
//! Centralizes the COW guard shared by PHP mutating array builtins.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::*::emit() for mutating array builtins`.
//!
//! Key details:
//! - The array pointer may change after uniqueness repair, so callers must store it back when mutating by reference.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::PhpType;

/// Emits a copy-on-write uniqueness check for array arguments before in-place mutation.
///
/// On ARM64: uses `bl` to call `__rt_array_ensure_unique` or `__rt_hash_ensure_unique`.
/// On x86_64: moves the array pointer from `rax` into `rdi` (first argument register), then calls the runtime function.
///
/// After the call, the array pointer in `rax` may have changed due to COW repair. Callers
/// must store the returned pointer back when mutating by reference.
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
