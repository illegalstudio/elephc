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

use crate::parser::ast::{AttributeGroup, ClassMethod, Expr, ExprKind, StaticReceiver, Visibility};
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

/// Collects attribute names from attribute groups while preserving source order.
///
/// Name resolution has already canonicalized fully-qualified names by the time
/// checker/codegen metadata uses this helper, so returned names match
/// `ReflectionAttribute::getName()` shape without synthetic leading slashes.
pub(crate) fn collect_attribute_names(groups: &[AttributeGroup]) -> Vec<String> {
    let mut out = Vec::new();
    for group in groups {
        for attr in &group.attributes {
            out.push(attr.name.as_str().to_string());
        }
    }
    out
}

/// Collects materializable positional, named, and array attribute arguments in source order.
///
/// Legal PHP attribute expressions outside the current literal subset are
/// represented as `None` so compilation can proceed until a reflection query
/// needs the missing payload and reports the unsupported metadata.
pub(crate) fn collect_attribute_args(
    groups: &[AttributeGroup],
) -> Vec<Option<Vec<AttrArgEntry>>> {
    let mut out = Vec::new();
    for group in groups {
        for attr in &group.attributes {
            let mut entries = Vec::new();
            let mut supported = true;
            for arg_expr in &attr.args {
                let (key, value_expr) = match &arg_expr.kind {
                    ExprKind::NamedArg { name, value } => {
                        (Some(AttrKey::Str(name.clone())), value.as_ref())
                    }
                    _ => (None, arg_expr),
                };
                match fold_attr_value(value_expr) {
                    Some(value) => entries.push(AttrArgEntry { key, value }),
                    None => {
                        supported = false;
                        break;
                    }
                }
            }
            out.push(if supported { Some(entries) } else { None });
        }
    }
    out
}

/// Folds one attribute argument expression to retained reflection metadata.
fn fold_attr_value(expr: &Expr) -> Option<AttrArgValue> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(AttrArgValue::Str(value.clone())),
        ExprKind::IntLiteral(value) => Some(AttrArgValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(AttrArgValue::Float(value.to_bits())),
        ExprKind::BoolLiteral(value) => Some(AttrArgValue::Bool(*value)),
        ExprKind::Null => Some(AttrArgValue::Null),
        ExprKind::ConstRef(name) => Some(AttrArgValue::ConstRef(name.as_str().to_string())),
        ExprKind::ScopedConstantAccess { receiver, name } => scoped_receiver_type_name(receiver)
            .map(|type_name| AttrArgValue::ScopedConst(type_name, name.clone())),
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => Some(AttrArgValue::Str(name.as_str().to_string())),
        ExprKind::ClassConstant { .. } => None,
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(n) => Some(AttrArgValue::Int(n.wrapping_neg())),
            ExprKind::FloatLiteral(n) => Some(AttrArgValue::Float((-*n).to_bits())),
            _ => None,
        },
        ExprKind::ArrayLiteral(elements) => {
            let mut entries = Vec::with_capacity(elements.len());
            for element in elements {
                entries.push(AttrArgEntry {
                    key: None,
                    value: fold_attr_value(element)?,
                });
            }
            Some(AttrArgValue::Array(entries))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut entries = Vec::with_capacity(pairs.len());
            for (key_expr, value_expr) in pairs {
                entries.push(AttrArgEntry {
                    key: Some(fold_attr_key(key_expr)?),
                    value: fold_attr_value(value_expr)?,
                });
            }
            Some(AttrArgValue::Array(entries))
        }
        _ => None,
    }
}

/// Folds one supported associative attribute array key.
fn fold_attr_key(expr: &Expr) -> Option<AttrKey> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(AttrKey::Int(*value)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(n) => Some(AttrKey::Int(n.wrapping_neg())),
            _ => None,
        },
        ExprKind::StringLiteral(value) => Some(AttrKey::Str(value.clone())),
        _ => None,
    }
}

/// Returns the canonical named receiver for class-constant attribute arguments.
fn scoped_receiver_type_name(receiver: &StaticReceiver) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => None,
    }
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
/// instance/static methods, constants, and instance vtable layout after name
/// resolution and inheritance flattening.
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub interface_id: u64,
    pub parents: Vec<String>,
    pub properties: HashMap<String, PropertyHookContract>,
    pub property_order: Vec<String>,
    /// Instance method contracts, keyed by PHP's case-insensitive method key.
    ///
    /// These entries are the only methods that participate in interface
    /// dispatch tables and `method_slots`.
    pub methods: HashMap<String, FunctionSig>,
    pub method_declaring_interfaces: HashMap<String, String>,
    pub method_order: Vec<String>,
    pub method_slots: HashMap<String, usize>,
    /// Static method contracts, keyed by PHP's case-insensitive method key.
    ///
    /// PHP requires implementors to provide matching public static methods, but
    /// these entries never participate in instance interface dispatch tables.
    pub static_methods: HashMap<String, FunctionSig>,
    pub static_method_declaring_interfaces: HashMap<String, String>,
    pub static_method_order: Vec<String>,
    /// Interface constants (PHP 5.0+). Inherited from parent interfaces.
    pub constants: HashMap<String, crate::parser::ast::Expr>,
    /// Declaring interface for each visible constant, keyed by case-sensitive constant name.
    pub constant_declaring_interfaces: HashMap<String, String>,
    /// Interface constants declared with PHP 8.1+ `final`, including inherited parents.
    pub final_constants: HashSet<String>,
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
    /// Class constant visibilities keyed by case-sensitive constant name.
    pub constant_visibilities: HashMap<String, Visibility>,
    /// Class constants declared with PHP 8.1+ `final`, keyed by constant name.
    pub final_constants: HashSet<String>,
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
    /// Attribute names attached to class constants visible on this class.
    /// Constant names are case-sensitive, so the source constant name is the key.
    pub constant_attribute_names: HashMap<String, Vec<String>>,
    /// Literal class-constant-attribute args aligned with `constant_attribute_names`.
    pub constant_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
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
    /// Per-layout-slot typed-declaration flags for instance properties.
    ///
    /// The name-keyed `declared_properties` map describes the property currently
    /// visible by name in this class. This vector follows `properties` by index
    /// so hidden private parent slots keep their typed-property initialization
    /// metadata when a child declares a same-named property.
    pub property_declared_slots: Vec<bool>,
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
    pub promoted_properties: HashSet<String>,
    /// Per-layout-slot by-reference flags for instance properties.
    ///
    /// The name-keyed `reference_properties` map describes the currently
    /// visible property by name. Runtime GC descriptors need the original slot
    /// flag even when a private parent slot is shadowed by a child property.
    pub property_reference_slots: Vec<bool>,
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

impl ClassInfo {
    /// Resolves the layout index of the property visible by name on this class.
    ///
    /// The result follows `property_offsets` when present so private parent
    /// slots shadowed by child declarations do not win merely because they occur
    /// earlier in the physical object layout.
    pub fn visible_property_index(&self, property: &str) -> Option<usize> {
        self.property_offsets
            .get(property)
            .and_then(|offset| property_index_from_offset(*offset, self.properties.len()))
            .or_else(|| {
                self.properties
                    .iter()
                    .rposition(|(name, _)| name == property)
            })
    }

    /// Returns the property tuple visible by name on this class.
    pub fn visible_property(&self, property: &str) -> Option<(usize, &(String, PhpType))> {
        let index = self.visible_property_index(property)?;
        self.properties.get(index).map(|entry| (index, entry))
    }

    /// Returns whether one physical property slot has a declared PHP type.
    pub fn property_slot_is_declared(&self, index: usize, property: &str) -> bool {
        self.property_declared_slots
            .get(index)
            .copied()
            .unwrap_or_else(|| self.declared_properties.contains(property))
    }

    /// Returns whether the property visible by name has a declared PHP type.
    pub fn visible_property_is_declared(&self, property: &str) -> bool {
        self.visible_property(property)
            .is_some_and(|(index, (name, _))| self.property_slot_is_declared(index, name))
    }

    /// Returns whether one physical property slot stores a by-reference cell.
    pub fn property_slot_is_reference(&self, index: usize, property: &str) -> bool {
        self.property_reference_slots
            .get(index)
            .copied()
            .unwrap_or_else(|| self.reference_properties.contains(property))
    }

    /// Returns whether the property visible by name stores a by-reference cell.
    pub fn visible_property_is_reference(&self, property: &str) -> bool {
        self.visible_property(property)
            .is_some_and(|(index, (name, _))| self.property_slot_is_reference(index, name))
    }
}

/// Converts a property offset into a `properties` vector index when it points
/// at a normal object-property slot.
fn property_index_from_offset(offset: usize, property_count: usize) -> Option<usize> {
    let payload_offset = offset.checked_sub(8)?;
    if payload_offset % 16 != 0 {
        return None;
    }
    let index = payload_offset / 16;
    (index < property_count).then_some(index)
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
    pub attribute_names: Vec<String>,
    pub attribute_args: Vec<Option<Vec<AttrArgEntry>>>,
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
