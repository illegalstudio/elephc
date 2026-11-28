//! Purpose:
//! Supplies the PHP scalar tags admitted by Memoir's native cleanup boundary.
//!
//! Called from:
//! - The compiler exception runtime orchestrator.
//!
//! Key details:
//! - Pending exception ownership and call marshalling live in Memoir.

use crate::codegen_support::{emit::Emitter, value_boxing::runtime_value_tag};
use crate::types::PhpType;
use elephc_memoir::ValueTag;

pub(crate) use elephc_memoir::cleanup::invoke::emit_guarded_cleanup_call;

/// Emits the native cleanup boundary with the compiler's non-owning scalar tags.
pub fn emit_cleanup_invoke(emitter: &mut Emitter) {
    let tags = [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Void]
        .map(|ty| ValueTag::new(runtime_value_tag(&ty)));
    elephc_memoir::cleanup::invoke::emit_cleanup_invoke(emitter, &tags);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{abi, platform::{Arch, Target}};
    use elephc_memoir::cleanup_fast_paths::{empty_array, scalar_mixed};

    /// Cleanup boundaries and throws use the same mask-free libc jump family on every target.
    #[test]
    fn cleanup_jump_boundaries_do_not_save_or_restore_signal_masks() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for pic in [false, true] {
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                crate::codegen_support::runtime::exceptions::emit_throw_current(&mut emitter);
                let asm = emitter.output();
                let call = if target.arch == Arch::AArch64 { "bl" } else { "call" };
                for symbol in ["_setjmp", "_longjmp"] {
                    assert!(asm.contains(&format!("{call} {}\n", target.extern_symbol(symbol))),
                        "{name}, pic={pic}: missing mask-free {symbol}");
                }
                for symbol in ["setjmp", "longjmp"] {
                    assert!(!asm.contains(&format!("{call} {}\n", target.extern_symbol(symbol))),
                        "{name}, pic={pic}: internal boundaries must not use mask-saving {symbol}");
                }
            }
        }
    }

    /// Every supported target captures callback exceptions and restores native state before returning.
    #[test]
    fn cleanup_invocation_is_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_cleanup_invoke(&mut emitter);
            let asm = emitter.output();
            let enter = asm.find(&target.extern_symbol("_setjmp")).unwrap();
            let invoke = asm.find(if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" }).unwrap();
            let caught = asm.find("__rt_cleanup_invoke_caught:").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            assert!(enter < invoke && invoke < caught && caught < chain, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
            assert!(asm.matches("_exc_handler_top").count() >= 3, "{name}");
            let caught_flag = match target.arch {
                Arch::AArch64 => "ldur x0, [x29, #-40]",
                Arch::X86_64 => "mov rax, QWORD PTR [rbp - 40]",
            };
            assert!(asm.contains(caught_flag), "{name}: return a separate new-throw flag");
        }
    }

    /// Validated scalar cells bypass handlers with identical proof gates in direct and PIC code.
    #[test]
    fn scalar_mixed_cleanup_gate_covers_all_targets_and_pic_modes() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let (fast, protected) = asm.split_once("__rt_cleanup_invoke_protected:").unwrap();
                assert!(fast.contains("__rt_decref_any"), "{name}, pic={pic}");
                assert!(fast.contains("_heap_buf") && fast.contains("_heap_off"), "{name}, pic={pic}");
                assert!(!fast.contains("_exc_value") && !fast.contains("_exc_handler_top"), "{name}, pic={pic}");
                assert!(!fast.contains("setjmp"), "{name}, pic={pic}");
                let callback = if target.arch == Arch::AArch64 {
                    ("mov x9, x0", "blr x9")
                } else {
                    ("mov r11, rdi", "call r11")
                };
                assert!(fast.contains(callback.0) && fast.contains(callback.1),
                    "{name}, pic={pic}: invoke the authenticated callback pointer unchanged");
                assert!(protected.contains(&target.extern_symbol("_setjmp")), "{name}, pic={pic}");
                assert!(protected.contains("__rt_cleanup_invoke_caught"), "{name}, pic={pic}");
                let (tag, branch) = if target.arch == Arch::AArch64 { ("cmp x10, #", "b.eq") }
                    else { ("cmp r10, ", "je") };
                for ty in [crate::types::PhpType::Int, crate::types::PhpType::Float,
                    crate::types::PhpType::Bool, crate::types::PhpType::Void] {
                    let value = crate::codegen_support::value_boxing::runtime_value_tag(&ty);
                    assert!(fast.contains(&format!("{tag}{value}")), "{name}, pic={pic}: {ty:?}");
                }
                assert_eq!(fast.matches(&format!("{branch} __rt_cleanup_invoke_direct")).count(), 4,
                    "{name}, pic={pic}: only four scalar tags qualify");
            }
        }
    }
    /// Typed empty arrays qualify by live storage and length, even with reserved capacity.
    #[test]
    fn empty_array_cleanup_gate_covers_all_targets_and_pic_modes() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let (_, probe) = asm.split_once(&format!("{}:", empty_array::CANDIDATE)).unwrap();
                assert!(probe.contains("__rt_decref_array"), "{name}, pic={pic}");
                assert!(probe.contains("_heap_buf") && probe.contains("_heap_off"), "{name}, pic={pic}");
                assert!(!probe.contains("_exc_value") && !probe.contains("setjmp"), "{name}, pic={pic}");
                assert!(probe.contains(scalar_mixed::PROTECTED) && probe.contains(scalar_mixed::DIRECT),
                    "{name}, pic={pic}");
                let required: &[&str] = if target.arch == Arch::AArch64 {
                    &["ldr w10, [x1, #-12]", "cmp w10, #24", "cmp x10, #2",
                        "ldr x10, [x1]", "cbnz x10, __rt_cleanup_invoke_protected"]
                } else {
                    &["cmp DWORD PTR [rsi - 12], 0", "cmp DWORD PTR [rsi - 16], 24",
                        "cmp r10d, 2", "cmp QWORD PTR [rsi], 0", "jne __rt_cleanup_invoke_protected"]
                };
                for instruction in required {
                    assert!(probe.contains(instruction), "{name}, pic={pic}: {instruction}");
                }
                assert!(!probe.contains("[x1, #8]") && !probe.contains("[rsi + 8]"),
                    "{name}, pic={pic}: reserved capacity is not logical child ownership");
            }
        }
    }

    /// Both ARM64 probes must retain the heap base until the upper bound rejects foreign pointers.
    #[test]
    fn cleanup_arm64_bounds_preserve_base_before_header_reads() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            for pic in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_cleanup_invoke(&mut emitter);
                let asm = emitter.output();
                let scalar = asm.split_once("__rt_cleanup_invoke_direct:").unwrap().0;
                let array = asm.split_once(&format!("{}:", empty_array::CANDIDATE)).unwrap().1;

                // -- resolving the bump offset must not overwrite the live base in x9 --
                let mut bounds = Emitter::new(target);
                bounds.pic_data_refs = pic;
                abi::emit_symbol_address(&mut bounds, "x10", "_heap_off");
                bounds.instruction("ldr x10, [x10]");                           // read the bump offset through its own address register
                bounds.instruction("add x10, x9, x10");                         // combine the offset with the preserved heap base
                bounds.instruction("cmp x1, x10");                              // bound the payload before any header access
                bounds.instruction("b.hs __rt_cleanup_invoke_protected");        // foreign pointers must take the protected fallback
                elephc_memoir::heap_refcount::emit_load_owner_count(
                    &mut bounds,
                    "w10",
                    "x1",
                );
                let required = bounds.output();
                for (kind, probe) in [("scalar", scalar), ("empty array", array)] {
                    assert!(probe.contains(&required),
                        "{name}, pic={pic}, {kind}: preserve x9 and reject the heap end before reading the header\n{probe}");
                }
            }
        }
    }

}
