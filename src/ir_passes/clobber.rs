//! Purpose:
//! Classifies which EIR opcodes and terminators are "volatile-safe": their
//! `codegen_ir` lowering emits no call and touches only the fixed result and
//! scratch registers, never the caller-saved registers the allocator may hand
//! out to non-call-crossing intervals.
//!
//! Called from:
//! - `crate::ir_passes::intervals` to mark each live interval as call-free.
//!
//! Key details:
//! - Safe-by-default in the correctness direction: the allowlist returns `true`
//!   only for opcodes whose lowering was audited to (a) emit no `bl`/`call`,
//!   (b) clobber a register set contained in {result, x9/x10/x11 and d0/d1 on
//!   AArch64; rax/rdx/rcx/r10/r11 and xmm0/xmm1 on x86_64}, and (c) load their
//!   SSA operands before using any scratch. Every other opcode falls through to
//!   `false`, so a forgotten opcode only loses an optimization opportunity — it
//!   can never place a live value in a register the lowering then clobbers.
//! - Because the caller-saved pools (`x12`–`x15`, `d16`–`d23`, `rsi`/`rdi`/`r8`/
//!   `r9`, `xmm2`–`xmm7`) are disjoint from every register the allowlisted
//!   lowerings touch, a value that lives only across allowlisted ops keeps its
//!   caller-saved register intact with no prologue save/restore.

use crate::ir::{Op, Terminator};

/// Returns true when `op`'s lowering neither emits a call nor touches a
/// caller-saved register outside the fixed result/scratch set.
///
/// Used to decide whether a live interval spanning this instruction can safely
/// keep a value in a caller-saved register. The default is `false`: only the
/// audited pure-compute opcodes below are treated as volatile-safe.
pub(super) fn op_is_volatile_safe(op: Op) -> bool {
    use Op::*;
    matches!(
        op,
        // Constants materialized directly into the result register.
        ConstI64 | ConstBool | ConstNull | ConstF64
        // Integer arithmetic, bitwise, and shift: result + secondary/tertiary
        // scratch only (see `lower_inst::arithmetic`).
        | IAdd | ISub | IMul | INeg
        | IBitAnd | IBitOr | IBitXor | IBitNot
        | IShl | IShrA | ISMod | IDiv
        // Floating-point arithmetic: d0/d1 or xmm0/xmm1 only (`lower_inst::floats`).
        | FAdd | FSub | FMul | FDiv | FNeg
        // Integer/float comparisons: result + secondary scratch only.
        | ICmp | FCmp
        // Scalar int<->float conversions: inline scvtf/fcvtzs / cvtsi2sd/cvttsd2si.
        | IToF | FToI
        | Nop
    )
}

/// Returns true when `term`'s lowering neither emits a call nor clobbers a
/// caller-saved register holding another live value.
///
/// Branches, multi-way switches, and returns only materialize their already
/// computed condition/scrutinee/return value through the standard chokepoints;
/// throws, generator suspends, fatals, and unreachable are conservatively unsafe.
pub(super) fn terminator_is_volatile_safe(term: &Terminator) -> bool {
    matches!(
        term,
        Terminator::Br { .. }
            | Terminator::CondBr { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
    )
}
