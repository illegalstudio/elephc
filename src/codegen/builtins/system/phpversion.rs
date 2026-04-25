use crate::codegen::abi;
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
    // -- return the Cargo package version string --
    let version = env!("CARGO_PKG_VERSION").as_bytes();
    let (label, len) = data.add_string(version);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                         // materialize the Cargo package version string in the active string-pointer result register
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);                 // publish the Cargo package version string length in the paired string-length result register
    Some(PhpType::Str)
}
