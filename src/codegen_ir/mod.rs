//! Purpose:
//! IR-consuming assembly backend. Produces functionally equivalent ASM to
//! `src/codegen/` while reading from an EIR `Module` instead of an AST.
//!
//! Called from:
//! - `crate::pipeline::compile()` when the default EIR backend, or explicit
//!   `--ir-backend`, is selected.
//!
//! Key details:
//! - EIR is the default user-facing backend.
//! - Current lowering is still 1:1, with IR optimization and register allocation
//!   planned as later passes.
//! - The legacy `src/codegen/` AST backend remains available through `--ast-backend`.

mod block_emit;
mod context;
mod fibers;
mod frame;
mod function_variants;
mod literal_defaults;
mod lower_inst;
mod lower_term;
pub mod value_placement;
mod web;

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::Emit;
use crate::exports::ExportedFunction;
use crate::codegen::runtime;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Function, Immediate, Module, Op, ValueDef};
use crate::names::{method_symbol, php_symbol_key, static_method_symbol};
use crate::types::{ClassInfo, FunctionSig, InterfaceInfo, PhpType};

/// Error returned by the Phase 04 IR backend while a required lowering path is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenIrError {
    message: String,
}

impl CodegenIrError {
    /// Creates an error for an EIR shape that is malformed or missing required metadata.
    pub(super) fn invalid_module(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Creates an error for an EIR opcode or backend option not lowered in Phase 04 yet.
    pub(super) fn unsupported(message: impl Into<String>) -> Self {
        Self {
            message: format!("unsupported EIR backend feature: {}", message.into()),
        }
    }

    /// Creates an error for a missing function-local table entry.
    pub(super) fn missing_entry(kind: &str, raw: u32) -> Self {
        Self {
            message: format!("EIR backend missing {} with id {}", kind, raw),
        }
    }
}

impl fmt::Display for CodegenIrError {
    /// Formats the backend error for CLI diagnostics.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CodegenIrError {}

/// Result type returned by IR backend entry points.
pub type Result<T> = std::result::Result<T, CodegenIrError>;

/// Generates user-code assembly from a lowered EIR module.
///
/// The Phase 04 backend currently supports straight-line scalar main programs and
/// returns explicit unsupported-feature errors for paths that are not lowered yet.
#[allow(dead_code)]
pub fn generate_user_asm_from_ir(
    module: &Module,
    gc_stats: bool,
    heap_debug: bool,
) -> Result<String> {
    let exported_functions: HashMap<String, ExportedFunction> = HashMap::new();
    generate_user_asm_from_ir_with_options(
        module,
        gc_stats,
        heap_debug,
        false,
        Emit::Executable,
        &exported_functions,
        true,
        false,
    )
}

/// Generates user-code assembly from EIR using the same artifact options as the CLI pipeline.
///
/// `regalloc_linear` selects the linear-scan register allocator; when false the
/// backend keeps every value on the stack (the `--regalloc=stack` fallback).
///
/// `web` restructures the process entry for `--web`: the top-level body becomes
/// the C-callable `_elephc_web_handler` and the real entry point becomes a thin
/// stub that calls `elephc_web_run`. When false the entry is byte-for-byte the
/// normal exit-based main.
#[allow(clippy::too_many_arguments)]
pub fn generate_user_asm_from_ir_with_options(
    module: &Module,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
    regalloc_linear: bool,
    web: bool,
) -> Result<String> {
    let mut emitter = match emit {
        Emit::Cdylib => Emitter::new_pic(module.target),
        Emit::Executable => Emitter::new(module.target),
    };
    if module.target.arch == Arch::X86_64 {
        emitter.emit_text_prelude();
    }
    let mut data = DataSection::new();
    block_emit::emit_module(
        module,
        &mut emitter,
        &mut data,
        gc_stats,
        heap_debug,
        requires_elephc_tls,
        emit,
        regalloc_linear,
        web,
    )?;
    Ok(finalize_user_asm(module, emitter, data, emit, exported_functions))
}

/// Appends literal data and the minimal user-runtime metadata needed by linked helpers.
fn finalize_user_asm(
    module: &Module,
    mut emitter: Emitter,
    data: DataSection,
    emit: Emit,
    exported_functions: &HashMap<String, ExportedFunction>,
) -> String {
    let data_output = data.emit();
    let empty_globals = HashSet::<String>::new();
    let empty_static_vars = HashMap::<(String, String), PhpType>::new();
    let user_functions = runtime_user_function_sigs(module);
    let function_variant_groups = runtime_function_variant_groups(module);
    let mut allowed_class_names = runtime_referenced_class_names(module);
    if module_uses_dynamic_callable_lookup(module) {
        allowed_class_names.extend(module.class_infos.keys().cloned());
    }
    let runtime_interfaces = runtime_referenced_interfaces(module, &allowed_class_names);
    let runtime_classes = runtime_class_infos(module);
    crate::codegen::interface_wrappers::emit_interface_return_wrappers(
        &mut emitter,
        &runtime_interfaces,
        &runtime_classes,
        Some(&allowed_class_names),
    );
    emit_intrinsic_method_wrappers(module, &mut emitter);
    if matches!(emit, Emit::Cdylib) {
        let mut sorted_exports: Vec<&ExportedFunction> = exported_functions.values().collect();
        sorted_exports.sort_by(|a, b| a.name.cmp(&b.name));
        crate::codegen::cdylib::emit_cdylib_exports(
            &mut emitter,
            module.target,
            &sorted_exports,
        );
    }
    let user_data = runtime::emit_runtime_data_user(
        &empty_globals,
        &empty_static_vars,
        &user_functions,
        &function_variant_groups,
        &runtime_interfaces,
        &runtime_classes,
        &module.enum_infos,
        Some(&allowed_class_names),
    );

    let mut user_asm = emitter.output();
    if !data_output.is_empty() {
        user_asm.push('\n');
        user_asm.push_str(&data_output);
    }
    user_asm.push('\n');
    user_asm.push_str(&user_data);
    if matches!(emit, Emit::Cdylib) && module.target.platform == Platform::Linux {
        let mut exported: HashSet<String> = exported_functions
            .values()
            .map(|export| module.target.extern_symbol(&export.name))
            .collect();
        for lifecycle in [
            "elephc_init",
            "elephc_shutdown",
            "elephc_last_error",
            "elephc_free",
        ] {
            exported.insert(module.target.extern_symbol(lifecycle));
        }
        return crate::codegen::visibility::append_hidden_directives(&user_asm, &exported);
    }
    user_asm
}

/// Returns user functions visible to runtime callable-name metadata.
fn runtime_user_function_sigs(module: &Module) -> HashMap<String, FunctionSig> {
    let mut functions = module
        .functions
        .iter()
        .filter(|function| !is_property_init_thunk_function(function))
        .map(|function| (function.name.clone(), ir_function_sig(function)))
        .collect::<HashMap<_, _>>();
    for group in function_variants::collect_dispatch_groups(module) {
        if let Some(function) = function_variants::variant_callee_for_group(module, &group.name) {
            functions
                .entry(group.name.clone())
                .or_insert_with(|| ir_function_sig(function));
        }
    }
    functions
}

/// Returns true for synthetic property-default init thunks, which are not PHP callables.
fn is_property_init_thunk_function(function: &Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Reconstructs callable metadata from an EIR function when no source signature is attached.
fn ir_function_sig(function: &Function) -> FunctionSig {
    if let Some(signature) = &function.signature {
        return signature.clone();
    }
    FunctionSig {
        params: function
            .params
            .iter()
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: vec![None; function.params.len()],
        return_type: function.return_php_type.clone(),
        declared_return: false,
        by_ref_return: false,
        ref_params: function.params.iter().map(|param| param.by_ref).collect(),
        declared_params: vec![true; function.params.len()],
        variadic: function
            .params
            .iter()
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}

/// Returns include-variant public names that runtime callable lookup must check dynamically.
fn runtime_function_variant_groups(module: &Module) -> HashSet<String> {
    function_variants::collect_dispatch_groups(module)
        .into_iter()
        .map(|group| group.name)
        .collect()
}

/// Returns true when runtime callable helpers need broad user callable metadata.
fn module_uses_dynamic_callable_lookup(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(|function| function_uses_dynamic_callable_lookup(module, function))
}

/// Returns true when one function calls `is_callable()` on a runtime-shaped value.
fn function_uses_dynamic_callable_lookup(module: &Module, function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        if !is_dynamic_callable_lookup_builtin(module, inst) || inst.operands.is_empty() {
            return false;
        }
        let Some(value) = function.value(inst.operands[0]) else {
            return false;
        };
        matches!(
            value.php_type.codegen_repr(),
            PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Iterable
        )
    })
}

/// Returns true for an EIR builtin instruction that calls PHP `is_callable()`.
fn is_dynamic_callable_lookup_builtin(module: &Module, inst: &crate::ir::Instruction) -> bool {
    if inst.op != Op::BuiltinCall {
        return false;
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(name) = module.data.function_names.get(data.as_raw() as usize) else {
        return false;
    };
    crate::names::php_symbol_key(name.trim_start_matches('\\')) == "is_callable"
}

/// Emits method-symbol wrappers for runtime-backed intrinsic class methods.
fn emit_intrinsic_method_wrappers(module: &Module, emitter: &mut Emitter) {
    for wrapper in intrinsic_method_wrapper_specs(module) {
        let symbol = if wrapper.is_static {
            static_method_symbol(&wrapper.class_name, &wrapper.method_key)
        } else {
            method_symbol(&wrapper.class_name, &wrapper.method_key)
        };
        emitter.label(&symbol);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("b {}", wrapper.helper));          // tail-call the runtime helper using the method ABI arguments
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("jmp {}", wrapper.helper));        // tail-call the runtime helper using the method ABI arguments
            }
        }
    }
}

/// Runtime-backed method wrapper that should be emitted as a PHP method symbol.
struct IntrinsicMethodWrapper {
    class_name: String,
    method_key: String,
    helper: &'static str,
    is_static: bool,
}

/// Returns intrinsic instance/static methods that need method-symbol wrappers.
fn intrinsic_method_wrapper_specs(module: &Module) -> Vec<IntrinsicMethodWrapper> {
    let eir_methods = eir_class_method_keys(module);
    let mut wrappers = Vec::new();
    for (class_name, class_info) in &module.class_infos {
        for method_key in class_info.methods.keys() {
            let impl_class = class_info
                .method_impl_classes
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if eir_methods.contains(&(impl_class.to_string(), method_key.clone(), false)) {
                continue;
            }
            if let Some(helper) = IntrinsicCall::instance_method(impl_class, method_key)
                .and_then(|intrinsic| intrinsic.runtime_helper())
            {
                wrappers.push(IntrinsicMethodWrapper {
                    class_name: impl_class.to_string(),
                    method_key: method_key.clone(),
                    helper,
                    is_static: false,
                });
            }
        }
        for method_key in class_info.static_methods.keys() {
            let impl_class = class_info
                .static_method_impl_classes
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if eir_methods.contains(&(impl_class.to_string(), method_key.clone(), true)) {
                continue;
            }
            if let Some(helper) = IntrinsicCall::static_method(impl_class, method_key)
                .and_then(|intrinsic| intrinsic.runtime_helper())
            {
                wrappers.push(IntrinsicMethodWrapper {
                    class_name: impl_class.to_string(),
                    method_key: method_key.clone(),
                    helper,
                    is_static: true,
                });
            }
        }
    }
    wrappers.sort_by(|left, right| {
        (&left.class_name, &left.method_key, left.is_static)
            .cmp(&(&right.class_name, &right.method_key, right.is_static))
    });
    wrappers.dedup_by(|left, right| {
        left.class_name == right.class_name
            && left.method_key == right.method_key
            && left.is_static == right.is_static
    });
    wrappers
}

/// Returns class metadata trimmed to method symbols emitted by the EIR backend.
fn runtime_class_infos(module: &Module) -> HashMap<String, ClassInfo> {
    let emitted_methods = emitted_class_method_keys(module);
    let mut classes = module.class_infos.clone();
    for class_info in classes.values_mut() {
        class_info.method_impl_classes.retain(|method_name, impl_class| {
            emitted_methods.contains(&(impl_class.clone(), method_name.clone(), false))
        });
        class_info
            .static_method_impl_classes
            .retain(|method_name, impl_class| {
                emitted_methods.contains(&(impl_class.clone(), method_name.clone(), true))
            });
    }
    classes
}

/// Returns classes that EIR object allocation or named `instanceof` can reference at runtime.
fn runtime_referenced_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    if module_contains_generator(module) {
        names.insert("Generator".to_string());
    }
    if module_uses_dynamic_instanceof(module) {
        names.extend(dynamic_instanceof_class_names(module));
    }
    for class_name in referenced_static_property_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_static_method_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_class_data_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_dynamic_object_new_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_class_name_lookup_builtin_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_stream_registration_class_names(module) {
        if let Some(canonical) = canonical_module_class_name(module, &class_name) {
            names.insert(canonical);
        }
    }
    for class_name in referenced_scoped_constant_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    seed_runtime_throwable_class_names(module, &mut names);
    seed_builtin_reflection_class_names(module, &mut names);
    expand_class_dependencies(&mut names, &module.class_infos);
    names
}

/// Adds builtin throwable classes that runtime helpers can materialize without EIR class references.
fn seed_runtime_throwable_class_names(module: &Module, names: &mut HashSet<String>) {
    if names.contains("Fiber") && module.class_infos.contains_key("FiberError") {
        names.insert("FiberError".to_string());
    }
    for class_name in [
        "Throwable",
        "Error",
        "TypeError",
        "ValueError",
        "Exception",
        "LogicException",
        "RuntimeException",
        "JsonException",
        "InvalidArgumentException",
        "OutOfBoundsException",
        "OutOfRangeException",
    ] {
        if module.class_infos.contains_key(class_name) {
            names.insert(class_name.to_string());
        }
    }
}

/// Adds builtin reflection classes whose objects can be materialized by metadata helpers.
fn seed_builtin_reflection_class_names(module: &Module, names: &mut HashSet<String>) {
    for class_name in [
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionFunction",
        "ReflectionParameter",
        "ReflectionNamedType",
    ] {
        if module.class_infos.contains_key(class_name) {
            names.insert(class_name.to_string());
        }
    }
}

/// Returns true when any EIR function is emitted through the generator bridge.
fn module_contains_generator(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .any(|function| function.flags.is_generator)
}

/// Returns interface metadata needed by named `instanceof` and emitted class metadata.
fn runtime_referenced_interfaces(
    module: &Module,
    class_names: &HashSet<String>,
) -> HashMap<String, InterfaceInfo> {
    let mut names = HashSet::new();
    if module_uses_dynamic_instanceof(module) {
        names.extend(dynamic_instanceof_interface_names(module));
    }
    for class_name in referenced_class_data_names(module) {
        if module.interface_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in class_names {
        if let Some(class_info) = module.class_infos.get(class_name) {
            names.extend(class_info.interfaces.iter().cloned());
        }
    }
    expand_interface_dependencies(&mut names, &module.interface_infos);
    names
        .into_iter()
        .filter_map(|name| module.interface_infos.get(&name).cloned().map(|info| (name, info)))
        .collect()
}

/// Returns whether any lowered EIR function uses dynamic `instanceof`.
fn module_uses_dynamic_instanceof(module: &Module) -> bool {
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        if function
            .instructions
            .iter()
            .any(|inst| matches!(inst.op, Op::InstanceOfDynamic))
        {
            return true;
        }
    }
    false
}

/// Returns class names safe to include in dynamic lookup metadata for the current EIR slice.
fn dynamic_instanceof_class_names(module: &Module) -> HashSet<String> {
    module
        .class_infos
        .keys()
        .filter(|name| class_metadata_supported_for_dynamic_instanceof(name, module))
        .cloned()
        .collect()
}

/// Returns interface names safe to include in dynamic lookup metadata for the current EIR slice.
fn dynamic_instanceof_interface_names(module: &Module) -> HashSet<String> {
    module
        .interface_infos
        .keys()
        .filter(|name| interface_metadata_supported_for_dynamic_instanceof(name, &module.interface_infos))
        .cloned()
        .collect()
}

/// Returns true when class metadata can be emitted for dynamic `instanceof` lookup.
fn class_metadata_supported_for_dynamic_instanceof(
    class_name: &str,
    module: &Module,
) -> bool {
    let emitted_methods = emitted_class_method_keys(module);
    let mut seen = HashSet::new();
    let mut current = Some(class_name);
    while let Some(name) = current {
        if !seen.insert(name.to_string()) {
            return false;
        }
        let Some(class_info) = module.class_infos.get(name) else {
            return false;
        };
        if !class_interfaces_supported_for_dynamic_instanceof(class_info, &module.interface_infos) {
            return false;
        }
        if !class_method_symbols_supported(class_info, name, false, &class_info.vtable_methods, &class_info.method_impl_classes, &emitted_methods) {
            return false;
        }
        if !class_method_symbols_supported(
            class_info,
            name,
            true,
            &class_info.static_vtable_methods,
            &class_info.static_method_impl_classes,
            &emitted_methods,
        ) {
            return false;
        }
        current = class_info.parent.as_deref();
    }
    true
}

/// Returns class-method symbols emitted by the EIR backend.
fn emitted_class_method_keys(module: &Module) -> HashSet<(String, String, bool)> {
    let mut keys = eir_class_method_keys(module);
    for wrapper in intrinsic_method_wrapper_specs(module) {
        keys.insert((wrapper.class_name, wrapper.method_key, wrapper.is_static));
    }
    keys
}

/// Returns class-method symbols backed by actual lowered EIR functions.
fn eir_class_method_keys(module: &Module) -> HashSet<(String, String, bool)> {
    module
        .class_methods
        .iter()
        .filter_map(|function| {
            let (class_name, method_name) = function.name.rsplit_once("::")?;
            Some((
                class_name.to_string(),
                crate::names::php_symbol_key(method_name),
                function.flags.is_static,
            ))
        })
        .collect()
}

/// Returns true when all vtable methods resolve to emitted EIR method symbols.
fn class_method_symbols_supported(
    class_info: &ClassInfo,
    fallback_class: &str,
    is_static: bool,
    methods: &[String],
    impl_classes: &HashMap<String, String>,
    emitted_methods: &HashSet<(String, String, bool)>,
) -> bool {
    methods.iter().all(|method_name| {
        let impl_class = impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        let key = (impl_class.to_string(), method_name.clone(), is_static);
        emitted_methods.contains(&key)
            || (!is_static
                && class_info.methods.contains_key(method_name)
                && emitted_methods.contains(&(impl_class.to_string(), method_name.clone(), false)))
    })
}

/// Returns true when implemented interfaces do not require missing method wrappers.
fn class_interfaces_supported_for_dynamic_instanceof(
    class_info: &ClassInfo,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> bool {
    class_info
        .interfaces
        .iter()
        .all(|name| interface_metadata_supported_for_dynamic_instanceof(name, interfaces))
}

/// Returns true when interface metadata does not require wrapper symbols missing from EIR output.
fn interface_metadata_supported_for_dynamic_instanceof(
    interface_name: &str,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![interface_name];
    while let Some(name) = stack.pop() {
        if !seen.insert(name.to_string()) {
            continue;
        }
        let Some(interface_info) = interfaces.get(name) else {
            return false;
        };
        if !interface_info.method_order.is_empty() {
            return false;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    true
}

/// Returns class names encoded in static property load/store immediates.
fn referenced_static_property_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::LoadStaticProperty | Op::StoreStaticProperty) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            if let Some(class_name) = resolve_static_property_metadata_class(module, function, class_name) {
                names.insert(class_name);
            }
            if class_name.trim_start_matches('\\') == "static" {
                names.extend(redeclared_late_static_property_classes(module, function, label));
            }
        }
    }
    names
}

/// Resolves lexical static-property receivers for runtime metadata collection.
fn resolve_static_property_metadata_class(
    module: &Module,
    function: &Function,
    class_name: &str,
) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    match class_name {
        "self" => current_function_class(function).map(str::to_string),
        "parent" => {
            let current = current_function_class(function)?;
            module.class_infos.get(current)?.parent.clone()
        }
        "static" => current_function_class(function).map(str::to_string),
        _ => Some(class_name.to_string()),
    }
}

/// Returns descendant classes that redeclare a late-bound static property label.
fn redeclared_late_static_property_classes(
    module: &Module,
    function: &Function,
    label: &str,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(base_class) = current_function_class(function) else {
        return names;
    };
    let Some((_, property)) = label.rsplit_once("::") else {
        return names;
    };
    let Some(base_info) = module.class_infos.get(base_class) else {
        return names;
    };
    let fallback_declaring_class = base_info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(base_class);
    for (class_name, class_info) in &module.class_infos {
        if !is_same_or_descendant(module, class_name, base_class) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property) else {
            continue;
        };
        if declaring_class != fallback_declaring_class {
            names.insert(declaring_class.clone());
        }
    }
    names
}

/// Returns class names encoded in static-method call immediates.
fn referenced_static_method_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::StaticMethodCall) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            if let Some(class_name) = resolve_static_method_metadata_class(module, function, class_name) {
                names.insert(class_name);
            }
        }
    }
    names
}

/// Resolves lexical static-method receivers for runtime metadata collection.
fn resolve_static_method_metadata_class(
    module: &Module,
    function: &Function,
    class_name: &str,
) -> Option<String> {
    let class_name = class_name.trim_start_matches('\\');
    match class_name {
        "self" | "static" => current_function_class(function).map(str::to_string),
        "parent" => {
            let current = current_function_class(function)?;
            module.class_infos.get(current)?.parent.clone()
        }
        _ => Some(class_name.to_string()),
    }
}

/// Returns true when `class_name` is `ancestor` or one of its descendants.
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

/// Returns the class encoded in an EIR method function name.
fn current_function_class(function: &Function) -> Option<&str> {
    function.name.rsplit_once("::").map(|(class_name, _)| class_name)
}

/// Returns class-name data entries attached to runtime object metadata opcodes.
fn referenced_class_data_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            match inst.op {
                Op::ObjectNew => {}
                Op::InstanceOf if instance_of_value_needs_runtime_metadata(function, inst) => {}
                Op::InstanceOf => continue,
                _ => continue,
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            if let Some(name) = module.data.class_names.get(data.as_raw() as usize) {
                names.insert(name.clone());
            }
        }
    }
    names
}

/// Returns class metadata needed by dynamic object factories.
fn referenced_dynamic_object_new_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if matches!(inst.op, Op::DynamicObjectNewMixed) {
                names.extend(
                    module
                        .class_infos
                        .keys()
                        .filter(|class_name| is_dynamic_new_mixed_metadata_candidate(class_name))
                        .cloned(),
                );
                continue;
            }
            if !matches!(inst.op, Op::DynamicObjectNew) {
                continue;
            }
            let Some((fallback_class, required_parent)) =
                dynamic_object_new_metadata_names(module, inst)
            else {
                continue;
            };
            names.insert(fallback_class.to_string());
            names.insert(required_parent.to_string());
            for class_name in module.class_infos.keys() {
                if is_same_or_descendant(module, class_name, required_parent) {
                    names.insert(class_name.clone());
                }
            }
        }
    }
    names
}

/// Returns true when generic `new $class` can emit static metadata for this class.
fn is_dynamic_new_mixed_metadata_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_name(class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_name(class_name)
}

/// Returns true for builtin classes with safe static allocation paths in generic dynamic new.
fn supported_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "callbackfilteriterator"
            | "domainexception"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "logicexception"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "rangeexception"
            | "recursivecallbackfilteriterator"
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionproperty"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfixedarray"
            | "splqueue"
            | "splstack"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Returns true for builtin classes that generic dynamic new must not treat as user classes.
fn known_dynamic_new_builtin_class_name(class_name: &str) -> bool {
    matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "appenditerator"
            | "arrayiterator"
            | "arrayobject"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "cachingiterator"
            | "callbackfilteriterator"
            | "directoryiterator"
            | "domainexception"
            | "emptyiterator"
            | "error"
            | "exception"
            | "fiber"
            | "fibererror"
            | "filesystemiterator"
            | "filteriterator"
            | "generator"
            | "globiterator"
            | "infiniteiterator"
            | "internaliterator"
            | "invalidargumentexception"
            | "iteratoriterator"
            | "jsonexception"
            | "lengthexception"
            | "limititerator"
            | "logicexception"
            | "multipleiterator"
            | "norewinditerator"
            | "outofboundsexception"
            | "outofrangeexception"
            | "overflowexception"
            | "parentiterator"
            | "phar"
            | "phardata"
            | "rangeexception"
            | "recursivearrayiterator"
            | "recursivecachingiterator"
            | "recursivecallbackfilteriterator"
            | "recursivedirectoryiterator"
            | "recursivefilteriterator"
            | "recursiveiteratoriterator"
            | "recursiveregexiterator"
            | "reflectionattribute"
            | "reflectionclass"
            | "reflectionmethod"
            | "reflectionproperty"
            | "regexiterator"
            | "runtimeexception"
            | "spldoublylinkedlist"
            | "splfileinfo"
            | "splfileobject"
            | "splfixedarray"
            | "splheap"
            | "splmaxheap"
            | "splminheap"
            | "splobjectstorage"
            | "splpriorityqueue"
            | "splqueue"
            | "splstack"
            | "spltempfileobject"
            | "typeerror"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "valueerror"
            | "stdclass"
    )
}

/// Parses the fallback and required-parent names from a dynamic object factory immediate.
fn dynamic_object_new_metadata_names<'a>(
    module: &'a Module,
    inst: &crate::ir::Instruction,
) -> Option<(&'a str, &'a str)> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return None;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)?
        .split_once('|')
        .map(|(fallback_class, required_parent)| {
            (
                fallback_class.trim_start_matches('\\'),
                required_parent.trim_start_matches('\\'),
            )
        })
}

/// Returns static class names that can feed `get_class()`/`get_parent_class()` lookups.
fn referenced_class_name_lookup_builtin_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::BuiltinCall) || !is_class_name_lookup_builtin(module, inst) {
                continue;
            }
            if inst.operands.is_empty() {
                if let Some(class_name) = current_function_class(function) {
                    names.insert(class_name.to_string());
                }
                continue;
            }
            for value in &inst.operands {
                let Some(metadata) = function.value(*value) else {
                    continue;
                };
                if let PhpType::Object(class_name) = metadata.php_type.codegen_repr() {
                    names.insert(class_name.trim_start_matches('\\').to_string());
                }
            }
        }
    }
    names
}

/// Returns whether an instruction is a class-name lookup builtin call.
fn is_class_name_lookup_builtin(module: &Module, inst: &crate::ir::Instruction) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(name) = module.data.function_names.get(data.as_raw() as usize) else {
        return false;
    };
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "get_class" | "get_parent_class"
    )
}

/// Returns class names passed as literals to stream wrapper/filter registration builtins.
fn referenced_stream_registration_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::BuiltinCall)
                || !is_stream_registration_builtin(module, inst)
                || inst.operands.len() < 2
            {
                continue;
            }
            if let Some(class_name) = const_string_value(module, function, inst.operands[1]) {
                names.insert(class_name.trim_start_matches('\\').to_string());
            }
        }
    }
    names
}

/// Resolves a class name against module metadata using PHP case-insensitive class rules.
fn canonical_module_class_name(module: &Module, class_name: &str) -> Option<String> {
    let wanted = php_symbol_key(class_name.trim_start_matches('\\'));
    module
        .class_infos
        .keys()
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == wanted)
        .cloned()
}

/// Returns true for builtins whose literal class argument is consumed by runtime metadata.
fn is_stream_registration_builtin(module: &Module, inst: &crate::ir::Instruction) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(name) = module.data.function_names.get(data.as_raw() as usize) else {
        return false;
    };
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "stream_wrapper_register" | "stream_filter_register"
    )
}

/// Returns the literal string payload produced by a `ConstStr` value.
fn const_string_value<'a>(
    module: &'a Module,
    function: &'a Function,
    value: crate::ir::ValueId,
) -> Option<&'a str> {
    let value_ref = function.value(value)?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return None;
    };
    let inst_ref = function.instruction(inst)?;
    if inst_ref.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns class-like receiver names encoded in scoped constant immediates.
fn referenced_scoped_constant_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for inst in &function.instructions {
            if !matches!(inst.op, Op::ScopedConstantGet) {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            let Some((class_name, _)) = label.rsplit_once("::") else {
                continue;
            };
            names.insert(class_name.trim_start_matches('\\').to_string());
        }
    }
    names
}

/// Returns whether an `instanceof` value can reach the runtime metadata matcher.
fn instance_of_value_needs_runtime_metadata(
    function: &crate::ir::Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(value) = inst.operands.first() else {
        return false;
    };
    function
        .value(*value)
        .is_some_and(|metadata| {
            matches!(
                metadata.php_type.codegen_repr(),
                PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
            )
        })
}

/// Adds parent classes needed by runtime class-id tables.
fn expand_class_dependencies(
    names: &mut HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
) {
    loop {
        let mut changed = false;
        let snapshot = names.iter().cloned().collect::<Vec<_>>();
        for class_name in snapshot {
            if let Some(parent) = classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.as_ref())
            {
                changed |= names.insert(parent.clone());
            }
        }
        if !changed {
            break;
        }
    }
}

/// Adds parent interfaces needed by runtime interface matching tables.
fn expand_interface_dependencies(
    names: &mut HashSet<String>,
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    loop {
        let mut changed = false;
        let snapshot = names.iter().cloned().collect::<Vec<_>>();
        for interface_name in snapshot {
            if let Some(interface_info) = interfaces.get(&interface_name) {
                for parent in &interface_info.parents {
                    changed |= names.insert(parent.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
}
