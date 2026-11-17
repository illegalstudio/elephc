//! Purpose:
//! Emits resumable memory cleanup scopes around throwing destructors.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

use crate::{abi, Arch, Emitter};

/// Offsets of a sixteen-byte cleanup record in the enclosing native frame.
#[derive(Clone, Copy)]
pub struct Scope { pub arm: usize, pub x86: usize }

impl Scope {
    /// Initializes pending state and suppresses collection until this container is fully released.
    pub fn begin(self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [sp, #{}]", self.arm));  // initialize this frame's pending-exception flag
                abi::emit_load_symbol_to_reg(emitter, "x10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("str x10, [sp, #{}]", self.arm + 8)); // retain the enclosing collector suppression state
                emitter.instruction("mov x10, #1");                             // prevent collection of partially destroyed containers
                abi::emit_store_reg_to_symbol(emitter, "x10", "_gc_release_suppressed", 0);
            },
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", self.x86)); // start this frame with no newly escaping exception
                abi::emit_load_symbol_to_reg(emitter, "r10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", self.x86 - 8)); // retain nested cleanup's incoming suppression state
                emitter.instruction("mov r10, 1");                              // suspend collector runs while child ownership is incomplete
                abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_release_suppressed", 0);
            },
        }
    }

    /// Runs one potentially throwing release with a native or C-ABI unary value argument.
    pub fn call(self, emitter: &mut impl Emitter, target: &str, c_input: bool) {
        self.call_with_address(emitter, target, c_input, false);
    }

    /// Runs a potentially throwing internal helper without routing its local label through the GOT.
    pub fn call_local(self, emitter: &mut impl Emitter, target: &str, c_input: bool) {
        self.call_with_address(emitter, target, c_input, true);
    }

    /// Materializes the selected target and invokes it through the shared protected boundary.
    fn call_with_address(
        self,
        emitter: &mut impl Emitter,
        target: &str,
        c_input: bool,
        local: bool,
    ) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // retain the native child value before materializing its release callback
                if local {
                    abi::emit_local_symbol_address(emitter, "x0", target);
                } else {
                    abi::emit_symbol_address(emitter, "x0", target);
                }
                emitter.instruction(&format!("add x2, sp, #{}", self.arm));     // pass this container's exception state by address
            },
            Arch::X86_64 => {
                emitter.instruction(if c_input { "mov rsi, rdi" } else { "mov rsi, rax" }); // adapt the child's C or native unary value convention
                if local {
                    abi::emit_local_symbol_address(emitter, "rdi", target);
                } else {
                    abi::emit_symbol_address(emitter, "rdi", target);
                }
                emitter.instruction(&format!("lea rdx, [rbp - {}]", self.x86)); // retain pending state in the enclosing deep-free frame
            },
        }
        abi::emit_call_label(emitter, "__rt_cleanup_call");
    }

    /// Restores collector state and leaves the pending flag in the native result register.
    pub fn finish(self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x10, [sp, #{}]", self.arm + 8)); // recover the suppression state of the enclosing release
                abi::emit_store_reg_to_symbol(emitter, "x10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("ldr x0, [sp, #{}]", self.arm));   // retain whether a child exception must propagate after frame teardown
            },
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", self.x86 - 8)); // restore the exact outer collector state
                abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", self.x86)); // preserve the pending flag across the native epilogue
            },
        }
    }
}

mod callback;

pub use callback::emit as emit_deep_cleanup_callback;
