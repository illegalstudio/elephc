//! Purpose:
//! Emits textual EIR for the compiler's `--emit-ir` terminal path.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST optimization and type checking.
//!
//! Key details:
//! - The requested EIR optimization setting is applied before printing.
//! - Exported code receives the same cdylib call-graph safety validation as final emission.

use std::collections::HashMap;

use super::*;

/// Lowers, optionally optimizes, and prints one checked program as textual EIR.
pub(super) fn emit(
    ast: &parser::ast::Program,
    check_result: &types::CheckResult,
    target: Target,
    filename: &str,
    web: bool,
    ir_opt: bool,
    exported_functions: &HashMap<String, exports::ExportedFunction>,
    timings: &mut CompileTimings,
) {
    crate::progress::phase("ir-lower");
    let phase_started = Instant::now();
    let mut module = match ir_lower::lower_program_with_source_path_and_web(
        &ast,
        &check_result,
        target,
        Path::new(filename),
        web,
    ) {
        Ok(module) => module,
        Err(err) => {
            crate::progress::clear();
            eprintln!("EIR lowering error: {}", err);
            process::exit(1);
        }
    };
    timings.record_since("ir-lower", phase_started);

    if !exported_functions.is_empty() {
        if let Err(error) = exports::validate_cdylib_call_graph(&module, exported_functions) {
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
