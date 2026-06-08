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

use crate::codegen::platform::Target;

/// Usage string printed to stderr when command-line arguments are invalid or missing.
pub(crate) const USAGE: &str = "Usage: elephc [--target TARGET] [--heap-size=BYTES] [--gc-stats] [--heap-debug] [--emit-asm] [--check] [--timings] [--source-map] [--define SYMBOL] [--link LIB|-lLIB] [--link-path DIR|-LDIR] [--framework NAME] <source.php>";

/// Configuration derived from command-line arguments, passed to the compile pipeline.
/// Controls heap allocation size, debug output, code generation options, and linking behavior.
pub(crate) struct CliConfig {
    pub(crate) filename: String,
    pub(crate) heap_size: usize,
    pub(crate) gc_stats: bool,
    pub(crate) heap_debug: bool,
    pub(crate) emit_asm: bool,
    pub(crate) check_only: bool,
    pub(crate) emit_timings: bool,
    pub(crate) emit_source_map: bool,
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
    let mut emit_asm = false;
    let mut check_only = false;
    let mut emit_timings = false;
    let mut emit_source_map = false;
    let mut filename_arg = None;
    let mut target = Target::detect_host();
    let mut extra_link_libs: Vec<String> = Vec::new();
    let mut extra_link_paths: Vec<String> = Vec::new();
    let mut extra_frameworks: Vec<String> = Vec::new();
    let mut defines: HashSet<String> = HashSet::new();

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
        } else if arg == "--emit-asm" {
            emit_asm = true;
        } else if arg == "--check" {
            check_only = true;
        } else if arg == "--timings" {
            emit_timings = true;
        } else if arg == "--source-map" {
            emit_source_map = true;
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
    if emit_asm && check_only {
        fail("--emit-asm and --check are mutually exclusive");
    }

    CliConfig {
        filename,
        heap_size,
        gc_stats,
        heap_debug,
        emit_asm,
        check_only,
        emit_timings,
        emit_source_map,
        target,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
        defines,
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
}
