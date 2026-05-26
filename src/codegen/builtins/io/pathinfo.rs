//! Purpose:
//! Emits PHP `pathinfo` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits code for PHP's `pathinfo($path, $options)` builtin.
///
/// `args[0]` is the path string (emitted first, returning ptr/len in x0/x1 or rax/rdx).
/// `args[1]`, if present, is the options flag.
///
/// Returns `PhpType::AssocArray{Str, Str}` when no flag or `PATHINFO_ALL` is determined at
/// compile time, `PhpType::Str` for a static single-flag value, or `PhpType::Mixed` when the
/// flag is dynamic or involves runtime-only constants.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pathinfo()");
    emit_expr(&args[0], emitter, ctx, data);
    let static_flag = pathinfo_static_flag_value(args.get(1));
    if args.len() == 1 || static_flag == Some(15) {
        // No-flag form: build the associative array via the runtime helper.
        abi::emit_call_label(emitter, "__rt_pathinfo_array");                   // call the runtime helper that builds the dirname/basename/extension/filename hash
        // The hash pointer comes back in x0 / rax — that is already the
        // standard integer-result register used everywhere else for hash-typed
        // expression results.
        return Some(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        });
    }
    if static_flag.is_none() {
        emit_dynamic_pathinfo(&args[1], emitter, ctx, data);
        return Some(PhpType::Mixed);
    }
    // Single-flag form.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the path ptr/len while the flag expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move the flag value into the runtime's flag register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the path ptr/len after evaluating the flag expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the path ptr/len while the flag expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the flag value into the x86_64 runtime flag register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the path ptr/len after evaluating the flag expression
        }
    }
    abi::emit_call_label(emitter, "__rt_pathinfo_str");                         // call the target-aware single-flag runtime helper that returns the requested component
    Some(PhpType::Str)
}

/// Emits code for a runtime-dynamic `pathinfo($path, $flag)` call.
///
/// The path ptr/len is already in registers from emitting `args[0]`. This function evaluates
/// the flag expression, then dispatches to either `__rt_pathinfo_str` (single component) or
/// `__rt_pathinfo_array` (PATHINFO_ALL), boxing the result as `PhpType::Mixed`. For non-all
/// flags the returned string is boxed as a mixed scalar; for PATHINFO_ALL the associative
/// array is boxed as a mixed container.
fn emit_dynamic_pathinfo(
    flag: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let array_label = ctx.next_label("pathinfo_dynamic_array");
    let done_label = ctx.next_label("pathinfo_dynamic_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the path ptr/len while the runtime flag expression is evaluated
            emit_expr(flag, emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // keep the evaluated flag in the pathinfo string-helper flag register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the path ptr/len for whichever runtime branch is selected
            emitter.instruction("cmp x3, #15");                                 // does the runtime flag request PATHINFO_ALL exactly?
            emitter.instruction(&format!("b.eq {}", array_label));              // runtime PATHINFO_ALL must return the associative-array shape
            abi::emit_call_label(emitter, "__rt_pathinfo_str");                 // compute the requested component string for all non-all runtime flags
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string for the mixed boxing helper
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the string branch as a mixed result
            emitter.instruction(&format!("b {}", done_label));                  // skip the array-shape branch after boxing the string result
            emitter.label(&array_label);
            abi::emit_call_label(emitter, "__rt_pathinfo_array");               // build the full associative array for runtime PATHINFO_ALL
            box_owned_pathinfo_array_as_mixed(emitter);
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the path ptr/len while the runtime flag expression is evaluated
            emit_expr(flag, emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // keep the evaluated flag in the x86_64 pathinfo flag register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the path ptr/len for the selected runtime branch
            emitter.instruction("cmp rdi, 15");                                 // does the runtime flag request PATHINFO_ALL exactly?
            emitter.instruction(&format!("je {}", array_label));                // runtime PATHINFO_ALL must return the associative-array shape
            abi::emit_call_label(emitter, "__rt_pathinfo_str");                 // compute the requested component string for all non-all runtime flags
            emitter.instruction("mov rdi, rax");                                // pass the component string pointer as the mixed payload low word
            emitter.instruction("mov rsi, rdx");                                // pass the component string length as the mixed payload high word
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string for the mixed boxing helper
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the string branch as a mixed result
            emitter.instruction(&format!("jmp {}", done_label));                // skip the array-shape branch after boxing the string result
            emitter.label(&array_label);
            abi::emit_call_label(emitter, "__rt_pathinfo_array");               // build the full associative array for runtime PATHINFO_ALL
            box_owned_pathinfo_array_as_mixed(emitter);
            emitter.label(&done_label);
        }
    }
}

/// Boxes a freshly owned pathinfo hash as a `PhpType::Mixed` cell in the runtime heap.
///
/// The owned hash pointer is consumed from x0/rax (caller's result register). Allocates a
/// 24-byte mixed cell (tag + two payload words), stamps it with heap kind 5, stores the
/// associative-array tag (5) at mixed[0] and the hash pointer at mixed[8], and zero-fills
/// mixed[16] (associative-array payloads do not use the high word). Preserves the hash
/// pointer by pushing/popping through a scratch register so the allocation itself does not
/// consume the owner.
fn box_owned_pathinfo_array_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0");                                  // preserve the freshly owned hash while allocating the mixed cell
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the boxed runtime result cell
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the heap allocation as a mixed cell
            emitter.instruction("mov x9, #5");                                  // runtime tag 5 = associative array payload
            emitter.instruction("str x9, [x0]");                                // store the array tag at mixed[0]
            abi::emit_pop_reg(emitter, "x10");                                  // reload the owned pathinfo hash pointer
            emitter.instruction("str x10, [x0, #8]");                           // store the hash pointer without retaining the fresh owner
            emitter.instruction("str xzr, [x0, #16]");                          // associative-array mixed payloads do not use a high word
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the freshly owned hash while allocating the mixed cell
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the boxed runtime result cell
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the mixed-cell heap kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the heap allocation as a mixed cell
            emitter.instruction("mov QWORD PTR [rax], 5");                      // store runtime tag 5 = associative array payload
            abi::emit_pop_reg(emitter, "r10");                                  // reload the owned pathinfo hash pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the hash pointer without retaining the fresh owner
            emitter.instruction("mov QWORD PTR [rax + 16], 0");                 // associative-array mixed payloads do not use a high word
        }
    }
}

/// Statically evaluates a `pathinfo` options argument at compile time.
///
/// Walks the expression tree to extract a literal integer or known `PATHINFO_*` constants.
/// Handles negation and bitwise AND/OR/XOR combinations of static operands. Returns `None`
/// when the flag cannot be resolved statically (e.g., variable, function call, or unsupported
/// operator), allowing the caller to emit runtime-dynamic dispatch.
fn pathinfo_static_flag_value(flag: Option<&Expr>) -> Option<i64> {
    match flag.map(|expr| &expr.kind) {
        Some(ExprKind::IntLiteral(value)) => Some(*value),
        Some(ExprKind::ConstRef(name)) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        Some(ExprKind::Negate(inner)) => {
            pathinfo_static_flag_value(Some(inner.as_ref())).map(|value| -value)
        }
        Some(ExprKind::BinaryOp { left, op, right }) => {
            let left = pathinfo_static_flag_value(Some(left.as_ref()))?;
            let right = pathinfo_static_flag_value(Some(right.as_ref()))?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
