//! Purpose:
//! Describes legal 64-bit integer immediate operands and materialization costs.
//!
//! Called from:
//! - EIR constant allocation and integer instruction selection.
//!
//! Key details:
//! - x86_64 ALU immediates sign-extend 32 bits; AArch64 has separate arithmetic
//!   and logical encodings. Never truncate a PHP integer to fit an instruction.

use crate::codegen_support::platform::Arch;

/// Integer operation whose right operand may have an immediate encoding.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ImmediateOp {
    Add, Sub, Mul, And, Or, Xor, Compare,
}

/// Legal mnemonic and assembler operand for a 64-bit operation.
pub(crate) struct IntImmediate {
    pub mnemonic: &'static str,
    pub operand: String,
}

/// Selects a legal immediate form, including add/sub inversion for negative values.
pub(crate) fn select_int_immediate(arch: Arch, op: ImmediateOp, value: i64) -> Option<IntImmediate> {
    use ImmediateOp::*;
    let mnemonic = match (arch, op) {
        (_, Add) => "add",
        (_, Sub) => "sub",
        (Arch::X86_64, Mul) => "imul",
        (Arch::AArch64, Mul) => return None,
        (_, And) => "and",
        (Arch::AArch64, Or) => "orr",
        (Arch::X86_64, Or) => "or",
        (Arch::AArch64, Xor) => "eor",
        (Arch::X86_64, Xor) => "xor",
        (_, Compare) => "cmp",
    };
    if arch == Arch::X86_64 {
        return i32::try_from(value).ok().map(|_| IntImmediate {
            mnemonic, operand: value.to_string(),
        });
    }
    if matches!(op, And | Or | Xor) {
        return aarch64_logical_immediate(value as u64).then(|| IntImmediate {
            mnemonic, operand: format!("#0x{:x}", value as u64),
        });
    }
    let magnitude = value.unsigned_abs();
    let operand = if magnitude <= 4095 {
        format!("#{magnitude}")
    } else if magnitude & 4095 == 0 && magnitude >> 12 <= 4095 {
        format!("#{}, lsl #12", magnitude >> 12)
    } else {
        return None;
    };
    let mnemonic = if value < 0 {
        match op { Add => "sub", Sub => "add", Compare => "cmn", _ => unreachable!() }
    } else {
        mnemonic
    };
    Some(IntImmediate { mnemonic, operand })
}

/// Recognizes a repeated, rotated nonempty run of ones in a power-of-two element.
pub(crate) fn aarch64_logical_immediate(bits: u64) -> bool {
    if bits == 0 || bits == u64::MAX { return false; }
    for width in [2, 4, 8, 16, 32, 64] {
        let mask = u64::MAX >> (64 - width);
        let element = bits & mask;
        let mut repeated = element;
        let mut shift = width;
        while shift < 64 {
            repeated |= element << shift;
            shift += width;
        }
        if repeated != bits { continue; }
        // Exactly two transitions in the circular element means one run of
        // ones and one run of zeros, including masks that wrap across bit zero.
        let rotated = (element >> 1) | ((element & 1) << (width - 1));
        if (element ^ rotated).count_ones() == 2 { return true; }
    }
    false
}

/// Estimates the existing integer loader's instruction cost, charging wide x86 moves twice.
pub(crate) fn int_materialization_cost(arch: Arch, value: i64) -> u32 {
    match arch {
        Arch::X86_64 => if i32::try_from(value).is_ok() { 1 } else { 2 },
        Arch::AArch64 => {
            if (-65536..=65535).contains(&value) { return 1; }
            1 + [16, 32, 48].iter().filter(|&&shift| (value as u64 >> shift) & 65535 != 0).count() as u32
        }
    }
}
