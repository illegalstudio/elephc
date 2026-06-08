//! Purpose:
//! Lowers target-specific instruction snippets shared by binary operators.
//! Keeps operator-specific conversions and result register setup out of the dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::binops`
//!
//! Key details:
//! - Runtime calls and target instructions must preserve left/right evaluation order and scratch register assumptions.

use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::parser::ast::BinOp;
use crate::types::PhpType;

/// Sets integer result (x0/rax) to 1 or 0 based on a condition code from flags.
pub(super) fn emit_set_bool_from_flags(emitter: &mut Emitter, cond: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cset x0, {}", cond));                 // set integer result to 1 if the comparison condition matched
        }
        Arch::X86_64 => {
            let setcc = match cond {
                "eq" => "sete",
                "ne" => "setne",
                "lt" => "setl",
                "gt" => "setg",
                "le" => "setle",
                "ge" => "setge",
                _ => unreachable!("unsupported comparison condition {cond}"),
            };
            emitter.instruction(&format!("{} al", setcc));                      // set the low result byte when the comparison condition matched
            emitter.instruction("movzx rax, al");                               // zero-extend the boolean byte into the integer result register
        }
    }
}

/// Sets integer result (x0/rax) from float comparison flags.
///
/// Float comparisons follow IEEE/PHP NaN rules: every ordered compare is false when either
/// operand is NaN, except `!=` which is true. The naive integer condition codes get this
/// wrong (arm64 `lt`/`le`, x86_64 `eq`/`ne`/`lt`/`le`), so this routes through NaN-aware
/// conditions and an x86_64 parity guard.
pub(super) fn emit_set_float_bool_from_flags(emitter: &mut Emitter, cond: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            // `fcmp` sets the flags to unordered (N=0,Z=0,C=1,V=1) when either operand is
            // NaN. Integer `lt`/`le` read true there (N!=V), so `NAN < x`/`NAN <= x` would be
            // true; the IEEE-aware `mi` (N set) and `ls` (C clear or Z set) are false on
            // unordered. `eq`/`ne`/`gt`/`ge` are already correct.
            let cset_cond = match cond {
                "lt" => "mi",
                "le" => "ls",
                other => other,
            };
            emitter.instruction(&format!("cset x0, {}", cset_cond));            // set result to 1 when the NaN-aware float condition matched
        }
        Arch::X86_64 => {
            // `ucomisd` sets ZF=CF=PF=1 for an unordered (NaN) compare, so `sete`/`setne`/
            // `setb`/`setbe` misread NaN. PHP makes every NaN compare false except `!=`, so
            // guard with the parity flag (PF, set on unordered). `seta`/`setae` (`>`/`>=`)
            // are already false on unordered and need no guard.
            let (setcc, guard) = match cond {
                "eq" => ("sete", Some(("setnp", "and"))),
                "ne" => ("setne", Some(("setp", "or"))),
                "lt" => ("setb", Some(("setnp", "and"))),
                "le" => ("setbe", Some(("setnp", "and"))),
                "gt" => ("seta", None),
                "ge" => ("setae", None),
                _ => unreachable!("unsupported float comparison condition {cond}"),
            };
            emitter.instruction(&format!("{} al", setcc));                      // set the comparison flag into the low result byte
            if let Some((parity, combine)) = guard {
                emitter.instruction(&format!("{} cl", parity));                 // capture the parity (NaN) flag for the unordered guard
                emitter.instruction(&format!("{} al, cl", combine));            // fold the guard so NaN follows PHP's float compare rules
            }
            emitter.instruction("movzx rax, al");                               // zero-extend the boolean byte into the integer result register
        }
    }
}

/// Promotes an integer operand to a float register (scvtf/cvtsi2sd).
pub(super) fn emit_promote_int_to_float(emitter: &mut Emitter, float_reg: &str, int_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("scvtf {}, {}", float_reg, int_reg));  // promote the integer operand into a floating-point register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cvtsi2sd {}, {}", float_reg, int_reg)); // promote the integer operand into a floating-point register
        }
    }
}

/// Pops the left float (or promoted int) into the comparison scratch register.
pub(super) fn emit_pop_left_float_for_comparison(emitter: &mut Emitter, left_ty: &PhpType) {
    let left_float_reg = match emitter.target.arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    };
    if *left_ty == PhpType::Float {
        abi::emit_pop_float_reg(emitter, left_float_reg);                       // pop left float operand into the comparison scratch register
    } else {
        let left_int_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_pop_reg(emitter, left_int_reg);                               // pop left integer operand before float promotion
        emit_promote_int_to_float(emitter, left_float_reg, left_int_reg);
    }
}

/// Emits a double-precision comparison (fcmp/ucomisd) setting NZCV/flags.
pub(super) fn emit_float_compare(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d1, d0");                                 // compare two doubles, setting NZCV flags
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm1, xmm0");                          // compare two doubles, setting x86_64 condition flags
        }
    }
}

/// Emits a float binop (+, -, *, /, %) using target instructions.
pub(super) fn emit_float_binop(emitter: &mut Emitter, op: &BinOp) {
    match emitter.target.arch {
        Arch::AArch64 => {
            match op {
                BinOp::Add => {
                    emitter.instruction("fadd d0, d1, d0");                     // float addition: left + right
                }
                BinOp::Sub => {
                    emitter.instruction("fsub d0, d1, d0");                     // float subtraction: left - right
                }
                BinOp::Mul => {
                    emitter.instruction("fmul d0, d1, d0");                     // float multiplication: left * right
                }
                BinOp::Div => {
                    emitter.instruction("fdiv d0, d1, d0");                     // float division: left / right
                }
                BinOp::Mod => {
                    // -- float modulo: a - trunc(a/b) * b (C/PHP truncated mod) --
                    emitter.instruction("fdiv d2, d1, d0");                     // d2 = left / right
                    emitter.instruction("frintz d2, d2");                       // d2 = trunc(left / right) toward zero
                    emitter.instruction("fmsub d0, d2, d0, d1");                // d0 = left - trunc(l/r)*right
                }
                _ => unreachable!(),
            }
        }
        Arch::X86_64 => {
            match op {
                BinOp::Add => {
                    emitter.instruction("addsd xmm1, xmm0");                    // float addition: left + right
                    emitter.instruction("movsd xmm0, xmm1");                    // move the sum back to the floating-point result register
                }
                BinOp::Sub => {
                    emitter.instruction("subsd xmm1, xmm0");                    // float subtraction: left - right
                    emitter.instruction("movsd xmm0, xmm1");                    // move the difference back to the floating-point result register
                }
                BinOp::Mul => {
                    emitter.instruction("mulsd xmm1, xmm0");                    // float multiplication: left * right
                    emitter.instruction("movsd xmm0, xmm1");                    // move the product back to the floating-point result register
                }
                BinOp::Div => {
                    emitter.instruction("divsd xmm1, xmm0");                    // float division: left / right
                    emitter.instruction("movsd xmm0, xmm1");                    // move the quotient back to the floating-point result register
                }
                BinOp::Mod => {
                    // -- float modulo: a - trunc(a/b) * b (C/PHP truncated mod) --
                    emitter.instruction("movsd xmm2, xmm1");                    // copy the left operand before quotient calculation
                    emitter.instruction("divsd xmm2, xmm0");                    // xmm2 = left / right
                    emitter.instruction("roundsd xmm2, xmm2, 3");               // xmm2 = trunc(left / right) toward zero
                    emitter.instruction("mulsd xmm2, xmm0");                    // xmm2 = trunc(left / right) * right
                    emitter.instruction("subsd xmm1, xmm2");                    // xmm1 = left - trunc(left/right)*right
                    emitter.instruction("movsd xmm0, xmm1");                    // move the modulo result back to the floating-point result register
                }
                _ => unreachable!(),
            }
        }
    }
}
