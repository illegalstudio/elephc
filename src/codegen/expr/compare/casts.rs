//! Purpose:
//! Lowers comparison-time casts and truthiness conversions.
//! Keeps comparison-specific branching and register normalization out of generic expression code.
//!
//! Called from:
//! - `crate::codegen::expr::compare`
//!
//! Key details:
//! - Null, type-tag, and string comparisons must follow PHP semantics before emitting boolean results.

use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{
    coerce_to_string_releasing_owned, coerce_to_truthiness, emit_expr, expr_result_heap_ownership,
};

/// Emits a PHP cast expression (`(int)`, `(float)`, `(string)`, `(bool)`, `(array)`).
///
/// # Arguments
/// - `target` — the cast kind (Int, Float, String, Bool, Array)
/// - `expr` — the expression to cast; must already be emitted so `ctx` holds the result type in `src_ty`
/// - `emitter` — target-aware instruction emitter
/// - `ctx` — codegen context; receives the cast result type
/// - `data` — data section for relocations and static data
///
/// # Returns
/// The `PhpType` that results from the cast (e.g., `PhpType::Int` for `(int)`).
///
/// # PHP cast semantics
/// - `(int)` from string → calls `__rt_str_to_int`; from resource → native payload + 1; from array → container length
/// - `(float)` from string → null-terminates via `__rt_cstr` then calls `atof`; from resource → id + conversion
/// - `(bool)` → uses shared truthiness coercion (`coerce_to_truthiness`)
/// - `(string)` → delegates to `coerce_to_string`
/// - `(array)` from scalar → allocates 1-element array via `__rt_array_new` / `__rt_array_push_int`; otherwise empty 4-capacity array
pub(in crate::codegen::expr) fn emit_cast(
    target: &crate::parser::ast::CastType,
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    use crate::parser::ast::CastType;
    let src_ty = emit_expr(expr, emitter, ctx, data);
    emitter.comment(&format!("cast to {:?}", target));
    match target {
        CastType::Int => {
            match &src_ty {
                PhpType::Int => {}
                PhpType::Float => {
                    abi::emit_float_result_to_int_result(emitter);              // convert double to signed 64-bit int (toward zero)
                }
                PhpType::Bool => {}
                PhpType::Void | PhpType::Never => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                }
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_str_to_int");           // parse the current string result through PHP string-to-int cast rules
                }
                PhpType::Resource(_) => match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction("add x0, x0, #1");                  // convert the native resource payload into the 1-based display id
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction("add rax, 1");                      // convert the native resource payload into the 1-based display id
                    }
                },
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    emitter.instruction("ldr x0, [x0]");                        // load array/hash container length from header (first field; iterable hash kind shares this layout)
                }
                PhpType::Iterable => {
                    emit_iterable_nonempty_as_int(emitter, ctx);                 // PHP casts array-backed iterables to 0/1 based on emptiness
                }
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_int");       // cast the boxed mixed payload to int through the target-aware helper
                }
                PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {}
            }
            PhpType::Int
        }
        CastType::Float => {
            match &src_ty {
                PhpType::Float => {}
                PhpType::Int | PhpType::Bool => {
                    abi::emit_int_result_to_float_result(emitter);              // signed int to double conversion
                }
                PhpType::Resource(_) => {
                    match emitter.target.arch {
                        crate::codegen::platform::Arch::AArch64 => {
                            emitter.instruction("add x0, x0, #1");              // convert the native resource payload into the 1-based display id
                        }
                        crate::codegen::platform::Arch::X86_64 => {
                            emitter.instruction("add rax, 1");                  // convert the native resource payload into the 1-based display id
                        }
                    }
                    abi::emit_int_result_to_float_result(emitter);              // convert the resource display id to double
                }
                PhpType::Void | PhpType::Never => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                    abi::emit_int_result_to_float_result(emitter);              // convert to 0.0 double
                }
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_cstr");                 // null-terminate the current string result through the target-aware C-string helper
                    if emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
                        emitter.instruction("mov rdi, rax");                    // pass the null-terminated C string in the SysV first-argument register before atof()
                    }
                    emitter.bl_c("atof");                            // parse C string as double → d0=result
                }
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // cast the boxed mixed payload to float through the target-aware helper
                }
                PhpType::Iterable => {
                    emit_iterable_nonempty_as_int(emitter, ctx);                 // PHP casts array-backed iterables to 0/1 before float widening
                    abi::emit_int_result_to_float_result(emitter);              // convert the normalized iterable integer cast to double
                }
                PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                    abi::emit_int_result_to_float_result(emitter);              // convert to 0.0 double (iterable joins the array group for elephc cast semantics)
                }
            }
            PhpType::Float
        }
        CastType::String => {
            coerce_to_string_releasing_owned(
                emitter,
                ctx,
                data,
                &src_ty,
                expr_result_heap_ownership(expr) == HeapOwnership::Owned,
            );
            PhpType::Str
        }
        CastType::Bool => {
            coerce_to_truthiness(emitter, ctx, &src_ty);                        // normalize any source value to PHP truthiness using the shared target-aware helper path
            PhpType::Bool
        }
        CastType::Array => {
            match &src_ty {
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    return src_ty;
                }
                PhpType::Int
                | PhpType::Bool
                | PhpType::Resource(_)
                | PhpType::Callable
                | PhpType::Buffer(_)
                | PhpType::Packed(_) => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // save scalar value during allocation
                    emitter.instruction("mov x0, #1");                          // capacity: 1 element (exact fit)
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate new array struct
                    emitter.instruction("ldr x1, [sp], #16");                   // pop saved scalar value
                    emitter.instruction("bl __rt_array_push_int");              // push scalar as first element
                }
                _ => {
                    emitter.instruction("mov x0, #4");                          // capacity: 4 (grows dynamically)
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate empty array struct
                }
            }
            PhpType::Array(Box::new(PhpType::Int))
        }
    }
}

/// Emits PHP iterable-to-int casting for `(int)` and `(float)` on `PhpType::Iterable`.
///
/// Classifies the iterable's heap kind (array/hash/object/null) then emits
/// the PHP-appropriate integer: arrays/hashes → 1 if non-empty else 0; objects → 1; null → 0.
///
/// # Arguments
/// - `emitter` — target-aware instruction emitter; must have the iterable pointer in `int_result_reg`
/// - `ctx` — codegen context; used to allocate local labels
///
/// # Side effects
/// - Caller must preserve the iterable pointer before calling (function pushes it on the stack)
/// - Consumes the preserved pointer and stack space before branching to `done`
fn emit_iterable_nonempty_as_int(emitter: &mut Emitter, ctx: &mut Context) {
    let array_case = ctx.next_label("iterable_cast_array");
    let true_case = ctx.next_label("iterable_cast_true");
    let false_case = ctx.next_label("iterable_cast_false");
    let done = ctx.next_label("iterable_cast_done");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the erased iterable pointer while checking its heap kind
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify the iterable payload by heap kind before reading its layout
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // is the iterable backed by an indexed array?
            emitter.instruction(&format!("b.eq {}", array_case));               // arrays cast by checking whether their length is non-zero
            emitter.instruction("cmp x0, #3");                                  // is the iterable backed by an associative array?
            emitter.instruction(&format!("b.eq {}", array_case));               // hashes cast by checking whether their length is non-zero
            emitter.instruction("cmp x0, #4");                                  // is the iterable backed by an object?
            emitter.instruction(&format!("b.eq {}", true_case));                // objects cast to 1 like PHP object values
            emitter.instruction(&format!("b {}", false_case));                  // null or unknown payloads cast to 0

            emitter.label(&array_case);
            abi::emit_pop_reg(emitter, "x9");                                   // restore the array/hash pointer for the length read
            emitter.instruction("ldr x0, [x9]");                                // load the runtime container length from the shared header
            emitter.instruction("cmp x0, #0");                                  // check whether the iterable container is empty
            emitter.instruction("cset x0, ne");                                 // PHP numeric array casts return 1 for non-empty arrays
            emitter.instruction(&format!("b {}", done));                        // finish after materializing the array-backed cast result

            emitter.label(&true_case);
            emitter.instruction("add sp, sp, #16");                             // discard the preserved iterable pointer before returning 1
            emitter.instruction("mov x0, #1");                                  // object-backed iterables cast to integer 1
            emitter.instruction(&format!("b {}", done));                        // finish after materializing the truthy object cast

            emitter.label(&false_case);
            emitter.instruction("add sp, sp, #16");                             // discard the preserved iterable pointer before returning 0
            emitter.instruction("mov x0, #0");                                  // null or unknown iterable payloads cast to integer 0
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // is the iterable backed by an indexed array?
            emitter.instruction(&format!("je {}", array_case));                 // arrays cast by checking whether their length is non-zero
            emitter.instruction("cmp rax, 3");                                  // is the iterable backed by an associative array?
            emitter.instruction(&format!("je {}", array_case));                 // hashes cast by checking whether their length is non-zero
            emitter.instruction("cmp rax, 4");                                  // is the iterable backed by an object?
            emitter.instruction(&format!("je {}", true_case));                  // objects cast to 1 like PHP object values
            emitter.instruction(&format!("jmp {}", false_case));                // null or unknown payloads cast to 0

            emitter.label(&array_case);
            abi::emit_pop_reg(emitter, "r10");                                  // restore the array/hash pointer for the length read
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // load the runtime container length from the shared header
            emitter.instruction("test rax, rax");                               // check whether the iterable container is empty
            emitter.instruction("setne al");                                    // PHP numeric array casts return 1 for non-empty arrays
            emitter.instruction("movzx rax, al");                               // widen the boolean byte to the canonical integer result
            emitter.instruction(&format!("jmp {}", done));                      // finish after materializing the array-backed cast result

            emitter.label(&true_case);
            abi::emit_pop_reg(emitter, "r10");                                  // discard the preserved iterable pointer before returning 1
            emitter.instruction("mov rax, 1");                                  // object-backed iterables cast to integer 1
            emitter.instruction(&format!("jmp {}", done));                      // finish after materializing the truthy object cast

            emitter.label(&false_case);
            abi::emit_pop_reg(emitter, "r10");                                  // discard the preserved iterable pointer before returning 0
            emitter.instruction("xor eax, eax");                                // null or unknown iterable payloads cast to integer 0
        }
    }
    emitter.label(&done);
}
