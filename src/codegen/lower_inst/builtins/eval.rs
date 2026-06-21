//! Purpose:
//! Lowers PHP `eval()` calls to the optional libelephc-eval bridge ABI.
//! Materializes a persistent per-function eval scope handle, flushes visible
//! locals into that scope, calls the bridge, and reloads synchronized locals
//! from boxed Mixed cells after the call returns.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Argument evaluation has already happened in PHP source order during EIR
//!   lowering; this module only materializes the bridge ABI call.
//! - The bridge is target-mangled like other C staticlib symbols.

use std::path::Path;

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_value_as_mixed, CodegenIrError, Result,
};
use crate::ir::{Function, Immediate, Instruction, LocalKind, LocalSlotId, Op};
use crate::names::{function_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::{Expr, ExprKind, TypeExpr, Visibility};
use crate::types::{ClassInfo, FunctionSig, PhpType};

use super::super::super::context::FunctionContext;
use super::super::{
    emit_runtime_callable_invoker_inline, expect_data, expect_operand, function_signature_from_eir,
    store_if_result,
};

const EVAL_STATUS_PARSE_ERROR: i64 = 1;
const EVAL_STATUS_UNCAUGHT_THROWABLE: i64 = 3;
const EVAL_STATUS_UNSUPPORTED: i64 = 4;
const EVAL_PARSE_ERROR_MESSAGE: &str = "Parse error: eval() fragment is invalid\n";
const EVAL_UNSUPPORTED_MESSAGE: &str =
    "Fatal error: eval() fragment uses an unsupported construct\n";
const EVAL_RUNTIME_FATAL_MESSAGE: &str = "Fatal error: eval() runtime failed\n";
const EVAL_STACK_BYTES: usize = 96;
const EVAL_RESULT_VALUE_CELL_OFFSET: usize = 8;
const EVAL_RESULT_ERROR_OFFSET: usize = 16;
const EVAL_CONTEXT_HANDLE_OFFSET: usize = 24;
const EVAL_SCOPE_HANDLE_OFFSET: usize = 32;
const EVAL_TEMP_CELL_OFFSET: usize = 40;
const EVAL_CODE_PTR_OFFSET: usize = 48;
const EVAL_CODE_LEN_OFFSET: usize = 56;
const EVAL_GLOBAL_SCOPE_HANDLE_OFFSET: usize = 64;
const EVAL_CALLED_CLASS_PTR_OFFSET: usize = 72;
const EVAL_CALLED_CLASS_LEN_OFFSET: usize = 80;
const EVAL_SCOPE_FLAG_PRESENT: i64 = 1;
const EVAL_SCOPE_FLAG_OWNED: i64 = 1 << 4;
const MAX_EVAL_NATIVE_METHOD_PARAMS: usize = 8;
const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";
const NATIVE_DEFAULT_NULL: i64 = 0;
const NATIVE_DEFAULT_BOOL: i64 = 1;
const NATIVE_DEFAULT_INT: i64 = 2;
const NATIVE_DEFAULT_FLOAT: i64 = 3;

/// Local slot metadata needed for conservative eval scope synchronization.
#[derive(Clone)]
struct EvalSyncLocal {
    name: String,
    slot: LocalSlotId,
    ty: PhpType,
}

/// Program-global metadata synchronized with eval `global` aliases.
#[derive(Clone)]
struct EvalSyncGlobal {
    name: String,
    ty: PhpType,
}

/// Local-to-global alias metadata inherited by eval from the caller function scope.
#[derive(Clone)]
struct EvalGlobalAlias {
    name: String,
    global_name: String,
}

/// A module-local function that can be registered with the eval context.
struct EvalNativeFunctionRegistration {
    name: String,
    signature: FunctionSig,
}

/// A module-local method signature that can be registered with the eval context.
struct EvalNativeMethodRegistration {
    class_name: String,
    method_name: String,
    is_static: bool,
    signature: FunctionSig,
}

/// A module-local constructor signature that can be registered with the eval context.
struct EvalNativeConstructorRegistration {
    class_name: String,
    signature: FunctionSig,
}

/// A module-local property type that can be registered with the eval context.
struct EvalNativePropertyTypeRegistration {
    class_name: String,
    property_name: String,
    type_spec: String,
}

/// Scalar native callable default that can be registered with libelephc-eval.
enum EvalNativeCallableDefault {
    Scalar { kind: i64, payload: i64 },
    String(String),
}

/// Lowers `eval($code)` to the eval bridge ABI and leaves the eval return cell in result registers.
pub(super) fn lower_eval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "eval", 1)?;
    let code = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(code)?.codegen_repr();
    if ty != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "eval() argument lowering for PHP type {:?}",
            ty
        )));
    }

    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    save_eval_code_string(ctx);
    ensure_eval_context(ctx)?;
    set_eval_call_site(ctx, inst);
    ensure_eval_scope(ctx)?;
    ensure_eval_global_scope(ctx)?;
    let sync_locals = eval_sync_locals(ctx);
    let sync_globals = eval_sync_globals(ctx);
    let global_aliases = eval_global_aliases(ctx);
    flush_eval_scope_locals(ctx, &sync_locals)?;
    flush_eval_global_scope(ctx, &sync_globals)?;
    mark_eval_scope_global_aliases(ctx, &global_aliases);
    set_eval_context_global_scope(ctx);
    let pushed_class_scope = push_eval_context_class_scope(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    load_eval_scope_to_arg(ctx, 1);
    move_saved_eval_code_to_eval_args(ctx);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_execute");
    abi::emit_call_label(ctx.emitter, &symbol);
    pop_eval_context_class_scope(ctx, pushed_class_scope);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    reload_eval_scope_locals(ctx, &sync_locals)?;
    reload_eval_global_scope(ctx, &sync_globals)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Updates eval context source metadata for file, directory, and call-site line magic constants.
fn set_eval_call_site(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    let Some(source_path) = ctx.module.source_path.as_deref() else {
        return;
    };
    load_eval_context_to_arg(ctx, 0);
    let (file_label, file_len) = ctx.data.add_string(source_path.as_bytes());
    let file_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, file_arg, &file_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        file_len as i64,
    );
    let dir = Path::new(source_path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let (dir_label, dir_len) = ctx.data.add_string(dir.as_bytes());
    let dir_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_symbol_address(ctx.emitter, dir_arg, &dir_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        dir_len as i64,
    );
    let line = inst
        .span
        .and_then(|span| i64::try_from(span.line).ok())
        .unwrap_or(0);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        line,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_set_call_site");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Lowers a native positional call to a function declared by a prior `eval()` call.
pub(super) fn lower_eval_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_function_call_args(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if inst.operands.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        inst.operands.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a native call to a prior eval-declared function using an argument array/hash.
pub(super) fn lower_eval_function_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "eval function call array", 1)?;
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let arg_array = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    let ty = ctx.load_value_to_result(arg_array)?.codegen_repr();
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, args_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_call_function_array");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic function existence probe to the eval bridge ABI.
pub(super) fn lower_eval_function_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(function_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_function_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic class existence probe to the eval bridge ABI.
pub(super) fn lower_eval_class_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_dynamic_class_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic constant existence probe to the eval bridge ABI.
pub(super) fn lower_eval_constant_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let constant_name = ctx.global_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(constant_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_constant_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic constant fetch to the eval bridge ABI.
pub(super) fn lower_eval_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let constant_name = ctx.global_name_data(expect_data(inst)?)?.to_string();
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let (name_label, name_len) = ctx.data.add_string(constant_name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_constant_fetch");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Returns the aligned scratch size for an eval-declared function call.
fn eval_function_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + arg_count * 8;
    (bytes + 15) & !15
}

/// Stores positional operands as boxed Mixed cells for the eval function-call ABI.
fn store_eval_function_call_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    for (index, operand) in inst.operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + index * 8);
    }
    Ok(())
}

/// Saves the loaded eval source string while scope setup calls use argument registers.
fn save_eval_code_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, EVAL_CODE_LEN_OFFSET);
}

/// Ensures a persistent eval context exists and stores its handle in the scratch frame.
fn ensure_eval_context(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let slot = eval_context_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_context_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    register_eval_native_functions(ctx, offset)?;
    register_eval_native_method_signatures(ctx, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    Ok(())
}

/// Returns the hidden frame slot that owns this function's persistent eval context.
fn eval_context_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalContext)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval context local"))
}

/// Registers eligible AOT global functions with a newly allocated eval context.
fn register_eval_native_functions(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
) -> Result<()> {
    let registrations = eval_native_function_registrations(ctx);
    for registration in registrations {
        register_eval_native_function(ctx, context_offset, &registration)?;
    }
    Ok(())
}

/// Registers eligible AOT method and constructor signatures with a newly allocated eval context.
fn register_eval_native_method_signatures(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    for registration in eval_native_method_registrations(ctx) {
        register_eval_native_method(ctx, context_offset, &registration);
    }
    for registration in eval_native_constructor_registrations(ctx) {
        register_eval_native_constructor(ctx, context_offset, &registration);
    }
    for registration in eval_native_property_type_registrations(ctx) {
        register_eval_native_property_type(ctx, context_offset, &registration);
    }
    register_eval_native_class_parents(ctx, context_offset);
}

/// Collects global PHP functions that can use the descriptor-invoker bridge.
fn eval_native_function_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeFunctionRegistration> {
    ctx.module
        .functions
        .iter()
        .filter(|function| function_can_register_with_eval(function))
        .map(|function| EvalNativeFunctionRegistration {
            name: function.name.clone(),
            signature: function_signature_from_eir(function),
        })
        .collect()
}

/// Collects public AOT methods whose parameter names can be exposed to eval binding.
fn eval_native_method_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeMethodRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_instance_methods(class_name, class_info, &mut registrations);
        collect_eval_native_static_methods(class_name, class_info, &mut registrations);
    }
    registrations
}

/// Collects public AOT constructors whose parameter names can be exposed to eval binding.
fn eval_native_constructor_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeConstructorRegistration> {
    let method_key = php_symbol_key("__construct");
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let Some(signature) = class_info.methods.get(&method_key) else {
            continue;
        };
        if !class_method_is_public(class_info, &method_key)
            || !method_signature_can_register_with_eval(signature)
        {
            continue;
        }
        registrations.push(EvalNativeConstructorRegistration {
            class_name: class_name.clone(),
            signature: signature.clone(),
        });
    }
    registrations
}

/// Collects AOT property types whose declared PHP type can be exposed to eval reflection.
fn eval_native_property_type_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativePropertyTypeRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_instance_property_types(class_name, class_info, &mut registrations);
        collect_eval_native_static_property_types(class_name, class_info, &mut registrations);
    }
    registrations
}

/// Registers generated AOT class parent metadata for eval `parent::` resolution.
fn register_eval_native_class_parents(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    let mut parents = ctx
        .module
        .class_infos
        .iter()
        .filter_map(|(class_name, class_info)| {
            let parent_name = class_info.parent.as_deref()?;
            Some((
                class_info.class_id,
                class_name.clone(),
                parent_name.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    parents.sort_by_key(|(class_id, _, _)| *class_id);
    for (_, class_name, parent_name) in parents {
        register_eval_native_class_parent(ctx, context_offset, &class_name, &parent_name);
    }
}

/// Adds declared instance-property type metadata for one class to eval registration.
fn collect_eval_native_instance_property_types(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativePropertyTypeRegistration>,
) {
    for (slot, (property_name, php_type)) in class_info.properties.iter().enumerate() {
        if !class_info.property_slot_is_declared(slot, property_name) {
            continue;
        }
        let Some(type_spec) = eval_native_php_type_spec(php_type, false) else {
            continue;
        };
        registrations.push(EvalNativePropertyTypeRegistration {
            class_name: eval_native_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            type_spec,
        });
    }
}

/// Adds declared static-property type metadata for one class to eval registration.
fn collect_eval_native_static_property_types(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativePropertyTypeRegistration>,
) {
    for (property_name, php_type) in &class_info.static_properties {
        if !class_info
            .declared_static_properties
            .contains(property_name)
        {
            continue;
        }
        let Some(type_spec) = eval_native_php_type_spec(php_type, false) else {
            continue;
        };
        registrations.push(EvalNativePropertyTypeRegistration {
            class_name: eval_native_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            type_spec,
        });
    }
}

/// Returns the class name that declares one AOT instance property row.
fn eval_native_instance_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one AOT static property row.
fn eval_native_static_property_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .static_property_declaring_classes
        .get(property_name)
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Adds eligible public instance methods for one class to eval signature registration.
fn collect_eval_native_instance_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        if method_name == "__construct"
            || !class_method_is_public(class_info, method_name)
            || !method_signature_can_register_with_eval(signature)
        {
            continue;
        }
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: false,
            signature: signature.clone(),
        });
    }
}

/// Adds eligible public static methods for one class to eval signature registration.
fn collect_eval_native_static_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        if !class_static_method_is_public(class_info, method_name)
            || !method_signature_can_register_with_eval(signature)
        {
            continue;
        }
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: true,
            signature: signature.clone(),
        });
    }
}

/// Returns true when a module function is a PHP-visible AOT function supported by this bridge.
fn function_can_register_with_eval(function: &Function) -> bool {
    !function.flags.is_main
        && !function.name.starts_with('_')
        && function
            .params
            .iter()
            .all(|param| !param.by_ref && !param.variadic)
}

/// Returns true when eval can bind native method arguments by fixed parameter names.
fn method_signature_can_register_with_eval(signature: &FunctionSig) -> bool {
    signature.params.len() <= MAX_EVAL_NATIVE_METHOD_PARAMS
        && signature.variadic.is_none()
        && signature.ref_params.iter().all(|is_ref| !*is_ref)
}

/// Returns generated type specs for declared native callable parameters.
fn eval_native_callable_param_type_specs(signature: &FunctionSig) -> Vec<Option<String>> {
    signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (_, php_type))| {
            if !signature
                .declared_params
                .get(index)
                .copied()
                .unwrap_or(false)
            {
                return None;
            }
            signature
                .param_type_exprs
                .get(index)
                .and_then(Option::as_ref)
                .and_then(eval_native_type_expr_spec)
                .or_else(|| eval_native_php_type_spec(php_type, false))
        })
        .collect()
}

/// Returns a generated type spec for a declared native callable return type.
fn eval_native_callable_return_type_spec(signature: &FunctionSig) -> Option<String> {
    signature
        .declared_return
        .then(|| eval_native_php_type_spec(&signature.return_type, true))
        .flatten()
}

/// Formats one parsed PHP type expression for eval native metadata registration.
fn eval_native_type_expr_spec(type_expr: &TypeExpr) -> Option<String> {
    match type_expr {
        TypeExpr::Int => Some("int".to_string()),
        TypeExpr::Float => Some("float".to_string()),
        TypeExpr::Bool => Some("bool".to_string()),
        TypeExpr::Str => Some("string".to_string()),
        TypeExpr::Void => Some("null".to_string()),
        TypeExpr::Never => None,
        TypeExpr::Iterable => Some("iterable".to_string()),
        TypeExpr::Array(_) => Some("array".to_string()),
        TypeExpr::Ptr(_) | TypeExpr::Buffer(_) => None,
        TypeExpr::Named(name) => Some(name.as_str().to_string()),
        TypeExpr::Nullable(inner) => {
            let inner = eval_native_type_expr_spec(inner)?;
            Some(format!("?{}", inner))
        }
        TypeExpr::Union(members) => eval_native_type_expr_member_specs(members, "|"),
        TypeExpr::Intersection(members) => eval_native_type_expr_member_specs(members, "&"),
    }
}

/// Formats a compound parsed type expression with the requested separator.
fn eval_native_type_expr_member_specs(members: &[TypeExpr], separator: &str) -> Option<String> {
    members
        .iter()
        .map(eval_native_type_expr_spec)
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join(separator))
}

/// Formats one checked PHP type for eval native metadata registration.
fn eval_native_php_type_spec(php_type: &PhpType, allow_return_atoms: bool) -> Option<String> {
    match php_type {
        PhpType::Int => Some("int".to_string()),
        PhpType::Float => Some("float".to_string()),
        PhpType::Str => Some("string".to_string()),
        PhpType::Bool => Some("bool".to_string()),
        PhpType::Void if allow_return_atoms => Some("void".to_string()),
        PhpType::Void => Some("null".to_string()),
        PhpType::Never if allow_return_atoms => Some("never".to_string()),
        PhpType::Never => None,
        PhpType::Iterable => Some("iterable".to_string()),
        PhpType::Mixed => Some("mixed".to_string()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some("array".to_string()),
        PhpType::Callable => Some("callable".to_string()),
        PhpType::Object(name) if name.is_empty() => Some("object".to_string()),
        PhpType::Object(name) => Some(name.clone()),
        PhpType::Union(members) => eval_native_php_type_member_specs(members),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => None,
    }
}

/// Formats union members from checked PHP types for eval native metadata registration.
fn eval_native_php_type_member_specs(members: &[PhpType]) -> Option<String> {
    members
        .iter()
        .map(|member| eval_native_php_type_spec(member, false))
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join("|"))
}

/// Converts a PHP signature default into the compact eval bridge default ABI.
fn eval_native_callable_default(expr: &Expr) -> Option<EvalNativeCallableDefault> {
    match &expr.kind {
        ExprKind::Null => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_NULL,
            payload: 0,
        }),
        ExprKind::BoolLiteral(value) => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_BOOL,
            payload: i64::from(*value),
        }),
        ExprKind::IntLiteral(value) => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_INT,
            payload: *value,
        }),
        ExprKind::FloatLiteral(value) => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload: value.to_bits() as i64,
        }),
        ExprKind::StringLiteral(value) => Some(EvalNativeCallableDefault::String(value.clone())),
        ExprKind::Negate(inner) => eval_native_callable_negated_default(inner),
        _ => None,
    }
}

/// Converts a negated literal default into the compact eval bridge default ABI.
fn eval_native_callable_negated_default(expr: &Expr) -> Option<EvalNativeCallableDefault> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => {
            value
                .checked_neg()
                .map(|payload| EvalNativeCallableDefault::Scalar {
                    kind: NATIVE_DEFAULT_INT,
                    payload,
                })
        }
        ExprKind::FloatLiteral(value) => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload: (-*value).to_bits() as i64,
        }),
        _ => None,
    }
}

/// Returns true when an instance method is public in the class metadata.
fn class_method_is_public(class_info: &ClassInfo, method_name: &str) -> bool {
    class_info
        .method_visibilities
        .get(method_name)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Returns true when a static method is public in the class metadata.
fn class_static_method_is_public(class_info: &ClassInfo, method_name: &str) -> bool {
    class_info
        .static_method_visibilities
        .get(method_name)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Emits one native-function registration call into the just-created eval context.
fn register_eval_native_function(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeFunctionRegistration,
) -> Result<()> {
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, &registration.signature, &[]);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &function_symbol(&registration.name),
        Some(&registration.name),
        callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
        Some(&registration.signature),
        &[],
        &[],
        callable_descriptor::CallableDescriptorInvocation::named(
            callable_descriptor::CallableDescriptorShape::Function,
            registration.name.clone(),
        ),
        Some(&invoker_label),
    );
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (name_label, name_len) = ctx.data.add_string(registration.name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &descriptor_label,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &invoker_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        registration.signature.params.len() as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function");
    abi::emit_call_label(ctx.emitter, &symbol);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        register_eval_native_function_param(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            param_name,
        );
    }
    Ok(())
}

/// Emits one native method signature registration call into the eval context.
fn register_eval_native_method(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeMethodRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let method_key = format!("{}::{}", registration.class_name, registration.method_name);
    let (method_key_label, method_key_len) = ctx.data.add_string(method_key.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &method_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        method_key_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        registration.signature.params.len() as i64,
    );
    let symbol = if registration.is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method")
    };
    abi::emit_call_label(ctx.emitter, &symbol);
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        register_eval_native_method_param(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            index,
            param_name,
        );
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_method_param_type(
                ctx,
                context_offset,
                &method_key_label,
                method_key_len,
                registration.is_static,
                index,
                type_spec,
            );
        }
    }
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default.as_ref().and_then(eval_native_callable_default) else {
            continue;
        };
        register_eval_native_method_param_default(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            index,
            &default,
        );
    }
    if let Some(type_spec) = eval_native_callable_return_type_spec(&registration.signature) {
        register_eval_native_method_return_type(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            &type_spec,
        );
    }
}

/// Emits one native method parameter-name registration call.
fn register_eval_native_method_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    param_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        method_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        method_key_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &param_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        param_name_len as i64,
    );
    let symbol = if is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method_param")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method_param")
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native method parameter-type registration call.
fn register_eval_native_method_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    type_spec: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        method_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        method_key_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        type_len as i64,
    );
    let symbol = if is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method_param_type")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method_param_type")
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native method return-type registration call.
fn register_eval_native_method_return_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    type_spec: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        method_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        method_key_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        type_len as i64,
    );
    let symbol = if is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method_return_type")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method_return_type")
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native method parameter-default registration call.
fn register_eval_native_method_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    default: &EvalNativeCallableDefault,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        method_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        method_key_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                *kind,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                *payload,
            );
            if is_static {
                ctx.emitter.target.extern_symbol(
                    "__elephc_eval_register_native_static_method_param_default_scalar",
                )
            } else {
                ctx.emitter
                    .target
                    .extern_symbol("__elephc_eval_register_native_method_param_default_scalar")
            }
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                default_len as i64,
            );
            if is_static {
                ctx.emitter.target.extern_symbol(
                    "__elephc_eval_register_native_static_method_param_default_string",
                )
            } else {
                ctx.emitter
                    .target
                    .extern_symbol("__elephc_eval_register_native_method_param_default_string")
            }
        }
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native constructor signature registration call into the eval context.
fn register_eval_native_constructor(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeConstructorRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (class_name_label, class_name_len) =
        ctx.data.add_string(registration.class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        registration.signature.params.len() as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor");
    abi::emit_call_label(ctx.emitter, &symbol);
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        register_eval_native_constructor_param(
            ctx,
            context_offset,
            &class_name_label,
            class_name_len,
            index,
            param_name,
        );
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_constructor_param_type(
                ctx,
                context_offset,
                &class_name_label,
                class_name_len,
                index,
                type_spec,
            );
        }
    }
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default.as_ref().and_then(eval_native_callable_default) else {
            continue;
        };
        register_eval_native_constructor_param_default(
            ctx,
            context_offset,
            &class_name_label,
            class_name_len,
            index,
            &default,
        );
    }
}

/// Emits one native class-parent metadata registration call into the eval context.
fn register_eval_native_class_parent(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name: &str,
    parent_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (class_name_label, class_name_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    let (parent_name_label, parent_name_len) = ctx.data.add_string(parent_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &parent_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        parent_name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_class_parent");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native property-type metadata registration call into the eval context.
fn register_eval_native_property_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativePropertyTypeRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}",
        registration.class_name, registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &property_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        property_key_len as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(registration.type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        type_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_property_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native constructor parameter-name registration call.
fn register_eval_native_constructor_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    param_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &param_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        param_name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_param");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native constructor parameter-type registration call.
fn register_eval_native_constructor_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    type_spec: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (type_label, type_len) = ctx.data.add_string(type_spec.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &type_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        type_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_param_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native constructor parameter-default registration call.
fn register_eval_native_constructor_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    default: &EvalNativeCallableDefault,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                *kind,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                *payload,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_constructor_param_default_scalar")
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 5),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_constructor_param_default_string")
        }
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function parameter-name registration call.
fn register_eval_native_function_param(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    param_name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        function_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        function_name_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        param_index as i64,
    );
    let (param_name_label, param_name_len) = ctx.data.add_string(param_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &param_name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        param_name_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_param");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Loads the persistent eval context local into the selected integer argument register.
fn load_eval_context_local_to_arg(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    arg_index: usize,
) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::load_at_offset(ctx.emitter, arg_reg, context_offset);
}

/// Loads the current eval context handle into the selected integer argument register.
fn load_eval_context_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_CONTEXT_HANDLE_OFFSET);
}

/// Reloads the saved eval source string into the bridge code pointer/length arguments.
fn move_saved_eval_code_to_eval_args(ctx: &mut FunctionContext<'_>) {
    let code_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let code_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_ptr_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, code_len_arg, EVAL_CODE_LEN_OFFSET);
}

/// Ensures a persistent eval scope exists and stores its handle in the scratch frame.
fn ensure_eval_scope(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let slot = eval_scope_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_scope_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_SCOPE_HANDLE_OFFSET);
    Ok(())
}

/// Ensures a persistent eval global-scope exists and stores its handle in scratch.
fn ensure_eval_global_scope(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let slot = eval_global_scope_slot(ctx)?;
    let offset = ctx.local_offset(slot)?;
    let ready = ctx.next_label("eval_global_scope_ready");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_new");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::store_at_offset(ctx.emitter, result_reg, offset);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
    Ok(())
}

/// Returns the hidden frame slot that owns this function's persistent eval scope.
fn eval_scope_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalScope)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval scope local"))
}

/// Returns the hidden frame slot that owns this function's eval global scope.
fn eval_global_scope_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalGlobalScope)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval global scope local"))
}

/// Loads the current eval scope handle into the selected integer argument register.
fn load_eval_scope_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_SCOPE_HANDLE_OFFSET);
}

/// Loads the current eval global-scope handle into the selected integer argument register.
fn load_eval_global_scope_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
}

/// Installs the current eval global-scope handle into the eval context.
fn set_eval_context_global_scope(ctx: &mut FunctionContext<'_>) {
    load_eval_context_to_arg(ctx, 0);
    load_eval_global_scope_to_arg(ctx, 1);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_set_global_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Enters the current AOT method's class scope in the eval context, if any.
fn push_eval_context_class_scope(ctx: &mut FunctionContext<'_>) -> Result<bool> {
    let Some(class_name) = current_eval_method_class(ctx).map(str::to_string) else {
        return Ok(false);
    };
    emit_eval_called_class_name_result(ctx, &class_name)?;
    let (called_ptr_reg, called_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, called_ptr_reg, EVAL_CALLED_CLASS_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, called_len_reg, EVAL_CALLED_CLASS_LEN_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let (class_label, class_len) = ctx.data.add_string(class_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &class_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        class_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_CALLED_CLASS_PTR_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        EVAL_CALLED_CLASS_LEN_OFFSET,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_push_class_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    Ok(true)
}

/// Leaves a pushed eval class scope while preserving the original eval status.
fn pop_eval_context_class_scope(ctx: &mut FunctionContext<'_>, pushed: bool) {
    if !pushed {
        return;
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_context_to_arg(ctx, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_pop_class_scope");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
}

/// Returns the lexical class encoded in the current EIR method name.
fn current_eval_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .flags
        .is_method
        .then(|| {
            ctx.function
                .name
                .rsplit_once("::")
                .map(|(class_name, _)| class_name)
        })
        .flatten()
}

/// Materializes the runtime called-class name for eval `static::` resolution.
fn emit_eval_called_class_name_result(
    ctx: &mut FunctionContext<'_>,
    fallback_class: &str,
) -> Result<()> {
    if eval_late_static_class_id_available(ctx) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_eval_called_class_name_result_aarch64(ctx),
            Arch::X86_64 => emit_eval_called_class_name_result_x86_64(ctx),
        }
    } else {
        emit_eval_static_string_result(ctx, fallback_class.as_bytes());
        Ok(())
    }
}

/// Emits the AArch64 class-id table lookup for eval's called class.
fn emit_eval_called_class_name_result_aarch64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("eval_called_class_missing");
    let done = ctx.next_label("eval_called_class_done");
    emit_eval_late_static_class_id_to_reg(ctx, "x12")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "x10", "_class_name_count", 0);
    ctx.emitter.instruction("cmp x12, x10"); // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("b.hs {}", missing)); // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x12, #4"); // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add x11, x11, x12"); // select the called-class metadata row
    ctx.emitter.instruction("ldr x1, [x11]"); // load the called-class name pointer
    ctx.emitter.instruction("ldr x2, [x11, #8]"); // load the called-class name length
    ctx.emitter.instruction(&format!("b {}", done)); // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "x1", "_class_name_missing");
    ctx.emitter.instruction("mov x2, #0"); // empty called-class name triggers lexical fallback in eval
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the x86_64 class-id table lookup for eval's called class.
fn emit_eval_called_class_name_result_x86_64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("eval_called_class_missing");
    let done = ctx.next_label("eval_called_class_done");
    emit_eval_late_static_class_id_to_reg(ctx, "r8")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_class_name_count", 0);
    ctx.emitter.instruction("cmp r8, r9"); // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("jae {}", missing)); // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
    ctx.emitter.instruction("shl r8, 4"); // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add r10, r8"); // select the called-class metadata row
    ctx.emitter.instruction("mov rax, QWORD PTR [r10]"); // load the called-class name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]"); // load the called-class name length
    ctx.emitter.instruction(&format!("jmp {}", done)); // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "rax", "_class_name_missing");
    ctx.emitter.instruction("mov rdx, 0"); // empty called-class name triggers lexical fallback in eval
    ctx.emitter.label(&done);
    Ok(())
}

/// Returns true when the current method frame can provide a late-static class id.
fn eval_late_static_class_id_available(ctx: &FunctionContext<'_>) -> bool {
    ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM).is_some()
        || ctx.local_slot_by_name("this").is_some()
}

/// Loads the late-static class id from the hidden static slot or `$this`.
fn emit_eval_late_static_class_id_to_reg(ctx: &mut FunctionContext<'_>, reg: &str) -> Result<()> {
    if let Some(slot) = ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM) {
        let offset = ctx.local_offset(slot)?;
        abi::load_at_offset(ctx.emitter, reg, offset);
        return Ok(());
    }
    if let Some(slot) = ctx.local_slot_by_name("this") {
        match ctx.local_php_type(slot)? {
            PhpType::Mixed | PhpType::Union(_) => {
                ctx.load_local_to_result(slot)?;
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                let object_reg = eval_mixed_unbox_low_payload_reg(ctx);
                abi::emit_load_from_address(ctx.emitter, reg, object_reg, 0);
            }
            PhpType::Object(_) => {
                let offset = ctx.local_offset(slot)?;
                abi::load_at_offset(ctx.emitter, reg, offset);
                abi::emit_load_from_address(ctx.emitter, reg, reg, 0);
            }
            other => {
                return Err(CodegenIrError::invalid_module(format!(
                    "eval class scope this local has PHP type {:?}",
                    other
                )))
            }
        }
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "eval class scope without called-class source in {}",
        ctx.function.name
    )))
}

/// Emits a static string result for eval class-scope setup fallback paths.
fn emit_eval_static_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Collects PHP-visible locals that the current conservative scope sync can round-trip.
fn eval_sync_locals(ctx: &FunctionContext<'_>) -> Vec<EvalSyncLocal> {
    ctx.function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::PhpLocal)
        .filter(|local| !local_uses_eval_global_sync(ctx, local.name.as_deref()))
        .filter_map(|local| {
            let name = local.name.clone()?;
            let ty = local.php_type.codegen_repr();
            eval_sync_type_supported(&ty).then_some(EvalSyncLocal {
                name,
                slot: local.id,
                ty,
            })
        })
        .collect()
}

/// Returns true when a local name is backed by program-global storage during eval.
fn local_uses_eval_global_sync(ctx: &FunctionContext<'_>, name: Option<&str>) -> bool {
    ctx.is_main && name.is_some_and(|name| ctx.has_global_name(name))
}

/// Collects caller-scope `global` aliases that eval fragments inherit by name.
fn eval_global_aliases(ctx: &FunctionContext<'_>) -> Vec<EvalGlobalAlias> {
    ctx.function
        .locals
        .iter()
        .filter(|local| local.kind == LocalKind::GlobalAlias)
        .filter_map(|local| {
            let name = local.name.clone()?;
            Some(EvalGlobalAlias {
                global_name: name.clone(),
                name,
            })
        })
        .collect()
}

/// Collects program globals that can be boxed into the eval global scope.
fn eval_sync_globals(ctx: &FunctionContext<'_>) -> Vec<EvalSyncGlobal> {
    let mut globals = ctx
        .module
        .data
        .global_names
        .iter()
        .filter_map(|name| {
            let ty = eval_sync_global_type(ctx, name)?;
            eval_sync_global_type_supported(&ty).then_some(EvalSyncGlobal {
                name: name.clone(),
                ty,
            })
        })
        .collect::<Vec<_>>();
    push_eval_process_superglobal(&mut globals, "argc", PhpType::Int);
    push_eval_process_superglobal(&mut globals, "argv", PhpType::Array(Box::new(PhpType::Str)));
    globals
}

/// Adds a process superglobal to eval global sync unless normal globals already include it.
fn push_eval_process_superglobal(globals: &mut Vec<EvalSyncGlobal>, name: &str, ty: PhpType) {
    if globals.iter().any(|global| global.name == name) {
        return;
    }
    globals.push(EvalSyncGlobal {
        name: name.to_string(),
        ty,
    });
}

/// Returns one unambiguous codegen type used for a program global, if available.
fn eval_sync_global_type(ctx: &FunctionContext<'_>, name: &str) -> Option<PhpType> {
    let mut inferred = None;
    for function in ctx
        .module
        .functions
        .iter()
        .chain(ctx.module.closures.iter())
    {
        for inst in &function.instructions {
            if global_instruction_name(ctx, inst) != Some(name) {
                continue;
            }
            let candidate = global_instruction_value_type(function, inst)?;
            let candidate = candidate.codegen_repr();
            if !eval_sync_global_type_supported(&candidate) {
                return None;
            }
            match &inferred {
                Some(existing) if existing != &candidate => return None,
                Some(_) => {}
                None => inferred = Some(candidate),
            }
        }
    }
    inferred
}

/// Returns the global name referenced by a load/store-global instruction.
fn global_instruction_name<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Option<&'a str> {
    let Some(Immediate::GlobalName(data)) = inst.immediate else {
        return None;
    };
    ctx.module
        .data
        .global_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns the value type carried by a global load or store instruction.
fn global_instruction_value_type(function: &Function, inst: &Instruction) -> Option<PhpType> {
    match inst.op {
        Op::LoadGlobal => {
            let result = inst.result?;
            function.value(result).map(|value| value.php_type.clone())
        }
        Op::StoreGlobal => {
            let value = *inst.operands.first()?;
            function.value(value).map(|value| value.php_type.clone())
        }
        _ => None,
    }
}

/// Returns true when a global type can round-trip through eval global scope sync.
fn eval_sync_global_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a local type can be boxed to Mixed and restored from Mixed after eval.
fn eval_sync_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Flushes visible native locals into the materialized eval scope before executing eval.
fn flush_eval_scope_locals(ctx: &mut FunctionContext<'_>, locals: &[EvalSyncLocal]) -> Result<()> {
    for local in locals {
        let ty = ctx.load_local_to_result(local.slot)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set(ctx, local, scope_set_flags_for_type(&ty));
    }
    Ok(())
}

/// Flushes supported program globals into the eval global scope before eval.
fn flush_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for global in globals {
        load_global_to_result(ctx, global);
        if !matches!(global.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &global.ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_global_scope_set(ctx, global, scope_set_flags_for_type(&global.ty));
    }
    Ok(())
}

/// Loads a program-global symbol into result registers using its inferred type.
fn load_global_to_result(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal) {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
}

/// Returns ABI flags for a scope value produced from the given native type.
fn scope_set_flags_for_type(ty: &PhpType) -> i64 {
    if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        0
    } else {
        EVAL_SCOPE_FLAG_OWNED
    }
}

/// Calls `__elephc_eval_scope_set` for one boxed global value.
fn emit_eval_global_scope_set(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(global.name.as_bytes());
    load_eval_global_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Marks caller-scope global aliases in the materialized eval scope.
fn mark_eval_scope_global_aliases(ctx: &mut FunctionContext<'_>, aliases: &[EvalGlobalAlias]) {
    for alias in aliases {
        let (name_label, name_len) = ctx.data.add_string(alias.name.as_bytes());
        let (global_name_label, global_name_len) =
            ctx.data.add_string(alias.global_name.as_bytes());
        load_eval_scope_to_arg(ctx, 0);
        let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
        abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 2),
            name_len as i64,
        );
        let global_name_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
        abi::emit_symbol_address(ctx.emitter, global_name_arg, &global_name_label);
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            global_name_len as i64,
        );
        let symbol = ctx
            .emitter
            .target
            .extern_symbol("__elephc_eval_scope_mark_global_alias");
        abi::emit_call_label(ctx.emitter, &symbol);
        emit_eval_status_check(ctx);
    }
}

/// Calls `__elephc_eval_scope_set` for one boxed local value.
fn emit_eval_scope_set(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(local.name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        flags,
    );
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Reloads synchronized locals from the eval scope after the eval interpreter returns.
fn reload_eval_scope_locals(ctx: &mut FunctionContext<'_>, locals: &[EvalSyncLocal]) -> Result<()> {
    for local in locals {
        emit_eval_scope_get(ctx, local);
        let missing = ctx.next_label("eval_scope_reload_missing");
        let done = ctx.next_label("eval_scope_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_local(ctx, local)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_local(ctx, local)?;
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Reloads synchronized program globals from the eval global scope after eval.
fn reload_eval_global_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for global in globals {
        emit_eval_global_scope_get(ctx, global);
        let missing = ctx.next_label("eval_global_reload_missing");
        let done = ctx.next_label("eval_global_reload_done");
        emit_branch_if_scope_entry_missing(ctx, &missing);
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
        store_mixed_scope_cell_to_global(ctx, global)?;
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&missing);
        store_missing_scope_entry_to_global(ctx, global)?;
        ctx.emitter.label(&done);
    }
    Ok(())
}

/// Calls `__elephc_eval_scope_get` and stores out cell/flags at the start of eval scratch.
fn emit_eval_scope_get(ctx: &mut FunctionContext<'_>, local: &EvalSyncLocal) {
    let (name_label, name_len) = ctx.data.add_string(local.name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, 0);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, 8);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_get` for one program global.
fn emit_eval_global_scope_get(ctx: &mut FunctionContext<'_>, global: &EvalSyncGlobal) {
    let (name_label, name_len) = ctx.data.add_string(global.name.as_bytes());
    load_eval_global_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, 0);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, 8);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Branches to `label` when the latest scope-get flags do not mark a visible value.
fn emit_branch_if_scope_entry_missing(ctx: &mut FunctionContext<'_>, label: &str) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("b.eq {}", label)); // skip reload when eval unset or omitted the local
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("je {}", label)); // skip reload when eval unset or omitted the local
        }
    }
}

/// Converts a scope Mixed cell back to the local's native storage type.
fn store_mixed_scope_cell_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            emit_retain_scope_cell_if_owned(ctx);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Object(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let object_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "rdi",
            };
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, object_reg)); // move unboxed object pointer into the local-store result register
            ctx.store_current_result_to_local(local.slot)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval scope reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Converts a scope Mixed cell back to a program-global storage symbol.
fn store_mixed_scope_cell_to_global(
    ctx: &mut FunctionContext<'_>,
    global: &EvalSyncGlobal,
) -> Result<()> {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            emit_retain_scope_cell_if_owned(ctx);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Mixed, false);
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Int, false);
        }
        PhpType::Bool => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Bool, false);
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Float, false);
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Str, false);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let payload_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "rdi",
            };
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, payload_reg)); // move the unboxed array payload into the ABI result register
            abi::emit_incref_if_refcounted(ctx.emitter, &ty);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval global reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Retains a scope-owned Mixed cell before storing it into a native local owner.
fn emit_retain_scope_cell_if_owned(ctx: &mut FunctionContext<'_>) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    let skip = ctx.next_label("eval_scope_reload_borrowed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_OWNED)); // check whether the scope keeps its own Mixed-cell owner
            ctx.emitter.instruction(&format!("b.eq {}", skip)); // borrowed scope entries can be copied back without retaining
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_OWNED)); // check whether the scope keeps its own Mixed-cell owner
            ctx.emitter.instruction(&format!("je {}", skip)); // borrowed scope entries can be copied back without retaining
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip);
}

/// Stores the local fallback used when eval unsets or removes a synchronized local.
fn store_missing_scope_entry_to_local(
    ctx: &mut FunctionContext<'_>,
    local: &EvalSyncLocal,
) -> Result<()> {
    match local.ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Int | PhpType::Bool => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Float => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            ctx.store_current_result_to_local(local.slot)?;
        }
        PhpType::Object(_) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            ctx.store_current_result_to_local(local.slot)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval scope missing reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Stores the program-global fallback for a missing eval global entry.
fn store_missing_scope_entry_to_global(
    ctx: &mut FunctionContext<'_>,
    global: &EvalSyncGlobal,
) -> Result<()> {
    let symbol = ir_global_symbol(&global.name);
    let ty = global.ty.codegen_repr();
    ctx.data.add_comm(symbol.clone(), ty.stack_size().max(8));
    match &ty {
        PhpType::Mixed | PhpType::Union(_) => {
            let symbol_name = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol_name);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Mixed, false);
        }
        PhpType::Int => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Int, false);
        }
        PhpType::Bool => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Bool, false);
        }
        PhpType::Float => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Float, false);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &PhpType::Str, false);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "eval global missing reload for PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Emits a fatal diagnostic when the eval bridge reports any non-zero status.
fn emit_eval_status_check(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("eval_status_ok");
    let parse_error_label = ctx.next_label("eval_status_parse_error");
    let throwable_label = ctx.next_label("eval_status_throwable");
    let unsupported_label = ctx.next_label("eval_status_unsupported");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_PARSE_ERROR, &parse_error_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNCAUGHT_THROWABLE, &throwable_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNSUPPORTED, &unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_RUNTIME_FATAL_MESSAGE);
    ctx.emitter.label(&parse_error_label);
    emit_eval_fatal_message(ctx, EVAL_PARSE_ERROR_MESSAGE);
    ctx.emitter.label(&throwable_label);
    emit_eval_throw_current(ctx);
    ctx.emitter.label(&unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_UNSUPPORTED_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Branches to a label when the eval bridge returned a specific status code.
fn emit_branch_if_eval_status(ctx: &mut FunctionContext<'_>, status: i64, label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, #{}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("b.eq {}", label)); // branch to the matching eval status handler
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("je {}", label)); // branch to the matching eval status handler
        }
    }
}

/// Publishes an eval-thrown Throwable and enters the normal runtime unwinder.
fn emit_eval_throw_current(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_ERROR_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let object_reg = eval_mixed_unbox_low_payload_reg(ctx);
    abi::emit_store_reg_to_symbol(ctx.emitter, object_reg, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
}

/// Returns the low payload register produced by `__rt_mixed_unbox` for eval status handling.
fn eval_mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Emits an eval diagnostic message and exits the process.
fn emit_eval_fatal_message(ctx: &mut FunctionContext<'_>, message: &str) {
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2"); // write the eval runtime diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2"); // write the eval runtime diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.instruction("mov eax, 1"); // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall"); // emit the eval runtime diagnostic before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}
