//! Purpose:
//! IR-consuming assembly backend. Produces functionally equivalent ASM to
//! `src/codegen/` while reading from an EIR `Module` instead of an AST.
//!
//! Called from:
//! - `crate::pipeline::compile()` when the `--ir-backend` flag is set.
//!
//! Key details:
//! - Phase 04: 1:1 lowering, no optimization, no register allocation.
//! - Phase 06 adds linear-scan register allocation.
//! - Phase 09 replaces `src/codegen/` as the default backend.

mod block_emit;
mod context;
mod frame;
mod function_variants;
mod lower_inst;
mod lower_term;
pub mod value_placement;

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime;
use crate::ir::{Function, Immediate, Module, Op};
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
pub fn generate_user_asm_from_ir(
    module: &Module,
    gc_stats: bool,
    heap_debug: bool,
) -> Result<String> {
    if gc_stats {
        return Err(CodegenIrError::unsupported("--gc-stats on the EIR backend"));
    }
    if heap_debug {
        return Err(CodegenIrError::unsupported("--heap-debug on the EIR backend"));
    }

    let mut emitter = Emitter::new(module.target);
    if module.target.arch == Arch::X86_64 {
        emitter.emit_text_prelude();
    }
    let mut data = DataSection::new();
    block_emit::emit_module(module, &mut emitter, &mut data)?;
    Ok(finalize_user_asm(module, emitter, data))
}

/// Appends literal data and the minimal user-runtime metadata needed by linked helpers.
fn finalize_user_asm(module: &Module, emitter: Emitter, data: DataSection) -> String {
    let data_output = data.emit();
    let empty_globals = HashSet::<String>::new();
    let empty_static_vars = HashMap::<(String, String), PhpType>::new();
    let empty_functions = HashMap::<String, FunctionSig>::new();
    let empty_variant_groups = HashSet::<String>::new();
    let allowed_class_names = runtime_referenced_class_names(module);
    let runtime_interfaces = runtime_referenced_interfaces(module, &allowed_class_names);
    let user_data = runtime::emit_runtime_data_user(
        &empty_globals,
        &empty_static_vars,
        &empty_functions,
        &empty_variant_groups,
        &runtime_interfaces,
        &module.class_infos,
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
    user_asm
}

/// Returns classes that EIR object allocation or named `instanceof` can reference at runtime.
fn runtime_referenced_class_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    if module_uses_dynamic_instanceof(module) {
        names.extend(dynamic_instanceof_class_names(module));
    }
    for class_name in referenced_static_property_class_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    for class_name in referenced_class_data_names(module) {
        if module.class_infos.contains_key(&class_name) {
            names.insert(class_name);
        }
    }
    expand_class_dependencies(&mut names, &module.class_infos);
    names
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
        "static" => None,
        _ => Some(class_name.to_string()),
    }
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
