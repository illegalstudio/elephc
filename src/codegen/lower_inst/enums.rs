//! Purpose:
//! Lowers enum-specific static helper methods for the EIR backend.
//! Handles enum singleton arrays and backed-enum lookup helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_static_method_call()`.
//!
//! Key details:
//! - Enum cases are pre-initialized global singleton object slots.
//! - `Enum::cases()` returns a new indexed array that owns retained singleton references.
//! - `Enum::from()` throws catchable `ValueError`; `tryFrom()` returns boxed null on no match.

use crate::codegen::abi;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::names::{enum_case_symbol, php_symbol_key};
use crate::types::{EnumCaseValue, EnumInfo, PhpType};

use super::super::context::FunctionContext;
use super::store_if_result;
use crate::codegen::{CodegenIrError, Result};

const ENUM_FROM_INVALID_BACKING_SUFFIX: &str = " is not a valid backing value for enum ";
const ENUM_FROM_INVALID_STRING_BACKING_SUFFIX: &str = "\" is not a valid backing value for enum ";

/// Attempts to lower a static method call when the receiver is an enum.
pub(super) fn try_lower_enum_static_method(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    method_name: &str,
    inst: &Instruction,
) -> Result<Option<()>> {
    let method_key = php_symbol_key(method_name);
    if !ctx.module.enum_infos.contains_key(enum_name) {
        return Ok(None);
    }
    match method_key.as_str() {
        "cases" => {
            lower_enum_cases(ctx, enum_name, inst)?;
            Ok(Some(()))
        }
        "from" => {
            lower_enum_from_like(ctx, enum_name, inst, false)?;
            Ok(Some(()))
        }
        "tryfrom" => {
            lower_enum_from_like(ctx, enum_name, inst, true)?;
            Ok(Some(()))
        }
        _ => Ok(None),
    }
}

/// Lowers `EnumName::cases()` into a fresh indexed array of retained singleton objects.
fn lower_enum_cases(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    inst: &Instruction,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{}::cases with EIR arguments",
            enum_name
        )));
    }
    let case_names = ctx
        .module
        .enum_infos
        .get(enum_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("enum cases for {}", enum_name)))?
        .cases
        .iter()
        .map(|case| case.name.clone())
        .collect::<Vec<_>>();
    emit_enum_cases_array(ctx, enum_name, &case_names)?;
    store_if_result(ctx, inst)
}

/// Emits allocation and element stores for an enum cases result array.
fn emit_enum_cases_array(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    case_names: &[String],
) -> Result<()> {
    let capacity = case_names.len().max(4);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let array_ptr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::temp_int_reg(ctx.emitter.target);
    emit_array_new_call(ctx, capacity);
    abi::emit_push_reg(ctx.emitter, result_reg);
    let elem_ty = PhpType::Object(enum_name.to_string());
    for (index, case_name) in case_names.iter().enumerate() {
        emit_enum_case_store(ctx, enum_name, case_name, index, &elem_ty, array_ptr_reg, len_reg);
    }
    abi::emit_pop_reg(ctx.emitter, result_reg);
    Ok(())
}

/// Emits the target-specific `__rt_array_new` call for an enum cases array.
fn emit_array_new_call(ctx: &mut FunctionContext<'_>, capacity: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Stores one retained enum case singleton into the in-progress cases array.
fn emit_enum_case_store(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
    index: usize,
    elem_ty: &PhpType,
    array_ptr_reg: &str,
    len_reg: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let case_label = enum_case_symbol(enum_name, case_name);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &case_label, 0);
    abi::emit_incref_if_refcounted(ctx.emitter, elem_ty);
    abi::emit_load_temporary_stack_slot(ctx.emitter, array_ptr_reg, 0);
    if index == 0 {
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, array_ptr_reg, elem_ty);
    }
    abi::emit_store_to_address(ctx.emitter, result_reg, array_ptr_reg, 24 + index * 8);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, (index + 1) as i64);
    abi::emit_store_to_address(ctx.emitter, len_reg, array_ptr_reg, 0);
}

/// Lowers `EnumName::from(value)` or `EnumName::tryFrom(value)` for backed enums.
fn lower_enum_from_like(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    inst: &Instruction,
    is_try: bool,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "{}::{} with {} EIR operands",
            enum_name,
            if is_try { "tryFrom" } else { "from" },
            inst.operands.len()
        )));
    }
    let enum_info = ctx
        .module
        .enum_infos
        .get(enum_name)
        .cloned()
        .ok_or_else(|| CodegenIrError::unsupported(format!("enum method for {}", enum_name)))?;
    let backing_ty = enum_info.backing_type.clone().ok_or_else(|| {
        CodegenIrError::unsupported(format!("{}::from on pure enum", enum_name))
    })?;
    let input = inst.operands[0];
    let input_ty = ctx.load_value_to_result(input)?;
    if input_ty.codegen_repr() != backing_ty.codegen_repr() {
        return Err(CodegenIrError::unsupported(format!(
            "{}::{} backing input PHP type {:?}",
            enum_name,
            if is_try { "tryFrom" } else { "from" },
            input_ty
        )));
    }
    emit_enum_from_scan(ctx, enum_name, &enum_info, &backing_ty, is_try)?;
    store_if_result(ctx, inst)
}

/// Emits the backing-value scan and no-match behavior for enum `from` helpers.
fn emit_enum_from_scan(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    enum_info: &EnumInfo,
    backing_ty: &PhpType,
    is_try: bool,
) -> Result<()> {
    let success_label = ctx.next_label("enum_from_success");
    let done_label = ctx.next_label("enum_from_done");
    let string_cleanup_label = if matches!(backing_ty, PhpType::Str) {
        Some(ctx.next_label("enum_from_cleanup_input"))
    } else {
        None
    };
    match backing_ty {
        PhpType::Int => emit_int_enum_from_scan(ctx, enum_name, enum_info, &success_label)?,
        PhpType::Str => emit_string_enum_from_scan(
            ctx,
            enum_name,
            enum_info,
            &success_label,
            string_cleanup_label.as_deref(),
        )?,
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "{}::from backing PHP type {:?}",
                enum_name, backing_ty
            )));
        }
    }
    if is_try {
        emit_enum_try_from_null(ctx, backing_ty);
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        emit_enum_from_value_error(ctx, enum_name, backing_ty)?;
    }
    if let Some(cleanup_label) = &string_cleanup_label {
        ctx.emitter.label(cleanup_label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, &success_label);
    }
    ctx.emitter.label(&success_label);
    if is_try {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(enum_name.to_string()));
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the integer-backed enum case comparison loop.
fn emit_int_enum_from_scan(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    enum_info: &EnumInfo,
    success_label: &str,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let case_value_reg = abi::temp_int_reg(ctx.emitter.target);
    for case in &enum_info.cases {
        let Some(EnumCaseValue::Int(value)) = case.value.as_ref() else {
            continue;
        };
        let next_label = ctx.next_label("enum_from_next");
        abi::emit_load_int_immediate(ctx.emitter, case_value_reg, *value);
        let compare_inst = format!("cmp {}, {}", result_reg, case_value_reg);
        ctx.emitter.instruction(&compare_inst);                                 // compare the input integer with this enum backing value
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("b.ne {}", next_label));       // continue scanning when this enum case does not match
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("jne {}", next_label));        // continue scanning when this enum case does not match
            }
        }
        emit_load_enum_case_singleton(ctx, enum_name, &case.name);
        abi::emit_jump(ctx.emitter, success_label);
        ctx.emitter.label(&next_label);
    }
    Ok(())
}

/// Emits the string-backed enum case comparison loop.
fn emit_string_enum_from_scan(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    enum_info: &EnumInfo,
    success_label: &str,
    cleanup_label: Option<&str>,
) -> Result<()> {
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, string_ptr_reg, string_len_reg);
    for case in &enum_info.cases {
        let Some(EnumCaseValue::Str(value)) = case.value.as_ref() else {
            continue;
        };
        let match_label = ctx.next_label("enum_from_case");
        let next_label = ctx.next_label("enum_from_next");
        emit_string_case_compare(ctx, value, &match_label);
        abi::emit_jump(ctx.emitter, &next_label);
        ctx.emitter.label(&match_label);
        emit_load_enum_case_singleton(ctx, enum_name, &case.name);
        if let Some(cleanup_label) = cleanup_label {
            abi::emit_jump(ctx.emitter, cleanup_label);
        } else {
            abi::emit_jump(ctx.emitter, success_label);
        }
        ctx.emitter.label(&next_label);
    }
    Ok(())
}

/// Emits one string-backed enum value comparison against the preserved input.
fn emit_string_case_compare(
    ctx: &mut FunctionContext<'_>,
    value: &str,
    match_label: &str,
) {
    let bytes = crate::string_bytes::literal_bytes(value);
    let (label, len) = ctx.data.add_string(&bytes);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, match_label);
}

/// Loads one enum singleton object into the integer result register.
fn emit_load_enum_case_singleton(ctx: &mut FunctionContext<'_>, enum_name: &str, case_name: &str) {
    let case_label = enum_case_symbol(enum_name, case_name);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &case_label, 0);
    // `from()`/`tryFrom()` return an owned reference to the case singleton: the caller's
    // lowering acquires the result for its destination and releases the temporary, so the
    // matched singleton must be retained here. Without this incref the singleton's refcount
    // drifts down by one per call (a reassigned result plus the temporary release), which
    // eventually frees the persistent case object and corrupts the heap (issue #349).
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Object(enum_name.to_string()));
}

/// Emits the boxed `null` return for an unmatched `tryFrom`.
fn emit_enum_try_from_null(ctx: &mut FunctionContext<'_>, backing_ty: &PhpType) {
    if matches!(backing_ty, PhpType::Str) {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    emit_null_into_result(ctx);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Builds and throws the PHP-compatible `ValueError` for unmatched `from`.
fn emit_enum_from_value_error(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    backing_ty: &PhpType,
) -> Result<()> {
    match backing_ty {
        PhpType::Int => emit_enum_from_int_value_error_message(ctx, enum_name),
        PhpType::Str => emit_enum_from_string_value_error_message(ctx, enum_name),
        _ => return Ok(()),
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    if matches!(backing_ty, PhpType::Str) {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    emit_throw_value_error_from_string_result(ctx);
    Ok(())
}

/// Emits the dynamic error message for an unmatched integer-backed enum value.
fn emit_enum_from_int_value_error_message(ctx: &mut FunctionContext<'_>, enum_name: &str) {
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    let suffix = format!("{}{}", ENUM_FROM_INVALID_BACKING_SUFFIX, enum_name);
    emit_concat_current_string_with_static_suffix(ctx, &suffix);
}

/// Emits the dynamic error message for an unmatched string-backed enum value.
fn emit_enum_from_string_value_error_message(ctx: &mut FunctionContext<'_>, enum_name: &str) {
    emit_concat_static_prefix_with_preserved_string(ctx, "\"");
    let suffix = format!(
        "{}{}",
        ENUM_FROM_INVALID_STRING_BACKING_SUFFIX,
        enum_name
    );
    emit_concat_current_string_with_static_suffix(ctx, &suffix);
}

/// Concatenates a static prefix with the preserved enum input string.
fn emit_concat_static_prefix_with_preserved_string(ctx: &mut FunctionContext<'_>, prefix: &str) {
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", 8);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Concatenates the current string result with a static suffix.
fn emit_concat_current_string_with_static_suffix(ctx: &mut FunctionContext<'_>, suffix: &str) {
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", suffix_len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Allocates a `ValueError` from the current persisted string result and throws it.
fn emit_throw_value_error_from_string_result(ctx: &mut FunctionContext<'_>) {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throw_value_error_from_string_result_aarch64(ctx),
        Arch::X86_64 => emit_throw_value_error_from_string_result_x86_64(ctx),
    }
}

/// Emits the AArch64 `ValueError` allocation and unwinder handoff.
fn emit_throw_value_error_from_string_result_aarch64(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, "x0", 32);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov x9, #6");                                      // heap kind 6 = throwable object instance
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_spl_value_error_class_id", 0);
    ctx.emitter.instruction("str x9, [x0]");                                    // store ValueError class id at object header
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
    ctx.emitter.instruction("str x9, [x0, #8]");                                // store dynamic exception message pointer
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 8);
    ctx.emitter.instruction("str x9, [x0, #16]");                               // store dynamic exception message length
    ctx.emitter.instruction("str xzr, [x0, #24]");                              // exception code defaults to zero
    abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
}

/// Emits the x86_64 `ValueError` allocation and unwinder handoff.
fn emit_throw_value_error_from_string_result_x86_64(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, "rax", 32);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov r10, 0x4548504c00000006");                     // x86_64 heap-kind word: HE LP magic + kind 6 object
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_value_error_class_id", 0);
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // store ValueError class id at object header
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
    ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");                    // store dynamic exception message pointer
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
    ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");                   // store dynamic exception message length
    ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");                     // exception code defaults to zero
    abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
}

/// Lowers `Op::EnumBackingStringToInt`: coerces a PHP numeric string operand into the
/// integer backing value for an int-backed enum `from()`/`tryFrom()`. A valid PHP numeric
/// string (whitespace-tolerant, float-form truncated toward zero) is accepted by the
/// int-parameter coercion probe and converted via `__rt_str_to_int`; a non-numeric string
/// throws a catchable `TypeError` whose message is carried by the instruction's data
/// immediate, matching PHP's coercive-typing behavior.
/// The integer result flows to the enum `from()` call as an ordinary scalar operand, so
/// the backing scan and its refcount handling run on a plain int (not a heap string).
pub(super) fn lower_enum_backing_string_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("enum_backing_string_to_int without operand".to_string())
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "enum_backing_string_to_int without a TypeError message immediate".to_string(),
        ));
    };
    let (message_label, message_len) = ctx.intern_string_data(data_id)?;
    ctx.load_value_to_result(input)?;
    let type_error_label = ctx.next_label("enum_from_type_error");
    let done_label = ctx.next_label("enum_from_coerce_done");
    emit_string_result_to_int_checked(ctx, &type_error_label);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&type_error_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throw_enum_from_type_error_aarch64(ctx, &message_label, message_len),
        Arch::X86_64 => emit_throw_enum_from_type_error_x86_64(ctx, &message_label, message_len),
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Probes PHP numeric validity of a string already in the string-result registers. On a
/// valid numeric string the integer value (float-form truncated toward zero) is left in the
/// int-result register and control falls through. On a non-numeric string, control branches
/// to `invalid_label` with the 16-byte temporary still on the stack — the caller is
/// responsible for releasing it and emitting the `TypeError` there.
fn emit_string_result_to_int_checked(ctx: &mut FunctionContext<'_>, invalid_label: &str) {
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    let int_reg = abi::int_result_reg(ctx.emitter);
    // Preserve the input string across the numeric-validity probe, which clobbers the
    // string-result registers.
    abi::emit_push_reg_pair(ctx.emitter, string_ptr_reg, string_len_reg);
    abi::emit_call_label(ctx.emitter, "__rt_str_looks_like_int_for_coercion");
    // The coercion probe returns 1 in the int-result register for strings PHP accepts
    // for an int parameter, 0 otherwise; rejected strings throw `TypeError`.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", int_reg, invalid_label)); // non-numeric string throws TypeError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", int_reg, int_reg)); // set flags from the numeric-validity result
            ctx.emitter.instruction(&format!("jz {}", invalid_label));          // non-numeric string throws TypeError
        }
    }
    // Valid numeric string: restore it and convert to the integer backing value.
    abi::emit_load_temporary_stack_slot(ctx.emitter, string_ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, string_len_reg, 8);
    abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Lowers `Op::EnumBackingMixedToInt`: coerces a `Mixed` operand to the integer backing
/// value for an int-backed enum `from()`/`tryFrom()`. Unboxes the runtime tag and, matching
/// PHP: int/bool forward the payload, float truncates toward zero, null becomes 0, a numeric
/// string coerces (a non-numeric string throws `TypeError`), and array/object/resource/
/// callable throw `TypeError`. The instruction's data immediate holds the message prefix
/// (`"E::from(): Argument #1 ($value) must be of type int, "`); codegen appends the runtime
/// type word to build PHP's exact message.
pub(super) fn lower_enum_backing_mixed_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("enum_backing_mixed_to_int without operand".to_string())
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "enum_backing_mixed_to_int without a TypeError message prefix".to_string(),
        ));
    };
    let (prefix_label, prefix_len) = ctx.intern_string_data(data_id)?;
    ctx.load_value_to_result(input)?;
    // Unbox the Mixed cell. `__rt_mixed_unbox` returns tag in the int-result register and the
    // payload lo/hi in target-specific registers (AArch64: x1/x2; x86_64: rdi/rdx).
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let (lo_reg, hi_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    let done = ctx.next_label("enum_mixed_coerce_done");
    let l_scalar = ctx.next_label("enum_mixed_scalar");
    let l_float = ctx.next_label("enum_mixed_float");
    let l_null = ctx.next_label("enum_mixed_null");
    let l_string = ctx.next_label("enum_mixed_string");
    let l_array = ctx.next_label("enum_mixed_array");
    let l_object = ctx.next_label("enum_mixed_object");
    let l_resource = ctx.next_label("enum_mixed_resource");
    let l_callable = ctx.next_label("enum_mixed_callable");
    // Tag values: 0 int, 1 string, 2 float, 3 bool, 4 indexed array, 5 hash, 6 object,
    // 8 null, 9 resource, 10 callable (7 nested is peeled by `__rt_mixed_unbox`).
    emit_mixed_tag_branch(ctx, tag_reg, 0, &l_scalar);
    emit_mixed_tag_branch(ctx, tag_reg, 3, &l_scalar);
    emit_mixed_tag_branch(ctx, tag_reg, 8, &l_null);
    emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float);
    emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string);
    emit_mixed_tag_branch(ctx, tag_reg, 4, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 5, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 6, &l_object);
    emit_mixed_tag_branch(ctx, tag_reg, 9, &l_resource);
    emit_mixed_tag_branch(ctx, tag_reg, 10, &l_callable);
    // Any other tag is a non-coercible object-like value.
    abi::emit_jump(ctx.emitter, &l_object);
    // int / bool: the payload is already the integer value.
    ctx.emitter.label(&l_scalar);
    emit_move_reg(ctx, tag_reg, lo_reg);
    abi::emit_jump(ctx.emitter, &done);
    // null: PHP coerces to 0 (which then has no matching case → ValueError).
    ctx.emitter.label(&l_null);
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, 0);
    abi::emit_jump(ctx.emitter, &done);
    // float: truncate toward zero.
    ctx.emitter.label(&l_float);
    emit_float_payload_to_int(ctx, lo_reg);
    abi::emit_jump(ctx.emitter, &done);
    // string: move the payload ptr/len into the string-result regs, then coerce strictly.
    ctx.emitter.label(&l_string);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    emit_move_reg(ctx, string_ptr_reg, lo_reg);
    emit_move_reg(ctx, string_len_reg, hi_reg);
    let string_invalid = ctx.next_label("enum_mixed_string_invalid");
    emit_string_result_to_int_checked(ctx, &string_invalid);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&string_invalid);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "string given");
    // Non-coercible types throw a TypeError naming the runtime type, like PHP.
    ctx.emitter.label(&l_array);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "array given");
    ctx.emitter.label(&l_resource);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "resource given");
    ctx.emitter.label(&l_callable);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "Closure given");
    ctx.emitter.label(&l_object);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "object given");
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Emits a `tag == value` comparison and a branch to `target` on equality.
fn emit_mixed_tag_branch(ctx: &mut FunctionContext<'_>, tag_reg: &str, value: i64, target: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #{}", tag_reg, value));   // compare the unboxed Mixed tag with this type
            ctx.emitter.instruction(&format!("b.eq {}", target));               // dispatch to the matching type handler
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", tag_reg, value));    // compare the unboxed Mixed tag with this type
            ctx.emitter.instruction(&format!("je {}", target));                 // dispatch to the matching type handler
        }
    }
}

/// Moves `src` into `dst` (int-result register), no-op when they alias.
fn emit_move_reg(ctx: &mut FunctionContext<'_>, dst: &str, src: &str) {
    if dst != src {
        ctx.emitter.instruction(&format!("mov {}, {}", dst, src));              // forward the unboxed integer payload to the result register
    }
}

/// Truncates a Mixed float payload (raw double bits in `bits_reg`) toward zero into the
/// int-result register, following PHP's float→int coercion.
fn emit_float_payload_to_int(ctx: &mut FunctionContext<'_>, bits_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("fmov d0, {}", bits_reg));         // move the raw double bits into the float register
            ctx.emitter.instruction("fcvtzs x0, d0");                           // truncate the double toward zero into the int result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movq xmm0, {}", bits_reg));       // move the raw double bits into the float register
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // truncate the double toward zero into the int result
        }
    }
}

/// Builds `<prefix><suffix>` (e.g. prefix `"E::from(): … must be of type int, "` + suffix
/// `"array given"`) and throws it as a catchable `TypeError` through the standard unwinder.
fn emit_throw_int_arg_type_error(
    ctx: &mut FunctionContext<'_>,
    prefix_label: &str,
    prefix_len: usize,
    suffix: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, prefix_len as i64);
    emit_concat_current_string_with_static_suffix(ctx, suffix);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_throw_type_error_from_string_result(ctx);
}

/// Allocates a `TypeError` from the current persisted string result and throws it.
fn emit_throw_type_error_from_string_result(ctx: &mut FunctionContext<'_>) {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 32);
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_spl_type_error_class_id", 0);
            ctx.emitter.instruction("str x9, [x0]");                            // store TypeError class id at object header
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store dynamic exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 8);
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store dynamic exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 32);
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov r10, 0x4548504c00000006");             // x86_64 heap-kind word: HELP magic + kind 6 object
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp allocation as a runtime object
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_type_error_class_id", 0);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store TypeError class id at object header
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store dynamic exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");           // store dynamic exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
}

/// Emits the AArch64 `TypeError` allocation, static-message stamping, and unwinder handoff.
fn emit_throw_enum_from_type_error_aarch64(
    ctx: &mut FunctionContext<'_>,
    message_label: &str,
    message_len: usize,
) {
    abi::emit_load_int_immediate(ctx.emitter, "x0", 32);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov x9, #6");                                      // heap kind 6 = throwable object instance
    ctx.emitter.instruction("str x9, [x0, #-8]");                               // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_spl_type_error_class_id", 0);
    ctx.emitter.instruction("str x9, [x0]");                                    // store TypeError class id at object header
    abi::emit_symbol_address(ctx.emitter, "x9", message_label);
    ctx.emitter.instruction("str x9, [x0, #8]");                                // store static exception message pointer
    abi::emit_load_int_immediate(ctx.emitter, "x9", message_len as i64);
    ctx.emitter.instruction("str x9, [x0, #16]");                               // store static exception message length
    ctx.emitter.instruction("str xzr, [x0, #24]");                              // exception code defaults to zero
    abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
}

/// Emits the x86_64 `TypeError` allocation, static-message stamping, and unwinder handoff.
fn emit_throw_enum_from_type_error_x86_64(
    ctx: &mut FunctionContext<'_>,
    message_label: &str,
    message_len: usize,
) {
    abi::emit_load_int_immediate(ctx.emitter, "rax", 32);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov r10, 0x4548504c00000006");                     // x86_64 heap-kind word: HELP magic + kind 6 object
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_spl_type_error_class_id", 0);
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // store TypeError class id at object header
    abi::emit_symbol_address(ctx.emitter, "r10", message_label);
    ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");                    // store static exception message pointer
    abi::emit_load_int_immediate(ctx.emitter, "r10", message_len as i64);
    ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");                   // store static exception message length
    ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");                     // exception code defaults to zero
    abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
}

/// Materializes the runtime null sentinel into the integer result register.
fn emit_null_into_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe_u64 as i64,
    );
}
