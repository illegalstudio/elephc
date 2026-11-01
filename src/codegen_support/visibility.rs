//! Purpose:
//! Symbol-visibility post-processing for `--emit cdylib` artifacts: appends
//! ELF `.hidden` or Mach-O `.private_extern` directives for internal globals.
//!
//! Called from:
//! - `crate::codegen_support::driver_support::generate_runtime_with_features_pic()` for
//!   the PIC runtime object.
//! - `crate::codegen::finalize_user_asm()` for cdylib user objects.
//!
//! Key details:
//! - Hidden visibility is what makes the cdylib correct and linkable: internal
//!   globals (`.comm` buffers, data-section labels, runtime routines) must not
//!   be preemptible, otherwise two elephc modules loaded into one process would
//!   alias each other's runtime state through dynamic-symbol interposition, and
//!   pre-sweep direct PC-relative references would be rejected by the linker.
//! - Only the export allowlist (lifecycle entry points plus `#[Export]`
//!   trampolines) keeps default visibility — those are the cdylib's public ABI.
//! - The ELF directive is `.hidden`, the Mach-O one `.private_extern`. Mach-O never needed one
//!   for CORRECTNESS — two-level namespace binding means a same-image reference cannot be
//!   preempted — but it needs one for SIZE: every `.globl` in a Mach-O image is an export, hence
//!   a `-dead_strip` root, so without marking, dead stripping a dylib collects nothing.

use std::collections::HashSet;

/// Scans `asm` for `.globl`/`.comm` symbol declarations and returns the same
/// assembly with a trailing block of visibility directives covering every
/// declared global except the names in `exported`. Symbol order follows the
/// first declaration so output stays deterministic for the runtime cache hash.
pub(crate) fn append_hidden_directives(
    asm: &str,
    exported: &HashSet<String>,
    platform: crate::codegen_support::platform::Platform,
) -> String {
    append_hidden_directives_with_extras(asm, exported, platform, &[])
}

/// Appends platform visibility directives for declared globals plus linker-supplied internals.
pub(crate) fn append_hidden_directives_with_extras(
    asm: &str,
    exported: &HashSet<String>,
    platform: crate::codegen_support::platform::Platform,
    additional_internal: &[&str],
) -> String {
    let directive = match platform {
        crate::codegen_support::platform::Platform::MacOS => ".private_extern",
        _ => ".hidden",
    };
    append_visibility_directives(asm, exported, directive, additional_internal)
}

/// Scans global declarations and appends one deterministic visibility directive per symbol.
fn append_visibility_directives(
    asm: &str,
    exported: &HashSet<String>,
    directive: &str,
    additional_internal: &[&str],
) -> String {
    let hidden = hidden_symbols(&[asm], exported, additional_internal);
    let suffix = visibility_suffix(asm.ends_with('\n'), &hidden, directive);
    let mut out = String::with_capacity(asm.len() + suffix.len());
    out.push_str(asm);
    out.push_str(&suffix);
    out
}

/// Returns only the block `append_hidden_directives_with_extras` would append to the
/// concatenation of `pieces` (which must each end on a line boundary, except the last).
///
/// The user program's text is 1.5 GB on the Symfony build; returning a copy of it with the
/// footer appended kept two copies live. `UserAssembly` appends this suffix while streaming.
pub(crate) fn hidden_directives_suffix(
    pieces: &[&str],
    exported: &HashSet<String>,
    platform: crate::codegen_support::platform::Platform,
    additional_internal: &[&str],
) -> String {
    let directive = match platform {
        crate::codegen_support::platform::Platform::MacOS => ".private_extern",
        _ => ".hidden",
    };
    let hidden = hidden_symbols(pieces, exported, additional_internal);
    let ends_with_newline = pieces
        .iter()
        .rev()
        .find(|piece| !piece.is_empty())
        .map_or(true, |piece| piece.ends_with('\n'));
    visibility_suffix(ends_with_newline, &hidden, directive)
}

/// Returns the declared global symbols to hide, deduplicated in first-declaration order.
fn hidden_symbols(
    pieces: &[&str],
    exported: &HashSet<String>,
    additional_internal: &[&str],
) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut hidden = Vec::<String>::new();
    for symbol in additional_internal {
        if !symbol.is_empty() && !exported.contains(*symbol) && seen.insert((*symbol).to_string()) {
            hidden.push((*symbol).to_string());
        }
    }
    // The directives are found with `memmem` rather than by walking every line: on the 2.8 GB
    // Symfony demo listing the line walk with its per-line trim took 1.2 s of serial codegen.
    for piece in pieces {
        for (symbol, _) in declared_symbols(piece) {
            if symbol.is_empty() || exported.contains(symbol) || !seen.insert(symbol.to_string()) {
                continue;
            }
            hidden.push(symbol.to_string());
        }
    }
    hidden
}

/// The symbols `piece` declares with a line-leading `.globl` or `.comm`, in text order.
fn declared_symbols(piece: &str) -> Vec<(&str, usize)> {
    let bytes = piece.as_bytes();
    let line_start_ok = |at: usize| {
        bytes[..at]
            .iter()
            .rev()
            .take_while(|byte| **byte != b'\n')
            .all(|byte| *byte == b' ' || *byte == b'\t')
    };
    let line_end = |from: usize| memchr::memchr(b'\n', &bytes[from..]).map_or(bytes.len(), |at| from + at);
    let mut found = Vec::new();
    for at in memchr::memmem::find_iter(bytes, b".globl ") {
        if line_start_ok(at) {
            let rest = &piece[at + ".globl ".len()..line_end(at)];
            found.push((rest.trim(), at));
        }
    }
    for at in memchr::memmem::find_iter(bytes, b".comm ") {
        if line_start_ok(at) {
            let rest = &piece[at + ".comm ".len()..line_end(at)];
            found.push((rest.split(',').next().unwrap_or("").trim(), at));
        }
    }
    found.sort_unstable_by_key(|(_, at)| *at);
    found
}

/// Formats the visibility footer for `hidden`, empty when there is nothing to hide.
fn visibility_suffix(ends_with_newline: bool, hidden: &[String], directive: &str) -> String {
    if hidden.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(hidden.len() * 24 + 64);
    if !ends_with_newline {
        out.push('\n');
    }
    out.push_str("\n// -- internal symbols are local to the cdylib public ABI --\n");
    for symbol in hidden {
        out.push_str(directive);
        out.push(' ');
        out.push_str(symbol);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Platform;

    /// Verifies that `.globl` and `.comm` declarations gain `.hidden`
    /// directives while exported names keep default visibility.
    #[test]
    fn hides_internal_globals_but_not_exports() {
        let asm = ".globl elephc_init\nelephc_init:\n.globl _fn_add\n_fn_add:\n.comm _concat_buf, 65536, 3\n";
        let exported: HashSet<String> = ["elephc_init".to_string()].into_iter().collect();
        let out = append_hidden_directives(asm, &exported, Platform::Linux);
        assert!(out.contains(".hidden _fn_add\n"));
        assert!(out.contains(".hidden _concat_buf\n"));
        assert!(!out.contains(".hidden elephc_init"));
    }

    /// Verifies that duplicate declarations of the same symbol emit a single
    /// `.hidden` directive and that symbol order follows first declaration.
    #[test]
    fn deduplicates_and_preserves_declaration_order() {
        let asm = ".globl _b\n_b:\n.globl _a\n_a:\n.globl _b\n";
        let out = append_hidden_directives(asm, &HashSet::new(), Platform::Linux);
        let b = out.find(".hidden _b").expect("hidden _b present");
        let a = out.find(".hidden _a").expect("hidden _a present");
        assert!(b < a, "first-declared symbol must be hidden first");
        assert_eq!(out.matches(".hidden _b").count(), 1);
    }

    /// Verifies that assembly without any global declarations is returned
    /// unchanged, with no trailing directive block appended.
    #[test]
    fn leaves_asm_without_globals_untouched() {
        let asm = "    mov x0, #0\n    ret\n";
        assert_eq!(append_hidden_directives(asm, &HashSet::new(), Platform::Linux), asm);
    }

    /// Uses Mach-O private extern visibility while preserving public symbols.
    #[test]
    fn privatizes_macho_internal_globals() {
        let asm = ".globl _roundtrip\n_roundtrip:\n.globl __rt_heap_alloc\n__rt_heap_alloc:\n";
        let exported: HashSet<String> = ["_roundtrip".to_string()].into_iter().collect();
        let out = append_hidden_directives(asm, &exported, Platform::MacOS);
        assert!(out.contains(".private_extern __rt_heap_alloc\n"));
        assert!(!out.contains(".private_extern _roundtrip"));
    }

    /// Hides ELF CRT globals that the compiler driver adds after assembly.
    #[test]
    fn hides_additional_linker_globals_without_assembly_definitions() {
        let asm = ".globl roundtrip\nroundtrip:\n";
        let exported: HashSet<String> = ["roundtrip".to_string()].into_iter().collect();
        let out = append_hidden_directives_with_extras(
            asm,
            &exported,
            Platform::Linux,
            &["_init", "_fini"],
        );
        assert!(out.contains(".hidden _init\n"));
        assert!(out.contains(".hidden _fini\n"));
        assert!(!out.contains(".hidden roundtrip"));
    }
}
