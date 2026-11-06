//! Purpose:
//! Reconstructs scalar SSA constants in the register requested by a consumer.
//!
//! Called from:
//! - `FunctionContext` value loads and constant definition lowering.
//!
//! Key details:
//! - Float recipes preserve all IEEE bits, including negative zero and NaN payloads.
//! - AArch64 uses the same x9 scratch contract as frame-slot loads. x86_64
//!   float loads use ABI symbol loads, preserving scratch registers in PIC mode.

use crate::codegen::{abi, data_section::DataSection, emit::Emitter, platform::Arch};
use crate::ir_passes::ScalarConstant;

#[cfg(test)]
mod tests;

/// Emits a scalar recipe, treating a float requested in a GPR as its raw bits.
pub(super) fn emit_constant(emitter: &mut Emitter, data: &mut DataSection, constant: ScalarConstant, reg: &str) {
    let bits = match constant {
        ScalarConstant::Integer(n) => n,
        ScalarConstant::Float(bits) => bits as i64,
    };
    if matches!(constant, ScalarConstant::Float(_)) && (reg.starts_with('d') || reg.starts_with("xmm")) {
        // -- transfer the exact IEEE representation without numeric conversion --
        match emitter.target.arch {
            Arch::AArch64 => {
                let scratch = abi::symbol_scratch_reg(emitter);
                abi::emit_load_int_immediate(emitter, scratch, bits);
                emitter.instruction(&format!("fmov {reg}, {scratch}"));         // preserve every floating-point payload bit
            }
            Arch::X86_64 => {
                let label = data.add_float(f64::from_bits(bits as u64));
                abi::emit_load_symbol_to_reg(emitter, reg, &label, 0);
            }
        }
    } else {
        abi::emit_load_int_immediate(emitter, reg, bits);
    }
}
