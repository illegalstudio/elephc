//! Purpose:
//! Lowers PHP `eval()` calls through internal EIR AOT functions or the optional
//! libelephc-magician bridge ABI. Materializes persistent eval scope state,
//! synchronizes visible locals, and reloads boxed Mixed cells after execution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Argument evaluation has already happened in PHP source order during EIR
//!   lowering; this module only materializes the bridge ABI call.
//! - The bridge is target-mangled like other C staticlib symbols.

use std::collections::BTreeSet;
use std::path::Path;

use crate::codegen::eval_ref_arg_helpers::eval_signature_ref_params_supported;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_callable_invoker::RuntimeCallableInvoker;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_value_as_mixed, CodegenIrError, Result,
};
use crate::ir::{Function, Immediate, Instruction, LocalKind, LocalSlotId, Module, Op, ValueId};
use crate::names::{function_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, TypeExpr, Visibility};
use crate::types::{
    is_php_integer_array_key, AttrArgEntry, AttrArgValue, AttrKey, ClassInfo, FunctionSig,
    InterfaceInfo, PhpType, PropertyHookContract,
};

use super::super::super::context::FunctionContext;
use super::super::{
    expect_data, expect_global_name, expect_operand, function_signature_from_eir, store_if_result,
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
const EVAL_CLASS_LOOKUP_GET_CLASS: i64 = 0;
const EVAL_CLASS_LOOKUP_GET_PARENT_CLASS: i64 = 1;
const EVAL_MEMBER_LOOKUP_METHOD_EXISTS: i64 = 0;
const EVAL_MEMBER_LOOKUP_PROPERTY_EXISTS: i64 = 1;
const EVAL_CLASS_RELATION_IMPLEMENTS: i64 = 0;
const EVAL_CLASS_RELATION_PARENTS: i64 = 1;
const EVAL_CLASS_RELATION_USES: i64 = 2;
const EVAL_CALLABLE_ARG_ARRAY_OFFSET: usize = EVAL_CODE_PTR_OFFSET;
const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";
const NATIVE_DEFAULT_NULL: i64 = 0;
const NATIVE_DEFAULT_BOOL: i64 = 1;
const NATIVE_DEFAULT_INT: i64 = 2;
const NATIVE_DEFAULT_FLOAT: i64 = 3;
const NATIVE_DEFAULT_EMPTY_ARRAY: i64 = 4;
const NATIVE_PROPERTY_REQUIRES_GET: i64 = 1;
const NATIVE_PROPERTY_REQUIRES_SET: i64 = 2;
const NATIVE_MEMBER_ATTRIBUTE_METHOD: u8 = 0;
const NATIVE_MEMBER_ATTRIBUTE_PROPERTY: u8 = 1;
const NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT: u8 = 2;
const NATIVE_MEMBER_ATTRIBUTE_CLASS: u8 = 3;
const NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED: u8 = 0;
const NATIVE_ATTRIBUTE_ARGS_SUPPORTED: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_NULL: u8 = 0;
const NATIVE_ATTRIBUTE_ARG_BOOL: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_INT: u8 = 2;
const NATIVE_ATTRIBUTE_ARG_STRING: u8 = 3;
const NATIVE_ATTRIBUTE_ARG_NAMED: u8 = 4;
const NATIVE_ATTRIBUTE_ARG_FLOAT: u8 = 5;
const NATIVE_ATTRIBUTE_ARG_ARRAY: u8 = 6;
const NATIVE_OBJECT_DEFAULT_ARG_SCALAR: u8 = 0;
const NATIVE_OBJECT_DEFAULT_ARG_STRING: u8 = 1;
const NATIVE_OBJECT_DEFAULT_ARG_OBJECT: u8 = 2;
const NATIVE_OBJECT_DEFAULT_ARG_NAMED: u8 = 3;
const NATIVE_OBJECT_DEFAULT_ARG_ARRAY: u8 = 4;
const NATIVE_ARRAY_DEFAULT_KEY_AUTO: u8 = 0;
const NATIVE_ARRAY_DEFAULT_KEY_INT: u8 = 1;
const NATIVE_ARRAY_DEFAULT_KEY_STRING: u8 = 2;
const MAX_NATIVE_OBJECT_DEFAULT_ARGS: usize = u8::MAX as usize;
const MAX_NATIVE_DEFAULT_CONSTANT_DEPTH: usize = 16;

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

/// Source location for one direct Mixed parameter passed into scope-read eval AOT.
enum EvalScopeReadParamSource {
    Local(EvalSyncLocal),
    Null,
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
    bridge_supported: bool,
}

/// A module-local method signature that can be registered with the eval context.
struct EvalNativeMethodRegistration {
    class_name: String,
    method_name: String,
    is_static: bool,
    signature: FunctionSig,
    bridge_supported: bool,
}

/// A module-local constructor signature that can be registered with the eval context.
struct EvalNativeConstructorRegistration {
    class_name: String,
    signature: FunctionSig,
    bridge_supported: bool,
}

/// Static metadata used while converting AOT defaults into eval bridge values.
struct EvalNativeDefaultContext<'a> {
    module: &'a Module,
    current_class: Option<&'a str>,
}

impl<'a> EvalNativeDefaultContext<'a> {
    /// Builds a default-materialization context for global function defaults.
    fn global(module: &'a Module) -> Self {
        Self {
            module,
            current_class: None,
        }
    }

    /// Builds a default-materialization context for class-like member defaults.
    fn for_class(module: &'a Module, class_name: &'a str) -> Self {
        Self {
            module,
            current_class: Some(class_name),
        }
    }
}

/// A module-local property type that can be registered with the eval context.
struct EvalNativePropertyTypeRegistration {
    class_name: String,
    property_name: String,
    type_spec: String,
}

/// A module-local interface property contract that can be registered with the eval context.
struct EvalNativeInterfacePropertyRegistration {
    interface_name: String,
    declaring_interface_name: String,
    property_name: String,
    type_spec: String,
    requires_get: bool,
    requires_set: bool,
}

/// A module-local abstract class property contract that can be registered with the eval context.
struct EvalNativeAbstractPropertyRegistration {
    class_name: String,
    declaring_class_name: String,
    property_name: String,
    type_spec: String,
    requires_get: bool,
    requires_set: bool,
}

/// A module-local property default that can be registered with the eval context.
struct EvalNativePropertyDefaultRegistration {
    class_name: String,
    property_name: String,
    default: EvalNativeCallableDefault,
}

/// A module-local member attribute that can be registered with the eval context.
struct EvalNativeMemberAttributeRegistration {
    owner_kind: u8,
    class_name: String,
    member_name: String,
    attribute_name: String,
    attribute_args: Option<Vec<AttrArgEntry>>,
}

/// Native callable default that can be registered with libelephc-magician.
enum EvalNativeCallableDefault {
    Scalar {
        kind: i64,
        payload: i64,
    },
    String(String),
    Array(Vec<EvalNativeCallableArrayDefaultElement>),
    Object {
        class_name: String,
        args: Vec<EvalNativeCallableObjectDefaultArg>,
    },
}

/// Array element metadata for a native callable default registered with eval.
struct EvalNativeCallableArrayDefaultElement {
    key: Option<EvalNativeCallableArrayDefaultKey>,
    default: EvalNativeCallableDefault,
}

/// Static array key metadata for a native callable default registered with eval.
enum EvalNativeCallableArrayDefaultKey {
    Int(i64),
    String(String),
}

/// Constructor argument metadata for an object-valued native callable default.
struct EvalNativeCallableObjectDefaultArg {
    name: Option<String>,
    default: EvalNativeCallableDefault,
}

/// Lowers `eval($code)` through internal EIR AOT or the bridge ABI and leaves its result in registers.
pub(super) fn lower_eval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "eval", 1)?;
    if let Some(fragment) = eval_literal_fragment(ctx, inst)? {
        if lower_eval_literal_eir_function(ctx, inst, &fragment)? {
            return Ok(());
        }
    }
    emit_eval_literal_aot_marker(ctx, inst)?;
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

/// Calls a pre-lowered internal EIR function for no-scope literal eval fragments.
fn lower_eval_literal_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let function_name = crate::eval_aot::eir_function_name(fragment);
    if let Some(callee) = ctx.callable_function_by_name(&function_name) {
        if callee.params.is_empty() && callee.return_php_type.codegen_repr() == PhpType::Mixed {
            ctx.emitter
                .comment("eval literal AOT compiled EIR function");
            let caller_stack_pad_bytes = abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, 0);
            abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
            abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
            store_if_result(ctx, inst)?;
            return Ok(true);
        }
    }
    lower_eval_literal_scope_eir_function(ctx, inst, fragment)
}

/// Calls a pre-lowered internal EIR function that uses direct params or eval scope.
fn lower_eval_literal_scope_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let function_name = crate::eval_aot::eir_scope_function_name(fragment);
    let Some(callee) = ctx.callable_function_by_name(&function_name) else {
        return Ok(false);
    };
    let param_types = callee
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect::<Vec<_>>();
    let return_type = callee.return_php_type.codegen_repr();
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        ctx.module.source_path.as_deref(),
        |name, args| eval_literal_static_function_supported_by_codegen(ctx, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_codegen(ctx, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return lower_eval_literal_scope_read_param_eir_function(
            ctx,
            inst,
            &function_name,
            &param_types,
            &return_type,
            plan.reads(),
            plan.array_read_constraints(),
            plan.assoc_array_read_constraints(),
            plan.float_predicate_read_constraints(),
        );
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        plan.array_read_constraints(),
        plan.assoc_array_read_constraints(),
        plan.float_predicate_read_constraints(),
    ) {
        return Ok(false);
    }
    if param_types.len() != 1 || return_type != PhpType::Mixed {
        return Ok(false);
    }
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with eval scope");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_scope(ctx)?;
    let read_names = plan.reads().clone();
    let write_names = plan.writes().clone();
    let mut flush_names = read_names.clone();
    flush_names.extend(write_names.iter().cloned());
    let sync_locals = eval_sync_locals(ctx);
    let sync_globals = eval_sync_globals(ctx);
    let flush_locals = filter_eval_sync_locals_by_name(sync_locals.clone(), &flush_names);
    let flush_globals = filter_eval_sync_globals_by_name(sync_globals.clone(), &flush_names);
    let reload_locals = filter_eval_sync_locals_by_name(sync_locals, &write_names);
    let reload_globals = filter_eval_sync_globals_by_name(sync_globals, &write_names);
    flush_eval_scope_locals(ctx, &flush_locals)?;
    flush_eval_globals_to_local_scope(ctx, &flush_globals);
    load_eval_scope_to_arg(ctx, 0);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    reload_eval_scope_locals(ctx, &reload_locals)?;
    reload_eval_globals_from_local_scope(ctx, &reload_globals)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Calls a read-only scope eval AOT function by passing direct boxed Mixed params.
fn lower_eval_literal_scope_read_param_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    function_name: &str,
    param_types: &[PhpType],
    return_type: &PhpType,
    read_names: &BTreeSet<String>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> Result<bool> {
    if param_types.len() != read_names.len()
        || param_types
            .iter()
            .any(|ty| ty.codegen_repr() != PhpType::Mixed)
        || return_type.codegen_repr() != PhpType::Mixed
    {
        return Ok(false);
    }
    if !eval_scope_read_constraints_supported(
        ctx,
        array_read_constraints,
        assoc_array_read_constraints,
        float_predicate_read_constraints,
    ) {
        return Ok(false);
    }
    let Some(param_sources) = eval_scope_read_param_sources(ctx, read_names) else {
        return Ok(false);
    };
    ctx.emitter
        .comment("eval literal AOT compiled EIR function with direct read params");
    for source in &param_sources {
        emit_eval_scope_read_param_source(ctx, source)?;
        abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    }
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, param_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let caller_stack_pad_bytes =
        abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Resolves read-only eval variables to direct local values or undefined null.
fn eval_scope_read_param_sources(
    ctx: &FunctionContext<'_>,
    read_names: &BTreeSet<String>,
) -> Option<Vec<EvalScopeReadParamSource>> {
    let sync_locals = eval_sync_locals(ctx);
    read_names
        .iter()
        .map(|name| {
            if let Some(local) = sync_locals.iter().find(|local| local.name == *name) {
                return Some(EvalScopeReadParamSource::Local(local.clone()));
            }
            if ctx.function.locals.iter().any(|local| {
                local.name.as_deref() == Some(name.as_str())
                    && local.kind == LocalKind::PhpLocal
                    && !local_uses_eval_global_sync(ctx, local.name.as_deref())
                    && local.php_type.codegen_repr() == PhpType::Void
            }) {
                return Some(EvalScopeReadParamSource::Null);
            }
            let has_unsupported_local = ctx
                .function
                .locals
                .iter()
                .any(|local| local.name.as_deref() == Some(name.as_str()));
            (!has_unsupported_local).then_some(EvalScopeReadParamSource::Null)
        })
        .collect()
}

/// Returns true when constrained direct read params have compatible local sources.
fn eval_scope_read_constraints_supported(
    ctx: &FunctionContext<'_>,
    array_read_constraints: &BTreeSet<String>,
    assoc_array_read_constraints: &BTreeSet<String>,
    float_predicate_read_constraints: &BTreeSet<String>,
) -> bool {
    let sync_locals = eval_sync_locals(ctx);
    array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_array_param_type_supported(&local.ty))
    }) && assoc_array_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_assoc_array_param_type_supported(&local.ty))
    }) && float_predicate_read_constraints.iter().all(|name| {
        sync_locals
            .iter()
            .find(|local| local.name == *name)
            .is_some_and(|local| eval_scope_read_float_predicate_param_type_supported(&local.ty))
    })
}

/// Returns true when a direct read-param source has array-only semantics.
fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a direct read-param source has associative-array-only semantics.
fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a direct read-param source can feed IEEE float predicates.
fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Emits one direct read-param value as a boxed Mixed result.
fn emit_eval_scope_read_param_source(
    ctx: &mut FunctionContext<'_>,
    source: &EvalScopeReadParamSource,
) -> Result<()> {
    match source {
        EvalScopeReadParamSource::Local(local) => {
            let ty = ctx.load_local_to_result(local.slot)?.codegen_repr();
            if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                emit_box_current_value_as_mixed(ctx.emitter, &ty);
            }
        }
        EvalScopeReadParamSource::Null => emit_core_mixed_null_cell(ctx),
    }
    Ok(())
}

/// Boxes PHP null with the core Mixed runtime helper.
fn emit_core_mixed_null_cell(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("mov x1, #0");                              // null has no low payload word
            ctx.emitter.instruction("mov x2, #0");                              // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 8");                              // materialize the core Mixed null runtime tag
            ctx.emitter.instruction("xor edi, edi");                            // null has no low payload word
            ctx.emitter.instruction("xor esi, esi");                            // null has no high payload word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Returns true when a static function call matches the EIR eval AOT codegen subset.
fn eval_literal_static_function_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = ctx
        .module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let signature = function_signature_from_eir(function);
    crate::eval_aot::static_function_signature_supported(&signature, args)
}

/// Returns true when a static method call matches the EIR eval AOT codegen subset.
fn eval_literal_static_method_supported_by_codegen(
    ctx: &FunctionContext<'_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = ctx.module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = ctx
        .module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Lowers an EIR eval-scope read for a static variable name.
pub(super) fn lower_eval_scope_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "eval scope get", 1)?;
    let scope = expect_operand(inst, 0)?;
    let name = eval_scope_instruction_name(ctx, inst)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_get_for_loaded_scope(ctx, &name, 0, 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers an EIR eval-scope write for a static variable name.
pub(super) fn lower_eval_scope_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "eval scope set", 2)?;
    let scope = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let name = eval_scope_instruction_name(ctx, inst)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    let flags = if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        EVAL_SCOPE_FLAG_OWNED
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
        scope_set_flags_for_type(&value_ty)
    };
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    load_eval_scope_operand_to_arg(ctx, scope, 0)?;
    emit_eval_scope_set_for_loaded_scope(ctx, &name, flags);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Returns the static PHP variable name attached to an eval-scope instruction.
fn eval_scope_instruction_name(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<String> {
    let data = expect_global_name(inst)?;
    ctx.module
        .data
        .global_names
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("global name", data.as_raw()))
}

/// Loads an eval-scope handle operand into the requested ABI argument register.
fn load_eval_scope_operand_to_arg(
    ctx: &mut FunctionContext<'_>,
    scope: ValueId,
    arg_index: usize,
) -> Result<()> {
    let arg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    let ty = ctx.load_value_to_reg(scope, arg)?.codegen_repr();
    if ty == PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "eval scope handle operand for PHP type {:?}",
        ty
    )))
}

/// Calls `__elephc_eval_scope_get` using an already-loaded scope handle arg.
fn emit_eval_scope_get_for_loaded_scope(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    out_cell_offset: usize,
    out_flags_offset: usize,
) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, out_cell_offset);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, out_flags_offset);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
}

/// Calls `__elephc_eval_scope_set` using an already-loaded scope handle arg.
fn emit_eval_scope_set_for_loaded_scope(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
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

/// Returns the literal fragment attached to an `EvalLiteralCall`, if this is one.
fn eval_literal_fragment(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<Option<String>> {
    if inst.op != Op::EvalLiteralCall {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Ok(None);
    };
    let fragment = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    Ok(Some(fragment.clone()))
}

/// Emits an assembly marker for literal eval fragments that still use the bridge fallback.
fn emit_eval_literal_aot_marker(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(fragment) = eval_literal_fragment(ctx, inst)? else {
        return Ok(());
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        &fragment,
        ctx.module.source_path.as_deref(),
        |name, args| eval_literal_static_function_supported_by_codegen(ctx, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_codegen(ctx, receiver, method, args)
        },
    );
    let reason = plan
        .fallback_reason()
        .map(crate::eval_aot::EvalAotFallbackReason::description)
        .unwrap_or("bridge fallback required");
    ctx.emitter.comment(&format!(
        "eval literal AOT fallback: {} ({} bytes), using bridge fallback",
        reason,
        fragment.len(),
    ));
    Ok(())
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

/// Lowers native construction of a class declared by a prior eval fragment.
pub(super) fn lower_eval_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (name_label, name_len) = ctx.intern_class_name_data(expect_data(inst)?)?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_function_call_args(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
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
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_new_object");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers fallback `new $class` construction through eval dynamic metadata.
pub(super) fn lower_eval_object_new_dynamic_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    miss_label: &str,
) -> Result<()> {
    let constructor_args = inst.operands.get(1..).ok_or_else(|| {
        CodegenIrError::invalid_module("eval dynamic object new missing class operand")
    })?;
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_function_call_stack_bytes(constructor_args.len());
    let eval_miss_label = ctx.next_label("eval_dynamic_new_missing_class");
    let done_label = ctx.next_label("eval_dynamic_new_done");
    let name_ptr_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_reg, 8);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_store_to_sp(ctx.emitter, name_ptr_reg, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, name_len_reg, EVAL_CODE_LEN_OFFSET);
    ensure_eval_context(ctx)?;
    store_eval_function_call_operands(ctx, constructor_args, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_arg, EVAL_CODE_PTR_OFFSET);
    let name_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_arg, EVAL_CODE_LEN_OFFSET);
    let args_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    if constructor_args.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, args_arg, 0);
    } else {
        abi::emit_temporary_stack_address(ctx.emitter, args_arg, args_offset);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        constructor_args.len() as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_try_new_object");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &eval_miss_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&eval_miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a method call that may dispatch to an eval-created dynamic object.
pub(super) fn lower_eval_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_method_call_stack_bytes(arg_count);
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    let object_ty = ctx.load_value_to_result(object)?.codegen_repr();
    if !matches!(object_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &object_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    store_eval_method_call_arg_pack(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    let method_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, method_ptr_arg, &method_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a native static-method call to an eval-declared dynamic class.
pub(super) fn lower_eval_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    method_name: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_context(ctx)?;
    store_eval_static_method_call_arg_pack(ctx, inst, args_offset)?;
    load_eval_context_to_arg(ctx, 0);
    let target = format!("{}::{}", class_name, method_name);
    let (target_label, target_len) = ctx.data.add_string(target.as_bytes());
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, target_arg, &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers a late-static AOT-frame static method call through an active eval override.
pub(super) fn lower_eval_native_frame_static_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    method_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let args_offset = EVAL_STACK_BYTES;
    let stack_bytes = eval_static_method_call_stack_bytes(inst.operands.len());
    let miss_stack_label = ctx.next_label("eval_native_frame_static_method_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    store_eval_static_method_call_arg_pack(ctx, inst, args_offset)?;
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let (method_label, method_len) = ctx.data.add_string(method_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &method_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        method_len as i64,
    );
    let pack_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, pack_arg, args_offset);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_method_call");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property read through an active eval override.
pub(super) fn lower_eval_native_frame_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_get_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        property_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    emit_eval_result_as_type(ctx, &inst.result_php_type)?;
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a late-static AOT-frame static-property write through an active eval override.
pub(super) fn lower_eval_native_frame_static_property_set(
    ctx: &mut FunctionContext<'_>,
    _inst: &Instruction,
    value: ValueId,
    frame_class: &str,
    property_name: &str,
    no_override_label: &str,
    done_label: &str,
) -> Result<()> {
    let miss_stack_label = ctx.next_label("eval_native_frame_static_prop_set_miss");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    emit_eval_native_frame_override_probe(ctx, frame_class, &miss_stack_label);
    store_eval_mixed_operand_at(ctx, value, EVAL_TEMP_CELL_OFFSET)?;
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        property_len as i64,
    );
    let value_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_load_temporary_stack_slot(ctx.emitter, value_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_static_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &miss_stack_label);
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&miss_stack_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, no_override_label);
    Ok(())
}

/// Lowers a callable-array dispatch through the eval bridge.
pub(super) fn lower_eval_callable_call_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    arg_array: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, arg_array, EVAL_CALLABLE_ARG_ARRAY_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let arg_array_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_array_arg, EVAL_CALLABLE_ARG_ARRAY_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_callable_call_array");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers an `is_callable()` probe through eval dynamic callable metadata.
pub(super) fn lower_eval_is_callable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, callback, EVAL_TEMP_CELL_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, EVAL_TEMP_CELL_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers member-existence introspection through eval dynamic metadata.
pub(super) fn lower_eval_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    member: ValueId,
    name: &str,
) -> Result<()> {
    let lookup_kind = eval_member_lookup_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, member, EVAL_CODE_PTR_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    let member_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, member_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        lookup_kind,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_member_exists");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers class/interface/trait relation introspection through eval dynamic metadata.
pub(super) fn lower_eval_class_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let relation_kind = eval_class_relation_kind(name)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_TEMP_CELL_OFFSET)?;
    load_eval_context_to_arg(ctx, 0);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        relation_kind,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_class_relation");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers object class-name introspection through the eval bridge.
pub(super) fn lower_eval_object_class_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    name: &str,
) -> Result<()> {
    let lookup_kind = eval_class_lookup_kind(name)?;
    let non_object_label = ctx.next_label("eval_object_class_non_object");
    let done_label = ctx.next_label("eval_object_class_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_object_operand(ctx, object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &non_object_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        lookup_kind,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_class_name");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_eval_unboxed_string_result(ctx);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&non_object_label);
    emit_eval_string_result(ctx, b"");

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    box_eval_bool_result_if_mixed(ctx, inst);
    store_if_result(ctx, inst)
}

/// Lowers object/class relation predicates through the eval bridge.
pub(super) fn lower_eval_object_is_a(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target_class: &str,
    exclude_self: bool,
) -> Result<()> {
    let false_label = ctx.next_label("eval_object_is_a_false");
    let done_label = ctx.next_label("eval_object_is_a_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_object_operand(ctx, object)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &false_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let (target_label, target_len) = ctx.data.add_string(target_class.as_bytes());
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, target_arg, &target_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        target_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        i64::from(exclude_self),
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers object/class relation predicates whose target is a runtime string or object cell.
pub(super) fn lower_eval_object_is_a_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    target: ValueId,
    exclude_self: bool,
) -> Result<()> {
    let false_label = ctx.next_label("eval_object_is_a_dynamic_false");
    let invalid_label = ctx.next_label("eval_object_is_a_dynamic_invalid");
    let done_label = ctx.next_label("eval_object_is_a_dynamic_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
    store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)?;
    store_eval_mixed_operand_at(ctx, target, EVAL_CODE_PTR_OFFSET)?;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_CODE_PTR_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_validate_eval_dynamic_instanceof_target(ctx, &invalid_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        EVAL_TEMP_CELL_OFFSET,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_eval_unboxed_not_object(ctx, &false_label);
    load_eval_context_to_arg(ctx, 0);
    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, EVAL_TEMP_CELL_OFFSET);
    let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, target_arg, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        i64::from(exclude_self),
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_object_is_a_dynamic");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &invalid_label);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&invalid_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_invalid_target");

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Returns true when the current function owns an eval context local.
pub(super) fn has_eval_context(ctx: &FunctionContext<'_>) -> bool {
    eval_context_slot(ctx).is_ok()
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
    box_eval_bool_result_if_mixed(ctx, inst);
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
    box_eval_bool_result_if_mixed(ctx, inst);
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
    box_eval_bool_result_if_mixed(ctx, inst);
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

/// Lowers a post-eval dynamic class-like constant fetch to the eval bridge ABI.
pub(super) fn lower_eval_class_constant_fetch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    constant_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
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
    let (constant_label, constant_len) = ctx.data.add_string(constant_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &constant_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        constant_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_class_constant_fetch");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic static-property read to the eval bridge ABI.
pub(super) fn lower_eval_static_property_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    ensure_eval_context(ctx)?;
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
    let (property_label, property_len) = ctx.data.add_string(property_name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &property_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        property_len as i64,
    );
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_property_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers a post-eval dynamic static-property write to the eval bridge ABI.
pub(super) fn lower_eval_static_property_set(
    ctx: &mut FunctionContext<'_>,
    _inst: &Instruction,
    value: ValueId,
    class_name: &str,
    property_name: &str,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    store_eval_mixed_operand_at(ctx, value, EVAL_TEMP_CELL_OFFSET)?;
    ensure_eval_context(ctx)?;
    load_eval_context_to_arg(ctx, 0);
    let target = format!("{}::{}", class_name, property_name);
    let (target_label, target_len) = ctx.data.add_string(target.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &target_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_len as i64,
    );
    let value_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, value_arg, EVAL_TEMP_CELL_OFFSET);
    let out_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_arg, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_static_property_set");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}

/// Returns the aligned scratch size for an eval-declared function call.
fn eval_function_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic method-call argument pack.
fn eval_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Returns the aligned scratch size for an eval dynamic static-method call.
fn eval_static_method_call_stack_bytes(arg_count: usize) -> usize {
    let bytes = EVAL_STACK_BYTES + 8 + arg_count * 8;
    (bytes + 15) & !15
}

/// Stores positional operands as boxed Mixed cells for the eval function-call ABI.
fn store_eval_function_call_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    store_eval_function_call_operands(ctx, &inst.operands, args_offset)
}

/// Stores one operand slice as boxed Mixed cells for eval positional-call ABIs.
fn store_eval_function_call_operands(
    ctx: &mut FunctionContext<'_>,
    operands: &[ValueId],
    args_offset: usize,
) -> Result<()> {
    for (index, operand) in operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + index * 8);
    }
    Ok(())
}

/// Stores a count-prefixed positional argument pack for the eval method-call ABI.
fn store_eval_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    let arg_count = inst.operands.len().saturating_sub(1);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, arg_count as i64);
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    for (index, operand) in inst.operands.iter().skip(1).enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + 8 + index * 8);
    }
    Ok(())
}

/// Stores all positional operands as a count-prefixed static-method argument pack.
fn store_eval_static_method_call_arg_pack(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args_offset: usize,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, inst.operands.len() as i64);
    abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset);
    for (index, operand) in inst.operands.iter().enumerate() {
        let ty = ctx.load_value_to_result(*operand)?.codegen_repr();
        if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, args_offset + 8 + index * 8);
    }
    Ok(())
}

/// Stores an object operand as a boxed Mixed cell in eval scratch storage.
fn store_eval_object_operand(ctx: &mut FunctionContext<'_>, object: ValueId) -> Result<()> {
    store_eval_mixed_operand_at(ctx, object, EVAL_TEMP_CELL_OFFSET)
}

/// Stores one operand as a boxed Mixed cell at an eval scratch offset.
fn store_eval_mixed_operand_at(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    offset: usize,
) -> Result<()> {
    let value_ty = ctx.load_value_to_result(value)?.codegen_repr();
    if !matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, offset);
    Ok(())
}

/// Probes whether eval has a late-static called-class override for an AOT frame.
fn emit_eval_native_frame_override_probe(
    ctx: &mut FunctionContext<'_>,
    frame_class: &str,
    no_override_label: &str,
) {
    let (frame_label, frame_len) = ctx.data.add_string(frame_class.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &frame_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        frame_len as i64,
    );
    let out_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_temporary_stack_address(ctx.emitter, out_ptr_arg, EVAL_CALLED_CLASS_PTR_OFFSET);
    let out_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_len_arg, EVAL_CALLED_CLASS_LEN_OFFSET);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_native_frame_called_class_override");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, no_override_label);
}

/// Converts an eval Mixed result cell to the concrete EIR type expected here.
fn emit_eval_result_as_type(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => Ok(()),
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        PhpType::Float => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        PhpType::Bool | PhpType::False => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            Ok(())
        }
        PhpType::TaggedScalar => {
            emit_eval_mixed_result_as_tagged_scalar(ctx);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(())
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_)
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => {
            emit_eval_unbox_mixed_to_owned_result(ctx, &result_ty.codegen_repr());
            Ok(())
        }
    }
}

/// Reorders an eval Mixed result cell into inline tagged-scalar result registers.
fn emit_eval_mixed_result_as_tagged_scalar(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed eval result tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed eval payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed eval tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed eval result tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed eval payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed eval tag into the tagged-scalar tag register
        }
    }
}

/// Unboxes an eval Mixed result cell and retains concrete refcounted payloads.
fn emit_eval_unbox_mixed_to_owned_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_eval_move_unboxed_low_payload_to_result(ctx);
    abi::emit_incref_if_refcounted(ctx.emitter, result_ty);
}

/// Moves the low payload from `__rt_mixed_unbox` into the integer result register.
fn emit_eval_move_unboxed_low_payload_to_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the unboxed eval low payload as the concrete result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the unboxed eval low payload as the concrete result
        }
    }
}

/// Boxes a raw eval predicate result when the enclosing IR value expects Mixed storage.
fn box_eval_bool_result_if_mixed(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result.is_some() && inst.result_php_type.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
}

/// Returns the eval ABI discriminator for a class-name builtin.
fn eval_class_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "get_class" => Ok(EVAL_CLASS_LOOKUP_GET_CLASS),
        "get_parent_class" => Ok(EVAL_CLASS_LOOKUP_GET_PARENT_CLASS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval object class-name lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for member-existence builtins.
fn eval_member_lookup_kind(name: &str) -> Result<i64> {
    match name {
        "method_exists" => Ok(EVAL_MEMBER_LOOKUP_METHOD_EXISTS),
        "property_exists" => Ok(EVAL_MEMBER_LOOKUP_PROPERTY_EXISTS),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval member-exists lookup {}",
            name
        ))),
    }
}

/// Returns the eval ABI discriminator for class/interface/trait relation builtins.
fn eval_class_relation_kind(name: &str) -> Result<i64> {
    match name {
        "class_implements" => Ok(EVAL_CLASS_RELATION_IMPLEMENTS),
        "class_parents" => Ok(EVAL_CLASS_RELATION_PARENTS),
        "class_uses" => Ok(EVAL_CLASS_RELATION_USES),
        _ => Err(CodegenIrError::unsupported(format!(
            "eval class-relation lookup {}",
            name
        ))),
    }
}

/// Branches when `__rt_mixed_unbox` did not expose an object payload.
fn emit_branch_if_eval_unboxed_not_object(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("b.ne {}", label));                // non-object values use the native false/empty fallback
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the Mixed value contains an object
            ctx.emitter.instruction(&format!("jne {}", label));                 // non-object values use the native false/empty fallback
        }
    }
}

/// Branches to the invalid-target fatal unless an eval dynamic target is string or object.
fn emit_validate_eval_dynamic_instanceof_target(ctx: &mut FunctionContext<'_>, label: &str) {
    let ok_label = ctx.next_label("eval_object_is_a_dynamic_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("b {}", label));                   // reject every other dynamic instanceof target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept string targets for dynamic instanceof
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", ok_label));               // accept object targets for dynamic instanceof
            ctx.emitter.instruction(&format!("jmp {}", label));                 // reject every other dynamic instanceof target kind
        }
    }
    ctx.emitter.label(&ok_label);
}

/// Branches when an eval C-ABI call returned a negative `int` sentinel.
fn emit_branch_if_eval_c_int_negative(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let branch = format!("tbnz w0, #31, {}", label);
            ctx.emitter.instruction(&branch);                                   // branch when the C int result is the invalid-target sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // set flags from the C int result
            ctx.emitter.instruction(&format!("js {}", label));                  // branch when the C int result is the invalid-target sentinel
        }
    }
}

/// Reorders an unboxed eval string cell into the target string result registers.
fn emit_eval_unboxed_string_result(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into the x86_64 string-result register
    }
}

/// Emits a borrowed string literal as the current native string result.
fn emit_eval_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
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
    register_eval_declared_symbols(ctx, offset);
    register_eval_native_functions(ctx, offset)?;
    register_eval_native_method_signatures(ctx, offset);
    mark_eval_strict_php(ctx);
    ctx.emitter.label(&ready);
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    Ok(())
}

/// Marks the eval bridge as strict-PHP when this compilation runs with
/// `--strict-php`, so runtime eval hides extension builtins exactly like the
/// AOT surface does. Emits nothing in normal compilations: non-strict binaries
/// never reference the setter symbol, and the bridge's flag defaults to off.
fn mark_eval_strict_php(ctx: &mut FunctionContext<'_>) {
    if !crate::strict_php::is_enabled() {
        return;
    }
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, 1);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_strict_php");
    abi::emit_call_label(ctx.emitter, &symbol);
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
    for registration in eval_native_abstract_property_registrations(ctx) {
        register_eval_native_abstract_property(ctx, context_offset, &registration);
    }
    for registration in eval_native_interface_property_registrations(ctx) {
        register_eval_native_interface_property(ctx, context_offset, &registration);
    }
    for registration in eval_native_property_default_registrations(ctx) {
        register_eval_native_property_default(ctx, context_offset, &registration);
    }
    for registration in eval_native_member_attribute_registrations(ctx) {
        register_eval_native_member_attribute(ctx, context_offset, &registration);
    }
    register_eval_native_class_parents(ctx, context_offset);
}

/// Registers generated declared-name metadata with a newly allocated eval context.
fn register_eval_declared_symbols(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    let class_names = ctx.module.declared_class_names.clone();
    let interface_names = ctx.module.declared_interface_names.clone();
    let trait_names = ctx.module.declared_trait_names.clone();
    for name in class_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_class_name",
            &name,
        );
    }
    for name in interface_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_interface_name",
            &name,
        );
    }
    for name in trait_names {
        register_eval_declared_symbol_name(
            ctx,
            context_offset,
            "__elephc_eval_register_declared_trait_name",
            &name,
        );
    }
}

/// Emits one declared-name metadata registration call into the eval context.
fn register_eval_declared_symbol_name(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    symbol_name: &str,
    name: &str,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
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
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Collects global PHP functions that can use the descriptor-invoker bridge.
fn eval_native_function_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeFunctionRegistration> {
    ctx.module
        .functions
        .iter()
        .filter(|function| function_has_eval_metadata(function))
        .map(|function| EvalNativeFunctionRegistration {
            name: function.name.clone(),
            signature: function_signature_from_eir(function),
            bridge_supported: function_signature_can_bridge_with_eval(function),
        })
        .collect()
}

/// Collects AOT method signatures whose metadata can be exposed to eval.
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
    let mut interfaces = ctx.module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, interface_info) in interfaces {
        collect_eval_native_interface_instance_methods(
            interface_name,
            interface_info,
            &mut registrations,
        );
        collect_eval_native_interface_static_methods(
            interface_name,
            interface_info,
            &mut registrations,
        );
    }
    registrations
}

/// Collects AOT constructors whose metadata can be exposed to eval.
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
        let bridge_supported = class_method_visibility_bridge_supported(class_info, &method_key)
            && constructor_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeConstructorRegistration {
            class_name: class_name.clone(),
            signature: signature.clone(),
            bridge_supported,
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

/// Collects AOT interface property contracts that eval can validate at declaration time.
fn eval_native_interface_property_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeInterfacePropertyRegistration> {
    let mut registrations = Vec::new();
    let mut interfaces = ctx.module.interface_infos.iter().collect::<Vec<_>>();
    interfaces.sort_by_key(|(_, interface_info)| interface_info.interface_id);
    for (interface_name, interface_info) in interfaces {
        let mut property_names = interface_info.property_order.iter().collect::<Vec<_>>();
        if property_names.is_empty() {
            property_names = interface_info.properties.keys().collect();
            property_names.sort();
        }
        for property_name in property_names {
            let Some(contract) = interface_info.properties.get(property_name) else {
                continue;
            };
            let Some(registration) = eval_native_interface_property_registration(
                interface_name,
                property_name,
                contract,
            ) else {
                continue;
            };
            registrations.push(registration);
        }
    }
    registrations
}

/// Collects AOT abstract class property contracts that eval can validate at declaration time.
fn eval_native_abstract_property_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeAbstractPropertyRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let mut property_names = class_info
            .abstract_property_hooks
            .keys()
            .collect::<Vec<_>>();
        property_names.sort();
        for property_name in property_names {
            let Some(contract) = class_info.abstract_property_hooks.get(property_name) else {
                continue;
            };
            let Some(registration) =
                eval_native_abstract_property_registration(class_name, property_name, contract)
            else {
                continue;
            };
            registrations.push(registration);
        }
    }
    registrations
}

/// Converts one static abstract class property contract into eval-native metadata.
fn eval_native_abstract_property_registration(
    class_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
) -> Option<EvalNativeAbstractPropertyRegistration> {
    let requires_get = contract.get_type.is_some();
    let requires_set = contract.set_type.is_some();
    if !requires_get && !requires_set {
        return None;
    }
    let type_spec = eval_native_interface_property_type_spec(contract)?;
    Some(EvalNativeAbstractPropertyRegistration {
        class_name: class_name.to_string(),
        declaring_class_name: contract.declaring_type.clone(),
        property_name: property_name.to_string(),
        type_spec,
        requires_get,
        requires_set,
    })
}

/// Converts one static interface property contract into eval-native metadata.
fn eval_native_interface_property_registration(
    interface_name: &str,
    property_name: &str,
    contract: &PropertyHookContract,
) -> Option<EvalNativeInterfacePropertyRegistration> {
    let requires_get = contract.get_type.is_some();
    let requires_set = contract.set_type.is_some();
    if !requires_get && !requires_set {
        return None;
    }
    let type_spec = eval_native_interface_property_type_spec(contract)?;
    Some(EvalNativeInterfacePropertyRegistration {
        interface_name: interface_name.to_string(),
        declaring_interface_name: contract.declaring_type.clone(),
        property_name: property_name.to_string(),
        type_spec,
        requires_get,
        requires_set,
    })
}

/// Returns the single property type representation accepted by EvalIR metadata.
fn eval_native_interface_property_type_spec(contract: &PropertyHookContract) -> Option<String> {
    match (contract.get_type.as_ref(), contract.set_type.as_ref()) {
        (Some(get_type), Some(set_type)) if get_type == set_type => {
            eval_native_php_type_spec(get_type, false)
        }
        (Some(get_type), None) => eval_native_php_type_spec(get_type, false),
        (None, Some(set_type)) => eval_native_php_type_spec(set_type, false),
        _ => None,
    }
}

/// Collects AOT property defaults whose value can be exposed to eval reflection.
fn eval_native_property_default_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativePropertyDefaultRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        let default_context = EvalNativeDefaultContext::for_class(ctx.module, class_name);
        collect_eval_native_instance_property_defaults(
            class_name,
            class_info,
            &default_context,
            &mut registrations,
        );
        collect_eval_native_static_property_defaults(
            class_name,
            class_info,
            &default_context,
            &mut registrations,
        );
    }
    registrations
}

/// Collects AOT member attributes whose metadata can be exposed to eval reflection.
fn eval_native_member_attribute_registrations(
    ctx: &FunctionContext<'_>,
) -> Vec<EvalNativeMemberAttributeRegistration> {
    let mut registrations = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_eval_native_class_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_method_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_property_attributes(class_name, class_info, &mut registrations);
        collect_eval_native_class_constant_attributes(class_name, class_info, &mut registrations);
    }
    dedupe_eval_native_member_attribute_registrations(registrations)
}

/// Removes inherited duplicate member-attribute registrations by normalized metadata key.
fn dedupe_eval_native_member_attribute_registrations(
    registrations: Vec<EvalNativeMemberAttributeRegistration>,
) -> Vec<EvalNativeMemberAttributeRegistration> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::with_capacity(registrations.len());
    for registration in registrations {
        let key = (
            registration.owner_kind,
            php_symbol_key(&registration.class_name),
            registration.member_name.clone(),
            registration.attribute_name.clone(),
            registration.attribute_args.clone(),
        );
        if seen.insert(key) {
            unique.push(registration);
        }
    }
    unique
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

/// Adds class-level attribute metadata for one class-like symbol to eval registration.
fn collect_eval_native_class_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    collect_eval_native_member_attributes(
        NATIVE_MEMBER_ATTRIBUTE_CLASS,
        class_name,
        "",
        &class_info.attribute_names,
        &class_info.attribute_args,
        registrations,
    );
}

/// Adds method attribute metadata for one class to eval registration.
fn collect_eval_native_method_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut methods = class_info.method_attribute_names.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method_name, _)| method_name.as_str());
    for (method_name, attribute_names) in methods {
        let attribute_args = class_info
            .method_attribute_args
            .get(method_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_METHOD,
            eval_native_method_declaring_class(class_name, class_info, method_name),
            method_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds property attribute metadata for one class to eval registration.
fn collect_eval_native_property_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut properties = class_info
        .property_attribute_names
        .iter()
        .collect::<Vec<_>>();
    properties.sort_by_key(|(property_name, _)| property_name.as_str());
    for (property_name, attribute_names) in properties {
        let attribute_args = class_info
            .property_attribute_args
            .get(property_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_PROPERTY,
            eval_native_property_attribute_declaring_class(class_name, class_info, property_name),
            property_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds class-constant attribute metadata for one class to eval registration.
fn collect_eval_native_class_constant_attributes(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    let mut constants = class_info
        .constant_attribute_names
        .iter()
        .collect::<Vec<_>>();
    constants.sort_by_key(|(constant_name, _)| constant_name.as_str());
    for (constant_name, attribute_names) in constants {
        let attribute_args = class_info
            .constant_attribute_args
            .get(constant_name)
            .cloned()
            .unwrap_or_default();
        collect_eval_native_member_attributes(
            NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT,
            class_name,
            constant_name,
            attribute_names,
            &attribute_args,
            registrations,
        );
    }
}

/// Adds aligned attribute name/argument metadata for one AOT member.
fn collect_eval_native_member_attributes(
    owner_kind: u8,
    class_name: &str,
    member_name: &str,
    attribute_names: &[String],
    attribute_args: &[Option<Vec<AttrArgEntry>>],
    registrations: &mut Vec<EvalNativeMemberAttributeRegistration>,
) {
    for (index, attribute_name) in attribute_names.iter().enumerate() {
        let Some(args) = attribute_args.get(index).cloned().flatten() else {
            continue;
        };
        let attribute_args = if eval_native_member_attribute_args_supported(&args) {
            Some(args)
        } else {
            None
        };
        registrations.push(EvalNativeMemberAttributeRegistration {
            owner_kind,
            class_name: class_name.to_string(),
            member_name: member_name.to_string(),
            attribute_name: attribute_name.clone(),
            attribute_args,
        });
    }
}

/// Adds supported instance-property default metadata for one class to eval registration.
fn collect_eval_native_instance_property_defaults(
    class_name: &str,
    class_info: &ClassInfo,
    default_context: &EvalNativeDefaultContext<'_>,
    registrations: &mut Vec<EvalNativePropertyDefaultRegistration>,
) {
    for (slot, (property_name, _)) in class_info.properties.iter().enumerate() {
        let default = class_info.defaults.get(slot).and_then(Option::as_ref);
        let is_declared = class_info.property_slot_is_declared(slot, property_name);
        let is_abstract = class_info.abstract_properties.contains(property_name);
        let Some(default) =
            eval_native_property_default(default, is_declared, is_abstract, default_context)
        else {
            continue;
        };
        registrations.push(EvalNativePropertyDefaultRegistration {
            class_name: eval_native_instance_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            default,
        });
    }
}

/// Adds supported static-property default metadata for one class to eval registration.
fn collect_eval_native_static_property_defaults(
    class_name: &str,
    class_info: &ClassInfo,
    default_context: &EvalNativeDefaultContext<'_>,
    registrations: &mut Vec<EvalNativePropertyDefaultRegistration>,
) {
    for (slot, (property_name, _)) in class_info.static_properties.iter().enumerate() {
        let default = class_info
            .static_defaults
            .get(slot)
            .and_then(Option::as_ref);
        let is_declared = class_info
            .declared_static_properties
            .contains(property_name);
        let Some(default) =
            eval_native_property_default(default, is_declared, false, default_context)
        else {
            continue;
        };
        registrations.push(EvalNativePropertyDefaultRegistration {
            class_name: eval_native_static_property_declaring_class(
                class_name,
                class_info,
                property_name,
            )
            .to_string(),
            property_name: property_name.clone(),
            default,
        });
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

/// Returns the class name that declares one AOT method metadata row.
fn eval_native_method_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    method_name: &str,
) -> &'a str {
    class_info
        .method_impl_classes
        .get(method_name)
        .or_else(|| class_info.static_method_impl_classes.get(method_name))
        .or_else(|| class_info.method_declaring_classes.get(method_name))
        .or_else(|| class_info.static_method_declaring_classes.get(method_name))
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Returns the class name that declares one AOT property attribute row.
fn eval_native_property_attribute_declaring_class<'a>(
    reflected_class: &'a str,
    class_info: &'a ClassInfo,
    property_name: &str,
) -> &'a str {
    class_info
        .property_declaring_classes
        .get(property_name)
        .or_else(|| {
            class_info
                .static_property_declaring_classes
                .get(property_name)
        })
        .map(String::as_str)
        .unwrap_or(reflected_class)
}

/// Adds instance method metadata for one class to eval signature registration.
fn collect_eval_native_instance_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        if method_name == "__construct" {
            continue;
        }
        let bridge_supported = class_method_visibility_bridge_supported(class_info, method_name)
            && method_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: false,
            signature: signature.clone(),
            bridge_supported,
        });
    }
}

/// Adds static method metadata for one class to eval signature registration.
fn collect_eval_native_static_methods(
    class_name: &str,
    class_info: &ClassInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = class_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        let bridge_supported =
            class_static_method_visibility_bridge_supported(class_info, method_name)
                && method_signature_can_bridge_with_eval(signature);
        registrations.push(EvalNativeMethodRegistration {
            class_name: class_name.to_string(),
            method_name: method_name.clone(),
            is_static: true,
            signature: signature.clone(),
            bridge_supported,
        });
    }
}

/// Adds interface instance-method metadata to eval signature registration.
fn collect_eval_native_interface_instance_methods(
    interface_name: &str,
    interface_info: &InterfaceInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = interface_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        registrations.push(EvalNativeMethodRegistration {
            class_name: eval_native_interface_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            )
            .to_string(),
            method_name: method_name.clone(),
            is_static: false,
            signature: signature.clone(),
            bridge_supported: false,
        });
    }
}

/// Adds interface static-method metadata to eval signature registration.
fn collect_eval_native_interface_static_methods(
    interface_name: &str,
    interface_info: &InterfaceInfo,
    registrations: &mut Vec<EvalNativeMethodRegistration>,
) {
    let mut methods = interface_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method_name, signature) in methods {
        registrations.push(EvalNativeMethodRegistration {
            class_name: eval_native_interface_static_method_declaring_interface(
                interface_name,
                interface_info,
                method_name,
            )
            .to_string(),
            method_name: method_name.clone(),
            is_static: true,
            signature: signature.clone(),
            bridge_supported: false,
        });
    }
}

/// Returns the interface name that declares one AOT interface instance method row.
fn eval_native_interface_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}

/// Returns the interface name that declares one AOT interface static method row.
fn eval_native_interface_static_method_declaring_interface<'a>(
    reflected_interface: &'a str,
    interface_info: &'a InterfaceInfo,
    method_name: &str,
) -> &'a str {
    interface_info
        .static_method_declaring_interfaces
        .get(method_name)
        .map(String::as_str)
        .unwrap_or(reflected_interface)
}

/// Returns true when a module function should expose metadata to eval fragments.
fn function_has_eval_metadata(function: &Function) -> bool {
    !function.flags.is_main && !function.name.starts_with('_')
}

/// Returns true when eval can dispatch a native function through the generated bridge.
fn function_signature_can_bridge_with_eval(function: &Function) -> bool {
    function
        .params
        .iter()
        .all(|param| !param.by_ref || eval_native_function_ref_param_supported(&param.php_type))
}

/// Returns true when a native function by-reference parameter can use eval bridge staging.
fn eval_native_function_ref_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Int
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Object(_)
            | PhpType::Str
    )
}

/// Returns true when eval can dispatch a native method through the generated bridge.
fn method_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_method_param_supported(ty))
        && eval_native_method_return_supported(&signature.return_type)
}

/// Returns true when eval can dispatch a native constructor through the generated bridge.
fn constructor_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_constructor_param_supported(ty))
}

/// Returns true when one native method argument type fits the eval method bridge.
fn eval_native_method_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native constructor argument type fits the eval bridge.
fn eval_native_constructor_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native method return type can be boxed back for eval.
fn eval_native_method_return_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Void
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Returns true when the indexed parameter is the signature's variadic slot.
fn signature_param_is_variadic(signature: &FunctionSig, index: usize, param_name: &str) -> bool {
    signature.variadic.as_deref().is_some_and(|variadic| {
        variadic == param_name
            || signature
                .params
                .get(index)
                .is_some_and(|(name, _)| name == variadic)
    })
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
        TypeExpr::False => Some("false".to_string()),
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
        PhpType::False => Some("false".to_string()),
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
fn eval_native_callable_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
) -> Option<EvalNativeCallableDefault> {
    eval_native_callable_default_at(expr, default_context, 0)
}

/// Converts a PHP default expression while preserving a recursion limit for constants.
fn eval_native_callable_default_at(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    if depth > MAX_NATIVE_DEFAULT_CONSTANT_DEPTH {
        return None;
    }
    eval_native_literal_default(expr)
        .or_else(|| eval_native_object_default(expr, default_context, depth))
        .or_else(|| eval_native_array_default(expr, default_context, depth))
        .or_else(|| eval_native_constant_expression_default(expr, default_context, depth))
}

/// Converts representable pure constant expressions into native eval defaults.
fn eval_native_constant_expression_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    match &expr.kind {
        ExprKind::ConstRef(name) => {
            eval_native_global_constant_default(default_context, name, depth + 1)
        }
        ExprKind::ClassConstant { receiver } => {
            eval_native_static_receiver_name(default_context, receiver)
                .map(EvalNativeCallableDefault::String)
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            eval_native_scoped_constant_default(default_context, receiver, name, depth + 1)
        }
        ExprKind::BinaryOp { left, op, right } => {
            eval_native_binary_expression_default(left, op, right, default_context, depth + 1)
        }
        ExprKind::Not(inner) => eval_native_default_truthy(&eval_native_callable_default_at(
            inner,
            default_context,
            depth + 1,
        )?)
        .map(|value| eval_native_bool_default(!value)),
        ExprKind::BitNot(inner) => eval_native_default_int(inner, default_context, depth + 1)
            .map(|value| eval_native_int_default(!value)),
        ExprKind::NullCoalesce { value, default } => {
            let value = eval_native_callable_default_at(value, default_context, depth + 1)?;
            if eval_native_default_is_null(&value) {
                eval_native_callable_default_at(default, default_context, depth + 1)
            } else {
                Some(value)
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            if eval_native_default_truthy(&eval_native_callable_default_at(
                condition,
                default_context,
                depth + 1,
            )?)? {
                eval_native_callable_default_at(then_expr, default_context, depth + 1)
            } else {
                eval_native_callable_default_at(else_expr, default_context, depth + 1)
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let value = eval_native_callable_default_at(value, default_context, depth + 1)?;
            if eval_native_default_truthy(&value)? {
                Some(value)
            } else {
                eval_native_callable_default_at(default, default_context, depth + 1)
            }
        }
        _ => None,
    }
}

/// Converts one supported binary constant expression into a native eval default.
fn eval_native_binary_expression_default(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            eval_native_numeric_binary_default(left, op, right, default_context, depth + 1)
        }
        BinOp::Mod => {
            let left = eval_native_default_int(left, default_context, depth + 1)?;
            let right = eval_native_default_int(right, default_context, depth + 1)?;
            (right != 0).then(|| eval_native_int_default(left % right))
        }
        BinOp::Concat => {
            let left = eval_native_default_string(left, default_context, depth + 1)?;
            let right = eval_native_default_string(right, default_context, depth + 1)?;
            Some(EvalNativeCallableDefault::String(format!("{left}{right}")))
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
            let left = eval_native_default_int(left, default_context, depth + 1)?;
            let right = eval_native_default_int(right, default_context, depth + 1)?;
            let value = match op {
                BinOp::BitAnd => left & right,
                BinOp::BitOr => left | right,
                BinOp::BitXor => left ^ right,
                _ => unreachable!("bitwise default operator was prefiltered"),
            };
            Some(eval_native_int_default(value))
        }
        BinOp::ShiftLeft | BinOp::ShiftRight => {
            let left = eval_native_default_int(left, default_context, depth + 1)?;
            let right =
                u32::try_from(eval_native_default_int(right, default_context, depth + 1)?).ok()?;
            let value = match op {
                BinOp::ShiftLeft => left.checked_shl(right),
                BinOp::ShiftRight => left.checked_shr(right),
                _ => unreachable!("shift default operator was prefiltered"),
            }?;
            Some(eval_native_int_default(value))
        }
        BinOp::And | BinOp::Or | BinOp::Xor => {
            let left = eval_native_default_truthy(&eval_native_callable_default_at(
                left,
                default_context,
                depth + 1,
            )?)?;
            let right = eval_native_default_truthy(&eval_native_callable_default_at(
                right,
                default_context,
                depth + 1,
            )?)?;
            let value = match op {
                BinOp::And => left && right,
                BinOp::Or => left || right,
                BinOp::Xor => left ^ right,
                _ => unreachable!("logical default operator was prefiltered"),
            };
            Some(eval_native_bool_default(value))
        }
        BinOp::NullCoalesce => {
            let left = eval_native_callable_default_at(left, default_context, depth + 1)?;
            if eval_native_default_is_null(&left) {
                eval_native_callable_default_at(right, default_context, depth + 1)
            } else {
                Some(left)
            }
        }
        _ => None,
    }
}

/// Converts one supported arithmetic expression into a native eval default.
fn eval_native_numeric_binary_default(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    if let (Some(left), Some(right)) = (
        eval_native_default_int(left, default_context, depth + 1),
        eval_native_default_int(right, default_context, depth + 1),
    ) {
        return match op {
            BinOp::Add => left.checked_add(right).map(eval_native_int_default),
            BinOp::Sub => left.checked_sub(right).map(eval_native_int_default),
            BinOp::Mul => left.checked_mul(right).map(eval_native_int_default),
            BinOp::Div if right != 0 => Some(eval_native_float_default(left as f64 / right as f64)),
            BinOp::Pow => {
                let value = (left as f64).powf(right as f64);
                value.is_finite().then(|| eval_native_float_default(value))
            }
            _ => None,
        };
    }

    let left = eval_native_default_numeric(left, default_context, depth + 1)?;
    let right = eval_native_default_numeric(right, default_context, depth + 1)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    value.is_finite().then(|| eval_native_float_default(value))
}

/// Builds one bool default metadata value.
fn eval_native_bool_default(value: bool) -> EvalNativeCallableDefault {
    EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_BOOL,
        payload: i64::from(value),
    }
}

/// Builds one int default metadata value.
fn eval_native_int_default(value: i64) -> EvalNativeCallableDefault {
    EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_INT,
        payload: value,
    }
}

/// Builds one float default metadata value.
fn eval_native_float_default(value: f64) -> EvalNativeCallableDefault {
    EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_FLOAT,
        payload: value.to_bits() as i64,
    }
}

/// Returns true when one default metadata value is PHP `null`.
fn eval_native_default_is_null(default: &EvalNativeCallableDefault) -> bool {
    matches!(
        default,
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_NULL,
            ..
        }
    )
}

/// Returns PHP truthiness for one representable native eval default.
fn eval_native_default_truthy(default: &EvalNativeCallableDefault) -> Option<bool> {
    match default {
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_NULL,
            ..
        } => Some(false),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_BOOL,
            payload,
        } => Some(*payload != 0),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_INT,
            payload,
        } => Some(*payload != 0),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload,
        } => Some(f64::from_bits(*payload as u64) != 0.0),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_EMPTY_ARRAY,
            ..
        } => Some(false),
        EvalNativeCallableDefault::String(value) => Some(!value.is_empty() && value != "0"),
        EvalNativeCallableDefault::Array(_) | EvalNativeCallableDefault::Object { .. } => None,
        EvalNativeCallableDefault::Scalar { .. } => None,
    }
}

/// Extracts an int value from one representable default expression.
fn eval_native_default_int(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<i64> {
    match eval_native_callable_default_at(expr, default_context, depth)? {
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_INT,
            payload,
        } => Some(payload),
        _ => None,
    }
}

/// Extracts a numeric value from one representable default expression.
fn eval_native_default_numeric(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<f64> {
    match eval_native_callable_default_at(expr, default_context, depth)? {
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_INT,
            payload,
        } => Some(payload as f64),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload,
        } => Some(f64::from_bits(payload as u64)),
        _ => None,
    }
}

/// Extracts a string value from one representable default expression.
fn eval_native_default_string(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<String> {
    match eval_native_callable_default_at(expr, default_context, depth)? {
        EvalNativeCallableDefault::String(value) => Some(value),
        _ => None,
    }
}

/// Converts scalar/string/empty-array defaults into the compact eval bridge default ABI.
fn eval_native_literal_default(expr: &Expr) -> Option<EvalNativeCallableDefault> {
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
        ExprKind::ArrayLiteral(elements) if elements.is_empty() => {
            Some(EvalNativeCallableDefault::Scalar {
                kind: NATIVE_DEFAULT_EMPTY_ARRAY,
                payload: 0,
            })
        }
        ExprKind::Negate(inner) => eval_native_callable_negated_default(inner),
        _ => None,
    }
}

/// Converts supported object-valued defaults into compact eval bridge metadata.
fn eval_native_object_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    let ExprKind::NewObject { class_name, args } = &expr.kind else {
        return None;
    };
    if args.len() > MAX_NATIVE_OBJECT_DEFAULT_ARGS {
        return None;
    }
    let mut default_args = Vec::with_capacity(args.len());
    for arg in args {
        default_args.push(eval_native_object_default_arg(
            arg,
            default_context,
            depth + 1,
        )?);
    }
    Some(EvalNativeCallableDefault::Object {
        class_name: class_name.as_canonical(),
        args: default_args,
    })
}

/// Converts one object-valued default constructor argument into bridge metadata.
fn eval_native_object_default_arg(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableObjectDefaultArg> {
    match &expr.kind {
        ExprKind::NamedArg { name, value } => Some(EvalNativeCallableObjectDefaultArg {
            name: Some(name.clone()),
            default: eval_native_callable_default_at(value, default_context, depth + 1)?,
        }),
        ExprKind::Spread(_) => None,
        _ => Some(EvalNativeCallableObjectDefaultArg {
            name: None,
            default: eval_native_callable_default_at(expr, default_context, depth + 1)?,
        }),
    }
}

/// Converts supported array-valued defaults into compact eval bridge metadata.
fn eval_native_array_default(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => {
            let mut default_elements = Vec::with_capacity(elements.len());
            for element in elements {
                if matches!(element.kind, ExprKind::Spread(_)) {
                    return None;
                }
                default_elements.push(EvalNativeCallableArrayDefaultElement {
                    key: None,
                    default: eval_native_callable_default_at(element, default_context, depth + 1)?,
                });
            }
            Some(EvalNativeCallableDefault::Array(default_elements))
        }
        ExprKind::ArrayLiteralAssoc(elements) => {
            let mut default_elements = Vec::with_capacity(elements.len());
            for (key, value) in elements {
                default_elements.push(EvalNativeCallableArrayDefaultElement {
                    key: Some(eval_native_array_default_key(
                        key,
                        default_context,
                        depth + 1,
                    )?),
                    default: eval_native_callable_default_at(value, default_context, depth + 1)?,
                });
            }
            Some(EvalNativeCallableDefault::Array(default_elements))
        }
        _ => None,
    }
}

/// Converts one supported static array key into bridge metadata.
fn eval_native_array_default_key(
    expr: &Expr,
    default_context: &EvalNativeDefaultContext<'_>,
    depth: usize,
) -> Option<EvalNativeCallableArrayDefaultKey> {
    if let Some(key) = eval_native_literal_array_default_key(expr) {
        return Some(key);
    }
    match eval_native_callable_default_at(expr, default_context, depth + 1)? {
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_NULL,
            ..
        } => Some(EvalNativeCallableArrayDefaultKey::String(String::new())),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_BOOL,
            payload,
        } => Some(EvalNativeCallableArrayDefaultKey::Int(
            (payload != 0) as i64,
        )),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_INT,
            payload,
        } => Some(EvalNativeCallableArrayDefaultKey::Int(payload)),
        EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_FLOAT,
            payload,
        } => Some(EvalNativeCallableArrayDefaultKey::Int(
            f64::from_bits(payload as u64) as i64,
        )),
        EvalNativeCallableDefault::String(value) => eval_native_string_array_default_key(&value),
        _ => None,
    }
}

/// Resolves and materializes one global constant default expression.
fn eval_native_global_constant_default(
    default_context: &EvalNativeDefaultContext<'_>,
    name: &str,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    let expr_kind = default_context
        .module
        .global_constants
        .get(name)
        .or_else(|| {
            default_context
                .module
                .global_constants
                .get(name.trim_start_matches('\\'))
        })
        .map(|(expr_kind, _)| expr_kind.clone())?;
    let expr = Expr::new(expr_kind, crate::span::Span::dummy());
    eval_native_callable_default_at(&expr, default_context, depth + 1)
}

/// Resolves and materializes one class-like constant default expression.
fn eval_native_scoped_constant_default(
    default_context: &EvalNativeDefaultContext<'_>,
    receiver: &StaticReceiver,
    constant_name: &str,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    let class_name = eval_native_static_receiver_name(default_context, receiver)?;
    if let Some((declaring_name, value)) =
        eval_native_class_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    if let Some((declaring_name, value)) =
        eval_native_interface_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    if let Some((declaring_name, value)) =
        eval_native_trait_constant_expr(default_context.module, &class_name, constant_name)
    {
        let nested_context =
            EvalNativeDefaultContext::for_class(default_context.module, declaring_name);
        return eval_native_callable_default_at(value, &nested_context, depth + 1);
    }
    None
}

/// Resolves `self`, `static`, `parent`, or a named receiver for default constants.
fn eval_native_static_receiver_name(
    default_context: &EvalNativeDefaultContext<'_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => {
            Some(name.as_canonical().trim_start_matches('\\').to_string())
        }
        StaticReceiver::Self_ | StaticReceiver::Static => {
            default_context.current_class.map(str::to_string)
        }
        StaticReceiver::Parent => {
            let current = default_context.current_class?;
            resolve_eval_native_default_class(default_context.module, current)
                .and_then(|(_, class_info)| class_info.parent.clone())
        }
    }
}

/// Looks up a class constant expression, including inherited parent classes.
fn eval_native_class_constant_expr<'a>(
    module: &'a Module,
    class_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let (resolved_name, class_info) = resolve_eval_native_default_class(module, class_name)?;
    if let Some(value) = class_info.constants.get(constant_name) {
        return Some((resolved_name, value));
    }
    for interface_name in &class_info.interfaces {
        if let Some(value) =
            eval_native_interface_constant_expr(module, interface_name, constant_name)
        {
            return Some(value);
        }
    }
    if let Some(parent_name) = class_info.parent.as_deref() {
        return eval_native_class_constant_expr(module, parent_name, constant_name);
    }
    None
}

/// Looks up an interface constant expression, including inherited interfaces.
fn eval_native_interface_constant_expr<'a>(
    module: &'a Module,
    interface_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![interface_name.to_string()];
    while let Some(name) = queue.pop() {
        let Some((resolved_name, interface_info)) =
            resolve_eval_native_default_interface(module, &name)
        else {
            continue;
        };
        if !visited.insert(php_symbol_key(resolved_name.trim_start_matches('\\'))) {
            continue;
        }
        if let Some(value) = interface_info.constants.get(constant_name) {
            return Some((resolved_name, value));
        }
        queue.extend(interface_info.parents.iter().cloned());
    }
    None
}

/// Looks up a direct trait constant expression by PHP-style trait name.
fn eval_native_trait_constant_expr<'a>(
    module: &'a Module,
    trait_name: &str,
    constant_name: &str,
) -> Option<(&'a str, &'a Expr)> {
    let trait_key = php_symbol_key(trait_name.trim_start_matches('\\'));
    let resolved_name = module
        .trait_table
        .names
        .iter()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == trait_key)?;
    let value = module
        .declared_trait_constants
        .get(resolved_name)
        .and_then(|constants| constants.get(constant_name))?;
    Some((resolved_name.as_str(), value))
}

/// Looks up class metadata by PHP-style case-insensitive name.
fn resolve_eval_native_default_class<'a>(
    module: &'a Module,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == class_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Looks up interface metadata by PHP-style case-insensitive name.
fn resolve_eval_native_default_interface<'a>(
    module: &'a Module,
    interface_name: &str,
) -> Option<(&'a str, &'a InterfaceInfo)> {
    let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
    module
        .interface_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == interface_key)
        .map(|(name, info)| (name.as_str(), info))
}

/// Converts one literal static array key into bridge metadata.
fn eval_native_literal_array_default_key(expr: &Expr) -> Option<EvalNativeCallableArrayDefaultKey> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(EvalNativeCallableArrayDefaultKey::Int(*value)),
        ExprKind::BoolLiteral(value) => {
            Some(EvalNativeCallableArrayDefaultKey::Int(i64::from(*value)))
        }
        ExprKind::FloatLiteral(value) => {
            Some(EvalNativeCallableArrayDefaultKey::Int(*value as i64))
        }
        ExprKind::StringLiteral(value) => eval_native_string_array_default_key(value),
        ExprKind::Null => Some(EvalNativeCallableArrayDefaultKey::String(String::new())),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(EvalNativeCallableArrayDefaultKey::Int),
            ExprKind::FloatLiteral(value) => {
                Some(EvalNativeCallableArrayDefaultKey::Int((-*value) as i64))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Normalizes one string default-array key to PHP's integer-key rules.
fn eval_native_string_array_default_key(value: &str) -> Option<EvalNativeCallableArrayDefaultKey> {
    if is_php_integer_array_key(value) {
        value
            .parse::<i64>()
            .ok()
            .map(EvalNativeCallableArrayDefaultKey::Int)
    } else {
        Some(EvalNativeCallableArrayDefaultKey::String(value.to_string()))
    }
}

/// Converts supported property defaults into the compact eval bridge default ABI.
fn eval_native_property_default(
    default: Option<&Expr>,
    is_declared: bool,
    is_abstract: bool,
    default_context: &EvalNativeDefaultContext<'_>,
) -> Option<EvalNativeCallableDefault> {
    if let Some(default) = default {
        return eval_native_literal_default(default)
            .or_else(|| eval_native_array_default(default, default_context, 0));
    }
    (!is_declared && !is_abstract).then_some(EvalNativeCallableDefault::Scalar {
        kind: NATIVE_DEFAULT_NULL,
        payload: 0,
    })
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

/// Encodes an object-valued native callable default for libelephc-magician.
fn encode_eval_native_object_default(default: &EvalNativeCallableDefault) -> Vec<u8> {
    let EvalNativeCallableDefault::Object { class_name, args } = default else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    encode_eval_native_default_string(&mut bytes, class_name);
    bytes.push(args.len() as u8);
    for arg in args {
        encode_eval_native_object_default_arg(&mut bytes, arg);
    }
    bytes
}

/// Encodes an array-valued native callable default for libelephc-magician.
fn encode_eval_native_array_default(default: &EvalNativeCallableDefault) -> Vec<u8> {
    let EvalNativeCallableDefault::Array(elements) = default else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(elements.len() as u32).to_le_bytes());
    for element in elements {
        encode_eval_native_array_default_element(&mut bytes, element);
    }
    bytes
}

/// Encodes one array-default element and its optional static key.
fn encode_eval_native_array_default_element(
    bytes: &mut Vec<u8>,
    element: &EvalNativeCallableArrayDefaultElement,
) {
    match &element.key {
        Some(EvalNativeCallableArrayDefaultKey::Int(value)) => {
            bytes.push(NATIVE_ARRAY_DEFAULT_KEY_INT);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Some(EvalNativeCallableArrayDefaultKey::String(value)) => {
            bytes.push(NATIVE_ARRAY_DEFAULT_KEY_STRING);
            encode_eval_native_default_string(bytes, value);
        }
        None => bytes.push(NATIVE_ARRAY_DEFAULT_KEY_AUTO),
    }
    encode_eval_native_object_default_arg_value(bytes, &element.default);
}

/// Encodes one object-default constructor argument for libelephc-magician.
fn encode_eval_native_object_default_arg(
    bytes: &mut Vec<u8>,
    arg: &EvalNativeCallableObjectDefaultArg,
) {
    if let Some(name) = &arg.name {
        bytes.push(NATIVE_OBJECT_DEFAULT_ARG_NAMED);
        encode_eval_native_default_string(bytes, name);
    }
    encode_eval_native_object_default_arg_value(bytes, &arg.default);
}

/// Encodes one object-default constructor argument value for libelephc-magician.
fn encode_eval_native_object_default_arg_value(
    bytes: &mut Vec<u8>,
    default: &EvalNativeCallableDefault,
) {
    match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
            bytes.extend_from_slice(&(*kind as u64).to_le_bytes());
            bytes.extend_from_slice(&(*payload as u64).to_le_bytes());
        }
        EvalNativeCallableDefault::String(value) => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_STRING);
            encode_eval_native_default_string(bytes, value);
        }
        EvalNativeCallableDefault::Object { .. } => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_OBJECT);
            bytes.extend_from_slice(&encode_eval_native_object_default(default));
        }
        EvalNativeCallableDefault::Array(_) => {
            bytes.push(NATIVE_OBJECT_DEFAULT_ARG_ARRAY);
            bytes.extend_from_slice(&encode_eval_native_array_default(default));
        }
    }
}

/// Encodes one UTF-8 string with a little-endian u32 byte-length prefix.
fn encode_eval_native_default_string(bytes: &mut Vec<u8>, value: &str) {
    let len = u32::try_from(value.len()).unwrap_or(u32::MAX);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

/// Returns true when eval can enforce this instance method visibility in the bridge.
fn class_method_visibility_bridge_supported(class_info: &ClassInfo, method_name: &str) -> bool {
    class_info
        .method_visibilities
        .get(method_name)
        .is_none_or(|visibility| {
            matches!(
                visibility,
                Visibility::Public | Visibility::Protected | Visibility::Private
            )
        })
}

/// Returns true when eval can enforce this static method visibility in the bridge.
fn class_static_method_visibility_bridge_supported(
    class_info: &ClassInfo,
    method_name: &str,
) -> bool {
    class_info
        .static_method_visibilities
        .get(method_name)
        .is_none_or(|visibility| {
            matches!(
                visibility,
                Visibility::Public | Visibility::Protected | Visibility::Private
            )
        })
}

/// Emits one native-function registration call into the just-created eval context.
fn register_eval_native_function(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeFunctionRegistration,
) -> Result<()> {
    let invoker_label = emit_eval_native_function_invoker_inline(ctx, &registration.signature);
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
    register_eval_native_function_bridge_support(
        ctx,
        context_offset,
        &name_label,
        name_len,
        registration.bridge_supported,
    );
    let param_type_specs = eval_native_callable_param_type_specs(&registration.signature);
    for (index, (param_name, _)) in registration.signature.params.iter().enumerate() {
        register_eval_native_function_param(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            param_name,
        );
        register_eval_native_function_param_flags(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            signature_param_is_variadic(&registration.signature, index, param_name),
        );
        if let Some(type_spec) = param_type_specs.get(index).and_then(Option::as_deref) {
            register_eval_native_function_param_type(
                ctx,
                context_offset,
                &name_label,
                name_len,
                index,
                type_spec,
            );
        }
    }
    let default_context = EvalNativeDefaultContext::global(ctx.module);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default
            .as_ref()
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        else {
            continue;
        };
        register_eval_native_function_param_default(
            ctx,
            context_offset,
            &name_label,
            name_len,
            index,
            &default,
        );
    }
    if let Some(type_spec) = eval_native_callable_return_type_spec(&registration.signature) {
        register_eval_native_function_return_type(
            ctx,
            context_offset,
            &name_label,
            name_len,
            &type_spec,
        );
    }
    Ok(())
}

/// Emits an eval-safe descriptor invoker for a registered native free function.
fn emit_eval_native_function_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
) -> String {
    let label = ctx.next_label("eval_callable_invoker");
    let done_label = ctx.next_label("eval_callable_invoker_done");
    let captures: [(String, PhpType, bool); 0] = [];
    let invoker = RuntimeCallableInvoker {
        label: &label,
        sig,
        captures: &captures,
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker_with_exception_boundary(
        ctx.emitter,
        ctx.data,
        &invoker,
    );
    ctx.emitter.label(&done_label);
    label
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
    register_eval_native_method_bridge_support(
        ctx,
        context_offset,
        &method_key_label,
        method_key_len,
        registration.is_static,
        registration.bridge_supported,
    );
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
        register_eval_native_method_param_flags(
            ctx,
            context_offset,
            &method_key_label,
            method_key_len,
            registration.is_static,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            signature_param_is_variadic(&registration.signature, index, param_name),
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
    let default_context = EvalNativeDefaultContext::for_class(ctx.module, &registration.class_name);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default
            .as_ref()
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        else {
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

/// Emits one native method bridge-support registration call.
fn register_eval_native_method_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    bridge_supported: bool,
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
        if bridge_supported { 1 } else { 0 },
    );
    let symbol = if is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method_bridge_support")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method_bridge_support")
    };
    abi::emit_call_label(ctx.emitter, &symbol);
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

/// Emits one native method parameter-flags registration call.
fn register_eval_native_method_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    method_key_label: &str,
    method_key_len: usize,
    is_static: bool,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
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
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        if is_by_ref { 1 } else { 0 },
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        if is_variadic { 1 } else { 0 },
    );
    let symbol = if is_static {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_static_method_param_flags")
    } else {
        ctx.emitter
            .target
            .extern_symbol("__elephc_eval_register_native_method_param_flags")
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
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                    "__elephc_eval_register_native_static_method_param_default_object",
                )
            } else {
                ctx.emitter
                    .target
                    .extern_symbol("__elephc_eval_register_native_method_param_default_object")
            }
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                    "__elephc_eval_register_native_static_method_param_default_array",
                )
            } else {
                ctx.emitter
                    .target
                    .extern_symbol("__elephc_eval_register_native_method_param_default_array")
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
    register_eval_native_constructor_bridge_support(
        ctx,
        context_offset,
        &class_name_label,
        class_name_len,
        registration.bridge_supported,
    );
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
        register_eval_native_constructor_param_flags(
            ctx,
            context_offset,
            &class_name_label,
            class_name_len,
            index,
            registration
                .signature
                .ref_params
                .get(index)
                .copied()
                .unwrap_or(false),
            signature_param_is_variadic(&registration.signature, index, param_name),
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
    let default_context = EvalNativeDefaultContext::for_class(ctx.module, &registration.class_name);
    for (index, default) in registration.signature.defaults.iter().enumerate() {
        let Some(default) = default
            .as_ref()
            .and_then(|expr| eval_native_callable_default(expr, &default_context))
        else {
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

/// Emits one native constructor bridge-support registration call.
fn register_eval_native_constructor_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    bridge_supported: bool,
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
        if bridge_supported { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_bridge_support");
    abi::emit_call_label(ctx.emitter, &symbol);
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

/// Emits one native interface-property metadata registration call into the eval context.
fn register_eval_native_interface_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeInterfacePropertyRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}::{}",
        registration.interface_name,
        registration.declaring_interface_name,
        registration.property_name
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
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        flags,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_interface_property");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native abstract-property metadata registration call into the eval context.
fn register_eval_native_abstract_property(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeAbstractPropertyRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}::{}",
        registration.class_name, registration.declaring_class_name, registration.property_name
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
    let mut flags = 0;
    if registration.requires_get {
        flags |= NATIVE_PROPERTY_REQUIRES_GET;
    }
    if registration.requires_set {
        flags |= NATIVE_PROPERTY_REQUIRES_SET;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        flags,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_abstract_property");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native property-default metadata registration call into the eval context.
fn register_eval_native_property_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativePropertyDefaultRegistration,
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
    let symbol = match &registration.default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                *kind,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                *payload,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_scalar")
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_string")
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(&registration.default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_array")
        }
        EvalNativeCallableDefault::Object { .. } => return,
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native member-attribute metadata registration call into the eval context.
fn register_eval_native_member_attribute(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeMemberAttributeRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let record = eval_native_member_attribute_record(registration);
    let (record_label, record_len) = ctx.data.add_string(&record);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &record_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        record_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_member_attribute");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Encodes one member-attribute registration record for the eval bridge ABI.
fn eval_native_member_attribute_record(
    registration: &EvalNativeMemberAttributeRegistration,
) -> Vec<u8> {
    let mut record = Vec::new();
    record.push(registration.owner_kind);
    let member_key = if registration.owner_kind == NATIVE_MEMBER_ATTRIBUTE_CLASS {
        registration.class_name.clone()
    } else {
        format!("{}::{}", registration.class_name, registration.member_name)
    };
    eval_native_member_attribute_push_string(&mut record, &member_key);
    eval_native_member_attribute_push_string(&mut record, &registration.attribute_name);
    match &registration.attribute_args {
        Some(args) => {
            record.push(NATIVE_ATTRIBUTE_ARGS_SUPPORTED);
            eval_native_member_attribute_push_u32(&mut record, args.len());
            for arg in args {
                eval_native_member_attribute_push_entry(&mut record, arg);
            }
        }
        None => record.push(NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED),
    }
    record
}

/// Returns true when an attribute argument list can be encoded for eval registration.
fn eval_native_member_attribute_args_supported(args: &[AttrArgEntry]) -> bool {
    args.iter()
        .all(|entry| eval_native_member_attribute_value_supported(&entry.value))
}

/// Returns true when one attribute argument value can be encoded for eval registration.
fn eval_native_member_attribute_value_supported(value: &AttrArgValue) -> bool {
    match value {
        AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(_, _) => false,
        AttrArgValue::Array(elements) => eval_native_member_attribute_args_supported(elements),
        AttrArgValue::Null
        | AttrArgValue::Bool(_)
        | AttrArgValue::Int(_)
        | AttrArgValue::Float(_)
        | AttrArgValue::Str(_) => true,
    }
}

/// Encodes one keyed attribute argument entry into a member-attribute registration record.
fn eval_native_member_attribute_push_entry(record: &mut Vec<u8>, entry: &AttrArgEntry) {
    match &entry.key {
        Some(AttrKey::Str(name)) => {
            record.push(NATIVE_ATTRIBUTE_ARG_NAMED);
            eval_native_member_attribute_push_string(record, name);
            eval_native_member_attribute_push_arg(record, &entry.value);
        }
        Some(AttrKey::Int(_)) | None => eval_native_member_attribute_push_arg(record, &entry.value),
    }
}

/// Encodes one attribute argument value into a member-attribute registration record.
fn eval_native_member_attribute_push_arg(record: &mut Vec<u8>, arg: &AttrArgValue) {
    match arg {
        AttrArgValue::Null => record.push(NATIVE_ATTRIBUTE_ARG_NULL),
        AttrArgValue::Bool(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_BOOL);
            record.push(u8::from(*value));
        }
        AttrArgValue::Int(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_INT);
            record.extend_from_slice(&value.to_le_bytes());
        }
        AttrArgValue::Float(bits) => {
            record.push(NATIVE_ATTRIBUTE_ARG_FLOAT);
            record.extend_from_slice(&bits.to_le_bytes());
        }
        AttrArgValue::Str(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_STRING);
            eval_native_member_attribute_push_string(record, value);
        }
        AttrArgValue::Array(elements) => {
            record.push(NATIVE_ATTRIBUTE_ARG_ARRAY);
            eval_native_member_attribute_push_u32(record, elements.len());
            for element in elements {
                eval_native_member_attribute_push_entry(record, element);
            }
        }
        AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(_, _) => {
            record.push(NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED);
        }
    }
}

/// Encodes one length-prefixed UTF-8 string into a member-attribute registration record.
fn eval_native_member_attribute_push_string(record: &mut Vec<u8>, value: &str) {
    eval_native_member_attribute_push_u32(record, value.len());
    record.extend_from_slice(value.as_bytes());
}

/// Encodes one little-endian u32 length into a member-attribute registration record.
fn eval_native_member_attribute_push_u32(record: &mut Vec<u8>, value: usize) {
    let value = u32::try_from(value).unwrap_or(u32::MAX);
    record.extend_from_slice(&value.to_le_bytes());
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

/// Emits one native constructor parameter-flags registration call.
fn register_eval_native_constructor_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    class_name_label: &str,
    class_name_len: usize,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
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
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        if is_by_ref { 1 } else { 0 },
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        if is_variadic { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_constructor_param_flags");
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
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                .extern_symbol("__elephc_eval_register_native_constructor_param_default_object")
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                .extern_symbol("__elephc_eval_register_native_constructor_param_default_array")
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

/// Emits one native-function bridge-support registration call.
fn register_eval_native_function_bridge_support(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    bridge_supported: bool,
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
        if bridge_supported { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_bridge_support");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function parameter-flags registration call.
fn register_eval_native_function_param_flags(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    is_by_ref: bool,
    is_variadic: bool,
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
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        if is_by_ref { 1 } else { 0 },
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        if is_variadic { 1 } else { 0 },
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_param_flags");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function parameter-type registration call.
fn register_eval_native_function_param_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    type_spec: &str,
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
        .extern_symbol("__elephc_eval_register_native_function_param_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native-function return-type registration call.
fn register_eval_native_function_return_type(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    type_spec: &str,
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_function_return_type");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native function parameter-default registration call.
fn register_eval_native_function_param_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    function_name_label: &str,
    function_name_len: usize,
    param_index: usize,
    default: &EvalNativeCallableDefault,
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
                .extern_symbol("__elephc_eval_register_native_function_param_default_scalar")
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
                .extern_symbol("__elephc_eval_register_native_function_param_default_string")
        }
        EvalNativeCallableDefault::Object { .. } => {
            let spec = encode_eval_native_object_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                .extern_symbol("__elephc_eval_register_native_function_param_default_object")
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
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
                .extern_symbol("__elephc_eval_register_native_function_param_default_array")
        }
    };
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
    ctx.emitter.instruction("cmp x12, x10");                                    // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("b.hs {}", missing));                      // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x12, #4");                                // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add x11, x11, x12");                               // select the called-class metadata row
    ctx.emitter.instruction("ldr x1, [x11]");                                   // load the called-class name pointer
    ctx.emitter.instruction("ldr x2, [x11, #8]");                               // load the called-class name length
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "x1", "_class_name_missing");
    ctx.emitter.instruction("mov x2, #0");                                      // empty called-class name triggers lexical fallback in eval
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the x86_64 class-id table lookup for eval's called class.
fn emit_eval_called_class_name_result_x86_64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("eval_called_class_missing");
    let done = ctx.next_label("eval_called_class_done");
    emit_eval_late_static_class_id_to_reg(ctx, "r8")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_class_name_count", 0);
    ctx.emitter.instruction("cmp r8, r9");                                      // reject called-class ids outside the class-name table
    ctx.emitter.instruction(&format!("jae {}", missing));                       // fall back to the lexical eval class when metadata is missing
    abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
    ctx.emitter.instruction("shl r8, 4");                                       // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add r10, r8");                                     // select the called-class metadata row
    ctx.emitter.instruction("mov rax, QWORD PTR [r10]");                        // load the called-class name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                    // load the called-class name length
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "rax", "_class_name_missing");
    ctx.emitter.instruction("mov rdx, 0");                                      // empty called-class name triggers lexical fallback in eval
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

/// Keeps only eval-sync locals whose PHP name appears in `names`.
fn filter_eval_sync_locals_by_name(
    locals: Vec<EvalSyncLocal>,
    names: &BTreeSet<String>,
) -> Vec<EvalSyncLocal> {
    locals
        .into_iter()
        .filter(|local| names.contains(&local.name))
        .collect()
}

/// Returns true when a local name is backed by program-global storage during eval.
fn local_uses_eval_global_sync(ctx: &FunctionContext<'_>, name: Option<&str>) -> bool {
    name.is_some_and(|name| main_name_uses_eval_global_scope(ctx, name))
}

/// Returns true when a main-scope name has actual EIR global storage to synchronize.
fn main_name_uses_eval_global_scope(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.is_main && eval_sync_global_type(ctx, name).is_some()
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

/// Keeps only eval-sync globals whose PHP name appears in `names`.
fn filter_eval_sync_globals_by_name(
    globals: Vec<EvalSyncGlobal>,
    names: &BTreeSet<String>,
) -> Vec<EvalSyncGlobal> {
    globals
        .into_iter()
        .filter(|global| names.contains(&global.name))
        .collect()
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
    let is_typed_superglobal = ctx.module.web && crate::superglobals::is_superglobal(name);
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
            // Only real global storage instructions make a name a program
            // global; eval scope ops reference names through the same data
            // pool without any global storage behind them.
            if !matches!(inst.op, Op::LoadGlobal | Op::StoreGlobal) {
                continue;
            }
            if !is_typed_superglobal {
                // Regular globals always hold one boxed Mixed word (see
                // `lower_store_global`); store operands carry narrower source
                // types, so per-instruction inference would reject globals
                // written as scalars and read back as Mixed after a barrier.
                return Some(PhpType::Mixed);
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
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
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

/// Flushes global-backed variables into the local eval scope for scope-read EIR AOT.
fn flush_eval_globals_to_local_scope(ctx: &mut FunctionContext<'_>, globals: &[EvalSyncGlobal]) {
    for global in globals {
        load_global_to_result(ctx, global);
        if !matches!(global.ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &global.ty);
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        emit_eval_scope_set_name(ctx, &global.name, scope_set_flags_for_type(&global.ty));
    }
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

/// Calls `__elephc_eval_scope_set` for a boxed value identified by a static name.
fn emit_eval_scope_set_name(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
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

/// Reloads synchronized program globals from the local eval scope after EIR eval AOT.
fn reload_eval_globals_from_local_scope(
    ctx: &mut FunctionContext<'_>,
    globals: &[EvalSyncGlobal],
) -> Result<()> {
    for global in globals {
        emit_eval_scope_get_name(ctx, &global.name, 0, 8);
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

/// Calls `__elephc_eval_scope_get` for a static name and caller-provided scratch slots.
fn emit_eval_scope_get_name(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    out_cell_offset: usize,
    out_flags_offset: usize,
) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    load_eval_scope_to_arg(ctx, 0);
    let name_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &name_label);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let out_cell_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_temporary_stack_address(ctx.emitter, out_cell_arg, out_cell_offset);
    let out_flags_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    abi::emit_temporary_stack_address(ctx.emitter, out_flags_arg, out_flags_offset);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_scope_get");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_eval_status_check(ctx);
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
    emit_branch_if_scope_entry_missing_at(ctx, 8, label);
}

/// Branches to `label` when the scope-get flags at `flags_offset` do not mark a visible value.
fn emit_branch_if_scope_entry_missing_at(
    ctx: &mut FunctionContext<'_>,
    flags_offset: usize,
    label: &str,
) {
    let flags_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flags_reg, flags_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("tst {}, #{}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("b.eq {}", label));                // skip reload when eval unset or omitted the local
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_PRESENT)); // check whether eval left the local visible
            ctx.emitter.instruction(&format!("je {}", label));                  // skip reload when eval unset or omitted the local
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
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Objects, arrays, and hashes are heap pointers boxed in the
            // scope cell; unbox and store the raw payload pointer.
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let payload_reg = match ctx.emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "rdi",
            };
            let result_reg = abi::int_result_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, payload_reg)); // move the unboxed heap pointer into the local-store result register
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
            ctx.emitter.instruction(&format!("b.eq {}", skip));                 // borrowed scope entries can be copied back without retaining
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flags_reg, EVAL_SCOPE_FLAG_OWNED)); // check whether the scope keeps its own Mixed-cell owner
            ctx.emitter.instruction(&format!("je {}", skip));                   // borrowed scope entries can be copied back without retaining
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
        PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // Heap-pointer locals fall back to the null pointer when eval
            // removed the entry, matching the object fallback.
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
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching eval status handler
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching eval status handler
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
            ctx.emitter.instruction("mov x0, #2");                              // write the eval runtime diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the eval runtime diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the eval runtime diagnostic before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}
