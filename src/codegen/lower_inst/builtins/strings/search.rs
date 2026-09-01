//! Purpose:
//! Lowers string search, substring, repetition, replacement-slice, and `strstr` builtins.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Search sentinels and optional bounds are materialized consistently for both targets.

use super::*;

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
    let neg_done = ctx.next_label("substr_neg_done");
    let len_done = ctx.next_label("substr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_aarch64(ctx, inst, &neg_done, &len_done)?,
        Arch::X86_64 => lower_substr_x86_64(ctx, inst, &neg_done, &len_done)?,
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_replace_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_substr_replace_x86_64(ctx, inst)?,
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
    let has_length = substr_count_has_length(ctx, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_count_aarch64(ctx, inst, has_length)?,
        Arch::X86_64 => lower_substr_count_x86_64(ctx, inst, has_length)?,
    }
    emit_substr_count_argument_guards(ctx, has_length);
    abi::emit_call_label(ctx.emitter, "__rt_substr_count");
    store_if_result(ctx, inst)
}

/// Reports whether `substr_count()` was given a `$length` that actually bounds the window.
///
/// PHP's default is `null`, meaning "to the end of the subject", and an explicitly written
/// `null` behaves identically. A statically-null operand (checker type `Void`/`Never`) is
/// therefore treated exactly like an omitted argument instead of being coerced to `0`, which
/// would have counted matches inside an empty window.
fn substr_count_has_length(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    let Some(length) = inst.operands.get(3) else {
        return Ok(false);
    };
    Ok(!matches!(
        ctx.value_php_type(*length)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ))
}

/// Materializes AArch64 `substr_count()` arguments into the counter's ABI registers.
///
/// Leaves `x1`/`x2` = subject, `x3`/`x4` = needle, `x5` = raw `$offset`, and `x6` = raw
/// `$length` when one was supplied. The guards that follow turn the subject plus the raw
/// offset/length pair into the window the runtime helper scans.
fn lower_substr_count_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    has_length: bool,
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
    if has_length {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_count length")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
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
    has_length: bool,
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
    if has_length {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_count length")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
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
fn emit_substr_count_argument_guards(ctx: &mut FunctionContext<'_>, has_length: bool) {
    emit_substr_count_needle_guard(ctx);
    emit_substr_count_offset_guard(ctx);
    emit_substr_count_length_guard(ctx, has_length);
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
fn emit_substr_count_length_guard(ctx: &mut FunctionContext<'_>, has_length: bool) {
    let non_negative_label = ctx.next_label("substr_count_length_non_negative");
    let bad_label = ctx.next_label("substr_count_length_bad");
    let ok_label = ctx.next_label("substr_count_length_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub x9, x2, x5");                          // compute the bytes remaining after the resolved offset
            if !has_length {
                ctx.emitter.instruction("mov x6, x9");                          // an omitted or null length runs to the subject end
                return;
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
            if !has_length {
                ctx.emitter.instruction("mov r9, r10");                         // an omitted or null length runs to the subject end
                return;
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
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_aarch64(ctx, inst)?;
    // Whether a length was PASSED is known here, at compile time, so it is never encoded in the
    // length's own value. It used to be: `-1` doubled as the "omitted" sentinel, which made an
    // explicit `substr($s, 1, -1)` indistinguishable from the two-argument call and kept the
    // whole tail. Every other negative length was then clamped to zero, so `substr("hello",0,-2)`
    // answered `""` where php answers `"hel"`.
    let has_length = inst.operands.len() >= 3;
    if has_length {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
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
    if has_length {
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
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_x86_64(ctx, inst)?;
    // See the AArch64 sibling: the "was a length passed" question is answered by the operand
    // count, never by the length's own value.
    let has_length = inst.operands.len() >= 3;
    if has_length {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
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
    if has_length {
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
    let times = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "str_repeat", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the repeat count
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the repeat count as the third string-helper argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string into runtime argument registers
    Ok(())
}

/// Materializes x86_64 `str_repeat()` runtime arguments.
pub(super) fn lower_str_repeat_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let times = expect_operand(inst, 1)?;
    load_string_arg_to_regs(ctx, inst, 0, "str_repeat", "rax", "rdx")?;
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
pub(super) fn lower_substr_replace_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    let replacement = expect_string_operand(ctx, inst, 1, "substr_replace")?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the subject string while materializing replacement and slice bounds
    ctx.load_string_value_to_regs(replacement, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the replacement string while materializing slice bounds
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_substr_replace_length_aarch64(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore replacement into the secondary runtime string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore subject into the primary runtime string argument
    Ok(())
}

/// Materializes x86_64 `substr_replace()` runtime arguments.
pub(super) fn lower_substr_replace_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let subject = expect_string_operand(ctx, inst, 0, "substr_replace")?;
    let replacement = expect_string_operand(ctx, inst, 1, "substr_replace")?;
    let start = expect_operand(inst, 2)?;
    ctx.load_string_value_to_regs(subject, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.load_string_value_to_regs(replacement, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, start, "substr_replace start")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_substr_replace_length_x86_64(ctx, inst)?;
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the AArch64 optional `substr_replace()` length argument.
pub(super) fn materialize_substr_replace_length_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_replace length")?;
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
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let length = expect_operand(inst, 3)?;
        load_as_int(ctx, length, "substr_replace length")?;
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
