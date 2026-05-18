//! Purpose:
//! Lowers `class_attribute_args()` calls into an indexed `array<mixed>` of
//! literal class-attribute arguments captured during schema construction.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Attribute matching is case-insensitive, and each captured scalar is boxed
//!   into a mixed cell before being appended to the result array.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{AttrArgValue, PhpType};

/// `class_attribute_args($class, $attr_name)`: return the positional
/// literal arguments of the named attribute attached to `$class` as an
/// indexed `array<mixed>`. Strings, ints, booleans, and null literals are
/// preserved with their original PHP types; the checker rejects calls that
/// would require unsupported metadata before codegen reaches this emitter.
///
/// Both arguments must be compile-time string literals — at codegen time
/// we look up `ClassInfo.attribute_args` and emit a sequence of
/// `__rt_mixed_from_value` + `__rt_array_push_int` calls. If the attribute
/// is not present on the class, the result is the empty array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("class_attribute_args()");
    let class_name = match args.first().map(|a| &a.kind) {
        Some(ExprKind::StringLiteral(name)) => name.clone(),
        _ => return Some(PhpType::Array(Box::new(PhpType::Mixed))),
    };
    let attr_name = match args.get(1).map(|a| &a.kind) {
        Some(ExprKind::StringLiteral(name)) => name.clone(),
        _ => return Some(PhpType::Array(Box::new(PhpType::Mixed))),
    };

    let attr_key = php_symbol_key(attr_name.trim_start_matches('\\'));
    let attr_args: Vec<AttrArgValue> = ctx
        .classes
        .get(super::resolve_class_name(ctx, &class_name)?)
        .and_then(|info| {
            info.attribute_names.iter().enumerate().find_map(|(idx, name)| {
                let candidate_key = php_symbol_key(name.trim_start_matches('\\'));
                if candidate_key == attr_key {
                    Some(
                        info.attribute_args
                            .get(idx)
                            .and_then(Clone::clone)
                            .unwrap_or_default(),
                    )
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    let result_reg = abi::int_result_reg(emitter);

    // -- allocate an empty indexed array of mixed-cell pointers --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_args.len().max(1))); // initial capacity (≥1 to avoid grow on first push)
            emitter.instruction("mov x1, #8");                                  // element stride: one heap pointer per slot
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated array pointer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", attr_args.len().max(1))); // initial capacity (≥1)
            emitter.instruction("mov rdx, 8");                                  // element stride: one heap pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = array pointer
        }
    }

    // Stamp the array's value_type so later iteration knows each slot is a
    // boxed mixed cell. The stamp lives in the heap header alongside the
    // indexed-array marker — without it `foreach` would not unbox the
    // mixed cells when the user iterates the result.
    crate::codegen::expr::arrays::emit_array_value_type_stamp(
        emitter,
        result_reg,
        &PhpType::Mixed,
    );

    // -- box each captured arg as a mixed cell and push the boxed pointer --
    for arg in &attr_args {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the array pointer across the boxing helper call
                emit_box_arg_aarch64(arg, emitter, data);                       // x0 = boxed mixed-cell pointer for this arg
                emitter.instruction("mov x1, x0");                              // x1 = mixed-cell pointer (push helper's value arg)
                emitter.instruction("ldr x0, [sp]");                            // x0 = array pointer (push helper's array arg)
                emitter.instruction("bl __rt_array_push_int");                  // x0 = (possibly realloc'd) array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved array slot now that the helper returned the up-to-date pointer
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the array pointer across the boxing helper call
                emit_box_arg_x86_64(arg, emitter, data);                        // rax = boxed mixed-cell pointer for this arg
                emitter.instruction("mov rsi, rax");                            // rsi = mixed-cell pointer (push helper's value arg)
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // rax = array pointer (push helper's array arg)
                emitter.instruction("call __rt_array_push_int");                // rax = updated array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date pointer
            }
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Mixed)))
}

fn emit_box_arg_aarch64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    // Set (tag in x0, lo in x1, hi in x2) per the mixed-cell ABI, then call
    // __rt_mixed_from_value. The helper persists strings and retains
    // refcounted heap children; scalars (int/bool/null) flow straight to
    // the alloc path with no ownership work.
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null payload
            emitter.instruction("mov x1, xzr");                                 // null mixed payloads carry no low word
            emitter.instruction("mov x2, xzr");                                 // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer payload
            emitter.instruction(&format!("mov x1, #{}", value));                // x1 = int value (low word)
            emitter.instruction("mov x2, xzr");                                 // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov x1, #{}", *value as u64));        // x1 = 0 or 1 boolean low word
            emitter.instruction("mov x2, xzr");                                 // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = string data address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = string length
        }
    }
    emitter.instruction("bl __rt_mixed_from_value");                            // box the captured payload into an owned mixed cell
}

fn emit_box_arg_x86_64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    // Set (tag in rax, lo in rdi, hi in rsi) per the mixed-cell ABI on x86_64.
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null payload
            emitter.instruction("xor rdi, rdi");                                // null mixed payloads carry no low word
            emitter.instruction("xor rsi, rsi");                                // null mixed payloads carry no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov rax, 0");                                  // runtime tag 0 = integer payload
            emitter.instruction(&format!("mov rdi, {}", value));                // rdi = int value (low word)
            emitter.instruction("xor rsi, rsi");                                // integer mixed payloads do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov rax, 3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov rdi, {}", *value as u64));        // rdi = 0 or 1 boolean low word
            emitter.instruction("xor rsi, rsi");                                // boolean mixed payloads do not use the high word
        }
        AttrArgValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (sym, len) = data.add_string(&bytes);
            emitter.instruction("mov rax, 1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "rdi", &sym);                     // rdi = string data address
            emitter.instruction(&format!("mov rsi, {}", len));                  // rsi = string length
        }
    }
    emitter.instruction("call __rt_mixed_from_value");                          // box the captured payload into an owned mixed cell
}
