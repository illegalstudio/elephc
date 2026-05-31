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
use crate::codegen::context::{Context, DeferredRuntimeCallableInvoker};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::abi;

const INVOKER_DESCRIPTOR_OFFSET: usize = 8;
const INVOKER_CONCAT_OFFSET: usize = 16;

/// Emits a descriptor invoker wrapper for a runtime-callable signature.
pub(crate) fn emit_runtime_callable_invoker(
    emitter: &mut Emitter,
    data: &mut DataSection,
    parent_ctx: &Context,
    invoker: &DeferredRuntimeCallableInvoker,
) {
    let mut wrapper_ctx = invoker_context(parent_ctx);
    wrapper_ctx.runtime_capture_descriptor_offset = Some(INVOKER_DESCRIPTOR_OFFSET);
    wrapper_ctx.nested_concat_offset_offset = Some(INVOKER_CONCAT_OFFSET);
    let frame_size = 32;
    let call_reg = abi::nested_call_reg(emitter);

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
    emit_descriptor_entry_to_call_reg(emitter, call_reg);

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
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
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
