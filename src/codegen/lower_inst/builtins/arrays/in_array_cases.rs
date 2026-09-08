//! Purpose:
//! In-array case selection and scalar cross-type lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Selects whether `in_array()` should use PHP loose or strict membership semantics.
#[derive(Clone, Copy)]
pub(in crate::codegen::lower_inst::builtins) enum InArrayMode {
    Loose,
    Strict,
}

/// Describes which indexed-array `in_array()` lowering path applies.
pub(super) enum InArrayCase {
    Empty,
    AlwaysFalse,
    ScalarExact,
    ScalarTruthy,
    StringExact,
    StringLoose,
    StringNeedleIntArray,
    IntNeedleStringArray,
    StringNeedleBoolArray,
    BoolNeedleStringArray,
    MixedNeedleStringExact,
    MixedNeedleStringLoose,
    MixedIntExact,
    MixedIntLoose,
    MixedStringExact,
    MixedStringLoose,
    MixedMixedExact,
    MixedMixedLoose,
}

/// Verifies that an indexed-array `in_array()` call has a lowered Phase 04 payload shape.
pub(super) fn supported_in_array_case(
    needle_ty: PhpType,
    array_ty: PhpType,
    mode: InArrayMode,
) -> Result<InArrayCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(InArrayCase::Empty),
            elem_ty @ (PhpType::Int | PhpType::Bool) => {
                supported_in_array_scalar_case(&needle_ty, elem_ty, mode)
            }
            PhpType::Str => supported_in_array_string_case(&needle_ty, mode),
            // An indexed `array<Mixed>` (e.g. the boxed result of a function that returns a
            // container built from an untyped parameter) stores one boxed Mixed cell per 8-byte
            // slot. A string needle is matched by unboxing each cell and string-comparing the
            // string-tagged ones, mirroring the concrete string-array path's scan.
            PhpType::Mixed if needle_ty == PhpType::Str => match mode {
                InArrayMode::Loose => Ok(InArrayCase::MixedStringLoose),
                InArrayMode::Strict => Ok(InArrayCase::MixedStringExact),
            },
            PhpType::Mixed if needle_ty == PhpType::Int => match mode {
                InArrayMode::Loose => Ok(InArrayCase::MixedIntLoose),
                InArrayMode::Strict => Ok(InArrayCase::MixedIntExact),
            },
            PhpType::Mixed if needle_ty == PhpType::Mixed => match mode {
                InArrayMode::Loose => Ok(InArrayCase::MixedMixedLoose),
                InArrayMode::Strict => Ok(InArrayCase::MixedMixedExact),
            },
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "in_array needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty, elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "in_array for PHP array type {:?}",
            other
        ))),
    }
}

/// Selects the scalar-array membership path for PHP loose or strict comparison.
pub(super) fn supported_in_array_scalar_case(
    needle_ty: &PhpType,
    elem_ty: PhpType,
    mode: InArrayMode,
) -> Result<InArrayCase> {
    match mode {
        InArrayMode::Strict => {
            if needle_ty == &elem_ty && matches!(needle_ty, PhpType::Int | PhpType::Bool) {
                Ok(InArrayCase::ScalarExact)
            } else if matches!(needle_ty, PhpType::Int | PhpType::Bool | PhpType::Str) {
                Ok(InArrayCase::AlwaysFalse)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "strict in_array needle PHP type {:?} for indexed-array element PHP type {:?}",
                    needle_ty, elem_ty
                )))
            }
        }
        InArrayMode::Loose => match (needle_ty, &elem_ty) {
            (PhpType::Int, PhpType::Int) | (PhpType::Bool, PhpType::Bool) => {
                Ok(InArrayCase::ScalarExact)
            }
            (PhpType::Int | PhpType::Bool, PhpType::Int | PhpType::Bool) => {
                Ok(InArrayCase::ScalarTruthy)
            }
            (PhpType::Str, PhpType::Int) => Ok(InArrayCase::StringNeedleIntArray),
            (PhpType::Str, PhpType::Bool) => Ok(InArrayCase::StringNeedleBoolArray),
            _ => Err(CodegenIrError::unsupported(format!(
                "loose in_array needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty, elem_ty
            ))),
        },
    }
}

/// Selects the string-array membership path for PHP loose or strict comparison.
pub(super) fn supported_in_array_string_case(needle_ty: &PhpType, mode: InArrayMode) -> Result<InArrayCase> {
    match mode {
        InArrayMode::Strict => match needle_ty {
            PhpType::Str => Ok(InArrayCase::StringExact),
            PhpType::Int | PhpType::Bool => Ok(InArrayCase::AlwaysFalse),
            PhpType::Mixed => Ok(InArrayCase::MixedNeedleStringExact),
            _ => Err(CodegenIrError::unsupported(format!(
                "strict in_array needle PHP type {:?} for string indexed-array",
                needle_ty
            ))),
        },
        InArrayMode::Loose => match needle_ty {
            PhpType::Str => Ok(InArrayCase::StringLoose),
            PhpType::Int => Ok(InArrayCase::IntNeedleStringArray),
            PhpType::Bool => Ok(InArrayCase::BoolNeedleStringArray),
            PhpType::Mixed => Ok(InArrayCase::MixedNeedleStringLoose),
            _ => Err(CodegenIrError::unsupported(format!(
                "loose in_array needle PHP type {:?} for string indexed-array",
                needle_ty
            ))),
        },
    }
}

/// Lowers integer-like indexed-array membership via the existing search helper.
pub(super) fn lower_in_array_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp x0, #0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("cset x0, ge");                             // materialize in_array() as true for any found index
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp rax, 0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("setge al");                                // materialize in_array() as true for any found index
            ctx.emitter.instruction("movzx rax, al");                           // widen the membership flag into the integer result register
        }
    }
    Ok(())
}

/// Lowers loose bool/int membership by comparing PHP truthiness on both sides.
pub(super) fn lower_in_array_scalar_truthy(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_scalar_truthy_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_scalar_truthy_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 truthiness-based scalar membership loop.
pub(super) fn lower_in_array_scalar_truthy_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_scalar_truthy_loop");
    let found_label = ctx.next_label("in_array_scalar_truthy_found");
    let end_label = ctx.next_label("in_array_scalar_truthy_end");
    let done_label = ctx.next_label("in_array_scalar_truthy_done");

    ctx.load_value_to_reg(needle, "x11")?;
    emit_reg_nonzero_bool(ctx, "x11");
    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed scalar-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed scalar payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the scalar membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all scalar elements are scanned
    ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                     // load the current scalar element
    emit_reg_nonzero_bool(ctx, "x13");
    ctx.emitter.instruction("cmp x13, x11");                                    // compare element truthiness against needle truthiness
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop as soon as a loosely equal element is found
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed scalar element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining scalar payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a loosely equal scalar
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no indexed scalar element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 truthiness-based scalar membership loop.
pub(super) fn lower_in_array_scalar_truthy_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_scalar_truthy_loop");
    let found_label = ctx.next_label("in_array_scalar_truthy_found");
    let end_label = ctx.next_label("in_array_scalar_truthy_end");
    let done_label = ctx.next_label("in_array_scalar_truthy_done");

    ctx.load_value_to_reg(needle, "r10")?;
    emit_reg_nonzero_bool(ctx, "r10");
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov r12, QWORD PTR [r11]");                        // load indexed scalar-array length before scanning payload slots
    ctx.emitter.instruction("lea r11, [r11 + 24]");                             // point at the first indexed scalar payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the scalar membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all scalar elements are scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r13*8]");                // load the current scalar element
    emit_reg_nonzero_bool(ctx, "rax");
    ctx.emitter.instruction("cmp rax, r10");                                    // compare element truthiness against needle truthiness
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop as soon as a loosely equal element is found
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed scalar element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining scalar payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a loosely equal scalar
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no indexed scalar element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers loose string-needle membership in an integer array via PHP numeric-string parsing.
pub(super) fn lower_in_array_string_needle_int_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_needle_int_array_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_string_needle_int_array_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-needle vs int-array loose membership loop.
pub(super) fn lower_in_array_string_needle_int_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_int_loop");
    let found_label = ctx.next_label("in_array_str_int_found");
    let end_label = ctx.next_label("in_array_str_int_end");
    let done_label = ctx.next_label("in_array_str_int_done");

    ctx.load_string_value_to_regs(needle, "x1", "x2")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    ctx.emitter.instruction("cmp x0, #0");                                      // reject non-numeric strings for PHP number/string loose equality
    ctx.emitter.instruction(&format!("b.eq {}", end_label));                    // a non-numeric string needle cannot equal an int element
    ctx.emitter.instruction("fmov d1, d0");                                     // preserve the parsed numeric-string value for the scan
    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed int-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed int payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the numeric membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all integer elements are scanned
    ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                     // load the current integer element
    ctx.emitter.instruction("scvtf d0, x13");                                   // promote the integer element for PHP numeric comparison
    ctx.emitter.instruction("fcmp d0, d1");                                     // compare element number with parsed string number
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop when the numeric values match
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed int element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining int payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a loose numeric match
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no integer element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-needle vs int-array loose membership loop.
pub(super) fn lower_in_array_string_needle_int_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_int_loop");
    let found_label = ctx.next_label("in_array_str_int_found");
    let end_label = ctx.next_label("in_array_str_int_end");
    let done_label = ctx.next_label("in_array_str_int_done");

    ctx.load_string_value_to_regs(needle, "rax", "rdx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    ctx.emitter.instruction("test rax, rax");                                   // reject non-numeric strings for PHP number/string loose equality
    ctx.emitter.instruction(&format!("je {}", end_label));                      // a non-numeric string needle cannot equal an int element
    ctx.emitter.instruction("movapd xmm1, xmm0");                               // preserve the parsed numeric-string value for the scan
    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed int-array length before scanning payload slots
    ctx.emitter.instruction("lea r10, [r10 + 24]");                             // point at the first indexed int payload slot
    ctx.emitter.instruction("xor r12d, r12d");                                  // start the numeric membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r12, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all integer elements are scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r12*8]");                // load the current integer element
    ctx.emitter.instruction("cvtsi2sd xmm0, rax");                              // promote the integer element for PHP numeric comparison
    ctx.emitter.instruction("ucomisd xmm0, xmm1");                              // compare element number with parsed string number
    ctx.emitter.instruction(&format!("jp {}", end_label));                      // unordered parsed values are never equal
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop when the numeric values match
    ctx.emitter.instruction("add r12, 1");                                      // advance to the next indexed int element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining int payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a loose numeric match
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no integer element matches
    ctx.emitter.label(&done_label);
    Ok(())
}
