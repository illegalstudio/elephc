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
//! - Owns the module-wide assembly label counter. It must not be per function: the readable part
//!   of a label is a lossy fragment of the PHP function/block name, so only a module-unique
//!   trailing id keeps two functions with similar names from emitting the same label.

use crate::codegen::callable_dispatch::{RuntimeCallableCase, RuntimeStaticMethodCallableCase};
use crate::types::{FunctionSig, PhpType};

/// Module-wide artifacts emitted once and reused by every function lowering context.
#[derive(Default)]
pub(crate) struct SharedCodegenState {
    runtime_string_descriptor_cases:
        Vec<(Option<PhpType>, Option<Vec<String>>, bool, Vec<RuntimeCallableCase>)>,
    runtime_static_method_descriptor_cases:
        Vec<(Option<Vec<String>>, Vec<RuntimeStaticMethodCallableCase>)>,
    runtime_static_method_descriptor_case_entries: Vec<RuntimeStaticMethodCallableCase>,
    runtime_instance_method_descriptors: Vec<RuntimeInstanceMethodDescriptorCacheEntry>,
    runtime_callable_invokers: Vec<RuntimeCallableInvokerCacheEntry>,
    runtime_builtin_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    runtime_extern_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    label_counter: usize,
    /// Memoized "does this module share the Mixed string-context ladder", indexed by mode.
    ///
    /// The predicate counts sites across every body in the module and is consulted at EVERY
    /// string context, so computing it per site is quadratic in module size. That is not
    /// theoretical here: an `eval()` program emits close to a million lines of assembly, and
    /// its sites would each rescan the whole instruction stream.
    mixed_string_sharing: [Option<bool>; 2],
    /// Memoized "does this module share the `count()` countable guard", for the same reason.
    count_guard_sharing: Option<bool>,
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
    strict_php: bool,
    label: String,
}

impl SharedCodegenState {
    /// Reserves the next module-unique assembly label id.
    ///
    /// Every generated local label ends in `_<id>` taken from this counter. Because the id is a
    /// decimal run terminated by the preceding `_`, it is recoverable from the finished label,
    /// which makes the whole label unique no matter how ambiguous its readable prefix is.
    pub(super) fn next_label_id(&mut self) -> usize {
        let id = self.label_counter;
        self.label_counter += 1;
        id
    }

    /// Returns the memoized string-context sharing decision for one mode, if already computed.
    pub(super) fn mixed_string_sharing(&self, mode_index: usize) -> Option<bool> {
        self.mixed_string_sharing[mode_index]
    }

    /// Records the string-context sharing decision so later sites reuse it.
    pub(super) fn set_mixed_string_sharing(&mut self, mode_index: usize, shares: bool) {
        self.mixed_string_sharing[mode_index] = Some(shares);
    }

    /// Returns the memoized `count()` guard sharing decision, if already computed.
    pub(super) fn count_guard_sharing(&self) -> Option<bool> {
        self.count_guard_sharing
    }

    /// Records the `count()` guard sharing decision so later sites reuse it.
    pub(super) fn set_count_guard_sharing(&mut self, shares: bool) {
        self.count_guard_sharing = Some(shares);
    }

    /// Returns cached runtime string-callable cases for the requested specialization.
    pub(super) fn runtime_string_descriptor_cases(
        &self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
        strict_php: bool,
    ) -> Option<Vec<RuntimeCallableCase>> {
        self.runtime_string_descriptor_cases
            .iter()
            .find(|(cached_ty, cached_names, cached_strict_php, _)| {
                cached_ty.as_ref() == source_arg_ty
                    && cached_names.as_deref() == candidate_names
                    && *cached_strict_php == strict_php
            })
            .map(|(_, _, _, cases)| cases.clone())
    }

    /// Stores runtime string-callable cases after their global wrappers are emitted.
    pub(super) fn cache_runtime_string_descriptor_cases(
        &mut self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
        strict_php: bool,
        cases: &[RuntimeCallableCase],
    ) {
        self.runtime_string_descriptor_cases.push((
            source_arg_ty.cloned(),
            candidate_names.map(|names| names.to_vec()),
            strict_php,
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
        strict_php: bool,
    ) -> Option<String> {
        cached_runtime_call_wrapper(
            &self.runtime_builtin_wrappers,
            name,
            signature,
            strict_php,
        )
    }

    /// Records a synthetic builtin wrapper for module-wide reuse.
    pub(super) fn cache_runtime_builtin_wrapper(
        &mut self,
        name: &str,
        signature: &FunctionSig,
        strict_php: bool,
        label: &str,
    ) {
        cache_runtime_call_wrapper(
            &mut self.runtime_builtin_wrappers,
            name,
            signature,
            strict_php,
            label,
        );
    }

    /// Returns a previously emitted synthetic extern wrapper for the same signature.
    pub(super) fn runtime_extern_wrapper(
        &self,
        name: &str,
        signature: &FunctionSig,
    ) -> Option<String> {
        cached_runtime_call_wrapper(&self.runtime_extern_wrappers, name, signature, false)
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
            false,
            label,
        );
    }
}

/// Looks up a cached synthetic call wrapper by PHP name and ABI signature.
fn cached_runtime_call_wrapper(
    entries: &[RuntimeCallWrapperCacheEntry],
    name: &str,
    signature: &FunctionSig,
    strict_php: bool,
) -> Option<String> {
    entries
        .iter()
        .find(|entry| {
            entry.name == name
                && entry.signature == *signature
                && entry.strict_php == strict_php
        })
        .map(|entry| entry.label.clone())
}

/// Adds one synthetic call wrapper to its module-wide cache.
fn cache_runtime_call_wrapper(
    entries: &mut Vec<RuntimeCallWrapperCacheEntry>,
    name: &str,
    signature: &FunctionSig,
    strict_php: bool,
    label: &str,
) {
    entries.push(RuntimeCallWrapperCacheEntry {
        name: name.to_string(),
        signature: signature.clone(),
        strict_php,
        label: label.to_string(),
    });
}
