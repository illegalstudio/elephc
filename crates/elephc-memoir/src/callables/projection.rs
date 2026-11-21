//! Purpose:
//! Projects private callable descriptor and signature fields through a native C ABI.
//!
//! Called from:
//! - The compiler's hosted eval runtime emitter.
//!
//! Key details:
//! - Projections allocate nothing and borrow metadata for the lifetime of the descriptor.
//! - The compiler supplies the public output layout independently of the private input layout.

use super::layout::{
    INVOKER_OFFSET, NAME_LENGTH_OFFSET, NAME_OFFSET, PARAM_NAME_WORDS, PARAM_TYPE_WORDS,
    SIGNATURE_COUNT_OFFSET, SIGNATURE_NAMES_OFFSET, SIGNATURE_OFFSET, SIGNATURE_REF_FLAGS_OFFSET,
    SIGNATURE_REGULAR_OFFSET, SIGNATURE_TYPES_OFFSET, SIGNATURE_VARIADIC_OFFSET,
};
use crate::{Arch, Emitter};

/// Exposes a signature and an opaque parameter table through the hosted V1 contract.
/// A result of one fills the view, zero denotes absent metadata, and minus one denotes malformed input.
pub fn emit_signature(emitter: &mut impl Emitter, offsets: [usize; 6]) {
    let descriptor = emitter.int_arg_reg_name(0);
    let out = emitter.int_arg_reg_name(1);
    let (signature, value) = match emitter.arch() {
        Arch::AArch64 => ("x10", "x11"),
        Arch::X86_64 => ("r10", "r11"),
    };
    let invalid = "L_eval_callable_signature_v1_invalid";
    let absent = "L_eval_callable_signature_v1_absent";
    emitter.label_global(&emitter.extern_symbol("__elephc_eval_callable_signature_v1"));
    emit_branch_if_null(emitter, descriptor, invalid);
    emit_branch_if_null(emitter, out, invalid);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("tst {descriptor}, #7"));              // confirm word alignment before opening the descriptor
            emitter.instruction(&format!("b.ne {invalid}"));                    // reject a misaligned native descriptor
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {descriptor}, 7"));              // confirm word alignment before opening the descriptor
            emitter.instruction(&format!("jnz {invalid}"));                     // reject a misaligned native descriptor
        }
    }
    emitter.emit_load_from_address(signature, descriptor, SIGNATURE_OFFSET);
    emit_branch_if_null(emitter, signature, absent);
    emitter.emit_load_from_address(value, descriptor, INVOKER_OFFSET);
    emit_branch_if_null(emitter, value, absent);

    let [table, name, length, count, regular, variadic] = offsets;
    emitter.emit_store_to_address(signature, out, table);
    for (record, input, output) in [
        (descriptor, NAME_OFFSET, name),
        (descriptor, NAME_LENGTH_OFFSET, length),
        (signature, SIGNATURE_COUNT_OFFSET, count),
        (signature, SIGNATURE_REGULAR_OFFSET, regular),
        (signature, SIGNATURE_VARIADIC_OFFSET, variadic),
    ] {
        emitter.emit_load_from_address(value, record, input);
        emitter.emit_store_to_address(value, out, output);
    }
    emit_status(emitter, 1);
    emitter.label(absent);
    emit_status(emitter, 0);
    emitter.label(invalid);
    emit_status(emitter, -1);
}

/// Exposes a parameter from the opaque signature obtained through the V1 signature accessor.
/// A result of one fills the view and minus one denotes malformed metadata or a bad index.
pub fn emit_parameter(emitter: &mut impl Emitter, offsets: [usize; 4]) {
    let signature = emitter.int_arg_reg_name(0);
    let index = emitter.int_arg_reg_name(1);
    let out = emitter.int_arg_reg_name(2);
    let (row, value, outside) = match emitter.arch() {
        Arch::AArch64 => ("x10", "x11", "b.hs"),
        Arch::X86_64 => ("r10", "r11", "jae"),
    };
    let invalid = "L_eval_callable_parameter_v1_invalid";
    emitter.label_global(&emitter.extern_symbol("__elephc_eval_callable_parameter_v1"));
    emit_branch_if_null(emitter, signature, invalid);
    emit_branch_if_null(emitter, out, invalid);
    emitter.emit_load_from_address(value, signature, SIGNATURE_COUNT_OFFSET);
    emitter.instruction(&format!("cmp {index}, {value}"));                      // check the index before addressing any parameter row
    emitter.instruction(&format!("{outside} {invalid}"));                       // reject indices outside the visible parameters

    let [name, length, tag, by_ref] = offsets;
    for (field, words, outputs) in [
        (SIGNATURE_NAMES_OFFSET, PARAM_NAME_WORDS, &[name, length][..]),
        (SIGNATURE_TYPES_OFFSET, PARAM_TYPE_WORDS, &[tag][..]),
        (SIGNATURE_REF_FLAGS_OFFSET, 1, &[by_ref][..]),
    ] {
        emitter.emit_load_from_address(row, signature, field);
        emit_branch_if_null(emitter, row, invalid);
        let stride = words * size_of::<u64>();
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.emit_load_int_immediate(value, stride as i64);
                emitter.instruction(&format!("madd {row}, {index}, {value}, {row}")); // address the selected metadata row
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("imul {value}, {index}, {stride}")); // scale the index by this metadata stride
                emitter.instruction(&format!("add {row}, {value}"));            // address the selected metadata row
            }
        }
        for (position, &output) in outputs.iter().enumerate() {
            emitter.emit_load_from_address(value, row, position * size_of::<u64>());
            emitter.emit_store_to_address(value, out, output);
        }
    }
    emit_status(emitter, 1);
    emitter.label(invalid);
    emit_status(emitter, -1);
}

/// Branches when a required borrowed pointer is missing.
fn emit_branch_if_null(emitter: &mut impl Emitter, register: &str, label: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbz {register}, {label}")), // fail before reading or writing through null
        Arch::X86_64 => {
            emitter.instruction(&format!("test {register}, {register}"));       // inspect the borrowed pointer
            emitter.instruction(&format!("jz {label}"));                        // fail before reading or writing through null
        }
    }
}

/// Returns the projection status through the target's signed C word result.
fn emit_status(emitter: &mut impl Emitter, status: i64) {
    emitter.emit_load_int_immediate(emitter.int_result_reg(), status);
    emitter.emit_return();
}
