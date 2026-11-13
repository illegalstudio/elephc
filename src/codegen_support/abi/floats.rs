//! Purpose:
//! Shares floating sign operations between scalar codegen and boxed runtime helpers.
//!
//! Called from:
//! - EIR floating negation and numeric unary runtime emission.
//!
//! Key details:
//! - The float result register is updated in place; x86 also clobbers xmm1.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Flips the floating result's sign bit while preserving zeros and NaN payloads.
pub(crate) fn emit_negate_float_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("fneg d0, d0"),                    // flip the floating sign without subtraction or conversion
        Arch::X86_64 => {
            emitter.instruction("pcmpeqd xmm1, xmm1");                          // build a sign mask without clobbering a general-purpose register
            emitter.instruction("psllq xmm1, 63");                              // retain each 64-bit lane's sign bit
            emitter.instruction("xorpd xmm0, xmm1");                            // flip the sign while preserving all other payload bits
        }
    }
}
