mod array_chunk;
mod array_column;
mod array_combine;
mod array_diff;
mod array_diff_key;
mod array_fill;
mod array_fill_keys;
mod array_filter;
mod array_flip;
mod array_intersect;
mod array_intersect_key;
mod array_key_exists;
mod array_keys;
mod array_map;
mod array_merge;
mod array_pad;
mod array_pop;
mod array_product;
mod array_push;
mod array_rand;
mod array_reduce;
mod array_reverse;
mod array_search;
mod array_shift;
mod array_slice;
mod array_splice;
mod array_sum;
mod array_unique;
mod array_unshift;
mod array_values;
mod array_walk;
mod arsort;
mod asort;
mod call_user_func;
mod call_user_func_array;
mod count;
mod function_exists;
mod in_array;
mod isset;
mod krsort;
mod ksort;
mod natcasesort;
mod natsort;
mod range;
mod rsort;
mod shuffle_fn;
mod sort;
mod uasort;
mod uksort;
mod usort;

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
        "array_key_exists" => array_key_exists::emit(name, args, emitter, ctx, data),
        "array_search" => array_search::emit(name, args, emitter, ctx, data),
        "array_reverse" => array_reverse::emit(name, args, emitter, ctx, data),
        "array_unique" => array_unique::emit(name, args, emitter, ctx, data),
        "array_sum" => array_sum::emit(name, args, emitter, ctx, data),
        "array_product" => array_product::emit(name, args, emitter, ctx, data),
        "array_shift" => array_shift::emit(name, args, emitter, ctx, data),
        "array_unshift" => array_unshift::emit(name, args, emitter, ctx, data),
        "array_merge" => array_merge::emit(name, args, emitter, ctx, data),
        "array_slice" => array_slice::emit(name, args, emitter, ctx, data),
        "array_splice" => array_splice::emit(name, args, emitter, ctx, data),
        "array_combine" => array_combine::emit(name, args, emitter, ctx, data),
        "array_flip" => array_flip::emit(name, args, emitter, ctx, data),
        "array_chunk" => array_chunk::emit(name, args, emitter, ctx, data),
        "array_column" => array_column::emit(name, args, emitter, ctx, data),
        "array_pad" => array_pad::emit(name, args, emitter, ctx, data),
        "array_fill" => array_fill::emit(name, args, emitter, ctx, data),
        "array_fill_keys" => array_fill_keys::emit(name, args, emitter, ctx, data),
        "array_diff" => array_diff::emit(name, args, emitter, ctx, data),
        "array_intersect" => array_intersect::emit(name, args, emitter, ctx, data),
        "array_diff_key" => array_diff_key::emit(name, args, emitter, ctx, data),
        "array_intersect_key" => array_intersect_key::emit(name, args, emitter, ctx, data),
        "array_rand" => array_rand::emit(name, args, emitter, ctx, data),
        "shuffle" => shuffle_fn::emit(name, args, emitter, ctx, data),
        "range" => range::emit(name, args, emitter, ctx, data),
        "asort" => asort::emit(name, args, emitter, ctx, data),
        "arsort" => arsort::emit(name, args, emitter, ctx, data),
        "ksort" => ksort::emit(name, args, emitter, ctx, data),
        "krsort" => krsort::emit(name, args, emitter, ctx, data),
        "natsort" => natsort::emit(name, args, emitter, ctx, data),
        "natcasesort" => natcasesort::emit(name, args, emitter, ctx, data),
        "array_map" => array_map::emit(name, args, emitter, ctx, data),
        "array_filter" => array_filter::emit(name, args, emitter, ctx, data),
        "array_reduce" => array_reduce::emit(name, args, emitter, ctx, data),
        "array_walk" => array_walk::emit(name, args, emitter, ctx, data),
        "usort" => usort::emit(name, args, emitter, ctx, data),
        "uksort" => uksort::emit(name, args, emitter, ctx, data),
        "uasort" => uasort::emit(name, args, emitter, ctx, data),
        "call_user_func" => call_user_func::emit(name, args, emitter, ctx, data),
        "call_user_func_array" => call_user_func_array::emit(name, args, emitter, ctx, data),
        "function_exists" => function_exists::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
