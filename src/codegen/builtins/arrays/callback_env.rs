//! Purpose:
//! Builds callback capture environments used by array and dynamic-call builtins.
//! Owns hidden capture materialization and deferred wrapper metadata for emitted callbacks.
//!
//! Called from:
//! - Array callback builtins such as `array_map()`, `array_filter()`, `array_reduce()`, and sort/walk helpers.
//! - Dynamic-call builtins such as `call_user_func()` and `call_user_func_array()`.
//!
//! Key details:
//! - Capture slots must preserve source-call evaluation order and ABI argument layout for wrapper calls.

use crate::codegen::abi;
use crate::codegen::context::{Context, DeferredCallbackWrapper};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Metadata for a deferred callback wrapper emitted after the main function body.
/// Holds the environment layout so the wrapper can reload captures and forward the call.
pub(crate) struct CallbackEnv {
    pub(crate) wrapper_label: String,
    pub(crate) env_bytes: usize,
    pub(crate) array_slot_offset: usize,
}

/// Resolves a callback expression and emits code to load its address into `call_reg`.
///
/// Handles string literals, callable variables, and evaluated callback expressions.
/// Returns the list of captured variables with their types and by-ref flags.
pub(crate) fn materialize_callback_address(
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
            let label = function_symbol(&resolved_name);
            abi::emit_symbol_address(emitter, call_reg, &label);
            Vec::new()
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);           // load the callback descriptor from the callable variable slot
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
            let result_reg = abi::int_result_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));  // keep the evaluated callback descriptor in the nested-call scratch register
            crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                call_reg,
                call_reg,
            );
            crate::codegen::callables::callable_captures(callback, ctx)
        }
    }
}

/// Emits code to push each captured variable as a hidden argument before a deferred wrapper call.
///
/// For by-ref captures, emits the variable's address; for value captures, loads the value from
/// the stack slot and pushes it. Appends corresponding types to `arg_types`.
pub(crate) fn push_captures_as_hidden_args(
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    ctx: &Context,
    arg_types: &mut Vec<PhpType>,
) {
    for (capture_name, capture_ty, by_ref) in captures {
        emitter.comment(&format!("push callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            crate::codegen::expr::calls::args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            crate::codegen::expr::calls::args::push_arg_value(emitter, capture_ty);
            arg_types.push(capture_ty.clone());
        }
    }
}

/// Allocates a temporary stack frame for the callback environment and stores the callback
/// address, array pointer, and all captures into it. Returns the wrapper label and stack layout.
pub(crate) fn emit_captured_callback_env(
    callback_reg: &str,
    array_reg: &str,
    captures: &[(String, PhpType, bool)],
    visible_arg_types: Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> CallbackEnv {
    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
    });

    let env_slots = captures.len() + 2;
    let env_bytes = env_slots * 16;
    let array_slot_offset = (env_slots - 1) * 16;

    emitter.comment("callback capture environment");
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_reg_to_env_slot(emitter, callback_reg, 0);
    store_reg_to_env_slot(emitter, array_reg, array_slot_offset);

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
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

    CallbackEnv {
        wrapper_label,
        env_bytes,
        array_slot_offset,
    }
}

/// Emits assembly for persistent callback env from result.
pub(crate) fn emit_persistent_callback_env_from_result(
    captures: &[(String, PhpType, bool)],
    visible_arg_types: Vec<PhpType>,
    target_visible_arg_types: Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> String {
    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: Some(target_visible_arg_types),
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
    });

    let env_bytes = (captures.len() + 1) * 16;
    emitter.comment("persistent callback capture environment");
    crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
        emitter,
        abi::int_result_reg(emitter),
        abi::int_result_reg(emitter),
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the original callback entry address while allocating its env
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", env_bytes));            // request persistent callback environment storage
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the persistent callback environment
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", env_bytes));            // request persistent callback environment storage
            emitter.instruction("call __rt_heap_alloc");                        // allocate the persistent callback environment
        }
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // keep the env pointer above the saved callback entry address
    store_saved_callback_to_persistent_env(emitter);

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store persistent callback capture ${}", capture_name));
        let slot_offset = (idx + 1) * 16;
        if *by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            store_current_result_to_persistent_env_slot(emitter, &PhpType::Int, slot_offset);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            store_current_result_to_persistent_env_slot(emitter, capture_ty, slot_offset);
            retain_persistent_capture_result(emitter, capture_ty);
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                  // return the persistent env pointer as the current result
    abi::emit_release_temporary_stack(emitter, 16);                            // discard the saved original callback entry address
    wrapper_label
}

/// Loads a value from an environment slot into `reg` by computing the slot address on the
/// temporary stack and performing a type-aware load.
pub(crate) fn load_env_slot_to_reg(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_load_from_address(emitter, reg, scratch, 0);
}

/// Emits the address of the base of the temporary callback environment stack frame into `reg`.
/// Used by the deferred wrapper to locate the environment.
pub(crate) fn load_env_pointer_to_reg(emitter: &mut Emitter, reg: &str) {
    abi::emit_temporary_stack_address(emitter, reg, 0);
}

/// Stores the raw value in `reg` directly into the environment slot at `offset` using a
/// temporary stack address scratch register.
fn store_reg_to_env_slot(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_store_to_address(emitter, reg, scratch, 0);
}

/// Stores the current ABI result register(s) into the environment slot at `offset` using a
/// temporary stack address scratch register. Handles float, string (ptr+len), and integer
/// representations per `ty.codegen_repr()`. No-op for `Void`/`Never` types.
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

/// Stores saved callback to persistent env into runtime storage or stack state.
fn store_saved_callback_to_persistent_env(emitter: &mut Emitter) {
    let env_reg = abi::symbol_scratch_reg(emitter);
    let callback_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, env_reg, 0);
    abi::emit_load_temporary_stack_slot(emitter, callback_reg, 16);
    abi::emit_store_to_address(emitter, callback_reg, env_reg, 0);
}

/// Stores current result to persistent env slot into runtime storage or stack state.
fn store_current_result_to_persistent_env_slot(
    emitter: &mut Emitter,
    ty: &PhpType,
    offset: usize,
) {
    let env_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, env_reg, 0);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), env_reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, env_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, env_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), env_reg, offset);
        }
    }
}

/// Retains persistent capture result so ownership remains valid across runtime calls.
fn retain_persistent_capture_result(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, _) = abi::string_result_regs(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov x0, {}", ptr_reg));       // pass the captured string pointer to the retain helper
                    emitter.instruction("bl __rt_incref");                      // retain the captured string for the persistent callback env
                }
                Arch::X86_64 => {
                    if ptr_reg != "rax" {
                        emitter.instruction(&format!("mov rax, {}", ptr_reg));  // pass the captured string pointer to the retain helper
                    }
                    emitter.instruction("call __rt_incref");                    // retain the captured string for the persistent callback env
                }
            }
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(emitter, &other);
        }
        _ => {}
    }
}
