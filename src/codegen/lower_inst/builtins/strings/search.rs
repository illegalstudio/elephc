//! Purpose:
//! Lowers string search, substring, repetition, replacement-slice, and `strstr` builtins.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Search sentinels and optional bounds are materialized consistently for both targets.

use super::*;
use crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL;

const STRPBRK_EMPTY_CHARACTERS_MESSAGE: &str =
    "strpbrk(): Argument #2 ($characters) must be a non-empty string";

/// Lowers `str_contains()` through `strpos()` and converts found positions to bool.
pub(crate) fn lower_str_contains(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_binary_string_args(ctx, inst, "str_contains")?;
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("cset x0, ge");                             // normalize the signed strpos() result into a PHP boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("setge al");                                // normalize the signed strpos() result into the low boolean byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized boolean byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `strpos()`/`strrpos()` and boxes position-or-false results as Mixed.
///
/// With two operands this is the plain whole-haystack search. With three, `$offset` is
/// normalized here rather than inside the runtime helper because an offset outside the
/// haystack is a catchable `ValueError` in reference PHP, and only the backend can emit a
/// throw the surrounding `try` will observe. The helper therefore always receives a window
/// that is known to sit inside the haystack, plus the absolute base offset that has to be
/// added back to a successful match.
pub(crate) fn lower_string_position(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    direction: StringPositionDirection,
) -> Result<()> {
    if inst.operands.len() == 2 {
        load_binary_string_args(ctx, inst, name)?;
        abi::emit_call_label(ctx.emitter, runtime_label);
        box_search_result(ctx, name);
        return store_if_result(ctx, inst);
    }
    if inst.operands.len() != 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 or 3 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    load_string_position_args(ctx, inst, name)?;
    emit_string_position_offset_guard(ctx, name, direction);
    abi::emit_push_reg(ctx.emitter, string_position_base_reg(ctx));
    abi::emit_call_label(ctx.emitter, runtime_label);
    abi::emit_pop_reg(ctx.emitter, string_position_base_reg(ctx));
    emit_string_position_rebase(ctx, name);
    box_search_result(ctx, name);
    store_if_result(ctx, inst)
}

/// Lowers php-src's byte-oriented `strcspn()` and `strspn()` scanners.
pub(crate) fn lower_string_span(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 to 4 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let length = inst.operands.get(3).copied();
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_string_span_aarch64(ctx, inst, name, length)?,
        Arch::X86_64 => lower_string_span_x86_64(ctx, inst, name, length)?,
    }
    emit_string_span_window_normalization(ctx, name);
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Materializes AArch64 span arguments and their nullable-length presence bit.
fn lower_string_span_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    length: Option<ValueId>,
) -> Result<()> {
    let subject = expect_operand(inst, 0)?;
    let characters = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, subject, name, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject while materializing the remaining arguments
    load_value_as_string_to_regs(ctx, characters, name, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the character mask while materializing the bounds
    if let Some(offset) = inst.operands.get(2).copied() {
        load_as_int(ctx, offset, &format!("{} offset", name))?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    resolve_string_span_length_present(ctx, length)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    resolve_string_span_length_value(ctx, length, name)?;
    ctx.emitter.instruction("mov x6, x0");                                      // park the raw nullable length until the subject is restored
    abi::emit_pop_reg(ctx.emitter, "x7");
    abi::emit_pop_reg(ctx.emitter, "x5");
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the mask pointer and byte length
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the subject pointer and byte length
    Ok(())
}

/// Materializes x86_64 span arguments and their nullable-length presence bit.
fn lower_string_span_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    length: Option<ValueId>,
) -> Result<()> {
    let subject = expect_operand(inst, 0)?;
    let characters = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, subject, name, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_value_as_string_to_regs(ctx, characters, name, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    if let Some(offset) = inst.operands.get(2).copied() {
        load_as_int(ctx, offset, &format!("{} offset", name))?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    resolve_string_span_length_present(ctx, length)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    resolve_string_span_length_value(ctx, length, name)?;
    ctx.emitter.instruction("mov r9, rax");                                     // park the raw nullable length until the subject is restored
    abi::emit_pop_reg(ctx.emitter, "r10");
    abi::emit_pop_reg(ctx.emitter, "r8");
    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    Ok(())
}

/// Resolves whether php-src should apply the nullable `$length` argument.
fn resolve_string_span_length_present(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
) -> Result<()> {
    let reg = abi::int_result_reg(ctx.emitter);
    if string_span_length_is_statically_absent(ctx, length)? {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
        return Ok(());
    }
    let length = length.expect("a non-absent span length has an operand");
    match ctx.value_php_type(length)?.codegen_repr() {
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(length)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!(
                        "cmp x1, #{}",
                        crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
                    )); // compare the nullable integer tag with PHP null
                    ctx.emitter.instruction("cset x0, ne");                     // only an integer payload supplies an explicit length
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!(
                        "cmp rdx, {}",
                        crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
                    )); // compare the nullable integer tag with PHP null
                    ctx.emitter.instruction("setne al");                        // only an integer payload supplies an explicit length
                    ctx.emitter.instruction("movzx rax, al");                   // widen the presence bit into the result register
                }
            }
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(length)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #8");                      // runtime tag 8 identifies a boxed PHP null
                    ctx.emitter.instruction("cset x0, ne");                     // non-null gradual values supply a length
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 8");                      // runtime tag 8 identifies a boxed PHP null
                    ctx.emitter.instruction("setne al");                        // non-null gradual values supply a length
                    ctx.emitter.instruction("movzx rax, al");                   // widen the presence bit into the result register
                }
            }
        }
        _ => abi::emit_load_int_immediate(ctx.emitter, reg, 1),
    }
    Ok(())
}

/// Resolves the nullable `$length` payload, using zero only when its presence bit is false.
fn resolve_string_span_length_value(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
    name: &str,
) -> Result<()> {
    if string_span_length_is_statically_absent(ctx, length)? {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return Ok(());
    }
    let length = length.expect("a non-absent span length has an operand");
    if ctx.value_php_type(length)?.codegen_repr() == PhpType::TaggedScalar {
        ctx.load_value_to_result(length)?;
        crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        return Ok(());
    }
    load_as_int(ctx, length, &format!("{} length", name))
}

/// Reports whether `$length` is omitted or statically PHP null.
fn string_span_length_is_statically_absent(
    ctx: &FunctionContext<'_>,
    length: Option<ValueId>,
) -> Result<bool> {
    match length {
        None => Ok(true),
        Some(length) => Ok(matches!(
            ctx.value_php_type(length)?.codegen_repr(),
            PhpType::Void | PhpType::Never
        )),
    }
}

/// Applies php-src's saturating offset and nullable-length window rules.
fn emit_string_span_window_normalization(ctx: &mut FunctionContext<'_>, name: &str) {
    let offset_non_negative = ctx.next_label(&format!("{}_offset_non_negative", name));
    let offset_done = ctx.next_label(&format!("{}_offset_done", name));
    let length_present = ctx.next_label(&format!("{}_length_present", name));
    let length_non_negative = ctx.next_label(&format!("{}_length_non_negative", name));
    let length_done = ctx.next_label(&format!("{}_length_done", name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x5, #0");                              // is the offset measured back from the subject end?
            ctx.emitter.instruction(&format!("b.ge {}", offset_non_negative));  // non-negative offsets are already absolute
            ctx.emitter.instruction("add x5, x2, x5");                          // resolve a negative offset against the subject length
            ctx.emitter.instruction("cmp x5, #0");                              // did the offset underflow the subject start?
            ctx.emitter.instruction("csel x5, xzr, x5, lt");                    // php-src clamps an underflowing offset to zero
            ctx.emitter.instruction(&format!("b {}", offset_done));             // join the normalized offset path
            ctx.emitter.label(&offset_non_negative);
            ctx.emitter.instruction("cmp x5, x2");                              // does the offset pass the subject end?
            ctx.emitter.instruction("csel x5, x2, x5, gt");                     // php-src clamps an overlong offset to the end
            ctx.emitter.label(&offset_done);
            ctx.emitter.instruction("add x1, x1, x5");                          // advance to the normalized scan window
            ctx.emitter.instruction("sub x2, x2, x5");                          // compute the bytes remaining after the offset
            ctx.emitter.instruction(&format!("cbnz x7, {}", length_present));   // only a non-null explicit length bounds the window
            ctx.emitter.instruction("mov x6, x2");                              // omitted or null length scans to the subject end
            ctx.emitter.instruction(&format!("b {}", length_done));             // skip explicit-length normalization
            ctx.emitter.label(&length_present);
            ctx.emitter.instruction("cmp x6, #0");                              // is the length measured back from the remaining end?
            ctx.emitter.instruction(&format!("b.ge {}", length_non_negative));  // non-negative lengths are direct byte counts
            ctx.emitter.instruction("add x6, x2, x6");                          // resolve a negative length against the remaining bytes
            ctx.emitter.instruction("cmp x6, #0");                              // did the negative length cross before the window start?
            ctx.emitter.instruction("csel x6, xzr, x6, lt");                    // php-src clamps an underflowing length to zero
            ctx.emitter.instruction(&format!("b {}", length_done));             // join the resolved-length path
            ctx.emitter.label(&length_non_negative);
            ctx.emitter.instruction("cmp x6, x2");                              // does the requested length exceed the remaining bytes?
            ctx.emitter.instruction("csel x6, x2, x6, gt");                     // php-src clamps an overlong length to the remainder
            ctx.emitter.label(&length_done);
            ctx.emitter.instruction("mov x2, x6");                              // pass only the normalized window length to the scanner
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r8, 0");                               // is the offset measured back from the subject end?
            ctx.emitter.instruction(&format!("jge {}", offset_non_negative));   // non-negative offsets are already absolute
            ctx.emitter.instruction("add r8, rsi");                             // resolve a negative offset against the subject length
            ctx.emitter.instruction("cmp r8, 0");                               // did the offset underflow the subject start?
            ctx.emitter.instruction("mov r11, 0");                              // prepare the clamped zero offset
            ctx.emitter.instruction("cmovl r8, r11");                           // php-src clamps an underflowing offset to zero
            ctx.emitter.instruction(&format!("jmp {}", offset_done));           // join the normalized offset path
            ctx.emitter.label(&offset_non_negative);
            ctx.emitter.instruction("cmp r8, rsi");                             // does the offset pass the subject end?
            ctx.emitter.instruction("cmovg r8, rsi");                           // php-src clamps an overlong offset to the end
            ctx.emitter.label(&offset_done);
            ctx.emitter.instruction("add rdi, r8");                             // advance to the normalized scan window
            ctx.emitter.instruction("sub rsi, r8");                             // compute the bytes remaining after the offset
            ctx.emitter.instruction("test r10, r10");                           // was a non-null explicit length supplied?
            ctx.emitter.instruction(&format!("jnz {}", length_present));        // normalize the explicit length when present
            ctx.emitter.instruction("mov r9, rsi");                             // omitted or null length scans to the subject end
            ctx.emitter.instruction(&format!("jmp {}", length_done));           // skip explicit-length normalization
            ctx.emitter.label(&length_present);
            ctx.emitter.instruction("cmp r9, 0");                               // is the length measured back from the remaining end?
            ctx.emitter.instruction(&format!("jge {}", length_non_negative));   // non-negative lengths are direct byte counts
            ctx.emitter.instruction("add r9, rsi");                             // resolve a negative length against the remaining bytes
            ctx.emitter.instruction("cmp r9, 0");                               // did the negative length cross before the window start?
            ctx.emitter.instruction("mov r11, 0");                              // prepare the clamped zero length
            ctx.emitter.instruction("cmovl r9, r11");                           // php-src clamps an underflowing length to zero
            ctx.emitter.instruction(&format!("jmp {}", length_done));           // join the resolved-length path
            ctx.emitter.label(&length_non_negative);
            ctx.emitter.instruction("cmp r9, rsi");                             // does the requested length exceed the remaining bytes?
            ctx.emitter.instruction("cmovg r9, rsi");                           // php-src clamps an overlong length to the remainder
            ctx.emitter.label(&length_done);
            ctx.emitter.instruction("mov rsi, r9");                             // pass only the normalized window length to the scanner
        }
    }
}

/// Returns the scratch register that carries a `strpos()`-family search's base offset.
///
/// The base is the number of haystack bytes the runtime helper never sees, so it is also
/// the value added back to a match before the result is boxed. It deliberately reuses the
/// register the offset was materialized into, which is the first argument register past
/// the haystack/needle pointer-length pairs on both supported ABIs.
fn string_position_base_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x5",
        Arch::X86_64 => "r8",
    }
}

/// Materializes a three-argument `strpos()`-family call into its runtime ABI registers.
///
/// Leaves the haystack in the primary string pointer/length pair, the needle in the
/// secondary pair, and the raw (still unnormalized) `$offset` in the scratch base register.
fn load_string_position_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let offset = expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the haystack pointer and length while the needle is materialized
            load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the needle pointer and length while the offset is materialized
            load_as_int(ctx, offset, name)?;
            ctx.emitter.instruction("mov x5, x0");                              // park the raw search offset until the haystack length is known
            ctx.emitter.instruction("ldp x3, x4, [sp], #16");                   // restore the needle into the secondary runtime string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the haystack into the primary runtime string argument
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_as_int(ctx, offset, name)?;
            ctx.emitter.instruction("mov r8, rax");                             // park the raw search offset until the haystack length is known
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");                  // restore the needle into the secondary SysV string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                  // restore the haystack into the primary SysV string argument
        }
    }
    Ok(())
}

/// Turns a raw `strpos()`-family `$offset` into a searched window plus a base offset.
///
/// Rejects, with php-src's verbatim `ValueError`, every offset that does not land inside the
/// haystack: `$offset > strlen($haystack)` in both directions, and `-$offset >
/// strlen($haystack)` for a negative one. On success the primary string pair describes the
/// bytes the runtime helper may scan and the base register holds the offset that must be
/// added back to a match.
fn emit_string_position_offset_guard(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    direction: StringPositionDirection,
) {
    let non_negative_label = ctx.next_label("strpos_offset_non_negative");
    let bad_label = ctx.next_label("strpos_offset_bad");
    let ok_label = ctx.next_label("strpos_offset_ok");
    let whole_label = ctx.next_label("strpos_offset_whole");
    match (ctx.emitter.target.arch, direction) {
        (Arch::AArch64, StringPositionDirection::Forward) => {
            ctx.emitter.instruction("cmp x5, #0");                              // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("b.ge {}", non_negative_label));   // a non-negative offset is already absolute
            ctx.emitter.instruction("add x5, x5, x2");                          // resolve a negative offset against the haystack length
            ctx.emitter.instruction("cmp x5, #0");                              // did the negative offset reach past the haystack start?
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // an offset still inside the haystack is usable
            ctx.emitter.instruction(&format!("b {}", bad_label));               // an offset before the haystack start is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp x5, x2");                              // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // an offset at or before the haystack end is usable
            ctx.emitter.label(&bad_label);
        }
        (Arch::AArch64, StringPositionDirection::Reverse) => {
            ctx.emitter.instruction("cmp x5, #0");                              // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("b.ge {}", non_negative_label));   // a non-negative offset starts the right-to-left scan
            ctx.emitter.instruction("neg x9, x5");                              // take the magnitude of the negative offset
            ctx.emitter.instruction("cmp x9, x2");                              // did the negative offset reach past the haystack start?
            ctx.emitter.instruction(&format!("b.gt {}", bad_label));            // an offset before the haystack start is rejected
            ctx.emitter.instruction("cmp x9, x4");                              // can a match still overlap the trimmed tail?
            ctx.emitter.instruction(&format!("b.lt {}", whole_label));          // a magnitude below the needle length leaves the whole haystack searchable
            ctx.emitter.instruction("add x2, x2, x5");                          // drop the trailing bytes the negative offset excludes
            ctx.emitter.instruction("add x2, x2, x4");                          // keep the bytes a match ending on the boundary still needs
            ctx.emitter.label(&whole_label);
            ctx.emitter.instruction("mov x5, #0");                              // a negative offset never slides the haystack, so matches are already absolute
            ctx.emitter.instruction(&format!("b {}", ok_label));                // the negative-offset window is ready for the runtime helper
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp x5, x2");                              // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("b.gt {}", bad_label));            // an offset past the haystack end is rejected
            ctx.emitter.instruction("add x1, x1, x5");                          // slide the haystack pointer to the first searchable byte
            ctx.emitter.instruction("sub x2, x2, x5");                          // shrink the haystack length to the searched window
            ctx.emitter.instruction(&format!("b {}", ok_label));                // the non-negative-offset window is ready for the runtime helper
            ctx.emitter.label(&bad_label);
        }
        (Arch::X86_64, StringPositionDirection::Forward) => {
            ctx.emitter.instruction("cmp r8, 0");                               // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("jge {}", non_negative_label));    // a non-negative offset is already absolute
            ctx.emitter.instruction("add r8, rsi");                             // resolve a negative offset against the haystack length
            ctx.emitter.instruction("cmp r8, 0");                               // did the negative offset reach past the haystack start?
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // an offset still inside the haystack is usable
            ctx.emitter.instruction(&format!("jmp {}", bad_label));             // an offset before the haystack start is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp r8, rsi");                             // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // an offset at or before the haystack end is usable
            ctx.emitter.label(&bad_label);
        }
        (Arch::X86_64, StringPositionDirection::Reverse) => {
            ctx.emitter.instruction("cmp r8, 0");                               // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("jge {}", non_negative_label));    // a non-negative offset starts the right-to-left scan
            ctx.emitter.instruction("mov r10, r8");                             // copy the negative offset before taking its magnitude
            ctx.emitter.instruction("neg r10");                                 // take the magnitude of the negative offset
            ctx.emitter.instruction("cmp r10, rsi");                            // did the negative offset reach past the haystack start?
            ctx.emitter.instruction(&format!("jg {}", bad_label));              // an offset before the haystack start is rejected
            ctx.emitter.instruction("cmp r10, rcx");                            // can a match still overlap the trimmed tail?
            ctx.emitter.instruction(&format!("jl {}", whole_label));            // a magnitude below the needle length leaves the whole haystack searchable
            ctx.emitter.instruction("add rsi, r8");                             // drop the trailing bytes the negative offset excludes
            ctx.emitter.instruction("add rsi, rcx");                            // keep the bytes a match ending on the boundary still needs
            ctx.emitter.label(&whole_label);
            ctx.emitter.instruction("xor r8d, r8d");                            // a negative offset never slides the haystack, so matches are already absolute
            ctx.emitter.instruction(&format!("jmp {}", ok_label));              // the negative-offset window is ready for the runtime helper
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp r8, rsi");                             // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("jg {}", bad_label));              // an offset past the haystack end is rejected
            ctx.emitter.instruction("add rdi, r8");                             // slide the haystack pointer to the first searchable byte
            ctx.emitter.instruction("sub rsi, r8");                             // shrink the haystack length to the searched window
            ctx.emitter.instruction(&format!("jmp {}", ok_label));              // the non-negative-offset window is ready for the runtime helper
            ctx.emitter.label(&bad_label);
        }
    }
    super::super::exceptions::emit_value_error(
        ctx,
        &format!("{}{}", name, STRING_POSITION_OFFSET_OUT_OF_RANGE_SUFFIX),
    );
    ctx.emitter.label(&ok_label);
    if direction == StringPositionDirection::Forward {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("add x1, x1, x5");                      // slide the haystack pointer to the first searchable byte
                ctx.emitter.instruction("sub x2, x2, x5");                      // shrink the haystack length to the searched window
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("add rdi, r8");                         // slide the haystack pointer to the first searchable byte
                ctx.emitter.instruction("sub rsi, r8");                         // shrink the haystack length to the searched window
            }
        }
    }
}

/// Turns a window-relative `strpos()`-family match back into a haystack-absolute offset.
///
/// The runtime helper only ever saw the searched window, so a found position has to gain the
/// base offset again. The not-found sentinel is signed and must survive untouched, which is
/// why the addition is branched over instead of applied unconditionally.
fn emit_string_position_rebase(ctx: &mut FunctionContext<'_>, name: &str) {
    let done_label = ctx.next_label(&format!("{}_rebase_done", name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a window-relative match from the not-found sentinel
            ctx.emitter.instruction(&format!("b.lt {}", done_label));           // leave the not-found sentinel alone
            ctx.emitter.instruction("add x0, x0, x5");                          // restore the haystack-absolute offset of the match
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a window-relative match from the not-found sentinel
            ctx.emitter.instruction(&format!("jl {}", done_label));             // leave the not-found sentinel alone
            ctx.emitter.instruction("add rax, r8");                             // restore the haystack-absolute offset of the match
        }
    }
    ctx.emitter.label(&done_label);
}

/// Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.
pub(crate) fn lower_substr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    let length_kind = optional_null_length_kind(ctx, inst, 2)?;
    let neg_done = ctx.next_label("substr_neg_done");
    let len_done = ctx.next_label("substr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_aarch64(ctx, inst, length_kind, &neg_done, &len_done)?,
        Arch::X86_64 => lower_substr_x86_64(ctx, inst, length_kind, &neg_done, &len_done)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `substr_replace(string, replacement, start, length?)`.
pub(crate) fn lower_substr_replace(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 3 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr_replace expected 3 or 4 args, got {}",
            inst.operands.len()
        )));
    }
    let length_kind = optional_null_length_kind(ctx, inst, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_replace_aarch64(ctx, inst, length_kind)?,
        Arch::X86_64 => lower_substr_replace_x86_64(ctx, inst, length_kind)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_substr_replace");
    store_if_result(ctx, inst)
}

/// Lowers `substr_count(haystack, needle, offset?, length?)` through the shared counter.
///
/// `$offset`/`$length` are normalized here rather than inside `__rt_substr_count` because
/// every out-of-range value is a catchable `ValueError` in reference PHP, and only the
/// backend can emit a throw the surrounding `try` will see. The helper therefore receives a
/// window that is already known to sit inside the subject.
pub(crate) fn lower_substr_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr_count expected 2 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    let length_kind = optional_null_length_kind(ctx, inst, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_count_aarch64(ctx, inst, length_kind)?,
        Arch::X86_64 => lower_substr_count_x86_64(ctx, inst, length_kind)?,
    }
    emit_substr_count_argument_guards(ctx, length_kind);
    abi::emit_call_label(ctx.emitter, "__rt_substr_count");
    store_if_result(ctx, inst)
}

/// Classifies whether an optional string-builtin `$length` is absent, concrete, or nullable.
///
/// PHP's default is `null`, meaning "to the end of the subject", and an explicitly written
/// `null` behaves identically. A `TaggedScalar` needs a runtime tag check, while a statically
/// null operand can immediately take the absent path.
fn optional_null_length_kind(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    operand_index: usize,
) -> Result<OptionalLengthKind> {
    let Some(length) = inst.operands.get(operand_index) else {
        return Ok(OptionalLengthKind::Absent);
    };
    let raw_type = ctx.value_php_type(*length)?;
    let codegen_type = raw_type.codegen_repr();
    match codegen_type {
        PhpType::Void | PhpType::Never => Ok(OptionalLengthKind::Absent),
        PhpType::TaggedScalar => Ok(OptionalLengthKind::TaggedScalar),
        PhpType::Mixed | PhpType::Union(_) => Ok(OptionalLengthKind::Boxed),
        _ => Ok(OptionalLengthKind::Concrete),
    }
}

/// Compile-time representation of a nullable optional string length.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OptionalLengthKind {
    /// The operand is omitted or statically null.
    Absent,
    /// The operand has a statically non-null representation.
    Concrete,
    /// The operand is an inline runtime `int|null` tagged scalar.
    TaggedScalar,
    /// The operand is a boxed `Mixed`/union value requiring a runtime null-tag check.
    Boxed,
}

/// Loads an optional length as an integer, mapping runtime null to `i64::MAX`.
///
/// The substring paths already clamp oversized positive lengths to the remaining subject, so
/// this gives runtime null the same through-the-end behavior as omission without consuming zero
/// or any negative integer as a sentinel. Runtime-nullable forms also leave their null/int tag
/// in the tagged-scalar tag register for `substr_count()`'s remaining-window selection.
fn load_optional_length_null_as_max(
    ctx: &mut FunctionContext<'_>,
    length: ValueId,
    name: &str,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    match length_kind {
        OptionalLengthKind::Absent | OptionalLengthKind::Concrete => {
            return load_as_int(ctx, length, name);
        }
        OptionalLengthKind::Boxed => {
            return load_mixed_weak_int_null_as_max(ctx, length, name);
        }
        OptionalLengthKind::TaggedScalar => {}
    }
    ctx.load_value_to_result(length)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x9", i64::MAX);
            ctx.emitter.instruction(&format!("cmp x1, #{}", TAGGED_SCALAR_TAG_NULL)); // check whether the optional length carries the runtime null tag
            ctx.emitter.instruction("csel x0, x9, x0, eq");                     // map only runtime null to the through-the-end length
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "r11", i64::MAX);
            ctx.emitter.instruction(&format!("cmp rdx, {}", TAGGED_SCALAR_TAG_NULL)); // check whether the optional length carries the runtime null tag
            ctx.emitter.instruction("cmove rax, r11");                          // map only runtime null to the through-the-end length
        }
    }
    Ok(())
}

/// Materializes AArch64 `substr_count()` arguments into the counter's ABI registers.
///
/// Leaves `x1`/`x2` = subject, `x3`/`x4` = needle, `x5` = raw `$offset`, and `x6` = raw
/// `$length` when one was supplied. The guards that follow turn the subject plus the raw
/// offset/length pair into the window the runtime helper scans.
fn lower_substr_count_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    let haystack = expect_operand(inst, 0)?;
    let needle = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, haystack, "substr_count", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject string while materializing the remaining arguments
    load_value_as_string_to_regs(ctx, needle, "substr_count", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the needle string while materializing the window bounds
    if inst.operands.len() >= 3 {
        let offset = expect_operand(inst, 2)?;
        load_as_int(ctx, offset, "substr_count offset")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
    }
    abi::emit_push_reg(ctx.emitter, "x0");
    match length_kind {
        OptionalLengthKind::Absent => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
        }
        OptionalLengthKind::Concrete => {
            let length = expect_operand(inst, 3)?;
            load_as_int(ctx, length, "substr_count length")?;
        }
        OptionalLengthKind::TaggedScalar | OptionalLengthKind::Boxed => {
            let length = expect_operand(inst, 3)?;
            load_optional_length_null_as_max(ctx, length, "substr_count length", length_kind)?;
            ctx.emitter.instruction("mov x7, x1");                              // preserve the optional count length's runtime tag
        }
    }
    ctx.emitter.instruction("mov x6, x0");                                      // park the raw window length until the subject length is known
    abi::emit_pop_reg(ctx.emitter, "x5");
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the needle into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the subject into the primary runtime string argument
    Ok(())
}

/// Materializes x86_64 `substr_count()` arguments into the counter's ABI registers.
///
/// Leaves `rdi`/`rsi` = subject, `rdx`/`rcx` = needle, `r8` = raw `$offset`, and `r9` = raw
/// `$length` when one was supplied, mirroring the AArch64 emitter's register roles.
fn lower_substr_count_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    let haystack = expect_operand(inst, 0)?;
    let needle = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, haystack, "substr_count", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_value_as_string_to_regs(ctx, needle, "substr_count", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    if inst.operands.len() >= 3 {
        let offset = expect_operand(inst, 2)?;
        load_as_int(ctx, offset, "substr_count offset")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    }
    abi::emit_push_reg(ctx.emitter, "rax");
    match length_kind {
        OptionalLengthKind::Absent => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
        }
        OptionalLengthKind::Concrete => {
            let length = expect_operand(inst, 3)?;
            load_as_int(ctx, length, "substr_count length")?;
        }
        OptionalLengthKind::TaggedScalar | OptionalLengthKind::Boxed => {
            let length = expect_operand(inst, 3)?;
            load_optional_length_null_as_max(ctx, length, "substr_count length", length_kind)?;
            ctx.emitter.instruction("mov r11, rdx");                            // preserve the optional count length's runtime tag
        }
    }
    ctx.emitter.instruction("mov r9, rax");                                     // park the raw window length until the subject length is known
    abi::emit_pop_reg(ctx.emitter, "r8");
    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    Ok(())
}

/// Validates and normalizes the `substr_count()` window, raising PHP's `ValueError`s.
///
/// php-src checks in exactly this order: the empty `$needle` first, then `$offset` (negative
/// values count back from the subject end and must not underflow it, positive values must not
/// pass its end), then `$length` (negative values are measured back from the subject end, so
/// they are added to the bytes remaining after `$offset`, and neither direction may leave the
/// subject). Afterwards the subject registers hold the window the counter scans.
fn emit_substr_count_argument_guards(
    ctx: &mut FunctionContext<'_>,
    length_kind: OptionalLengthKind,
) {
    emit_substr_count_needle_guard(ctx);
    emit_substr_count_offset_guard(ctx);
    emit_substr_count_length_guard(ctx, length_kind);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x1, x1, x5");                          // slide the subject pointer to the start of the counted window
            ctx.emitter.instruction("mov x2, x6");                              // pass the resolved window length to the counter
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rdi, r8");                             // slide the subject pointer to the start of the counted window
            ctx.emitter.instruction("mov rsi, r9");                             // pass the resolved window length to the counter
        }
    }
}

/// Rejects the empty `substr_count()` needle reference PHP refuses to count.
fn emit_substr_count_needle_guard(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("substr_count_needle_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x4, {}", ok_label));         // a non-empty needle can be counted
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rcx, rcx");                           // is the needle zero-length?
            ctx.emitter.instruction(&format!("jnz {}", ok_label));              // a non-empty needle can be counted
        }
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COUNT_EMPTY_NEEDLE_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Normalizes `substr_count()`'s `$offset` and rejects one that leaves the subject.
fn emit_substr_count_offset_guard(ctx: &mut FunctionContext<'_>) {
    let non_negative_label = ctx.next_label("substr_count_offset_non_negative");
    let bad_label = ctx.next_label("substr_count_offset_bad");
    let ok_label = ctx.next_label("substr_count_offset_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x5, #0");                              // is the requested offset measured from the subject end?
            ctx.emitter.instruction(&format!("b.ge {}", non_negative_label));   // a non-negative offset is already absolute
            ctx.emitter.instruction("add x5, x5, x2");                          // resolve a negative offset against the subject length
            ctx.emitter.instruction("cmp x5, #0");                              // did the negative offset reach past the subject start?
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // an offset still inside the subject is usable
            ctx.emitter.instruction(&format!("b {}", bad_label));               // an offset before the subject start is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp x5, x2");                              // compare the absolute offset against the subject length
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // an offset at or before the subject end is usable
            ctx.emitter.label(&bad_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r8, 0");                               // is the requested offset measured from the subject end?
            ctx.emitter.instruction(&format!("jge {}", non_negative_label));    // a non-negative offset is already absolute
            ctx.emitter.instruction("add r8, rsi");                             // resolve a negative offset against the subject length
            ctx.emitter.instruction("cmp r8, 0");                               // did the negative offset reach past the subject start?
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // an offset still inside the subject is usable
            ctx.emitter.instruction(&format!("jmp {}", bad_label));             // an offset before the subject start is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp r8, rsi");                             // compare the absolute offset against the subject length
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // an offset at or before the subject end is usable
            ctx.emitter.label(&bad_label);
        }
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COUNT_OFFSET_OUT_OF_RANGE_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Resolves `substr_count()`'s `$length` into a window size and rejects out-of-subject values.
///
/// With no explicit `$length` the window simply runs to the subject end. Otherwise a negative
/// length is measured back from that end, which is why it is added to the remaining byte count
/// rather than to the offset.
fn emit_substr_count_length_guard(
    ctx: &mut FunctionContext<'_>,
    length_kind: OptionalLengthKind,
) {
    let non_negative_label = ctx.next_label("substr_count_length_non_negative");
    let bad_label = ctx.next_label("substr_count_length_bad");
    let ok_label = ctx.next_label("substr_count_length_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x2, x5");                          // compute the bytes remaining after the resolved offset
            if length_kind == OptionalLengthKind::Absent {
                ctx.emitter.instruction("mov x6, x9");                          // an omitted or null length runs to the subject end
                return;
            }
            if matches!(
                length_kind,
                OptionalLengthKind::TaggedScalar | OptionalLengthKind::Boxed
            ) {
                ctx.emitter.instruction(&format!("cmp x7, #{}", TAGGED_SCALAR_TAG_NULL)); // does the saved length tag represent runtime null?
                ctx.emitter.instruction("csel x6, x9, x6, eq");                 // runtime null selects all bytes remaining after the offset
            }
            ctx.emitter.instruction("cmp x6, #0");                              // is the requested length measured back from the subject end?
            ctx.emitter.instruction(&format!("b.ge {}", non_negative_label));   // a non-negative length is already a window size
            ctx.emitter.instruction("add x6, x6, x9");                          // resolve a negative length against the remaining bytes
            ctx.emitter.instruction("cmp x6, #0");                              // did the negative length cross back before the offset?
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // a window that still has a non-negative size is usable
            ctx.emitter.instruction(&format!("b {}", bad_label));               // a window that ends before it starts is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp x6, x9");                              // compare the requested window against the remaining bytes
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // a window inside the subject is usable
            ctx.emitter.label(&bad_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rsi");                            // copy the subject length before deriving the remaining bytes
            ctx.emitter.instruction("sub r10, r8");                             // compute the bytes remaining after the resolved offset
            if length_kind == OptionalLengthKind::Absent {
                ctx.emitter.instruction("mov r9, r10");                         // an omitted or null length runs to the subject end
                return;
            }
            if matches!(
                length_kind,
                OptionalLengthKind::TaggedScalar | OptionalLengthKind::Boxed
            ) {
                ctx.emitter.instruction(&format!("cmp r11, {}", TAGGED_SCALAR_TAG_NULL)); // does the saved length tag represent runtime null?
                ctx.emitter.instruction("cmove r9, r10");                       // runtime null selects all bytes remaining after the offset
            }
            ctx.emitter.instruction("cmp r9, 0");                               // is the requested length measured back from the subject end?
            ctx.emitter.instruction(&format!("jge {}", non_negative_label));    // a non-negative length is already a window size
            ctx.emitter.instruction("add r9, r10");                             // resolve a negative length against the remaining bytes
            ctx.emitter.instruction("cmp r9, 0");                               // did the negative length cross back before the offset?
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // a window that still has a non-negative size is usable
            ctx.emitter.instruction(&format!("jmp {}", bad_label));             // a window that ends before it starts is rejected
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp r9, r10");                             // compare the requested window against the remaining bytes
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // a window inside the subject is usable
            ctx.emitter.label(&bad_label);
        }
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COUNT_LENGTH_OUT_OF_RANGE_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Lowers `substr_compare(haystack, needle, offset, length?, case_insensitive?)`.
///
/// `$offset` and `$length` are normalized here rather than inside `__rt_substr_compare`
/// because both out-of-range values are catchable `ValueError`s in reference PHP, and only
/// the backend can emit a throw the surrounding `try` will see. The helper therefore receives
/// a window that is already known to sit inside the haystack, plus the resolved comparison
/// length.
///
/// php-src's validation order is OBSERVABLE and reproduced exactly: `$length` first (a
/// negative one is refused outright, unlike `substr_count()`, which measures it back from the
/// subject end), then `$offset`. `substr_compare("abc", "a", 100, -1)` reports the `$length`
/// error, not the `$offset` one (php 8.5.10, measured).
pub(crate) fn lower_substr_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 3 || inst.operands.len() > 5 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr_compare expected 3 to 5 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_compare_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_substr_compare_x86_64(ctx, inst)?,
    }
    emit_substr_compare_argument_guards(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_substr_compare");
    store_if_result(ctx, inst)
}

/// Materializes the raw `$length` and its presence flag onto the temporary stack, value first.
///
/// PHP's default is `null`, meaning "compare to the end of the longer operand", and an
/// explicitly written `null` behaves identically -- but so does a `?int` variable that merely
/// HOLDS null at run time, which `substr_count()`'s static-only test gets wrong. The presence
/// flag is therefore a real run-time value: a statically-null operand folds to the `0`
/// immediate, and anything that could be null at run time is tested with `is_null()`'s own
/// lowering before the integer coercion that would otherwise turn `null` into `0`.
fn materialize_substr_compare_length(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let Some(length) = inst.operands.get(3).copied() else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        abi::emit_push_reg(ctx.emitter, result_reg);                            // an omitted length parks a zero the guards never read
        abi::emit_push_reg(ctx.emitter, result_reg);                            // ... behind a cleared presence flag
        return Ok(());
    };
    if matches!(
        ctx.value_php_type(length)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        abi::emit_push_reg(ctx.emitter, result_reg);                            // a literal `null` length parks a zero the guards never read
        abi::emit_push_reg(ctx.emitter, result_reg);                            // ... behind a cleared presence flag
        return Ok(());
    }
    let absent_label = ctx.next_label("substr_compare_length_absent");
    let done_label = ctx.next_label("substr_compare_length_done");
    predicates::emit_is_null_result(ctx, length)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", absent_label));     // a run-time null length means "to the end", exactly like an omitted one
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did `is_null()` answer true for the length argument?
            ctx.emitter.instruction(&format!("jnz {}", absent_label));          // a run-time null length means "to the end", exactly like an omitted one
        }
    }
    load_as_int(ctx, length, "substr_compare length")?;
    abi::emit_push_reg(ctx.emitter, result_reg);                                // park the coerced length
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_push_reg(ctx.emitter, result_reg);                                // ... behind a raised presence flag
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&absent_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_push_reg(ctx.emitter, result_reg);                                // the null arm parks the same zero pair as an omitted argument
    abi::emit_push_reg(ctx.emitter, result_reg);                                // ... so both arms leave the stack at one depth
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes AArch64 `substr_compare()` arguments into the comparer's ABI registers.
///
/// Leaves `x1`/`x2` = haystack, `x3`/`x4` = needle, `x5` = raw `$offset`, `x6` = raw
/// `$length`, `x7` = the `$length` presence flag, and `x10` = the `$case_insensitive` flag.
/// Every operand is parked on the temporary stack while the next one is materialized, because
/// coercing a non-string or non-integer argument can call runtime helpers that clobber the
/// very registers the earlier operands occupy.
fn lower_substr_compare_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let haystack = expect_operand(inst, 0)?;
    let needle = expect_operand(inst, 1)?;
    let offset = expect_operand(inst, 2)?;
    materialize_truthy_flag(ctx, inst, 4, "substr_compare")?;
    abi::emit_push_reg(ctx.emitter, "x0");                                      // park the case-insensitivity flag before any string coercion runs
    load_value_as_string_to_regs(ctx, haystack, "substr_compare", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack string while materializing the remaining arguments
    load_value_as_string_to_regs(ctx, needle, "substr_compare", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the needle string while materializing the window bounds
    load_as_int(ctx, offset, "substr_compare offset")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_substr_compare_length(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "x7");                                       // restore the length presence flag
    abi::emit_pop_reg(ctx.emitter, "x6");                                       // restore the raw window length
    abi::emit_pop_reg(ctx.emitter, "x5");                                       // restore the raw offset
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the needle into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack into the primary runtime string argument
    abi::emit_pop_reg(ctx.emitter, "x10");                                      // restore the case-insensitivity flag
    Ok(())
}

/// Materializes x86_64 `substr_compare()` arguments into the comparer's ABI registers.
///
/// Leaves `rdi`/`rsi` = haystack, `rdx`/`rcx` = needle, `r8` = raw `$offset`, `r9` = raw
/// `$length`, `r10` = the `$length` presence flag, and `r11` = the `$case_insensitive` flag,
/// mirroring the AArch64 emitter's register roles.
fn lower_substr_compare_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let haystack = expect_operand(inst, 0)?;
    let needle = expect_operand(inst, 1)?;
    let offset = expect_operand(inst, 2)?;
    materialize_truthy_flag(ctx, inst, 4, "substr_compare")?;
    abi::emit_push_reg(ctx.emitter, "rax");                                     // park the case-insensitivity flag before any string coercion runs
    load_value_as_string_to_regs(ctx, haystack, "substr_compare", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_value_as_string_to_regs(ctx, needle, "substr_compare", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, offset, "substr_compare offset")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_substr_compare_length(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "r10");                                      // restore the length presence flag
    abi::emit_pop_reg(ctx.emitter, "r9");                                       // restore the raw window length
    abi::emit_pop_reg(ctx.emitter, "r8");                                       // restore the raw offset
    abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");                          // restore the needle into the secondary runtime string argument
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                          // restore the haystack into the primary runtime string argument
    abi::emit_pop_reg(ctx.emitter, "r11");                                      // restore the case-insensitivity flag
    Ok(())
}

/// Validates and normalizes the `substr_compare()` window, raising PHP's `ValueError`s.
///
/// php-src checks `$length` first (it must be non-negative when given at all), then `$offset`
/// (a negative one counts back from the haystack end and CLAMPS to zero when it underflows --
/// `substr_compare("abcdef", "def", -100)` is `-3` in php, not an error -- while a positive one
/// may reach the end but not pass it). Afterwards the haystack registers hold the compared
/// window and the comparison length is resolved.
fn emit_substr_compare_argument_guards(ctx: &mut FunctionContext<'_>) {
    emit_substr_compare_length_guard(ctx);
    emit_substr_compare_offset_guard(ctx);
    emit_substr_compare_window(ctx);
}

/// Rejects the negative `substr_compare()` `$length` reference PHP refuses outright.
///
/// The check is skipped entirely when no `$length` was supplied, because the parked zero that
/// stands in for an omitted argument is not a value php ever validates.
fn emit_substr_compare_length_guard(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("substr_compare_length_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x7, {}", ok_label));          // an omitted or null length is never range-checked
            ctx.emitter.instruction("cmp x6, #0");                              // is the supplied length negative?
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // a non-negative length is accepted
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test r10, r10");                           // was a length supplied at all?
            ctx.emitter.instruction(&format!("jz {}", ok_label));               // an omitted or null length is never range-checked
            ctx.emitter.instruction("cmp r9, 0");                               // is the supplied length negative?
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // a non-negative length is accepted
        }
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COMPARE_NEGATIVE_LENGTH_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Normalizes `substr_compare()`'s `$offset` and rejects one that passes the haystack end.
///
/// A negative offset counts back from the end and CLAMPS to zero when its magnitude exceeds
/// the haystack -- php raises nothing there, unlike `substr_count()`. Only a positive offset
/// strictly greater than `strlen($haystack)` is a `ValueError`; `$offset === strlen($haystack)`
/// is legal and compares an empty window.
fn emit_substr_compare_offset_guard(ctx: &mut FunctionContext<'_>) {
    let non_negative_label = ctx.next_label("substr_compare_offset_non_negative");
    let ok_label = ctx.next_label("substr_compare_offset_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x5, #0");                              // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("b.ge {}", non_negative_label));   // a non-negative offset is already absolute
            ctx.emitter.instruction("add x5, x5, x2");                          // resolve a negative offset against the haystack length
            ctx.emitter.instruction("cmp x5, #0");                              // did the negative offset reach past the haystack start?
            ctx.emitter.instruction("csel x5, xzr, x5, lt");                    // php CLAMPS an underflowing negative offset to zero instead of raising
            ctx.emitter.instruction(&format!("b {}", ok_label));                // the negative-offset window is ready for the runtime helper
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp x5, x2");                              // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // an offset at or before the haystack end is usable
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r8, 0");                               // is the requested offset measured from the haystack end?
            ctx.emitter.instruction(&format!("jge {}", non_negative_label));    // a non-negative offset is already absolute
            ctx.emitter.instruction("add r8, rsi");                             // resolve a negative offset against the haystack length
            ctx.emitter.instruction("xor eax, eax");                            // materialize the zero clamp without disturbing the comparison
            ctx.emitter.instruction("cmp r8, 0");                               // did the negative offset reach past the haystack start?
            ctx.emitter.instruction("cmovl r8, rax");                           // php CLAMPS an underflowing negative offset to zero instead of raising
            ctx.emitter.instruction(&format!("jmp {}", ok_label));              // the negative-offset window is ready for the runtime helper
            ctx.emitter.label(&non_negative_label);
            ctx.emitter.instruction("cmp r8, rsi");                             // compare the absolute offset against the haystack length
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // an offset at or before the haystack end is usable
        }
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COMPARE_OFFSET_OUT_OF_RANGE_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Slides the haystack registers onto the compared window and resolves the comparison length.
///
/// With no explicit `$length`, php compares `MAX(strlen($needle), strlen($haystack) - $offset)`
/// bytes -- the LONGER of the two operands, which is what makes a prefix answer `-1`/`1`
/// instead of `0`. An explicit `$length` replaces that default outright, even when it reaches
/// past both operands.
fn emit_substr_compare_window(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x2, x5");                          // compute the bytes remaining after the resolved offset
            ctx.emitter.instruction("add x1, x1, x5");                          // slide the haystack pointer to the start of the compared window
            ctx.emitter.instruction("mov x2, x9");                              // pass the window length as the first operand's length
            ctx.emitter.instruction("cmp x4, x9");                              // is the needle longer than what remains of the haystack?
            ctx.emitter.instruction("csel x11, x4, x9, gt");                    // the default comparison length is the LONGER of the two operands
            ctx.emitter.instruction("cmp x7, #0");                              // was an explicit length supplied?
            ctx.emitter.instruction("csel x5, x6, x11, ne");                    // an explicit length replaces the default outright
            ctx.emitter.instruction("mov x6, x10");                             // pass the case-insensitivity flag to the comparer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rsi");                            // copy the haystack length before deriving the remaining bytes
            ctx.emitter.instruction("sub rax, r8");                             // compute the bytes remaining after the resolved offset
            ctx.emitter.instruction("add rdi, r8");                             // slide the haystack pointer to the start of the compared window
            ctx.emitter.instruction("mov rsi, rax");                            // pass the window length as the first operand's length
            ctx.emitter.instruction("mov r8, rcx");                             // seed the default comparison length from the needle length
            ctx.emitter.instruction("cmp r8, rax");                             // is the needle longer than what remains of the haystack?
            ctx.emitter.instruction("cmovl r8, rax");                           // the default comparison length is the LONGER of the two operands
            ctx.emitter.instruction("test r10, r10");                           // was an explicit length supplied?
            ctx.emitter.instruction("cmovne r8, r9");                           // an explicit length replaces the default outright
            ctx.emitter.instruction("mov r9, r11");                             // pass the case-insensitivity flag to the comparer
        }
    }
}

/// Lowers `str_repeat(string, times)` through the shared runtime helper.
pub(crate) fn lower_str_repeat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_repeat expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_repeat_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_repeat_x86_64(ctx, inst)?,
    }
    // `__rt_str_repeat` still carries its own negative-count fatal as a backstop, but that
    // fatal is not catchable. Reference PHP raises a ValueError here, so screen the count
    // before the helper ever sees it.
    let times_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(times_reg, 0),
        STR_REPEAT_NEGATIVE_TIMES_MESSAGE,
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_repeat");
    store_if_result(ctx, inst)
}

/// Lowers `strstr(haystack, needle)` by searching and returning the matching suffix.
pub(crate) fn lower_strstr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "strstr expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        // `crate::builtins::string::strstr::check` types EVERY call `string|false`, whose
        // representation is `Mixed`, and the arms below leave a BOXED cell in the integer
        // result register. A `Str` result type here would make `store_if_result` copy the
        // string-pair registers instead, which no longer hold the answer — fail loudly rather
        // than emit that silently wrong store.
        return Err(CodegenIrError::invalid_module(format!(
            "strstr result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }
    let labels = StrstrLabels {
        prefix: ctx.next_label("strstr_prefix"),
        miss: ctx.next_label("strstr_miss"),
        box_match: ctx.next_label("strstr_box_match"),
        end: ctx.next_label("strstr_end"),
    };
    // The `$before_needle` flag is materialized FIRST and parked on the temporary stack: every
    // register that could hold it (including the caller-saved scratch the truthiness helpers
    // use) is clobbered by the haystack/needle materialization and the `__rt_strpos` call.
    materialize_truthy_flag(ctx, inst, 2, "strstr")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_strstr_aarch64(ctx, inst, &labels)?,
        Arch::X86_64 => lower_strstr_x86_64(ctx, inst, &labels)?,
    }
    ctx.emitter.label(&labels.end);
    store_if_result(ctx, inst)
}

/// Lowers `strpbrk()` through the existing byte-membership span scanner.
///
/// The scanner reports how many leading bytes are absent from `$characters`. A count equal to
/// the haystack length is PHP `false`; otherwise the remaining suffix begins at the first member.
pub(crate) fn lower_strpbrk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strpbrk expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "strpbrk result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }
    let miss = ctx.next_label("strpbrk_miss");
    let box_match = ctx.next_label("strpbrk_box_match");
    let end = ctx.next_label("strpbrk_end");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_strpbrk_aarch64(ctx, inst, &miss, &box_match, &end)?,
        Arch::X86_64 => lower_strpbrk_x86_64(ctx, inst, &miss, &box_match, &end)?,
    }
    ctx.emitter.label(&end);
    store_if_result(ctx, inst)
}

/// Emits AArch64 `strpbrk()` argument setup, span search, and boxed result selection.
fn lower_strpbrk_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss: &str,
    box_match: &str,
    end: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 1, "strpbrk", "x1", "x2")?;
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::NotEqualToImmediate("x2", 0),
        STRPBRK_EMPTY_CHARACTERS_MESSAGE,
    );
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                       // preserve the validated byte mask while materializing the haystack
    load_string_arg_to_regs(ctx, inst, 0, "strpbrk", "x1", "x2")?;
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                         // restore the mask into the span scanner argument pair
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                       // keep the haystack for suffix construction after the scan
    abi::emit_call_label(ctx.emitter, "__rt_strcspn");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                         // restore the haystack pointer and byte length after the scan
    ctx.emitter.instruction("cmp x0, x2");                                    // a complete non-member span means no character matched
    ctx.emitter.instruction(&format!("b.ge {miss}"));                          // PHP returns false when the character set never appears
    ctx.emitter.instruction("add x1, x1, x0");                                // advance the result pointer to the first matching byte
    ctx.emitter.instruction("sub x2, x2, x0");                                // retain the matching byte and every trailing byte in the suffix
    ctx.emitter.label(box_match);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("b {end}"));                              // bypass the false arm after boxing the suffix
    ctx.emitter.label(miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    Ok(())
}

/// Emits x86_64 `strpbrk()` argument setup, span search, and boxed result selection.
fn lower_strpbrk_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss: &str,
    box_match: &str,
    end: &str,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 1, "strpbrk", "rax", "rdx")?;
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::NotEqualToImmediate("rdx", 0),
        STRPBRK_EMPTY_CHARACTERS_MESSAGE,
    );
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 0, "strpbrk", "rax", "rdx")?;
    abi::emit_pop_reg_pair(ctx.emitter, "r8", "r9");
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("mov rdi, rax");                                  // pass the haystack pointer to the byte-span scanner
    ctx.emitter.instruction("mov rsi, rdx");                                  // pass the haystack byte length to the byte-span scanner
    ctx.emitter.instruction("mov rdx, r8");                                   // pass the validated character mask pointer to the scanner
    ctx.emitter.instruction("mov rcx, r9");                                   // pass the validated character mask byte length to the scanner
    abi::emit_call_label(ctx.emitter, "__rt_strcspn");
    ctx.emitter.instruction("mov r8, rax");                                   // preserve the leading non-member byte count across haystack restoration
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("cmp r8, rdx");                                   // a complete non-member span means no character matched
    ctx.emitter.instruction(&format!("jge {miss}"));                          // PHP returns false when the character set never appears
    ctx.emitter.instruction("add rax, r8");                                   // advance the result pointer to the first matching byte
    ctx.emitter.instruction("sub rdx, r8");                                   // retain the matching byte and every trailing byte in the suffix
    ctx.emitter.label(box_match);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("jmp {end}"));                            // bypass the false arm after boxing the suffix
    ctx.emitter.label(miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    Ok(())
}

/// Lowers `strrchr()` using `strrpos()` on PHP's first-needle-byte semantics.
pub(crate) fn lower_strrchr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strrchr expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "strrchr result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }
    let miss = ctx.next_label("strrchr_miss");
    let empty = ctx.next_label("strrchr_empty");
    let end = ctx.next_label("strrchr_end");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strrchr", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                // preserve haystack while loading the needle
            load_string_arg_to_regs(ctx, inst, 1, "strrchr", "x1", "x2")?;
            ctx.emitter.instruction(&format!("cbz x2, {empty}"));              // empty needles have no usable byte in this AOT path
            ctx.emitter.instruction("mov x3, x1");                             // needle first-byte pointer
            ctx.emitter.instruction("mov x4, #1");                             // PHP strrchr uses exactly the first byte
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                  // restore haystack for reverse search
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                // preserve haystack across strrpos
            abi::emit_call_label(ctx.emitter, "__rt_strrpos");
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                  // restore haystack for suffix reconstruction
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction(&format!("b.lt {miss}"));
            ctx.emitter.instruction("add x1, x1, x0");                         // suffix begins at the final matching byte
            ctx.emitter.instruction("sub x2, x2, x0");                         // suffix extends through haystack end
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {end}"));
            ctx.emitter.label(&empty);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // discard preserved haystack when needle is empty
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "strrchr", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, "strrchr", "rax", "rdx")?;
            ctx.emitter.instruction("test rdx, rdx");
            ctx.emitter.instruction(&format!("jz {empty}"));
            ctx.emitter.instruction("mov r8, rax");                            // needle first-byte pointer
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("mov rsi, rdx");
            ctx.emitter.instruction("mov rdx, r8");
            ctx.emitter.instruction("mov rcx, 1");                             // PHP strrchr uses exactly the first byte
            abi::emit_call_label(ctx.emitter, "__rt_strrpos");
            ctx.emitter.instruction("mov r8, rax");                            // preserve signed match offset
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("cmp r8, 0");
            ctx.emitter.instruction(&format!("jl {miss}"));
            ctx.emitter.instruction("add rax, r8");                            // suffix begins at the final matching byte
            ctx.emitter.instruction("sub rdx, r8");                            // suffix extends through haystack end
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {end}"));
            ctx.emitter.label(&empty);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // discard preserved haystack when needle is empty
        }
    }
    ctx.emitter.label(&miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&end);
    store_if_result(ctx, inst)
}

/// The four branch targets `lower_strstr` threads through its per-architecture emitters.
///
/// `prefix` selects the `$before_needle` substring, `miss` boxes PHP's `false`, `box_match` is
/// where both hit arms converge to box the selected substring as a string, and `end` is the
/// common continuation where the boxed `Mixed` cell is stored.
pub(super) struct StrstrLabels {
    prefix: String,
    miss: String,
    box_match: String,
    end: String,
}

/// Emits the AArch64 inline substring pointer/length calculation.
pub(super) fn lower_substr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_aarch64(ctx, inst)?;
    // Omitted and statically-null lengths skip the clamp. Runtime-tagged null maps to a
    // saturating positive length, while every integer payload remains a real PHP length.
    if length_kind != OptionalLengthKind::Absent {
        let length = expect_operand(inst, 2)?;
        load_optional_length_null_as_max(ctx, length, "substr length", length_kind)?;
        ctx.emitter.instruction("mov x3, x0");                                  // move the explicit substring length into the clamp register
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the substring offset after optional length materialization
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string pointer and length
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("b.ge {}", neg_done));                     // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add x0, x2, x0");                                  // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("csel x0, xzr, x0, lt");                            // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp x0, x2");                                      // compare the final offset against the full source-string length
    ctx.emitter.instruction("csel x0, x2, x0, gt");                             // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub x2, x2, x0");                                  // compute the remaining byte length after the selected offset
    if length_kind != OptionalLengthKind::Absent {
        // A NEGATIVE length is php's "stop this many bytes before the end", counted from the
        // remaining tail — not an error and not zero.
        ctx.emitter.instruction("cmp x3, #0");                                  // check whether the requested substring length is negative
        ctx.emitter.instruction(&format!("b.ge {}", len_done));                 // a non-negative length is already a byte count
        ctx.emitter.instruction("add x3, x2, x3");                              // omit that many bytes from the end of the remaining tail
        ctx.emitter.instruction("cmp x3, #0");                                  // check whether more bytes were omitted than remain
        ctx.emitter.instruction("csel x3, xzr, x3, lt");                        // an over-long omission selects the empty string
        ctx.emitter.label(len_done);
        ctx.emitter.instruction("cmp x3, x2");                                  // compare requested length against the remaining tail length
        ctx.emitter.instruction("csel x2, x3, x2, lt");                         // shrink the result length when the requested length is shorter
    }
    Ok(())
}

/// Loads the source string and offset for AArch64 `substr()` lowering.
pub(super) fn load_substr_string_and_offset_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let offset = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "substr", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing numeric arguments
    load_as_int(ctx, offset, "substr offset")?;
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the substring offset while materializing the optional length
    Ok(())
}

/// Emits the x86_64 inline substring pointer/length calculation.
pub(super) fn lower_substr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_x86_64(ctx, inst)?;
    // See the AArch64 sibling: omitted and statically-null lengths both skip the clamp.
    if length_kind != OptionalLengthKind::Absent {
        let length = expect_operand(inst, 2)?;
        load_optional_length_null_as_max(ctx, length, "substr length", length_kind)?;
        ctx.emitter.instruction("mov rcx, rax");                                // move the explicit substring length into the clamp register
    }
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("jge {}", neg_done));                      // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add rax, rsi");                                    // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("mov r8, 0");                                       // materialize zero for offset and length clamping
    ctx.emitter.instruction("cmovl rax, r8");                                   // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp rax, rsi");                                    // compare the final offset against the full source-string length
    ctx.emitter.instruction("cmovg rax, rsi");                                  // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add rdi, rax");                                    // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub rsi, rax");                                    // compute the remaining byte length after the selected offset
    if length_kind != OptionalLengthKind::Absent {
        // A NEGATIVE length is php's "stop this many bytes before the end", counted from the
        // remaining tail — not an error and not zero.
        ctx.emitter.instruction("cmp rcx, 0");                                  // check whether the requested substring length is negative
        ctx.emitter.instruction(&format!("jge {}", len_done));                  // a non-negative length is already a byte count
        ctx.emitter.instruction("add rcx, rsi");                                // omit that many bytes from the end of the remaining tail
        ctx.emitter.instruction("cmp rcx, 0");                                  // check whether more bytes were omitted than remain
        ctx.emitter.instruction("mov r8, 0");                                   // materialize zero for the over-omission clamp
        ctx.emitter.instruction("cmovl rcx, r8");                               // an over-long omission selects the empty string
        ctx.emitter.label(len_done);
        ctx.emitter.instruction("cmp rcx, rsi");                                // compare requested length against the remaining tail length
        ctx.emitter.instruction("cmovl rsi, rcx");                              // shrink the result length when the requested length is shorter
    }
    ctx.emitter.instruction("mov rax, rdi");                                    // return the selected substring pointer in the string result register
    ctx.emitter.instruction("mov rdx, rsi");                                    // return the selected substring length in the string result register
    Ok(())
}

/// Loads the source string and offset for x86_64 `substr()` lowering.
pub(super) fn load_substr_string_and_offset_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let offset = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "substr", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, offset, "substr offset")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    Ok(())
}

/// Materializes AArch64 `str_repeat()` runtime arguments.
pub(super) fn lower_str_repeat_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the repeat count
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the repeat count as the third string-helper argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string into runtime argument registers
    Ok(())
}

/// Materializes x86_64 `str_repeat()` runtime arguments.
pub(super) fn lower_str_repeat_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the repeat count as the extra x86_64 runtime argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Emits AArch64 `strstr()` search and suffix reconstruction.
pub(super) fn lower_strstr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    labels: &StrstrLabels,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "strstr", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while materializing the needle string
    load_string_arg_to_regs(ctx, inst, 1, "strstr", "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the needle pointer as the secondary string argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the needle length as the secondary string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack into primary string argument registers
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while strpos() returns the match offset
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack for substring reconstruction
    ctx.emitter.instruction("ldr x9, [sp], #16");                               // reload the parked $before_needle flag now that every call is done
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("b.lt {}", labels.miss));                  // PHP returns false, not "", when the needle is absent
    ctx.emitter.instruction("cmp x9, #0");                                      // was $before_needle truthy?
    ctx.emitter.instruction(&format!("b.ne {}", labels.prefix));                // a truthy flag selects the part BEFORE the needle
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub x2, x2, x0");                                  // shrink the haystack length to the matching suffix length
    ctx.emitter.instruction(&format!("b {}", labels.box_match));                // both hit arms box the selected substring identically
    ctx.emitter.label(&labels.prefix);
    ctx.emitter.instruction("mov x2, x0");                                      // keep the haystack pointer and cut the length at the match offset
    ctx.emitter.label(&labels.box_match);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("b {}", labels.end));                      // skip the miss arm once the substring is boxed
    ctx.emitter.label(&labels.miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    Ok(())
}

/// Emits x86_64 `strstr()` search and suffix reconstruction.
pub(super) fn lower_strstr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    labels: &StrstrLabels,
) -> Result<()> {
    load_string_arg_to_regs(ctx, inst, 0, "strstr", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_arg_to_regs(ctx, inst, 1, "strstr", "rax", "rdx")?;
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the needle pointer while restoring the haystack
    ctx.emitter.instruction("mov r9, rdx");                                     // preserve the needle length while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the haystack pointer as the first SysV string argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the haystack length as the second SysV string argument
    ctx.emitter.instruction("mov rdx, r8");                                     // pass the needle pointer as the third SysV string argument
    ctx.emitter.instruction("mov rcx, r9");                                     // pass the needle length as the fourth SysV string argument
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the signed match offset while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_pop_reg(ctx.emitter, "r9");                                       // reload the parked $before_needle flag now that every call is done
    ctx.emitter.instruction("cmp r8, 0");                                       // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("jl {}", labels.miss));                    // PHP returns false, not "", when the needle is absent
    ctx.emitter.instruction("cmp r9, 0");                                       // was $before_needle truthy?
    ctx.emitter.instruction(&format!("jne {}", labels.prefix));                 // a truthy flag selects the part BEFORE the needle
    ctx.emitter.instruction("add rax, r8");                                     // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub rdx, r8");                                     // shrink the haystack length to the matching suffix length
    ctx.emitter.instruction(&format!("jmp {}", labels.box_match));              // both hit arms box the selected substring identically
    ctx.emitter.label(&labels.prefix);
    ctx.emitter.instruction("mov rdx, r8");                                     // keep the haystack pointer and cut the length at the match offset
    ctx.emitter.label(&labels.box_match);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.instruction(&format!("jmp {}", labels.end));                    // skip the miss arm once the substring is boxed
    ctx.emitter.label(&labels.miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    Ok(())
}
/// Materializes AArch64 `substr_replace()` runtime arguments.
pub(super) fn lower_substr_replace_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    // php converts a scalar `$replace` to string (`substr_replace($h, strlen($b), 16)` in
    // Symfony's HttpClientTrait); the subject stays a string operand, so at most one value is
    // converted into scratch storage.
    let replacement = expect_operand(inst, 1)?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject string while materializing replacement and slice bounds
    load_value_as_string_to_regs(ctx, replacement, "substr_replace", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the replacement string while materializing slice bounds
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_substr_replace_length_aarch64(ctx, inst, length_kind)?;
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore replacement into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore subject into the primary runtime string argument
    Ok(())
}

/// Materializes x86_64 `substr_replace()` runtime arguments.
pub(super) fn lower_substr_replace_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    // php converts a scalar `$replace` to string (`substr_replace($h, strlen($b), 16)` in
    // Symfony's HttpClientTrait); the subject stays a string operand, so at most one value is
    // converted into scratch storage.
    let replacement = expect_operand(inst, 1)?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_value_as_string_to_regs(ctx, replacement, "substr_replace", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_substr_replace_length_x86_64(ctx, inst, length_kind)?;
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the AArch64 optional `substr_replace()` length argument.
pub(super) fn materialize_substr_replace_length_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    if length_kind != OptionalLengthKind::Absent {
        let length = expect_operand(inst, 3)?;
        load_optional_length_null_as_max(ctx, length, "substr_replace length", length_kind)?;
        ctx.emitter.instruction("mov x7, x0");                                  // pass the explicit replacement length to the runtime helper
    } else {
        // `i64::MAX`, not `-1`: the helper bounds the length by what remains, so a saturating
        // value runs through the subject end by the ordinary path. `-1` cannot serve here — it
        // is a real php length meaning "stop one byte before the end".
        abi::emit_load_int_immediate(ctx.emitter, "x7", i64::MAX);
    }
    Ok(())
}

/// Materializes the x86_64 optional `substr_replace()` length argument.
pub(super) fn materialize_substr_replace_length_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    length_kind: OptionalLengthKind,
) -> Result<()> {
    if length_kind != OptionalLengthKind::Absent {
        let length = expect_operand(inst, 3)?;
        load_optional_length_null_as_max(ctx, length, "substr_replace length", length_kind)?;
        ctx.emitter.instruction("mov r8, rax");                                 // pass the explicit replacement length to the runtime helper
    } else {
        // See the AArch64 sibling: `i64::MAX` runs through the subject end, `-1` is a real length.
        abi::emit_load_int_immediate(ctx.emitter, "r8", i64::MAX);
    }
    Ok(())
}
/// Boxes a raw string-search position result into the Mixed pointer representation.
pub(super) fn box_search_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let found_label = ctx.next_label(&format!("{}_found", label_prefix));
    let end_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found offset as an integer result
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found offset as an integer result
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}
