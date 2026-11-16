//! Purpose:
//! Lowers PHP array filtering through the boxed value/key predicate runtime.
//!
//! Called from:
//! - The typed ArrayFilter runtime-function dispatcher.
//!
//! Key details:
//! - Borrowed source/callback triples preserve EIR ownership and support null callbacks.
//! - PHP 8.6 validates mode values; earlier profiles use value-only mode for other integers.

use super::*;

const STACK_BYTES: usize = 96;
const MODE_OFFSET: usize = 64;
const DESCRIPTOR_OFFSET: usize = 80;

/// Validates the source, acquires an optional descriptor and transfers it into the filtering scan.
pub(crate) fn lower_array_filter(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_filter", 3)?;
    let array = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let mode = expect_operand(inst, 2)?;
    if try_lower_direct_int_filter(ctx, inst, array, callback, mode)? {
        return store_if_result(ctx, inst);
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, STACK_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, array, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, callback, 32)?;
    super::boxed_predicates::validate_source(ctx, "array_filter", STACK_BYTES);
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(mode)?;
    abi::emit_store_to_sp(ctx.emitter, result, MODE_OFFSET);
    let null_callback = ctx.next_label("array_filter_null_callback");
    let ready = ctx.next_label("array_filter_descriptor_ready");
    abi::emit_temporary_stack_address(ctx.emitter, result, 32);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // null callbacks select PHP's empty-value filtering
            abi::emit_branch_if_equal_wide(ctx.emitter, &null_callback);        // omit callback normalization and its runtime-name case tables
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // null callbacks select PHP's empty-value filtering
            ctx.emitter.instruction(&format!("je {null_callback}"));            // omit callback normalization only for null
        }
    }
    super::boxed_predicates::acquire_callback_descriptor(
        ctx, inst, callback, "array_filter",
        "array_filter(): Argument #2 ($callback) must be a valid callback or null",
    )?;
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&null_callback);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    ctx.emitter.label(&ready);
    abi::emit_store_to_sp(ctx.emitter, result, DESCRIPTOR_OFFSET);
    normalize_filter_mode(ctx);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), DESCRIPTOR_OFFSET);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), MODE_OFFSET);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        ctx.lexical_class_id(),
    );
    abi::emit_call_label(ctx.emitter, "__rt_array_predicate_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, STACK_BYTES);
    store_if_result(ctx, inst)
}

/// Filters a packed `array<int>` through a pure raw bool predicate.
///
/// The generic predicate runtime boxes every element and key, builds argument arrays, and
/// invokes a callable descriptor per element. A capture-free closure whose parameter is `int`
/// and whose returns are `bool` can run directly: the element is passed in the first integer
/// register, and each kept value is inserted into a hash under its original integer key without
/// boxing. Only value mode (the default) is admitted, and only for a predicate whose EIR effects
/// cannot throw, warn, or mutate state, so the helper needs no exception boundary.
fn try_lower_direct_int_filter(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    callback: ValueId,
    mode: ValueId,
) -> Result<bool> {
    if ctx.value_php_type(array)?.codegen_repr()
        != PhpType::Array(Box::new(PhpType::Int))
    {
        return Ok(false);
    }
    if !constant_zero(ctx, mode) {
        return Ok(false);
    }
    let Some(closure) = super::map_dispatch::capture_free_closure(ctx, callback)? else {
        return Ok(false);
    };
    if closure.params.len() != 1
        || closure.params[0].by_ref
        || closure.params[0].variadic
        || closure.params[0].php_type.codegen_repr() != PhpType::Int
        || closure.return_php_type.codegen_repr() != PhpType::Bool
        || closure.flags.is_generator
        || closure.flags.is_fiber_wrapper
        || !pure_predicate_body(closure)
    {
        return Ok(false);
    }
    let symbol = crate::names::function_symbol(&closure.name);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg, &symbol);
    ctx.load_value_to_reg(array, array_arg)?;
    abi::emit_load_int_immediate(ctx.emitter, env_arg, 0);
    abi::emit_call_label(ctx.emitter, "__rt_array_filter_int_direct");
    crate::codegen_support::emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        },
    );
    store_if_result(ctx, inst)?;
    Ok(true)
}

/// Returns whether `value` is a literal zero, the only accepted filter mode.
fn constant_zero(ctx: &FunctionContext<'_>, value: ValueId) -> bool {
    let Some(value_ref) = ctx.function.value(value) else {
        return false;
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return false;
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return false;
    };
    matches!(
        (inst.op, &inst.immediate),
        (Op::ConstI64, Some(Immediate::I64(0)))
    )
}

/// Returns whether a closure body can run without throwing, warning, or mutating state.
fn pure_predicate_body(closure: &crate::ir::Function) -> bool {
    closure.instructions.iter().all(|inst| {
        if !inst
            .effects
            .intersection(
                crate::ir::Effects::MAY_THROW
                    | crate::ir::Effects::MAY_WARN
                    | crate::ir::Effects::WRITES_GLOBAL
                    | crate::ir::Effects::WRITES_HEAP
                    | crate::ir::Effects::WRITES_FS
                    | crate::ir::Effects::WRITES_PROCESS
                    | crate::ir::Effects::OUTPUT
                    | crate::ir::Effects::REFCOUNT_OP
                    | crate::ir::Effects::BLOCKING_IO
                    | crate::ir::Effects::NETWORK_IO,
            )
            .is_empty()
        {
            return false;
        }
        !inst.effects.contains(crate::ir::Effects::MAY_FATAL)
            || safe_constant_division(closure, inst)
    })
}

/// Returns whether an integer division or modulo has a nonzero, non-minus-one constant divisor.
///
/// The generic effect model marks every `%`/`intdiv` as possibly fatal because a zero divisor
/// throws. A syntactic constant divisor removes that risk, which is what lets a predicate such
/// as `$x % 2 === 0` use the raw filter helper. `-1` stays excluded because `PHP_INT_MIN % -1`
/// is the one remaining overflow edge.
fn safe_constant_division(function: &crate::ir::Function, inst: &crate::ir::Instruction) -> bool {
    if !matches!(
        inst.op,
        crate::ir::Op::ISMod | crate::ir::Op::IDiv | crate::ir::Op::ISDiv
    ) {
        return false;
    }
    let Some(&rhs) = inst.operands.get(1) else {
        return false;
    };
    let Some(value) = function.value(rhs) else {
        return false;
    };
    let crate::ir::ValueDef::Instruction { inst, .. } = value.def else {
        return false;
    };
    let Some(divisor) = function.instruction(inst) else {
        return false;
    };
    matches!(
        (divisor.op, &divisor.immediate),
        (crate::ir::Op::ConstI64, Some(crate::ir::Immediate::I64(n)))
            if *n != 0 && *n != -1
    )
}

/// Maps PHP modes to scan modes 3/4/5, retiring the descriptor before a PHP 8.6 mode error.
fn normalize_filter_mode(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    let valid = ctx.next_label("array_filter_valid_mode");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, MODE_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // an unsigned range check excludes negative and excessive modes
            ctx.emitter.instruction(&format!("b.ls {valid}"));                  // retain value, both and key mode selection
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // an unsigned range check excludes negative and excessive modes
            ctx.emitter.instruction(&format!("jbe {valid}"));                   // retain value, both and key mode selection
        }
    }
    if crate::codegen::compile_php_version().version_id() >= 80600 {
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, DESCRIPTOR_OFFSET);
        abi::emit_call_label(ctx.emitter, "__rt_callable_descriptor_release");
        abi::emit_release_temporary_stack(ctx.emitter, STACK_BYTES);
        crate::codegen::lower_inst::exceptions::emit_value_error(
            ctx, "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH",
        );
    } else {
        abi::emit_load_int_immediate(ctx.emitter, result, 0);
    }
    ctx.emitter.label(&valid);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #3"),             // select the predicate runtime's filtering mode family
        Arch::X86_64 => ctx.emitter.instruction("add rax, 3"),                  // select the predicate runtime's filtering mode family
    }
    abi::emit_store_to_sp(ctx.emitter, result, MODE_OFFSET);
}
