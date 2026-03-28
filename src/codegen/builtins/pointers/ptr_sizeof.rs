use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
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
                } else {
                    0 // unknown type
                }
            }
        };
        emitter.instruction(&format!("mov x0, #{}", size));                     // load computed size
    } else {
        // Non-literal argument — return 0
        emitter.instruction("mov x0, #0");                                      // unknown type size
    }
    Some(PhpType::Int)
}
