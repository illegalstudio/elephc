//! Purpose:
//! Lowers extern declarations and calls into target ABI-compatible assembly boundaries.
//! Handles C-facing symbols, argument movement, return values, and required library metadata.
//!
//! Called from:
//! - `crate::codegen::generate()` and extern call expression lowering
//!
//! Key details:
//! - Extern lowering follows platform ABI rules and must not use PHP call normalization for C-only details.

use crate::codegen::abi;
use crate::codegen::builtins::callable_lookup::{lookup_function, FunctionLookup};
use crate::codegen::context::{
    Context, DeferredExternCallbackTrampoline, HeapOwnership,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{can_coerce_result_to_type, coerce_result_to_type, emit_expr};
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

/// Lowers an extern (FFI) call into target C ABI.
/// Handles argument preevaluation, cleanup slot reservation, string conversion,
/// register allocation, the foreign call, and borrowed-string cleanup after the call.
/// Returns the PHP return type derived from the extern function's signature.
pub fn emit_extern_call(
    name: &str,
    args: &[Expr],
    call_span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let sig = ctx
        .extern_functions
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("codegen bug: extern function '{}' not found", name));
    let call_sig = ctx
        .functions
        .get(name)
        .cloned()
        .unwrap_or_else(|| FunctionSig {
            params: sig.params.clone(),
            defaults: vec![None; sig.params.len()],
            return_type: sig.return_type.clone(),
            declared_return: true,
            ref_params: vec![false; sig.params.len()],
            declared_params: vec![true; sig.params.len()],
            variadic: None,
            deprecation: None,
        });
    let regular_param_count =
        crate::codegen::expr::calls::args::regular_param_count(Some(&call_sig), args.len());
    let normalized = if crate::codegen::expr::calls::args::has_named_args(args) {
        crate::codegen::expr::calls::args::preevaluate_named_call_args_to_temps(
            &call_sig,
            args,
            call_span,
            regular_param_count,
            false,
            emitter,
            ctx,
            data,
        )
    } else {
        crate::codegen::expr::calls::args::normalize_named_call_args_with_checks(
            &call_sig,
            args,
            regular_param_count,
        )
    };
    crate::codegen::expr::calls::args::emit_spread_length_checks(
        &normalized.spread_length_checks,
        emitter,
        ctx,
        data,
    );
    let normalized_args = normalized.args;
    let args = normalized_args.as_slice();

    emitter.comment(&format!("extern call: {}()", name));

    let string_arg_count = sig
        .params
        .iter()
        .take(args.len())
        .filter(|(_, ty)| *ty == PhpType::Str)
        .count();
    let cleanup_bytes = string_arg_count * 16;

    let source_temp_types = preevaluate_extern_args(args, &sig, emitter, ctx, data);
    let source_temp_bytes = pushed_temp_bytes(&source_temp_types);

    if cleanup_bytes > 0 {
        abi::emit_reserve_temporary_stack(emitter, cleanup_bytes);              // reserve per-call cleanup slots for borrowed C-string temporaries
    }

    // -- push already-evaluated arguments onto the C ABI stack in reverse order --
    let mut final_pushed_bytes = 0usize;
    for (i, _) in args.iter().enumerate().rev() {
        let param_ty = sig
            .params
            .get(i)
            .map(|(_, t)| t.clone())
            .unwrap_or(PhpType::Int);
        let actual_ty = load_extern_source_temp_to_result(
            i,
            &source_temp_types,
            cleanup_bytes + final_pushed_bytes,
            emitter,
        );

        if param_ty == PhpType::Float && actual_ty != PhpType::Float {
            emit_widen_int_like_to_float(emitter);                              // widen integer-like value to C double in the native return register
        } else if matches!(param_ty, PhpType::Pointer(_)) && actual_ty == PhpType::Void {
            emit_zero_int_result(emitter);                                      // PHP null becomes a null pointer for C
        }

        // Convert elephc string (x1, x2) to a dedicated null-terminated C string (x0)
        if param_ty == PhpType::Str && actual_ty == PhpType::Str {
            abi::emit_call_label(emitter, "__rt_str_to_cstr");                  // allocate a null-terminated copy for the foreign C ABI
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // push the returned C string pointer onto the temporary arg stack
        } else if param_ty == PhpType::Float {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // push the floating-point argument onto the temporary arg stack
        } else {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // push the integer or pointer argument onto the temporary arg stack
        }
        final_pushed_bytes += 16;
    }

    // -- pop arguments into registers (C ABI: x0-x7, d0-d7) --
    let mut int_reg = 0usize;
    let mut float_reg = 0usize;
    let mut cleanup_idx = 0usize;
    let temp_arg_bytes = args.len() * 16;
    let cleanup_base_reg = abi::temp_int_reg(emitter.target);
    if cleanup_bytes > 0 {
        abi::emit_temporary_stack_address(emitter, cleanup_base_reg, temp_arg_bytes); // compute the base address of the borrowed C-string cleanup slots above the temporary arg stack
    }
    for (i, _) in args.iter().enumerate() {
        let param_ty = sig
            .params
            .get(i)
            .map(|(_, t)| t.clone())
            .unwrap_or(PhpType::Int);
        if param_ty == PhpType::Float {
            abi::emit_pop_float_reg(emitter, float_abi_arg_reg(emitter, float_reg)); // pop the floating-point argument into the next ABI float register
            float_reg += 1;
        } else {
            // String args were already converted to char* (single x register)
            let arg_reg = int_abi_arg_reg(emitter, int_reg);
            abi::emit_pop_reg(emitter, arg_reg);                                // pop the integer, pointer, or converted C-string argument into the next ABI int register
            if param_ty == PhpType::Str {
                abi::emit_store_to_address(emitter, arg_reg, cleanup_base_reg, cleanup_idx * 16); // record the borrowed C-string pointer so it can be freed after the foreign call
                cleanup_idx += 1;
            }
            int_reg += 1;
        }
    }

    // -- call the C function --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    let c_sym = emitter.target.extern_symbol(name);
    abi::emit_call_label(emitter, &c_sym);                                      // call the extern C function symbol through the target-aware direct-call helper
    if sig.return_type == PhpType::Int {
        emit_sign_extend_i32_result(emitter);                                   // sign-extend 32-bit C int returns before PHP comparisons use the native integer result register
    }
    let nested_return_ty = if sig.return_type == PhpType::Str {
        PhpType::Pointer(None)
    } else {
        sig.return_type.clone()
    };
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &nested_return_ty);

    // -- handle return value --
    if sig.return_type == PhpType::Str {
        // C returned char* in x0 — convert to owned elephc string (x1, x2)
        abi::emit_call_label(emitter, "__rt_cstr_to_str");                      // convert the returned C string into the elephc string result convention
    }

    if cleanup_bytes > 0 {
        // -- preserve the extern return value while borrowed C-string temps are released --
        let saved_return_bytes = push_ffi_return_value(emitter, &sig.return_type);

        // -- borrowed C-string arguments are call-scoped and freed immediately after the call --
        for idx in 0..string_arg_count {
            abi::emit_load_temporary_stack_slot(
                emitter,
                abi::int_result_reg(emitter),
                saved_return_bytes + idx * 16,
            );                                                                  // reload one borrowed temporary C-string pointer from the cleanup area
            abi::emit_call_label(emitter, "__rt_heap_free");                    // release the call-scoped C-string copy after the extern call returns
        }

        pop_ffi_return_value(emitter, &sig.return_type);
        abi::emit_release_temporary_stack(emitter, cleanup_bytes);              // release the borrowed C-string cleanup area after all temporaries are freed
    }
    abi::emit_release_temporary_stack(emitter, source_temp_bytes);              // release source-order extern argument temporaries after the call

    sig.return_type
}

/// Evaluates all extern call arguments in source order before the C ABI stack is set up.
/// Emits each argument expression, coerces it to the target parameter type, and pushes the result
/// onto a temporary stack. Returns a vector of the emitted PHP types for each argument (after coercion).
/// Callable arguments are resolved to symbol addresses at emit time.
fn preevaluate_extern_args(
    args: &[Expr],
    sig: &crate::types::ExternFunctionSig,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let mut source_temp_types = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let param_ty = sig
            .params
            .get(i)
            .map(|(_, t)| t.clone())
            .unwrap_or(PhpType::Int);
        let mut actual_ty = if param_ty == PhpType::Callable {
            emit_extern_callable_arg(arg, emitter, ctx, data)
        } else {
            emit_expr(arg, emitter, ctx, data)
        };
        if can_coerce_result_to_type(&actual_ty, &param_ty) {
            if should_release_owned_mixed_after_extern_arg_coerce(arg, &actual_ty, &param_ty) {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the owned Mixed argument while coercing it to the extern parameter type
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &param_ty);
                crate::codegen::expr::calls::args::release_preserved_mixed_after_arg_coercion(
                    emitter,
                    &param_ty,
                );
            } else {
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &param_ty);
            }
            actual_ty = param_ty.codegen_repr();
        }
        if !matches!(actual_ty, PhpType::Void | PhpType::Never) {
            abi::emit_push_result_value(emitter, &actual_ty);
        }
        source_temp_types.push(actual_ty);
    }
    source_temp_types
}

/// Materializes an extern `callable` argument as a raw C function pointer.
///
/// String literals still lower to direct user-function symbols. Descriptor-backed
/// callables lower to a generated C-ABI trampoline that reloads the selected
/// descriptor from global storage, preserving closure captures and receivers for
/// C APIs that only accept a plain function pointer.
fn emit_extern_callable_arg(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match &arg.kind {
        ExprKind::StringLiteral(func_name) => {
            let resolved_name = match lookup_function(ctx, func_name) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => func_name.clone(),
            };
            let label = function_symbol(&resolved_name);
            abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &label); // materialize the callback target address in the integer result register
        }
        _ => {
            let precomputed_sig = crate::codegen::callables::callable_sig(arg, ctx);
            let actual_ty = emit_expr(arg, emitter, ctx, data);
            debug_assert_eq!(actual_ty, PhpType::Callable);
            let Some(callback_sig) = precomputed_sig.or_else(|| inline_callable_sig_after_emit(arg, ctx)) else {
                crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                    emitter,
                    abi::int_result_reg(emitter),
                    abi::int_result_reg(emitter),
                );
                return PhpType::Callable;
            };
            emit_stateful_extern_callback_trampoline(arg, &callback_sig, emitter, ctx, data);
        }
    }
    PhpType::Callable
}

/// Returns the signature for an inline callable after its descriptor was emitted.
fn inline_callable_sig_after_emit(arg: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &arg.kind {
        ExprKind::Closure { .. } => ctx.deferred_closures.last().map(|closure| closure.sig.clone()),
        ExprKind::Assignment { value, .. } => inline_callable_sig_after_emit(value, ctx),
        _ => None,
    }
}

/// Stores the current descriptor in a global slot and returns a trampoline address.
fn emit_stateful_extern_callback_trampoline(
    arg: &Expr,
    callback_sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let slot_label = data.add_comm(ctx.next_label("extern_callback_descriptor"), 8);
    let trampoline_label = ctx.next_label("extern_callback_trampoline");
    ctx.deferred_extern_callback_trampolines
        .push(DeferredExternCallbackTrampoline {
            label: trampoline_label.clone(),
            descriptor_slot_label: slot_label.clone(),
            visible_arg_types: callback_sig
                .params
                .iter()
                .map(|(_, ty)| ty.codegen_repr())
                .collect(),
            return_type: callback_sig.return_type.codegen_repr(),
        });

    emitter.comment("extern callback: bind descriptor trampoline");
    if expr_result_needs_retain_for_extern_callback_slot(arg) {
        crate::codegen::callable_descriptor::emit_retain_current_descriptor(emitter);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the new extern callback descriptor while replacing the slot owner
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &slot_label, 0);
    crate::codegen::callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the descriptor that will back the C callback trampoline
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &slot_label, 0);
    abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &trampoline_label);
}

/// Returns whether the current descriptor result must be retained for global storage.
fn expr_result_needs_retain_for_extern_callback_slot(arg: &Expr) -> bool {
    !matches!(
        crate::codegen::expr::expr_result_heap_ownership(arg),
        HeapOwnership::Owned
    )
}

/// Determines whether an owned `Mixed` or `Union` source value must be preserved on the stack
/// while coercing it to a non-Mixed extern parameter type, to avoid leaking the heap value.
/// Returns true when the argument is owned and not itself being coerced to a reference type.
fn should_release_owned_mixed_after_extern_arg_coerce(
    arg: &Expr,
    source_ty: &PhpType,
    target_ty: &PhpType,
) -> bool {
    let source_repr = source_ty.codegen_repr();
    let target_repr = target_ty.codegen_repr();
    matches!(source_repr, PhpType::Mixed | PhpType::Union(_))
        && !matches!(target_repr, PhpType::Mixed | PhpType::Union(_))
        && (crate::codegen::expr::expr_result_heap_ownership(arg) == HeapOwnership::Owned
            || matches!(
                arg.kind,
                ExprKind::BinaryOp {
                    op: BinOp::Add | BinOp::Sub | BinOp::Mul,
                    ..
                }
            ))
}

/// Returns the stack slot size for a PHP type used as an extern argument or return value.
/// `Void` and `Never` types occupy 0 bytes; all other types occupy 16 bytes (one slot).
fn temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty, PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

/// Computes the total bytes occupied by all extern argument temporaries on the stack,
/// using `temp_slot_size` for each type.
fn pushed_temp_bytes(types: &[PhpType]) -> usize {
    types.iter().map(temp_slot_size).sum()
}

/// Computes the byte offset of each extern argument temporary from the top of the stack,
/// iterating in reverse order so later arguments have higher offsets (matching the C call convention).
fn temp_offsets(types: &[PhpType]) -> Vec<usize> {
    let mut offsets = vec![0usize; types.len()];
    let mut running = 0usize;
    for idx in (0..types.len()).rev() {
        offsets[idx] = running;
        running += temp_slot_size(&types[idx]);
    }
    offsets
}

/// Computes the absolute stack byte offset for a given extern argument temporary index,
/// adding `extra_bytes` to account for cleanup slots reserved below the argument area.
fn source_temp_offset(source_temp_types: &[PhpType], temp_idx: usize, extra_bytes: usize) -> usize {
    extra_bytes + temp_offsets(source_temp_types)[temp_idx]
}

/// Loads an extern argument temporary from the stack into the appropriate result register(s)
/// based on its type: float to `d0`, string to `(x1, x2)`, scalar/pointer to `x0`.
/// `extra_bytes` accounts for any cleanup slots positioned below the argument area on the stack.
/// Returns the PHP type of the loaded value.
fn load_extern_source_temp_to_result(
    temp_idx: usize,
    source_temp_types: &[PhpType],
    extra_bytes: usize,
    emitter: &mut Emitter,
) -> PhpType {
    let ty = source_temp_types[temp_idx].clone();
    let offset = source_temp_offset(source_temp_types, temp_idx, extra_bytes);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        }
    }
    ty
}

/// Returns the name of the Nth integer/pointer argument register for the current target's C ABI.
/// ARM64: x0–x7; x86_64: rdi, rsi, rdx, rcx, r8, r9.
fn int_abi_arg_reg(emitter: &Emitter, idx: usize) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"][idx],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx", "r8", "r9"][idx],
    }
}

/// Returns the name of the Nth floating-point argument register for the current target's C ABI.
/// ARM64: d0–d7; x86_64: xmm0–xmm7.
fn float_abi_arg_reg(emitter: &Emitter, idx: usize) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"][idx],
        Arch::X86_64 => ["xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7"][idx],
    }
}

/// Widens an integer-like result (in the integer result register) to a C double in the
/// floating-point result register. Used when a PHP integer is passed to a C float parameter.
fn emit_widen_int_like_to_float(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("scvtf d0, x0");                                // widen the integer-like result in x0 into the floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction("cvtsi2sd xmm0, rax");                          // widen the integer-like result in rax into the floating-point result register
        }
    }
}

/// Emits a zero literal into the integer result register to represent a null pointer
/// when a PHP `Void` value (null) is passed to a C `Pointer` parameter.
fn emit_zero_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // materialize a null C pointer in the integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 0");                                  // materialize a null C pointer in the integer result register
        }
    }
}

/// Sign-extends a 32-bit C integer return value into the native integer register.
/// Required so PHP comparisons use the full 64-bit result after a C `int` return.
fn emit_sign_extend_i32_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sxtw x0, w0");                                 // sign-extend the 32-bit C integer return into the 64-bit result register
        }
        Arch::X86_64 => {
            emitter.instruction("movsxd rax, eax");                             // sign-extend the 32-bit C integer return into the 64-bit result register
        }
    }
}

/// Pushes the current FFI return value (in result registers) onto the temporary stack to
/// preserve it while borrowed C-string temporaries are cleaned up. Returns the number of bytes pushed (0 for void).
fn push_ffi_return_value(emitter: &mut Emitter, ty: &PhpType) -> usize {
    match ty {
        PhpType::Void => 0,
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));   // preserve the floating-point return value while borrowed C-string temporaries are freed
            16
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                  // preserve the string return register pair while borrowed C-string temporaries are freed
            16
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));           // preserve the scalar or pointer return value while borrowed C-string temporaries are freed
            16
        }
    }
}

/// Pops a previously preserved FFI return value off the temporary stack back into the
/// result registers. No-op for void return type.
fn pop_ffi_return_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Void => {}
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));    // restore the floating-point return value after borrowed C-string cleanup
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                   // restore the string return register pair after borrowed C-string cleanup
        }
        _ => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));            // restore the scalar or pointer return value after borrowed C-string cleanup
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::context::Context;
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::parser::ast::Expr;
    use crate::types::ExternFunctionSig;

    /// Builds a Linux x86_64 emitter for FFI unit tests.
    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
    }

    /// Verifies Linux x86_64 extern calls use native call lowering and sign-extend
    /// 32-bit integer returns.
    #[test]
    fn test_emit_extern_call_linux_x86_64_uses_native_call_and_sign_extend() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        let mut data = DataSection::new();
        ctx.extern_functions.insert(
            "abs".into(),
            ExternFunctionSig {
                name: "abs".into(),
                params: vec![("n".into(), PhpType::Int)],
                return_type: PhpType::Int,
                library: None,
            },
        );

        let ret_ty = emit_extern_call(
            "abs",
            &[Expr::int_lit(-42)],
            Span::dummy(),
            &mut emitter,
            &mut ctx,
            &mut data,
        );
        let out = emitter.output();

        assert_eq!(ret_ty, PhpType::Int);
        assert!(out.contains("    mov rax, -42\n"));
        assert!(out.contains("    sub rsp, 16\n"));
        assert!(out.contains("    mov QWORD PTR [rsp], rax\n"));
        assert!(out.contains("    mov rdi, QWORD PTR [rsp]\n"));
        assert!(out.contains("    call abs\n"));
        assert!(out.contains("    movsxd rax, eax\n"));
    }

    /// Verifies Linux x86_64 extern calls with string arguments reserve cleanup
    /// stack space for borrowed C string temporaries.
    #[test]
    fn test_emit_extern_call_linux_x86_64_string_args_use_cleanup_stack() {
        let mut emitter = test_emitter_x86();
        let mut ctx = Context::new();
        let mut data = DataSection::new();
        ctx.extern_functions.insert(
            "strlen".into(),
            ExternFunctionSig {
                name: "strlen".into(),
                params: vec![("s".into(), PhpType::Str)],
                return_type: PhpType::Int,
                library: None,
            },
        );

        let ret_ty = emit_extern_call(
            "strlen",
            &[Expr::string_lit("hello")],
            Span::dummy(),
            &mut emitter,
            &mut ctx,
            &mut data,
        );
        let out = emitter.output();

        assert_eq!(ret_ty, PhpType::Int);
        assert!(out.contains("    sub rsp, 16\n"));
        assert!(out.contains("    call __rt_str_to_cstr\n"));
        assert!(out.contains("    lea r10, [rsp + 16]\n"));
        assert!(out.contains("    mov QWORD PTR [r10], rdi\n"));
        assert!(out.contains("    call strlen\n"));
        assert!(out.contains("    movsxd rax, eax\n"));
        assert!(out.contains("    mov QWORD PTR [rsp], rax\n"));
        assert!(out.contains("    mov rax, QWORD PTR [rsp + 16]\n"));
        assert!(out.contains("    call __rt_heap_free\n"));
        assert!(out.contains("    add rsp, 16\n"));
    }
}
