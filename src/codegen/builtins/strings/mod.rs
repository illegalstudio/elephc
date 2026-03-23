mod addslashes;
mod base64_decode;
mod base64_encode;
mod bin2hex;
mod chr;
mod ctype_alnum;
mod ctype_alpha;
mod ctype_digit;
mod ctype_space;
mod explode;
mod hex2bin;
mod html_entity_decode;
mod htmlentities;
mod htmlspecialchars;
mod implode;
mod intval;
mod lcfirst;
mod ltrim;
mod md5;
mod nl2br;
mod number_format;
mod ord;
mod printf;
mod sprintf;
mod rawurldecode;
mod rawurlencode;
mod rtrim;
mod sha1;
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
mod urldecode;
mod urlencode;
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
        "htmlspecialchars" => htmlspecialchars::emit(name, args, emitter, ctx, data),
        "htmlentities" => htmlentities::emit(name, args, emitter, ctx, data),
        "html_entity_decode" => html_entity_decode::emit(name, args, emitter, ctx, data),
        "urlencode" => urlencode::emit(name, args, emitter, ctx, data),
        "urldecode" => urldecode::emit(name, args, emitter, ctx, data),
        "rawurlencode" => rawurlencode::emit(name, args, emitter, ctx, data),
        "rawurldecode" => rawurldecode::emit(name, args, emitter, ctx, data),
        "base64_encode" => base64_encode::emit(name, args, emitter, ctx, data),
        "base64_decode" => base64_decode::emit(name, args, emitter, ctx, data),
        "ctype_alpha" => ctype_alpha::emit(name, args, emitter, ctx, data),
        "ctype_digit" => ctype_digit::emit(name, args, emitter, ctx, data),
        "ctype_alnum" => ctype_alnum::emit(name, args, emitter, ctx, data),
        "ctype_space" => ctype_space::emit(name, args, emitter, ctx, data),
        "sprintf" => sprintf::emit(name, args, emitter, ctx, data),
        "md5" => md5::emit(name, args, emitter, ctx, data),
        "sha1" => sha1::emit(name, args, emitter, ctx, data),
        "printf" => printf::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
