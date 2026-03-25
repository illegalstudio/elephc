mod date;
mod define;
mod exec_fn;
mod exit;
mod getenv;
mod json_decode;
mod json_encode;
mod json_last_error;
mod microtime;
mod mktime;
mod passthru;
mod php_uname;
mod phpversion;
mod preg_match;
mod preg_match_all;
mod preg_replace;
mod preg_split;
mod putenv;
mod shell_exec;
mod sleep;
mod strtotime;
mod system_fn;
mod time;
mod usleep;

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
        "exit" | "die" => exit::emit(name, args, emitter, ctx, data),
        "define" => define::emit(name, args, emitter, ctx, data),
        "time" => time::emit(name, args, emitter, ctx, data),
        "microtime" => microtime::emit(name, args, emitter, ctx, data),
        "sleep" => sleep::emit(name, args, emitter, ctx, data),
        "usleep" => usleep::emit(name, args, emitter, ctx, data),
        "getenv" => getenv::emit(name, args, emitter, ctx, data),
        "putenv" => putenv::emit(name, args, emitter, ctx, data),
        "php_uname" => php_uname::emit(name, args, emitter, ctx, data),
        "phpversion" => phpversion::emit(name, args, emitter, ctx, data),
        "exec" => exec_fn::emit(name, args, emitter, ctx, data),
        "shell_exec" => shell_exec::emit(name, args, emitter, ctx, data),
        "system" => system_fn::emit(name, args, emitter, ctx, data),
        "passthru" => passthru::emit(name, args, emitter, ctx, data),
        "date" => date::emit(name, args, emitter, ctx, data),
        "mktime" => mktime::emit(name, args, emitter, ctx, data),
        "strtotime" => strtotime::emit(name, args, emitter, ctx, data),
        "json_encode" => json_encode::emit(name, args, emitter, ctx, data),
        "json_decode" => json_decode::emit(name, args, emitter, ctx, data),
        "json_last_error" => json_last_error::emit(name, args, emitter, ctx, data),
        "preg_match" => preg_match::emit(name, args, emitter, ctx, data),
        "preg_match_all" => preg_match_all::emit(name, args, emitter, ctx, data),
        "preg_replace" => preg_replace::emit(name, args, emitter, ctx, data),
        "preg_split" => preg_split::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
