//! Purpose:
//! Symbol-visibility post-processing for generated artifacts: appends
//! ELF `.hidden` or Mach-O `.private_extern` directives for internal globals.
//!
//! Called from:
//! - `crate::codegen_support::driver_support::generate_runtime_with_features_pic()` for
//!   the PIC runtime object.
//! - `crate::codegen::finalize_user_asm()` for generated user objects.
//!
//! Key details:
//! - Final user assembly transfers its owned buffer so visibility suffixes do not copy the body.
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
    append_hidden_directives_with_extras(asm.to_string(), exported, platform, &[])
}

/// Appends visibility directives using the owned assembly buffer, without copying its body.
pub(crate) fn append_hidden_directives_with_extras(
    asm: String,
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
    mut asm: String,
    exported: &HashSet<String>,
    directive: &str,
    additional_internal: &[&str],
) -> String {
    let mut seen = HashSet::<String>::new();
    let mut hidden = Vec::<String>::new();
    for symbol in additional_internal {
        if !symbol.is_empty() && !exported.contains(*symbol) && seen.insert((*symbol).to_string()) {
            hidden.push((*symbol).to_string());
        }
    }
    for line in asm.lines() {
        let trimmed = line.trim_start();
        let symbol = if let Some(rest) = trimmed.strip_prefix(".globl ") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix(".comm ") {
            rest.split(',').next().unwrap_or("").trim()
        } else {
            continue;
        };
        if symbol.is_empty() || exported.contains(symbol) || !seen.insert(symbol.to_string()) {
            continue;
        }
        hidden.push(symbol.to_string());
    }
    if hidden.is_empty() {
        return asm;
    }
    if !asm.ends_with('\n') {
        asm.push('\n');
    }
    asm.push_str("\n// -- internal symbols are local to the cdylib public ABI --\n");
    for symbol in hidden {
        asm.push_str(directive);
        asm.push(' ');
        asm.push_str(&symbol);
        asm.push('\n');
    }
    asm
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
            asm.to_string(),
            &exported,
            Platform::Linux,
            &["_init", "_fini"],
        );
        assert!(out.contains(".hidden _init\n"));
        assert!(out.contains(".hidden _fini\n"));
        assert!(!out.contains(".hidden roundtrip"));
    }

    /// Every supported target reuses reserved input storage and retains exact directive order.
    #[test]
    fn owned_visibility_reuses_reserved_buffer_for_every_target() {
        use crate::codegen_support::platform::Target;

        let source = "  .globl public\npublic:\n.globl internal\n.comm storage, 64, 3\n.globl internal";
        let exported: HashSet<String> = ["public".to_string()].into_iter().collect();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let directive = if target.platform == Platform::MacOS { ".private_extern" } else { ".hidden" };
            let mut input = String::with_capacity(1024 * 1024);
            input.push_str(source);
            let original_buffer = input.as_ptr();
            let original_capacity = input.capacity();
            let output = append_hidden_directives_with_extras(
                input, &exported, target.platform, &["extra", "public", "", "extra", "internal"],
            );
            assert_eq!(output.as_ptr(), original_buffer, "{name}: body must not be copied");
            assert_eq!(output.capacity(), original_capacity, "{name}: reserved capacity must survive");
            assert_eq!(output, format!(
                "{source}\n\n// -- internal symbols are local to the cdylib public ABI --\n{directive} extra\n{directive} internal\n{directive} storage\n",
            ), "{name}: visibility and declaration order must remain exact");
        }
    }

    /// Empty inputs and fully exported inputs return the original allocation without a suffix.
    #[test]
    fn owned_visibility_without_internals_keeps_the_input_allocation() {
        let exported: HashSet<String> = ["public".to_string()].into_iter().collect();
        for source in ["", "    ret\n", ".globl public\npublic:"] {
            let mut input = String::with_capacity(4096);
            input.push_str(source);
            let original_buffer = input.as_ptr();
            let output = append_hidden_directives_with_extras(input, &exported, Platform::Linux, &[]);
            assert_eq!(output.as_ptr(), original_buffer, "unchanged assembly must not be copied");
            assert_eq!(output, source);
            assert_eq!(output.capacity(), 4096);
        }
    }
}
