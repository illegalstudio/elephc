use crate::codegen::{
    emit::Emitter,
    platform::{Arch, Platform, Target},
};
use crate::types::PhpType;

const MAX_INT_ARG_REGS: usize = 8;
const MAX_FLOAT_ARG_REGS: usize = 8;
const CALLER_STACK_START_OFFSET: usize = 32;
pub(crate) const STACK_ARG_SENTINEL: usize = usize::MAX;

pub(crate) fn int_arg_reg_limit(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => MAX_INT_ARG_REGS,
        Arch::X86_64 => 6,
    }
}

pub(crate) fn float_arg_reg_limit(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => MAX_FLOAT_ARG_REGS,
        Arch::X86_64 => MAX_FLOAT_ARG_REGS,
    }
}

pub(crate) fn caller_stack_start_offset(target: Target) -> usize {
    match target.arch {
        Arch::AArch64 => CALLER_STACK_START_OFFSET,
        Arch::X86_64 => 16,
    }
}

pub(crate) fn int_arg_reg_name(target: Target, idx: usize) -> &'static str {
    match target.arch {
        Arch::AArch64 => ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"][idx],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx", "r8", "r9"][idx],
    }
}

pub(crate) fn float_arg_reg_name(target: Target, idx: usize) -> &'static str {
    match target.arch {
        Arch::AArch64 => ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"][idx],
        Arch::X86_64 => ["xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7"][idx],
    }
}

pub(crate) fn int_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rax",
    }
}

pub(crate) fn float_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "d0",
        Arch::X86_64 => "xmm0",
    }
}

pub(crate) fn string_result_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

pub(crate) fn frame_pointer_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x29",
        Arch::X86_64 => "rbp",
    }
}

pub(crate) fn symbol_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r11",
    }
}

pub(crate) fn secondary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

pub(crate) fn tertiary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x11",
        Arch::X86_64 => "rcx",
    }
}

pub fn temp_int_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

pub fn process_argc_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rdi",
    }
}

pub fn process_argv_reg(target: Target) -> &'static str {
    match target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rsi",
    }
}

pub fn nested_call_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    }
}

pub(crate) fn is_float_register(reg: &str) -> bool {
    reg.starts_with('d') || reg.starts_with("xmm")
}

#[derive(Debug, Clone, Copy)]
pub struct IncomingArgCursor {
    pub(crate) int_reg_idx: usize,
    pub(crate) float_reg_idx: usize,
    pub(crate) caller_stack_offset: usize,
    pub(crate) int_stack_only: bool,
    pub(crate) float_stack_only: bool,
}

impl IncomingArgCursor {
    pub fn new(initial_int_reg_idx: usize) -> Self {
        Self::for_target(Target::new(Platform::MacOS, Arch::AArch64), initial_int_reg_idx)
    }

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
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingArgAssignment {
    pub ty: PhpType,
    pub start_reg: usize,
    pub is_float: bool,
}

impl OutgoingArgAssignment {
    pub(crate) fn in_register(&self) -> bool {
        self.start_reg != STACK_ARG_SENTINEL
    }
}
