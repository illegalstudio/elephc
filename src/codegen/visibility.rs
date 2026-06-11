//! Purpose:
//! ELF symbol-visibility post-processing for `--emit cdylib` artifacts: appends
//! `.hidden` directives for every internal global symbol so the dynamic linker
//! binds them locally instead of treating them as preemptible exports.
//!
//! Called from:
//! - `crate::codegen::driver_support::generate_runtime_with_features_pic()` for
//!   the PIC runtime object.
//! - `crate::codegen::generate_user_asm()` for cdylib user objects.
//!
//! Key details:
//! - Hidden visibility is what makes the cdylib correct and linkable: internal
//!   globals (`.comm` buffers, data-section labels, runtime routines) must not
//!   be preemptible, otherwise two elephc modules loaded into one process would
//!   alias each other's runtime state through dynamic-symbol interposition, and
//!   pre-sweep direct PC-relative references would be rejected by the linker.
//! - Only the export allowlist (lifecycle entry points plus `#[Export]`
//!   trampolines) keeps default visibility — those are the cdylib's public ABI.
//! - ELF-only: Mach-O uses two-level namespace binding, so same-image
//!   references can never be preempted there and no directive is required.

use std::collections::HashSet;

/// Scans `asm` for `.globl`/`.comm` symbol declarations and returns the same
/// assembly with a trailing block of `.hidden` directives covering every
/// declared global except the names in `exported`. Symbol order follows the
/// first declaration so output stays deterministic for the runtime cache hash.
pub(crate) fn append_hidden_directives(asm: &str, exported: &HashSet<String>) -> String {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut hidden: Vec<&str> = Vec::new();
    for line in asm.lines() {
        let trimmed = line.trim_start();
        let symbol = if let Some(rest) = trimmed.strip_prefix(".globl ") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix(".comm ") {
            rest.split(',').next().unwrap_or("").trim()
        } else {
            continue;
        };
        if symbol.is_empty() || exported.contains(symbol) || !seen.insert(symbol) {
            continue;
        }
        hidden.push(symbol);
    }
    if hidden.is_empty() {
        return asm.to_string();
    }
    let mut out = String::with_capacity(asm.len() + hidden.len() * 16);
    out.push_str(asm);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n// -- internal symbols are hidden so the cdylib exports only its public ABI --\n");
    for symbol in hidden {
        out.push_str(".hidden ");
        out.push_str(symbol);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `.globl` and `.comm` declarations gain `.hidden`
    /// directives while exported names keep default visibility.
    #[test]
    fn hides_internal_globals_but_not_exports() {
        let asm = ".globl elephc_init\nelephc_init:\n.globl _fn_add\n_fn_add:\n.comm _concat_buf, 65536, 3\n";
        let exported: HashSet<String> = ["elephc_init".to_string()].into_iter().collect();
        let out = append_hidden_directives(asm, &exported);
        assert!(out.contains(".hidden _fn_add\n"));
        assert!(out.contains(".hidden _concat_buf\n"));
        assert!(!out.contains(".hidden elephc_init"));
    }

    /// Verifies that duplicate declarations of the same symbol emit a single
    /// `.hidden` directive and that symbol order follows first declaration.
    #[test]
    fn deduplicates_and_preserves_declaration_order() {
        let asm = ".globl _b\n_b:\n.globl _a\n_a:\n.globl _b\n";
        let out = append_hidden_directives(asm, &HashSet::new());
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
        assert_eq!(append_hidden_directives(asm, &HashSet::new()), asm);
    }
}
