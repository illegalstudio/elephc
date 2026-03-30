pub mod checker;
pub mod traits;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{CType, ClassMethod, Program, Visibility};

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
    Pointer(Option<String>), // None = opaque ptr, Some("Class") = typed ptr<Class>
}

impl PhpType {
    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Bool => 8,
            PhpType::Int => 8,
            PhpType::Float => 8,
            PhpType::Str => 16,
            PhpType::Void => 8,              // null sentinel stored as 8 bytes
            PhpType::Array(_) => 8,          // pointer to heap
            PhpType::AssocArray { .. } => 8, // pointer to heap
            PhpType::Callable => 8,          // function address
            PhpType::Object(_) => 8,         // pointer to heap
            PhpType::Pointer(_) => 8,        // 64-bit address
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
            PhpType::Pointer(_) => 1,
        }
    }

    /// Returns true if this type uses a floating-point register (d0-d7).
    pub fn is_float_reg(&self) -> bool {
        matches!(self, PhpType::Float)
    }

    /// Returns true for heap values whose lifetime is tracked with runtime refcounts.
    pub fn is_refcounted(&self) -> bool {
        matches!(
            self,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        )
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
    pub method_decls: Vec<ClassMethod>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    pub method_visibilities: HashMap<String, Visibility>,
    pub static_method_visibilities: HashMap<String, Visibility>,
    /// Maps constructor param index → property name (for type propagation from new ClassName(args))
    pub constructor_param_to_prop: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields read by codegen via pattern matching
pub struct ExternFunctionSig {
    pub name: String,
    pub params: Vec<(String, PhpType)>,
    pub return_type: PhpType,
    pub library: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in extern class codegen
pub struct ExternClassInfo {
    pub name: String,
    pub fields: Vec<ExternFieldInfo>,
    pub total_size: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in extern class codegen
pub struct ExternFieldInfo {
    pub name: String,
    pub php_type: PhpType,
    pub offset: usize,
}

/// Convert a parser CType to a PhpType.
pub fn ctype_to_php_type(ct: &crate::parser::ast::CType) -> PhpType {
    match ct {
        CType::Int => PhpType::Int,
        CType::Float => PhpType::Float,
        CType::Str => PhpType::Str,
        CType::Bool => PhpType::Bool,
        CType::Void => PhpType::Void,
        CType::Ptr => PhpType::Pointer(None),
        CType::TypedPtr(name) => PhpType::Pointer(Some(name.clone())),
        CType::Callable => PhpType::Callable,
    }
}

/// Size in bytes used by a C-facing FFI type.
pub fn ctype_stack_size(ct: &CType) -> usize {
    match ct {
        CType::Int
        | CType::Float
        | CType::Bool
        | CType::Ptr
        | CType::TypedPtr(_)
        | CType::Callable => 8,
        CType::Str => 8, // char*
        CType::Void => 0,
    }
}

#[derive(Debug)]
pub struct CheckResult {
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub classes: HashMap<String, ClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
}

pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program)
}
