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
    emitter.comment("phpversion()");
    // -- return hardcoded version string from Cargo.toml --
    let (label, len) = data.add_string(b"0.7.1");
    emitter.instruction(&format!("adrp x1, {}@PAGE", label));                   // load page address of version string
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));             // resolve exact address of version string
    emitter.instruction(&format!("mov x2, #{}", len));                          // string length = 5
    Some(PhpType::Str)
}
