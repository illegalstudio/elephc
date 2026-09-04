//! Purpose:
//! Lowers compact Throwable method payload access.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Returns true when a direct method call can be satisfied from the compact Throwable payload.
///
/// PDOException and its subclasses keep `getCode()` and `getPrevious()` on their PHP
/// implementations because those values live outside the compiler-owned base payload.
pub(super) fn is_throwable_standard_method_call(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let method_key = php_symbol_key(method_name);
    let mut current = Some(class_name.trim_start_matches('\\'));
    let mut pdo_exception_receiver = false;
    while let Some(name) = current {
        if name == "PDOException" {
            pdo_exception_receiver = true;
            break;
        }
        current = ctx
            .module
            .class_infos
            .get(name)
            .and_then(|info| info.parent.as_deref());
    }
    if pdo_exception_receiver && matches!(method_key.as_str(), "getcode" | "getprevious") {
        return false;
    }
    is_throwable_standard_method_key(&method_key)
        && is_throwable_like_class(ctx, class_name)
}

/// Returns true for method keys supplied by PHP's built-in `Throwable` surface.
pub(super) fn is_throwable_standard_method_key(method_key: &str) -> bool {
    matches!(
        method_key,
        "getmessage"
            | "getcode"
            | "getfile"
            | "getline"
            | "gettrace"
            | "gettraceasstring"
            | "getprevious"
            | "__tostring"
    )
}

/// Returns true when class metadata says the receiver is Throwable-compatible.
pub(super) fn is_throwable_like_class(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    if matches!(class_name, "Throwable") {
        return true;
    }
    if interface_extends_throwable(ctx, class_name) {
        return true;
    }
    let mut current = Some(class_name);
    while let Some(name) = current {
        let Some(class_info) = ctx.module.class_infos.get(name) else {
            return false;
        };
        if class_info
            .interfaces
            .iter()
            .any(|interface| interface == "Throwable")
        {
            return true;
        }
        current = class_info.parent.as_deref();
    }
    false
}

/// Returns true when an interface is `Throwable` or transitively extends it.
pub(super) fn interface_extends_throwable(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    if interface_name == "Throwable" {
        return true;
    }
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        let parent = parent.trim_start_matches('\\');
        parent == "Throwable" || interface_extends_throwable(ctx, parent)
    })
}

/// Lowers compact Throwable methods without requiring synthetic EIR method bodies.
pub(super) fn lower_throwable_standard_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Throwable::{} with {} EIR operands",
            method_name,
            inst.operands.len()
        )));
    }
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    lower_throwable_standard_method_loaded(ctx, inst, object_reg, method_name)
}

/// Lowers a compact Throwable method when the receiver object pointer is already in `object_reg`.
///
/// Used by nullable `?Throwable` / Mixed-unbox paths that have already extracted the object
/// payload and must not reload the original SSA value (which may still be a Mixed cell).
pub(super) fn lower_throwable_standard_method_from_reg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object_reg: &str,
    method_name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::unsupported(format!(
            "Throwable::{} with {} EIR operands",
            method_name,
            inst.operands.len()
        )));
    }
    lower_throwable_standard_method_loaded(ctx, inst, object_reg, method_name)
}

/// Shared compact-payload Throwable method body after the receiver object is in `object_reg`.
pub(super) fn lower_throwable_standard_method_loaded(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object_reg: &str,
    method_name: &str,
) -> Result<()> {
    let return_ty = match php_symbol_key(method_name).as_str() {
        "getmessage" => lower_throwable_get_message(ctx, object_reg),
        "getcode" => lower_throwable_get_code(ctx, object_reg),
        "getfile" => lower_throwable_get_file(ctx),
        "gettraceasstring" => lower_throwable_get_trace_as_string(ctx, object_reg),
        "getline" => lower_throwable_get_line(ctx, object_reg),
        "gettrace" => lower_throwable_empty_trace_array(ctx),
        "getprevious" => lower_throwable_get_previous(ctx, object_reg, inst),
        "__tostring" => lower_throwable_get_message(ctx, object_reg),
        _ => Err(CodegenIrError::unsupported(format!(
            "Throwable intrinsic method {}",
            method_name
        ))),
    }?;
    if inst.result.is_some()
        && matches!(inst.result_php_type.codegen_repr(), PhpType::Mixed)
        && !matches!(return_ty.codegen_repr(), PhpType::Mixed)
    {
        emit_box_current_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    }
    store_if_result(ctx, inst)
}

/// Loads `Throwable::getMessage()` from payload offsets 8/16 and returns a caller-owned string copy.
pub(super) fn lower_throwable_get_message(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, ptr_reg, object_reg, 8);
    abi::emit_load_from_address(ctx.emitter, len_reg, object_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    Ok(PhpType::Str)
}

/// Loads `Throwable::getCode()` from payload offset 24 into the integer result register.
pub(super) fn lower_throwable_get_code(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, 24);
    Ok(PhpType::Int)
}

/// Renders `Throwable::getTraceAsString()` from the frames recorded for THIS exception.
///
/// php's text is the same frames the uncaught report prints, without the `Stack trace:` header,
/// without the ` thrown in` tail, and — because php's frames are newline-SEPARATED — without a
/// trailing newline. A frameless exception is `#0 {main}`, not the empty string.
///
/// The completeness proof travels on the payload rather than being consulted from a global,
/// because the site that built this exception is long gone by the time anyone asks. Without a
/// proof the answer stays EMPTY: a trace that is short asserts an empty stack, which is a wrong
/// answer rather than a missing one.
pub(super) fn lower_throwable_get_trace_as_string(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
) -> Result<PhpType> {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_from_address(
        ctx.emitter,
        arg_reg,
        object_reg,
        crate::codegen_support::sentinels::THROWABLE_TRACE_EXACT_OFFSET as usize,
    );
    abi::emit_call_label(ctx.emitter, "__rt_trace_as_string");
    Ok(PhpType::Str)
}

/// Loads `Throwable::getFile()` from the compiled script's canonical path.
///
/// The path is a per-MODULE constant rather than a per-object field because EIR spans carry a line
/// and column but no filename (`crate::span::Span`), so the compiler has exactly one path to
/// report. That is the same string `__FILE__` yields, and it is right for every single-file
/// program; code merged in from an `include` reports the including script's path, which is the
/// known limit of this approximation.
///
/// `__rt_str_persist` gives the caller an owned copy, matching `getMessage()`, so the result can be
/// released like any other string without freeing the shared constant.
pub(super) fn lower_throwable_get_file(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, "_script_source_file");
    abi::emit_load_symbol_to_reg(ctx.emitter, len_reg, "_script_source_file_len", 0);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    Ok(PhpType::Str)
}

/// Loads `Throwable::getLine()` from the creation line stamped into the compact payload.
///
/// Zero means the Throwable had no user `new` behind it — an `ArithmeticError` raised by a
/// division, say — which is the same value the method returned for everything before the slot
/// existed.
pub(super) fn lower_throwable_get_line(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(
        ctx.emitter,
        result_reg,
        object_reg,
        crate::codegen_support::sentinels::THROWABLE_CREATION_LINE_OFFSET as usize,
    );
    Ok(PhpType::Int)
}

/// Materializes the synthetic empty indexed array used by `Throwable::getTrace()`.
pub(super) fn lower_throwable_empty_trace_array(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 4);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 4);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Loads `Throwable::getPrevious()` from payload offset 40, retaining a non-null previous.
///
/// When the EIR result is `Mixed` (`?Throwable`), both the object and null arms box here and
/// return `Mixed` so the shared intrinsic post-box path does not retag a live object as null
/// (`PhpType::Void` → runtime tag 8).
pub(super) fn lower_throwable_get_previous(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    inst: &Instruction,
) -> Result<PhpType> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let null_label = ctx.next_label("throwable_previous_null");
    let done_label = ctx.next_label("throwable_previous_done");
    let result_is_mixed = matches!(inst.result_php_type.codegen_repr(), PhpType::Mixed);
    let object_ty = PhpType::Object("Throwable".to_string());
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, 40);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, null_label)); // missing previous → null
            // `__rt_incref` expects the object in x0.
            if result_reg != "x0" {
                ctx.emitter
                    .instruction(&format!("mov x0, {}", result_reg)); // move previous into incref arg
            }
            abi::emit_call_label(ctx.emitter, "__rt_incref"); // caller owns the returned previous
            if result_reg != "x0" {
                ctx.emitter
                    .instruction(&format!("mov {}, x0", result_reg)); // restore result register
            }
            if result_is_mixed {
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
            }
            ctx.emitter
                .instruction(&format!("b {}", done_label)); // skip null materialization
            ctx.emitter.label(&null_label);
            if result_is_mixed {
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
            } else {
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0x7fff_ffff_ffff_fffe);
            }
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("test {}, {}", result_reg, result_reg)
            );                                                                  // missing previous → null
            ctx.emitter
                .instruction(&format!("jz {}", null_label));
            if result_reg != "rax" {
                ctx.emitter
                    .instruction(&format!("mov rax, {}", result_reg)); // move previous into incref arg
            }
            abi::emit_call_label(ctx.emitter, "__rt_incref"); // caller owns the returned previous
            if result_reg != "rax" {
                ctx.emitter
                    .instruction(&format!("mov {}, rax", result_reg)); // restore result register
            }
            if result_is_mixed {
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
            }
            ctx.emitter
                .instruction(&format!("jmp {}", done_label)); // skip null materialization
            ctx.emitter.label(&null_label);
            if result_is_mixed {
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
            } else {
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0x7fff_ffff_ffff_fffe);
            }
            ctx.emitter.label(&done_label);
        }
    }
    if result_is_mixed {
        Ok(PhpType::Mixed)
    } else {
        Ok(object_ty)
    }
}
