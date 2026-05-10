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

use crate::parser::ast::{Expr, Visibility};
use crate::types::traits::FlattenedClass;
use crate::types::{ClassInfo, FunctionSig, PhpType};

#[derive(Default)]
pub(super) struct ClassBuildState {
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
        }
        state
    }

    pub(super) fn into_class_info(
        self,
        class_id: u64,
        class: &FlattenedClass,
        constructor_param_to_prop: Vec<Option<String>>,
    ) -> ClassInfo {
        ClassInfo {
            class_id,
            parent: class.extends.clone(),
            is_abstract: class.is_abstract,
            is_final: class.is_final,
            is_readonly_class: class.is_readonly_class,
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
        }
    }

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
