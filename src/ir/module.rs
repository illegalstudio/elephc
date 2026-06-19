//! Purpose:
//! Defines the top-level EIR module, data pool, extern declarations, and
//! metadata tables needed by later lowering/codegen phases.
//!
//! Called from:
//! - Future AST-to-EIR lowering and the EIR-to-ASM backend.
//!
//! Key details:
//! - Runtime helper bodies remain outside EIR; modules reference runtime
//!   features and metadata needed to select/link helpers.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::ir::function::{Function, FunctionId};
use crate::ir::types::IrType;
use crate::parser::ast::Visibility;
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
};

/// Data-pool identifier shared by string, float, and name tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataId(u32);

impl DataId {
    /// Creates a data identifier from its raw zero-based table index.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw zero-based table index represented by this identifier.
    pub fn as_raw(self) -> u32 {
        self.0
    }
}

/// Method metadata retained for standalone trait reflection.
#[derive(Debug, Clone)]
pub struct TraitMethodInfo {
    pub signature: FunctionSig,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub is_abstract: bool,
}

/// Complete EIR module for one compile target.
#[derive(Debug, Clone)]
pub struct Module {
    pub target: Target,
    pub source_path: Option<String>,
    pub functions: Vec<Function>,
    pub class_methods: Vec<Function>,
    pub closures: Vec<Function>,
    pub fiber_wrappers: Vec<Function>,
    pub callback_wrappers: Vec<Function>,
    pub extern_callback_trampolines: Vec<Function>,
    pub runtime_callable_invokers: Vec<Function>,
    pub data: DataPool,
    pub extern_decls: Vec<ExternDecl>,
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    pub class_table: ClassTable,
    pub enum_table: EnumTable,
    pub interface_table: InterfaceTable,
    pub trait_table: TraitTable,
    pub declared_class_names: Vec<String>,
    pub declared_interface_names: Vec<String>,
    pub declared_trait_names: Vec<String>,
    pub declared_trait_uses: HashMap<String, Vec<String>>,
    pub declared_trait_method_names: HashMap<String, Vec<String>>,
    pub declared_trait_methods: HashMap<String, HashMap<String, TraitMethodInfo>>,
    pub declared_trait_property_names: HashMap<String, Vec<String>>,
    pub declared_trait_constant_names: HashMap<String, Vec<String>>,
    pub declared_trait_constants: HashMap<String, HashMap<String, crate::parser::ast::Expr>>,
    pub declared_trait_constant_visibilities: HashMap<String, HashMap<String, Visibility>>,
    pub declared_trait_final_constants: HashMap<String, HashSet<String>>,
    pub class_infos: HashMap<String, ClassInfo>,
    pub interface_infos: HashMap<String, InterfaceInfo>,
    pub enum_infos: HashMap<String, EnumInfo>,
    pub extern_class_infos: HashMap<String, ExternClassInfo>,
    pub packed_class_infos: HashMap<String, PackedClassInfo>,
    pub packed_layouts: PackedLayoutTable,
    pub required_runtime_features: RuntimeFeatures,
}

impl Module {
    /// Creates an empty module for the given target.
    pub fn new(target: Target) -> Self {
        Self {
            target,
            source_path: None,
            functions: Vec::new(),
            class_methods: Vec::new(),
            closures: Vec::new(),
            fiber_wrappers: Vec::new(),
            callback_wrappers: Vec::new(),
            extern_callback_trampolines: Vec::new(),
            runtime_callable_invokers: Vec::new(),
            data: DataPool::default(),
            extern_decls: Vec::new(),
            callable_param_sigs: HashMap::new(),
            class_table: ClassTable::default(),
            enum_table: EnumTable::default(),
            interface_table: InterfaceTable::default(),
            trait_table: TraitTable::default(),
            declared_class_names: Vec::new(),
            declared_interface_names: Vec::new(),
            declared_trait_names: Vec::new(),
            declared_trait_uses: HashMap::new(),
            declared_trait_method_names: HashMap::new(),
            declared_trait_methods: HashMap::new(),
            declared_trait_property_names: HashMap::new(),
            declared_trait_constant_names: HashMap::new(),
            declared_trait_constants: HashMap::new(),
            declared_trait_constant_visibilities: HashMap::new(),
            declared_trait_final_constants: HashMap::new(),
            class_infos: HashMap::new(),
            interface_infos: HashMap::new(),
            enum_infos: HashMap::new(),
            extern_class_infos: HashMap::new(),
            packed_class_infos: HashMap::new(),
            packed_layouts: PackedLayoutTable::default(),
            required_runtime_features: RuntimeFeatures::none(),
        }
    }

    /// Adds a user function and returns its module-local identifier.
    pub fn add_function(&mut self, mut function: Function) -> FunctionId {
        let id = FunctionId::from_raw(self.functions.len() as u32);
        function.set_id(id);
        self.functions.push(function);
        id
    }

    /// Adds a closure function and returns its closure-table identifier.
    pub fn add_closure(&mut self, mut function: Function) -> FunctionId {
        let id = FunctionId::from_raw(self.closures.len() as u32);
        function.set_id(id);
        self.closures.push(function);
        id
    }
}

/// Deterministic literal/name pool used by IR immediates and printer output.
#[derive(Debug, Clone, Default)]
pub struct DataPool {
    pub strings: Vec<String>,
    pub float_literals: Vec<f64>,
    pub global_names: Vec<String>,
    pub function_names: Vec<String>,
    pub class_names: Vec<String>,
    pub method_names: Vec<String>,
    pub property_names: Vec<String>,
}

impl DataPool {
    /// Interns a string literal and returns its stable data identifier.
    pub fn intern_string(&mut self, value: &str) -> DataId {
        intern_string_vec(&mut self.strings, value)
    }

    /// Interns a floating-point literal by exact bit pattern.
    pub fn intern_float(&mut self, value: f64) -> DataId {
        if let Some(idx) = self
            .float_literals
            .iter()
            .position(|existing| existing.to_bits() == value.to_bits())
        {
            return DataId::from_raw(idx as u32);
        }
        let id = DataId::from_raw(self.float_literals.len() as u32);
        self.float_literals.push(value);
        id
    }

    /// Interns a global symbol name and returns its stable data identifier.
    pub fn intern_global_name(&mut self, value: &str) -> DataId {
        intern_string_vec(&mut self.global_names, value)
    }

    /// Interns a function name and returns its stable data identifier.
    pub fn intern_function_name(&mut self, value: &str) -> DataId {
        intern_string_vec(&mut self.function_names, value)
    }

    /// Interns a class name and returns its stable data identifier.
    pub fn intern_class_name(&mut self, value: &str) -> DataId {
        intern_string_vec(&mut self.class_names, value)
    }
}

/// Interns `value` into a string vector and returns its zero-based index.
fn intern_string_vec(values: &mut Vec<String>, value: &str) -> DataId {
    if let Some(idx) = values.iter().position(|existing| existing == value) {
        return DataId::from_raw(idx as u32);
    }
    let id = DataId::from_raw(values.len() as u32);
    values.push(value.to_string());
    id
}

/// C-facing extern function declaration referenced by EIR.
#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<ExternParamDecl>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    pub link_libs: Vec<String>,
}

/// One extern function parameter.
#[derive(Debug, Clone)]
pub struct ExternParamDecl {
    pub name: String,
    pub ir_type: IrType,
    pub php_type: PhpType,
}

/// Minimal class metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct ClassTable {
    pub names: Vec<String>,
}

/// Minimal enum metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct EnumTable {
    pub names: Vec<String>,
}

/// Minimal interface metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct InterfaceTable {
    pub names: Vec<String>,
}

/// Minimal trait metadata table placeholder for Phase 04 introspection.
#[derive(Debug, Clone, Default)]
pub struct TraitTable {
    pub names: Vec<String>,
}

/// Minimal packed-layout metadata table placeholder for Phase 02.
#[derive(Debug, Clone, Default)]
pub struct PackedLayoutTable {
    pub names: Vec<String>,
}
