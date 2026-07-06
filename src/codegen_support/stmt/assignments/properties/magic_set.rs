//! Purpose:
//! Lowers __set dispatch for inaccessible or dynamic property writes.
//! Shares receiver and property metadata with object expression lowering.
//!
//! Called from:
//! - `crate::codegen_support::stmt::assignments::properties`
//!
//! Key details:
//! - Property writes must respect declared types, visibility checks, and runtime object layout.

use crate::codegen_support::context::Context;
use crate::codegen_support::NULL_SENTINEL;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Returns `Some(class_name)` when the receiver's inferred type is a
/// declared object class that does not have the property but does have a
/// `__set` method, meaning property writes should route through the magic
/// setter. Returns `None` otherwise.
pub(super) fn resolve_magic_set_target(object: &Expr, property: &str, ctx: &Context) -> Option<String> {
    let obj_ty = crate::codegen_support::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    class_info.methods.contains_key("__set").then_some(class_name)
}

/// Emits a call to the `__set` magic method of `class_name` for the given
/// property name and RHS value. Boxes the value as a `Mixed` cell, pushes
/// the property name string, the boxed value, and the `$this` pointer, then
/// dispatches to `__set` via `emit_method_call_with_pushed_args`. The
/// `val_ty` is used to determine boxing strategy (void emits a boxed null,
/// floats and strings require reload from the saved stack slot, other types
/// are reloaded from the temporary stack).
pub(super) fn emit_magic_set_call(
    class_name: &str,
    property: &str,
    value: &Expr,
    val_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment(&format!("magic __set('{}')", property));
    let object_reg = abi::symbol_scratch_reg(emitter);
    let boxed_reg = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                        // push $this pointer while boxing the value argument

    if *val_ty == PhpType::Void {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("mov x1, xzr");                             // null mixed payloads have no low word
                emitter.instruction("mov x2, xzr");                             // null mixed payloads have no high word
                emitter.instruction("bl __rt_mixed_from_value");                // box null into an owned Mixed cell for __set
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rdi, {}", NULL_SENTINEL));    // use the runtime null sentinel as the boxed null payload low word
                emitter.instruction("xor rsi, rsi");                            // null mixed payloads have no high word
                emitter.instruction("mov rax, 8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("call __rt_mixed_from_value");              // box null into an owned Mixed cell for __set
            }
        }
        abi::emit_pop_reg(emitter, object_reg);                                      // reload $this after the boxing helper may clobber caller-saved registers
    } else {
        match val_ty {
            PhpType::Float => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr d0, [sp, #16]");               // reload the saved float value for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movsd xmm0, QWORD PTR [rsp + 16]"); // reload the saved float value for Mixed boxing
                    }
                }
                crate::codegen_support::emit_box_current_expr_value_as_mixed_for_container(
                    emitter, value, val_ty,
                );
            }
            PhpType::Str => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldp x1, x2, [sp, #16]");           // reload the saved string payload for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");   // reload the saved string pointer for Mixed boxing
                        emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");   // reload the saved string length for Mixed boxing
                    }
                }
                crate::codegen_support::emit_box_current_expr_value_as_mixed_for_container(
                    emitter, value, val_ty,
                );
            }
            _ => {
                abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16); // reload the saved scalar/heap value for Mixed boxing
                if !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
                    crate::codegen_support::emit_box_current_expr_value_as_mixed_for_container(
                        emitter, value, val_ty,
                    );
                }
            }
        }
        abi::emit_pop_reg(emitter, object_reg);                                      // reload $this after the boxing helper may clobber caller-saved registers
        abi::emit_release_temporary_stack(emitter, 16);                              // drop the saved original value after boxing it into Mixed storage
    }

    emitter.instruction(&format!("mov {}, {}", boxed_reg, abi::int_result_reg(emitter))); // keep the boxed Mixed value across property-name setup
    crate::codegen_support::expr::push_magic_property_name_arg(property, emitter, data);
    abi::emit_push_reg(emitter, boxed_reg);                                          // push the boxed Mixed $value argument
    abi::emit_push_reg(emitter, object_reg);                                         // push $this pointer for __set dispatch
    crate::codegen_support::expr::emit_method_call_with_pushed_args(
        class_name,
        "__set",
        &[PhpType::Str, PhpType::Mixed],
        emitter,
        ctx,
    );
}
