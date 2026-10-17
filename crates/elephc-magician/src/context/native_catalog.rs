//! Purpose:
//! Shares immutable generated native eval metadata across activation-local contexts.
//!
//! Called from:
//! - `crate::ffi::native_catalog` when generated code creates or publishes a context.
//!
//! Key details:
//! - Only Rust metadata and module-lifetime callback addresses enter the catalog.
//! - Runtime cells, scopes, declarations, handlers and aliases stay context-local.
//! - The thread-local cache retains at most eight module generations.

use super::*;
use std::collections::VecDeque;
use std::rc::Rc;

mod shared;
pub(crate) use shared::SharedNative;

const CATALOG_CACHE_LIMIT: usize = 8;

thread_local! {
    static NATIVE_CATALOGS: RefCell<VecDeque<(u64, NativeCatalog)>> =
        const { RefCell::new(VecDeque::new()) };
}

/// Frozen generated metadata; cloning shares allocations without cloning descriptors.
#[derive(Clone)]
struct NativeCatalog {
    declared_class_names: Rc<Vec<String>>,
    declared_interface_names: Rc<Vec<String>>,
    declared_trait_names: Rc<Vec<String>>,
    native_global_constants: Rc<HashMap<String, EvalNativeGlobalConstant>>,
    native_user_constants: Rc<HashMap<String, EvalNativeUserConstant>>,
    native_functions: Rc<HashMap<String, NativeFunction>>,
    native_methods: Rc<HashMap<(String, String), NativeCallableSignature>>,
    native_static_methods: Rc<HashMap<(String, String), NativeCallableSignature>>,
    native_constructors: Rc<HashMap<String, NativeCallableSignature>>,
    native_class_parents: Rc<HashMap<String, String>>,
    native_class_attributes: Rc<HashMap<String, Vec<EvalAttribute>>>,
    native_method_attributes: Rc<HashMap<(String, String), Vec<EvalAttribute>>>,
    native_constant_attributes: Rc<HashMap<(String, String), Vec<EvalAttribute>>>,
    native_interface_properties: Rc<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    native_abstract_properties: Rc<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    native_property_types: Rc<HashMap<(String, String), EvalParameterType>>,
    native_property_defaults: Rc<HashMap<(String, String), NativeCallableDefault>>,
    native_property_attributes: Rc<HashMap<(String, String), Vec<EvalAttribute>>>,
}

impl ElephcEvalContext {
    /// Attaches cached native metadata while retaining this context's fresh runtime state.
    pub(crate) fn use_native_catalog(&mut self, generation: u64) {
        self.native_catalog_generation = generation;
        let catalog = NATIVE_CATALOGS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let index = cache.iter().position(|(key, _)| *key == generation)?;
            let entry = cache.remove(index)?;
            let catalog = entry.1.clone();
            cache.push_back(entry);
            Some(catalog)
        });
        if let Some(catalog) = catalog {
            self.declared_class_names = SharedNative::from_shared(catalog.declared_class_names);
            self.declared_interface_names = SharedNative::from_shared(catalog.declared_interface_names);
            self.declared_trait_names = SharedNative::from_shared(catalog.declared_trait_names);
            self.native_global_constants = SharedNative::from_shared(catalog.native_global_constants);
            self.native_user_constants = SharedNative::from_shared(catalog.native_user_constants);
            self.native_functions = SharedNative::from_shared(catalog.native_functions);
            self.native_methods = SharedNative::from_shared(catalog.native_methods);
            self.native_static_methods = SharedNative::from_shared(catalog.native_static_methods);
            self.native_constructors = SharedNative::from_shared(catalog.native_constructors);
            self.native_class_parents = SharedNative::from_shared(catalog.native_class_parents);
            self.native_class_attributes = SharedNative::from_shared(catalog.native_class_attributes);
            self.native_method_attributes = SharedNative::from_shared(catalog.native_method_attributes);
            self.native_constant_attributes = SharedNative::from_shared(catalog.native_constant_attributes);
            self.native_interface_properties = SharedNative::from_shared(catalog.native_interface_properties);
            self.native_abstract_properties = SharedNative::from_shared(catalog.native_abstract_properties);
            self.native_property_types = SharedNative::from_shared(catalog.native_property_types);
            self.native_property_defaults = SharedNative::from_shared(catalog.native_property_defaults);
            self.native_property_attributes = SharedNative::from_shared(catalog.native_property_attributes);
            self.native_catalog_ready = true;
        }
    }

    /// Reports whether codegen can skip the generated registration sequence.
    pub(crate) fn native_catalog_is_ready(&self) -> bool {
        self.native_catalog_ready
    }

    /// Remembers a failed registration so a partial catalog cannot be published.
    pub(crate) fn fail_native_catalog_registration(&mut self) {
        self.native_catalog_registration_failed = true;
    }

    /// Publishes only a complete generated catalog before any dynamic execution.
    pub(crate) fn publish_native_catalog(&mut self) -> bool {
        if self.native_catalog_generation == 0 || self.native_catalog_ready
            || self.native_catalog_registration_failed || !self.classes.is_empty()
            || !self.interfaces.is_empty() || !self.traits.is_empty() || !self.enums.is_empty()
            || !self.class_aliases.is_empty() || !self.functions.is_empty()
            || !self.closures.is_empty() || !self.constants.is_empty()
        {
            return false;
        }
        let catalog = NativeCatalog {
            declared_class_names: self.declared_class_names.share(),
            declared_interface_names: self.declared_interface_names.share(),
            declared_trait_names: self.declared_trait_names.share(),
            native_global_constants: self.native_global_constants.share(),
            native_user_constants: self.native_user_constants.share(),
            native_functions: self.native_functions.share(),
            native_methods: self.native_methods.share(),
            native_static_methods: self.native_static_methods.share(),
            native_constructors: self.native_constructors.share(),
            native_class_parents: self.native_class_parents.share(),
            native_class_attributes: self.native_class_attributes.share(),
            native_method_attributes: self.native_method_attributes.share(),
            native_constant_attributes: self.native_constant_attributes.share(),
            native_interface_properties: self.native_interface_properties.share(),
            native_abstract_properties: self.native_abstract_properties.share(),
            native_property_types: self.native_property_types.share(),
            native_property_defaults: self.native_property_defaults.share(),
            native_property_attributes: self.native_property_attributes.share(),
        };
        NATIVE_CATALOGS.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.retain(|(key, _)| *key != self.native_catalog_generation);
            if cache.len() == CATALOG_CACHE_LIMIT { cache.pop_front(); }
            cache.push_back((self.native_catalog_generation, catalog));
        });
        self.native_catalog_ready = true;
        true
    }
}

#[cfg(test)]
mod tests;
