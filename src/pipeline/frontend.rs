//! Purpose:
//! Reads, tokenizes, parses, and finalizes the physical source program.
//!
//! Called from:
//! - `crate::pipeline::compile()` before include and autoload resolution.
//!
//! Key details:
//! - Source mode and compiler defines remain fixed across tokenization, parsing, and finalization.

use crate::fast_hash::FastSet as HashSet;

use super::*;

/// Produces the finalized physical AST or reports the same fatal diagnostics as the CLI pipeline.
pub(super) fn read_and_parse(
    filename: &str,
    source_mode: SourceMode,
    defines: &HashSet<String>,
    timings: &mut CompileTimings,
    generated_source: Option<&str>,
) -> (parser::ast::Program, crate::resolver::SourceUnit) {
    crate::progress::phase("read");
    let phase_started = Instant::now();
    let source = match generated_source.map(str::to_owned).map(Ok)
        .unwrap_or_else(|| crate::source::read_physical_source(filename)) {
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
    let mut parsed = match parser::parse_with_mode(&tokens, source_mode) {
        Ok(ast) => ast,
        Err(e) => {
            crate::progress::clear();
            errors::report(&e.with_file(filename.to_string()));
            process::exit(1);
        }
    };
    if generated_source.is_some() {
        for statement in &mut parsed {
            if matches!(statement.kind, parser::ast::StmtKind::ExternFunctionDecl { .. }) {
                statement.source_mode = SourceMode::Internal;
            }
        }
    }
    timings.record_since("parse", phase_started);

    crate::progress::phase("magic-constants");
    let phase_started = Instant::now();
    let main_file_path = Path::new(filename).to_path_buf();
    let parsed = match crate::source::finalize_physical_program(
        parsed,
        &source,
        &main_file_path,
        // The embedded host owns extern declarations, but its parsed PHP statements keep
        // their PHP source profile so eval still receives the user's --strict-php setting.
        if generated_source.is_some() { SourceMode::Internal } else { source_mode },
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
    let unit = crate::resolver::SourceUnit {
        canonical_path: crate::source::canonical(&main_file_path).unwrap_or(main_file_path),
        mode: if generated_source.is_some() { SourceMode::Internal } else { source_mode },
        source: source.into(),
    };
    (parsed, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_source_snapshot_precedes_magic_constant_substitution() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("elephc_entry_source_{}_{nonce}.php", std::process::id()));
        let source = "<?php return __FILE__;";
        std::fs::write(&path, source).unwrap();
        let canonical = path.canonicalize().unwrap();
        let mut timings = CompileTimings::new(false);
        let (parsed, unit) = read_and_parse(
            path.to_str().unwrap(), SourceMode::Php, &HashSet::default(), &mut timings, None,
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(unit.canonical_path, canonical);
        assert_eq!(unit.mode, SourceMode::Php);
        assert_eq!(&*unit.source, source);
        assert!(!format!("{parsed:?}").contains("__FILE__"));
    }
}
