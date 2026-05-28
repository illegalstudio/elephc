//! Purpose:
//! Defines target register names, argument cursors, and outgoing argument assignment planning.
//! Abstracts integer, float, string-pair, scratch, and result-register conventions.
//!
//! Called from:
//! - `crate::codegen::abi` helpers and all higher-level emitters using ABI registers
//!
//! Key details:
//! - Register allocation here models platform ABI limits and must stay in sync with call stack spill logic.

use crate::codegen::{
    emit::Emitter,
    platform::{Arch, Platform, Target},
};
use crate::types::PhpType;

const MAX_INT_ARG_REGS: usize = 8;
const MAX_FLOAT_ARG_REGS: usize = 8;
const CALLER_STACK_START_OFFSET: usize = 32;
/// Sentinel value indicating an outgoing argument is passed on the caller stack rather than in a register.
pub(crate) const STACK_ARG_SENTINEL: usize = usize::MAX;

/// Returns the maximum number of integer arguments that can be passed in registers for the target ABI.
/// AArch64: 8 (x0–x7). x86_64: 6 (rdi, rsi, rdx, rcx, r8, r9).
pub(crate) fn int_arg_reg_limit(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => MAX_INT_ARG_REGS,
        Arch::X86_64 => 6,
    }
}

/// Returns the maximum number of float arguments that can be passed in registers for the target ABI.
/// Both AArch64 and x86_64 support 8 float registers (d0–d7 and xmm0–xmm7 respectively).
pub(crate) fn float_arg_reg_limit(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => MAX_FLOAT_ARG_REGS,
        Arch::X86_64 => MAX_FLOAT_ARG_REGS,
    }
}

/// Returns the frame-pointer offset where the caller's outgoing stack arguments begin.
/// AArch64 call sites keep a 16-byte nested-call save slot above outgoing stack args;
/// after the callee saves x29/x30, the first stack arg is therefore at x29+32.
/// x86_64 reaches the first stack arg at rbp+16 after call pushes the return address.
pub(crate) fn caller_stack_start_offset(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => CALLER_STACK_START_OFFSET,
        Arch::X86_64 => 16,
    }
}

/// Returns the register name for the `idx`-th integer argument register.
/// Panics if `idx >= int_arg_reg_limit(target)`.
pub(crate) fn int_arg_reg_name(target: Target, idx: usize) -> &'static str {
    match target.arch {
        Arch::AArch64 => ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"][idx],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx", "r8", "r9"][idx],
    }
}

/// Returns the register name for the `idx`-th float argument register.
/// Panics if `idx >= float_arg_reg_limit(target)`.
pub(crate) fn float_arg_reg_name(target: Target, idx: usize) -> &'static str {
    match target.arch {
        Arch::AArch64 => ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"][idx],
        Arch::X86_64 => ["xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7"][idx],
    }
}

/// Returns the register used to hold integer/pointer function results.
/// AArch64: x0. x86_64: rax.
pub(crate) fn int_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rax",
    }
}

/// Returns the register used to hold float/double function results.
/// AArch64: d0. x86_64: xmm0.
pub(crate) fn float_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "d0",
        Arch::X86_64 => "xmm0",
    }
}

/// Returns the pair of registers used to hold a string result (pointer, length).
/// AArch64: (x1, x2). x86_64: (rax, rdx).
pub(crate) fn string_result_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

/// Returns the frame pointer register for the target.
/// AArch64: x29 (used as platform register). x86_64: rbp.
pub(crate) fn frame_pointer_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x29",
        Arch::X86_64 => "rbp",
    }
}

/// Returns a callee-saved scratch register safe for symbol address materialization.
/// AArch64: x9. x86_64: r11 ( caller-saved on x86_64, but used as scratch here).
pub(crate) fn symbol_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r11",
    }
}

/// Returns a secondary scratch register for temporary use during code generation.
/// AArch64: x10. x86_64: r10.
pub(crate) fn secondary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

/// Returns a tertiary scratch register for temporary use during code generation.
/// AArch64: x11. x86_64: rcx.
pub(crate) fn tertiary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x11",
        Arch::X86_64 => "rcx",
    }
}

/// Returns a temporary integer register for general intermediate computations.
/// AArch64: x10. x86_64: r10.
pub fn temp_int_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

/// Returns the register holding argc (argument count) on entry to `main`.
/// AArch64: x0. x86_64: rdi.
pub fn process_argc_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rdi",
    }
}

/// Returns the register holding argv (argument vector pointer) on entry to `main`.
/// AArch64: x1. x86_64: rsi.
pub fn process_argv_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rsi",
    }
}

/// Returns a callee-saved register used to preserve the frame pointer across nested calls.
/// AArch64: x19. x86_64: r12.
pub fn nested_call_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    }
}

/// Returns true if `reg` is a floating-point register name (d0–d7 or xmm0–xmm7).
pub(crate) fn is_float_register(reg: &str) -> bool {
    reg.starts_with('d') || reg.starts_with("xmm")
}

/// Tracks the next available integer/float registers and stack offset when receiving arguments in a function prologue.
/// `int_stack_only` is set to true when the callee receives all arguments from the caller stack (e.g., variadic functions).
/// `float_stack_only` is reserved for calls that pass floats only on the stack.
#[derive(Debug, Clone, Copy)]
pub struct IncomingArgCursor {
    pub(crate) int_reg_idx: usize,
    pub(crate) float_reg_idx: usize,
    pub(crate) caller_stack_offset: usize,
    pub(crate) int_stack_only: bool,
    pub(crate) float_stack_only: bool,
}

impl IncomingArgCursor {
    /// Creates a cursor for a callee receiving `initial_int_reg_idx` integer register arguments.
    pub fn new(initial_int_reg_idx: usize) -> Self {
        Self::for_target(Target::new(Platform::MacOS, Arch::AArch64), initial_int_reg_idx)
    }

    /// Creates a cursor for the given target and initial integer register index.
    /// Sets `int_stack_only` when `initial_int_reg_idx` exceeds the target's integer arg register limit.
    pub fn for_target(target: Target, initial_int_reg_idx: usize) -> Self {
        Self {
            int_reg_idx: initial_int_reg_idx,
            float_reg_idx: 0,
            caller_stack_offset: caller_stack_start_offset(target),
            int_stack_only: initial_int_reg_idx >= int_arg_reg_limit(target),
            float_stack_only: false,
        }
    }
}

impl Default for IncomingArgCursor {
    /// Default cursor starts at integer register 0 with default AArch64 target.
    fn default() -> Self {
        Self::new(0)
    }
}

/// Describes where a single outgoing argument is placed: register or stack.
#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingArgAssignment {
    pub ty: PhpType,
    pub start_reg: usize,
    pub is_float: bool,
}

impl OutgoingArgAssignment {
    /// Returns true if the argument is passed in a register (not on the caller stack).
    pub(crate) fn in_register(&self) -> bool {
        self.start_reg != STACK_ARG_SENTINEL
    }
}
