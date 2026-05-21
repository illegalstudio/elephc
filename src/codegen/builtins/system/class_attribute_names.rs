//! Purpose:
//! Lowers `class_attribute_names()` calls into an indexed array of class-level
//! PHP attribute names captured during schema construction.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Class lookup is case-insensitive and resolved at compile time from a
//!   string literal so the emitted code can unroll one push per attribute.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// `class_attribute_names($class)`: return an array of attribute name
/// strings attached to a class declaration. Currently the class argument
/// must be a compile-time string literal — at codegen time we look up
/// the `ClassInfo.attribute_names` list and emit a sequence of
/// `__rt_array_push_str` calls for each name. Dynamic class lookup
/// (string variable → class_id) is reserved for a future iteration.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("class_attribute_names()");
    let class_name = match args.first().map(|a| &a.kind) {
        Some(ExprKind::StringLiteral(name)) => name.clone(),
        _ => {
            // Type checker already rejects non-literal arguments — this is a
            // defensive fallback that returns an empty array of strings.
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
    };
    let names: Vec<String> = ctx
        .classes
        .get(super::resolve_class_name(ctx, &class_name)?)
        .map(|info| info.attribute_names.clone())
        .unwrap_or_default();

    let result_reg = abi::int_result_reg(emitter);

    // -- allocate an empty indexed array --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", names.len().max(1)));   // initial capacity (≥1 to avoid grow on first push)
            emitter.instruction("mov x1, #16");                                 // element stride: ptr (8 B) + len (8 B) per string slot
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated array pointer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", names.len().max(1)));   // initial capacity (≥1)
            emitter.instruction("mov rsi, 16");                                 // element stride: ptr (8 B) + len (8 B)
            emitter.instruction("call __rt_array_new");                         // rax = array pointer
        }
    }

    // -- push each name string in source order --
    for name in &names {
        let (sym, len) = data.add_string(name.as_bytes());
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the array pointer across the push helper call
                abi::emit_symbol_address(emitter, "x1", &sym);                  // x1 = attribute name string address
                emitter.instruction(&format!("mov x2, #{}", len));              // x2 = attribute name string length
                emitter.instruction("ldr x0, [sp]");                            // reload the array pointer for the push helper
                emitter.instruction("bl __rt_array_push_str");                  // x0 = (possibly realloc'd) array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved array pointer slot now that the push returned the up-to-date pointer
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the array pointer across the push helper call
                abi::emit_symbol_address(emitter, "rsi", &sym);                 // rsi = attribute name string address (System V arg 1 for str ptr)
                emitter.instruction(&format!("mov rdx, {}", len));              // rdx = attribute name length (System V arg 2)
                emitter.instruction("mov rdi, QWORD PTR [rsp]");                // rdi = current array pointer for the push helper
                emitter.instruction("call __rt_array_push_str");                // rax = updated array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date pointer
            }
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Str)))
}
