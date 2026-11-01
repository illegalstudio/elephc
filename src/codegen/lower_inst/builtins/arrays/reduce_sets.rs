//! Purpose:
//! Array walk, merge, set operations, slice, and splice entry points.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen::lower_inst::receiver_place::ReceiverPlace;

/// Lowers `array_walk()` through the callback-driven runtime helper.
/// A closure literal whose FIRST parameter is by-reference, resolved at the `array_walk` call site.
///
/// Everything needed to call it directly is here: `closure_new` carries the closure's name and its
/// capture operands, and the compiled closure's parameter list is `[element, captures…]` with the
/// captures appended by `lower_closure`.
struct ByRefClosureWalkTarget {
    symbol: String,
    capture_types: Vec<PhpType>,
    capture_values: Vec<ValueId>,
    /// Whether each capture is a `use (&$x)` one, which is passed as the SLOT'S ADDRESS.
    capture_by_ref: Vec<bool>,
}

/// Recognises `array_walk($a, function (&$v) { … })` at the point where the closure is still known.
///
/// `None` for every other callback shape — a string name, a first-class callable, a `callable`
/// parameter — because those reach the closure through the descriptor invoker, which passes
/// arguments by value and has no by-reference support (see the `!param.by_ref` filter in
/// `codegen::lower_inst::callables`). Those keep the checker's refusal rather than silently
/// dropping the callback's writes.
fn by_ref_closure_walk_target(
    ctx: &FunctionContext<'_>,
    callback: ValueId,
) -> Option<ByRefClosureWalkTarget> {
    let value = ctx.function.value(callback)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = ctx.function.instruction(inst)?;
    if inst.op != crate::ir::Op::ClosureNew {
        return None;
    }
    let Some(crate::ir::Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    let name = ctx.module.data.strings.get(data.as_raw() as usize)?;
    let closure = ctx
        .module
        .closures
        .iter()
        .find(|closure| closure.name == *name)?;
    if !closure.params.first()?.by_ref {
        return None;
    }
    // The captures are the TAIL of the closure's parameter list, one per `closure_new` operand,
    // in the same order.
    let capture_count = inst.operands.len();
    if closure.params.len() != capture_count + 1 {
        return None;
    }
    Some(ByRefClosureWalkTarget {
        symbol: function_symbol(&closure.name),
        capture_types: closure
            .params
            .iter()
            .skip(1)
            .map(|param| param.php_type.codegen_repr())
            .collect(),
        capture_values: inst.operands.clone(),
        capture_by_ref: closure.params.iter().skip(1).map(|param| param.by_ref).collect(),
    })
}

/// Lowers `array_walk()` for a by-reference closure, walking slot ADDRESSES.
///
/// Builds the direct-callback environment the non-descriptor wrapper expects — the closure entry
/// in slot 0 and one capture per 16-byte slot after it — and hands `__rt_array_walk_ref` a wrapper
/// that forwards its first argument, the element's address, straight to the closure's
/// by-reference parameter.
fn lower_array_walk_by_ref(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    elem_ty: PhpType,
    target: ByRefClosureWalkTarget,
) -> Result<()> {
    // The visible argument is a raw cell address, and declaring it with the element's own type is
    // what keeps the wrapper from converting it: `target_visible_arg_types: None` means the
    // wrapper's source and target agree, so the address is moved rather than coerced.
    let wrapper_label = ctx.next_global_label("array_walk_by_ref_callback_wrapper");
    let done_label = ctx.next_label("array_walk_by_ref_after_wrapper");
    let wrapper = DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types: vec![elem_ty],
        target_visible_arg_types: None,
        capture_types: target.capture_types.clone(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: None,
        invocation_scope_class_id: ctx.lexical_class_id(),
    };
    abi::emit_jump(ctx.emitter, &done_label);
    crate::codegen::emit_callback_wrapper(ctx.emitter, &wrapper);
    ctx.emitter.label(&done_label);

    let env_bytes = 16 * (target.capture_values.len() + 1);
    abi::emit_reserve_temporary_stack(ctx.emitter, env_bytes);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, result_reg, &target.symbol);
    store_walk_env_slot(ctx, 0);
    for (index, capture) in target.capture_values.iter().enumerate() {
        // A `use (&$x)` capture is the slot's ADDRESS, not its value — the same thing
        // `emit_runtime_closure_descriptor_with_captures` stores into the descriptor, and what the
        // closure's `load_ref_cell` / `store_ref_cell` reach through. The slot was already
        // promoted to a reference cell when `closure_new` ran, so only the address is needed here.
        if target.capture_by_ref.get(index).copied().unwrap_or(false) {
            crate::codegen::lower_inst::materialize_local_ref_arg_address(ctx, *capture)?;
        } else {
            ctx.load_value_to_result(*capture)?;
        }
        store_walk_env_slot(ctx, index + 1);
    }

    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &wrapper_label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("mov {}, sp", env_arg_reg)),
        Arch::X86_64 => ctx.emitter.instruction(&format!("mov {}, rsp", env_arg_reg)),
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_walk_ref");
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    store_void_builtin_result(ctx, inst)
}

/// Stores the current integer result into one 16-byte slot of the walk callback environment.
fn store_walk_env_slot(ctx: &mut FunctionContext<'_>, index: usize) {
    let offset = index * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx
            .emitter
            .instruction(&format!("str x0, [sp, #{}]", offset)),   // env slot: closure entry, then one capture each
        Arch::X86_64 => ctx
            .emitter
            .instruction(&format!("mov QWORD PTR [rsp + {}], rax", offset)), // env slot: closure entry, then one capture each
    }
}

/// Returns the element type `array_walk()` can hand its callback, one 8-byte slot at a time.
///
/// Wider than the shared `eight_byte_callback_value_type`, which admits only `Int`/`Bool` and is
/// also `array_reduce`'s gate — a gradual element there is an accumulator question, not this one.
/// `__rt_array_walk` walks fixed 8-byte slots and hands each to the wrapper, so anything whose
/// runtime payload IS one 8-byte word rides through: a boxed `Mixed` cell and an object handle,
/// exactly the reasoning `indexed_sort_element_type` already applies to the permuting sorts.
///
/// A declared bare `array` parameter is `array<mixed>`, so without `Mixed` this refused the most
/// ordinary spelling there is: `function f(array $rows) { array_walk($rows, fn ($v) => ...); }`.
/// Symfony's `Yaml\Command\LintCommand::displayJson` is that shape.
///
/// `Str` stays out: a string element is a multi-word descriptor, not one slot, which is the same
/// reason the slot-permuting sorts refuse it — and a clear unsupported-feature error beats a
/// corrupt walk.
fn array_walk_element_type(ty: PhpType) -> Result<PhpType> {
    let PhpType::Array(elem) = ty.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_walk for PHP type {:?}",
            ty.codegen_repr()
        )));
    };
    let elem = elem.codegen_repr();
    if matches!(
        elem,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Never
            | PhpType::Mixed
            | PhpType::Object(_)
    ) {
        return Ok(elem);
    }
    Err(CodegenIrError::unsupported(format!(
        "array_walk PHP type {:?}",
        elem
    )))
}

pub(crate) fn lower_array_walk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_walk", 2)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    if super::boxed_walk::lower_boxed_array_walk(
        ctx,
        inst,
        array,
        callback,
        "array_walk",
        false,
    )? {
        return Ok(());
    }
    let elem_ty = array_walk_element_type(ctx.value_php_type(array)?)?;
    // A callback that takes its element BY REFERENCE needs the slot's ADDRESS, not its value.
    if let Some(target) = by_ref_closure_walk_target(ctx, callback) {
        return lower_array_walk_by_ref(ctx, inst, array, elem_ty, target);
    }
    match ctx.value_php_type(callback)?.codegen_repr() {
        PhpType::Callable => {
            lower_descriptor_callback_runtime(
                ctx,
                callback,
                vec![elem_ty.clone()],
                PhpType::Void,
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
                    Ok(())
                },
            )?;
            store_true_builtin_result(ctx, inst)?;
            return Ok(());
        }
        PhpType::Str => {
            lower_runtime_string_descriptor_callback(
                ctx,
                callback,
                Some(&PhpType::Array(Box::new(elem_ty.clone()))),
                vec![elem_ty.clone()],
                PhpType::Void,
                super::super::super::instruction_strict_php_profile(inst),
                "array_walk",
                |ctx, wrapper_label, env_bytes| {
                    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
                    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
                    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, wrapper_label);
                    ctx.load_value_to_reg(array, array_arg_reg)?;
                    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
                    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
                    Ok(())
                },
            )?;
            store_true_builtin_result(ctx, inst)?;
            return Ok(());
        }
        _ => {}
    }
    let callback_binding =
        static_sort_callback_binding(ctx, callback, "array_walk callback", Some(&[elem_ty]))?;
    let env_bytes = reserve_static_callback_env(ctx, callback_binding.env_source)?;
    let callback_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg_reg, &callback_binding.label);
    ctx.load_value_to_reg(array, array_arg_reg)?;
    load_static_callback_env_arg(ctx, env_arg_reg, env_bytes);
    abi::emit_call_label(ctx.emitter, "__rt_array_walk");
    if env_bytes != 0 {
        abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    }
    store_true_builtin_result(ctx, inst)
}

/// Merges boxed PHP arrays through layout dispatch, retaining the concrete indexed fast path.
pub(crate) fn lower_array_merge(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() {
        let capacity = abi::int_arg_reg_name(ctx.emitter.target, 0);
        let element_size = abi::int_arg_reg_name(ctx.emitter.target, 1);
        abi::emit_load_int_immediate(ctx.emitter, capacity, 0);
        abi::emit_load_int_immediate(ctx.emitter, element_size, 8);
        abi::emit_call_label(ctx.emitter, "__rt_array_new");
        return store_if_result(ctx, inst);
    }
    if inst.operands.len() == 1 {
        let operand = expect_operand(inst, 0)?;
        return match ctx.value_php_type(operand)?.codegen_repr() {
            PhpType::Array(_) => {
                ctx.load_value_to_result(operand)?;
                if ctx.emitter.target.arch == Arch::X86_64 {
                    ctx.emitter.instruction("mov rdi, rax");                    // pass the sole packed input to the ownership-preserving clone helper
                }
                abi::emit_call_label(ctx.emitter, "__rt_array_clone_shallow");
                store_if_result(ctx, inst)
            }
            PhpType::Mixed | PhpType::Union(_) => {
                super::misc_dispatch::materialize_owned_mixed_hash_operand(
                    ctx,
                    operand,
                    "array_merge",
                )?;
                store_if_result(ctx, inst)
            }
            other => Err(CodegenIrError::unsupported(format!(
                "array_merge single argument PHP type {:?}",
                other
            ))),
        };
    }

    if inst.operands.len() == 2 {
        let first = expect_operand(inst, 0)?;
        let second = expect_operand(inst, 1)?;
        if ctx.value_php_type(first)?.codegen_repr() == PhpType::Mixed
            || ctx.value_php_type(second)?.codegen_repr() == PhpType::Mixed
        {
            return super::boxed_merge::lower_boxed_array_merge(ctx, inst, first, second);
        }
    }

    if inst.operands.len() == 2
        && inst.operands.iter().any(|operand| {
            ctx.value_php_type(*operand).is_ok_and(|ty| {
                matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            })
        })
    {
        return super::misc_dispatch::lower_gradual_two_hash_arg_builtin(
            ctx,
            inst,
            "array_merge",
            "__rt_array_replace",
            Some(1),
        );
    }

    let operand_types = inst
        .operands
        .iter()
        .map(|operand| ctx.value_php_type(*operand).map(|ty| ty.codegen_repr()))
        .collect::<Result<Vec<_>>>()?;
    let element_types = operand_types
        .iter()
        .map(|ty| {
            indexed_array_element_repr(ty).ok_or_else(|| {
                CodegenIrError::unsupported(format!("array_merge for PHP type {:?}", ty))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let uses_string_slots = element_types.iter().any(|ty| matches!(ty, PhpType::Str))
        && element_types
            .iter()
            .all(|ty| matches!(ty, PhpType::Str | PhpType::Never | PhpType::Void));
    let merged_element_type = if uses_string_slots {
        PhpType::Str
    } else {
        let mut merged = element_types[0].clone();
        for next in &element_types[1..] {
            merged = compatible_eight_byte_indexed_array_element_type(
                PhpType::Array(Box::new(merged)),
                PhpType::Array(Box::new(next.clone())),
                "array_merge",
            )?;
        }
        merged
    };
    let helper = if uses_string_slots {
        "__rt_array_merge_str"
    } else {
        array_merge_runtime_helper(&merged_element_type)
    };
    emit_array_merge_pair(ctx, inst.operands[0], inst.operands[1], helper)?;
    stamp_array_merge_result(ctx, &merged_element_type);

    if inst.operands.len() > 2 {
        abi::emit_reserve_temporary_stack(ctx.emitter, 16);                     // preserve the old and replacement owned merge results across each fold step
        for operand in &inst.operands[2..] {
            abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
                    ctx.load_value_to_reg(*operand, "x1")?;
                }
                Arch::X86_64 => {
                    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
                    ctx.load_value_to_reg(*operand, "rsi")?;
                }
            }
            abi::emit_call_label(ctx.emitter, helper);
            stamp_array_merge_result(ctx, &merged_element_type);
            abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0,
            );
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");             // release the previous owned fold result after the replacement retained its payloads
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                8,
            );
        }
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    store_if_result(ctx, inst)
}

/// Emits one two-array merge call with target-aware argument registers.
fn emit_array_merge_pair(
    ctx: &mut FunctionContext<'_>,
    first: ValueId,
    second: ValueId,
    helper: &str,
) -> Result<()> {
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
    Ok(())
}

/// Stamps a merge result so later indexed reads decode its payloads correctly.
///
/// Every merge helper allocates its result through the shared array constructor, which leaves
/// the value_type lane empty. Unstamped, every reader treats the merged slots as raw words:
/// merging two heterogeneous arrays produced the right COUNT and printed ADDRESSES. The stamp
/// is a no-op for element types that carry no runtime value_type tag (`Int`, `Never`, …), so it
/// is applied for every helper rather than only the refcounted one.
fn stamp_array_merge_result(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) {
    let result = abi::int_result_reg(ctx.emitter);
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, result, elem_ty);
}

/// Returns the indexed-array element representation, or `None` for another container kind.
fn indexed_array_element_repr(ty: &PhpType) -> Option<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(element) => Some(element.codegen_repr()),
        _ => None,
    }
}

/// Lowers `array_diff()` for two compatible indexed arrays with pointer-sized payload slots.
pub(crate) fn lower_array_diff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_diff",
        "__rt_array_diff",
        "__rt_array_diff_refcounted",
    )
}

/// Lowers `array_intersect()` for two compatible indexed arrays with pointer-sized payload slots.
pub(crate) fn lower_array_intersect(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_indexed_array_set_op(
        ctx,
        inst,
        "array_intersect",
        "__rt_array_intersect",
        "__rt_array_intersect_refcounted",
    )
}

/// Lowers `array_diff_key()` for two associative arrays by filtering first-operand keys.
pub(crate) fn lower_array_diff_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_diff_key", "__rt_array_diff_key")
}

/// Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys.
pub(crate) fn lower_array_intersect_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_assoc_array_key_set_op(ctx, inst, "array_intersect_key", "__rt_array_intersect_key")
}

/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
///
/// PHP's `bool $preserve_keys = false` renumbers the selected window from zero; a literal `true`
/// keeps the source integer keys instead. A dense indexed array cannot hold a window that does not
/// start at key 0, so the key-preserving form lowers to `__rt_array_slice_to_hash`, which builds an
/// owned hash. For concrete storage the checker guarantees the flag is a literal (it decides the
/// result's static shape), so a non-literal operand can only mean the checker and the backend
/// disagree. A boxed source is dispatched first: its result is the boxed PHP array whatever the
/// flag says, so it reads the flag at runtime (see `lower_mixed_array_slice`).
pub(crate) fn lower_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_slice", 2, 4)?;
    let array = expect_operand(inst, 0)?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_array_slice(ctx, inst);
    }
    // A concrete indexed source whose flag is only known at run time builds both arms and boxes
    // the result as Mixed (the checker restricts this to an indexed source).
    if let Some(flag) = slice_like_non_literal_preserve_keys(ctx, inst)? {
        return lower_array_slice_dynamic_preserve_keys(ctx, inst, array, flag);
    }
    let preserve_keys = slice_like_preserve_keys(ctx, inst, "array_slice")?;
    if matches!(
        ctx.value_php_type(array)?.codegen_repr(),
        PhpType::AssocArray { .. }
    ) {
        return lower_hash_slice(ctx, inst, array, preserve_keys);
    }
    if preserve_keys {
        return lower_array_slice_preserve_keys(ctx, inst, array);
    }
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    let source_elem_ty = array_slice_source_element_type(ctx.value_php_type(array)?)?;
    // A concrete source whose local the checker widened to Mixed elsewhere in the body (it was
    // seeded with a scope value, then assigned an `explode()`): the slice is the source's own
    // array, boxed, which is what the Mixed result slot holds.
    if inst.result_php_type.codegen_repr() == PhpType::Mixed {
        let source_ty = ctx.value_php_type(array)?.codegen_repr();
        lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
        crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &source_ty);
        return store_if_result(ctx, inst);
    }
    let result_elem_ty =
        result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_slice", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_slice($array, $offset, $length, $flag)` when `$flag` is only known at run time.
///
/// The checker has already restricted this to an INDEXED source, whose two arms are a dense array
/// and an integer-keyed hash. Both are emitted and boxed as Mixed, because they are different
/// representations and the call's result type is their union — the same shape `array_reverse()`
/// uses for its own runtime flag.
fn lower_array_slice_dynamic_preserve_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    flag: ValueId,
) -> Result<()> {
    let false_case = ctx.next_label("array_slice_preserve_false");
    let done = ctx.next_label("array_slice_preserve_done");

    let source_ty = ctx.value_php_type(array)?.codegen_repr();
    let PhpType::Array(elem) = source_ty.clone() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice with a runtime preserve_keys flag for PHP type {:?}",
            source_ty
        )));
    };
    let preserved_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: elem,
    };
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;

    crate::codegen::lower_inst::builtins::spl::emit_preserve_keys_truthiness(ctx, flag)?;
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_case);

    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    abi::emit_call_label(ctx.emitter, "__rt_array_slice_to_hash");
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &preserved_ty);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&false_case);
    let source_elem_ty = array_slice_source_element_type(ctx.value_php_type(array)?)?;
    lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &source_ty);

    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` over an ASSOCIATIVE receiver, in either `preserve_keys` mode.
///
/// `$offset`/`$length` count positions in insertion order, so the window cannot be addressed by
/// key and `__rt_hash_slice` walks the source instead. One helper serves both modes because they
/// differ by a single per-entry decision: php-src renumbers INTEGER keys when `preserve_keys` is
/// false and leaves string keys alone either way, so the result's key type is the source's in
/// both modes — which is exactly what the checker records.
///
/// Both modes were an explicit `unsupported` diagnostic until issue #683, one from this
/// function's key-preserving sibling and one from `array_slice_source_element_type`.
fn lower_hash_slice(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    preserve_keys: bool,
) -> Result<()> {
    let PhpType::AssocArray { .. } = inst.result_php_type.codegen_repr() else {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice of an associative array into result PHP type {:?}",
            inst.result_php_type
        )));
    };
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    lower_slice_like_args(ctx, array, offset, length, "array_slice")?;
    // The window arguments occupy the first four registers; the mode flag rides in the fifth.
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_load_int_immediate(
            ctx.emitter,
            "x4",
            i64::from(preserve_keys),
        ),
        Arch::X86_64 => abi::emit_load_int_immediate(
            ctx.emitter,
            "r8",
            i64::from(preserve_keys),
        ),
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_slice");
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` for an indexed or hash array stored inside a boxed Mixed cell.
///
/// A boxed result (the PHP array type the checker, the fallback, and the callable wrapper give a
/// boxed source) receives whichever storage the slice built, boxed by its runtime heap kind. An
/// `array<mixed>` result only comes from a checked list type whose operand EIR still boxes, such
/// as a call-site-specialized untyped parameter; a hash payload there stays a hash-backed array,
/// which boxing recognizes by its heap kind.
///
/// `$preserve_keys` is read at runtime. The callable wrapper forwards whatever its caller passed,
/// and `call_user_func_array($f, $args)` hides that argument from the checker, so the flag is
/// staged on the temporary stack before the payload dispatch and both payload shapes honor it: a
/// hash hands it to `__rt_hash_slice`, and a list keeps its keys through
/// `__rt_array_slice_to_hash`. That hash can only be carried by a boxed result, so a flag that may
/// be set is refused for any other result type.
pub(super) fn lower_mixed_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = slice_like_length_operand(inst)?;
    let preserve_keys = inst.operands.get(3).copied();
    let result_ty = inst.result_php_type.codegen_repr();
    let boxed_result = matches!(result_ty, PhpType::Mixed | PhpType::Union(_));
    let result_elem_ty = if boxed_result {
        PhpType::Mixed
    } else {
        result_array_element_type("array_slice", &result_ty)?
    };
    require_array_slice_result_type(&PhpType::Mixed, &result_elem_ty)?;
    let keys_may_be_kept = slice_preserve_keys_may_be_set(ctx, preserve_keys)?;
    if keys_may_be_kept && !boxed_result {
        return Err(CodegenIrError::unsupported(format!(
            "array_slice preserve_keys of a boxed source into result PHP type {:?}",
            inst.result_php_type
        )));
    }
    stage_slice_preserve_keys_flag(ctx, preserve_keys)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_mixed_array_slice_aarch64(
            ctx,
            array,
            offset,
            length,
            &result_elem_ty,
            keys_may_be_kept,
        )?,
        Arch::X86_64 => lower_mixed_array_slice_x86_64(
            ctx,
            array,
            offset,
            length,
            &result_elem_ty,
            keys_may_be_kept,
        )?,
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    if boxed_result {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(PhpType::Mixed)),
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` by mutating an indexed source array and returning removed elements.
///
/// PHP's optional `$replacement` is written into the gap the removal opened, which can make the
/// source array longer than it was. `__rt_array_splice_insert*` grows the payload for that, and a
/// growth relocates the array, so the by-reference receiver is written back a second time after
/// the insertion rather than only after the copy-on-write split.
///
/// Both write-backs go through `ReceiverPlace::store_back`, whose LOCAL arm publishes through the
/// mutated-container path so a frame slot a LATER store widened to boxed `Mixed` receives a cell
/// that RETAINS the container instead of one that takes over the still-live SSA value's
/// reference. See that method for the measured miscompile.
pub(crate) fn lower_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_splice", 2, 4)?;
    let array = expect_operand(inst, 0)?;
    let array_ty = ctx.value_php_type(array)?.codegen_repr();
    if matches!(array_ty, PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_array_splice(ctx, inst);
    }
    if matches!(array_ty, PhpType::Pointer(_)) {
        let addressed_ty = crate::codegen::lower_inst::reference_arguments::array_element_address_value_type(
            ctx, array,
        )?;
        if matches!(addressed_ty, Some(PhpType::Mixed | PhpType::Union(_))) {
            return lower_mixed_array_splice(ctx, inst);
        }
    }
    let offset = expect_operand(inst, 1)?;
    let length = inst.operands.get(2).copied();
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    let replacement =
        SpliceReplacement::resolve(ctx, inst.operands.get(3).copied(), &elem_ty)?;
    let receiver_ty = ctx.value_php_type(array)?;
    let receiver = ReceiverPlace::resolve(ctx, array)?;
    // The pre-mutation bookkeeping every other mutating container builtin performs, and the half
    // `array_splice` was missing: a concrete container loaded out of a boxed frame slot carries an
    // extra owned reference, and the slot's previous Mixed cell has to be released before the
    // mutation or the write-back publishes a second cell over a live one. A no-op unless the slot
    // is raw-represented AND boxed, so a ref-cell or concrete receiver is untouched.
    if let Some(slot) = receiver.slot() {
        ctx.release_mutated_source_local_owner(slot, array)?;
    }
    ensure_unique_array_pop_source(ctx, array)?;
    receiver.store_back(ctx, array, &receiver_ty)?;
    lower_array_splice_call(ctx, array, offset, length, &elem_ty)?;
    emit_splice_replacement_insert(ctx, array, receiver, &receiver_ty, &replacement, &elem_ty)?;
    normalize_array_splice_result(ctx, &elem_ty, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers `array_splice()` for an indexed array stored inside a boxed Mixed cell.
pub(super) fn lower_mixed_array_splice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let receiver_is_slot_address =
        crate::codegen::lower_inst::reference_arguments::value_is_array_element_address(
            ctx, array,
        )?;
    let offset = expect_operand(inst, 1)?;
    let length = inst.operands.get(2).copied();
    let replacement =
        SpliceReplacement::resolve(ctx, inst.operands.get(3).copied(), &PhpType::Mixed)?;
    super::boxed_mutation::prepare_boxed_array_receiver(ctx, array, "array_splice")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_mixed_array_splice_aarch64(
                ctx,
                array,
                offset,
                length,
                &replacement,
                receiver_is_slot_address,
            )?
        }
        Arch::X86_64 => {
            lower_mixed_array_splice_x86_64(
                ctx,
                array,
                offset,
                length,
                &replacement,
                receiver_is_slot_address,
            )?
        }
    }
    normalize_array_splice_result(ctx, &PhpType::Mixed, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}
