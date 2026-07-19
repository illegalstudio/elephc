//! Purpose:
//! Declares the internal PDOStatement native-state initializer used by `PDO::prepare()`.
//!
//! Called from:
//! - The generated PDO prelude after allocating the configured statement subclass.
//!
//! Key details:
//! - The lowering invokes PDOStatement's private initializer directly, so subclasses are
//!   initialized before their user constructor without exposing a reset API to PHP code.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_initialize_pdo_statement",
    area: Internal,
    params: [statement: Mixed, handle: Int, connection: Int, errorMode: Int, query: Str],
    returns: Void,
    lower: lower,
    summary: "Initializes a dynamically allocated PDOStatement subclass.",
    internal: true
}

/// Lowers the private PDOStatement initializer call through the object backend.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_initialize_pdo_statement(ctx, inst)
}
