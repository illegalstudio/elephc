//! Purpose:
//! Lowers the hot explicit-UTF-8 mbstring cases to native ASCII helpers that fall back to
//! the shared Unicode bridge for any non-ASCII byte.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::strings::lower_mbstring()`.
//!
//! Key details:
//! - Case conversion reuses the bytewise case helpers only after proving every subject byte
//!   is ASCII; invalid or non-ASCII UTF-8 therefore keeps PHP's Unicode behavior.
//! - `mb_check_encoding()` over UTF-8 is answered directly from the shared validator.
//! - Every fallback stores its result through the same `store_if_result()` path, so the fast
//!   and bridge branches converge on one SSA destination.

use super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

/// Attempts one ASCII-only fast path before the shared mbstring coordinator runs.
pub(super) fn try_lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    match operation {
        RuntimeBuiltinId::MbStrtoupper => {
            try_lower_case(ctx, inst, operation, "mb_strtoupper", true)
        }
        RuntimeBuiltinId::MbStrtolower => {
            try_lower_case(ctx, inst, operation, "mb_strtolower", false)
        }
        RuntimeBuiltinId::MbConvertCase => try_lower_convert_case(ctx, inst, operation),
        RuntimeBuiltinId::MbCheckEncoding => try_lower_check_encoding(ctx, inst, operation),
        RuntimeBuiltinId::MbUcfirst => {
            try_lower_ascii_string(ctx, inst, operation, "mb_ucfirst", "__rt_mb_ucfirst_utf8")
        }
        RuntimeBuiltinId::MbLcfirst => {
            try_lower_ascii_string(ctx, inst, operation, "mb_lcfirst", "__rt_mb_lcfirst_utf8")
        }
        RuntimeBuiltinId::MbStrwidth => try_lower_strwidth(ctx, inst, operation),
        RuntimeBuiltinId::MbTrim => {
            try_lower_trim(ctx, inst, operation, "mb_trim", "__rt_trim_mask")
        }
        RuntimeBuiltinId::MbLtrim => {
            try_lower_trim(ctx, inst, operation, "mb_ltrim", "__rt_ltrim_mask")
        }
        RuntimeBuiltinId::MbRtrim => {
            try_lower_trim(ctx, inst, operation, "mb_rtrim", "__rt_rtrim_mask")
        }
        RuntimeBuiltinId::MbSubstrCount => try_lower_substr_count(ctx, inst, operation),
        RuntimeBuiltinId::MbSubstr => {
            try_lower_substr_like(ctx, inst, operation, "mb_substr")
        }
        RuntimeBuiltinId::MbStrcut => {
            try_lower_substr_like(ctx, inst, operation, "mb_strcut")
        }
        RuntimeBuiltinId::MbStrPad => try_lower_str_pad(ctx, inst, operation),
        RuntimeBuiltinId::MbOrd => try_lower_ord(ctx, inst, operation),
        RuntimeBuiltinId::MbStrpos => try_lower_position(
            ctx,
            inst,
            operation,
            "mb_strpos",
            "__rt_strpos",
            super::super::StringPositionDirection::Forward,
        ),
        RuntimeBuiltinId::MbStripos => try_lower_position(
            ctx,
            inst,
            operation,
            "mb_stripos",
            "__rt_stripos",
            super::super::StringPositionDirection::Forward,
        ),
        RuntimeBuiltinId::MbStrrpos => try_lower_position(
            ctx,
            inst,
            operation,
            "mb_strrpos",
            "__rt_strrpos",
            super::super::StringPositionDirection::Reverse,
        ),
        RuntimeBuiltinId::MbStrripos => try_lower_position(
            ctx,
            inst,
            operation,
            "mb_strripos",
            "__rt_strripos",
            super::super::StringPositionDirection::Reverse,
        ),
        RuntimeBuiltinId::MbStrstr => try_lower_strstr(ctx, inst, operation, "__rt_strpos"),
        RuntimeBuiltinId::MbStristr => try_lower_strstr(ctx, inst, operation, "__rt_stripos"),
        RuntimeBuiltinId::MbStrrchr => try_lower_strstr(ctx, inst, operation, "__rt_strrpos"),
        RuntimeBuiltinId::MbStrrichr => try_lower_strstr(ctx, inst, operation, "__rt_strripos"),
        RuntimeBuiltinId::MbScrub => try_lower_scrub(ctx, inst, operation),
        RuntimeBuiltinId::MbConvertEncoding => {
            try_lower_convert_encoding(ctx, inst, operation)
        }
        RuntimeBuiltinId::MbPreferredMimeName => {
            try_lower_preferred_mime_name(ctx, inst)
        }
        RuntimeBuiltinId::MbEncodeMimeheader => try_lower_mimeheader(ctx, inst),
        RuntimeBuiltinId::MbConvertKana => try_lower_convert_kana(ctx, inst),
        RuntimeBuiltinId::MbOutputHandler => try_lower_output_handler(ctx, inst),
        RuntimeBuiltinId::MbInternalEncoding => try_lower_string_setting_bool(
            ctx,
            inst,
            operation,
            "elephc_mbstring_internal_encoding_set_v1",
        ),
        RuntimeBuiltinId::MbLanguage => try_lower_string_setting_bool(
            ctx,
            inst,
            operation,
            "elephc_mbstring_language_set_v1",
        ),
        RuntimeBuiltinId::MbHttpOutput => try_lower_string_setting_bool(
            ctx,
            inst,
            operation,
            "elephc_mbstring_http_output_set_v1",
        ),
        RuntimeBuiltinId::MbRegexEncoding => try_lower_string_setting_bool(
            ctx,
            inst,
            operation,
            "elephc_mbstring_regex_encoding_set_v1",
        ),
        RuntimeBuiltinId::MbSubstituteCharacter => {
            try_lower_substitute_character(ctx, inst, operation)
        }
        RuntimeBuiltinId::MbEregSearchInit => try_lower_ereg_search_init(ctx, inst),
        RuntimeBuiltinId::MbEregSearch => try_lower_ereg_search_step(ctx, inst),
        RuntimeBuiltinId::MbEregSearchGetpos => lower_ereg_search_getpos(ctx, inst),
        RuntimeBuiltinId::MbEregSearchSetpos => try_lower_ereg_search_setpos(ctx, inst),
        RuntimeBuiltinId::MbEregSearchPos => try_lower_ereg_search_pos(ctx, inst),
        RuntimeBuiltinId::MbEregSearchRegs => {
            try_lower_ereg_search_regs(ctx, inst, operation, true)
        }
        RuntimeBuiltinId::MbEregSearchGetregs => {
            try_lower_ereg_search_regs(ctx, inst, operation, false)
        }
        _ => Ok(false),
    }
}

/// Lowers `mb_strtoupper()` or `mb_strtolower()` when the encoding is explicitly UTF-8.
fn try_lower_case(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    name: &str,
    uppercase: bool,
) -> Result<bool> {
    if inst.operands.len() != 2 || !super::explicit_utf8_encoding(ctx, inst.operands[1])? {
        return Ok(false);
    }
    let helper = if uppercase { "__rt_mb_strtoupper_utf8" } else { "__rt_mb_strtolower_utf8" };
    try_lower_ascii_string(ctx, inst, operation, name, helper)
}

/// Lowers a two-argument ASCII string transform with a null-pointer fallback.
fn try_lower_ascii_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    name: &str,
    helper: &str,
) -> Result<bool> {
    if inst.operands.len() != 2 || !super::explicit_utf8_encoding(ctx, inst.operands[1])? {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, helper);
    let fallback = ascii_fallback_label(ctx, "mb_ascii_string_fallback");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers `mb_strwidth()` over an explicit UTF-8 string whose bytes are all ASCII.
fn try_lower_strwidth(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 2
        || !super::explicit_utf8_encoding(ctx, inst.operands[1])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_strwidth", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ctx.next_label("mb_strwidth_ascii_fallback");
    let done = ctx.next_label("mb_strwidth_ascii_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));           // non-ASCII subjects need the width table
            ctx.emitter.instruction("mov x0, x2");                            // every ASCII codepoint has display width one
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rcx, rcx");                          // did every byte stay inside ASCII?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // non-ASCII subjects need the width table
            ctx.emitter.instruction("mov rax, rdx");                          // every ASCII codepoint has display width one
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, operation, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_convert_case()` when the mode is literal and the encoding is explicitly UTF-8.
fn try_lower_convert_case(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 3 || !super::explicit_utf8_encoding(ctx, inst.operands[2])? {
        return Ok(false);
    }
    let Some(mode) = const_int_operand(ctx, inst.operands[1])? else {
        return Ok(false);
    };
    let helper = match mode {
        0 | 4 => "__rt_mb_strtoupper_utf8",
        1 | 3 | 5 | 7 => "__rt_mb_strtolower_utf8",
        2 | 6 => "__rt_mb_convert_case_title_utf8",
        _ => return Ok(false),
    };
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_convert_case", pointer, length)?;
    abi::emit_call_label(ctx.emitter, helper);
    let fallback = ascii_fallback_label(ctx, "mb_convert_case_ascii_fallback");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers `mb_check_encoding()` over an explicit UTF-8 string without entering the bridge.
fn try_lower_check_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 2
        || !super::explicit_utf8_encoding(ctx, inst.operands[1])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_check_encoding", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_strlen_utf8");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #-1");                            // did the validator reject the subject?
            ctx.emitter.instruction("cset x0, ne");                           // a valid count means the encoding check succeeds
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                           // did the validator reject the subject?
            ctx.emitter.instruction("setne al");                              // a valid count means the encoding check succeeds
            ctx.emitter.instruction("movzx rax, al");                         // clear the stale high bits of the boolean result
        }
    }
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_trim()`/`mb_ltrim()`/`mb_rtrim()` when the subject, mask, and encoding are ASCII UTF-8.
fn try_lower_trim(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    name: &str,
    helper: &str,
) -> Result<bool> {
    if inst.operands.len() != 3
        || !super::explicit_utf8_encoding(ctx, inst.operands[2])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || ctx.value_php_type(inst.operands[1])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_trim_subject_fallback");
    load_string_arg_to_regs(ctx, inst, 1, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    // Both operands are proven ASCII; bytewise trimming is identical to character trimming.
    load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
    super::super::common::lower_trim_mask_arg(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_call_label(ctx.emitter, "__rt_str_dup_owned");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers `mb_substr_count()` when both strings and the encoding are ASCII UTF-8.
fn try_lower_substr_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 3
        || !super::explicit_utf8_encoding(ctx, inst.operands[2])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || ctx.value_php_type(inst.operands[1])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_substr_count", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_substr_count_subject_fallback");
    load_string_arg_to_regs(ctx, inst, 1, "mb_substr_count", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    super::super::common::load_binary_string_args(ctx, inst, "mb_substr_count")?;
    abi::emit_call_label(ctx.emitter, "__rt_substr_count");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers ASCII `mb_substr()`/`mb_strcut()` through the ordinary substring bounds and an owned copy.
fn try_lower_substr_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    name: &str,
) -> Result<bool> {
    if inst.operands.len() != 4
        || !super::explicit_utf8_encoding(ctx, inst.operands[3])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_substr_ascii_fallback");
    // Byte offsets equal codepoint offsets for ASCII, so reuse PHP's exact substring bounds.
    let mut synthetic = inst.clone();
    synthetic.operands.truncate(3);
    let length_kind = super::super::search::optional_null_length_kind(ctx, &synthetic, 2)?;
    let neg_done = ctx.next_label("mb_substr_neg_done");
    let len_done = ctx.next_label("mb_substr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => super::super::search::lower_substr_aarch64(
            ctx,
            &synthetic,
            length_kind,
            &neg_done,
            &len_done,
        )?,
        Arch::X86_64 => super::super::search::lower_substr_x86_64(
            ctx,
            &synthetic,
            length_kind,
            &neg_done,
            &len_done,
        )?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_dup_owned");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers ASCII `mb_str_pad()` through the ordinary byte-padding helper and an owned copy.
fn try_lower_str_pad(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 5
        || !super::explicit_utf8_encoding(ctx, inst.operands[4])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || ctx.value_php_type(inst.operands[2])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_str_pad", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_str_pad_subject_fallback");
    load_string_arg_to_regs(ctx, inst, 2, "mb_str_pad", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    // ASCII padding units are one byte wide, so PHP's byte padding matches UTF-8 padding.
    let mut synthetic = inst.clone();
    synthetic.operands.truncate(4);
    match ctx.emitter.target.arch {
        Arch::AArch64 => super::super::replace_wrap::lower_str_pad_aarch64(ctx, &synthetic)?,
        Arch::X86_64 => super::super::replace_wrap::lower_str_pad_x86_64(ctx, &synthetic)?,
    }
    super::super::emit_str_pad_argument_guards(ctx, true);
    abi::emit_call_label(ctx.emitter, "__rt_str_pad");
    abi::emit_call_label(ctx.emitter, "__rt_str_dup_owned");
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers `mb_ord()` over an explicit UTF-8 ASCII subject into a boxed integer or false.
fn try_lower_ord(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 2
        || !super::explicit_utf8_encoding(ctx, inst.operands[1])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_ord", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_ord_ascii_fallback");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {fallback}"));          // an empty subject raises the bridge's ValueError
            ctx.emitter.instruction("ldrb w1, [x1]");                          // the first ASCII byte is the codepoint
            ctx.emitter.instruction("mov x2, xzr");                            // integer payloads have no high word
            ctx.emitter.instruction("mov x0, xzr");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                          // is the subject empty?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // an empty subject raises the bridge's ValueError
            ctx.emitter.instruction("movzx edi, BYTE PTR [rax]");              // the first ASCII byte is the codepoint
            ctx.emitter.instruction("xor esi, esi");                           // integer payloads have no high word
            ctx.emitter.instruction("xor eax, eax");                           // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers ASCII `mb_strpos()`-family searches through the ordinary position helpers.
fn try_lower_position(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    name: &str,
    runtime_label: &str,
    direction: super::super::StringPositionDirection,
) -> Result<bool> {
    if inst.operands.len() != 4
        || !super::explicit_utf8_encoding(ctx, inst.operands[3])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || ctx.value_php_type(inst.operands[1])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_position_haystack_fallback");
    load_string_arg_to_regs(ctx, inst, 1, name, pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    // ASCII byte offsets equal UTF-8 codepoint offsets, so reuse PHP's exact search bounds.
    let mut synthetic = inst.clone();
    synthetic.operands.truncate(3);
    super::super::search::lower_string_position_no_store(
        ctx,
        &synthetic,
        name,
        runtime_label,
        direction,
    )?;
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers a raw integer `(int)mb_strpos()`-family result selected by the cast sink.
pub(super) fn lower_position_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
    name: &str,
    runtime_label: &str,
    direction: super::super::StringPositionDirection,
) -> Result<()> {
    let fast = inst.operands.len() == 4
        && super::explicit_utf8_encoding(ctx, inst.operands[3])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str
        && ctx.value_php_type(inst.operands[1])?.codegen_repr() == PhpType::Str;
    let (fallback, done) = if fast {
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, name, pointer, length)?;
        abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
        let fallback = ascii_predicate_fallback_label(ctx, "mb_position_int_haystack_fallback");
        load_string_arg_to_regs(ctx, inst, 1, name, pointer, length)?;
        abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
        emit_predicate_branch(ctx, &fallback);
        let mut synthetic = inst.clone();
        synthetic.operands.truncate(3);
        super::super::search::lower_string_position_int_no_store(
            ctx,
            &synthetic,
            name,
            runtime_label,
            direction,
        )?;
        let done = ctx.next_label("mb_position_int_done");
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&fallback);
        (true, Some(done))
    } else {
        (false, None)
    };
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    emit_unbox_boxed_int(ctx);
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
    emit_negative_to_zero(ctx);
    store_if_result(ctx, inst)?;
    let _ = fallback;
    Ok(())
}

/// Lowers a raw integer `(int)mb_ord()` result selected by the cast sink.
pub(super) fn lower_ord_int(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let fast = inst.operands.len() == 2
        && super::explicit_utf8_encoding(ctx, inst.operands[1])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbOrd, false)?;
        emit_unbox_boxed_int(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_ord", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_ord_int_ascii_fallback");
    let done = ctx.next_label("mb_ord_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {fallback}"));           // an empty subject raises the bridge's ValueError
            ctx.emitter.instruction("ldrb w0, [x1]");                          // the first ASCII byte is the codepoint
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                          // is the subject empty?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // an empty subject raises the bridge's ValueError
            ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");              // the first ASCII byte is the codepoint
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbOrd, false)?;
    emit_unbox_boxed_int(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers a string mbstring setting through its allocation-free C ABI setter.
fn try_lower_string_setting_bool(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
    helper: &str,
) -> Result<bool> {
    if inst.operands.len() != 1
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_setting", pointer, length)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                             // pass the setting pointer as the first C argument
            ctx.emitter.instruction("mov x1, x2");                             // pass the setting length as the second C argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                          // pass the setting pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                          // pass the setting length as the second SysV argument
        }
    }
    ctx.emitter.bl_c(helper);
    zero_extend_c_i32(ctx);
    let fallback = ctx.next_label("mb_setting_fallback");
    let done = ctx.next_label("mb_setting_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));          // invalid settings keep the bridge diagnostics
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the setting accept the value?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // invalid settings keep the bridge diagnostics
        }
    }
    // The setting functions return bool|string, so the accepted boolean becomes one Mixed cell.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                             // pass the boolean payload
            ctx.emitter.instruction("mov x2, xzr");                            // boolean payloads have no high word
            ctx.emitter.instruction("mov x0, #3");                             // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                          // pass the boolean payload
            ctx.emitter.instruction("xor esi, esi");                          // boolean payloads have no high word
            ctx.emitter.instruction("mov eax, 3");                            // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_substitute_character()` through its allocation-free C ABI setter.
fn try_lower_substitute_character(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 1
        || !matches!(
            ctx.value_php_type(inst.operands[0])?.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float
        )
    {
        return Ok(false);
    }
    load_as_int(ctx, inst.operands[0], "mb_substitute_character")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                           // pass the codepoint as the first SysV argument
        }
    }
    ctx.emitter.bl_c("elephc_mbstring_substitute_character_set_v1");
    zero_extend_c_i32(ctx);
    let fallback = ctx.next_label("mb_substitute_fallback");
    let done = ctx.next_label("mb_substitute_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));          // invalid codepoints keep the bridge diagnostics
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the setter accept the codepoint?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // invalid codepoints keep the bridge diagnostics
        }
    }
    // The setting functions return bool|string, so the accepted boolean becomes one Mixed cell.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                             // pass the boolean payload
            ctx.emitter.instruction("mov x2, xzr");                            // boolean payloads have no high word
            ctx.emitter.instruction("mov x0, #3");                             // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                          // pass the boolean payload
            ctx.emitter.instruction("xor esi, esi");                          // boolean payloads have no high word
            ctx.emitter.instruction("mov eax, 3");                            // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_ereg_search_init()` for the literal ASCII `[0-9]+` pattern.
fn try_lower_ereg_search_init(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 2
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(
            ctx,
            inst.operands[1],
        )?
        .as_deref()
            != Some("[0-9]+")
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_ereg_search_init", pointer, length)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                             // pass the subject pointer as the first C argument
            ctx.emitter.instruction("mov x1, x2");                             // pass the subject length as the second C argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                          // pass the subject pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                          // pass the subject length as the second SysV argument
        }
    }
    ctx.emitter.bl_c("elephc_mbstring_ereg_search_init_simple_v1");
    let fallback = ctx.next_label("mb_ereg_search_init_fallback");
    let done = ctx.next_label("mb_ereg_search_init_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp w0, #0");                              // inspect the signed C i32 status
            ctx.emitter.instruction(&format!("b.lt {fallback}"));              // bridge preserves unexpected diagnostics
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                          // did the dedicated initializer fail?
            ctx.emitter.instruction(&format!("js {fallback}"));                // bridge preserves unexpected diagnostics
        }
    }
    zero_extend_c_i32(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEregSearchInit, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers the no-argument `mb_ereg_search()` progressive step.
fn try_lower_ereg_search_step(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if !inst.operands.is_empty() {
        return Ok(false);
    }
    ctx.emitter.bl_c("elephc_mbstring_ereg_search_step_v1");
    let fallback = ctx.next_label("mb_ereg_search_fallback");
    let done = ctx.next_label("mb_ereg_search_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp w0, #0");                              // inspect the signed C i32 status
            ctx.emitter.instruction(&format!("b.lt {fallback}"));              // bridge preserves unexpected diagnostics
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                          // did the dedicated search fail?
            ctx.emitter.instruction(&format!("js {fallback}"));                // bridge preserves unexpected diagnostics
        }
    }
    zero_extend_c_i32(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEregSearch, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Emits the provider-ready probe that selects the dedicated or generic regex path.
///
/// The dedicated ABI also reports its unavailable sentinel, but probing first keeps the shared
/// bridge responsible for the very first call, where it registers the provider with Magician.
fn emit_regex_provider_probe(ctx: &mut FunctionContext<'_>, fallback: &str) {
    ctx.emitter.bl_c("elephc_mbstring_regex_available_v1");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz w0, {fallback}"));            // run the bridge until the provider is registered
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // is the shared regex provider registered yet?
            ctx.emitter.instruction(&format!("jz {fallback}"));                 // run the bridge until the provider is registered
        }
    }
}

/// Lowers `mb_ereg_search_pos()` into a native two-integer indexed array.
fn try_lower_ereg_search_pos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if !inst.operands.is_empty() {
        return Ok(false);
    }
    let fallback = ctx.next_label("mb_ereg_search_pos_fallback");
    let no_match = ctx.next_label("mb_ereg_search_pos_no_match");
    let done = ctx.next_label("mb_ereg_search_pos_done");
    emit_regex_provider_probe(ctx, &fallback);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve the start/length output slots
            ctx.emitter.instruction("mov x0, sp");                              // pass the start output slot
            ctx.emitter.instruction("add x1, sp, #8");                          // pass the length output slot
            ctx.emitter.bl_c("elephc_mbstring_ereg_search_pos_v1");
            ctx.emitter.instruction("cmp w0, #0");                              // inspect the signed match status
            ctx.emitter.instruction(&format!("b.lt {fallback}_pop"));           // bridge preserves unexpected diagnostics
            ctx.emitter.instruction(&format!("cbz w0, {no_match}_pop"));        // no retained match returns false
            ctx.emitter.instruction("mov x0, #2");                              // the result carries start and length
            ctx.emitter.instruction("mov x1, #8");                              // indexed int slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            ctx.emitter.instruction("ldr x9, [sp]");                            // load the match start
            ctx.emitter.instruction("str x9, [x0, #24]");                       // append the match start
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // load the match length
            ctx.emitter.instruction("str x9, [x0, #32]");                       // append the match length
            ctx.emitter.instruction("mov x9, #2");                              // publish both integer elements
            ctx.emitter.instruction("str x9, [x0]");                            // set the logical array length
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Int)),
            );
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{no_match}_pop"));
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
            ctx.emitter.instruction("mov x0, xzr");                             // false payload
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{fallback}_pop"));
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve the start/length output slots
            ctx.emitter.instruction("mov rdi, rsp");                            // pass the start output slot
            ctx.emitter.instruction("lea rsi, [rsp + 8]");                      // pass the length output slot
            ctx.emitter.bl_c("elephc_mbstring_ereg_search_pos_v1");
            ctx.emitter.instruction("test eax, eax");                           // inspect the signed match status
            ctx.emitter.instruction(&format!("js {fallback}_pop"));             // bridge preserves unexpected diagnostics
            ctx.emitter.instruction(&format!("jz {no_match}_pop"));             // no retained match returns false
            ctx.emitter.instruction("mov rdi, 2");                              // the result carries start and length
            ctx.emitter.instruction("mov rsi, 8");                              // indexed int slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // load the match start
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], r10");           // append the match start
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 8]");            // load the match length
            ctx.emitter.instruction("mov QWORD PTR [rax + 32], r10");           // append the match length
            ctx.emitter.instruction("mov QWORD PTR [rax], 2");                  // publish both integer elements
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Int)),
            );
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{no_match}_pop"));
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
            ctx.emitter.instruction("xor eax, eax");                            // false payload
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{fallback}_pop"));
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
        }
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEregSearchPos, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_ereg_search_regs()`/`mb_ereg_search_getregs()` into a native one-element array.
fn try_lower_ereg_search_regs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    advance: bool,
) -> Result<bool> {
    if !inst.operands.is_empty() {
        return Ok(false);
    }
    let fallback = ctx.next_label("mb_ereg_search_regs_fallback");
    let no_match = ctx.next_label("mb_ereg_search_regs_no_match");
    let done = ctx.next_label("mb_ereg_search_regs_done");
    let helper = if advance {
        "elephc_mbstring_ereg_search_regs_v1"
    } else {
        "elephc_mbstring_ereg_search_getregs_v1"
    };
    emit_regex_provider_probe(ctx, &fallback);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve the borrowed match range output slots
            ctx.emitter.instruction("mov x0, sp");                              // pass the match pointer output slot
            ctx.emitter.instruction("add x1, sp, #8");                          // pass the match length output slot
            ctx.emitter.bl_c(helper);
            ctx.emitter.instruction("cmp w0, #0");                              // inspect the signed match status
            ctx.emitter.instruction(&format!("b.lt {fallback}_pop"));           // bridge preserves unexpected diagnostics
            ctx.emitter.instruction(&format!("cbz w0, {no_match}_pop"));        // no match returns false
            ctx.emitter.instruction("mov x0, #1");                              // the result carries one match
            ctx.emitter.instruction("mov x1, #16");                             // string slots carry pointer and length
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            ctx.emitter.instruction("ldr x1, [sp]");                            // load the borrowed match pointer
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // load the borrowed match length
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Str)),
            );
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{no_match}_pop"));
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
            ctx.emitter.instruction("mov x0, xzr");                             // false payload
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{fallback}_pop"));
            ctx.emitter.instruction("add sp, sp, #32");                         // release the output slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve the borrowed match range output slots
            ctx.emitter.instruction("mov rdi, rsp");                            // pass the match pointer output slot
            ctx.emitter.instruction("lea rsi, [rsp + 8]");                      // pass the match length output slot
            ctx.emitter.bl_c(helper);
            ctx.emitter.instruction("test eax, eax");                           // inspect the signed match status
            ctx.emitter.instruction(&format!("js {fallback}_pop"));             // bridge preserves unexpected diagnostics
            ctx.emitter.instruction(&format!("jz {no_match}_pop"));             // no match returns false
            ctx.emitter.instruction("mov rdi, 1");                              // the result carries one match
            ctx.emitter.instruction("mov rsi, 16");                             // string slots carry pointer and length
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            ctx.emitter.instruction("mov rdi, rax");                            // append into the new array
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                // load the borrowed match pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // load the borrowed match length
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Str)),
            );
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{no_match}_pop"));
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
            ctx.emitter.instruction("xor eax, eax");                            // false payload
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&format!("{fallback}_pop"));
            ctx.emitter.instruction("add rsp, 32");                             // release the output slots
        }
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, operation, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers the no-argument `mb_ereg_search_getpos()` read.
fn lower_ereg_search_getpos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if !inst.operands.is_empty() {
        return Ok(false);
    }
    ctx.emitter.bl_c("elephc_mbstring_ereg_search_getpos_v1");
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `mb_ereg_search_setpos()` through the shared session setter.
fn try_lower_ereg_search_setpos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 1 {
        return Ok(false);
    }
    load_as_int(ctx, inst.operands[0], "mb_ereg_search_setpos")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                              // pass the position as the first SysV argument
    }
    ctx.emitter.bl_c("elephc_mbstring_ereg_search_setpos_v1");
    zero_extend_c_i32(ctx);
    let fallback = ctx.next_label("mb_ereg_search_setpos_fallback");
    let done = ctx.next_label("mb_ereg_search_setpos_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));           // out-of-range positions need PHP's ValueError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the position fit the retained subject?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // out-of-range positions need PHP's ValueError
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEregSearchSetpos, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers a raw `(int)mb_* setting` result selected by the integer cast sink.
pub(super) fn lower_setting_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
    helper: &str,
    string_arg: bool,
) -> Result<()> {
    let fallback = ctx.next_label("mb_setting_int_fallback");
    let done = ctx.next_label("mb_setting_int_done");
    if inst.operands.len() == 1 {
        if string_arg {
            let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
            load_string_arg_to_regs(ctx, inst, 0, "mb_setting", pointer, length)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x0, x1");                     // pass the setting pointer as the first C argument
                    ctx.emitter.instruction("mov x1, x2");                     // pass the setting length as the second C argument
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov rdi, rax");                  // pass the setting pointer as the first SysV argument
                    ctx.emitter.instruction("mov rsi, rdx");                  // pass the setting length as the second SysV argument
                }
            }
        } else {
            load_as_int(ctx, inst.operands[0], "mb_setting")?;
            if ctx.emitter.target.arch == Arch::X86_64 {
                ctx.emitter.instruction("mov rdi, rax");                       // pass the codepoint as the first SysV argument
            }
        }
        ctx.emitter.bl_c(helper);
        zero_extend_c_i32(ctx);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbz x0, {fallback}"));      // invalid settings keep the bridge diagnostics
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                      // did the setting accept the value?
                ctx.emitter.instruction(&format!("jz {fallback}"));            // invalid settings keep the bridge diagnostics
            }
        }
        abi::emit_jump(ctx.emitter, &done);
    } else {
        abi::emit_jump(ctx.emitter, &fallback);
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    emit_unbox_boxed_int(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `mb_output_handler()` when the request converts UTF-8 to UTF-8 unchanged.
fn try_lower_output_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 2
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    ctx.emitter.bl_c("elephc_mbstring_output_utf8_passthrough_v1");
    zero_extend_c_i32(ctx);
    let fallback = ctx.next_label("mb_output_handler_fallback");
    let done = ctx.next_label("mb_output_handler_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));           // other output encodings keep the bridge conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // does the request convert UTF-8 to UTF-8?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // other output encodings keep the bridge conversion
        }
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_output_handler", pointer, length)?;
    // UTF-8 to UTF-8 output handling is value-preserving, so return the borrowed subject
    // with one retained reference instead of copying its bytes.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the returned string pair across the retain
            ctx.emitter.instruction("mov x0, x1");                             // pass the string payload to the retain helper
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                  // restore the retained string pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");               // preserve the returned string pair across the retain
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");                // restore the retained string pair
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbOutputHandler, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers explicit-UTF-8 `mb_convert_kana($s, "K")` over half-width katakana.
fn try_lower_convert_kana(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 3
        || !super::explicit_utf8_encoding(ctx, inst.operands[2])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let Some(mode) =
        crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, inst.operands[1])?
    else {
        return Ok(false);
    };
    if mode != "K" {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_convert_kana", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_convert_kana_k_utf8");
    let fallback = ctx.next_label("mb_convert_kana_fallback");
    let done = ctx.next_label("mb_convert_kana_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {fallback}"));           // a null result means the converter declined the value
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the converter accept the value?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // a null result means the converter declined the value
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbConvertKana, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers one non-ASCII UTF-8 `mb_encode_mimeheader()` value through the MIME base64 wrapper.
fn try_lower_mimeheader(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 2
        || !super::explicit_utf8_encoding(ctx, inst.operands[1])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_encode_mimeheader", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_mime_b64_utf8");
    let fallback = ctx.next_label("mb_encode_mimeheader_fallback");
    let done = ctx.next_label("mb_encode_mimeheader_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {fallback}"));           // a null result means the wrapper declined the value
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the wrapper accept the value?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // a null result means the wrapper declined the value
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEncodeMimeheader, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Zero-extends a C `i32` result on AArch64 before generated code treats it as an `x` register.
fn zero_extend_c_i32(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::AArch64 {
        ctx.emitter.instruction("mov w0, w0");                                 // clear the unspecified high half of a C i32 result
    }
}

/// Lowers the specialized one-element UTF-8 `mb_detect_order()` setter.
pub(super) fn lower_detect_order_single(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(
            "mb_detect_order_single expected one operand",
        ));
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_detect_order", pointer, length)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                             // pass the encoding pointer as the first C argument
            ctx.emitter.instruction("mov x1, x2");                             // pass the encoding length as the second C argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                          // pass the encoding pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                          // pass the encoding length as the second SysV argument
        }
    }
    ctx.emitter.bl_c("elephc_mbstring_detect_order_single_v1");
    zero_extend_c_i32(ctx);
    if inst.result_php_type.codegen_repr() == PhpType::Int {
        // The integer-observed form returns PHP's boolean truthiness directly.
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("movzx rax, al");                          // clear the stale high bits of the boolean result
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, x0");                         // pass the boolean payload
                ctx.emitter.instruction("mov x2, xzr");                        // boolean payloads have no high word
                ctx.emitter.instruction("mov x0, #3");                         // runtime tag 3 = boolean
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                      // pass the boolean payload
                ctx.emitter.instruction("xor esi, esi");                      // boolean payloads have no high word
                ctx.emitter.instruction("mov eax, 3");                        // runtime tag 3 = boolean
                abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `mb_preferred_mime_name("UTF-8")` to the same persistent encoding literal.
fn try_lower_preferred_mime_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<bool> {
    if inst.operands.len() != 1 {
        return Ok(false);
    }
    let Some(name) =
        crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, inst.operands[0])?
    else {
        return Ok(false);
    };
    if !name.eq_ignore_ascii_case("UTF-8") {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_preferred_mime_name", pointer, length)?;
    // The preferred MIME name for UTF-8 is its canonical spelling, a persistent literal.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_owned_string");
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers explicit `mb_convert_encoding($s, "ISO-8859-1", "UTF-8")` through a byte converter.
fn try_lower_convert_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 3
        || !explicit_latin1_encoding(ctx, inst.operands[1])?
        || !super::explicit_utf8_encoding(ctx, inst.operands[2])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_convert_encoding", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_utf8_to_latin1");
    let fallback = ctx.next_label("mb_convert_encoding_latin1_fallback");
    let done = ctx.next_label("mb_convert_encoding_latin1_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {fallback}"));           // a null result means the bridge must handle it
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the byte converter accept the subject?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // a null result means the bridge must handle it
        }
    }
    // `mb_convert_encoding()` returns string|false, so the owned pair becomes one Mixed cell.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_owned_string");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, operation, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Returns whether the operand is a literal ISO-8859-1/Latin-1 encoding name.
fn explicit_latin1_encoding(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<bool> {
    let Some(encoding) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, value)? else {
        return Ok(false);
    };
    Ok(matches!(
        encoding.to_ascii_uppercase().as_str(),
        "ISO-8859-1" | "ISO8859-1" | "LATIN1" | "LATIN-1"
    ))
}

/// Lowers `mb_scrub()` over an already-valid explicit UTF-8 subject to an owned copy.
fn try_lower_scrub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    if inst.operands.len() != 2
        || !super::explicit_utf8_encoding(ctx, inst.operands[1])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_scrub", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_strlen_utf8");
    let fallback = ctx.next_label("mb_scrub_invalid_fallback");
    let done = ctx.next_label("mb_scrub_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #-1");                            // did the validator reject malformed UTF-8?
            ctx.emitter.instruction(&format!("b.eq {fallback}"));             // malformed input needs the replacement bridge
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                           // did the validator reject malformed UTF-8?
            ctx.emitter.instruction(&format!("je {fallback}"));               // malformed input needs the replacement bridge
        }
    }
    // Valid UTF-8 scrubbing is a byte-for-byte copy, so reuse the owned-slice helper.
    load_string_arg_to_regs(ctx, inst, 0, "mb_scrub", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_str_dup_owned");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, operation, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers `strlen(mb_detect_encoding(...))` for a single explicit UTF-8 encoding list.
pub(super) fn lower_detect_encoding_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let fast = inst.operands.len() == 2
        && super::explicit_utf8_encoding(ctx, inst.operands[1])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbDetectEncoding, false)?;
        emit_boxed_string_length(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_detect_encoding", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_strlen_utf8");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #-1");                            // did the validator reject malformed UTF-8?
            ctx.emitter.instruction("mov x9, #5");                             // detected encoding name "UTF-8" has five bytes
            ctx.emitter.instruction("csel x0, xzr, x9, eq");                   // invalid input detects false, whose length is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                           // did the validator reject malformed UTF-8?
            ctx.emitter.instruction("mov r10, 5");                            // detected encoding name "UTF-8" has five bytes
            ctx.emitter.instruction("mov r11, 0");                            // invalid input detects false, whose length is zero
            ctx.emitter.instruction("cmove r10, r11");                        // select zero for malformed input
            ctx.emitter.instruction("mov rax, r10");                          // carry the selected name length
        }
    }
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_chr(...))` to the UTF-8 byte length of one explicit-UTF-8 codepoint.
pub(super) fn lower_chr_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let fast = inst.operands.len() == 2
        && super::explicit_utf8_encoding(ctx, inst.operands[1])?;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbChr, false)?;
        emit_boxed_string_length(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    load_as_int(ctx, inst.operands[0], "mb_chr codepoint")?;
    let zero = ctx.next_label("mb_chr_len_zero");
    let one = ctx.next_label("mb_chr_len_one");
    let two = ctx.next_label("mb_chr_len_two");
    let three = ctx.next_label("mb_chr_len_three");
    let four = ctx.next_label("mb_chr_len_four");
    let done = ctx.next_label("mb_chr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                             // is the codepoint negative?
            ctx.emitter.instruction(&format!("b.lt {zero}"));                  // invalid codepoints encode as false
            ctx.emitter.instruction("cmp x0, #0x80");                          // does the codepoint fit one UTF-8 byte?
            ctx.emitter.instruction(&format!("b.lo {one}"));                   // a one-byte sequence
            ctx.emitter.instruction("cmp x0, #0x800");                         // does the codepoint fit two UTF-8 bytes?
            ctx.emitter.instruction(&format!("b.lo {two}"));                   // a two-byte sequence
            ctx.emitter.instruction("mov w9, #0xD800");                        // materialize the first surrogate codepoint
            ctx.emitter.instruction("cmp x0, x9");                             // is the codepoint below the surrogate range?
            ctx.emitter.instruction(&format!("b.lo {three}"));                 // a three-byte sequence
            ctx.emitter.instruction("mov w9, #0xE000");                        // materialize the first post-surrogate codepoint
            ctx.emitter.instruction("cmp x0, x9");                             // is the codepoint inside the surrogate range?
            ctx.emitter.instruction(&format!("b.lo {zero}"));                  // surrogates encode as false
            ctx.emitter.instruction("mov w9, #0x10000");                       // materialize the first four-byte codepoint
            ctx.emitter.instruction("cmp x0, x9");                             // does the codepoint fit three UTF-8 bytes?
            ctx.emitter.instruction(&format!("b.lo {three}"));                 // a three-byte sequence
            ctx.emitter.instruction("mov w9, #0x110000");                      // materialize one past the last Unicode codepoint
            ctx.emitter.instruction("cmp x0, x9");                             // is the codepoint beyond Unicode?
            ctx.emitter.instruction(&format!("b.lo {four}"));                  // a four-byte sequence
            abi::emit_jump(ctx.emitter, &zero);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                         // is the codepoint negative?
            ctx.emitter.instruction(&format!("js {zero}"));                    // invalid codepoints encode as false
            ctx.emitter.instruction("cmp rax, 0x80");                         // does the codepoint fit one UTF-8 byte?
            ctx.emitter.instruction(&format!("jb {one}"));                     // a one-byte sequence
            ctx.emitter.instruction("cmp rax, 0x800");                        // does the codepoint fit two UTF-8 bytes?
            ctx.emitter.instruction(&format!("jb {two}"));                     // a two-byte sequence
            ctx.emitter.instruction("cmp rax, 0xD800");                       // is the codepoint below the surrogate range?
            ctx.emitter.instruction(&format!("jb {three}"));                   // a three-byte sequence
            ctx.emitter.instruction("cmp rax, 0xE000");                       // is the codepoint inside the surrogate range?
            ctx.emitter.instruction(&format!("jb {zero}"));                    // surrogates encode as false
            ctx.emitter.instruction("cmp rax, 0x10000");                      // does the codepoint fit three UTF-8 bytes?
            ctx.emitter.instruction(&format!("jb {three}"));                   // a three-byte sequence
            ctx.emitter.instruction("cmp rax, 0x110000");                     // is the codepoint beyond Unicode?
            ctx.emitter.instruction(&format!("jb {four}"));                    // a four-byte sequence
            abi::emit_jump(ctx.emitter, &zero);
        }
    }
    for (label, length) in [(&one, 1), (&two, 2), (&three, 3), (&four, 4), (&zero, 0)] {
        ctx.emitter.label(label);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), length);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_strimwidth(...))` for a zero start and explicit UTF-8 ASCII strings.
pub(super) fn lower_strimwidth_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let fast = inst.operands.len() == 5
        && super::explicit_utf8_encoding(ctx, inst.operands[4])?
        && const_int_operand(ctx, inst.operands[1])? == Some(0)
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str
        && ctx.value_php_type(inst.operands[3])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbStrimwidth, false)?;
        move_string_length_to_result(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_strimwidth", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_strimwidth_subject_fallback");
    load_string_arg_to_regs(ctx, inst, 3, "mb_strimwidth", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    let done = ctx.next_label("mb_strimwidth_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "mb_strimwidth", "x1", "x2")?;
            ctx.emitter.instruction("str x2, [sp, #-16]!");                    // preserve the subject byte length
            load_string_arg_to_regs(ctx, inst, 3, "mb_strimwidth", "x1", "x2")?;
            ctx.emitter.instruction("str x2, [sp, #-16]!");                    // preserve the trim marker byte length
            load_as_int(ctx, inst.operands[2], "mb_strimwidth width")?;
            ctx.emitter.instruction("cmp x0, #0");                             // is the requested width negative?
            let cleanup = ctx.next_label("mb_strimwidth_negative_cleanup");
            ctx.emitter.instruction(&format!("b.lt {cleanup}"));               // negative widths keep the deprecated bridge path
            ctx.emitter.instruction("mov x10, x0");                            // preserve the requested width
            ctx.emitter.instruction("ldr x11, [sp], #16");                     // reload the trim marker byte length
            ctx.emitter.instruction("ldr x2, [sp], #16");                      // reload the subject byte length
            ctx.emitter.instruction("cmp x2, x10");                            // does the subject exceed the requested width?
            let full = ctx.next_label("mb_strimwidth_full_length");
            ctx.emitter.instruction(&format!("b.le {full}"));                  // no marker is appended when the subject fits
            ctx.emitter.instruction("cmp x10, x11");                           // is the width no longer than the marker itself?
            ctx.emitter.instruction("csel x0, x11, x10, le");                  // a short width yields the whole marker
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&full);
            ctx.emitter.instruction("mov x0, x2");                             // the whole subject is returned unchanged
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&cleanup);
            ctx.emitter.instruction("add sp, sp, #32");                        // drop both saved lengths before the bridge
            abi::emit_jump(ctx.emitter, &fallback);
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "mb_strimwidth", "rax", "rdx")?;
            ctx.emitter.instruction("push rdx");                               // preserve the subject byte length
            load_string_arg_to_regs(ctx, inst, 3, "mb_strimwidth", "rax", "rdx")?;
            ctx.emitter.instruction("push rdx");                               // preserve the trim marker byte length
            load_as_int(ctx, inst.operands[2], "mb_strimwidth width")?;
            ctx.emitter.instruction("test rax, rax");                          // is the requested width negative?
            let cleanup = ctx.next_label("mb_strimwidth_negative_cleanup_x86");
            ctx.emitter.instruction(&format!("js {cleanup}"));                 // negative widths keep the deprecated bridge path
            ctx.emitter.instruction("mov rcx, rax");                           // preserve the requested width
            ctx.emitter.instruction("pop r10");                                // reload the trim marker byte length
            ctx.emitter.instruction("pop rsi");                                // reload the subject byte length
            ctx.emitter.instruction("cmp rsi, rcx");                           // does the subject exceed the requested width?
            let full = ctx.next_label("mb_strimwidth_full_length_x86");
            ctx.emitter.instruction(&format!("jle {full}"));                   // no marker is appended when the subject fits
            ctx.emitter.instruction("mov rax, rcx");                           // a truncated result keeps the requested width
            ctx.emitter.instruction("cmp rcx, r10");                           // is the width no longer than the marker itself?
            ctx.emitter.instruction("cmovle rax, r10");                        // a short width yields the whole marker
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&full);
            ctx.emitter.instruction("mov rax, rsi");                           // the whole subject is returned unchanged
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&cleanup);
            ctx.emitter.instruction("add rsp, 16");                            // drop both saved lengths before the bridge
            abi::emit_jump(ctx.emitter, &fallback);
        }
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbStrimwidth, false)?;
    emit_string_length_and_release(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Releases a native string pair after moving its length into the integer result register.
fn emit_string_length_and_release(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                        // reserve the string pair across the release
            ctx.emitter.instruction("stp x1, x2, [sp, #0]");                   // save the produced string pointer and length
            ctx.emitter.instruction("mov x0, x1");                            // pass the produced string to the uniform release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                       // return the measured string length
            ctx.emitter.instruction("add sp, sp, #32");                        // release the measurement frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                            // reserve the string length across the release
            ctx.emitter.instruction("mov QWORD PTR [rsp], rdx");               // save the produced string length
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");               // return the measured string length
            ctx.emitter.instruction("add rsp, 32");                            // release the measurement frame
        }
    }
}

/// Moves a native string pair's length into the integer result register.
fn move_string_length_to_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x2");                             // return the produced string's byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdx");                          // return the produced string's byte length
        }
    }
}

/// Lowers `strlen(mb_convert_encoding(...))` for an explicit UTF-8 to Latin-1 conversion.
pub(super) fn lower_convert_encoding_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let fast = inst.operands.len() == 3
        && explicit_latin1_encoding(ctx, inst.operands[1])?
        && super::explicit_utf8_encoding(ctx, inst.operands[2])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbConvertEncoding, false)?;
        emit_boxed_string_length(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_convert_encoding", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_strlen_utf8");
    let fallback = ctx.next_label("mb_convert_encoding_len_fallback");
    let done = ctx.next_label("mb_convert_encoding_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #-1");                            // did the validator reject malformed UTF-8?
            ctx.emitter.instruction(&format!("b.eq {fallback}"));             // malformed input keeps the bridge's substitution length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                           // did the validator reject malformed UTF-8?
            ctx.emitter.instruction(&format!("je {fallback}"));               // malformed input keeps the bridge's substitution length
        }
    }
    // One Latin-1 byte per UTF-8 codepoint, so the codepoint count is the result length.
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbConvertEncoding, false)?;
    emit_boxed_string_length(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_encode_numericentity(...))` for the full decimal Unicode map.
pub(super) fn lower_encode_numericentity_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_numericentity_len(
        ctx,
        inst,
        RuntimeBuiltinId::MbEncodeNumericentity,
        "__rt_mb_encode_numericentity_len",
    )
}

/// Lowers `strlen(mb_decode_numericentity(...))` for the full decimal Unicode map.
pub(super) fn lower_decode_numericentity_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_numericentity_len(
        ctx,
        inst,
        RuntimeBuiltinId::MbDecodeNumericentity,
        "__rt_mb_decode_numericentity_len",
    )
}

/// Shared numeric-entity length lowering with map verification and a bridge fallback.
fn lower_numericentity_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
    helper: &str,
) -> Result<()> {
    let fallback = ctx.next_label("mb_numericentity_len_fallback");
    let done = ctx.next_label("mb_numericentity_len_done");
    let fast = inst.operands.len() == 3
        && super::explicit_utf8_encoding(ctx, inst.operands[2])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str
        && verify_full_decimal_map(ctx, inst.operands[1], &fallback)?;
    if fast {
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, "numericentity", pointer, length)?;
        abi::emit_call_label(ctx.emitter, helper);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #-1");                        // did the helper decline the input?
                ctx.emitter.instruction(&format!("b.eq {fallback}"));         // invalid input keeps the bridge's exact semantics
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, -1");                       // did the helper decline the input?
                ctx.emitter.instruction(&format!("je {fallback}"));           // invalid input keeps the bridge's exact semantics
            }
        }
        abi::emit_jump(ctx.emitter, &done);
    } else {
        abi::emit_jump(ctx.emitter, &fallback);
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    emit_string_length_and_release(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Verifies the map is the packed `[0x80, 0x10FFFF, 0, 0x1FFFFF]` integer literal.
fn verify_full_decimal_map(
    ctx: &mut FunctionContext<'_>,
    map: crate::ir::ValueId,
    fallback: &str,
) -> Result<bool> {
    let ty = ctx.value_php_type(map)?;
    if !matches!(&ty, PhpType::Array(inner) if inner.codegen_repr() == PhpType::Int) {
        return Ok(false);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(map, "x9")?;
            ctx.emitter.instruction("ldr x10, [x9, #-8]");                     // load the packed array metadata
            ctx.emitter.instruction("and w11, w10, #0xff");                   // isolate the top-level heap kind
            ctx.emitter.instruction("cmp w11, #2");                           // is this a packed indexed array?
            ctx.emitter.instruction(&format!("b.ne {fallback}"));             // other array shapes use the bridge
            ctx.emitter.instruction("ubfx x10, x10, #8, #7");                  // isolate the packed element type
            ctx.emitter.instruction(&format!("cbnz x10, {fallback}"));        // non-int arrays use the bridge
            ctx.emitter.instruction("ldr x10, [x9]");                          // load the map entry count
            ctx.emitter.instruction("cmp x10, #4");                           // does the map contain four bounds?
            ctx.emitter.instruction(&format!("b.ne {fallback}"));             // other maps use the bridge
            ctx.emitter.instruction("ldr x10, [x9, #24]");                     // load the first map bound
            ctx.emitter.instruction("cmp x10, #0x80");                        // is the start the full non-ASCII range?
            ctx.emitter.instruction(&format!("b.ne {fallback}"));             // other maps use the bridge
            ctx.emitter.instruction("ldr x10, [x9, #32]");                     // load the second map bound
            ctx.emitter.instruction("movz w11, #0xFFFF");                     // low half of the last Unicode codepoint
            ctx.emitter.instruction("movk w11, #0x10, lsl #16");               // high half of the last Unicode codepoint
            ctx.emitter.instruction("cmp x10, x11");                          // is the end the full Unicode range?
            ctx.emitter.instruction(&format!("b.ne {fallback}"));             // other maps use the bridge
            ctx.emitter.instruction("ldr x10, [x9, #40]");                     // load the map offset
            ctx.emitter.instruction(&format!("cbnz x10, {fallback}"));        // nonzero offsets use the bridge
            ctx.emitter.instruction("ldr x10, [x9, #48]");                     // load the map format mask
            ctx.emitter.instruction("movz w11, #0xFFFF");                     // low half of the decimal full-range mask
            ctx.emitter.instruction("movk w11, #0x1F, lsl #16");               // high half of the decimal full-range mask
            ctx.emitter.instruction("cmp x10, x11");                          // is this the decimal full-range map?
            ctx.emitter.instruction(&format!("b.ne {fallback}"));             // other maps use the bridge
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(map, "r10")?;
            ctx.emitter.instruction("mov r11, QWORD PTR [r10 - 8]");            // load the packed array metadata
            ctx.emitter.instruction("movzx eax, r11b");                       // isolate the top-level heap kind
            ctx.emitter.instruction("cmp eax, 2");                            // is this a packed indexed array?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // other array shapes use the bridge
            ctx.emitter.instruction("shr r11, 8");                            // move the packed element type down
            ctx.emitter.instruction("and r11d, 0x7f");                         // isolate the element type
            ctx.emitter.instruction(&format!("jnz {fallback}"));              // non-int arrays use the bridge
            ctx.emitter.instruction("cmp QWORD PTR [r10], 4");                // does the map contain four bounds?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // other maps use the bridge
            ctx.emitter.instruction("cmp QWORD PTR [r10 + 24], 0x80");         // is the start the full non-ASCII range?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // other maps use the bridge
            ctx.emitter.instruction("cmp QWORD PTR [r10 + 32], 0x10FFFF");     // is the end the full Unicode range?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // other maps use the bridge
            ctx.emitter.instruction("cmp QWORD PTR [r10 + 40], 0");            // is the map offset zero?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // nonzero offsets use the bridge
            ctx.emitter.instruction("cmp QWORD PTR [r10 + 48], 0x1FFFFF");     // is this the decimal full-range map?
            ctx.emitter.instruction(&format!("jne {fallback}"));              // other maps use the bridge
        }
    }
    Ok(true)
}

/// Lowers `count(mb_split(...))` for a single literal ASCII separator.
pub(super) fn lower_split_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let fast = inst.operands.len() == 2
        && ctx.value_php_type(inst.operands[1])?.codegen_repr() == PhpType::Str
        && literal_split_separator(ctx, inst.operands[0])?;
    let fallback = ctx.next_label("mb_split_count_fallback");
    let done = ctx.next_label("mb_split_count_done");
    if fast {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                load_string_arg_to_regs(ctx, inst, 1, "mb_split", "x1", "x2")?;
                ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve the subject while loading the separator
                load_string_arg_to_regs(ctx, inst, 0, "mb_split", "x1", "x2")?;
                ctx.emitter.instruction("mov x3, x1");                         // pass the separator pointer as the needle
                ctx.emitter.instruction("mov x4, x2");                         // pass the separator length as the needle
                ctx.emitter.instruction("ldp x1, x2, [sp], #16");               // restore the subject into the haystack arguments
            }
            Arch::X86_64 => {
                load_string_arg_to_regs(ctx, inst, 1, "mb_split", "rax", "rdx")?;
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");            // preserve the subject while loading the separator
                load_string_arg_to_regs(ctx, inst, 0, "mb_split", "rax", "rdx")?;
                ctx.emitter.instruction("mov r8, rax");                        // preserve the separator pointer
                ctx.emitter.instruction("mov r9, rdx");                        // preserve the separator length
                abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");             // restore the subject into the haystack arguments
                ctx.emitter.instruction("mov rdx, r8");                        // pass the separator pointer as the needle
                ctx.emitter.instruction("mov rcx, r9");                        // pass the separator length as the needle
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_substr_count");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("add x0, x0, #1");                     // one field per separator occurrence plus the final field
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("inc rax");                           // one field per separator occurrence plus the final field
            }
        }
        abi::emit_jump(ctx.emitter, &done);
    } else {
        abi::emit_jump(ctx.emitter, &fallback);
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbSplit, false)?;
    emit_boxed_array_length_and_release(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Unboxes a `array|false` cell and returns the array length, falling back to PHP count().
fn emit_boxed_array_length_and_release(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                        // reserve the cell and its count
            ctx.emitter.instruction("str x0, [sp, #0]");                       // save the boxed array-or-false result
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
            ctx.emitter.instruction("str x0, [sp, #8]");                       // save the count across the cell release
            ctx.emitter.instruction("ldr x0, [sp, #0]");                       // reload the boxed cell
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                       // return the count
            ctx.emitter.instruction("add sp, sp, #32");                        // release the count frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                            // reserve the cell and its count
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");               // save the boxed array-or-false result
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");           // save the count across the cell release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");               // reload the boxed cell
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");           // return the count
            ctx.emitter.instruction("add rsp, 32");                            // release the count frame
        }
    }
}

/// Lowers `strlen(mb_ereg_replace(...))` for the `[0-9]+` pattern and `#` replacement.
pub(super) fn lower_ereg_replace_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let fast = inst.operands.len() == 3
        && ctx.value_php_type(inst.operands[2])?.codegen_repr() == PhpType::Str
        && literal_digit_run_pattern(ctx, inst.operands[0])?
        && literal_hash_replacement(ctx, inst.operands[1])?;
    let fallback = ctx.next_label("mb_ereg_replace_len_fallback");
    let done = ctx.next_label("mb_ereg_replace_len_done");
    if fast {
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 2, "mb_ereg_replace", pointer, length)?;
        abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
        let ascii = ctx.next_label("mb_ereg_replace_len_ascii");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz x0, {ascii}"));          // ASCII subjects use the digit-run census
                ctx.emitter.instruction(&format!("b {fallback}"));             // other subjects keep the regex bridge
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rcx, rcx");                      // did every byte stay inside ASCII?
                ctx.emitter.instruction(&format!("jnz {ascii}"));              // ASCII subjects use the digit-run census
                ctx.emitter.instruction(&format!("jmp {fallback}"));           // other subjects keep the regex bridge
            }
        }
        ctx.emitter.label(&ascii);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, xzr");                        // replaced length starts at zero
                ctx.emitter.instruction("mov x9, x1");                        // subject cursor
                ctx.emitter.instruction("mov x10, x2");                      // remaining subject bytes
                ctx.emitter.instruction("mov w12, #0");                       // no digit run is open
                let scan = ctx.next_label("mb_ereg_replace_len_scan");
                let scan_done = ctx.next_label("mb_ereg_replace_len_scan_done");
                let digit = ctx.next_label("mb_ereg_replace_len_digit");
                let nondigit = ctx.next_label("mb_ereg_replace_len_nondigit");
                let skip = ctx.next_label("mb_ereg_replace_len_skip");
                ctx.emitter.label(&scan);
                ctx.emitter.instruction(&format!("cbz x10, {scan_done}"));    // stop after the final subject byte
                ctx.emitter.instruction("ldrb w11, [x9], #1");                 // load and consume one subject byte
                ctx.emitter.instruction("sub x10, x10, #1");                  // account for the consumed byte
                ctx.emitter.instruction("cmp w11, #'0'");                     // is this below a decimal digit?
                ctx.emitter.instruction(&format!("b.lo {nondigit}"));         // copy non-digits one for one
                ctx.emitter.instruction("cmp w11, #'9'");                     // is this above a decimal digit?
                ctx.emitter.instruction(&format!("b.hi {nondigit}"));         // copy non-digits one for one
                ctx.emitter.label(&digit);
                ctx.emitter.instruction(&format!("cbnz w12, {skip}"));        // a continuing digit run is already replaced
                ctx.emitter.instruction("add x0, x0, #1");                     // one replacement byte per digit run
                ctx.emitter.instruction("mov w12, #1");                       // mark the run as open
                ctx.emitter.instruction(&format!("b {skip}"));                // continue with the next byte
                ctx.emitter.label(&nondigit);
                ctx.emitter.instruction("add x0, x0, #1");                     // copy one non-digit byte
                ctx.emitter.instruction("mov w12, #0");                       // close the digit run
                ctx.emitter.label(&skip);
                ctx.emitter.instruction(&format!("b {scan}"));                // inspect the next subject byte
                ctx.emitter.label(&scan_done);
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r8, rax");                         // subject cursor before the count reuse
                ctx.emitter.instruction("xor eax, eax");                       // replaced length starts at zero
                ctx.emitter.instruction("mov r9, rdx");                        // remaining subject bytes
                ctx.emitter.instruction("xor r10d, r10d");                     // no digit run is open
                let scan = ctx.next_label("mb_ereg_replace_len_scan_x86");
                let digit = ctx.next_label("mb_ereg_replace_len_digit_x86");
                let nondigit = ctx.next_label("mb_ereg_replace_len_nondigit_x86");
                let skip = ctx.next_label("mb_ereg_replace_len_skip_x86");
                ctx.emitter.label(&scan);
                ctx.emitter.instruction("test r9, r9");                        // is the subject exhausted?
                let scan_done = ctx.next_label("mb_ereg_replace_len_scan_done_x86");
                ctx.emitter.instruction(&format!("jz {scan_done}"));           // return the replaced length
                ctx.emitter.instruction("movzx r11d, BYTE PTR [r8]");          // load the next subject byte
                ctx.emitter.instruction("inc r8");                            // consume the subject byte
                ctx.emitter.instruction("dec r9");                            // account for the consumed byte
                ctx.emitter.instruction("cmp r11d, '0'");                      // is this below a decimal digit?
                ctx.emitter.instruction(&format!("jb {nondigit}"));            // copy non-digits one for one
                ctx.emitter.instruction("cmp r11d, '9'");                      // is this above a decimal digit?
                ctx.emitter.instruction(&format!("ja {nondigit}"));            // copy non-digits one for one
                ctx.emitter.label(&digit);
                ctx.emitter.instruction(&format!("test r10d, r10d"));          // is a digit run already open?
                ctx.emitter.instruction(&format!("jnz {skip}"));               // a continuing digit run is already replaced
                ctx.emitter.instruction("inc rax");                            // one replacement byte per digit run
                ctx.emitter.instruction("mov r10d, 1");                        // mark the run as open
                ctx.emitter.instruction(&format!("jmp {skip}"));               // continue with the next byte
                ctx.emitter.label(&nondigit);
                ctx.emitter.instruction("inc rax");                            // copy one non-digit byte
                ctx.emitter.instruction("xor r10d, r10d");                     // close the digit run
                ctx.emitter.label(&skip);
                ctx.emitter.instruction(&format!("jmp {scan}"));               // inspect the next subject byte
                ctx.emitter.label(&scan_done);
            }
        }
        abi::emit_jump(ctx.emitter, &done);
    } else {
        abi::emit_jump(ctx.emitter, &fallback);
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEregReplace, false)?;
    emit_boxed_string_length(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Returns whether the operand is a single safe literal separator byte.
fn literal_split_separator(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<bool> {
    let Some(pattern) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, value)? else {
        return Ok(false);
    };
    let bytes = pattern.as_bytes();
    Ok(bytes.len() == 1
        && matches!(bytes[0], b',' | b';' | b':' | b'_' | b'=' | b'@' | b'!' | b'%' | b'&' | b'/'))
}

/// Returns whether the operand is the literal `[0-9]+` regex pattern.
fn literal_digit_run_pattern(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<bool> {
    Ok(crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, value)?
        .as_deref()
        == Some("[0-9]+"))
}

/// Returns whether the operand is a plain `#` replacement with no backreferences.
fn literal_hash_replacement(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<bool> {
    Ok(crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(ctx, value)?
        .as_deref()
        == Some("#"))
}

/// Lowers `strlen(mb_preferred_mime_name(...))` for the canonical UTF-8 spelling.
pub(super) fn lower_preferred_mime_name_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() == 1 {
        if let Some(name) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(
            ctx,
            inst.operands[0],
        )? {
            if name.eq_ignore_ascii_case("UTF-8") {
                abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 5);
                store_if_result(ctx, inst)?;
                return Ok(());
            }
        }
    }
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbPreferredMimeName, false)?;
    emit_boxed_string_length(ctx);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `count(mb_encoding_aliases(...))` through the allocation-free catalog query.
pub(super) fn lower_encoding_aliases_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() == 1 {
        if let Some(name) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(
            ctx,
            inst.operands[0],
        )? {
            if name.eq_ignore_ascii_case("UTF-8") {
                // The shared catalog has exactly one public alias for UTF-8; the bridge unit
                // tests assert that catalog entry, so a drift fails there before this constant.
                abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
                store_if_result(ctx, inst)?;
                return Ok(());
            }
        }
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, "mb_encoding_aliases", pointer, length)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, x1");                         // pass the encoding pointer as the first C argument
                ctx.emitter.instruction("mov x1, x2");                         // pass the encoding length as the second C argument
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                      // pass the encoding pointer as the first SysV argument
                ctx.emitter.instruction("mov rsi, rdx");                      // pass the encoding length as the second SysV argument
            }
        }
        ctx.emitter.bl_c("elephc_mbstring_encoding_aliases_count_v1");
        let fallback = ctx.next_label("mb_encoding_aliases_count_fallback");
        let done = ctx.next_label("mb_encoding_aliases_count_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");                         // did the catalog recognize the encoding?
                ctx.emitter.instruction(&format!("b.lt {fallback}"));          // unknown encodings keep the bridge diagnostics
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                      // did the catalog recognize the encoding?
                ctx.emitter.instruction(&format!("js {fallback}"));            // unknown encodings keep the bridge diagnostics
            }
        }
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&fallback);
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEncodingAliases, false)?;
        emit_array_length_and_release(ctx);
        ctx.emitter.label(&done);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbEncodingAliases, false)?;
    emit_array_length_and_release(ctx);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `count(mb_list_encodings())` through the allocation-free catalog query.
pub(super) fn lower_list_encodings_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    // `mb_list_encodings()` enumerates the immutable shared catalog, whose count is fixed at
    // compile time. The bridge catalog test asserts the same 79 entries, so a catalog change
    // fails there before this immediate can drift.
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 79);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_regex_set_options(...))` through the allocation-free option length query.
pub(super) fn lower_regex_set_options_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() == 1 {
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, "mb_regex_set_options", pointer, length)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, x1");                         // pass the option pointer as the first C argument
                ctx.emitter.instruction("mov x1, x2");                         // pass the option length as the second C argument
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                      // pass the option pointer as the first SysV argument
                ctx.emitter.instruction("mov rsi, rdx");                      // pass the option length as the second SysV argument
            }
        }
        ctx.emitter.bl_c("elephc_mbstring_regex_set_options_len_v1");
        let fallback = ctx.next_label("mb_regex_set_options_len_fallback");
        let done = ctx.next_label("mb_regex_set_options_len_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");                         // did the option parser accept the value?
                ctx.emitter.instruction(&format!("b.lt {fallback}"));          // invalid options keep the bridge diagnostics
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                      // did the option parser accept the value?
                ctx.emitter.instruction(&format!("js {fallback}"));            // invalid options keep the bridge diagnostics
            }
        }
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&fallback);
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbRegexSetOptions, false)?;
        emit_string_length_and_release(ctx);
        ctx.emitter.label(&done);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbRegexSetOptions, false)?;
    emit_string_length_and_release(ctx);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_http_input(...))` through the allocation-free input length query.
pub(super) fn lower_http_input_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() == 1 {
        let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, "mb_http_input", pointer, length)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, x1");                         // pass the selector pointer as the first C argument
                ctx.emitter.instruction("mov x1, x2");                         // pass the selector length as the second C argument
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                      // pass the selector pointer as the first SysV argument
                ctx.emitter.instruction("mov rsi, rdx");                      // pass the selector length as the second SysV argument
            }
        }
        ctx.emitter.bl_c("elephc_mbstring_http_input_len_v1");
        let fallback = ctx.next_label("mb_http_input_len_fallback");
        let done = ctx.next_label("mb_http_input_len_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");                         // did the selector resolve to one string?
                ctx.emitter.instruction(&format!("b.lt {fallback}"));          // lists and invalid selectors keep the bridge
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                      // did the selector resolve to one string?
                ctx.emitter.instruction(&format!("js {fallback}"));            // lists and invalid selectors keep the bridge
            }
        }
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&fallback);
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbHttpInput, false)?;
        emit_boxed_string_length(ctx);
        ctx.emitter.label(&done);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbHttpInput, false)?;
    emit_boxed_string_length(ctx);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Lowers `strlen(mb_get_info("internal_encoding"))` through the catalog length query.
pub(super) fn lower_get_info_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() == 1 {
        if let Some(selector) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(
            ctx,
            inst.operands[0],
        )? {
            if selector.eq_ignore_ascii_case("internal_encoding") {
                ctx.emitter.bl_c("elephc_mbstring_internal_encoding_length_v1");
                let fallback = ctx.next_label("mb_get_info_len_fallback");
                let done = ctx.next_label("mb_get_info_len_done");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("cmp x0, #0");                 // did the catalog return a valid length?
                        ctx.emitter.instruction(&format!("b.lt {fallback}"));  // unknown selectors keep the bridge
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("test rax, rax");              // did the catalog return a valid length?
                        ctx.emitter.instruction(&format!("js {fallback}"));    // unknown selectors keep the bridge
                    }
                }
                abi::emit_jump(ctx.emitter, &done);
                ctx.emitter.label(&fallback);
                super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbGetInfo, false)?;
                emit_boxed_string_length(ctx);
                ctx.emitter.label(&done);
                store_if_result(ctx, inst)?;
                return Ok(());
            }
        }
    }
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbGetInfo, false)?;
    emit_boxed_string_length(ctx);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Unboxes a boxed `int|false` result and releases its temporary Mixed cell.
fn emit_unbox_boxed_int(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                        // reserve the cell and payload across unboxing
            ctx.emitter.instruction("str x0, [sp, #0]");                       // save the boxed cell before unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("str x1, [sp, #8]");                       // save the integer payload before releasing the cell
            ctx.emitter.instruction("ldr x0, [sp, #0]");                       // reload the cell for its final release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                       // return the unboxed integer payload
            ctx.emitter.instruction("add sp, sp, #32");                        // release the unboxing spill slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                            // reserve the cell and payload across unboxing
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");               // save the boxed cell before unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdi");           // save the integer payload before releasing the cell
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");               // reload the cell for its final release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");           // return the unboxed integer payload
            ctx.emitter.instruction("add rsp, 32");                            // release the unboxing spill slots
        }
    }
}

/// Normalizes a not-found signed position to PHP's `(int)false` zero.
fn emit_negative_to_zero(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                             // did the search report its negative not-found sentinel?
            ctx.emitter.instruction("csel x0, xzr, x0, lt");                    // map not-found to zero for the integer observer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the search report its negative not-found sentinel?
            ctx.emitter.instruction("mov r10, 0");                             // materialize the integer zero replacement
            ctx.emitter.instruction("cmovs rax, r10");                         // map not-found to zero for the integer observer
        }
    }
}

/// Lowers ASCII `mb_strstr()`-family searches through the ordinary substring helpers.
fn try_lower_strstr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    runtime_label: &str,
) -> Result<bool> {
    if inst.operands.len() != 4
        || !super::explicit_utf8_encoding(ctx, inst.operands[3])?
        || ctx.value_php_type(inst.operands[0])?.codegen_repr() != PhpType::Str
        || ctx.value_php_type(inst.operands[1])?.codegen_repr() != PhpType::Str
    {
        return Ok(false);
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_strstr", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_strstr_haystack_fallback");
    load_string_arg_to_regs(ctx, inst, 1, "mb_strstr", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    emit_predicate_branch(ctx, &fallback);
    // ASCII byte matches equal UTF-8 character matches, so reuse PHP's substring helpers.
    let mut synthetic = inst.clone();
    synthetic.operands.truncate(3);
    super::super::search::lower_strstr_no_store(ctx, &synthetic, runtime_label)?;
    finish_fast_path(ctx, inst, operation, fallback)
}

/// Lowers `strlen(mb_strstr(...))` through the allocation-free ASCII length helper.
pub(super) fn lower_strstr_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: RuntimeBuiltinId,
    mode: i64,
) -> Result<()> {
    let fast = inst.operands.len() == 4
        && super::explicit_utf8_encoding(ctx, inst.operands[3])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str
        && ctx.value_php_type(inst.operands[1])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, source, false)?;
        emit_boxed_string_length(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    super::super::common::materialize_truthy_flag(ctx, inst, 2, "mb_strstr")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                             // park the before-needle flag
            load_string_arg_to_regs(ctx, inst, 0, "mb_strstr", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the haystack while loading the needle
            load_string_arg_to_regs(ctx, inst, 1, "mb_strstr", "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                             // pass the needle pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                             // pass the needle length as the secondary string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the haystack into the primary string argument
            abi::emit_pop_reg(ctx.emitter, "x5");                              // restore the before-needle flag
            abi::emit_load_int_immediate(ctx.emitter, "x6", mode);             // select the forward/reverse case mode
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                            // park the before-needle flag
            load_string_arg_to_regs(ctx, inst, 0, "mb_strstr", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                // preserve the haystack while loading the needle
            load_string_arg_to_regs(ctx, inst, 1, "mb_strstr", "rax", "rdx")?;
            ctx.emitter.instruction("mov r10, rax");                           // preserve the needle pointer
            ctx.emitter.instruction("mov r11, rdx");                           // preserve the needle length
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                 // restore the haystack into the primary SysV string arguments
            abi::emit_pop_reg(ctx.emitter, "r8");                              // restore the before-needle flag
            ctx.emitter.instruction("mov rdx, r10");                           // pass the needle pointer as the third SysV string argument
            ctx.emitter.instruction("mov rcx, r11");                           // pass the needle length as the fourth SysV string argument
            abi::emit_load_int_immediate(ctx.emitter, "r9", mode);             // select the forward/reverse case mode
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mb_strstr_len_utf8");
    let fallback = ctx.next_label("mb_strstr_len_fallback");
    let done = ctx.next_label("mb_strstr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #-1");                             // did the ASCII helper decline the operands?
            ctx.emitter.instruction(&format!("b.eq {fallback}"));              // non-ASCII operands need the Unicode bridge
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                            // did the ASCII helper decline the operands?
            ctx.emitter.instruction(&format!("je {fallback}"));                // non-ASCII operands need the Unicode bridge
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, source, false)?;
    emit_boxed_string_length(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Unboxes a `string|false` cell and returns the byte length of a string, or zero for false.
fn emit_boxed_string_length(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                        // reserve the cell, tag, and length
            ctx.emitter.instruction("str x0, [sp, #0]");                       // save the boxed cell before unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("str x0, [sp, #8]");                       // save the runtime tag
            ctx.emitter.instruction("str x2, [sp, #16]");                      // save the string byte length
            ctx.emitter.instruction("ldr x0, [sp, #0]");                       // reload the cell for its final release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                      // start with the string byte length
            ctx.emitter.instruction("ldr x9, [sp, #8]");                       // reload the runtime tag
            ctx.emitter.instruction("cmp x9, #1");                             // is this a boxed string?
            ctx.emitter.instruction("csel x0, xzr, x0, ne");                   // false and null have length zero
            ctx.emitter.instruction("add sp, sp, #32");                        // release the unboxing spill slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                            // reserve the cell, tag, and length
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");               // save the boxed cell before unboxing
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");           // save the runtime tag
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");          // save the string byte length
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");               // reload the cell for its final release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");          // start with the string byte length
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 8]");           // reload the runtime tag
            ctx.emitter.instruction("cmp r10, 1");                             // is this a boxed string?
            ctx.emitter.instruction("mov r11, 0");                             // materialize the zero length for false and null
            ctx.emitter.instruction("cmovne rax, r11");                        // false and null have length zero
            ctx.emitter.instruction("add rsp, 32");                            // release the unboxing spill slots
        }
    }
}

/// Lowers `count(mb_str_split(...))` through the allocation-free ASCII chunk count.
pub(super) fn lower_str_split_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let fast = inst.operands.len() == 3
        && super::explicit_utf8_encoding(ctx, inst.operands[2])?
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str;
    if !fast {
        super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbStrSplit, false)?;
        emit_array_length_and_release(ctx);
        store_if_result(ctx, inst)?;
        return Ok(());
    }
    let (pointer, length) = (string_ptr_reg(ctx), string_len_reg(ctx));
    load_string_arg_to_regs(ctx, inst, 0, "mb_str_split", pointer, length)?;
    abi::emit_call_label(ctx.emitter, "__rt_mb_is_ascii");
    let fallback = ascii_predicate_fallback_label(ctx, "mb_str_split_count_ascii_fallback");
    let done = ctx.next_label("mb_str_split_count_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x2, [sp, #-16]!");                    // preserve the subject length across the chunk load
            load_as_int(ctx, inst.operands[1], "mb_str_split length")?;
            ctx.emitter.instruction("cmp x0, #0");                             // is the chunk length positive?
            let cleanup = ctx.next_label("mb_str_split_count_cleanup");
            ctx.emitter.instruction(&format!("b.le {cleanup}"));               // a non-positive chunk lets the bridge raise its ValueError
            ctx.emitter.instruction("mov x10, x0");                            // preserve the chunk length
            ctx.emitter.instruction("ldr x0, [sp], #16");                      // reload the subject byte length
            ctx.emitter.instruction("add x0, x0, x10");                        // add one chunk so the ceiling divide is exact
            ctx.emitter.instruction("sub x0, x0, #1");                         // complete the ceiling numerator
            ctx.emitter.instruction("udiv x0, x0, x10");                       // ceil(subject bytes / chunk bytes)
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&cleanup);
            ctx.emitter.instruction("add sp, sp, #16");                        // drop the saved subject length before the bridge
            abi::emit_jump(ctx.emitter, &fallback);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rdx");                              // preserve the subject length across the chunk load
            load_as_int(ctx, inst.operands[1], "mb_str_split length")?;
            ctx.emitter.instruction("test rax, rax");                         // is the chunk length positive?
            let cleanup = ctx.next_label("mb_str_split_count_cleanup_x86");
            ctx.emitter.instruction(&format!("jle {cleanup}"));                // a non-positive chunk lets the bridge raise its ValueError
            ctx.emitter.instruction("mov rcx, rax");                          // preserve the chunk length
            ctx.emitter.instruction("pop rax");                               // reload the subject byte length
            ctx.emitter.instruction("lea rax, [rax + rcx - 1]");              // add one chunk so the ceiling divide is exact
            ctx.emitter.instruction("xor edx, edx");                          // clear the dividend high half
            ctx.emitter.instruction("div rcx");                               // ceil(subject bytes / chunk bytes)
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&cleanup);
            ctx.emitter.instruction("pop rdx");                               // drop the saved subject length before the bridge
            abi::emit_jump(ctx.emitter, &fallback);
        }
    }
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, RuntimeBuiltinId::MbStrSplit, false)?;
    emit_array_length_and_release(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(())
}

/// Reads an indexed array's length header and releases the temporary array.
fn emit_array_length_and_release(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                        // reserve the array pointer and its length
            ctx.emitter.instruction("str x0, [sp, #0]");                       // save the array before reading its length
            ctx.emitter.instruction("ldr x9, [x0]");                           // load the indexed-array length header
            ctx.emitter.instruction("str x9, [sp, #8]");                       // save the length across the array release
            ctx.emitter.instruction("bl __rt_decref_any");                    // release the temporary split array
            ctx.emitter.instruction("ldr x0, [sp, #8]");                       // return the array length
            ctx.emitter.instruction("add sp, sp, #32");                        // release the length spill slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                            // reserve the array pointer and its length
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");               // save the array before reading its length
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");               // load the indexed-array length header
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r10");           // save the length across the array release
            ctx.emitter.instruction("call __rt_decref_any");                   // release the temporary split array
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");           // return the array length
            ctx.emitter.instruction("add rsp, 32");                            // release the length spill slots
        }
    }
}

/// Emits a branch to `fallback` when the ASCII predicate in the result register is false.
///
/// Returns the fallback label so the caller can finish the convergence block.
fn ascii_predicate_fallback_label(ctx: &mut FunctionContext<'_>, prefix: &str) -> String {
    let fallback = ctx.next_label(prefix);
    emit_predicate_branch(ctx, &fallback);
    fallback
}

/// Emits a branch to an existing fallback label when the ASCII predicate failed.
fn emit_predicate_branch(ctx: &mut FunctionContext<'_>, fallback: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {fallback}"));           // a zero predicate means a non-ASCII byte
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rcx, rcx");                          // did every byte stay inside ASCII?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // a zero predicate means a non-ASCII byte
        }
    }
}

/// Branches around the shared bridge only when the ASCII helper returned a null pointer.
///
/// Returns the fallback label so the caller can finish the convergence block.
fn ascii_fallback_label(ctx: &mut FunctionContext<'_>, prefix: &str) -> String {
    let fallback = ctx.next_label(prefix);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {fallback}"));           // a null result pointer means the subject is not ASCII
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                          // did the ASCII converter accept the subject?
            ctx.emitter.instruction(&format!("jz {fallback}"));                // a null result pointer means the subject is not ASCII
        }
    }
    fallback
}

/// Emits the shared bridge fallback, converges both branches, and stores the result.
fn finish_fast_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    fallback: String,
) -> Result<bool> {
    let done = ctx.next_label("mb_ascii_fast_done");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    super::lower_mbstring_impl(ctx, inst, operation, false)?;
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Returns a literal `i64` operand when the value was produced by `const_i64`.
fn const_int_operand(ctx: &FunctionContext<'_>, value: crate::ir::ValueId) -> Result<Option<i64>> {
    let Some(definition) = ctx.function.value(value) else { return Ok(None) };
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else { return Ok(None) };
    let Some(instruction) = ctx.function.instruction(inst) else { return Ok(None) };
    Ok(match (instruction.op, &instruction.immediate) {
        (crate::ir::Op::ConstI64, Some(crate::ir::Immediate::I64(value))) => Some(*value),
        _ => None,
    })
}
