//! Purpose:
//! Lowers PHP `strspn`, `strcspn`, `strrchr`, `substr_compare` and `preg_quote` to their
//! runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch groups via the strings facade.
//!
//! Key details:
//! - `strspn` / `strcspn` share `__rt_strspn`, selected by a mode register; a `null` (or
//!   omitted) `$length` is passed as `i64::MAX`, which the helper clamps to the window.
//! - `strrchr` boxes its `string|false` result exactly like `strstr`.
//! - `substr_compare` raises PHP's catchable `ValueError` from the helper's error code.
//! - `preg_quote` passes the delimiter's first byte, or a sentinel when there is none.

use super::*;
use crate::codegen_support::runtime::{
    PREG_QUOTE_NO_DELIMITER, STRSPN_MODE_COMPLEMENT, STRSPN_MODE_SPAN,
    SUBSTR_COMPARE_NEGATIVE_LENGTH,
};

/// `ValueError` raised for a negative explicit `substr_compare()` `$length`.
const SUBSTR_COMPARE_NEGATIVE_LENGTH_MESSAGE: &str =
    "substr_compare(): Argument #4 ($length) must be greater than or equal to 0";

/// `ValueError` raised for a `substr_compare()` `$offset` past the end of the haystack.
const SUBSTR_COMPARE_OFFSET_MESSAGE: &str =
    "substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)";

/// Lowers `strspn(...)` / `strcspn(...)` through the shared span helper.
pub(crate) fn lower_strspn(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    complement: bool,
) -> Result<()> {
    let name = if complement { "strcspn" } else { "strspn" };
    if inst.operands.len() < 2 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "{name} expected 2 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    let length_kind = optional_null_length_kind(ctx, inst, 3)?;
    let mode = if complement { STRSPN_MODE_COMPLEMENT } else { STRSPN_MODE_SPAN };
    let (subject_ptr, subject_len, mask_ptr, mask_len, offset_reg, length_reg, mode_reg) =
        match ctx.emitter.target.arch {
            Arch::AArch64 => ("x1", "x2", "x3", "x4", "x5", "x6", "x0"),
            Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx", "r8", "r9", "r10"),
        };
    let subject = expect_operand(inst, 0)?;
    let mask = expect_operand(inst, 1)?;
    load_value_as_string_to_regs(ctx, subject, name, subject_ptr, subject_len)?;
    abi::emit_push_reg_pair(ctx.emitter, subject_ptr, subject_len);
    load_value_as_string_to_regs(ctx, mask, name, mask_ptr, mask_len)?;
    abi::emit_push_reg_pair(ctx.emitter, mask_ptr, mask_len);
    if let Some(&offset) = inst.operands.get(2) {
        load_as_int(ctx, offset, name)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match length_kind {
        OptionalLengthKind::Absent => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::MAX);
        }
        _ => {
            let length = expect_operand(inst, 3)?;
            load_optional_length_null_as_max(ctx, length, name, length_kind)?;
        }
    }
    abi::emit_reg_move(ctx.emitter, length_reg, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, offset_reg);
    abi::emit_pop_reg_pair(ctx.emitter, mask_ptr, mask_len);
    abi::emit_pop_reg_pair(ctx.emitter, subject_ptr, subject_len);
    abi::emit_load_int_immediate(ctx.emitter, mode_reg, mode);
    abi::emit_call_label(ctx.emitter, "__rt_strspn");
    store_if_result(ctx, inst)
}

/// Lowers `strrchr(haystack, needle, before_needle?)` and boxes its `string|false` result.
pub(crate) fn lower_strrchr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "strrchr expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        // `crate::builtins::string::strrchr::check` types every call `string|false`, whose
        // representation is `Mixed`; the arms below leave a boxed cell in the result register.
        return Err(CodegenIrError::invalid_module(format!(
            "strrchr result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }
    let prefix = ctx.next_label("strrchr_prefix");
    let miss = ctx.next_label("strrchr_miss");
    let box_match = ctx.next_label("strrchr_box_match");
    let end = ctx.next_label("strrchr_end");
    // `$before_needle` is materialized first and parked: the string materialization and the
    // helper call clobber every register that could hold it.
    materialize_truthy_flag(ctx, inst, 2, "strrchr")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (hay_ptr, hay_len, needle_ptr, needle_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2", "x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx"),
    };
    load_string_arg_to_regs(ctx, inst, 0, "strrchr", hay_ptr, hay_len)?;
    abi::emit_push_reg_pair(ctx.emitter, hay_ptr, hay_len);
    load_string_arg_to_regs(ctx, inst, 1, "strrchr", needle_ptr, needle_len)?;
    abi::emit_pop_reg_pair(ctx.emitter, hay_ptr, hay_len);
    abi::emit_push_reg_pair(ctx.emitter, hay_ptr, hay_len);
    abi::emit_call_label(ctx.emitter, "__rt_strrchr");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the haystack for substring selection
            ctx.emitter.instruction("ldr x9, [sp], #16");                       // reload the parked $before_needle flag
            ctx.emitter.instruction("cmp x0, #0");                              // did the helper find the byte?
            ctx.emitter.instruction(&format!("b.lt {miss}"));                   // PHP returns false when the byte is absent
            ctx.emitter.instruction("cmp x9, #0");                              // was $before_needle truthy?
            ctx.emitter.instruction(&format!("b.ne {prefix}"));                 // a truthy flag selects the part before the byte
            ctx.emitter.instruction("add x1, x1, x0");                          // advance the haystack pointer to the last occurrence
            ctx.emitter.instruction("sub x2, x2, x0");                          // shrink the length to the suffix from that byte on
            ctx.emitter.instruction(&format!("b {box_match}"));                 // both hit arms box the selected substring identically
            ctx.emitter.label(&prefix);
            ctx.emitter.instruction("mov x2, x0");                              // keep the haystack pointer and cut at the last occurrence
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, rax");                             // keep the match offset while restoring the haystack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_pop_reg(ctx.emitter, "r9");                               // reload the parked $before_needle flag
            ctx.emitter.instruction("cmp r8, 0");                               // did the helper find the byte?
            ctx.emitter.instruction(&format!("jl {miss}"));                     // PHP returns false when the byte is absent
            ctx.emitter.instruction("cmp r9, 0");                               // was $before_needle truthy?
            ctx.emitter.instruction(&format!("jne {prefix}"));                  // a truthy flag selects the part before the byte
            ctx.emitter.instruction("add rax, r8");                             // advance the haystack pointer to the last occurrence
            ctx.emitter.instruction("sub rdx, r8");                             // shrink the length to the suffix from that byte on
            ctx.emitter.instruction(&format!("jmp {box_match}"));               // both hit arms box the selected substring identically
            ctx.emitter.label(&prefix);
            ctx.emitter.instruction("mov rdx, r8");                             // keep the haystack pointer and cut at the last occurrence
        }
    }
    ctx.emitter.label(&box_match);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &end);
    ctx.emitter.label(&miss);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&end);
    store_if_result(ctx, inst)
}

/// Lowers `substr_compare(...)` and raises PHP's `ValueError` from the helper's error code.
pub(crate) fn lower_substr_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 3 || inst.operands.len() > 5 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr_compare expected 3 to 5 args, got {}",
            inst.operands.len()
        )));
    }
    let length_kind = optional_null_length_kind(ctx, inst, 3)?;
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let (hay_ptr, hay_len, needle_ptr, needle_len, offset_reg, length_reg, has_length_reg, ci_reg) =
        if arm {
            ("x1", "x2", "x3", "x4", "x5", "x6", "x7", "x0")
        } else {
            ("rdi", "rsi", "rdx", "rcx", "r8", "r9", "r10", "r11")
        };
    let result = abi::int_result_reg(ctx.emitter);
    load_string_arg_to_regs(ctx, inst, 0, "substr_compare", hay_ptr, hay_len)?;
    abi::emit_push_reg_pair(ctx.emitter, hay_ptr, hay_len);
    load_string_arg_to_regs(ctx, inst, 1, "substr_compare", needle_ptr, needle_len)?;
    abi::emit_push_reg_pair(ctx.emitter, needle_ptr, needle_len);
    let offset = expect_operand(inst, 2)?;
    load_as_int(ctx, offset, "substr_compare offset")?;
    abi::emit_push_reg(ctx.emitter, result);
    // The length travels with a "given" flag: `null` and omission compare up to the longer
    // operand, while an explicit 0 or negative length is checked first.
    match length_kind {
        OptionalLengthKind::Absent => {
            abi::emit_load_int_immediate(ctx.emitter, result, 0);
            abi::emit_push_reg(ctx.emitter, result);
            abi::emit_push_reg(ctx.emitter, result);
        }
        OptionalLengthKind::Concrete => {
            let length = expect_operand(inst, 3)?;
            load_as_int(ctx, length, "substr_compare length")?;
            abi::emit_push_reg(ctx.emitter, result);
            abi::emit_load_int_immediate(ctx.emitter, result, 1);
            abi::emit_push_reg(ctx.emitter, result);
        }
        OptionalLengthKind::TaggedScalar | OptionalLengthKind::Boxed => {
            let length = expect_operand(inst, 3)?;
            load_optional_length_null_as_max(ctx, length, "substr_compare length", length_kind)?;
            abi::emit_push_reg(ctx.emitter, result);
            // `i64::MAX` is the null marker of the loader: no PHP string is that long, so an
            // explicit `PHP_INT_MAX` and null both compare through the end, as in php-src.
            if arm {
                abi::emit_load_int_immediate(ctx.emitter, "x9", i64::MAX);
                ctx.emitter.instruction("cmp x0, x9");                          // did the loader map a runtime null to the end marker?
                ctx.emitter.instruction("cset x0, ne");                         // only a non-null length counts as given
            } else {
                abi::emit_load_int_immediate(ctx.emitter, "r11", i64::MAX);
                ctx.emitter.instruction("cmp rax, r11");                        // did the loader map a runtime null to the end marker?
                ctx.emitter.instruction("setne al");                            // only a non-null length counts as given
                ctx.emitter.instruction("movzx eax, al");                       // widen the given flag
            }
            abi::emit_push_reg(ctx.emitter, result);
        }
    }
    materialize_truthy_flag(ctx, inst, 4, "substr_compare")?;
    abi::emit_reg_move(ctx.emitter, ci_reg, result);
    abi::emit_pop_reg(ctx.emitter, has_length_reg);
    abi::emit_pop_reg(ctx.emitter, length_reg);
    abi::emit_pop_reg(ctx.emitter, offset_reg);
    abi::emit_pop_reg_pair(ctx.emitter, needle_ptr, needle_len);
    abi::emit_pop_reg_pair(ctx.emitter, hay_ptr, hay_len);
    abi::emit_call_label(ctx.emitter, "__rt_substr_compare");
    let not_length = ctx.next_label("substr_compare_not_length");
    let ok = ctx.next_label("substr_compare_ok");
    if arm {
        ctx.emitter.instruction(&format!("cbz x1, {ok}"));                      // a zero error code is a valid comparison result
        ctx.emitter.instruction(&format!("cmp x1, #{SUBSTR_COMPARE_NEGATIVE_LENGTH}")); // was the explicit length negative?
        ctx.emitter.instruction(&format!("b.ne {not_length}"));                 // otherwise the offset was out of range
    } else {
        ctx.emitter.instruction("test rdx, rdx");                               // a zero error code is a valid comparison result
        ctx.emitter.instruction(&format!("jz {ok}"));                           // keep the comparison result
        ctx.emitter.instruction(&format!("cmp rdx, {SUBSTR_COMPARE_NEGATIVE_LENGTH}")); // was the explicit length negative?
        ctx.emitter.instruction(&format!("jne {not_length}"));                  // otherwise the offset was out of range
    }
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COMPARE_NEGATIVE_LENGTH_MESSAGE);
    ctx.emitter.label(&not_length);
    super::super::exceptions::emit_value_error(ctx, SUBSTR_COMPARE_OFFSET_MESSAGE);
    ctx.emitter.label(&ok);
    store_if_result(ctx, inst)
}

/// Lowers `preg_quote(str, delimiter?)` through the escaping helper.
pub(crate) fn lower_preg_quote(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "preg_quote expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let (src_ptr, src_len, delimiter_reg) = if arm { ("x1", "x2", "x3") } else { ("rax", "rdx", "rdi") };
    let delimiter_given = match inst.operands.get(1) {
        Some(&delimiter) => !matches!(
            ctx.value_php_type(delimiter)?.codegen_repr(),
            PhpType::Void | PhpType::Never
        ),
        None => false,
    };
    if delimiter_given {
        // A runtime null converts to "", which selects no delimiter, exactly like PHP's null.
        let (ptr, len) = if arm { ("x1", "x2") } else { ("rax", "rdx") };
        load_string_arg_to_regs(ctx, inst, 1, "preg_quote", ptr, len)?;
        let none = ctx.next_label("preg_quote_no_delimiter");
        let ready = ctx.next_label("preg_quote_delimiter_ready");
        if arm {
            ctx.emitter.instruction(&format!("cbz x2, {none}"));                // an empty delimiter escapes nothing extra
            ctx.emitter.instruction("ldrb w0, [x1]");                           // php-src uses only the delimiter's first byte
            ctx.emitter.instruction(&format!("b {ready}"));                     // the delimiter byte is selected
        } else {
            ctx.emitter.instruction("test rdx, rdx");                           // is the delimiter empty?
            ctx.emitter.instruction(&format!("jz {none}"));                     // an empty delimiter escapes nothing extra
            ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");               // php-src uses only the delimiter's first byte
            ctx.emitter.instruction(&format!("jmp {ready}"));                   // the delimiter byte is selected
        }
        ctx.emitter.label(&none);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), PREG_QUOTE_NO_DELIMITER);
        ctx.emitter.label(&ready);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), PREG_QUOTE_NO_DELIMITER);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_arg_to_regs(ctx, inst, 0, "preg_quote", src_ptr, src_len)?;
    abi::emit_pop_reg(ctx.emitter, delimiter_reg);
    abi::emit_call_label(ctx.emitter, "__rt_preg_quote");
    store_if_result(ctx, inst)
}
