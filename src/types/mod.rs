pub mod checker;

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::Program;

#[derive(Debug, Clone, PartialEq)]
pub enum PhpType {
    Int,
    Str,
    Void,
    Array(Box<PhpType>),
}

impl PhpType {
    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Int => 8,
            PhpType::Str => 16,
            PhpType::Void => 8, // null sentinel stored as 8 bytes
            PhpType::Array(_) => 8, // pointer to heap
        }
    }

    /// Number of registers used to pass this type as an argument.
    pub fn register_count(&self) -> usize {
        match self {
            PhpType::Int => 1,
            PhpType::Str => 2,
            PhpType::Void => 0,
            PhpType::Array(_) => 1,
        }
    }

}

/// Maps variable names to their resolved types.
pub type TypeEnv = HashMap<String, PhpType>;

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub return_type: PhpType,
}

#[derive(Debug)]
pub struct CheckResult {
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
}

pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program)
}
