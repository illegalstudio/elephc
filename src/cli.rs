//! Purpose:
//! Owns command-line argument parsing for compiler options and target selection.
//! Converts user flags into a single configuration object for the compile pipeline.
//!
//! Called from:
//! - `crate::main()` before invoking `crate::pipeline::compile()`.
//!
//! Key details:
//! - Exits immediately on invalid CLI state so later compiler stages receive normalized options.

use std::collections::HashSet;
use std::process;

pub(crate) use crate::codegen::Emit;
use crate::codegen::platform::Target;

/// Usage string printed to stderr when command-line arguments are invalid or missing.
pub(crate) const USAGE: &str = "Usage: elephc [--target TARGET] [--heap-size=BYTES] [--gc-stats] [--heap-debug] [--emit-ir] [--ir-backend] [--ast-backend] [--emit-asm] [--emit KIND] [--check] [--null-repr=sentinel|tagged] [--regalloc=linear|stack] [--ir-opt=on|off] [--timings] [--source-map] [--define SYMBOL] [--link LIB|-lLIB] [--link-path DIR|-LDIR] [--framework NAME] <source.php>";

/// Backend selected for assembly generation after frontend and optimization passes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodegenBackend {
    Eir,
    Ast,
}

/// Configuration derived from command-line arguments, passed to the compile pipeline.
/// Controls heap allocation size, debug output, code generation options, and linking behavior.
pub(crate) struct CliConfig {
    pub(crate) filename: String,
    pub(crate) heap_size: usize,
    pub(crate) gc_stats: bool,
    pub(crate) heap_debug: bool,
    pub(crate) emit_ir: bool,
    pub(crate) backend: CodegenBackend,
    pub(crate) null_repr: crate::codegen::NullRepr,
    pub(crate) emit_asm: bool,
    pub(crate) emit: Emit,
    pub(crate) check_only: bool,
    pub(crate) emit_timings: bool,
    pub(crate) emit_source_map: bool,
    pub(crate) regalloc_linear: bool,
    pub(crate) ir_opt: bool,
    pub(crate) target: Target,
    pub(crate) extra_link_libs: Vec<String>,
    pub(crate) extra_link_paths: Vec<String>,
    pub(crate) extra_frameworks: Vec<String>,
    pub(crate) defines: HashSet<String>,
}

/// Parse command-line arguments into a CliConfig struct.
pub(crate) fn parse_args(args: &[String]) -> CliConfig {
    if args.len() < 2 {
        eprintln!("{USAGE}");
        process::exit(1);
    }

    let mut heap_size: usize = 8_388_608; // 8MB default
    let mut gc_stats = false;
    let mut heap_debug = false;
    let mut emit_ir = false;
    let mut backend = CodegenBackend::Eir;
    let mut explicit_ir_backend = false;
    let mut explicit_ast_backend = false;
    let mut emit_asm = false;
    let mut emit = Emit::Executable;
    let mut check_only = false;
    let mut emit_timings = false;
    let mut emit_source_map = false;
    let mut filename_arg = None;
    let mut target = Target::detect_host();
    let mut extra_link_libs: Vec<String> = Vec::new();
    let mut extra_link_paths: Vec<String> = Vec::new();
    let mut extra_frameworks: Vec<String> = Vec::new();
    let mut defines: HashSet<String> = HashSet::new();
    let mut null_repr = match std::env::var("ELEPHC_NULL_REPR").as_deref() {
        Ok("tagged") => crate::codegen::NullRepr::Tagged,
        Ok("sentinel") => crate::codegen::NullRepr::Sentinel,
        _ => crate::codegen::NullRepr::default(),
    };
    // The register allocator is on by default; an env override lets the test
    // harness compile the whole suite under the stack fallback for comparison.
    let mut regalloc_linear = match std::env::var("ELEPHC_REGALLOC").as_deref() {
        Ok("stack") => false,
        Ok("linear") => true,
        _ => true,
    };
    // EIR optimization passes are on by default; an env override lets the test
    // harness or a benchmark compile with the IR pass driver disabled.
    let mut ir_opt = match std::env::var("ELEPHC_IR_OPT").as_deref() {
        Ok("off") => false,
        Ok("on") => true,
        _ => true,
    };

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(val) = arg.strip_prefix("--heap-size=") {
            heap_size = parse_heap_size(val);
        } else if arg == "--target" {
            i += 1;
            target = parse_required_target(args, i);
        } else if let Some(value) = arg.strip_prefix("--target=") {
            target = parse_target(value);
        } else if arg == "--gc-stats" {
            gc_stats = true;
        } else if arg == "--heap-debug" {
            heap_debug = true;
        } else if arg == "--emit-ir" {
            emit_ir = true;
        } else if arg == "--ir-backend" {
            explicit_ir_backend = true;
            backend = CodegenBackend::Eir;
        } else if arg == "--ast-backend" {
            explicit_ast_backend = true;
            backend = CodegenBackend::Ast;
        } else if arg == "--emit-asm" {
            emit_asm = true;
        } else if arg == "--emit" {
            i += 1;
            emit = parse_required_emit(args, i);
        } else if let Some(value) = arg.strip_prefix("--emit=") {
            emit = parse_emit(value);
        } else if arg == "--check" {
            check_only = true;
        } else if arg == "--timings" {
            emit_timings = true;
        } else if arg == "--source-map" {
            emit_source_map = true;
        } else if let Some(value) = arg.strip_prefix("--null-repr=") {
            null_repr = parse_null_repr(value);
        } else if let Some(value) = arg.strip_prefix("--regalloc=") {
            regalloc_linear = parse_regalloc(value);
        } else if let Some(value) = arg.strip_prefix("--ir-opt=") {
            ir_opt = parse_ir_opt(value);
        } else if arg == "--no-ir-opt" {
            ir_opt = false;
        } else if arg == "--define" {
            i += 1;
            let symbol = required_value(args, i, "Missing symbol after --define");
            if let Err(message) = validate_define_symbol(&symbol) {
                fail(message);
            }
            defines.insert(symbol);
        } else if let Some(symbol) = arg.strip_prefix("--define=") {
            if let Err(message) = validate_define_symbol(symbol) {
                fail(message);
            }
            defines.insert(symbol.to_string());
        } else if arg == "--link" || arg == "-l" {
            i += 1;
            extra_link_libs.push(required_value(
                args,
                i,
                &format!("Missing library name after {}", arg),
            ));
        } else if let Some(lib) = arg.strip_prefix("-l") {
            extra_link_libs.push(lib.to_string());
        } else if arg == "--link-path" || arg == "-L" {
            i += 1;
            extra_link_paths.push(required_value(args, i, &format!("Missing path after {}", arg)));
        } else if let Some(path) = arg.strip_prefix("-L") {
            extra_link_paths.push(path.to_string());
        } else if arg == "--framework" {
            i += 1;
            extra_frameworks.push(required_value(
                args,
                i,
                "Missing framework name after --framework",
            ));
        } else if arg.starts_with("--") {
            fail(&format!("Unknown flag: {}", arg));
        } else {
            filename_arg = Some(arg.clone());
        }
        i += 1;
    }

    let filename = match filename_arg {
        Some(filename) => filename,
        None => {
            eprintln!("{USAGE}");
            process::exit(1);
        }
    };
    let output_modes = usize::from(emit_ir) + usize::from(emit_asm) + usize::from(check_only);
    if output_modes > 1 {
        fail("--emit-ir, --emit-asm, and --check are mutually exclusive");
    }
    if explicit_ir_backend && explicit_ast_backend {
        fail("cannot use --ir-backend and --ast-backend together");
    }
    if explicit_ast_backend {
        eprintln!(
            "warning: --ast-backend is deprecated and will be removed in v0.26.0. The EIR backend is now the default. See docs/internals/the-ir.md for details."
        );
    }

    CliConfig {
        filename,
        heap_size,
        gc_stats,
        heap_debug,
        emit_ir,
        backend,
        null_repr,
        emit_asm,
        emit,
        check_only,
        emit_timings,
        emit_source_map,
        regalloc_linear,
        ir_opt,
        target,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
    }
}

/// Parse the required emit-kind argument at the given index, or fail if missing.
fn parse_required_emit(args: &[String], index: usize) -> Emit {
    if index < args.len() {
        parse_emit(&args[index])
    } else {
        fail("Missing emit kind after --emit (expected: executable, cdylib)")
    }
}

/// Parse an emit-kind string into an `Emit` value, or fail with an error message.
fn parse_emit(value: &str) -> Emit {
    match value {
        "executable" | "exe" | "bin" => Emit::Executable,
        "cdylib" | "dylib" | "shared" => Emit::Cdylib,
        other => fail(&format!(
            "Invalid --emit kind '{}': expected one of: executable, cdylib",
            other
        )),
    }
}

/// Parse a heap size value, returning a value >= 65536 or exit with an error.
fn parse_heap_size(value: &str) -> usize {
    match value.parse::<usize>() {
        Ok(n) if n >= 65536 => n,
        _ => fail("Invalid --heap-size: must be a number >= 65536"),
    }
}

/// Parse the required target argument at the given index, or fail if missing.
fn parse_required_target(args: &[String], index: usize) -> Target {
    if index < args.len() {
        parse_target(&args[index])
    } else {
        fail("Missing target after --target")
    }
}

/// Parse a `--null-repr=` value into a NullRepr, or fail with an error message.
fn parse_null_repr(value: &str) -> crate::codegen::NullRepr {
    match value {
        "sentinel" => crate::codegen::NullRepr::Sentinel,
        "tagged" => crate::codegen::NullRepr::Tagged,
        other => fail(&format!("Unknown null representation: {}", other)),
    }
}

/// Parse a `--regalloc=` value into the linear-scan toggle, or fail.
fn parse_regalloc(value: &str) -> bool {
    match value {
        "linear" => true,
        "stack" => false,
        other => fail(&format!("Unknown register allocator: {}", other)),
    }
}

/// Parse an `--ir-opt=` value into the EIR optimization-pass toggle, or fail.
fn parse_ir_opt(value: &str) -> bool {
    match value {
        "on" => true,
        "off" => false,
        other => fail(&format!("Unknown --ir-opt value: {} (expected on|off)", other)),
    }
}

/// Parse a target string to a Target enum, or fail with an error message.
fn parse_target(value: &str) -> Target {
    match Target::parse(value) {
        Ok(target) => target,
        Err(err) => fail(&err),
    }
}

/// Retrieve a required argument at index, or fail with the given message.
fn required_value(args: &[String], index: usize, message: &str) -> String {
    if index < args.len() {
        args[index].clone()
    } else {
        fail(message)
    }
}

/// Validates an ifdef symbol supplied via `--define`, rejecting an empty symbol.
///
/// Kept pure (no IO/exit) so both the `--define SYMBOL` and `--define=SYMBOL` forms
/// can share one consistent rule and the rejection can be unit-tested.
fn validate_define_symbol(symbol: &str) -> Result<(), &'static str> {
    if symbol.is_empty() {
        return Err("Invalid --define: symbol cannot be empty");
    }
    Ok(())
}

/// Prints a message to stderr and exits the process with code 1.
/// Never returns.
fn fail(message: &str) -> ! {
    eprintln!("{}", message);
    process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an empty `--define` symbol is rejected, matching the `--define=` form,
    /// so the two spellings no longer behave inconsistently.
    #[test]
    fn empty_define_symbol_is_rejected() {
        assert!(validate_define_symbol("").is_err());
    }

    /// Verifies a normal `--define` symbol is accepted.
    #[test]
    fn non_empty_define_symbol_is_accepted() {
        assert!(validate_define_symbol("FEATURE").is_ok());
    }

    /// Verifies the canonical `--emit` spellings parse to the expected `Emit` variants.
    #[test]
    fn emit_kind_parses_canonical_spellings() {
        assert_eq!(parse_emit("executable"), Emit::Executable);
        assert_eq!(parse_emit("cdylib"), Emit::Cdylib);
    }

    /// Verifies the accepted aliases map to their canonical variants so users coming
    /// from cargo (`cdylib`/`dylib`) and unix toolchains (`shared`, `bin`) all work.
    #[test]
    fn emit_kind_accepts_aliases() {
        assert_eq!(parse_emit("exe"), Emit::Executable);
        assert_eq!(parse_emit("bin"), Emit::Executable);
        assert_eq!(parse_emit("dylib"), Emit::Cdylib);
        assert_eq!(parse_emit("shared"), Emit::Cdylib);
    }

    /// Verifies the canonical `--ir-opt=` spellings toggle the EIR optimization
    /// pass driver, with `on` enabling it and `off` disabling it.
    #[test]
    fn ir_opt_parses_on_and_off() {
        assert!(parse_ir_opt("on"));
        assert!(!parse_ir_opt("off"));
    }

    /// Verifies the register-allocator toggle parses its canonical spellings.
    #[test]
    fn regalloc_parses_linear_and_stack() {
        assert!(parse_regalloc("linear"));
        assert!(!parse_regalloc("stack"));
    }
}
