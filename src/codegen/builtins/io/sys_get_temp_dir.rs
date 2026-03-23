use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sys_get_temp_dir()");
    let (lbl, len) = data.add_string(b"/tmp");
    // -- load "/tmp" string literal --
    emitter.instruction(&format!("adrp x1, {}@PAGE", lbl));                     // load "/tmp" string page address
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", lbl));               // resolve "/tmp" string offset
    emitter.instruction(&format!("mov x2, #{}", len));                          // string length = 4
    Some(PhpType::Str)
}
