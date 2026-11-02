//! Purpose:
//! Lowers PHP's single-array `min()` / `max()` form for the EIR backend.
//! Reduces an indexed array's payload slots, or a hash-backed table's values, to one
//! element.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::math::lower_min_max()`.
//!
//! Key details:
//! - Indexed arrays store their logical length in the first header word and their
//!   payload slots 24 bytes after the header: one 8-byte slot per `int`/`float`/`bool`
//!   or boxed-`Mixed` element, one 16-byte `[ptr][len]` slot per string element.
//! - Scalar indexed arrays reduce with an inline loop; string, boxed-`Mixed`, and
//!   hash-backed containers reduce through the `__rt_min_max_str` /
//!   `__rt_min_max_mixed` / `__rt_min_max_hash` runtime helpers, which apply PHP 8's
//!   full comparison table through `__rt_php_compare`.
//! - An empty array is PHP's `ValueError`, thrown through the shared math
//!   `emit_throw_value_error()` path so it stays catchable like `clamp()`'s. The
//!   runtime reductions report emptiness with runtime tag `-1`.
//! - Scratch is limited to the registers the backend already treats as clobbered by
//!   a builtin call (`x9`–`x13`, `d0`/`d1`; `rax`/`rcx`/`rdx`/`r10`/`r11`, `xmm0`/`xmm1`),
//!   so no register-allocated value can be destroyed by the loop.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::expect_operand;

/// Byte offset of the first payload slot inside an indexed-array allocation.
const ARRAY_DATA_OFFSET: i64 = 24;

/// Selects the runtime reduction that matches a container's element storage.
#[derive(Clone, Copy)]
enum ContainerReduction {
    /// Indexed array of 16-byte `[ptr][len]` string slots (`__rt_min_max_str`).
    IndexedStr,
    /// Indexed array of borrowed boxed-`Mixed` cells (`__rt_min_max_mixed`).
    IndexedMixed,
    /// Hash-backed associative array of any value type (`__rt_min_max_hash`).
    Hash,
}

impl ContainerReduction {
    /// Returns the `__rt_*` symbol that reduces this container shape.
    fn symbol(self) -> &'static str {
        match self {
            ContainerReduction::IndexedStr => "__rt_min_max_str",
            ContainerReduction::IndexedMixed => "__rt_min_max_mixed",
            ContainerReduction::Hash => "__rt_min_max_hash",
        }
    }
}

/// Lowers PHP's single-argument `min()` / `max()` form and reports whether it applied.
///
/// Returns `Ok(false)` for the variadic form so the caller keeps its own lowering.
/// Any single-argument call whose operand is an array is handled here — including the
/// element types the reduction cannot compare, which are rejected with an explicit
/// diagnostic rather than falling through to the numeric paths.
pub(super) fn try_lower_single_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
) -> Result<bool> {
    if inst.operands.len() != 1 {
        return Ok(false);
    }
    let array = expect_operand(inst, 0)?;
    match ctx.value_php_type(array)?.codegen_repr() {
        PhpType::Array(element) => {
            let element = element.codegen_repr();
            match element {
                PhpType::Str => {
                    lower_container_min_max(ctx, inst, want_max, ContainerReduction::IndexedStr)?
                }
                PhpType::Mixed => {
                    lower_container_min_max(ctx, inst, want_max, ContainerReduction::IndexedMixed)?
                }
                _ => lower_array_min_max(ctx, inst, want_max, &element)?,
            }
            Ok(true)
        }
        PhpType::AssocArray { .. } => {
            lower_container_min_max(ctx, inst, want_max, ContainerReduction::Hash)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Formats the diagnostic for a single-array `min()` / `max()` the reduction cannot compare.
///
/// Indexed arrays of `int`, `float`, `bool` and `string`, indexed arrays of boxed
/// `Mixed` cells, and hash-backed associative arrays all reduce. What is left are the
/// element representations no reduction can read as a comparable payload: the tagged
/// nullable-scalar slots, whose runtime tag lives in a side register, and homogeneous
/// arrays of a heap shape (`array<array<int>>`, `array<Foo>`) that PHP would compare
/// structurally. Rejecting them keeps a wrong ordering out of the generated program.
fn unsupported_element_error(name: &str, shape: &str) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "{}() with a single array argument cannot reduce an array of {} values",
        name, shape
    ))
}

/// Lowers `min($array)` / `max($array)` by reducing the array's payload slots.
///
/// `element` is the array's codegen element representation. Integer-like elements
/// are compared as signed 64-bit words and floating elements through the same
/// `fmin`/`fmax` (AArch64) and `minsd`/`maxsd` (x86_64) selection the variadic form
/// uses, so both call forms agree on every target. An empty array throws PHP's
/// `ValueError` and never falls through to the reduction.
fn lower_array_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
    element: &PhpType,
) -> Result<()> {
    let name = super::min_max_name(want_max);
    let array = expect_operand(inst, 0)?;
    let float_elements = match element {
        PhpType::Float => true,
        // `Void`/`Never` is the element type of an empty array literal and of an
        // all-null array: both store zeroed scalar slots, so the integer reduction
        // handles them and an empty one reaches the ValueError path.
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => false,
        other => {
            return Err(unsupported_element_error(
                name,
                &format!("array<{}>", other),
            ))
        }
    };
    let result_ty = inst
        .result
        .map(|value| ctx.value_php_type(value))
        .transpose()?
        .unwrap_or_else(|| element.clone())
        .codegen_repr();
    let empty_label = ctx.next_label("min_max_array_empty");
    let loop_label = ctx.next_label("min_max_array_loop");
    let reduced_label = ctx.next_label("min_max_array_reduced");
    let done_label = ctx.next_label("min_max_array_done");
    let message = format!(
        "{}(): Argument #1 ($value) must contain at least one element",
        name
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());

    ctx.load_value_to_result(array)?;
    match (ctx.emitter.target.arch, float_elements) {
        (Arch::AArch64, false) => {
            emit_int_reduce_aarch64(ctx, want_max, &empty_label, &loop_label, &reduced_label)
        }
        (Arch::AArch64, true) => {
            emit_float_reduce_aarch64(ctx, want_max, &empty_label, &loop_label, &reduced_label)
        }
        (Arch::X86_64, false) => {
            emit_int_reduce_x86_64(ctx, want_max, &empty_label, &loop_label, &reduced_label)
        }
        (Arch::X86_64, true) => {
            emit_float_reduce_x86_64(ctx, want_max, &empty_label, &loop_label, &reduced_label)
        }
    }
    ctx.emitter.label(&reduced_label);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    super::emit_throw_value_error(ctx, &message_label, message_len);
    ctx.emitter.label(&done_label);
    materialize_result(ctx, element, float_elements, &result_ty)
}

/// Converts the reduced element into the representation the EIR result value expects.
///
/// EIR array element types and checker call-site types are inferred separately, so a
/// reduction over an `array<bool>` can still feed a boxed `Mixed` result. The reduced
/// element is boxed with its own element type (so `min([true, false])` stays `bool`)
/// and int-typed elements promote when the result value is a float.
fn materialize_result(
    ctx: &mut FunctionContext<'_>,
    element: &PhpType,
    float_elements: bool,
    result_ty: &PhpType,
) -> Result<()> {
    match result_ty {
        PhpType::Mixed | PhpType::Union(_) => {
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, element);
            Ok(())
        }
        PhpType::Float if !float_elements => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Lowers `min($container)` / `max($container)` through a runtime reduction helper.
///
/// Handles the container shapes whose elements are not raw 8-byte scalar words:
/// indexed `array<string>`, indexed arrays of boxed `Mixed` cells, and hash-backed
/// associative arrays. The helper returns the winning element as an unboxed
/// `(tag, lo, hi)` triple, or tag `-1` for an empty container, which is turned into
/// PHP's catchable `ValueError` here.
fn lower_container_min_max(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    want_max: bool,
    reduction: ContainerReduction,
) -> Result<()> {
    let name = super::min_max_name(want_max);
    let array = expect_operand(inst, 0)?;
    let result_ty = inst
        .result
        .map(|value| ctx.value_php_type(value))
        .transpose()?
        .unwrap_or(PhpType::Mixed)
        .codegen_repr();
    let empty_label = ctx.next_label("min_max_container_empty");
    let done_label = ctx.next_label("min_max_container_done");
    let message = format!(
        "{}(): Argument #1 ($value) must contain at least one element",
        name
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    let direction = i64::from(want_max);

    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x1, #{}", direction));        // pass 1 for max() and 0 for min() as the reduction direction
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move the container pointer into the reduction argument register
            ctx.emitter.instruction(&format!("mov rsi, {}", direction));        // pass 1 for max() and 0 for min() as the reduction direction
        }
    }
    abi::emit_call_label(ctx.emitter, reduction.symbol());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmn x0, #1");                              // did the reduction report the empty-container tag?
            ctx.emitter.instruction(&format!("b.eq {}", empty_label));          // an empty container is PHP's ValueError, not a reduction
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                             // did the reduction report the empty-container tag?
            ctx.emitter.instruction(&format!("je {}", empty_label));            // an empty container is PHP's ValueError, not a reduction
        }
    }
    materialize_container_result(ctx, &result_ty, name)?;
    // The reduction materializes an independent result for scalars and boxed Mixed
    // values. Retire the owned container there instead of leaving it to the conservative
    // MayAliasArguments contract, which leaks one array per call. A static string/object/
    // array result keeps the container alive: its materialized payload can still borrow
    // the container storage, and releasing here corrupts the winning string on AArch64.
    let releasable = matches!(
        result_ty.codegen_repr(),
        PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Never
            | PhpType::Mixed
            | PhpType::Union(_)
    );
    // A local load is only `MaybeOwned`: its frame slot still owns the array and the scope
    // exit retires it. Releasing it here would free the caller's local after the first
    // min()/max(), so only an actually owned temporary (the array_values copy created for a
    // property or element place) is retired.
    if releasable && ctx.value_ownership(array)? == crate::ir::Ownership::Owned {
        abi::emit_push_result_value(ctx.emitter, &result_ty);
        crate::codegen::lower_inst::ownership::release_value_if_owned(ctx, array)?;
        match result_ty.codegen_repr() {
            PhpType::Float => {
                abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter))
            }
            PhpType::Void | PhpType::Never => {}
            _ => abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)),
        }
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    super::emit_throw_value_error(ctx, &message_label, message_len);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Converts a reduced `(tag, lo, hi)` triple into the EIR result value's representation.
///
/// The triple already sits in the registers `__rt_mixed_from_value` consumes and in the
/// registers a string result is returned in on AArch64, so the boxed and string cases
/// cost at most a register move. Numeric results carry a defensive tag check so an
/// element whose runtime tag disagrees with the inferred result type is converted
/// instead of reinterpreted.
fn materialize_container_result(
    ctx: &mut FunctionContext<'_>,
    result_ty: &PhpType,
    name: &str,
) -> Result<()> {
    match result_ty {
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            Ok(())
        }
        PhpType::Str => {
            if ctx.emitter.target.arch == Arch::X86_64 {
                ctx.emitter.instruction("mov rax, rdi");                        // publish the reduced string pointer in the string result register
                ctx.emitter.instruction("mov rdx, rsi");                        // publish the reduced string length in the string result register
            }
            // The reduction borrows the winning bytes from the container, which the
            // caller is free to release right after this call, so the result has to
            // own its own copy.
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            Ok(())
        }
        PhpType::Float => {
            let double_label = ctx.next_label("min_max_container_double");
            let ready_label = ctx.next_label("min_max_container_float_ready");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #2");                      // is the reduced element already a float payload?
                    ctx.emitter.instruction(&format!("b.eq {}", double_label)); // reinterpret its payload word directly
                    ctx.emitter.instruction("scvtf d0, x1");                    // widen an integer-like payload into the float result register
                    abi::emit_jump(ctx.emitter, &ready_label);
                    ctx.emitter.label(&double_label);
                    ctx.emitter.instruction("fmov d0, x1");                     // reinterpret the payload word as the double it encodes
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 2");                      // is the reduced element already a float payload?
                    ctx.emitter.instruction(&format!("je {}", double_label));   // reinterpret its payload word directly
                    ctx.emitter.instruction("cvtsi2sd xmm0, rdi");              // widen an integer-like payload into the float result register
                    abi::emit_jump(ctx.emitter, &ready_label);
                    ctx.emitter.label(&double_label);
                    ctx.emitter.instruction("movq xmm0, rdi");                  // reinterpret the payload word as the double it encodes
                }
            }
            ctx.emitter.label(&ready_label);
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            let ready_label = ctx.next_label("min_max_container_int_ready");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #2");                      // is the reduced element a float payload?
                    ctx.emitter.instruction(&format!("b.ne {}", ready_label));  // integer-like payloads publish unchanged
                    ctx.emitter.instruction("fmov d0, x1");                     // reinterpret the payload word as the double it encodes
                    ctx.emitter.instruction("fcvtzs x1, d0");                   // truncate the double toward zero like PHP's int cast
                    ctx.emitter.label(&ready_label);
                    ctx.emitter.instruction("mov x0, x1");                      // publish the reduced payload in the integer result register
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 2");                      // is the reduced element a float payload?
                    ctx.emitter.instruction(&format!("jne {}", ready_label));   // integer-like payloads publish unchanged
                    ctx.emitter.instruction("movq xmm0, rdi");                  // reinterpret the payload word as the double it encodes
                    ctx.emitter.instruction("cvttsd2si rdi, xmm0");             // truncate the double toward zero like PHP's int cast
                    ctx.emitter.label(&ready_label);
                    ctx.emitter.instruction("mov rax, rdi");                    // publish the reduced payload in the integer result register
                }
            }
            Ok(())
        }
        other => Err(unsupported_element_error(name, &format!("{}", other))),
    }
}

/// Emits the AArch64 integer reduction over an indexed array's payload slots.
fn emit_int_reduce_aarch64(
    ctx: &mut FunctionContext<'_>,
    want_max: bool,
    empty_label: &str,
    loop_label: &str,
    reduced_label: &str,
) {
    let exit_label = ctx.next_label("min_max_array_exit");
    let condition = if want_max { "gt" } else { "lt" };
    // -- seed the reduction with the first payload slot --
    ctx.emitter.instruction("ldr x9, [x0]");                                    // x9 = the array's logical element count from its header
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // an empty array is PHP's ValueError, not a reduction
    ctx.emitter.instruction(&format!("add x10, x0, #{}", ARRAY_DATA_OFFSET));   // x10 = address of the first payload slot
    ctx.emitter.instruction("ldr x11, [x10]");                                  // x11 = running result seeded with element 0
    ctx.emitter.instruction("mov x12, #1");                                     // x12 = cursor starting at the second element
    // -- fold every remaining element into the running result --
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the cursor against the element count
    ctx.emitter.instruction(&format!("b.ge {}", exit_label));                   // stop once every element has been folded in
    ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                     // x13 = the payload slot the cursor points at
    ctx.emitter.instruction("cmp x13, x11");                                    // compare the candidate against the running result
    ctx.emitter.instruction(&format!("csel x11, x13, x11, {}", condition));     // keep whichever element wins the min/max comparison
    ctx.emitter.instruction("add x12, x12, #1");                                // advance the cursor to the next payload slot
    abi::emit_jump(ctx.emitter, loop_label);
    ctx.emitter.label(&exit_label);
    ctx.emitter.instruction("mov x0, x11");                                     // publish the reduced element in the integer result register
    abi::emit_jump(ctx.emitter, reduced_label);
}

/// Emits the AArch64 floating reduction over an indexed array's payload slots.
fn emit_float_reduce_aarch64(
    ctx: &mut FunctionContext<'_>,
    want_max: bool,
    empty_label: &str,
    loop_label: &str,
    reduced_label: &str,
) {
    let exit_label = ctx.next_label("min_max_array_exit");
    let select = if want_max { "fmax" } else { "fmin" };
    // -- seed the reduction with the first payload slot --
    ctx.emitter.instruction("ldr x9, [x0]");                                    // x9 = the array's logical element count from its header
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // an empty array is PHP's ValueError, not a reduction
    ctx.emitter.instruction(&format!("add x10, x0, #{}", ARRAY_DATA_OFFSET));   // x10 = address of the first payload slot
    ctx.emitter.instruction("ldr d0, [x10]");                                   // d0 = running result seeded with element 0
    ctx.emitter.instruction("mov x12, #1");                                     // x12 = cursor starting at the second element
    // -- fold every remaining element into the running result --
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the cursor against the element count
    ctx.emitter.instruction(&format!("b.ge {}", exit_label));                   // stop once every element has been folded in
    ctx.emitter.instruction("ldr d1, [x10, x12, lsl #3]");                      // d1 = the payload slot the cursor points at
    ctx.emitter.instruction(&format!("{} d0, d1, d0", select));                 // keep whichever element wins the min/max comparison
    ctx.emitter.instruction("add x12, x12, #1");                                // advance the cursor to the next payload slot
    abi::emit_jump(ctx.emitter, loop_label);
    ctx.emitter.label(&exit_label);
    abi::emit_jump(ctx.emitter, reduced_label);
}

/// Emits the x86_64 integer reduction over an indexed array's payload slots.
fn emit_int_reduce_x86_64(
    ctx: &mut FunctionContext<'_>,
    want_max: bool,
    empty_label: &str,
    loop_label: &str,
    reduced_label: &str,
) {
    let exit_label = ctx.next_label("min_max_array_exit");
    let select = if want_max { "cmovg" } else { "cmovl" };
    // -- seed the reduction with the first payload slot --
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // r10 = the array's logical element count from its header
    ctx.emitter.instruction("test r10, r10");                                   // check whether the array holds any element at all
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // an empty array is PHP's ValueError, not a reduction
    ctx.emitter.instruction(&format!("lea r11, [rax + {}]", ARRAY_DATA_OFFSET));// r11 = address of the first payload slot
    ctx.emitter.instruction("mov rax, QWORD PTR [r11]");                        // rax = running result seeded with element 0
    ctx.emitter.instruction("mov rcx, 1");                                      // rcx = cursor starting at the second element
    // -- fold every remaining element into the running result --
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp rcx, r10");                                    // compare the cursor against the element count
    ctx.emitter.instruction(&format!("jge {}", exit_label));                    // stop once every element has been folded in
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");              // rdx = the payload slot the cursor points at
    ctx.emitter.instruction("cmp rdx, rax");                                    // compare the candidate against the running result
    ctx.emitter.instruction(&format!("{} rax, rdx", select));                   // keep whichever element wins the min/max comparison
    ctx.emitter.instruction("add rcx, 1");                                      // advance the cursor to the next payload slot
    abi::emit_jump(ctx.emitter, loop_label);
    ctx.emitter.label(&exit_label);
    abi::emit_jump(ctx.emitter, reduced_label);
}

/// Emits the x86_64 floating reduction over an indexed array's payload slots.
fn emit_float_reduce_x86_64(
    ctx: &mut FunctionContext<'_>,
    want_max: bool,
    empty_label: &str,
    loop_label: &str,
    reduced_label: &str,
) {
    let exit_label = ctx.next_label("min_max_array_exit");
    let select = if want_max { "maxsd" } else { "minsd" };
    // -- seed the reduction with the first payload slot --
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // r10 = the array's logical element count from its header
    ctx.emitter.instruction("test r10, r10");                                   // check whether the array holds any element at all
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // an empty array is PHP's ValueError, not a reduction
    ctx.emitter.instruction(&format!("lea r11, [rax + {}]", ARRAY_DATA_OFFSET));// r11 = address of the first payload slot
    ctx.emitter.instruction("movsd xmm0, QWORD PTR [r11]");                     // xmm0 = running result seeded with element 0
    ctx.emitter.instruction("mov rcx, 1");                                      // rcx = cursor starting at the second element
    // -- fold every remaining element into the running result --
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp rcx, r10");                                    // compare the cursor against the element count
    ctx.emitter.instruction(&format!("jge {}", exit_label));                    // stop once every element has been folded in
    ctx.emitter.instruction("movsd xmm1, QWORD PTR [r11 + rcx * 8]");           // xmm1 = the payload slot the cursor points at
    ctx.emitter.instruction(&format!("{} xmm0, xmm1", select));                 // keep whichever element wins the min/max comparison
    ctx.emitter.instruction("add rcx, 1");                                      // advance the cursor to the next payload slot
    abi::emit_jump(ctx.emitter, loop_label);
    ctx.emitter.label(&exit_label);
    abi::emit_jump(ctx.emitter, reduced_label);
}
