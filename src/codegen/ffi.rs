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
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

/// Emit an extern (FFI) function call using the C ABI.
/// The C symbol is `_{name}` (macOS convention).
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
        let actual_ty = if param_ty == PhpType::Callable {
            match &arg.kind {
                ExprKind::StringLiteral(func_name) => {
                    let label = function_symbol(func_name);
                    abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &label); // materialize the callback target address in the integer result register
                    PhpType::Callable
                }
                _ => panic!(
                    "codegen bug: extern callable argument must be a function-name string literal"
                ),
            }
        } else {
            emit_expr(arg, emitter, ctx, data)
        };
        if !matches!(actual_ty, PhpType::Void | PhpType::Never) {
            abi::emit_push_result_value(emitter, &actual_ty);
        }
        source_temp_types.push(actual_ty);
    }
    source_temp_types
}

fn temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty, PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

fn pushed_temp_bytes(types: &[PhpType]) -> usize {
    types.iter().map(temp_slot_size).sum()
}

fn temp_offsets(types: &[PhpType]) -> Vec<usize> {
    let mut offsets = vec![0usize; types.len()];
    let mut running = 0usize;
    for idx in (0..types.len()).rev() {
        offsets[idx] = running;
        running += temp_slot_size(&types[idx]);
    }
    offsets
}

fn source_temp_offset(source_temp_types: &[PhpType], temp_idx: usize, extra_bytes: usize) -> usize {
    extra_bytes + temp_offsets(source_temp_types)[temp_idx]
}

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

fn int_abi_arg_reg(emitter: &Emitter, idx: usize) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"][idx],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx", "r8", "r9"][idx],
    }
}

fn float_abi_arg_reg(emitter: &Emitter, idx: usize) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"][idx],
        Arch::X86_64 => ["xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7"][idx],
    }
}

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

    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
    }

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
