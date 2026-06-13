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
use crate::types::{AttrArgEntry, AttrArgValue, PhpType};

/// Emits code for `class_attribute_args($class, $attr_name)`.
///
/// Returns `PhpType::Array(Box::new(PhpType::Mixed))` on success; on error
/// (non-literal args, missing class, or absent attribute) returns early with
/// the same type so the caller can proceed.  `ctx` provides the class lookup
/// via `ctx.classes`; attribute arguments are matched case-insensitively and
/// then boxed into mixed cells and pushed onto a newly allocated indexed array.
/// On x86_64 the result register is `rax`; on AArch64 it is `x0`.
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
    let attr_args: Vec<AttrArgEntry> = ctx
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
            emitter.instruction(&format!("mov rdi, {}", attr_args.len().max(1))); // initial capacity (≥1)
            emitter.instruction("mov rsi, 8");                                  // element stride: one heap pointer per slot
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
    for entry in &attr_args {
        let arg = &entry.value;
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
                emitter.instruction("mov rdi, QWORD PTR [rsp]");                // rdi = array pointer (push helper's array arg)
                emitter.instruction("call __rt_array_push_int");                // rax = updated array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date pointer
            }
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Emits AArch64 instructions to box `arg` into a runtime mixed cell.
///
/// Sets `x0` = runtime tag, `x1` = low word, `x2` = high word per the
/// mixed-cell ABI, then calls `__rt_mixed_from_value`.  Caller saves the
/// array pointer on the stack before this call (see call site in `emit`).
/// For `Str` args the string is added to `data` as a literal and the symbol
/// address is materialized into `x1`; `data` is only mutated for `Str`.
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
        AttrArgValue::Float(bits) => {
            emitter.instruction("mov x0, #2");                                  // runtime tag 2 = float payload
            abi::emit_load_int_immediate(emitter, "x1", *bits as i64);          // x1 = IEEE-754 bit pattern
            emitter.instruction("mov x2, xzr");                                 // float mixed payloads do not use the high word
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
        AttrArgValue::Array(_) | AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(..) => {
            // Frozen legacy AST backend: nested arrays and deferred symbolic
            // references (global/class constants, enum cases) are not
            // materialized here; emit a null placeholder. The active EIR path
            // builds the real value.
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null placeholder
            emitter.instruction("mov x1, xzr");                                 // null carries no low word
            emitter.instruction("mov x2, xzr");                                 // null carries no high word
        }
    }
    emitter.instruction("bl __rt_mixed_from_value");                            // box the captured payload into an owned mixed cell
}

/// Emits x86_64 instructions to box `arg` into a runtime mixed cell.
///
/// Sets `rax` = runtime tag, `rdi` = low word, `rsi` = high word per the
/// mixed-cell ABI, then calls `__rt_mixed_from_value`.  Caller saves the
/// array pointer on the stack before this call (see call site in `emit`).
/// For `Str` args the string is added to `data` as a literal and the symbol
/// address is materialized into `rdi`; `data` is only mutated for `Str`.
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
        AttrArgValue::Float(bits) => {
            emitter.instruction("mov rax, 2");                                  // runtime tag 2 = float payload
            abi::emit_load_int_immediate(emitter, "rdi", *bits as i64);         // rdi = IEEE-754 bit pattern
            emitter.instruction("xor rsi, rsi");                                // float mixed payloads do not use the high word
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
        AttrArgValue::Array(_) | AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(..) => {
            // Frozen legacy AST backend: nested arrays and deferred symbolic
            // references (global/class constants, enum cases) are not
            // materialized here; emit a null placeholder. The active EIR path
            // builds the real value.
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null placeholder
            emitter.instruction("xor rdi, rdi");                                // null carries no low word
            emitter.instruction("xor rsi, rsi");                                // null carries no high word
        }
    }
    emitter.instruction("call __rt_mixed_from_value");                          // box the captured payload into an owned mixed cell
}
