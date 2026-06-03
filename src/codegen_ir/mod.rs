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
use crate::ir::Module;
use crate::types::{ClassInfo, EnumInfo, FunctionSig, InterfaceInfo, PhpType};

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
    Ok(finalize_user_asm(emitter, data))
}

/// Appends literal data and the minimal user-runtime metadata needed by linked helpers.
fn finalize_user_asm(emitter: Emitter, data: DataSection) -> String {
    let data_output = data.emit();
    let empty_globals = HashSet::<String>::new();
    let empty_static_vars = HashMap::<(String, String), PhpType>::new();
    let empty_functions = HashMap::<String, FunctionSig>::new();
    let empty_variant_groups = HashSet::<String>::new();
    let empty_interfaces = HashMap::<String, InterfaceInfo>::new();
    let empty_classes = HashMap::<String, ClassInfo>::new();
    let empty_enums = HashMap::<String, EnumInfo>::new();
    let user_data = runtime::emit_runtime_data_user(
        &empty_globals,
        &empty_static_vars,
        &empty_functions,
        &empty_variant_groups,
        &empty_interfaces,
        &empty_classes,
        &empty_enums,
        None,
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
