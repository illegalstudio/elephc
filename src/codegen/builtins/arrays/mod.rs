mod array_keys;
mod array_pop;
mod array_push;
mod array_values;
mod count;
mod in_array;
mod isset;
mod rsort;
mod sort;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "count" => count::emit(name, args, emitter, ctx, data),
        "array_push" => array_push::emit(name, args, emitter, ctx, data),
        "array_pop" => array_pop::emit(name, args, emitter, ctx, data),
        "in_array" => in_array::emit(name, args, emitter, ctx, data),
        "array_keys" => array_keys::emit(name, args, emitter, ctx, data),
        "array_values" => array_values::emit(name, args, emitter, ctx, data),
        "sort" => sort::emit(name, args, emitter, ctx, data),
        "rsort" => rsort::emit(name, args, emitter, ctx, data),
        "isset" => isset::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
