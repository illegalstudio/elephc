use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

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
                    8 + class_info.properties.len() * 16
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
