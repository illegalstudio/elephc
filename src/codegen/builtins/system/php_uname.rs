use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("php_uname()");

    // -- materialize the PHP default mode when no explicit mode was passed --
    if args.is_empty() {
        let (label, len) = data.add_string(b"a");
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        abi::emit_symbol_address(emitter, ptr_reg, &label);                     // materialize php_uname() default mode "a" in the string pointer result register
        abi::emit_load_int_immediate(emitter, len_reg, len as i64);             // publish the one-byte default mode length in the paired string-length result register
    } else {
        emit_expr(&args[0], emitter, ctx, data);
    }

    // -- query the target runtime's uname data and select the requested PHP mode --
    abi::emit_call_label(emitter, "__rt_php_uname");                            // call the target-aware uname helper with the mode string in the native string result registers
    Some(PhpType::Str)
}
