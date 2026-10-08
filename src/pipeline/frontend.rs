//! Purpose:
//! Reads, tokenizes, parses, and finalizes the physical source program.
//!
//! Called from:
//! - `crate::pipeline::compile()` before include and autoload resolution.
//!
//! Key details:
//! - Source mode and compiler defines remain fixed across tokenization, parsing, and finalization.

use std::collections::HashSet;

use super::*;

/// Produces the finalized physical AST or reports the same fatal diagnostics as the CLI pipeline.
pub(super) fn read_and_parse(
    filename: &str,
    source_mode: SourceMode,
    defines: &HashSet<String>,
    timings: &mut CompileTimings,
) -> parser::ast::Program {
    crate::progress::phase("read");
    let phase_started = Instant::now();
    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            crate::progress::clear();
            eprintln!("Error reading '{}': {}", filename, e);
            process::exit(1);
        }
    };
    timings.record_since("read", phase_started);

    crate::progress::phase("tokenize");
    let phase_started = Instant::now();
    let tokens = match lexer::tokenize_with_mode(&source, source_mode) {
        Ok(tokens) => tokens,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("tokenize", phase_started);

    crate::progress::phase("parse");
    let phase_started = Instant::now();
    let parsed = match parser::parse_with_mode(&tokens, source_mode) {
        Ok(ast) => ast,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    timings.record_since("parse", phase_started);

    crate::progress::phase("magic-constants");
    let phase_started = Instant::now();
    let main_file_path = Path::new(filename).to_path_buf();
    let parsed = match crate::source::finalize_physical_program(
        parsed,
        &source,
        &main_file_path,
        source_mode,
        &defines,
    ) {
        Ok(parsed) => parsed,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e);
            process::exit(1);
        }
    };
    timings.record_since("magic-constants", phase_started);
    parsed
}
