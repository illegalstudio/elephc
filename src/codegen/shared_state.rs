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
    /// `--counters`: every non-synthetic PHP function prologue increments a BSS slot.
    pub(super) counters: bool,
    /// `--instrument`: every non-synthetic PHP function calls
    /// `elephc_instr_enter(id)`/`_exit(id)` around its body for exact timing.
    pub(super) instrument: crate::codegen::Instrumentation,
    /// `--probe`: the embedded symbol table `(data label, entry count)` main's
    /// prologue hands to `elephc_probe_init`. `None` unless the probe is enabled.
    pub(super) probe_table: Option<(String, usize)>,
    /// Counted functions as `(display name, counter symbol)`, in emission order. Main's
    /// epilogue renders the exit dump from this list — main is emitted last, so the
    /// registry is complete by then.
    counter_registry: Vec<(String, String)>,
    /// Instrumented function names, in id order (id = index). Main emits the
    /// name table `elephc_instr_init` reads; main is emitted last, so it is
    /// complete by then.
    instr_registry: Vec<String>,
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

    /// Records one counted function for the exit dump.
    pub(super) fn register_counter(&mut self, display_name: String, symbol: String) {
        self.counter_registry.push((display_name, symbol));
    }

    /// Returns the counted functions in emission order.
    pub(super) fn counter_registry(&self) -> &[(String, String)] {
        &self.counter_registry
    }

    /// Registers one instrumented function and returns its stable id (its index
    /// in the name table `elephc_instr_init` receives).
    pub(super) fn register_instr(&mut self, display_name: String) -> usize {
        let id = self.instr_registry.len();
        self.instr_registry.push(display_name);
        id
    }

    /// Returns the instrumented function names in id order.
    pub(super) fn instr_registry(&self) -> &[String] {
        &self.instr_registry
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
