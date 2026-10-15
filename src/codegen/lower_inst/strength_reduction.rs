//! Purpose:
//! Target-aware strength reduction of wrapping integer multiplication by a constant into
//! shift / add / subtract / `lea` sequences when the target's costs make that cheaper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::arithmetic::lower_int_binop()` for unchecked `imul`.
//!
//! Key details:
//! - Only unchecked `imul` qualifies: its result is defined modulo 2^64, and every rewrite is an
//!   identity in that ring (`x * 31 == (x << 5) - x` for every 64-bit `x`). Checked multiplies
//!   still need the hardware overflow flag and are left alone.
//! - A constant `c` is factored as `±m * 2^k` with `m` odd; `m` must be `1`, `2^j + 1` or
//!   `2^j - 1`. Each candidate is priced per target and kept only when it beats `mul`/`imul`.
//! - The sequences only touch the integer result register and the secondary scratch register,
//!   so `imul` stays in the allocator's volatile-safe opcode set.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::{Instruction, Op};
use crate::ir_passes::rematerialize::scalar_constant;
use crate::ir_passes::ScalarConstant;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{arithmetic::load_integer_operand, store_if_result};

/// One accumulator update; each multiplies the accumulator by a fixed factor modulo 2^64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MulStep {
    /// `acc <<= k` (factor `2^k`).
    Shl(u32),
    /// `acc += acc << j` (factor `2^j + 1`).
    AddShifted(u32),
    /// `acc = (acc << j) - acc` (factor `2^j - 1`).
    ShiftedMinus(u32),
    /// `acc = acc - (acc << j)` (factor `1 - 2^j`).
    MinusShifted(u32),
    /// `acc = -(acc << k)` (factor `-2^k`); `k == 0` is a plain negation.
    NegShl(u32),
}

impl MulStep {
    /// Returns `(instructions, latency in cycles)` of the step on `arch`.
    fn cost(self, arch: Arch) -> (u32, u32) {
        match (arch, self) {
            (Arch::AArch64, Self::Shl(_)) => (1, 1),
            (Arch::AArch64, Self::AddShifted(_) | Self::MinusShifted(_)) => (1, 2),
            (Arch::AArch64, Self::ShiftedMinus(_)) => (2, 2),
            (Arch::AArch64, Self::NegShl(0)) => (1, 1),
            (Arch::AArch64, Self::NegShl(_)) => (1, 2),
            (Arch::X86_64, Self::Shl(_)) => (1, 1),
            // `lea acc, [acc + acc*{2,4,8}]`.
            (Arch::X86_64, Self::AddShifted(1..=3)) => (1, 1),
            // The copy into the scratch register is eliminated at rename on current cores.
            (Arch::X86_64, Self::AddShifted(_) | Self::ShiftedMinus(_) | Self::MinusShifted(_)) => (3, 2),
            (Arch::X86_64, Self::NegShl(0)) => (1, 1),
            (Arch::X86_64, Self::NegShl(_)) => (2, 2),
        }
    }

    /// Applies the step to a 64-bit value with wrapping semantics (used to verify plans).
    #[cfg(test)]
    fn apply(self, acc: i64) -> i64 {
        match self {
            Self::Shl(k) => acc.wrapping_shl(k),
            Self::AddShifted(j) => acc.wrapping_add(acc.wrapping_shl(j)),
            Self::ShiftedMinus(j) => acc.wrapping_shl(j).wrapping_sub(acc),
            Self::MinusShifted(j) => acc.wrapping_sub(acc.wrapping_shl(j)),
            Self::NegShl(k) => acc.wrapping_shl(k).wrapping_neg(),
        }
    }
}

/// Returns the cheapest shift/add plan computing `x * constant`, or `None` when `mul` wins.
pub(super) fn plan_constant_multiply(arch: Arch, constant: i64) -> Option<Vec<MulStep>> {
    if constant == 0 || constant == 1 {
        return None;
    }
    let negative = constant < 0;
    let magnitude = (constant as i128).unsigned_abs();
    let k = magnitude.trailing_zeros();
    let odd = magnitude >> k;
    let shift = |steps: &mut Vec<MulStep>| {
        if k > 0 {
            steps.push(MulStep::Shl(k));
        }
    };
    let mut candidates: Vec<Vec<MulStep>> = Vec::new();
    if odd == 1 {
        candidates.push(if negative {
            vec![MulStep::NegShl(k)]
        } else {
            vec![MulStep::Shl(k)]
        });
    } else {
        let j_plus = (odd - 1).trailing_zeros();
        if odd - 1 == 1u128 << j_plus {
            // odd == 2^j + 1.
            let mut steps = vec![MulStep::AddShifted(j_plus)];
            if negative {
                steps.push(MulStep::NegShl(k));
            } else {
                shift(&mut steps);
            }
            candidates.push(steps);
        }
        let j_minus = (odd + 1).trailing_zeros();
        if odd + 1 == 1u128 << j_minus {
            // odd == 2^j - 1.
            let first = if negative { MulStep::MinusShifted(j_minus) } else { MulStep::ShiftedMinus(j_minus) };
            let mut steps = vec![first];
            shift(&mut steps);
            candidates.push(steps);
        }
    }
    candidates
        .into_iter()
        .map(|steps| {
            let (instructions, latency) = steps.iter().fold((0, 0), |(i, l), step| {
                let (si, sl) = step.cost(arch);
                (i + si, l + sl)
            });
            (latency, instructions, steps)
        })
        .filter(|&(latency, instructions, _)| beats_multiply(arch, latency, instructions))
        .min_by_key(|&(latency, instructions, _)| (latency, instructions))
        .map(|(_, _, steps)| steps)
}

/// Returns true when a shift/add sequence is cheaper than the target's multiply.
///
/// AArch64 `mul` has no immediate form: it costs a constant materialization plus a
/// 3-cycle multiply, so two ALU instructions within that latency are a win. x86_64 `imul r,
/// r, imm32` is a single 3-cycle instruction, so only strictly lower latency pays.
fn beats_multiply(arch: Arch, latency: u32, instructions: u32) -> bool {
    match arch {
        Arch::AArch64 => instructions <= 2 && latency <= 3,
        Arch::X86_64 => latency <= 2,
    }
}

/// Lowers `imul` by a constant through a cheaper target sequence; returns false otherwise.
pub(super) fn try_lower_constant_multiply(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    if inst.op != Op::IMul || inst.operands.len() != 2 {
        return Ok(false);
    }
    let arch = ctx.emitter.target.arch;
    let selected = [1usize, 0].into_iter().find_map(|index| {
        let id = inst.operands[index];
        let value = ctx.function.value(id)?;
        if !matches!(value.php_type, PhpType::Int | PhpType::Bool) {
            return None;
        }
        let Some(ScalarConstant::Integer(constant)) = scalar_constant(ctx.function, id) else {
            return None;
        };
        plan_constant_multiply(arch, constant).map(|plan| (inst.operands[1 - index], constant, plan))
    });
    let Some((other, constant, plan)) = selected else {
        return Ok(false);
    };
    let acc = abi::int_result_reg(ctx.emitter);
    let tmp = abi::secondary_scratch_reg(ctx.emitter);
    load_integer_operand(ctx, other, acc, inst)?;
    ctx.emitter.comment(&format!("-- multiply by {constant} with shifts and adds --"));
    for step in plan {
        emit_step(ctx, step, acc, tmp);
    }
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Emits one multiply step on the accumulator, using `tmp` as the only scratch register.
fn emit_step(ctx: &mut FunctionContext<'_>, step: MulStep, acc: &str, tmp: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => match step {
            MulStep::Shl(k) => {
                ctx.emitter.instruction(&format!("lsl {acc}, {acc}, #{k}"));    // scale the product by a power of two
            }
            MulStep::AddShifted(j) => {
                ctx.emitter.instruction(&format!("add {acc}, {acc}, {acc}, lsl #{j}")); // multiply by 2^j + 1 in one shifted add
            }
            MulStep::ShiftedMinus(j) => {
                ctx.emitter.instruction(&format!("lsl {tmp}, {acc}, #{j}"));    // form the operand times 2^j
                ctx.emitter.instruction(&format!("sub {acc}, {tmp}, {acc}"));   // subtract once to multiply by 2^j - 1
            }
            MulStep::MinusShifted(j) => {
                ctx.emitter.instruction(&format!("sub {acc}, {acc}, {acc}, lsl #{j}")); // multiply by 1 - 2^j in one shifted subtract
            }
            MulStep::NegShl(0) => {
                ctx.emitter.instruction(&format!("neg {acc}, {acc}"));          // negate the product
            }
            MulStep::NegShl(k) => {
                let negate = format!("neg {acc}, {acc}, lsl #{k}");
                ctx.emitter.instruction(&negate);                               // scale by 2^k and negate in one shifted negate
            }
        },
        Arch::X86_64 => match step {
            MulStep::Shl(k) => {
                ctx.emitter.instruction(&format!("shl {acc}, {k}"));            // scale the product by a power of two
            }
            MulStep::AddShifted(j @ 1..=3) => {
                let scale = 1u32 << j;
                ctx.emitter.instruction(&format!("lea {acc}, [{acc} + {acc}*{scale}]")); // multiply by 3, 5, or 9 in the address unit
            }
            MulStep::AddShifted(j) => {
                ctx.emitter.instruction(&format!("mov {tmp}, {acc}"));          // copy the operand before scaling it
                ctx.emitter.instruction(&format!("shl {tmp}, {j}"));            // form the operand times 2^j
                ctx.emitter.instruction(&format!("add {acc}, {tmp}"));          // add once to multiply by 2^j + 1
            }
            MulStep::ShiftedMinus(j) => {
                ctx.emitter.instruction(&format!("mov {tmp}, {acc}"));          // keep the original operand for the subtraction
                ctx.emitter.instruction(&format!("shl {acc}, {j}"));            // form the operand times 2^j
                ctx.emitter.instruction(&format!("sub {acc}, {tmp}"));          // subtract once to multiply by 2^j - 1
            }
            MulStep::MinusShifted(j) => {
                ctx.emitter.instruction(&format!("mov {tmp}, {acc}"));          // copy the operand before scaling it
                ctx.emitter.instruction(&format!("shl {tmp}, {j}"));            // form the operand times 2^j
                ctx.emitter.instruction(&format!("sub {acc}, {tmp}"));          // subtract to multiply by 1 - 2^j
            }
            MulStep::NegShl(k) => {
                if k > 0 {
                    ctx.emitter.instruction(&format!("shl {acc}, {k}"));        // scale the product by a power of two
                }
                ctx.emitter.instruction(&format!("neg {acc}"));                 // negate the product
            }
        },
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for constant-multiply plan selection and its wrapping-arithmetic identity.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Every selected plan is replayed against `wrapping_mul` on edge-case inputs.

    use super::{plan_constant_multiply, MulStep};
    use crate::codegen::platform::Arch;

    /// Inputs that exercise sign, carries, and wraparound for every plan.
    const SAMPLES: [i64; 9] = [0, 1, -1, 7, -12345, 0x0123_4567_89ab_cdef, i64::MAX, i64::MIN, i64::MIN + 1];

    /// Replays a plan and returns the product it computes.
    fn run(plan: &[MulStep], x: i64) -> i64 {
        plan.iter().fold(x, |acc, step| step.apply(acc))
    }

    /// Verifies every accepted plan equals wrapping multiplication on both targets.
    #[test]
    fn accepted_plans_match_wrapping_multiply() {
        let mut constants: Vec<i64> = (-1100..=1100).collect();
        constants.extend([i64::MIN, i64::MAX, 1 << 62, -(1 << 62), (1 << 40) + 1, (1 << 33) - 1]);
        for arch in [Arch::AArch64, Arch::X86_64] {
            for &constant in &constants {
                let Some(plan) = plan_constant_multiply(arch, constant) else {
                    continue;
                };
                for &x in &SAMPLES {
                    assert_eq!(run(&plan, x), x.wrapping_mul(constant), "{arch:?} x={x} c={constant} {plan:?}");
                }
            }
        }
    }

    /// Verifies the roadmap's `x * 31` example and other profitable shapes are selected.
    #[test]
    fn selects_profitable_shapes_per_target() {
        assert_eq!(plan_constant_multiply(Arch::AArch64, 31), Some(vec![MulStep::ShiftedMinus(5)]));
        assert_eq!(plan_constant_multiply(Arch::X86_64, 31), Some(vec![MulStep::ShiftedMinus(5)]));
        assert_eq!(plan_constant_multiply(Arch::AArch64, 8), Some(vec![MulStep::Shl(3)]));
        assert_eq!(plan_constant_multiply(Arch::X86_64, 9), Some(vec![MulStep::AddShifted(3)]));
        assert_eq!(plan_constant_multiply(Arch::X86_64, 10), Some(vec![MulStep::AddShifted(2), MulStep::Shl(1)]));
        assert_eq!(plan_constant_multiply(Arch::AArch64, -7), Some(vec![MulStep::MinusShifted(3)]));
        assert_eq!(plan_constant_multiply(Arch::AArch64, -16), Some(vec![MulStep::NegShl(4)]));
    }

    /// Verifies constants that no cheap sequence covers keep the hardware multiply.
    #[test]
    fn keeps_multiply_when_no_sequence_is_cheaper() {
        for arch in [Arch::AArch64, Arch::X86_64] {
            assert_eq!(plan_constant_multiply(arch, 0), None);
            assert_eq!(plan_constant_multiply(arch, 1), None);
            assert_eq!(plan_constant_multiply(arch, 11), None);
            assert_eq!(plan_constant_multiply(arch, 1000), None);
        }
        // x86_64 `imul` (3 cycles) beats a three-step 2-cycle-plus-shift sequence.
        assert_eq!(plan_constant_multiply(Arch::X86_64, 62), None);
    }
}
