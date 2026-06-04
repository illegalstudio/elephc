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
use crate::ir::{Immediate, Module, Op};
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
