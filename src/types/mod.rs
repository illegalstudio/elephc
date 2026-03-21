pub mod checker;

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::Program;

#[derive(Debug, Clone, PartialEq)]
pub enum PhpType {
    Int,
    Str,
}

impl PhpType {
    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Int => 8,
            PhpType::Str => 16, // pointer + length
        }
    }
}

/// Maps variable names to their resolved types.
pub type TypeEnv = HashMap<String, PhpType>;

pub fn check(program: &Program) -> Result<(Program, TypeEnv), CompileError> {
    checker::check_types(program)
}
