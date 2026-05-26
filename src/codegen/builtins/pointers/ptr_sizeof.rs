//! Purpose:
//! Emits compiler-extension `ptr_sizeof` queries for low-level types.
//! Returns static byte sizes used by pointer arithmetic and raw memory access code.
//!
//! Called from:
//! - `crate::codegen::builtins::pointers::emit()`.
//!
//! Key details:
//! - Reported sizes must match the layouts used by buffer, packed class, and ABI lowering.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits the `ptr_sizeof` builtin call: returns the static byte size of a low-level type.
///
/// The type name is resolved from the first argument's string literal (`"int"`, `"float"`,
/// `"bool"`, `"string"`, `"ptr"`, or a class name). For class names, the size is computed
/// as `[8] + [properties.len() * 16] + [optional 8 bytes for dynamic properties]`.
///
/// On success, the computed size (as `usize`) is materialized in the integer result register
/// (`x0` on AArch64, `rax` on x86_64). On failure (non-literal argument or unknown type),
/// zero is placed in the result register.
///
/// Returns `PhpType::Int` to indicate the result is an integer value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_sizeof()");
    // -- determine size from string literal argument --
    if let ExprKind::StringLiteral(type_name) = &args[0].kind {
        let size: usize = match type_name.as_str() {
            "int" | "integer" => 8,
            "float" | "double" => 8,
            "bool" | "boolean" => 8,
            "string" => 16,
            "ptr" | "pointer" => 8,
            class_name => {
                // Look up class and compute total property size
                if let Some(class_info) = ctx.classes.get(class_name) {
                    // Object layout: [class_id:8] + [prop:16] * num_properties
                    // + optional [dyn_props_ptr:8] for #[\AllowDynamicProperties]
                    let dyn_slot = if class_info.allow_dynamic_properties { 8 } else { 0 };
                    8 + class_info.properties.len() * 16 + dyn_slot
                } else if let Some(class_info) = ctx.extern_classes.get(class_name) {
                    class_info.total_size
                } else {
                    0 // unknown type
                }
            }
        };
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov x0, #{}", size));             // materialize the computed pointee size in the AArch64 integer result register
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, {}", size));             // materialize the computed pointee size in the x86_64 integer result register
            }
        }
    } else {
        // Non-literal argument — return 0
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // return zero when ptr_sizeof() cannot resolve a literal type name on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // return zero when ptr_sizeof() cannot resolve a literal type name on x86_64
            }
        }
    }
    Some(PhpType::Int)
}
