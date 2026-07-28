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
use crate::codegen::{
    abi, callable_descriptor, callable_dispatch, callable_invoker_args,
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_release_pushed_refcounted_temp_after_array_push,
};
use crate::ir::{Instruction, Op, ValueDef, ValueId};
use crate::names::{function_symbol, method_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::{FunctionSig, PhpType};

use super::super::context::FunctionContext;
use super::super::shared_state::RuntimeInstanceMethodDescriptorTemplate;
use super::{
    class_method_already_emitted, class_method_body_exists, direct_call_stack_pad_bytes,
    emit_instance_method_descriptor_entry_wrapper, emit_ref_arg_writebacks,
    emit_runtime_builtin_wrapper_inline, emit_runtime_callable_invoker_inline,
    emit_runtime_descriptor_with_receiver_capture, emit_runtime_extern_wrapper_inline,
    emit_static_method_descriptor_entry_wrapper, expect_operand, function_signature_from_eir,
    materialize_direct_call_args, materialize_method_call_args_with_receiver_reg_and_refs,
    runtime_builtin_wrapper_sig, store_call_result,
};
use crate::codegen::{CodegenIrError, Result};

mod instance_expr;

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

/// Resolved user function candidate for a runtime string callable.
struct RuntimeStringFunctionTarget {
    name: String,
    param_types: Vec<PhpType>,
    return_ty: PhpType,
}

/// Resolved public instance-method candidate for a runtime callable array.
#[derive(Clone)]
struct RuntimeArrayInstanceMethodTarget {
    class_name: String,
    class_id: u64,
    method_key: String,
    method_name: String,
    impl_class: String,
    sig: FunctionSig,
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
fn lower_mixed_callable_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_mixed: ValueId,
    op_name: &str,
) -> Result<()> {
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let invokable_targets = instance_targets
        .iter()
        .filter(|target| target.method_key == "__invoke")
        .cloned()
        .collect::<Vec<_>>();
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    let array_label = (!instance_targets.is_empty() || !static_cases.is_empty())
        .then(|| ctx.next_label("mixed_callable_array"));
    let object_label = (!invokable_targets.is_empty())
        .then(|| ctx.next_label("mixed_callable_object"));
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    let string_label = ctx.next_label("mixed_callable_string_name");
    let callable_label = ctx.next_label("mixed_callable_closure");
    let fatal_label = ctx.next_label("mixed_callable_not_callable");
    let done_label = ctx.next_label("mixed_callable_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(callable, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");              // unbox → x0=tag, x1=payload lo, x2=payload hi
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_STRING)); // is the boxed Mixed payload a string function name?
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // dispatch a boxed string-name callable
            ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_CALLABLE)); // is the boxed Mixed payload a callable descriptor?
            ctx.emitter.instruction(&format!("b.eq {}", callable_label));       // dispatch a boxed closure/first-class callable descriptor
            if let Some(array_label) = &array_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_INDEXED_ARRAY)); // is the boxed Mixed payload a two-element callable array?
                ctx.emitter.instruction(&format!("b.eq {}", array_label));      // dispatch a boxed instance/static-method callable array
            }
            if let Some(object_label) = &object_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_OBJECT)); // is the boxed Mixed payload an invokable object?
                ctx.emitter.instruction(&format!("b.eq {}", object_label));     // dispatch the object's public __invoke method
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
    // Closure descriptor (tag 10): invoke the unboxed descriptor directly.
    emit_descriptor_reg_invoker_call_with_mixed_arg(
        ctx,
        inst,
        descriptor_reg,
        arg_mixed,
        op_name,
        false,
    )?;
    abi::emit_jump(ctx.emitter, &done_label);

    // String function name (tag 1): the unboxed pointer/length are still in the
    // payload registers. Move them into the string ABI result pair, spill them onto
    // the temporary stack, and reuse the concrete-string per-candidate dispatch.
    ctx.emitter.label(&string_label);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction(&format!("mov {}, rdi", ptr_reg));              // move the unboxed string pointer into the string ABI result register
    }
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);                    // spill the unboxed function name across dispatch emission
    let candidate_names = ctx.runtime_callable_candidates(callable);
    let cases = runtime_string_descriptor_cases(ctx, None, candidate_names.as_deref())?;
    if cases.is_empty() {
        emit_undefined_runtime_string_call_fatal(ctx);
    } else {
        emit_string_name_descriptor_cases_loop(ctx, inst, arg_mixed, &cases, op_name)?;
    }
    abi::emit_jump(ctx.emitter, &done_label);

    // Boxed indexed array (tag 4): `[$obj, "method"]` or `["Class", "method"]`.
    if let Some(array_label) = &array_label {
        ctx.emitter.label(array_label);
        emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::BoxedArray(callable))?;
        emit_mixed_callable_array_descriptor_dispatch(
            ctx,
            inst,
            callable,
            arg_mixed,
            op_name,
            &instance_targets,
            &static_cases,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    // Boxed object (tag 6): dispatch its public `__invoke` implementation by
    // runtime class id, capturing the unboxed receiver in a descriptor.
    if let Some(object_label) = &object_label {
        ctx.emitter.label(object_label);
        emit_push_mixed_unbox_payload(ctx);
        let object_done_label = ctx.next_label("mixed_callable_object_done");
        for target in &invokable_targets {
            let next_label = ctx.next_label("mixed_callable_object_next");
            emit_branch_if_saved_receiver_class_id_mismatch(
                ctx,
                target.class_id,
                MIXED_VALUE_PAYLOAD_OFFSET,
                &next_label,
            );
            emit_runtime_array_instance_descriptor_invoke(
                ctx,
                inst,
                arg_mixed,
                target,
                MIXED_VALUE_PAYLOAD_OFFSET,
            )?;
            abi::emit_jump(ctx.emitter, &object_done_label);
            ctx.emitter.label(&next_label);
        }
        emit_mixed_callable_not_callable_fatal(ctx, op_name);
        ctx.emitter.label(&object_done_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fatal_label);
    emit_mixed_callable_not_callable_fatal(ctx, op_name);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits a fatal diagnostic for a boxed Mixed value that is called but is not callable.
fn emit_mixed_callable_not_callable_fatal(ctx: &mut FunctionContext<'_>, op_name: &str) {
    let message = format!(
        "Fatal error: Unsupported EIR {} mixed value is not callable\n",
        op_name
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
    let cases = runtime_string_descriptor_cases(ctx, None, candidate_names.as_deref())?;
    if cases.is_empty() {
        return Err(CodegenIrError::unsupported(
            "callable_descriptor_invoke for runtime string with no descriptor targets",
        ));
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(callable, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_string_name_descriptor_cases_loop(ctx, inst, arg_mixed, &cases, op_name)
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
    arg_mixed: ValueId,
    cases: &[callable_dispatch::RuntimeCallableCase],
    op_name: &str,
) -> Result<()> {
    let done_label = ctx.next_label(&format!("{}_runtime_string_done", op_name));
    let miss_label = ctx.next_label(&format!("{}_runtime_string_missing", op_name));
    let call_reg = abi::nested_call_reg(ctx.emitter);
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
    emit_undefined_runtime_string_call_fatal(ctx);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Builds runtime callable descriptor cases for string-name dynamic invocation.
pub(super) fn runtime_string_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    let cache_ty = source_arg_ty.map(PhpType::codegen_repr);
    if let Some(cases) = ctx
        .shared
        .runtime_string_descriptor_cases(cache_ty.as_ref(), candidate_names)
    {
        return Ok(cases);
    }
    let mut cases = runtime_extern_descriptor_cases(ctx, candidate_names)?;
    cases.extend(runtime_builtin_descriptor_cases(
        ctx,
        source_arg_ty,
        candidate_names,
    )?);
    cases.extend(runtime_user_function_descriptor_cases(
        ctx,
        source_arg_ty,
        candidate_names,
    ));
    cases.extend(
        runtime_static_method_descriptor_cases(ctx, candidate_names)
            .into_iter()
            .map(|case| case.case),
    );
    cases.sort_by(|left, right| left.label.cmp(&right.label));
    cases.dedup_by(|left, right| left.label == right.label);
    if cases.is_empty() && candidate_names.is_some() {
        let fallback = runtime_string_descriptor_cases(ctx, source_arg_ty, None)?;
        ctx.shared.cache_runtime_string_descriptor_cases(
            cache_ty.as_ref(),
            candidate_names,
            &fallback,
        );
        return Ok(fallback);
    }
    ctx.shared
        .cache_runtime_string_descriptor_cases(cache_ty.as_ref(), candidate_names, &cases);
    Ok(cases)
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

    let mut cases = Vec::new();
    for decl in decls {
        if !runtime_callable_name_is_reachable(&decl.name, candidate_names) {
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
            label: entry_label,
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
    }
}

/// Builds runtime descriptor cases for PHP builtins that support callable dispatch.
fn runtime_builtin_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
) -> Result<Vec<callable_dispatch::RuntimeCallableCase>> {
    let mut cases = Vec::new();
    for name in crate::types::checker::builtins::supported_builtin_function_names_on_platform(
        ctx.emitter.target.platform,
    ) {
        if !runtime_callable_name_is_reachable(name, candidate_names)
            || !callable_dispatch::runtime_builtin_wrapper_supported(name, source_arg_ty)
            || ctx
                .module
                .extern_decls
                .iter()
                .any(|decl| php_symbol_key(&decl.name) == php_symbol_key(name))
        {
            continue;
        }
        let Some(sig) = crate::types::first_class_callable_builtin_sig(name) else {
            continue;
        };
        let wrapper_sig =
            runtime_builtin_wrapper_sig(name, &crate::types::callable_wrapper_sig(&sig));
        let case_sig = callable_dispatch::specialized_runtime_case_sig(&wrapper_sig, source_arg_ty);
        let entry_label = emit_runtime_builtin_wrapper_inline(ctx, name, &case_sig)?;
        let invoker_label = emit_runtime_callable_invoker_inline(ctx, &case_sig, &[]);
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
        cases.push(callable_dispatch::RuntimeCallableCase {
            label: entry_label,
            descriptor_label,
            php_name: Some(name.to_string()),
        });
    }
    Ok(cases)
}

/// Builds runtime descriptor cases for user functions emitted in the EIR module.
fn runtime_user_function_descriptor_cases(
    ctx: &mut FunctionContext<'_>,
    source_arg_ty: Option<&PhpType>,
    candidate_names: Option<&[String]>,
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

    let mut cases = Vec::new();
    for function in functions {
        if !runtime_callable_name_is_reachable(&function.name, candidate_names) {
            continue;
        }
        let wrapper_sig =
            crate::types::callable_wrapper_sig(&function_signature_from_eir(function));
        let case_sig = callable_dispatch::specialized_runtime_case_sig(&wrapper_sig, source_arg_ty);
        let invoker_label = emit_runtime_callable_invoker_inline(ctx, &case_sig, &[]);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &function_symbol(&function.name),
            Some(&function.name),
            callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            Some(&case_sig),
            &[],
            &[],
            callable_descriptor::CallableDescriptorInvocation::named(
                callable_descriptor::CallableDescriptorShape::Function,
                &function.name,
            ),
            Some(&invoker_label),
        );
        cases.push(callable_dispatch::RuntimeCallableCase {
            label: function_symbol(&function.name),
            descriptor_label,
            php_name: Some(function.name.clone()),
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
) -> Result<()> {
    let candidate_names = ctx.runtime_callable_candidates(callable);
    let cases = runtime_string_descriptor_cases(ctx, None, candidate_names.as_deref())?;
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
    emit_undefined_runtime_string_call_fatal(ctx);

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
    let sig = class_info
        .methods
        .get(method_key)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "{} for non-invokable object '{}'",
                op_name, normalized_class
            ))
        })?
        .clone();
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

/// Returns one module-wide static descriptor template for a public instance method.
/// Receiver objects are captured into runtime copies, so the wrapper, invoker, and
/// immutable descriptor header can be shared by every dynamic call site.
fn runtime_instance_method_descriptor_template(
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
    let invoker_label = emit_runtime_callable_invoker_inline(ctx, sig, &captures);
    let php_name = format!("{}::{}", class_name, method_name);
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
    if instance_targets.is_empty() && static_cases.is_empty() {
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
    for target in instance_targets {
        let next_label = ctx.next_label("callable_array_instance_next");
        emit_branch_if_runtime_array_instance_mismatch(ctx, target, &next_label);
        emit_runtime_array_instance_descriptor_invoke(
            ctx,
            inst,
            arg_mixed,
            target,
            MIXED_RECEIVER_PAYLOAD_OFFSET,
        )?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    for case in static_cases {
        let next_label = ctx.next_label("callable_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(ctx, case, &next_label);
        emit_static_descriptor_case_invoke(ctx, inst, arg_mixed, &case.case.descriptor_label)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    ctx.emitter.label(&miss_label);
    if super::builtins::has_eval_context(ctx) {
        super::builtins::lower_eval_callable_call_array(ctx, inst, callable, arg_mixed)?;
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
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    if static_cases.is_empty() {
        return Err(CodegenIrError::unsupported(
            "callable_descriptor_invoke for runtime string callable array with no static targets",
        ));
    }

    emit_string_callable_array_selector_slots(ctx, callable)?;
    let done_label = ctx.next_label("callable_array_runtime_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
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
    match ctx.value_php_type(callable)?.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            emit_mixed_callable_array_descriptor_value(ctx, callable, op_name)
        }
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => {
            emit_string_callable_array_descriptor_value(ctx, callable, op_name)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for callable-array PHP type {:?}",
            op_name, other
        ))),
    }
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
    for target in &targets {
        let next_label = ctx.next_label("callable_array_instance_next");
        emit_branch_if_runtime_array_instance_mismatch(ctx, target, &next_label);
        emit_runtime_array_instance_descriptor_value(ctx, target)?;
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

/// Selects a callable descriptor from a mixed callable-array value.
fn emit_mixed_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let instance_targets = runtime_array_instance_method_targets_for_descriptor(ctx);
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    if instance_targets.is_empty() && static_cases.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime mixed callable array with no descriptor targets",
            op_name
        )));
    }

    emit_mixed_callable_array_selector_slots(ctx, &CallableArraySource::RawArray(callable))?;
    let done_label = ctx.next_label("callable_array_descriptor_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
    for target in &instance_targets {
        let next_label = ctx.next_label("callable_array_instance_next");
        emit_branch_if_runtime_array_instance_mismatch(ctx, target, &next_label);
        emit_runtime_array_instance_descriptor_value(ctx, target)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    for case in &static_cases {
        let next_label = ctx.next_label("callable_array_static_next");
        emit_branch_if_mixed_static_case_mismatch(ctx, case, &next_label);
        abi::emit_symbol_address(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            &case.case.descriptor_label,
        );
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

/// Selects a callable descriptor from a string-only static callable-array value.
fn emit_string_callable_array_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    op_name: &str,
) -> Result<()> {
    let static_cases = runtime_static_method_descriptor_cases(ctx, None);
    if static_cases.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime string callable array with no static targets",
            op_name
        )));
    }

    emit_string_callable_array_selector_slots(ctx, callable)?;
    let done_label = ctx.next_label("callable_array_descriptor_done");
    let miss_label = ctx.next_label(&format!("{}_callable_array_missing", op_name));
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

    ctx.emitter.label(&miss_label);
    emit_runtime_callable_array_no_match_abort(ctx);

    ctx.emitter.label(&done_label);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_SELECTOR_BYTES);
    Ok(())
}

/// Collects public instance methods for runtime descriptor selection.
fn runtime_array_instance_method_targets_for_descriptor(
    ctx: &FunctionContext<'_>,
) -> Vec<RuntimeArrayInstanceMethodTarget> {
    let mut targets = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut methods = class_info.methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, sig) in methods {
            if !class_info
                .method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
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
                class_name: class_name.clone(),
                class_id: class_info.class_id,
                method_key,
                method_name: method_name.clone(),
                impl_class,
                sig: sig.clone(),
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
        let wrapper_sig = callable_dispatch::static_method_runtime_wrapper_sig(&sig);
        let php_name = format!("{}::{}", class_name, method_name);
        if let Some(case) = ctx
            .shared
            .runtime_static_method_descriptor_case(&php_name)
        {
            cases.push(case);
            continue;
        }
        let Ok(entry_label) = emit_static_method_descriptor_entry_wrapper(
            ctx,
            &impl_class,
            &method_key,
            &wrapper_sig,
            class_id,
        ) else {
            continue;
        };
        let invoker_label = emit_runtime_callable_invoker_inline(ctx, &wrapper_sig, &[]);
        let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
            ctx.data,
            &entry_label,
            Some(&php_name),
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
            label: entry_label,
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
        cases.push(static_case);
    }
    ctx.shared
        .cache_runtime_static_method_descriptor_cases(candidate_names, &cases);
    cases
}

/// Collects public instance-method targets that can receive this positional shape.
fn runtime_array_instance_method_targets(
    ctx: &FunctionContext<'_>,
    arg_count: usize,
) -> Vec<RuntimeArrayInstanceMethodTarget> {
    let mut targets = Vec::new();
    let mut classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        let mut methods = class_info.methods.iter().collect::<Vec<_>>();
        methods.sort_by(|left, right| left.0.cmp(right.0));
        for (method_name, sig) in methods {
            if sig.params.len() != arg_count || sig.variadic.is_some() {
                continue;
            }
            if !class_info
                .method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
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
                class_name: class_name.clone(),
                class_id: class_info.class_id,
                method_key,
                method_name: method_name.clone(),
                impl_class,
                sig: sig.clone(),
            });
        }
    }
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
    emit_require_mixed_callable_array_pair(ctx, source)?;
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
        return emit_boxed_callable_array_selector_slots(ctx, source);
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
    emit_runtime_callable_array_no_match_abort(ctx);

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

/// Returns true when a selector value was produced by an EIR array literal allocation.
fn value_is_array_literal(ctx: &FunctionContext<'_>, value: ValueId) -> bool {
    let Some(value_ref) = ctx.function.value(value) else {
        return false;
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return false;
    };
    ctx.function
        .instruction(inst)
        .is_some_and(|inst| matches!(inst.op, Op::ArrayNew))
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
                    ctx.emitter.instruction(&format!("mov {}, rdi", array_reg)); // borrow the unboxed indexed-array pointer as the selector base
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
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_load_callable_array_base(ctx, source, array_reg)?;
    emit_require_callable_array_pair_in_reg(ctx, array_reg);
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
    emit_require_callable_array_pair_in_reg(ctx, array_reg);
    Ok(())
}

/// Emits the target-aware length check for a loaded indexed-array pointer.
fn emit_require_callable_array_pair_in_reg(ctx: &mut FunctionContext<'_>, array_reg: &str) {
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
    emit_runtime_callable_array_no_match_abort(ctx);
    ctx.emitter.label(&valid_label);
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

/// Calls one matched runtime instance-method target through the normal EIR ABI.
fn emit_runtime_array_instance_method_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    args: &[ValueId],
    target: &RuntimeArrayInstanceMethodTarget,
) -> Result<()> {
    let receiver_ty = PhpType::Object(target.class_name.clone());
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
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&target.impl_class, &target.method_key),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    store_call_result(ctx, inst, &target.sig.return_type)?;
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Builds and invokes a receiver-captured descriptor for a matched instance method.
fn emit_runtime_array_instance_descriptor_invoke(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    arg_mixed: ValueId,
    target: &RuntimeArrayInstanceMethodTarget,
    receiver_payload_offset: usize,
) -> Result<()> {
    let receiver_ty = PhpType::Object(target.class_name.clone());
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
    emit_descriptor_reg_invoker_call_with_mixed_arg(
        ctx,
        inst,
        abi::nested_call_reg(ctx.emitter),
        arg_mixed,
        "callable_descriptor_invoke",
        true,
    )
}

/// Builds a receiver-captured descriptor for a matched runtime instance method.
fn emit_runtime_array_instance_descriptor_value(
    ctx: &mut FunctionContext<'_>,
    target: &RuntimeArrayInstanceMethodTarget,
) -> Result<()> {
    let receiver_ty = PhpType::Object(target.class_name.clone());
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

/// Emits the fatal path for runtime callable arrays without a matching method.
fn emit_runtime_callable_array_no_match_abort(ctx: &mut FunctionContext<'_>) {
    let (message_label, message_len) = ctx
        .data
        .add_string(b"Fatal error: callable array did not resolve to an invokable target\n");
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
    emit_descriptor_reg_invoker_call_with_args(ctx, inst, descriptor_reg, visible_args, op_name)
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
    emit_invoker_arg_mixed(ctx, visible_args)?;
    if release_runtime_descriptor {
        abi::emit_push_reg(ctx.emitter, descriptor_reg);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed Mixed argument array across descriptor register setup
    move_reg_to_arg(ctx, descriptor_reg, 0);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg_reg, 0);
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    emit_descriptor_invoker_reg_call(ctx.emitter, invoker_reg);
    release_invoker_arg_preserving_result(ctx);
    if release_runtime_descriptor {
        release_saved_runtime_descriptor_preserving_result(ctx);
    }
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
    )?;
    store_descriptor_invoker_result(ctx, inst)
}

/// Calls a loaded descriptor invoker with an argument container and leaves a Mixed result.
pub(super) fn emit_descriptor_reg_invoker_mixed_result_with_arg_container(
    ctx: &mut FunctionContext<'_>,
    descriptor_reg: &str,
    arg_mixed: ValueId,
    op_name: &str,
    release_runtime_descriptor: bool,
) -> Result<()> {
    if descriptor_arg_is_prebuilt_mixed_box(ctx, arg_mixed)? {
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

/// Calls a descriptor invoker with a boxed Mixed argument created by EIR lowering.
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
    if release_runtime_descriptor {
        abi::emit_push_reg(ctx.emitter, descriptor_reg);
    }
    move_reg_to_arg(ctx, descriptor_reg, 0);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    ctx.load_value_to_reg(arg_mixed, arg_reg)?;
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    emit_descriptor_invoker_reg_call(ctx.emitter, invoker_reg);
    if release_runtime_descriptor {
        release_saved_runtime_descriptor_preserving_result(ctx);
    }
    release_prebuilt_invoker_arg_preserving_result(ctx, arg_mixed)?;
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
    callable_descriptor::emit_load_invoker_from_descriptor(
        ctx.emitter,
        invoker_reg,
        descriptor_reg,
    );
    emit_descriptor_invoker_reg_call(ctx.emitter, invoker_reg);
    release_invoker_arg_preserving_result(ctx);
    release_saved_descriptor_after_normalized_arg(ctx, release_runtime_descriptor);
    Ok(())
}

/// Calls a generated descriptor invoker with the platform-required caller stack area.
fn emit_descriptor_invoker_reg_call(emitter: &mut crate::codegen::emit::Emitter, invoker_reg: &str) {
    let call_pad_bytes = abi::outgoing_call_stack_pad_bytes(emitter.target, 0);
    abi::emit_reserve_temporary_stack(emitter, call_pad_bytes);
    abi::emit_call_reg(emitter, invoker_reg);
    abi::emit_release_temporary_stack(emitter, call_pad_bytes);
}

/// Returns the branch-ready label name for a descriptor invoker callsite.
fn descriptor_invoker_ready_label(ctx: &mut FunctionContext<'_>, op_name: &str) -> String {
    if matches!(op_name, "callable_descriptor_invoke" | "iterator_apply") {
        return ctx.next_label("cufa_descriptor_invoker_ready");
    }
    ctx.next_label(&format!("{}_descriptor_invoker_ready", op_name))
}

/// Returns true when the argument container is already a temporary Mixed box.
fn descriptor_arg_is_prebuilt_mixed_box(
    ctx: &FunctionContext<'_>,
    arg_mixed: ValueId,
) -> Result<bool> {
    if ctx.value_php_type(arg_mixed)?.codegen_repr() != PhpType::Mixed {
        return Ok(false);
    }
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

/// Emits a normalized boxed Mixed argument container for descriptor invokers.
fn emit_normalized_invoker_arg_container(
    ctx: &mut FunctionContext<'_>,
    arg_container: ValueId,
    emit_receiver_mixed_markers: bool,
) -> Result<()> {
    let container_ty = ctx.value_php_type(arg_container)?.codegen_repr();
    let dest_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    match container_ty {
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

/// Releases the descriptor saved while normalizing the argument container.
fn release_saved_descriptor_after_normalized_arg(
    ctx: &mut FunctionContext<'_>,
    release_runtime_descriptor: bool,
) {
    if release_runtime_descriptor {
        release_saved_runtime_descriptor_preserving_result(ctx);
    } else {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
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
fn emit_invoker_arg_mixed(ctx: &mut FunctionContext<'_>, args: &[ValueId]) -> Result<()> {
    emit_invoker_arg_array(ctx, args)?;
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    Ok(())
}

/// Creates the indexed array consumed by runtime callable descriptor invokers.
fn emit_invoker_arg_array(ctx: &mut FunctionContext<'_>, args: &[ValueId]) -> Result<()> {
    emit_new_invoker_arg_array(ctx, args.len());
    if args.is_empty() {
        return Ok(());
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the in-progress invoker argument array across element boxing
    for arg in args {
        emit_box_invoker_arg(ctx, *arg)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // preserve the boxed argument while loading the invoker array
        emit_append_boxed_invoker_arg(ctx);
        emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, &PhpType::Mixed);
        emit_store_result_to_top_stack_slot(ctx);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
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
    ctx.load_value_to_result(arg)?;
    if matches!(arg_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_incref_if_refcounted(ctx.emitter, &arg_ty);
    } else if ctx.value_can_own_mixed_box_source(arg)? {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &arg_ty);
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &arg_ty);
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
fn release_invoker_arg_preserving_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Releases a prebuilt Mixed argument container while preserving the Mixed result.
fn release_prebuilt_invoker_arg_preserving_result(
    ctx: &mut FunctionContext<'_>,
    arg_mixed: ValueId,
) -> Result<()> {
    abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    ctx.load_value_to_result(arg_mixed)?;
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Releases the saved runtime descriptor while preserving the Mixed call result.
fn release_saved_runtime_descriptor_preserving_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_result_value(ctx.emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Stores the Mixed descriptor-invoker result using the EIR result type.
fn store_descriptor_invoker_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    match ctx.value_php_type(result)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => ctx.store_result_value(result),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            ctx.store_result_value(result)
        }
        PhpType::Int => {
            move_result_to_arg(ctx, 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.store_result_value(result)
        }
        PhpType::Bool => {
            move_result_to_arg(ctx, 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
            ctx.store_result_value(result)
        }
        PhpType::Float => {
            move_result_to_arg(ctx, 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
            ctx.store_result_value(result)
        }
        PhpType::Str => {
            move_result_to_arg(ctx, 0);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            ctx.store_result_value(result)
        }
        PhpType::TaggedScalar => store_descriptor_invoker_tagged_scalar_result(ctx, result),
        other => Err(CodegenIrError::unsupported(format!(
            "descriptor invoker result for PHP type {:?}",
            other
        ))),
    }
}

/// Unboxes a Mixed descriptor result into the inline nullable-int result shape.
fn store_descriptor_invoker_tagged_scalar_result(
    ctx: &mut FunctionContext<'_>,
    result: ValueId,
) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov x0, x1");                              // place the unboxed nullable-int payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov x1, x9");                              // place the unboxed Mixed tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the unboxed Mixed tag before moving the payload
            ctx.emitter.instruction("mov rax, rdi");                            // place the unboxed nullable-int payload into the tagged-scalar payload register
            ctx.emitter.instruction("mov rdx, r10");                            // place the unboxed Mixed tag into the tagged-scalar tag register
        }
    }
    ctx.store_result_value(result)
}

/// Moves the current integer result register into an ABI argument register.
fn move_result_to_arg(ctx: &mut FunctionContext<'_>, arg_index: usize) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    move_reg_to_arg(ctx, result_reg, arg_index);
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
            Some(RuntimeStringFunctionTarget {
                name: function.name.clone(),
                param_types: function
                    .params
                    .iter()
                    .map(|param| param.php_type.codegen_repr())
                    .collect(),
                return_ty,
            })
        })
        .collect::<Vec<_>>();
    Ok(targets)
}

/// Returns true when the selected runtime function can be stored into the EIR result.
fn runtime_string_result_type_supported(result_ty: &PhpType, return_ty: &PhpType) -> bool {
    result_ty == return_ty || matches!(result_ty, PhpType::Mixed | PhpType::Union(_))
}

/// Converts arbitrary PHP function names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
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
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // dispatch to this user function when names match case-insensitively
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
        emit_box_current_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    }
    ctx.store_result_value(result)
}

/// Emits the fatal path for an unmatched runtime string callable name.
fn emit_undefined_runtime_string_call_fatal(ctx: &mut FunctionContext<'_>) {
    let message = b"Fatal error: Call to undefined function <dynamic>()\n";
    let (message_label, message_len) = ctx.data.add_string(message);
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
