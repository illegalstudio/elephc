//! Codegen for `get_class` and `get_parent_class`.
//!
//! In the AOT model the static type of the argument is enough to derive
//! the class name at compile time. The argument is evaluated for side
//! effects, then the class name (or its parent) is emitted as a string
//! literal. When the static type isn't a known `Object(...)` (e.g. a
//! plain `mixed`), we emit an empty string — runtime polymorphism via a
//! class_id → name table is left for a follow-up; the static-type path
//! covers the dominant cases (`get_class($this)`, `get_class(new Foo())`,
//! `$x = new Bar(); get_class($x);`).

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}() — AOT static-type lookup", name));

    let resolved_class = if args.is_empty() {
        ctx.current_class.clone().unwrap_or_default()
    } else {
        let arg_ty = emit_expr(&args[0], emitter, ctx, data);
        match arg_ty {
            PhpType::Object(class_name) => class_name,
            _ => String::new(),
        }
    };

    let final_name = match name {
        "get_class" => resolved_class,
        "get_parent_class" => parent_of(&resolved_class, ctx),
        _ => String::new(),
    };

    let bytes = final_name.as_bytes();
    let (label, len) = data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                                 // expose the resolved class name in the string-pointer result register
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);                         // publish the resolved class name length in the paired length result register
    Some(PhpType::Str)
}

fn parent_of(class_name: &str, ctx: &Context) -> String {
    if class_name.is_empty() {
        return String::new();
    }
    ctx.classes
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.parent.clone())
        .unwrap_or_default()
}
