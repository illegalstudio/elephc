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
use crate::types::{ClassInfo, FunctionSig, PhpType};

#[derive(Default)]
pub(super) struct ClassBuildState {
    pub(super) allow_dynamic_properties: bool,
    pub(super) prop_types: Vec<(String, PhpType)>,
    pub(super) property_offsets: HashMap<String, usize>,
    pub(super) property_declaring_classes: HashMap<String, String>,
    pub(super) defaults: Vec<Option<Expr>>,
    pub(super) property_visibilities: HashMap<String, Visibility>,
    pub(super) declared_properties: HashSet<String>,
    pub(super) final_properties: HashSet<String>,
    pub(super) readonly_properties: HashSet<String>,
    pub(super) reference_properties: HashSet<String>,
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
}

impl ClassBuildState {
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
                .map(|c| (c.name.clone(), c.value.clone()))
                .collect(),
            attribute_names: collect_attribute_names(&class.attributes),
            attribute_args,
            properties: self.prop_types,
            property_offsets: self.property_offsets,
            property_declaring_classes: self.property_declaring_classes,
            defaults: self.defaults,
            property_visibilities: self.property_visibilities,
            declared_properties: self.declared_properties,
            final_properties: self.final_properties,
            readonly_properties: self.readonly_properties,
            reference_properties: self.reference_properties,
            static_properties: self.static_prop_types,
            static_defaults: self.static_defaults,
            static_property_declaring_classes: self.static_property_declaring_classes,
            static_property_visibilities: self.static_property_visibilities,
            declared_static_properties: self.declared_static_properties,
            final_static_properties: self.final_static_properties,
            method_decls: class.methods.clone(),
            methods: self.method_sigs,
            static_methods: self.static_sigs,
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
) -> Vec<Option<Vec<crate::types::AttrArgValue>>> {
    use crate::parser::ast::ExprKind;
    use crate::types::AttrArgValue;

    let mut out = Vec::new();
    for group in groups {
        for attr in &group.attributes {
            let mut args = Vec::new();
            let mut supported = true;
            for arg_expr in &attr.args {
                match &arg_expr.kind {
                    ExprKind::StringLiteral(value) => {
                        args.push(AttrArgValue::Str(value.clone()))
                    }
                    ExprKind::IntLiteral(value) => args.push(AttrArgValue::Int(*value)),
                    ExprKind::BoolLiteral(value) => args.push(AttrArgValue::Bool(*value)),
                    ExprKind::Null => args.push(AttrArgValue::Null),
                    ExprKind::Negate(inner) => {
                        if let ExprKind::IntLiteral(n) = &inner.kind {
                            args.push(AttrArgValue::Int(n.wrapping_neg()));
                        } else {
                            supported = false;
                            break;
                        }
                    }
                    ExprKind::NamedArg { .. } => {
                        supported = false;
                        break;
                    }
                    _ => {
                        supported = false;
                        break;
                    }
                }
            }
            out.push(if supported { Some(args) } else { None });
        }
    }
    out
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
    fn inherit_properties(&mut self, parent: &ClassInfo) {
        for (index, (name, ty)) in parent.properties.iter().enumerate() {
            self.prop_types.push((name.clone(), ty.clone()));
            self.property_offsets.insert(name.clone(), 8 + index * 16);
            self.defaults.push(parent.defaults[index].clone());
            if let Some(visibility) = parent.property_visibilities.get(name) {
                self.property_visibilities
                    .insert(name.clone(), visibility.clone());
            }
            if parent.declared_properties.contains(name) {
                self.declared_properties.insert(name.clone());
            }
            if let Some(declaring_class) = parent.property_declaring_classes.get(name) {
                self.property_declaring_classes
                    .insert(name.clone(), declaring_class.clone());
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
        }
    }

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
            if parent.final_static_properties.contains(name) {
                self.final_static_properties.insert(name.clone());
            }
        }
    }

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
        }
        self.vtable_methods = parent.vtable_methods.clone();
        self.vtable_slots = parent.vtable_slots.clone();
    }

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
        }
        self.static_vtable_methods = parent.static_vtable_methods.clone();
        self.static_vtable_slots = parent.static_vtable_slots.clone();
    }
}
