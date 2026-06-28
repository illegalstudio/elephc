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
use crate::span::Span;

use super::{FunctionSig, PhpType};

/// Compile-time attribute argument value. Captures the subset of PHP
/// attribute argument expressions that reflection helpers can materialize:
/// scalars (string/int/bool/null/float), and nested arrays of the same.
///
/// `Float` stores the IEEE-754 bit pattern (`f64::to_bits`) rather than an
/// `f64` so the enum can keep deriving `Eq`/`Hash`/`Ord` (used by the
/// reflection de-duplication `BTreeMap` and schema hashing). Reconstruct the
/// value with `f64::from_bits`.
///
/// `ConstRef` and `ScopedConst` are *deferred symbolic references* — a global
/// constant name, or a `Type::MEMBER` class-constant / enum-case reference.
/// Their values are not known at schema-collection time (global constants are
/// not yet registered and enum cases are not yet built), so they carry the
/// canonical names and are resolved later, when the synthetic reflection method
/// bodies (`getArguments()` / `newInstance()`) are lowered through the normal
/// constant/enum resolution path. Enum-case references resolve to the case
/// *object*, matching PHP's `ReflectionAttribute::getArguments()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttrArgValue {
    Null,
    Int(i64),
    Bool(bool),
    Str(String),
    Float(u64),
    Array(Vec<AttrArgEntry>),
    /// Reference to a global constant by canonical name (`#[A(SOME_CONST)]`).
    ConstRef(String),
    /// Reference to a class constant or enum case, carried as
    /// (canonical type name, member name) — e.g. `#[A(C::BAR)]` or `#[A(E::Case)]`.
    ScopedConst(String, String),
}

/// One entry of an attribute argument list or of a nested attribute array.
/// `key` is `None` for a positional argument / next sequential array element,
/// `Some(AttrKey::Str)` for a named argument (`#[A(name: 1)]`) or string array
/// key, and `Some(AttrKey::Int)` for an explicit integer array key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AttrArgEntry {
    pub key: Option<AttrKey>,
    pub value: AttrArgValue,
}

/// A resolved array/named-argument key for an [`AttrArgEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttrKey {
    Int(i64),
    Str(String),
}

/// Property hook contract for `get`/`set` hook declarations in classes and interfaces.
#[derive(Debug, Clone)]
pub struct PropertyHookContract {
    pub get_type: Option<PhpType>,
    pub set_type: Option<PhpType>,
    pub get_by_ref: bool,
    pub declaring_type: String,
    pub span: Span,
}

/// Compares PropertyHookContract by get/set types and declaring type.
/// Does not compare span — two contracts at different source positions
/// are considered equivalent if their types and declaring class match.
impl PartialEq for PropertyHookContract {
    /// Provides the Eq helper used by the schema module.
    fn eq(&self, other: &Self) -> bool {
        self.get_type == other.get_type
            && self.set_type == other.set_type
            && self.get_by_ref == other.get_by_ref
            && self.declaring_type == other.declaring_type
    }
}

/// Interface metadata for resolved declarations. Tracks parents, properties,
/// methods, constants, and vtable layout after name resolution and inheritance flattening.
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub interface_id: u64,
    pub parents: Vec<String>,
    pub properties: HashMap<String, PropertyHookContract>,
    pub property_order: Vec<String>,
    pub methods: HashMap<String, FunctionSig>,
    pub method_declaring_interfaces: HashMap<String, String>,
    pub method_order: Vec<String>,
    pub method_slots: HashMap<String, usize>,
    /// Interface constants (PHP 5.0+). Inherited from parent interfaces.
    pub constants: HashMap<String, crate::parser::ast::Expr>,
}

/// Class metadata for resolved declarations. Tracks inheritance, properties,
/// methods, constants, attributes, and vtable layout after name resolution and inheritance flattening.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassInfo {
    pub class_id: u64,
    pub parent: Option<String>,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly_class: bool,
    /// `true` if the class declaration carries the PHP 8.2
    /// `#[\AllowDynamicProperties]` attribute or inherits it from a parent.
    /// Codegen routes undeclared property storage through a per-object
    /// side-table when this flag is set.
    pub allow_dynamic_properties: bool,
    /// User-declared class constants (PHP 7.1+). Maps the constant name to
    /// its value expression — codegen inlines the literal at access time.
    pub constants: HashMap<String, crate::parser::ast::Expr>,
    /// Names of PHP 8 attributes attached to this class declaration, in
    /// source order. Name resolution stores canonical class-like text without
    /// a synthetic leading backslash, matching `ReflectionAttribute::getName()`.
    /// Reflection helpers read this list during codegen when materializing
    /// attribute-name arrays and `ReflectionAttribute` objects.
    pub attribute_names: Vec<String>,
    /// Literal arguments captured for each attribute, in source order and
    /// aligned with `attribute_names`. `None` means the source uses legal PHP
    /// attribute arguments that this reflection metadata model cannot
    /// materialize yet; callers that need arguments report that at query time.
    pub attribute_args: Vec<Option<Vec<AttrArgEntry>>>,
    /// Attribute names attached to methods visible on this class, keyed by
    /// PHP's case-insensitive method key. Inherited methods keep the metadata
    /// from the declaring class until overridden.
    pub method_attribute_names: HashMap<String, Vec<String>>,
    /// Literal method-attribute args aligned with `method_attribute_names`.
    pub method_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
    /// Attribute names attached to properties visible on this class. Property
    /// names are case-sensitive, so the source property name is the key.
    pub property_attribute_names: HashMap<String, Vec<String>>,
    /// Literal property-attribute args aligned with `property_attribute_names`.
    pub property_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
    /// Trait names used directly by this class declaration, preserving source order.
    pub used_traits: Vec<String>,
    pub properties: Vec<(String, PhpType)>,
    pub property_offsets: HashMap<String, usize>,
    pub property_declaring_classes: HashMap<String, String>,
    pub defaults: Vec<Option<Expr>>,
    pub property_visibilities: HashMap<String, Visibility>,
    /// PHP 8.4 asymmetric write (`set`) visibility, only for properties whose write visibility
    /// differs from their read visibility (e.g. `public private(set)`). Properties absent here
    /// use their `property_visibilities` entry for writes too.
    pub property_set_visibilities: HashMap<String, Visibility>,
    pub declared_properties: HashSet<String>,
    pub final_properties: HashSet<String>,
    pub readonly_properties: HashSet<String>,
    pub reference_properties: HashSet<String>,
    /// Reference properties whose ref-cell the OBJECT allocates and frees (created by
    /// taking a reference to a regular property — `$x = &$obj->prop` — or returning one
    /// by reference). A subset of `reference_properties`. Constructor-promoted `&$param`
    /// properties are reference properties but NOT here (their cell is borrowed from the
    /// caller). The object allocates a cell per such property at construction and releases
    /// it on destruction.
    pub owned_reference_properties: HashSet<String>,
    pub abstract_properties: HashSet<String>,
    pub abstract_property_hooks: HashMap<String, PropertyHookContract>,
    pub static_properties: Vec<(String, PhpType)>,
    pub static_defaults: Vec<Option<Expr>>,
    pub static_property_declaring_classes: HashMap<String, String>,
    pub static_property_visibilities: HashMap<String, Visibility>,
    pub declared_static_properties: HashSet<String>,
    pub final_static_properties: HashSet<String>,
    pub method_decls: Vec<ClassMethod>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    /// Callable signatures returned by instance/static methods, keyed by PHP's
    /// case-insensitive method key. The method body pass fills this after schemas exist.
    pub callable_method_return_sigs: HashMap<String, FunctionSig>,
    /// Callable element signatures returned by methods whose effective return
    /// type is `array<callable>` or an assoc array of callable values.
    pub callable_array_method_return_sigs: HashMap<String, FunctionSig>,
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

/// Enum case value, either an integer or a string (PHP 8.1+ backed enums).
#[derive(Debug, Clone, PartialEq)]
pub enum EnumCaseValue {
    Int(i64),
    Str(String),
}

/// Enum case metadata for a single case in a backed enum (PHP 8.1+).
/// The `value` field is `None` for unit-only enums with no backing type.
#[derive(Debug, Clone)]
pub struct EnumCaseInfo {
    pub name: String,
    pub value: Option<EnumCaseValue>,
}

/// Enum metadata for a resolved backed enum declaration (PHP 8.1+).
/// Tracks the backing type and ordered case list.
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub backing_type: Option<PhpType>,
    pub cases: Vec<EnumCaseInfo>,
}

/// Extern (FFI) function signature with name, parameters, return type,
/// and optional linked library for codegen linkage.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExternFunctionSig {
    pub name: String,
    pub params: Vec<(String, PhpType)>,
    pub return_type: PhpType,
    pub library: Option<String>,
}

/// Extern (FFI) class metadata with name, fields, total size, and field offsets
/// for codegen to emit packed struct layout.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExternClassInfo {
    pub name: String,
    pub fields: Vec<ExternFieldInfo>,
    pub total_size: usize,
}

/// Extern (FFI) field metadata with name, PHP type, and offset into the
/// containing extern class struct for codegen layout.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExternFieldInfo {
    pub name: String,
    pub php_type: PhpType,
    pub offset: usize,
}

/// Packed (non-nullable) class metadata with fields, total size, and per-field
/// offsets for codegen to emit a packed struct layout.
#[derive(Debug, Clone)]
pub struct PackedClassInfo {
    pub fields: Vec<PackedFieldInfo>,
    pub total_size: usize,
}

/// Packed field metadata with name, PHP type, and offset into the containing
/// packed class struct for codegen layout.
#[derive(Debug, Clone)]
pub struct PackedFieldInfo {
    pub name: String,
    pub php_type: PhpType,
    pub offset: usize,
}
