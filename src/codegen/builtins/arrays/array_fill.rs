//! Purpose:
//! Emits PHP `array_fill` builtin calls that allocate or reshape array values.
//! Coordinates element type selection with runtime helpers that build indexed or associative arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Returned arrays must use the payload layout expected by later codegen and GC/refcount paths.
//! - A non-zero start index, or a string fill value, needs a keyed (hash) result because a
//!   0-based indexed array cannot represent keys `start..start+count-1` and the scalar indexed
//!   fill cannot store a string pointer+length. Those cases route through `__rt_array_fill_assoc`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::platform::Arch;
use crate::codegen::runtime_value_tag;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Returns the assoc-result type produced by `__rt_array_fill_assoc` (int keys, boxed values).
fn assoc_fill_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Mixed),
    }
}

/// Returns true when `array_fill` must build a keyed (hash) array rather than a 0-based
/// indexed one. A non-literal-zero start index produces keys `start..start+count-1`, which a
/// 0-based indexed array cannot represent, so it always goes through the keyed path. A
/// literal-zero start with a string value still uses the dedicated `__rt_array_fill_str`
/// indexed path (the 0-based indexed string array is what `array_fill(0, n, "ab")` should
/// return — `[0=>"ab", 1=>"ab", ...]`, not a hash).
fn needs_assoc_fill(start_arg: &Expr, _value_ty: &PhpType) -> bool {
    let start_is_literal_zero = matches!(start_arg.kind, ExprKind::IntLiteral(0));
    !start_is_literal_zero
}

/// Emits the `array_fill(start_index, count, value)` builtin call.
///
/// Evaluates arguments left-to-right, pushing `start_index` and `count` on the stack before
/// evaluating `value` to preserve ordering. A literal-zero start with a scalar/refcounted value
/// uses the indexed `__rt_array_fill`/`__rt_array_fill_refcounted` helpers; a non-zero start or
/// a string value routes through `__rt_array_fill_assoc`, which builds a Mixed-valued hash with
/// keys `start..start+count-1`. On x86_64 Linux, delegates to `emit_array_fill_linux_x86_64`.
///
/// Returns `PhpType::Array(value_ty)` for the indexed path or `AssocArray{Int, Mixed}` for the
/// keyed path.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_fill()");
    if emitter.target.arch == Arch::X86_64 {
        return emit_array_fill_linux_x86_64(args, emitter, ctx, data);
    }

    let start_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_int(emitter, &start_ty);                                          // unbox a Mixed/Union start index into a raw integer
    // -- save start index, evaluate count --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start index onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- save count, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push count onto stack
    let value_ty = emit_expr(&args[2], emitter, ctx, data);

    if needs_assoc_fill(&args[0], &value_ty) {
        // -- marshal the fill value into value_lo (x2), value_hi (x3), value_tag (x4) --
        match value_ty.codegen_repr() {
            PhpType::Str => {
                emitter.instruction("mov x3, x2");                              // string length becomes the value high word
                emitter.instruction("mov x2, x1");                              // string pointer becomes the value low word
            }
            PhpType::Float => {
                emitter.instruction("fmov x2, d0");                             // move the float bits into the value low word
                emitter.instruction("mov x3, #0");                              // floats use no high word
            }
            _ => {
                emitter.instruction("mov x2, x0");                              // scalar value or heap pointer becomes the value low word
                emitter.instruction("mov x3, #0");                              // non-string payloads use no high word
            }
        }
        abi::emit_load_int_immediate(emitter, "x4", runtime_value_tag(&value_ty) as i64); // runtime value tag for per-slot boxing
        emitter.instruction("ldr x1, [sp], #16");                               // pop count into x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop start index into x0 (first arg)
        emitter.instruction("bl __rt_array_fill_assoc");                        // build a keyed hash with keys start..start+count-1
        return Some(assoc_fill_type());
    }

    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        // -- literal-zero start with a string value: use the dedicated indexed string path --
        // String ABI: x1 = pointer, x2 = length; x0 still holds the count we pushed on the stack.
        // Marshal to (x0=count, x1=ptr, x2=len): pop the count, discard the (literal-zero) start.
        emitter.instruction("ldr x0, [sp], #16");                               // pop count into x0 (first arg)
        emitter.instruction("ldr x9, [sp], #16");                               // pop and discard the (literal-zero) start index
        emitter.instruction("bl __rt_array_fill_str");                          // build the indexed string array via repeated push_str
        return Some(PhpType::Array(Box::new(PhpType::Str)));
    }

    let uses_refcounted_runtime = value_ty.is_refcounted();
    // -- set up three-arg call: start, count, value --
    emitter.instruction("mov x2, x0");                                          // move fill value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop count into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start index into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_fill_refcounted"
    } else {
        "bl __rt_array_fill"
    };
    emitter.instruction(runtime_call);                                          // call runtime: fill array → x0=new array

    Some(PhpType::Array(Box::new(value_ty)))
}

/// x86_64 Linux-specific entry point for `array_fill`.
///
/// Uses System V AMD64 ABI: `rdi` = start_index, `rsi` = count, `rdx` = fill value (or
/// value_lo for the keyed path). The keyed path additionally passes value_hi in `rcx` and the
/// runtime value tag in `r8`, then calls `__rt_array_fill_assoc`.
///
/// Returns `PhpType::Array(value_ty)` for the indexed path or `AssocArray{Int, Mixed}`.
fn emit_array_fill_linux_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let start_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_int(emitter, &start_ty);                                          // unbox a Mixed/Union start index into a raw integer
    abi::emit_push_reg(emitter, "rax");                                         // preserve the start index while evaluating the count and fill value arguments
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, "rax");                                         // preserve the count while evaluating the fill value argument
    let value_ty = emit_expr(&args[2], emitter, ctx, data);

    if needs_assoc_fill(&args[0], &value_ty) {
        // -- marshal the fill value into value_lo (rdx), value_hi (rcx), value_tag (r8) --
        match value_ty.codegen_repr() {
            PhpType::Str => {
                emitter.instruction("mov rcx, rdx");                            // string length becomes the value high word
                emitter.instruction("mov rdx, rax");                            // string pointer becomes the value low word
            }
            PhpType::Float => {
                emitter.instruction("movq rdx, xmm0");                          // move the float bits into the value low word
                emitter.instruction("xor rcx, rcx");                            // floats use no high word
            }
            _ => {
                emitter.instruction("mov rdx, rax");                            // scalar value or heap pointer becomes the value low word
                emitter.instruction("xor rcx, rcx");                            // non-string payloads use no high word
            }
        }
        abi::emit_load_int_immediate(emitter, "r8", runtime_value_tag(&value_ty) as i64); // runtime value tag for per-slot boxing
        abi::emit_pop_reg(emitter, "rsi");                                      // restore the requested count into the second argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the start index into the first argument register
        abi::emit_call_label(emitter, "__rt_array_fill_assoc");                 // build a keyed hash with keys start..start+count-1
        return Some(assoc_fill_type());
    }

    if matches!(value_ty.codegen_repr(), PhpType::Str) {
        // -- literal-zero start with a string value: use the dedicated indexed string path --
        // String ABI: rax = pointer, rdx = length. Marshal to (rdi=count, rsi=ptr, rdx=len).
        abi::emit_push_reg(emitter, "rdx");                                      // preserve the string length across the rsi move
        emitter.instruction("mov rsi, rax");                                    // string pointer into the second runtime argument register
        abi::emit_pop_reg(emitter, "rdx");                                      // restore the string length into the third runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // pop count into the first runtime argument register
        abi::emit_pop_reg(emitter, "r11");                                      // pop and discard the (literal-zero) start index
        abi::emit_call_label(emitter, "__rt_array_fill_str");                   // build the indexed string array via repeated push_str
        return Some(PhpType::Array(Box::new(PhpType::Str)));
    }

    if matches!(value_ty, PhpType::Float) {
        emitter.instruction("movq rdx, xmm0");                                  // move the floating-point fill payload bits into the third x86_64 runtime argument register
    } else {
        emitter.instruction("mov rdx, rax");                                    // place the fill payload in the third x86_64 runtime argument register
    }
    abi::emit_pop_reg(emitter, "rsi");                                          // restore the requested count into the second x86_64 runtime argument register
    abi::emit_pop_reg(emitter, "rdi");                                          // restore the start index into the first x86_64 runtime argument register
    if value_ty.is_refcounted() {
        abi::emit_call_label(emitter, "__rt_array_fill_refcounted");            // build an indexed array by repeatedly retaining the borrowed heap payload
    } else {
        abi::emit_call_label(emitter, "__rt_array_fill");                       // build a scalar indexed array through the plain fill runtime helper
    }

    Some(PhpType::Array(Box::new(value_ty)))
}
