//! Purpose:
//! Emits textual EIR for the compiler's `--emit-ir` terminal path.
//!
//! Called from:
//! - `crate::pipeline::compile()` after shared lowering and source metadata binding.
//!
//! Key details:
//! - The requested EIR optimization setting is applied before printing.
//! - Exported code receives the output kind's call-graph safety validation, as in final emission.

use std::collections::HashMap;

use super::*;

/// Optionally optimizes and prints the same prepared module used by native output.
pub(super) fn emit(
    mut module: ir::Module,
    emit_kind: Emit,
    filename: &str,
    ir_opt: bool,
    exported_functions: &HashMap<String, exports::ExportedFunction>,
    timings: &mut CompileTimings,
) {
    debug_assert!(module.source_catalog().is_some(), "textual EIR requires the prepared source catalog");
    if !exported_functions.is_empty() {
        let validate = if emit_kind == Emit::Phpext {
            exports::validate_phpext_call_graph
        } else {
            exports::validate_cdylib_call_graph
        };
        if let Err(error) = validate(&module, exported_functions) {
            crate::progress::clear();
            errors::report(&error.with_file(filename.to_string()));
            process::exit(1);
        }
    }

    crate::progress::phase("ir-opt");
    let phase_started = Instant::now();
    if ir_opt {
        ir_passes::optimize_module(&mut module);
    }
    timings.record_since("ir-opt", phase_started);

    crate::progress::phase("ir-print");
    let phase_started = Instant::now();
    let text = ir::print_module(&module);
    timings.record_since("ir-print", phase_started);
    crate::progress::clear();
    timings.report();
    print!("{}", text);
}
