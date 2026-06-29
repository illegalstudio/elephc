//! Purpose:
//! Emits PHP `preg_replace_callback` PCRE-style regex builtin calls.
//! Wires a statically known callback into the regex replacement runtime.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - The callback receives `array<string>` matches so untyped closure params
//!   must be specialized before deferred closure emission.
//! - Descriptor-valued callbacks keep receiver and capture environments in descriptor storage.

use crate::codegen::abi;
use crate::codegen::builtins::arrays::{
    call_user_func_array, callback_env, runtime_callable_array_callback,
};
use crate::codegen::callable_dispatch::{RuntimeCallableCase, RuntimeCallableSelector};
use crate::codegen::context::{Context, DeferredCallbackWrapper};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Emits the `preg_replace_callback` builtin call.
///
/// Evaluates arguments in PHP source order (pattern, callback, subject),
/// materializes the callback address, and calls the `__rt_preg_replace_callback`
/// runtime helper. Returns `PhpType::Str` on success.
///
/// # Arguments
/// * `_name` - Unused, follows dispatcher convention
/// * `args` - `[pattern, callback, subject]`
/// * `emitter` - Target assembly emitter
/// * `ctx` - Codegen context (variables, deferred closures)
/// * `data` - Data section for constants/symbols
///
/// # Returns
/// `Some(PhpType::Str)` on success, `None` if the call was deferred
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("preg_replace_callback()");

    // -- evaluate pattern first, matching PHP source order --
    emit_expr(&args[0], emitter, ctx, data);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, string_ptr_reg, string_len_reg);

    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);

    if let Some(array_callback) =
        callback_env::resolve_callable_array_descriptor_callback(&args[1], ctx, data)
    {
        let receiver_ty = if let Some((receiver, receiver_ty)) = &array_callback.receiver_prefix {
            emit_expr(receiver, emitter, ctx, data);
            abi::emit_push_reg(emitter, result_reg);                            // preserve callable-array receiver across subject evaluation
            Some(receiver_ty.clone())
        } else {
            None
        };

        // -- evaluate subject last --
        emit_expr(&args[2], emitter, ctx, data);

        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x5, x1");                              // pass subject pointer to the regex callback runtime
                emitter.instruction("mov x6, x2");                              // pass subject length to the regex callback runtime
                if receiver_ty.is_some() {
                    abi::emit_pop_reg(emitter, call_reg);                       // recover callable-array receiver for descriptor prefix storage
                }
                abi::emit_pop_reg_pair(emitter, "x7", "x8");

                let wrapper = static_callable_array_preg_callback_env(
                    &array_callback,
                    receiver_ty.as_ref(),
                    call_reg,
                    "x5",
                    emitter,
                    ctx,
                );
                abi::emit_symbol_address(emitter, "x3", &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, "x4");
                emitter.instruction("mov x1, x7");                              // pass pattern pointer to the regex callback runtime
                emitter.instruction("mov x2, x8");                              // pass pattern length to the regex callback runtime
                emitter.instruction("bl __rt_preg_replace_callback");           // run regex replacement through the callable-array descriptor callback → x1=ptr, x2=len
                release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
            }
            Arch::X86_64 => {
                emitter.instruction("mov r8, rax");                             // pass subject pointer to the regex callback runtime
                emitter.instruction("mov r9, rdx");                             // pass subject length to the regex callback runtime
                if receiver_ty.is_some() {
                    abi::emit_pop_reg(emitter, call_reg);                       // recover callable-array receiver for descriptor prefix storage
                }
                abi::emit_pop_reg_pair(emitter, "r13", "r14");

                let wrapper = static_callable_array_preg_callback_env(
                    &array_callback,
                    receiver_ty.as_ref(),
                    call_reg,
                    "r8",
                    emitter,
                    ctx,
                );
                abi::emit_symbol_address(emitter, "rdx", &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, "rcx");
                emitter.instruction("mov rdi, r13");                            // pass pattern pointer to the regex callback runtime
                emitter.instruction("mov rsi, r14");                            // pass pattern length to the regex callback runtime
                abi::emit_call_label(emitter, "__rt_preg_replace_callback");    // run regex replacement through the callable-array descriptor callback → rax=ptr, rdx=len
                release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
            }
        }

        return Some(PhpType::Str);
    }

    if runtime_callable_array_callback::emit_without_saved_array(
        &args[1],
        emitter,
        ctx,
        data,
        |case, receiver_ty, emitter, ctx, data| {
            emit_runtime_callable_array_preg_case(
                case,
                receiver_ty,
                &args[2],
                emitter,
                ctx,
                data,
            );
        },
    ) {
        return Some(PhpType::Str);
    }

    if call_user_func_array::callback_is_runtime_string(&args[1], ctx) {
        emit_runtime_string_preg_callback(&args[1], &args[2], emitter, ctx, data);
        return Some(PhpType::Str);
    }

    if callback_env::expr_call_needs_descriptor_callback_env(&args[1], ctx)
        && callback_env::descriptor_callback_env_supported(&args[1])
    {
        // -- evaluate the selected descriptor before the subject, matching PHP source order --
        let (expected_installed, previous_expected) =
            install_preg_callback_expected_sig(&args[1], ctx);
        emit_expr(&args[1], emitter, ctx, data);
        restore_preg_callback_expected_sig(expected_installed, previous_expected, ctx);
        specialize_recent_inline_callback(&args[1], ctx);
        let retained_borrowed =
            callback_env::retain_borrowed_descriptor_callback_result(&args[1], emitter);
        abi::emit_push_reg(emitter, result_reg);                                // preserve the selected callable descriptor across subject evaluation

        // -- evaluate subject last --
        emit_expr(&args[2], emitter, ctx, data);

        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x5, x1");                              // pass subject pointer to the regex callback runtime
                emitter.instruction("mov x6, x2");                              // pass subject length to the regex callback runtime
                abi::emit_pop_reg(emitter, result_reg);
                abi::emit_pop_reg_pair(emitter, "x7", "x8");

                let wrapper = descriptor_preg_callback_env(
                    &args[1],
                    "x5",
                    retained_borrowed,
                    emitter,
                    ctx,
                );
                abi::emit_symbol_address(emitter, "x3", &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, "x4");
                emitter.instruction("mov x1, x7");                              // pass pattern pointer to the regex callback runtime
                emitter.instruction("mov x2, x8");                              // pass pattern length to the regex callback runtime
                emitter.instruction("bl __rt_preg_replace_callback");           // run regex replacement through the descriptor callback → x1=ptr, x2=len
                release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
            }
            Arch::X86_64 => {
                emitter.instruction("mov r8, rax");                             // pass subject pointer to the regex callback runtime
                emitter.instruction("mov r9, rdx");                             // pass subject length to the regex callback runtime
                abi::emit_pop_reg(emitter, result_reg);
                abi::emit_pop_reg_pair(emitter, "r13", "r14");

                let wrapper = descriptor_preg_callback_env(
                    &args[1],
                    "r8",
                    retained_borrowed,
                    emitter,
                    ctx,
                );
                abi::emit_symbol_address(emitter, "rdx", &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, "rcx");
                emitter.instruction("mov rdi, r13");                            // pass pattern pointer to the regex callback runtime
                emitter.instruction("mov rsi, r14");                            // pass pattern length to the regex callback runtime
                abi::emit_call_label(emitter, "__rt_preg_replace_callback");    // run regex replacement through the descriptor callback → rax=ptr, rdx=len
                release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
            }
        }

        return Some(PhpType::Str);
    }

    // -- evaluate callback second and remember its address --
    let (expected_installed, previous_expected) =
        install_preg_callback_expected_sig(&args[1], ctx);
    let captures = materialize_callback_address(&args[1], call_reg, emitter, ctx, data);
    restore_preg_callback_expected_sig(expected_installed, previous_expected, ctx);
    specialize_recent_inline_callback(&args[1], ctx);
    abi::emit_push_reg(emitter, call_reg);

    // -- evaluate subject last --
    emit_expr(&args[2], emitter, ctx, data);

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- stage runtime arguments away from helper scratch registers --
            abi::emit_push_reg_pair(emitter, "x1", "x2");
            abi::emit_pop_reg_pair(emitter, "x5", "x6");
            abi::emit_pop_reg(emitter, call_reg);
            abi::emit_pop_reg_pair(emitter, "x7", "x8");

            let env_bytes = materialize_capture_env(&captures, call_reg, "x3", "x4", emitter, ctx);
            emitter.instruction("mov x1, x7");                                  // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov x2, x8");                                  // pass pattern length to the regex callback runtime
            emitter.instruction("bl __rt_preg_replace_callback");               // run regex replacement through the callback → x1=ptr, x2=len
            if env_bytes > 0 {
                abi::emit_release_temporary_stack(emitter, env_bytes);
            }
        }
        Arch::X86_64 => {
            // -- stage runtime arguments away from helper scratch registers --
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            abi::emit_pop_reg_pair(emitter, "r8", "r9");
            abi::emit_pop_reg(emitter, call_reg);
            abi::emit_pop_reg_pair(emitter, "r13", "r14");

            let env_bytes =
                materialize_capture_env(&captures, call_reg, "rdx", "rcx", emitter, ctx);
            emitter.instruction("mov rdi, r13");                                // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov rsi, r14");                                // pass pattern length to the regex callback runtime
            abi::emit_call_label(emitter, "__rt_preg_replace_callback");        // run regex replacement through the callback → rax=ptr, rdx=len
            if env_bytes > 0 {
                abi::emit_release_temporary_stack(emitter, env_bytes);
            }
        }
    }

    Some(PhpType::Str)
}

/// Emits one selected runtime callable-array descriptor case for `preg_replace_callback()`.
///
/// Static-method cases enter with only the saved pattern on the temporary stack.
/// Instance-method cases enter with the selected receiver above the saved pattern,
/// and this helper consumes both after evaluating the subject in PHP source order.
fn emit_runtime_callable_array_preg_case(
    case: &RuntimeCallableCase,
    receiver_ty: Option<&PhpType>,
    subject: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let call_reg = abi::nested_call_reg(emitter);

    // -- evaluate subject last --
    emit_expr(subject, emitter, ctx, data);

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x5, x1");                                  // pass subject pointer to the regex callback runtime
            emitter.instruction("mov x6, x2");                                  // pass subject length to the regex callback runtime
            if receiver_ty.is_some() {
                abi::emit_pop_reg(emitter, call_reg);                           // recover selected callable-array receiver for descriptor prefix storage
            }
            abi::emit_pop_reg_pair(emitter, "x7", "x8");

            let wrapper = callable_array_descriptor_preg_callback_env(
                &case.descriptor_label,
                receiver_ty,
                call_reg,
                "x5",
                emitter,
                ctx,
            );
            abi::emit_symbol_address(emitter, "x3", &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, "x4");
            emitter.instruction("mov x1, x7");                                  // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov x2, x8");                                  // pass pattern length to the regex callback runtime
            emitter.instruction("bl __rt_preg_replace_callback");               // run regex replacement through the runtime callable-array descriptor callback → x1=ptr, x2=len
            release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r8, rax");                                 // pass subject pointer to the regex callback runtime
            emitter.instruction("mov r9, rdx");                                 // pass subject length to the regex callback runtime
            if receiver_ty.is_some() {
                abi::emit_pop_reg(emitter, call_reg);                           // recover selected callable-array receiver for descriptor prefix storage
            }
            abi::emit_pop_reg_pair(emitter, "r13", "r14");

            let wrapper = callable_array_descriptor_preg_callback_env(
                &case.descriptor_label,
                receiver_ty,
                call_reg,
                "r8",
                emitter,
                ctx,
            );
            abi::emit_symbol_address(emitter, "rdx", &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, "rcx");
            emitter.instruction("mov rdi, r13");                                // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov rsi, r14");                                // pass pattern length to the regex callback runtime
            abi::emit_call_label(emitter, "__rt_preg_replace_callback");        // run regex replacement through the runtime callable-array descriptor callback → rax=ptr, rdx=len
            release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
        }
    }
}

/// Emits descriptor selection for a runtime string regex callback.
///
/// The callback expression is evaluated before the subject to preserve PHP
/// source order. Descriptor matching happens afterward, so all arguments have
/// already been evaluated before an unknown callback name can abort.
fn emit_runtime_string_preg_callback(
    callback: &Expr,
    subject: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let callback_ty = emit_expr(callback, emitter, ctx, data);
    debug_assert!(matches!(callback_ty.codegen_repr(), PhpType::Str));
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the regex callback string name across subject evaluation

    // -- evaluate subject last --
    emit_expr(subject, emitter, ctx, data);
    let (subject_ptr_reg, subject_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, subject_ptr_reg, subject_len_reg);         // preserve subject while runtime callback string cases are matched

    let cases = crate::codegen::callable_dispatch::runtime_callable_cases(
        ctx,
        data,
        &[],
        Some(&preg_matches_type()),
    );
    let call_reg = abi::nested_call_reg(emitter);
    let done_label = ctx.next_label("preg_runtime_string_done");
    let selector = RuntimeCallableSelector::StringNameStack {
        ptr_offset: 16,
        len_offset: 24,
        call_reg,
    };

    for case in &cases {
        let next_case = ctx.next_label("preg_runtime_string_next");
        crate::codegen::callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            emitter,
            ctx,
            data,
        );
        emit_runtime_string_preg_case(case, emitter, ctx);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&next_case);
    }

    call_user_func_array::emit_dynamic_string_callback_abort(emitter, data);
    emitter.label(&done_label);
}

/// Emits one matched runtime string callback case for `preg_replace_callback()`.
fn emit_runtime_string_preg_case(
    case: &RuntimeCallableCase,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(emitter, "x5", "x6");                       // recover subject pointer and length for the regex callback runtime
            abi::emit_release_temporary_stack(emitter, 16);                     // discard the matched callback string name
            abi::emit_pop_reg_pair(emitter, "x7", "x8");
            let wrapper = callable_array_descriptor_preg_callback_env(
                &case.descriptor_label,
                None,
                abi::int_result_reg(emitter),
                "x5",
                emitter,
                ctx,
            );
            abi::emit_symbol_address(emitter, "x3", &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, "x4");
            emitter.instruction("mov x1, x7");                                  // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov x2, x8");                                  // pass pattern length to the regex callback runtime
            emitter.instruction("bl __rt_preg_replace_callback");               // run regex replacement through the runtime string descriptor callback → x1=ptr, x2=len
            release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(emitter, "r8", "r9");                       // recover subject pointer and length for the regex callback runtime
            abi::emit_release_temporary_stack(emitter, 16);                     // discard the matched callback string name
            abi::emit_pop_reg_pair(emitter, "r13", "r14");
            let wrapper = callable_array_descriptor_preg_callback_env(
                &case.descriptor_label,
                None,
                abi::int_result_reg(emitter),
                "r8",
                emitter,
                ctx,
            );
            abi::emit_symbol_address(emitter, "rdx", &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, "rcx");
            emitter.instruction("mov rdi, r13");                                // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov rsi, r14");                                // pass pattern length to the regex callback runtime
            abi::emit_call_label(emitter, "__rt_preg_replace_callback");        // run regex replacement through the runtime string descriptor callback → rax=ptr, rdx=len
            release_descriptor_preg_callback_env(wrapper.env_bytes, emitter);
        }
    }
}

/// Builds the descriptor-backed wrapper environment for `preg_replace_callback()`.
///
/// The regex runtime passes one visible `array<string>` argument and expects a
/// string result. The dummy register fills the shared callback-env helper's
/// unused array slot; only env slot zero (the descriptor) is read by the wrapper.
fn descriptor_preg_callback_env(
    callback: &Expr,
    dummy_array_reg: &str,
    retained_borrowed: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> callback_env::DescriptorCallbackEnv {
    let wrapper = if retained_borrowed {
        callback_env::emit_descriptor_callback_env_from_retained_result(
            callback,
            dummy_array_reg,
            vec![preg_matches_type()],
            PhpType::Str,
            emitter,
            ctx,
        )
    } else {
        callback_env::emit_descriptor_callback_env_from_result(
            callback,
            dummy_array_reg,
            vec![preg_matches_type()],
            PhpType::Str,
            emitter,
            ctx,
        )
    };
    wrapper.expect("descriptor callback env support checked before emitting preg_replace_callback")
}

/// Builds a descriptor-backed regex callback environment for a tracked callable-array variable.
///
/// Instance-method callable arrays store their receiver as a descriptor prefix so the
/// shared descriptor wrapper can prepend it to the regex matches array before invoking
/// the method descriptor. Static-method callable arrays have no prefix.
fn static_callable_array_preg_callback_env(
    array_callback: &callback_env::CallableArrayDescriptorCallback,
    receiver_ty: Option<&PhpType>,
    receiver_reg: &str,
    dummy_array_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> callback_env::DescriptorCallbackEnv {
    callable_array_descriptor_preg_callback_env(
        &array_callback.descriptor_label,
        receiver_ty,
        receiver_reg,
        dummy_array_reg,
        emitter,
        ctx,
    )
}

/// Builds a descriptor-backed regex callback environment from a descriptor label.
fn callable_array_descriptor_preg_callback_env(
    descriptor_label: &str,
    receiver_ty: Option<&PhpType>,
    receiver_reg: &str,
    dummy_array_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> callback_env::DescriptorCallbackEnv {
    let descriptor_prefix_types = receiver_ty.iter().map(|ty| (*ty).clone()).collect();
    let wrapper = callback_env::emit_descriptor_callback_env_from_static_descriptor(
        descriptor_label,
        vec![preg_matches_type()],
        descriptor_prefix_types,
        PhpType::Str,
        emitter,
        ctx,
    );
    if let Some(ty) = receiver_ty {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), receiver_reg)); // restore callable-array receiver for regex descriptor prefix storage
        callback_env::store_descriptor_callback_prefix_result(&wrapper, 0, ty, emitter);
    }
    callback_env::store_descriptor_callback_array_reg(&wrapper, dummy_array_reg, emitter);
    wrapper
}

/// Releases a descriptor-backed regex callback environment while preserving the string result.
fn release_descriptor_preg_callback_env(env_bytes: usize, emitter: &mut Emitter) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    crate::codegen::callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(emitter, env_bytes);
}

/// Installs the contextual regex-callback signature before emitting inline closures.
///
/// `emit_closure()` reads `expected_first_class_callable_sig` while building the
/// descriptor metadata. Setting it before emission keeps descriptor invokers and
/// deferred closure bodies aligned on the `array<string>` `$matches` parameter.
fn install_preg_callback_expected_sig(
    callback: &Expr,
    ctx: &mut Context,
) -> (bool, Option<FunctionSig>) {
    if !matches!(callback.kind, ExprKind::Closure { .. }) {
        return (false, None);
    }
    let previous = ctx.expected_first_class_callable_sig.replace(FunctionSig {
        params: vec![("matches".to_string(), preg_matches_type())],
        defaults: vec![None],
        return_type: PhpType::Str,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![false],
        variadic: None,
        deprecation: None,
    });
    (true, previous)
}

/// Restores the previous contextual callable signature after callback emission.
fn restore_preg_callback_expected_sig(
    installed: bool,
    previous: Option<FunctionSig>,
    ctx: &mut Context,
) {
    if installed {
        ctx.expected_first_class_callable_sig = previous;
    }
}

/// Returns the PHP type for preg_replace_callback closure parameters.
///
/// `preg_replace_callback` passes `array<string>` (matches) to the callback,
/// so untyped closure params must be specialized to `array<Str>` before emission.
fn preg_matches_type() -> PhpType {
    PhpType::Array(Box::new(PhpType::Str))
}

/// Specializes the most recently deferred inline closure's first parameter type.
///
/// When `callback` is an inline `Closure` expression, this updates the closure's
/// signature so its first parameter is `preg_matches_type()` (`array<Str>`),
/// matching what `preg_replace_callback` passes at runtime.
///
/// No-op for non-closure callbacks or when no deferred closure is pending.
fn specialize_recent_inline_callback(callback: &Expr, ctx: &mut Context) {
    if !matches!(callback.kind, ExprKind::Closure { .. }) {
        return;
    }
    let Some(deferred) = ctx.deferred_closures.last_mut() else {
        return;
    };
    if let Some((_, ty)) = deferred.sig.params.first_mut() {
        *ty = preg_matches_type();
    }
    if let Some(declared) = deferred.sig.declared_params.first_mut() {
        *declared = false;
    }
}

/// Loads the callback address into `call_reg` and returns capture variables.
///
/// Handles three callback forms:
/// - **String literal**: looks up the function name and emits its symbol address
/// - **Variable**: loads the callable value from the stack slot
/// - **Other expression**: emits the expression and moves the result address
///
/// Returns capture metadata `(name, PhpType, by_ref)` from `callable_captures`.
fn materialize_callback_address(
    callback: &Expr,
    call_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<(String, PhpType, bool)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => {
            let resolved_name = match lookup_function(ctx, name) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.clone(),
            };
            abi::emit_symbol_address(emitter, call_reg, &function_symbol(&resolved_name));
            Vec::new()
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);
            if ctx.ref_params.contains(name) {
                abi::emit_load_from_address(emitter, call_reg, call_reg, 0);
            }
            crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                call_reg,
                call_reg,
            );
            crate::codegen::callables::callable_captures(callback, ctx)
        }
        _ => {
            emit_expr(callback, emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", call_reg, abi::int_result_reg(emitter))); // keep the evaluated callback descriptor in the nested-call scratch register
            crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                call_reg,
                call_reg,
            );
            crate::codegen::callables::callable_captures(callback, ctx)
        }
    }
}

/// Emits a capture environment on the temporary stack and registers a wrapper.
///
/// If `captures` is empty, passes the direct callback address with no environment.
/// Otherwise:
/// - Reserves `env_bytes` on the temporary stack (slot 0 = callback, slots 1+ = captures)
/// - Stores each capture variable into the corresponding environment slot
/// - Registers a `DeferredCallbackWrapper` for later wrapper emission
/// - Returns the environment size in bytes
///
/// # Returns
/// Bytes of reserved temporary stack, or 0 if no captures
fn materialize_capture_env(
    captures: &[(String, PhpType, bool)],
    callback_reg: &str,
    runtime_callback_reg: &str,
    runtime_env_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> usize {
    if captures.is_empty() {
        emitter.instruction(&format!("mov {}, {}", runtime_callback_reg, callback_reg)); // pass the direct callback address to the regex runtime
        abi::emit_load_int_immediate(emitter, runtime_env_reg, 0);
        return 0;
    }

    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types: vec![preg_matches_type()],
        target_visible_arg_types: None,
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: None,
    });

    let env_bytes = (captures.len() + 1) * 16;
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_reg_to_env_slot(emitter, callback_reg, 0);
    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store preg_replace_callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "preg_replace_callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            store_current_result_to_env_slot(emitter, &PhpType::Int, (idx + 1) * 16);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            store_current_result_to_env_slot(emitter, capture_ty, (idx + 1) * 16);
        }
    }

    abi::emit_symbol_address(emitter, runtime_callback_reg, &wrapper_label);
    abi::emit_temporary_stack_address(emitter, runtime_env_reg, 0);
    env_bytes
}

/// Stores a raw register value into an environment slot at `offset`.
///
/// Uses `symbol_scratch_reg` to compute the slot address on the temporary stack,
/// then stores `reg` at that address.
fn store_reg_to_env_slot(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_store_to_address(emitter, reg, scratch, 0);
}

/// Stores the current expression result into an environment slot at `offset`.
///
/// Reads the ABI result registers appropriate for `ty` (int, float, or string
/// pointer+length) and stores them into the environment slot. For `Float`,
/// uses `float_result_reg`; for `Str`, uses both pointer and length registers;
/// otherwise uses `int_result_reg`. No-op for `Void`/`Never`.
fn store_current_result_to_env_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), scratch, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, scratch, 0);
            abi::emit_store_to_address(emitter, len_reg, scratch, 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, 0);
        }
    }
}
