mod addslashes;
mod bin2hex;
mod chr;
mod explode;
mod hex2bin;
mod implode;
mod intval;
mod lcfirst;
mod ltrim;
mod nl2br;
mod number_format;
mod ord;
mod rtrim;
mod str_contains;
mod str_ends_with;
mod str_ireplace;
mod str_pad;
mod str_repeat;
mod str_replace;
mod str_split;
mod str_starts_with;
mod strcasecmp;
mod strcmp;
mod stripslashes;
mod strlen;
mod strpos;
mod strrev;
mod strrpos;
mod strstr;
mod strtolower;
mod strtoupper;
mod substr;
mod substr_replace;
mod trim;
mod ucfirst;
mod ucwords;
mod wordwrap;

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
        "strlen" => strlen::emit(name, args, emitter, ctx, data),
        "intval" => intval::emit(name, args, emitter, ctx, data),
        "number_format" => number_format::emit(name, args, emitter, ctx, data),
        "substr" => substr::emit(name, args, emitter, ctx, data),
        "strpos" => strpos::emit(name, args, emitter, ctx, data),
        "strrpos" => strrpos::emit(name, args, emitter, ctx, data),
        "strstr" => strstr::emit(name, args, emitter, ctx, data),
        "strtolower" => strtolower::emit(name, args, emitter, ctx, data),
        "strtoupper" => strtoupper::emit(name, args, emitter, ctx, data),
        "ucfirst" => ucfirst::emit(name, args, emitter, ctx, data),
        "lcfirst" => lcfirst::emit(name, args, emitter, ctx, data),
        "trim" => trim::emit(name, args, emitter, ctx, data),
        "ltrim" => ltrim::emit(name, args, emitter, ctx, data),
        "rtrim" => rtrim::emit(name, args, emitter, ctx, data),
        "str_repeat" => str_repeat::emit(name, args, emitter, ctx, data),
        "strrev" => strrev::emit(name, args, emitter, ctx, data),
        "ord" => ord::emit(name, args, emitter, ctx, data),
        "chr" => chr::emit(name, args, emitter, ctx, data),
        "strcmp" => strcmp::emit(name, args, emitter, ctx, data),
        "strcasecmp" => strcasecmp::emit(name, args, emitter, ctx, data),
        "str_contains" => str_contains::emit(name, args, emitter, ctx, data),
        "str_starts_with" => str_starts_with::emit(name, args, emitter, ctx, data),
        "str_ends_with" => str_ends_with::emit(name, args, emitter, ctx, data),
        "str_replace" => str_replace::emit(name, args, emitter, ctx, data),
        "explode" => explode::emit(name, args, emitter, ctx, data),
        "implode" => implode::emit(name, args, emitter, ctx, data),
        "ucwords" => ucwords::emit(name, args, emitter, ctx, data),
        "str_ireplace" => str_ireplace::emit(name, args, emitter, ctx, data),
        "substr_replace" => substr_replace::emit(name, args, emitter, ctx, data),
        "str_pad" => str_pad::emit(name, args, emitter, ctx, data),
        "str_split" => str_split::emit(name, args, emitter, ctx, data),
        "addslashes" => addslashes::emit(name, args, emitter, ctx, data),
        "stripslashes" => stripslashes::emit(name, args, emitter, ctx, data),
        "nl2br" => nl2br::emit(name, args, emitter, ctx, data),
        "wordwrap" => wordwrap::emit(name, args, emitter, ctx, data),
        "bin2hex" => bin2hex::emit(name, args, emitter, ctx, data),
        "hex2bin" => hex2bin::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
