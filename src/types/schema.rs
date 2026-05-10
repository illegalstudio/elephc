//! Purpose:
//! Defines declaration schema records shared across checker phases.
//! Models functions, classes, interfaces, enums, constants, and class members after parser/name resolution.
//!
//! Called from:
//! - `crate::types::checker::schema`
//! - `crate::types::checker::Checker`
//!
//! Key details:
//! - Schema data is the canonical contract for inheritance, calls, property access, and method validation.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{ClassMethod, Expr, Visibility};

use super::{FunctionSig, PhpType};

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
    pub defaults: Vec<Option<Expr>>,
    pub property_visibilities: HashMap<String, Visibility>,
    pub declared_properties: HashSet<String>,
    pub final_properties: HashSet<String>,
    pub readonly_properties: HashSet<String>,
    pub reference_properties: HashSet<String>,
    pub static_properties: Vec<(String, PhpType)>,
    pub static_defaults: Vec<Option<Expr>>,
    pub static_property_declaring_classes: HashMap<String, String>,
    pub static_property_visibilities: HashMap<String, Visibility>,
    pub declared_static_properties: HashSet<String>,
    pub final_static_properties: HashSet<String>,
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
    /// Maps constructor param index -> property name (for type propagation from new ClassName(args))
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
