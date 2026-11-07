//! Purpose:
//! Lowers EIR callable invocation opcodes that need runtime dispatch.
//! Starts with runtime string callables that select among user functions.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime string callable dispatch preserves the callable name while
//!   comparing candidates, then reuses direct-call ABI materialization.
//! - Callable descriptors use a uniform invoker ABI with Mixed argument arrays;
//!   signature-dependent direct dispatch stays on explicit guarded paths.

use crate::codegen::platform::Arch;
use crate::codegen_support::data_section::DataWord;
use crate::codegen_support::callable_lookup::{
    emit_instance_lookup_table, emit_invokable_lookup_table, emit_static_lookup_table,
    emit_string_lookup_table, InstanceLookupCase, InvokableLookupCase, StaticLookupCase,
    StringLookupTable, INLINE_LOOKUP_LIMIT,
};
use crate::codegen::{
    abi, callable_descriptor, callable_dispatch, callable_invoker_args,
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_release_pushed_refcounted_temp_after_array_push,
};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{function_symbol, label_fragment, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::{FunctionSig, PhpType};

use super::super::context::FunctionContext;
use super::super::shared_state::{
    CallableResolverMiss, CallableResolverSpec, DynamicPropHelperKind,
    RuntimeInstanceMethodDescriptorTemplate,
};
use super::{
    class_method_already_emitted, class_method_body_exists, direct_call_stack_pad_bytes,
    emit_call_arg_temp_cleanups, emit_direct_resolved_method_call,
    emit_instance_method_descriptor_entry_wrapper, emit_ref_arg_writebacks,
    emit_runtime_builtin_wrapper_inline, emit_runtime_builtin_invoker_inline,
    emit_runtime_callable_invoker_inline,
    emit_runtime_callable_invoker_in_class,
    emit_runtime_callable_invoker_with_string_owner,
    emit_runtime_descriptor_with_receiver_capture, emit_runtime_extern_wrapper_inline,
    emit_static_method_descriptor_entry_wrapper, expect_operand, function_signature_from_eir,
    materialize_direct_call_args, materialize_method_call_args_with_receiver_reg_and_refs,
    runtime_builtin_wrapper_sig, store_call_result,
};
use crate::codegen::{CodegenIrError, Result};

mod instance_expr;
mod results;

const MIXED_METHOD_TAG_OFFSET: usize = 0;
const MIXED_METHOD_PAYLOAD_OFFSET: usize = 16;
const MIXED_RECEIVER_TAG_OFFSET: usize = 32;
const MIXED_RECEIVER_PAYLOAD_OFFSET: usize = 48;
const MIXED_SELECTOR_BYTES: usize = 64;
const MIXED_VALUE_PAYLOAD_OFFSET: usize = 16;
const MIXED_VALUE_BYTES: usize = 32;
const MIXED_TAG_STRING: i64 = 1;
const MIXED_TAG_OBJECT: i64 = 6;
const STRING_METHOD_OFFSET: usize = 0;
const STRING_CLASS_OFFSET: usize = 16;
const STRING_SELECTOR_BYTES: usize = 32;
const MIXED_TAG_CALLABLE: i64 = 10;
/// Runtime Mixed tag for a boxed indexed (list) array; payload-low holds the array pointer.
const MIXED_TAG_INDEXED_ARRAY: i64 = 4;

/// Returns the secondary result register carrying runtime descriptor ownership.
fn runtime_descriptor_owned_flag_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    }
}

/// Records whether the descriptor currently being resolved owns a runtime allocation.
fn emit_runtime_descriptor_owned_flag(ctx: &mut FunctionContext<'_>, owned: bool) {
    let flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    abi::emit_load_int_immediate(ctx.emitter, flag_reg, if owned { 1 } else { 0 });
}

/// Whether a runtime string callable name that no compiled function answers can still be
/// resolved by the eval bridge.
///
/// A function a runtime-loaded file declares -- `if (!function_exists('f')) { function f() {} }`
/// in a polyfill bootstrap that Composer includes by a computed path -- exists only in the
/// interpreter. The compiled candidate tables cannot name it, so without this fallback a
/// `$name(...)` call aborted with `Call to undefined function <dynamic>()` for a function PHP
/// calls fine (symfony/string's `UnicodeString::split()` calls `grapheme_strpos` this way).
fn runtime_string_callable_has_eval_fallback(ctx: &FunctionContext<'_>) -> bool {
    super::builtins::has_eval_context(ctx) || ctx.module.required_runtime_features.eval_bridge
}

/// Invokes a runtime string callable name through the eval bridge with a prebuilt argument
/// container, using the frame's own eval context when it has one.
fn lower_runtime_string_callable_eval_fallback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
) -> Result<()> {
    if super::builtins::has_eval_context(ctx) {
        super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)
    } else {
        super::builtins::lower_eval_global_callable_call_array(ctx, inst, callable, arg_mixed)
    }
}

/// Resolved user function candidate for a runtime string callable.
struct RuntimeStringFunctionTarget {
    name: String,
    param_types: Vec<PhpType>,
    return_ty: PhpType,
    /// Per-parameter argument-`TypeError` message prefix for a `false`-declared parameter
    /// (`f(): Argument #1 ($v) must be of type false, `), `None` elsewhere — the invocation
    /// guard needs the identity php names.
    false_param_prefixes: Vec<Option<String>>,
}

/// Spells a compile-time argument type the way php names it in an argument `TypeError`.
fn php_argument_type_name(ty: &PhpType) -> String {
    match ty {
        PhpType::Int => "int".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Str => "string".to_string(),
        PhpType::Bool => "bool".to_string(),
        PhpType::Array(_) => "array".to_string(),
        PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Iterable => "iterable".to_string(),
        PhpType::Callable => "callable".to_string(),
        PhpType::Object(name) => name.clone(),
        PhpType::Void | PhpType::Never => "null".to_string(),
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => "mixed".to_string(),
        other => format!("{other:?}"),
    }
}

/// Resolved public instance-method candidate for a runtime callable array.
///
/// The signature is shared: every callable-array site of a module selects from the same
/// module-wide candidate list, and cloning each candidate's signature per site was a large part
/// of a Symfony build's codegen.
#[derive(Clone)]
pub(in crate::codegen) struct RuntimeArrayInstanceMethodTarget {
    // Shared names: every callable-array site clones its scope's view of the module-wide target
    // list, and four fresh strings per target per site were most of that copy.
    class_name: std::sync::Arc<str>,
    class_id: u64,
    method_key: std::sync::Arc<str>,
    method_name: std::sync::Arc<str>,
    impl_class: std::sync::Arc<str>,
    /// `method_name` is public on `class_name`: decided once here, it was asked per target at
    /// every callable-array site that splits its table by visibility.
    public: bool,
    sig: std::sync::Arc<FunctionSig>,
}

/// Describes how the base pointer of a runtime callable array is materialized.
///
/// Both forms re-load the pointer on demand so it survives the `__rt_mixed_unbox`
/// calls that read each selector element.
enum CallableArraySource {
    /// The value's local slot already holds the raw indexed-array pointer.
    RawArray(ValueId),
    /// The value's local slot holds a boxed `Mixed` indexed array; unbox to reach it.
    BoxedArray(ValueId),
}

/// Lowers `$callable(...)` calls when the callable is a runtime string function name.
pub(super) fn lower_closure_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let callable = expect_operand(inst, 0)?;
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Str => lower_runtime_string_call(ctx, inst, callable, "closure_call"),
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            lower_runtime_mixed_callable_array_call(ctx, inst, callable, "closure_call")
        }
        PhpType::Callable => instance_expr::lower_instance_method_closure_call(ctx, inst, callable)
            .or_else(|_| lower_descriptor_invoker_call(ctx, inst, callable, "closure_call")),
        other => Err(CodegenIrError::unsupported(format!(
            "closure_call for callable PHP type {:?}",
            other
        ))),
    }
}

/// Lowers expression-call forms like `($expr)(...)` when the callee is a runtime string.
pub(super) fn lower_expr_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let callable = expect_operand(inst, 0)?;
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Str => lower_runtime_string_call(ctx, inst, callable, "expr_call"),
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            lower_runtime_mixed_callable_array_call(ctx, inst, callable, "expr_call")
        }
        PhpType::Callable => instance_expr::lower_instance_method_expr_call(ctx, inst, callable)
            .or_else(|_| lower_descriptor_invoker_call(ctx, inst, callable, "expr_call")),
        other => Err(CodegenIrError::unsupported(format!(
            "expr_call for callable PHP type {:?}",
            other
        ))),
    }
}

/// Lowers descriptor-invoker calls whose argument container was built in EIR.
pub(super) fn lower_callable_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let callable = expect_operand(inst, 0)?;
    let arg_mixed = expect_operand(inst, 1)?;
    require_descriptor_arg_container(ctx, arg_mixed, "callable_descriptor_invoke")?;
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Str => lower_runtime_string_descriptor_invoke(
            ctx,
            inst,
            callable,
            arg_mixed,
            "callable_descriptor_invoke",
        ),
        PhpType::Callable => lower_descriptor_invoker_call_with_mixed_arg(
            ctx,
            inst,
            callable,
            arg_mixed,
            "callable_descriptor_invoke",
        ),
        PhpType::Mixed | PhpType::Union(_) => lower_mixed_callable_descriptor_invoke(
            ctx,
            inst,
            callable,
            arg_mixed,
            "callable_descriptor_invoke",
        ),
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            lower_runtime_mixed_callable_array_descriptor_invoke(
                ctx,
                inst,
                callable,
                arg_mixed,
                "callable_descriptor_invoke",
            )
        }
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => {
            lower_runtime_string_callable_array_descriptor_invoke(
                ctx,
                inst,
                callable,
                arg_mixed,
                "callable_descriptor_invoke",
            )
        }
        PhpType::Object(class_name) => lower_invokable_object_descriptor_invoke(
            ctx,
            inst,
            callable,
            arg_mixed,
            &class_name,
            "callable_descriptor_invoke",
        ),
        other => Err(CodegenIrError::unsupported(format!(
            "callable_descriptor_invoke for callable PHP type {:?}",
            other
        ))),
    }
}

/// Lowers descriptor invocation when the callable traveled through a boxed Mixed value.
///
/// Unboxes the callback and dispatches every PHP callable runtime shape: string
/// function names, closure descriptors, invokable objects, and two-element
/// instance/static method arrays. Any other tag or malformed array is fatal.
///
/// Every tag test branches through `abi::emit_branch_if_equal_wide`. The arms are one per
/// callable target in the program, so on a large one a plain `b.eq` (±1 MiB on AArch64) no
/// longer reaches them and the ASSEMBLER refuses the whole program with `fixup value out of
/// range`. MEASURED on a two-class fixture whose only unusual feature is an `eval()`: 1.5M
/// lines of assembly and that error.
fn lower_mixed_callable_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    if let Some(label) = crate::codegen::shared_mixed_callable::shared_helper_label(
        ctx,
        inst,
        callable,
        arg_mixed,
    ) {
        return lower_shared_mixed_callable_descriptor_invoke(
            ctx,
            inst,
            callable,
            arg_mixed,
            &label,
        );
    }
    lower_mixed_callable_descriptor_invoke_inline(ctx, inst, callable, arg_mixed, op_name)
}

/// Calls the module-wide open Mixed callable dispatcher and stores its Mixed result.
fn lower_shared_mixed_callable_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    helper_label: &str,
) -> Result<()> {
    ctx.load_value_to_result(callable)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(arg_mixed)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let container_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, container_arg, 0);
    let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_int_immediate(ctx.emitter, kind_arg, callback_kind_of(ctx, inst));
    abi::emit_call_label(ctx.emitter, helper_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_descriptor_invoker_result(ctx, inst)
}

/// Emits the full open Mixed callable dispatcher at one inline or shared-helper site.
pub(in crate::codegen) fn lower_mixed_callable_descriptor_invoke_inline(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let has_eval_context = super::builtins::has_eval_context(ctx);
    if ctx.runtime_callable_candidates(callable).is_none()
        && !has_eval_context
    {
        return lower_open_mixed_callable_descriptor_invoke_via_value(
            ctx, inst, callable, arg_mixed, op_name,
        );
    }

    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let invokable_targets = instance_targets
        .iter()
        .filter(|target| &*target.method_key == "__invoke")
        .cloned()
        .collect::<Vec<_>>();
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    let array_label = (!instance_targets.is_empty()
        || !static_cases.is_empty()
        || module_has_unreachable_callable_methods(ctx))
    .then(|| ctx.next_label("mixed_callable_array"));
    // An eval-owned Closure is represented as a PHP object instead of an EIR
    // callable descriptor. Keep the object arm available whenever the eval
    // bridge is active so it can validate and invoke that object after native
    // `__invoke` candidates have been exhausted.
    let object_label = (!invokable_targets.is_empty() || has_eval_context)
        .then(|| ctx.next_label("mixed_callable_object"));
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let owned_flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    let string_label = ctx.next_label("mixed_callable_string_name");
    let callable_label = ctx.next_label("mixed_callable_closure");
    let fatal_label = ctx.next_label("mixed_callable_not_callable");
    let invoke_label = ctx.next_label("mixed_callable_invoke_resolved");
    let done_label = ctx.next_label("mixed_callable_done");
    let eval_fallback_slot = if crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(
        ctx.module,
    ) {
        super::builtins::eval::eval_context_local_slot(ctx)
    } else {
        None
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(callable, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");              // unbox → x0=tag, x1=payload lo, x2=payload hi
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_STRING)); // is the boxed Mixed payload a string function name?
            abi::emit_branch_if_equal_wide(ctx.emitter, &string_label);         // dispatch a boxed string-name callable
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_CALLABLE)); // is the boxed Mixed payload a callable descriptor?
            abi::emit_branch_if_equal_wide(ctx.emitter, &callable_label);       // dispatch a boxed closure/first-class callable descriptor
            if let Some(array_label) = &array_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_INDEXED_ARRAY)); // is the boxed Mixed payload a two-element callable array?
                abi::emit_branch_if_equal_wide(ctx.emitter, array_label);       // dispatch a boxed callable array past the string-name case table
            }
            if let Some(object_label) = &object_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_OBJECT)); // is the boxed Mixed payload an invokable object?
                abi::emit_branch_if_equal_wide(ctx.emitter, object_label);      // dispatch __invoke past the string-name and array dispatch blocks
            }
            abi::emit_jump(ctx.emitter, &fatal_label);
            ctx.emitter.label(&callable_label);
            ctx.emitter.instruction(&format!("mov {}, x1", descriptor_reg));    // keep the unboxed closure descriptor in the nested-call register
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(callable, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");             // unbox → rax=tag, rdi=payload lo, rdx=payload hi
            ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_STRING)); // is the boxed Mixed payload a string function name?
            ctx.emitter.instruction(&format!("je {}", string_label));           // dispatch a boxed string-name callable
            ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_CALLABLE)); // is the boxed Mixed payload a callable descriptor?
            ctx.emitter.instruction(&format!("je {}", callable_label));         // dispatch a boxed closure/first-class callable descriptor
            if let Some(array_label) = &array_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_INDEXED_ARRAY)); // is the boxed Mixed payload a two-element callable array?
                ctx.emitter.instruction(&format!("je {}", array_label));        // dispatch a boxed instance/static-method callable array
            }
            if let Some(object_label) = &object_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_OBJECT)); // is the boxed Mixed payload an invokable object?
                ctx.emitter.instruction(&format!("je {}", object_label));       // dispatch the object's public __invoke method
            }
            abi::emit_jump(ctx.emitter, &fatal_label);
            ctx.emitter.label(&callable_label);
            ctx.emitter.instruction(&format!("mov {}, rdi", descriptor_reg));   // keep the unboxed closure descriptor in the nested-call register
        }
    }
    // Closure descriptor (tag 10): keep borrowing the descriptor owned by the boxed Mixed value.
    emit_runtime_descriptor_owned_flag(ctx, false);
    abi::emit_jump(ctx.emitter, &invoke_label);

    // String function name (tag 1): the unboxed pointer/length are still in the
    // payload registers. Move them into the string ABI result pair, spill them onto
    // the temporary stack, and reuse the concrete-string per-candidate dispatch.
    ctx.emitter.label(&string_label);
    let string_eval_label = runtime_string_callable_has_eval_fallback(ctx)
        .then(|| ctx.next_label("mixed_callable_string_eval"));
    emit_runtime_string_descriptor_value_from_unboxed(
        ctx,
        callable,
        op_name,
        super::instruction_strict_php_profile(inst),
        None,
        string_eval_label.as_deref(),
        None,
        None,
    )?;
    abi::emit_reg_move(
        ctx.emitter,
        descriptor_reg,
        abi::int_result_reg(ctx.emitter),
    );
    emit_runtime_descriptor_owned_flag(ctx, false);
    abi::emit_jump(ctx.emitter, &invoke_label);
    if let Some(string_eval_label) = &string_eval_label {
        ctx.emitter.label(string_eval_label);
        lower_runtime_string_callable_eval_fallback(ctx, inst, callable, arg_mixed)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // Boxed indexed array (tag 4): `[$obj, "method"]` or `["Class", "method"]`.
    if let Some(array_label) = &array_label {
        ctx.emitter.label(array_label);
        emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::BoxedArray(callable))?;
        let array_miss_label = ctx.next_label("mixed_callable_array_missing");
        emit_mixed_callable_array_descriptor_lookup(
            ctx,
            &instance_targets,
            &static_cases,
            &array_miss_label,
            &invoke_inaccessible_method_error(ctx, inst),
        )?;
        abi::emit_reg_move(
            ctx.emitter,
            descriptor_reg,
            abi::int_result_reg(ctx.emitter),
        );
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        abi::emit_jump(ctx.emitter, &invoke_label);
        ctx.emitter.label(&array_miss_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        if has_eval_context {
            super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)?;
            abi::emit_jump(ctx.emitter, &done_label);
        } else {
            emit_runtime_callable_array_no_match_abort(ctx);
        }
    }

    // Boxed object (tag 6): dispatch its public `__invoke` implementation by
    // runtime class id, capturing the unboxed receiver in a descriptor. An
    // eval-owned Closure object has no static `__invoke` body in this module,
    // so unresolved cases fall through to Magician's callable bridge.
    if let Some(object_label) = &object_label {
        ctx.emitter.label(object_label);
        if invokable_targets.is_empty() {
            debug_assert!(has_eval_context);
            super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)?;
            abi::emit_jump(ctx.emitter, &done_label);
        } else {
            let object_miss_label = if has_eval_context {
                ctx.next_label("mixed_callable_object_eval")
            } else {
                fatal_label.clone()
            };
            emit_push_mixed_unbox_payload(ctx);
            emit_invokable_object_descriptor_lookup(
                ctx,
                &invokable_targets,
                MIXED_VALUE_PAYLOAD_OFFSET,
                &object_miss_label,
            )?;
            abi::emit_reg_move(
                ctx.emitter,
                descriptor_reg,
                abi::int_result_reg(ctx.emitter),
            );
            abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
            abi::emit_jump(ctx.emitter, &invoke_label);

            if has_eval_context {
                ctx.emitter.label(&object_miss_label);
                abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
                super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)?;
                abi::emit_jump(ctx.emitter, &done_label);
            }
        }
    }

    ctx.emitter.label(&invoke_label);
    emit_descriptor_reg_invoker_call_with_mixed_arg_and_runtime_ownership(
        ctx,
        inst,
        descriptor_reg,
        owned_flag_reg,
        arg_mixed,
        op_name,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&fatal_label);
    if let Some(slot) = eval_fallback_slot {
        // An eval closure object matched no native shape. Wrap it into an owned adapter
        // descriptor and invoke that; the invoker releases the descriptor afterwards.
        let miss = ctx.next_label("mixed_callable_eval_miss");
        emit_eval_closure_descriptor_or_branch(ctx, callable, slot, &miss)?;
        abi::emit_reg_move(ctx.emitter, descriptor_reg, abi::int_result_reg(ctx.emitter));
        emit_descriptor_reg_invoker_call_with_mixed_arg(
            ctx,
            inst,
            descriptor_reg,
            arg_mixed,
            op_name,
            true,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&miss);
    }
    emit_mixed_callable_not_callable_fatal(ctx, op_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// The diagnostic an unreachable method raises at a descriptor-invoke site.
///
/// Every descriptor invocation is a call of the callable, so php's direct-call `Error` applies
/// unless the instruction says it came from a callback-validating builtin.
fn invoke_inaccessible_method_error(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> InaccessibleMethodError {
    // The module-wide helper serves every call site, so which builtin the caller was is a run
    // time fact there: the call site passes it as the helper's third argument.
    if crate::codegen::shared_mixed_callable::is_shared_helper_name(&ctx.function.name) {
        return InaccessibleMethodError::Dynamic(crate::codegen::shared_mixed_callable::kind_value());
    }
    match callback_kind_of(ctx, inst) {
        CALLBACK_KIND_CALL_USER_FUNC => {
            InaccessibleMethodError::InvalidCallback(CALL_USER_FUNC_CALLBACK_PREFIX.to_string())
        }
        CALLBACK_KIND_CALL_USER_FUNC_ARRAY => InaccessibleMethodError::InvalidCallback(
            CALL_USER_FUNC_ARRAY_CALLBACK_PREFIX.to_string(),
        ),
        _ => InaccessibleMethodError::DirectCall,
    }
}

/// Callback kind of a descriptor invocation that is a plain call of the callable (`$f()`).
const CALLBACK_KIND_DIRECT: i64 = 0;
/// Callback kind of `call_user_func($callable, ...)`.
const CALLBACK_KIND_CALL_USER_FUNC: i64 = 1;
/// Callback kind of `call_user_func_array($callable, $args)`.
const CALLBACK_KIND_CALL_USER_FUNC_ARRAY: i64 = 2;
/// php's wording of a rejected `call_user_func()` callback, up to the reason.
const CALL_USER_FUNC_CALLBACK_PREFIX: &str =
    "call_user_func(): Argument #1 ($callback) must be a valid callback";
/// php's wording of a rejected `call_user_func_array()` callback, up to the reason.
const CALL_USER_FUNC_ARRAY_CALLBACK_PREFIX: &str =
    "call_user_func_array(): Argument #1 ($callback) must be a valid callback";

/// Classifies a descriptor invocation by the builtin the lowering recorded on it.
///
/// `call_user_func()` and `call_user_func_array()` carry their name as the instruction's data
/// immediate; every other invocation is a direct call of the callable.
fn callback_kind_of(ctx: &FunctionContext<'_>, inst: &Instruction) -> i64 {
    let Some(Immediate::ProfiledData { data, .. }) = inst.immediate else {
        return CALLBACK_KIND_DIRECT;
    };
    match ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
    {
        Some("call_user_func") => CALLBACK_KIND_CALL_USER_FUNC,
        Some("call_user_func_array") => CALLBACK_KIND_CALL_USER_FUNC_ARRAY,
        _ => CALLBACK_KIND_DIRECT,
    }
}

/// Resolves one open Mixed callable to a descriptor before invoking its uniform entry point.
fn lower_open_mixed_callable_descriptor_invoke_via_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let inaccessible = invoke_inaccessible_method_error(ctx, inst);
    emit_runtime_mixed_callable_descriptor_value_impl(
        ctx,
        callable,
        op_name,
        false,
        super::instruction_strict_php_profile(inst),
        None,
        None,
        None,
        Some(inaccessible),
    )?;
    let descriptor_reg = abi::nested_call_reg(ctx.emitter).to_string();
    let owned_flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reg_move(ctx.emitter, &descriptor_reg, &result_reg);
    emit_descriptor_reg_invoker_call_with_mixed_arg_and_runtime_ownership(
        ctx,
        inst,
        &descriptor_reg,
        owned_flag_reg,
        arg_mixed,
        op_name,
    )
}

/// Materializes a descriptor pointer for any callable shape boxed in `Mixed`.
///
/// PDO's SQLite callbacks keep this descriptor for later native invocation, so this normalizes
/// strings, callable arrays, invokable objects, and existing callable descriptors without calling
/// the PHP target at registration time.
pub(super) fn emit_runtime_mixed_callable_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    retain_existing_descriptor: bool,
    strict_php: bool,
) -> Result<()> {
    emit_runtime_mixed_callable_descriptor_value_impl(
        ctx,
        callable,
        op_name,
        retain_existing_descriptor,
        strict_php,
        None,
        None,
        None,
        None,
    )
}

/// Materializes a boxed callable descriptor while turning an unknown string name into `TypeError`.
pub(in crate::codegen) fn emit_runtime_mixed_callable_descriptor_value_with_string_type_error(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    retain_existing_descriptor: bool,
    message: &'static str,
) -> Result<()> {
    emit_runtime_mixed_callable_descriptor_value_impl(
        ctx,
        callable,
        op_name,
        retain_existing_descriptor,
        // `pcntl_signal()` is this variant's only caller and it lowers from a builtin argument
        // rather than from an instruction, so the compilation-wide profile is what it has.
        crate::strict_php::is_enabled(),
        Some(message),
        None,
        None,
        None,
    )
}

/// Acquires a boxed callback descriptor and throws the supplied TypeError on every invalid shape.
pub(super) fn emit_runtime_mixed_callable_descriptor_value_with_type_error(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    message: &'static str,
) -> Result<()> {
    emit_runtime_mixed_callable_descriptor_value_impl(
        ctx,
        callable,
        op_name,
        true,
        crate::strict_php::is_enabled(),
        Some(message),
        Some(message),
        None,
        None,
    )
}

/// Resolves a boxed callback whose host always invokes it with one PHP argument.
pub(in crate::codegen) fn emit_runtime_unary_callback_descriptor_value(
    ctx: &mut FunctionContext<'_>, callable: ValueId, op_name: &str, message: &'static str,
) -> Result<()> {
    emit_runtime_mixed_callable_descriptor_value_impl(
        ctx,
        callable,
        op_name,
        true,
        crate::strict_php::is_enabled(),
        Some(message),
        Some(message),
        Some(1),
        None,
    )
}

/// Implements boxed callable descriptor selection with a configurable string-name miss path.
#[allow(clippy::too_many_arguments)]
fn emit_runtime_mixed_callable_descriptor_value_impl(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    retain_existing_descriptor: bool,
    strict_php: bool,
    string_type_error: Option<&'static str>,
    invalid_type_error: Option<&'static str>,
    arity: Option<usize>,
    inaccessible_override: Option<InaccessibleMethodError>,
) -> Result<()> {
    let miss = match &inaccessible_override {
        None => Some(None),
        Some(InaccessibleMethodError::Ignore) => Some(Some(CallableResolverMiss::Ignore)),
        Some(InaccessibleMethodError::InvalidCallback(prefix)) => {
            Some(Some(CallableResolverMiss::InvalidCallback(prefix.clone())))
        }
        Some(InaccessibleMethodError::DirectCall) => Some(Some(CallableResolverMiss::DirectCall)),
        // Names a value of the frame it runs in, which a shared body does not have.
        Some(InaccessibleMethodError::Dynamic(_) | InaccessibleMethodError::AtSite(_)) => None,
    };
    if let Some(miss) = miss.filter(|_| callable_resolver_is_shareable(ctx, callable)) {
        let spec = CallableResolverSpec {
            op_name: op_name.to_string(),
            retain_existing_descriptor,
            strict_php,
            string_type_error,
            invalid_type_error,
            arity,
            miss,
        };
        // Only a direct call's refusal names the scope, so any other resolver of a scope that
        // reaches no non-public method is the global one.
        let scope = if matches!(spec.miss, Some(CallableResolverMiss::DirectCall)) {
            ctx.function
                .lexical_class
                .as_deref()
                .map(|class| class.trim_start_matches('\\').to_string())
        } else {
            crate::codegen::shared_mixed_callable::reach_scope(ctx)
        };
        let key = format!("{}:{:?}:{}", CALLABLE_RESOLVER_PREFIX, spec, scope.as_deref().unwrap_or(""));
        let label = ctx.keyed_global_label(&key, CALLABLE_RESOLVER_PREFIX);
        ctx.shared.request_dynamic_prop_helper(
            &key,
            &label,
            DynamicPropHelperKind::MixedCallableResolver(Box::new(spec)),
            scope.as_deref(),
        );
        ctx.load_value_to_result(callable)?;
        abi::emit_call_label(ctx.emitter, &label);
        return Ok(());
    }
    emit_inline_mixed_callable_descriptor_value(
        ctx,
        callable,
        op_name,
        retain_existing_descriptor,
        strict_php,
        string_type_error,
        invalid_type_error,
        arity,
        inaccessible_override,
    )
}

/// Readable part of the shared resolver's entry symbol.
const CALLABLE_RESOLVER_PREFIX: &str = "callable_resolve";

/// Whether a boxed callable may be resolved by a shared body.
///
/// The body is the inline sequence, so everything it reads must be a fact of the module or of the
/// helper's key (see [`CallableResolverSpec`]). What keeps a site inline is what ties the sequence
/// to its frame: the eval fallbacks read the frame's context, a finite set of callable names is
/// known about this value only, and a closure's runtime scope is a local of its body.
fn callable_resolver_is_shareable(ctx: &FunctionContext<'_>, callable: ValueId) -> bool {
    !ctx.module.required_runtime_features.eval_bridge
        && !super::builtins::has_eval_context(ctx)
        && !crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(
            ctx.module,
        )
        && ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_none()
        && ctx.function.name != super::callable_argument_normalizer::CALLABLE_ARGUMENT_NORMALIZER
        && !crate::codegen::shared_mixed_callable::is_shared_helper_name(&ctx.function.name)
        && ctx.runtime_callable_candidates(callable).is_none()
        && matches!(
            ctx.value_php_type(callable).map(|ty| ty.codegen_repr()),
            Ok(PhpType::Mixed | PhpType::Union(_))
        )
}

/// Emits one shared resolver: the inline sequence around a frame that gives its boxed callable a
/// home, returning the descriptor and its ownership flag in the registers the sequence leaves
/// them in. Takes the boxed callable in the integer result register.
pub(in crate::codegen) fn emit_mixed_callable_resolver(
    module: &crate::ir::Module,
    emitter: &mut crate::codegen_support::emit::Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    shared: &mut crate::codegen::shared_state::SharedCodegenState,
    label: &str,
    scope: Option<&str>,
    spec: &CallableResolverSpec,
) -> Result<()> {
    use crate::codegen::shared_helper::{helper_function, helper_value};

    let mut function = helper_function(label, PhpType::Callable);
    function.lexical_class = scope.map(str::to_string);
    let layout = crate::codegen::frame::layout_for_function(
        &function, emitter.target, false, false, false,
    );
    let saved_nested_offset = layout.frame_size - 8;
    let frame_size = layout.frame_size + 16;
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );
    ctx.emitter.blank();
    ctx.emitter.comment(&format!("--- shared callable resolver: {} ---", label));
    ctx.emitter.label(label);
    abi::emit_frame_prologue(ctx.emitter, frame_size);
    let nested = abi::nested_call_reg(ctx.emitter);
    abi::store_at_offset(ctx.emitter, nested, saved_nested_offset);
    ctx.store_result_value(helper_value())?;
    let inaccessible = spec.miss.as_ref().map(|miss| match miss {
        CallableResolverMiss::Ignore => InaccessibleMethodError::Ignore,
        CallableResolverMiss::InvalidCallback(prefix) => {
            InaccessibleMethodError::InvalidCallback(prefix.clone())
        }
        CallableResolverMiss::DirectCall | CallableResolverMiss::DirectCallAtSite => {
            InaccessibleMethodError::DirectCall
        }
    });
    emit_inline_mixed_callable_descriptor_value(
        &mut ctx,
        helper_value(),
        &spec.op_name,
        spec.retain_existing_descriptor,
        spec.strict_php,
        spec.string_type_error,
        spec.invalid_type_error,
        spec.arity,
        inaccessible,
    )?;
    abi::load_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}

/// Emits the resolution sequence itself; [`emit_runtime_mixed_callable_descriptor_value_impl`]
/// decides whether it runs at the site or in a shared body.
#[allow(clippy::too_many_arguments)]
fn emit_inline_mixed_callable_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    retain_existing_descriptor: bool,
    strict_php: bool,
    string_type_error: Option<&'static str>,
    invalid_type_error: Option<&'static str>,
    arity: Option<usize>,
    inaccessible_override: Option<InaccessibleMethodError>,
) -> Result<()> {
    let has_eval_callback_owner_fallback = ctx.module.required_runtime_features.eval_bridge;
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let invokable_targets = instance_targets
        .iter()
        .filter(|target| &*target.method_key == "__invoke")
        .cloned()
        .collect::<Vec<_>>();
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    let array_label = (!instance_targets.is_empty()
        || !static_cases.is_empty()
        || has_eval_callback_owner_fallback
        || module_has_unreachable_callable_methods(ctx))
        .then(|| ctx.next_label("mixed_callable_value_array"));
    let object_label = (!invokable_targets.is_empty() || has_eval_callback_owner_fallback)
        .then(|| ctx.next_label("mixed_callable_value_object"));
    let descriptor_label = ctx.next_label("mixed_callable_value_descriptor");
    let string_label = ctx.next_label("mixed_callable_value_string");
    let fatal_label = ctx.next_label("mixed_callable_value_not_callable");
    let done_label = ctx.next_label("mixed_callable_value_done");
    // Only an owned result can carry a freshly built adapter descriptor back to the caller.
    let eval_fallback_slot = if retain_existing_descriptor
        && crate::codegen::eval_callable_helpers::module_needs_eval_callable_descriptor_support(
            ctx.module,
        ) {
        super::builtins::eval::eval_context_local_slot(ctx)
    } else {
        None
    };

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(callable, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_CALLABLE)); // classify an existing callable descriptor
            abi::emit_branch_if_equal_wide(ctx.emitter, &descriptor_label);     // return the existing descriptor payload
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_STRING)); // classify a runtime callable name
            abi::emit_branch_if_equal_wide(ctx.emitter, &string_label);         // resolve the callable name through the descriptor table
            if let Some(array_label) = &array_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_INDEXED_ARRAY)); // classify a two-element callable array
                abi::emit_branch_if_equal_wide(ctx.emitter, array_label);       // resolve an instance/static method descriptor across large string dispatch tables
            }
            if let Some(object_label) = &object_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_OBJECT)); // classify an invokable object
                abi::emit_branch_if_equal_wide(ctx.emitter, object_label);      // bind the public __invoke descriptor across large generated dispatch blocks
            }
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(callable, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_CALLABLE)); // classify an existing callable descriptor
            ctx.emitter.instruction(&format!("je {}", descriptor_label));       // return the existing descriptor payload
            ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_STRING)); // classify a runtime callable name
            ctx.emitter.instruction(&format!("je {}", string_label));           // resolve the callable name through the descriptor table
            if let Some(array_label) = &array_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_INDEXED_ARRAY)); // classify a two-element callable array
                ctx.emitter.instruction(&format!("je {}", array_label));        // resolve an instance/static method descriptor
            }
            if let Some(object_label) = &object_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_OBJECT)); // classify an invokable object
                ctx.emitter.instruction(&format!("je {}", object_label));       // bind the public __invoke descriptor
            }
        }
    }
    abi::emit_jump(ctx.emitter, &fatal_label);

    ctx.emitter.label(&descriptor_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // return the unboxed descriptor payload
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // return the unboxed descriptor payload
    }
    if retain_existing_descriptor {
        callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    }
    emit_runtime_descriptor_owned_flag(ctx, retain_existing_descriptor);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&string_label);
    // A name only a runtime-loaded file declared is still callable: the eval bridge captures it
    // in an owned adapter descriptor, exactly as it does an eval-owned Closure below. That holds
    // for a `callable` parameter too (whose normalizer throws TypeError on a miss): an
    // eval-declared function passed by name to a compiled `callable $f` is a valid callback.
    let string_eval_label =
        has_eval_callback_owner_fallback.then(|| ctx.next_label("mixed_callable_value_string_eval"));
    emit_runtime_string_descriptor_value_from_unboxed(
        ctx,
        callable,
        op_name,
        strict_php,
        string_type_error,
        string_eval_label.as_deref(),
        None,
        arity,
    )?;
    emit_runtime_descriptor_owned_flag(ctx, false);
    abi::emit_jump(ctx.emitter, &done_label);
    if let Some(string_eval_label) = &string_eval_label {
        // The eval bridge does not know the name either: PHP's undefined-function fatal, as
        // before the fallback existed.
        let undefined_label = ctx.next_label("mixed_callable_value_string_undefined");
        ctx.emitter.label(string_eval_label);
        emit_eval_owned_callable_descriptor(ctx, callable, &undefined_label)?;
        emit_runtime_descriptor_owned_flag(ctx, true);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&undefined_label);
        match string_type_error {
            Some(message) => super::exceptions::emit_type_error(ctx, message),
            None => emit_undefined_runtime_string_call_fatal(ctx),
        }
    }

    if let Some(array_label) = &array_label {
        ctx.emitter.label(array_label);
        emit_mixed_callable_array_selector_slots_with_error(
            ctx, &CallableArraySource::BoxedArray(callable), invalid_type_error,
        )?;
        let selected_label = ctx.next_label("mixed_callable_value_array_done");
        let array_miss_label = ctx.next_label("mixed_callable_value_array_missing");
        let inaccessible = inaccessible_override.clone().unwrap_or(match invalid_type_error {
            Some(message) => InaccessibleMethodError::InvalidCallback(message.to_string()),
            None => InaccessibleMethodError::Ignore,
        });
        emit_mixed_callable_array_descriptor_lookup(
            ctx,
            &instance_targets,
            &static_cases,
            &array_miss_label,
            &inaccessible,
        )?;
        abi::emit_jump(ctx.emitter, &selected_label);
        ctx.emitter.label(&array_miss_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        if has_eval_callback_owner_fallback {
            emit_eval_owned_callable_descriptor(ctx, callable, &fatal_label)?;
            emit_runtime_descriptor_owned_flag(ctx, true);
            abi::emit_jump(ctx.emitter, &done_label);
        } else if let Some(message) = invalid_type_error {
            super::exceptions::emit_type_error(ctx, message);
        } else {
            emit_runtime_callable_array_no_match_abort(ctx);
        }
        ctx.emitter.label(&selected_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    if let Some(object_label) = &object_label {
        ctx.emitter.label(object_label);
        if invokable_targets.is_empty() {
            debug_assert!(has_eval_callback_owner_fallback);
            emit_eval_owned_callable_descriptor(ctx, callable, &fatal_label)?;
            emit_runtime_descriptor_owned_flag(ctx, true);
            abi::emit_jump(ctx.emitter, &done_label);
        } else {
            emit_push_mixed_unbox_payload(ctx);
            let selected_label = ctx.next_label("mixed_callable_value_object_done");
            let object_miss_label = ctx.next_label("mixed_callable_value_object_missing");
            emit_invokable_object_descriptor_lookup(
                ctx,
                &invokable_targets,
                MIXED_VALUE_PAYLOAD_OFFSET,
                &object_miss_label,
            )?;
            abi::emit_jump(ctx.emitter, &selected_label);
            ctx.emitter.label(&object_miss_label);
            abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
            if has_eval_callback_owner_fallback {
                emit_eval_owned_callable_descriptor(ctx, callable, &fatal_label)?;
                emit_runtime_descriptor_owned_flag(ctx, true);
                abi::emit_jump(ctx.emitter, &done_label);
            } else if eval_fallback_slot.is_some() {
                abi::emit_jump(ctx.emitter, &fatal_label);
            } else if let Some(message) = invalid_type_error {
                super::exceptions::emit_type_error(ctx, message);
            } else {
                emit_mixed_callable_not_callable_fatal(ctx, op_name);
            }
            ctx.emitter.label(&selected_label);
            abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
            abi::emit_jump(ctx.emitter, &done_label);
        }
    }

    ctx.emitter.label(&fatal_label);
    if let Some(slot) = eval_fallback_slot {
        emit_eval_closure_descriptor_fallback(ctx, callable, slot, &done_label)?;
    }
    if let Some(message) = invalid_type_error {
        super::exceptions::emit_type_error(ctx, message);
    } else {
        emit_mixed_callable_not_callable_fatal(ctx, op_name);
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Builds an owned descriptor for a Closure object that outlived the eval frame that created it.
///
/// Magician registers each dynamic Closure object in the process-local object-owner table. This
/// lookup recovers that persistent context, validates the boxed value with Magician, and captures
/// both pointers in the existing callback-adapter descriptor used by eval-to-native bridges.
fn emit_eval_owned_callable_descriptor(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    fail_label: &str,
) -> Result<()> {
    // The eval ABI reads the callback through a boxed Mixed cell, which is what the boxed-Mixed
    // caller already holds. A statically typed callable array (`[$object, 'method']` whose
    // element type the checker knew) arrives as a bare container pointer instead, so it is boxed
    // here — otherwise the resolver reads a cell header that is not one.
    let callable_repr = ctx.value_php_type(callable)?.codegen_repr();
    let boxed_here = callable_repr != PhpType::Mixed;

    ctx.load_value_to_result(callable)?;
    if boxed_here {
        emit_box_current_value_as_mixed(ctx.emitter, &callable_repr);
    }
    emit_eval_owned_callable_descriptor_from_result(ctx, boxed_here, fail_label);
    Ok(())
}

/// Builds the eval-owned descriptor for `[$receiver, 'method']` when no compiled class matched
/// a boxed receiver: `$component->render(...$args)` on an object the interpreter declared went
/// through a native-only candidate ladder and died as "Call to a member function render() on
/// null" (Laravel's console components). The pair is built as a boxed callable array, which
/// the eval bridge resolves like any `[$object, 'method']` callable.
pub(super) fn emit_eval_owned_bound_method_descriptor(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    method_name: &str,
    fail_label: &str,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 4);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, &result_reg, &PhpType::Mixed);
    abi::emit_push_reg(ctx.emitter, &result_reg);
    let receiver_ty = ctx.load_value_to_result(receiver)?.codegen_repr();
    super::reference_arguments::emit_push_current_value_onto_array(ctx, &receiver_ty, &result_reg);
    abi::emit_push_reg(ctx.emitter, &result_reg);
    let (label, len) = ctx.data.add_string(method_name.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    super::reference_arguments::emit_push_current_value_onto_array(ctx, &PhpType::Str, &result_reg);
    crate::codegen::emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(PhpType::Mixed)),
    );
    emit_eval_owned_callable_descriptor_from_result(ctx, true, fail_label);
    emit_runtime_descriptor_owned_flag(ctx, true);
    Ok(())
}

/// [`emit_eval_owned_callable_descriptor`] for a boxed callable cell already in the integer result
/// register; `boxed_here` says the cell's one reference is this code's to hand to the descriptor.
fn emit_eval_owned_callable_descriptor_from_result(
    ctx: &mut FunctionContext<'_>,
    boxed_here: bool,
    fail_label: &str,
) {
    const CONTEXT_OFFSET: usize = 0;
    const CALLBACK_OFFSET: usize = 16;
    const SCRATCH_BYTES: usize = 32;
    const CAPTURE_BYTES: usize = 32;
    const CONTEXT_CAPTURE: usize = 0;
    const CALLBACK_CAPTURE: usize = 1;

    let descriptor_label = crate::codegen::eval_callable_helpers::eval_dynamic_callable_descriptor(ctx.data);
    let capture_offset = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET;
    let total_bytes = capture_offset + CAPTURE_BYTES;
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let descriptor_reg = abi::nested_call_reg(ctx.emitter).to_string();
    let failed_label = ctx.next_label("eval_owned_callable_descriptor_failed");
    let done_label = ctx.next_label("eval_owned_callable_descriptor_done");

    abi::emit_reserve_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, CALLBACK_OFFSET);

    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, CALLBACK_OFFSET);
    let owner_symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_callable_owner_context");
    abi::emit_call_label(ctx.emitter, &owner_symbol);
    abi::emit_store_to_sp(ctx.emitter, &result_reg, CONTEXT_OFFSET);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &failed_label);

    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, context_arg, CONTEXT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, callback_arg, CALLBACK_OFFSET);
    let is_callable_symbol = ctx.emitter.target.extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(ctx.emitter, &is_callable_symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &failed_label);

    if !boxed_here {
        // The descriptor takes an owner on the cell it captures. A cell boxed just above already
        // carries exactly that one owner, so only a borrowed caller cell is retained here.
        abi::emit_load_temporary_stack_slot(ctx.emitter, &result_reg, CALLBACK_OFFSET);
        abi::emit_call_label(ctx.emitter, "__rt_incref");
    }
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    abi::emit_reg_move(ctx.emitter, &descriptor_reg, &result_reg);
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        &descriptor_reg,
        &descriptor_label,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", CONTEXT_OFFSET);
            abi::emit_store_to_address(
                ctx.emitter,
                "x10",
                &descriptor_reg,
                capture_offset + CONTEXT_CAPTURE * 16,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", CALLBACK_OFFSET);
            abi::emit_store_to_address(
                ctx.emitter,
                "x10",
                &descriptor_reg,
                capture_offset + CALLBACK_CAPTURE * 16,
            );
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", CONTEXT_OFFSET);
            abi::emit_store_to_address(
                ctx.emitter,
                "r10",
                &descriptor_reg,
                capture_offset + CONTEXT_CAPTURE * 16,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", CALLBACK_OFFSET);
            abi::emit_store_to_address(
                ctx.emitter,
                "r10",
                &descriptor_reg,
                capture_offset + CALLBACK_CAPTURE * 16,
            );
        }
    }
    abi::emit_reg_move(ctx.emitter, &result_reg, &descriptor_reg);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&failed_label);
    abi::emit_release_temporary_stack(ctx.emitter, SCRATCH_BYTES);
    abi::emit_jump(ctx.emitter, fail_label);
    ctx.emitter.label(&done_label);
}

/// Hands a boxed value no native shape matched to Magician before calling it not callable.
///
/// A closure created by `eval()` is an eval `Closure` object: runtime tag 6 with no generated
/// class behind it, so every native arm misses. When this function owns an eval context,
/// `__elephc_eval_is_callable` recognises the object and the adapter wrapper turns it into the
/// descriptor `__elephc_eval_dynamic_callable_invoker` dispatches. A context that was never
/// created (the slot is still zero) cannot own such an object, so that case keeps the fatal.
fn emit_eval_closure_descriptor_fallback(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    context_slot: crate::ir::LocalSlotId,
    done_label: &str,
) -> Result<()> {
    let miss = ctx.next_label("mixed_callable_eval_miss");
    emit_eval_closure_descriptor_or_branch(ctx, callable, context_slot, &miss)?;
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&miss);
    Ok(())
}

/// Leaves an owned adapter descriptor for an eval callback in the result register, or branches.
///
/// Branches to `miss` when this function never created an eval context or when Magician does
/// not recognise the boxed value as callable; both leave the registers as the caller's fatal
/// path expects. Shared by the descriptor-value and the direct-invoke ladders.
fn emit_eval_closure_descriptor_or_branch(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    context_slot: crate::ir::LocalSlotId,
    miss: &str,
) -> Result<()> {
    let target = ctx.emitter.target;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let (arg0, arg1) = (abi::int_arg_reg_name(target, 0), abi::int_arg_reg_name(target, 1));
    let offset = ctx.local_offset(context_slot)?;
    abi::load_at_offset(ctx.emitter, result_reg, offset);
    abi::emit_branch_if_int_result_zero(ctx.emitter, miss);
    abi::emit_reg_move(ctx.emitter, arg0, result_reg);
    ctx.load_value_to_reg(callable, arg1)?;
    let is_callable = target.extern_symbol("__elephc_eval_is_callable");
    abi::emit_call_label(ctx.emitter, &is_callable);
    abi::emit_branch_if_int_result_zero(ctx.emitter, miss);
    abi::load_at_offset(ctx.emitter, arg0, offset);
    ctx.load_value_to_reg(callable, arg1)?;
    abi::emit_call_label(
        ctx.emitter,
        crate::codegen::eval_callable_helpers::EVAL_CALLBACK_WRAPPER_LABEL,
    );
    Ok(())
}

/// Emits a fatal diagnostic for a boxed Mixed value that is called but is not callable.
///
/// The enclosing function is part of the message ON PURPOSE. This aborts the process with a raw
/// write, so there is no backtrace and no PHP frame to inspect afterwards; without the name, a
/// report from a large application says only that SOME call site refused SOME value, and finding
/// which one means disassembling the binary.
fn emit_mixed_callable_not_callable_fatal(ctx: &mut FunctionContext<'_>, op_name: &str) {
    let message = format!(
        "Fatal error: Unsupported EIR {} mixed value is not callable in {}\n",
        op_name, ctx.function.name,
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the non-callable Mixed diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label); // load the non-callable Mixed diagnostic page
            ctx.emitter.add_lo12("x1", "x1", &message_label); // resolve the non-callable Mixed diagnostic address
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the non-callable Mixed diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the non-callable Mixed diagnostic to stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the non-callable Mixed diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the non-callable Mixed diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Selects a descriptor for a runtime string callable and invokes it with a Mixed arg container.
fn lower_runtime_string_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let candidate_names = ctx.runtime_callable_candidates(callable);
    let cases = runtime_string_descriptor_cases(
        ctx,
        None,
        candidate_names.as_deref(),
        super::instruction_strict_php_profile(inst),
    )?;
    if cases.is_empty() {
        if runtime_string_callable_has_eval_fallback(ctx) {
            return lower_runtime_string_callable_eval_fallback(ctx, inst, callable, arg_mixed);
        }
        return Err(CodegenIrError::unsupported(
            "callable_descriptor_invoke for runtime string with no descriptor targets",
        ));
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(callable, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_string_name_descriptor_cases_loop(ctx, inst, callable, arg_mixed, &cases, op_name)
}

/// Emits the per-candidate string function-name dispatch loop.
///
/// Assumes the callable's `(ptr, len)` are already pushed onto the temporary stack
/// (pointer at `[sp]`, length at `[sp + 8]`) and `cases` is non-empty. Each case
/// compares the pushed name against a candidate function and, on a match, invokes
/// that function's descriptor with `arg_mixed`; a miss is a fatal "undefined
/// function" call. The 16-byte name spill is released before returning. Shared by
/// the concrete-string and boxed-Mixed string-callable descriptor paths.
fn emit_string_name_descriptor_cases_loop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    cases: &[callable_dispatch::RuntimeCallableCase],
    op_name: &str,
) -> Result<()> {
    let done_label = ctx.next_label(&format!("{}_runtime_string_done", op_name));
    let miss_label = ctx.next_label(&format!("{}_runtime_string_missing", op_name));
    let call_reg = abi::nested_call_reg(ctx.emitter);
    if cases.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_string_lookup_table(ctx, cases)?;
        emit_branch_if_lookup_missed(ctx, &miss_label);
        abi::emit_reg_move(ctx.emitter, call_reg, abi::int_result_reg(ctx.emitter));
        emit_descriptor_reg_invoker_call_with_mixed_arg(
            ctx,
            inst,
            call_reg,
            arg_mixed,
            "callable_descriptor_invoke",
            false,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&miss_label);
        emit_string_name_descriptor_miss(ctx, inst, callable, arg_mixed, &done_label)?;
        ctx.emitter.label(&done_label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        return Ok(());
    }
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };
    for case in cases {
        let next_case = ctx.next_label("runtime_string_descriptor_next");
        let matched_label = ctx.next_label("callable_string_match");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &matched_label,
            ctx.data,
        );
        emit_static_descriptor_case_invoke(ctx, inst, arg_mixed, &case.descriptor_label)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_case);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    emit_string_name_descriptor_miss(ctx, inst, callable, arg_mixed, &done_label)?;

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Emits the miss arm of the string-name descriptor dispatch.
///
/// The 16-byte name spill is still pushed here; `done_label` releases it, so the eval arm jumps
/// there with the spill intact. Without an eval bridge the miss is PHP's undefined-function fatal.
fn emit_string_name_descriptor_miss(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    done_label: &str,
) -> Result<()> {
    if runtime_string_callable_has_eval_fallback(ctx) {
        lower_runtime_string_callable_eval_fallback(ctx, inst, callable, arg_mixed)?;
        abi::emit_jump(ctx.emitter, done_label);
    } else {
        emit_undefined_runtime_string_call_fatal(ctx);
    }
    Ok(())
}

/// Resolves the pushed runtime string name through a compact metadata table.
fn emit_runtime_string_lookup_table(
    ctx: &mut FunctionContext<'_>,
    cases: &[callable_dispatch::RuntimeCallableCase],
) -> Result<()> {
    let table = emit_string_lookup_table(ctx.data, cases).map_err(CodegenIrError::invalid_module)?;
    let (label, bound, helper) = match table {
        StringLookupTable::Linear { label, count } => {
            (label, count, "__rt_callable_lookup_string_linear")
        }
        StringLookupTable::Hashed { label, mask } => {
            (label, mask, "__rt_callable_lookup_string_hash")
        }
    };
    let ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let len_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let table_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let bound_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_arg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_arg, 8);
    abi::emit_symbol_address(ctx.emitter, table_arg, &label);
    abi::emit_load_int_immediate(ctx.emitter, bound_arg, bound as i64);
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Resolves an instance callable-array selector through compact composite tables.
///
/// The public methods form one table that every site with the same positional shape shares
/// (`DataSection::add_words` keeps one copy per content). The `protected`/`private` methods the
/// site's own class scope reaches go in a second, small table searched first. One table holding
/// both made every class scope its own near-copy of the whole public universe: measured on the
/// Laravel `--web` build, ~245 tables of 3.67 MB that differed by 14 entries out of 23 383, i.e.
/// 897 MB of `__data`, enough to push the image past what dyld maps. The two key sets never
/// overlap — a method has one visibility — so searching the scope table first changes nothing.
fn emit_runtime_instance_lookup_table(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    receiver_payload_offset: usize,
    method_ptr_offset: usize,
    method_len_offset: usize,
    miss_label: &str,
) -> Result<()> {
    let (public, scoped): (Vec<_>, Vec<_>) = targets
        .iter()
        .partition(|target| runtime_instance_target_is_public(ctx, target));
    if public.is_empty() || scoped.is_empty() {
        let all = targets.iter().collect::<Vec<_>>();
        return emit_runtime_instance_lookup_table_part(
            ctx,
            &all,
            receiver_payload_offset,
            method_ptr_offset,
            method_len_offset,
            miss_label,
        );
    }
    let found_label = ctx.next_label("callable_array_instance_scope_hit");
    emit_runtime_instance_lookup_table_part(
        ctx,
        &scoped,
        receiver_payload_offset,
        method_ptr_offset,
        method_len_offset,
        miss_label,
    )?;
    emit_branch_if_lookup_hit(ctx, &found_label);
    emit_runtime_instance_lookup_table_part(
        ctx,
        &public,
        receiver_payload_offset,
        method_ptr_offset,
        method_len_offset,
        miss_label,
    )?;
    ctx.emitter.label(&found_label);
    Ok(())
}

/// Whether an instance callable target is a public method, reachable from every scope.
fn runtime_instance_target_is_public(
    _ctx: &FunctionContext<'_>,
    target: &RuntimeArrayInstanceMethodTarget,
) -> bool {
    target.public
}

/// Whether `method_name` is declared public on `class_name` in the module's class table.
fn method_is_public(module: &crate::ir::Module, class_name: &str, method_name: &str) -> bool {
    module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.method_visibilities.get(method_name))
        .is_some_and(|visibility| matches!(visibility, Visibility::Public))
}

/// Branches to `hit_label` when a callable table resolver returned a descriptor.
fn emit_branch_if_lookup_hit(ctx: &mut FunctionContext<'_>, hit_label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbnz {}, {}", result_reg, hit_label)); // the scope table matched: skip the shared public table
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg)); // test the scope table's descriptor result
            ctx.emitter.instruction(&format!("jne {}", hit_label));             // the scope table matched: skip the shared public table
        }
    }
}

/// What a callable-array lookup raises when the receiver's class declares the method but the
/// scope the callable is resolved from cannot reach it.
///
/// php separates two situations that the compiled candidate tables both see as a plain miss. A
/// callable that is being VALIDATED (`call_user_func()`, `array_map()`, `usort()` ...) throws a
/// `TypeError` that ends in `cannot access protected method Box::hidden()`; a callable that is
/// being CALLED (`$f()` with `$f = [$obj, 'hidden']`) throws `Error: Call to protected method
/// Box::hidden() from global scope`. Anything else keeps the generic abort.
#[derive(Clone)]
enum InaccessibleMethodError {
    /// No dedicated diagnostic: the miss continues to the eval fallback and the generic abort.
    Ignore,
    /// php's callback validation, wording: `<prefix>, cannot access <visibility> method C::m()`.
    InvalidCallback(String),
    /// php's direct call, wording: `Call to <visibility> method C::m() from <scope>`.
    DirectCall,
    /// Chosen at run time among the two above by the callback kind held in this value; used by
    /// the module-wide helper, which serves `call_user_func()`, `call_user_func_array()` and
    /// plain calls alike.
    Dynamic(ValueId),
    /// Hands the inaccessible record back to the site, which words the message for its own scope.
    /// The search shared by every scope that reaches no non-public method uses it, jumping to
    /// this label with the record in the result register.
    AtSite(String),
}

/// Resolves a callable array whose method the class does not offer to the class's `__call()`.
///
/// php answers `[$object, 'name']` with `$object->__call('name', $args)` when `name` is not a
/// method the caller can reach and the class declares `__call()`; the miss paths used to abort
/// instead. The receiver's class id is checked against the compiled classes that declare
/// `__call()` (an object of an eval-declared class is left to the eval fallback), and a match asks
/// the `__elephc_magic_callable` prelude helper for a closure forwarding to `__call()`. That
/// closure is the descriptor, handed on owned like every receiver-bound descriptor.
///
/// Falls through when the receiver's class has no `__call()`; a match never falls through.
fn emit_magic_call_forwarder(ctx: &mut FunctionContext<'_>, done_label: &str) -> Result<()> {
    let mut classes = ctx
        .module
        .class_infos
        .values()
        .filter(|info| info.methods.keys().any(|name| name.eq_ignore_ascii_case("__call")))
        .map(|info| info.class_id)
        .collect::<Vec<_>>();
    if classes.is_empty()
        || !ctx.module.has_function_named(crate::magic_call_prelude::MAGIC_CALLABLE_HELPER_NAME)
    {
        return Ok(());
    }
    classes.sort_unstable();
    let marker = ctx.data.add_words(vec![DataWord::U64(1)]);
    let cases = classes
        .into_iter()
        .map(|class_id| InvokableLookupCase {
            class_id,
            descriptor_label: marker.clone(),
        })
        .collect::<Vec<_>>();
    let table =
        emit_invokable_lookup_table(ctx.data, cases).map_err(CodegenIrError::invalid_module)?;
    let no_forwarder = ctx.next_label("callable_array_no_magic_call");
    let not_closure = ctx.next_label("callable_array_magic_call_not_closure");
    emit_invokable_table_lookup_call(ctx, table, MIXED_RECEIVER_PAYLOAD_OFFSET, &no_forwarder);
    emit_branch_if_lookup_missed(ctx, &no_forwarder);

    let object_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let name_ptr_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_arg, MIXED_RECEIVER_PAYLOAD_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr_arg, MIXED_METHOD_PAYLOAD_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len_arg, MIXED_METHOD_PAYLOAD_OFFSET + 8);
    abi::emit_call_label(
        ctx.emitter,
        &function_symbol(crate::magic_call_prelude::MAGIC_CALLABLE_HELPER_NAME),
    );
    // The helper answered a boxed `?Closure`: keep the box, read the descriptor out of it.
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_CALLABLE)); // did the helper return a closure?
            ctx.emitter.instruction(&format!("b.ne {}", not_closure)); // null: the class has no usable __call()
            ctx.emitter.instruction("mov x0, x1");                              // take the closure descriptor out of the box
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_CALLABLE)); // did the helper return a closure?
            ctx.emitter.instruction(&format!("jne {}", not_closure)); // null: the class has no usable __call()
            ctx.emitter.instruction("mov rax, rdi");                            // take the closure descriptor out of the box
        }
    }
    // Retain the descriptor for the caller, then drop the box that held the helper's reference.
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_runtime_descriptor_owned_flag(ctx, true);
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&not_closure);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 0);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.label(&no_forwarder);
    Ok(())
}

/// The spelling every method of a class was declared with, keyed by the lower-cased name.
///
/// The method tables are keyed by the lower-cased name, and php words `cannot access protected
/// method Box::doIt()` with the declaration's own case. The nearest declaration wins, so an
/// override's spelling shadows its ancestor's.
fn declared_method_names(
    ctx: &FunctionContext<'_>,
    class_info: &crate::types::ClassInfo,
) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();
    let mut owner = Some(class_info);
    while let Some(info) = owner {
        for decl in &info.method_decls {
            names
                .entry(php_symbol_key(&decl.name))
                .or_insert_with(|| decl.name.clone());
        }
        owner = info
            .parent
            .as_deref()
            .and_then(|parent| ctx.module.class_infos.get(parent.trim_start_matches('\\')));
    }
    names
}

/// Whether some compiled class holds a non-public instance method.
///
/// A program whose classes expose no public method still has callable arrays worth dispatching:
/// `call_user_func([$object, 'hidden'])` must reach the unreachable-method diagnostic instead of
/// being refused for having no candidate to select.
fn module_has_unreachable_callable_methods(ctx: &FunctionContext<'_>) -> bool {
    ctx.module.class_infos.values().any(|info| {
        info.method_visibilities
            .values()
            .any(|visibility| !matches!(visibility, Visibility::Public))
    })
}

/// Builds the lookup rows that name every non-public instance method of every compiled class.
///
/// The row value is not a descriptor but a two-word record (pointer, length) of the text
/// `protected method Box::hidden()`, so the miss path can word the diagnostic without a second
/// table. The rows are independent of the call site's scope: they are only consulted AFTER the
/// shared public table and the site's own scope table both missed, which is exactly "the class
/// declares this method and this scope cannot reach it". That keeps the table identical for every
/// site (`DataSection::add_words` stores one copy per content) instead of a near-copy per scope.
fn runtime_inaccessible_instance_method_cases(
    ctx: &mut FunctionContext<'_>,
) -> Vec<InstanceLookupCase> {
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let declared = declared_method_names(ctx, class_info);
        let spelled = |key: &String| declared.get(key).cloned().unwrap_or_else(|| key.clone());
        let mut methods = class_info.methods.keys().collect::<Vec<_>>();
        methods.sort();
        for method_name in methods {
            let word = match class_info.method_visibilities.get(method_name) {
                Some(Visibility::Protected) => "protected",
                Some(Visibility::Private) => "private",
                _ => continue,
            };
            rows.push((
                class_info.class_id,
                method_name.clone(),
                format!("{word} method {}::", class_name.trim_start_matches('\\')),
                spelled(method_name),
            ));
        }
        // A private method is not copied into the child's own tables here, but php still finds
        // it there and refuses it: `cannot access private method Child::m()`.
        let mut ancestor = class_info.parent.as_deref();
        while let Some(parent_name) = ancestor {
            let Some(parent_info) = ctx.module.class_infos.get(parent_name.trim_start_matches('\\'))
            else {
                break;
            };
            for (method_name, _) in &parent_info.methods {
                if parent_info.method_visibilities.get(method_name) != Some(&Visibility::Private)
                    || class_info.methods.contains_key(method_name)
                    || !seen.insert((class_info.class_id, php_symbol_key(method_name)))
                {
                    continue;
                }
                rows.push((
                    class_info.class_id,
                    method_name.clone(),
                    format!("private method {}::", class_name.trim_start_matches('\\')),
                    spelled(method_name),
                ));
            }
            ancestor = parent_info.parent.as_deref();
        }
    }
    rows.into_iter()
        .map(|(class_id, method_name, lead, declared)| {
            let (lead_label, lead_len) = ctx.data.add_string(lead.as_bytes());
            let (declared_label, declared_len) = ctx.data.add_string(declared.as_bytes());
            let record = ctx.data.add_words(vec![
                DataWord::Symbol(lead_label),
                DataWord::U64(lead_len as u64),
                DataWord::Symbol(declared_label),
                DataWord::U64(declared_len as u64),
            ]);
            InstanceLookupCase {
                class_id,
                method_name,
                descriptor_label: record,
            }
        })
        .collect()
}

/// Returns the unreachable-method lookup table, building it once per data section.
///
/// `None` when no compiled class holds a non-public instance method.
fn inaccessible_method_table(ctx: &mut FunctionContext<'_>) -> Result<Option<StringLookupTable>> {
    if let Some(cached) = ctx.shared.inaccessible_method_table() {
        return Ok(cached);
    }
    let cases = runtime_inaccessible_instance_method_cases(ctx);
    let table = if cases.is_empty() {
        None
    } else {
        Some(emit_instance_lookup_table(ctx.data, cases).map_err(CodegenIrError::invalid_module)?)
    };
    ctx.shared.cache_inaccessible_method_table(table.clone());
    Ok(table)
}

/// Throws the diagnostic for a callable array that names an unreachable method, or falls through.
///
/// Looks the receiver's class id and the selected method name up in the inaccessible-method rows.
/// A miss falls through to the instruction after this sequence (the caller's own miss handling);
/// a hit never returns. The selector block still sits on the temporary stack on entry.
fn emit_inaccessible_method_error(
    ctx: &mut FunctionContext<'_>,
    error: &InaccessibleMethodError,
) -> Result<()> {
    if matches!(error, InaccessibleMethodError::Ignore) {
        return Ok(());
    }
    let Some(table) = inaccessible_method_table(ctx)? else {
        return Ok(());
    };
    let not_inaccessible = ctx.next_label("callable_array_not_inaccessible");
    emit_instance_table_lookup_call(
        ctx,
        table,
        MIXED_RECEIVER_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET + 8,
        &not_inaccessible,
    );
    emit_branch_if_lookup_missed(ctx, &not_inaccessible);
    // The record address stays in the result register until the branch that words the message
    // has read it; the selector block stays on the stack for the same reason (a direct call
    // quotes the method name exactly as the caller spelled it).
    match error {
        InaccessibleMethodError::Ignore => unreachable!("filtered above"),
        InaccessibleMethodError::AtSite(done_label) => {
            let flag_reg = runtime_descriptor_owned_flag_reg(ctx);
            abi::emit_load_int_immediate(ctx.emitter, flag_reg, CALLABLE_ARRAY_LOOKUP_REFUSED);
            abi::emit_jump(ctx.emitter, done_label);
        }
        InaccessibleMethodError::Dynamic(kind) => {
            let user_func = ctx.next_label("callable_array_inaccessible_user_func");
            let user_func_array = ctx.next_label("callable_array_inaccessible_user_func_array");
            let kind_reg = abi::int_arg_reg_name(ctx.emitter.target, 5);
            ctx.load_value_to_reg(*kind, kind_reg)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cmp {}, #{}", kind_reg, CALLBACK_KIND_CALL_USER_FUNC)); // is the callback a call_user_func() argument?
                    ctx.emitter.instruction(&format!("b.eq {}", user_func)); // word the call_user_func() validation error
                    ctx.emitter.instruction(&format!("cmp {}, #{}", kind_reg, CALLBACK_KIND_CALL_USER_FUNC_ARRAY)); // is the callback a call_user_func_array() argument?
                    ctx.emitter.instruction(&format!("b.eq {}", user_func_array)); // word the call_user_func_array() validation error
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", kind_reg, CALLBACK_KIND_CALL_USER_FUNC)); // is the callback a call_user_func() argument?
                    ctx.emitter.instruction(&format!("je {}", user_func)); // word the call_user_func() validation error
                    ctx.emitter.instruction(&format!("cmp {}, {}", kind_reg, CALLBACK_KIND_CALL_USER_FUNC_ARRAY)); // is the callback a call_user_func_array() argument?
                    ctx.emitter.instruction(&format!("je {}", user_func_array)); // word the call_user_func_array() validation error
                }
            }
            emit_inaccessible_method_throw(ctx, &InaccessibleMethodError::DirectCall);
            ctx.emitter.label(&user_func);
            emit_inaccessible_method_throw(
                ctx,
                &InaccessibleMethodError::InvalidCallback(CALL_USER_FUNC_CALLBACK_PREFIX.to_string()),
            );
            ctx.emitter.label(&user_func_array);
            emit_inaccessible_method_throw(
                ctx,
                &InaccessibleMethodError::InvalidCallback(
                    CALL_USER_FUNC_ARRAY_CALLBACK_PREFIX.to_string(),
                ),
            );
        }
        other => emit_inaccessible_method_throw(ctx, other),
    }
    ctx.emitter.label(&not_inaccessible);
    Ok(())
}

/// Words and throws the diagnostic for the inaccessible-method record in the result register.
///
/// The record holds `protected method Box::` and the method as declared. php words the two
/// diagnostics differently: the callback `TypeError` quotes the declared name, the direct-call
/// `Error` quotes the name the caller wrote, which is still in the selector block.
fn emit_inaccessible_method_throw(ctx: &mut FunctionContext<'_>, error: &InaccessibleMethodError) {
    let record_reg = abi::int_result_reg(ctx.emitter);
    let (lead_ptr, lead_len) = abi::string_result_regs(ctx.emitter);
    let (tail_ptr, tail_len) = super::runtime_class_messages::concat_right_operand_regs(ctx.emitter);
    // The tail is loaded first: on x86_64 the record register is also the lead's pointer register.
    match error {
        InaccessibleMethodError::InvalidCallback(_) => {
            abi::emit_load_from_address(ctx.emitter, tail_ptr, record_reg, 16);
            abi::emit_load_from_address(ctx.emitter, tail_len, record_reg, 24);
        }
        _ => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, tail_ptr, MIXED_METHOD_PAYLOAD_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, tail_len, MIXED_METHOD_PAYLOAD_OFFSET + 8);
        }
    }
    abi::emit_load_from_address(ctx.emitter, lead_len, record_reg, 8);
    abi::emit_load_from_address(ctx.emitter, lead_ptr, record_reg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    match error {
        InaccessibleMethodError::InvalidCallback(prefix) => {
            super::runtime_class_messages::emit_concat_static_suffix(ctx, "()");
            super::runtime_class_messages::emit_concat_static_prefix(ctx, ", cannot access ");
            super::runtime_class_messages::emit_concat_static_prefix(ctx, prefix);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            super::exceptions::emit_type_error_from_string_result(ctx);
        }
        _ => {
            let scope = match ctx.function.lexical_class.as_deref() {
                Some(class) => format!("() from scope {}", class.trim_start_matches('\\')),
                None => "() from global scope".to_string(),
            };
            super::runtime_class_messages::emit_concat_static_suffix(ctx, &scope);
            super::runtime_class_messages::emit_concat_static_prefix(ctx, "Call to ");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            super::exceptions::emit_error_from_string_result(ctx);
        }
    }
}

/// Resolves an instance callable-array selector through one composite table.
fn emit_runtime_instance_lookup_table_part(
    ctx: &mut FunctionContext<'_>,
    targets: &[&RuntimeArrayInstanceMethodTarget],
    receiver_payload_offset: usize,
    method_ptr_offset: usize,
    method_len_offset: usize,
    miss_label: &str,
) -> Result<()> {
    let key = base_table_key(ctx, targets);
    if let Some(table) = key.as_deref().and_then(|key| ctx.shared.instance_lookup_table(key)) {
        emit_instance_table_lookup_call(
            ctx,
            table,
            receiver_payload_offset,
            method_ptr_offset,
            method_len_offset,
            miss_label,
        );
        return Ok(());
    }
    let mut cases = Vec::with_capacity(targets.len());
    for target in targets {
        let template = runtime_instance_method_descriptor_template(
            ctx,
            &target.class_name,
            &target.method_name,
            &target.method_key,
            &target.impl_class,
            &target.sig,
        )?;
        cases.push(InstanceLookupCase {
            class_id: target.class_id,
            method_name: target.method_name.to_string(),
            descriptor_label: template.descriptor_label,
        });
    }
    let table = emit_instance_lookup_table(ctx.data, cases)
        .map_err(CodegenIrError::invalid_module)?;
    if let Some(key) = key {
        ctx.shared.cache_instance_lookup_table(key, table.clone());
    }
    emit_instance_table_lookup_call(
        ctx,
        table,
        receiver_payload_offset,
        method_ptr_offset,
        method_len_offset,
        miss_label,
    );
    Ok(())
}

/// Names a target list by the addresses of its signatures when every one of them is a base
/// target's: the state keeps those alive and each belongs to exactly one target, so equal
/// address lists are equal target lists. A list holding a per-site override (a private method
/// of the calling scope) gets no key and is built as before.
fn base_table_key(
    ctx: &FunctionContext<'_>,
    targets: &[&RuntimeArrayInstanceMethodTarget],
) -> Option<Vec<usize>> {
    let base = ctx.shared.instance_method_target_base(|| instance_method_target_base(ctx));
    let base_signatures = ctx.shared.base_target_signatures(|| {
        base.iter()
            .map(|target| std::sync::Arc::as_ptr(&target.sig) as usize)
            .collect()
    });
    targets
        .iter()
        .map(|target| {
            let address = std::sync::Arc::as_ptr(&target.sig) as usize;
            base_signatures.contains(&address).then_some(address)
        })
        .collect()
}

/// Looks the saved receiver class id and method name up in one composite instance table.
///
/// A null receiver branches to `miss_label`; any other input leaves the matching value in the
/// integer result register, or zero when the table has no row for the pair.
fn emit_instance_table_lookup_call(
    ctx: &mut FunctionContext<'_>,
    table: StringLookupTable,
    receiver_payload_offset: usize,
    method_ptr_offset: usize,
    method_len_offset: usize,
    miss_label: &str,
) {
    let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let class_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let method_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let table_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_arg, receiver_payload_offset);
    emit_branch_if_register_is_null(ctx, class_arg, miss_label);
    abi::emit_load_from_address(ctx.emitter, class_arg, class_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, kind_arg, 1);
    abi::emit_load_int_immediate(ctx.emitter, class_len_arg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_arg, method_ptr_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_len_arg, method_len_offset);
    emit_composite_lookup_call(ctx, table_arg, table);
}

/// Resolves `[$object, 'staticName']` through the same composite table the instance arm uses.
///
/// WHY THIS IS A TABLE. The arms this replaces were a linear chain, one link per candidate, each
/// re-comparing both tags, the receiver class id and the method name with `__rt_strcasecmp`.
/// Measured on the compiled Symfony `--web` app: **189 355 links, ~21 lines each, 3 971 055 lines
/// — 6.5% of the module**, and a callable that matches the last one walks all of them at RUN
/// time. Every other arm of this resolver already switches to a table above
/// `INLINE_LOOKUP_LIMIT`; this one never did.
///
/// The key is the receiver's class id plus the method name, which is exactly
/// `emit_instance_lookup_table`'s key, so the existing table format and the existing
/// `__rt_callable_lookup_composite_*` resolvers are reused unchanged. The table's own capture
/// flag is ignored: a static method binds no receiver, and the caller sets the flag explicitly.
///
/// Duplicates are dropped here rather than left to the table builder. The chain resolved to its
/// FIRST match, and the same `(class, method)` pair can arrive twice — once from the public
/// static cases and once from the call site's own class scope — so the first occurrence wins and
/// the rest are discarded, which is what keeps the table's "one value per key" invariant.
fn emit_runtime_object_static_lookup_table(
    ctx: &mut FunctionContext<'_>,
    object_static_cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
    receiver_payload_offset: usize,
    method_ptr_offset: usize,
    method_len_offset: usize,
    miss_label: &str,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let mut cases = Vec::with_capacity(object_static_cases.len());
    for case in object_static_cases {
        let Some(class_id) = ctx
            .module
            .class_infos
            .get(case.class_name.trim_start_matches('\\'))
            .map(|class_info| class_info.class_id)
        else {
            continue;
        };
        if !seen.insert((class_id, case.method_name.to_ascii_lowercase())) {
            continue;
        }
        cases.push(InstanceLookupCase {
            class_id,
            method_name: case.method_name.clone(),
            descriptor_label: case.case.descriptor_label.clone(),
        });
    }
    if cases.is_empty() {
        abi::emit_jump(ctx.emitter, miss_label);
        return Ok(());
    }
    let table = emit_instance_lookup_table(ctx.data, cases)
        .map_err(CodegenIrError::invalid_module)?;
    let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let class_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let method_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let table_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_arg, receiver_payload_offset);
    emit_branch_if_register_is_null(ctx, class_arg, miss_label);
    abi::emit_load_from_address(ctx.emitter, class_arg, class_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, kind_arg, 1);
    abi::emit_load_int_immediate(ctx.emitter, class_len_arg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_arg, method_ptr_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_len_arg, method_len_offset);
    emit_composite_lookup_call(ctx, table_arg, table);
    Ok(())
}

/// Resolves a static callable-array selector through a compact composite table.
fn emit_runtime_static_lookup_table(
    ctx: &mut FunctionContext<'_>,
    cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
    class_ptr_offset: usize,
    class_len_offset: usize,
    method_ptr_offset: usize,
    method_len_offset: usize,
) -> Result<()> {
    let table_cases = cases
        .iter()
        .map(|case| StaticLookupCase {
            class_name: case.class_name.clone(),
            method_name: case.method_name.clone(),
            descriptor_label: case.case.descriptor_label.clone(),
        })
        .collect();
    let table = emit_static_lookup_table(ctx.data, table_cases)
        .map_err(CodegenIrError::invalid_module)?;
    let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let class_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let method_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let table_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_load_int_immediate(ctx.emitter, kind_arg, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_arg, class_ptr_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_len_arg, class_len_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_arg, method_ptr_offset);
    abi::emit_load_temporary_stack_slot(ctx.emitter, method_len_arg, method_len_offset);
    emit_composite_lookup_call(ctx, table_arg, table);
    Ok(())
}

/// Resolves an invokable receiver class through a compact composite table.
fn emit_runtime_invokable_lookup_table(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    receiver_payload_offset: usize,
    miss_label: &str,
) -> Result<()> {
    let mut cases = Vec::with_capacity(targets.len());
    for target in targets {
        let template = runtime_instance_method_descriptor_template(
            ctx,
            &target.class_name,
            &target.method_name,
            &target.method_key,
            &target.impl_class,
            &target.sig,
        )?;
        cases.push(InvokableLookupCase {
            class_id: target.class_id,
            descriptor_label: template.descriptor_label,
        });
    }
    let table = emit_invokable_lookup_table(ctx.data, cases)
        .map_err(CodegenIrError::invalid_module)?;
    emit_invokable_table_lookup_call(ctx, table, receiver_payload_offset, miss_label);
    Ok(())
}

/// Looks the saved receiver's class id up in one composite invokable (class-id keyed) table.
///
/// A null receiver branches to `miss_label`; otherwise the matching value, or zero, is left in
/// the integer result register.
fn emit_invokable_table_lookup_call(
    ctx: &mut FunctionContext<'_>,
    table: StringLookupTable,
    receiver_payload_offset: usize,
    miss_label: &str,
) {
    let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let class_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let class_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let method_arg = abi::int_arg_reg_name(ctx.emitter.target, 3);
    let method_len_arg = abi::int_arg_reg_name(ctx.emitter.target, 4);
    let table_arg = abi::int_arg_reg_name(ctx.emitter.target, 5);
    abi::emit_load_temporary_stack_slot(ctx.emitter, class_arg, receiver_payload_offset);
    emit_branch_if_register_is_null(ctx, class_arg, miss_label);
    abi::emit_load_from_address(ctx.emitter, class_arg, class_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, kind_arg, 3);
    abi::emit_load_int_immediate(ctx.emitter, class_len_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, method_arg, 0);
    abi::emit_load_int_immediate(ctx.emitter, method_len_arg, 0);
    emit_composite_lookup_call(ctx, table_arg, table);
}

/// Materializes one composite table address and calls the selected resolver.
fn emit_composite_lookup_call(
    ctx: &mut FunctionContext<'_>,
    table_arg: &str,
    table: StringLookupTable,
) {
    let (label, helper) = match table {
        StringLookupTable::Linear { label, .. } => {
            (label, "__rt_callable_lookup_composite_linear")
        }
        StringLookupTable::Hashed { label, .. } => {
            (label, "__rt_callable_lookup_composite_hash")
        }
    };
    abi::emit_symbol_address(ctx.emitter, table_arg, &label);
    abi::emit_call_label(ctx.emitter, helper);
}

/// Branches when a lookup input register contains a null object pointer.
fn emit_branch_if_register_is_null(
    ctx: &mut FunctionContext<'_>,
    reg: &str,
    miss_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx
            .emitter
            .instruction(&format!("cbz {}, {}", reg, miss_label)), // reject a null runtime receiver before reading its class id
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", reg, reg)); // check the runtime receiver pointer before reading its class id
            ctx.emitter.instruction(&format!("je {}", miss_label));             // treat a null runtime receiver as a lookup miss
        }
    }
}

/// Branches to `miss_label` when a callable table resolver returned a null value.
fn emit_branch_if_lookup_missed(ctx: &mut FunctionContext<'_>, miss_label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", result_reg, miss_label)); // a null descriptor means no canonical callable key matched
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", result_reg, result_reg)); // test the table resolver's descriptor result
            ctx.emitter.instruction(&format!("je {}", miss_label));             // use the existing fatal path after a lookup miss
        }
    }
}

/// Builds runtime callable descriptor cases for string-name dynamic invocation.
pub(super) fn runtime_string_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
    strict_php: bool,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    runtime_string_descriptor_cases_at_arity(ctx, source_arg_ty, candidate_names, strict_php, None)
}

/// Builds descriptor cases for a host with an optional known callback arity.
///
/// Only the arity-free table is cached: a host's arity narrows the builtin cases, and the cache
/// key does not carry it.
fn runtime_string_descriptor_cases_at_arity(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
    strict_php: bool,
    arity: Option<usize>,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    let cache_ty = source_arg_ty.map(PhpType::codegen_repr);
    if let Some(cases) = ctx
        .shared
        .runtime_string_descriptor_cases(cache_ty.as_ref(), candidate_names, strict_php, arity)
    {
        return Ok(cases);
    }
    let mut cases = runtime_extern_descriptor_cases(ctx, candidate_names)?;
    let builtin_cases = runtime_builtin_descriptor_cases_at_arity(
        ctx,
        source_arg_ty,
        candidate_names,
        strict_php,
        arity,
    )?;
    // A user declaration of a builtin's name is one php never makes: the builtin keeps
    // answering, and executing the declaration would be a redeclaration fatal. It is still
    // EMITTED when it sits behind a guard (`if (...) { function strlen() {} }`), so both reached
    // this table under one key and codegen refused the program outright.
    let builtin_names = builtin_cases
        .iter()
        .filter_map(|case| case.php_name.as_deref().map(php_symbol_key))
        .collect::<std::collections::HashSet<_>>();
    cases.extend(builtin_cases);
    cases.extend(
        runtime_user_function_descriptor_cases(ctx, source_arg_ty, candidate_names, strict_php)
            .into_iter()
            .filter(|case| {
                case.php_name
                    .as_deref()
                    .is_none_or(|name| !builtin_names.contains(&php_symbol_key(name)))
            }),
    );
    cases.extend(
        runtime_static_method_descriptor_cases(ctx, candidate_names)
            .into_iter()
            .map(|case| case.case),
    );
    cases.sort_by(|left, right| {
        left.php_name
            .cmp(&right.php_name)
            .then_with(|| left.descriptor_label.cmp(&right.descriptor_label))
    });
    cases.dedup_by(|left, right| {
        left.php_name == right.php_name && left.descriptor_label == right.descriptor_label
    });
    // A finite candidate set without a compiled case keeps an empty table: every case of the
    // open table is keyed by a name outside that set, so none could ever match, and building it
    // cost a full runtime-name ladder (tens of thousands of compares) at every such site.
    ctx.shared.cache_runtime_string_descriptor_cases(
        cache_ty.as_ref(), candidate_names, strict_php, arity, &cases,
    );
    Ok(cases)
}

/// The builtin names an OPEN runtime string callable may still dispatch to natively.
///
/// When provenance cannot bound a `$name(...)` callable, every builtin in the catalog used to get
/// a descriptor wrapper and invoker: ~5,000 invokers and a 7 M-instruction dispatcher in the
/// Symfony demo, three times the pre-merge code size. A builtin named at run time almost always
/// comes from a string literal somewhere in the program ('strlen', 'trim', callable arrays...),
/// so with an eval bridge present only those names keep a compiled case; any other name still
/// runs, through the interpreter's builtin table on the table miss
/// (`lower_runtime_string_callable_eval_fallback`). Without the bridge nothing is narrowed.
fn literal_builtin_gate(ctx: &FunctionContext<'_>) -> Option<std::sync::Arc<std::collections::HashSet<String>>> {
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Option<(usize, usize, Arc<std::collections::HashSet<String>>)>> = Mutex::new(None);
    if !runtime_string_callable_has_eval_fallback(ctx) {
        return None;
    }
    let strings = &ctx.module.data.strings;
    let identity = (strings.as_ptr() as usize, strings.len());
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((ptr, len, set)) = cache.as_ref() {
        if (*ptr, *len) == identity {
            return Some(set.clone());
        }
    }
    let set: std::collections::HashSet<String> = strings
        .iter()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\\')
        })
        .map(|s| php_symbol_key(s.trim_start_matches('\\')))
        .collect();
    let set = Arc::new(set);
    *cache = Some((identity.0, identity.1, set.clone()));
    Some(set)
}

/// Returns whether one PHP callable name belongs to a finite reachable target set.
fn runtime_callable_name_is_reachable(
    name: &str,
    candidate_names: Option<&[String]>,
) -> bool {
    let Some(candidate_names) = candidate_names else {
        return true;
    };
    let key = php_symbol_key(name.trim_start_matches('\\'));
    candidate_names.iter().any(|candidate| candidate == &key)
}

/// Builds runtime descriptor cases for extern functions declared in the EIR module.
fn runtime_extern_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    candidate_names: Option<&[String]>,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    let mut decls = ctx.module.extern_decls.iter().collect::<Vec<_>>();
    decls.sort_by(|left, right| left.name.cmp(&right.name));
    // The same narrowing `literal_builtin_gate` applies to builtins. The module's externs are
    // mostly the preludes' own FFI bridges, which no program names at run time; building a
    // wrapper and an invoker for every one of them was the largest part of the serial shared
    // callable helper in the Symfony demo build.
    let literal_gate = if candidate_names.is_none() { literal_builtin_gate(ctx) } else { None };

    let mut cases = Vec::new();
    for decl in decls {
        if !runtime_callable_name_is_reachable(&decl.name, candidate_names) {
            continue;
        }
        if literal_gate.as_ref().is_some_and(|gate| {
            !gate.contains(&php_symbol_key(decl.name.trim_start_matches('\\')))
        }) {
            continue;
        }
        let wrapper_sig = crate::types::callable_wrapper_sig(&extern_decl_signature(decl));
        let entry_label = emit_runtime_extern_wrapper_inline(ctx, &decl.name, &wrapper_sig)?;
        let invoker_label = emit_runtime_callable_invoker_inline(ctx, &wrapper_sig, &[]);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &entry_label,
            Some(&decl.name),
            callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
            Some(&wrapper_sig),
            &[],
            &[],
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Extern,
                &decl.name,
            ),
            Some(&invoker_label),
        );
        cases.push(callable_dispatch::RuntimeCallableCase {
            descriptor_label,
            php_name: Some(decl.name.clone()),
        });
    }
    Ok(cases)
}

/// Converts an EIR extern declaration into the PHP-facing wrapper signature.
fn extern_decl_signature(decl: &crate::ir::ExternDecl) -> FunctionSig {
    FunctionSig {
        params: decl
            .params
            .iter()
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        param_type_exprs: vec![None; decl.params.len()],
        param_attributes: vec![Vec::new(); decl.params.len()],
        defaults: vec![None; decl.params.len()],
        return_type: decl.return_php_type.clone(),
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; decl.params.len()],
        declared_params: vec![true; decl.params.len()],
        variadic: None,
        deprecation: None,
        is_generator: false,
    }
}


/// Builds descriptor cases with an optional host-proven callback arity.
fn runtime_builtin_descriptor_cases_at_arity(
    ctx: &mut FunctionContext<'_>, source_arg_ty: Option<&PhpType>, candidate_names: Option<&[String]>,
    strict_php: bool, arity: Option<usize>,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    let mut cases = Vec::new();
    let literal_gate = if candidate_names.is_none() { literal_builtin_gate(ctx) } else { None };
    for name in crate::types::checker::builtins::supported_builtin_function_names_for_profile(
        strict_php,
    ) {
        if let Some(gate) = &literal_gate {
            let named = gate.contains(&php_symbol_key(name))
                || crate::name_resolver::function_alias_names().any(|alias| {
                    crate::name_resolver::canonical_function_alias(alias)
                        .is_some_and(|canonical| php_symbol_key(canonical) == php_symbol_key(name))
                        && gate.contains(&php_symbol_key(alias))
                });
            if !named {
                continue;
            }
        }
        // PHP's function aliases (`sizeof`, `key_exists`, `pos`, ...) name the same function, so a
        // runtime string holding one dispatches to the canonical builtin's descriptor.
        // `$f = "sizeof"; $f($a);` died with "Call to undefined function".
        // An alias the registry also binds on its own (`user_error`) gets its own case from that
        // entry; adding it here as well would give one runtime key two descriptors.
        let reachable_aliases = crate::name_resolver::function_alias_names()
            .filter(|alias| {
                crate::name_resolver::canonical_function_alias(alias)
                    .is_some_and(|canonical| php_symbol_key(canonical) == php_symbol_key(name))
                    && crate::builtins::registry::lookup(alias).is_none()
                    // A program function that reuses an alias name (`function pos($x)`) keeps the
                    // runtime string: the alias case would hand it to the builtin instead.
                    && !ctx
                        .module
                        .functions
                        .iter()
                        .any(|function| php_symbol_key(&function.name) == php_symbol_key(alias))
                    && runtime_callable_name_is_reachable(alias, candidate_names)
            })
            .collect::<Vec<_>>();
        let wrapper_supported = if arity.is_some() {
            callable_dispatch::runtime_builtin_wrapper_supported_at_arity(
                name,
                source_arg_ty,
                ctx.module.required_runtime_features,
                arity,
            )
        } else {
            callable_dispatch::runtime_builtin_wrapper_supported(
                name,
                source_arg_ty,
                ctx.module.required_runtime_features,
            )
        };
        if !(runtime_callable_name_is_reachable(name, candidate_names)
            || !reachable_aliases.is_empty())
            || !wrapper_supported
            || runtime_builtin_needs_unlinked_bridge(ctx, name)
            || ctx
                .module
                .extern_decls
                .iter()
                .any(|decl| php_symbol_key(&decl.name) == php_symbol_key(name))
        {
            continue;
        }
        // GMP's EIR lowering forwards to prelude helpers. Without the injected prelude
        // (`--with-gmp`, or a compiled GMP reference) there is no helper to wrap, and a
        // callable-table entry would only fail codegen on an unrelated eval host.
        if elephc_builtin_contract::lookup(name)
            .is_some_and(|contract| contract.module == elephc_builtin_contract::PhpModule::Gmp)
            && !ctx
                .module
                .functions
                .iter()
                .any(|function| function.name == "__elephc_gmp_abs")
        {
            continue;
        }
        let Some(sig) = crate::types::first_class_callable_builtin_sig(name) else {
            continue;
        };
        let wrapper_sig =
            runtime_builtin_wrapper_sig(name, &crate::types::callable_wrapper_sig(&sig));
        let case_sig = callable_dispatch::specialized_runtime_case_sig(&wrapper_sig, source_arg_ty);
        if !super::runtime_wrappers::builtin_wrapper_result_is_representable(
            name,
            &case_sig.return_type,
        ) {
            continue;
        }
        let entry_label =
            emit_runtime_builtin_wrapper_inline(ctx, name, &case_sig, strict_php)?;
        let invoker_label = emit_runtime_builtin_invoker_inline(ctx, name, &case_sig);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &entry_label,
            Some(name),
            callable_descriptor::CALLABLE_DESC_KIND_BUILTIN,
            Some(&case_sig),
            &[],
            &[],
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Builtin,
                name,
            ),
            Some(&invoker_label),
        );
        for alias in reachable_aliases {
            cases.push(callable_dispatch::RuntimeCallableCase {
                descriptor_label: descriptor_label.clone(),
                php_name: Some(alias.to_string()),
            });
        }
        cases.push(callable_dispatch::RuntimeCallableCase {
            descriptor_label,
            php_name: Some(name.to_string()),
        });
    }
    Ok(cases)
}

/// Returns whether a runtime-selectable builtin needs a native bridge this program does not link.
///
/// A wrapper for `mb_strtolower` calls through the iconv bridge, and a program only links a bridge
/// its own code calls (`-lelephc_iconv` follows the first static iconv-backed call). Putting such a
/// wrapper in the shared string-callable ladder of every program left `_elephc_iconv_call` undefined
/// at link time. The eval bridge (Magician) embeds the crypto, iconv and phar bridges, so a program
/// that has it can always offer them -- which is what lets Laravel's `$function($value)` with
/// `mb_strtolower` reach the builtin.
fn runtime_builtin_needs_unlinked_bridge(ctx: &FunctionContext<'_>, name: &str) -> bool {
    const EMBEDDED_BY_EVAL_BRIDGE: &[&str] = &["elephc_crypto", "elephc_highlight", "elephc_iconv", "elephc_phar"];
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(def) = crate::builtins::registry::lookup(&key) else {
        return false;
    };
    let requirements = match def.spec.semantics.requirements {
        crate::builtins::semantics::BuiltinRequirements::Static(requirements) => requirements,
        crate::builtins::semantics::BuiltinRequirements::Shared(_) => return false,
    };
    // A bridge the program's runtime features already link is not missing: a program that uses
    // mbstring links `elephc_mbstring`, so `$decode = "mb_decode_mimeheader"; $decode($a, $b)`
    // finds its case and raises php's `ArgumentCountError` instead of "Call to undefined function".
    let features = ctx.module.required_runtime_features;
    let linked = crate::codegen::link_requirements_for_runtime_features(features);
    requirements.iter().any(|requirement| match requirement {
        crate::builtins::semantics::BuiltinRequirement::Bridge(library) => {
            !(features.eval_bridge && EMBEDDED_BY_EVAL_BRIDGE.contains(library))
                && !linked.iter().any(|linked| {
                    matches!(linked, crate::codegen::LinkRequirement::Bridge(name) if name == library)
                })
        }
        _ => false,
    })
}

/// Builds runtime descriptor cases for user functions emitted in the EIR module.
fn runtime_user_function_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
    strict_php: bool,
) -> Vec<callable_dispatch::RuntimeCallableCase> {
    let mut functions = ctx
        .module
        .functions
        .iter()
        .filter(|function| {
            !function.flags.is_main && !function.name.starts_with("_class_propinit_")
        })
        .collect::<Vec<_>>();
    functions.sort_by(|left, right| left.name.cmp(&right.name));
    // A wrapper answers for the builtin it calls (`builtin_callable_prelude`), unless a compiled
    // function already goes by that name: a prelude implements `error_log` itself.
    let declared: std::collections::HashSet<String> =
        functions.iter().map(|function| php_symbol_key(&function.name)).collect();
    let mut entries: Vec<(&crate::ir::Function, String)> = functions
        .into_iter()
        .filter_map(|function| {
            match crate::builtin_callable_prelude::builtin_name_of_wrapper(&function.name) {
                Some(name) if declared.contains(&php_symbol_key(name)) => None,
                Some(name) => Some((function, name.to_string())),
                None => Some((function, function.name.clone())),
            }
        })
        .collect();
    // An include-loaded function body is compiled under a generated variant symbol, and the name
    // PHP code writes (`'Lib\\Ext\\is_root'`) is a dispatcher thunk over it. A string callable
    // names the public one, so each group is also offered under its public name, pointing at the
    // dispatcher (which fatals with php's message until the declaring file has run).
    let module = ctx.module;
    for group in crate::ir::function_variants::collect_dispatch_groups(module) {
        if let Some(representative) =
            crate::ir::function_variants::variant_callee_for_group(module, &group.name)
        {
            entries.push((representative, group.name.clone()));
        }
    }

    let mut cases = Vec::new();
    for (function, php_name) in entries {
        if !strict_php
            && crate::types::checker::builtins::catalog::strict_php_hidden_builtin_for_profile(
                &php_symbol_key(&php_name),
                true,
            )
        {
            continue;
        }
        if !runtime_callable_name_is_reachable(&php_name, candidate_names) {
            continue;
        }
        let wrapper_sig =
            crate::types::callable_wrapper_sig(&function_signature_from_eir(function));
        let case_sig = callable_dispatch::specialized_runtime_case_sig(&wrapper_sig, source_arg_ty);
        let owns_string_return = crate::codegen::runtime_callable_invoker::function_returns_owned_string(function);
        let invoker_label = emit_runtime_callable_invoker_with_string_owner(ctx, &case_sig, &[], owns_string_return, true);
        // A builtin's wrapper is called under its own symbol, whatever name it answers to.
        let callee = if crate::builtin_callable_prelude::builtin_name_of_wrapper(&function.name)
            .is_some()
        {
            function.name.as_str()
        } else {
            php_name.as_str()
        };
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &function_symbol(callee),
            Some(&php_name),
            callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            Some(&case_sig),
            &[],
            &[],
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Function,
                callee,
            ),
            Some(&invoker_label),
        );
        cases.push(callable_dispatch::RuntimeCallableCase {
            descriptor_label,
            php_name: Some(php_name),
        });
    }
    cases
}

/// Selects a callable descriptor from a runtime string callable name.
pub(super) fn emit_runtime_string_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    dest_reg: &str,
    op_name: &str,
    strict_php: bool,
) -> Result<()> {
    emit_runtime_string_descriptor_value_impl(
        ctx, callable, dest_reg, op_name, strict_php, None,
    )
}

/// Selects a descriptor from a string and throws `TypeError` when the name is not callable.
pub(super) fn emit_runtime_string_descriptor_value_with_type_error(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    dest_reg: &str,
    op_name: &str,
    strict_php: bool,
    message: &'static str,
) -> Result<()> {
    emit_runtime_string_descriptor_value_impl(
        ctx,
        callable,
        dest_reg,
        op_name,
        strict_php,
        Some(message),
    )
}

/// Implements runtime string descriptor selection with a configurable miss path.
fn emit_runtime_string_descriptor_value_impl(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    dest_reg: &str,
    op_name: &str,
    strict_php: bool,
    type_error: Option<&'static str>,
) -> Result<()> {
    let candidate_names = ctx.runtime_callable_candidates(callable);
    let cases =
        runtime_string_descriptor_cases(ctx, None, candidate_names.as_deref(), strict_php)?;
    if cases.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime string with no descriptor targets",
            op_name
        )));
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(callable, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let done_label = ctx.next_label(&format!("{}_runtime_string_descriptor_done", op_name));
    let miss_label = ctx.next_label(&format!("{}_runtime_string_descriptor_missing", op_name));
    if cases.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_string_lookup_table(ctx, &cases)?;
        emit_branch_if_lookup_missed(ctx, &miss_label);
        abi::emit_reg_move(
            ctx.emitter,
            dest_reg,
            abi::int_result_reg(ctx.emitter),
        );
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&miss_label);
        emit_undefined_runtime_string_call_fatal(ctx);
        ctx.emitter.label(&done_label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        return Ok(());
    }
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg: dest_reg,
    };
    for case in &cases {
        let next_case = ctx.next_label("runtime_string_descriptor_next");
        let matched_label = ctx.next_label("callable_string_match");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &matched_label,
            ctx.data,
        );
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_case);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    if let Some(message) = type_error {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::exceptions::emit_type_error(ctx, message);
    } else {
        emit_undefined_runtime_string_call_fatal(ctx);
    }

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Selects a callable descriptor from a string payload just returned by `__rt_mixed_unbox`.
///
/// With `eval_miss_label`, a name no compiled function answers jumps there with the name spill
/// already released, so the caller can hand the boxed name to the eval bridge instead.
fn emit_runtime_string_descriptor_value_from_unboxed(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    strict_php: bool,
    type_error: Option<&'static str>,
    eval_miss_label: Option<&str>,
    candidate_names: Option<&[String]>,
    arity: Option<usize>,
) -> Result<()> {
    let known_candidates;
    let candidate_names = match candidate_names {
        Some(names) => Some(names),
        None => {
            known_candidates = ctx.runtime_callable_candidates(callable);
            known_candidates.as_deref()
        }
    };
    let cases = runtime_string_descriptor_cases_at_arity(
        ctx,
        None,
        candidate_names,
        strict_php,
        arity,
    )?;
    if cases.is_empty() {
        // Nothing is pushed yet, so this miss throws without a stack release.
        if let Some(label) = eval_miss_label {
            abi::emit_jump(ctx.emitter, label);
            return Ok(());
        }
        match type_error {
            Some(message) => super::exceptions::emit_type_error(ctx, message),
            None => emit_undefined_runtime_string_call_fatal(ctx),
        }
        return Ok(());
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction(&format!("mov {}, rdi", ptr_reg));              // move the unboxed string pointer into the canonical string result register
    }
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let done_label = ctx.next_label(&format!("{}_mixed_string_descriptor_done", op_name));
    let miss_label = ctx.next_label(&format!("{}_mixed_string_descriptor_missing", op_name));
    if cases.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_string_lookup_table(ctx, &cases)?;
        emit_branch_if_lookup_missed(ctx, &miss_label);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&miss_label);
        if let Some(label) = eval_miss_label {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_jump(ctx.emitter, label);
        } else if let Some(message) = type_error {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            super::exceptions::emit_type_error(ctx, message);
        } else {
            emit_undefined_runtime_string_call_fatal(ctx);
        }
        ctx.emitter.label(&done_label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        return Ok(());
    }
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg: abi::int_result_reg(ctx.emitter),
    };
    for case in &cases {
        let next_case = ctx.next_label("mixed_string_descriptor_next");
        let matched_label = ctx.next_label("mixed_string_descriptor_match");
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next_case,
            ctx.emitter,
            &matched_label,
            ctx.data,
        );
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_case);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    if let Some(label) = eval_miss_label {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, label);
    } else if let Some(message) = type_error {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        super::exceptions::emit_type_error(ctx, message);
    } else {
        emit_undefined_runtime_string_call_fatal(ctx);
    }
    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Lowers `call_user_func_array($object, $args)` through an `__invoke` descriptor.
fn lower_invokable_object_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver: ValueId,
    arg_mixed: ValueId,
    class_name: &str,
    op_name: &str,
) -> Result<()> {
    emit_invokable_object_descriptor_value(ctx, receiver, class_name, op_name)?;
    emit_descriptor_reg_invoker_call_with_mixed_arg(
        ctx,
        inst,
        abi::nested_call_reg(ctx.emitter),
        arg_mixed,
        op_name,
        true,
    )
}

/// Materializes a receiver-bound descriptor for an invokable object value.
pub(super) fn emit_invokable_object_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    class_name: &str,
    op_name: &str,
) -> Result<()> {
    let normalized_class = class_name.trim_start_matches('\\').to_string();
    let method_key = "__invoke";
    let class_info = ctx
        .module
        .class_infos
        .get(normalized_class.as_str())
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "{} for invokable object with unknown class '{}'",
                op_name, normalized_class
            ))
        })?;
    let Some(sig) = class_info.methods.get(method_key).cloned() else {
        return emit_runtime_subclass_invokable_descriptor_value(
            ctx,
            receiver,
            &normalized_class,
            op_name,
        );
    };
    let impl_class = class_info
        .method_impl_classes
        .get(method_key)
        .cloned()
        .unwrap_or_else(|| normalized_class.clone());
    if !class_method_body_exists(ctx, &impl_class, method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "{} for invokable object '{}' without emitted __invoke body",
            op_name, normalized_class
        )));
    }

    let receiver_ty = PhpType::Object(normalized_class.clone());
    let template = runtime_instance_method_descriptor_template(
        ctx,
        &normalized_class,
        "__invoke",
        method_key,
        &impl_class,
        &sig,
    )?;
    emit_runtime_descriptor_with_receiver_capture(
        ctx,
        &template.descriptor_label,
        receiver,
        &receiver_ty,
    )
}

/// Selects an invokable concrete subclass for a receiver typed as a non-invokable base class.
fn emit_runtime_subclass_invokable_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    receiver: ValueId,
    base_class: &str,
    _op_name: &str,
) -> Result<()> {
    let method_key = "__invoke";
    let mut targets = ctx
        .module
        .class_infos
        .iter()
        .filter_map(|(class_name, class_info)| {
            if !runtime_class_is_same_or_descendant(ctx, class_name, base_class) {
                return None;
            }
            let sig = class_info.methods.get(method_key)?;
            if !class_info
                .method_visibilities
                .get(method_key)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                return None;
            }
            let impl_class = class_info
                .method_impl_classes
                .get(method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            class_method_body_exists(ctx, &impl_class, method_key).then(|| {
                RuntimeArrayInstanceMethodTarget {
                    class_name: class_name.as_str().into(),
                    class_id: class_info.class_id,
                    public: method_is_public(ctx.module, class_name, method_key),
                    method_key: method_key.into(),
                    method_name: method_key.into(),
                    impl_class: impl_class.into(),
                    sig: std::sync::Arc::new(sig.clone()),
                }
            })
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(|target| target.class_id);
    if targets.is_empty() {
        emit_runtime_callable_array_no_match_abort(ctx);
        return Ok(());
    }

    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let miss_label = ctx.next_label("subclass_invokable_no_match");
    let done_label = ctx.next_label("subclass_invokable_done");
    let match_labels = targets
        .iter()
        .map(|target| {
            ctx.next_label(&format!(
                "subclass_invokable_{}",
                label_fragment(&target.class_name)
            ))
        })
        .collect::<Vec<_>>();
    ctx.load_value_to_reg(receiver, receiver_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [{}]", receiver_reg)); // load the concrete class id for invokable-subclass selection
            for (target, label) in targets.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", target.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare against this invokable subclass id
                ctx.emitter.instruction(&format!("b.eq {}", label));            // select the matching invokable descriptor
            }
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [{}]", receiver_reg)); // load the concrete class id for invokable-subclass selection
            for (target, label) in targets.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", target.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare against this invokable subclass id
                ctx.emitter.instruction(&format!("je {}", label));              // select the matching invokable descriptor
            }
        }
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (target, label) in targets.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let template = runtime_instance_method_descriptor_template(
            ctx,
            &target.class_name,
            method_key,
            method_key,
            &target.impl_class,
            &target.sig,
        )?;
        emit_runtime_descriptor_with_receiver_capture(
            ctx,
            &template.descriptor_label,
            receiver,
            &PhpType::Object(target.class_name.to_string()),
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    emit_runtime_callable_array_no_match_abort(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Returns whether `candidate` is `base_class` or inherits from it.
fn runtime_class_is_same_or_descendant(
    ctx: &FunctionContext<'_>,
    candidate: &str,
    base_class: &str,
) -> bool {
    let base_key = php_symbol_key(base_class.trim_start_matches('\\'));
    let mut current = Some(candidate.trim_start_matches('\\'));
    while let Some(class_name) = current {
        if php_symbol_key(class_name) == base_key {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(class_name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns one module-wide static descriptor template for a public instance method.
/// Receiver objects are captured into runtime copies, so the wrapper, invoker, and
/// immutable descriptor header can be shared by every dynamic call site.
pub(super) fn runtime_instance_method_descriptor_template(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    method_key: &str,
    impl_class: &str,
    sig: &FunctionSig,
) -> Result<RuntimeInstanceMethodDescriptorTemplate> {
    if let Some(template) = ctx.shared.runtime_instance_method_descriptor(
        class_name,
        method_key,
        impl_class,
        sig,
    ) {
        return Ok(template);
    }
    let receiver_ty = PhpType::Object(class_name.to_string());
    let captures = vec![("receiver".to_string(), receiver_ty, false)];
    let entry_label =
        emit_instance_method_descriptor_entry_wrapper(ctx, impl_class, method_key, sig)?;
    // Bounded: an instance-method first-class callable (`$obj->method(...)`) is routinely handed
    // to interpreted code, which then calls it inside a PHP `try`. Without the boundary a
    // Throwable raised in the callee unwinds natively past that `try`.
    let owns_string_return = crate::codegen::runtime_callable_invoker::method_returns_owned_string(
        ctx.module, impl_class, method_key, false,
    );
    let invoker_label = super::runtime_wrappers::emit_runtime_callable_invoker_bounded_in_class(
        ctx,
        sig,
        &captures,
        owns_string_return,
        true,
        Some(impl_class),
    );
    let php_name = format!("{}::{}", impl_class, method_name);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        ctx.data,
        &entry_label,
        Some(&php_name),
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        Some(sig),
        &captures,
        &[],
        callable_descriptor::CallableDescriptorInvocation::method(
            callable_descriptor::CallableDescriptorShape::InstanceMethod,
            Some(class_name.to_string()),
            method_name,
        ),
        Some(&invoker_label),
    );
    let template = RuntimeInstanceMethodDescriptorTemplate { descriptor_label };
    ctx.shared.cache_runtime_instance_method_descriptor(
        class_name,
        method_key,
        impl_class,
        sig,
        template.clone(),
    );
    Ok(template)
}

/// Verifies that a descriptor-invoker argument operand is a supported container shape.
fn require_descriptor_arg_container(
    ctx: &FunctionContext<'_>,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let arg_ty = ctx.value_php_type(arg_mixed)?.codegen_repr();
    if matches!(
        arg_ty,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. }
    ) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} argument container has unsupported PHP type {:?}",
        op_name, arg_ty
    )))
}

/// Lowers runtime `[$object, $method]` callable arrays through public method cases.
fn lower_runtime_mixed_callable_array_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let args = inst.operands.iter().skip(1).copied().collect::<Vec<_>>();
    let targets = runtime_array_instance_method_targets(ctx, args.len());
    if targets.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime callable array with {} positional args",
            op_name,
            args.len()
        )));
    }

    emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::RawArray(callable))?;
    let done_label = ctx.next_label("callable_array_runtime_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    for target in &targets {
        let next_label = ctx.next_label("callable_array_instance_next");
        emit_branch_if_runtime_array_instance_mismatch(ctx, target, &next_label);
        emit_runtime_array_instance_method_call(ctx, inst, &args, target)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    emit_runtime_callable_array_no_match_abort(ctx);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    Ok(())
}

/// Selects a descriptor for a runtime `array<mixed>` callable and invokes it.
fn lower_runtime_mixed_callable_array_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    if instance_targets.is_empty()
        && static_cases.is_empty()
        && !ctx.module.required_runtime_features.eval_bridge
        && !module_has_unreachable_callable_methods(ctx)
    {
        return Err(CodegenIrError::unsupported(
            "callable_descriptor_invoke for runtime mixed callable array with no descriptor targets",
        ));
    }

    emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::RawArray(callable))?;
    emit_mixed_callable_array_descriptor_dispatch(
        ctx,
        inst,
        callable,
        arg_mixed,
        op_name,
        &instance_targets,
        &static_cases,
    )
}

/// Dispatches already-filled callable-array selector slots to a matching descriptor.
///
/// Assumes the 64-byte selector block (`MIXED_*_OFFSET` slots) is already on the
/// temporary stack. Tries each public instance-method target (receiver class-id and
/// method-name match) then each public static-method case; a match invokes that
/// descriptor with the prebuilt `arg_mixed` container, and no match is a fatal.
/// Releases the selector block on the matched path before returning. Shared by the
/// statically-typed `array<mixed>` path and the boxed-Mixed indexed-array path.
fn emit_mixed_callable_array_descriptor_dispatch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
    instance_targets: &[RuntimeArrayInstanceMethodTarget],
    static_cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
) -> Result<()> {
    let done_label = ctx.next_label("callable_array_runtime_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    emit_mixed_callable_array_descriptor_lookup(
        ctx,
        instance_targets,
        static_cases,
        &miss_label,
        &invoke_inaccessible_method_error(ctx, inst),
    )?;
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let owned_flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    abi::emit_reg_move(
        ctx.emitter,
        descriptor_reg,
        abi::int_result_reg(ctx.emitter),
    );
    emit_descriptor_reg_invoker_call_with_mixed_arg_and_runtime_ownership(
        ctx,
        inst,
        descriptor_reg,
        owned_flag_reg,
        arg_mixed,
        op_name,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    if super::builtins::has_eval_context(ctx) {
        super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else if ctx.module.required_runtime_features.eval_bridge {
        super::builtins::lower_eval_global_callable_call_array(
            ctx,
            inst,
            callable,
            arg_mixed,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        emit_runtime_callable_array_no_match_abort(ctx);
    }

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    Ok(())
}

/// Selects a descriptor for a runtime `array<string>` static-method callable.
fn lower_runtime_string_callable_array_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    if ctx.module.required_runtime_features.eval_bridge {
        normalize_typed_callable_array_to_mixed(ctx, callable)?;
        lower_runtime_mixed_callable_array_descriptor_invoke(
            ctx,
            inst,
            callable,
            arg_mixed,
            op_name,
        )?;
        return restore_typed_callable_array_after_mixed_use(ctx, callable);
    }

    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    if static_cases.is_empty() {
        return Err(CodegenIrError::unsupported(
            "callable_descriptor_invoke for runtime string callable array with no static targets",
        ));
    }

    emit_string_callable_array_selector_slots(ctx, callable)?;
    let done_label = ctx.next_label("callable_array_runtime_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    if static_cases.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_static_lookup_table(
            ctx,
            &static_cases,
            STRING_CLASS_OFFSET,
            STRING_CLASS_OFFSET + 8,
            STRING_METHOD_OFFSET,
            STRING_METHOD_OFFSET + 8,
        )?;
        emit_branch_if_lookup_missed(ctx, &miss_label);
        let descriptor_reg = abi::nested_call_reg(ctx.emitter);
        abi::emit_reg_move(
            ctx.emitter,
            descriptor_reg,
            abi::int_result_reg(ctx.emitter),
        );
        emit_descriptor_reg_invoker_call_with_mixed_arg(
            ctx,
            inst,
            descriptor_reg,
            arg_mixed,
            op_name,
            false,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&miss_label);
        emit_runtime_callable_array_no_match_abort(ctx);
        ctx.emitter.label(&done_label);
        abi::emit_release_temporary_stack(ctx.emitter, STRING_SELECTOR_BYTES);
        return Ok(());
    }
    for case in &static_cases {
        let next_label = ctx.next_label("callable_array_static_next");
        emit_branch_if_string_static_case_mismatch(ctx, case, &next_label);
        emit_static_descriptor_case_invoke(ctx, inst, arg_mixed, &case.case.descriptor_label)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    emit_runtime_callable_array_no_match_abort(ctx);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_SELECTOR_BYTES);
    Ok(())
}

/// Materializes a callable descriptor selected from a runtime callable-array value.
pub(super) fn emit_runtime_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    emit_runtime_callable_array_descriptor_value_with_callback_error(ctx, callable, op_name, None)
}

/// Same, for a builtin that validates its callback: a method the caller's scope cannot reach is
/// php's `TypeError` `<invalid_callback>, cannot access protected method C::m()`.
///
/// `invalid_callback` is the builtin's own wording up to the reason, for example
/// `array_map(): Argument #1 ($callback) must be a valid callback or null`.
pub(in crate::codegen) fn emit_runtime_callable_array_descriptor_value_with_callback_error(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    invalid_callback: Option<&str>,
) -> Result<()> {
    let inaccessible = match invalid_callback {
        Some(prefix) => InaccessibleMethodError::InvalidCallback(prefix.to_string()),
        None => InaccessibleMethodError::Ignore,
    };
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            emit_mixed_callable_array_descriptor_value(ctx, callable, op_name, &inaccessible)
        }
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => {
            emit_string_callable_array_descriptor_value(ctx, callable, op_name)
        }
        PhpType::Array(_) => {
            normalize_typed_callable_array_to_mixed(ctx, callable)?;
            emit_mixed_callable_array_descriptor_value(ctx, callable, op_name, &inaccessible)?;
            restore_typed_callable_array_after_mixed_use(ctx, callable)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for callable-array PHP type {:?}",
            op_name, other
        ))),
    }
}

/// Copies a typed callable-array candidate into boxed Mixed slots so descriptor selection can
/// inspect each selector's runtime tag rather than trusting a stale flow-inferred element type.
///
/// The copy replaces the array in `callable`'s own storage, and the original is pushed on the
/// temporary stack: every caller must end the span with
/// [`restore_typed_callable_array_after_mixed_use`]. Without it neither array had an owner --
/// an owned `[$class, 'm']` temporary lost its original (three blocks per `$class::m()`), and a
/// borrowed local's load left the copy behind instead.
pub(crate) fn normalize_typed_callable_array_to_mixed(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
) -> Result<()> {
    let original_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(callable, original_reg)?;
    abi::emit_push_reg(ctx.emitter, original_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(callable, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // recover the source indexed-slot runtime value type
            ctx.emitter.instruction("lsr x1, x1, #8");                          // shift the value-type byte into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // discard heap-kind and persistent metadata
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(callable, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the retained candidate to the conversion helper
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // recover the source indexed-slot runtime value type
            ctx.emitter.instruction("shr rsi, 8");                              // shift the value-type byte into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // discard heap-kind and persistent metadata
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    ctx.store_result_value(callable)
}

/// Ends a [`normalize_typed_callable_array_to_mixed`] span: releases the Mixed copy and puts
/// the original array back in `callable`'s storage, so the value owns (or borrows) exactly what
/// it did before. The integer result pair survives, for a caller that selected a descriptor.
pub(crate) fn restore_typed_callable_array_after_mixed_use(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    abi::emit_push_reg_pair(ctx.emitter, result_reg, flag_reg);
    ctx.load_value_to_reg(callable, result_reg)?;
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    ctx.store_result_value(callable)?;
    abi::emit_pop_reg_pair(ctx.emitter, result_reg, flag_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Materializes an instance-method descriptor selected from a mixed callable-array value.
pub(super) fn emit_runtime_mixed_instance_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for instance callable-array PHP type {:?}",
                op_name, other
            )))
        }
    }

    let targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    if targets.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime mixed callable array with no instance descriptor targets",
            op_name
        )));
    }

    emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::RawArray(callable))?;
    let done_label = ctx.next_label("callable_array_instance_descriptor_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_instance_missing", op_name));
    emit_inline_callable_array_lookup(
        ctx,
        &targets,
        &[],
        &miss_label,
        &InaccessibleMethodError::Ignore,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    emit_runtime_callable_array_no_match_abort(ctx);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    Ok(())
}

/// Selects a callable descriptor from a mixed callable-array value.
fn emit_mixed_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
    inaccessible: &InaccessibleMethodError,
) -> Result<()> {
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    // With the eval bridge linked, no compiled candidate is still a lowerable program: every
    // receiver in the array can legitimately be a runtime-declared class, which only the
    // interpreter can dispatch.
    if instance_targets.is_empty()
        && static_cases.is_empty()
        && !ctx.module.required_runtime_features.eval_bridge
        && !module_has_unreachable_callable_methods(ctx)
    {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime mixed callable array with no descriptor targets",
            op_name
        )));
    }

    emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::RawArray(callable))?;
    let done_label = ctx.next_label("callable_array_descriptor_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    let exit_label = ctx.next_label("callable_array_descriptor_exit");
    emit_mixed_callable_array_descriptor_lookup(
        ctx,
        &instance_targets,
        &static_cases,
        &miss_label,
        inaccessible,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    emit_callable_array_descriptor_miss(ctx, callable, MIXED_SELECTOR_BYTES, &exit_label)?;

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    ctx.emitter.label(&exit_label);
    Ok(())
}

/// Selects a callable descriptor from a string-only static callable-array value.
fn emit_string_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    // As above: with the eval bridge linked, `['Svc', 'make']` can name a class declared at
    // runtime, so having no compiled static candidate is not a lowering failure.
    if static_cases.is_empty() && !ctx.module.required_runtime_features.eval_bridge {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime string callable array with no static targets",
            op_name
        )));
    }

    emit_string_callable_array_selector_slots(ctx, callable)?;
    let done_label = ctx.next_label("callable_array_descriptor_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    let exit_label = ctx.next_label("callable_array_descriptor_exit");
    if static_cases.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_static_lookup_table(
            ctx,
            &static_cases,
            STRING_CLASS_OFFSET,
            STRING_CLASS_OFFSET + 8,
            STRING_METHOD_OFFSET,
            STRING_METHOD_OFFSET + 8,
        )?;
        emit_branch_if_lookup_missed(ctx, &miss_label);
        abi::emit_jump(ctx.emitter, &done_label);
    } else {
        for case in &static_cases {
            let next_label = ctx.next_label("callable_array_static_next");
            emit_branch_if_string_static_case_mismatch(ctx, case, &next_label);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &case.case.descriptor_label,
            );
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&next_label);
        }
        abi::emit_jump(ctx.emitter, &miss_label);
    }

    ctx.emitter.label(&miss_label);
    emit_callable_array_descriptor_miss(ctx, callable, STRING_SELECTOR_BYTES, &exit_label)?;

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_SELECTOR_BYTES);
    ctx.emitter.label(&exit_label);
    Ok(())
}

/// Emits the tail a callable-array descriptor selection reaches when no compiled candidate matched.
///
/// The candidate tables only describe AOT-compiled classes, so a receiver whose class was declared
/// at runtime always misses. That is not "not callable": it is a callable the interpreter owns, and
/// this hands it to the same eval descriptor the boxed-Mixed selection path uses. Only a value the
/// interpreter also refuses reaches the abort, which is the behaviour every miss used to have.
///
/// The selector slots are dead on this path, so they are released here rather than at the
/// success label; control rejoins at `exit_label`, past that release.
fn emit_callable_array_descriptor_miss(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    selector_bytes: usize,
    exit_label: &str,
) -> Result<()> {
    if !ctx.module.required_runtime_features.eval_bridge {
        emit_runtime_callable_array_no_match_abort(ctx);
        return Ok(());
    }
    abi::emit_release_temporary_stack(ctx.emitter, selector_bytes);
    let not_callable_label = ctx.next_label("callable_array_descriptor_not_callable");
    emit_eval_owned_callable_descriptor(ctx, callable, &not_callable_label)?;
    abi::emit_jump(ctx.emitter, exit_label);
    ctx.emitter.label(&not_callable_label);
    emit_runtime_callable_array_no_match_abort(ctx);
    Ok(())
}

/// The private methods of the call site's own class, as seen on an object of a DESCENDANT class.
///
/// A `private` method is not inherited into a child's method table, yet php resolves
/// `[$child, 'm']` written inside the declaring class `S` to `S::m`: the callable is checked from
/// `S`'s scope, which owns a private `m` and so wins over whatever the child holds under the same
/// name, including a method the child redeclared. Without these rows an `array_map([$this, 'm'],
/// ...)` in a parent class stopped working the moment the object was a subclass instance.
///
/// Returns the extra targets plus the `(class, method key)` pairs they replace in the ordinary
/// tables, because a table holds one value per key.
fn scope_private_method_overrides(
    ctx: &FunctionContext<'_>,
    arity: Option<usize>,
) -> (
    Vec<RuntimeArrayInstanceMethodTarget>,
    std::collections::HashSet<(String, String)>,
) {
    let mut targets = Vec::new();
    let mut shadowed = std::collections::HashSet::new();
    let Some(scope) = ctx.function.lexical_class.as_deref() else {
        return (targets, shadowed);
    };
    let scope = scope.trim_start_matches('\\');
    let Some(scope_info) = ctx.module.class_infos.get(scope) else {
        return (targets, shadowed);
    };
    let mut private_methods = scope_info
        .methods
        .iter()
        .filter(|(name, _)| {
            scope_info.method_visibilities.get(*name) == Some(&Visibility::Private)
        })
        .collect::<Vec<_>>();
    if private_methods.is_empty() {
        return (targets, shadowed);
    }
    private_methods.sort_by(|left, right| left.0.cmp(right.0));
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        if class_name.trim_start_matches('\\') == scope
            || !class_descends_from_module_class_name(ctx, class_name, scope)
        {
            continue;
        }
        for (method_name, physical_sig) in &private_methods {
            let sig = match arity {
                None => (*physical_sig).clone(),
                Some(arg_count) => {
                    let Ok(sig) =
                        crate::codegen_support::source_method_adapters::source_visible_signature(
                            physical_sig,
                        )
                    else {
                        continue;
                    };
                    if sig.params.len() != arg_count || sig.variadic.is_some() {
                        continue;
                    }
                    sig
                }
            };
            let method_key = php_symbol_key(method_name);
            let impl_class = scope_info
                .method_impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| scope.to_string());
            if !class_method_already_emitted(ctx, &impl_class, &method_key, false) {
                continue;
            }
            shadowed.insert((class_name.clone(), method_key.clone()));
            targets.push(RuntimeArrayInstanceMethodTarget {
                class_name: class_name.as_str().into(),
                class_id: class_info.class_id,
                public: method_is_public(ctx.module, class_name, method_name.as_str()),
                method_key: method_key.into(),
                method_name: method_name.as_str().into(),
                impl_class: impl_class.into(),
                sig: std::sync::Arc::new(sig),
            });
        }
    }
    (targets, shadowed)
}

/// Collects public instance methods for runtime descriptor selection.
///
/// Depends on the calling scope only (`ctx.function.lexical_class`), so it is built once per
/// scope and shared by every callable-array site in it.
fn runtime_array_instance_method_targets_for_descriptor(
    ctx: &FunctionContext<'_>,
) -> std::sync::Arc<Vec<RuntimeArrayInstanceMethodTarget>> {
    ctx.shared.instance_method_targets_for_scope(ctx.function.lexical_class.as_deref(), || {
        build_runtime_array_instance_method_targets(ctx)
    })
}

/// Builds `runtime_array_instance_method_targets_for_descriptor`'s answer for `ctx`'s scope.
fn build_runtime_array_instance_method_targets(
    ctx: &FunctionContext<'_>,
) -> Vec<RuntimeArrayInstanceMethodTarget> {
    #[cfg(test)]
    INSTANCE_TARGET_BUILDS.with(|count| count.set(count.get() + 1));
    let mut targets = Vec::new();
    let (overrides, shadowed) = scope_private_method_overrides(ctx, None);
    // The scope-independent part (every emitted instance method, sorted by class then method)
    // is built once per codegen state; a site only applies its scope's visibility and private
    // shadowing to it, instead of re-sorting and re-cloning the whole class x method universe.
    let base = ctx.shared.instance_method_target_base(|| instance_method_target_base(ctx));
    for target in base.iter() {
        if !shadowed.is_empty()
            && shadowed.contains(&(target.class_name.to_string(), target.method_key.to_string()))
        {
            continue;
        }
        let Some(class_info) = ctx.module.class_infos.get(&*target.class_name) else {
            continue;
        };
        if !instance_method_reachable_here(ctx, class_info, &target.method_name, &target.impl_class) {
            continue;
        }
        targets.push(target.clone());
    }
    targets.extend(overrides);
    targets
}

/// Every emitted instance method of the module as a descriptor candidate, sorted by class then
/// method name: the part of `runtime_array_instance_method_targets_for_descriptor` that does not
/// depend on the calling scope.
fn instance_method_target_base(ctx: &FunctionContext<'_>) -> Vec<RuntimeArrayInstanceMethodTarget> {
    let mut targets = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut methods = class_info.methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, sig) in methods {
            let method_key = php_symbol_key(method_name);
            let impl_class = class_info
                .method_impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !class_method_already_emitted(ctx, &impl_class, &method_key, false) {
                continue;
            }
            targets.push(RuntimeArrayInstanceMethodTarget {
                class_name: class_name.as_str().into(),
                class_id: class_info.class_id,
                public: method_is_public(ctx.module, class_name, method_name.as_str()),
                method_key: method_key.into(),
                method_name: method_name.as_str().into(),
                impl_class: impl_class.into(),
                sig: std::sync::Arc::new(sig.clone()),
            });
        }
    }
    targets
}

/// Builds public static-method descriptor cases directly from EIR class metadata.
fn runtime_static_method_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    candidate_names: Option<&[String]>,
) -> Vec<callable_dispatch::RuntimeStaticMethodCallableCase> {
    if let Some(cases) = ctx
        .shared
        .runtime_static_method_descriptor_cases(candidate_names)
    {
        return cases;
    }
    let mut methods = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut static_methods = class_info.static_methods.iter().collect::<Vec<_>>();
        static_methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, sig) in static_methods {
            if !class_info
                .static_method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
            let method_key = php_symbol_key(method_name);
            let php_name = format!("{}::{}", class_name, method_name);
            if !runtime_callable_name_is_reachable(&php_name, candidate_names) {
                continue;
            }
            let impl_class = class_info
                .static_method_impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !class_method_already_emitted(ctx, &impl_class, &method_key, true) {
                continue;
            }
            methods.push((
                class_name.clone(),
                method_name.clone(),
                method_key,
                impl_class,
                class_info.class_id,
                sig.clone(),
            ));
        }
    }

    let mut cases = Vec::new();
    for (class_name, method_name, method_key, impl_class, class_id, sig) in methods {
        if let Some(case) = build_static_method_descriptor_case(
            ctx,
            &class_name,
            &method_name,
            &method_key,
            &impl_class,
            class_id,
            &sig,
        ) {
            cases.push(case);
        }
    }
    if let Some(case) = closure_from_callable_case(ctx, candidate_names) {
        cases.push(case);
    }
    ctx.shared
        .cache_runtime_static_method_descriptor_cases(candidate_names, &cases);
    cases
}

/// The `Closure::fromCallable` row: `Closure` is no compiled class, so its one static method a
/// run-time callable can name points at the injected helper class
/// (`crate::closure_from_callable_prelude`), when the program has it.
fn closure_from_callable_case(
    ctx: &mut FunctionContext<'_>,
    candidate_names: Option<&[String]>,
) -> Option<callable_dispatch::RuntimeStaticMethodCallableCase> {
    use crate::closure_from_callable_prelude::{CLOSURE_FACTORY_CLASS, CLOSURE_FACTORY_METHOD};
    if !runtime_callable_name_is_reachable("Closure::fromCallable", candidate_names) {
        return None;
    }
    let (class_name, class_info) = ctx
        .module
        .class_infos
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(CLOSURE_FACTORY_CLASS))?;
    let (method_name, sig) = class_info
        .static_methods
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(CLOSURE_FACTORY_METHOD))?;
    let (class_name, method_name, sig) = (class_name.clone(), method_name.clone(), sig.clone());
    let class_id = class_info.class_id;
    let method_key = php_symbol_key(&method_name);
    if !class_method_already_emitted(ctx, &class_name, &method_key, true) {
        return None;
    }
    let mut case = build_static_method_descriptor_case(
        ctx,
        &class_name,
        &method_name,
        &method_key,
        &class_name,
        class_id,
        &sig,
    )?;
    case.class_name = "Closure".to_string();
    case.method_name = "fromCallable".to_string();
    Some(case)
}

/// Collects static methods this CALL SITE may reach that the public ladder leaves out.
///
/// `[$object, 'name']` is a valid PHP callable when `name` is a STATIC method, and a call written
/// inside the class may reach a `protected` or `private` one. The shared ladder is built for an
/// arbitrary callable, so it keeps public methods only — which is right for a callable that
/// escapes, and wrong for `$this->{$name}()`.
///
/// Symfony's generated container is the shape that needs this: `Container::make` dispatches every
/// service through `$container->{$container->methodMap[$id]}($container)` and each getter is
/// `protected static`, so every service lookup fell through to the interpreter.
///
/// The scope is the lexical class of the function being emitted, which is exactly what PHP checks:
/// `protected` is reachable from a class related to the declaring one, `private` only from the
/// declaring class itself.
fn runtime_scope_visible_static_method_cases(
    ctx: &mut FunctionContext<'_>,
) -> Vec<callable_dispatch::RuntimeStaticMethodCallableCase> {
    let Some(scope) = ctx.function.lexical_class.clone() else {
        return Vec::new();
    };
    let scope = scope.trim_start_matches('\\').to_string();
    let mut collected = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut static_methods = class_info.static_methods.iter().collect::<Vec<_>>();
        static_methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, sig) in static_methods {
            let Some(visibility) = class_info.static_method_visibilities.get(method_name) else {
                continue;
            };
            // The public ones are already in the shared ladder; only the rest are new here.
            if matches!(visibility, Visibility::Public) {
                continue;
            }
            if !scope_reaches_member(ctx, &scope, class_name, visibility) {
                continue;
            }
            let method_key = php_symbol_key(method_name);
            let impl_class = class_info
                .static_method_impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !class_method_already_emitted(ctx, &impl_class, &method_key, true) {
                continue;
            }
            collected.push((
                class_name.clone(),
                method_name.clone(),
                method_key,
                impl_class,
                class_info.class_id,
                sig.clone(),
            ));
        }
    }
    let mut cases = Vec::new();
    for (class_name, method_name, method_key, impl_class, class_id, sig) in collected {
        if let Some(case) = build_static_method_descriptor_case(
            ctx,
            &class_name,
            &method_name,
            &method_key,
            &impl_class,
            class_id,
            &sig,
        ) {
            cases.push(case);
        }
    }
    cases
}

/// Whether an `[$object, 'name']` callable built in the function being emitted may reach this
/// instance method.
///
/// Public methods are reachable from anywhere. A `protected` or `private` one is reachable when
/// the function's lexical class reaches the class that implements it, which is the check PHP makes
/// for a callable invoked from inside a class: Laravel's `Container::getClosure()` closure calls
/// `$container->resolve(..., raiseEvents: false)`, lowered as `call_user_func_array([$container,
/// 'resolve'], ...)`, and the public-only ladder sent the protected `resolve()` to `__call`.
fn instance_method_reachable_here(
    ctx: &FunctionContext<'_>,
    class_info: &crate::types::ClassInfo,
    method_name: &str,
    impl_class: &str,
) -> bool {
    let Some(visibility) = class_info.method_visibilities.get(method_name) else {
        return false;
    };
    if matches!(visibility, Visibility::Public) {
        return true;
    }
    let Some(scope) = ctx.function.lexical_class.as_deref() else {
        return false;
    };
    scope_reaches_member(ctx, scope.trim_start_matches('\\'), impl_class, visibility)
}

/// Applies PHP's visibility rule for a member declared by `declaring_class` seen from `scope`.
fn scope_reaches_member(
    ctx: &FunctionContext<'_>,
    scope: &str,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    let declaring = declaring_class.trim_start_matches('\\');
    if scope == declaring {
        return true;
    }
    // `ReflectionMethod::invoke()` / `invokeArgs()` / `getClosure()` and `ReflectionProperty`'s
    // accessors reach private and protected members by design (php has ignored `setAccessible()`
    // since 8.1). Their synthetic bodies call through the ordinary callable machinery with the
    // reflection class as lexical scope, the scope no user code can have -- the interpreter's
    // `validate_eval_member_access` makes the same exception. Without it the per-scope lookup
    // table left `App\Kernel::configureRoutes()` unreachable from `MicroKernelTrait::loadRoutes()`
    // and every Symfony console command that builds the router or the container failed.
    if matches!(scope, "ReflectionMethod" | "ReflectionProperty") {
        return true;
    }
    // The run-time `['Closure', 'fromCallable']` helper converts with its caller's access, which
    // it cannot know: Symfony's container wraps the protected `[$container, 'getEnv']` from inside
    // `ContainerBuilder`. See `crate::closure_from_callable_prelude`.
    if scope == crate::closure_from_callable_prelude::CLOSURE_FACTORY_CLASS {
        return true;
    }
    match visibility {
        Visibility::Private => false,
        // PHP reaches a `protected` member from anywhere in the same hierarchy, in either
        // direction: a parent method may call one a descendant declares on an instance of it.
        _ => {
            class_descends_from_module_class_name(ctx, scope, declaring)
                || class_descends_from_module_class_name(ctx, declaring, scope)
        }
    }
}

/// Walks a module class's parent chain looking for `ancestor`.
fn class_descends_from_module_class_name(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let mut current = ctx
        .module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.parent.as_deref());
    while let Some(parent) = current {
        let normalized = parent.trim_start_matches('\\');
        if normalized == ancestor {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(normalized)
            .and_then(|info| info.parent.as_deref());
    }
    false
}

/// Emits (or reuses) the descriptor for one static method reachable as a callable.
fn build_static_method_descriptor_case(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
    method_key: &str,
    impl_class: &str,
    class_id: u64,
    sig: &FunctionSig,
) -> Option<callable_dispatch::RuntimeStaticMethodCallableCase> {
    {
        let class_name = class_name.to_string();
        let method_name = method_name.to_string();
        let wrapper_sig = callable_dispatch::static_method_runtime_wrapper_sig(sig);
        let php_name = format!("{}::{}", class_name, method_name);
        if let Some(case) = ctx
            .shared
            .runtime_static_method_descriptor_case(&php_name)
        {
            return Some(case);
        }
        let Ok(entry_label) = emit_static_method_descriptor_entry_wrapper(
            ctx,
            impl_class,
            method_key,
            &wrapper_sig,
            class_id,
        ) else {
            return None;
        };
        let owns_string_return = crate::codegen::runtime_callable_invoker::method_returns_owned_string(
            ctx.module, &impl_class, &method_key, true,
        );
        let invoker_label = emit_runtime_callable_invoker_in_class(
            ctx, &wrapper_sig, &[], owns_string_return, true, Some(&impl_class),
        );
        let declared_name = format!("{}::{}", impl_class, method_name);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &entry_label,
            Some(&declared_name),
            callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
            Some(&wrapper_sig),
            &[],
            &[],
            callable_descriptor::CallableDescriptorInvocation::method(
                callable_descriptor::CallableDescriptorShape::StaticMethod,
                Some(class_name.clone()),
                method_name.as_str(),
            ),
            Some(&invoker_label),
        );
        let case = callable_dispatch::RuntimeCallableCase {
            descriptor_label,
            php_name: Some(php_name),
        };
        let static_case = callable_dispatch::RuntimeStaticMethodCallableCase {
            class_name,
            method_name,
            case,
        };
        ctx.shared
            .cache_runtime_static_method_descriptor_case(&static_case);
        Some(static_case)
    }
}

/// Collects public instance-method targets that can receive this positional shape.
fn runtime_array_instance_method_targets(
    ctx: &FunctionContext<'_>,
    arg_count: usize,
) -> Vec<RuntimeArrayInstanceMethodTarget> {
    let mut targets = Vec::new();
    let (overrides, shadowed) = scope_private_method_overrides(ctx, Some(arg_count));
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut methods = class_info.methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, physical_sig) in methods {
            let Ok(sig) = crate::codegen_support::source_method_adapters::source_visible_signature(
                physical_sig,
            ) else {
                continue;
            };
            if sig.params.len() != arg_count || sig.variadic.is_some() {
                continue;
            }
            let method_key = php_symbol_key(method_name);
            if shadowed.contains(&(class_name.clone(), method_key.clone())) {
                continue;
            }
            let impl_class = class_info
                .method_impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !instance_method_reachable_here(ctx, class_info, method_name, &impl_class) {
                continue;
            }
            if !class_method_already_emitted(ctx, &impl_class, &method_key, false) {
                continue;
            }
            targets.push(RuntimeArrayInstanceMethodTarget {
                class_name: class_name.as_str().into(),
                class_id: class_info.class_id,
                public: method_is_public(ctx.module, class_name, method_name.as_str()),
                method_key: method_key.into(),
                method_name: method_name.as_str().into(),
                impl_class: impl_class.into(),
                sig: std::sync::Arc::new(sig),
            });
        }
    }
    targets.extend(overrides);
    targets
}

/// Saves the receiver and method slots from a mixed callable array.
///
/// Reads both elements of the `[$obj, "method"]` array through the `source`, unboxes
/// each to a runtime tag/payload triple, and pushes them so the later dispatch can
/// match receiver class-id and method name. `RawArray` reads a directly-typed
/// `array<mixed>`; `BoxedArray` reads an array that arrived wrapped in a `Mixed` cell.
fn emit_mixed_callable_array_selector_slots(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
) -> Result<()> {
    emit_mixed_callable_array_selector_slots_with_error(ctx, source, None)
}

/// Reads callable-array selectors while preserving a builtin's catchable validation policy.
fn emit_mixed_callable_array_selector_slots_with_error(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    type_error: Option<&'static str>,
) -> Result<()> {
    emit_require_mixed_callable_array_pair(ctx, source, type_error)?;
    match source {
        CallableArraySource::RawArray(callable) if value_is_array_literal(ctx, *callable) => {
            ctx.emitter.comment("runtime callable-array literal mixed selector");
        }
        CallableArraySource::RawArray(_) => {
            ctx.emitter.comment("runtime callable-array mixed selector");
        }
        CallableArraySource::BoxedArray(_) => {
            ctx.emitter.comment("runtime callable-array boxed-mixed selector");
        }
    }
    if matches!(source, CallableArraySource::BoxedArray(_)) {
        return emit_boxed_callable_array_selector_slots(ctx, source, type_error);
    }
    emit_unbox_mixed_callable_array_slot(ctx, source, 0)?;
    emit_push_mixed_unbox_payload(ctx);
    emit_unbox_mixed_callable_array_slot(ctx, source, 1)?;
    emit_push_mixed_unbox_payload(ctx);
    Ok(())
}

/// Saves selectors from a boxed callable array according to its runtime slot stamp.
///
/// `array<mixed>` stores boxed-cell pointers in 8-byte slots, while a static
/// `['Class', 'method']` callback normally arrives as `array<string>` with
/// pointer/length pairs in 16-byte slots. Both paths produce the same 64-byte
/// tag-plus-payload selector block consumed by the shared dispatch below.
fn emit_boxed_callable_array_selector_slots(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    type_error: Option<&'static str>,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let stamp_reg = abi::secondary_scratch_reg(ctx.emitter);
    let mixed_label = ctx.next_label("boxed_callable_array_mixed_slots");
    let string_label = ctx.next_label("boxed_callable_array_string_slots");
    let done_label = ctx.next_label("boxed_callable_array_slots_done");
    emit_load_callable_array_base(ctx, source, array_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [{}, #-8]", stamp_reg, array_reg)); // load the indexed-array packed value-type stamp
            ctx.emitter.instruction(&format!("lsr {}, {}, #8", stamp_reg, stamp_reg)); // move the value-type byte into the low bits
            ctx.emitter.instruction(&format!("and {}, {}, #0x7f", stamp_reg, stamp_reg)); // isolate the callable-array slot representation
            ctx.emitter.instruction(&format!("cmp {}, #7", stamp_reg));         // do the array slots contain boxed Mixed cells?
            ctx.emitter.instruction(&format!("b.eq {}", mixed_label));          // decode each selector through __rt_mixed_unbox
            ctx.emitter.instruction(&format!("cmp {}, #1", stamp_reg));         // do the array slots contain string pointer/length pairs?
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // synthesize string-tagged selectors from typed slots
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} - 8]", stamp_reg, array_reg)); // load the indexed-array packed value-type stamp
            ctx.emitter.instruction(&format!("shr {}, 8", stamp_reg));          // move the value-type byte into the low bits
            ctx.emitter.instruction(&format!("and {}, 0x7f", stamp_reg));       // isolate the callable-array slot representation
            ctx.emitter.instruction(&format!("cmp {}, 7", stamp_reg));          // do the array slots contain boxed Mixed cells?
            ctx.emitter.instruction(&format!("je {}", mixed_label));            // decode each selector through __rt_mixed_unbox
            ctx.emitter.instruction(&format!("cmp {}, 1", stamp_reg));          // do the array slots contain string pointer/length pairs?
            ctx.emitter.instruction(&format!("je {}", string_label));           // synthesize string-tagged selectors from typed slots
        }
    }
    emit_callable_array_shape_error(ctx, type_error);

    ctx.emitter.label(&mixed_label);
    emit_unbox_mixed_callable_array_slot(ctx, source, 0)?;
    emit_push_mixed_unbox_payload(ctx);
    emit_unbox_mixed_callable_array_slot(ctx, source, 1)?;
    emit_push_mixed_unbox_payload(ctx);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&string_label);
    emit_push_boxed_string_callable_array_slot(ctx, source, 0)?;
    emit_push_boxed_string_callable_array_slot(ctx, source, 1)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Pushes one typed string-array slot as a Mixed-compatible selector block.
fn emit_push_boxed_string_callable_array_slot(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    slot: usize,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let offset = 24 + slot * 16;
    emit_load_callable_array_base(ctx, source, array_reg)?;
    abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, offset);
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, offset + 8);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, MIXED_TAG_STRING);
    abi::emit_push_reg(ctx.emitter, tag_reg);
    Ok(())
}

/// Saves class and method string slots from a runtime static callable array.
fn emit_string_callable_array_selector_slots(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
) -> Result<()> {
    emit_require_string_callable_array_pair(ctx, callable)?;
    if value_is_array_literal(ctx, callable) {
        ctx.emitter
            .comment("runtime callable-array literal string selector");
    } else {
        ctx.emitter
            .comment("runtime callable-array string selector");
    }
    emit_push_string_callable_array_slot(ctx, callable, 0)?;
    emit_push_string_callable_array_slot(ctx, callable, 1)?;
    Ok(())
}

/// Pushes one two-word string slot from a callable array onto the temporary stack.
fn emit_push_string_callable_array_slot(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    slot: usize,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let offset = 24 + slot * 16;
    ctx.load_value_to_reg(callable, array_reg)?;
    abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, offset);
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, offset + 8);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    Ok(())
}

/// Recognizes array literal selectors through identity-preserving ownership retains.
fn value_is_array_literal(ctx: &FunctionContext<'_>, mut value: ValueId) -> bool {
    loop {
        let Some(value_ref) = ctx.function.value(value) else {
            return false;
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return false;
        };
        let Some(inst) = ctx.function.instruction(inst) else {
            return false;
        };
        if inst.op != Op::Acquire {
            return inst.op == Op::ArrayNew;
        }
        let Some(source) = inst.operands.first() else {
            return false;
        };
        value = *source;
    }
}

/// Loads and unboxes one boxed-Mixed slot from a callable array.
fn emit_unbox_mixed_callable_array_slot(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    slot: usize,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let offset = 24 + slot * 8;
    emit_load_callable_array_base(ctx, source, array_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x0, [{}, #{}]", array_reg, offset)); // load the boxed callable-array selector slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, QWORD PTR [{} + {}]", array_reg, offset)); // load the boxed callable-array selector slot
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    Ok(())
}

/// Materializes the callable-array base pointer into `array_reg` from its source.
///
/// `RawArray` reloads the pointer straight from the value's local slot. `BoxedArray`
/// reloads the boxed `Mixed` indexed array and unboxes it, taking the borrowed array
/// pointer from the unbox payload-low word (`x1`/`rdi`) without changing its refcount.
/// Both forms re-materialize on every call so the pointer survives the intervening
/// `__rt_mixed_unbox` calls that read each selector element.
fn emit_load_callable_array_base(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    array_reg: &str,
) -> Result<()> {
    match source {
        CallableArraySource::RawArray(callable) => {
            ctx.load_value_to_reg(*callable, array_reg)?;
            Ok(())
        }
        CallableArraySource::BoxedArray(callable) => {
            let unbox_arg = abi::int_result_reg(ctx.emitter);
            ctx.load_value_to_reg(*callable, unbox_arg)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("mov {}, x1", array_reg)); // borrow the unboxed indexed-array pointer as the selector base
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("mov {}, rdi", array_reg));// borrow the unboxed indexed-array pointer as the selector base
                }
            }
            Ok(())
        }
    }
}

/// Verifies a mixed callable-array source has exactly two elements before any
/// selector slot is read, preventing short arrays from reading past allocation.
fn emit_require_mixed_callable_array_pair(
    ctx: &mut FunctionContext<'_>,
    source: &CallableArraySource,
    type_error: Option<&'static str>,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_load_callable_array_base(ctx, source, array_reg)?;
    emit_require_callable_array_pair_in_reg(ctx, array_reg, type_error);
    Ok(())
}

/// Verifies a string callable array has exactly two elements before loading its
/// class and method string slots.
fn emit_require_string_callable_array_pair(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(callable, array_reg)?;
    emit_require_callable_array_pair_in_reg(ctx, array_reg, None);
    Ok(())
}

/// Emits the target-aware length check for a loaded indexed-array pointer.
fn emit_require_callable_array_pair_in_reg(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    type_error: Option<&'static str>,
) {
    let valid_label = ctx.next_label("callable_array_pair_valid");
    let invalid_label = ctx.next_label("callable_array_pair_invalid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", array_reg, invalid_label)); // reject a null callable-array pointer before reading its header
            ctx.emitter.instruction(&format!("ldr x10, [{}]", array_reg));      // load the callable-array element count
            ctx.emitter.instruction("cmp x10, #2");                             // require exactly [receiver, method]
            ctx.emitter.instruction(&format!("b.eq {}", valid_label));          // read selectors only for a valid pair
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", array_reg, array_reg)); // reject a null callable-array pointer before reading its header
            ctx.emitter.instruction(&format!("je {}", invalid_label));          // avoid dereferencing a null callable-array pointer
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [{}]", array_reg)); // load the callable-array element count
            ctx.emitter.instruction("cmp r10, 2");                              // require exactly [receiver, method]
            ctx.emitter.instruction(&format!("je {}", valid_label));            // read selectors only for a valid pair
        }
    }
    ctx.emitter.label(&invalid_label);
    emit_callable_array_shape_error(ctx, type_error);
    ctx.emitter.label(&valid_label);
}

/// Uses a catchable builtin diagnostic for malformed pairs without changing legacy call behavior.
fn emit_callable_array_shape_error(ctx: &mut FunctionContext<'_>, type_error: Option<&'static str>) {
    if let Some(message) = type_error {
        super::exceptions::emit_type_error(ctx, message);
    } else {
        emit_runtime_callable_array_no_match_abort(ctx);
    }
}

/// Preserves the tag and payload returned by `__rt_mixed_unbox`.
fn emit_push_mixed_unbox_payload(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Branches to `next_label` unless the saved selector matches this target.
fn emit_branch_if_runtime_array_instance_mismatch(
    ctx: &mut FunctionContext<'_>,
    target: &RuntimeArrayInstanceMethodTarget,
    next_label: &str,
) {
    emit_branch_if_stack_tag_mismatch(ctx, MIXED_RECEIVER_TAG_OFFSET, MIXED_TAG_OBJECT, next_label);
    emit_branch_if_stack_tag_mismatch(ctx, MIXED_METHOD_TAG_OFFSET, MIXED_TAG_STRING, next_label);
    emit_branch_if_saved_receiver_class_id_mismatch(
        ctx,
        target.class_id,
        MIXED_RECEIVER_PAYLOAD_OFFSET,
        next_label,
    );
    emit_branch_if_stack_string_mismatch(
        ctx,
        MIXED_METHOD_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET + 8,
        target.method_name.as_bytes(),
        next_label,
    );
}

/// Emits the arms that let an OBJECT selector reach a static method.
///
/// `[$object, 'name']` calls `name` statically when that is what the class declares — PHP does not
/// require a string class-name selector for it. The descriptor is the ordinary static one, with no
/// receiver bound and no ownership, because a static method takes no `$this`.
///
/// Matching is by the receiver's CLASS ID, so an object whose class declares the method is
/// selected while an unrelated object with a same-named method is not.
fn emit_object_selector_static_arms(
    ctx: &mut FunctionContext<'_>,
    object_static_cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
    done_label: &str,
) -> Result<()> {
    if object_static_cases.len() > INLINE_LOOKUP_LIMIT {
        let table_miss = ctx.next_label("callable_array_object_static_table_miss");
        emit_runtime_object_static_lookup_table(
            ctx,
            object_static_cases,
            MIXED_RECEIVER_PAYLOAD_OFFSET,
            MIXED_METHOD_PAYLOAD_OFFSET,
            MIXED_METHOD_PAYLOAD_OFFSET + 8,
            &table_miss,
        )?;
        emit_branch_if_lookup_missed(ctx, &table_miss);
        // A static method binds no receiver, so the descriptor is handed on unowned — the same
        // flag the chain below set at each of its arms.
        emit_runtime_descriptor_owned_flag(ctx, false);
        abi::emit_jump(ctx.emitter, done_label);
        // Falls through on a miss, exactly where the exhausted chain used to leave control.
        ctx.emitter.label(&table_miss);
        return Ok(());
    }
    for case in object_static_cases {
        let Some(class_id) = ctx
            .module
            .class_infos
            .get(case.class_name.trim_start_matches('\\'))
            .map(|class_info| class_info.class_id)
        else {
            continue;
        };
        let next_label = ctx.next_label("callable_array_object_static_next");
        emit_branch_if_stack_tag_mismatch(
            ctx,
            MIXED_RECEIVER_TAG_OFFSET,
            MIXED_TAG_OBJECT,
            &next_label,
        );
        emit_branch_if_stack_tag_mismatch(
            ctx,
            MIXED_METHOD_TAG_OFFSET,
            MIXED_TAG_STRING,
            &next_label,
        );
        emit_branch_if_saved_receiver_class_id_mismatch(
            ctx,
            class_id,
            MIXED_RECEIVER_PAYLOAD_OFFSET,
            &next_label,
        );
        emit_branch_if_stack_string_mismatch(
            ctx,
            MIXED_METHOD_PAYLOAD_OFFSET,
            MIXED_METHOD_PAYLOAD_OFFSET + 8,
            case.method_name.as_bytes(),
            &next_label,
        );
        abi::emit_symbol_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            &case.case.descriptor_label,
        );
        emit_runtime_descriptor_owned_flag(ctx, false);
        abi::emit_jump(ctx.emitter, done_label);
        ctx.emitter.label(&next_label);
    }
    Ok(())
}

/// Readable part of the shared callable-array search's entry symbol.
const CALLABLE_ARRAY_LOOKUP_PREFIX: &str = "callable_array_lookup";
/// What the shared search leaves in the ownership flag register when nothing matched.
const CALLABLE_ARRAY_LOOKUP_MISS: i64 = 2;
/// What it leaves there when the method exists and the scope cannot reach it, the inaccessible
/// record in the result register.
const CALLABLE_ARRAY_LOOKUP_REFUSED: i64 = 3;

/// Resolves a saved mixed callable-array selector to a descriptor and ownership flag, against
/// every descriptor target the scope reaches.
///
/// The search is the same for every site of a scope, and ran through a whole-program table
/// build at each one: a tenth of a Symfony build. So it lives in one body per scope and how it
/// reports a miss. The selector stays on the site's temporary stack and the body works on a copy.
fn emit_mixed_callable_array_descriptor_lookup(
    ctx: &mut FunctionContext<'_>,
    instance_targets: &[RuntimeArrayInstanceMethodTarget],
    static_cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
    miss_label: &str,
    inaccessible: &InaccessibleMethodError,
) -> Result<()> {
    let miss = match inaccessible {
        InaccessibleMethodError::Ignore => CallableResolverMiss::Ignore,
        InaccessibleMethodError::InvalidCallback(prefix) => {
            CallableResolverMiss::InvalidCallback(prefix.clone())
        }
        InaccessibleMethodError::DirectCall => CallableResolverMiss::DirectCall,
        InaccessibleMethodError::Dynamic(_) | InaccessibleMethodError::AtSite(_) => {
            return emit_inline_callable_array_lookup(
                ctx, instance_targets, static_cases, miss_label, inaccessible,
            );
        }
    };
    if ctx.function.name.contains(CALLABLE_ARRAY_LOOKUP_PREFIX)
        || ctx.local_slot_by_name(crate::names::CLOSURE_SCOPE_LOCAL).is_some()
    {
        return emit_inline_callable_array_lookup(
            ctx, instance_targets, static_cases, miss_label, inaccessible,
        );
    }
    // What a scope can reach decides the search, and only a scope that reaches a non-public method
    // sees anything the global one does not. The scope's name is needed by one message only, a
    // direct call's refusal, which the site raises itself.
    let scope = crate::codegen::shared_mixed_callable::reach_scope(ctx);
    let miss = match (&scope, miss) {
        (None, CallableResolverMiss::DirectCall) => CallableResolverMiss::DirectCallAtSite,
        (_, miss) => miss,
    };
    let reports_at_site = matches!(miss, CallableResolverMiss::DirectCallAtSite);
    let key = format!("{}:{:?}:{}", CALLABLE_ARRAY_LOOKUP_PREFIX, miss, scope.as_deref().unwrap_or(""));
    let label = ctx.keyed_global_label(&key, CALLABLE_ARRAY_LOOKUP_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::MixedCallableArrayLookup(miss),
        scope.as_deref(),
    );
    abi::emit_call_label(ctx.emitter, &label);
    let flag_reg = runtime_descriptor_owned_flag_reg(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #{}", flag_reg, CALLABLE_ARRAY_LOOKUP_MISS)); // did the search find nothing?
            abi::emit_branch_if_equal_wide(ctx.emitter, miss_label);            // nothing matched
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", flag_reg, CALLABLE_ARRAY_LOOKUP_MISS)); // did the search find nothing?
            ctx.emitter.instruction(&format!("je {}", miss_label));             // nothing matched
        }
    }
    if reports_at_site {
        let reachable = ctx.next_label("callable_array_lookup_reachable");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #{}", flag_reg, CALLABLE_ARRAY_LOOKUP_REFUSED)); // did the search find a method this scope cannot reach?
                ctx.emitter.instruction(&format!("b.ne {}", reachable));        // it found a descriptor
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", flag_reg, CALLABLE_ARRAY_LOOKUP_REFUSED)); // did the search find a method this scope cannot reach?
                ctx.emitter.instruction(&format!("jne {}", reachable));         // it found a descriptor
            }
        }
        emit_inaccessible_method_throw(ctx, &InaccessibleMethodError::DirectCall);
        ctx.emitter.label(&reachable);
    }
    Ok(())
}

/// Emits one shared callable-array search over a copy of the caller's 64-byte selector block.
pub(in crate::codegen) fn emit_callable_array_lookup_helper(
    module: &crate::ir::Module,
    emitter: &mut crate::codegen_support::emit::Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    shared: &mut crate::codegen::shared_state::SharedCodegenState,
    label: &str,
    scope: Option<&str>,
    miss: &CallableResolverMiss,
) -> Result<()> {
    let mut function = crate::codegen::shared_helper::helper_function(label, PhpType::Callable);
    function.lexical_class = scope.map(str::to_string);
    let layout = crate::codegen::frame::layout_for_function(
        &function, emitter.target, false, false, false,
    );
    let saved_nested_offset = layout.frame_size - 8;
    let frame_size = layout.frame_size + 16;
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );
    ctx.emitter.blank();
    ctx.emitter.comment(&format!("--- shared callable array search: {} ---", label));
    ctx.emitter.label(label);
    abi::emit_frame_prologue(ctx.emitter, frame_size);
    let nested = abi::nested_call_reg(ctx.emitter);
    abi::store_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_reserve_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    for offset in (0..MIXED_SELECTOR_BYTES).step_by(8) {
        let source = frame_size + MIXED_SELECTOR_BYTES + offset;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("ldr x9, [sp, #{}]", source)); // read the caller's selector word
                ctx.emitter.instruction(&format!("str x9, [sp, #{}]", offset)); // keep it where the search expects it
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", source)); // read the caller's selector word
                ctx.emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r10", offset)); // keep it where the search expects it
            }
        }
    }
    let miss_label = ctx.next_label("callable_array_lookup_miss");
    let done_label = ctx.next_label("callable_array_lookup_found");
    let inaccessible = match miss {
        CallableResolverMiss::Ignore => InaccessibleMethodError::Ignore,
        CallableResolverMiss::InvalidCallback(prefix) => {
            InaccessibleMethodError::InvalidCallback(prefix.clone())
        }
        CallableResolverMiss::DirectCall => InaccessibleMethodError::DirectCall,
        CallableResolverMiss::DirectCallAtSite => InaccessibleMethodError::AtSite(done_label.clone()),
    };
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(&ctx);
    let static_cases = runtime_static_method_descriptor_cases(&mut ctx, None);
    emit_inline_callable_array_lookup(
        &mut ctx, &instance_targets, &static_cases, &miss_label, &inaccessible,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&miss_label);
    let flag_reg = runtime_descriptor_owned_flag_reg(&ctx);
    abi::emit_load_int_immediate(ctx.emitter, flag_reg, CALLABLE_ARRAY_LOOKUP_MISS);
    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
    abi::load_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}

/// Emits the search itself, at the site or in its shared body.
fn emit_inline_callable_array_lookup(
    ctx: &mut FunctionContext<'_>,
    instance_targets: &[RuntimeArrayInstanceMethodTarget],
    static_cases: &[callable_dispatch::RuntimeStaticMethodCallableCase],
    miss_label: &str,
    inaccessible: &InaccessibleMethodError,
) -> Result<()> {
    // An OBJECT selector may name a static method — `[$obj, 'staticName']` is a valid PHP
    // callable — so the object branch gets the public static cases plus the ones this call site's
    // own class scope reaches. The string branch keeps exactly today's set.
    //
    // The two sets stay two tables. The public one has the same content at every site, so
    // `DataSection::add_words` keeps a single copy; merged, every class scope owned a near-copy
    // of it. Measured on the Symfony Demo: 357 tables of 917 KB that differed by 44 rows out of
    // 4 151, 330 MB of `__data`. A method has one visibility, so the key sets never overlap and
    // the search order changes nothing.
    let scope_static_cases = runtime_scope_visible_static_method_cases(ctx);
    // Rows for the methods this scope cannot reach, consulted only after every reachable table
    // missed; see `emit_inaccessible_method_error`.
    let reports_inaccessible = !matches!(inaccessible, InaccessibleMethodError::Ignore)
        && module_has_unreachable_callable_methods(ctx);
    let instance_label = (!instance_targets.is_empty()
        || !static_cases.is_empty()
        || !scope_static_cases.is_empty()
        || reports_inaccessible)
    .then(|| ctx.next_label("callable_array_lookup_instance"));
    let static_label = (!static_cases.is_empty())
        .then(|| ctx.next_label("callable_array_lookup_static"));
    let done_label = ctx.next_label("callable_array_lookup_done");
    emit_branch_if_stack_tag_mismatch(
        ctx,
        MIXED_METHOD_TAG_OFFSET,
        MIXED_TAG_STRING,
        miss_label,
    );
    let tag_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, tag_reg, MIXED_RECEIVER_TAG_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if let Some(label) = &instance_label {
                ctx.emitter.instruction(&format!("cmp {}, #{}", tag_reg, MIXED_TAG_OBJECT)); // classify an object receiver for instance lookup
                ctx.emitter.instruction(&format!("b.eq {}", label));            // resolve an instance callable-array selector
            }
            if let Some(label) = &static_label {
                ctx.emitter.instruction(&format!("cmp {}, #{}", tag_reg, MIXED_TAG_STRING)); // classify a string receiver for static lookup
                ctx.emitter.instruction(&format!("b.eq {}", label));            // resolve a static callable-array selector
            }
        }
        Arch::X86_64 => {
            if let Some(label) = &instance_label {
                ctx.emitter.instruction(&format!("cmp {}, {}", tag_reg, MIXED_TAG_OBJECT)); // classify an object receiver for instance lookup
                ctx.emitter.instruction(&format!("je {}", label));              // resolve an instance callable-array selector
            }
            if let Some(label) = &static_label {
                ctx.emitter.instruction(&format!("cmp {}, {}", tag_reg, MIXED_TAG_STRING)); // classify a string receiver for static lookup
                ctx.emitter.instruction(&format!("je {}", label));              // resolve a static callable-array selector
            }
        }
    }
    abi::emit_jump(ctx.emitter, miss_label);

    if let Some(label) = &instance_label {
        ctx.emitter.label(label);
        if instance_targets.len() > INLINE_LOOKUP_LIMIT {
            emit_runtime_instance_lookup_table(
                ctx,
                instance_targets,
                MIXED_RECEIVER_PAYLOAD_OFFSET,
                MIXED_METHOD_PAYLOAD_OFFSET,
                MIXED_METHOD_PAYLOAD_OFFSET + 8,
                miss_label,
            )?;
            let instance_table_miss = ctx.next_label("callable_array_instance_table_miss");
            emit_branch_if_lookup_missed(ctx, &instance_table_miss);
            emit_selected_template_with_saved_receiver_capture(
                ctx,
                MIXED_RECEIVER_PAYLOAD_OFFSET,
            );
            emit_runtime_descriptor_owned_flag(ctx, true);
            abi::emit_jump(ctx.emitter, &done_label);
            ctx.emitter.label(&instance_table_miss);
            emit_object_selector_static_arms(ctx, static_cases, &done_label)?;
            emit_object_selector_static_arms(ctx, &scope_static_cases, &done_label)?;
            emit_magic_call_forwarder(ctx, &done_label)?;
            emit_inaccessible_method_error(ctx, inaccessible)?;
            abi::emit_jump(ctx.emitter, miss_label);
        } else {
            for target in instance_targets {
                let next_label = ctx.next_label("callable_array_instance_next");
                emit_branch_if_runtime_array_instance_mismatch(ctx, target, &next_label);
                emit_runtime_array_instance_descriptor_value(ctx, target)?;
                emit_runtime_descriptor_owned_flag(ctx, true);
                abi::emit_jump(ctx.emitter, &done_label);
                ctx.emitter.label(&next_label);
            }
            emit_object_selector_static_arms(ctx, static_cases, &done_label)?;
            emit_object_selector_static_arms(ctx, &scope_static_cases, &done_label)?;
            emit_magic_call_forwarder(ctx, &done_label)?;
            emit_inaccessible_method_error(ctx, inaccessible)?;
            abi::emit_jump(ctx.emitter, miss_label);
        }
    }

    if let Some(label) = &static_label {
        ctx.emitter.label(label);
        if static_cases.len() > INLINE_LOOKUP_LIMIT {
            emit_runtime_static_lookup_table(
                ctx,
                static_cases,
                MIXED_RECEIVER_PAYLOAD_OFFSET,
                MIXED_RECEIVER_PAYLOAD_OFFSET + 8,
                MIXED_METHOD_PAYLOAD_OFFSET,
                MIXED_METHOD_PAYLOAD_OFFSET + 8,
            )?;
            emit_branch_if_lookup_missed(ctx, miss_label);
            emit_runtime_descriptor_owned_flag(ctx, false);
            abi::emit_jump(ctx.emitter, &done_label);
        } else {
            for case in static_cases {
                let next_label = ctx.next_label("callable_array_static_next");
                emit_branch_if_mixed_static_case_mismatch(ctx, case, &next_label);
                abi::emit_symbol_address(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    &case.case.descriptor_label,
                );
                emit_runtime_descriptor_owned_flag(ctx, false);
                abi::emit_jump(ctx.emitter, &done_label);
                ctx.emitter.label(&next_label);
            }
            abi::emit_jump(ctx.emitter, miss_label);
        }
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Resolves a saved invokable receiver to a fresh receiver-bound descriptor.
fn emit_invokable_object_descriptor_lookup(
    ctx: &mut FunctionContext<'_>,
    targets: &[RuntimeArrayInstanceMethodTarget],
    receiver_payload_offset: usize,
    miss_label: &str,
) -> Result<()> {
    if targets.len() > INLINE_LOOKUP_LIMIT {
        emit_runtime_invokable_lookup_table(ctx, targets, receiver_payload_offset, miss_label)?;
        emit_branch_if_lookup_missed(ctx, miss_label);
        emit_selected_template_with_saved_receiver_capture(ctx, receiver_payload_offset);
        emit_runtime_descriptor_owned_flag(ctx, true);
        return Ok(());
    }
    let done_label = ctx.next_label("invokable_object_lookup_done");
    for target in targets {
        let next_label = ctx.next_label("mixed_callable_object_next");
        emit_branch_if_saved_receiver_class_id_mismatch(
            ctx,
            target.class_id,
            receiver_payload_offset,
            &next_label,
        );
        let receiver_ty = PhpType::Object(target.class_name.to_string());
        let template = runtime_instance_method_descriptor_template(
            ctx,
            &target.class_name,
            &target.method_name,
            &target.method_key,
            &target.impl_class,
            &target.sig,
        )?;
        emit_runtime_descriptor_with_saved_receiver_capture(
            ctx,
            &template.descriptor_label,
            &receiver_ty,
            receiver_payload_offset,
        );
        emit_runtime_descriptor_owned_flag(ctx, true);
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches unless saved mixed slots match a public static-method callable case.
fn emit_branch_if_mixed_static_case_mismatch(
    ctx: &mut FunctionContext<'_>,
    case: &callable_dispatch::RuntimeStaticMethodCallableCase,
    next_label: &str,
) {
    emit_branch_if_stack_tag_mismatch(ctx, MIXED_RECEIVER_TAG_OFFSET, MIXED_TAG_STRING, next_label);
    emit_branch_if_stack_tag_mismatch(ctx, MIXED_METHOD_TAG_OFFSET, MIXED_TAG_STRING, next_label);
    emit_branch_if_static_class_string_mismatch(
        ctx,
        MIXED_RECEIVER_PAYLOAD_OFFSET,
        MIXED_RECEIVER_PAYLOAD_OFFSET + 8,
        &case.class_name,
        next_label,
    );
    emit_branch_if_stack_string_mismatch(
        ctx,
        MIXED_METHOD_PAYLOAD_OFFSET,
        MIXED_METHOD_PAYLOAD_OFFSET + 8,
        case.method_name.as_bytes(),
        next_label,
    );
}

/// Branches unless saved string slots match a public static-method callable case.
fn emit_branch_if_string_static_case_mismatch(
    ctx: &mut FunctionContext<'_>,
    case: &callable_dispatch::RuntimeStaticMethodCallableCase,
    next_label: &str,
) {
    emit_branch_if_static_class_string_mismatch(
        ctx,
        STRING_CLASS_OFFSET,
        STRING_CLASS_OFFSET + 8,
        &case.class_name,
        next_label,
    );
    emit_branch_if_stack_string_mismatch(
        ctx,
        STRING_METHOD_OFFSET,
        STRING_METHOD_OFFSET + 8,
        case.method_name.as_bytes(),
        next_label,
    );
}

/// Branches when a saved Mixed tag stack slot does not equal `expected_tag`.
fn emit_branch_if_stack_tag_mismatch(
    ctx: &mut FunctionContext<'_>,
    tag_offset: usize,
    expected_tag: i64,
    next_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", tag_offset);
            ctx.emitter
                .instruction(&format!("cmp x9, #{}", expected_tag)); // compare the callable-array selector runtime tag
            ctx.emitter.instruction(&format!("b.ne {}", next_label));           // try the next callable-array target when the tag differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", tag_offset);
            ctx.emitter
                .instruction(&format!("cmp r10, {}", expected_tag)); // compare the callable-array selector runtime tag
            ctx.emitter.instruction(&format!("jne {}", next_label));            // try the next callable-array target when the tag differs
        }
    }
}

/// Branches when the saved receiver object's class id does not match the target.
fn emit_branch_if_saved_receiver_class_id_mismatch(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    receiver_payload_offset: usize,
    next_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", receiver_payload_offset);
            ctx.emitter.instruction(&format!("cbz x9, {}", next_label));        // reject null callable-array receivers before reading class id
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the callable-array receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare the receiver class id against this target
            ctx.emitter.instruction(&format!("b.ne {}", next_label));           // try the next callable-array target when the class differs
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", receiver_payload_offset);
            ctx.emitter.instruction("test r10, r10");                           // reject null callable-array receivers before reading class id
            ctx.emitter.instruction(&format!("je {}", next_label));             // try the next callable-array target when the receiver is null
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // load the callable-array receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "r10", class_id as i64);
            ctx.emitter.instruction("cmp r11, r10");                            // compare the receiver class id against this target
            ctx.emitter.instruction(&format!("jne {}", next_label));            // try the next callable-array target when the class differs
        }
    }
}

/// Branches when a saved stack string does not match the expected PHP name.
fn emit_branch_if_stack_string_mismatch(
    ctx: &mut FunctionContext<'_>,
    ptr_offset: usize,
    len_offset: usize,
    expected: &[u8],
    next_label: &str,
) {
    let matched_label = ctx.next_label("callable_array_string_match");
    emit_stack_string_compare_branch(ctx, ptr_offset, len_offset, expected, &matched_label);
    abi::emit_jump(ctx.emitter, next_label);
    ctx.emitter.label(&matched_label);
}

/// Branches when a saved class string does not match bare or leading-slash forms.
fn emit_branch_if_static_class_string_mismatch(
    ctx: &mut FunctionContext<'_>,
    ptr_offset: usize,
    len_offset: usize,
    class_name: &str,
    next_label: &str,
) {
    let matched_label = ctx.next_label("callable_array_class_match");
    emit_stack_string_compare_branch(
        ctx,
        ptr_offset,
        len_offset,
        class_name.as_bytes(),
        &matched_label,
    );
    let leading_slash = format!("\\{}", class_name);
    emit_stack_string_compare_branch(
        ctx,
        ptr_offset,
        len_offset,
        leading_slash.as_bytes(),
        &matched_label,
    );
    abi::emit_jump(ctx.emitter, next_label);
    ctx.emitter.label(&matched_label);
}

/// Compares a saved stack string with `expected` and branches on equality.
fn emit_stack_string_compare_branch(
    ctx: &mut FunctionContext<'_>,
    ptr_offset: usize,
    len_offset: usize,
    expected: &[u8],
    matched_label: &str,
) {
    let (expected_label, expected_len) = ctx.data.add_string(expected);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", ptr_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", len_offset);
            abi::emit_symbol_address(ctx.emitter, "x3", &expected_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", expected_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the runtime method string matched
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // select this callable-array target when names match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", len_offset);
            abi::emit_symbol_address(ctx.emitter, "rdx", &expected_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", expected_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // check whether the runtime method string matched
            ctx.emitter.instruction(&format!("je {}", matched_label));          // select this callable-array target when names match
        }
    }
}

/// Checks literal parameters and calls the selected runtime array instance method.
/// Guard operands before ABI conversion can erase their original Mixed tags.
fn emit_runtime_array_instance_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args: &[ValueId],
    target: &RuntimeArrayInstanceMethodTarget,
) -> Result<()> {
    super::direct_calls::emit_literal_declared_parameter_guards(
        ctx, args, &target.sig.params,
        &format!("{}::{}", target.impl_class, target.method_key),
    )?;
    let receiver_ty = PhpType::Object(target.class_name.to_string());
    let mut param_types = Vec::with_capacity(target.sig.params.len() + 1);
    param_types.push(receiver_ty.clone());
    param_types.extend(target.sig.params.iter().map(|(_, ty)| ty.codegen_repr()));
    let mut ref_params = Vec::with_capacity(target.sig.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend(target.sig.ref_params.iter().copied());
    let mut operands = Vec::with_capacity(args.len() + 1);
    operands.push(expect_operand(inst, 0)?);
    operands.extend(args.iter().copied());
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, receiver_reg, MIXED_RECEIVER_PAYLOAD_OFFSET);
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        receiver_reg,
        &receiver_ty,
        &operands,
        &param_types,
        &ref_params,
        crate::codegen::lower_inst::RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    let resolved = super::MethodCallTarget {
        impl_class: target.impl_class.to_string(),
        method_key: target.method_key.to_string(),
        dynamic_slot: None,
        params: target
            .sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect(),
        ref_params: target.sig.ref_params.clone(),
        return_ty: target.sig.return_type.clone(),
        by_ref_return: target.sig.by_ref_return,
        source_abi: true,
    };
    emit_direct_resolved_method_call(ctx, &resolved)?;
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.sig.return_type)?;
    emit_call_arg_temp_cleanups(ctx, &call_args, inst.result)?;
    emit_ref_arg_writebacks(ctx, &call_args)
}

/// Builds a receiver-captured descriptor for a matched runtime instance method.
fn emit_runtime_array_instance_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    target: &RuntimeArrayInstanceMethodTarget,
) -> Result<()> {
    let receiver_ty = PhpType::Object(target.class_name.to_string());
    let template = runtime_instance_method_descriptor_template(
        ctx,
        &target.class_name,
        &target.method_name,
        &target.method_key,
        &target.impl_class,
        &target.sig,
    )?;
    emit_runtime_descriptor_with_saved_receiver_capture(
        ctx,
        &template.descriptor_label,
        &receiver_ty,
        MIXED_RECEIVER_PAYLOAD_OFFSET,
    );
    Ok(())
}

/// Invokes a matched static-method descriptor through the prebuilt argument container.
fn emit_static_descriptor_case_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    arg_mixed: ValueId,
    descriptor_label: &str,
) -> Result<()> {
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, descriptor_reg, descriptor_label);
    emit_descriptor_reg_invoker_call_with_mixed_arg(
        ctx,
        inst,
        descriptor_reg,
        arg_mixed,
        "callable_descriptor_invoke",
        false,
    )
}

/// Allocates a runtime descriptor and captures the saved receiver selector slot.
fn emit_runtime_descriptor_with_saved_receiver_capture(
    ctx: &mut FunctionContext<'_>,
    descriptor_label: &str,
    receiver_ty: &PhpType,
    receiver_payload_offset: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, receiver_payload_offset);
    abi::emit_incref_if_refcounted(ctx.emitter, receiver_ty);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // keep the receiver-bound descriptor while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        ctx.emitter,
        descriptor_reg,
        descriptor_label,
    );
    let kind_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        kind_reg,
        callable_descriptor::CALLABLE_DESC_KIND_ARRAY as i64,
    );
    abi::emit_store_to_address(ctx.emitter, kind_reg, descriptor_reg, 0);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the receiver-bound callable-array descriptor
    }
}

/// Captures a saved object receiver into a runtime-selected descriptor template.
fn emit_selected_template_with_saved_receiver_capture(
    ctx: &mut FunctionContext<'_>,
    receiver_payload_offset: usize,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let template_reg = abi::symbol_scratch_reg(ctx.emitter);
    let receiver_ty = PhpType::Object(String::new());
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        result_reg,
        receiver_payload_offset + 16,
    );
    abi::emit_incref_if_refcounted(ctx.emitter, &receiver_ty);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, total_bytes as i64);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter
        .instruction(&format!("mov {}, {}", descriptor_reg, result_reg)); // preserve the receiver-bound descriptor allocation
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_pop_reg(ctx.emitter, template_reg);
    callable_descriptor::emit_copy_descriptor_template_to_runtime(
        ctx.emitter,
        descriptor_reg,
        template_reg,
    );
    let kind_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        kind_reg,
        callable_descriptor::CALLABLE_DESC_KIND_ARRAY as i64,
    );
    abi::emit_store_to_address(ctx.emitter, kind_reg, descriptor_reg, 0);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        ctx.emitter,
        descriptor_reg,
        0,
        &receiver_ty,
    );
    if descriptor_reg != result_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", result_reg, descriptor_reg)); // return the receiver-bound descriptor selected by the table
    }
}

/// Emits the fatal path for runtime callable arrays without a matching method.
fn emit_runtime_callable_array_no_match_abort(ctx: &mut FunctionContext<'_>) {
    let message = if std::env::var_os("ELEPHC_EVAL_TRACE").is_some() {
        format!(
            "Fatal error: callable array did not resolve to an invokable target in {}\n",
            ctx.function.name
        )
    } else {
        "Fatal error: callable array did not resolve to an invokable target\n".to_owned()
    };
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the callable-array failure diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the callable-array diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the callable-array failure diagnostic to stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the callable-array diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the fatal diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Lowers a callable descriptor call through its uniform invoker slot.
fn lower_descriptor_invoker_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let visible_args = inst.operands.iter().skip(1).copied().collect::<Vec<_>>();
    lower_descriptor_invoker_call_with_args(ctx, inst, callable, &visible_args, op_name)
}

/// Lowers a descriptor call with a prebuilt Mixed argument container.
fn lower_descriptor_invoker_call_with_mixed_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(callable, descriptor_reg)?;
    emit_descriptor_reg_invoker_call_with_mixed_arg(
        ctx,
        inst,
        descriptor_reg,
        arg_mixed,
        op_name,
        false,
    )
}

/// Lowers a callable descriptor call with an explicitly provided visible argument list.
fn lower_descriptor_invoker_call_with_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    visible_args: &[ValueId],
    op_name: &str,
) -> Result<()> {
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    ctx.load_value_to_reg(callable, descriptor_reg)?;
    // A callee read out of a boxed (promoted) local is unboxed with a reference of its own and
    // nothing in EIR releases it: `$f = $o->m(...); echo $f(); $f = null;` leaked the
    // descriptor and so its receiver. The call boundary drops that owner after the call.
    let release_owned_callee = ctx.value_can_own_mixed_box_source(callable)?;
    emit_descriptor_reg_invoker_mixed_result_with_args(
        ctx,
        descriptor_reg,
        visible_args,
        op_name,
        release_owned_callee,
    )?;
    store_descriptor_invoker_result(ctx, inst)
}

/// Calls a loaded descriptor through its uniform invoker using visible EIR arguments.
pub(super) fn emit_descriptor_reg_invoker_call_with_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    descriptor_reg: &str,
    visible_args: &[ValueId],
    op_name: &str,
) -> Result<()> {
    emit_descriptor_reg_invoker_mixed_result_with_args(
        ctx,
        descriptor_reg,
        visible_args,
        op_name,
        false,
    )?;
    store_descriptor_invoker_result(ctx, inst)
}

/// Calls a loaded descriptor invoker and leaves its boxed Mixed result in the result register.
pub(super) fn emit_descriptor_reg_invoker_mixed_result_with_args(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    visible_args: &[ValueId],
    op_name: &str,
    release_runtime_descriptor: bool,
) -> Result<()> {
    let invoker_reg = abi::symbol_scratch_reg(ctx.emitter);
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    let ready_label = descriptor_invoker_ready_label(ctx, op_name);
    emit_branch_if_invoker_present(ctx, invoker_reg, &ready_label);
    emit_missing_descriptor_invoker_fatal(ctx, op_name);

    ctx.emitter.label(&ready_label);
    abi::emit_push_reg(ctx.emitter, descriptor_reg);
    emit_invoker_arg_mixed(ctx, visible_args)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed Mixed argument array across descriptor register setup
    abi::emit_load_temporary_stack_slot(ctx.emitter, descriptor_reg, 16);
    move_reg_to_arg(ctx, descriptor_reg, 0);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, 0);
    emit_owned_descriptor_args_boundary(ctx, release_runtime_descriptor);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Calls a descriptor pointer through its uniform invoker using a stored Mixed arg container.
fn emit_descriptor_reg_invoker_call_with_mixed_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    descriptor_reg: &str,
    arg_mixed: ValueId,
    op_name: &str,
    release_runtime_descriptor: bool,
) -> Result<()> {
    emit_descriptor_reg_invoker_mixed_result_with_arg_container(
        ctx,
        descriptor_reg,
        arg_mixed,
        op_name,
        release_runtime_descriptor,
        true,
    )?;
    store_descriptor_invoker_result(ctx, inst)
}

/// Invokes a resolved descriptor and conditionally releases its runtime-owned allocation.
fn emit_descriptor_reg_invoker_call_with_mixed_arg_and_runtime_ownership(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    descriptor_reg: &str,
    owned_flag_reg: &str,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    retain_borrowed_descriptor_for_invocation(ctx, descriptor_reg, owned_flag_reg);
    // The invocation now always owns one descriptor reference. The owning boundary releases it
    // on the exceptional exit as well: releasing it only after a normal return leaked the
    // descriptor -- and every capture -- whenever the callback threw (`function f(mixed $f) {
    // $f(); }` with a throwing closure kept its `use ($object)` alive).
    emit_descriptor_reg_invoker_mixed_result_with_arg_container(
        ctx,
        descriptor_reg,
        arg_mixed,
        op_name,
        true,
        true,
    )?;
    store_descriptor_invoker_result(ctx, inst)
}

/// Acquires the temporary owner that keeps a borrowed descriptor alive for its invocation.
///
/// A descriptor read from a boxed `Mixed` value is normally borrowed from that value. The PHP
/// callback may nevertheless replace the last owner of that value through a by-reference capture
/// while its invoker is still executing. Retain the borrowed descriptor before entering the
/// invoker and represent that retained reference through the existing ownership flag, so the
/// normal post-invocation release balances it. Fresh descriptors already have such an owner.
fn retain_borrowed_descriptor_for_invocation(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    owned_flag_reg: &str,
) {
    let already_owned_label = ctx.next_label("descriptor_invocation_owner_present");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx
            .emitter
            .instruction(&format!("cbnz {}, {}", owned_flag_reg, already_owned_label)),
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", owned_flag_reg, owned_flag_reg));
            ctx.emitter
                .instruction(&format!("jnz {}", already_owned_label));
        }
    }

    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reg_move(ctx.emitter, &result_reg, descriptor_reg);
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, descriptor_reg, &result_reg);
    abi::emit_load_int_immediate(ctx.emitter, owned_flag_reg, 1);
    ctx.emitter.label(&already_owned_label);
}

/// Calls a loaded descriptor invoker with an argument container and leaves a Mixed result.
pub(super) fn emit_descriptor_reg_invoker_mixed_result_with_arg_container(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    arg_mixed: ValueId,
    op_name: &str,
    release_runtime_descriptor: bool,
    argument_owned_by_eir: bool,
) -> Result<()> {
    // An EIR owner guards and releases its source around the enclosing call, so the backend only
    // owns its normalized copy. Legacy runtime-only consumers can still transfer a prebuilt box.
    if !argument_owned_by_eir && descriptor_arg_is_prebuilt_mixed_box(ctx, arg_mixed)? {
        return emit_descriptor_reg_invoker_mixed_result_with_prebuilt_mixed_arg(
            ctx,
            descriptor_reg,
            arg_mixed,
            op_name,
            release_runtime_descriptor,
        );
    }

    emit_descriptor_reg_invoker_mixed_result_with_normalized_arg(
        ctx,
        descriptor_reg,
        arg_mixed,
        op_name,
        release_runtime_descriptor,
    )
}

/// Calls a descriptor invoker with a separate retain of the caller-owned prebuilt Mixed box.
fn emit_descriptor_reg_invoker_mixed_result_with_prebuilt_mixed_arg(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    arg_mixed: ValueId,
    op_name: &str,
    release_runtime_descriptor: bool,
) -> Result<()> {
    let invoker_reg = abi::symbol_scratch_reg(ctx.emitter);
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    let ready_label = descriptor_invoker_ready_label(ctx, op_name);
    emit_branch_if_invoker_present(ctx, invoker_reg, &ready_label);
    emit_missing_descriptor_invoker_fatal(ctx, op_name);

    ctx.emitter.label(&ready_label);
    abi::emit_push_reg(ctx.emitter, descriptor_reg);
    ctx.load_value_to_result(arg_mixed)?;
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_load_temporary_stack_slot(ctx.emitter, descriptor_reg, 16);
    move_reg_to_arg(ctx, descriptor_reg, 0);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, 0);
    emit_owned_descriptor_args_boundary(ctx, release_runtime_descriptor);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Calls a descriptor invoker after cloning and boxing a raw argument-array container.
fn emit_descriptor_reg_invoker_mixed_result_with_normalized_arg(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    arg_container: ValueId,
    op_name: &str,
    release_runtime_descriptor: bool,
) -> Result<()> {
    let invoker_reg = abi::symbol_scratch_reg(ctx.emitter);
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    let ready_label = descriptor_invoker_ready_label(ctx, op_name);
    emit_branch_if_invoker_present(ctx, invoker_reg, &ready_label);
    emit_missing_descriptor_invoker_fatal(ctx, op_name);

    ctx.emitter.label(&ready_label);
    abi::emit_push_reg(ctx.emitter, descriptor_reg); // preserve the callable descriptor while normalizing call_user_func_array() args
    emit_normalized_invoker_arg_container(ctx, arg_container, release_runtime_descriptor)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed normalized argument container for invocation and cleanup
    abi::emit_load_temporary_stack_slot(ctx.emitter, descriptor_reg, 16);
    move_reg_to_arg(ctx, descriptor_reg, 0);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, 0);
    emit_owned_descriptor_args_boundary(ctx, release_runtime_descriptor);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Transfers normalized arguments and any temporary descriptor to the bounded invocation helper.
fn emit_owned_descriptor_args_boundary(ctx: &mut FunctionContext<'_>, owns_descriptor: bool) {
    let entry = if owns_descriptor {
        "__rt_callable_invoke_owned_descriptor_args"
    } else {
        "__rt_callable_invoke_owned_args"
    };
    // The descriptor itself may have escaped from another scope. Transport the scope of this
    // invocation as hidden ABI state instead of capturing descriptor-creation context.
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        ctx.callable_invocation_profile(),
    );
    abi::emit_call_label(ctx.emitter, entry);
}

/// Returns the branch-ready label name for a descriptor invoker callsite.
fn descriptor_invoker_ready_label(ctx: &mut FunctionContext<'_>, op_name: &str) -> String {
    if matches!(op_name, "callable_descriptor_invoke" | "iterator_apply") {
        return ctx.next_label("cufa_descriptor_invoker_ready");
    }
    ctx.next_label(&format!("{}_descriptor_invoker_ready", op_name))
}

/// Recognizes a prebuilt Mixed box through identity-preserving lifetime retains.
fn descriptor_arg_is_prebuilt_mixed_box(
    ctx: &FunctionContext<'_>,
    arg_mixed: ValueId,
) -> Result<bool> {
    if ctx.value_php_type(arg_mixed)?.codegen_repr() != PhpType::Mixed {
        return Ok(false);
    }
    let arg_mixed = transparent_descriptor_arg_source(ctx.function, arg_mixed)?;
    let Some(value_ref) = ctx.function.value(arg_mixed) else {
        return Err(CodegenIrError::missing_entry("value", arg_mixed.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(inst.op == Op::MixedBox)
}

/// Follows identity-preserving ownership operations used to stage descriptor arguments.
fn transparent_descriptor_arg_source(
    function: &crate::ir::Function,
    mut arg_mixed: ValueId,
) -> Result<ValueId> {
    loop {
        let Some(value_ref) = function.value(arg_mixed) else {
            return Err(CodegenIrError::missing_entry("value", arg_mixed.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(arg_mixed);
        };
        let Some(inst) = function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        if !matches!(inst.op, Op::Acquire | Op::Move | Op::Borrow) {
            return Ok(arg_mixed);
        }
        arg_mixed = expect_operand(inst, 0)?;
    }
}

#[cfg(test)]
mod descriptor_producer_tests {
    use super::transparent_descriptor_arg_source;
    use crate::ir::{Builder, Function, IrType, Op, Ownership};
    use crate::types::PhpType;

    /// Descriptor argument classification reaches a prebuilt Mixed box through the
    /// ownership chain while invocation remains free to load the final staged operand.
    #[test]
    fn wrapped_prebuilt_mixed_box_keeps_its_box_producer() {
        let mut function =
            Function::new("wrapped_descriptor_arg".into(), IrType::Void, PhpType::Void);
        let (mixed_box, wrapped) = {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let scalar = builder.emit_const_i64(1);
            let mixed_box = builder
                .emit(
                    Op::MixedBox,
                    vec![scalar],
                    None,
                    IrType::from_php(&PhpType::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let acquired = builder
                .emit(
                    Op::Acquire,
                    vec![mixed_box],
                    None,
                    IrType::from_php(&PhpType::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap();
            let borrowed = builder
                .emit(
                    Op::Borrow,
                    vec![acquired],
                    None,
                    IrType::from_php(&PhpType::Mixed),
                    PhpType::Mixed,
                    Ownership::Borrowed,
                )
                .unwrap();
            let moved = builder
                .emit(
                    Op::Move,
                    vec![borrowed],
                    None,
                    IrType::from_php(&PhpType::Mixed),
                    PhpType::Mixed,
                    Ownership::Borrowed,
                )
                .unwrap();
            (mixed_box, moved)
        };

        assert_eq!(
            transparent_descriptor_arg_source(&function, wrapped).unwrap(),
            mixed_box
        );
    }
}

/// Emits a normalized boxed Mixed argument container for descriptor invokers.
fn emit_normalized_invoker_arg_container(
    ctx: &mut FunctionContext<'_>,
    arg_container: ValueId,
    emit_receiver_mixed_markers: bool,
) -> Result<()> {
    let container_ty = ctx.value_php_type(arg_container)?.codegen_repr();
    let dest_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    match container_ty {
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => {
            ctx.load_value_to_reg(arg_container, dest_reg)?;
            let mut labels = [
                ctx.next_label("invoker_arg_kind_hash"),
                ctx.next_label("invoker_arg_kind_done"),
            ]
            .into_iter();
            callable_invoker_args::emit_clone_mixed_array_or_hash_for_invoker(
                dest_reg,
                ctx.emitter,
                &mut |_| {
                    labels
                        .next()
                        .expect("codegen bug: missing preallocated invoker-kind label")
                },
            );
            Ok(())
        }
        PhpType::Array(elem_ty) => {
            ctx.load_value_to_reg(arg_container, dest_reg)?;
            callable_invoker_args::emit_clone_indexed_array_for_invoker(
                dest_reg,
                &elem_ty.codegen_repr(),
                ctx.emitter,
            );
            let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
            callable_invoker_args::emit_box_invoker_arg_clone_as_mixed(
                dest_reg,
                &mixed_array_ty,
                ctx.emitter,
            );
            Ok(())
        }
        PhpType::AssocArray { value, .. } => {
            ctx.load_value_to_reg(arg_container, dest_reg)?;
            callable_invoker_args::emit_clone_assoc_array_for_invoker_with_value_type(
                dest_reg,
                &value.codegen_repr(),
                ctx.emitter,
            );
            let mixed_hash_ty = PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            };
            callable_invoker_args::emit_box_invoker_arg_clone_as_mixed(
                dest_reg,
                &mixed_hash_ty,
                ctx.emitter,
            );
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            if emit_receiver_mixed_markers {
                ctx.emitter.comment("receiver_mixed_indexed_args");
                ctx.emitter.comment("receiver_mixed_assoc_args");
            }
            ctx.load_value_to_reg(arg_container, dest_reg)?;
            let mut labels = [
                ctx.next_label("cufa_normalize_mixed_indexed"),
                ctx.next_label("cufa_normalize_mixed_assoc"),
                ctx.next_label("cufa_normalize_mixed_done"),
            ]
            .into_iter();
            callable_invoker_args::emit_clone_runtime_mixed_invoker_arg_as_mixed(
                dest_reg,
                ctx.emitter,
                &mut |_| {
                    labels
                        .next()
                        .expect("codegen bug: missing preallocated invoker-normalization label")
                },
                ctx.data,
            );
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "callable_descriptor_invoke argument container PHP type {:?}",
            other
        ))),
    }?;
    move_normalized_invoker_arg_to_result(ctx, dest_reg);
    Ok(())
}

/// Moves the normalized Mixed argument container into the ABI result register.
fn move_normalized_invoker_arg_to_result(ctx: &mut FunctionContext<'_>, source_reg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if source_reg == result_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // place the normalized invoker argument where the caller will preserve it
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", result_reg, source_reg)); // place the normalized invoker argument where the caller will preserve it
        }
    }
}

/// Branches to `ready_label` when a callable descriptor has a uniform invoker.
fn emit_branch_if_invoker_present(
    ctx: &mut FunctionContext<'_>,
    invoker_reg: &str,
    ready_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbnz {}, {}", invoker_reg, ready_label));
            // continue when the callable descriptor has a uniform invoker
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", invoker_reg, invoker_reg)); // check whether the callable descriptor has a uniform invoker
            ctx.emitter.instruction(&format!("jnz {}", ready_label));           // continue when the callable descriptor has a uniform invoker
        }
    }
}

/// Emits a fatal diagnostic for callable descriptors without a uniform invoker.
fn emit_missing_descriptor_invoker_fatal(ctx: &mut FunctionContext<'_>, op_name: &str) {
    let message = format!(
        "Fatal error: Unsupported EIR {} callable descriptor without invoker\n",
        op_name
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the missing descriptor-invoker diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label); // load the missing descriptor-invoker diagnostic page
            ctx.emitter.add_lo12("x1", "x1", &message_label); // resolve the missing descriptor-invoker diagnostic address
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the descriptor-invoker diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the missing descriptor-invoker diagnostic to stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the descriptor-invoker diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the missing descriptor-invoker diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Creates an indexed argument array and boxes it as the descriptor-invoker container.
pub(super) fn emit_invoker_arg_mixed(ctx: &mut FunctionContext<'_>, args: &[ValueId]) -> Result<()> {
    emit_invoker_arg_array(ctx, args)?;
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    Ok(())
}

/// Builds `trigger_error`'s handler arguments, replacing the baked file and line with the
/// caller location when the call was compiled from injected prelude code.
///
/// php-src attributes an internal function's user-level notice to the user's call site, not to
/// the internal implementation. Injected prelude functions are the compiler's internal
/// functions, so their `trigger_error` must report the location `prepare_call_site` published
/// for the entering call instead of the prelude's synthetic span.
pub(super) fn emit_trigger_error_handler_args(
    ctx: &mut FunctionContext<'_>,
    level: ValueId,
    message: ValueId,
    file: ValueId,
    line: ValueId,
    caller_site: bool,
) -> Result<()> {
    if !caller_site {
        return emit_invoker_arg_mixed(ctx, &[level, message, file, line]);
    }
    emit_invoker_arg_array_with(ctx, &[level, message], true)?;
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    Ok(())
}

/// Creates the indexed array consumed by runtime callable descriptor invokers.
fn emit_invoker_arg_array(ctx: &mut FunctionContext<'_>, args: &[ValueId]) -> Result<()> {
    emit_invoker_arg_array_with(ctx, args, false)
}

/// Builds the invoker argument array, optionally appending the published caller file and line.
fn emit_invoker_arg_array_with(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    caller_site: bool,
) -> Result<()> {
    let extra = if caller_site { 2 } else { 0 };
    emit_new_invoker_arg_array(ctx, args.len() + extra);
    if args.is_empty() && !caller_site {
        return Ok(());
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the in-progress invoker argument array across element boxing
    for arg in args {
        emit_box_invoker_arg(ctx, *arg)?;
        emit_append_boxed_result_as_invoker_arg(ctx);
    }
    if caller_site {
        emit_append_published_invoker_arg(ctx, PublishedInvokerArg::File);
        emit_append_published_invoker_arg(ctx, PublishedInvokerArg::Line);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// One component of the caller location `prepare_call_site` published before the call.
#[derive(Clone, Copy)]
enum PublishedInvokerArg {
    /// `_php_diagnostic_file` plus its length.
    File,
    /// `_php_diagnostic_line`.
    Line,
}

/// Loads one published caller-location component, boxes it, and appends it to the argument array.
fn emit_append_published_invoker_arg(ctx: &mut FunctionContext<'_>, component: PublishedInvokerArg) {
    match component {
        PublishedInvokerArg::File => {
            let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_symbol_to_reg(ctx.emitter, pointer_reg, "_php_diagnostic_file", 0);
            abi::emit_load_symbol_to_reg(ctx.emitter, length_reg, "_php_diagnostic_file_len", 0);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        }
        PublishedInvokerArg::Line => {
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                "_php_diagnostic_line",
                0,
            );
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
    }
    emit_append_boxed_result_as_invoker_arg(ctx);
}

/// Appends the boxed cell currently in the result register into the invoker argument array.
fn emit_append_boxed_result_as_invoker_arg(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed argument while loading the invoker array
    emit_append_boxed_invoker_arg(ctx);
    emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
    emit_store_result_to_top_stack_slot(ctx);
}

/// Allocates the raw indexed array used to pass visible arguments to descriptor invokers.
fn emit_new_invoker_arg_array(ctx: &mut FunctionContext<'_>, arg_count: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", arg_count as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", arg_count as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
}

/// Boxes or retains a visible descriptor-invoker argument as an owned Mixed cell.
fn emit_box_invoker_arg(ctx: &mut FunctionContext<'_>, arg: ValueId) -> Result<()> {
    let arg_ty = ctx.value_php_type(arg)?.codegen_repr();
    // The declared type, so a `?ptr<T>` argument boxes address 0 as PHP null.
    let box_ty = ctx.mixed_box_type(arg)?;
    ctx.load_value_to_result(arg)?;
    if matches!(arg_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &arg_ty);
    } else if ctx.value_can_own_mixed_box_source(arg)? {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &box_ty);
    } else {
        let staged = if arg_ty == PhpType::Str {
            let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
            super::ownership::stage_string_byte_view(ctx, arg, pointer_reg, length_reg)
        } else {
            false
        };
        emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        super::ownership::release_string_byte_stage(ctx, staged);
    }
    Ok(())
}

/// Appends the boxed top-of-stack Mixed cell into the saved invoker argument array.
fn emit_append_boxed_invoker_arg(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("mov x1, x0");                              // pass the boxed visible argument to the invoker array append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the saved invoker argument array to the append helper
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            ctx.emitter.instruction("mov rsi, rax");                            // pass the boxed visible argument to the invoker array append helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
}

/// Stores the current single-register result into the temporary stack slot at `sp`.
fn emit_store_result_to_top_stack_slot(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp]");                            // update the saved invoker argument array after append growth
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // update the saved invoker argument array after append growth
        }
    }
}

/// Moves a general-purpose register into an ABI argument register.
fn move_reg_to_arg(ctx: &mut FunctionContext<'_>, source_reg: &str, arg_index: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, arg_index);
    if source_reg == arg_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", arg_reg, source_reg)); // move the callable descriptor into the invoker ABI argument
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", arg_reg, source_reg)); // move the callable descriptor into the invoker ABI argument
        }
    }
}

/// Releases the temporary invoker argument while preserving the Mixed call result.
pub(super) fn release_invoker_arg_preserving_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Stores the Mixed descriptor-invoker result using the EIR result type.
fn store_descriptor_invoker_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(result) = inst.result else {
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        return Ok(());
    };
    let result_type = ctx.value_php_type(result)?.codegen_repr();
    // the invoker was generated for the callee's declared return type and already hands back
    // the object payload with its own reference (`coerce_result_to_type`), so it is stored as
    // it comes.
    if matches!(result_type, PhpType::Object(_)) {
        let result_reg = abi::int_result_reg(ctx.emitter).to_string();
        abi::emit_push_reg(ctx.emitter, &result_reg);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        let payload_reg = super::reference_arguments::mixed_unbox_low_payload_reg(ctx);
        super::reference_arguments::move_reg_to_int_result(ctx, payload_reg);
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Object(String::new()));
        let scratch = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_pop_reg(ctx.emitter, scratch);
        abi::emit_push_reg(ctx.emitter, &result_reg);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x0, {}", scratch)); // the box is released after its object was retained
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rax, {}", scratch)); // the box is released after its object was retained
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, &result_reg);
        return ctx.store_result_value(result);
    }
    let result_ownership = ctx.value_ownership(result)?;
    results::emit_unbox_owned_descriptor_result(
        ctx.emitter,
        &result_type,
        result_ownership,
    )?;
    ctx.store_result_value(result)
}

/// Lowers `value |> $callable` through the callable descriptor's uniform invoker.
pub(super) fn lower_pipe_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "pipe_call expected value and callable operands, got {}",
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    let callable = expect_operand(inst, 1)?;
    if ctx.value_php_type(callable)?.codegen_repr() != PhpType::Callable {
        return Err(CodegenIrError::unsupported(format!(
            "pipe_call for callable PHP type {:?}",
            ctx.value_php_type(callable)?.codegen_repr()
        )));
    }
    lower_descriptor_invoker_call_with_args(ctx, inst, callable, &[value], "pipe_call")
}

/// Dispatches a runtime string callable across user functions with compatible ABI shape.
fn lower_runtime_string_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let args = inst.operands.iter().skip(1).copied().collect::<Vec<_>>();
    let targets = runtime_string_function_targets(ctx, args.len(), inst)?;
    if targets.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} with no compatible user-function targets",
            op_name
        )));
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(callable, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    let done_label = ctx.next_label(&format!("{}_done", op_name));
    let miss_label = ctx.next_label(&format!("{}_missing", op_name));
    let mut case_labels = Vec::with_capacity(targets.len());
    for target in &targets {
        let label = ctx.next_label(&format!("{}_{}", op_name, label_fragment(&target.name)));
        emit_branch_if_runtime_callable_name_matches(ctx, &target.name, &label);
        case_labels.push(label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (target, label) in targets.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_runtime_string_function_call(ctx, inst, &args, target)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_undefined_runtime_string_call_fatal(ctx);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Collects compatible user functions that a runtime string callable may select.
fn runtime_string_function_targets(
    ctx: &FunctionContext<'_>,
    arg_count: usize,
    inst: &Instruction,
) -> Result<Vec<RuntimeStringFunctionTarget>> {
    let targets = ctx
        .module
        .functions
        .iter()
        .filter(|function| !function.flags.is_main)
        .filter(|function| function.params.len() == arg_count)
        .filter(|function| {
            function
                .params
                .iter()
                .all(|param| !param.by_ref && !param.variadic)
        })
        .filter_map(|function| {
            let return_ty = function.return_php_type.codegen_repr();
            if !runtime_string_result_type_supported(
                &inst.result_php_type.codegen_repr(),
                &return_ty,
            ) {
                return None;
            }
            let base_name = function.name.split('<').next().unwrap_or(&function.name);
            Some(RuntimeStringFunctionTarget {
                name: function.name.clone(),
                param_types: function
                    .params
                    .iter()
                    .map(|param| param.php_type.codegen_repr())
                    .collect(),
                return_ty,
                false_param_prefixes: function
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| {
                        (param.php_type == PhpType::False).then(|| {
                            format!(
                                "{base_name}(): Argument #{} (${}) must be of type false, ",
                                index + 1,
                                param.name
                            )
                        })
                    })
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    Ok(targets)
}

/// Returns true when the selected runtime function can be stored into the EIR result.
fn runtime_string_result_type_supported(result_ty: &PhpType, return_ty: &PhpType) -> bool {
    result_ty == return_ty || matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
}


/// Emits one branch comparing the saved callable name with a candidate function name.
fn emit_branch_if_runtime_callable_name_matches(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    matched_label: &str,
) {
    emit_runtime_callable_name_compare(ctx, name.as_bytes(), matched_label);
    let trimmed = name.trim_start_matches('\\');
    if trimmed == name {
        let qualified = format!("\\{}", name);
        emit_runtime_callable_name_compare(ctx, qualified.as_bytes(), matched_label);
    }
}

/// Emits a case-insensitive compare against the saved runtime callable name.
fn emit_runtime_callable_name_compare(
    ctx: &mut FunctionContext<'_>,
    candidate: &[u8],
    matched_label: &str,
) {
    let (candidate_label, candidate_len) = ctx.data.add_string(candidate);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // did the runtime string callable name match this user function?
            // THE ONE DISPATCH HERE THAT HOISTS EVERY COMPARE ABOVE EVERY ARM.
            // `lower_runtime_string_call` emits all name compares first and all call arms
            // after, so this branch jumps over the remaining compares AND every earlier arm —
            // a distance that grows with the number of user functions the call could name.
            // The per-case dispatches in this file interleave compare and arm, so their
            // conditional branches only ever skip one bounded arm; this one does not.
            //
            // DEFENSIVE, AND SAID SO: no ordinary PHP reaches this today. `$f()` on a string
            // lowers to `callable_descriptor_invoke` and takes the descriptor path, whose
            // normalizer carries the same fix and is pinned by a test. No fixture could be
            // built that emits this dispatch, so there is no test here — one was written and
            // its premise guard refused to pass on a dispatch that was never emitted. The form
            // costs one instruction on the taken path and removes an assembler failure the day
            // a lowering change routes string callables back here.
            abi::emit_branch_if_equal_wide(ctx.emitter, matched_label);         // dispatch to this user function when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // did the runtime string callable name match this user function?
            ctx.emitter.instruction(&format!("je {}", matched_label));          // dispatch to this user function when names match case-insensitively
        }
    }
}

/// Calls one resolved runtime string callable target and stores the converted result.
fn emit_runtime_string_function_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args: &[ValueId],
    target: &RuntimeStringFunctionTarget,
) -> Result<()> {
    // A `false`-declared parameter accepts ONLY the exact `false` bool: the runtime-callable
    // surface cannot be checked statically (the callable's identity is dynamic), so every
    // argument bound to one is guarded here. A bool source is checked by payload, a boxed
    // mixed by tag dispatch, and every other compile-time type is refused outright with php's
    // argument `TypeError` naming it.
    for (prefix, operand) in target.false_param_prefixes.iter().zip(args) {
        let Some(prefix) = prefix else { continue };
        let operand_ty = ctx.value_php_type(*operand)?;
        match operand_ty.codegen_repr() {
            PhpType::False => {}
            PhpType::Bool => {
                ctx.load_value_to_result(*operand)?;
                let ok_label = ctx.next_label("false_invoker_ok");
                abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
                super::exceptions::emit_type_error(ctx, &format!("{prefix}true given"));
                ctx.emitter.label(&ok_label);
            }
            PhpType::Mixed | PhpType::Union(_) => {
                super::emit_false_param_mixed_guard(ctx, *operand, prefix)?
            }
            other => super::exceptions::emit_type_error(
                ctx,
                &format!("{prefix}{} given", php_argument_type_name(&other)),
            ),
        }
    }
    let overflow_bytes = materialize_direct_call_args(ctx, args, &target.param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&target.name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    store_runtime_string_call_result(ctx, inst, &target.return_ty)
}

/// Stores a runtime string callable result, boxing scalar returns for Mixed slots.
fn store_runtime_string_call_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?;
    if return_ty.codegen_repr() == PhpType::Void || result_ty == PhpType::Void {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
        if matches!(result_ty, PhpType::Mixed | PhpType::Union(_)) {
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
        }
        ctx.store_result_value(result)?;
        return Ok(());
    }
    if matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
        && return_ty.codegen_repr() != PhpType::Mixed
    {
        // A `?ptr<T>` return keeps its union, so address 0 is PHP null.
        emit_box_current_value_as_mixed(ctx.emitter, &crate::types::mixed_box_php_type(return_ty));
    }
    ctx.store_result_value(result)
}

/// Emits the fatal path for an unmatched runtime string callable name.
fn emit_undefined_runtime_string_call_fatal(ctx: &mut FunctionContext<'_>) {
    // `ELEPHC_EVAL_TRACE` names the compiled function the call is in, as the callable-array
    // fatal does, so a build can say which site needs a candidate the closed world lacks.
    let message = if std::env::var_os("ELEPHC_EVAL_TRACE").is_some() {
        format!(
            "Fatal error: Call to undefined function <dynamic>() in {}\n",
            ctx.function.name
        )
    } else {
        "Fatal error: Call to undefined function <dynamic>()\n".to_owned()
    };
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the undefined dynamic-call diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label); // load the dynamic-call diagnostic string page
            ctx.emitter.add_lo12("x1", "x1", &message_label); // resolve the dynamic-call diagnostic string address
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the dynamic-call diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the undefined dynamic-call diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the dynamic-call diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the fatal diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

#[cfg(test)]
std::thread_local! {
    /// Callable-array instance candidate lists built (once per class scope) on this test thread.
    pub(crate) static INSTANCE_TARGET_BUILDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
