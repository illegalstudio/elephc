//! Purpose:
//! Emits `get_declared_classes()`, `get_declared_interfaces()`, and `get_declared_traits()`.
//! Materializes compile-time declaration registries as indexed string arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - Internal names are emitted first in deterministic order, then user declarations in source order.
//! - The fallback path sorts map keys for tests or callers that bypass normal codegen setup.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let mut names: Vec<String> = match name {
        "get_declared_classes" => crate::codegen::declared_class_names(),
        "get_declared_interfaces" => crate::codegen::declared_interface_names(),
        "get_declared_traits" => crate::codegen::declared_trait_names(),
        _ => return None,
    };
    if names.is_empty() {
        names = match name {
            "get_declared_classes" => ctx.classes.keys().cloned().collect(),
            "get_declared_interfaces" => ctx.interfaces.keys().cloned().collect(),
            "get_declared_traits" => ctx.traits.iter().cloned().collect(),
            _ => unreachable!(),
        };
        names.sort();
    }

    emitter.comment(&format!("{}() — AOT introspection snapshot", name));

    // -- allocate the result array with capacity = N, elem_size = 16 (str) --
    let cap = names.len().max(1);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", cap));                  // request capacity for one entry per declared name
            emitter.instruction("mov x1, #16");                                 // request 16-byte string slots so the array can store ptr+len pairs
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", cap));                  // request capacity for one entry per declared name
            emitter.instruction("mov rsi, 16");                                 // request 16-byte string slots so the array can store ptr+len pairs
        }
    }
    abi::emit_call_label(emitter, "__rt_array_new");                                    // allocate the introspection array through the shared array constructor

    if !names.is_empty() {
        emit_push_names(&names, emitter, data);
    }

    Some(PhpType::Array(Box::new(PhpType::Str)))
}

/// Push each name onto the array via `__rt_array_push_str`. The array
/// pointer is parked on the stack between iterations because
/// `__rt_array_push_str` may grow the storage and return a new pointer.
fn emit_push_names(names: &[String], emitter: &mut Emitter, data: &mut DataSection) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // park the indexed-array pointer while we push the declared-name entries
            for name in names {
                let (label, len) = data.add_string(name.as_bytes());
                emitter.instruction("ldr x0, [sp]");                            // reload the array pointer for this push call
                abi::emit_symbol_address(emitter, "x1", &label);                        // load the address of this name's string literal
                emitter.instruction(&format!("mov x2, #{}", len));              // load the length of this name's string literal
                emitter.instruction("bl __rt_array_push_str");                  // append the name and may grow the storage
                emitter.instruction("str x0, [sp]");                            // refresh the saved array pointer if __rt_array_push_str grew it
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the final array pointer as the builtin result
        }
        Arch::X86_64 => {
            emitter.instruction("push rax");                                    // park the indexed-array pointer while we push the declared-name entries
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for the call sequence
            for name in names {
                let (label, len) = data.add_string(name.as_bytes());
                emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // reload the array pointer for this push call
                abi::emit_symbol_address(emitter, "rsi", &label);                       // load the address of this name's string literal
                emitter.instruction(&format!("mov rdx, {}", len));              // load the length of this name's string literal
                emitter.instruction("call __rt_array_push_str");                // append the name and may grow the storage
                emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // refresh the saved array pointer if __rt_array_push_str grew it
            }
            emitter.instruction("add rsp, 8");                                  // pop the alignment padding before restoring the array pointer
            emitter.instruction("pop rax");                                     // restore the final array pointer as the builtin result
        }
    }
}
