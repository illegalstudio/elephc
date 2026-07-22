//! Purpose:
//! Stores module-wide codegen artifacts that can be shared across function contexts.
//! Deduplicates runtime callable descriptors, wrappers, and invokers by semantic shape.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` creates one state per generated module.
//! - `crate::codegen::lower_inst::callables` reuses emitted callable artifacts through it.
//!
//! Key details:
//! - Cached labels are global assembly entries emitted at their first call site.
//! - Receiver-bearing descriptors cache only immutable templates; each call still captures its object.

use crate::codegen::callable_dispatch::{RuntimeCallableCase, RuntimeStaticMethodCallableCase};
use crate::types::{FunctionSig, PhpType};

/// Module-wide artifacts emitted once and reused by every function lowering context.
#[derive(Default)]
pub(crate) struct SharedCodegenState {
    runtime_string_descriptor_cases:
        Vec<(Option<PhpType>, Option<Vec<String>>, Vec<RuntimeCallableCase>)>,
    runtime_static_method_descriptor_cases:
        Vec<(Option<Vec<String>>, Vec<RuntimeStaticMethodCallableCase>)>,
    runtime_static_method_descriptor_case_entries: Vec<RuntimeStaticMethodCallableCase>,
    runtime_instance_method_descriptors: Vec<RuntimeInstanceMethodDescriptorCacheEntry>,
    runtime_callable_invokers: Vec<RuntimeCallableInvokerCacheEntry>,
    runtime_builtin_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    runtime_extern_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
}

/// Reusable static descriptor template for one public instance method.
#[derive(Clone)]
pub(super) struct RuntimeInstanceMethodDescriptorTemplate {
    pub(super) descriptor_label: String,
}

/// Cache key and emitted template for one receiver-class/method/signature shape.
struct RuntimeInstanceMethodDescriptorCacheEntry {
    class_name: String,
    method_key: String,
    impl_class: String,
    signature: FunctionSig,
    template: RuntimeInstanceMethodDescriptorTemplate,
}

/// Cache key and label for one signature-compatible descriptor invoker body.
struct RuntimeCallableInvokerCacheEntry {
    signature: FunctionSig,
    captures: Vec<(String, PhpType, bool)>,
    label: String,
}

/// Cache key and label for one synthetic builtin or extern entry wrapper.
struct RuntimeCallWrapperCacheEntry {
    name: String,
    signature: FunctionSig,
    label: String,
}

impl SharedCodegenState {
    /// Returns cached runtime string-callable cases for the requested specialization.
    pub(super) fn runtime_string_descriptor_cases(
        &self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
    ) -> Option<Vec<RuntimeCallableCase>> {
        self.runtime_string_descriptor_cases
            .iter()
            .find(|(cached_ty, cached_names, _)| {
                cached_ty.as_ref() == source_arg_ty
                    && cached_names.as_deref() == candidate_names
            })
            .map(|(_, _, cases)| cases.clone())
    }

    /// Stores runtime string-callable cases after their global wrappers are emitted.
    pub(super) fn cache_runtime_string_descriptor_cases(
        &mut self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
        cases: &[RuntimeCallableCase],
    ) {
        self.runtime_string_descriptor_cases.push((
            source_arg_ty.cloned(),
            candidate_names.map(|names| names.to_vec()),
            cases.to_vec(),
        ));
    }

    /// Returns the module-wide public static-method descriptor cases, if emitted.
    pub(super) fn runtime_static_method_descriptor_cases(
        &self,
        candidate_names: Option<&[String]>,
    ) -> Option<Vec<RuntimeStaticMethodCallableCase>> {
        self.runtime_static_method_descriptor_cases
            .iter()
            .find(|(cached_names, _)| cached_names.as_deref() == candidate_names)
            .map(|(_, cases)| cases.clone())
    }

    /// Stores public static-method descriptors for reuse by later call sites.
    pub(super) fn cache_runtime_static_method_descriptor_cases(
        &mut self,
        candidate_names: Option<&[String]>,
        cases: &[RuntimeStaticMethodCallableCase],
    ) {
        self.runtime_static_method_descriptor_cases.push((
            candidate_names.map(|names| names.to_vec()),
            cases.to_vec(),
        ));
    }

    /// Returns one static-method descriptor case already emitted for another target set.
    pub(super) fn runtime_static_method_descriptor_case(
        &self,
        php_name: &str,
    ) -> Option<RuntimeStaticMethodCallableCase> {
        self.runtime_static_method_descriptor_case_entries
            .iter()
            .find(|case| case.case.php_name.as_deref() == Some(php_name))
            .cloned()
    }

    /// Records one static-method descriptor case for reuse across candidate sets.
    pub(super) fn cache_runtime_static_method_descriptor_case(
        &mut self,
        case: &RuntimeStaticMethodCallableCase,
    ) {
        self.runtime_static_method_descriptor_case_entries
            .push(case.clone());
    }

    /// Returns an emitted receiver-captured descriptor template for one method shape.
    pub(super) fn runtime_instance_method_descriptor(
        &self,
        class_name: &str,
        method_key: &str,
        impl_class: &str,
        signature: &FunctionSig,
    ) -> Option<RuntimeInstanceMethodDescriptorTemplate> {
        self.runtime_instance_method_descriptors
            .iter()
            .find(|entry| {
                entry.class_name == class_name
                    && entry.method_key == method_key
                    && entry.impl_class == impl_class
                    && entry.signature == *signature
            })
            .map(|entry| entry.template.clone())
    }

    /// Stores a receiver-captured descriptor template after first emission.
    pub(super) fn cache_runtime_instance_method_descriptor(
        &mut self,
        class_name: &str,
        method_key: &str,
        impl_class: &str,
        signature: &FunctionSig,
        template: RuntimeInstanceMethodDescriptorTemplate,
    ) {
        self.runtime_instance_method_descriptors
            .push(RuntimeInstanceMethodDescriptorCacheEntry {
                class_name: class_name.to_string(),
                method_key: method_key.to_string(),
                impl_class: impl_class.to_string(),
                signature: signature.clone(),
                template,
            });
    }

    /// Returns an already-emitted descriptor invoker with the same ABI shape.
    pub(super) fn runtime_callable_invoker(
        &self,
        signature: &FunctionSig,
        captures: &[(String, PhpType, bool)],
    ) -> Option<String> {
        self.runtime_callable_invokers
            .iter()
            .find(|entry| entry.signature == *signature && entry.captures == captures)
            .map(|entry| entry.label.clone())
    }

    /// Records a descriptor invoker body for module-wide signature reuse.
    pub(super) fn cache_runtime_callable_invoker(
        &mut self,
        signature: &FunctionSig,
        captures: &[(String, PhpType, bool)],
        label: &str,
    ) {
        self.runtime_callable_invokers
            .push(RuntimeCallableInvokerCacheEntry {
                signature: signature.clone(),
                captures: captures.to_vec(),
                label: label.to_string(),
            });
    }

    /// Returns a previously emitted synthetic builtin wrapper for the same signature.
    pub(super) fn runtime_builtin_wrapper(
        &self,
        name: &str,
        signature: &FunctionSig,
    ) -> Option<String> {
        cached_runtime_call_wrapper(&self.runtime_builtin_wrappers, name, signature)
    }

    /// Records a synthetic builtin wrapper for module-wide reuse.
    pub(super) fn cache_runtime_builtin_wrapper(
        &mut self,
        name: &str,
        signature: &FunctionSig,
        label: &str,
    ) {
        cache_runtime_call_wrapper(
            &mut self.runtime_builtin_wrappers,
            name,
            signature,
            label,
        );
    }

    /// Returns a previously emitted synthetic extern wrapper for the same signature.
    pub(super) fn runtime_extern_wrapper(
        &self,
        name: &str,
        signature: &FunctionSig,
    ) -> Option<String> {
        cached_runtime_call_wrapper(&self.runtime_extern_wrappers, name, signature)
    }

    /// Records a synthetic extern wrapper for module-wide reuse.
    pub(super) fn cache_runtime_extern_wrapper(
        &mut self,
        name: &str,
        signature: &FunctionSig,
        label: &str,
    ) {
        cache_runtime_call_wrapper(
            &mut self.runtime_extern_wrappers,
            name,
            signature,
            label,
        );
    }
}

/// Looks up a cached synthetic call wrapper by PHP name and ABI signature.
fn cached_runtime_call_wrapper(
    entries: &[RuntimeCallWrapperCacheEntry],
    name: &str,
    signature: &FunctionSig,
) -> Option<String> {
    entries
        .iter()
        .find(|entry| entry.name == name && entry.signature == *signature)
        .map(|entry| entry.label.clone())
}

/// Adds one synthetic call wrapper to its module-wide cache.
fn cache_runtime_call_wrapper(
    entries: &mut Vec<RuntimeCallWrapperCacheEntry>,
    name: &str,
    signature: &FunctionSig,
    label: &str,
) {
    entries.push(RuntimeCallWrapperCacheEntry {
        name: name.to_string(),
        signature: signature.clone(),
        label: label.to_string(),
    });
}
