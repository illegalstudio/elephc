mod define;
mod exec_fn;
mod exit;
mod getenv;
mod microtime;
mod passthru;
mod php_uname;
mod phpversion;
mod putenv;
mod shell_exec;
mod sleep;
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
        _ => None,
    }
}
