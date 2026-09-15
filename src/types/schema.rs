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

use crate::parser::ast::{
    AttributeGroup, ClassMethod, Expr, ExprKind, StaticReceiver, TypeExpr, Visibility,
};
use crate::span::Span;

use super::{FunctionSig, PhpType};

/// Compile-time attribute argument value. Captures the subset of PHP
/// attribute argument expressions that reflection helpers can materialize:
/// scalars (string/int/bool/null/float), `ClassName::class` strings, symbolic
/// references, and nested arrays of the same.
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
    /// Source span of the interface declaration, or `Span::dummy()` for compiler-injected interfaces.
    pub declaration_span: crate::span::Span,
    pub parents: Vec<String>,
    pub properties: HashMap<String, PropertyHookContract>,
    pub property_order: Vec<String>,
    /// Source declarations retained so Reflection can preserve lexical parameter-default names.
    pub method_decls: Vec<crate::parser::ast::ClassMethod>,
    /// Instance method contracts, keyed by PHP's case-insensitive method key.
    ///
    /// These entries are the only methods that participate in interface
    /// dispatch tables and `method_slots`.
    pub methods: HashMap<String, FunctionSig>,
    /// Exact return syntax for instance methods containing PHP's late-bound `static` type.
    pub late_static_method_returns: HashMap<String, TypeExpr>,
    pub method_declaring_interfaces: HashMap<String, String>,
    pub method_order: Vec<String>,
    pub method_slots: HashMap<String, usize>,
    /// Static method contracts, keyed by PHP's case-insensitive method key.
    ///
    /// PHP requires implementors to provide matching public static methods, but
    /// these entries never participate in instance interface dispatch tables.
    pub static_methods: HashMap<String, FunctionSig>,
    /// Exact return syntax for static methods containing PHP's late-bound `static` type.
    pub late_static_static_method_returns: HashMap<String, TypeExpr>,
    pub static_method_declaring_interfaces: HashMap<String, String>,
    pub static_method_order: Vec<String>,
    /// Interface constants (PHP 5.0+). Inherited from parent interfaces.
    pub constants: HashMap<String, crate::parser::ast::Expr>,
    /// PHP 8.3 declared types for visible interface constants.
    pub constant_types: HashMap<String, TypeExpr>,
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
    /// Source span of the class-like declaration, or `Span::dummy()` for compiler-injected classes.
    pub declaration_span: crate::span::Span,
    pub parent: Option<String>,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly_class: bool,
    /// `true` if the class declaration carries the PHP 8.2
    /// `#[\AllowDynamicProperties]` attribute or inherits it from a parent.
    /// Codegen routes undeclared property storage through a per-object
    /// side-table when this flag is set.
    pub allow_dynamic_properties: bool,
    /// EIR reserves a GC-visible property hash for subclasses declared by opaque eval.
    /// This is a storage capability, not PHP's permission to create dynamic properties.
    pub eval_property_storage: bool,
    /// EIR reserves the same GC-visible property hash for a class a two-argument `clone()`
    /// can reach with an override key whose name is only known at run time. Like
    /// `eval_property_storage` this is a storage capability, not PHP's permission: creating
    /// the property still emits php 8.5's `Creation of dynamic property C::$n is deprecated`.
    pub clone_override_property_storage: bool,
    /// EIR reserves the same GC-visible property hash for a class a reachable MUTATION can
    /// address under a strict ancestor's private name.
    ///
    /// php 7.4 removed shadow properties: `$child->p = 1` outside the class that declared
    /// `private $p` creates a DISTINCT dynamic property and leaves the ancestor's slot alone.
    /// Without a hash the write has nowhere to go and the backend's by-name ladder falls back
    /// onto the ancestor's physical slot, which is the storage escape phase B2 exists to close.
    /// Like the two flags above this is a storage capability, not php's permission: creating the
    /// property still emits php 8.5's `Creation of dynamic property C::$n is deprecated`.
    pub scope_dynamic_property_storage: bool,
    /// User-declared class constants (PHP 7.1+). Maps the constant name to
    /// its value expression — codegen inlines the literal at access time.
    pub constants: HashMap<String, crate::parser::ast::Expr>,
    /// Deprecation reason for class constants carrying `#[\Deprecated]`, keyed
    /// by the case-sensitive constant name. An empty string means no reason.
    pub constant_deprecations: HashMap<String, String>,
    /// PHP 8.3 declared types for constants declared directly on this class-like symbol.
    pub constant_types: HashMap<String, TypeExpr>,
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
    /// Trait method aliases declared directly by this class, as `(alias, Trait::method)`.
    pub trait_aliases: Vec<(String, String)>,
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
    /// Concrete and inherited hooks, including whether the visible property has backing storage.
    pub property_hooks: HashMap<String, crate::parser::ast::PropertyHooks>,
    pub static_properties: Vec<(String, PhpType)>,
    pub static_defaults: Vec<Option<Expr>>,
    pub static_property_declaring_classes: HashMap<String, String>,
    pub static_property_visibilities: HashMap<String, Visibility>,
    pub declared_static_properties: HashSet<String>,
    pub final_static_properties: HashSet<String>,
    pub method_decls: Vec<ClassMethod>,
    pub methods: HashMap<String, FunctionSig>,
    pub static_methods: HashMap<String, FunctionSig>,
    /// Exact return syntax for instance methods containing PHP's late-bound `static` type.
    pub late_static_method_returns: HashMap<String, TypeExpr>,
    /// Exact return syntax for static methods containing PHP's late-bound `static` type.
    pub late_static_static_method_returns: HashMap<String, TypeExpr>,
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

/// Returns the class whose `__construct` PHP runs when `class_name` is instantiated.
///
/// A private method is not inherited, so `ClassInfo::methods` deliberately carries no
/// `__construct` entry on a descendant of a class with a private constructor. PHP still
/// instantiates that descendant through the ancestor's constructor and names the declaring
/// class when the call site may not reach it, which is also what
/// `ReflectionClass::getConstructor()` reports while `method_exists()` answers `false`.
/// Walking to the nearest ancestor that still owns the entry models both halves.
///
/// The owner is the instantiated class itself whenever its own map has the entry, so an
/// inherited public or protected constructor resolves exactly as before.
pub fn constructor_owner<'a>(
    classes: &'a HashMap<String, ClassInfo>,
    class_name: &str,
) -> Option<(&'a str, &'a ClassInfo)> {
    let mut current = Some(class_name);
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(name) {
            return None;
        }
        let (owner_name, info) = classes.get_key_value(name)?;
        if info.methods.contains_key("__construct") {
            return Some((owner_name.as_str(), info));
        }
        current = info.parent.as_deref();
    }
    None
}

/// What php does with ONE property NAME on one class, seen from one scope.
///
/// A by-name property dispatch has FOUR possible answers, not two, and this compiler's
/// `ClassInfo` cannot distinguish them on its own: `properties` is the PHYSICAL slot table, so it
/// still carries a strict ancestor's private slot under its plain name, and `property_offsets`,
/// `property_visibilities` and `property_declaring_classes` all still answer for that name.
/// Every by-name dispatch has to ask `resolve_property_name` before it matches a runtime name
/// against a slot. All four outcomes were measured against php 8.5.10.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyNameResolution {
    /// The name addresses the slot `visible_property_index` resolves on this class.
    Visible,
    /// The name addresses the private slot `scope` declares and this class INHERITS.
    ///
    /// The index is the scope class's own layout index, which is also the receiver's because the
    /// physical layout of a subclass starts with its parent's slots in order. This is the same
    /// fact `crate::ir_lower::clone_overrides::scoped_setters` relies on, and it is what keeps
    /// `Base::readP()` on a `Child` that redeclares `private $p` reading BASE's slot.
    ScopePrivate {
        /// Class that declares the private property, an ancestor of the receiver's class.
        scope: String,
        /// That class's own physical slot index for the property.
        index: usize,
    },
    /// The name is not in this class's by-name table from this scope, so it is a DYNAMIC property.
    ///
    /// php 7.4 removed shadow properties: a strict ancestor's private property lives under a
    /// mangled key and the child's by-name table does not contain it at all. Outside the
    /// declaring class a read reports `Undefined property`, `isset()` answers false, `unset()` is
    /// a no-op, and a write CREATES a distinct dynamic property with the usual deprecation.
    Dynamic,
    /// The name is in the table, but this scope may not touch it: php raises a catchable `Error`.
    ///
    /// `Cannot access private property D::$n` for a private property declared by the receiver's
    /// OWN class, `Cannot access protected property P::$p` for a protected one reached from an
    /// unrelated scope. Both verbatim from php 8.5.10, with no scope suffix.
    Inaccessible(Visibility),
}

/// Resolves one property NAME against a receiver class and an invocation scope, `None` for global.
///
/// php's own order, measured against php 8.5.10 across the declaring scope, a child scope, an
/// unrelated scope and global scope:
///
/// 1. A scope that DECLARES a private property of this name, and that the receiver is an instance
///    of, selects its own slot. This is what makes a parent-private property reachable on a child
///    object and what keeps two same-named private slots apart.
/// 2. A name with no visible declaration is dynamic.
/// 3. Public is always visible.
/// 4. Protected needs php's ancestor-or-descendant test against the DECLARING class.
/// 5. Private declared by a STRICT ancestor is invisible, so it is dynamic; private declared by
///    the receiver's own class is an access error, because step 1 already took the one scope that
///    may reach it.
pub fn resolve_property_name(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    property: &str,
    scope: Option<&str>,
) -> PropertyNameResolution {
    if let Some(resolution) =
        resolve_scope_private_property_name(classes, class_name, property, scope)
    {
        return resolution;
    }
    let Some(info) = classes.get(class_name) else {
        return PropertyNameResolution::Dynamic;
    };
    if info.visible_property_index(property).is_none() {
        return PropertyNameResolution::Dynamic;
    }
    let visibility = info
        .property_visibilities
        .get(property)
        .cloned()
        .unwrap_or(Visibility::Public);
    let declaring = info
        .property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(class_name);
    match visibility {
        Visibility::Public => PropertyNameResolution::Visible,
        Visibility::Protected => {
            if scope_shares_class_hierarchy(classes, scope, declaring) {
                PropertyNameResolution::Visible
            } else {
                PropertyNameResolution::Inaccessible(Visibility::Protected)
            }
        }
        // Checker-injected builtin subclasses use synthetic PHP bodies to model engine-owned
        // methods. Those bodies must be able to reach the private storage inherited from another
        // injected builtin, such as FilterIterator::__construct writing IteratorIterator::$inner.
        // User code never receives this privilege: its lexical scope is not a catalog builtin.
        Visibility::Private
            if builtin_scope_owns_inherited_storage(classes, class_name, declaring, scope) =>
        {
            PropertyNameResolution::Visible
        }
        // Step 1 already answered for the declaring scope, so reaching here means this scope is
        // not it. A STRICT ancestor's slot is invisible rather than refused.
        Visibility::Private if declaring != class_name => PropertyNameResolution::Dynamic,
        Visibility::Private => PropertyNameResolution::Inaccessible(Visibility::Private),
    }
}

/// Returns whether a checker-injected builtin method is accessing inherited engine storage.
///
/// Builtin class bodies are synthetic compiler implementation details, not user-authored PHP.
/// Requiring their inherited private slots to follow userland dynamic-property rules would either
/// add a hash to fixed builtin layouts or reject every program that injects the relevant prelude.
/// The receiver and lexical scope must name the same builtin subclass, and the declaring class
/// must be a builtin ancestor, so ordinary subclasses and unrelated builtin receivers remain
/// governed by PHP visibility.
fn builtin_scope_owns_inherited_storage(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    declaring: &str,
    scope: Option<&str>,
) -> bool {
    let Some(scope) = scope else {
        return false;
    };
    let is_checker_injected = |name| {
        elephc_builtin_contract::lookup_class(name).is_some_and(|contract| {
            contract.aot == elephc_builtin_contract::ClassRoute::CheckerInjected
        })
    };
    scope == class_name
        && declaring != class_name
        && class_inherits_from(classes, class_name, declaring)
        && is_checker_injected(scope)
        && is_checker_injected(declaring)
}

/// Returns whether the layout of `class_name` carries `property` but php resolves it to a
/// DYNAMIC property from `scope`.
///
/// This is the strict-ancestor-private shape and nothing else. `resolve_property_name` alone is
/// too wide for it: that function answers `Dynamic` for EVERY name a class does not declare, so
/// an ordinary undeclared name on an `#[\AllowDynamicProperties]` class would pass too. The
/// physical-slot test is what narrows it, because only a strict ancestor's private slot is both
/// present in the layout and invisible by name.
///
/// It is the single authority for three decisions that must agree: which mutation sites reserve
/// per-instance hash storage (`crate::types::checker::scope_dynamic_storage`), which names the
/// backend must keep away from the physical slot
/// (`crate::codegen::lower_inst::objects::property_name_is_scope_dynamic`), and which names the
/// checker must not validate against the ancestor's declared type.
pub fn property_name_shadows_ancestor_private_slot(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    property: &str,
    scope: Option<&str>,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    classes.get(normalized).is_some_and(|class_info| {
        class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
    }) && resolve_property_name(classes, normalized, property, scope)
        == PropertyNameResolution::Dynamic
}

/// Returns the scope-selected private slot for a name, when the scope owns one the receiver has.
fn resolve_scope_private_property_name(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    property: &str,
    scope: Option<&str>,
) -> Option<PropertyNameResolution> {
    let scope_name = scope?;
    let scope_info = classes.get(scope_name)?;
    if !class_declares_private_property(scope_info, scope_name, property) {
        return None;
    }
    if scope_name == class_name {
        return Some(PropertyNameResolution::Visible);
    }
    if !class_inherits_from(classes, class_name, scope_name) {
        return None;
    }
    // The receiver's own by-name table resolves this name to its own shadowing slot, so the arm
    // has to carry the SCOPE's layout index instead of taking the receiver's answer.
    let index = scope_info.visible_property_index(property)?;
    Some(PropertyNameResolution::ScopePrivate {
        scope: scope_name.to_string(),
        index,
    })
}

/// Returns whether `class_info` itself declares `property` as private.
pub fn class_declares_private_property(
    class_info: &ClassInfo,
    class_name: &str,
    property: &str,
) -> bool {
    class_info.visible_property_index(property).is_some()
        && class_info.property_visibilities.get(property) == Some(&Visibility::Private)
        && class_info
            .property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name)
            == class_name
}

/// Returns whether `child` reaches `ancestor` through the declared parent chain.
pub fn class_inherits_from(
    classes: &HashMap<String, ClassInfo>,
    child: &str,
    ancestor: &str,
) -> bool {
    let mut current = classes.get(child).and_then(|info| info.parent.as_deref());
    let mut guard = 0usize;
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        guard += 1;
        if guard > classes.len() + 1 {
            return false;
        }
        current = classes.get(name).and_then(|info| info.parent.as_deref());
    }
    false
}

/// Returns php-src's `zend_check_protected` verdict: the scope is the class, an ancestor OR a
/// descendant of it.
pub fn scope_shares_class_hierarchy(
    classes: &HashMap<String, ClassInfo>,
    scope: Option<&str>,
    declaring: &str,
) -> bool {
    let Some(scope) = scope else {
        return false;
    };
    scope == declaring
        || class_inherits_from(classes, scope, declaring)
        || class_inherits_from(classes, declaring, scope)
}

impl ClassInfo {
    /// Returns whether the physical object layout includes a trailing property hash pointer.
    pub fn has_property_hash_storage(&self) -> bool {
        self.allow_dynamic_properties
            || self.eval_property_storage
            || self.clone_override_property_storage
            || self.scope_dynamic_property_storage
    }

    /// Returns whether CREATING a dynamic property on an instance is deprecated rather than free.
    ///
    /// The clone-override hash is reserved by the compiler, not requested by the program, so php
    /// 8.5 still reports `Creation of dynamic property C::$n is deprecated` for every class that
    /// carries neither `#[\AllowDynamicProperties]` nor stdClass's engine exemption. Eval's own
    /// reserved storage keeps its established behavior and is deliberately not consulted here.
    pub fn dynamic_property_creation_is_deprecated(&self) -> bool {
        (self.clone_override_property_storage || self.scope_dynamic_property_storage)
            && !self.allow_dynamic_properties
    }

    /// Returns whether an UNDECLARED property name addresses this class's hash directly.
    ///
    /// Eval's reserved storage is deliberately excluded: it is reached only through
    /// `__elephc_eval_property_hash_slot`, which gates every access on eval ownership so an
    /// ordinary native instance of the same class keeps refusing the name.
    pub fn dynamic_property_hash_is_name_addressable(&self) -> bool {
        self.allow_dynamic_properties
            || self.clone_override_property_storage
            || self.scope_dynamic_property_storage
    }

    /// Returns whether a method-map entry is a generated property accessor rather than a PHP method.
    pub fn is_property_hook_method(&self, method: &str) -> bool {
        self.property_hooks.iter().any(|(property, hooks)| hooks.matches_accessor(property, method))
    }

    /// Returns whether the visible property has hooks but no backing value anywhere in its ancestry.
    pub fn property_is_virtual(&self, property: &str) -> bool {
        self.property_hooks.get(property).is_some_and(|hooks| hooks.is_virtual())
    }

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
