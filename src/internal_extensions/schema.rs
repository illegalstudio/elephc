//! Purpose:
//! Defines PHP-visible internal-extension metadata independent of checker AST and native ABI details.
//! Represents exact Reflection contracts for extensions, classes, methods, properties, constants, and functions.
//!
//! Called from:
//! - `crate::internal_extensions::loader`.
//! - Type-checker, Reflection/catalog, effects, and EIR integration.
//!
//! Key details:
//! - Values/defaults remain JSON so the locked snapshot retains PHP's exact scalar and enum representation.
//! - Native operation IDs are added by implementation manifests, never inferred from public names at runtime.

use serde_json::Value;

/// One PHP attribute attached to an internal declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSpec {
    pub name: String,
    pub arguments: Value,
}

/// One callable parameter in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterSpec {
    pub name: String,
    pub position: usize,
    pub php_type: Option<String>,
    pub optional: bool,
    pub variadic: bool,
    pub by_reference: bool,
    pub can_be_passed_by_value: bool,
    pub allows_null: bool,
    pub default: Option<Value>,
    pub attributes: Vec<AttributeSpec>,
}

/// Shared PHP signature metadata for an internal function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureSpec {
    pub name: String,
    pub internal: bool,
    pub deprecated: bool,
    pub returns_reference: bool,
    pub required_parameters: usize,
    pub parameters: Vec<ParameterSpec>,
    pub return_type: Option<String>,
    pub tentative_return_type: Option<String>,
    pub attributes: Vec<AttributeSpec>,
}

/// One directly declared internal method.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSpec {
    pub signature: SignatureSpec,
    pub declaring_class: String,
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub static_method: bool,
    pub abstract_method: bool,
    pub final_method: bool,
    pub constructor: bool,
    pub destructor: bool,
}

/// One directly declared internal virtual or stored property.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    pub name: String,
    pub declaring_class: String,
    pub php_type: Option<String>,
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub static_property: bool,
    /// Whether php-src exposes a native write handler for this property.
    pub writable: bool,
    pub readonly: bool,
    pub virtual_property: bool,
    pub deprecated: bool,
    pub has_default: bool,
    pub default: Option<Value>,
    pub hooks: Vec<(String, MethodSpec)>,
    pub attributes: Vec<AttributeSpec>,
}

/// One directly declared internal class constant or enum case.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassConstantSpec {
    pub name: String,
    pub declaring_class: String,
    pub value: Value,
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub final_constant: bool,
    pub deprecated: bool,
    pub php_type: Option<String>,
    pub attributes: Vec<AttributeSpec>,
}

/// One exported class name and its canonical internal definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassSpec {
    pub exported_name: String,
    pub canonical_name: String,
    pub extension: String,
    pub internal: bool,
    pub interface: bool,
    pub trait_type: bool,
    pub enum_type: bool,
    pub abstract_class: bool,
    pub final_class: bool,
    pub readonly_class: bool,
    pub instantiable: bool,
    pub cloneable: bool,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
    pub methods: Vec<MethodSpec>,
    pub properties: Vec<PropertySpec>,
    pub constants: Vec<ClassConstantSpec>,
    pub attributes: Vec<AttributeSpec>,
}

/// One exported internal function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    pub exported_name: String,
    pub signature: SignatureSpec,
}

/// One complete PHP internal extension contract.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionSpec {
    pub name: String,
    pub version: Option<String>,
    pub classes: Vec<ClassSpec>,
    pub functions: Vec<FunctionSpec>,
    pub constants: Vec<(String, Value)>,
}
