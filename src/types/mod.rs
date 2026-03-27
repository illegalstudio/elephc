pub mod checker;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::Visibility;
use crate::parser::ast::Program;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Callable used in match arms, constructed when closures are added
pub enum PhpType {
    Int,
    Float,
    Str,
    Bool,
    Void,
    Array(Box<PhpType>),
    AssocArray {
        key: Box<PhpType>,
        value: Box<PhpType>,
    },
    Callable,
    Object(String),
}

impl PhpType {
    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Bool => 8,
            PhpType::Int => 8,
            PhpType::Float => 8,
            PhpType::Str => 16,
            PhpType::Void => 8, // null sentinel stored as 8 bytes
            PhpType::Array(_) => 8, // pointer to heap
            PhpType::AssocArray { .. } => 8, // pointer to heap
            PhpType::Callable => 8, // function address
            PhpType::Object(_) => 8, // pointer to heap
        }
    }

    /// Number of registers used to pass this type as an argument.
    pub fn register_count(&self) -> usize {
        match self {
            PhpType::Bool => 1,
            PhpType::Int => 1,
            PhpType::Float => 1,
            PhpType::Str => 2,
            PhpType::Void => 0,
            PhpType::Array(_) => 1,
            PhpType::AssocArray { .. } => 1,
            PhpType::Callable => 1,
            PhpType::Object(_) => 1,
        }
    }

    /// Returns true if this type uses a floating-point register (d0-d7).
    pub fn is_float_reg(&self) -> bool {
        matches!(self, PhpType::Float)
    }
}

/// Maps variable names to their resolved types.
pub type TypeEnv = HashMap<String, PhpType>;

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<crate::parser::ast::Expr>>,
    pub return_type: PhpType,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub class_id: u64,
    pub properties: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<crate::parser::ast::Expr>>,
    pub property_visibilities: HashMap<String, Visibility>,
    pub readonly_properties: HashSet<String>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    pub method_visibilities: HashMap<String, Visibility>,
    pub static_method_visibilities: HashMap<String, Visibility>,
    /// Maps constructor param index → property name (for type propagation from new ClassName(args))
    pub constructor_param_to_prop: Vec<Option<String>>,
}

#[derive(Debug)]
pub struct CheckResult {
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub classes: HashMap<String, ClassInfo>,
}

pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program)
}
