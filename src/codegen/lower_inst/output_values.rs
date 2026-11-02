//! Purpose:
//! Lowers echo and print, including object-to-string dispatch.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers PHP echo output for a previously computed SSA value.
pub(super) fn lower_echo_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Object(class_name) => {
            let normalized = class_name.trim_start_matches('\\');
            if interface_has_tostring(ctx, normalized) {
                // See `lower_object_to_string`: an interpreter-built receiver matches no
                // interface table, so the miss has to reach the runtime resolver.
                super::method_intrinsics::lower_interface_method_call_with_miss(
                    ctx,
                    inst,
                    normalized,
                    "__toString",
                    super::iterators::InterfaceDispatchMiss::DynamicToString(value),
                )?;
                return emit_owned_string_result_to_stdout(ctx);
            }
            return lower_object_echo_value(ctx, value, &class_name);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            return conversions::emit_mixed_string_context_stdout(ctx, value);
        }
        _ => {}
    }
    let ty = ctx.load_value_to_result(value)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    // A null `?ptr<T>` (pointer word 0) echoes nothing, like PHP `null`.
    if crate::types::is_nullable_pointer_type(&raw_ty) {
        let skip = ctx.next_label("echo_skip_null_pointer");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &skip);
        emit_loaded_value_to_stdout(ctx, &ty)?;
        ctx.emitter.label(&skip);
        return Ok(());
    }
    let output_ty = match raw_ty {
        PhpType::Resource(_) => raw_ty,
        // A packed record address prints like the `ptr<T>` the checker types it as.
        PhpType::Packed(class_name) => PhpType::Pointer(Some(class_name)),
        _ => ty,
    };
    emit_loaded_value_to_stdout(ctx, &output_ty)
}

/// Returns true when interface metadata exposes a string-returning `__toString()` contract.
fn interface_has_tostring(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    ctx.module
        .interface_infos
        .get(interface_name)
        .and_then(|interface| interface.methods.get("__tostring"))
        .is_some_and(|signature| signature.return_type.codegen_repr() == PhpType::Str)
}

/// Lowers PHP `print` output for a previously computed SSA value.
pub(super) fn lower_print_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_echo_value(ctx, inst)
}

/// Lowers `echo $object` through `__toString()`, statically bound when the class publishes one.
pub(super) fn lower_object_echo_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if !object_class_has_tostring(ctx, normalized) {
        // The object arm of the run-time coercion returns `__toString()`'s owned result.
        emit_value_dynamic_object_to_string(ctx, value)?;
        return emit_owned_string_result_to_stdout(ctx);
    }
    let return_ty = emit_object_tostring_call(ctx, value, normalized)?.codegen_repr();
    if return_ty == PhpType::Str {
        return emit_owned_string_result_to_stdout(ctx);
    }
    emit_loaded_value_to_stdout(ctx, &return_ty)
}

/// Writes the string a `__toString()` call just returned, then releases it.
///
/// The call happens inside this instruction, so no IR value owns its result: `echo $object`
/// leaked one string per echo (Blade and `Stringable` output in a long-running `--web` worker).
pub(super) fn emit_owned_string_result_to_stdout(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_loaded_value_to_stdout(ctx, &PhpType::Str)?;
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    super::ownership::release_loaded_string(ctx);
    Ok(())
}

/// Emits the zero-argument `__toString()` method call for an object value.
pub(super) fn emit_object_tostring_call(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
) -> Result<PhpType> {
    let target = resolve_method_call_target(ctx, class_name, "__toString", 1)?;
    let args = [value];
    let param_types = [PhpType::Object(class_name.to_string())];
    let ref_params = [false];
    let call_args = materialize_direct_call_args_with_refs(ctx, &args, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    emit_direct_resolved_method_call(ctx, &target)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_ref_arg_writebacks(ctx, &call_args)?;
    Ok(target.return_ty)
}

/// Returns true when class metadata exposes a `__toString()` method.
pub(super) fn object_class_has_tostring(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__tostring"))
}

/// Coerces an object to string at RUN TIME, for every case compile-time binding cannot settle.
///
/// This replaces a static fatal that four lowering sites emitted whenever `class_infos` did not
/// show a `__toString` on the exact named class. That verdict was wrong three ways, and none of
/// them is rare:
///
///   - the DECLARED type `object` is spelled `PhpType::Object("")`, so every value typed
///     `object` failed the lookup and the fatal printed an EMPTY class name;
///   - a SUBCLASS publishing `__toString` was refused because its parent, the static type,
///     publishes none;
///   - an object that genuinely has no `__toString` stops PHP with a CATCHABLE `Error` naming
///     the class, not with an uncatchable write-and-exit. Code that catches it — Symfony's DI
///     dumper does — cannot see a raw `exit`.
///
/// `__rt_sprintf_mixed_to_string` already decides all three from the object itself and is
/// emitted unconditionally, so this calls the existing behavior instead of restating it: the
/// method resolves through the dense class-id-indexed `_class_tostring_ptrs` table (which covers
/// every AOT class, inherited entries included), synthetic negative and out-of-range ids fall
/// through to the eval bridge, and anything still unresolved throws PHP's catchable `Error` with
/// the class name read from `_class_name_entries`. The `sprintf` in its name records its first
/// caller, not its scope; the body is the general non-scalar string coercion.
///
/// `receiver_reg` holds the raw object payload. Both arches return the coerced pair directly in
/// `abi::string_result_regs` (x1/x2, rax/rdx), so callers need no shuffling afterwards.
///
/// The eval context is passed as null deliberately: the boxed-Mixed caller is a per-module shared
/// helper with no caller context in scope, and `__elephc_eval_string_context` documents null as
/// supported for exactly this case — an eval-created object is resolved through the context it is
/// still registered with.
pub(super) fn emit_dynamic_object_to_string(ctx: &mut FunctionContext<'_>, receiver_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x1, {}", receiver_reg));      // pass the object payload as the coercion operand
            ctx.emitter.instruction("mov x0, #6");                              // tag 6 selects the helper's object arm
            ctx.emitter.instruction("mov x2, #0");                              // no caller eval context; the bridge uses the object's own
            abi::emit_call_label(ctx.emitter, "__rt_sprintf_mixed_to_string");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rsi, {}", receiver_reg));     // pass the object payload as the coercion operand
            ctx.emitter.instruction("mov edi, 6");                              // tag 6 selects the helper's object arm
            ctx.emitter.instruction("xor edx, edx");                            // no caller eval context; the bridge uses the object's own
            abi::emit_call_label(ctx.emitter, "__rt_sprintf_mixed_to_string");
        }
    }
}

/// Loads a statically typed object operand and coerces it to string at run time.
///
/// The shared entry for the three sites that hold the object as an EIR value rather than in a
/// register already.
pub(super) fn emit_value_dynamic_object_to_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    let receiver_reg = abi::int_result_reg(ctx.emitter);
    emit_dynamic_object_to_string(ctx, receiver_reg);
    Ok(())
}

/// Emits stdout output for the value currently loaded into result register(s).
pub(super) fn emit_loaded_value_to_stdout(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    ctx.emitter.blank();
    ctx.emitter.comment("echo");
    match ty {
        PhpType::Void | PhpType::Never => Ok(()),
        PhpType::Bool => {
            let skip_label = ctx.next_label("echo_skip_false");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::TaggedScalar => {
            let skip_label = ctx.next_label("echo_skip_tagged_null");
            crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, &PhpType::Int);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Int => {
            if crate::codegen::sentinels::null_repr_is_tagged() {
                abi::emit_write_stdout(ctx.emitter, ty);
                return Ok(());
            }
            let skip_label = ctx.next_label("echo_skip_null");
            let sentinel_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_load_int_immediate(
                ctx.emitter,
                sentinel_reg,
                crate::codegen::sentinels::NULL_SENTINEL,
            );
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("b.eq {}", skip_label));   // skip integer echo when the value represents null
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("je {}", skip_label));     // skip integer echo when the value represents null
                }
            }
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Float
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::Resource(_)
        | PhpType::Pointer(_) => {
            abi::emit_write_stdout(ctx.emitter, ty);
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            conversions::emit_array_like_string_result(ctx);
            abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!(
            "echo for PHP type {:?}",
            ty
        ))),
    }
}
