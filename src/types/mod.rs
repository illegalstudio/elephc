pub mod checker;
pub mod traits;
mod warnings;

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::codegen::platform::{Platform, Target};
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::{CType, ClassMethod, Program, Visibility};

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Callable used in match arms, constructed when closures are added
pub enum PhpType {
    Int,
    Float,
    Str,
    Bool,
    Void,
    Mixed,
    Array(Box<PhpType>),
    AssocArray {
        key: Box<PhpType>,
        value: Box<PhpType>,
    },
    Buffer(Box<PhpType>),
    Callable,
    Object(String),
    Packed(String),
    Pointer(Option<String>), // None = opaque ptr, Some("Class") = typed ptr<Class>
    Union(Vec<PhpType>),
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
            PhpType::Mixed => 8,             // pointer to heap-tagged mixed cell
            PhpType::Array(_) => 8,          // pointer to heap
            PhpType::AssocArray { .. } => 8, // pointer to heap
            PhpType::Buffer(_) => 8,         // pointer to buffer header
            PhpType::Callable => 8,          // function address
            PhpType::Object(_) => 8,         // pointer to heap
            PhpType::Packed(_) => 8,         // metadata-only nominal type, usually accessed by pointer
            PhpType::Pointer(_) => 8,        // 64-bit address
            PhpType::Union(_) => 8,          // boxed runtime-tagged payload (same storage as Mixed)
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
            PhpType::Mixed => 1,
            PhpType::Array(_) => 1,
            PhpType::AssocArray { .. } => 1,
            PhpType::Buffer(_) => 1,
            PhpType::Callable => 1,
            PhpType::Object(_) => 1,
            PhpType::Packed(_) => 1,
            PhpType::Pointer(_) => 1,
            PhpType::Union(_) => 1,
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
            PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Union(_)
        )
    }

    /// Lower high-level checker-only types to the runtime representation used by codegen.
    pub fn codegen_repr(&self) -> PhpType {
        match self {
            PhpType::Union(_) => PhpType::Mixed,
            _ => self.clone(),
        }
    }
}

impl fmt::Display for PhpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhpType::Int => write!(f, "int"),
            PhpType::Float => write!(f, "float"),
            PhpType::Str => write!(f, "string"),
            PhpType::Bool => write!(f, "bool"),
            PhpType::Void => write!(f, "null"),
            PhpType::Mixed => write!(f, "mixed"),
            PhpType::Array(inner) => write!(f, "array<{}>", inner),
            PhpType::AssocArray { key, value } => write!(f, "array<{}, {}>", key, value),
            PhpType::Buffer(inner) => write!(f, "buffer<{}>", inner),
            PhpType::Callable => write!(f, "callable"),
            PhpType::Object(name) => write!(f, "{}", name),
            PhpType::Packed(name) => write!(f, "packed {}", name),
            PhpType::Pointer(Some(name)) => write!(f, "ptr<{}>", name),
            PhpType::Pointer(None) => write!(f, "ptr"),
            PhpType::Union(members) => {
                for (i, member) in members.iter().enumerate() {
                    if i > 0 {
                        write!(f, "|")?;
                    }
                    write!(f, "{}", member)?;
                }
                Ok(())
            }
        }
    }
}

/// Maps variable names to their resolved types.
pub type TypeEnv = HashMap<String, PhpType>;

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub defaults: Vec<Option<crate::parser::ast::Expr>>,
    pub return_type: PhpType,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
    pub variadic: Option<String>,
}

pub(crate) fn first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "strlen" => Some(FunctionSig {
            params: vec![("arg0".to_string(), PhpType::Str)],
            defaults: vec![None],
            return_type: PhpType::Int,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        "count" => Some(FunctionSig {
            params: vec![(
                "arg0".to_string(),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            )],
            defaults: vec![None],
            return_type: PhpType::Int,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        "buffer_len" => Some(FunctionSig {
            params: vec![("arg0".to_string(), PhpType::Buffer(Box::new(PhpType::Int)))],
            defaults: vec![None],
            return_type: PhpType::Int,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub interface_id: u64,
    pub parents: Vec<String>,
    pub methods: HashMap<String, FunctionSig>,
    pub method_declaring_interfaces: HashMap<String, String>,
    pub method_order: Vec<String>,
    pub method_slots: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassInfo {
    pub class_id: u64,
    pub parent: Option<String>,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly_class: bool,
    pub properties: Vec<(String, PhpType)>,
    pub property_offsets: HashMap<String, usize>,
    pub property_declaring_classes: HashMap<String, String>,
    pub defaults: Vec<Option<crate::parser::ast::Expr>>,
    pub property_visibilities: HashMap<String, Visibility>,
    pub declared_properties: HashSet<String>,
    pub final_properties: HashSet<String>,
    pub readonly_properties: HashSet<String>,
    pub reference_properties: HashSet<String>,
    pub method_decls: Vec<ClassMethod>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    pub method_visibilities: HashMap<String, Visibility>,
    pub final_methods: HashSet<String>,
    pub method_declaring_classes: HashMap<String, String>,
    pub method_impl_classes: HashMap<String, String>,
    pub vtable_methods: Vec<String>,
    pub vtable_slots: HashMap<String, usize>,
    pub static_method_visibilities: HashMap<String, Visibility>,
    pub final_static_methods: HashSet<String>,
    pub static_method_declaring_classes: HashMap<String, String>,
    pub static_method_impl_classes: HashMap<String, String>,
    pub static_vtable_methods: Vec<String>,
    pub static_vtable_slots: HashMap<String, usize>,
    pub interfaces: Vec<String>,
    /// Maps constructor param index → property name (for type propagation from new ClassName(args))
    pub constructor_param_to_prop: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnumCaseValue {
    Int(i64),
    Str(String),
}

#[derive(Debug, Clone)]
pub struct EnumCaseInfo {
    pub name: String,
    pub value: Option<EnumCaseValue>,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub backing_type: Option<PhpType>,
    pub cases: Vec<EnumCaseInfo>,
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

#[derive(Debug, Clone)]
pub struct PackedClassInfo {
    pub fields: Vec<PackedFieldInfo>,
    pub total_size: usize,
}

#[derive(Debug, Clone)]
pub struct PackedFieldInfo {
    pub name: String,
    pub php_type: PhpType,
    pub offset: usize,
}

/// Convert a parser CType to a PhpType.
pub fn ctype_to_php_type(ct: &CType) -> PhpType {
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
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub classes: HashMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
    pub warnings: Vec<CompileWarning>,
}

pub fn packed_type_size(
    ty: &PhpType,
    packed_classes: &HashMap<String, PackedClassInfo>,
) -> Option<usize> {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_) => Some(8),
        PhpType::Packed(name) => packed_classes.get(name).map(|info| info.total_size),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    checker::check_types(program, Platform::detect_host())
}

pub fn check_with_target(program: &Program, target: Target) -> Result<CheckResult, CompileError> {
    checker::check_types(program, target.platform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Target};

    fn parse_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    #[test]
    fn test_linux_crypto_builtin_linking_tracks_target_not_host() {
        let program = parse_program("<?php echo md5(\"abc\");");

        let linux = check_with_target(&program, Target::new(Platform::Linux, Arch::AArch64))
            .expect("linux type check failed");
        assert_eq!(linux.required_libraries, vec!["crypto"]);

        let mac = check_with_target(&program, Target::new(Platform::MacOS, Arch::AArch64))
            .expect("mac type check failed");
        assert!(mac.required_libraries.is_empty());
    }
}
