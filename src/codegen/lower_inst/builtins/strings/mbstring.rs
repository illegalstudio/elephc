//! Purpose:
//! Passes concrete AOT values to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - Typed mbstring RuntimeFnId dispatch and generated callable wrappers.
//!
//! Key details:
//! - Stack-local invoker descriptors borrow raw SSA values without allocating argument owners.
//! - The coordinator copies values before coercion and owns all conversion/snapshot cleanup.
//! - Physical-source strictness reaches direct calls; callable wrapper inputs retain their existing validation.

use super::*;
use elephc_builtin_contract::{RuntimeBuiltinId, TypeSpec};

mod capture;
mod mbstring_fast;

/// Stages borrowed value descriptors and invokes shared PHP argument preparation on either architecture.
pub(crate) fn lower_mbstring(ctx: &mut FunctionContext<'_>, inst: &Instruction, operation: RuntimeBuiltinId) -> Result<()> {
    if mbstring_fast::try_lower(ctx, inst, operation)? {
        return Ok(());
    }
    lower_mbstring_impl(ctx, inst, operation, true)
}

/// Lowers `mb_strlen()` with a direct UTF-8 counter when the encoding is explicitly UTF-8.
///
/// The fast path validates the subject and returns `-1` for malformed input, in which case
/// the shared coordinator runs the same call and reproduces PHP's replacement-character
/// counting. An omitted, null, or non-UTF-8 encoding stays on the coordinator.
pub(crate) fn lower_mb_strlen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<()> {
    if operation == RuntimeBuiltinId::MbStrlen
        && inst.operands.len() == 2
        && ctx.value_php_type(inst.operands[0])?.codegen_repr() == PhpType::Str
        && explicit_utf8_encoding(ctx, inst.operands[1])?
    {
        // An ASCII subject has one UTF-8 codepoint per byte, so fold the count even when
        // the literal only became visible after constant propagation.
        if let Some(subject) = crate::codegen::lower_inst::builtins::shared::maybe_const_string_operand(
            ctx,
            inst.operands[0],
        )? {
            if subject.is_ascii() {
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    subject.len() as i64,
                );
                return store_if_result(ctx, inst);
            }
        }
        let (ptr_reg, len_reg) = (string_ptr_reg(ctx), string_len_reg(ctx));
        load_string_arg_to_regs(ctx, inst, 0, "mb_strlen", ptr_reg, len_reg)?;
        abi::emit_call_label(ctx.emitter, "__rt_mb_strlen_utf8");
        let done = ctx.next_label("mb_strlen_utf8_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #-1");                         // did the helper reject malformed UTF-8?
                ctx.emitter.instruction(&format!("b.ne {done}"));               // a valid count is the final result
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, -1");                         // did the helper reject malformed UTF-8?
                ctx.emitter.instruction(&format!("jne {done}"));                // a valid count is the final result
            }
        }
        lower_mbstring_impl(ctx, inst, operation, false)?;
        ctx.emitter.label(&done);
        return store_if_result(ctx, inst);
    }
    lower_mbstring(ctx, inst, operation)
}

/// Lowers a raw integer mbstring search selected by the position cast sink.
pub(crate) fn lower_mb_position_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: crate::ir::RuntimeFnId,
) -> Result<()> {
    use crate::ir::RuntimeFnId;
    let (source, name, label, direction) = match target {
        RuntimeFnId::MbStrposInt => (
            RuntimeBuiltinId::MbStrpos,
            "mb_strpos",
            "__rt_strpos",
            super::StringPositionDirection::Forward,
        ),
        RuntimeFnId::MbStriposInt => (
            RuntimeBuiltinId::MbStripos,
            "mb_stripos",
            "__rt_stripos",
            super::StringPositionDirection::Forward,
        ),
        RuntimeFnId::MbStrrposInt => (
            RuntimeBuiltinId::MbStrrpos,
            "mb_strrpos",
            "__rt_strrpos",
            super::StringPositionDirection::Reverse,
        ),
        RuntimeFnId::MbStrriposInt => (
            RuntimeBuiltinId::MbStrripos,
            "mb_strripos",
            "__rt_strripos",
            super::StringPositionDirection::Reverse,
        ),
        _ => {
            return Err(CodegenIrError::invalid_module(
                "raw mbstring position lowering requires an mbstring position target",
            ))
        }
    };
    mbstring_fast::lower_position_int(ctx, inst, source, name, label, direction)
}

/// Lowers a raw integer mbstring substring-search length selected by the length sink.
pub(crate) fn lower_mb_strstr_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: crate::ir::RuntimeFnId,
) -> Result<()> {
    use crate::ir::RuntimeFnId;
    let (source, mode) = match target {
        RuntimeFnId::MbStrstrLen => (RuntimeBuiltinId::MbStrstr, 0),
        RuntimeFnId::MbStristrLen => (RuntimeBuiltinId::MbStristr, 1),
        RuntimeFnId::MbStrrchrLen => (RuntimeBuiltinId::MbStrrchr, 2),
        RuntimeFnId::MbStrrichrLen => (RuntimeBuiltinId::MbStrrichr, 3),
        _ => {
            return Err(CodegenIrError::invalid_module(
                "raw mbstring search-length lowering requires an mbstring length target",
            ))
        }
    };
    mbstring_fast::lower_strstr_len(ctx, inst, source, mode)
}

/// Lowers an allocation-free mbstring split count selected by the split count sink.
pub(crate) fn lower_mb_str_split_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_str_split_count(ctx, inst)
}

/// Lowers a raw integer mbstring codepoint selected by the position cast sink.
pub(crate) fn lower_mb_ord_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_ord_int(ctx, inst)
}

/// Lowers an allocation-free mbstring encoding-detection length selected by the length sink.
pub(crate) fn lower_mb_detect_encoding_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_detect_encoding_len(ctx, inst)
}

/// Lowers an allocation-free mbstring codepoint length selected by the length sink.
pub(crate) fn lower_mb_chr_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_chr_len(ctx, inst)
}

/// Lowers an allocation-free mbstring trim-width length selected by the length sink.
pub(crate) fn lower_mb_strimwidth_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_strimwidth_len(ctx, inst)
}

/// Lowers an allocation-free mbstring conversion length selected by the length sink.
pub(crate) fn lower_mb_convert_encoding_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_convert_encoding_len(ctx, inst)
}

/// Lowers an allocation-free numeric-entity encode length selected by the length sink.
pub(crate) fn lower_mb_encode_numericentity_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_encode_numericentity_len(ctx, inst)
}

/// Lowers an allocation-free numeric-entity decode length selected by the length sink.
pub(crate) fn lower_mb_decode_numericentity_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_decode_numericentity_len(ctx, inst)
}

/// Lowers an allocation-free literal-separator split count selected by the split sink.
pub(crate) fn lower_mb_split_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_split_count(ctx, inst)
}

/// Lowers an allocation-free `mb_ereg_replace()` length selected by the length sink.
pub(crate) fn lower_mb_ereg_replace_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_ereg_replace_len(ctx, inst)
}

/// Lowers an allocation-free preferred MIME name length selected by the length sink.
pub(crate) fn lower_mb_preferred_mime_name_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_preferred_mime_name_len(ctx, inst)
}

/// Lowers the specialized one-element UTF-8 detection-order setter.
pub(crate) fn lower_mb_detect_order_single(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_detect_order_single(ctx, inst, target)
}

/// Lowers an allocation-free mbstring alias count selected by the catalog count sink.
pub(crate) fn lower_mb_encoding_aliases_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_encoding_aliases_count(ctx, inst)
}

/// Lowers an allocation-free mbstring encoding list count selected by the catalog count sink.
pub(crate) fn lower_mb_list_encodings_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_list_encodings_count(ctx, inst)
}

/// Lowers an allocation-free mbstring info length selected by the length sink.
pub(crate) fn lower_mb_get_info_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_get_info_len(ctx, inst)
}

/// Lowers a raw integer mbstring setting selected by the integer cast sink.
pub(crate) fn lower_mb_setting_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: crate::ir::RuntimeFnId,
) -> Result<()> {
    use crate::ir::RuntimeFnId;
    let (source, helper, string_arg) = match target {
        RuntimeFnId::MbInternalEncodingInt => (
            RuntimeBuiltinId::MbInternalEncoding,
            "elephc_mbstring_internal_encoding_set_v1",
            true,
        ),
        RuntimeFnId::MbLanguageInt => (
            RuntimeBuiltinId::MbLanguage,
            "elephc_mbstring_language_set_v1",
            true,
        ),
        RuntimeFnId::MbHttpOutputInt => (
            RuntimeBuiltinId::MbHttpOutput,
            "elephc_mbstring_http_output_set_v1",
            true,
        ),
        RuntimeFnId::MbRegexEncodingInt => (
            RuntimeBuiltinId::MbRegexEncoding,
            "elephc_mbstring_regex_encoding_set_v1",
            true,
        ),
        RuntimeFnId::MbSubstituteCharacterInt => (
            RuntimeBuiltinId::MbSubstituteCharacter,
            "elephc_mbstring_substitute_character_set_v1",
            false,
        ),
        _ => {
            return Err(CodegenIrError::invalid_module(
                "raw mbstring setting lowering requires a setting target",
            ))
        }
    };
    mbstring_fast::lower_setting_int(ctx, inst, source, helper, string_arg)
}

/// Lowers an allocation-free mbstring regex option length selected by the length sink.
pub(crate) fn lower_mb_regex_set_options_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_regex_set_options_len(ctx, inst)
}

/// Lowers an allocation-free mbstring HTTP input length selected by the length sink.
pub(crate) fn lower_mb_http_input_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    _target: crate::ir::RuntimeFnId,
) -> Result<()> {
    mbstring_fast::lower_http_input_len(ctx, inst)
}

/// Returns whether a literal encoding operand names UTF-8 or its `UTF8` alias.
fn explicit_utf8_encoding(ctx: &FunctionContext<'_>, value: crate::ir::ValueId) -> Result<bool> {
    let mut value = value;
    for _ in 0..4 {
        let Some(definition) = ctx.function.value(value) else {
            break;
        };
        let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
            break;
        };
        let Some(instruction) = ctx.function.instruction(inst) else {
            break;
        };
        if instruction.op == crate::ir::Op::Acquire && instruction.operands.len() == 1 {
            value = instruction.operands[0];
            continue;
        }
        break;
    }
    let Some(encoding) = super::super::shared::maybe_const_string_operand(ctx, value)? else {
        return Ok(false);
    };
    Ok(encoding.eq_ignore_ascii_case("UTF-8") || encoding.eq_ignore_ascii_case("UTF8"))
}

/// Shared mbstring invocation with an optional result store for a fast-path fallback.
fn lower_mbstring_impl(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
    store: bool,
) -> Result<()> {
    let capture = matches!(operation, RuntimeBuiltinId::MbEreg | RuntimeBuiltinId::MbEregi);
    let query = operation == RuntimeBuiltinId::MbParseStr;
    let variables = operation == RuntimeBuiltinId::MbConvertVariables;
    let output = if query { Some(1) } else if capture || variables { Some(2) } else { None };
    if matches!(inst.immediate, Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction {
        arguments: crate::ir::RuntimeArgumentLayout::IndexedArray, ..
    }))) {
        if output.is_some() { return Err(CodegenIrError::unsupported("dynamic spread mbstring output references are not yet supported")); }
        return lower_packed(ctx, inst, operation);
    }
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let pointers_size = (inst.operands.len() * 8 + 15) & !15;
    let state_offset = pointers_size + inst.operands.len() * 48;
    let size = state_offset + if query { crate::codegen_support::mbstring_query::STATE_BYTES }
        else if capture { 16 } else { 0 };
    let strict = match inst.immediate {
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction { strict_types: Some(strict), .. })) => strict,
        // Callable wrapper parameters are currently validated by their descriptor invoker.
        // Propagating the invoking source profile remains necessary when that validation moves here.
        _ => false,
    };
    abi::emit_reserve_temporary_stack(ctx.emitter, size);
    for (index, &value) in inst.operands.iter().enumerate() {
        if variables && index == 3 {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction(&format!("str x0, [sp, #{}]", index * 8)), // preserve the live reference address in the bridge pointer table
                Arch::X86_64 => ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", index * 8)), // preserve the live reference address in the bridge pointer table
            }
            continue;
        }
        if output == Some(index) {
            if variables { capture::stage_live_reference(ctx, value, index * 8)?; }
            else { capture::stage_reference(ctx, value, index * 8)?; }
            continue;
        }
        let record = pointers_size + index * 48;
        let actual = ctx.load_value_to_result(value)?.codegen_repr();
        match ctx.emitter.target.arch {
            Arch::AArch64 => stage_aarch64(ctx, &actual, record, index * 8),
            Arch::X86_64 => stage_x86_64(ctx, &actual, record, index * 8),
        }

    }
    super::load_optional_sprintf_eval_context(ctx, 4)?;
    let target = ctx.emitter.target;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), inst.operands.len() as i64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), i64::from(strict));
    if query {
        crate::codegen_support::mbstring_query::stage(ctx.emitter, state_offset);
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_query_native");
    } else if capture {
        capture::stage_state(ctx, state_offset);
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_capture_native");
    } else if variables {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 5), 1);
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_variables_native");
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_native");
    }
    abi::emit_release_temporary_stack(ctx.emitter, size);
    finish_result(ctx, inst, contract.returns, store)
}

/// Adapts the shared successful result into the PHP type declared by the neutral contract.
fn finish_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    returns: TypeSpec,
    store: bool,
) -> Result<()> {
    let target = ctx.emitter.target;
    if returns == TypeSpec::Str {
        match target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x1, x0"),             // adapt the owned native string pointer to the PHP string-result pair
            Arch::X86_64 => ctx.emitter.instruction("mov rdx, rcx"),            // adapt the returned native byte length to the PHP string-result pair
        }
    }
    if matches!(returns, TypeSpec::Mixed | TypeSpec::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_box_result");
    }
    if store { store_if_result(ctx, inst) } else { Ok(()) }
}

/// Invokes the same coordinator over an owned Mixed array without padding or truncating its length.
fn lower_packed(ctx: &mut FunctionContext<'_>, inst: &Instruction, operation: RuntimeBuiltinId) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    if ctx.value_php_type(array)? != PhpType::Array(Box::new(PhpType::Mixed)) {
        return Err(CodegenIrError::invalid_module("packed mbstring arguments require array<mixed>"));
    }
    let strict = matches!(inst.immediate, Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction {
        strict_types: Some(true), ..
    })));
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.max_args.unwrap_or(contract.params.len());
    let size = (capacity * 8 + 15) & !15;
    abi::emit_reserve_temporary_stack(ctx.emitter, size);
    super::load_optional_sprintf_eval_context(ctx, 4)?;
    let target = ctx.emitter.target;
    let scratch = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    ctx.load_value_to_reg(array, scratch)?;
    abi::emit_load_from_address(ctx.emitter, abi::int_arg_reg_name(target, 2), scratch, 0);
    // Native heap arrays can be byte-aligned. Rust's pointer slice requires aligned storage.
    // Stage only the bounded accepted argument prefix; the coordinator retains actual arity.
    let ready = ctx.next_label("mbstring_packed_arguments_ready");
    for index in 0..capacity {
        match target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x2, #{index}"));          // compare the actual argument count with the next pointer slot
                ctx.emitter.instruction(&format!("b.ls {ready}"));              // leave absent arguments unread for shared arity validation
                ctx.emitter.instruction(&format!("ldr x10, [x9, #{}]", 24 + index * 8)); // read the native array's potentially unaligned pointer payload
                ctx.emitter.instruction(&format!("str x10, [sp, #{}]", index * 8)); // copy the borrowed cell pointer into aligned C argument storage
            },
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rdx, {index}"));          // preserve actual arity independently of the fixed staging capacity
                ctx.emitter.instruction(&format!("jbe {ready}"));               // skip absent pointer slots without padding PHP arguments
                ctx.emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", 24 + index * 8)); // read one possibly unaligned native array element
                ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", index * 8)); // provide Rust with an aligned pointer slice
            },
        }
    }
    ctx.emitter.label(&ready);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), i64::from(strict));
    abi::emit_call_label(ctx.emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(ctx.emitter, size);
    finish_result(ctx, inst, contract.returns, true)
}

/// Stores an AArch64 raw value and a borrowed invoker marker with its concrete runtime tag.
fn stage_aarch64(ctx: &mut FunctionContext<'_>, actual: &PhpType, record: usize, pointer: usize) {
    let raw = record + 24;
    match actual {
        PhpType::Str => {
            ctx.emitter.instruction(&format!("stp x1, x2, [sp, #{raw}]"));      // retain binary string pointer/length without coercion or allocation
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov x0, d0");                             // preserve all float bits before source-type dispatch
            ctx.emitter.instruction(&format!("stp x0, xzr, [sp, #{raw}]"));     // borrow the raw float bits with no high payload
        }
        _ => ctx.emitter.instruction(&format!("stp x0, xzr, [sp, #{raw}]")),    // borrow the scalar, object, array, or boxed value without transferring ownership
    }
    if *actual == PhpType::TaggedScalar {
        ctx.emitter.instruction("mov x9, x1");                                  // keep the nullable scalar's dynamic tag separate from its payload
    } else if *actual == PhpType::Iterable {
        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
        ctx.emitter.instruction("add x9, x0, #2");                              // map indexed/hash/object heap kinds to concrete PHP tags
        ctx.emitter.instruction("sub x10, x0, #2");                             // prepare an unsigned range check for the three valid iterable kinds
        ctx.emitter.instruction("cmp x10, #2");                                 // reject unknown or null iterable payload kinds
        ctx.emitter.instruction("mov x10, #8");                                 // prepare the existing boxed-iterable null fallback
        ctx.emitter.instruction("csel x9, x9, x10, ls");                        // keep the concrete kind only for a valid heap-backed iterable
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x9", crate::codegen::runtime_value_tag(actual) as i64);
    }
    ctx.emitter.instruction("mov x10, #11");                                    // describe borrowed raw storage using the shared invoker marker ABI
    ctx.emitter.instruction(&format!("add x11, sp, #{raw}"));                   // point the marker at this call's stable raw-value storage
    ctx.emitter.instruction(&format!("stp x10, x11, [sp, #{record}]"));         // stage the marker tag and borrowed storage address
    ctx.emitter.instruction(&format!("str x9, [sp, #{}]", record + 16));        // preserve the source tag for coordinator-owned value copying
    ctx.emitter.instruction(&format!("add x10, sp, #{record}"));                // point at the completed borrowed marker
    ctx.emitter.instruction(&format!("str x10, [sp, #{pointer}]"));             // populate the source-order argument-pointer array
}

/// Stores a SysV raw value and the identical invoker marker layout without temporary heap owners.
fn stage_x86_64(ctx: &mut FunctionContext<'_>, actual: &PhpType, record: usize, pointer: usize) {
    let raw = record + 24;
    if *actual == PhpType::Float {
        ctx.emitter.instruction("movq rax, xmm0");                              // preserve all floating-point bits for the shared coercion planner
    }
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {raw}], rax"));      // borrow the raw low payload while retaining its original type
    if *actual == PhpType::Str {
        ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rdx", raw + 8)); // preserve binary string length without converting the value
    } else {
        ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", raw + 8)); // scalar and heap-pointer values do not carry a high payload
    }
    if *actual == PhpType::TaggedScalar {
        ctx.emitter.instruction("mov r10, rdx");                                // retain the nullable scalar's runtime tag before any helper call
    } else if *actual == PhpType::Iterable {
        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
        ctx.emitter.instruction("lea r10, [rax + 2]");                          // map indexed/hash/object heap kinds to their concrete PHP tags
        ctx.emitter.instruction("sub rax, 2");                                  // prepare an unsigned range check covering exactly the valid iterable kinds
        ctx.emitter.instruction("cmp rax, 2");                                  // recognize null or unsupported payload kinds
        ctx.emitter.instruction("mov r11d, 8");                                 // prepare the existing boxed-iterable null fallback
        ctx.emitter.instruction("cmova r10, r11");                              // preserve a concrete tag only for valid heap-backed iterables
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "r10", crate::codegen::runtime_value_tag(actual) as i64);
    }
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {record}], 11"));    // use the shared marker for borrowed raw-value storage
    ctx.emitter.instruction(&format!("lea r11, [rsp + {raw}]"));                // address this call's stable raw-value slot
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", record + 8)); // publish borrowed storage for coordinator-owned copying
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", record + 16)); // publish the authoritative source value tag
    ctx.emitter.instruction(&format!("lea r11, [rsp + {record}]"));             // address the completed argument marker
    ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {pointer}], r11"));  // populate the argument-pointer array without allocating boxes
}
