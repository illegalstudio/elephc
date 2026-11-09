//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician call native instance and
//! static methods known to the current module.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user class ids, method symbols,
//!   or return types, so this bridge is emitted into the user assembly.
//! - This method-call slice supports public methods plus protected/private
//!   methods when the active eval class scope satisfies PHP visibility.
//! - Magician binds eval method calls to the physical native signature. A generated
//!   argument collector therefore stays in the slot metadata, and the bridge enters the
//!   raw method symbol. Source adapters remain reserved for source/vtable boundaries.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Module, Ownership};
use crate::codegen_support::source_method_adapters::MethodKind;
use crate::names::{join_php_symbol, method_symbol, php_symbol_key, static_method_symbol};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

use super::eval_ref_arg_helpers::{
    EvalRefArgSlot, eval_abi_param_types_for_refs, eval_arg_temp_slot_size,
    eval_normalized_ref_params, eval_ref_arg_slots, eval_signature_ref_params_supported,
    emit_aarch64_write_back_ref_args, emit_acquire_mixed_ref_args, emit_x86_64_write_back_ref_args,
};
use super::eval_callable_helpers::{EVAL_ARG_BOXED_VALUE_AARCH64_OFFSET, EVAL_ARG_BOXED_VALUE_X86_64_OFFSET,
    EVAL_ARG_CONTEXT_AARCH64_OFFSET};
use super::eval_callable_helpers::EvalCallableDescriptorSupport;
use super::class_id_dispatch::{emit_class_id_switch, ClassIdSwitch};
use super::eval_argument_helpers::emit_borrowed_string_arg;
use super::eval_value_helpers::{emit_borrowed_eval_string_argument, native_method_returns_owned_value};

/// Method metadata needed by eval method-call bridge dispatch.
#[derive(Clone)]
struct EvalMethodSlot {
    class_id: u64,
    class_name: String,
    method: String,
    impl_class: String,
    visibility: Visibility,
    allowed_scopes: Vec<String>,
    params: Vec<PhpType>,
    ref_params: Vec<bool>,
    return_ty: PhpType,
    return_is_owned: bool,
    return_uses_marker: bool,
    /// The compiled method returns BY REFERENCE: its result register holds the address of the
    /// returned storage, which the bridge reads before boxing.
    return_by_ref: bool,
    is_hidden_shadow: bool,
    entry_symbol: String,
    runtime_helper: Option<&'static str>,
}

/// Static method metadata needed by eval static method-call bridge dispatch.
#[derive(Clone)]
struct EvalStaticMethodSlot {
    class_id: u64,
    class_name: String,
    method: String,
    impl_class: String,
    visibility: Visibility,
    allowed_scopes: Vec<String>,
    params: Vec<PhpType>,
    ref_params: Vec<bool>,
    return_ty: PhpType,
    return_is_owned: bool,
    return_uses_marker: bool,
    /// See `EvalMethodSlot::return_by_ref`.
    return_by_ref: bool,
    entry_symbol: String,
}

/// Origin of a method result crossing the eval bridge.
#[derive(Clone, Copy)]
enum EvalMethodResultSource {
    CompiledInstance,
    CompiledStatic,
    RuntimeHelper,
}

const BUILTIN_THROWABLE_METHOD_CLASSES: &[&str] = &[
    "ErrorException",
    "Error",
    "TypeError",
    "ArgumentCountError",
    "ValueError",
    "ArithmeticError",
    "DivisionByZeroError",
    "AssertionError",
    "UnhandledMatchError",
    "ClosedGeneratorException",
    "CompileError",
    "ParseError",
    "RequestParseBodyException",
    "Exception",
    "LogicException",
    "BadFunctionCallException",
    "BadMethodCallException",
    "DomainException",
    "InvalidArgumentException",
    "LengthException",
    "OutOfRangeException",
    "RuntimeException",
    "OutOfBoundsException",
    "OverflowException",
    "RangeException",
    "UnderflowException",
    "UnexpectedValueException",
    "ReflectionException",
    "JsonException",
    "FiberError",
];
const BUILTIN_THROWABLE_GET_MESSAGE_LABEL: &str = "__elephc_eval_builtin_throwable_getmessage";
const BUILTIN_THROWABLE_GET_CODE_LABEL: &str = "__elephc_eval_builtin_throwable_getcode";
/// `getFile()` and `getLine()` are NOT rows here: their result is the Throwable's EFFECTIVE
/// location (an `ErrorException` override, a synthesized origin), which `__rt_throwable_get_file`
/// and `__rt_throwable_get_line` resolve; see `emit_builtin_throwable_location_bodies`.
const BUILTIN_THROWABLE_SCALAR_GETTERS: &[(&str, &str, usize, bool)] = &[
    ("getmessage", BUILTIN_THROWABLE_GET_MESSAGE_LABEL, 8, true),
    ("getcode", BUILTIN_THROWABLE_GET_CODE_LABEL, 24, false),
];
/// Builtin `Throwable` methods the bridge answers WITHOUT reading a payload field.
///
/// Both are synthetic on the compiled side too: elephc keeps no call stack, so
/// `lower_inst::throwable_methods` answers an empty array and an empty string. The bridge has to
/// answer the SAME rather than refuse the call, because a builtin Throwable has no emitted method
/// body to dispatch to -- `nm` on a compiled Symfony shows `gettraceasstring` only for the one
/// userland class that declares it. An eval-declared subclass reading either accessor therefore
/// failed outright, and Symfony's `ErrorListener::logKernelException` reads both while flattening
/// an exception. That listener runs at priority 0, ahead of the `RouterListener` at -64 that
/// renders the 404 welcome page, so the failure hid the page the request was meant to produce.
const BUILTIN_THROWABLE_SYNTHETIC_GETTERS: &[(&str, &str)] = &[
    ("gettrace", "__elephc_eval_builtin_throwable_gettrace"),
    (
        "gettraceasstring",
        "__elephc_eval_builtin_throwable_gettraceasstring",
    ),
];
/// `Throwable::getPrevious()`, whose slot two producers write with DIFFERENT shapes.
///
/// It gets its own entry rather than a row in either table above because neither shape fits:
/// the value is not a scalar field to re-box, and it is not synthetic either. The compiled
/// `Exception::__construct` assigns `$this->previous`, a declared `?Throwable`, so the slot holds
/// a BOXED Mixed cell; the eval/native bridge instead materializes a raw object pointer there.
/// `lower_inst::throwable_methods::lower_throwable_get_previous` tells them apart by heap kind 5
/// and this bridge body mirrors it exactly -- reading every slot as raw answers non-null for an
/// exception that has no previous, which is what turns Symfony's
/// `do { ... } while ($prev = $prev->getPrevious());` into a walk off the end.
const BUILTIN_THROWABLE_GET_PREVIOUS: (&str, &str) = (
    "getprevious",
    "__elephc_eval_builtin_throwable_getprevious",
);
const METHOD_HELPER_BASE_FRAME_SIZE: usize = 80;
const METHOD_HELPER_HANDLER_OFFSET: usize = METHOD_HELPER_BASE_FRAME_SIZE;
const METHOD_HELPER_FRAME_SIZE: usize = METHOD_HELPER_BASE_FRAME_SIZE + TRY_HANDLER_SLOT_SIZE;
const STATIC_METHOD_HELPER_BASE_FRAME_SIZE: usize = 96;
const STATIC_METHOD_HELPER_HANDLER_OFFSET: usize = STATIC_METHOD_HELPER_BASE_FRAME_SIZE;
const STATIC_METHOD_HELPER_FRAME_SIZE: usize =
    STATIC_METHOD_HELPER_BASE_FRAME_SIZE + TRY_HANDLER_SLOT_SIZE;
const X86_64_METHOD_CONTEXT_FRAME_OFFSET: usize = 64;
const X86_64_STATIC_METHOD_CONTEXT_FRAME_OFFSET: usize = 72;

/// Emits eval method-call helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_method_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if !module.required_runtime_features.eval_bridge {
        return;
    }
    let slots = collect_eval_method_slots(module);
    let static_slots = collect_eval_static_method_slots(module);
    let builtin_throwable_class_ids = collect_builtin_throwable_method_class_ids(module);
    emit_method_call_helper(
        module,
        emitter,
        data,
        &slots,
        &builtin_throwable_class_ids,
        callable_support,
    );
    emit_static_method_call_helper(module, emitter, data, &static_slots, callable_support);
}

/// Collects bridge-supported instance methods backed by emitted EIR symbols.
fn collect_eval_method_slots(module: &Module) -> Vec<EvalMethodSlot> {
    let emitted_methods = super::eir_class_method_keys(module);
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_method_slots(module, class_name, class_info, &emitted_methods, &mut slots);
    }
    slots
}

/// Collects bridge-supported static methods backed by emitted EIR symbols.
fn collect_eval_static_method_slots(module: &Module) -> Vec<EvalStaticMethodSlot> {
    let emitted_methods = super::eir_class_method_keys(module);
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_static_method_slots(module, class_name, class_info, &emitted_methods, &mut slots);
    }
    slots
}

/// Collects compact builtin Throwable class ids that eval can inspect directly.
///
/// THE INHERITING SUBCLASSES BELONG HERE TOO, for the same reason they do in
/// `eval_constructor_helpers::collect_builtin_throwable_constructor_class_ids`: a builtin
/// Throwable's getters have no EIR body to build a slot from, so a class that INHERITS
/// them matches neither this dispatch (its own id is not a builtin id) nor the slot table,
/// and the bridge reports `unsupported MethodCall expression` where PHP returns the value.
///
/// SHADOWING IS NOT A RISK HERE, and that is what makes routing by class id sound: this
/// dispatch runs BEFORE the slot dispatch, so it would hide a subclass override — but the
/// only methods it answers are getMessage, getCode, getFile, getLine, getTrace,
/// getTraceAsString and getPrevious, and php-src declares every one of them `final` on
/// `Exception` and `Error`. No subclass can override them, so the builtin body is always the
/// right one. Adding a non-final method to any of those tables would break this argument and
/// must be reconsidered here -- `__toString()`, notably, is NOT final and does not belong.
///
/// Measured against `php -n` 8.5.10: with the constructor half fixed but this one not, the
/// same `AotEx extends InvalidArgumentException` probe constructed correctly and then died
/// on `$e->getMessage()` with `unsupported MethodCall expression`, where PHP printed the
/// message. It is also the mechanism recorded as deferred for `getPrevious()`.
fn collect_builtin_throwable_method_class_ids(module: &Module) -> Vec<u64> {
    let mut class_ids = BUILTIN_THROWABLE_METHOD_CLASSES
        .iter()
        .filter_map(|class_name| module.class_infos.get(*class_name))
        .map(|class_info| class_info.class_id)
        .collect::<Vec<_>>();
    for class_info in module.class_infos.values() {
        let inherits_builtin_getter = BUILTIN_THROWABLE_SCALAR_GETTERS
            .iter()
            .map(|(name, ..)| *name)
            .chain(BUILTIN_THROWABLE_SYNTHETIC_GETTERS.iter().map(|(name, _)| *name))
            .chain(std::iter::once(BUILTIN_THROWABLE_GET_PREVIOUS.0))
            .any(|name| {
                class_info
                    .method_impl_classes
                    .get(&php_symbol_key(name))
                    .is_some_and(|impl_class| {
                        BUILTIN_THROWABLE_METHOD_CLASSES
                            .iter()
                            .any(|builtin| *builtin == impl_class.trim_start_matches('\\'))
                    })
            });
        if inherits_builtin_getter {
            class_ids.push(class_info.class_id);
        }
    }
    class_ids.sort_unstable();
    class_ids.dedup();
    class_ids
}

/// How the eval bridge reaches one method implementation.
enum EvalMethodEntry {
    /// Magician binds this physical signature before entering the raw method symbol.
    Raw(crate::types::FunctionSig, String),
}

/// Returns the physical entry symbol one method kind publishes without any adaptation.
fn raw_method_entry_symbol(impl_class: &str, method: &str, kind: MethodKind) -> String {
    match kind {
        MethodKind::Instance => method_symbol(impl_class, method),
        MethodKind::Static => static_method_symbol(impl_class, method),
    }
}

/// Resolves the physical signature and raw entry symbol for one method implementation.
///
/// Eval method registration and Magician's native argument binder both retain the physical
/// parameter count. Generated collectors and hidden actual-count slots therefore reach the raw
/// implementation with their physical signature instead of entering a source adapter. Magician's
/// binder materializes both internal shapes before entering this bridge. The physical signature
/// is read from the implementing class so inherited instance and static methods both resolve to
/// the symbol that owns their body.
fn eval_method_entry(
    module: &Module,
    impl_class: &str,
    method: &str,
    declared: &crate::types::FunctionSig,
    kind: MethodKind,
) -> EvalMethodEntry {
    let physical = module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| match kind {
            MethodKind::Instance => class_info.methods.get(method),
            MethodKind::Static => class_info.static_methods.get(method),
        })
        .unwrap_or(declared);
    EvalMethodEntry::Raw(
        physical.clone(),
        raw_method_entry_symbol(impl_class, method, kind),
    )
}

/// Adds bridge-supported instance methods for one class.
fn collect_class_method_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &crate::fast_hash::FastSet<(String, String, bool)>,
    slots: &mut Vec<EvalMethodSlot>,
) {
    collect_hidden_private_ancestor_method_slots(
        module,
        class_name,
        class_info,
        emitted_methods,
        slots,
    );
    let mut methods = class_info.methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method, sig) in methods {
        let visibility = method_visibility(class_info, method);
        if !method_visibility_supported(visibility) {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name);
        let runtime_intrinsic = eval_runtime_backed_instance_method_intrinsic(class_name, method);
        let runtime_helper = runtime_intrinsic.and_then(IntrinsicCall::runtime_helper);
        let entry = match runtime_helper {
            Some(_) => EvalMethodEntry::Raw(sig.clone(), method_symbol(impl_class, method)),
            None => eval_method_entry(module, impl_class, method, sig, MethodKind::Instance),
        };
        let (sig, entry_symbol) = match &entry {
            EvalMethodEntry::Raw(physical, symbol) => (physical, symbol),
        };
        if !method_signature_supported(sig) || !method_return_supported(&sig.return_type) {
            continue;
        }
        if runtime_helper.is_none()
            && !emitted_methods.contains(&(impl_class.to_string(), method.clone(), false))
        {
            continue;
        }
        slots.push(EvalMethodSlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            method: method.clone(),
            impl_class: impl_class.to_string(),
            visibility: visibility.clone(),
            allowed_scopes: visibility_scope_names(module, impl_class, visibility),
            params: sig.params.iter().map(|(_, ty)| super::eval_argument_helpers::bridge_storage_type(ty)).collect(),
            ref_params: eval_normalized_ref_params(sig.params.len(), &sig.ref_params),
            return_ty: emitted_method_return_type(module, entry_symbol, sig),
            return_is_owned: runtime_helper.is_none()
                && native_method_returns_owned_value(module, impl_class, method, false),
            return_uses_marker: runtime_helper.is_none()
                && compiled_method_return_uses_marker(sig),
            return_by_ref: runtime_helper.is_none() && sig.by_ref_return,
            is_hidden_shadow: false,
            entry_symbol: entry_symbol.clone(),
            runtime_helper,
        });
    }
}

/// Adds private ancestor instance methods hidden by descendant class method lookup.
fn collect_hidden_private_ancestor_method_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &crate::fast_hash::FastSet<(String, String, bool)>,
    slots: &mut Vec<EvalMethodSlot>,
) {
    for (ancestor_name, ancestor_info) in class_ancestry(module, class_name) {
        if ancestor_name == class_name {
            continue;
        }
        let mut methods = ancestor_info.methods.iter().collect::<Vec<_>>();
        methods.sort_by_key(|(method, _)| method.as_str());
        for (method, sig) in methods {
            let visibility = method_visibility(ancestor_info, method);
            if visibility != &Visibility::Private {
                continue;
            }
            let impl_class = ancestor_info
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(ancestor_name);
            let entry = eval_method_entry(module, impl_class, method, sig, MethodKind::Instance);
            let (sig, entry_symbol) = match &entry {
                EvalMethodEntry::Raw(physical, symbol) => (physical, symbol),
            };
            if !method_signature_supported(sig) || !method_return_supported(&sig.return_type) {
                continue;
            }
            if !emitted_methods.contains(&(impl_class.to_string(), method.clone(), false)) {
                continue;
            }
            slots.push(EvalMethodSlot {
                class_id: class_info.class_id,
                class_name: class_name.to_string(),
                method: method.clone(),
                impl_class: impl_class.to_string(),
                visibility: visibility.clone(),
                allowed_scopes: visibility_scope_names(module, impl_class, visibility),
                params: sig.params.iter().map(|(_, ty)| super::eval_argument_helpers::bridge_storage_type(ty)).collect(),
                ref_params: eval_normalized_ref_params(sig.params.len(), &sig.ref_params),
                return_ty: emitted_method_return_type(module, entry_symbol, sig),
                return_is_owned: native_method_returns_owned_value(module, impl_class, method, false),
                return_uses_marker: compiled_method_return_uses_marker(sig),
                return_by_ref: sig.by_ref_return,
                is_hidden_shadow: true,
                entry_symbol: entry_symbol.clone(),
                runtime_helper: None,
            });
        }
    }
}

/// Returns a normal-ABI runtime helper for builtin instance methods that eval can bridge.
fn eval_runtime_backed_instance_method_intrinsic(
    class_name: &str,
    method_name: &str,
) -> Option<IntrinsicCall> {
    // `Generator` belongs here too: interpreted code iterating a COMPILED generator --
    // `foreach ($this->warmers as $w)` over a container `RewindableGenerator` whose
    // `getIterator()` runs a compiled closure -- drives it by method name, and without a slot
    // `rewind()` failed as an unknown native method ("unsupported Foreach statement").
    if !matches!(
        class_name.trim_start_matches('\\'),
        "SplDoublyLinkedList" | "SplStack" | "SplQueue" | "SplFixedArray" | "Generator"
    ) {
        return None;
    }
    let intrinsic = IntrinsicCall::instance_method(class_name, method_name)?;
    intrinsic.runtime_helper()?;
    Some(intrinsic)
}

/// Adds bridge-supported static methods for one class.
fn collect_class_static_method_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &crate::fast_hash::FastSet<(String, String, bool)>,
    slots: &mut Vec<EvalStaticMethodSlot>,
) {
    let mut methods = class_info.static_methods.iter().collect::<Vec<_>>();
    methods.sort_by_key(|(method, _)| method.as_str());
    for (method, sig) in methods {
        let visibility = static_method_visibility(class_info, method);
        if !method_visibility_supported(visibility) {
            continue;
        }
        let impl_class = class_info
            .static_method_impl_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name);
        let entry = eval_method_entry(module, impl_class, method, sig, MethodKind::Static);
        let (sig, entry_symbol) = match &entry {
            EvalMethodEntry::Raw(physical, symbol) => (physical, symbol),
        };
        if !method_signature_supported(sig) || !method_return_supported(&sig.return_type) {
            continue;
        }
        if !emitted_methods.contains(&(impl_class.to_string(), method.clone(), true)) {
            continue;
        }
        slots.push(EvalStaticMethodSlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            method: method.clone(),
            impl_class: impl_class.to_string(),
            visibility: visibility.clone(),
            allowed_scopes: visibility_scope_names(module, impl_class, visibility),
            params: sig.params.iter().map(|(_, ty)| super::eval_argument_helpers::bridge_storage_type(ty)).collect(),
            ref_params: eval_normalized_ref_params(sig.params.len(), &sig.ref_params),
            return_ty: emitted_method_return_type(module, entry_symbol, sig),
            return_is_owned: native_method_returns_owned_value(module, impl_class, method, true),
            return_uses_marker: compiled_method_return_uses_marker(sig),
            return_by_ref: sig.by_ref_return,
            entry_symbol: entry_symbol.clone(),
        });
    }
}

/// Returns the actual ABI return representation of an emitted method symbol.
///
/// Class metadata may retain a checker-facing interface or union contract while
/// the emitted EIR function returns a concrete object or scalar representation.
/// The eval bridge must box the latter, because it receives the raw ABI result.
/// `entry_symbol` is the physical entry the bridge calls (an adapter when the source method
/// needed one), so the lookup follows the function that is actually invoked.
fn emitted_method_return_type(
    module: &Module,
    entry_symbol: &str,
    signature: &crate::types::FunctionSig,
) -> PhpType {
    module
        .class_methods
        .iter()
        .find(|function| function.name == entry_symbol)
        .map(|function| crate::types::mixed_box_php_type(&function.return_php_type))
        .unwrap_or_else(|| crate::types::mixed_box_php_type(&signature.return_type))
}

/// Returns whether a compiled by-value method publishes the private ownership marker.
fn compiled_method_return_uses_marker(sig: &crate::types::FunctionSig) -> bool {
    let return_ty = sig.return_type.codegen_repr();
    !sig.by_ref_return && Ownership::php_type_needs_lifetime_tracking(&return_ty)
}

/// Returns the declared instance-method visibility, defaulting to public metadata.
fn method_visibility<'a>(class_info: &'a ClassInfo, method: &str) -> &'a Visibility {
    class_info
        .method_visibilities
        .get(method)
        .unwrap_or(&Visibility::Public)
}

/// Returns the declared static-method visibility, defaulting to public metadata.
fn static_method_visibility<'a>(class_info: &'a ClassInfo, method: &str) -> &'a Visibility {
    class_info
        .static_method_visibilities
        .get(method)
        .unwrap_or(&Visibility::Public)
}

/// Returns class metadata from root parent to the requested class.
fn class_ancestry<'a>(module: &'a Module, class_name: &'a str) -> Vec<(&'a str, &'a ClassInfo)> {
    let mut chain = Vec::new();
    collect_class_ancestry(module, class_name, &mut chain);
    chain
}

/// Recursively collects class metadata from parent to child.
fn collect_class_ancestry<'a>(
    module: &'a Module,
    class_name: &'a str,
    chain: &mut Vec<(&'a str, &'a ClassInfo)>,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    if let Some(parent) = class_info.parent.as_deref() {
        collect_class_ancestry(module, parent, chain);
    }
    chain.push((class_name, class_info));
}

/// Returns true when the eval method bridge can enforce this visibility.
fn method_visibility_supported(visibility: &Visibility) -> bool {
    matches!(
        visibility,
        Visibility::Public | Visibility::Protected | Visibility::Private
    )
}

/// Returns true for method signatures supported by the eval bridge.
fn method_signature_supported(sig: &crate::types::FunctionSig) -> bool {
    eval_signature_ref_params_supported(sig)
        && sig.params.iter().all(|(_, ty)| method_param_supported(ty))
}

/// Returns true for an eval-supplied method argument type supported by this bridge.
///
/// A `?ptr<T>` argument is cast like an int: PHP null becomes address 0.
fn method_param_supported(ty: &PhpType) -> bool {
    crate::types::is_nullable_pointer_type(ty) || matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true for return storage shapes the bridge can box for eval.
///
/// A `?ptr<T>` return is boxed by its declared union, address 0 as PHP null.
fn method_return_supported(ty: &PhpType) -> bool {
    crate::types::is_nullable_pointer_type(ty) || matches!(
        ty.codegen_repr(),
        PhpType::Void
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Emits `__elephc_eval_value_method_call(Mixed*, name, len, MixedArray*, scope, scope_len, ctx) -> Mixed*`.
fn emit_method_call_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user method call ---");
    label_c_global(module, emitter, "__elephc_eval_value_method_call");
    match module.target.arch {
        Arch::AArch64 => {
            emit_method_call_aarch64(
                module,
                emitter,
                data,
                slots,
                builtin_throwable_class_ids,
                callable_support,
            )
        }
        Arch::X86_64 => {
            emit_method_call_x86_64(
                module,
                emitter,
                data,
                slots,
                builtin_throwable_class_ids,
                callable_support,
            )
        }
    }
}

/// Emits `__elephc_eval_value_static_method_call(class, method, MixedArray*, scope, scope_len, ctx) -> Mixed*`.
fn emit_static_method_call_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    callable_support: &EvalCallableDescriptorSupport,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user static method call ---");
    label_c_global(module, emitter, "__elephc_eval_value_static_method_call");
    match module.target.arch {
        Arch::AArch64 => {
            emit_static_method_call_aarch64(module, emitter, data, slots, callable_support)
        }
        Arch::X86_64 => {
            emit_static_method_call_x86_64(module, emitter, data, slots, callable_support)
        }
    }
}

/// Emits the ARM64 static method-call helper body.
fn emit_static_method_call_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let fail_label = "__elephc_eval_value_static_method_call_fail";
    let done_label = "__elephc_eval_value_static_method_call_done";
    emitter.instruction(                                                        // reserve helper frame plus a boundary exception handler
        &format!("sub sp, sp, #{}", STATIC_METHOD_HELPER_FRAME_SIZE)
    );
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested method-name pointer
    emitter.instruction("str x4, [sp, #24]");                                   // save the boxed eval argument array
    emitter.instruction("str x5, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the requested method-name length
    emitter.instruction("str x6, [sp, #48]");                                   // save the active eval class-scope length
    emitter.instruction("str x7, [sp, #80]");                                   // save the active eval context for callable descriptors
    emit_aarch64_static_method_dispatch(module, emitter, data, slots, fail_label);
    emitter.instruction(&format!("b {}", fail_label));                          // no supported static method matched the request
    emit_aarch64_static_method_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the Rust caller frame
    emitter.instruction(                                                        // release the helper frame and boundary handler
        &format!("add sp, sp, #{}", STATIC_METHOD_HELPER_FRAME_SIZE)
    );
    emitter.instruction("ret");                                                 // return the boxed static method result to Rust
}

/// Emits the x86_64 static method-call helper body.
fn emit_static_method_call_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let fail_label = "__elephc_eval_value_static_method_call_fail_x";
    let done_label = "__elephc_eval_value_static_method_call_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction(                                                        // reserve aligned slots plus a boundary exception handler
        &format!("sub rsp, {}", STATIC_METHOD_HELPER_FRAME_SIZE)
    );
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested method-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // save the boxed eval argument array
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the requested method-name length
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the active eval class-scope pointer
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load the active eval class-scope length stack argument
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the active eval class-scope length
    emitter.instruction("mov rax, QWORD PTR [rbp + 24]");                       // load the active eval context stack argument
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the active eval context for callable descriptors
    emit_x86_64_static_method_dispatch(module, emitter, data, slots, fail_label);
    emitter.instruction(&format!("jmp {}", fail_label));                        // no supported static method matched the request
    emit_x86_64_static_method_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed static method result to Rust
}

/// Branches on AArch64 zero through an unconditional branch with the wider displacement range.
fn emit_aarch64_branch_if_zero_far(emitter: &mut Emitter, reg: &str, label: &str) {
    emitter.instruction(&format!("cbnz {}, 1f", reg));                          // skip the far failure branch when the tested value is nonzero
    emitter.instruction(&format!("b {}", label));                               // reach the shared failure tail through the wider branch encoding
    emitter.label("1");
}

/// Branches on AArch64 inequality through an unconditional branch with wider reach.
fn emit_aarch64_branch_if_not_equal_far(emitter: &mut Emitter, label: &str) {
    emitter.instruction("b.eq 1f");                                             // skip the far failure branch when the compared values are equal
    emitter.instruction(&format!("b {}", label));                               // reach the shared failure tail through the wider branch encoding
    emitter.label("1");
}

/// Emits the ARM64 method-call helper body.
fn emit_method_call_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let fail_label = "__elephc_eval_value_method_call_fail";
    let done_label = "__elephc_eval_value_method_call_done";
    emitter.instruction(&format!("sub sp, sp, #{}", METHOD_HELPER_FRAME_SIZE)); // reserve helper frame plus a boundary exception handler
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested method-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested method-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed eval argument array
    emitter.instruction("str x4, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #40]");                                   // save the active eval class-scope length
    emitter.instruction("str x6, [sp, #64]");                                   // save the active eval context for callable descriptors
    emit_aarch64_branch_if_zero_far(emitter, "x0", fail_label);
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for method calls
    emit_aarch64_builtin_throwable_method_dispatch(
        module,
        emitter,
        data,
        builtin_throwable_class_ids,
    );
    emit_aarch64_method_dispatch(module, emitter, data, slots, fail_label);
    emitter.instruction(&format!("b {}", fail_label));                          // no supported method matched the request
    emit_aarch64_builtin_throwable_method_bodies(module, emitter, data, done_label, fail_label);
    emit_aarch64_method_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction(&format!("add sp, sp, #{}", METHOD_HELPER_FRAME_SIZE)); // release the helper frame and boundary handler
    emitter.instruction("ret");                                                 // return the boxed method result to Rust
}

/// Emits the x86_64 method-call helper body.
fn emit_method_call_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    builtin_throwable_class_ids: &[u64],
    callable_support: &EvalCallableDescriptorSupport,
) {
    let fail_label = "__elephc_eval_value_method_call_fail_x";
    let done_label = "__elephc_eval_value_method_call_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction(&format!("sub rsp, {}", METHOD_HELPER_FRAME_SIZE));     // reserve aligned slots plus a boundary exception handler
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested method-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested method-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed eval argument array
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the active eval class-scope length
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load the active eval context stack argument
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the active eval context for callable descriptors
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label));                         // null Mixed receiver cannot dispatch a method
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // non-object receivers cannot dispatch instance methods
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for method calls
    emit_x86_64_builtin_throwable_method_dispatch(
        module,
        emitter,
        data,
        builtin_throwable_class_ids,
    );
    emit_x86_64_method_dispatch(module, emitter, data, slots, fail_label);
    emitter.instruction(&format!("jmp {}", fail_label));                        // no supported method matched the request
    emit_x86_64_builtin_throwable_method_bodies(module, emitter, data, done_label, fail_label);
    emit_x86_64_method_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed method result to Rust
}

/// Emits an ARM64 boundary handler so native method throws return to magician.
fn emit_aarch64_method_exception_boundary_push(
    emitter: &mut Emitter,
    handler_offset: usize,
    escape_label: &str,
) {
    emitter.comment("push eval method exception boundary");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("str x10, [x29, #{}]", handler_offset));       // save the previous native exception-handler head
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("str x10, [x29, #{}]", handler_offset + 8));   // preserve the caller activation frame across method unwinding
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(                                               // save diagnostic suppression depth for restoration
        "str x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    emitter.instruction(&format!("add x10, x29, #{}", handler_offset));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // pass the boundary jmp_buf to setjmp
        "add x0, x29, #{}",
        handler_offset + TRY_HANDLER_JMP_BUF_OFFSET
    ));
    emitter.save_exception_handler();                                           // snapshot the bridge stack before entering native methods
    emitter.instruction(&format!("cbnz x0, {}", escape_label));                 // non-zero setjmp result means a method Throwable escaped
}

/// Emits an ARM64 boundary pop after a native method call returns to magician.
fn emit_aarch64_method_exception_boundary_pop(emitter: &mut Emitter, handler_offset: usize) {
    emitter.comment("pop eval method exception boundary");
    emitter.instruction(&format!("ldr x10, [x29, #{}]", handler_offset));       // reload the previous native exception-handler head
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emit_native_handler_pop_trace(emitter, 1, "x10");
    emitter.instruction(&format!(
        "ldr x10, [x29, #{}]",
        handler_offset + TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
}

/// Emits an x86_64 boundary handler so native method throws return to magician.
fn emit_x86_64_method_exception_boundary_push(
    emitter: &mut Emitter,
    handler_base: usize,
    escape_label: &str,
) {
    emitter.comment("push eval method exception boundary");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(                                                        // save the previous native exception-handler head
        &format!("mov QWORD PTR [rbp - {}], r10", handler_base)
    );
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(                                                        // preserve the caller activation frame across method unwinding
        &format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)
    );
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(                                               // save diagnostic suppression depth for restoration
        "mov QWORD PTR [rbp - {}], r10",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    emitter.instruction(&format!("lea r10, [rbp - {}]", handler_base));         // compute the boundary handler record address
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // pass the boundary jmp_buf to setjmp
        "lea rdi, [rbp - {}]",
        handler_base - TRY_HANDLER_JMP_BUF_OFFSET
    ));
    emitter.save_exception_handler();                                           // snapshot the bridge stack before entering native methods
    emitter.instruction("test eax, eax");                                       // did control arrive through longjmp?
    emitter.instruction(&format!("jne {}", escape_label));                      // non-zero setjmp result means a method Throwable escaped
}

/// Emits an x86_64 boundary pop after a native method call returns to magician.
fn emit_x86_64_method_exception_boundary_pop(emitter: &mut Emitter, handler_base: usize) {
    emitter.comment("pop eval method exception boundary");
    emitter.instruction(                                                        // reload the previous native exception-handler head
        &format!("mov r10, QWORD PTR [rbp - {}]", handler_base)
    );
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emit_native_handler_pop_trace(emitter, 1, "r10");
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rbp - {}]",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
}

/// Emits an opt-in native boundary-pop trace while preserving the boxed result.
fn emit_native_handler_pop_trace(emitter: &mut Emitter, site: i64, top_reg: &str) {
    if std::env::var_os("ELEPHC_CODEGEN_HANDLER_TRACE").is_none() {
        return;
    }
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 3);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), site);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), top_reg);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 3), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 4), 0);
    let symbol = emitter
        .target
        .extern_symbol("__elephc_eval_trace_aot_handler_top");
    abi::emit_call_label(emitter, &symbol);
    abi::emit_pop_reg(emitter, result_reg);
}

/// Emits ARM64 class-id and method-name dispatch for helper method bodies.
fn emit_aarch64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    fail_label: &str,
) {
    let groups = grouped_slots(slots).into_iter().collect::<Vec<_>>();
    let ids = groups.iter().map(|(class_id, _)| *class_id).collect::<Vec<_>>();
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("ldr {}, [sp, #16]", reg));                // reload the unboxed object pointer before this class test
        emitter.instruction(&format!("ldr {0}, [{0}]", reg));                   // load the receiver class id for method dispatch
    };
    let switch = ClassIdSwitch { load_id: &load_id, stem: "__elephc_eval_method", suffix: "" };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, index| {
        for slot in &groups[index].1 {
            emit_aarch64_method_name_compare(module, emitter, data, slot, fail_label);
        }
    });
}

/// Emits x86_64 class-id and method-name dispatch for helper method bodies.
fn emit_x86_64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    fail_label: &str,
) {
    let groups = grouped_slots(slots).into_iter().collect::<Vec<_>>();
    let ids = groups.iter().map(|(class_id, _)| *class_id).collect::<Vec<_>>();
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("mov {}, QWORD PTR [rbp - 24]", reg));     // reload the unboxed object pointer before this class test
        emitter.instruction(&format!("mov {0}, QWORD PTR [{0}]", reg));         // load the receiver class id for method dispatch
    };
    let switch = ClassIdSwitch { load_id: &load_id, stem: "__elephc_eval_method", suffix: "_x" };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, index| {
        for slot in &groups[index].1 {
            emit_x86_64_method_name_compare(module, emitter, data, slot, fail_label);
        }
    });
}

/// Emits ARM64 class-name and method-name dispatch for static method helper bodies.
fn emit_aarch64_static_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    fail_label: &str,
) {
    for (class_name, class_slots) in grouped_static_slots(slots) {
        let next_label = join_php_symbol("__elephc_eval_static_method_next", &[class_name]);
        emit_aarch64_static_class_name_compare(emitter, data, class_name, &next_label);
        for slot in class_slots {
            emit_aarch64_static_method_name_compare(module, emitter, data, slot, fail_label);
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-name and method-name dispatch for static method helper bodies.
fn emit_x86_64_static_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    fail_label: &str,
) {
    for (class_name, class_slots) in grouped_static_slots(slots) {
        let next_label = format!(
            "{}_x",
            join_php_symbol("__elephc_eval_static_method_next", &[class_name])
        );
        emit_x86_64_static_class_name_compare(emitter, data, class_name, &next_label);
        for slot in class_slots {
            emit_x86_64_static_method_name_compare(module, emitter, data, slot, fail_label);
        }
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 case-insensitive class-name comparison for a static method group.
fn emit_aarch64_static_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested class-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction(&format!("cbnz x0, {}", next_label));                   // try the next class when names differ
}

/// Emits one x86_64 case-insensitive class-name comparison for a static method group.
fn emit_x86_64_static_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested class-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the class names matched
    emitter.instruction(&format!("jne {}", next_label));                        // try the next class when names differ
}

/// Emits ARM64 class-id and method-name dispatch for compact Throwable methods.
fn emit_aarch64_builtin_throwable_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_ids: &[u64],
) {
    let ids = super::eval_constructor_helpers::sorted_class_ids(class_ids);
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("ldr {}, [sp, #16]", reg));                // reload the unboxed object pointer before this builtin Throwable test
        emitter.instruction(&format!("ldr {0}, [{0}]", reg));                   // load the receiver class id for builtin Throwable dispatch
    };
    let switch = ClassIdSwitch {
        load_id: &load_id,
        stem: "__elephc_eval_builtin_throwable_method",
        suffix: "",
    };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, _| {
        for (method, label, _, _) in BUILTIN_THROWABLE_SCALAR_GETTERS {
            emit_aarch64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
        for (method, label) in BUILTIN_THROWABLE_SYNTHETIC_GETTERS {
            emit_aarch64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
        let (previous_method, previous_label) = BUILTIN_THROWABLE_GET_PREVIOUS;
        emit_aarch64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            previous_method,
            previous_label,
        );
        for (method, label) in [
            ("getfile", "__elephc_eval_throwable_file"),
            ("getline", "__elephc_eval_throwable_line"),
        ] {
            emit_aarch64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
    });
}

/// Emits x86_64 class-id and method-name dispatch for compact Throwable methods.
fn emit_x86_64_builtin_throwable_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_ids: &[u64],
) {
    let ids = super::eval_constructor_helpers::sorted_class_ids(class_ids);
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("mov {}, QWORD PTR [rbp - 24]", reg));     // reload the unboxed object pointer before this builtin Throwable test
        emitter.instruction(&format!("mov {0}, QWORD PTR [{0}]", reg));         // load the receiver class id for builtin Throwable dispatch
    };
    let switch = ClassIdSwitch {
        load_id: &load_id,
        stem: "__elephc_eval_builtin_throwable_method",
        suffix: "_x",
    };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, _| {
        for (method, label, _, _) in BUILTIN_THROWABLE_SCALAR_GETTERS {
            emit_x86_64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
        for (method, label) in BUILTIN_THROWABLE_SYNTHETIC_GETTERS {
            emit_x86_64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
        let (previous_method, previous_label) = BUILTIN_THROWABLE_GET_PREVIOUS;
        emit_x86_64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            previous_method,
            previous_label,
        );
        for (method, label) in [
            ("getfile", "__elephc_eval_throwable_file"),
            ("getline", "__elephc_eval_throwable_line"),
        ] {
            emit_x86_64_builtin_throwable_method_name_branch(module, emitter, data, method, label);
        }
    });
}

/// Emits one ARM64 method-name comparison for a compact Throwable method.
fn emit_aarch64_builtin_throwable_method_name_branch(
    _module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    method_key: &str,
    target_label: &str,
) {
    let (label, len) = data.add_string(method_key.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested method-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested method-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare Throwable method names with PHP case-insensitive rules
    emit_aarch64_branch_if_zero_far(emitter, "x0", target_label);               // dispatch to the compact Throwable method when names match
}

/// Emits one x86_64 method-name comparison for a compact Throwable method.
fn emit_x86_64_builtin_throwable_method_name_branch(
    _module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    method_key: &str,
    target_label: &str,
) {
    let (label, len) = data.add_string(method_key.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested method-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare Throwable method names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the method names matched
    emitter.instruction(&format!("je {}", target_label));                       // dispatch to the compact Throwable method when names match
}

/// Emits one ARM64 method-name comparison and branch to the matching body.
fn emit_aarch64_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested method-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested method-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare method names with PHP case-insensitive rules
    let target_label = method_body_label(module, slot);
    if matches!(slot.visibility, Visibility::Public) {
        emit_aarch64_branch_if_zero_far(emitter, "x0", &target_label);         // dispatch to the method body when the names match
        return;
    }
    let miss_label = method_access_miss_label(module, slot);
    emitter.instruction(&format!("cbnz x0, {}", miss_label));                   // continue method dispatch when names differ
    let scope_fail_label = if slot.is_hidden_shadow {
        miss_label.as_str()
    } else {
        fail_label
    };
    emit_aarch64_method_scope_check(
        emitter,
        data,
        &slot.allowed_scopes,
        false,
        &target_label,
        scope_fail_label,
    );
    emitter.label(&miss_label);
}

/// Emits one x86_64 method-name comparison and branch to the matching body.
fn emit_x86_64_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested method-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare method names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the method names matched
    let target_label = method_body_label(module, slot);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("je {}", target_label));                   // dispatch to the method body when the names match
        return;
    }
    let miss_label = method_access_miss_label(module, slot);
    emitter.instruction(&format!("jne {}", miss_label));                        // continue method dispatch when names differ
    let scope_fail_label = if slot.is_hidden_shadow {
        miss_label.as_str()
    } else {
        fail_label
    };
    emit_x86_64_method_scope_check(
        emitter,
        data,
        &slot.allowed_scopes,
        false,
        &target_label,
        scope_fail_label,
    );
    emitter.label(&miss_label);
}

/// Emits one ARM64 static method-name comparison and branch to the matching body.
fn emit_aarch64_static_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload requested static method-name pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload requested static method-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare static method names with PHP case-insensitive rules
    let target_label = static_method_body_label(module, slot);
    if matches!(slot.visibility, Visibility::Public) {
        emit_aarch64_branch_if_zero_far(emitter, "x0", &target_label);         // dispatch to the static method body when the names match
        return;
    }
    let miss_label = static_method_access_miss_label(module, slot);
    emitter.instruction(&format!("cbnz x0, {}", miss_label));                   // continue static method dispatch when names differ
    emit_aarch64_method_scope_check(emitter, data, &slot.allowed_scopes, true, &target_label, fail_label);
    emitter.label(&miss_label);
}

/// Emits one x86_64 static method-name comparison and branch to the matching body.
fn emit_x86_64_static_method_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.method.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload requested static method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload requested static method-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare static method names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the static method names matched
    let target_label = static_method_body_label(module, slot);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("je {}", target_label));                   // dispatch to the static method body when the names match
        return;
    }
    let miss_label = static_method_access_miss_label(module, slot);
    emitter.instruction(&format!("jne {}", miss_label));                        // continue static method dispatch when names differ
    emit_x86_64_method_scope_check(emitter, data, &slot.allowed_scopes, true, &target_label, fail_label);
    emitter.label(&miss_label);
}

/// Emits ARM64 visibility checks for a protected/private method bridge hit.
fn emit_aarch64_method_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    allowed_scopes: &[String],
    is_static: bool,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = aarch64_method_scope_offsets(is_static);
    emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset));       // reload the active eval class-scope pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset));       // reload the active eval class-scope length
    emit_aarch64_branch_if_zero_far(emitter, "x1", fail_label);
    // The scope list is a data table walked by the shared `__rt_eval_scope_allowed`; unrolled
    // inline it cost one `strcasecmp` per related class at every arm, three million arms over
    // Symfony's compiled container.
    let table = eval_scope_table_label(data, allowed_scopes);
    abi::emit_symbol_address(emitter, "x3", &table);
    emitter.instruction("bl __rt_eval_scope_allowed");                          // test the active scope against the member's allowed classes
    emit_aarch64_branch_if_zero_far(emitter, "x0", fail_label);
    emitter.instruction(&format!("b {}", success_label));                       // dispatch when scoped visibility is satisfied
}

/// Builds the deduplicated `[count, ptr, len, ...]` table of class names that may see a member,
/// the operand of the shared `__rt_eval_scope_allowed` loop.
pub(super) fn eval_scope_table_label(data: &mut DataSection, allowed_scopes: &[String]) -> String {
    use crate::codegen_support::data_section::DataWord;
    let mut words = Vec::with_capacity(1 + allowed_scopes.len() * 2);
    words.push(DataWord::U64(allowed_scopes.len() as u64));
    for scope_name in allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        words.push(DataWord::Symbol(label));
        words.push(DataWord::U64(len as u64));
    }
    data.add_words(words)
}

/// Emits x86_64 visibility checks for a protected/private method bridge hit.
fn emit_x86_64_method_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    allowed_scopes: &[String],
    is_static: bool,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = x86_64_method_scope_offsets(is_static);
    emitter.instruction(                                                        // reload the active eval class-scope pointer
        &format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)
    );
    emitter.instruction(                                                        // reload the active eval class-scope length
        &format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)
    );
    emitter.instruction("test rdi, rdi");                                       // check whether eval is executing inside a class scope
    emitter.instruction(&format!("jz {}", fail_label));                         // reject scoped method access outside a class scope
    let table = eval_scope_table_label(data, allowed_scopes);
    abi::emit_symbol_address(emitter, "rdx", &table);
    emitter.instruction("call __rt_eval_scope_allowed");                        // test the active scope against the member's allowed classes
    emitter.instruction("test rax, rax");                                       // did any allowed class match?
    emitter.instruction(&format!("jz {}", fail_label));                         // reject scoped method access from unrelated classes
    emitter.instruction(&format!("jmp {}", success_label));                     // dispatch when scoped visibility is satisfied
}

/// Returns ARM64 stack offsets for method class-scope pointer and length.
fn aarch64_method_scope_offsets(is_static: bool) -> (usize, usize) {
    if is_static {
        (32, 48)
    } else {
        (32, 40)
    }
}

/// Returns x86_64 frame offsets for method class-scope pointer and length.
fn x86_64_method_scope_offsets(is_static: bool) -> (usize, usize) {
    if is_static {
        (56, 64)
    } else {
        (48, 56)
    }
}

/// Emits ARM64 bodies for compact Throwable methods used by eval.
fn emit_aarch64_builtin_throwable_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    done_label: &str,
    fail_label: &str,
) {
    for (_, label, offset, string_field) in BUILTIN_THROWABLE_SCALAR_GETTERS {
        emitter.label(label);
        emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the Throwable receiver before reading its declared property
        abi::emit_load_from_address(emitter, "x1", "x9", *offset);
        if *string_field {
            abi::emit_load_from_address(emitter, "x2", "x9", offset + 8);
        } else {
            abi::emit_load_int_immediate(emitter, "x2", 0);
        }
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::from(*string_field));
        abi::emit_call_label(emitter, "__rt_mixed_from_value");
        abi::emit_jump(emitter, done_label);
    }
    for (method, label) in BUILTIN_THROWABLE_SYNTHETIC_GETTERS {
        emitter.label(label);
        emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        if *method == "gettrace" {
            // A trace recorded at creation lives in the declared `trace` slot and is boxed
            // borrowed (the box takes its own reference). Otherwise the same empty indexed
            // array `lower_throwable_empty_trace_array` builds: runtime tag 4, capacity 8,
            // value type Mixed.
            let stored_label = format!("{label}_stored");
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the Throwable receiver before reading its trace slot
            abi::emit_load_from_address(
                emitter,
                "x1",
                "x9",
                crate::codegen_support::throwable_layout::TRACE_OFFSET,
            );
            emitter.instruction(&format!("cbnz x1, {stored_label}"));           // a recorded trace is returned as it is
            abi::emit_load_int_immediate(emitter, "x0", 4);
            abi::emit_load_int_immediate(emitter, "x1", 8);
            abi::emit_call_label(emitter, "__rt_array_new");
            crate::codegen::emit_array_value_type_stamp(
                emitter,
                abi::int_result_reg(emitter),
                &PhpType::Mixed,
            );
            emitter.instruction("mov x1, x0");                                  // move the fresh array into the Mixed payload word
            emitter.label(&stored_label);
            abi::emit_load_int_immediate(emitter, "x0", 4);
            abi::emit_load_int_immediate(emitter, "x2", 0);
        } else {
            let (string_label, string_len) = data.add_string(b"");
            abi::emit_symbol_address(emitter, "x1", &string_label);
            abi::emit_load_int_immediate(emitter, "x2", string_len as i64);
            abi::emit_load_int_immediate(emitter, "x0", 1);
        }
        abi::emit_call_label(emitter, "__rt_mixed_from_value");
        abi::emit_jump(emitter, done_label);
    }
    emit_aarch64_builtin_throwable_get_previous_body(module, emitter, done_label, fail_label);
    emit_builtin_throwable_location_bodies(module, emitter, done_label, fail_label);
}

/// Emits the ARM64 `Throwable::getPrevious()` bridge body.
///
/// Mirrors `lower_inst::throwable_methods::lower_throwable_get_previous`: an empty slot answers
/// null, heap kind 5 means the compiled constructor already boxed the previous and the cell is
/// handed back retained, and anything else is the raw object pointer the eval bridge stores.
fn emit_aarch64_builtin_throwable_get_previous_body(
    module: &Module,
    emitter: &mut Emitter,
    done_label: &str,
    fail_label: &str,
) {
    let (_, label) = BUILTIN_THROWABLE_GET_PREVIOUS;
    let boxed_label = format!("{}_boxed", label);
    let null_label = format!("{}_null", label);
    emitter.label(label);
    emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the Throwable receiver before reading its previous slot
    abi::emit_load_from_address(
        emitter,
        "x0",
        "x9",
        crate::codegen_support::throwable_layout::PREVIOUS_OFFSET,
    );
    emitter.instruction(&format!("cbz x0, {}", null_label));                    // an empty slot is a missing previous
    emitter.instruction("str x0, [sp, #-16]!");                                 // preserve the slot across the kind probe
    abi::emit_call_label(emitter, "__rt_heap_kind");
    emitter.instruction("mov x11, x0");                                         // heap kind of the stored value
    emitter.instruction("ldr x0, [sp], #16");                                   // restore the stored value
    emitter.instruction("cmp x11, #5");                                         // kind 5 = boxed Mixed cell
    emitter.instruction(&format!("b.eq {}", boxed_label));
    emitter.instruction("mov x1, x0");                                          // raw object pointer becomes the Mixed payload word
    abi::emit_load_int_immediate(emitter, "x2", 0);
    abi::emit_load_int_immediate(emitter, "x0", 6);                             // runtime tag 6 = object; the helper retains the payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_jump(emitter, done_label);

    emitter.label(&boxed_label);
    abi::emit_call_label(emitter, "__rt_incref");                               // boxed cell: retain and hand the same cell back
    abi::emit_jump(emitter, done_label);

    emitter.label(&null_label);
    abi::emit_load_int_immediate(emitter, "x1", 0);
    abi::emit_load_int_immediate(emitter, "x2", 0);
    abi::emit_load_int_immediate(emitter, "x0", 8);                             // runtime tag 8 = null
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_jump(emitter, done_label);
}

/// Emits x86_64 bodies for compact Throwable methods used by eval.
fn emit_x86_64_builtin_throwable_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    done_label: &str,
    fail_label: &str,
) {
    for (_, label, offset, string_field) in BUILTIN_THROWABLE_SCALAR_GETTERS {
        emitter.label(label);
        emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                   // reload the Throwable receiver before reading its declared property
        abi::emit_load_from_address(emitter, "rdi", "r10", *offset);
        if *string_field {
            abi::emit_load_from_address(emitter, "rsi", "r10", offset + 8);
        } else {
            abi::emit_load_int_immediate(emitter, "rsi", 0);
        }
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), i64::from(*string_field));
        abi::emit_call_label(emitter, "__rt_mixed_from_value");
        abi::emit_jump(emitter, done_label);
    }
    for (method, label) in BUILTIN_THROWABLE_SYNTHETIC_GETTERS {
        emitter.label(label);
        emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        if *method == "gettrace" {
            // See the ARM64 body: a recorded trace, or the empty array the intrinsic builds.
            let stored_label = format!("{label}_stored");
            emitter.instruction("mov r10, QWORD PTR [rbp - 24]");               // reload the Throwable receiver before reading its trace slot
            abi::emit_load_from_address(
                emitter,
                "rdi",
                "r10",
                crate::codegen_support::throwable_layout::TRACE_OFFSET,
            );
            emitter.instruction("test rdi, rdi");                               // was a trace recorded at creation?
            emitter.instruction(&format!("jnz {stored_label}"));                // a recorded trace is returned as it is
            abi::emit_load_int_immediate(emitter, "rdi", 4);
            abi::emit_load_int_immediate(emitter, "rsi", 8);
            abi::emit_call_label(emitter, "__rt_array_new");
            crate::codegen::emit_array_value_type_stamp(
                emitter,
                abi::int_result_reg(emitter),
                &PhpType::Mixed,
            );
            emitter.instruction("mov rdi, rax");                                // move the fresh array into the Mixed payload word
            emitter.label(&stored_label);
            abi::emit_load_int_immediate(emitter, "rsi", 0);
            abi::emit_load_int_immediate(emitter, "rax", 4);
        } else {
            let (string_label, string_len) = data.add_string(b"");
            abi::emit_symbol_address(emitter, "rdi", &string_label);
            abi::emit_load_int_immediate(emitter, "rsi", string_len as i64);
            abi::emit_load_int_immediate(emitter, "rax", 1);
        }
        abi::emit_call_label(emitter, "__rt_mixed_from_value");
        abi::emit_jump(emitter, done_label);
    }
    emit_x86_64_builtin_throwable_get_previous_body(module, emitter, done_label, fail_label);
    emit_builtin_throwable_location_bodies(module, emitter, done_label, fail_label);
}

/// Materializes runtime-aware Throwable location results for opaque eval method calls.
fn emit_builtin_throwable_location_bodies(
    module: &Module,
    emitter: &mut Emitter,
    done_label: &str,
    fail_label: &str,
) {
    for file in [true, false] {
        emitter.label(if file { "__elephc_eval_throwable_file" } else { "__elephc_eval_throwable_line" });
        match emitter.target.arch {
            Arch::AArch64 => {
                emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
                emitter.instruction("ldr x0, [sp, #16]");                       // reload the Throwable receiver for location access
            }
            Arch::X86_64 => {
                emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
                emitter.instruction("mov rax, QWORD PTR [rbp - 24]");           // reload the Throwable receiver for location access
            }
        }
        abi::emit_call_label(emitter, if file { "__rt_throwable_get_file" } else { "__rt_throwable_get_line" });
        if file {
            crate::codegen::emit_box_current_owned_value_as_mixed(emitter, &PhpType::Str);
        } else {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Int);
        }
        abi::emit_jump(emitter, done_label);
    }
}

/// Emits the x86_64 `Throwable::getPrevious()` bridge body. See the ARM64 body for the shapes.
fn emit_x86_64_builtin_throwable_get_previous_body(
    module: &Module,
    emitter: &mut Emitter,
    done_label: &str,
    fail_label: &str,
) {
    let (_, label) = BUILTIN_THROWABLE_GET_PREVIOUS;
    let boxed_label = format!("{}_boxed_x", label);
    let null_label = format!("{}_null_x", label);
    emitter.label(label);
    emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the Throwable receiver before reading its previous slot
    abi::emit_load_from_address(
        emitter,
        "rax",
        "r10",
        crate::codegen_support::throwable_layout::PREVIOUS_OFFSET,
    );
    emitter.instruction("test rax, rax");                                       // an empty slot is a missing previous
    emitter.instruction(&format!("jz {}", null_label));
    emitter.instruction("push rax");                                            // preserve the slot across the kind probe
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned for the call
    abi::emit_call_label(emitter, "__rt_heap_kind");
    emitter.instruction("mov r11, rax");                                        // heap kind of the stored value
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rax");                                             // restore the stored value
    emitter.instruction("cmp r11, 5");                                          // kind 5 = boxed Mixed cell
    emitter.instruction(&format!("je {}", boxed_label));
    emitter.instruction("mov rdi, rax");                                        // raw object pointer becomes the Mixed payload word
    abi::emit_load_int_immediate(emitter, "rsi", 0);
    abi::emit_load_int_immediate(emitter, "rax", 6);                            // runtime tag 6 = object; the helper retains the payload
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_jump(emitter, done_label);

    emitter.label(&boxed_label);
    abi::emit_call_label(emitter, "__rt_incref");                               // boxed cell: retain and hand the same cell back
    abi::emit_jump(emitter, done_label);

    emitter.label(&null_label);
    abi::emit_load_int_immediate(emitter, "rdi", 0);
    abi::emit_load_int_immediate(emitter, "rsi", 0);
    abi::emit_load_int_immediate(emitter, "rax", 8);                            // runtime tag 8 = null
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::emit_jump(emitter, done_label);
}

/// Emits ARM64 zero-argument validation for compact Throwable eval methods.
fn emit_aarch64_validate_builtin_throwable_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for Throwable method arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    emitter.instruction("cmp x0, #0");                                          // compact Throwable methods accept no eval arguments
    emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
}

/// Emits x86_64 zero-argument validation for compact Throwable eval methods.
fn emit_x86_64_validate_builtin_throwable_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for Throwable method arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    emitter.instruction("test rax, rax");                                       // compact Throwable methods accept no eval arguments
    emitter.instruction(&format!("jne {}", fail_label));                        // reject unsupported Throwable method arguments from eval
}

/// Emits ARM64 method-call bodies for every bridge-supported method.
fn emit_aarch64_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        let body_label = method_body_label(module, slot);
        let prep_fail_label = format!("{}_prep_fail", body_label);
        emitter.label(&body_label);
        emit_aarch64_validate_method_arg_count(module, emitter, slot, fail_label);
        let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
        super::eval_argument_helpers::emit_reserve_callable_arg_owners(emitter, callable_owners.len());
        let (arg_temp_bytes, ref_slots) =
            emit_aarch64_prepare_method_args(
                module,
                emitter,
                data,
                slot,
                &prep_fail_label,
                callable_support,
            );
        emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
        let escape_label = format!("{}_escape", body_label);
        emit_aarch64_method_exception_boundary_push(
            emitter,
            METHOD_HELPER_HANDLER_OFFSET - 48,
            &escape_label,
        );
        if slot.runtime_helper.is_some() {
            let intrinsic = IntrinsicCall::instance_method(&slot.class_name, &slot.method).unwrap();
            super::eval_argument_helpers::emit_consumed_intrinsic_arguments(emitter, intrinsic, &slot.params);
        }
        let receiver_ty = PhpType::Object(slot.class_name.clone());
        let overflow_bytes =
            materialize_method_args(module, emitter, &receiver_ty, &slot.params, &slot.ref_params);
        let caller_stack_pad_bytes =
            abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
        abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
        let callee = slot.runtime_helper.unwrap_or(slot.entry_symbol.as_str());
        abi::emit_call_label(emitter, callee);
        abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        let result_source = if slot.runtime_helper.is_some() {
            EvalMethodResultSource::RuntimeHelper
        } else {
            EvalMethodResultSource::CompiledInstance
        };
        if slot.return_by_ref {
            emit_load_by_ref_method_result(emitter, &slot.return_ty);
        }
        emit_box_method_result(
            module,
            emitter,
            &slot.return_ty,
            slot.return_is_owned && !slot.return_by_ref,
            slot.return_uses_marker && !slot.return_by_ref,
            result_source,
            &body_label,
        );
        preserve_result_and_write_back_aarch64_ref_args(emitter, &ref_slots, &body_label);
        emit_aarch64_method_exception_boundary_pop(emitter, METHOD_HELPER_HANDLER_OFFSET - 48);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), true);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the native method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_aarch64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_aarch64_method_exception_boundary_pop(emitter, METHOD_HELPER_HANDLER_OFFSET - 48);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), false);
        emitter.instruction(&format!("b {}", fail_label));                      // return failure after preserving by-reference writes
        emitter.label(&prep_fail_label);
        emit_aarch64_method_prep_fail_cleanup(emitter, 48, fail_label);
    }
}

/// Emits x86_64 method-call bodies for every bridge-supported method.
fn emit_x86_64_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        let body_label = method_body_label(module, slot);
        let prep_fail_label = format!("{}_prep_fail_x", body_label);
        emitter.label(&body_label);
        emit_x86_64_validate_method_arg_count(module, emitter, slot, fail_label);
        let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
        super::eval_argument_helpers::emit_reserve_callable_arg_owners(emitter, callable_owners.len());
        let (arg_temp_bytes, ref_slots) =
            emit_x86_64_prepare_method_args(
                module,
                emitter,
                data,
                slot,
                &prep_fail_label,
                callable_support,
            );
        emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
        let escape_label = format!("{}_escape_x", body_label);
        emit_x86_64_method_exception_boundary_push(emitter, METHOD_HELPER_FRAME_SIZE, &escape_label);
        if slot.runtime_helper.is_some() {
            let intrinsic = IntrinsicCall::instance_method(&slot.class_name, &slot.method).unwrap();
            super::eval_argument_helpers::emit_consumed_intrinsic_arguments(emitter, intrinsic, &slot.params);
        }
        let receiver_ty = PhpType::Object(slot.class_name.clone());
        let overflow_bytes =
            materialize_method_args(module, emitter, &receiver_ty, &slot.params, &slot.ref_params);
        let caller_stack_pad_bytes =
            abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
        abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
        let callee = slot.runtime_helper.unwrap_or(slot.entry_symbol.as_str());
        abi::emit_call_label(emitter, callee);
        abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        let result_source = if slot.runtime_helper.is_some() {
            EvalMethodResultSource::RuntimeHelper
        } else {
            EvalMethodResultSource::CompiledInstance
        };
        if slot.return_by_ref {
            emit_load_by_ref_method_result(emitter, &slot.return_ty);
        }
        emit_box_method_result(
            module,
            emitter,
            &slot.return_ty,
            slot.return_is_owned && !slot.return_by_ref,
            slot.return_uses_marker && !slot.return_by_ref,
            result_source,
            &body_label,
        );
        preserve_result_and_write_back_x86_64_ref_args(emitter, &ref_slots, &body_label);
        emit_x86_64_method_exception_boundary_pop(emitter, METHOD_HELPER_FRAME_SIZE);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), true);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the native method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_x86_64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_x86_64_method_exception_boundary_pop(emitter, METHOD_HELPER_FRAME_SIZE);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), false);
        emitter.instruction(&format!("jmp {}", fail_label));                    // return failure after preserving by-reference writes
        emitter.label(&prep_fail_label);
        emit_x86_64_method_prep_fail_cleanup(emitter, fail_label);
    }
}

/// Emits ARM64 static method-call bodies for every bridge-supported static method.
fn emit_aarch64_static_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        let body_label = static_method_body_label(module, slot);
        let prep_fail_label = format!("{}_prep_fail", body_label);
        emitter.label(&body_label);
        emit_aarch64_validate_static_method_arg_count(module, emitter, slot, fail_label);
        let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
        super::eval_argument_helpers::emit_reserve_callable_arg_owners(emitter, callable_owners.len());
        let (arg_temp_bytes, ref_slots) =
            emit_aarch64_prepare_static_method_args(
                module,
                emitter,
                data,
                slot,
                &prep_fail_label,
                callable_support,
            );
        emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
        let escape_label = format!("{}_escape", body_label);
        emit_aarch64_method_exception_boundary_push(
            emitter,
            STATIC_METHOD_HELPER_HANDLER_OFFSET - 64,
            &escape_label,
        );
        let overflow_bytes = materialize_static_method_args(module, emitter, slot);
        let caller_stack_pad_bytes =
            abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
        abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_call_label(emitter, &slot.entry_symbol);
        abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        if slot.return_by_ref {
            emit_load_by_ref_method_result(emitter, &slot.return_ty);
        }
        emit_box_method_result(
            module,
            emitter,
            &slot.return_ty,
            slot.return_is_owned && !slot.return_by_ref,
            slot.return_uses_marker && !slot.return_by_ref,
            EvalMethodResultSource::CompiledStatic,
            &body_label,
        );
        preserve_result_and_write_back_aarch64_ref_args(emitter, &ref_slots, &body_label);
        emit_aarch64_method_exception_boundary_pop(
            emitter,
            STATIC_METHOD_HELPER_HANDLER_OFFSET - 64,
        );
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), true);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the native static method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_aarch64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_aarch64_method_exception_boundary_pop(
            emitter,
            STATIC_METHOD_HELPER_HANDLER_OFFSET - 64,
        );
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), false);
        emitter.instruction(&format!("b {}", fail_label));                      // return failure after preserving by-reference writes
        emitter.label(&prep_fail_label);
        emit_aarch64_method_prep_fail_cleanup(emitter, 64, fail_label);
    }
}

/// Emits x86_64 static method-call bodies for every bridge-supported static method.
fn emit_x86_64_static_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticMethodSlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        let body_label = static_method_body_label(module, slot);
        let prep_fail_label = format!("{}_prep_fail_x", body_label);
        emitter.label(&body_label);
        emit_x86_64_validate_static_method_arg_count(module, emitter, slot, fail_label);
        let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
        super::eval_argument_helpers::emit_reserve_callable_arg_owners(emitter, callable_owners.len());
        let (arg_temp_bytes, ref_slots) =
            emit_x86_64_prepare_static_method_args(
                module,
                emitter,
                data,
                slot,
                &prep_fail_label,
                callable_support,
            );
        emit_acquire_mixed_ref_args(emitter, &ref_slots, arg_temp_bytes);
        let escape_label = format!("{}_escape_x", body_label);
        emit_x86_64_method_exception_boundary_push(
            emitter,
            STATIC_METHOD_HELPER_FRAME_SIZE,
            &escape_label,
        );
        let overflow_bytes = materialize_static_method_args(module, emitter, slot);
        let caller_stack_pad_bytes =
            abi::outgoing_call_stack_pad_bytes(module.target, overflow_bytes);
        abi::emit_reserve_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_call_label(emitter, &slot.entry_symbol);
        abi::emit_release_temporary_stack(emitter, caller_stack_pad_bytes);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        if slot.return_by_ref {
            emit_load_by_ref_method_result(emitter, &slot.return_ty);
        }
        emit_box_method_result(
            module,
            emitter,
            &slot.return_ty,
            slot.return_is_owned && !slot.return_by_ref,
            slot.return_uses_marker && !slot.return_by_ref,
            EvalMethodResultSource::CompiledStatic,
            &body_label,
        );
        preserve_result_and_write_back_x86_64_ref_args(emitter, &ref_slots, &body_label);
        emit_x86_64_method_exception_boundary_pop(emitter, STATIC_METHOD_HELPER_FRAME_SIZE);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), true);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the native static method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_x86_64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_x86_64_method_exception_boundary_pop(emitter, STATIC_METHOD_HELPER_FRAME_SIZE);
        super::eval_argument_helpers::emit_release_callable_arg_owners(emitter, callable_owners.len(), false);
        emitter.instruction(&format!("jmp {}", fail_label));                    // return failure after preserving by-reference writes
        emitter.label(&prep_fail_label);
        emit_x86_64_method_prep_fail_cleanup(emitter, fail_label);
    }
}

/// Restores an ARM64 method-helper frame before reporting an argument-prep fatal.
fn emit_aarch64_method_prep_fail_cleanup(
    emitter: &mut Emitter,
    frame_pointer_offset: usize,
    fail_label: &str,
) {
    emitter.instruction(&format!("sub sp, x29, #{}", frame_pointer_offset));    // restore the helper frame base after argument staging failed
    emitter.instruction(&format!("b {}", fail_label));                          // report the argument-prep failure through the shared fail path
}

/// Restores an x86_64 method-helper frame before reporting an argument-prep fatal.
fn emit_x86_64_method_prep_fail_cleanup(emitter: &mut Emitter, fail_label: &str) {
    emitter.instruction("mov rsp, rbp");                                        // restore the helper frame base after argument staging failed
    emitter.instruction(&format!("jmp {}", fail_label));                        // report the argument-prep failure through the shared fail path
}

/// Emits ARM64 arity validation for one method body.
fn emit_aarch64_validate_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "x9", slot.params.len() as i64);
    emitter.instruction("cmp x0, x9");                                          // compare supplied eval argument count with the method signature
    emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
}

/// Emits x86_64 arity validation for one method body.
fn emit_x86_64_validate_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "r10", slot.params.len() as i64);
    emitter.instruction("cmp rax, r10");                                        // compare supplied eval argument count with the method signature
    emitter.instruction(&format!("jne {}", fail_label));                        // reject method dispatch when arity differs
}

/// Emits ARM64 arity validation for one static method body.
fn emit_aarch64_validate_static_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the eval argument array for static arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "x9", slot.params.len() as i64);
    emitter.instruction("cmp x0, x9");                                          // compare supplied eval argument count with the static method signature
    emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
}

/// Emits x86_64 arity validation for one static method body.
fn emit_x86_64_validate_static_method_arg_count(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the eval argument array for static arity validation
    let array_len_symbol = module.target.extern_symbol("__elephc_eval_value_array_len");
    abi::emit_call_label(emitter, &array_len_symbol);
    abi::emit_load_int_immediate(emitter, "r10", slot.params.len() as i64);
    emitter.instruction("cmp rax, r10");                                        // compare supplied eval argument count with the static method signature
    emitter.instruction(&format!("jne {}", fail_label));                        // reject static method dispatch when arity differs
}

/// Prepares ARM64 method ABI registers for the supported argument shapes.
fn emit_aarch64_prepare_method_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = method_body_label(module, slot);
    let ref_slots = emit_aarch64_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        24,
        &body_label,
        fail_label,
        callable_support,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    // The body reserved one zeroed owner slot per by-value callable below the ref cells.
    let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("ldr x0, [x29, #-32]");                                 // load the unboxed receiver as the first method argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_aarch64_load_eval_arg(module, emitter, index, 24, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 16, fail_label) {
                emit_aarch64_cast_eval_arg(
                    module, emitter, data, param_ty, &label_prefix, fail_label, callable_support,
                );
            }
            super::eval_argument_helpers::emit_record_callable_arg_owner_for_param(
                emitter,
                &callable_owners,
                index,
                ref_slots.len() * 32 + arg_temp_bytes,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Prepares ARM64 static method ABI registers for the supported argument shapes.
fn emit_aarch64_prepare_static_method_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = static_method_body_label(module, slot);
    let ref_slots = emit_aarch64_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        40,
        &body_label,
        fail_label,
        callable_support,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    // The body reserved one zeroed owner slot per by-value callable below the ref cells.
    let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
    abi::emit_load_int_immediate(emitter, "x0", slot.class_id as i64);
    abi::emit_push_result_value(emitter, &PhpType::Int);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&PhpType::Int);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_aarch64_load_eval_arg(module, emitter, index, 40, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 16, fail_label) {
                emit_aarch64_cast_eval_arg(
                    module, emitter, data, param_ty, &label_prefix, fail_label, callable_support,
                );
            }
            super::eval_argument_helpers::emit_record_callable_arg_owner_for_param(
                emitter,
                &callable_owners,
                index,
                ref_slots.len() * 32 + arg_temp_bytes,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Prepares x86_64 method ABI registers for the supported argument shapes.
fn emit_x86_64_prepare_method_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalMethodSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = method_body_label(module, slot);
    let ref_slots = emit_x86_64_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        &body_label,
        fail_label,
        callable_support,
        X86_64_METHOD_CONTEXT_FRAME_OFFSET,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    // The body reserved one zeroed owner slot per by-value callable below the ref cells.
    let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
    let receiver_ty = PhpType::Object(slot.class_name.clone());
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // load the unboxed receiver as the first method argument
    abi::emit_push_result_value(emitter, &receiver_ty);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&receiver_ty);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_x86_64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 40, fail_label) {
                emit_x86_64_cast_eval_arg(
                    module, emitter, data, param_ty, &label_prefix, fail_label, callable_support,
                    X86_64_METHOD_CONTEXT_FRAME_OFFSET,
                );
            }
            super::eval_argument_helpers::emit_record_callable_arg_owner_for_param(
                emitter,
                &callable_owners,
                index,
                ref_slots.len() * 32 + arg_temp_bytes,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Prepares x86_64 static method ABI registers for the supported argument shapes.
fn emit_x86_64_prepare_static_method_args(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticMethodSlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> (usize, Vec<EvalRefArgSlot>) {
    let body_label = static_method_body_label(module, slot);
    let ref_slots = emit_x86_64_ref_arg_cells(
        module,
        emitter,
        data,
        &slot.params,
        &slot.ref_params,
        &body_label,
        fail_label,
        callable_support,
        X86_64_STATIC_METHOD_CONTEXT_FRAME_OFFSET,
    );
    let visible_abi_params = eval_abi_param_types_for_refs(&slot.params, &slot.ref_params);
    // The body reserved one zeroed owner slot per by-value callable below the ref cells.
    let callable_owners = super::eval_argument_helpers::callable_arg_owner_params(&slot.params, &slot.ref_params);
    abi::emit_load_int_immediate(emitter, "rax", slot.class_id as i64);
    abi::emit_push_result_value(emitter, &PhpType::Int);
    let mut arg_temp_bytes = eval_arg_temp_slot_size(&PhpType::Int);
    for (index, param_ty) in slot.params.iter().enumerate() {
        if let Some(ref_slot) = ref_slots.iter().find(|ref_slot| ref_slot.param_index == index) {
            abi::emit_temporary_stack_address(
                emitter,
                abi::int_result_reg(emitter),
                arg_temp_bytes + ref_slot.raw_offset,
            );
            abi::emit_push_result_value(emitter, &PhpType::Int);
        } else {
            emit_x86_64_load_eval_arg(module, emitter, index, fail_label);
            let label_prefix = format!("{}_arg_{}", body_label, index);
            if !emit_borrowed_string_arg(emitter, param_ty, 40, fail_label) {
                emit_x86_64_cast_eval_arg(
                    module, emitter, data, param_ty, &label_prefix, fail_label, callable_support,
                    X86_64_STATIC_METHOD_CONTEXT_FRAME_OFFSET,
                );
            }
            super::eval_argument_helpers::emit_record_callable_arg_owner_for_param(
                emitter,
                &callable_owners,
                index,
                ref_slots.len() * 32 + arg_temp_bytes,
            );
            abi::emit_push_result_value(emitter, &param_ty.codegen_repr());
        }
        arg_temp_bytes += eval_arg_temp_slot_size(&visible_abi_params[index]);
    }
    (arg_temp_bytes, ref_slots)
}

/// Materializes the pushed receiver and eval arguments into the target method ABI.
fn materialize_method_args(
    module: &Module,
    emitter: &mut Emitter,
    receiver_ty: &PhpType,
    params: &[PhpType],
    ref_params: &[bool],
) -> usize {
    let mut arg_types = Vec::with_capacity(params.len() + 1);
    arg_types.push(receiver_ty.clone());
    arg_types.extend(eval_abi_param_types_for_refs(params, ref_params));
    let assignments = abi::build_outgoing_arg_assignments_for_target(module.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// Materializes pushed eval arguments into the target static method ABI.
fn materialize_static_method_args(
    module: &Module,
    emitter: &mut Emitter,
    slot: &EvalStaticMethodSlot,
) -> usize {
    let mut arg_types = Vec::with_capacity(slot.params.len() + 1);
    arg_types.push(PhpType::Int);
    arg_types.extend(eval_abi_param_types_for_refs(&slot.params, &slot.ref_params));
    let assignments = abi::build_outgoing_arg_assignments_for_target(module.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// Prepares ARM64 stack cells for eval-supplied by-reference arguments.
fn emit_aarch64_ref_arg_cells(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_types: &[PhpType],
    ref_params: &[bool],
    arg_array_frame_offset: usize,
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) -> Vec<EvalRefArgSlot> {
    let mut ref_slots = eval_ref_arg_slots(param_types, ref_params, false);
    for slot in &mut ref_slots {
        emit_aarch64_load_eval_arg(module, emitter, slot.param_index, arg_array_frame_offset, fail_label);
        emitter.instruction("ldr x0, [x29, #-16]");                             // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            slot.raw_refcounted_owned = true;
            emitter.instruction("ldr x0, [x29, #-16]");                         // seed the mutable by-reference Mixed slot with the original cell
            abi::emit_push_result_value(emitter, &PhpType::Mixed);
        } else {
            let arg_label = format!("{}_ref_arg_{}", label_prefix, slot.param_index);
            emit_aarch64_cast_eval_arg(
                module,
                emitter,
                data,
                &slot.param_ty,
                &arg_label,
                fail_label,
                callable_support,
            );
            if slot.param_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(emitter, "__rt_str_persist");
            }
            abi::emit_push_result_value(emitter, &slot.param_ty);
        }
    }
    ref_slots
}

/// Prepares x86_64 stack cells for eval-supplied by-reference arguments.
fn emit_x86_64_ref_arg_cells(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_types: &[PhpType],
    ref_params: &[bool],
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
    context_frame_offset: usize,
) -> Vec<EvalRefArgSlot> {
    let mut ref_slots = eval_ref_arg_slots(param_types, ref_params, false);
    for slot in &mut ref_slots {
        emit_x86_64_load_eval_arg(module, emitter, slot.param_index, fail_label);
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
            slot.raw_refcounted_owned = true;
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // seed the mutable by-reference Mixed slot with the original cell
            abi::emit_push_result_value(emitter, &PhpType::Mixed);
        } else {
            let arg_label = format!("{}_ref_arg_{}", label_prefix, slot.param_index);
            emit_x86_64_cast_eval_arg(
                module,
                emitter,
                data,
                &slot.param_ty,
                &arg_label,
                fail_label,
                callable_support,
                context_frame_offset,
            );
            if slot.param_ty.codegen_repr() == PhpType::Str {
                abi::emit_call_label(emitter, "__rt_str_persist");
            }
            abi::emit_push_result_value(emitter, &slot.param_ty);
        }
    }
    ref_slots
}

/// Preserves an ARM64 boxed return value while by-reference eval args are written back.
fn preserve_result_and_write_back_aarch64_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalRefArgSlot],
    label_prefix: &str,
) {
    if ref_slots.is_empty() {
        return;
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    emit_aarch64_write_back_ref_args(emitter, ref_slots, 16, label_prefix);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
}

/// Preserves an x86_64 boxed return value while by-reference eval args are written back.
fn preserve_result_and_write_back_x86_64_ref_args(
    emitter: &mut Emitter,
    ref_slots: &[EvalRefArgSlot],
    label_prefix: &str,
) {
    if ref_slots.is_empty() {
        return;
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    emit_x86_64_write_back_ref_args(emitter, ref_slots, 16, label_prefix);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
}

/// Borrows one normalized eval argument in an ARM64 spill slot for the native call.
fn emit_aarch64_load_eval_arg(
    _module: &Module,
    emitter: &mut Emitter,
    index: usize,
    arg_array_frame_offset: usize,
    fail_label: &str,
) {
    super::eval_argument_helpers::emit_borrowed_argument(
        emitter, index, arg_array_frame_offset, 16, fail_label,
    );
}

/// Borrows one normalized eval argument in an x86_64 spill slot for the native call.
fn emit_x86_64_load_eval_arg(_module: &Module, emitter: &mut Emitter, index: usize, fail_label: &str) {
    super::eval_argument_helpers::emit_borrowed_argument(emitter, index, 32, 40, fail_label);
}

/// Casts one boxed eval argument into ARM64 result registers for temporary staging.
fn emit_aarch64_cast_eval_arg(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if param_ty.is_php_array() {
        emitter.instruction("ldr x0, [x29, #-16]");                             // borrow the boxed argument before checking its PHP array constraint
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
    match param_ty.codegen_repr() {
        PhpType::Int | PhpType::Pointer(_) => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for integer coercion
            emitter.instruction("bl __rt_mixed_cast_int");                      // coerce the eval argument to a PHP int
        }
        PhpType::Bool => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for boolean coercion
            emitter.instruction("bl __rt_mixed_cast_bool");                     // coerce the eval argument to a PHP bool
        }
        PhpType::Float => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for float coercion
            emitter.instruction("bl __rt_mixed_cast_float");                    // coerce the eval argument to a PHP float in d0
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for string coercion
            emit_borrowed_eval_string_argument(emitter, label_prefix);
        }
        PhpType::Callable => {
            super::eval_callable_helpers::emit_aarch64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                label_prefix,
                fail_label,
                EVAL_ARG_CONTEXT_AARCH64_OFFSET,
                EVAL_ARG_BOXED_VALUE_AARCH64_OFFSET,
            );
        }
        PhpType::TaggedScalar => {
            emit_aarch64_cast_eval_tagged_scalar_arg(emitter, label_prefix);
        }
        PhpType::Mixed => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // reload the boxed eval argument for a Mixed method parameter
        }
        PhpType::Object(class_name) => {
            emit_aarch64_cast_eval_object_arg(module, emitter, data, &class_name, fail_label);
        }
        PhpType::Array(_) => {
            emit_aarch64_cast_eval_array_arg(emitter, 4, true, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_aarch64_cast_eval_array_arg(emitter, 5, false, fail_label);
        }
        PhpType::Iterable => {
            emit_aarch64_cast_eval_iterable_arg(module, emitter, label_prefix, fail_label);
        }
        _ => {}
    }
}

/// Coerces one ARM64 eval argument into the inline nullable-int tagged-scalar ABI pair.
fn emit_aarch64_cast_eval_tagged_scalar_arg(emitter: &mut Emitter, label_prefix: &str) {
    let null_label = format!("{}_tagged_scalar_null", label_prefix);
    let done_label = format!("{}_tagged_scalar_done", label_prefix);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for nullable-int inspection
    emitter.instruction("str x0, [sp, #-16]!");                                 // preserve the boxed eval argument across tag inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete eval argument tag and payload words
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the nullable-int argument is null
    emitter.instruction(&format!("b.eq {}", null_label));                       // materialize a tagged null for null eval arguments
    emitter.instruction("ldr x0, [sp]");                                        // reload the boxed eval argument for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce the non-null eval argument to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // skip the null materialization path after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("add sp, sp, #16");                                     // discard the preserved boxed eval argument
}

/// Validates and unboxes one ARM64 object-typed eval argument for native method dispatch.
fn emit_aarch64_cast_eval_object_arg(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("ldr x0, [x29, #-16]");                             // reload the boxed eval argument for named object type validation
        abi::emit_symbol_address(emitter, "x1", &label);
        abi::emit_load_int_immediate(emitter, "x2", len as i64);
        emitter.instruction("mov x3, xzr");                                     // allow exact class matches for named object type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emit_aarch64_branch_if_zero_far(emitter, "x0", fail_label);
    }
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for object unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the object payload for the native method call
    emitter.instruction("cmp x0, #6");                                          // object type hints require an object payload, not a class string
    emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
    emitter.instruction("mov x0, x1");                                          // place the unboxed object pointer in the result register
}

/// Validates and unboxes one ARM64 array-typed eval argument for native method dispatch.
///
/// A PHP `array` parameter accepts both runtime storage shapes; an internally associative
/// parameter still requires its hash representation so its native body can use hash operations.
fn emit_aarch64_cast_eval_array_arg(
    emitter: &mut Emitter,
    expected_tag: i64,
    allow_assoc_shape: bool,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for array unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the array payload for the native method call
    // A null reaching an array slot here is a parameter DEFAULT (binding type-checks every
    // argument the caller passed): `$default = null` in storage adopted from an ancestor's
    // hidden variadic. Compiled callers pass that default as a null pointer, so this does too.
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the defaulted argument is null
    emitter.instruction("b.eq 3f");                                             // pass the null default as a null array pointer
    abi::emit_load_int_immediate(emitter, "x9", expected_tag);
    emitter.instruction("cmp x0, x9");                                          // compare the eval payload tag with the expected array ABI
    if allow_assoc_shape {
        emitter.instruction("b.eq 2f");                                         // accept the indexed representation immediately
        emitter.instruction("cmp x0, #5");                                      // generic PHP arrays also accept associative storage
        emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
        emitter.label("2");
    } else {
        emit_aarch64_branch_if_not_equal_far(emitter, fail_label);
    }
    emitter.instruction("mov x0, x1");                                          // place the unboxed array pointer in the result register
    emitter.instruction("b 4f");                                                // skip the null-default materialization
    emitter.label("3");
    emitter.instruction("mov x0, #0");                                          // a null default travels as a null array pointer
    emitter.label("4");
}

/// Validates and unboxes one ARM64 iterable-typed eval argument for native method dispatch.
fn emit_aarch64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    label_prefix: &str,
    fail_label: &str,
) {
    let payload_ok = format!("{}_iterable_payload", label_prefix);
    let object_case = format!("{}_iterable_object", label_prefix);
    let object_ok = format!("{}_iterable_object_ok", label_prefix);
    let done = format!("{}_iterable_done", label_prefix);
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for iterable unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete iterable payload tag and pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.eq {}", payload_ok));                       // indexed arrays satisfy iterable parameters
    emitter.instruction("cmp x0, #5");                                          // runtime tag 5 means associative array
    emitter.instruction(&format!("b.eq {}", payload_ok));                       // associative arrays satisfy iterable parameters
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means object
    emitter.instruction(&format!("b.eq {}", object_case));                      // object values need Traversable interface validation
    emitter.instruction(&format!("b {}", fail_label));                          // reject scalar values for iterable parameters
    emitter.label(&payload_ok);
    emitter.instruction("mov x0, x1");                                          // place the array payload pointer in the result register
    emitter.instruction(&format!("b {}", done));                                // skip object-specific interface validation
    emitter.label(&object_case);
    emit_aarch64_validate_iterable_object(module, emitter, &object_ok, fail_label);
    emitter.label(&object_ok);
    emitter.instruction("ldr x0, [sp], #16");                                   // restore the iterable object pointer as the result
    emitter.label(&done);
}

/// Validates the ARM64 object payload saved in `x1` against Traversable interfaces.
fn emit_aarch64_validate_iterable_object(
    module: &Module,
    emitter: &mut Emitter,
    object_ok: &str,
    fail_label: &str,
) {
    let interface_ids = traversable_interface_ids(module);
    if interface_ids.is_empty() {
        emitter.instruction(&format!("b {}", fail_label));                      // reject objects when no Traversable interface metadata exists
        return;
    }
    emitter.instruction("str x1, [sp, #-16]!");                                 // preserve the object payload across Traversable checks
    for interface_id in interface_ids {
        emitter.instruction("ldr x0, [sp]");                                    // reload the object pointer as matcher argument 1
        abi::emit_load_int_immediate(emitter, "x1", interface_id as i64);
        abi::emit_load_int_immediate(emitter, "x2", 1);
        abi::emit_call_label(emitter, "__rt_exception_matches");
        emitter.instruction("cmp x0, #0");                                      // test whether the object implements this Traversable interface
        emitter.instruction(&format!("b.ne {}", object_ok));                    // matching Iterator metadata accepts the object
    }
    emitter.instruction("add sp, sp, #16");                                     // discard the rejected object payload
    emitter.instruction(&format!("b {}", fail_label));                          // reject non-Traversable objects for iterable parameters
}

/// Casts one boxed eval argument into x86_64 result registers for temporary staging.
fn emit_x86_64_cast_eval_arg(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    param_ty: &PhpType,
    label_prefix: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
    context_frame_offset: usize,
) {
    if param_ty.is_php_array() {
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // borrow the boxed argument before checking its PHP array constraint
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
    match param_ty.codegen_repr() {
        PhpType::Int | PhpType::Pointer(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for integer coercion
            emitter.instruction("call __rt_mixed_cast_int");                    // coerce the eval argument to a PHP int
        }
        PhpType::Bool => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for boolean coercion
            emitter.instruction("call __rt_mixed_cast_bool");                   // coerce the eval argument to a PHP bool
        }
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval argument to a PHP float in xmm0
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for string coercion
            emit_borrowed_eval_string_argument(emitter, label_prefix);
        }
        PhpType::Callable => {
            super::eval_callable_helpers::emit_x86_64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                label_prefix,
                fail_label,
                context_frame_offset,
                EVAL_ARG_BOXED_VALUE_X86_64_OFFSET,
            );
        }
        PhpType::TaggedScalar => {
            emit_x86_64_cast_eval_tagged_scalar_arg(emitter, label_prefix);
        }
        PhpType::Mixed => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval argument for a Mixed method parameter
        }
        PhpType::Object(class_name) => {
            emit_x86_64_cast_eval_object_arg(module, emitter, data, &class_name, fail_label);
        }
        PhpType::Array(_) => {
            emit_x86_64_cast_eval_array_arg(emitter, 4, true, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_x86_64_cast_eval_array_arg(emitter, 5, false, fail_label);
        }
        PhpType::Iterable => {
            emit_x86_64_cast_eval_iterable_arg(module, emitter, label_prefix, fail_label);
        }
        _ => {}
    }
}

/// Coerces one x86_64 eval argument into the inline nullable-int tagged-scalar ABI pair.
fn emit_x86_64_cast_eval_tagged_scalar_arg(emitter: &mut Emitter, label_prefix: &str) {
    let null_label = format!("{}_tagged_scalar_null", label_prefix);
    let done_label = format!("{}_tagged_scalar_done", label_prefix);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete eval argument tag and payload words
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the nullable-int argument is null
    emitter.instruction(&format!("je {}", null_label));                         // materialize a tagged null for null eval arguments
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce the non-null eval argument to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the null materialization path after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
}

/// Validates and unboxes one x86_64 object-typed eval argument for native method dispatch.
fn emit_x86_64_cast_eval_object_arg(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // reload the boxed eval argument for named object type validation
        abi::emit_symbol_address(emitter, "rsi", &label);
        abi::emit_load_int_immediate(emitter, "rdx", len as i64);
        emitter.instruction("xor ecx, ecx");                                    // allow exact class matches for named object type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction("test rax, rax");                                   // check whether the value satisfied the named object type hint
        emitter.instruction(&format!("je {}", fail_label));                     // reject values that fail the named object type hint
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for object unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the object payload for the native method call
    emitter.instruction("cmp rax, 6");                                          // object type hints require an object payload, not a class string
    emitter.instruction(&format!("jne {}", fail_label));                        // reject malformed non-object payloads
    emitter.instruction("mov rax, rdi");                                        // place the unboxed object pointer in the result register
}

/// Validates and unboxes one x86_64 array-typed eval argument for native method dispatch.
///
/// A PHP `array` parameter accepts both runtime storage shapes; an internally associative
/// parameter still requires its hash representation so its native body can use hash operations.
fn emit_x86_64_cast_eval_array_arg(
    emitter: &mut Emitter,
    expected_tag: i64,
    allow_assoc_shape: bool,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for array unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the array payload for the native method call
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the defaulted argument is null
    emitter.instruction("je 3f");                                               // pass the null default as a null array pointer
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10");                                        // compare the eval payload tag with the expected array ABI
    if allow_assoc_shape {
        emitter.instruction("je 2f");                                           // accept the indexed representation immediately
        emitter.instruction("cmp rax, 5");                                      // generic PHP arrays also accept associative storage
        emitter.instruction(&format!("jne {}", fail_label));                    // reject values that are not either PHP array representation
        emitter.label("2");
    } else {
        emitter.instruction(&format!("jne {}", fail_label));                    // reject array payloads with an incompatible ABI shape
    }
    emitter.instruction("mov rax, rdi");                                        // place the unboxed array pointer in the result register
    emitter.instruction("jmp 4f");                                              // skip the null-default materialization
    emitter.label("3");
    emitter.instruction("xor eax, eax");                                        // a null default travels as a null array pointer
    emitter.label("4");
}

/// Validates and unboxes one x86_64 iterable-typed eval argument for native method dispatch.
fn emit_x86_64_cast_eval_iterable_arg(
    module: &Module,
    emitter: &mut Emitter,
    label_prefix: &str,
    fail_label: &str,
) {
    let payload_ok = format!("{}_iterable_payload", label_prefix);
    let object_case = format!("{}_iterable_object", label_prefix);
    let object_ok = format!("{}_iterable_object_ok", label_prefix);
    let done = format!("{}_iterable_done", label_prefix);
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for iterable unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete iterable payload tag and pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("je {}", payload_ok));                         // indexed arrays satisfy iterable parameters
    emitter.instruction("cmp rax, 5");                                          // runtime tag 5 means associative array
    emitter.instruction(&format!("je {}", payload_ok));                         // associative arrays satisfy iterable parameters
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means object
    emitter.instruction(&format!("je {}", object_case));                        // object values need Traversable interface validation
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject scalar values for iterable parameters
    emitter.label(&payload_ok);
    emitter.instruction("mov rax, rdi");                                        // place the array payload pointer in the result register
    emitter.instruction(&format!("jmp {}", done));                              // skip object-specific interface validation
    emitter.label(&object_case);
    emit_x86_64_validate_iterable_object(module, emitter, &object_ok, fail_label);
    emitter.label(&object_ok);
    abi::emit_pop_reg(emitter, "rax");
    emitter.label(&done);
}

/// Validates the x86_64 object payload saved in `rdi` against Traversable interfaces.
fn emit_x86_64_validate_iterable_object(
    module: &Module,
    emitter: &mut Emitter,
    object_ok: &str,
    fail_label: &str,
) {
    let interface_ids = traversable_interface_ids(module);
    if interface_ids.is_empty() {
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject objects when no Traversable interface metadata exists
        return;
    }
    abi::emit_push_reg(emitter, "rdi");
    for interface_id in interface_ids {
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // reload the object pointer as matcher argument 1
        abi::emit_load_int_immediate(emitter, "rsi", interface_id as i64);
        abi::emit_load_int_immediate(emitter, "rdx", 1);
        abi::emit_call_label(emitter, "__rt_exception_matches");
        emitter.instruction("test rax, rax");                                   // test whether the object implements this Traversable interface
        emitter.instruction(&format!("jne {}", object_ok));                     // matching Iterator metadata accepts the object
    }
    abi::emit_pop_reg(emitter, "r10");
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject non-Traversable objects for iterable parameters
}

/// Boxes the current native method result as the owned Mixed cell expected by eval.
///
/// The boxer is handed the CODEGEN REPRESENTATION, not the declared type. The two differ for
/// exactly one shape that matters here: a method declared `: \Closure` is `PhpType::Object` but
/// its value in the result register is a callable DESCRIPTOR, not an object pointer. Boxing that
/// with the declared type stamped runtime tag 6 on a descriptor, and the interpreter then unboxed
/// it as an object — Symfony's `HtmlErrorRenderer::isDebug(): \Closure` segfaulted the worker the
/// first time interpreted code asked the returned value for its type.
fn emit_box_method_result(
    module: &Module,
    emitter: &mut Emitter,
    return_ty: &PhpType,
    owned: bool,
    return_uses_marker: bool,
    source: EvalMethodResultSource,
    label_prefix: &str,
) {
    let repr = return_ty.codegen_repr();
    if repr == PhpType::Void {
        let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(emitter, &null_symbol);
    } else if return_uses_marker
        && matches!(
            source,
            EvalMethodResultSource::CompiledInstance | EvalMethodResultSource::CompiledStatic
        )
    {
        let owned_label = format!("{}_return_owned", label_prefix);
        let done_label = format!("{}_return_boxed", label_prefix);
        super::return_ownership::emit_branch_if_owned(emitter, &owned_label);
        emit_box_borrowed_method_result(emitter, return_ty);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&owned_label);
        emit_box_current_owned_value_as_mixed(emitter, &repr);
        emitter.label(&done_label);
    } else if owned {
        emit_box_current_owned_value_as_mixed(emitter, &repr);
    } else {
        emit_box_current_value_as_mixed(emitter, &repr);
    }
}

/// Reads the value a by-reference method returned the ADDRESS of into the result registers.
///
/// A `&method()` hands its caller the storage address; compiled callers bind or read through it,
/// but the eval bridge boxed the address itself, so `&$session->getUsageIndex()` from interpreted
/// code (Symfony's `ContextListener`) produced an integer that was a pointer. The value read here
/// is borrowed, and is boxed as such.
fn emit_load_by_ref_method_result(emitter: &mut Emitter, return_ty: &PhpType) {
    let address = abi::symbol_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_reg_move(emitter, address, result);
    match return_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, address, 0);
            abi::emit_load_from_address(emitter, len_reg, address, 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), address, 0);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(emitter, result, address, 0);
            abi::emit_load_from_address(
                emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(emitter),
                address,
                8,
            );
        }
        _ => abi::emit_load_from_address(emitter, result, address, 0),
    }
}

/// Promotes one borrowed compiled-PHP method result into an owned eval result cell.
fn emit_box_borrowed_method_result(emitter: &mut Emitter, return_ty: &PhpType) {
    let repr = return_ty.codegen_repr();
    if matches!(repr, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_incref");
    } else {
        if method_result_is_object_only(return_ty) {
            emit_box_owned_object_method_result(emitter);
        } else {
            emit_box_current_value_as_mixed(emitter, &repr);
        }
    }
}

/// Returns whether a method bridge result is represented only by object alternatives.
fn method_result_is_object_only(return_ty: &PhpType) -> bool {
    match return_ty.codegen_repr() {
        PhpType::Object(_) => true,
        PhpType::Union(members) => members
            .iter()
            .all(|member| matches!(member.codegen_repr(), PhpType::Object(_))),
        _ => false,
    }
}

/// Boxes an owned native object return while transferring its call-result ownership to eval.
fn emit_box_owned_object_method_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0");
            emit_box_current_value_as_mixed(emitter, &PhpType::Object(String::new()));
            abi::emit_push_reg(emitter, "x0");
            abi::emit_load_temporary_stack_slot(emitter, "x0", 16);
            abi::emit_call_label(emitter, "__rt_decref_object");
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_release_temporary_stack(emitter, 16);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");
            emit_box_current_value_as_mixed(emitter, &PhpType::Object(String::new()));
            abi::emit_push_reg(emitter, "rax");
            abi::emit_load_temporary_stack_slot(emitter, "rax", 16);
            abi::emit_call_label(emitter, "__rt_decref_object");
            abi::emit_pop_reg(emitter, "rax");
            abi::emit_release_temporary_stack(emitter, 16);
        }
    }
}

/// Returns runtime interface ids for object values accepted by PHP iterable parameters.
fn traversable_interface_ids(module: &Module) -> Vec<u64> {
    ["Iterator", "IteratorAggregate"]
        .into_iter()
        .filter_map(|name| module.interface_infos.get(name).map(|info| info.interface_id))
        .collect()
}

/// Groups method slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalMethodSlot]) -> BTreeMap<u64, Vec<&EvalMethodSlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_id)
            .or_insert_with(Vec::new)
            .push(slot);
    }
    grouped
}

/// Groups static method slots by class name while preserving sorted class order.
fn grouped_static_slots(
    slots: &[EvalStaticMethodSlot],
) -> BTreeMap<&str, Vec<&EvalStaticMethodSlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_name.as_str())
            .or_insert_with(Vec::new)
            .push(slot);
    }
    grouped
}

/// Returns a platform-safe body label for a method slot.
///
/// The class/impl-class/method triplet is joined through `join_php_symbol()` so two slots whose
/// names differ only in where an underscore falls cannot land on the same label.
fn method_body_label(module: &Module, slot: &EvalMethodSlot) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!(
        "{}{}",
        join_php_symbol(
            "__elephc_eval_method",
            &[&slot.class_name, &slot.impl_class, &slot.method]
        ),
        suffix
    )
}

/// Returns a platform-safe label for continuing after a scoped method name miss.
fn method_access_miss_label(module: &Module, slot: &EvalMethodSlot) -> String {
    format!("{}_access_miss", method_body_label(module, slot))
}

/// Returns a platform-safe body label for a static method slot.
///
/// Uses the same injective join as `method_body_label()` under a distinct symbol prefix.
fn static_method_body_label(module: &Module, slot: &EvalStaticMethodSlot) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!(
        "{}{}",
        join_php_symbol(
            "__elephc_eval_static_method_body",
            &[&slot.class_name, &slot.impl_class, &slot.method]
        ),
        suffix
    )
}

/// Returns a platform-safe label for continuing after a scoped static method name miss.
fn static_method_access_miss_label(module: &Module, slot: &EvalStaticMethodSlot) -> String {
    format!("{}_access_miss", static_method_body_label(module, slot))
}

/// Returns class scopes that satisfy one method visibility for a declaring class.
fn visibility_scope_names(
    module: &Module,
    declaring_class: &str,
    visibility: &Visibility,
) -> Vec<String> {
    match visibility {
        Visibility::Public => Vec::new(),
        Visibility::Private => vec![declaring_class.to_string()],
        Visibility::Protected => related_class_scope_names(module, declaring_class),
    }
}

/// Returns AOT classes in the same inheritance line as `declaring_class`.
fn related_class_scope_names(module: &Module, declaring_class: &str) -> Vec<String> {
    let mut scopes = module
        .class_infos
        .keys()
        .filter(|class_name| {
            is_same_or_descendant(module, class_name, declaring_class)
                || is_same_or_descendant(module, declaring_class, class_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    scopes.sort_by(|left, right| {
        class_id_for_scope(module, left)
            .cmp(&class_id_for_scope(module, right))
            .then_with(|| left.cmp(right))
    });
    scopes
}

/// Returns true when `class_name` is `ancestor` or descends from it.
fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the deterministic class id used to order generated scope checks.
fn class_id_for_scope(module: &Module, class_name: &str) -> u64 {
    module
        .class_infos
        .get(class_name)
        .map(|class_info| class_info.class_id)
        .unwrap_or(u64::MAX)
}


/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};
    use crate::ir::{Function, IrType};
    use crate::types::FunctionSig;

    /// Verifies far zero checks invert `cbz` locally before the wider unconditional branch.
    #[test]
    fn far_zero_branch_uses_inverted_compare_and_branch_shape() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_aarch64_branch_if_zero_far(&mut emitter, "x3", "_far_fail");

        assert_eq!(
            emitter.output(),
            concat!("    cbnz x3, 1f\n", "    b _far_fail\n", "1:\n")
        );
    }

    /// Verifies far inequality checks invert the condition before the wider branch.
    #[test]
    fn far_not_equal_branch_uses_inverted_condition_shape() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_aarch64_branch_if_not_equal_far(&mut emitter, "_far_fail");

        assert_eq!(
            emitter.output(),
            concat!("    b.eq 1f\n", "    b _far_fail\n", "1:\n")
        );
    }

    /// Verifies eval bridge slots box the physical EIR object return despite a Mixed signature.
    #[test]
    fn emitted_method_return_type_uses_eir_abi_representation() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let mut module = Module::new(target);
        module.class_methods.push(Function::new(
            method_symbol("ConcreteResolver", "resolve"),
            IrType::I64,
            PhpType::Object("ConcreteResolver".to_string()),
        ));
        let signature = FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Mixed,
            declared_return: true,
            by_ref_return: false,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
            is_generator: false,
        };

        assert_eq!(
            emitted_method_return_type(
                &module,
                &method_symbol("ConcreteResolver", "resolve"),
                &signature,
            ),
            PhpType::Object("ConcreteResolver".to_string()),
        );
    }
}

#[cfg(test)]
mod catalog_tests {
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
    use crate::ir::Module;
    use crate::types::PhpType;

    use super::{
        EvalMethodResultSource, emit_box_method_result,
    };

    /// Every throwable this helper can materialize is a catalogued builtin class.
    #[test]
    fn throwable_list_is_a_subset_of_the_class_catalog() {
        for name in super::BUILTIN_THROWABLE_METHOD_CLASSES {
            assert!(
                elephc_builtin_contract::lookup_class(name).is_some(),
                "{name} is not in the shared class catalog"
            );
        }
    }

    /// Verifies compiled instance and static Mixed returns consume the private marker.
    #[test]
    fn compiled_method_results_use_the_return_ownership_marker_on_all_targets() {
        for target in supported_targets() {
            for return_ty in [PhpType::Mixed, PhpType::Str] {
                for (source, name) in [
                    (EvalMethodResultSource::CompiledInstance, "instance"),
                    (EvalMethodResultSource::CompiledStatic, "static"),
                ] {
                    let asm = method_result_asm(target, source, true, name, &return_ty);
                    match target.arch {
                        Arch::AArch64 => assert!(asm.contains("cbnz x15"), "{target:?}: {asm}"),
                        Arch::X86_64 => {
                            assert!(asm.contains("test r11, r11"), "{target:?}: {asm}")
                        }
                    }
                    match &return_ty {
                        PhpType::Mixed => {
                            assert_eq!(
                                asm.matches("__rt_incref").count(),
                                1,
                                "{target:?}: {asm}"
                            );
                        }
                        PhpType::Str => {
                            assert!(asm.contains("__rt_mixed_from_value"), "{target:?}: {asm}");
                            assert!(asm.contains("__rt_heap_alloc"), "{target:?}: {asm}");
                        }
                        _ => unreachable!(),
                    }
                    assert!(
                        asm.contains(&format!("eval_{name}_return_owned")),
                        "{target:?}: {asm}"
                    );
                }
            }
        }
    }

    /// Verifies runtime-backed methods never interpret a caller-saved register as a return marker.
    #[test]
    fn runtime_method_results_ignore_the_compiled_return_marker() {
        for target in supported_targets() {
            let asm = method_result_asm(
                target,
                EvalMethodResultSource::RuntimeHelper,
                true,
                "runtime",
                &PhpType::Mixed,
            );
            assert!(!asm.contains("x15"), "{target:?}: {asm}");
            assert!(!asm.contains("r11"), "{target:?}: {asm}");
            assert!(!asm.contains("__rt_incref"), "{target:?}: {asm}");
        }
    }

    /// Emits one isolated method-result boxing sequence for structural ABI assertions.
    fn method_result_asm(
        target: Target,
        source: EvalMethodResultSource,
        return_uses_marker: bool,
        name: &str,
        return_ty: &PhpType,
    ) -> String {
        let module = Module::new(target);
        let mut emitter = Emitter::new(target);
        emit_box_method_result(
            &module,
            &mut emitter,
            return_ty,
            false,
            return_uses_marker,
            source,
            &format!("eval_{name}"),
        );
        emitter.output()
    }

    /// Returns every supported target so the private return marker stays target-aware.
    fn supported_targets() -> [Target; 5] {
        [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ]
    }
}

#[cfg(test)]
mod source_entry_tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};
    use crate::types::FunctionSig;

    /// Builds a fixed-arity signature with the given parameter types.
    fn signature(params: Vec<(String, PhpType)>) -> FunctionSig {
        let len = params.len();
        FunctionSig {
            params,
            param_type_exprs: vec![None; len],
            param_attributes: vec![Vec::new(); len],
            defaults: vec![None; len],
            return_type: PhpType::Mixed,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false; len],
            declared_params: vec![true; len],
            variadic: None,
            deprecation: None,
            is_generator: false,
        }
    }

    /// Appends the generated trailing argument collector the `func_args` pass injects.
    fn with_generated_collector(mut sig: FunctionSig) -> FunctionSig {
        sig.params.push((
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        sig.param_type_exprs.push(None);
        sig.param_attributes.push(Vec::new());
        sig.defaults.push(None);
        sig.ref_params.push(false);
        sig.declared_params.push(false);
        sig.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());
        sig
    }

    /// Returns one empty module per architecture the backend emits eval helpers for.
    fn modules() -> Vec<Module> {
        vec![
            Module::new(Target::new(Platform::Linux, Arch::X86_64)),
            Module::new(Target::new(Platform::Linux, Arch::AArch64)),
            Module::new(Target::new(Platform::MacOS, Arch::AArch64)),
        ]
    }

    /// A method without generated slots keeps the declared arity and the raw method symbol.
    #[test]
    fn plain_methods_keep_the_declared_signature_and_raw_symbol() {
        let declared = signature(vec![("handler".to_string(), PhpType::Callable)]);
        for module in modules() {
            let EvalMethodEntry::Raw(physical, symbol) = eval_method_entry(
                &module,
                "XMLParser",
                "setHandler",
                &declared,
                MethodKind::Instance,
            );
            assert_eq!(physical.params, declared.params);
            assert_eq!(symbol, method_symbol("XMLParser", "setHandler"));
        }
    }

    /// Magician materializes a generated collector, so eval keeps its physical slot and enters
    /// the raw method rather than asking a source adapter to append another collector.
    #[test]
    fn hidden_collector_methods_keep_physical_arity_and_enter_the_raw_symbol() {
        let visible = signature(vec![
            ("start".to_string(), PhpType::Callable),
            ("end".to_string(), PhpType::Callable),
        ]);
        let physical = with_generated_collector(visible.clone());
        for module in modules() {
            let EvalMethodEntry::Raw(resolved, symbol) = eval_method_entry(
                &module,
                "Handlers",
                "setElementHandler",
                &physical,
                MethodKind::Instance,
            );
            assert_eq!(resolved.params, physical.params);
            assert_eq!(resolved.ref_params, physical.ref_params);
            assert_eq!(resolved.variadic, physical.variadic);
            assert_eq!(resolved.return_type, physical.return_type);
            assert_eq!(symbol, method_symbol("Handlers", "setElementHandler"));
        }
    }

    /// An inherited implementation resolves against the class that owns the body, so a
    /// descendant reaches the ancestor's raw physical entry rather than a nonexistent symbol.
    #[test]
    fn inherited_implementations_resolve_against_the_implementing_owner() {
        let physical =
            with_generated_collector(signature(vec![("data".to_string(), PhpType::Str)]));
        for module in modules() {
            let EvalMethodEntry::Raw(resolved, symbol) = eval_method_entry(
                &module,
                "BaseParser",
                "parseInto",
                &physical,
                MethodKind::Instance,
            );
            assert_eq!(resolved.params, physical.params);
            assert_eq!(symbol, method_symbol("BaseParser", "parseInto"));
            assert_ne!(symbol, method_symbol("DerivedParser", "parseInto"));
        }
    }

    /// A source variadic and its hidden actual-count slot both stay on the raw physical entry.
    #[test]
    fn source_variadics_and_hidden_counts_stay_on_the_raw_entry() {
        let mut variadic = signature(vec![("values".to_string(), PhpType::Mixed)]);
        variadic.variadic = Some("values".to_string());
        let mut with_hidden_count = variadic.clone();
        with_hidden_count.params.push((
            crate::func_args::HIDDEN_ARGC_PARAM.to_string(),
            PhpType::Int,
        ));
        with_hidden_count.param_type_exprs.push(None);
        with_hidden_count.param_attributes.push(Vec::new());
        with_hidden_count.defaults.push(None);
        with_hidden_count.ref_params.push(false);
        with_hidden_count.declared_params.push(false);
        for module in modules() {
            assert!(matches!(
                eval_method_entry(&module, "Collector", "add", &variadic, MethodKind::Instance),
                EvalMethodEntry::Raw(_, _)
            ));
            match eval_method_entry(
                &module,
                "Collector",
                "add",
                &with_hidden_count,
                MethodKind::Instance,
            ) {
                EvalMethodEntry::Raw(physical, symbol) => {
                    assert_eq!(physical.params, with_hidden_count.params);
                    assert_eq!(symbol, method_symbol("Collector", "add"));
                }
            }
        }
    }

    /// A static method without generated slots keeps its declared arity and the raw static
    /// symbol, so the shared resolver leaves every ordinary static entry exactly where it was.
    #[test]
    fn plain_static_methods_keep_the_declared_signature_and_raw_symbol() {
        let declared = signature(vec![("path".to_string(), PhpType::Str)]);
        for module in modules() {
            let EvalMethodEntry::Raw(physical, symbol) =
                eval_method_entry(&module, "Loader", "fromFile", &declared, MethodKind::Static);
            assert_eq!(physical.params, declared.params);
            assert_eq!(symbol, static_method_symbol("Loader", "fromFile"));
            assert_ne!(symbol, method_symbol("Loader", "fromFile"));
        }
    }

    /// A static method reading `func_get_args()` grows the same hidden collector an instance
    /// method does, so eval keeps that physical slot and enters the raw static symbol.
    #[test]
    fn hidden_collector_static_methods_keep_physical_arity_and_enter_the_raw_symbol() {
        let visible = signature(vec![("label".to_string(), PhpType::Str)]);
        let physical = with_generated_collector(visible.clone());
        for module in modules() {
            let EvalMethodEntry::Raw(resolved, symbol) =
                eval_method_entry(&module, "Registry", "record", &physical, MethodKind::Static);
            assert_eq!(resolved.params, physical.params);
            assert_eq!(resolved.ref_params, physical.ref_params);
            assert_eq!(resolved.variadic, physical.variadic);
            assert_eq!(resolved.return_type, physical.return_type);
            assert_eq!(symbol, static_method_symbol("Registry", "record"));
            assert_ne!(symbol, method_symbol("Registry", "record"));
        }
    }

    /// An inherited static implementation resolves against the class that owns the body, so a
    /// descendant reaches the ancestor's raw physical static entry.
    #[test]
    fn inherited_static_implementations_resolve_against_the_implementing_owner() {
        let physical =
            with_generated_collector(signature(vec![("data".to_string(), PhpType::Str)]));
        for module in modules() {
            let EvalMethodEntry::Raw(resolved, symbol) =
                eval_method_entry(&module, "BaseRegistry", "make", &physical, MethodKind::Static);
            assert_eq!(resolved.params, physical.params);
            assert_eq!(symbol, static_method_symbol("BaseRegistry", "make"));
            assert_ne!(symbol, static_method_symbol("DerivedRegistry", "make"));
        }
    }

    /// A static source variadic and its hidden actual-count slot stay on the raw physical entry.
    #[test]
    fn static_source_variadics_and_hidden_counts_stay_on_the_raw_entry() {
        let mut variadic = signature(vec![("values".to_string(), PhpType::Mixed)]);
        variadic.variadic = Some("values".to_string());
        let mut with_hidden_count = variadic.clone();
        with_hidden_count
            .params
            .push((crate::func_args::HIDDEN_ARGC_PARAM.to_string(), PhpType::Int));
        with_hidden_count.param_type_exprs.push(None);
        with_hidden_count.param_attributes.push(Vec::new());
        with_hidden_count.defaults.push(None);
        with_hidden_count.ref_params.push(false);
        with_hidden_count.declared_params.push(false);
        for module in modules() {
            assert!(matches!(
                eval_method_entry(&module, "Aggregator", "sum", &variadic, MethodKind::Static),
                EvalMethodEntry::Raw(_, _)
            ));
            match eval_method_entry(
                &module,
                "Aggregator",
                "sum",
                &with_hidden_count,
                MethodKind::Static,
            ) {
                EvalMethodEntry::Raw(physical, symbol) => {
                    assert_eq!(physical.params, with_hidden_count.params);
                    assert_eq!(symbol, static_method_symbol("Aggregator", "sum"));
                }
            }
        }
    }
}
