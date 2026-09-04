//! Purpose:
//! Aggregate helpers and callback-aware sorting.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;

/// Loads an indexed array argument and calls the selected runtime aggregate helper.
///
/// HASH storage is aggregated through the same helpers: php's `array_sum()`/`array_product()`
/// walk the values with `ZEND_HASH_FOREACH_VAL` (ext/standard/array.c) and never read a key, so
/// a hash aggregates exactly like the indexed array of its values.
pub(super) fn lower_indexed_array_aggregate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    scalar_helper: &str,
    mixed_helper: Option<&str>,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    let array_ty = ctx.value_php_type(array)?;
    if let PhpType::AssocArray { value, .. } = array_ty.codegen_repr() {
        return lower_hash_aggregate_via_values(
            ctx,
            inst,
            name,
            scalar_helper,
            mixed_helper,
            array,
            &array_ty.codegen_repr(),
            &value.codegen_repr(),
        );
    }
    let helper = match array_ty.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => mixed_helper
            .ok_or_else(|| {
                CodegenIrError::unsupported(format!(
                    "{} for PHP type {:?}",
                    name,
                    array_ty.codegen_repr()
                ))
            })?,
        _ => {
            require_supported_indexed_array(array_ty, name)?;
            scalar_helper
        }
    };
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer as the runtime helper argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Aggregates HASH storage by materializing its values and reusing the packed helper.
///
/// The values are copied into a temporary indexed array — the route `implode()` already takes
/// for hash storage — so the per-element rules stay in the one packed helper instead of being
/// restated for the hash layout, and both architectures are covered without a second assembly
/// body. The temporary is owned HERE: it holds the same payload pointers the hash entries
/// carry, so it is deep-freed around the aggregate's integer result, which the free helper's
/// own argument register (`x0`/`rax`, NOT `rdi`) would otherwise destroy.
///
/// The supported value types are exactly the packed ones, and an unsupported value keeps the
/// refusal worded with the ORIGINAL hash type: reporting the materialized `Array(Mixed)`
/// instead would name a type the source never wrote.
#[allow(clippy::too_many_arguments)]
fn lower_hash_aggregate_via_values(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    scalar_helper: &str,
    mixed_helper: Option<&str>,
    array: ValueId,
    array_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let unsupported =
        || CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, array_ty));
    let helper = match value_ty {
        PhpType::Mixed => mixed_helper.ok_or_else(unsupported)?,
        PhpType::Int | PhpType::Bool | PhpType::Never | PhpType::Void => scalar_helper,
        _ => return Err(unsupported()),
    };
    ctx.load_value_to_result(array)?;
    values::emit_loaded_assoc_array_values(ctx, value_ty)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);                                // preserve the temporary values array across the aggregate call
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the materialized values array as the runtime helper argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_push_reg(ctx.emitter, result_reg);                                // preserve the aggregate result across the deep free
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx
            .emitter
            .instruction(&format!("ldr {}, [sp, #16]", result_reg)),             // reload the temporary values array pointer
        Arch::X86_64 => ctx
            .emitter
            .instruction(&format!("mov {}, QWORD PTR [rsp + 16]", result_reg)),  // reload the temporary values array pointer
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_free_deep");
    abi::emit_pop_reg(ctx.emitter, result_reg);                                 // restore the aggregate result
    abi::emit_release_temporary_stack(ctx.emitter, 16);                         // drop the temporary values array slot
    store_if_result(ctx, inst)
}

/// Calls a key-preserving value set-operation helper after validating indexed-array layouts.
///
/// php keeps the FIRST operand's keys: `array_diff(["a","b","c"], ["b"])` is `{0:"a", 2:"c"}` and
/// `array_intersect(["a","b","c"], ["b","c"])` is `{1:"b", 2:"c"}`. A dense indexed array cannot
/// hold a gap, so both lower to `__rt_array_*_to_hash`, which inserts each survivor at its
/// ORIGINAL index. The helper reads the element layout from the source header at runtime, so the
/// scalar, refcounted and string element types share one entry point instead of the three-way
/// helper split the reindexing form needed. The result carries the hash's own header, so the
/// indexed-array value_type stamp the dense form applied would be meaningless here.
pub(super) fn lower_indexed_array_set_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    hash_helper: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let first_elem_ty = set_op_indexed_array_element_type(ctx.value_php_type(first)?, name, true)?;
    let second_elem_ty = set_op_indexed_array_element_type(ctx.value_php_type(second)?, name, true)?;
    require_set_op_compatible_element_types(name, &first_elem_ty, &second_elem_ty)?;
    require_set_op_result_type(name, &first_elem_ty, &inst.result_php_type.codegen_repr())?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, hash_helper);
    store_if_result(ctx, inst)
}

/// Calls a key set-operation helper after validating associative-array hash operands.
pub(super) fn lower_assoc_array_key_set_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let first_ty = assoc_array_key_set_operand_type(ctx.value_php_type(first)?, name, "first")?;
    let _second_ty = assoc_array_key_set_operand_type(ctx.value_php_type(second)?, name, "second")?;
    require_assoc_array_key_set_result_type(name, &first_ty, &inst.result_php_type.codegen_repr())?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Sorts a hash receiver by value and republishes it re-keyed `0..n-1`, the way `sort()` does.
///
/// `sort()` and `rsort()` DISCARD keys: PHP renumbers the result from zero, which is what makes
/// `sort(array_unique($xs))` — a hash, since `array_unique` preserves the original keys — an
/// everyday idiom. The backend used to refuse the whole call (`sort for PHP type AssocArray`),
/// so that idiom did not compile at all.
///
/// Renumbering a hash in place is not available: an entry's position is `hash(key) % capacity`
/// under linear probing, so rewriting a key without moving its entry would make it unfindable.
/// The receiver is therefore rebuilt from three steps that already exist and already agree on
/// ownership — take the values into a fresh list, sort the list, turn the list back into a hash
/// keyed `0..n-1` — rather than a new sorter that would have to redo bucket placement itself.
///
/// The receiver keeps its `AssocArray` type across the call. A hash whose keys are exactly
/// `0..n-1` in iteration order holds what PHP's own result holds, so nothing downstream has to
/// know that a sort happened.
pub(super) fn lower_hash_reindexing_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    int_helper: &str,
    str_helper: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    let PhpType::AssocArray { value, .. } = ctx.value_php_type(array)?.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            ctx.value_php_type(array)?
        )));
    };
    let value_ty = value.codegen_repr();
    let helper = match value_ty {
        PhpType::Str => str_helper,
        PhpType::Int => int_helper,
        ref other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for hash values of PHP type {:?}",
                name, other
            )))
        }
    };

    let receiver = ReceiverPlace::resolve(ctx, array)?;
    receiver.require_writable(name)?;
    // Same opening as the key-preserving hash sorts: drop the slot's ownership, split a shared
    // table so a copy taken before the call keeps its order, and publish the split pointer.
    if let Some(slot) = receiver.slot() {
        ctx.release_mutated_source_local_owner(slot, array)?;
    }
    ensure_unique_hash_sort_source(ctx, array)?;
    receiver.store_back_value(ctx, array)?;

    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);

    // Park the table being replaced; it is released once its values have been copied out.
    ctx.load_value_to_result(array)?;
    abi::emit_push_reg(ctx.emitter, result_reg);
    super::values::emit_loaded_assoc_array_values(ctx, &value_ty)?;

    // Sort the fresh list in place. The sorters answer nothing useful and clobber the argument
    // registers, so the list pointer is parked rather than read back afterwards.
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_reg_move(ctx.emitter, arg0, result_reg);
    abi::emit_call_label(ctx.emitter, helper);

    // Rebuild a hash keyed 0..n-1 out of the sorted list. `__rt_array_slice_to_hash` reads
    // (array, offset, length, length_present), and a zero flag means "to the end", so the length
    // argument is ignored. The list is re-read from the stack because the sort call clobbered it.
    abi::emit_pop_reg(ctx.emitter, arg0);
    abi::emit_push_reg(ctx.emitter, arg0);
    for index in 1..=3 {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, index),
            0,
        );
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_to_hash");

    // Publish the rebuilt table as the receiver's value, then release the two intermediates in
    // the order they were stacked: the sorted list, then the table its values came from.
    ctx.store_result_value(array)?;
    receiver.store_back_value(ctx, array)?;
    // Both release helpers read their pointer from the RESULT register, not the first argument
    // register — `x0` on ARM64, `rax` on x86_64. Passing it in `arg0` happened to work on ARM64,
    // where the two are the same register, and released a garbage pointer on x86_64.
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_array");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_hash");

    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Calls a mutating indexed-array sort helper after copy-on-write splitting.
pub(super) fn lower_indexed_array_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    int_helper: &str,
    str_helper: Option<&str>,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    let operand_ty = ctx.value_php_type(array)?;
    // An `array|false` union — `$d = scandir($dir); sort($d);` — arrives BOXED. The payload is
    // sorted in place, so the box keeps pointing at the sorted storage and no write-back is
    // needed; a runtime `false` throws php's TypeError, worded exactly as php words it. The
    // copy-on-write split is skipped: the box owns its array, so the sort mutates the only
    // storage the value has, which is also why `ensure_unique_sort_source` has nothing to do.
    if let Some(member) = operand_ty.array_or_false_member().cloned() {
        let elem_ty = indexed_sort_element_type(member, name, str_helper.is_some())?;
        ctx.load_value_to_result(array)?;
        let sortable = ctx.next_label(&format!("{}_union_array", name));
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x9, [x0]");                        // the boxed payload tag
                ctx.emitter.instruction("cmp x9, #4");                          // an indexed array?
                ctx.emitter.instruction(&format!("b.eq {}", sortable));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov r9, QWORD PTR [rax]");             // the boxed payload tag
                ctx.emitter.instruction("cmp r9, 4");                           // an indexed array?
                ctx.emitter.instruction(&format!("je {}", sortable));
            }
        }
        let message = format!(
            "{}(): Argument #1 ($array) must be of type array, false given",
            name
        );
        let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_symbol_address(ctx.emitter, ptr_reg, &message_label);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, message_len as i64);
        super::super::exceptions::emit_type_error_from_string_result(ctx);
        ctx.emitter.label(&sortable);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x0, [x0, #8]");                    // the raw array the box owns
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");        // the raw array the box owns
                ctx.emitter.instruction("mov rdi, rax");                        // the sort helpers take rdi
            }
        }
        if elem_ty == PhpType::Mixed {
            emit_mixed_element_sort(ctx, name);
        } else {
            let helper = if elem_ty == PhpType::Str {
                str_helper.expect("string sort helper is required after validation")
            } else {
                int_helper
            };
            abi::emit_call_label(ctx.emitter, helper);
        }
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
        return store_if_result(ctx, inst);
    }
    let elem_ty = indexed_sort_element_type(operand_ty, name, str_helper.is_some())?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    receiver.store_back_value(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    if elem_ty == PhpType::Mixed {
        emit_mixed_element_sort(ctx, name);
    } else {
        let helper = if elem_ty == PhpType::Str {
            str_helper.expect("string sort helper is required after validation")
        } else {
            int_helper
        };
        abi::emit_call_label(ctx.emitter, helper);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Sorts an indexed array of boxed `Mixed` cells through `__rt_usort` and a built-in comparator.
///
/// The array pointer is already in the first argument register, where the non-Mixed helpers want
/// it; `__rt_usort` wants it SECOND, after the comparator address, so the two are placed in that
/// order here. The capture environment is zero, which selects `__rt_usort`'s two-argument
/// comparator ABI — the comparator takes a pair of cells and nothing else.
fn emit_mixed_element_sort(ctx: &mut FunctionContext<'_>, name: &str) {
    let comparator = if name == "rsort" {
        "__rt_sort_mixed_desc"
    } else {
        "__rt_sort_mixed_asc"
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // the array becomes __rt_usort's second argument
            abi::emit_symbol_address(ctx.emitter, "x0", comparator);
            ctx.emitter.instruction("mov x2, #0");                              // no capture environment: the two-argument ABI
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rdi");                            // the array becomes __rt_usort's second argument
            abi::emit_symbol_address(ctx.emitter, "rdi", comparator);
            ctx.emitter.instruction("xor edx, edx");                            // no capture environment: the two-argument ABI
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_usort");
}

/// Calls the mutating shuffle helper for indexed arrays whose payload slots are pointer-sized.
pub(super) fn lower_indexed_array_shuffle(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "shuffle", 1)?;
    let array = expect_operand(inst, 0)?;
    // String arrays store 16-byte (ptr, len) slots the 8-byte swap would tear in half,
    // pairing one string's pointer with another's length; they get their own helper.
    let helper = match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => "__rt_shuffle_str",
        ty => {
            eight_byte_indexed_array_element_type(ty, "shuffle")?;
            "__rt_shuffle"
        }
    };
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    receiver.store_back_value(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Calls the user-sort helper with a static comparator and optional late-static environment.
pub(super) fn lower_user_sort_static_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let elem_ty = user_sort_element_type(ctx.value_php_type(array)?, name)?;
    let sort_helper = user_sort_runtime_label(&elem_ty);
    let callback_arg_types = [elem_ty.clone(), elem_ty];
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    ensure_unique_sort_source(ctx, array)?;
    receiver.store_back_value(ctx, array)?;
    let callback_ty = ctx.value_php_type(callback)?.codegen_repr();
    let callback_owner = format!("{} callback", name);
    if callback_ty == PhpType::Callable && static_callback_operand_is_recoverable(ctx, callback) {
        let callback_binding = static_sort_callback_binding(
            ctx,
            callback,
            &callback_owner,
            Some(&callback_arg_types),
        )?;
        return lower_user_sort_with_static_callback_binding(
            ctx,
            inst,
            array,
            callback_binding,
            sort_helper,
        );
    }
    match callback_ty {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                callback_arg_types.to_vec(),
                PhpType::Int,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, sort_helper);
                    Ok(())
                },
            )?;
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(callback_arg_types[0].clone()))),
                callback_arg_types.to_vec(),
                PhpType::Int,
                super::super::instruction_strict_php_profile(inst),
                name,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, sort_helper);
                    Ok(())
                },
            )?;
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            store_if_result(ctx, inst)?;
            return Ok(());
        }
        _ => {}
    }
    let callback_binding = static_sort_callback_binding(
        ctx,
        callback,
        &callback_owner,
        Some(&callback_arg_types),
    )?;
    lower_user_sort_with_static_callback_binding(ctx, inst, array, callback_binding, sort_helper)
}

/// Calls the user-sort runtime with a statically recovered callback binding.
///
/// `sort_helper` selects the slot permuter matching the receiver's element
/// width: `__rt_usort` for 8-byte payload slots, `__rt_usort_str` for the
/// 16-byte `[ptr][len]` string descriptors.
pub(super) fn lower_user_sort_with_static_callback_binding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    callback_binding: StaticSortCallbackBinding,
    sort_helper: &str,
) -> Result<()> {
    let callback_label = sort_callback_label_returning_int(ctx, &callback_binding)?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, sort_helper);
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Returns a callback label whose runtime ABI produces an integer comparison result.
pub(super) fn sort_callback_label_returning_int(
    ctx: &mut FunctionContext<'_>,
    callback_binding: &StaticSortCallbackBinding,
) -> Result<String> {
    match callback_binding.return_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(callback_binding.label.clone()),
        PhpType::Mixed | PhpType::Union(_) => {
            Ok(emit_sort_callback_mixed_return_int_adapter(ctx, &callback_binding.label))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "user sort callback return PHP type {:?}",
            other
        ))),
    }
}

/// Emits a sort callback adapter that casts an owned Mixed return value to int.
pub(super) fn emit_sort_callback_mixed_return_int_adapter(
    ctx: &mut FunctionContext<'_>,
    inner_label: &str,
) -> String {
    let wrapper_label = ctx.next_label("sort_callback_mixed_return_int");
    let done_label = ctx.next_label("sort_callback_after_mixed_return_int");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve wrapper spill space for the runtime sort return address
            ctx.emitter.instruction("str x30, [sp, #8]");                       // preserve the runtime sort return address across nested callback work
            abi::emit_call_label(ctx.emitter, inner_label);
            emit_owned_mixed_result_cast_to_int(ctx);
            ctx.emitter.instruction("ldr x30, [sp, #8]");                       // restore the runtime sort return address after result coercion
            ctx.emitter.instruction("add sp, sp, #16");                         // release wrapper spill space before returning to the sort helper
            ctx.emitter.instruction("ret");                                     // return the integer comparator result to the sort helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the runtime sort frame pointer across nested callback work
            ctx.emitter.instruction("mov rbp, rsp");                            // establish an aligned frame for nested runtime calls
            abi::emit_call_label(ctx.emitter, inner_label);
            emit_owned_mixed_result_cast_to_int(ctx);
            ctx.emitter.instruction("pop rbp");                                 // restore the runtime sort frame pointer before returning
            ctx.emitter.instruction("ret");                                     // return the integer comparator result to the sort helper
        }
    }
    ctx.emitter.label(&done_label);
    wrapper_label
}

/// Casts the current owned Mixed result to int and releases the consumed Mixed cell.
pub(super) fn emit_owned_mixed_result_cast_to_int(ctx: &mut FunctionContext<'_>) {
    move_sort_callback_int_result_to_first_arg(ctx);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_push_reg(ctx.emitter, arg_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #16]");                       // save the coerced integer above the saved Mixed pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // save the coerced integer above the saved Mixed pointer
        }
    }
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(ctx.emitter, result_reg);
}

/// Moves the integer result register into the first argument register when required.
pub(super) fn move_sort_callback_int_result_to_first_arg(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if result_reg == arg_reg {
        return;
    }
    ctx.emitter.instruction(&format!("mov {}, {}", arg_reg, result_reg));       // move the callback result into the runtime cast argument register
}

/// Calls the key-sort helper for array-like values.
/// Direction of a PHP key sort (`ksort` versus `krsort`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum KeySortOrder {
    /// Ascending key order, as produced by `ksort()`.
    Ascending,
    /// Descending key order, as produced by `krsort()`.
    Descending,
}

/// Lowers `ksort()` / `krsort()` for every receiver shape the backend can represent.
///
/// Hash-backed associative arrays are reordered by `__rt_hash_ksort` / `__rt_hash_krsort`,
/// which relink the table's insertion-order chain and therefore keep each key attached to
/// its own value. An indexed array stores its keys implicitly as slot positions `0..n-1`,
/// which are already in ascending key order, so `ksort()` on one is a genuine no-op, as is
/// either sort over a statically empty indexed array. EIR argument lowering promotes a
/// non-empty packed receiver to an integer-keyed hash before `krsort()` reaches this layer.
pub(super) fn lower_array_key_sort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    order: KeySortOrder,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::AssocArray { .. } => {
            let helper = match order {
                KeySortOrder::Ascending => "__rt_hash_ksort",
                KeySortOrder::Descending => "__rt_hash_krsort",
            };
            lower_hash_link_sort(ctx, inst, helper)
        }
        PhpType::Array(elem)
            if order == KeySortOrder::Ascending
                || matches!(elem.codegen_repr(), PhpType::Never | PhpType::Void) =>
        {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                1,
            );
            store_if_result(ctx, inst)
        }
        PhpType::Array(elem) => Err(CodegenIrError::unsupported(format!(
            "{} for indexed array<{:?}>: an indexed array stores its keys as slot \
             positions 0..n-1, so descending key order has no representation; convert the \
             receiver to an associative array (for example with array_reverse($a, true)) \
             before sorting it by key",
            name, elem
        ))),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the indexed-array element type accepted by the selected sort helper.
pub(super) fn indexed_sort_element_type(ty: PhpType, name: &str, allow_strings: bool) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            // `Mixed` rides the same route as a user comparator: an 8-byte boxed cell per slot,
            // ordered by `__rt_php_compare`. Only the sorts that take a comparator can carry it,
            // which is the same condition `allow_strings` already expresses — `asort`/`arsort`
            // permute slots with no comparator at all.
            if matches!(elem, PhpType::Int | PhpType::Void | PhpType::Never)
                || (allow_strings && matches!(elem, PhpType::Str | PhpType::Mixed))
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name, elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the indexed-array element type accepted by a user-comparator sort.
///
/// User-comparator sorts (`usort`/`uasort`/`uksort`) permute existing
/// pointer-sized slots through `__rt_usort`, so integer and object/refcounted
/// handles and boxed `Mixed` cells (each a single 8-byte payload) are sortable;
/// the comparator decides the ordering and receives each element through an ABI
/// adapter when the runtime slot type differs from its declared parameters.
/// String elements are rejected here exactly as before — their multi-word
/// descriptors are not permuted by the 8-byte slot sorter — so they keep
/// producing a clear unsupported-feature error rather than a corrupt sort.
/// String elements are 16-byte `[ptr][len]` descriptors, so they are routed to
/// the dedicated `__rt_usort_str` slot permuter instead; only `usort` accepts
/// them because it is the sort that renumbers keys, which an indexed array
/// without key storage can represent exactly.
pub(super) fn user_sort_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Void
                    | PhpType::Never
                    | PhpType::Mixed
                    | PhpType::Object(_)
            ) || (elem == PhpType::Str && name == "usort")
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name, elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}


/// Returns the user-sort runtime helper matching an indexed array's slot width.
///
/// String elements occupy 16-byte `[ptr][len]` slots and must be permuted by
/// `__rt_usort_str`; every other supported element kind is a single 8-byte
/// payload handled by `__rt_usort`.
fn user_sort_runtime_label(elem_ty: &PhpType) -> &'static str {
    if elem_ty.codegen_repr() == PhpType::Str {
        "__rt_usort_str"
    } else {
        "__rt_usort"
    }
}

/// Splits a shared indexed array before a sort helper mutates its slots in place.
pub(super) fn ensure_unique_sort_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Splits a shared hash table before a sort helper relinks its iteration order in place.
pub(super) fn ensure_unique_hash_sort_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_ensure_unique");
    ctx.store_result_value(array)
}
