//! Purpose:
//! Lowers `class_get_attributes()` into an indexed array of populated
//! synthetic `ReflectionAttribute` objects for class-level attributes.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - The objects are internally populated by writing private metadata slots,
//!   while user code only sees `getName()` and `getArguments()`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::expr::objects::emit_new_object;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{AttrArgValue, PhpType};

/// `class_get_attributes($class)`: return an indexed array of populated
/// `ReflectionAttribute` instances, one per attribute attached to the
/// class declaration. The class argument must be a compile-time string
/// literal — at codegen time we walk `ClassInfo.attribute_names` and
/// `ClassInfo.attribute_args` to fully unroll the construction sequence.
///
/// For each attribute we:
///   1. Allocate a `ReflectionAttribute` instance via `emit_new_object`
///   2. Overwrite `$__name` with the attribute's source-order name string
///   3. Build a fresh `array<mixed>` of literal args (string / int / bool /
///      null) and store it in `$__args`
///   4. Push the populated object pointer into the result indexed array
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("class_get_attributes()");
    let class_name = match args.first().map(|a| &a.kind) {
        Some(ExprKind::StringLiteral(name)) => name.clone(),
        _ => return Some(PhpType::Array(Box::new(PhpType::Object("ReflectionAttribute".to_string())))),
    };

    let class_info = match super::resolve_class_name(ctx, &class_name)
        .and_then(|resolved| ctx.classes.get(resolved))
        .cloned()
    {
        Some(info) => info,
        None => {
            return Some(PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            ))))
        }
    };
    let attr_names = class_info.attribute_names.clone();
    let attr_args = class_info.attribute_args.clone();

    let result_reg = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);

    // -- allocate the result indexed array (one heap-pointer slot per attr) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_names.len().max(1))); // initial capacity (≥1 to avoid grow on first push)
            emitter.instruction("mov x1, #8");                                  // element stride: one heap pointer per slot (object handle)
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated array pointer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", attr_names.len().max(1))); // initial capacity (≥1)
            emitter.instruction("mov rdx, 8");                                  // element stride: one heap pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = array pointer
        }
    }
    emit_array_value_type_stamp(
        emitter,
        result_reg,
        &PhpType::Object("ReflectionAttribute".to_string()),
    );

    for (idx, attr_name) in attr_names.iter().enumerate() {
        let empty_args = Vec::new();
        let attr_arg_list = attr_args
            .get(idx)
            .and_then(Option::as_ref)
            .unwrap_or(&empty_args);

        // -- save the result array pointer below later temporaries --
        abi::emit_push_reg(emitter, result_reg);

        // -- allocate a fresh ReflectionAttribute via the normal new path --
        // emit_new_object walks the registered class and runs its private
        // synthetic zero-arg constructor; this internal emitter is the only
        // code path that can populate ReflectionAttribute metadata slots.
        emit_new_object(
            "ReflectionAttribute",
            &[],
            emitter,
            ctx,
            data,
        );

        // The new object pointer is now in the result reg. Save it below
        // both the array pointer and the spilled per-property scratch
        // values that follow.
        abi::emit_push_reg(emitter, result_reg);

        // -- overwrite `$__name` (offset 8 = lo, 16 = hi) --
        emit_set_name_property(emitter, data, attr_name, scratch);

        // -- build the mixed args array and overwrite `$__args` --
        emit_set_args_property(emitter, data, attr_arg_list, scratch);

        // -- push the populated object pointer into the result array --
        // After emit_set_args_property, the spilled object pointer is still
        // on the stack one slot below the result array. Pop both back, push.
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop the populated ReflectionAttribute pointer into the value-arg register
                emitter.instruction("ldr x0, [sp], #16");                       // pop the result array pointer into the array-arg register
                emitter.instruction("bl __rt_array_push_int");                  // append the object handle to the result array
            }
            Arch::X86_64 => {
                emitter.instruction("pop rsi");                                 // pop the populated ReflectionAttribute pointer into the value-arg register
                emitter.instruction("pop rax");                                 // pop the result array pointer into the array-arg register
                emitter.instruction("call __rt_array_push_int");                // append the object handle to the result array
            }
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Object(
        "ReflectionAttribute".to_string(),
    ))))
}

/// Overwrite the freshly-allocated ReflectionAttribute's `$__name` slot
/// with a heap-persisted copy of `attr_name`. The object pointer is
/// expected at the top of the temporary stack — we leave it there so the
/// caller can keep using it.
fn emit_set_name_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    attr_name: &str,
    obj_ptr_scratch: &str,
) {
    let (sym, len) = data.add_string(attr_name.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            // Free the old default-initialised string (empty literal) before
            // overwriting the slot — the default's heap copy was allocated
            // by emit_new_object and is now unreachable.
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer from the temporary stack
            emitter.instruction("ldr x0, [x9, #8]");                            // load the old __name.lo (heap-resident default copy)
            emitter.instruction("bl __rt_heap_free_safe");                      // release the previous owned name string
            // Persist the new attribute name into freshly heap-owned storage.
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = source string address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = source string length
            emitter.instruction("bl __rt_str_persist");                         // x1 = heap-resident pointer, x2 = length
            // Re-load the obj pointer (the helper may have clobbered scratch).
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer back
            emitter.instruction(&format!("str x1, [{}, #8]", obj_ptr_scratch)); // commit __name.lo (heap pointer)
            emitter.instruction(&format!("str x2, [{}, #16]", obj_ptr_scratch)); // commit __name.hi (length)
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // load old __name.lo for the free helper
            emitter.instruction("call __rt_heap_free_safe");                    // release the previous owned name string
            abi::emit_symbol_address(emitter, "rax", &sym);                     // rax = source string address
            emitter.instruction(&format!("mov rdx, {}", len));                  // rdx = source string length
            emitter.instruction("call __rt_str_persist");                       // rax = heap-resident pointer, rdx = length
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer back
            emitter.instruction(&format!("mov QWORD PTR [{} + 8], rax", obj_ptr_scratch)); // commit __name.lo
            emitter.instruction(&format!("mov QWORD PTR [{} + 16], rdx", obj_ptr_scratch)); // commit __name.hi (length)
        }
    }
}

/// Overwrite the freshly-allocated ReflectionAttribute's `$__args` slot
/// with a fresh `array<mixed>` built from `attr_arg_list`. The object
/// pointer is expected at the top of the temporary stack and is left
/// undisturbed for the caller's downstream push.
fn emit_set_args_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    attr_arg_list: &[AttrArgValue],
    obj_ptr_scratch: &str,
) {
    let result_reg = abi::int_result_reg(emitter);

    // -- decref the previous default `[]` value before overwriting --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // peek the obj pointer
            emitter.instruction("ldr x0, [x9, #24]");                           // load old __args.lo (heap array pointer)
            emitter.instruction("bl __rt_decref_array");                        // release the previous default empty array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // peek the obj pointer
            emitter.instruction("mov rax, QWORD PTR [r10 + 24]");               // load old __args.lo
            emitter.instruction("call __rt_decref_array");                      // release the previous default empty array
        }
    }

    // -- allocate a fresh mixed-cell pointer array for the literal args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", attr_arg_list.len().max(1))); // initial capacity (≥1)
            emitter.instruction("mov x1, #8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("bl __rt_array_new");                           // x0 = freshly allocated args array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", attr_arg_list.len().max(1))); // initial capacity (≥1)
            emitter.instruction("mov rdx, 8");                                  // element stride: one boxed mixed-cell pointer per slot
            emitter.instruction("call __rt_array_new");                         // rax = freshly allocated args array
        }
    }
    emit_array_value_type_stamp(emitter, result_reg, &PhpType::Mixed);

    // -- box and push each literal arg --
    for arg in attr_arg_list {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer across the boxing helper call
                emit_box_arg_aarch64(arg, emitter, data);                       // x0 = boxed mixed-cell pointer for this arg
                emitter.instruction("mov x1, x0");                              // x1 = mixed-cell pointer (push helper's value arg)
                emitter.instruction("ldr x0, [sp]");                            // x0 = args array pointer
                emitter.instruction("bl __rt_array_push_int");                  // x0 = (possibly realloc'd) args array pointer
                abi::emit_release_temporary_stack(emitter, 16);                 // drop the saved slot now that the helper returned the up-to-date array pointer
            }
            Arch::X86_64 => {
                abi::emit_push_reg(emitter, result_reg);                        // save the args array pointer
                emit_box_arg_x86_64(arg, emitter, data);                        // rax = boxed mixed-cell pointer
                emitter.instruction("mov rsi, rax");                            // rsi = mixed-cell pointer
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // rax = args array pointer
                emitter.instruction("call __rt_array_push_int");                // rax = updated args array pointer
                abi::emit_release_temporary_stack(emitter, 16);
            }
        }
    }

    // -- store the args array pointer + array kind tag in __args --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [sp]", obj_ptr_scratch));     // peek the obj pointer
            emitter.instruction(&format!("str {}, [{}, #24]", result_reg, obj_ptr_scratch)); // commit __args.lo (array pointer)
            emitter.instruction("mov x10, #4");                                 // runtime kind tag 4 = indexed array (x10 to avoid clobbering obj_ptr_scratch)
            emitter.instruction(&format!("str x10, [{}, #32]", obj_ptr_scratch)); // commit __args.hi (kind tag)
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", obj_ptr_scratch)); // peek the obj pointer
            emitter.instruction(&format!("mov QWORD PTR [{} + 24], {}", obj_ptr_scratch, result_reg)); // commit __args.lo (array pointer)
            emitter.instruction(&format!("mov QWORD PTR [{} + 32], 4", obj_ptr_scratch)); // commit __args.hi (kind tag = 4 = indexed array)
        }
    }
}

fn emit_box_arg_aarch64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = null payload
            emitter.instruction("mov x1, xzr");                                 // null carries no low word
            emitter.instruction("mov x2, xzr");                                 // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer payload
            emitter.instruction(&format!("mov x1, #{}", value));                // x1 = int value
            emitter.instruction("mov x2, xzr");                                 // ints do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov x1, #{}", *value as u64));        // x1 = 0 or 1
            emitter.instruction("mov x2, xzr");                                 // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let (sym, len) = data.add_string(value.as_bytes());
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "x1", &sym);                      // x1 = string data address
            emitter.instruction(&format!("mov x2, #{}", len));                  // x2 = string length
        }
    }
    emitter.instruction("bl __rt_mixed_from_value");                            // box the captured payload into an owned mixed cell
}

fn emit_box_arg_x86_64(arg: &AttrArgValue, emitter: &mut Emitter, data: &mut DataSection) {
    match arg {
        AttrArgValue::Null => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = null payload
            emitter.instruction("xor rdi, rdi");                                // null carries no low word
            emitter.instruction("xor rsi, rsi");                                // null carries no high word
        }
        AttrArgValue::Int(value) => {
            emitter.instruction("mov rax, 0");                                  // runtime tag 0 = integer payload
            emitter.instruction(&format!("mov rdi, {}", value));                // rdi = int value
            emitter.instruction("xor rsi, rsi");                                // ints do not use the high word
        }
        AttrArgValue::Bool(value) => {
            emitter.instruction("mov rax, 3");                                  // runtime tag 3 = boolean payload
            emitter.instruction(&format!("mov rdi, {}", *value as u64));        // rdi = 0 or 1
            emitter.instruction("xor rsi, rsi");                                // bools do not use the high word
        }
        AttrArgValue::Str(value) => {
            let (sym, len) = data.add_string(value.as_bytes());
            emitter.instruction("mov rax, 1");                                  // runtime tag 1 = string payload
            abi::emit_symbol_address(emitter, "rdi", &sym);                     // rdi = string data address
            emitter.instruction(&format!("mov rsi, {}", len));                  // rsi = string length
        }
    }
    emitter.instruction("call __rt_mixed_from_value");                          // box the captured payload into an owned mixed cell
}
