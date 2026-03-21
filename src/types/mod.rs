pub mod checker;

use crate::errors::CompileError;
use crate::parser::ast::Program;

pub fn check(program: &Program) -> Result<Program, CompileError> {
    checker::check_types(program)
}
