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
        PhpType::Object(class_name) => return lower_object_echo_value(ctx, value, &class_name),
        PhpType::Mixed | PhpType::Union(_) => {
            return conversions::emit_mixed_string_context_stdout(ctx, value);
        }
        _ => {}
    }
    let ty = ctx.load_value_to_result(value)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    let output_ty = if matches!(raw_ty, PhpType::Resource(_)) {
        raw_ty
    } else {
        ty
    };
    emit_loaded_value_to_stdout(ctx, &output_ty)
}

/// Lowers PHP `print` output for a previously computed SSA value.
pub(super) fn lower_print_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_echo_value(ctx, inst)
}

/// Lowers `echo $object` through `__toString()` or PHP's conversion fatal.
pub(super) fn lower_object_echo_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    class_name: &str,
) -> Result<()> {
    let normalized = class_name.trim_start_matches('\\');
    if !object_class_has_tostring(ctx, normalized) {
        emit_missing_tostring_fatal(ctx, normalized);
        return Ok(());
    }
    let return_ty = emit_object_tostring_call(ctx, value, normalized)?;
    emit_loaded_value_to_stdout(ctx, &return_ty.codegen_repr())
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
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&target.impl_class, &target.method_key),
    );
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

/// Emits PHP's fatal diagnostic for object-to-string conversion without `__toString()`.
pub(super) fn emit_missing_tostring_fatal(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let message = format!(
        "Fatal error: Object of class {} could not be converted to string\n",
        class_name
    );
    let (label, len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the object string-cast fatal to stderr
            ctx.emitter.adrp("x1", &label);
            ctx.emitter.add_lo12("x1", "x1", &label);
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the object string-cast fatal byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the object string-cast fatal to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            ctx.emitter.instruction(&format!("mov edx, {}", len));              // pass the object string-cast fatal byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the object string-cast fatal before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
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
                    ctx.emitter.instruction(&format!(
                        "cmp {}, {}",
                        abi::int_result_reg(ctx.emitter),
                        sentinel_reg
                    ));                                                         // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("b.eq {}", skip_label));   // skip integer echo when the value represents null
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!(
                        "cmp {}, {}",
                        abi::int_result_reg(ctx.emitter),
                        sentinel_reg
                    ));                                                         // compare integer value against the runtime null sentinel
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

