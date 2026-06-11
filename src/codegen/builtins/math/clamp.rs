//! Purpose:
//! Emits PHP 8.6 `clamp` builtin calls for integer, floating-point, string, and boxed numeric paths.
//! Validates PHP's bound rules before selecting the upper, lower, or original value.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - Bounds are checked before clamping; `$max` is tested before `$min` when selecting.
//! - Floating bounds reject NaN, and Mixed/Union call surfaces return a boxed Mixed float.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::functions::infer_contextual_type;
use crate::codegen::{abi, emit_box_current_value_as_mixed, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

const CLAMP_MIN_NAN_MESSAGE: &str = "clamp(): Argument #2 ($min) must not be NAN";
const CLAMP_MAX_NAN_MESSAGE: &str = "clamp(): Argument #3 ($max) must not be NAN";
const CLAMP_BOUNDS_MESSAGE: &str =
    "clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)";

const MAX_SLOT: usize = 0;
const MIN_SLOT: usize = 16;
const VALUE_SLOT: usize = 32;
const CLAMP_STACK_BYTES: usize = 48;

/// Lowers a PHP `clamp()` call into target assembly.
///
/// The direct integer and all-string paths preserve their scalar result shape.
/// Float-like and Mixed paths normalize operands to doubles so bound validation,
/// NaN checks, and upper-before-lower selection can share one target-aware flow.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if args.len() != 3 {
        return None;
    }

    emitter.comment("clamp()");
    let arg_types = clamp_arg_types(args, ctx);
    if all_args_are_strings(&arg_types) {
        return Some(emit_string_clamp(args, emitter, ctx, data));
    }
    if all_args_are_ints(&arg_types) {
        return Some(emit_int_clamp(args, emitter, ctx, data));
    }

    Some(emit_float_or_mixed_clamp(
        args,
        &arg_types,
        emitter,
        ctx,
        data,
    ))
}

/// Infers contextual argument types before lowering so the emitter can choose the result representation.
fn clamp_arg_types(args: &[Expr], ctx: &Context) -> Vec<PhpType> {
    args.iter()
        .map(|arg| infer_contextual_type(arg, ctx).codegen_repr())
        .collect()
}

/// Returns true when every argument is statically represented as a string.
fn all_args_are_strings(arg_types: &[PhpType]) -> bool {
    arg_types.iter().all(|ty| matches!(ty, PhpType::Str))
}

/// Returns true when every argument is statically represented as an integer.
fn all_args_are_ints(arg_types: &[PhpType]) -> bool {
    arg_types.iter().all(|ty| matches!(ty, PhpType::Int))
}

/// Returns true when the normalized float path must produce a boxed Mixed result.
fn float_path_returns_mixed(arg_types: &[PhpType]) -> bool {
    arg_types
        .iter()
        .any(|ty| !matches!(ty, PhpType::Int | PhpType::Float))
}

/// Emits integer `clamp()` selection, including bound validation before the upper/lower tests.
fn emit_int_clamp(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }

    let throw_label = ctx.next_label("clamp_int_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_int_use_max");
    let use_min_label = ctx.next_label("clamp_int_use_min");
    let selected_label = ctx.next_label("clamp_int_selected");
    let finish_label = ctx.next_label("clamp_int_finish");
    let (message_label, message_len) = data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x9", MIN_SLOT);
            abi::emit_load_temporary_stack_slot(emitter, "x10", MAX_SLOT);
            emitter.instruction("cmp x9, x10");                                 // validate that the integer lower bound does not exceed the upper bound
            emitter.instruction(&format!("b.gt {}", throw_label));              // throw ValueError when min > max before clamping

            abi::emit_load_temporary_stack_slot(emitter, "x9", VALUE_SLOT);
            abi::emit_load_temporary_stack_slot(emitter, "x10", MAX_SLOT);
            emitter.instruction("cmp x9, x10");                                 // compare the candidate against the upper bound first
            emitter.instruction(&format!("b.gt {}", use_max_label));            // choose max when value is greater than the upper bound
            abi::emit_load_temporary_stack_slot(emitter, "x10", MIN_SLOT);
            emitter.instruction("cmp x9, x10");                                 // compare the candidate against the lower bound second
            emitter.instruction(&format!("b.lt {}", use_min_label));            // choose min when value is lower than the lower bound
            emitter.instruction("mov x0, x9");                                  // keep the original integer value when it is inside the bounds
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_max_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", MAX_SLOT);
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_min_label);
            abi::emit_load_temporary_stack_slot(emitter, "x0", MIN_SLOT);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r9", MIN_SLOT);
            abi::emit_load_temporary_stack_slot(emitter, "r10", MAX_SLOT);
            emitter.instruction("cmp r9, r10");                                 // validate that the integer lower bound does not exceed the upper bound
            emitter.instruction(&format!("jg {}", throw_label));                // throw ValueError when min > max before clamping

            abi::emit_load_temporary_stack_slot(emitter, "r9", VALUE_SLOT);
            abi::emit_load_temporary_stack_slot(emitter, "r10", MAX_SLOT);
            emitter.instruction("cmp r9, r10");                                 // compare the candidate against the upper bound first
            emitter.instruction(&format!("jg {}", use_max_label));              // choose max when value is greater than the upper bound
            abi::emit_load_temporary_stack_slot(emitter, "r10", MIN_SLOT);
            emitter.instruction("cmp r9, r10");                                 // compare the candidate against the lower bound second
            emitter.instruction(&format!("jl {}", use_min_label));              // choose min when value is lower than the lower bound
            emitter.instruction("mov rax, r9");                                 // keep the original integer value when it is inside the bounds
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_max_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", MAX_SLOT);
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_min_label);
            abi::emit_load_temporary_stack_slot(emitter, "rax", MIN_SLOT);
        }
    }

    emitter.label(&selected_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    abi::emit_jump(emitter, &finish_label);

    emitter.label(&throw_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(emitter, &message_label, message_len);

    emitter.label(&finish_label);
    PhpType::Int
}

/// Emits floating-point `clamp()` selection and boxes the result when the static call surface is Mixed.
fn emit_float_or_mixed_clamp(
    args: &[Expr],
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    for arg in args {
        emit_arg_as_float(arg, emitter, ctx, data);
        abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
    }

    let return_mixed = float_path_returns_mixed(arg_types);
    let throw_min_nan_label = ctx.next_label("clamp_float_min_nan");
    let throw_max_nan_label = ctx.next_label("clamp_float_max_nan");
    let throw_bounds_label = ctx.next_label("clamp_float_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_float_use_max");
    let use_min_label = ctx.next_label("clamp_float_use_min");
    let in_range_label = ctx.next_label("clamp_float_in_range");
    let selected_label = ctx.next_label("clamp_float_selected");
    let finish_label = ctx.next_label("clamp_float_finish");
    let (min_nan_label, min_nan_len) = data.add_string(CLAMP_MIN_NAN_MESSAGE.as_bytes());
    let (max_nan_label, max_nan_len) = data.add_string(CLAMP_MAX_NAN_MESSAGE.as_bytes());
    let (bounds_label, bounds_len) = data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "d1", MIN_SLOT);
            emitter.instruction("fcmp d1, d1");                                 // detect NaN in the lower bound before any range comparison
            emitter.instruction(&format!("b.vs {}", throw_min_nan_label));      // throw ValueError for a NaN lower bound
            abi::emit_load_temporary_stack_slot(emitter, "d2", MAX_SLOT);
            emitter.instruction("fcmp d2, d2");                                 // detect NaN in the upper bound before any range comparison
            emitter.instruction(&format!("b.vs {}", throw_max_nan_label));      // throw ValueError for a NaN upper bound
            emitter.instruction("fcmp d1, d2");                                 // validate that the lower bound does not exceed the upper bound
            emitter.instruction(&format!("b.gt {}", throw_bounds_label));       // throw ValueError when min > max before clamping

            abi::emit_load_temporary_stack_slot(emitter, "d0", VALUE_SLOT);
            emitter.instruction("fcmp d0, d2");                                 // compare the candidate against the upper bound first
            emitter.instruction(&format!("b.vs {}", in_range_label));           // leave a NaN value unclamped because only bounds reject NaN
            emitter.instruction(&format!("b.gt {}", use_max_label));            // choose max when value is greater than the upper bound
            emitter.instruction("fcmp d0, d1");                                 // compare the candidate against the lower bound second
            emitter.instruction(&format!("b.vs {}", in_range_label));           // leave a NaN value unclamped after the lower-bound comparison too
            emitter.instruction(&format!("b.lt {}", use_min_label));            // choose min when value is lower than the lower bound
            abi::emit_jump(emitter, &in_range_label);

            emitter.label(&use_max_label);
            emitter.instruction("fmov d0, d2");                                 // return the upper bound when the candidate is too large
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_min_label);
            emitter.instruction("fmov d0, d1");                                 // return the lower bound when the candidate is too small
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "xmm1", MIN_SLOT);
            emitter.instruction("ucomisd xmm1, xmm1");                          // detect NaN in the lower bound before any range comparison
            emitter.instruction(&format!("jp {}", throw_min_nan_label));        // throw ValueError for a NaN lower bound
            abi::emit_load_temporary_stack_slot(emitter, "xmm2", MAX_SLOT);
            emitter.instruction("ucomisd xmm2, xmm2");                          // detect NaN in the upper bound before any range comparison
            emitter.instruction(&format!("jp {}", throw_max_nan_label));        // throw ValueError for a NaN upper bound
            emitter.instruction("ucomisd xmm1, xmm2");                          // validate that the lower bound does not exceed the upper bound
            emitter.instruction(&format!("ja {}", throw_bounds_label));         // throw ValueError when min > max before clamping

            abi::emit_load_temporary_stack_slot(emitter, "xmm0", VALUE_SLOT);
            emitter.instruction("ucomisd xmm0, xmm2");                          // compare the candidate against the upper bound first
            emitter.instruction(&format!("jp {}", in_range_label));             // leave a NaN value unclamped because only bounds reject NaN
            emitter.instruction(&format!("ja {}", use_max_label));              // choose max when value is greater than the upper bound
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the candidate against the lower bound second
            emitter.instruction(&format!("jp {}", in_range_label));             // leave a NaN value unclamped after the lower-bound comparison too
            emitter.instruction(&format!("jb {}", use_min_label));              // choose min when value is lower than the lower bound
            abi::emit_jump(emitter, &in_range_label);

            emitter.label(&use_max_label);
            emitter.instruction("movsd xmm0, xmm2");                            // return the upper bound when the candidate is too large
            abi::emit_jump(emitter, &selected_label);

            emitter.label(&use_min_label);
            emitter.instruction("movsd xmm0, xmm1");                            // return the lower bound when the candidate is too small
        }
    }

    emitter.label(&in_range_label);
    abi::emit_jump(emitter, &selected_label);

    emitter.label(&selected_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    if return_mixed {
        emit_box_current_value_as_mixed(emitter, &PhpType::Float);
    }
    abi::emit_jump(emitter, &finish_label);

    emitter.label(&throw_min_nan_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(emitter, &min_nan_label, min_nan_len);

    emitter.label(&throw_max_nan_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(emitter, &max_nan_label, max_nan_len);

    emitter.label(&throw_bounds_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(emitter, &bounds_label, bounds_len);

    emitter.label(&finish_label);
    if return_mixed {
        PhpType::Mixed
    } else {
        PhpType::Float
    }
}

/// Emits an argument expression and normalizes its result to the active floating-point result register.
fn emit_arg_as_float(
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ty = emit_expr(arg, emitter, ctx, data).codegen_repr();
    match ty {
        PhpType::Float => {}
        PhpType::Mixed => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_float");
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_str_to_number");
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
            abi::emit_int_result_to_float_result(emitter);
        }
        _ => {
            abi::emit_int_result_to_float_result(emitter);
        }
    }
}

/// Emits all-string `clamp()` selection using `strcmp` ordering and PHP's upper-before-lower rule.
fn emit_string_clamp(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
    }

    let throw_label = ctx.next_label("clamp_string_invalid_bounds");
    let use_max_label = ctx.next_label("clamp_string_use_max");
    let use_min_label = ctx.next_label("clamp_string_use_min");
    let selected_label = ctx.next_label("clamp_string_selected");
    let finish_label = ctx.next_label("clamp_string_finish");
    let (message_label, message_len) = data.add_string(CLAMP_BOUNDS_MESSAGE.as_bytes());

    emit_compare_string_slots(emitter, MIN_SLOT, MAX_SLOT);
    emit_branch_if_string_compare_gt(emitter, &throw_label);
    emit_compare_string_slots(emitter, VALUE_SLOT, MAX_SLOT);
    emit_branch_if_string_compare_gt(emitter, &use_max_label);
    emit_compare_string_slots(emitter, VALUE_SLOT, MIN_SLOT);
    emit_branch_if_string_compare_lt(emitter, &use_min_label);
    emit_load_string_slot_to_result(emitter, VALUE_SLOT);
    abi::emit_jump(emitter, &selected_label);

    emitter.label(&use_max_label);
    emit_load_string_slot_to_result(emitter, MAX_SLOT);
    abi::emit_jump(emitter, &selected_label);

    emitter.label(&use_min_label);
    emit_load_string_slot_to_result(emitter, MIN_SLOT);

    emitter.label(&selected_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    abi::emit_jump(emitter, &finish_label);

    emitter.label(&throw_label);
    abi::emit_release_temporary_stack(emitter, CLAMP_STACK_BYTES);
    emit_throw_value_error(emitter, &message_label, message_len);

    emitter.label(&finish_label);
    PhpType::Str
}

/// Compares two saved string slots with `__rt_strcmp` and leaves the integer result active.
fn emit_compare_string_slots(emitter: &mut Emitter, left_offset: usize, right_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", left_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", left_offset + 8);
            abi::emit_load_temporary_stack_slot(emitter, "x3", right_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x4", right_offset + 8);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", left_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", left_offset + 8);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", right_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rcx", right_offset + 8);
        }
    }
    abi::emit_call_label(emitter, "__rt_strcmp");
}

/// Branches to `label` when the most recent string comparison result is greater than zero.
fn emit_branch_if_string_compare_gt(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // test whether the left string sorted after the right string
            emitter.instruction(&format!("b.gt {}", label));                    // branch when the string comparison result is positive
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // test whether the left string sorted after the right string
            emitter.instruction(&format!("jg {}", label));                      // branch when the string comparison result is positive
        }
    }
}

/// Branches to `label` when the most recent string comparison result is less than zero.
fn emit_branch_if_string_compare_lt(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // test whether the left string sorted before the right string
            emitter.instruction(&format!("b.lt {}", label));                    // branch when the string comparison result is negative
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // test whether the left string sorted before the right string
            emitter.instruction(&format!("jl {}", label));                      // branch when the string comparison result is negative
        }
    }
}

/// Loads a saved string slot into the target's string result registers.
fn emit_load_string_slot_to_result(emitter: &mut Emitter, offset: usize) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
    abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
}

/// Emits a catchable `ValueError` using a static message string.
fn emit_throw_value_error(emitter: &mut Emitter, message_symbol: &str, message_len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => emit_throw_value_error_aarch64(emitter, message_symbol, message_len),
        Arch::X86_64 => emit_throw_value_error_x86_64(emitter, message_symbol, message_len),
    }
}

/// Emits the AArch64 allocation and unwinder handoff for a `ValueError`.
fn emit_throw_value_error_aarch64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("mov x0, #32");                                         // request Throwable payload storage
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the ValueError object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp allocation as a runtime object
    abi::emit_symbol_address(emitter, "x9", "_spl_value_error_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load ValueError's runtime class id for this program
    emitter.instruction("str x9, [x0]");                                        // store class id at the object header
    abi::emit_symbol_address(emitter, "x9", message_symbol);
    emitter.instruction("str x9, [x0, #8]");                                    // store static ValueError message pointer
    emitter.instruction(&format!("mov x9, #{}", message_len));                  // load static ValueError message length
    emitter.instruction("str x9, [x0, #16]");                                   // store exception message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder
}

/// Emits the Linux x86_64 allocation and unwinder handoff for a `ValueError`.
fn emit_throw_value_error_x86_64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    emitter.instruction("mov rax, 32");                                         // request Throwable payload storage
    emitter.instruction("call __rt_heap_alloc");                                // allocate the ValueError object payload
    emitter.instruction("mov r10, 0x4548504c00000006");                         // x86_64 heap-kind word: HE LP magic + kind 6 object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0); // load ValueError's runtime class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at the object header
    abi::emit_symbol_address(emitter, "r10", message_symbol);                   // materialize static ValueError message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store static ValueError message pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store static ValueError message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
