//! Purpose:
//! Validates class schema state rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, Visibility};
use crate::types::traits::FlattenedClass;
use crate::types::{ClassInfo, FunctionSig, PhpType, PropertyHookContract};

use super::constants::resolve_lexical_class_constant_value;

/// Accumulates class metadata during schema validation, then emits the
/// immutable `ClassInfo` that the type checker and codegen consume.
///
/// All fields start empty and are populated by the class-building pipeline.
/// When inheriting from a parent, the four `inherit_*` methods copy
/// non-private, non-final members so the child class correctly overrides
/// or augments the parent's surface area.
#[derive(Default)]
pub(super) struct ClassBuildState {
    pub(super) allow_dynamic_properties: bool,
    pub(super) prop_types: Vec<(String, PhpType)>,
    pub(super) property_offsets: HashMap<String, usize>,
    pub(super) property_declaring_classes: HashMap<String, String>,
    pub(super) defaults: Vec<Option<Expr>>,
    pub(super) property_visibilities: HashMap<String, Visibility>,
    pub(super) property_set_visibilities: HashMap<String, Visibility>,
    pub(super) declared_properties: HashSet<String>,
    pub(super) final_properties: HashSet<String>,
    pub(super) readonly_properties: HashSet<String>,
    pub(super) reference_properties: HashSet<String>,
    pub(super) abstract_properties: HashSet<String>,
    pub(super) abstract_property_hooks: HashMap<String, PropertyHookContract>,
    pub(super) static_prop_types: Vec<(String, PhpType)>,
    pub(super) static_defaults: Vec<Option<Expr>>,
    pub(super) static_property_declaring_classes: HashMap<String, String>,
    pub(super) static_property_visibilities: HashMap<String, Visibility>,
    pub(super) declared_static_properties: HashSet<String>,
    pub(super) final_static_properties: HashSet<String>,
    pub(super) method_sigs: HashMap<String, FunctionSig>,
    pub(super) static_sigs: HashMap<String, FunctionSig>,
    pub(super) method_visibilities: HashMap<String, Visibility>,
    pub(super) final_methods: HashSet<String>,
    pub(super) method_declaring_classes: HashMap<String, String>,
    pub(super) method_impl_classes: HashMap<String, String>,
    pub(super) vtable_methods: Vec<String>,
    pub(super) vtable_slots: HashMap<String, usize>,
    pub(super) static_method_visibilities: HashMap<String, Visibility>,
    pub(super) final_static_methods: HashSet<String>,
    pub(super) static_method_declaring_classes: HashMap<String, String>,
    pub(super) static_method_impl_classes: HashMap<String, String>,
    pub(super) static_vtable_methods: Vec<String>,
    pub(super) static_vtable_slots: HashMap<String, usize>,
    pub(super) interfaces: Vec<String>,
    pub(super) method_attribute_names: HashMap<String, Vec<String>>,
    pub(super) method_attribute_args:
        HashMap<String, Vec<Option<Vec<crate::types::AttrArgEntry>>>>,
    pub(super) property_attribute_names: HashMap<String, Vec<String>>,
    pub(super) property_attribute_args:
        HashMap<String, Vec<Option<Vec<crate::types::AttrArgEntry>>>>,
}

impl ClassBuildState {
    /// Creates a fresh `ClassBuildState`, optionally inheriting metadata from
    /// a parent `ClassInfo` by copying all inheritable properties, static
    /// properties, methods, and static methods.  Private and final members
    /// are excluded.  The parent's interface list and `allow_dynamic_properties`
    /// flag are also propagated.
    pub(super) fn from_parent(parent_info: Option<&ClassInfo>) -> Self {
        let mut state = Self::default();
        if let Some(parent) = parent_info {
            state.inherit_properties(parent);
            state.inherit_static_properties(parent);
            state.inherit_methods(parent);
            state.inherit_static_methods(parent);
            state.interfaces = parent.interfaces.clone();
            state.allow_dynamic_properties = parent.allow_dynamic_properties;
        }
        state
    }

    /// Consumes `self` and assembles an immutable `ClassInfo` struct.
    /// Resolves all class constant expressions via
    /// `resolve_lexical_class_constant_value`, collects attributes, and
    /// merges the accumulated property/method metadata with the flattened
    /// class AST.  Returns `CompileError` if any constant expression is
    /// ill-formed.
    pub(super) fn into_class_info(
        self,
        class_id: u64,
        class: &FlattenedClass,
        constructor_param_to_prop: Vec<Option<String>>,
    ) -> Result<ClassInfo, CompileError> {
        let attribute_args = collect_attribute_args(&class.attributes);
        Ok(ClassInfo {
            class_id,
            parent: class.extends.clone(),
            is_abstract: class.is_abstract,
            is_final: class.is_final,
            is_readonly_class: class.is_readonly_class,
            allow_dynamic_properties: self.allow_dynamic_properties
                || class_has_allow_dynamic_properties(class),
            constants: class
                .constants
                .iter()
                .map(|c| {
                    Ok((
                        c.name.clone(),
                        resolve_lexical_class_constant_value(&c.value, class)?,
                    ))
                })
                .collect::<Result<HashMap<_, _>, CompileError>>()?,
            attribute_names: collect_attribute_names(&class.attributes),
            attribute_args,
            method_attribute_names: self.method_attribute_names,
            method_attribute_args: self.method_attribute_args,
            property_attribute_names: self.property_attribute_names,
            property_attribute_args: self.property_attribute_args,
            used_traits: class.used_traits.clone(),
            properties: self.prop_types,
            property_offsets: self.property_offsets,
            property_declaring_classes: self.property_declaring_classes,
            defaults: self.defaults,
            property_visibilities: self.property_visibilities,
            property_set_visibilities: self.property_set_visibilities,
            declared_properties: self.declared_properties,
            final_properties: self.final_properties,
            readonly_properties: self.readonly_properties,
            reference_properties: self.reference_properties,
            owned_reference_properties: HashSet::new(),
            abstract_properties: self.abstract_properties,
            abstract_property_hooks: self.abstract_property_hooks,
            static_properties: self.static_prop_types,
            static_defaults: self.static_defaults,
            static_property_declaring_classes: self.static_property_declaring_classes,
            static_property_visibilities: self.static_property_visibilities,
            declared_static_properties: self.declared_static_properties,
            final_static_properties: self.final_static_properties,
            method_decls: class.methods.clone(),
            methods: self.method_sigs,
            static_methods: self.static_sigs,
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: self.method_visibilities,
            final_methods: self.final_methods,
            method_declaring_classes: self.method_declaring_classes,
            method_impl_classes: self.method_impl_classes,
            vtable_methods: self.vtable_methods,
            vtable_slots: self.vtable_slots,
            static_method_visibilities: self.static_method_visibilities,
            final_static_methods: self.final_static_methods,
            static_method_declaring_classes: self.static_method_declaring_classes,
            static_method_impl_classes: self.static_method_impl_classes,
            static_vtable_methods: self.static_vtable_methods,
            static_vtable_slots: self.static_vtable_slots,
            interfaces: self.interfaces,
            constructor_param_to_prop,
        })
    }

}

/// Collect attribute names from a class's attribute groups, preserving source
/// order. Name resolution has already canonicalised fully-qualified names by
/// the time this runs, so names are emitted in ReflectionAttribute::getName()
/// shape without a synthetic leading backslash.
pub(super) fn collect_attribute_names(
    groups: &[crate::parser::ast::AttributeGroup],
) -> Vec<String> {
    let mut out = Vec::new();
    for group in groups {
        for attr in &group.attributes {
            // Name resolution normalises attribute references to the canonical
            // class-like text (no leading backslash), so emit it as-is and let
            // `class_attribute_names()` callers see PHP's `ReflectionAttribute::
            // getName()` shape — namespace-qualified or bare identifier, never a
            // synthetic leading `\`.
            out.push(attr.name.as_str().to_string());
        }
    }
    out
}

/// Collect the positional literal arguments of every attribute, in the
/// same order as `collect_attribute_names`. Captures strings, ints, bools,
/// and null directly. Negation (`-N`) of an int literal is folded so PHP's
/// `#[Status(-1)]` survives parsing. Unsupported metadata is marked as
/// `None` so legal PHP attribute syntax can still compile until a runtime
/// reflection helper needs the missing argument payload.
pub(super) fn collect_attribute_args(
    groups: &[crate::parser::ast::AttributeGroup],
) -> Vec<Option<Vec<crate::types::AttrArgEntry>>> {
    use crate::parser::ast::ExprKind;
    use crate::types::{AttrArgEntry, AttrKey};

    let mut out = Vec::new();
    for group in groups {
        for attr in &group.attributes {
            let mut entries = Vec::new();
            let mut supported = true;
            for arg_expr in &attr.args {
                // A named argument (`#[A(name: value)]`) keys the entry by its
                // string name; positional arguments stay unkeyed and reflection
                // materializes them at sequential integer offsets like PHP.
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

/// Folds a single attribute-argument expression to a compile-time
/// [`AttrArgValue`], or `None` when the expression is not a constant shape
/// reflection can materialize. Handles scalars (string/int/bool/null/float),
/// negation of numeric literals, positional/associative array literals, and
/// symbolic references (global constants, class constants, enum cases). The
/// symbolic references are captured by canonical name and resolved later, when
/// the synthetic reflection method bodies are lowered (see [`AttrArgValue`]).
fn fold_attr_value(expr: &crate::parser::ast::Expr) -> Option<crate::types::AttrArgValue> {
    use crate::parser::ast::ExprKind;
    use crate::types::{AttrArgEntry, AttrArgValue};

    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(AttrArgValue::Str(value.clone())),
        ExprKind::IntLiteral(value) => Some(AttrArgValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(AttrArgValue::Float(value.to_bits())),
        ExprKind::BoolLiteral(value) => Some(AttrArgValue::Bool(*value)),
        ExprKind::Null => Some(AttrArgValue::Null),
        // A bare constant reference (`#[A(SOME_CONST)]`). Name resolution has
        // already canonicalised the name, so capture it for late resolution.
        ExprKind::ConstRef(name) => Some(AttrArgValue::ConstRef(name.as_str().to_string())),
        // A class constant or enum case (`#[A(C::BAR)]` / `#[A(E::Case)]`). Only
        // a named (class/enum) receiver can be resolved outside a class scope;
        // `self`/`parent`/`static` have no meaning here and stay unsupported.
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_receiver_type_name(receiver)
                .map(|type_name| AttrArgValue::ScopedConst(type_name, name.clone()))
        }
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(n) => Some(AttrArgValue::Int(n.wrapping_neg())),
            ExprKind::FloatLiteral(n) => Some(AttrArgValue::Float((-n).to_bits())),
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

/// Folds an associative-array key expression to an [`AttrKey`], or `None` when
/// it is not an integer or string literal key (the only keys PHP allows).
fn fold_attr_key(expr: &crate::parser::ast::Expr) -> Option<crate::types::AttrKey> {
    use crate::parser::ast::ExprKind;
    use crate::types::AttrKey;

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

/// Returns the canonical type name of a static receiver when it is a named
/// class/enum (`#[A(C::BAR)]`), or `None` for `self`/`parent`/`static`, which
/// have no resolvable meaning in the attribute argument position.
fn scoped_receiver_type_name(
    receiver: &crate::parser::ast::StaticReceiver,
) -> Option<String> {
    use crate::parser::ast::StaticReceiver;
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => None,
    }
}

/// Returns `true` if the class declaration carries the PHP 8.2
/// `#[\AllowDynamicProperties]` marker attribute.
pub(super) fn class_has_allow_dynamic_properties(class: &FlattenedClass) -> bool {
    class.attributes.iter().any(|group| {
        group.attributes.iter().any(|attr| {
            super::super::validation::matches_global_builtin_attribute(
                attr,
                "AllowDynamicProperties",
            )
        })
    })
}

impl ClassBuildState {
    /// Copies all inheritable instance property metadata from `parent` into
    /// `self`: types, offsets, defaults, visibilities, declaring classes, and
    /// attributes.  Final, private, and abstract flags are propagated.
    /// Skips properties that are not declared on `parent` itself.
    fn inherit_properties(&mut self, parent: &ClassInfo) {
        for (index, (name, ty)) in parent.properties.iter().enumerate() {
            self.prop_types.push((name.clone(), ty.clone()));
            self.property_offsets.insert(name.clone(), 8 + index * 16);
            self.defaults.push(parent.defaults[index].clone());
            if let Some(visibility) = parent.property_visibilities.get(name) {
                self.property_visibilities
                    .insert(name.clone(), visibility.clone());
            }
            if let Some(set_visibility) = parent.property_set_visibilities.get(name) {
                self.property_set_visibilities
                    .insert(name.clone(), set_visibility.clone());
            }
            if parent.declared_properties.contains(name) {
                self.declared_properties.insert(name.clone());
            }
            if let Some(declaring_class) = parent.property_declaring_classes.get(name) {
                self.property_declaring_classes
                    .insert(name.clone(), declaring_class.clone());
            }
            if let Some(names) = parent.property_attribute_names.get(name) {
                self.property_attribute_names
                    .insert(name.clone(), names.clone());
            }
            if let Some(args) = parent.property_attribute_args.get(name) {
                self.property_attribute_args
                    .insert(name.clone(), args.clone());
            }
            if parent.final_properties.contains(name) {
                self.final_properties.insert(name.clone());
            }
            if parent.readonly_properties.contains(name) {
                self.readonly_properties.insert(name.clone());
            }
            if parent.reference_properties.contains(name) {
                self.reference_properties.insert(name.clone());
            }
            if parent.abstract_properties.contains(name) {
                self.abstract_properties.insert(name.clone());
            }
            if let Some(contract) = parent.abstract_property_hooks.get(name) {
                self.abstract_property_hooks
                    .insert(name.clone(), contract.clone());
            }
        }
    }

    /// Copies all inheritable static property metadata from `parent` into
    /// `self`: types, defaults, visibilities, declaring classes, and
    /// attributes.  Final and declared flags are propagated.
    fn inherit_static_properties(&mut self, parent: &ClassInfo) {
        for (index, (name, ty)) in parent.static_properties.iter().enumerate() {
            self.static_prop_types.push((name.clone(), ty.clone()));
            self.static_defaults
                .push(parent.static_defaults[index].clone());
            if let Some(visibility) = parent.static_property_visibilities.get(name) {
                self.static_property_visibilities
                    .insert(name.clone(), visibility.clone());
            }
            if parent.declared_static_properties.contains(name) {
                self.declared_static_properties.insert(name.clone());
            }
            if let Some(declaring_class) = parent.static_property_declaring_classes.get(name) {
                self.static_property_declaring_classes
                    .insert(name.clone(), declaring_class.clone());
            }
            if let Some(names) = parent.property_attribute_names.get(name) {
                self.property_attribute_names
                    .insert(name.clone(), names.clone());
            }
            if let Some(args) = parent.property_attribute_args.get(name) {
                self.property_attribute_args
                    .insert(name.clone(), args.clone());
            }
            if parent.final_static_properties.contains(name) {
                self.final_static_properties.insert(name.clone());
            }
        }
    }

    /// Copies all non-private, non-final method metadata from `parent` into
    /// `self`: signatures, visibilities, declaring/implementing classes, and
    /// attributes.  Also copies the parent's vtable so the child starts
    /// with the parent's method order and slot mapping.
    fn inherit_methods(&mut self, parent: &ClassInfo) {
        for (name, sig) in &parent.methods {
            if parent.method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            self.method_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.method_visibilities.get(name) {
                self.method_visibilities
                    .insert(name.clone(), visibility.clone());
            }
            if parent.final_methods.contains(name) {
                self.final_methods.insert(name.clone());
            }
            if let Some(declaring_class) = parent.method_declaring_classes.get(name) {
                self.method_declaring_classes
                    .insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.method_impl_classes.get(name) {
                self.method_impl_classes
                    .insert(name.clone(), impl_class.clone());
            }
            if let Some(names) = parent.method_attribute_names.get(name) {
                self.method_attribute_names
                    .insert(name.clone(), names.clone());
            }
            if let Some(args) = parent.method_attribute_args.get(name) {
                self.method_attribute_args
                    .insert(name.clone(), args.clone());
            }
        }
        self.vtable_methods = parent.vtable_methods.clone();
        self.vtable_slots = parent.vtable_slots.clone();
    }

    /// Copies all non-private, non-final static method metadata from `parent`
    /// into `self`: signatures, visibilities, declaring/implementing classes,
    /// and attributes.  Also copies the parent's static vtable.
    fn inherit_static_methods(&mut self, parent: &ClassInfo) {
        for (name, sig) in &parent.static_methods {
            if parent.static_method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            self.static_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.static_method_visibilities.get(name) {
                self.static_method_visibilities
                    .insert(name.clone(), visibility.clone());
            }
            if parent.final_static_methods.contains(name) {
                self.final_static_methods.insert(name.clone());
            }
            if let Some(declaring_class) = parent.static_method_declaring_classes.get(name) {
                self.static_method_declaring_classes
                    .insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.static_method_impl_classes.get(name) {
                self.static_method_impl_classes
                    .insert(name.clone(), impl_class.clone());
            }
            if let Some(names) = parent.method_attribute_names.get(name) {
                self.method_attribute_names
                    .insert(name.clone(), names.clone());
            }
            if let Some(args) = parent.method_attribute_args.get(name) {
                self.method_attribute_args
                    .insert(name.clone(), args.clone());
            }
        }
        self.static_vtable_methods = parent.static_vtable_methods.clone();
        self.static_vtable_slots = parent.static_vtable_slots.clone();
    }
}
