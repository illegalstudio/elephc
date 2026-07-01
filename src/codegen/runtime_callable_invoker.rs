//! Purpose:
//! Emits descriptor-based runtime callable invokers shared by dynamic callback dispatch.
//! Adapts a uniform `(descriptor, argument array) -> mixed` ABI to typed AOT entries.
//!
//! Called from:
//! - `crate::codegen::driver_support::emit_deferred_closures()`
//! - `crate::codegen::functions` deferred-emission loops.
//!
//! Key details:
//! - Invokers load the native entry from the descriptor at runtime, then reuse
//!   `call_user_func_array` argument materialization for defaults, by-ref flags,
//!   named arguments, variadics, and return boxing.

use crate::codegen::builtins::arrays::call_user_func_array::{
    self, LoadedArraySource,
};
use crate::codegen::callable_descriptor;
use crate::codegen::context::{
    Context, DeferredRuntimeCallableInvoker, TRY_HANDLER_DIAG_DEPTH_OFFSET,
    TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::abi;

const INVOKER_DESCRIPTOR_OFFSET: usize = 8;
const INVOKER_CONCAT_OFFSET: usize = 16;
const INVOKER_ARG_ARRAY_OFFSET: usize = 24;
const INVOKER_BASE_FRAME_SIZE: usize = 32;
const INVOKER_BOUNDARY_FRAME_SIZE: usize =
    INVOKER_BASE_FRAME_SIZE + TRY_HANDLER_SLOT_SIZE + 16;
const INVOKER_BOUNDARY_BASE_OFFSET: usize = INVOKER_BOUNDARY_FRAME_SIZE - 16;

/// Emits a descriptor invoker wrapper for a runtime-callable signature.
pub(crate) fn emit_runtime_callable_invoker(
    emitter: &mut Emitter,
    data: &mut DataSection,
    parent_ctx: &Context,
    invoker: &DeferredRuntimeCallableInvoker,
) {
    emit_runtime_callable_invoker_impl(emitter, data, parent_ctx, invoker, false);
}

/// Emits a descriptor invoker wrapper that catches native throws for eval callbacks.
pub(crate) fn emit_runtime_callable_invoker_with_exception_boundary(
    emitter: &mut Emitter,
    data: &mut DataSection,
    parent_ctx: &Context,
    invoker: &DeferredRuntimeCallableInvoker,
) {
    emit_runtime_callable_invoker_impl(emitter, data, parent_ctx, invoker, true);
}

/// Emits a descriptor invoker wrapper, optionally bounded by an exception handler.
fn emit_runtime_callable_invoker_impl(
    emitter: &mut Emitter,
    data: &mut DataSection,
    parent_ctx: &Context,
    invoker: &DeferredRuntimeCallableInvoker,
    catch_native_throws: bool,
) {
    let mut wrapper_ctx = invoker_context(parent_ctx);
    wrapper_ctx.runtime_capture_descriptor_offset = Some(INVOKER_DESCRIPTOR_OFFSET);
    wrapper_ctx.nested_concat_offset_offset = Some(INVOKER_CONCAT_OFFSET);
    let frame_size = if catch_native_throws {
        INVOKER_BOUNDARY_FRAME_SIZE
    } else {
        INVOKER_BASE_FRAME_SIZE
    };
    let call_reg = abi::nested_call_reg(emitter);
    let escape_label = format!("{}_eval_escape", invoker.label);

    emitter.blank();
    emitter.comment(&format!("runtime callable invoker {}", invoker.label));
    emitter.raw(".align 2");
    emitter.label_global(&invoker.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        INVOKER_DESCRIPTOR_OFFSET,
    );
    if catch_native_throws {
        abi::store_at_offset(
            emitter,
            abi::int_arg_reg_name(emitter.target, 1),
            INVOKER_ARG_ARRAY_OFFSET,
        );
        emit_invoker_exception_boundary_push(
            emitter,
            INVOKER_BOUNDARY_BASE_OFFSET,
            &escape_label,
        );
        abi::load_at_offset(
            emitter,
            abi::int_arg_reg_name(emitter.target, 1),
            INVOKER_ARG_ARRAY_OFFSET,
        );
        emit_saved_descriptor_entry_to_call_reg(emitter, call_reg);
    } else {
        emit_descriptor_entry_to_call_reg(emitter, call_reg);
    }

    let ret_ty = call_user_func_array::emit_loaded_array_callback_call(
        LoadedArraySource::ArgumentRegister(1),
        &PhpType::Mixed,
        None,
        call_reg,
        &invoker.captures,
        &invoker.sig,
        false,
        emitter,
        &mut wrapper_ctx,
        data,
    );
    crate::codegen::emit_box_current_value_as_mixed(emitter, &ret_ty.codegen_repr());
    if catch_native_throws {
        emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
    }
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
    if catch_native_throws {
        emitter.label(&escape_label);
        emit_invoker_exception_boundary_pop(emitter, INVOKER_BOUNDARY_BASE_OFFSET);
        emit_null_invoker_result(emitter);
        abi::emit_frame_restore(emitter, frame_size);
        abi::emit_return(emitter);
    }
}

/// Builds a small codegen context for invoker wrapper bodies.
fn invoker_context(parent_ctx: &Context) -> Context {
    let mut ctx = Context::new();
    ctx.functions = parent_ctx.functions.clone();
    ctx.function_variant_groups = parent_ctx.function_variant_groups.clone();
    ctx.constants = parent_ctx.constants.clone();
    ctx.all_global_var_names = parent_ctx.all_global_var_names.clone();
    ctx.all_static_vars = parent_ctx.all_static_vars.clone();
    ctx.runtime_callable_vars = parent_ctx.runtime_callable_vars.clone();
    ctx.callable_param_sigs = parent_ctx.callable_param_sigs.clone();
    ctx.callable_return_sigs = parent_ctx.callable_return_sigs.clone();
    ctx.callable_array_return_sigs = parent_ctx.callable_array_return_sigs.clone();
    ctx.interfaces = parent_ctx.interfaces.clone();
    ctx.traits = parent_ctx.traits.clone();
    ctx.classes = parent_ctx.classes.clone();
    ctx.enums = parent_ctx.enums.clone();
    ctx.packed_classes = parent_ctx.packed_classes.clone();
    ctx.extern_functions = parent_ctx.extern_functions.clone();
    ctx.extern_classes = parent_ctx.extern_classes.clone();
    ctx.extern_globals = parent_ctx.extern_globals.clone();
    ctx.runtime_callable_extern_wrappers = parent_ctx.runtime_callable_extern_wrappers.clone();
    ctx.runtime_callable_instance_method_wrappers =
        parent_ctx.runtime_callable_instance_method_wrappers.clone();
    ctx
}

/// Loads the descriptor entry slot from the first invoker argument into `call_reg`.
fn emit_descriptor_entry_to_call_reg(emitter: &mut Emitter, call_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, x0", call_reg));              // keep the descriptor pointer while loading the target entry
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, rdi", call_reg));             // keep the descriptor pointer while loading the target entry
        }
    }
    callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
}

/// Loads the saved descriptor entry slot into `call_reg` after a `setjmp` boundary.
fn emit_saved_descriptor_entry_to_call_reg(emitter: &mut Emitter, call_reg: &str) {
    abi::load_at_offset(emitter, call_reg, INVOKER_DESCRIPTOR_OFFSET);
    callable_descriptor::emit_load_entry_from_descriptor(emitter, call_reg, call_reg);
}

/// Pushes a native exception boundary around an eval-owned descriptor invoker call.
fn emit_invoker_exception_boundary_push(
    emitter: &mut Emitter,
    handler_base: usize,
    escape_label: &str,
) {
    emitter.comment("push eval callable exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!("stur x10, [x29, #-{}]", handler_base)); // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!("stur x10, [x29, #-{}]", handler_base - 8)); // preserve the caller activation frame across callable unwinding
            abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "stur x10, [x29, #-{}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                  // save diagnostic suppression depth for restoration
            emitter.instruction(&format!("sub x10, x29, #{}", handler_base));   // compute the boundary handler record address
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "sub x0, x29, #{}",
                handler_base - TRY_HANDLER_JMP_BUF_OFFSET
            ));                                                                  // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp");                                              // snapshot the bridge stack before entering the callable
            emitter.instruction(&format!("cbnz x0, {}", escape_label));         // non-zero setjmp result means a callable Throwable escaped
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base)); // save the previous native exception-handler head
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)); // preserve the caller activation frame across callable unwinding
            abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [rbp - {}], r10",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                  // save diagnostic suppression depth for restoration
            emitter.instruction(&format!("lea r10, [rbp - {}]", handler_base)); // compute the boundary handler record address
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "lea rdi, [rbp - {}]",
                handler_base - TRY_HANDLER_JMP_BUF_OFFSET
            ));                                                                  // pass the boundary jmp_buf to setjmp
            emitter.bl_c("setjmp");                                              // snapshot the bridge stack before entering the callable
            emitter.instruction("test eax, eax");                               // did control arrive through longjmp?
            emitter.instruction(&format!("jne {}", escape_label));              // non-zero setjmp result means a callable Throwable escaped
        }
    }
}

/// Pops the native exception boundary around an eval-owned descriptor invoker call.
fn emit_invoker_exception_boundary_pop(emitter: &mut Emitter, handler_base: usize) {
    emitter.comment("pop eval callable exception boundary");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldur x10, [x29, #-{}]", handler_base)); // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "ldur x10, [x29, #-{}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                  // reload the saved diagnostic suppression depth
            abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", handler_base)); // reload the previous native exception-handler head
            abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rbp - {}]",
                handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
            ));                                                                  // reload the saved diagnostic suppression depth
            abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
        }
    }
}

/// Leaves a null boxed-Mixed result for Rust to translate into a pending throwable.
fn emit_null_invoker_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, xzr");                                 // return null so magician takes the pending Throwable
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // return null so magician takes the pending Throwable
        }
    }
}
