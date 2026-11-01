//! Purpose:
//! Lowers compact Throwable method payload access.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - Nullable previous-exception results acquire their object owner exactly once during boxing.

use super::*;
use crate::codegen_support::emit::Emitter;

/// Returns true when a direct method call can be satisfied from the compact Throwable payload.
///
/// PDOException keeps getCode on its PHP implementation because its SQLSTATE string lives
/// outside the compiler-owned integer code payload. getPrevious uses the inherited final
/// Throwable accessor.
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
    if pdo_exception_receiver && method_key == "getcode" {
        return false;
    }
    is_throwable_standard_method_key(&method_key)
        && is_throwable_like_class(ctx, class_name)
}

/// Returns true for method keys supplied by PHP's built-in `Throwable` surface.
///
/// `getTraceAsString()` and `__toString()` are not among them any more: `Exception` and `Error`
/// emit real bodies for both (see `builtin_exception_to_string_method`), so they dispatch like
/// any method and a user `__toString()` override is honoured.
pub(super) fn is_throwable_standard_method_key(method_key: &str) -> bool {
    matches!(
        method_key,
        "getmessage" | "getcode" | "getfile" | "getline" | "gettrace" | "getprevious"
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
    let method_key = php_symbol_key(method_name);
    let return_ty = match method_key.as_str() {
        "getmessage" => lower_throwable_get_message(ctx, object_reg),
        "getcode" => lower_throwable_get_code(ctx, object_reg),
        "getfile" => lower_throwable_get_file(ctx, object_reg),
        "gettraceasstring" => lower_throwable_empty_string(ctx),
        "getline" => lower_throwable_get_line(ctx, object_reg),
        "gettrace" => lower_throwable_get_trace(ctx, object_reg),
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
        emit_box_throwable_method_result(ctx.emitter, &method_key, &return_ty.codegen_repr());
    }
    store_if_result(ctx, inst)
}

/// Transfers getter-owned payloads into Mixed while preserving the borrowed empty trace string.
fn emit_box_throwable_method_result(emitter: &mut Emitter, method_key: &str, return_ty: &PhpType) {
    if method_key == "gettraceasstring" {
        emit_box_current_value_as_mixed(emitter, return_ty);
    } else {
        // String getters persist their payload and getTrace allocates its own array.
        // Borrowed boxing would copy or retain those results without retiring the original owner.
        emit_box_current_owned_value_as_mixed(emitter, return_ty);
    }
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

/// Materializes the synthetic empty-string result used by `Throwable::getTraceAsString()`.
///
/// Still synthetic: reference PHP returns at least `#0 {main}`, and elephc keeps no call stack to
/// render. `getFile()` no longer shares this path — see [`lower_throwable_get_file`].
pub(super) fn lower_throwable_empty_string(ctx: &mut FunctionContext<'_>) -> Result<PhpType> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (label, len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
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
pub(super) fn lower_throwable_get_file(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), object_reg);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_get_file");
    Ok(PhpType::Str)
}

/// Loads `Throwable::getLine()` from the creation line stamped into the compact payload.
///
/// Zero means the Throwable had no user `new` behind it — an `ArithmeticError` raised by a
/// division, say — which is the same value the method returned for everything before the slot
/// existed.
pub(super) fn lower_throwable_get_line(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), object_reg);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_get_line");
    Ok(PhpType::Int)
}

/// Returns the trace recorded at creation, retained, or a fresh empty array when none was.
///
/// `__rt_throwable_capture_trace` fills the declared `trace` slot when interpreter frames are live
/// at the `new`. A compact payload that recorded nothing holds zero there; a class-declared
/// layout holds the default empty array. Both nonzero shapes are an owned array the object keeps,
/// so the caller gets its own reference.
///
/// The slot is not always raw. A class-declared layout runs the property initializer, which stores
/// the `trace` default through the slot's declared storage -- a boxed `Mixed` since the merge made
/// a declared `array` the packed|hash union -- while `__rt_throwable_capture_trace` writes a raw
/// array. Reading a box as an array returned its header words as elements: `ErrorException`'s
/// `getTrace()` gave `[0, 4294967328, 5, 8]`, and Symfony's `ErrorHandler::cleanTrace()` then
/// refused it. A boxed array or hash is unwrapped; any other box answers a fresh empty array.
pub(super) fn lower_throwable_get_trace(ctx: &mut FunctionContext<'_>, object_reg: &str) -> Result<PhpType> {
    let fresh_label = ctx.next_label("throwable_trace_fresh");
    let raw_label = ctx.next_label("throwable_trace_raw");
    let done_label = ctx.next_label("throwable_trace_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let trace_offset = crate::codegen_support::throwable_layout::TRACE_OFFSET;
    abi::emit_push_reg(ctx.emitter, object_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, trace_offset);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &fresh_label);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    emit_branch_if_result_not(ctx, 5, &raw_label);
    // -- a boxed Mixed slot: hand back its array or hash payload --
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, trace_offset);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let boxed_array = ctx.next_label("throwable_trace_boxed_array");
    emit_branch_if_result_is(ctx, 4, &boxed_array);
    emit_branch_if_result_is(ctx, 5, &boxed_array);
    abi::emit_jump(ctx.emitter, &fresh_label);
    ctx.emitter.label(&boxed_array);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_jump(ctx.emitter, &done_label);
    // -- a raw array slot --
    ctx.emitter.label(&raw_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, trace_offset);
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, trace_offset);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&fresh_label);
    abi::emit_pop_reg(ctx.emitter, object_reg);
    lower_throwable_empty_trace_array(ctx)?;
    ctx.emitter.label(&done_label);
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Branches when the integer result register holds exactly `value`.
fn emit_branch_if_result_is(ctx: &mut FunctionContext<'_>, value: i64, label: &str) {
    match ctx.emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{value}"));               // compare the runtime kind or tag
            ctx.emitter.instruction(&format!("b.eq {label}"));                  // take the matching shape's path
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {value}"));              // compare the runtime kind or tag
            ctx.emitter.instruction(&format!("je {label}"));                    // take the matching shape's path
        }
    }
}

/// Branches when the integer result register holds anything but `value`.
fn emit_branch_if_result_not(ctx: &mut FunctionContext<'_>, value: i64, label: &str) {
    match ctx.emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{value}"));               // compare the runtime kind or tag
            ctx.emitter.instruction(&format!("b.ne {label}"));                  // take the other shape's path
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {value}"));              // compare the runtime kind or tag
            ctx.emitter.instruction(&format!("jne {label}"));                   // take the other shape's path
        }
    }
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

/// Loads `Throwable::getPrevious()` from its declared slot, retaining a non-null previous.
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
    // Two producers write this slot with DIFFERENT shapes, so the read discriminates at
    // runtime instead of trusting either. The compiled `Exception::__construct` assigns
    // `$this->previous`, whose declared `?Throwable` type stores a BOXED Mixed cell — never a
    // raw pointer, and never null (a boxed null is a live cell with tag 8). The eval/native
    // bridge instead materializes a raw object pointer there. Reading every slot as raw made
    // `getPrevious()` answer non-null for an exception with no previous, which turned Symfony's
    // `do { ... } while ($prev = $prev->getPrevious());` into a walk off the end; reading every
    // slot as boxed broke the bridge round-trip instead. Heap kind 5 tells them apart.
    abi::emit_load_from_address(ctx.emitter, result_reg, object_reg, crate::codegen_support::throwable_layout::PREVIOUS_OFFSET);
    if result_is_mixed {
        let boxed_label = ctx.next_label("throwable_previous_boxed");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter
                    .instruction(&format!("cbz {}, {}", result_reg, null_label)); // an empty slot is a missing previous
                if result_reg != "x0" {
                    ctx.emitter.instruction(&format!("mov x0, {}", result_reg));
                }
                ctx.emitter.instruction("str x0, [sp, #-16]!");                 // preserve the slot across the kind probe
                abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
                ctx.emitter.instruction("mov x11, x0");                         // heap kind of the stored value
                ctx.emitter.instruction("ldr x0, [sp], #16");                   // restore the stored value
                ctx.emitter.instruction("cmp x11, #5");                         // kind 5 = boxed Mixed cell
                ctx.emitter.instruction(&format!("b.eq {}", boxed_label));
                abi::emit_call_label(ctx.emitter, "__rt_incref");               // raw object: caller owns it
                if result_reg != "x0" {
                    ctx.emitter.instruction(&format!("mov {}, x0", result_reg));
                }
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
                ctx.emitter.instruction(&format!("b {}", done_label));

                ctx.emitter.label(&boxed_label);
                abi::emit_call_label(ctx.emitter, "__rt_incref");               // boxed cell: retain and hand back as-is
                if result_reg != "x0" {
                    ctx.emitter.instruction(&format!("mov {}, x0", result_reg));
                }
                ctx.emitter.instruction(&format!("b {}", done_label));

                ctx.emitter.label(&null_label);
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
                ctx.emitter.label(&done_label);
            }
            Arch::X86_64 => {
                ctx.emitter
                    .instruction(&format!("test {}, {}", result_reg, result_reg)); // an empty slot is a missing previous
                ctx.emitter.instruction(&format!("jz {}", null_label));
                if result_reg != "rax" {
                    ctx.emitter.instruction(&format!("mov rax, {}", result_reg));
                }
                ctx.emitter.instruction("push rax");                            // preserve the slot across the kind probe
                ctx.emitter.instruction("sub rsp, 8");                          // keep the stack 16-byte aligned for the call
                abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
                ctx.emitter.instruction("mov r11, rax");                        // heap kind of the stored value
                ctx.emitter.instruction("add rsp, 8");                          // undo the alignment padding
                ctx.emitter.instruction("pop rax");                             // restore the stored value
                ctx.emitter.instruction("cmp r11, 5");                          // kind 5 = boxed Mixed cell
                ctx.emitter.instruction(&format!("je {}", boxed_label));
                abi::emit_call_label(ctx.emitter, "__rt_incref");               // raw object: caller owns it
                if result_reg != "rax" {
                    ctx.emitter.instruction(&format!("mov {}, rax", result_reg));
                }
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
                ctx.emitter.instruction(&format!("jmp {}", done_label));

                ctx.emitter.label(&boxed_label);
                abi::emit_call_label(ctx.emitter, "__rt_incref");               // boxed cell: retain and hand back as-is
                if result_reg != "rax" {
                    ctx.emitter.instruction(&format!("mov {}, rax", result_reg));
                }
                ctx.emitter.instruction(&format!("jmp {}", done_label));

                ctx.emitter.label(&null_label);
                abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
                emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
                ctx.emitter.label(&done_label);
            }
        }
        return Ok(PhpType::Mixed);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, null_label)); // missing previous → null
            // `__rt_incref` expects the object in x0.
            if result_reg != "x0" {
                ctx.emitter
                    .instruction(&format!("mov x0, {}", result_reg)); // move previous into incref arg
            }
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // caller owns the returned previous
            if result_reg != "x0" {
                ctx.emitter
                    .instruction(&format!("mov {}, x0", result_reg)); // restore result register
            }
            if result_is_mixed {
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, &object_ty);
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
            ctx.emitter.instruction(                                            // distinguish a missing previous object before taking ownership
                &format!("test {}, {}", result_reg, result_reg)
            );                                                                  // missing previous → null
            ctx.emitter
                .instruction(&format!("jz {}", null_label));
            if result_reg != "rax" {
                ctx.emitter
                    .instruction(&format!("mov rax, {}", result_reg)); // move previous into incref arg
            }
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // caller owns the returned previous
            if result_reg != "rax" {
                ctx.emitter
                    .instruction(&format!("mov {}, rax", result_reg)); // restore result register
            }
            if result_is_mixed {
                emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
            } else {
                abi::emit_incref_if_refcounted(ctx.emitter, &object_ty);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Mixed getter results consume fresh strings and arrays on every supported target.
    #[test]
    fn throwable_getter_boxing_transfers_fresh_payload_owners_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for method in ["getmessage", "getfile", "__tostring"] {
                let mut emitter = Emitter::new(target);
                emit_box_throwable_method_result(&mut emitter, method, &PhpType::Str);
                let asm = emitter.output();
                assert!(asm.contains("__rt_heap_alloc"), "{name}: {method}");
                assert!(!asm.contains("__rt_mixed_from_value"), "{name}: {method}");
                assert!(!asm.contains("__rt_str_persist"), "{name}: {method}");
            }
            let mut emitter = Emitter::new(target);
            emit_box_throwable_method_result(
                &mut emitter, "gettrace", &PhpType::Array(Box::new(PhpType::Mixed)),
            );
            let asm = emitter.output();
            // A boxed `array<mixed>` takes its Mixed tag from the heap kind (indexed or hash),
            // since the declared `array` is a packed|hash union: the shared runtime routine
            // `__rt_mixed_box_container_3` classifies the container and calls
            // `__rt_mixed_from_value` itself. The invariant is the order: box first, then retire
            // the getter's own owner.
            let boxed = asm.find("__rt_mixed_box_container_3").unwrap();
            // The original owner is retired by heap kind, since the array may be a promoted hash.
            let released = asm.find("__rt_decref_any").unwrap();
            assert!(boxed < released, "{name}");

            let mut emitter = Emitter::new(target);
            emit_box_throwable_method_result(&mut emitter, "gettraceasstring", &PhpType::Str);
            assert!(emitter.output().contains("__rt_mixed_from_value"), "{name}");
        }
    }
}
