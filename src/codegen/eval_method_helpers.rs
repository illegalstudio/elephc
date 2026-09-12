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
//! - A method carrying a generated argument collector is reached through its source
//!   adapter, so a slot validates the PHP-visible arity, not the physical one. Instance
//!   and static slots resolve that entry through the same shared resolver.

use std::collections::BTreeMap;

use crate::codegen::abi;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::platform::Arch;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Function, LocalKind, Module};
use crate::codegen_support::source_method_adapters::{
    MethodKind, source_method_entry_symbol, source_visible_signature, uses_physical_func_args_abi,
};
use crate::names::{join_php_symbol, method_symbol, static_method_symbol};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

use super::eval_ref_arg_helpers::{
    EvalRefArgSlot, eval_abi_param_types_for_refs, eval_arg_temp_slot_size,
    eval_normalized_ref_params, eval_ref_arg_slots, eval_signature_ref_params_supported,
    emit_aarch64_write_back_ref_args, emit_acquire_mixed_ref_args, emit_x86_64_write_back_ref_args,
};
use super::eval_callable_helpers::EvalCallableDescriptorSupport;
use super::eval_argument_helpers::emit_borrowed_string_arg;

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
    entry_symbol: String,
}

const BUILTIN_THROWABLE_METHOD_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "ArgumentCountError",
    "ValueError",
    "ArithmeticError",
    "DivisionByZeroError",
    "AssertionError",
    "UnhandledMatchError",
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
const BUILTIN_THROWABLE_GET_PREVIOUS_LABEL: &str = "__elephc_eval_builtin_throwable_getprevious";
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
    if !module_uses_eval(module) {
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

/// Returns true when the EIR module contains a function that can call eval.
fn module_uses_eval(module: &Module) -> bool {
    all_module_functions(module).any(function_uses_eval)
}

/// Iterates every EIR function body emitted or inspected by the backend.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a function has hidden eval state locals.
fn function_uses_eval(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
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
fn collect_builtin_throwable_method_class_ids(module: &Module) -> Vec<u64> {
    let mut class_ids = BUILTIN_THROWABLE_METHOD_CLASSES
        .iter()
        .filter_map(|class_name| module.class_infos.get(*class_name))
        .map(|class_info| class_info.class_id)
        .collect::<Vec<_>>();
    class_ids.sort_unstable();
    class_ids.dedup();
    class_ids
}

/// How the eval bridge reaches one method implementation.
enum EvalMethodEntry {
    /// The declared signature is already PHP-visible and the raw method symbol accepts it.
    Raw(String),
    /// Generated `func_args` slots sit behind a source adapter the visible arity enters.
    Adapted(crate::types::FunctionSig, String),
    /// The generated ABI cannot be bridged from eval, so this method gets no slot.
    Unsupported,
}

/// Returns the physical entry symbol one method kind publishes without any adaptation.
fn raw_method_entry_symbol(impl_class: &str, method: &str, kind: MethodKind) -> String {
    match kind {
        MethodKind::Instance => method_symbol(impl_class, method),
        MethodKind::Static => static_method_symbol(impl_class, method),
    }
}

/// Resolves the PHP-visible signature and entry symbol for one method implementation.
///
/// Only a generated argument collector changes anything here. Every other method keeps its
/// declared signature and raw symbol, including a source variadic, whose hidden actual count
/// `source_visible_signature()` deliberately refuses to project. The physical signature is read
/// from the implementing class because that owner is what `emit_source_method_adapters()` keys
/// its adapters on, so an inherited method resolves to the same entry its ancestor published.
/// Instance and static methods differ only in which implementation map holds that physical
/// signature and in which symbol family names the entry, so both kinds share this resolution.
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
    if !uses_physical_func_args_abi(physical) {
        return EvalMethodEntry::Raw(raw_method_entry_symbol(impl_class, method, kind));
    }
    let (Ok(source), Ok(symbol)) = (
        source_visible_signature(physical),
        source_method_entry_symbol(impl_class, method, physical, kind),
    ) else {
        return EvalMethodEntry::Unsupported;
    };
    EvalMethodEntry::Adapted(source, symbol)
}

/// Adds bridge-supported instance methods for one class.
fn collect_class_method_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &std::collections::HashSet<(String, String, bool)>,
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
        let runtime_helper = eval_runtime_backed_instance_method_helper(class_name, method);
        let entry = match runtime_helper {
            Some(_) => EvalMethodEntry::Raw(method_symbol(impl_class, method)),
            None => eval_method_entry(module, impl_class, method, sig, MethodKind::Instance),
        };
        let (sig, entry_symbol) = match &entry {
            EvalMethodEntry::Raw(symbol) => (sig, symbol),
            EvalMethodEntry::Adapted(source, symbol) => (source, symbol),
            EvalMethodEntry::Unsupported => continue,
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
            return_ty: sig.return_type.codegen_repr(),
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
    emitted_methods: &std::collections::HashSet<(String, String, bool)>,
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
                EvalMethodEntry::Raw(symbol) => (sig, symbol),
                EvalMethodEntry::Adapted(source, symbol) => (source, symbol),
                EvalMethodEntry::Unsupported => continue,
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
                return_ty: sig.return_type.codegen_repr(),
                is_hidden_shadow: true,
                entry_symbol: entry_symbol.clone(),
                runtime_helper: None,
            });
        }
    }
}

/// Returns a normal-ABI runtime helper for builtin instance methods that eval can bridge.
fn eval_runtime_backed_instance_method_helper(
    class_name: &str,
    method_name: &str,
) -> Option<&'static str> {
    if !matches!(
        class_name.trim_start_matches('\\'),
        "SplDoublyLinkedList" | "SplStack" | "SplQueue" | "SplFixedArray"
    ) {
        return None;
    }
    IntrinsicCall::instance_method(class_name, method_name)?.runtime_helper()
}

/// Adds bridge-supported static methods for one class.
fn collect_class_static_method_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    emitted_methods: &std::collections::HashSet<(String, String, bool)>,
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
            EvalMethodEntry::Raw(symbol) => (sig, symbol),
            EvalMethodEntry::Adapted(source, symbol) => (source, symbol),
            EvalMethodEntry::Unsupported => continue,
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
            return_ty: sig.return_type.codegen_repr(),
            entry_symbol: entry_symbol.clone(),
        });
    }
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
fn method_param_supported(ty: &PhpType) -> bool {
    matches!(
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
fn method_return_supported(ty: &PhpType) -> bool {
    matches!(
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
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // null Mixed receiver cannot dispatch a method
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-object receivers cannot dispatch instance methods
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for method calls
    emit_aarch64_builtin_throwable_method_dispatch(
        module,
        emitter,
        data,
        builtin_throwable_class_ids,
    );
    emit_aarch64_method_dispatch(module, emitter, data, slots, fail_label);
    emitter.instruction(&format!("b {}", fail_label));                          // no supported method matched the request
    emit_aarch64_builtin_throwable_method_bodies(module, emitter, done_label, fail_label);
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
    emit_x86_64_builtin_throwable_method_bodies(module, emitter, done_label, fail_label);
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
    emitter.bl_c("setjmp");                                                      // snapshot the bridge stack before entering native methods
    emitter.instruction(&format!("cbnz x0, {}", escape_label));                 // non-zero setjmp result means a method Throwable escaped
}

/// Emits an ARM64 boundary pop after a native method call returns to magician.
fn emit_aarch64_method_exception_boundary_pop(emitter: &mut Emitter, handler_offset: usize) {
    emitter.comment("pop eval method exception boundary");
    emitter.instruction(&format!("ldr x10, [x29, #{}]", handler_offset));       // reload the previous native exception-handler head
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // reload the saved diagnostic suppression depth
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
    emitter.bl_c("setjmp");                                                      // snapshot the bridge stack before entering native methods
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
    emitter.instruction(&format!(                                               // reload the saved diagnostic suppression depth
        "mov r10, QWORD PTR [rbp - {}]",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
}

/// Emits ARM64 class-id and method-name dispatch for helper method bodies.
fn emit_aarch64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    fail_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_method_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this class test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for method dispatch
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next class when ids differ
        for slot in class_slots {
            emit_aarch64_method_name_compare(module, emitter, data, slot, fail_label);
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id and method-name dispatch for helper method bodies.
fn emit_x86_64_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalMethodSlot],
    fail_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let next_label = format!("__elephc_eval_method_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this class test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for method dispatch
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next class when ids differ
        for slot in class_slots {
            emit_x86_64_method_name_compare(module, emitter, data, slot, fail_label);
        }
        emitter.label(&next_label);
    }
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
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_method_next_{}", class_id);
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the unboxed object pointer before this builtin Throwable test
        emitter.instruction("ldr x9, [x9]");                                    // load the receiver class id for builtin Throwable dispatch
        abi::emit_load_int_immediate(emitter, "x10", *class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next builtin Throwable class when ids differ
        emit_aarch64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            "getmessage",
            BUILTIN_THROWABLE_GET_MESSAGE_LABEL,
        );
        emit_aarch64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            "getcode",
            BUILTIN_THROWABLE_GET_CODE_LABEL,
        );
        emit_aarch64_builtin_throwable_method_name_branch(
            module, emitter, data, "getprevious", BUILTIN_THROWABLE_GET_PREVIOUS_LABEL,
        );
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id and method-name dispatch for compact Throwable methods.
fn emit_x86_64_builtin_throwable_method_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_ids: &[u64],
) {
    for class_id in class_ids {
        let next_label = format!("__elephc_eval_builtin_throwable_method_next_{}_x", class_id);
        emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                   // reload the unboxed object pointer before this builtin Throwable test
        emitter.instruction("mov r11, QWORD PTR [r11]");                        // load the receiver class id for builtin Throwable dispatch
        abi::emit_load_int_immediate(emitter, "r10", *class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this builtin Throwable class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next builtin Throwable class when ids differ
        emit_x86_64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            "getmessage",
            BUILTIN_THROWABLE_GET_MESSAGE_LABEL,
        );
        emit_x86_64_builtin_throwable_method_name_branch(
            module,
            emitter,
            data,
            "getcode",
            BUILTIN_THROWABLE_GET_CODE_LABEL,
        );
        emit_x86_64_builtin_throwable_method_name_branch(
            module, emitter, data, "getprevious", BUILTIN_THROWABLE_GET_PREVIOUS_LABEL,
        );
        emitter.label(&next_label);
    }
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
    emitter.instruction(&format!("cbz x0, {}", target_label));                  // dispatch to the compact Throwable method when names match
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
        emitter.instruction(&format!("cbz x0, {}", target_label));              // dispatch to the method body when the names match
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
        emitter.instruction(&format!("cbz x0, {}", target_label));              // dispatch to the static method body when the names match
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
    emitter.instruction(&format!("cbz x1, {}", fail_label));                    // reject scoped method access outside a class scope
    for scope_name in allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset));   // reload the active eval class-scope pointer
        emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset));   // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp");                              // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", success_label));             // dispatch when scoped visibility is satisfied
    }
    emitter.instruction(&format!("b {}", fail_label));                          // reject scoped method access from unrelated classes
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
    for scope_name in allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(                                                    // reload the active eval class-scope pointer
            &format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)
        );
        emitter.instruction(                                                    // reload the active eval class-scope length
            &format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)
        );
        abi::emit_symbol_address(emitter, "rdx", &label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp");                            // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax");                                   // check whether the current scope matched
        emitter.instruction(&format!("je {}", success_label));                  // dispatch when scoped visibility is satisfied
    }
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject scoped method access from unrelated classes
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
    done_label: &str,
    fail_label: &str,
) {
    emitter.label(BUILTIN_THROWABLE_GET_MESSAGE_LABEL);
    emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for getMessage()
    emitter.instruction("ldr x1, [x9, #8]");                                    // load Throwable message pointer
    emitter.instruction("ldr x2, [x9, #16]");                                   // load Throwable message length
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // box the Throwable message as a Mixed string
    emitter.instruction(&format!("b {}", done_label));                          // return the boxed Throwable method result

    emitter.label(BUILTIN_THROWABLE_GET_CODE_LABEL);
    emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the compact Throwable object for getCode()
    emitter.instruction("ldr x1, [x9, #24]");                                   // load Throwable integer code
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the Throwable code as a Mixed integer
    emitter.instruction(&format!("b {}", done_label));                          // return the boxed Throwable method result
    emit_builtin_throwable_previous_body(module, emitter, done_label, fail_label);
}

/// Emits x86_64 bodies for compact Throwable methods used by eval.
fn emit_x86_64_builtin_throwable_method_bodies(
    module: &Module,
    emitter: &mut Emitter,
    done_label: &str,
    fail_label: &str,
) {
    emitter.label(BUILTIN_THROWABLE_GET_MESSAGE_LABEL);
    emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for getMessage()
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // load Throwable message pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load Throwable message length
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the Throwable message as a Mixed string
    emitter.instruction(&format!("jmp {}", done_label));                        // return the boxed Throwable method result

    emitter.label(BUILTIN_THROWABLE_GET_CODE_LABEL);
    emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the compact Throwable object for getCode()
    emitter.instruction("mov rdi, QWORD PTR [r10 + 24]");                       // load Throwable integer code
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the Throwable code as a Mixed integer
    emitter.instruction(&format!("jmp {}", done_label));                        // return the boxed Throwable method result
    emit_builtin_throwable_previous_body(module, emitter, done_label, fail_label);
}

/// Boxes a borrowed previous link independently so retiring the outer exception cannot invalidate it.
fn emit_builtin_throwable_previous_body(
    module: &Module,
    emitter: &mut Emitter,
    done_label: &str,
    fail_label: &str,
) {
    let null_label = "__elephc_eval_builtin_throwable_previous_null";
    emitter.label(BUILTIN_THROWABLE_GET_PREVIOUS_LABEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
            emitter.instruction("ldr x0, [sp, #16]");                           // borrow the compact outer exception for previous-link lookup
        }
        Arch::X86_64 => {
            emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // borrow the compact outer exception for previous-link lookup
        }
    }
    abi::emit_call_label(emitter, "__rt_throwable_previous");
    abi::emit_branch_if_int_result_zero(emitter, null_label);
    emit_box_current_value_as_mixed(emitter, &PhpType::Object("Throwable".to_string()));
    abi::emit_jump(emitter, done_label);
    emitter.label(null_label);
    emit_box_current_value_as_mixed(emitter, &PhpType::Void);
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
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject unsupported Throwable method arguments from eval
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
        emit_box_method_result(module, emitter, &slot.return_ty);
        preserve_result_and_write_back_aarch64_ref_args(emitter, &ref_slots, &body_label);
        emit_aarch64_method_exception_boundary_pop(emitter, METHOD_HELPER_HANDLER_OFFSET - 48);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the native method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_aarch64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_aarch64_method_exception_boundary_pop(emitter, METHOD_HELPER_HANDLER_OFFSET - 48);
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
        emit_box_method_result(module, emitter, &slot.return_ty);
        preserve_result_and_write_back_x86_64_ref_args(emitter, &ref_slots, &body_label);
        emit_x86_64_method_exception_boundary_pop(emitter, METHOD_HELPER_FRAME_SIZE);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the native method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_x86_64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_x86_64_method_exception_boundary_pop(emitter, METHOD_HELPER_FRAME_SIZE);
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
        emit_box_method_result(module, emitter, &slot.return_ty);
        preserve_result_and_write_back_aarch64_ref_args(emitter, &ref_slots, &body_label);
        emit_aarch64_method_exception_boundary_pop(
            emitter,
            STATIC_METHOD_HELPER_HANDLER_OFFSET - 64,
        );
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
        emit_box_method_result(module, emitter, &slot.return_ty);
        preserve_result_and_write_back_x86_64_ref_args(emitter, &ref_slots, &body_label);
        emit_x86_64_method_exception_boundary_pop(emitter, STATIC_METHOD_HELPER_FRAME_SIZE);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the native static method result
        emitter.label(&escape_label);
        abi::emit_release_temporary_stack(emitter, arg_temp_bytes);
        let escape_writeback_label = format!("{}_throw", body_label);
        emit_x86_64_write_back_ref_args(emitter, &ref_slots, 0, &escape_writeback_label);
        abi::emit_release_temporary_stack(emitter, ref_slots.len() * 32);
        emit_x86_64_method_exception_boundary_pop(emitter, STATIC_METHOD_HELPER_FRAME_SIZE);
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
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject method dispatch when arity differs
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
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject static method dispatch when arity differs
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
    let ref_slots = eval_ref_arg_slots(param_types, ref_params, false);
    for slot in &ref_slots {
        emit_aarch64_load_eval_arg(module, emitter, slot.param_index, arg_array_frame_offset, fail_label);
        emitter.instruction("ldr x0, [x29, #-16]");                             // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
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
    let ref_slots = eval_ref_arg_slots(param_types, ref_params, false);
    for slot in &ref_slots {
        emit_x86_64_load_eval_arg(module, emitter, slot.param_index, fail_label);
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // reload the original eval Mixed cell for by-reference writeback
        abi::emit_push_result_value(emitter, &PhpType::Mixed);
        if matches!(slot.param_ty.codegen_repr(), PhpType::Mixed) {
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
        PhpType::Int => {
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
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval argument to a PHP string pair in x1/x2
        }
        PhpType::Callable => {
            super::eval_callable_helpers::emit_aarch64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                label_prefix,
                fail_label,
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
            emit_aarch64_cast_eval_array_arg(emitter, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_aarch64_cast_eval_array_arg(emitter, 5, fail_label);
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
    let (label, len) = data.add_string(class_name.as_bytes());
    let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for object type validation
    abi::emit_symbol_address(emitter, "x1", &label);
    abi::emit_load_int_immediate(emitter, "x2", len as i64);
    emitter.instruction("mov x3, xzr");                                         // allow exact class matches for object type hints
    abi::emit_call_label(emitter, &is_a_symbol);
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject values that fail the object type hint
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for object unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the object payload for the native method call
    emitter.instruction("cmp x0, #6");                                          // object type hints require an object payload, not a class string
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject malformed non-object payloads
    emitter.instruction("mov x0, x1");                                          // place the unboxed object pointer in the result register
}

/// Validates and unboxes one ARM64 array-typed eval argument for native method dispatch.
fn emit_aarch64_cast_eval_array_arg(emitter: &mut Emitter, expected_tag: i64, fail_label: &str) {
    emitter.instruction("ldr x0, [x29, #-16]");                                 // reload the boxed eval argument for array unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the array payload for the native method call
    abi::emit_load_int_immediate(emitter, "x9", expected_tag);
    emitter.instruction("cmp x0, x9");                                          // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov x0, x1");                                          // place the unboxed array pointer in the result register
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
        PhpType::Int => {
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
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval argument to a PHP string pair
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
            emit_x86_64_cast_eval_array_arg(emitter, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_x86_64_cast_eval_array_arg(emitter, 5, fail_label);
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
    let (label, len) = data.add_string(class_name.as_bytes());
    let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for object type validation
    abi::emit_symbol_address(emitter, "rsi", &label);
    abi::emit_load_int_immediate(emitter, "rdx", len as i64);
    emitter.instruction("xor ecx, ecx");                                        // allow exact class matches for object type hints
    abi::emit_call_label(emitter, &is_a_symbol);
    emitter.instruction("test rax, rax");                                       // check whether the value satisfied the object type hint
    emitter.instruction(&format!("je {}", fail_label));                         // reject values that fail the object type hint
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for object unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the object payload for the native method call
    emitter.instruction("cmp rax, 6");                                          // object type hints require an object payload, not a class string
    emitter.instruction(&format!("jne {}", fail_label));                        // reject malformed non-object payloads
    emitter.instruction("mov rax, rdi");                                        // place the unboxed object pointer in the result register
}

/// Validates and unboxes one x86_64 array-typed eval argument for native method dispatch.
fn emit_x86_64_cast_eval_array_arg(emitter: &mut Emitter, expected_tag: i64, fail_label: &str) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval argument for array unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // expose the array payload for the native method call
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10");                                        // compare the eval payload tag with the expected array ABI
    emitter.instruction(&format!("jne {}", fail_label));                        // reject array payloads with an incompatible ABI shape
    emitter.instruction("mov rax, rdi");                                        // place the unboxed array pointer in the result register
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

/// Boxes the current native method result as the Mixed cell expected by eval.
fn emit_box_method_result(module: &Module, emitter: &mut Emitter, return_ty: &PhpType) {
    if return_ty.codegen_repr() == PhpType::Void {
        let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(emitter, &null_symbol);
    } else {
        emit_box_current_value_as_mixed(emitter, return_ty);
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
mod catalog_tests {
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
}

#[cfg(test)]
mod source_entry_tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};
    use crate::codegen_support::source_method_adapters::source_method_adapter_symbol;
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
            match eval_method_entry(
                &module,
                "XMLParser",
                "setHandler",
                &declared,
                MethodKind::Instance,
            ) {
                EvalMethodEntry::Raw(symbol) => {
                    assert_eq!(symbol, method_symbol("XMLParser", "setHandler"));
                }
                _ => panic!("a method without generated slots must resolve to its raw entry"),
            }
        }
    }

    /// A generated collector is hidden from PHP: the visible arity drops it and the call
    /// enters through the source adapter rather than the physical method symbol.
    #[test]
    fn hidden_collector_methods_project_visible_arity_and_enter_the_source_adapter() {
        let visible = signature(vec![
            ("start".to_string(), PhpType::Callable),
            ("end".to_string(), PhpType::Callable),
        ]);
        let physical = with_generated_collector(visible.clone());
        for module in modules() {
            match eval_method_entry(
                &module,
                "Handlers",
                "setElementHandler",
                &physical,
                MethodKind::Instance,
            ) {
                EvalMethodEntry::Adapted(source, symbol) => {
                    assert_eq!(source.params.len(), visible.params.len());
                    assert_eq!(source.ref_params, visible.ref_params);
                    assert!(source.variadic.is_none());
                    assert_eq!(source.return_type, physical.return_type);
                    assert_eq!(
                        symbol,
                        source_method_adapter_symbol(
                            "Handlers",
                            "setElementHandler",
                            MethodKind::Instance
                        )
                    );
                    assert_ne!(symbol, method_symbol("Handlers", "setElementHandler"));
                }
                _ => panic!("a generated collector must resolve to its source adapter"),
            }
        }
    }

    /// An inherited implementation resolves against the class that owns the body, which is
    /// the same owner `emit_source_method_adapters()` published the adapter under, so a
    /// descendant reaches the ancestor entry instead of a symbol no emitter ever wrote.
    #[test]
    fn inherited_implementations_resolve_against_the_implementing_owner() {
        let physical =
            with_generated_collector(signature(vec![("data".to_string(), PhpType::Str)]));
        for module in modules() {
            match eval_method_entry(
                &module,
                "BaseParser",
                "parseInto",
                &physical,
                MethodKind::Instance,
            ) {
                EvalMethodEntry::Adapted(_, symbol) => {
                    assert_eq!(
                        symbol,
                        source_method_adapter_symbol(
                            "BaseParser",
                            "parseInto",
                            MethodKind::Instance
                        )
                    );
                    assert_ne!(
                        symbol,
                        source_method_adapter_symbol(
                            "DerivedParser",
                            "parseInto",
                            MethodKind::Instance
                        )
                    );
                }
                _ => panic!("an inherited collector must share the owner's source adapter"),
            }
        }
    }

    /// A source variadic keeps its raw physical entry, and the hidden actual-count form the
    /// projection refuses to bridge drops out of the bridge instead of being mis-emitted.
    #[test]
    fn source_variadics_stay_raw_and_unbridgeable_shapes_get_no_slot() {
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
                EvalMethodEntry::Raw(_)
            ));
            assert!(matches!(
                eval_method_entry(
                    &module,
                    "Collector",
                    "add",
                    &with_hidden_count,
                    MethodKind::Instance
                ),
                EvalMethodEntry::Unsupported
            ));
        }
    }

    /// A static method without generated slots keeps its declared arity and the raw static
    /// symbol, so the shared resolver leaves every ordinary static entry exactly where it was.
    #[test]
    fn plain_static_methods_keep_the_declared_signature_and_raw_symbol() {
        let declared = signature(vec![("path".to_string(), PhpType::Str)]);
        for module in modules() {
            match eval_method_entry(&module, "Loader", "fromFile", &declared, MethodKind::Static) {
                EvalMethodEntry::Raw(symbol) => {
                    assert_eq!(symbol, static_method_symbol("Loader", "fromFile"));
                    assert_ne!(symbol, method_symbol("Loader", "fromFile"));
                }
                _ => {
                    panic!("a static method without generated slots must resolve to its raw entry")
                }
            }
        }
    }

    /// A static method reading `func_get_args()` grows the same hidden collector an instance
    /// method does, so eval must validate the projected visible arity and enter the static
    /// source adapter instead of the physical symbol that still wants the collector.
    #[test]
    fn hidden_collector_static_methods_project_visible_arity_and_enter_the_source_adapter() {
        let visible = signature(vec![("label".to_string(), PhpType::Str)]);
        let physical = with_generated_collector(visible.clone());
        for module in modules() {
            match eval_method_entry(&module, "Registry", "record", &physical, MethodKind::Static) {
                EvalMethodEntry::Adapted(source, symbol) => {
                    assert_eq!(source.params.len(), visible.params.len());
                    assert_eq!(source.ref_params, visible.ref_params);
                    assert!(source.variadic.is_none());
                    assert_eq!(source.return_type, physical.return_type);
                    assert_eq!(
                        symbol,
                        source_method_adapter_symbol("Registry", "record", MethodKind::Static)
                    );
                    assert_ne!(symbol, static_method_symbol("Registry", "record"));
                    assert_ne!(
                        symbol,
                        source_method_adapter_symbol("Registry", "record", MethodKind::Instance)
                    );
                }
                _ => panic!("a generated static collector must resolve to its source adapter"),
            }
        }
    }

    /// An inherited static implementation resolves against the class that owns the body, which
    /// is the owner `emit_source_method_adapters()` published the static adapter under, so a
    /// descendant reaches the ancestor entry instead of a symbol no emitter ever wrote.
    #[test]
    fn inherited_static_implementations_resolve_against_the_implementing_owner() {
        let physical =
            with_generated_collector(signature(vec![("data".to_string(), PhpType::Str)]));
        for module in modules() {
            match eval_method_entry(&module, "BaseRegistry", "make", &physical, MethodKind::Static)
            {
                EvalMethodEntry::Adapted(_, symbol) => {
                    assert_eq!(
                        symbol,
                        source_method_adapter_symbol("BaseRegistry", "make", MethodKind::Static)
                    );
                    assert_ne!(
                        symbol,
                        source_method_adapter_symbol("DerivedRegistry", "make", MethodKind::Static)
                    );
                }
                _ => panic!("an inherited static collector must share the owner's source adapter"),
            }
        }
    }

    /// A static source variadic keeps its raw physical entry, and the hidden actual-count form
    /// the projection refuses to bridge stays fail-closed with no slot at all.
    #[test]
    fn static_source_variadics_stay_raw_and_unbridgeable_shapes_get_no_slot() {
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
                EvalMethodEntry::Raw(_)
            ));
            assert!(matches!(
                eval_method_entry(
                    &module,
                    "Aggregator",
                    "sum",
                    &with_hidden_count,
                    MethodKind::Static
                ),
                EvalMethodEntry::Unsupported
            ));
        }
    }
}
