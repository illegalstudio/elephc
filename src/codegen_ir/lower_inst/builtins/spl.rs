//! Purpose:
//! Lowers SPL object-introspection builtins for the EIR backend.
//! Handles stable object ids and object hashes using the concrete heap pointer.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - The legacy backend exposes the object pointer as a process-stable identity.
//!   `spl_object_hash()` stringifies that same identity with the shared itoa helper.

use crate::codegen::{
    abi, callable_descriptor, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    runtime_value_tag,
};
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{BlockId, Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::function_symbol;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{
    callables, direct_call_stack_pad_bytes, iterators, materialize_direct_call_args, predicates,
};
use super::{expect_operand, store_if_result};

const EXTS_PTR_SYMBOL: &str = "_spl_autoload_exts_ptr";
const EXTS_LEN_SYMBOL: &str = "_spl_autoload_exts_len";
const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;
const APPLY_DYNAMIC_CALLBACK_PTR_OFFSET: usize = 32;
const APPLY_DYNAMIC_CALLBACK_LEN_OFFSET: usize = 40;
const APPLY_DESCRIPTOR_OFFSET: usize = 32;

const SPL_CLASS_NAMES: &[&str] = &[
    "AppendIterator",
    "ArrayAccess",
    "ArrayIterator",
    "ArrayObject",
    "BadFunctionCallException",
    "BadMethodCallException",
    "CachingIterator",
    "CallbackFilterIterator",
    "Countable",
    "DomainException",
    "DirectoryIterator",
    "EmptyIterator",
    "Error",
    "Exception",
    "FilterIterator",
    "FilesystemIterator",
    "GlobIterator",
    "InfiniteIterator",
    "InvalidArgumentException",
    "Iterator",
    "IteratorAggregate",
    "IteratorIterator",
    "JsonSerializable",
    "LengthException",
    "LimitIterator",
    "LogicException",
    "MultipleIterator",
    "NoRewindIterator",
    "OuterIterator",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OverflowException",
    "ParentIterator",
    "RangeException",
    "RecursiveArrayIterator",
    "RecursiveCachingIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveDirectoryIterator",
    "RecursiveFilterIterator",
    "RecursiveIterator",
    "RecursiveIteratorIterator",
    "RecursiveRegexIterator",
    "RegexIterator",
    "RuntimeException",
    "SeekableIterator",
    "SplDoublyLinkedList",
    "SplFixedArray",
    "SplFileInfo",
    "SplFileObject",
    "SplObserver",
    "SplQueue",
    "SplStack",
    "SplSubject",
    "SplTempFileObject",
    "Stringable",
    "Throwable",
    "Traversable",
    "TypeError",
    "UnderflowException",
    "UnexpectedValueException",
    "ValueError",
];

/// Callback strategy supported by the current EIR `iterator_apply()` lowering.
enum IteratorApplyCallback {
    StaticUserFunction {
        label: String,
        return_ty: PhpType,
        args: Vec<ValueId>,
        param_types: Vec<PhpType>,
    },
    DynamicString {
        callable: ValueId,
        targets: Vec<IteratorApplyCallbackTarget>,
    },
    DescriptorCallable {
        callable: ValueId,
        arg_container: Option<ValueId>,
    },
    DescriptorString {
        callable: ValueId,
        arg_container: Option<ValueId>,
    },
    DescriptorCallableArray {
        callable: ValueId,
        arg_container: Option<ValueId>,
        release_runtime_descriptor: bool,
    },
}

/// Runtime-selectable zero-argument user function for dynamic string callbacks.
struct IteratorApplyCallbackTarget {
    name: String,
    label: String,
    return_ty: PhpType,
}

/// Lowers autoload registration stubs by preserving arg effects and returning true.
pub(super) fn lower_spl_autoload_bool(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match name {
        "spl_autoload_register" => super::ensure_arg_count_between(inst, name, 0, 3)?,
        "spl_autoload_unregister" => super::ensure_arg_count(inst, name, 1)?,
        _ => return Err(CodegenIrError::unsupported(format!("autoload bool stub {}", name))),
    }
    emit_args_for_side_effects(ctx, inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    store_if_result(ctx, inst)
}

/// Lowers no-op autoload calls by preserving arg effects and returning PHP null if used.
pub(super) fn lower_spl_autoload_void(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match name {
        "spl_autoload_call" => super::ensure_arg_count(inst, name, 1)?,
        "spl_autoload" => super::ensure_arg_count_between(inst, name, 1, 2)?,
        _ => return Err(CodegenIrError::unsupported(format!("autoload void stub {}", name))),
    }
    emit_args_for_side_effects(ctx, inst)?;
    emit_null_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `spl_autoload_functions()` to an indexed array of AOT rule placeholders.
pub(super) fn lower_spl_autoload_functions(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_autoload_functions", 0)?;
    let rule_count = crate::codegen::autoload_rule_count();
    emit_int_array(ctx, rule_count.max(1), |ctx| emit_autoload_function_placeholders(ctx, rule_count))?;
    store_if_result(ctx, inst)
}

/// Lowers `spl_autoload_extensions()` against the legacy mutable extension globals.
pub(super) fn lower_spl_autoload_extensions(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "spl_autoload_extensions", 0, 1)?;
    if inst.operands.is_empty() {
        emit_extensions_read(ctx);
        return store_if_result(ctx, inst);
    }

    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => emit_extensions_write(ctx, value)?,
        PhpType::Void => {
            ctx.load_value_to_result(value)?;
            emit_extensions_read(ctx);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "spl_autoload_extensions for PHP type {:?}",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `spl_classes()` to the static compiler-shipped SPL/core type snapshot.
pub(super) fn lower_spl_classes(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_classes", 0)?;
    emit_string_array(ctx, SPL_CLASS_NAMES)?;
    store_if_result(ctx, inst)
}

/// Lowers `spl_object_id(object)` by returning the loaded object pointer as an integer.
pub(super) fn lower_spl_object_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_object_id", 1)?;
    load_object_operand(ctx, inst, "spl_object_id")?;
    store_if_result(ctx, inst)
}

/// Lowers `spl_object_hash(object)` by formatting the loaded object pointer as a string.
pub(super) fn lower_spl_object_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "spl_object_hash", 1)?;
    load_object_operand(ctx, inst, "spl_object_hash")?;
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    store_if_result(ctx, inst)
}

/// Lowers `iterator_count()` over arrays, `iterable`, and Traversable objects.
pub(super) fn lower_iterator_count(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "iterator_count", 1)?;
    let source = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(source)?.codegen_repr();
    ctx.load_value_to_result(source)?;
    match source_ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_count_loaded_array(ctx);
        }
        PhpType::Iterable => {
            emit_count_loaded_iterable(ctx)?;
        }
        PhpType::Object(_) => {
            emit_count_loaded_traversable_object(ctx)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "iterator_count for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `iterator_to_array()` over arrays, `iterable`, and Traversable objects.
pub(super) fn lower_iterator_to_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "iterator_to_array", 1, 2)?;
    let source = expect_operand(inst, 0)?;
    let preserve = inst.operands.get(1).copied();
    let source_ty = ctx.value_php_type(source)?.codegen_repr();

    if let Some(preserve) = preserve {
        if let Some(preserve_keys) = static_preserve_keys_operand(ctx, preserve)? {
            ctx.load_value_to_result(source)?;
            emit_to_array_loaded_source(ctx, &source_ty, preserve_keys)?;
            return store_if_result(ctx, inst);
        }
        return emit_dynamic_preserve_keys(ctx, inst, source, preserve, &source_ty);
    }

    ctx.load_value_to_result(source)?;
    emit_to_array_loaded_source(ctx, &source_ty, true)?;
    store_if_result(ctx, inst)
}

/// Lowers `iterator_apply()` over supported Traversable sources and callback forms.
pub(super) fn lower_iterator_apply(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "iterator_apply", 2, 3)?;
    let source = expect_operand(inst, 0)?;
    let callback_value = expect_operand(inst, 1)?;
    let callback = iterator_apply_callback(ctx, callback_value, inst)?;
    let source_ty = ctx.value_php_type(source)?.codegen_repr();

    emit_apply_callback_state(ctx, &callback)?;

    ctx.load_value_to_result(source)?;
    match source_ty {
        PhpType::Iterable => emit_apply_loaded_iterable(ctx, &callback)?,
        PhpType::Object(_) => emit_apply_loaded_traversable_object(ctx, &callback)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "iterator_apply for PHP type {:?}",
                other
            )))
        }
    }
    release_apply_callback_state(ctx, &callback);
    store_if_result(ctx, inst)
}

/// Materializes any callback state that must stay alive across the iterator loop.
fn emit_apply_callback_state(
    ctx: &mut FunctionContext<'_>,
    callback: &IteratorApplyCallback,
) -> Result<()> {
    match callback {
        IteratorApplyCallback::DynamicString { callable, .. } => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(*callable, ptr_reg, len_reg)?;
            abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
            Ok(())
        }
        IteratorApplyCallback::DescriptorCallable { callable, .. } => {
            ctx.load_value_to_result(*callable)?;
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Callable);
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        IteratorApplyCallback::DescriptorString { callable, .. } => {
            callables::emit_runtime_string_descriptor_value(
                ctx,
                *callable,
                abi::int_result_reg(ctx.emitter),
                "iterator_apply",
            )?;
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        IteratorApplyCallback::DescriptorCallableArray {
            callable,
            release_runtime_descriptor,
            ..
        } => {
            if *release_runtime_descriptor {
                callables::emit_runtime_mixed_instance_callable_array_descriptor_value(
                    ctx,
                    *callable,
                    "iterator_apply",
                )?;
            } else {
                callables::emit_runtime_callable_array_descriptor_value(ctx, *callable, "iterator_apply")?;
            }
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            Ok(())
        }
        IteratorApplyCallback::StaticUserFunction { .. } => Ok(()),
    }
}

/// Releases callback state after the iterator loop has produced the count result.
fn release_apply_callback_state(ctx: &mut FunctionContext<'_>, callback: &IteratorApplyCallback) {
    match callback {
        IteratorApplyCallback::DynamicString { .. } => {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        IteratorApplyCallback::DescriptorCallable { .. } => {
            emit_release_saved_apply_descriptor(ctx);
        }
        IteratorApplyCallback::DescriptorString { .. } => {
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        IteratorApplyCallback::DescriptorCallableArray {
            release_runtime_descriptor,
            ..
        } => {
            if *release_runtime_descriptor {
                emit_release_saved_apply_descriptor(ctx);
            } else {
                abi::emit_release_temporary_stack(ctx.emitter, 16);
            }
        }
        IteratorApplyCallback::StaticUserFunction { .. } => {}
    }
}

/// Releases the saved runtime descriptor while preserving the final callback count.
fn emit_release_saved_apply_descriptor(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, 16);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Resolves the supported callback forms for `iterator_apply()`.
fn iterator_apply_callback(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    inst: &Instruction,
) -> Result<IteratorApplyCallback> {
    if let Some(callback_name) = static_string_operand(ctx, callback)? {
        let arg_container = iterator_apply_arg_container(ctx, inst)?;
        let args = if let Some(arg_container) = arg_container {
            match iterator_apply_static_array_items(ctx, inst, arg_container)? {
                Some(args) => args,
                None => {
                    return Ok(IteratorApplyCallback::DescriptorString {
                        callable: callback,
                        arg_container: Some(arg_container),
                    })
                }
            }
        } else {
            Vec::new()
        };
        let Some(callback_function) = ctx.callable_function_by_name(&callback_name) else {
            if arg_container.is_some() {
                return Ok(IteratorApplyCallback::DescriptorString {
                    callable: callback,
                    arg_container,
                });
            }
            return Err(CodegenIrError::unsupported(format!(
                "iterator_apply callback {} without emitted EIR function",
                callback_name
            )));
        };
        if callback_function.params.len() != args.len() {
            if arg_container.is_some() {
                return Ok(IteratorApplyCallback::DescriptorString {
                    callable: callback,
                    arg_container,
                });
            }
            return Err(CodegenIrError::unsupported(format!(
                "iterator_apply callback {} with {} params and {} EIR args",
                callback_name,
                callback_function.params.len(),
                args.len()
            )));
        }
        if callback_function
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic)
        {
            if arg_container.is_some() {
                return Ok(IteratorApplyCallback::DescriptorString {
                    callable: callback,
                    arg_container,
                });
            }
            return Err(CodegenIrError::unsupported(format!(
                "iterator_apply callback {} with by-ref or variadic params",
                callback_name
            )));
        }
        let param_types = callback_function
            .params
            .iter()
            .map(|param| param.php_type.codegen_repr())
            .collect();
        return Ok(IteratorApplyCallback::StaticUserFunction {
            label: function_symbol(&callback_function.name),
            return_ty: callback_function.return_php_type.codegen_repr(),
            args,
            param_types,
        });
    }

    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Str => {
            let arg_container = iterator_apply_arg_container(ctx, inst)?;
            if arg_container.is_some() {
                return Ok(IteratorApplyCallback::DescriptorString {
                    callable: callback,
                    arg_container,
                });
            }
            let targets = iterator_apply_runtime_string_targets(ctx);
            if targets.is_empty() {
                return Err(CodegenIrError::unsupported(
                    "iterator_apply EIR dynamic string callback with no zero-arg targets",
                ));
            }
            Ok(IteratorApplyCallback::DynamicString { callable: callback, targets })
        }
        PhpType::Callable => {
            Ok(IteratorApplyCallback::DescriptorCallable {
                callable: callback,
                arg_container: iterator_apply_arg_container(ctx, inst)?,
            })
        }
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            Ok(IteratorApplyCallback::DescriptorCallableArray {
                callable: callback,
                arg_container: iterator_apply_arg_container(ctx, inst)?,
                release_runtime_descriptor: true,
            })
        }
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => {
            Ok(IteratorApplyCallback::DescriptorCallableArray {
                callable: callback,
                arg_container: iterator_apply_arg_container(ctx, inst)?,
                release_runtime_descriptor: false,
            })
        }
        other => Err(CodegenIrError::unsupported(format!(
            "iterator_apply EIR dynamic callback PHP type {:?}",
            other
        ))),
    }
}

/// Collects zero-argument user functions that dynamic string callbacks may select.
fn iterator_apply_runtime_string_targets(
    ctx: &FunctionContext<'_>,
) -> Vec<IteratorApplyCallbackTarget> {
    let mut targets = ctx
        .module
        .functions
        .iter()
        .filter(|function| !function.flags.is_main)
        .filter(|function| function.params.is_empty())
        .filter_map(|function| {
            let return_ty = function.return_php_type.codegen_repr();
            if !iterator_apply_callback_return_supported(&return_ty) {
                return None;
            }
            Some(IteratorApplyCallbackTarget {
                name: function.name.clone(),
                label: function_symbol(&function.name),
                return_ty,
            })
        })
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| left.label.cmp(&right.label));
    targets.dedup_by(|left, right| left.label == right.label);
    targets
}

/// Returns true when callback truthiness can be tested by this lowering.
fn iterator_apply_callback_return_supported(return_ty: &PhpType) -> bool {
    matches!(
        return_ty.codegen_repr(),
        PhpType::Bool
            | PhpType::Int
            | PhpType::Float
            | PhpType::Void
            | PhpType::Never
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns the original callback argument container for descriptor-backed callbacks.
fn iterator_apply_arg_container(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<ValueId>> {
    let Some(args) = inst.operands.get(2).copied() else {
        return Ok(None);
    };
    if iterator_apply_arg_is_null(ctx, args)? {
        return Ok(None);
    }
    match ctx.value_php_type(args)?.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => {
            Ok(Some(args))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "iterator_apply EIR callback argument container PHP type {:?}",
            other
        ))),
    }
}

/// Returns true when an explicit third argument is PHP null.
fn iterator_apply_arg_is_null(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let value = strip_iterator_apply_arg_acquire(ctx, value)?;
    Ok(value_defining_op(ctx, value)? == Some(Op::ConstNull))
}

/// Recovers values pushed into a simple indexed-array literal before `iterator_apply()`.
fn iterator_apply_static_array_items(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<Option<Vec<ValueId>>> {
    let array = strip_iterator_apply_arg_acquire(ctx, value)?;
    let Some((array_inst, array_block, _)) = value_instruction_with_location(ctx, array)? else {
        return Ok(None);
    };
    if array_inst.op != Op::ArrayNew {
        return Ok(None);
    }
    let limit_index = iterator_apply_instruction_index(ctx, inst)?
        .filter(|(block, _)| *block == array_block)
        .map(|(_, index)| index)
        .unwrap_or(u32::MAX);
    Ok(Some(iterator_apply_array_items(
        ctx,
        array,
        array_block,
        limit_index,
    )?))
}

/// Removes an acquire wrapper from an arg-array value when EIR inserted one.
fn strip_iterator_apply_arg_acquire(ctx: &FunctionContext<'_>, value: ValueId) -> Result<ValueId> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(value);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op == Op::Acquire {
        Ok(inst_ref.operands.first().copied().unwrap_or(value))
    } else {
        Ok(value)
    }
}

/// Returns the defining opcode for an SSA value when it comes from an instruction.
fn value_defining_op(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<Op>> {
    Ok(value_instruction_with_location(ctx, value)?.map(|(inst, _, _)| inst.op))
}

/// Returns an instruction-backed value definition and its block/index location.
fn value_instruction_with_location<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<(&'a Instruction, BlockId, u32)>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(Some((inst_ref, block, index)))
}

/// Returns the current builtin instruction location from its result value when available.
fn iterator_apply_instruction_index(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<Option<(BlockId, u32)>> {
    let Some(result) = inst.result else {
        return Ok(None);
    };
    let Some(value_ref) = ctx.function.value(result) else {
        return Err(CodegenIrError::missing_entry("value", result.as_raw()));
    };
    let ValueDef::Instruction { block, index, .. } = value_ref.def else {
        return Ok(None);
    };
    Ok(Some((block, index)))
}

/// Collects item values pushed into an indexed-array literal before the builtin call.
fn iterator_apply_array_items(
    ctx: &FunctionContext<'_>,
    array: ValueId,
    block: BlockId,
    limit_index: u32,
) -> Result<Vec<ValueId>> {
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut items = Vec::new();
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= limit_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::ArrayPush && inst_ref.operands.first().copied() == Some(array) {
            let Some(item) = inst_ref.operands.get(1).copied() else {
                return Err(CodegenIrError::invalid_module(
                    "iterator_apply arg array push missing value operand",
                ));
            };
            items.push(item);
        }
    }
    Ok(items)
}

/// Loads the single object operand into the canonical integer result register.
fn load_object_operand(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty {
        PhpType::Object(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Returns a string literal operand when EIR preserved it as a `ConstStr`.
fn static_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let (Op::ConstStr, Some(Immediate::Data(data))) = (inst_ref.op, inst_ref.immediate.as_ref()) else {
        return Ok(None);
    };
    let value = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    Ok(Some(value.clone()))
}

/// Reads the runtime length header from the loaded indexed array or hash pointer.
fn emit_count_loaded_array(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
}

/// Emits `iterator_to_array()` once the source value is loaded into result registers.
fn emit_to_array_loaded_source(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    preserve_keys: bool,
) -> Result<()> {
    match source_ty.codegen_repr() {
        PhpType::Array(_) => {
            emit_clone_loaded_array(ctx);
            Ok(())
        }
        PhpType::AssocArray { value, .. } if preserve_keys => {
            emit_clone_loaded_hash(ctx, value.codegen_repr() == PhpType::Mixed);
            Ok(())
        }
        PhpType::AssocArray { value, .. } => {
            super::arrays::values::emit_loaded_assoc_array_values(ctx, &value.codegen_repr())
        }
        PhpType::Iterable => emit_to_array_loaded_iterable(ctx, preserve_keys),
        PhpType::Object(_) => emit_to_array_loaded_traversable_object(ctx, preserve_keys),
        other => Err(CodegenIrError::unsupported(format!(
            "iterator_to_array for PHP type {:?}",
            other
        ))),
    }
}

/// Emits the dynamic preserve-keys branch and boxes both possible result containers as Mixed.
fn emit_dynamic_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
    preserve: ValueId,
    source_ty: &PhpType,
) -> Result<()> {
    let false_case = ctx.next_label("iterator_to_array_preserve_false");
    let done = ctx.next_label("iterator_to_array_preserve_done");

    ctx.load_value_to_result(source)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_preserve_keys_truthiness(ctx, preserve)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_case);

    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_to_array_loaded_source(ctx, source_ty, true)?;
    let true_ty = static_iterator_to_array_result_ty(source_ty, true);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &true_ty);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_to_array_loaded_source(ctx, source_ty, false)?;
    let false_ty = static_iterator_to_array_result_ty(source_ty, false);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &false_ty);

    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Returns a static boolean for literal preserve-keys operands when available.
fn static_preserve_keys_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<bool>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match (inst_ref.op, inst_ref.immediate.as_ref()) {
        (Op::ConstBool, Some(Immediate::Bool(value))) => Ok(Some(*value)),
        (Op::ConstI64, Some(Immediate::I64(value))) => Ok(Some(*value != 0)),
        (Op::ConstF64, Some(Immediate::F64(value))) => Ok(Some(*value != 0.0)),
        (Op::ConstNull, _) => Ok(Some(false)),
        (Op::ConstStr, Some(Immediate::Data(data))) => {
            let value = ctx
                .module
                .data
                .strings
                .get(data.as_raw() as usize)
                .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
            Ok(Some(!value.is_empty() && value != "0"))
        }
        _ => Ok(None),
    }
}

/// Materializes PHP truthiness for a dynamic preserve-keys operand.
fn emit_preserve_keys_truthiness(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    match ctx.raw_value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Union(_) | PhpType::Mixed => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "iterator_to_array preserve_keys PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Computes the concrete result container type for one preserve-keys branch.
fn static_iterator_to_array_result_ty(source_ty: &PhpType, preserve_keys: bool) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem_ty) => PhpType::Array(elem_ty),
        PhpType::AssocArray { key, value } if preserve_keys => PhpType::AssocArray { key, value },
        PhpType::AssocArray { value, .. } => PhpType::Array(value),
        _ if preserve_keys => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

/// Clones the loaded indexed array through the shared shallow-copy runtime helper.
fn emit_clone_loaded_array(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the loaded indexed array to the shallow-clone helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_clone_shallow");
}

/// Clones the loaded hash and optionally converts values to boxed Mixed cells.
fn emit_clone_loaded_hash(ctx: &mut FunctionContext<'_>, mixed_values: bool) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the loaded hash to the shallow-clone helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_clone_shallow");
    if !mixed_values {
        return;
    }
    emit_loaded_hash_as_mixed(ctx);
}

/// Converts the loaded indexed array payload to boxed Mixed slots.
fn emit_loaded_runtime_indexed_array_as_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before widening to Mixed slots
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the indexed-array value_type tag
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, QWORD PTR [rax - 8]");            // load indexed-array metadata before widening to Mixed slots
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the indexed-array value_type tag
            ctx.emitter.instruction("mov rdi, rax");                            // pass the loaded indexed array to the Mixed conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        }
    }
}

/// Converts the loaded hash payload to boxed Mixed values.
fn emit_loaded_hash_as_mixed(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the loaded hash to the Mixed-entry conversion helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
}

/// Allocates an indexed array whose slots store boxed Mixed values.
fn emit_new_mixed_indexed_array(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 16);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Mixed,
    );
}

/// Allocates an associative array whose values are boxed Mixed cells.
fn emit_new_mixed_hash(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 16);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Dispatches an `iterable` pointer to direct array/hash counts or object traversal.
fn emit_count_loaded_iterable(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let indexed_case = ctx.next_label("iterator_count_iterable_indexed");
    let hash_case = ctx.next_label("iterator_count_iterable_hash");
    let object_case = ctx.next_label("iterator_count_iterable_object");
    let done = ctx.next_label("iterator_count_iterable_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // is the iterable an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // count indexed-array entries directly
            ctx.emitter.instruction("cmp x0, #3");                              // is the iterable an associative hash?
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // count hash entries directly
            ctx.emitter.instruction("cmp x0, #4");                              // is the iterable an object?
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // count a Traversable object through Iterator dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // is the iterable an indexed array?
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // count indexed-array entries directly
            ctx.emitter.instruction("cmp rax, 3");                              // is the iterable an associative hash?
            ctx.emitter.instruction(&format!("je {}", hash_case));              // count hash entries directly
            ctx.emitter.instruction("cmp rax, 4");                              // is the iterable an object?
            ctx.emitter.instruction(&format!("je {}", object_case));            // count a Traversable object through Iterator dispatch
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&object_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_count_loaded_traversable_object(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_count_loaded_array(ctx);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_count_loaded_array(ctx);

    ctx.emitter.label(&done);
    Ok(())
}

/// Counts a loaded Traversable object by probing Iterator versus IteratorAggregate at runtime.
fn emit_count_loaded_traversable_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let direct_case = ctx.next_label("iterator_count_object_iterator");
    let aggregate_case = ctx.next_label("iterator_count_object_aggregate");
    let done = ctx.next_label("iterator_count_object_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_branch_if_saved_receiver_implements(ctx, "Iterator", &direct_case)?;
    emit_branch_if_saved_receiver_implements(ctx, "IteratorAggregate", &aggregate_case)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&direct_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_count_loaded_iterator_object(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&aggregate_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    move_result_to_receiver_arg(ctx);
    iterators::emit_interface_dispatch_call(ctx, "IteratorAggregate", "getiterator", None)?;
    emit_count_loaded_iterator_object(ctx)?;

    ctx.emitter.label(&done);
    Ok(())
}

/// Dispatches an `iterable` pointer to object traversal for `iterator_apply()`.
fn emit_apply_loaded_iterable(
    ctx: &mut FunctionContext<'_>,
    callback: &IteratorApplyCallback,
) -> Result<()> {
    let object_case = ctx.next_label("iterator_apply_iterable_object");
    let done = ctx.next_label("iterator_apply_iterable_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // is the iterable a Traversable object?
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // dispatch object iterables through Iterator protocol
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // is the iterable a Traversable object?
            ctx.emitter.instruction(&format!("je {}", object_case));            // dispatch object iterables through Iterator protocol
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&object_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_apply_loaded_traversable_object(ctx, callback)?;

    ctx.emitter.label(&done);
    Ok(())
}

/// Applies a callback to a loaded Traversable object by probing IteratorAggregate first.
fn emit_apply_loaded_traversable_object(
    ctx: &mut FunctionContext<'_>,
    callback: &IteratorApplyCallback,
) -> Result<()> {
    let direct_case = ctx.next_label("iterator_apply_object_iterator");
    let aggregate_case = ctx.next_label("iterator_apply_object_aggregate");
    let done = ctx.next_label("iterator_apply_object_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_branch_if_saved_receiver_implements(ctx, "Iterator", &direct_case)?;
    emit_branch_if_saved_receiver_implements(ctx, "IteratorAggregate", &aggregate_case)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&direct_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_apply_loaded_iterator_object(ctx, callback)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&aggregate_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    move_result_to_receiver_arg(ctx);
    iterators::emit_interface_dispatch_call(ctx, "IteratorAggregate", "getiterator", None)?;
    emit_apply_loaded_iterator_object(ctx, callback)?;

    ctx.emitter.label(&done);
    Ok(())
}

/// Drives Iterator and invokes the selected callback once for each valid position.
fn emit_apply_loaded_iterator_object(
    ctx: &mut FunctionContext<'_>,
    callback: &IteratorApplyCallback,
) -> Result<()> {
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let save_receiver = format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(ctx.emitter)
    );
    ctx.emitter.instruction(&save_receiver);                                    // preserve iterator_apply()'s receiver while initializing the count slot
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let restore_receiver = format!(
        "mov {}, {}",
        abi::int_result_reg(ctx.emitter),
        receiver_reg
    );
    ctx.emitter.instruction(&restore_receiver);                                 // restore iterator_apply()'s receiver for the Iterator loop

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "rewind", None)?;

    let loop_start = ctx.next_label("iterator_apply_start");
    let loop_end = ctx.next_label("iterator_apply_end");
    ctx.emitter.label(&loop_start);

    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "valid", None)?;
    emit_branch_if_invalid_iterator(ctx, &loop_end);

    emit_apply_callback_invocation(ctx, callback, &loop_end)?;

    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "next", None)?;
    abi::emit_jump(ctx.emitter, &loop_start);

    ctx.emitter.label(&loop_end);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Dispatches an `iterable` pointer for `iterator_to_array()`.
fn emit_to_array_loaded_iterable(
    ctx: &mut FunctionContext<'_>,
    preserve_keys: bool,
) -> Result<()> {
    let indexed_case = ctx.next_label("iterator_to_array_iterable_indexed");
    let hash_case = ctx.next_label("iterator_to_array_iterable_hash");
    let object_case = ctx.next_label("iterator_to_array_iterable_object");
    let done = ctx.next_label("iterator_to_array_iterable_done");

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // is the iterable an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // clone and widen the indexed-array payload
            ctx.emitter.instruction("cmp x0, #3");                              // is the iterable an associative hash?
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // materialize hash payload according to preserve_keys
            ctx.emitter.instruction("cmp x0, #4");                              // is the iterable an object?
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // collect a Traversable object through Iterator dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // is the iterable an indexed array?
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // clone and widen the indexed-array payload
            ctx.emitter.instruction("cmp rax, 3");                              // is the iterable an associative hash?
            ctx.emitter.instruction(&format!("je {}", hash_case));              // materialize hash payload according to preserve_keys
            ctx.emitter.instruction("cmp rax, 4");                              // is the iterable an object?
            ctx.emitter.instruction(&format!("je {}", object_case));            // collect a Traversable object through Iterator dispatch
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&object_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_to_array_loaded_traversable_object(ctx, preserve_keys)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if preserve_keys {
        emit_clone_loaded_hash(ctx, true);
    } else {
        super::arrays::values::emit_loaded_assoc_array_values(ctx, &PhpType::Mixed)?;
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&indexed_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_clone_loaded_array(ctx);
    emit_loaded_runtime_indexed_array_as_mixed(ctx);

    ctx.emitter.label(&done);
    Ok(())
}

/// Collects a loaded Traversable object into an array or hash result container.
fn emit_to_array_loaded_traversable_object(
    ctx: &mut FunctionContext<'_>,
    preserve_keys: bool,
) -> Result<()> {
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let direct_case = ctx.next_label("iterator_to_array_object_iterator");
    let aggregate_case = ctx.next_label("iterator_to_array_object_aggregate");
    let done = ctx.next_label("iterator_to_array_object_done");

    let save_receiver = format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(ctx.emitter)
    );
    ctx.emitter.instruction(&save_receiver);                                    // preserve iterator_to_array()'s receiver while allocating the result container
    if preserve_keys {
        emit_new_mixed_hash(ctx);
    } else {
        emit_new_mixed_indexed_array(ctx);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let restore_receiver = format!(
        "mov {}, {}",
        abi::int_result_reg(ctx.emitter),
        receiver_reg
    );
    ctx.emitter.instruction(&restore_receiver);                                 // restore iterator_to_array()'s receiver for Traversable probing

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_branch_if_saved_receiver_implements(ctx, "Iterator", &direct_case)?;
    emit_branch_if_saved_receiver_implements(ctx, "IteratorAggregate", &aggregate_case)?;
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&direct_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_to_array_loaded_iterator_object(ctx, preserve_keys)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&aggregate_case);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    move_result_to_receiver_arg(ctx);
    iterators::emit_interface_dispatch_call(ctx, "IteratorAggregate", "getiterator", None)?;
    emit_to_array_loaded_iterator_object(ctx, preserve_keys)?;

    ctx.emitter.label(&done);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Drives the Iterator protocol and appends each current value to the saved result container.
fn emit_to_array_loaded_iterator_object(
    ctx: &mut FunctionContext<'_>,
    preserve_keys: bool,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "rewind", None)?;

    let loop_start = ctx.next_label("iterator_to_array_start");
    let loop_end = ctx.next_label("iterator_to_array_end");
    ctx.emitter.label(&loop_start);

    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "valid", None)?;
    emit_branch_if_invalid_iterator(ctx, &loop_end);

    if preserve_keys {
        emit_insert_current_with_iterator_key(ctx)?;
    } else {
        emit_append_current_to_saved_array(ctx)?;
    }
    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "next", None)?;
    abi::emit_jump(ctx.emitter, &loop_start);

    ctx.emitter.label(&loop_end);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    Ok(())
}

/// Appends the current Iterator value to the saved indexed result array.
fn emit_append_current_to_saved_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    reload_saved_iterator_receiver(ctx);
    let current_ty = iterators::emit_interface_dispatch_call(ctx, "Iterator", "current", None)?;
    emit_box_current_value_as_mixed(ctx.emitter, &current_ty.codegen_repr());

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve boxed current() while loading the result array
            ctx.emitter.instruction("ldr x0, [sp, #32]");                       // load iterator_to_array()'s indexed result beneath receiver and value
            ctx.emitter.instruction("ldr x1, [sp], #16");                       // pass boxed current() as the appended Mixed payload
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            ctx.emitter.instruction("str x0, [sp, #16]");                       // save the possibly-grown result array beneath the receiver
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // load iterator_to_array()'s indexed result beneath receiver and value
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                // pass boxed current() as the appended Mixed payload
            ctx.emitter.instruction("add rsp, 16");                             // restore receiver to the top temporary slot before appending
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // save the possibly-grown result array beneath the receiver
        }
    }
    Ok(())
}

/// Inserts the current Iterator value into the saved hash using the normalized Iterator key.
fn emit_insert_current_with_iterator_key(ctx: &mut FunctionContext<'_>) -> Result<()> {
    reload_saved_iterator_receiver(ctx);
    let key_ty = iterators::emit_interface_dispatch_call(ctx, "Iterator", "key", None)?;
    emit_normalized_key_from_result(ctx, &key_ty.codegen_repr())?;
    let (key_lo, key_hi) = normalized_key_regs(ctx);
    abi::emit_push_reg_pair(ctx.emitter, key_lo, key_hi);

    reload_saved_iterator_receiver_at_offset(ctx, 16);
    let current_ty = iterators::emit_interface_dispatch_call(ctx, "Iterator", "current", None)?;
    emit_box_current_value_as_mixed(ctx.emitter, &current_ty.codegen_repr());

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass boxed current() as hash value_lo
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash values do not use value_hi
            ctx.emitter.instruction("mov x5, #7");                              // value tag 7 marks an owned boxed Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // load iterator_to_array()'s hash result beneath the receiver
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("str x0, [sp, #16]");                       // save the possibly-grown hash result beneath the receiver
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass boxed current() as hash value_lo
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash values do not use value_hi
            ctx.emitter.instruction("mov r9, 7");                               // value tag 7 marks an owned boxed Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // load iterator_to_array()'s hash result beneath the receiver
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // save the possibly-grown hash result beneath the receiver
        }
    }
    Ok(())
}

/// Counts a loaded Iterator object by driving rewind(), valid(), and next().
fn emit_count_loaded_iterator_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let save_receiver = format!(
        "mov {}, {}",
        receiver_reg,
        abi::int_result_reg(ctx.emitter)
    );
    ctx.emitter.instruction(&save_receiver);                                    // preserve iterator_count()'s receiver while initializing the count slot
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let restore_receiver = format!(
        "mov {}, {}",
        abi::int_result_reg(ctx.emitter),
        receiver_reg
    );
    ctx.emitter.instruction(&restore_receiver);                                 // restore iterator_count()'s receiver for the Iterator loop

    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "rewind", None)?;

    let loop_start = ctx.next_label("iterator_count_start");
    let loop_end = ctx.next_label("iterator_count_end");
    ctx.emitter.label(&loop_start);

    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "valid", None)?;
    emit_branch_if_invalid_iterator(ctx, &loop_end);

    emit_increment_saved_count(ctx);
    reload_saved_iterator_receiver(ctx);
    iterators::emit_interface_dispatch_call(ctx, "Iterator", "next", None)?;
    abi::emit_jump(ctx.emitter, &loop_start);

    ctx.emitter.label(&loop_end);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Returns the normalized key register pair for the active target.
fn normalized_key_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

/// Normalizes the current Iterator key result for insertion into an associative hash.
fn emit_normalized_key_from_result(
    ctx: &mut FunctionContext<'_>,
    key_ty: &PhpType,
) -> Result<()> {
    match key_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            emit_integer_key_from_result(ctx);
            Ok(())
        }
        PhpType::Float => {
            emit_float_key_from_result(ctx);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_key_from_result(ctx),
        _ => {
            emit_integer_key_from_result(ctx);
            Ok(())
        }
    }
}

/// Marks the current scalar key payload as an integer hash key.
fn emit_integer_key_from_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // use the scalar key payload as normalized key_lo
            ctx.emitter.instruction("mov x2, #-1");                             // key_hi sentinel marks an integer key
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, -1");                             // key_hi sentinel marks an integer key while rax stays key_lo
        }
    }
}

/// Casts the current float key payload to PHP's integer array-key form.
fn emit_float_key_from_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcvtzs x1, d0");                           // PHP casts float iterator keys to integer array keys
            ctx.emitter.instruction("mov x2, #-1");                             // key_hi sentinel marks an integer key
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // PHP casts float iterator keys to integer array keys
            ctx.emitter.instruction("mov rdx, -1");                             // key_hi sentinel marks an integer key
        }
    }
}

/// Unboxes a Mixed key and normalizes supported scalar payloads.
fn emit_mixed_key_from_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let string_label = ctx.next_label("iterator_key_string");
    let int_label = ctx.next_label("iterator_key_int");
    let bool_label = ctx.next_label("iterator_key_bool");
    let float_label = ctx.next_label("iterator_key_float");
    let null_label = ctx.next_label("iterator_key_null");
    let done_label = ctx.next_label("iterator_key_done");
    let (empty_label, _) = ctx.data.add_string(b"");

    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // is the mixed iterator key a string?
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // normalize string iterator keys through the hash key helper
            ctx.emitter.instruction("cmp x0, #0");                              // is the mixed iterator key an integer?
            ctx.emitter.instruction(&format!("b.eq {}", int_label));            // use integer payloads directly as array keys
            ctx.emitter.instruction("cmp x0, #3");                              // is the mixed iterator key a boolean?
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));           // use boolean payloads as integer array keys
            ctx.emitter.instruction("cmp x0, #2");                              // is the mixed iterator key a float?
            ctx.emitter.instruction(&format!("b.eq {}", float_label));          // cast float iterator keys to integer array keys
            ctx.emitter.instruction("cmp x0, #8");                              // is the mixed iterator key null?
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // PHP treats null array keys as the empty string
            ctx.emitter.instruction(&format!("b {}", int_label));               // unsupported key payloads fall back to their low word

            ctx.emitter.label(&string_label);
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip scalar-key normalization after string handling

            ctx.emitter.label(&int_label);
            ctx.emitter.instruction("mov x2, #-1");                             // mark the unboxed integer low word as an integer key
            ctx.emitter.instruction(&format!("b {}", done_label));              // finish normalized mixed-key handling

            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("mov x2, #-1");                             // mark the unboxed boolean low word as an integer key
            ctx.emitter.instruction(&format!("b {}", done_label));              // finish normalized mixed-key handling

            ctx.emitter.label(&float_label);
            ctx.emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed float payload bits for casting
            ctx.emitter.instruction("fcvtzs x1, d0");                           // PHP casts float array keys to integer keys
            ctx.emitter.instruction("mov x2, #-1");                             // mark the converted float payload as an integer key
            ctx.emitter.instruction(&format!("b {}", done_label));              // finish normalized mixed-key handling

            ctx.emitter.label(&null_label);
            abi::emit_symbol_address(ctx.emitter, "x1", &empty_label);
            ctx.emitter.instruction("mov x2, #0");                              // null iterator keys become the empty-string key
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // is the mixed iterator key a string?
            ctx.emitter.instruction(&format!("je {}", string_label));           // normalize string iterator keys through the hash key helper
            ctx.emitter.instruction("cmp rax, 0");                              // is the mixed iterator key an integer?
            ctx.emitter.instruction(&format!("je {}", int_label));              // use integer payloads directly as array keys
            ctx.emitter.instruction("cmp rax, 3");                              // is the mixed iterator key a boolean?
            ctx.emitter.instruction(&format!("je {}", bool_label));             // use boolean payloads as integer array keys
            ctx.emitter.instruction("cmp rax, 2");                              // is the mixed iterator key a float?
            ctx.emitter.instruction(&format!("je {}", float_label));            // cast float iterator keys to integer array keys
            ctx.emitter.instruction("cmp rax, 8");                              // is the mixed iterator key null?
            ctx.emitter.instruction(&format!("je {}", null_label));             // PHP treats null array keys as the empty string
            ctx.emitter.instruction(&format!("jmp {}", int_label));             // unsupported key payloads fall back to their low word

            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into hash-normalize key_lo
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip scalar-key normalization after string handling

            ctx.emitter.label(&int_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed integer low word into normalized key_lo
            ctx.emitter.instruction("mov rdx, -1");                             // mark the key as integer
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // finish normalized mixed-key handling

            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed boolean low word into normalized key_lo
            ctx.emitter.instruction("mov rdx, -1");                             // mark the key as integer
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // finish normalized mixed-key handling

            ctx.emitter.label(&float_label);
            ctx.emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed float payload bits for casting
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // PHP casts float array keys to integer keys
            ctx.emitter.instruction("mov rdx, -1");                             // mark the converted float payload as an integer key
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // finish normalized mixed-key handling

            ctx.emitter.label(&null_label);
            abi::emit_symbol_address(ctx.emitter, "rax", &empty_label);
            ctx.emitter.instruction("xor rdx, rdx");                            // null iterator keys become the empty-string key
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.label(&done_label);
        }
    }
    Ok(())
}

/// Branches when the saved Traversable receiver implements `interface_name`.
fn emit_branch_if_saved_receiver_implements(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    target_label: &str,
) -> Result<()> {
    let interface_id = ctx
        .module
        .interface_infos
        .get(interface_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("missing interface {}", interface_name)))?
        .interface_id as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // load the saved Traversable object for interface probing
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("cmp x0, #0");                              // did the runtime interface matcher succeed?
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // branch to the matching iterator_count object path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // load the saved Traversable object for interface probing
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("test rax, rax");                           // did the runtime interface matcher succeed?
            ctx.emitter.instruction(&format!("jne {}", target_label));          // branch to the matching iterator_count object path
        }
    }
    Ok(())
}

/// Moves the loaded object result into the ABI receiver register for method dispatch.
fn move_result_to_receiver_arg(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the object result as the interface method receiver
    }
}

/// Reloads the saved iterator receiver from the top temporary stack slot.
fn reload_saved_iterator_receiver(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload iterator receiver before the next protocol call
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload iterator receiver before the next protocol call
        }
    }
}

/// Reloads the saved iterator receiver from a non-top temporary stack slot.
fn reload_saved_iterator_receiver_at_offset(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", offset));     // reload iterator receiver from below preserved key state
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", offset)); // reload iterator receiver from below preserved key state
        }
    }
}

/// Branches out of the count loop when `valid()` returns false.
fn emit_branch_if_invalid_iterator(ctx: &mut FunctionContext<'_>, loop_end: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // valid() false means the iterator is exhausted
            ctx.emitter.instruction(&format!("b.eq {}", loop_end));             // exit iterator_count() loop when exhausted
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // valid() false means the iterator is exhausted
            ctx.emitter.instruction(&format!("je {}", loop_end));               // exit iterator_count() loop when exhausted
        }
    }
}

/// Increments the saved iterator_count counter beneath the receiver slot.
fn emit_increment_saved_count(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // load iterator_count()'s counter beneath the receiver slot
            ctx.emitter.instruction("add x9, x9, #1");                          // count this valid iterator position
            ctx.emitter.instruction("str x9, [sp, #16]");                       // persist the updated iterator_count() counter
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add QWORD PTR [rsp + 16], 1");             // count this valid iterator position beneath the receiver slot
        }
    }
}

/// Invokes the callback selected for the current `iterator_apply()` iteration.
fn emit_apply_callback_invocation(
    ctx: &mut FunctionContext<'_>,
    callback: &IteratorApplyCallback,
    loop_end: &str,
) -> Result<()> {
    match callback {
        IteratorApplyCallback::StaticUserFunction {
            label,
            return_ty,
            args,
            param_types,
        } => emit_static_apply_callback_invocation(ctx, label, return_ty, args, param_types, loop_end),
        IteratorApplyCallback::DynamicString { targets, .. } => {
            emit_dynamic_string_apply_callback_invocation(ctx, targets, loop_end)
        }
        IteratorApplyCallback::DescriptorCallable { arg_container, .. } => {
            emit_descriptor_apply_callback_invocation(ctx, *arg_container, loop_end)
        }
        IteratorApplyCallback::DescriptorString { arg_container, .. } => {
            emit_descriptor_apply_callback_invocation(ctx, *arg_container, loop_end)
        }
        IteratorApplyCallback::DescriptorCallableArray { arg_container, .. } => {
            emit_descriptor_apply_callback_invocation(ctx, *arg_container, loop_end)
        }
    }
}

/// Materializes static callback args and invokes the resolved user function.
fn emit_static_apply_callback_invocation(
    ctx: &mut FunctionContext<'_>,
    label: &str,
    return_ty: &PhpType,
    args: &[ValueId],
    param_types: &[PhpType],
    loop_end: &str,
) -> Result<()> {
    let overflow_bytes = materialize_direct_call_args(ctx, args, param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, label);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    emit_increment_saved_apply_count(ctx);
    emit_branch_if_callback_result_false(ctx, return_ty, loop_end)
}

/// Invokes a saved callable descriptor and tests its boxed Mixed result for truthiness.
fn emit_descriptor_apply_callback_invocation(
    ctx: &mut FunctionContext<'_>,
    arg_container: Option<ValueId>,
    loop_end: &str,
) -> Result<()> {
    let descriptor_reg = abi::nested_call_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, descriptor_reg, APPLY_DESCRIPTOR_OFFSET);
    if let Some(arg_container) = arg_container {
        callables::emit_descriptor_reg_invoker_mixed_result_with_arg_container(
            ctx,
            descriptor_reg,
            arg_container,
            "iterator_apply",
            false,
        )?;
    } else {
        callables::emit_descriptor_reg_invoker_mixed_result_with_args(
            ctx,
            descriptor_reg,
            &[],
            "iterator_apply",
            false,
        )?;
    }
    emit_increment_saved_apply_count(ctx);
    emit_branch_if_callback_result_false(ctx, &PhpType::Mixed, loop_end)
}

/// Dispatches a saved runtime string callback to a zero-argument user function.
fn emit_dynamic_string_apply_callback_invocation(
    ctx: &mut FunctionContext<'_>,
    targets: &[IteratorApplyCallbackTarget],
    loop_end: &str,
) -> Result<()> {
    let done_label = ctx.next_label("iterator_apply_dynamic_callback_done");
    let miss_label = ctx.next_label("iterator_apply_dynamic_callback_missing");
    let mut case_labels = Vec::with_capacity(targets.len());
    for target in targets {
        let label = ctx.next_label(&format!(
            "iterator_apply_dynamic_callback_{}",
            label_fragment(&target.name)
        ));
        emit_branch_if_saved_apply_callback_name_matches(ctx, &target.name, &label);
        case_labels.push(label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (target, label) in targets.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_call_label(ctx.emitter, &target.label);
        emit_increment_saved_apply_count(ctx);
        emit_branch_if_callback_result_false(ctx, &target.return_ty, loop_end)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    emit_dynamic_string_apply_callback_abort(ctx);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches when the saved runtime callback string matches a candidate PHP function name.
fn emit_branch_if_saved_apply_callback_name_matches(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    matched_label: &str,
) {
    emit_apply_callback_name_compare(ctx, name.as_bytes(), matched_label);
    let trimmed = name.trim_start_matches('\\');
    if trimmed == name {
        let qualified = format!("\\{}", name);
        emit_apply_callback_name_compare(ctx, qualified.as_bytes(), matched_label);
    }
}

/// Converts PHP function names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Emits a case-insensitive compare against the saved `iterator_apply()` callback name.
fn emit_apply_callback_name_compare(
    ctx: &mut FunctionContext<'_>,
    candidate: &[u8],
    matched_label: &str,
) {
    let (candidate_label, candidate_len) = ctx.data.add_string(candidate);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x1",
                APPLY_DYNAMIC_CALLBACK_PTR_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x2",
                APPLY_DYNAMIC_CALLBACK_LEN_OFFSET,
            );
            abi::emit_symbol_address(ctx.emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // did the saved iterator_apply callback name match this function?
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // dispatch to this callback when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdi",
                APPLY_DYNAMIC_CALLBACK_PTR_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rsi",
                APPLY_DYNAMIC_CALLBACK_LEN_OFFSET,
            );
            abi::emit_symbol_address(ctx.emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // did the saved iterator_apply callback name match this function?
            ctx.emitter.instruction(&format!("je {}", matched_label));          // dispatch to this callback when names match case-insensitively
        }
    }
}

/// Increments the saved iterator_apply callback-invocation count beneath the receiver slot.
fn emit_increment_saved_apply_count(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // load iterator_apply()'s callback count beneath the receiver slot
            ctx.emitter.instruction("add x9, x9, #1");                          // count this callback invocation before testing its result
            ctx.emitter.instruction("str x9, [sp, #16]");                       // persist the updated iterator_apply() callback count
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add QWORD PTR [rsp + 16], 1");             // count this callback invocation beneath the receiver slot
        }
    }
}

/// Emits a fatal diagnostic when a dynamic callback string cannot select a supported function.
fn emit_dynamic_string_apply_callback_abort(ctx: &mut FunctionContext<'_>) {
    let message =
        b"Fatal error: iterator_apply callback string does not name a supported callable\n";
    let (message_label, message_len) = ctx.data.add_string(message);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the unresolved iterator_apply callback diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);                             // load the iterator_apply callback diagnostic page
            ctx.emitter.add_lo12("x1", "x1", &message_label);                  // resolve the iterator_apply callback diagnostic address
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the iterator_apply callback diagnostic byte length to write
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the unresolved iterator_apply callback diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the iterator_apply callback diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the callback diagnostic before terminating
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}

/// Branches to `loop_end` when the just-returned iterator_apply callback result is falsy.
fn emit_branch_if_callback_result_false(
    ctx: &mut FunctionContext<'_>,
    callback_return_ty: &PhpType,
    loop_end: &str,
) -> Result<()> {
    match callback_return_ty.codegen_repr() {
        PhpType::Bool | PhpType::Int => {}
        PhpType::Float => predicates::emit_float_result_nonzero_bool(ctx),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "iterator_apply callback return PHP type {:?}",
                other
            )))
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the iterator_apply callback request iteration stop?
            ctx.emitter.instruction(&format!("b.eq {}", loop_end));             // stop before next() when the callback result is falsy
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the iterator_apply callback request iteration stop?
            ctx.emitter.instruction(&format!("je {}", loop_end));               // stop before next() when the callback result is falsy
        }
    }
    Ok(())
}

/// Evaluates lowered operands in source order and discards their results.
fn emit_args_for_side_effects(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    for operand in &inst.operands {
        ctx.load_value_to_result(*operand)?;
    }
    Ok(())
}

/// Emits the shared runtime null sentinel into the integer result register.
fn emit_null_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        NULL_SENTINEL,
    );
}

/// Allocates an indexed integer array and lets `fill` append values.
fn emit_int_array<F>(
    ctx: &mut FunctionContext<'_>,
    capacity: usize,
    fill: F,
) -> Result<()>
where
    F: FnOnce(&mut FunctionContext<'_>) -> Result<()>,
{
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    fill(ctx)
}

/// Appends placeholder rule indexes to the current autoload-functions array.
fn emit_autoload_function_placeholders(
    ctx: &mut FunctionContext<'_>,
    rule_count: usize,
) -> Result<()> {
    if rule_count == 0 {
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_autoload_function_placeholders_aarch64(ctx, rule_count),
        Arch::X86_64 => emit_autoload_function_placeholders_x86_64(ctx, rule_count),
    }
    Ok(())
}

/// Appends placeholder autoload indexes on AArch64.
fn emit_autoload_function_placeholders_aarch64(
    ctx: &mut FunctionContext<'_>,
    rule_count: usize,
) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the autoload-functions array while appending rule indexes
    for index in 0..rule_count {
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the autoload-functions array for this append
        abi::emit_load_int_immediate(ctx.emitter, "x1", index as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown array pointer for the next append
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final autoload-functions array as the result
}

/// Appends placeholder autoload indexes on x86_64.
fn emit_autoload_function_placeholders_x86_64(
    ctx: &mut FunctionContext<'_>,
    rule_count: usize,
) {
    ctx.emitter.instruction("push rax");                                        // park the autoload-functions array while appending rule indexes
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for index in 0..rule_count {
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the autoload-functions array for this append
        abi::emit_load_int_immediate(ctx.emitter, "rsi", index as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown array pointer for the next append
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final autoload-functions array as the result
}

/// Loads the current autoload extension string from runtime globals.
fn emit_extensions_read(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, ptr_reg, EXTS_PTR_SYMBOL, 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, len_reg, EXTS_LEN_SYMBOL, 0);
}

/// Writes a new autoload extension string and returns the previous value.
fn emit_extensions_write(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_extensions_read(ctx);
    let new_ptr = abi::secondary_scratch_reg(ctx.emitter);
    let new_len = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg_pair(ctx.emitter, new_ptr, new_len);
    abi::emit_store_reg_to_symbol(ctx.emitter, new_ptr, EXTS_PTR_SYMBOL, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, new_len, EXTS_LEN_SYMBOL, 0);
    Ok(())
}

/// Allocates an indexed string array and appends all static names.
fn emit_string_array(ctx: &mut FunctionContext<'_>, names: &[&str]) -> Result<()> {
    let capacity = names.len().max(1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends static string names to the current result array on AArch64.
fn emit_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the string array while appending names
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown string array for the next append
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final string array as the result
}

/// Appends static string names to the current result array on x86_64.
fn emit_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("push rax");                                        // park the string array while appending names
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown string array for the next append
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final string array as the result
}
