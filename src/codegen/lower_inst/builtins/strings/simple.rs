//! Purpose:
//! Lowers direct string transforms, trim variants, HTML escaping, and first-byte case changes.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Direct runtime calls share coercion helpers while specialized results retain PHP boxing.

use super::*;


/// PHP's default `htmlspecialchars()` flags: `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401`.
const HTML_ESCAPE_DEFAULT_FLAGS: i64 = 3 | 8;

/// Lowers `htmlspecialchars()` / `htmlentities()` — escapes the subject string (operand 0).
/// `name` is the calling builtin's PHP name, used in argument-coercion diagnostics.
///
/// The `flags` argument (operand 1) reaches `__rt_htmlspecialchars` in the third argument
/// register: its quote bits choose which quotes are escaped, and its doctype bits choose
/// `&apos;` over `&#039;` (#645). Omitted, it is PHP's default. `encoding` and `double_encode`
/// are accepted but not applied.
pub(crate) fn lower_html_escape(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    // The flags are loaded first and parked on the stack: materializing the subject string can
    // call helpers that clobber the argument registers.
    let result_reg = abi::int_result_reg(ctx.emitter);
    if let Some(flags) = inst.operands.get(1).copied() {
        super::scalar::load_as_int(ctx, flags, name)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, HTML_ESCAPE_DEFAULT_FLAGS);
    }
    abi::emit_push_reg(ctx.emitter, result_reg);
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)?;
    let flags_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdi",
    };
    abi::emit_pop_reg(ctx.emitter, flags_reg);
    abi::emit_call_label(ctx.emitter, "__rt_htmlspecialchars");
    store_if_result(ctx, inst)
}

/// Lowers `grapheme_strrev()` and boxes its `string|false` result as `Mixed`.
pub(crate) fn lower_grapheme_strrev(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "grapheme_strrev")?;
    abi::emit_call_label(ctx.emitter, "__rt_grapheme_strrev");
    box_grapheme_strrev_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers the allocation-free `strlen(bin2hex(...))` helper.
pub(crate) fn lower_bin2hex_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "bin2hex")?;
    abi::emit_call_label(ctx.emitter, "__rt_bin2hex_len");
    store_if_result(ctx, inst)
}

/// Lowers `ucfirst()` by copying the string and uppercasing the first ASCII byte.
pub(crate) fn lower_ucfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ucfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "ucfirst", 97, 122, FirstCharAdjust::Uppercase);
    store_if_result(ctx, inst)
}

/// Lowers `lcfirst()` by copying the string and lowercasing the first ASCII byte.
pub(crate) fn lower_lcfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "lcfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "lcfirst", 65, 90, FirstCharAdjust::Lowercase);
    store_if_result(ctx, inst)
}

/// Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.
pub(crate) fn lower_trim_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    default_runtime_label: &str,
    mask_runtime_label: &str,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 or 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)?;
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, default_runtime_label);
    } else {
        lower_trim_mask_arg(ctx, inst, name)?;
        abi::emit_call_label(ctx.emitter, mask_runtime_label);
    }
    store_if_result(ctx, inst)
}

/// Lowers a two-argument string builtin that directly delegates to a runtime helper.
pub(crate) fn lower_binary_string_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    if try_lower_constant_prefix_suffix(ctx, inst, name)? {
        return Ok(());
    }
    load_binary_string_args(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Inlines a short constant prefix/suffix comparison for `str_starts_with`/`str_ends_with`.
///
/// A literal needle of at most eight bytes fits in one register. A haystack of at least eight
/// bytes is compared with one masked word load; shorter haystacks use a byte loop. Longer or
/// dynamic needles keep the libc-backed runtime helper.
fn try_lower_constant_prefix_suffix(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<bool> {
    if name != "str_starts_with" && name != "str_ends_with" {
        return Ok(false);
    }
    let haystack = expect_operand(inst, 0)?;
    let needle = expect_operand(inst, 1)?;
    if ctx.value_php_type(haystack)?.codegen_repr() != PhpType::Str {
        return Ok(false);
    }
    let Some((data_id, bytes)) = constant_string_value(ctx, needle) else {
        return Ok(false);
    };
    let length = bytes.len();
    if length > 8 {
        return Ok(false);
    }
    if length == 0 {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return store_if_result(ctx, inst).map(|()| true);
    }
    let starts = name == "str_starts_with";
    let mut padded = [0u8; 8];
    padded[..length].copy_from_slice(&bytes);
    let word = u64::from_le_bytes(padded);
    let mask = if length == 8 { u64::MAX } else { (1u64 << (length * 8)) - 1 };
    let shift = (8 - length) * 8;
    let no = ctx.next_label("const_prefix_no");
    let short = ctx.next_label("const_prefix_short");
    let short_loop = ctx.next_label("const_prefix_short_loop");
    let done = ctx.next_label("const_prefix_done");
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(haystack, "x1", "x2")?;
            ctx.emitter.instruction(&format!("cmp x2, #{}", length));           // does the haystack hold the full needle?
            ctx.emitter.instruction(&format!("b.lt {}", no));                   // a shorter haystack cannot match
            ctx.emitter.instruction("cmp x2, #8");                              // is one word load safe?
            ctx.emitter.instruction(&format!("b.lt {}", short));                // short haystacks use the byte loop
            if starts {
                ctx.emitter.instruction("ldr x9, [x1]");                        // load the first haystack word
                if length < 8 {
                    abi::emit_load_int_immediate(ctx.emitter, "x10", mask as i64);
                    ctx.emitter.instruction("and x9, x9, x10");                 // keep only the prefix bytes
                }
            } else {
                ctx.emitter.instruction("sub x9, x2, #8");                      // address the final haystack word
                ctx.emitter.instruction("add x9, x1, x9");                      // compute the suffix word address
                ctx.emitter.instruction("ldr x9, [x9]");                        // load the final haystack word
                if shift > 0 {
                    ctx.emitter.instruction(&format!("lsr x9, x9, #{}", shift));// move the suffix to the low bytes
                }
            }
            abi::emit_load_int_immediate(ctx.emitter, "x10", word as i64);
            ctx.emitter.instruction("cmp x9, x10");                             // compare the masked haystack word
            ctx.emitter.instruction(&format!("cset x0, eq"));                   // materialize the boolean result
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the byte-loop result
            ctx.emitter.label(&short);
            let (label, _) = ctx.intern_string_data(data_id)?;
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            if starts {
                ctx.emitter.instruction("mov x5, #0");                          // start the short prefix at offset zero
            } else {
                ctx.emitter.instruction(&format!("sub x10, x2, #{}", length));  // start the short suffix at the tail
                ctx.emitter.instruction("mov x5, #0");                          // start the short suffix index at zero
            }
            ctx.emitter.label(&short_loop);
            if starts {
                ctx.emitter.instruction("ldrb w6, [x1, x5]");                   // load the next haystack byte
            } else {
                ctx.emitter.instruction("add x11, x10, x5");                    // compute the suffix byte position
                ctx.emitter.instruction("ldrb w6, [x1, x11]");                  // load the next suffix byte
            }
            ctx.emitter.instruction("ldrb w7, [x3, x5]");                       // load the matching needle byte
            ctx.emitter.instruction("cmp w6, w7");                              // compare the bytes
            ctx.emitter.instruction(&format!("b.ne {}", no));                   // a mismatch answers false
            ctx.emitter.instruction("add x5, x5, #1");                          // advance the short comparison
            ctx.emitter.instruction(&format!("cmp x5, #{}", length));           // has the whole needle been compared?
            ctx.emitter.instruction(&format!("b.lt {}", short_loop));           // continue the short comparison
            ctx.emitter.instruction("mov x0, #1");                              // a full short match answers true
            ctx.emitter.instruction(&format!("b {}", done));                    // join the word-path result
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(haystack, "rdi", "rsi")?;
            ctx.emitter.instruction(&format!("cmp rsi, {}", length));           // does the haystack hold the full needle?
            ctx.emitter.instruction(&format!("jl {}", no));                     // a shorter haystack cannot match
            ctx.emitter.instruction("cmp rsi, 8");                              // is one word load safe?
            ctx.emitter.instruction(&format!("jl {}", short));                  // short haystacks use the byte loop
            if starts {
                ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");            // load the first haystack word
                if length < 8 {
                    abi::emit_load_int_immediate(ctx.emitter, "r11", mask as i64);
                    ctx.emitter.instruction("and r10, r11");                    // keep only the prefix bytes
                }
            } else {
                ctx.emitter.instruction("mov r10, rsi");                        // copy the haystack length
                ctx.emitter.instruction("sub r10, 8");                          // address the final haystack word
                ctx.emitter.instruction("mov r10, QWORD PTR [rdi + r10]");      // load the final haystack word
                if shift > 0 {
                    ctx.emitter.instruction(&format!("shr r10, {}", shift));    // move the suffix to the low bytes
                }
            }
            abi::emit_load_int_immediate(ctx.emitter, "r11", word as i64);
            ctx.emitter.instruction("cmp r10, r11");                            // compare the masked haystack word
            ctx.emitter.instruction("sete al");                                 // materialize equality in the low byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the boolean result
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the byte-loop result
            ctx.emitter.label(&short);
            let (label, _) = ctx.intern_string_data(data_id)?;
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            if starts {
                ctx.emitter.instruction("xor r10d, r10d");                      // start the short prefix at offset zero
            } else {
                ctx.emitter.instruction("mov r8, rsi");                         // copy the haystack length
                ctx.emitter.instruction(&format!("sub r8, {}", length));        // start the short suffix at the tail
                ctx.emitter.instruction("xor r10d, r10d");                      // start the short suffix index at zero
            }
            ctx.emitter.label(&short_loop);
            if starts {
                ctx.emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");     // load the next haystack byte
            } else {
                ctx.emitter.instruction("lea r11, [r8 + r10]");                 // compute the suffix byte position
                ctx.emitter.instruction("movzx eax, BYTE PTR [rdi + r11]");     // load the next suffix byte
            }
            ctx.emitter.instruction("movzx r11d, BYTE PTR [rdx + r10]");        // load the matching needle byte
            ctx.emitter.instruction("cmp eax, r11d");                           // compare the bytes
            ctx.emitter.instruction(&format!("jne {}", no));                    // a mismatch answers false
            ctx.emitter.instruction("add r10, 1");                              // advance the short comparison
            ctx.emitter.instruction(&format!("cmp r10, {}", length));           // has the whole needle been compared?
            ctx.emitter.instruction(&format!("jl {}", short_loop));             // continue the short comparison
            ctx.emitter.instruction("mov eax, 1");                              // a full short match answers true
            ctx.emitter.instruction(&format!("jmp {}", done));                  // join the word-path result
        }
    }
    ctx.emitter.label(&no);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst).map(|()| true)
}

/// Returns the data id and decoded bytes of a direct `ConstStr` value.
fn constant_string_value(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Option<(crate::ir::DataId, Vec<u8>)> {
    let ValueDef::Instruction { inst, .. } = ctx.function.value(value)?.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(data_id)) = inst.immediate else {
        return None;
    };
    let string = ctx.module.data.strings.get(data_id.as_raw() as usize)?;
    Some((
        data_id,
        elephc_builtin_contract::string_literal::literal_bytes(string),
    ))
}
/// Describes how the first-byte ASCII case helper mutates matched characters.
pub(super) enum FirstCharAdjust {
    Uppercase,
    Lowercase,
}

/// Boxes the raw `grapheme_strrev()` runtime result as PHP `string|false`.
pub(super) fn box_grapheme_strrev_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("grapheme_strrev_false");
    let done_label = ctx.next_label("grapheme_strrev_done");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box false when grapheme scanning reports a null string pointer
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a successful grapheme reversal
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for grapheme_strrev() failure
            ctx.emitter.instruction("mov x2, #0");                              // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the returned string pointer for the failure sentinel
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when grapheme scanning reports a null string pointer
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a successful grapheme reversal
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for grapheme_strrev() failure
            ctx.emitter.instruction("xor esi, esi");                            // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = bool false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Emits target-aware first-byte ASCII case adjustment for `ucfirst()` and `lcfirst()`.
pub(super) fn emit_first_char_case_adjust(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    lower_bound: u8,
    upper_bound: u8,
    adjust: FirstCharAdjust,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction(&format!("cbz x2, {}", done));              // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction("ldrb w9, [x1]");                           // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp w9, #{}", lower_bound));      // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("b.lt {}", done));                 // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp w9, #{}", upper_bound));      // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("b.gt {}", done));                 // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub w9, w9, #32");                 // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add w9, w9, #32");                 // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("strb w9, [x1]");                           // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction("test rdx, rdx");                           // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction(&format!("jz {}", done));                   // skip first-byte mutation for empty strings
            ctx.emitter.instruction("movzx ecx, BYTE PTR [rax]");               // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp cl, {}", lower_bound));       // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("jb {}", done));                   // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp cl, {}", upper_bound));       // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("ja {}", done));                   // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub cl, 32");                      // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add cl, 32");                      // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("mov BYTE PTR [rax], cl");                  // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
    }
}
