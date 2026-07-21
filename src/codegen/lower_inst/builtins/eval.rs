//! Purpose:
//! Lowers PHP `eval()` calls to the optional libelephc-magician bridge ABI.
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::codegen::eval_ref_arg_helpers::eval_signature_ref_params_supported;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_callable_invoker::RuntimeCallableInvoker;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_value_as_mixed, CodegenIrError, Result,
};
use crate::ir::{Function, Immediate, Instruction, LocalKind, LocalSlotId, Module, Op, ValueId};
use crate::names::{function_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::{
    BinOp, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::{
    is_php_integer_array_key, AttrArgEntry, AttrArgValue, AttrKey, ClassInfo, FunctionSig,
    InterfaceInfo, PhpType, PropertyHookContract,
};

use super::super::super::context::FunctionContext;
use super::super::{
    expect_data, expect_global_name, expect_operand, function_signature_from_eir, predicates,
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
const EVAL_LOCAL_SCALAR_SLOT_BYTES: usize = 32;
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

/// Straight-line literal eval instruction that can be emitted without the interpreter.
enum EvalLiteralAotInst {
    Echo(EvalLiteralAotExpr),
    Store {
        name: String,
        value: EvalLiteralAotExpr,
    },
    Return(EvalLiteralAotExpr),
}

/// Boxed-Mixed expression accepted by the literal eval AOT path.
enum EvalLiteralAotExpr {
    Scalar(EvalLiteralAotScalar),
    LoadVar(String),
    Binary {
        op: EvalLiteralAotBinaryOp,
        left: Box<EvalLiteralAotExpr>,
        right: Box<EvalLiteralAotExpr>,
    },
}

/// Runtime-backed binary operation accepted by the literal eval AOT path.
enum EvalLiteralAotBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
}

/// Scalar value accepted by the first conservative literal-eval AOT subset.
#[derive(Clone)]
enum EvalLiteralAotScalar {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// Parsed and validated literal eval fragment ready for direct code emission.
enum EvalLiteralAotProgram {
    Boxed(EvalLiteralBoxedAotProgram),
    LocalScalar(EvalLocalScalarAotProgram),
}

/// Parsed boxed-Mixed literal eval fragment for scope-oriented direct emission.
struct EvalLiteralBoxedAotProgram {
    instructions: Vec<EvalLiteralAotInst>,
    scope_reads: BTreeSet<String>,
    scope_writes: BTreeSet<String>,
    has_scope_writes: bool,
    has_scope_access: bool,
}

/// Local scalar eval statement emitted as stack-slot native code before final scope flush.
enum EvalLocalScalarStmt {
    Noop,
    Echo(EvalLocalScalarExpr),
    Store {
        name: String,
        value: EvalLocalScalarExpr,
    },
    If {
        branches: Vec<(EvalLocalScalarExpr, Vec<EvalLocalScalarStmt>)>,
        else_body: Vec<EvalLocalScalarStmt>,
    },
    While {
        condition: EvalLocalScalarExpr,
        body: Vec<EvalLocalScalarStmt>,
    },
    DoWhile {
        body: Vec<EvalLocalScalarStmt>,
        condition: EvalLocalScalarExpr,
    },
    For {
        init: Option<Box<EvalLocalScalarStmt>>,
        condition: Option<EvalLocalScalarExpr>,
        update: Option<Box<EvalLocalScalarStmt>>,
        body: Vec<EvalLocalScalarStmt>,
    },
    Switch {
        subject: EvalLocalScalarExpr,
        cases: Vec<(Vec<EvalLocalScalarExpr>, Vec<EvalLocalScalarStmt>)>,
        default: Vec<EvalLocalScalarStmt>,
        default_index: Option<usize>,
    },
    Break(usize),
    Continue(usize),
    Return(Option<EvalLocalScalarExpr>),
}

/// Local scalar expression accepted by the control-flow AOT subset.
struct EvalLocalScalarExpr {
    kind: EvalLocalScalarExprKind,
    ty: EvalLocalScalarType,
}

/// Expression payload for the local scalar AOT subset.
enum EvalLocalScalarExprKind {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    LoadVar(String),
    Isset(Vec<String>),
    EmptyVar(String),
    Negate(Box<EvalLocalScalarExpr>),
    BitNot(Box<EvalLocalScalarExpr>),
    Not(Box<EvalLocalScalarExpr>),
    Print(Box<EvalLocalScalarExpr>),
    Ternary {
        condition: Box<EvalLocalScalarExpr>,
        then_expr: Box<EvalLocalScalarExpr>,
        else_expr: Box<EvalLocalScalarExpr>,
    },
    Binary {
        op: EvalLocalScalarBinaryOp,
        left: Box<EvalLocalScalarExpr>,
        right: Box<EvalLocalScalarExpr>,
    },
    StaticFunctionCall {
        name: String,
        args: Vec<EvalLocalScalarExpr>,
    },
}

/// Runtime scalar type tracked by the local eval AOT subset.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EvalLocalScalarType {
    Null,
    Int,
    Float,
    Bool,
    String,
}

/// Binary operations accepted by the local scalar AOT subset.
enum EvalLocalScalarBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Eq,
    NotEq,
    And,
    Or,
    Concat,
}

/// Parsed and analyzed local-scalar eval fragment ready for native control-flow emission.
struct EvalLocalScalarAotProgram {
    statements: Vec<EvalLocalScalarStmt>,
    locals: BTreeMap<String, usize>,
    local_types: BTreeMap<String, EvalLocalScalarType>,
    scratch_slots: usize,
}

/// Mutable compile-time state for local scalar eval eligibility analysis.
struct EvalLocalScalarAnalysis {
    locals: BTreeMap<String, usize>,
    local_types: BTreeMap<String, EvalLocalScalarType>,
    max_scratch_slots: usize,
}

/// Break and continue targets for one nested local AOT loop.
struct EvalLocalLoopLabels {
    break_label: String,
    continue_label: String,
}

/// Selects how local scalar AOT boxes eval return values.
#[derive(Clone, Copy)]
enum EvalLocalScalarBoxing {
    EvalRuntime,
    CoreRuntime,
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

/// Lowers `eval($code)` to the eval bridge ABI and leaves the eval return cell in result registers.
pub(super) fn lower_eval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "eval", 1)?;
    if let Some(fragment) = eval_literal_fragment(ctx, inst)? {
        if crate::eval_aot::literal_fragment_direct_local_read_write_writes(&fragment).is_none()
            && lower_eval_literal_eir_function(ctx, inst, &fragment)?
        {
            return Ok(());
        }
    }
    if let Some(program) = eval_literal_aot_program(ctx, inst)? {
        if lower_eval_literal_aot(ctx, inst, &program)? {
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
    lower_eval_literal_scope_read_eir_function(ctx, inst, fragment)
}

/// Calls a pre-lowered internal EIR function that reads from the eval scope.
fn lower_eval_literal_scope_read_eir_function(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fragment: &str,
) -> Result<bool> {
    let function_name = crate::eval_aot::eir_scope_read_function_name(fragment);
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
        .comment("eval literal AOT compiled EIR function with scope reads");
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
        EvalScopeReadParamSource::Null => emit_eval_local_scalar_core_null_cell(ctx),
    }
    Ok(())
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

/// Parses an `EvalLiteralCall` payload into the conservative scalar AOT subset.
fn eval_literal_aot_program(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<EvalLiteralAotProgram>> {
    let Some(fragment) = eval_literal_fragment(ctx, inst)? else {
        return Ok(None);
    };
    Ok(parse_eval_literal_aot_program(&fragment))
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

/// Parses a PHP eval fragment and accepts only side-effect-free scalar statements plus scalar stores.
fn parse_eval_literal_aot_program(fragment: &str) -> Option<EvalLiteralAotProgram> {
    let program = crate::eval_aot::parse_literal_fragment(fragment)?;
    if let Some(local_scalar) = parse_eval_local_scalar_aot_program(&program) {
        return Some(EvalLiteralAotProgram::LocalScalar(local_scalar));
    }
    if let Some(boxed) = parse_eval_literal_boxed_aot_program(&program) {
        return Some(EvalLiteralAotProgram::Boxed(boxed));
    }
    None
}

/// Parses a PHP eval fragment into the boxed-Mixed AOT subset.
fn parse_eval_literal_boxed_aot_program(program: &[Stmt]) -> Option<EvalLiteralBoxedAotProgram> {
    let mut instructions = Vec::new();
    let mut terminated = false;
    for stmt in program {
        if terminated {
            break;
        }
        terminated = push_eval_literal_aot_stmt(stmt, &mut instructions)?;
    }
    if !terminated {
        instructions.push(EvalLiteralAotInst::Return(EvalLiteralAotExpr::Scalar(
            EvalLiteralAotScalar::Null,
        )));
    }
    let has_scope_writes = instructions
        .iter()
        .any(|inst| matches!(inst, EvalLiteralAotInst::Store { .. }));
    let has_scope_access = instructions
        .iter()
        .any(EvalLiteralAotInst::has_scope_access);
    let scope_reads = instructions
        .iter()
        .flat_map(EvalLiteralAotInst::scope_reads)
        .collect();
    let scope_writes = instructions
        .iter()
        .filter_map(EvalLiteralAotInst::scope_write)
        .collect();
    Some(EvalLiteralBoxedAotProgram {
        instructions,
        scope_reads,
        scope_writes,
        has_scope_writes,
        has_scope_access,
    })
}

/// Parses a PHP eval fragment into the local int/bool control-flow AOT subset.
fn parse_eval_local_scalar_aot_program(program: &[Stmt]) -> Option<EvalLocalScalarAotProgram> {
    let mut analysis = EvalLocalScalarAnalysis {
        locals: BTreeMap::new(),
        local_types: BTreeMap::new(),
        max_scratch_slots: 0,
    };
    let mut assigned = BTreeSet::new();
    let statements = parse_eval_local_scalar_block(program, &mut analysis, &mut assigned, 0, true)?;
    Some(EvalLocalScalarAotProgram {
        statements,
        locals: analysis.locals,
        local_types: analysis.local_types,
        scratch_slots: analysis.max_scratch_slots.max(1),
    })
}

/// Parses a statement block for the local scalar AOT subset.
fn parse_eval_local_scalar_block(
    statements: &[Stmt],
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    loop_depth: usize,
    allow_type_changes: bool,
) -> Option<Vec<EvalLocalScalarStmt>> {
    let mut out = Vec::new();
    let mut terminated = false;
    for stmt in statements {
        if terminated {
            break;
        }
        let (local_stmt, terminates_block) =
            parse_eval_local_scalar_stmt(stmt, analysis, assigned, loop_depth, allow_type_changes)?;
        out.push(local_stmt);
        terminated = terminates_block;
    }
    Some(out)
}

/// Parses one statement for the local scalar AOT subset.
fn parse_eval_local_scalar_stmt(
    stmt: &Stmt,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    loop_depth: usize,
    allow_type_changes: bool,
) -> Option<(EvalLocalScalarStmt, bool)> {
    match &stmt.kind {
        StmtKind::Echo(expr) => {
            let expr = eval_local_scalar_echo_expr(expr, analysis, assigned)?;
            Some((EvalLocalScalarStmt::Echo(expr), false))
        }
        StmtKind::Assign { name, value } => {
            let value = eval_local_scalar_value_expr(value, analysis, assigned)?;
            analysis.ensure_local(name, value.ty, allow_type_changes)?;
            assigned.insert(name.clone());
            Some((
                EvalLocalScalarStmt::Store {
                    name: name.clone(),
                    value,
                },
                false,
            ))
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut branches = Vec::new();
            let condition = eval_local_scalar_condition_expr(condition, analysis, assigned)?;
            let mut then_assigned = assigned.clone();
            let then_body = parse_eval_local_scalar_block(
                then_body,
                analysis,
                &mut then_assigned,
                loop_depth,
                false,
            )?;
            branches.push((condition, then_body));
            for (elseif_condition, elseif_body) in elseif_clauses {
                let condition =
                    eval_local_scalar_condition_expr(elseif_condition, analysis, assigned)?;
                let mut elseif_assigned = assigned.clone();
                let body = parse_eval_local_scalar_block(
                    elseif_body,
                    analysis,
                    &mut elseif_assigned,
                    loop_depth,
                    false,
                )?;
                branches.push((condition, body));
            }
            let else_body = if let Some(else_body) = else_body {
                let mut else_assigned = assigned.clone();
                parse_eval_local_scalar_block(
                    else_body,
                    analysis,
                    &mut else_assigned,
                    loop_depth,
                    false,
                )?
            } else {
                Vec::new()
            };
            Some((
                EvalLocalScalarStmt::If {
                    branches,
                    else_body,
                },
                false,
            ))
        }
        StmtKind::While { condition, body } => {
            let condition = eval_local_scalar_condition_expr(condition, analysis, assigned)?;
            let mut body_assigned = assigned.clone();
            let body = parse_eval_local_scalar_block(
                body,
                analysis,
                &mut body_assigned,
                loop_depth + 1,
                false,
            )?;
            Some((EvalLocalScalarStmt::While { condition, body }, false))
        }
        StmtKind::DoWhile { body, condition } => {
            let mut body_assigned = assigned.clone();
            let body = parse_eval_local_scalar_block(
                body,
                analysis,
                &mut body_assigned,
                loop_depth + 1,
                false,
            )?;
            let condition = eval_local_scalar_condition_expr(condition, analysis, &body_assigned)?;
            Some((EvalLocalScalarStmt::DoWhile { body, condition }, false))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let init = if let Some(init) = init.as_deref() {
                Some(Box::new(parse_eval_local_scalar_for_clause(
                    init,
                    analysis,
                    assigned,
                    allow_type_changes,
                )?))
            } else {
                None
            };
            let condition = match condition {
                Some(condition) => Some(eval_local_scalar_condition_expr(
                    condition, analysis, assigned,
                )?),
                None => None,
            };
            let mut body_assigned = assigned.clone();
            let body = parse_eval_local_scalar_block(
                body,
                analysis,
                &mut body_assigned,
                loop_depth + 1,
                false,
            )?;
            let update = if let Some(update) = update.as_deref() {
                Some(Box::new(parse_eval_local_scalar_for_clause(
                    update,
                    analysis,
                    &mut body_assigned,
                    false,
                )?))
            } else {
                None
            };
            Some((
                EvalLocalScalarStmt::For {
                    init,
                    condition,
                    update,
                    body,
                },
                false,
            ))
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => parse_eval_local_scalar_switch_stmt(
            subject,
            cases,
            default.as_deref(),
            analysis,
            assigned,
            loop_depth,
        )
        .map(|stmt| (stmt, false)),
        StmtKind::Break(level) if *level > 0 && *level <= loop_depth => {
            Some((EvalLocalScalarStmt::Break(*level), true))
        }
        StmtKind::Continue(level) if *level > 0 && *level <= loop_depth => {
            Some((EvalLocalScalarStmt::Continue(*level), true))
        }
        StmtKind::Return(Some(expr)) => {
            let expr = eval_local_scalar_value_expr(expr, analysis, assigned)?;
            Some((EvalLocalScalarStmt::Return(Some(expr)), true))
        }
        StmtKind::Return(None) => Some((EvalLocalScalarStmt::Return(None), true)),
        StmtKind::ExprStmt(expr) => {
            parse_eval_local_scalar_expr_stmt(expr, analysis, assigned, allow_type_changes)
        }
        _ => None,
    }
}

/// Parses a switch statement accepted by the local scalar AOT subset.
fn parse_eval_local_scalar_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    loop_depth: usize,
) -> Option<EvalLocalScalarStmt> {
    let default_index = eval_local_scalar_switch_default_index(cases, default)?;
    let subject = eval_local_scalar_condition_expr(subject, analysis, assigned)?;
    let mut parsed_cases = Vec::new();
    for (conditions, body) in cases {
        let conditions = conditions
            .iter()
            .map(|condition| eval_local_scalar_condition_expr(condition, analysis, assigned))
            .collect::<Option<Vec<_>>>()?;
        conditions
            .iter()
            .all(|condition| condition.ty == subject.ty)
            .then_some(())?;
        let mut case_assigned = assigned.clone();
        let body = parse_eval_local_scalar_switch_block(
            body,
            analysis,
            &mut case_assigned,
            loop_depth + 1,
            false,
        )?;
        parsed_cases.push((conditions, body));
    }
    let default = if let Some(default) = default {
        let mut default_assigned = assigned.clone();
        parse_eval_local_scalar_switch_block(
            default,
            analysis,
            &mut default_assigned,
            loop_depth + 1,
            false,
        )?
    } else {
        Vec::new()
    };
    let subject_slots = 1 + subject.scratch_slots();
    let condition_slots = parsed_cases
        .iter()
        .flat_map(|(conditions, _)| conditions)
        .map(|condition| 1 + condition.scratch_slots())
        .max()
        .unwrap_or(1);
    analysis.max_scratch_slots = analysis
        .max_scratch_slots
        .max(subject_slots)
        .max(condition_slots);
    Some(EvalLocalScalarStmt::Switch {
        subject,
        cases: parsed_cases,
        default,
        default_index,
    })
}

/// Parses one switch case/default body for the local scalar AOT subset.
fn parse_eval_local_scalar_switch_block(
    statements: &[Stmt],
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    loop_depth: usize,
    allow_type_changes: bool,
) -> Option<Vec<EvalLocalScalarStmt>> {
    parse_eval_local_scalar_block(
        statements,
        analysis,
        assigned,
        loop_depth,
        allow_type_changes,
    )
}

/// Returns the source-order insertion point for a switch default body.
fn eval_local_scalar_switch_default_index(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) -> Option<Option<usize>> {
    let Some(default) = default else {
        return Some(None);
    };
    if cases.is_empty() {
        return Some(Some(0));
    }
    let Some(default_start) = default.first().map(|stmt| stmt.span) else {
        return None;
    };
    if default_start == crate::span::Span::dummy() {
        return None;
    }
    let mut default_index = 0;
    for (conditions, _) in cases {
        let case_start = conditions.first()?.span;
        if case_start == crate::span::Span::dummy() {
            return None;
        }
        if eval_local_scalar_span_is_before(case_start, default_start) {
            default_index += 1;
        }
    }
    Some(Some(default_index))
}

/// Returns true when `span` appears before `pivot` in the same eval fragment.
fn eval_local_scalar_span_is_before(span: crate::span::Span, pivot: crate::span::Span) -> bool {
    span.line < pivot.line || (span.line == pivot.line && span.col < pivot.col)
}

/// Parses one inline `for` init/update statement for local scalar AOT.
fn parse_eval_local_scalar_for_clause(
    stmt: &Stmt,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    allow_type_changes: bool,
) -> Option<EvalLocalScalarStmt> {
    match &stmt.kind {
        StmtKind::Assign { .. } | StmtKind::ExprStmt(_) => {
            let (stmt, terminates) =
                parse_eval_local_scalar_stmt(stmt, analysis, assigned, 0, allow_type_changes)?;
            (!terminates).then_some(stmt)
        }
        _ => None,
    }
}

/// Parses expression statements accepted by the local scalar AOT subset.
fn parse_eval_local_scalar_expr_stmt(
    expr: &Expr,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &mut BTreeSet<String>,
    allow_type_changes: bool,
) -> Option<(EvalLocalScalarStmt, bool)> {
    match &expr.kind {
        ExprKind::Print(inner) => {
            let expr = eval_local_scalar_echo_expr(inner, analysis, assigned)?;
            Some((EvalLocalScalarStmt::Echo(expr), false))
        }
        ExprKind::Assignment {
            target,
            value,
            prelude,
            conditional_value_temp,
            ..
        } if prelude.is_empty() && conditional_value_temp.is_none() => {
            let ExprKind::Variable(name) = &target.kind else {
                return None;
            };
            let value = eval_local_scalar_value_expr(value, analysis, assigned)?;
            analysis.ensure_local(name, value.ty, allow_type_changes)?;
            assigned.insert(name.clone());
            Some((
                EvalLocalScalarStmt::Store {
                    name: name.clone(),
                    value,
                },
                false,
            ))
        }
        ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name) => {
            let value = eval_local_scalar_inc_dec_expr(name, true, analysis, assigned)?;
            Some((
                EvalLocalScalarStmt::Store {
                    name: name.clone(),
                    value,
                },
                false,
            ))
        }
        ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
            let value = eval_local_scalar_inc_dec_expr(name, false, analysis, assigned)?;
            Some((
                EvalLocalScalarStmt::Store {
                    name: name.clone(),
                    value,
                },
                false,
            ))
        }
        _ => {
            let parsed = eval_local_scalar_value_expr(expr, analysis, assigned)?;
            // A bare expression statement lowers to Noop, discarding the value.
            // Calls and prints inside it would lose their side effects (e.g. a
            // dropped `define(...)`), so those fragments must fall back to the
            // bridge instead of the local scalar AOT subset.
            if eval_local_scalar_expr_has_side_effects(&parsed) {
                return None;
            }
            Some((EvalLocalScalarStmt::Noop, false))
        }
    }
}

/// Returns true when a parsed local-scalar expression carries side effects
/// that a discarded expression statement would lose.
fn eval_local_scalar_expr_has_side_effects(expr: &EvalLocalScalarExpr) -> bool {
    match &expr.kind {
        EvalLocalScalarExprKind::Null
        | EvalLocalScalarExprKind::Int(_)
        | EvalLocalScalarExprKind::Float(_)
        | EvalLocalScalarExprKind::Bool(_)
        | EvalLocalScalarExprKind::String(_)
        | EvalLocalScalarExprKind::LoadVar(_)
        | EvalLocalScalarExprKind::Isset(_)
        | EvalLocalScalarExprKind::EmptyVar(_) => false,
        EvalLocalScalarExprKind::Negate(inner)
        | EvalLocalScalarExprKind::BitNot(inner)
        | EvalLocalScalarExprKind::Not(inner) => eval_local_scalar_expr_has_side_effects(inner),
        EvalLocalScalarExprKind::Print(_) => true,
        EvalLocalScalarExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            eval_local_scalar_expr_has_side_effects(condition)
                || eval_local_scalar_expr_has_side_effects(then_expr)
                || eval_local_scalar_expr_has_side_effects(else_expr)
        }
        EvalLocalScalarExprKind::Binary { left, right, .. } => {
            eval_local_scalar_expr_has_side_effects(left)
                || eval_local_scalar_expr_has_side_effects(right)
        }
        EvalLocalScalarExprKind::StaticFunctionCall { .. } => true,
    }
}

/// Builds an increment/decrement assignment value for local scalar expression statements.
fn eval_local_scalar_inc_dec_expr(
    name: &str,
    increment: bool,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    if !assigned.contains(name) || analysis.local_types.get(name) != Some(&EvalLocalScalarType::Int)
    {
        return None;
    }
    let op = if increment {
        EvalLocalScalarBinaryOp::Add
    } else {
        EvalLocalScalarBinaryOp::Sub
    };
    let expr = EvalLocalScalarExpr {
        kind: EvalLocalScalarExprKind::Binary {
            op,
            left: Box::new(EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::LoadVar(name.to_string()),
                ty: EvalLocalScalarType::Int,
            }),
            right: Box::new(EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Int(1),
                ty: EvalLocalScalarType::Int,
            }),
        },
        ty: EvalLocalScalarType::Int,
    };
    analysis.record_expr(&expr);
    Some(expr)
}

/// Parses a condition expression accepted by local scalar AOT control flow.
fn eval_local_scalar_condition_expr(
    expr: &Expr,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    let expr = eval_local_scalar_value_expr(expr, analysis, assigned)?;
    matches!(
        expr.ty,
        EvalLocalScalarType::Int | EvalLocalScalarType::Bool
    )
    .then_some(expr)
}

/// Parses an echo expression accepted by the local scalar AOT subset.
fn eval_local_scalar_echo_expr(
    expr: &Expr,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    let parsed = match &expr.kind {
        ExprKind::StringLiteral(value) => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::String(value.clone()),
            ty: EvalLocalScalarType::String,
        },
        ExprKind::BinaryOp { left, op, right } if *op == BinOp::Concat => {
            let left = eval_local_scalar_echo_expr(left, analysis, assigned)?;
            let right = eval_local_scalar_echo_expr(right, analysis, assigned)?;
            EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Binary {
                    op: EvalLocalScalarBinaryOp::Concat,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty: EvalLocalScalarType::String,
            }
        }
        _ => eval_local_scalar_value_expr(expr, analysis, assigned)?,
    };
    analysis.record_expr(&parsed);
    Some(parsed)
}

/// Parses an int/bool value expression accepted by the local scalar AOT subset.
fn eval_local_scalar_value_expr(
    expr: &Expr,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    let parsed = match &expr.kind {
        ExprKind::Null => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::Null,
            ty: EvalLocalScalarType::Null,
        },
        ExprKind::IntLiteral(value) => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::Int(*value),
            ty: EvalLocalScalarType::Int,
        },
        ExprKind::FloatLiteral(value) if value.is_finite() => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::Float(*value),
            ty: EvalLocalScalarType::Float,
        },
        ExprKind::BoolLiteral(value) => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::Bool(*value),
            ty: EvalLocalScalarType::Bool,
        },
        ExprKind::StringLiteral(value) => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::String(value.clone()),
            ty: EvalLocalScalarType::String,
        },
        ExprKind::Variable(name) if assigned.contains(name) => EvalLocalScalarExpr {
            kind: EvalLocalScalarExprKind::LoadVar(name.clone()),
            ty: *analysis.local_types.get(name)?,
        },
        ExprKind::Negate(inner) => {
            let inner = eval_local_scalar_value_expr(inner, analysis, assigned)?;
            if !matches!(
                inner.ty,
                EvalLocalScalarType::Int | EvalLocalScalarType::Float
            ) {
                return None;
            }
            let ty = inner.ty;
            EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Negate(Box::new(inner)),
                ty,
            }
        }
        ExprKind::BitNot(inner) => {
            let inner = eval_local_scalar_value_expr(inner, analysis, assigned)?;
            if inner.ty != EvalLocalScalarType::Int {
                return None;
            }
            EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::BitNot(Box::new(inner)),
                ty: EvalLocalScalarType::Int,
            }
        }
        ExprKind::Not(inner) => {
            let inner = eval_local_scalar_condition_expr(inner, analysis, assigned)?;
            EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Not(Box::new(inner)),
                ty: EvalLocalScalarType::Bool,
            }
        }
        ExprKind::ErrorSuppress(inner) => eval_local_scalar_value_expr(inner, analysis, assigned)?,
        ExprKind::Print(inner) => {
            let inner = eval_local_scalar_echo_expr(inner, analysis, assigned)?;
            EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Print(Box::new(inner)),
                ty: EvalLocalScalarType::Int,
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = eval_local_scalar_condition_expr(condition, analysis, assigned)?;
            let then_expr = eval_local_scalar_value_expr(then_expr, analysis, assigned)?;
            let else_expr = eval_local_scalar_value_expr(else_expr, analysis, assigned)?;
            if then_expr.ty != else_expr.ty {
                return None;
            }
            EvalLocalScalarExpr {
                ty: then_expr.ty,
                kind: EvalLocalScalarExprKind::Ternary {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                },
            }
        }
        ExprKind::BinaryOp { left, op, right } => {
            eval_local_scalar_binary_expr(left, op, right, analysis, assigned)?
        }
        ExprKind::FunctionCall { name, args } => {
            eval_local_scalar_call_expr(name, args, analysis, assigned)?
        }
        _ => return None,
    };
    analysis.record_expr(&parsed);
    Some(parsed)
}

/// Parses a static call accepted by the local scalar AOT subset.
fn eval_local_scalar_call_expr(
    name: &crate::names::Name,
    args: &[Expr],
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    let short_name = name.as_str().trim_start_matches('\\');
    if let Some(expr) = eval_local_scalar_construct_call_expr(short_name, args, analysis, assigned)
    {
        return Some(expr);
    }
    if let Some(value) = crate::eval_aot::fold_static_builtin_int_call(short_name, args) {
        return Some(eval_local_scalar_int_literal(value));
    }
    let args = args
        .iter()
        .map(|arg| eval_local_scalar_value_expr(arg, analysis, assigned))
        .collect::<Option<Vec<_>>>()?;
    Some(EvalLocalScalarExpr {
        kind: EvalLocalScalarExprKind::StaticFunctionCall {
            name: name.as_str().to_string(),
            args,
        },
        ty: EvalLocalScalarType::Int,
    })
}

/// Parses local-scalar language constructs that behave like expressions.
fn eval_local_scalar_construct_call_expr(
    name: &str,
    args: &[Expr],
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    if args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return None;
    }
    match php_symbol_key(name).as_str() {
        "isset" => {
            if args.is_empty() {
                return None;
            }
            let names = args
                .iter()
                .map(|arg| match &arg.kind {
                    ExprKind::Variable(name) if assigned.contains(name) => Some(name.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            Some(EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::Isset(names),
                ty: EvalLocalScalarType::Bool,
            })
        }
        "empty" if args.len() == 1 => {
            let ExprKind::Variable(name) = &args[0].kind else {
                return None;
            };
            if !assigned.contains(name) || !analysis.local_types.contains_key(name) {
                return None;
            }
            Some(EvalLocalScalarExpr {
                kind: EvalLocalScalarExprKind::EmptyVar(name.clone()),
                ty: EvalLocalScalarType::Bool,
            })
        }
        _ => None,
    }
}

/// Constructs a local-scalar integer literal expression.
fn eval_local_scalar_int_literal(value: i64) -> EvalLocalScalarExpr {
    EvalLocalScalarExpr {
        kind: EvalLocalScalarExprKind::Int(value),
        ty: EvalLocalScalarType::Int,
    }
}

/// Parses a binary int/bool expression accepted by the local scalar AOT subset.
fn eval_local_scalar_binary_expr(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    analysis: &mut EvalLocalScalarAnalysis,
    assigned: &BTreeSet<String>,
) -> Option<EvalLocalScalarExpr> {
    let left = eval_local_scalar_value_expr(left, analysis, assigned)?;
    let right = eval_local_scalar_value_expr(right, analysis, assigned)?;
    let (op, ty) = EvalLocalScalarBinaryOp::from_value_binop(op, left.ty, right.ty)?;
    let expr = EvalLocalScalarExpr {
        kind: EvalLocalScalarExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        ty,
    };
    analysis.record_expr(&expr);
    Some(expr)
}

/// Appends AOT instructions for one eligible eval-fragment statement.
fn push_eval_literal_aot_stmt(
    stmt: &Stmt,
    instructions: &mut Vec<EvalLiteralAotInst>,
) -> Option<bool> {
    match &stmt.kind {
        StmtKind::Echo(expr) => {
            instructions.push(EvalLiteralAotInst::Echo(eval_literal_aot_expr(expr)?));
            Some(false)
        }
        StmtKind::Assign { name, value } => {
            instructions.push(EvalLiteralAotInst::Store {
                name: name.clone(),
                value: eval_literal_aot_expr(value)?,
            });
            Some(false)
        }
        StmtKind::Return(Some(expr)) => {
            instructions.push(EvalLiteralAotInst::Return(eval_literal_aot_expr(expr)?));
            Some(true)
        }
        StmtKind::Return(None) => {
            instructions.push(EvalLiteralAotInst::Return(EvalLiteralAotExpr::Scalar(
                EvalLiteralAotScalar::Null,
            )));
            Some(true)
        }
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Print(inner) => {
                instructions.push(EvalLiteralAotInst::Echo(eval_literal_aot_expr(inner)?));
                Some(false)
            }
            _ => {
                let _ = eval_literal_aot_scalar_expr(expr)?;
                Some(false)
            }
        },
        _ => None,
    }
}

/// Builds a boxed-Mixed AOT expression, folding pure scalar expressions when possible.
fn eval_literal_aot_expr(expr: &Expr) -> Option<EvalLiteralAotExpr> {
    if let Some(value) = eval_literal_aot_scalar_expr(expr) {
        return Some(EvalLiteralAotExpr::Scalar(value));
    }
    match &expr.kind {
        ExprKind::Variable(name) => Some(EvalLiteralAotExpr::LoadVar(name.clone())),
        ExprKind::BinaryOp { left, op, right } => Some(EvalLiteralAotExpr::Binary {
            op: EvalLiteralAotBinaryOp::from_binop(op)?,
            left: Box::new(eval_literal_aot_expr(left)?),
            right: Box::new(eval_literal_aot_expr(right)?),
        }),
        _ => None,
    }
}

/// Evaluates a scalar-only expression at compile time for the literal eval AOT subset.
fn eval_literal_aot_scalar_expr(expr: &Expr) -> Option<EvalLiteralAotScalar> {
    match &expr.kind {
        ExprKind::Null => Some(EvalLiteralAotScalar::Null),
        ExprKind::BoolLiteral(value) => Some(EvalLiteralAotScalar::Bool(*value)),
        ExprKind::IntLiteral(value) => Some(EvalLiteralAotScalar::Int(*value)),
        ExprKind::FloatLiteral(value) if value.is_finite() => {
            Some(EvalLiteralAotScalar::Float(*value))
        }
        ExprKind::StringLiteral(value) => Some(EvalLiteralAotScalar::String(value.clone())),
        ExprKind::Negate(inner) => eval_literal_aot_negate(inner),
        ExprKind::Not(inner) => {
            let value = eval_literal_aot_scalar_expr(inner)?;
            Some(EvalLiteralAotScalar::Bool(!value.truthy()))
        }
        ExprKind::BinaryOp { left, op, right } => {
            let left = eval_literal_aot_scalar_expr(left)?;
            let right = eval_literal_aot_scalar_expr(right)?;
            eval_literal_aot_binary(&left, op, &right)
        }
        _ => None,
    }
}

/// Applies unary minus for scalar integer and float literals.
fn eval_literal_aot_negate(expr: &Expr) -> Option<EvalLiteralAotScalar> {
    match eval_literal_aot_scalar_expr(expr)? {
        EvalLiteralAotScalar::Int(value) => value.checked_neg().map(EvalLiteralAotScalar::Int),
        EvalLiteralAotScalar::Float(value) => Some(EvalLiteralAotScalar::Float(-value)),
        _ => None,
    }
}

/// Applies a safe scalar binary operation for the literal eval AOT subset.
fn eval_literal_aot_binary(
    left: &EvalLiteralAotScalar,
    op: &BinOp,
    right: &EvalLiteralAotScalar,
) -> Option<EvalLiteralAotScalar> {
    match op {
        BinOp::Add => eval_literal_aot_int_binop(left, right, i64::checked_add),
        BinOp::Sub => eval_literal_aot_int_binop(left, right, i64::checked_sub),
        BinOp::Mul => eval_literal_aot_int_binop(left, right, i64::checked_mul),
        BinOp::Mod => match (left, right) {
            (EvalLiteralAotScalar::Int(_), EvalLiteralAotScalar::Int(0)) => None,
            (EvalLiteralAotScalar::Int(left), EvalLiteralAotScalar::Int(right)) => {
                left.checked_rem(*right).map(EvalLiteralAotScalar::Int)
            }
            _ => None,
        },
        BinOp::Concat => Some(EvalLiteralAotScalar::String(format!(
            "{}{}",
            left.as_php_string()?,
            right.as_php_string()?
        ))),
        _ => None,
    }
}

/// Applies an integer-only checked binary operation.
fn eval_literal_aot_int_binop(
    left: &EvalLiteralAotScalar,
    right: &EvalLiteralAotScalar,
    op: fn(i64, i64) -> Option<i64>,
) -> Option<EvalLiteralAotScalar> {
    match (left, right) {
        (EvalLiteralAotScalar::Int(left), EvalLiteralAotScalar::Int(right)) => {
            op(*left, *right).map(EvalLiteralAotScalar::Int)
        }
        _ => None,
    }
}

impl EvalLiteralAotInst {
    /// Returns true when this AOT instruction needs eval scope setup.
    fn has_scope_access(&self) -> bool {
        match self {
            Self::Echo(expr) | Self::Return(expr) => expr.has_scope_access(),
            Self::Store { .. } => true,
        }
    }

    /// Returns variable names read from eval scope while executing this instruction.
    fn scope_reads(&self) -> BTreeSet<String> {
        let mut reads = BTreeSet::new();
        match self {
            Self::Echo(expr) | Self::Return(expr) => expr.collect_scope_reads(&mut reads),
            Self::Store { value, .. } => value.collect_scope_reads(&mut reads),
        }
        reads
    }

    /// Returns the eval scope variable written by this instruction, if any.
    fn scope_write(&self) -> Option<String> {
        match self {
            Self::Store { name, .. } => Some(name.clone()),
            Self::Echo(_) | Self::Return(_) => None,
        }
    }
}

impl EvalLiteralAotExpr {
    /// Returns true when this expression reads a variable from the eval scope.
    fn has_scope_access(&self) -> bool {
        match self {
            Self::Scalar(_) => false,
            Self::LoadVar(_) => true,
            Self::Binary { left, right, .. } => left.has_scope_access() || right.has_scope_access(),
        }
    }

    /// Adds all variable names read from eval scope by this expression.
    fn collect_scope_reads(&self, reads: &mut BTreeSet<String>) {
        match self {
            Self::Scalar(_) => {}
            Self::LoadVar(name) => {
                reads.insert(name.clone());
            }
            Self::Binary { left, right, .. } => {
                left.collect_scope_reads(reads);
                right.collect_scope_reads(reads);
            }
        }
    }
}

impl EvalLiteralAotBinaryOp {
    /// Maps an AST operator to the boxed-Mixed helper used by literal eval AOT.
    fn from_binop(op: &BinOp) -> Option<Self> {
        match op {
            BinOp::Add => Some(Self::Add),
            BinOp::Sub => Some(Self::Sub),
            BinOp::Mul => Some(Self::Mul),
            BinOp::Div => Some(Self::Div),
            BinOp::Mod => Some(Self::Mod),
            BinOp::Concat => Some(Self::Concat),
            _ => None,
        }
    }

    /// Returns the runtime helper symbol for this boxed-Mixed binary operation.
    fn helper(&self) -> &'static str {
        match self {
            Self::Add => "__elephc_eval_value_add",
            Self::Sub => "__elephc_eval_value_sub",
            Self::Mul => "__elephc_eval_value_mul",
            Self::Div => "__elephc_eval_value_div",
            Self::Mod => "__elephc_eval_value_mod",
            Self::Concat => "__elephc_eval_value_concat",
        }
    }
}

impl EvalLiteralAotScalar {
    /// Returns PHP truthiness for the scalar forms accepted by the AOT subset.
    fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty() && value != "0",
        }
    }

    /// Converts scalar forms to the PHP string form safe for compile-time concat.
    fn as_php_string(&self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(false) => Some(String::new()),
            Self::Bool(true) => Some("1".to_string()),
            Self::Int(value) => Some(value.to_string()),
            Self::String(value) => Some(value.clone()),
            Self::Float(_) => None,
        }
    }
}

impl EvalLocalScalarAnalysis {
    /// Registers or validates a local variable slot and its current scalar type.
    fn ensure_local(
        &mut self,
        name: &str,
        ty: EvalLocalScalarType,
        allow_type_change: bool,
    ) -> Option<()> {
        if let Some(existing) = self.local_types.get(name) {
            if *existing != ty {
                allow_type_change.then_some(())?;
                self.local_types.insert(name.to_string(), ty);
            }
        } else {
            self.local_types.insert(name.to_string(), ty);
        }
        if !self.locals.contains_key(name) {
            let slot = self.locals.len();
            self.locals.insert(name.to_string(), slot);
        }
        Some(())
    }

    /// Records scratch-slot demand for a parsed local scalar expression.
    fn record_expr(&mut self, expr: &EvalLocalScalarExpr) {
        self.max_scratch_slots = self.max_scratch_slots.max(expr.scratch_slots());
    }
}

impl EvalLocalScalarAotProgram {
    /// Returns the byte offset of a local scalar value slot from the reserved eval stack base.
    fn value_offset(&self, name: &str) -> usize {
        EVAL_STACK_BYTES + self.locals[name] * EVAL_LOCAL_SCALAR_SLOT_BYTES
    }

    /// Returns the byte offset of a local scalar secondary payload word.
    fn value_aux_offset(&self, name: &str) -> usize {
        self.value_offset(name) + 8
    }

    /// Returns the byte offset of a local scalar defined-flag slot from the eval stack base.
    fn defined_offset(&self, name: &str) -> usize {
        self.value_offset(name) + 16
    }

    /// Returns the byte offset for a recursive expression scratch slot.
    fn scratch_offset(&self, depth: usize) -> usize {
        EVAL_STACK_BYTES + self.locals.len() * EVAL_LOCAL_SCALAR_SLOT_BYTES + depth * 8
    }

    /// Returns the byte offset used to preserve the eval return cell across scope flushes.
    fn result_cell_offset(&self) -> usize {
        self.scratch_offset(self.scratch_slots)
    }

    /// Returns the total temporary stack bytes needed by this local AOT fragment.
    fn stack_bytes(&self) -> usize {
        align_to_16(self.result_cell_offset() + 8)
    }

    /// Returns the scalar type tracked for a local name.
    fn local_type(&self, name: &str) -> EvalLocalScalarType {
        self.local_types[name]
    }
}

impl EvalLocalScalarExpr {
    /// Returns how many fixed scratch slots this expression can need during lowering.
    fn scratch_slots(&self) -> usize {
        match &self.kind {
            EvalLocalScalarExprKind::Null
            | EvalLocalScalarExprKind::Int(_)
            | EvalLocalScalarExprKind::Float(_)
            | EvalLocalScalarExprKind::Bool(_)
            | EvalLocalScalarExprKind::String(_)
            | EvalLocalScalarExprKind::LoadVar(_)
            | EvalLocalScalarExprKind::Isset(_)
            | EvalLocalScalarExprKind::EmptyVar(_) => 0,
            EvalLocalScalarExprKind::Negate(inner)
            | EvalLocalScalarExprKind::BitNot(inner)
            | EvalLocalScalarExprKind::Not(inner)
            | EvalLocalScalarExprKind::Print(inner) => inner.scratch_slots(),
            EvalLocalScalarExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => condition
                .scratch_slots()
                .max(then_expr.scratch_slots())
                .max(else_expr.scratch_slots()),
            EvalLocalScalarExprKind::Binary { op, left, right } => {
                if matches!(
                    op,
                    EvalLocalScalarBinaryOp::And | EvalLocalScalarBinaryOp::Or
                ) {
                    left.scratch_slots().max(right.scratch_slots())
                } else if matches!(op, EvalLocalScalarBinaryOp::Concat) {
                    left.scratch_slots().max(right.scratch_slots())
                } else {
                    left.scratch_slots().max(1 + right.scratch_slots()).max(1)
                }
            }
            EvalLocalScalarExprKind::StaticFunctionCall { args, .. } => {
                let arg_slots = args.len();
                args.iter()
                    .map(EvalLocalScalarExpr::scratch_slots)
                    .max()
                    .unwrap_or(0)
                    + arg_slots
            }
        }
    }
}

impl EvalLocalScalarType {
    /// Returns true when the type can participate in numeric local-scalar operations.
    fn is_numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    /// Maps the local scalar type to the nearest PHP codegen representation.
    fn php_type(self) -> PhpType {
        match self {
            Self::Null => PhpType::Void,
            Self::Int => PhpType::Int,
            Self::Float => PhpType::Float,
            Self::Bool => PhpType::Bool,
            Self::String => PhpType::Str,
        }
    }
}

impl EvalLocalScalarBinaryOp {
    /// Maps an AST binary operator and operand types into the local scalar AOT subset.
    fn from_value_binop(
        op: &BinOp,
        left_ty: EvalLocalScalarType,
        right_ty: EvalLocalScalarType,
    ) -> Option<(Self, EvalLocalScalarType)> {
        match op {
            BinOp::Add
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::Add, EvalLocalScalarType::Int))
            }
            BinOp::Sub
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::Sub, EvalLocalScalarType::Int))
            }
            BinOp::Mul
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::Mul, EvalLocalScalarType::Int))
            }
            BinOp::Div if left_ty.is_numeric() && right_ty.is_numeric() => {
                Some((Self::Div, EvalLocalScalarType::Float))
            }
            BinOp::Mod if left_ty.is_numeric() && right_ty.is_numeric() => {
                Some((Self::Mod, EvalLocalScalarType::Int))
            }
            BinOp::BitAnd
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::BitAnd, EvalLocalScalarType::Int))
            }
            BinOp::BitOr
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::BitOr, EvalLocalScalarType::Int))
            }
            BinOp::BitXor
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::BitXor, EvalLocalScalarType::Int))
            }
            BinOp::ShiftLeft
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::ShiftLeft, EvalLocalScalarType::Int))
            }
            BinOp::ShiftRight
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::ShiftRight, EvalLocalScalarType::Int))
            }
            BinOp::Lt
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::Lt, EvalLocalScalarType::Bool))
            }
            BinOp::Gt
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::Gt, EvalLocalScalarType::Bool))
            }
            BinOp::LtEq
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::LtEq, EvalLocalScalarType::Bool))
            }
            BinOp::GtEq
                if left_ty == EvalLocalScalarType::Int && right_ty == EvalLocalScalarType::Int =>
            {
                Some((Self::GtEq, EvalLocalScalarType::Bool))
            }
            BinOp::Eq
                if left_ty == right_ty
                    && matches!(
                        left_ty,
                        EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                    ) =>
            {
                Some((Self::Eq, EvalLocalScalarType::Bool))
            }
            BinOp::NotEq
                if left_ty == right_ty
                    && matches!(
                        left_ty,
                        EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                    ) =>
            {
                Some((Self::NotEq, EvalLocalScalarType::Bool))
            }
            BinOp::And
                if matches!(
                    left_ty,
                    EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                ) && matches!(
                    right_ty,
                    EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                ) =>
            {
                Some((Self::And, EvalLocalScalarType::Bool))
            }
            BinOp::Or
                if matches!(
                    left_ty,
                    EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                ) && matches!(
                    right_ty,
                    EvalLocalScalarType::Int | EvalLocalScalarType::Bool
                ) =>
            {
                Some((Self::Or, EvalLocalScalarType::Bool))
            }
            _ => None,
        }
    }
}

/// Rounds a byte count up to the next 16-byte stack-aligned size.
fn align_to_16(value: usize) -> usize {
    (value + 15) & !15
}

/// Lowers an eligible literal eval fragment directly, returning false when codegen must fall back.
fn lower_eval_literal_aot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLiteralAotProgram,
) -> Result<bool> {
    match program {
        EvalLiteralAotProgram::Boxed(program) => lower_eval_literal_boxed_aot(ctx, inst, program),
        EvalLiteralAotProgram::LocalScalar(program) => {
            lower_eval_local_scalar_aot(ctx, inst, program)
        }
    }
}

/// Lowers an eligible boxed-Mixed literal eval fragment directly.
fn lower_eval_literal_boxed_aot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLiteralBoxedAotProgram,
) -> Result<bool> {
    if let Some(targets) = eval_literal_boxed_direct_local_store_targets(ctx, program)? {
        lower_eval_literal_boxed_aot_direct_local_stores(ctx, inst, program, &targets)?;
        return Ok(true);
    }
    if let Some(targets) = eval_literal_boxed_direct_read_write_targets(ctx, program)? {
        lower_eval_literal_boxed_aot_direct_read_writes(ctx, inst, program, &targets)?;
        return Ok(true);
    }
    if program.has_scope_writes && !eval_global_aliases(ctx).is_empty() {
        ctx.emitter.comment(
            "eval literal AOT fallback: scope writes with global aliases need bridge semantics",
        );
        return Ok(false);
    }

    ctx.emitter.comment(&format!(
        "eval literal AOT compiled ({} ops)",
        program.instructions.len()
    ));
    if program.has_scope_access {
        abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
        ensure_eval_scope(ctx)?;
        ensure_eval_global_scope(ctx)?;
        let flush_names = program
            .scope_reads
            .union(&program.scope_writes)
            .cloned()
            .collect::<BTreeSet<_>>();
        let sync_locals = eval_sync_locals(ctx);
        let sync_globals = eval_sync_globals(ctx);
        let flush_locals = filter_eval_sync_locals_by_name(sync_locals.clone(), &flush_names);
        let flush_globals = filter_eval_sync_globals_by_name(sync_globals.clone(), &flush_names);
        let reload_locals = filter_eval_sync_locals_by_name(sync_locals, &program.scope_writes);
        let reload_globals = filter_eval_sync_globals_by_name(sync_globals, &program.scope_writes);
        flush_eval_scope_locals(ctx, &flush_locals)?;
        flush_eval_global_scope(ctx, &flush_globals)?;
        for aot_inst in &program.instructions {
            match aot_inst {
                EvalLiteralAotInst::Echo(value) => emit_eval_literal_aot_echo(ctx, value, 0),
                EvalLiteralAotInst::Store { name, value } => {
                    emit_eval_literal_aot_scope_store(ctx, name, value, 0)?;
                }
                EvalLiteralAotInst::Return(value) => {
                    emit_eval_literal_aot_expr_cell(ctx, value, 0);
                    break;
                }
            }
        }
        let result_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        reload_eval_scope_locals(ctx, &reload_locals)?;
        reload_eval_global_scope(ctx, &reload_globals)?;
        abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
        abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    } else {
        for aot_inst in &program.instructions {
            match aot_inst {
                EvalLiteralAotInst::Echo(value) => emit_eval_literal_aot_echo(ctx, value, 0),
                EvalLiteralAotInst::Store { .. } => unreachable!("scope writes are handled above"),
                EvalLiteralAotInst::Return(value) => {
                    emit_eval_literal_aot_expr_cell(ctx, value, 0);
                    break;
                }
            }
        }
    }
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Lowers write-only boxed AOT stores directly into caller local slots without eval scope.
fn lower_eval_literal_boxed_aot_direct_local_stores(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLiteralBoxedAotProgram,
    targets: &BTreeMap<String, Option<LocalSlotId>>,
) -> Result<()> {
    ctx.emitter.comment(&format!(
        "eval literal AOT compiled direct local stores ({} ops)",
        program.instructions.len()
    ));
    for aot_inst in &program.instructions {
        match aot_inst {
            EvalLiteralAotInst::Store { name, value } => {
                let target = targets.get(name).ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "direct eval local store target ${} was not prepared",
                        name
                    ))
                })?;
                if let Some(slot) = target {
                    emit_eval_literal_aot_direct_local_store(ctx, *slot, value)?;
                }
            }
            EvalLiteralAotInst::Return(value) => {
                emit_eval_literal_aot_core_mixed_expr(ctx, value)?;
                store_if_result(ctx, inst)?;
                return Ok(());
            }
            EvalLiteralAotInst::Echo(_) => {
                return Err(CodegenIrError::unsupported(
                    "direct eval local store echo should be rejected before lowering".to_string(),
                ));
            }
        }
    }
    emit_eval_local_scalar_core_null_cell(ctx);
    store_if_result(ctx, inst)
}

/// Lowers read/write eval stores by reading and writing caller integer locals directly.
fn lower_eval_literal_boxed_aot_direct_read_writes(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLiteralBoxedAotProgram,
    targets: &BTreeMap<String, LocalSlotId>,
) -> Result<()> {
    ctx.emitter
        .comment("eval literal AOT compiled direct local read/write stores");
    for aot_inst in &program.instructions {
        match aot_inst {
            EvalLiteralAotInst::Store { name, value } => {
                let slot = targets.get(name).ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "direct eval read/write target ${} was not prepared",
                        name
                    ))
                })?;
                let source_ty = emit_eval_literal_aot_direct_read_write_expr(ctx, *slot, value)?;
                emit_eval_literal_prepare_direct_read_write_store(ctx, *slot, &source_ty)?;
                ctx.store_current_result_to_local(*slot)?;
            }
            EvalLiteralAotInst::Return(value) => {
                let EvalLiteralAotExpr::Scalar(value) = value else {
                    return Err(CodegenIrError::unsupported(
                        "direct eval read/write return requires a static scalar".to_string(),
                    ));
                };
                emit_eval_literal_aot_core_mixed_scalar(ctx, value);
                store_if_result(ctx, inst)?;
                return Ok(());
            }
            EvalLiteralAotInst::Echo(_) => {
                return Err(CodegenIrError::unsupported(
                    "direct eval read/write echo should be rejected before lowering".to_string(),
                ));
            }
        }
    }
    emit_eval_local_scalar_core_null_cell(ctx);
    store_if_result(ctx, inst)
}

/// Resolves direct read/write eval targets, returning `None` when scope semantics are needed.
fn eval_literal_boxed_direct_read_write_targets(
    ctx: &FunctionContext<'_>,
    program: &EvalLiteralBoxedAotProgram,
) -> Result<Option<BTreeMap<String, LocalSlotId>>> {
    if !program.has_scope_writes || program.scope_reads.is_empty() {
        return Ok(None);
    }
    let mut targets = BTreeMap::new();
    for inst in &program.instructions {
        match inst {
            EvalLiteralAotInst::Store { name, value } => {
                let Some(slot) = eval_literal_direct_read_write_local_slot(ctx, name)? else {
                    return Ok(None);
                };
                let target_ty = ctx.local_php_type(slot)?.codegen_repr();
                let Some(result_ty) =
                    eval_literal_direct_read_write_expr_type(value, name, &target_ty)
                else {
                    return Ok(None);
                };
                if !eval_literal_direct_read_write_result_supported(&target_ty, &result_ty) {
                    return Ok(None);
                }
                targets.insert(name.clone(), slot);
            }
            EvalLiteralAotInst::Return(value) => {
                if !matches!(value, EvalLiteralAotExpr::Scalar(_)) {
                    return Ok(None);
                }
            }
            EvalLiteralAotInst::Echo(_) => return Ok(None),
        }
    }
    Ok(Some(targets))
}

/// Returns an initialized scalar caller local slot for direct read/write eval.
fn eval_literal_direct_read_write_local_slot(
    ctx: &FunctionContext<'_>,
    name: &str,
) -> Result<Option<LocalSlotId>> {
    if main_name_uses_eval_global_scope(ctx, name) {
        return Ok(None);
    }
    let Some(slot) = ctx.local_slot_by_name(name) else {
        return Ok(None);
    };
    if ctx.local_kind(slot)? != LocalKind::PhpLocal
        || ctx.local_stores_ref_cell_pointer(slot)
        || !matches!(
            ctx.local_php_type(slot)?.codegen_repr(),
            PhpType::Int | PhpType::Float | PhpType::Mixed | PhpType::Union(_)
        )
        // Slots initialized by function parameters (including by-value closure
        // captures) hold a defined value without an EIR store instruction.
        || !(eval_literal_local_slot_has_eir_write(ctx, slot)
            || ctx.function.params.iter().any(|param| param.name == name))
    {
        return Ok(None);
    }
    Ok(Some(slot))
}

/// Returns the native result type for a direct scalar read/write expression.
fn eval_literal_direct_read_write_expr_type(
    value: &EvalLiteralAotExpr,
    target_name: &str,
    target_ty: &PhpType,
) -> Option<PhpType> {
    match value {
        EvalLiteralAotExpr::Scalar(EvalLiteralAotScalar::Int(_)) => Some(PhpType::Int),
        EvalLiteralAotExpr::Scalar(EvalLiteralAotScalar::Float(_)) => Some(PhpType::Float),
        EvalLiteralAotExpr::LoadVar(name) if name == target_name => Some(target_ty.clone()),
        EvalLiteralAotExpr::Binary { op, left, right } => {
            let left_ty = eval_literal_direct_read_write_expr_type(left, target_name, target_ty)?;
            let right_ty = eval_literal_direct_read_write_expr_type(right, target_name, target_ty)?;
            match op {
                EvalLiteralAotBinaryOp::Add
                | EvalLiteralAotBinaryOp::Sub
                | EvalLiteralAotBinaryOp::Mul
                    if left_ty == PhpType::Int && right_ty == PhpType::Int =>
                {
                    Some(PhpType::Int)
                }
                EvalLiteralAotBinaryOp::Div
                    if eval_literal_direct_read_write_numeric_type(&left_ty)
                        && eval_literal_direct_read_write_numeric_type(&right_ty) =>
                {
                    Some(PhpType::Float)
                }
                EvalLiteralAotBinaryOp::Mod
                    if eval_literal_direct_read_write_numeric_type(&left_ty)
                        && eval_literal_direct_read_write_numeric_type(&right_ty) =>
                {
                    Some(PhpType::Int)
                }
                EvalLiteralAotBinaryOp::Concat
                | EvalLiteralAotBinaryOp::Add
                | EvalLiteralAotBinaryOp::Sub
                | EvalLiteralAotBinaryOp::Mul
                | EvalLiteralAotBinaryOp::Div
                | EvalLiteralAotBinaryOp::Mod => None,
            }
        }
        _ => None,
    }
}

/// Returns true when a direct read/write native result can be stored in the caller slot.
fn eval_literal_direct_read_write_result_supported(
    target_ty: &PhpType,
    result_ty: &PhpType,
) -> bool {
    match target_ty.codegen_repr() {
        PhpType::Int => result_ty.codegen_repr() == PhpType::Int,
        PhpType::Float => result_ty.codegen_repr() == PhpType::Float,
        PhpType::Mixed | PhpType::Union(_) => result_ty.codegen_repr() == PhpType::Float,
        _ => false,
    }
}

/// Returns true for native numeric types accepted by direct read/write arithmetic.
fn eval_literal_direct_read_write_numeric_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Float | PhpType::Mixed | PhpType::Union(_)
    )
}

/// Emits a direct scalar read/write expression into the matching result register.
fn emit_eval_literal_aot_direct_read_write_expr(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: &EvalLiteralAotExpr,
) -> Result<PhpType> {
    match value {
        EvalLiteralAotExpr::Scalar(EvalLiteralAotScalar::Int(value)) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
            Ok(PhpType::Int)
        }
        EvalLiteralAotExpr::Scalar(EvalLiteralAotScalar::Float(value)) => {
            emit_eval_literal_aot_scalar_native_result(ctx, &EvalLiteralAotScalar::Float(*value));
            Ok(PhpType::Float)
        }
        EvalLiteralAotExpr::LoadVar(_) => ctx.load_local_to_result(slot),
        EvalLiteralAotExpr::Binary { op, left, right } => {
            let left_ty = emit_eval_literal_aot_direct_read_write_expr(ctx, slot, left)?;
            emit_eval_literal_direct_read_write_push_result(ctx, &left_ty)?;
            let right_ty = emit_eval_literal_aot_direct_read_write_expr(ctx, slot, right)?;
            match op {
                EvalLiteralAotBinaryOp::Div => {
                    emit_eval_literal_direct_read_write_numeric_to_float(ctx, &right_ty)?;
                    let rhs_reg = abi::float_arg_reg_name(ctx.emitter.target, 1);
                    if matches!(left_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                        abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
                        abi::emit_load_temporary_stack_slot(
                            ctx.emitter,
                            abi::int_result_reg(ctx.emitter),
                            16,
                        );
                        emit_eval_literal_direct_read_write_numeric_to_float(ctx, &left_ty)?;
                        abi::emit_pop_float_reg(ctx.emitter, rhs_reg);
                        abi::emit_release_temporary_stack(ctx.emitter, 16);
                    } else {
                        abi::emit_reg_move(
                            ctx.emitter,
                            rhs_reg,
                            abi::float_result_reg(ctx.emitter),
                        );
                        emit_eval_literal_direct_read_write_pop_result(ctx, &left_ty)?;
                        emit_eval_literal_direct_read_write_numeric_to_float(ctx, &left_ty)?;
                    }
                    emit_eval_literal_direct_read_write_div(ctx, rhs_reg);
                    Ok(PhpType::Float)
                }
                EvalLiteralAotBinaryOp::Mod => {
                    emit_eval_literal_direct_read_write_numeric_to_int(ctx, &right_ty)?;
                    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
                    abi::emit_reg_move(ctx.emitter, rhs_reg, abi::int_result_reg(ctx.emitter));
                    emit_eval_literal_direct_read_write_pop_result(ctx, &left_ty)?;
                    emit_eval_literal_direct_read_write_numeric_to_int(ctx, &left_ty)?;
                    emit_eval_local_scalar_mod(ctx, rhs_reg);
                    Ok(PhpType::Int)
                }
                EvalLiteralAotBinaryOp::Add
                | EvalLiteralAotBinaryOp::Sub
                | EvalLiteralAotBinaryOp::Mul => {
                    if left_ty != PhpType::Int || right_ty != PhpType::Int {
                        return Err(CodegenIrError::unsupported(
                            "direct eval read/write float arithmetic".to_string(),
                        ));
                    }
                    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
                    abi::emit_reg_move(ctx.emitter, rhs_reg, abi::int_result_reg(ctx.emitter));
                    emit_eval_literal_direct_read_write_pop_result(ctx, &left_ty)?;
                    let local_op = match op {
                        EvalLiteralAotBinaryOp::Add => EvalLocalScalarBinaryOp::Add,
                        EvalLiteralAotBinaryOp::Sub => EvalLocalScalarBinaryOp::Sub,
                        EvalLiteralAotBinaryOp::Mul => EvalLocalScalarBinaryOp::Mul,
                        EvalLiteralAotBinaryOp::Div
                        | EvalLiteralAotBinaryOp::Mod
                        | EvalLiteralAotBinaryOp::Concat => unreachable!(
                            "direct read/write arithmetic op filtered before integer lowering"
                        ),
                    };
                    emit_eval_local_scalar_binary_result(ctx, &local_op, rhs_reg);
                    Ok(PhpType::Int)
                }
                EvalLiteralAotBinaryOp::Concat => {
                    return Err(CodegenIrError::unsupported(
                        "direct eval read/write concat".to_string(),
                    ));
                }
            }
        }
        _ => Err(CodegenIrError::unsupported(
            "direct eval read/write expression".to_string(),
        )),
    }
}

/// Preserves the current direct read/write result on the temporary stack.
fn emit_eval_literal_direct_read_write_push_result(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        PhpType::Float => {
            abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "direct eval read/write push for PHP type {:?}",
            other
        ))),
    }
}

/// Restores a preserved direct read/write result from the temporary stack.
fn emit_eval_literal_direct_read_write_pop_result(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "direct eval read/write pop for PHP type {:?}",
            other
        ))),
    }
}

/// Coerces the current direct read/write numeric result into the float register.
fn emit_eval_literal_direct_read_write_numeric_to_float(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Float => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "direct eval read/write float coercion for PHP type {:?}",
            other
        ))),
    }
}

/// Coerces the current direct read/write numeric result into the integer register.
fn emit_eval_literal_direct_read_write_numeric_to_int(
    ctx: &mut FunctionContext<'_>,
    ty: &PhpType,
) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => Ok(()),
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "direct eval read/write int coercion for PHP type {:?}",
            other
        ))),
    }
}

/// Emits the target-specific direct read/write floating division.
fn emit_eval_literal_direct_read_write_div(ctx: &mut FunctionContext<'_>, rhs_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "fdiv {}, {}, {}",
                abi::float_result_reg(ctx.emitter),
                abi::float_result_reg(ctx.emitter),
                rhs_reg
            )); // compute the direct eval read/write floating-point quotient
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "divsd {}, {}",
                abi::float_result_reg(ctx.emitter),
                rhs_reg
            )); // compute the direct eval read/write floating-point quotient
        }
    }
}

/// Converts a direct read/write result to the caller slot representation and releases old ownership.
fn emit_eval_literal_prepare_direct_read_write_store(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    source_ty: &PhpType,
) -> Result<()> {
    let target_ty = ctx.local_php_type(slot)?.codegen_repr();
    if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_box_current_value_as_mixed(ctx.emitter, &source_ty.codegen_repr());
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        emit_eval_literal_release_old_direct_local_value(ctx, slot, &target_ty)?;
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    Ok(())
}

/// Releases the previous refcounted value stored in a direct eval local target.
fn emit_eval_literal_release_old_direct_local_value(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    target_ty: &PhpType,
) -> Result<()> {
    if target_ty.codegen_repr().is_refcounted() {
        let offset = ctx.local_offset(slot)?;
        abi::load_at_offset_scratch(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            offset,
            abi::secondary_scratch_reg(ctx.emitter),
        );
        abi::emit_decref_if_refcounted(ctx.emitter, &target_ty.codegen_repr());
    }
    Ok(())
}

/// Resolves direct local-store targets, returning `None` when scope semantics are still needed.
fn eval_literal_boxed_direct_local_store_targets(
    ctx: &FunctionContext<'_>,
    program: &EvalLiteralBoxedAotProgram,
) -> Result<Option<BTreeMap<String, Option<LocalSlotId>>>> {
    if !program.has_scope_writes || !program.scope_reads.is_empty() {
        return Ok(None);
    }
    let mut targets = BTreeMap::new();
    for inst in &program.instructions {
        match inst {
            EvalLiteralAotInst::Store { name, value } => {
                if !matches!(value, EvalLiteralAotExpr::Scalar(_)) {
                    return Ok(None);
                }
                let target = match eval_literal_direct_store_local_slot(ctx, name)? {
                    Some(slot)
                        if eval_literal_aot_expr_direct_store_supported(ctx, slot, value)? =>
                    {
                        Some(slot)
                    }
                    _ => None,
                };
                targets.insert(name.clone(), target);
            }
            EvalLiteralAotInst::Return(value) => {
                if !matches!(value, EvalLiteralAotExpr::Scalar(_)) {
                    return Ok(None);
                }
            }
            EvalLiteralAotInst::Echo(_) => return Ok(None),
        }
    }
    Ok(Some(targets))
}

/// Returns the caller local slot for a direct eval store when overwriting is known safe.
fn eval_literal_direct_store_local_slot(
    ctx: &FunctionContext<'_>,
    name: &str,
) -> Result<Option<LocalSlotId>> {
    if main_name_uses_eval_global_scope(ctx, name) {
        return Ok(None);
    }
    let Some(slot) = ctx.local_slot_by_name(name) else {
        return Ok(None);
    };
    if ctx.local_kind(slot)? != LocalKind::PhpLocal || ctx.local_stores_ref_cell_pointer(slot) {
        return Ok(None);
    }
    // Slots the native code already writes are fine: the store emitter
    // releases the previous refcounted value before overwriting.
    Ok(Some(slot))
}

/// Returns true when existing EIR already writes a local slot and old-value release matters.
fn eval_literal_local_slot_has_eir_write(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.function.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::StoreLocal
                | Op::StoreRefCell
                | Op::UnsetLocal
                | Op::PromoteLocalRefCell
                | Op::AliasLocalRefCell
                | Op::ReleaseLocalRefCell
        ) && inst.immediate == Some(Immediate::LocalSlot(slot))
    })
}

/// Returns true when a boxed AOT expression can be stored into a local target type.
fn eval_literal_aot_expr_direct_store_supported(
    ctx: &FunctionContext<'_>,
    slot: LocalSlotId,
    value: &EvalLiteralAotExpr,
) -> Result<bool> {
    let EvalLiteralAotExpr::Scalar(value) = value else {
        return Ok(false);
    };
    let target_ty = ctx.local_php_type(slot)?.codegen_repr();
    Ok(eval_literal_aot_scalar_direct_store_supported(
        value, &target_ty,
    ))
}

/// Returns true when a scalar value can be materialized in a direct local target format.
fn eval_literal_aot_scalar_direct_store_supported(
    value: &EvalLiteralAotScalar,
    target_ty: &PhpType,
) -> bool {
    if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
        return true;
    }
    match (value, target_ty) {
        (EvalLiteralAotScalar::Int(_), PhpType::Int) => true,
        (EvalLiteralAotScalar::Bool(_), PhpType::Bool) => true,
        (EvalLiteralAotScalar::Float(_), PhpType::Float) => true,
        (EvalLiteralAotScalar::String(_), PhpType::Str) => true,
        (EvalLiteralAotScalar::Null | EvalLiteralAotScalar::Int(_), PhpType::TaggedScalar) => true,
        _ => false,
    }
}

/// Stores one static scalar eval assignment directly into an already allocated local slot.
fn emit_eval_literal_aot_direct_local_store(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: &EvalLiteralAotExpr,
) -> Result<()> {
    let EvalLiteralAotExpr::Scalar(value) = value else {
        return Err(CodegenIrError::unsupported(
            "direct eval local store requires a static scalar value".to_string(),
        ));
    };
    match ctx.local_php_type(slot)?.codegen_repr() {
        target_ty @ (PhpType::Mixed | PhpType::Union(_)) => {
            emit_eval_literal_aot_core_mixed_scalar(ctx, value);
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_eval_literal_release_old_direct_local_value(ctx, slot, &target_ty)?;
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            ctx.store_current_result_to_local(slot)
        }
        PhpType::Str => {
            // Static scalar strings live in the data section; the old value
            // may be a heap string and must be released before overwriting.
            emit_eval_literal_release_old_direct_local_value(ctx, slot, &PhpType::Str)?;
            emit_eval_literal_aot_scalar_native_result(ctx, value);
            ctx.store_current_result_to_local(slot)
        }
        PhpType::TaggedScalar => {
            emit_eval_literal_aot_tagged_scalar_result(ctx, value)?;
            ctx.store_current_result_to_local(slot)
        }
        target_ty => {
            emit_eval_literal_aot_scalar_native_result(ctx, value);
            if eval_literal_aot_scalar_direct_store_supported(value, &target_ty) {
                ctx.store_current_result_to_local(slot)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "direct eval local store to PHP type {:?}",
                    target_ty
                )))
            }
        }
    }
}

/// Emits a scalar eval expression as a core-runtime Mixed cell in the result register.
fn emit_eval_literal_aot_core_mixed_expr(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotExpr,
) -> Result<()> {
    let EvalLiteralAotExpr::Scalar(value) = value else {
        return Err(CodegenIrError::unsupported(
            "direct eval local store return requires a static scalar value".to_string(),
        ));
    };
    emit_eval_literal_aot_core_mixed_scalar(ctx, value);
    Ok(())
}

/// Emits a scalar value as a core-runtime Mixed cell without eval bridge helpers.
fn emit_eval_literal_aot_core_mixed_scalar(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotScalar,
) {
    if matches!(value, EvalLiteralAotScalar::Null) {
        emit_eval_local_scalar_core_null_cell(ctx);
        return;
    }
    let source_ty = emit_eval_literal_aot_scalar_native_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
}

/// Emits a scalar value in its native result-register representation.
fn emit_eval_literal_aot_scalar_native_result(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotScalar,
) -> PhpType {
    match value {
        EvalLiteralAotScalar::Null => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            PhpType::TaggedScalar
        }
        EvalLiteralAotScalar::Bool(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                i64::from(*value),
            );
            PhpType::Bool
        }
        EvalLiteralAotScalar::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
            PhpType::Int
        }
        EvalLiteralAotScalar::Float(value) => {
            let label = ctx.data.add_float(*value);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &label,
                0,
            );
            PhpType::Float
        }
        EvalLiteralAotScalar::String(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
            PhpType::Str
        }
    }
}

/// Emits a scalar value as the tagged-scalar local representation.
fn emit_eval_literal_aot_tagged_scalar_result(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotScalar,
) -> Result<()> {
    match value {
        EvalLiteralAotScalar::Null => {
            crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
            Ok(())
        }
        EvalLiteralAotScalar::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(
            "direct eval local store tagged-scalar source".to_string(),
        )),
    }
}

/// Lowers a self-contained int/bool literal eval fragment as native local control flow.
fn lower_eval_local_scalar_aot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLocalScalarAotProgram,
) -> Result<bool> {
    if !eval_global_aliases(ctx).is_empty() {
        ctx.emitter.comment(
            "eval literal AOT fallback: local scalar writes with global aliases need bridge semantics",
        );
        return Ok(false);
    }
    if !eval_local_scalar_codegen_supported(ctx, program) {
        ctx.emitter
            .comment("eval literal AOT fallback: local scalar static call is not supported");
        return Ok(false);
    }

    ctx.emitter.comment(&format!(
        "eval literal AOT compiled local scalar ({} locals, {} stmts)",
        program.locals.len(),
        program.statements.len()
    ));
    if let Some(targets) = eval_local_scalar_direct_sync_targets(ctx, program)? {
        lower_eval_local_scalar_aot_with_direct_sync(ctx, inst, program, &targets)?;
    } else if eval_local_scalar_needs_scope_sync(ctx, program) {
        lower_eval_local_scalar_aot_with_scope_sync(ctx, inst, program)?;
    } else {
        lower_eval_local_scalar_aot_native_only(ctx, inst, program)?;
    }
    Ok(true)
}

/// Returns true when local scalar AOT must synchronize variables with eval scope.
fn eval_local_scalar_needs_scope_sync(
    ctx: &FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
) -> bool {
    if !program.locals.is_empty() {
        return true;
    }
    let sync_local_names = eval_sync_locals(ctx)
        .into_iter()
        .map(|local| local.name)
        .collect::<BTreeSet<_>>();
    let sync_global_names = eval_sync_globals(ctx)
        .into_iter()
        .map(|global| global.name)
        .collect::<BTreeSet<_>>();
    program
        .locals
        .keys()
        .any(|name| sync_local_names.contains(name) || sync_global_names.contains(name))
}

/// Resolves caller local slots that can receive final local-scalar eval writes directly.
fn eval_local_scalar_direct_sync_targets(
    ctx: &FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
) -> Result<Option<BTreeMap<String, Option<LocalSlotId>>>> {
    if program.locals.is_empty() {
        return Ok(Some(BTreeMap::new()));
    }
    let mut targets = BTreeMap::new();
    for (name, local_type) in &program.local_types {
        if main_name_uses_eval_global_scope(ctx, name) {
            return Ok(None);
        }
        let Some(slot) = ctx.local_slot_by_name(name) else {
            targets.insert(name.clone(), None);
            continue;
        };
        if ctx.local_kind(slot)? != LocalKind::PhpLocal
            || ctx.local_stores_ref_cell_pointer(slot)
            || !eval_local_scalar_direct_sync_type_supported(ctx.local_php_type(slot)?, *local_type)
        {
            return Ok(None);
        }
        // Slots the native code already writes are fine: the sync store
        // releases the previous refcounted value before overwriting.
        targets.insert(name.clone(), Some(slot));
    }
    Ok(Some(targets))
}

/// Returns true when one local-scalar value type fits the caller local slot type.
fn eval_local_scalar_direct_sync_type_supported(
    target_ty: PhpType,
    local_type: EvalLocalScalarType,
) -> bool {
    match target_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => true,
        PhpType::Int => local_type == EvalLocalScalarType::Int,
        PhpType::Float => local_type == EvalLocalScalarType::Float,
        PhpType::Bool => local_type == EvalLocalScalarType::Bool,
        PhpType::Str => local_type == EvalLocalScalarType::String,
        PhpType::TaggedScalar => matches!(
            local_type,
            EvalLocalScalarType::Null | EvalLocalScalarType::Int
        ),
        _ => false,
    }
}

/// Lowers local scalar AOT without any eval bridge or eval scope runtime dependency.
fn lower_eval_local_scalar_aot_native_only(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLocalScalarAotProgram,
) -> Result<()> {
    let stack_bytes = program.stack_bytes();
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_local_scalar_init_slots(ctx, program);

    let return_label = ctx.next_label("eval_local_aot_return");
    let mut loop_stack = Vec::new();
    for stmt in &program.statements {
        emit_eval_local_scalar_stmt(
            ctx,
            program,
            stmt,
            &mut loop_stack,
            &return_label,
            EvalLocalScalarBoxing::CoreRuntime,
        );
    }
    emit_eval_local_scalar_null_cell(ctx, EvalLocalScalarBoxing::CoreRuntime);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );

    ctx.emitter.label(&return_label);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers local scalar AOT and writes supported final locals directly to caller slots.
fn lower_eval_local_scalar_aot_with_direct_sync(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLocalScalarAotProgram,
    targets: &BTreeMap<String, Option<LocalSlotId>>,
) -> Result<()> {
    ctx.emitter
        .comment("eval literal AOT compiled local scalar with direct local sync");
    let stack_bytes = program.stack_bytes();
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    emit_eval_local_scalar_init_slots(ctx, program);

    let return_label = ctx.next_label("eval_local_aot_return");
    let mut loop_stack = Vec::new();
    for stmt in &program.statements {
        emit_eval_local_scalar_stmt(
            ctx,
            program,
            stmt,
            &mut loop_stack,
            &return_label,
            EvalLocalScalarBoxing::CoreRuntime,
        );
    }
    emit_eval_local_scalar_null_cell(ctx, EvalLocalScalarBoxing::CoreRuntime);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );

    ctx.emitter.label(&return_label);
    emit_eval_local_scalar_flush_direct_locals(ctx, program, targets)?;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Lowers local scalar AOT and syncs defined locals through eval scope.
fn lower_eval_local_scalar_aot_with_scope_sync(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    program: &EvalLocalScalarAotProgram,
) -> Result<()> {
    let stack_bytes = program.stack_bytes();
    abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
    ensure_eval_scope(ctx)?;
    ensure_eval_global_scope(ctx)?;
    emit_eval_local_scalar_init_slots(ctx, program);

    let return_label = ctx.next_label("eval_local_aot_return");
    let mut loop_stack = Vec::new();
    for stmt in &program.statements {
        emit_eval_local_scalar_stmt(
            ctx,
            program,
            stmt,
            &mut loop_stack,
            &return_label,
            EvalLocalScalarBoxing::EvalRuntime,
        );
    }
    emit_eval_local_scalar_null_cell(ctx, EvalLocalScalarBoxing::EvalRuntime);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );

    ctx.emitter.label(&return_label);
    emit_eval_local_scalar_flush_defined_locals(ctx, program);
    let sync_locals = eval_sync_locals(ctx)
        .into_iter()
        .filter(|local| program.locals.contains_key(&local.name))
        .collect::<Vec<_>>();
    let sync_globals = eval_sync_globals(ctx)
        .into_iter()
        .filter(|global| program.locals.contains_key(&global.name))
        .collect::<Vec<_>>();
    reload_eval_scope_locals(ctx, &sync_locals)?;
    reload_eval_global_scope(ctx, &sync_globals)?;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );
    abi::emit_release_temporary_stack(ctx.emitter, stack_bytes);
    store_if_result(ctx, inst)
}

/// Returns true when all parsed local scalar statements are supported by codegen.
fn eval_local_scalar_codegen_supported(
    ctx: &FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
) -> bool {
    eval_local_scalar_stmts_codegen_supported(ctx, &program.statements)
}

/// Returns true when all statements in a local scalar block are supported by codegen.
fn eval_local_scalar_stmts_codegen_supported(
    ctx: &FunctionContext<'_>,
    statements: &[EvalLocalScalarStmt],
) -> bool {
    statements.iter().all(|stmt| match stmt {
        EvalLocalScalarStmt::Noop
        | EvalLocalScalarStmt::Break(_)
        | EvalLocalScalarStmt::Continue(_)
        | EvalLocalScalarStmt::Return(None) => true,
        EvalLocalScalarStmt::Echo(expr) | EvalLocalScalarStmt::Return(Some(expr)) => {
            eval_local_scalar_expr_codegen_supported(ctx, expr)
        }
        EvalLocalScalarStmt::Store { value, .. } => {
            eval_local_scalar_expr_codegen_supported(ctx, value)
        }
        EvalLocalScalarStmt::If {
            branches,
            else_body,
        } => {
            branches.iter().all(|(condition, body)| {
                eval_local_scalar_expr_codegen_supported(ctx, condition)
                    && eval_local_scalar_stmts_codegen_supported(ctx, body)
            }) && eval_local_scalar_stmts_codegen_supported(ctx, else_body)
        }
        EvalLocalScalarStmt::While { condition, body } => {
            eval_local_scalar_expr_codegen_supported(ctx, condition)
                && eval_local_scalar_stmts_codegen_supported(ctx, body)
        }
        EvalLocalScalarStmt::DoWhile { body, condition } => {
            eval_local_scalar_stmts_codegen_supported(ctx, body)
                && eval_local_scalar_expr_codegen_supported(ctx, condition)
        }
        EvalLocalScalarStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_none_or(|stmt| {
                eval_local_scalar_stmts_codegen_supported(ctx, std::slice::from_ref(stmt))
            }) && condition
                .as_ref()
                .is_none_or(|condition| eval_local_scalar_expr_codegen_supported(ctx, condition))
                && update.as_deref().is_none_or(|stmt| {
                    eval_local_scalar_stmts_codegen_supported(ctx, std::slice::from_ref(stmt))
                })
                && eval_local_scalar_stmts_codegen_supported(ctx, body)
        }
        EvalLocalScalarStmt::Switch {
            subject,
            cases,
            default,
            default_index: _,
        } => {
            eval_local_scalar_expr_codegen_supported(ctx, subject)
                && cases.iter().all(|(conditions, body)| {
                    conditions
                        .iter()
                        .all(|condition| eval_local_scalar_expr_codegen_supported(ctx, condition))
                        && eval_local_scalar_stmts_codegen_supported(ctx, body)
                })
                && eval_local_scalar_stmts_codegen_supported(ctx, default)
        }
    })
}

/// Returns true when a local scalar expression is supported by codegen.
fn eval_local_scalar_expr_codegen_supported(
    ctx: &FunctionContext<'_>,
    expr: &EvalLocalScalarExpr,
) -> bool {
    match &expr.kind {
        EvalLocalScalarExprKind::Null
        | EvalLocalScalarExprKind::Int(_)
        | EvalLocalScalarExprKind::Float(_)
        | EvalLocalScalarExprKind::Bool(_)
        | EvalLocalScalarExprKind::String(_)
        | EvalLocalScalarExprKind::LoadVar(_)
        | EvalLocalScalarExprKind::Isset(_)
        | EvalLocalScalarExprKind::EmptyVar(_) => true,
        EvalLocalScalarExprKind::Negate(inner)
        | EvalLocalScalarExprKind::BitNot(inner)
        | EvalLocalScalarExprKind::Not(inner)
        | EvalLocalScalarExprKind::Print(inner) => {
            eval_local_scalar_expr_codegen_supported(ctx, inner)
        }
        EvalLocalScalarExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            eval_local_scalar_expr_codegen_supported(ctx, condition)
                && eval_local_scalar_expr_codegen_supported(ctx, then_expr)
                && eval_local_scalar_expr_codegen_supported(ctx, else_expr)
        }
        EvalLocalScalarExprKind::Binary { left, right, .. } => {
            eval_local_scalar_expr_codegen_supported(ctx, left)
                && eval_local_scalar_expr_codegen_supported(ctx, right)
        }
        EvalLocalScalarExprKind::StaticFunctionCall { name, args } => {
            eval_local_static_function_codegen_supported(ctx, name, args)
        }
    }
}

/// Returns true when a static user-function call can be emitted by local scalar AOT.
fn eval_local_static_function_codegen_supported(
    ctx: &FunctionContext<'_>,
    name: &str,
    args: &[EvalLocalScalarExpr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let Some(callee) = ctx.callable_function_by_name(name) else {
        return false;
    };
    if callee.params.len() != args.len() || callee.return_php_type.codegen_repr() != PhpType::Int {
        return false;
    }
    callee.params.iter().zip(args).all(|(param, arg)| {
        !param.by_ref
            && !param.variadic
            && matches!(
                (param.php_type.codegen_repr(), arg.ty),
                (PhpType::Int, EvalLocalScalarType::Int)
                    | (PhpType::Bool, EvalLocalScalarType::Bool)
            )
    })
}

/// Clears local value/defined slots before running the local scalar eval fragment.
fn emit_eval_local_scalar_init_slots(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    for name in program.locals.keys() {
        abi::emit_store_to_sp(ctx.emitter, result_reg, program.value_offset(name));
        abi::emit_store_to_sp(ctx.emitter, result_reg, program.value_aux_offset(name));
        abi::emit_store_to_sp(ctx.emitter, result_reg, program.defined_offset(name));
    }
}

/// Emits one local scalar AOT statement.
fn emit_eval_local_scalar_stmt(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    stmt: &EvalLocalScalarStmt,
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    match stmt {
        EvalLocalScalarStmt::Noop => {}
        EvalLocalScalarStmt::Echo(expr) => {
            emit_eval_local_scalar_echo_expr(ctx, program, expr, 0);
        }
        EvalLocalScalarStmt::Store { name, value } => {
            emit_eval_local_scalar_store(ctx, program, name, value);
        }
        EvalLocalScalarStmt::If {
            branches,
            else_body,
        } => {
            emit_eval_local_scalar_if(
                ctx,
                program,
                branches,
                else_body,
                loop_stack,
                return_label,
                boxing,
            );
        }
        EvalLocalScalarStmt::While { condition, body } => {
            emit_eval_local_scalar_while(
                ctx,
                program,
                condition,
                body,
                loop_stack,
                return_label,
                boxing,
            );
        }
        EvalLocalScalarStmt::DoWhile { body, condition } => {
            emit_eval_local_scalar_do_while(
                ctx,
                program,
                body,
                condition,
                loop_stack,
                return_label,
                boxing,
            );
        }
        EvalLocalScalarStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            emit_eval_local_scalar_for(
                ctx,
                program,
                init.as_deref(),
                condition.as_ref(),
                update.as_deref(),
                body,
                loop_stack,
                return_label,
                boxing,
            );
        }
        EvalLocalScalarStmt::Switch {
            subject,
            cases,
            default,
            default_index,
        } => {
            emit_eval_local_scalar_switch(
                ctx,
                program,
                subject,
                cases,
                default,
                *default_index,
                loop_stack,
                return_label,
                boxing,
            );
        }
        EvalLocalScalarStmt::Break(level) => {
            let target_index = loop_stack.len() - level;
            abi::emit_jump(ctx.emitter, &loop_stack[target_index].break_label);
        }
        EvalLocalScalarStmt::Continue(level) => {
            let target_index = loop_stack.len() - level;
            abi::emit_jump(ctx.emitter, &loop_stack[target_index].continue_label);
        }
        EvalLocalScalarStmt::Return(value) => {
            emit_eval_local_scalar_return_cell(ctx, program, value.as_ref(), boxing);
            abi::emit_jump(ctx.emitter, return_label);
        }
    }
}

/// Stores one local scalar expression into its stack slot and marks the slot defined.
fn emit_eval_local_scalar_store(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
    value: &EvalLocalScalarExpr,
) {
    emit_eval_local_scalar_expr_value(ctx, program, value, 0);
    emit_eval_local_scalar_store_current_value(ctx, program, name, value.ty);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_to_sp(ctx.emitter, result_reg, program.defined_offset(name));
}

/// Stores the current expression result registers into one local scalar slot.
fn emit_eval_local_scalar_store_current_value(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
    ty: EvalLocalScalarType,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ty {
        EvalLocalScalarType::String => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_sp(ctx.emitter, ptr_reg, program.value_offset(name));
            abi::emit_store_to_sp(ctx.emitter, len_reg, program.value_aux_offset(name));
        }
        EvalLocalScalarType::Null => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
            abi::emit_store_to_sp(ctx.emitter, result_reg, program.value_offset(name));
            abi::emit_store_to_sp(ctx.emitter, result_reg, program.value_aux_offset(name));
        }
        EvalLocalScalarType::Float => {
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                program.value_offset(name),
            );
        }
        EvalLocalScalarType::Int | EvalLocalScalarType::Bool => {
            abi::emit_store_to_sp(ctx.emitter, result_reg, program.value_offset(name));
        }
    }
}

/// Emits a local scalar if/elseif/else chain.
fn emit_eval_local_scalar_if(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    branches: &[(EvalLocalScalarExpr, Vec<EvalLocalScalarStmt>)],
    else_body: &[EvalLocalScalarStmt],
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    let done_label = ctx.next_label("eval_local_if_done");
    for (condition, body) in branches {
        let next_label = ctx.next_label("eval_local_if_next");
        emit_eval_local_scalar_expr_value(ctx, program, condition, 0);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &next_label);
        for stmt in body {
            emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
        }
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    for stmt in else_body {
        emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
    }
    ctx.emitter.label(&done_label);
}

/// Emits a local scalar while loop.
fn emit_eval_local_scalar_while(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    condition: &EvalLocalScalarExpr,
    body: &[EvalLocalScalarStmt],
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    let start_label = ctx.next_label("eval_local_while_start");
    let done_label = ctx.next_label("eval_local_while_done");
    ctx.emitter.label(&start_label);
    emit_eval_local_scalar_expr_value(ctx, program, condition, 0);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done_label);
    loop_stack.push(EvalLocalLoopLabels {
        break_label: done_label.clone(),
        continue_label: start_label.clone(),
    });
    for stmt in body {
        emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
    }
    loop_stack.pop();
    abi::emit_jump(ctx.emitter, &start_label);
    ctx.emitter.label(&done_label);
}

/// Emits a local scalar do/while loop.
fn emit_eval_local_scalar_do_while(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    body: &[EvalLocalScalarStmt],
    condition: &EvalLocalScalarExpr,
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    let start_label = ctx.next_label("eval_local_do_start");
    let condition_label = ctx.next_label("eval_local_do_condition");
    let done_label = ctx.next_label("eval_local_do_done");
    ctx.emitter.label(&start_label);
    loop_stack.push(EvalLocalLoopLabels {
        break_label: done_label.clone(),
        continue_label: condition_label.clone(),
    });
    for stmt in body {
        emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
    }
    loop_stack.pop();
    ctx.emitter.label(&condition_label);
    emit_eval_local_scalar_expr_value(ctx, program, condition, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &start_label);
    ctx.emitter.label(&done_label);
}

/// Emits a local scalar for loop with PHP continue-to-update behavior.
fn emit_eval_local_scalar_for(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    init: Option<&EvalLocalScalarStmt>,
    condition: Option<&EvalLocalScalarExpr>,
    update: Option<&EvalLocalScalarStmt>,
    body: &[EvalLocalScalarStmt],
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    if let Some(init) = init {
        emit_eval_local_scalar_stmt(ctx, program, init, loop_stack, return_label, boxing);
    }
    let start_label = ctx.next_label("eval_local_for_start");
    let update_label = ctx.next_label("eval_local_for_update");
    let done_label = ctx.next_label("eval_local_for_done");
    ctx.emitter.label(&start_label);
    if let Some(condition) = condition {
        emit_eval_local_scalar_expr_value(ctx, program, condition, 0);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &done_label);
    }
    loop_stack.push(EvalLocalLoopLabels {
        break_label: done_label.clone(),
        continue_label: update_label.clone(),
    });
    for stmt in body {
        emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
    }
    loop_stack.pop();
    ctx.emitter.label(&update_label);
    if let Some(update) = update {
        emit_eval_local_scalar_stmt(ctx, program, update, loop_stack, return_label, boxing);
    }
    abi::emit_jump(ctx.emitter, &start_label);
    ctx.emitter.label(&done_label);
}

/// Emits a local scalar switch with PHP-style fallthrough between case bodies.
fn emit_eval_local_scalar_switch(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    subject: &EvalLocalScalarExpr,
    cases: &[(Vec<EvalLocalScalarExpr>, Vec<EvalLocalScalarStmt>)],
    default: &[EvalLocalScalarStmt],
    default_index: Option<usize>,
    loop_stack: &mut Vec<EvalLocalLoopLabels>,
    return_label: &str,
    boxing: EvalLocalScalarBoxing,
) {
    let done_label = ctx.next_label("eval_local_switch_done");
    let default_label = default_index.map(|_| ctx.next_label("eval_local_switch_default"));
    let case_labels = cases
        .iter()
        .map(|_| ctx.next_label("eval_local_switch_case"))
        .collect::<Vec<_>>();
    emit_eval_local_scalar_expr_value(ctx, program, subject, 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.scratch_offset(0),
    );
    for ((conditions, _), case_label) in cases.iter().zip(&case_labels) {
        for condition in conditions {
            emit_eval_local_scalar_expr_value(ctx, program, condition, 1);
            let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
            abi::emit_reg_move(ctx.emitter, rhs_reg, abi::int_result_reg(ctx.emitter));
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                program.scratch_offset(0),
            );
            emit_eval_local_scalar_cmp(ctx, rhs_reg, "eq", "e");
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, case_label);
        }
    }
    if let Some(default_label) = &default_label {
        abi::emit_jump(ctx.emitter, default_label);
    } else {
        abi::emit_jump(ctx.emitter, &done_label);
    }
    loop_stack.push(EvalLocalLoopLabels {
        break_label: done_label.clone(),
        continue_label: done_label.clone(),
    });
    for index in 0..=cases.len() {
        if default_index == Some(index) {
            if let Some(default_label) = &default_label {
                ctx.emitter.label(default_label);
                for stmt in default {
                    emit_eval_local_scalar_stmt(
                        ctx,
                        program,
                        stmt,
                        loop_stack,
                        return_label,
                        boxing,
                    );
                }
            }
        }
        if index < cases.len() {
            ctx.emitter.label(&case_labels[index]);
            for stmt in &cases[index].1 {
                emit_eval_local_scalar_stmt(ctx, program, stmt, loop_stack, return_label, boxing);
            }
        }
    }
    loop_stack.pop();
    ctx.emitter.label(&done_label);
}

/// Emits an AOT echo expression, splitting concat into sequential writes.
fn emit_eval_local_scalar_echo_expr(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    expr: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    match &expr.kind {
        EvalLocalScalarExprKind::String(value) => {
            emit_eval_local_scalar_string_stdout(ctx, value.as_bytes());
        }
        EvalLocalScalarExprKind::Binary {
            op: EvalLocalScalarBinaryOp::Concat,
            left,
            right,
        } => {
            emit_eval_local_scalar_echo_expr(ctx, program, left, scratch_depth);
            emit_eval_local_scalar_echo_expr(ctx, program, right, scratch_depth);
        }
        _ => {
            emit_eval_local_scalar_expr_value(ctx, program, expr, scratch_depth);
            match expr.ty {
                EvalLocalScalarType::Null => {}
                EvalLocalScalarType::Bool => {
                    let skip_label = ctx.next_label("eval_local_echo_skip_false");
                    abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
                    abi::emit_write_stdout(ctx.emitter, &PhpType::Bool);
                    ctx.emitter.label(&skip_label);
                }
                EvalLocalScalarType::Int
                | EvalLocalScalarType::Float
                | EvalLocalScalarType::String => {
                    abi::emit_write_stdout(ctx.emitter, &expr.ty.php_type());
                }
            }
        }
    }
}

/// Emits a static string directly to stdout for local scalar AOT echo.
fn emit_eval_local_scalar_string_stdout(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
}

/// Emits one local scalar expression value into the integer result register.
fn emit_eval_local_scalar_expr_value(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    expr: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    match &expr.kind {
        EvalLocalScalarExprKind::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        EvalLocalScalarExprKind::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
        }
        EvalLocalScalarExprKind::Float(value) => {
            emit_eval_local_scalar_float_value(ctx, *value);
        }
        EvalLocalScalarExprKind::Bool(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                i64::from(*value),
            );
        }
        EvalLocalScalarExprKind::String(value) => {
            emit_eval_local_scalar_string_value(ctx, value);
        }
        EvalLocalScalarExprKind::LoadVar(name) => {
            emit_eval_local_scalar_load_local_value_as(ctx, program, name, expr.ty);
        }
        EvalLocalScalarExprKind::Isset(names) => {
            emit_eval_local_scalar_isset(ctx, program, names);
        }
        EvalLocalScalarExprKind::EmptyVar(name) => {
            emit_eval_local_scalar_empty_var(ctx, program, name);
        }
        EvalLocalScalarExprKind::Negate(inner) => {
            emit_eval_local_scalar_expr_value(ctx, program, inner, scratch_depth);
            emit_eval_local_scalar_negate(ctx, inner.ty);
        }
        EvalLocalScalarExprKind::BitNot(inner) => {
            emit_eval_local_scalar_expr_value(ctx, program, inner, scratch_depth);
            emit_eval_local_scalar_bitnot(ctx);
        }
        EvalLocalScalarExprKind::Not(inner) => {
            emit_eval_local_scalar_expr_value(ctx, program, inner, scratch_depth);
            emit_eval_local_scalar_not(ctx);
        }
        EvalLocalScalarExprKind::Print(inner) => {
            emit_eval_local_scalar_echo_expr(ctx, program, inner, scratch_depth);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        EvalLocalScalarExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            emit_eval_local_scalar_ternary_expr(
                ctx,
                program,
                condition,
                then_expr,
                else_expr,
                scratch_depth,
            );
        }
        EvalLocalScalarExprKind::Binary { op, left, right } => {
            emit_eval_local_scalar_binary_expr(ctx, program, op, left, right, scratch_depth);
        }
        EvalLocalScalarExprKind::StaticFunctionCall { name, args } => {
            emit_eval_local_scalar_static_function_call(ctx, program, name, args, scratch_depth);
        }
    }
}

/// Emits a static string into the local-scalar string result registers.
fn emit_eval_local_scalar_string_value(ctx: &mut FunctionContext<'_>, value: &str) {
    let (label, len) = ctx.data.add_string(value.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Emits a static float into the local-scalar float result register.
fn emit_eval_local_scalar_float_value(ctx: &mut FunctionContext<'_>, value: f64) {
    let label = ctx.data.add_float(value);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::float_result_reg(ctx.emitter), &label, 0);
}

/// Loads one local scalar slot into the result registers for its tracked type.
fn emit_eval_local_scalar_load_local_value(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
) {
    emit_eval_local_scalar_load_local_value_as(ctx, program, name, program.local_type(name));
}

/// Loads one local scalar slot into result registers using the type at the read point.
fn emit_eval_local_scalar_load_local_value_as(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
    ty: EvalLocalScalarType,
) {
    match ty {
        EvalLocalScalarType::String => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, program.value_offset(name));
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                len_reg,
                program.value_aux_offset(name),
            );
        }
        EvalLocalScalarType::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        EvalLocalScalarType::Float => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                program.value_offset(name),
            );
        }
        EvalLocalScalarType::Int | EvalLocalScalarType::Bool => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                program.value_offset(name),
            );
        }
    }
}

/// Emits PHP `isset()` for definitely local scalar variables.
fn emit_eval_local_scalar_isset(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    names: &[String],
) {
    let false_label = ctx.next_label("eval_local_isset_false");
    let done_label = ctx.next_label("eval_local_isset_done");
    for name in names {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            program.defined_offset(name),
        );
        abi::emit_branch_if_int_result_zero(ctx.emitter, &false_label);
        if program.local_type(name) == EvalLocalScalarType::Null {
            abi::emit_jump(ctx.emitter, &false_label);
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
}

/// Emits PHP `empty()` for a definitely local scalar variable.
fn emit_eval_local_scalar_empty_var(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
) {
    let true_label = ctx.next_label("eval_local_empty_true");
    let false_label = ctx.next_label("eval_local_empty_false");
    let done_label = ctx.next_label("eval_local_empty_done");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.defined_offset(name),
    );
    abi::emit_branch_if_int_result_zero(ctx.emitter, &true_label);
    match program.local_type(name) {
        EvalLocalScalarType::Null => {
            abi::emit_jump(ctx.emitter, &true_label);
        }
        EvalLocalScalarType::Int | EvalLocalScalarType::Bool => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                program.value_offset(name),
            );
            abi::emit_branch_if_int_result_zero(ctx.emitter, &true_label);
            abi::emit_jump(ctx.emitter, &false_label);
        }
        EvalLocalScalarType::Float => {
            emit_eval_local_scalar_load_local_value_as(
                ctx,
                program,
                name,
                EvalLocalScalarType::Float,
            );
            predicates::emit_float_result_nonzero_bool(ctx);
            abi::emit_branch_if_int_result_zero(ctx.emitter, &true_label);
            abi::emit_jump(ctx.emitter, &false_label);
        }
        EvalLocalScalarType::String => {
            emit_eval_local_scalar_empty_string(ctx, program, name, &true_label, &false_label);
        }
    }
    ctx.emitter.label(&true_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
}

/// Emits the PHP string truthiness check used by `empty()`.
fn emit_eval_local_scalar_empty_string(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
    true_label: &str,
    false_label: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, program.value_offset(name));
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, program.value_aux_offset(name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", len_reg));           // empty strings are falsey in PHP
            ctx.emitter.instruction(&format!("b.eq {}", true_label));           // branch when length is zero
            ctx.emitter.instruction(&format!("cmp {}, #1", len_reg));           // check for PHP's special string "0" empty case
            ctx.emitter.instruction(&format!("b.ne {}", false_label));          // non-empty non-"0" strings are truthy
            ctx.emitter.instruction(&format!("ldrb w11, [{}]", ptr_reg));       // load the only byte of the candidate "0" string
            ctx.emitter.instruction("cmp w11, #48");                            // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("b.eq {}", true_label));           // string "0" is empty in PHP truthiness
            ctx.emitter.instruction(&format!("b {}", false_label));             // any other one-byte string is truthy
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", len_reg));            // empty strings are falsey in PHP
            ctx.emitter.instruction(&format!("je {}", true_label));             // branch when length is zero
            ctx.emitter.instruction(&format!("cmp {}, 1", len_reg));            // check for PHP's special string "0" empty case
            ctx.emitter.instruction(&format!("jne {}", false_label));           // non-empty non-"0" strings are truthy
            ctx.emitter
                .instruction(&format!("movzx ecx, BYTE PTR [{}]", ptr_reg)); // load the only byte of the candidate "0" string
            ctx.emitter.instruction("cmp ecx, 48");                             // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("je {}", true_label));             // string "0" is empty in PHP truthiness
            ctx.emitter.instruction(&format!("jmp {}", false_label));           // any other one-byte string is truthy
        }
    }
}

/// Emits a local scalar ternary expression with same-typed branches.
fn emit_eval_local_scalar_ternary_expr(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    condition: &EvalLocalScalarExpr,
    then_expr: &EvalLocalScalarExpr,
    else_expr: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    let else_label = ctx.next_label("eval_local_ternary_else");
    let done_label = ctx.next_label("eval_local_ternary_done");
    emit_eval_local_scalar_expr_value(ctx, program, condition, scratch_depth);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &else_label);
    emit_eval_local_scalar_expr_value(ctx, program, then_expr, scratch_depth);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&else_label);
    emit_eval_local_scalar_expr_value(ctx, program, else_expr, scratch_depth);
    ctx.emitter.label(&done_label);
}

/// Emits a validated static user-function call for local scalar AOT.
fn emit_eval_local_scalar_static_function_call(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    name: &str,
    args: &[EvalLocalScalarExpr],
    scratch_depth: usize,
) {
    let arg_base_depth = scratch_depth;
    let eval_depth = scratch_depth + args.len();
    for (index, arg) in args.iter().enumerate() {
        emit_eval_local_scalar_expr_value(ctx, program, arg, eval_depth);
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            program.scratch_offset(arg_base_depth + index),
        );
    }
    for index in 0..args.len() {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, index),
            program.scratch_offset(arg_base_depth + index),
        );
    }
    abi::emit_call_label(ctx.emitter, &function_symbol(name.trim_start_matches('\\')));
}

/// Emits a local scalar binary expression into the integer result register.
fn emit_eval_local_scalar_binary_expr(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    op: &EvalLocalScalarBinaryOp,
    left: &EvalLocalScalarExpr,
    right: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    match op {
        EvalLocalScalarBinaryOp::And => {
            emit_eval_local_scalar_and(ctx, program, left, right, scratch_depth);
        }
        EvalLocalScalarBinaryOp::Or => {
            emit_eval_local_scalar_or(ctx, program, left, right, scratch_depth);
        }
        EvalLocalScalarBinaryOp::Div => {
            emit_eval_local_scalar_div(ctx, program, left, right, scratch_depth);
        }
        EvalLocalScalarBinaryOp::Mod => {
            emit_eval_local_scalar_mod_expr(ctx, program, left, right, scratch_depth);
        }
        EvalLocalScalarBinaryOp::Concat => unreachable!("concat is emitted as sequential echo"),
        _ => {
            emit_eval_local_scalar_expr_value(ctx, program, left, scratch_depth);
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_sp(
                ctx.emitter,
                result_reg,
                program.scratch_offset(scratch_depth),
            );
            emit_eval_local_scalar_expr_value(ctx, program, right, scratch_depth + 1);
            let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
            abi::emit_reg_move(ctx.emitter, rhs_reg, result_reg);
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                result_reg,
                program.scratch_offset(scratch_depth),
            );
            emit_eval_local_scalar_binary_result(ctx, op, rhs_reg);
        }
    }
}

/// Emits a numeric local-scalar division into the float result register.
fn emit_eval_local_scalar_div(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    left: &EvalLocalScalarExpr,
    right: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    emit_eval_local_scalar_expr_value(ctx, program, left, scratch_depth);
    emit_eval_local_scalar_store_numeric_scratch(ctx, program, scratch_depth, left.ty);
    emit_eval_local_scalar_expr_value(ctx, program, right, scratch_depth + 1);
    emit_eval_local_scalar_numeric_result_to_float(ctx, right.ty);
    let rhs_reg = abi::float_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_reg_move(ctx.emitter, rhs_reg, abi::float_result_reg(ctx.emitter));
    emit_eval_local_scalar_load_numeric_scratch(ctx, program, scratch_depth, left.ty);
    emit_eval_local_scalar_numeric_result_to_float(ctx, left.ty);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "fdiv {}, {}, {}",
                abi::float_result_reg(ctx.emitter),
                abi::float_result_reg(ctx.emitter),
                rhs_reg
            )); // compute the local scalar floating-point quotient
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "divsd {}, {}",
                abi::float_result_reg(ctx.emitter),
                rhs_reg
            )); // compute the local scalar floating-point quotient
        }
    }
}

/// Emits a numeric local-scalar modulo after PHP-style int coercion.
fn emit_eval_local_scalar_mod_expr(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    left: &EvalLocalScalarExpr,
    right: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    emit_eval_local_scalar_expr_value(ctx, program, left, scratch_depth);
    emit_eval_local_scalar_store_numeric_scratch(ctx, program, scratch_depth, left.ty);
    emit_eval_local_scalar_expr_value(ctx, program, right, scratch_depth + 1);
    emit_eval_local_scalar_numeric_result_to_int(ctx, right.ty);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, rhs_reg, abi::int_result_reg(ctx.emitter));
    emit_eval_local_scalar_load_numeric_scratch(ctx, program, scratch_depth, left.ty);
    emit_eval_local_scalar_numeric_result_to_int(ctx, left.ty);
    emit_eval_local_scalar_mod(ctx, rhs_reg);
}

/// Stores the current numeric result into a local-scalar scratch slot.
fn emit_eval_local_scalar_store_numeric_scratch(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    depth: usize,
    ty: EvalLocalScalarType,
) {
    let reg = match ty {
        EvalLocalScalarType::Float => abi::float_result_reg(ctx.emitter),
        EvalLocalScalarType::Int => abi::int_result_reg(ctx.emitter),
        EvalLocalScalarType::Null | EvalLocalScalarType::Bool | EvalLocalScalarType::String => {
            unreachable!("numeric scratch only accepts int/float local scalar values")
        }
    };
    abi::emit_store_to_sp(ctx.emitter, reg, program.scratch_offset(depth));
}

/// Loads a numeric scratch slot into the matching result register.
fn emit_eval_local_scalar_load_numeric_scratch(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    depth: usize,
    ty: EvalLocalScalarType,
) {
    let reg = match ty {
        EvalLocalScalarType::Float => abi::float_result_reg(ctx.emitter),
        EvalLocalScalarType::Int => abi::int_result_reg(ctx.emitter),
        EvalLocalScalarType::Null | EvalLocalScalarType::Bool | EvalLocalScalarType::String => {
            unreachable!("numeric scratch only accepts int/float local scalar values")
        }
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, reg, program.scratch_offset(depth));
}

/// Normalizes the current numeric result into the float result register.
fn emit_eval_local_scalar_numeric_result_to_float(
    ctx: &mut FunctionContext<'_>,
    ty: EvalLocalScalarType,
) {
    match ty {
        EvalLocalScalarType::Int => abi::emit_int_result_to_float_result(ctx.emitter),
        EvalLocalScalarType::Float => {}
        EvalLocalScalarType::Null | EvalLocalScalarType::Bool | EvalLocalScalarType::String => {
            unreachable!("numeric float coercion only accepts int/float local scalar values")
        }
    }
}

/// Normalizes the current numeric result into the integer result register.
fn emit_eval_local_scalar_numeric_result_to_int(
    ctx: &mut FunctionContext<'_>,
    ty: EvalLocalScalarType,
) {
    match ty {
        EvalLocalScalarType::Int => {}
        EvalLocalScalarType::Float => abi::emit_float_result_to_int_result(ctx.emitter),
        EvalLocalScalarType::Null | EvalLocalScalarType::Bool | EvalLocalScalarType::String => {
            unreachable!("numeric int coercion only accepts int/float local scalar values")
        }
    }
}

/// Emits a short-circuiting local scalar `&&` expression.
fn emit_eval_local_scalar_and(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    left: &EvalLocalScalarExpr,
    right: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    let false_label = ctx.next_label("eval_local_and_false");
    let done_label = ctx.next_label("eval_local_and_done");
    emit_eval_local_scalar_expr_value(ctx, program, left, scratch_depth);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_label);
    emit_eval_local_scalar_expr_value(ctx, program, right, scratch_depth);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
}

/// Emits a short-circuiting local scalar `||` expression.
fn emit_eval_local_scalar_or(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    left: &EvalLocalScalarExpr,
    right: &EvalLocalScalarExpr,
    scratch_depth: usize,
) {
    let true_label = ctx.next_label("eval_local_or_true");
    let done_label = ctx.next_label("eval_local_or_done");
    emit_eval_local_scalar_expr_value(ctx, program, left, scratch_depth);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_label);
    emit_eval_local_scalar_expr_value(ctx, program, right, scratch_depth);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&true_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
}

/// Emits unary numeric negation in the matching result register.
fn emit_eval_local_scalar_negate(ctx: &mut FunctionContext<'_>, ty: EvalLocalScalarType) {
    if ty == EvalLocalScalarType::Float {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fneg d0, d0");                         // negate the local scalar floating-point result
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorpd xmm1, xmm1");                    // materialize a zero float register for local scalar negation
                ctx.emitter.instruction("subsd xmm1, xmm0");                    // compute 0.0 minus the local scalar float
                ctx.emitter.instruction("movsd xmm0, xmm1");                    // move the negated local scalar float into the result register
            }
        }
        return;
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("neg {}, {}", result_reg, result_reg)); //negate the local scalar integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("neg {}", result_reg));            // negate the local scalar integer result
        }
    }
}

/// Emits a unary integer bitwise-not in the result register.
fn emit_eval_local_scalar_bitnot(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mvn {}, {}", result_reg, result_reg)); // invert every bit of the local scalar integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("not {}", result_reg));            // invert every bit of the local scalar integer result
        }
    }
}

/// Emits PHP boolean negation for an int/bool local scalar result.
fn emit_eval_local_scalar_not(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // test local scalar truthiness against false
            ctx.emitter.instruction(&format!("cset {}, eq", result_reg));       // materialize logical negation as 0 or 1
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg)); //test local scalar truthiness against false
            ctx.emitter.instruction("sete al");                                 // materialize logical negation in the low byte
            ctx.emitter
                .instruction(&format!("movzx {}, al", result_reg)); // widen logical negation into the result register
        }
    }
}

/// Emits the final arithmetic/comparison step for a local scalar binary operation.
fn emit_eval_local_scalar_binary_result(
    ctx: &mut FunctionContext<'_>,
    op: &EvalLocalScalarBinaryOp,
    rhs_reg: &str,
) {
    match op {
        EvalLocalScalarBinaryOp::Add => emit_eval_local_scalar_arith(ctx, "add", "add", rhs_reg),
        EvalLocalScalarBinaryOp::Sub => emit_eval_local_scalar_arith(ctx, "sub", "sub", rhs_reg),
        EvalLocalScalarBinaryOp::Mul => emit_eval_local_scalar_arith(ctx, "mul", "imul", rhs_reg),
        EvalLocalScalarBinaryOp::Mod => emit_eval_local_scalar_mod(ctx, rhs_reg),
        EvalLocalScalarBinaryOp::BitAnd => emit_eval_local_scalar_arith(ctx, "and", "and", rhs_reg),
        EvalLocalScalarBinaryOp::BitOr => emit_eval_local_scalar_arith(ctx, "orr", "or", rhs_reg),
        EvalLocalScalarBinaryOp::BitXor => emit_eval_local_scalar_arith(ctx, "eor", "xor", rhs_reg),
        EvalLocalScalarBinaryOp::ShiftLeft => {
            emit_eval_local_scalar_shift(ctx, "lsl", "shl", rhs_reg)
        }
        EvalLocalScalarBinaryOp::ShiftRight => {
            emit_eval_local_scalar_shift(ctx, "asr", "sar", rhs_reg)
        }
        EvalLocalScalarBinaryOp::Lt => emit_eval_local_scalar_cmp(ctx, rhs_reg, "lt", "l"),
        EvalLocalScalarBinaryOp::Gt => emit_eval_local_scalar_cmp(ctx, rhs_reg, "gt", "g"),
        EvalLocalScalarBinaryOp::LtEq => emit_eval_local_scalar_cmp(ctx, rhs_reg, "le", "le"),
        EvalLocalScalarBinaryOp::GtEq => emit_eval_local_scalar_cmp(ctx, rhs_reg, "ge", "ge"),
        EvalLocalScalarBinaryOp::Eq => emit_eval_local_scalar_cmp(ctx, rhs_reg, "eq", "e"),
        EvalLocalScalarBinaryOp::NotEq => emit_eval_local_scalar_cmp(ctx, rhs_reg, "ne", "ne"),
        EvalLocalScalarBinaryOp::And
        | EvalLocalScalarBinaryOp::Div
        | EvalLocalScalarBinaryOp::Or
        | EvalLocalScalarBinaryOp::Concat => unreachable!("handled before final binary step"),
    }
}

/// Emits a target-aware integer arithmetic operation for local scalar AOT.
fn emit_eval_local_scalar_arith(
    ctx: &mut FunctionContext<'_>,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
    rhs_reg: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "{} {}, {}, {}",
                aarch64_mnemonic, result_reg, result_reg, rhs_reg
            )); //compute the local scalar arithmetic result
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("{} {}, {}", x86_64_mnemonic, result_reg, rhs_reg));
            //update the local scalar arithmetic result
        }
    }
}

/// Emits a target-aware variable-count integer shift for local scalar AOT.
fn emit_eval_local_scalar_shift(
    ctx: &mut FunctionContext<'_>,
    aarch64_mnemonic: &str,
    x86_64_mnemonic: &str,
    rhs_reg: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "{} {}, {}, {}",
                aarch64_mnemonic, result_reg, result_reg, rhs_reg
            )); // shift the local scalar integer by the evaluated count
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rcx, {}", rhs_reg));          // move the local scalar shift count into x86_64's cl register
            ctx.emitter
                .instruction(&format!("{} {}, cl", x86_64_mnemonic, result_reg));
            // shift the local scalar integer by the low count byte
        }
    }
}

/// Emits a target-aware signed modulo operation for local scalar AOT.
fn emit_eval_local_scalar_mod(ctx: &mut FunctionContext<'_>, rhs_reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let zero_label = ctx.next_label("eval_local_mod_zero");
    let done_label = ctx.next_label("eval_local_mod_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let quotient_reg = abi::tertiary_scratch_reg(ctx.emitter);
            ctx.emitter
                .instruction(&format!("cbz {}, {}", rhs_reg, zero_label)); //branch to the local scalar modulo zero guard
            ctx.emitter.instruction(&format!(
                "sdiv {}, {}, {}",
                quotient_reg, result_reg, rhs_reg
            )); //compute the local scalar signed quotient
            ctx.emitter.instruction(&format!(
                "msub {}, {}, {}, {}",
                result_reg, quotient_reg, rhs_reg, result_reg
            )); //compute the local scalar signed remainder
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&zero_label);
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", rhs_reg, rhs_reg)); // test whether the local scalar modulo divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the local scalar modulo zero guard
            ctx.emitter.instruction("cqo");                                     // sign-extend the local scalar dividend before division
            ctx.emitter.instruction(&format!("idiv {}", rhs_reg));              // divide local scalar integers
            ctx.emitter.instruction(&format!("mov {}, rdx", result_reg));       // move the local scalar remainder into the result register
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&zero_label);
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
            ctx.emitter.label(&done_label);
        }
    }
}

/// Emits a target-aware integer comparison for local scalar AOT.
fn emit_eval_local_scalar_cmp(
    ctx: &mut FunctionContext<'_>,
    rhs_reg: &str,
    aarch64_condition: &str,
    x86_64_condition: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); //compare local scalar operands
            ctx.emitter
                .instruction(&format!("cset {}, {}", result_reg, aarch64_condition));
            //materialize the local scalar comparison result
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); //compare local scalar operands
            ctx.emitter
                .instruction(&format!("set{} al", x86_64_condition)); //materialize the local scalar comparison byte
            ctx.emitter
                .instruction(&format!("movzx {}, al", result_reg)); // widen the local scalar comparison result
        }
    }
}

/// Boxes the return expression, or null when eval exits without `return`.
fn emit_eval_local_scalar_return_cell(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    value: Option<&EvalLocalScalarExpr>,
    boxing: EvalLocalScalarBoxing,
) {
    if let Some(value) = value {
        emit_eval_local_scalar_expr_value(ctx, program, value, 0);
        emit_eval_local_scalar_box_current_result(ctx, value.ty, boxing);
    } else {
        emit_eval_local_scalar_null_cell(ctx, boxing);
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        program.result_cell_offset(),
    );
}

/// Emits a boxed eval null cell into the result register.
fn emit_eval_local_scalar_null_cell(ctx: &mut FunctionContext<'_>, boxing: EvalLocalScalarBoxing) {
    match boxing {
        EvalLocalScalarBoxing::EvalRuntime => {
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
        EvalLocalScalarBoxing::CoreRuntime => emit_eval_local_scalar_core_null_cell(ctx),
    }
}

/// Boxes the current int/bool result register as an eval Mixed cell.
fn emit_eval_local_scalar_box_current_result(
    ctx: &mut FunctionContext<'_>,
    ty: EvalLocalScalarType,
    boxing: EvalLocalScalarBoxing,
) {
    if matches!(boxing, EvalLocalScalarBoxing::CoreRuntime) {
        if ty == EvalLocalScalarType::Null {
            emit_eval_local_scalar_core_null_cell(ctx);
            return;
        }
        emit_box_current_value_as_mixed(ctx.emitter, &ty.php_type());
        return;
    }
    if ty == EvalLocalScalarType::Null {
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(ctx.emitter, &symbol);
        return;
    }
    if ty == EvalLocalScalarType::String {
        emit_eval_local_scalar_eval_runtime_string_cell(ctx);
        return;
    }
    if ty == EvalLocalScalarType::Float {
        emit_eval_local_scalar_eval_runtime_float_cell(ctx);
        return;
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_reg_move(ctx.emitter, arg_reg, result_reg);
    let helper = match ty {
        EvalLocalScalarType::Int => "__elephc_eval_value_int",
        EvalLocalScalarType::Bool => "__elephc_eval_value_bool",
        EvalLocalScalarType::Null | EvalLocalScalarType::Float | EvalLocalScalarType::String => {
            unreachable!("non-integer eval cells are handled before integer helpers")
        }
    };
    let symbol = ctx.emitter.target.extern_symbol(helper);
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Boxes the current float result register with the eval runtime float helper.
fn emit_eval_local_scalar_eval_runtime_float_cell(ctx: &mut FunctionContext<'_>) {
    let float_arg = abi::float_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_reg_move(ctx.emitter, float_arg, abi::float_result_reg(ctx.emitter));
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_value_float");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Boxes the current string result registers with the eval runtime string helper.
fn emit_eval_local_scalar_eval_runtime_string_cell(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let len_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_reg_move(ctx.emitter, ptr_arg, ptr_reg);
    abi::emit_reg_move(ctx.emitter, len_arg, len_reg);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_value_string");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Boxes PHP null using the core runtime Mixed helper, without eval bridge symbols.
fn emit_eval_local_scalar_core_null_cell(ctx: &mut FunctionContext<'_>) {
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

/// Flushes defined local scalar slots into eval scope once native execution completes.
fn emit_eval_local_scalar_flush_defined_locals(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
) {
    for name in program.locals.keys() {
        let skip_label = ctx.next_label("eval_local_flush_skip_undefined");
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            program.defined_offset(name),
        );
        abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
        emit_eval_local_scalar_load_local_value(ctx, program, name);
        emit_eval_local_scalar_box_current_result(
            ctx,
            program.local_type(name),
            EvalLocalScalarBoxing::EvalRuntime,
        );
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            EVAL_TEMP_CELL_OFFSET,
        );
        if main_name_uses_eval_global_scope(ctx, name) {
            emit_eval_global_scope_set_name(ctx, name, EVAL_SCOPE_FLAG_OWNED);
        } else {
            emit_eval_scope_set_name(ctx, name, EVAL_SCOPE_FLAG_OWNED);
        }
        ctx.emitter.label(&skip_label);
    }
}

/// Flushes defined local scalar slots directly into caller locals when no eval scope is needed.
fn emit_eval_local_scalar_flush_direct_locals(
    ctx: &mut FunctionContext<'_>,
    program: &EvalLocalScalarAotProgram,
    targets: &BTreeMap<String, Option<LocalSlotId>>,
) -> Result<()> {
    for name in program.locals.keys() {
        let Some(target) = targets.get(name) else {
            continue;
        };
        let Some(slot) = target else {
            continue;
        };
        let skip_label = ctx.next_label("eval_local_direct_flush_skip_undefined");
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            program.defined_offset(name),
        );
        abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
        emit_eval_local_scalar_load_local_value(ctx, program, name);
        emit_eval_local_scalar_store_current_result_to_direct_local(
            ctx,
            *slot,
            program.local_type(name),
        )?;
        ctx.emitter.label(&skip_label);
    }
    Ok(())
}

/// Stores the current local-scalar result in one caller local slot.
fn emit_eval_local_scalar_store_current_result_to_direct_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    local_type: EvalLocalScalarType,
) -> Result<()> {
    match ctx.local_php_type(slot)?.codegen_repr() {
        target_ty @ (PhpType::Mixed | PhpType::Union(_)) => {
            emit_eval_local_scalar_box_current_result(
                ctx,
                local_type,
                EvalLocalScalarBoxing::CoreRuntime,
            );
            // The caller slot may already hold a refcounted value written by
            // native code before the eval; release it before overwriting.
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            emit_eval_literal_release_old_direct_local_value(ctx, slot, &target_ty)?;
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            ctx.store_current_result_to_local(slot)
        }
        PhpType::TaggedScalar => match local_type {
            EvalLocalScalarType::Null => {
                crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
                ctx.store_current_result_to_local(slot)
            }
            EvalLocalScalarType::Int => {
                crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
                ctx.store_current_result_to_local(slot)
            }
            EvalLocalScalarType::Bool
            | EvalLocalScalarType::Float
            | EvalLocalScalarType::String => Err(CodegenIrError::unsupported(
                "direct local scalar eval sync to tagged scalar".to_string(),
            )),
        },
        target_ty
            if eval_local_scalar_direct_sync_type_supported(target_ty.clone(), local_type) =>
        {
            ctx.store_current_result_to_local(slot)
        }
        target_ty => Err(CodegenIrError::unsupported(format!(
            "direct local scalar eval sync to PHP type {:?}",
            target_ty
        ))),
    }
}

/// Boxes and echoes one AOT value, releasing the temporary box afterward.
fn emit_eval_literal_aot_echo(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotExpr,
    stack_depth: usize,
) {
    emit_eval_literal_aot_expr_cell(ctx, value, stack_depth);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_write_stdout");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
}

/// Stores one boxed scalar into the local or global eval scope used by bridge reloads.
fn emit_eval_literal_aot_scope_store(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    value: &EvalLiteralAotExpr,
    stack_depth: usize,
) -> Result<()> {
    emit_eval_literal_aot_expr_cell(ctx, value, stack_depth);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_TEMP_CELL_OFFSET);
    if main_name_uses_eval_global_scope(ctx, name) {
        emit_eval_global_scope_set_name(ctx, name, EVAL_SCOPE_FLAG_OWNED);
    } else {
        emit_eval_scope_set_name(ctx, name, EVAL_SCOPE_FLAG_OWNED);
    }
    Ok(())
}

/// Emits one AOT expression as an owned boxed Mixed value in the result register.
fn emit_eval_literal_aot_expr_cell(
    ctx: &mut FunctionContext<'_>,
    value: &EvalLiteralAotExpr,
    stack_depth: usize,
) {
    match value {
        EvalLiteralAotExpr::Scalar(value) => emit_eval_literal_aot_scalar_cell(ctx, value),
        EvalLiteralAotExpr::LoadVar(name) => {
            emit_eval_literal_aot_scope_load(ctx, name, stack_depth);
        }
        EvalLiteralAotExpr::Binary { op, left, right } => {
            emit_eval_literal_aot_binary_cell(ctx, op, left, right, stack_depth);
        }
    }
}

/// Emits a boxed-Mixed binary operation and releases owned operand cells.
fn emit_eval_literal_aot_binary_cell(
    ctx: &mut FunctionContext<'_>,
    op: &EvalLiteralAotBinaryOp,
    left: &EvalLiteralAotExpr,
    right: &EvalLiteralAotExpr,
    stack_depth: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    emit_eval_literal_aot_expr_cell(ctx, left, stack_depth);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_eval_literal_aot_expr_cell(ctx, right, stack_depth + 1);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        16,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        0,
    );
    let symbol = ctx.emitter.target.extern_symbol(op.helper());
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 32);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
}

/// Loads one variable from the eval scope and retains it as an owned Mixed cell.
fn emit_eval_literal_aot_scope_load(ctx: &mut FunctionContext<'_>, name: &str, stack_depth: usize) {
    let out_cell_offset = stack_depth * 16;
    let out_flags_offset = out_cell_offset + 8;
    if main_name_uses_eval_global_scope(ctx, name) {
        emit_eval_global_scope_get_name(ctx, name, out_cell_offset, out_flags_offset);
    } else {
        emit_eval_scope_get_name(ctx, name, out_cell_offset, out_flags_offset);
    }
    let missing = ctx.next_label("eval_literal_aot_load_missing");
    let done = ctx.next_label("eval_literal_aot_load_done");
    emit_branch_if_scope_entry_missing_at(ctx, out_flags_offset, &missing);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, out_cell_offset);
    let retain_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if retain_arg != result_reg {
        abi::emit_reg_move(ctx.emitter, retain_arg, result_reg);
    }
    let retain = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_value_retain");
    abi::emit_call_label(ctx.emitter, &retain);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&missing);
    let null = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(ctx.emitter, &null);
    ctx.emitter.label(&done);
}

/// Boxes one scalar AOT value into the standard eval `Mixed` return register.
fn emit_eval_literal_aot_scalar_cell(ctx: &mut FunctionContext<'_>, value: &EvalLiteralAotScalar) {
    match value {
        EvalLiteralAotScalar::Null => {
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
        EvalLiteralAotScalar::Bool(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 0),
                i64::from(*value),
            );
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_bool");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
        EvalLiteralAotScalar::Int(value) => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 0),
                *value,
            );
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_value_int");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
        EvalLiteralAotScalar::Float(value) => {
            let label = ctx.data.add_float(*value);
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::float_arg_reg_name(ctx.emitter.target, 0),
                &label,
                0,
            );
            let symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_value_float");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
        EvalLiteralAotScalar::String(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 0),
                &label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 1),
                len as i64,
            );
            let symbol = ctx
                .emitter
                .target
                .extern_symbol("__elephc_eval_value_string");
            abi::emit_call_label(ctx.emitter, &symbol);
        }
    }
}

/// Calls `__elephc_eval_scope_get` for a direct AOT local-scope load.
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

/// Calls `__elephc_eval_scope_get` for a direct AOT global-scope load.
fn emit_eval_global_scope_get_name(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    out_cell_offset: usize,
    out_flags_offset: usize,
) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    load_eval_global_scope_to_arg(ctx, 0);
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

/// Calls `__elephc_eval_scope_set` for a direct AOT local-scope store.
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

/// Calls `__elephc_eval_scope_set` for a direct AOT global-scope store.
fn emit_eval_global_scope_set_name(ctx: &mut FunctionContext<'_>, name: &str, flags: i64) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
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
