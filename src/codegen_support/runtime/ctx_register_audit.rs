//! Purpose:
//! Audits the generated runtime's reserved context register without mistaking frame saves for scratch.
//!
//! Called from:
//! - `super::tests::ctx_register_scratch_offenders()` and negative controls in this module.
//!
//! Key details:
//! - Frame reloads require a preceding context save to that exact slot in the same global helper.
//! - Fiber switch is the explicit boundary that may restore another context from non-stack storage.
//! - This shape audit complements, but does not replace, executable foreign-register preservation tests.

use super::Arch;
use std::collections::HashSet;

/// Identifies frame-backed addresses rather than arbitrary heap loads that overwrite the context.
fn is_frame(address: &str) -> bool {
    ["[sp", "[rsp", "[rbp", "[x29"].iter().any(|base| address.starts_with(base))
}

/// Finds exact assembly-symbol tokens, independent of the destination register or relocation form.
pub(super) fn symbol_references(asm: &str, symbol: &str, comment: &str) -> Vec<String> {
    let identifier = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    asm.lines().filter_map(|raw| {
        let line = raw.split(comment).next().unwrap_or(raw).trim();
        let bytes = line.as_bytes();
        let found = line.match_indices(symbol).any(|(start, _)| {
            let end = start + symbol.len();
            (start == 0 || !identifier(bytes[start - 1]))
                && (end == bytes.len() || !identifier(bytes[end]))
        });
        found.then(|| line.to_owned())
    }).collect()
}

/// Returns every instruction that borrows the context register outside publication or paired saves.
pub(super) fn offenders(asm: &str, arch: Arch, context: &str, comment: &str) -> Vec<String> {
    let mut helper = "";
    let mut slots = HashSet::new();
    let mut pairs = HashSet::new();
    let mut pushed = false;
    let mut offenders = Vec::new();
    for raw in asm.lines() {
        let line = raw.trim();
        if let Some(global) = line.strip_prefix(".globl ") {
            helper = global;
            slots.clear();
            pairs.clear();
            pushed = false;
            continue;
        }
        if line.is_empty() || line.starts_with(comment) || line.starts_with('.') { continue; }
        let line = line.split(comment).next().unwrap_or(line).trim();
        if line.is_empty() || line.ends_with(':') || !super::tests::mentions_register(line, context) { continue; }
        // Coroutine entry runs inside its creator's context, not at a foreign-host boundary.
        if helper == "__rt_fiber_entry" && line.contains("_rt_ctx") {
            offenders.push(format!("{helper}: {line}"));
            continue;
        }
        let switching = helper == "__rt_fiber_switch";
        let sanctioned = match arch {
            Arch::AArch64 => {
                if let Some(pair) = line.strip_prefix("stp ").filter(|pair| pair.contains(", [sp")) {
                    pairs.insert(pair.to_owned());
                    true
                } else if let Some(pair) = line.strip_prefix("ldp ").filter(|pair| pair.contains(", [sp")) {
                    pairs.contains(pair)
                } else if let Some(address) = line.strip_prefix("str x28, ").or_else(|| line.strip_prefix("stur x28, ")).filter(|address| is_frame(address)) {
                    slots.insert(address.to_owned());
                    true
                } else if let Some(address) = line.strip_prefix("ldr x28, ").or_else(|| line.strip_prefix("ldur x28, ")).filter(|address| is_frame(address)) {
                    slots.contains(address)
                } else {
                    (switching && (line.starts_with("stp x27, x28,") || line.starts_with("ldp x27, x28,")))
                        || (helper == "__rt_fiber_construct" && line == "str x28, [x10, #72]")
                        || line.starts_with("adrp x28, _rt_ctx")
                        || line.starts_with("add x28, x28, _rt_ctx")
                        || line == "add x28, x28, :lo12:_rt_ctx"
                        || line == "mov x28, x0"
                        || ((line.starts_with("ldr ") || line.starts_with("str ")) && line.contains("[x28, #") && !line.starts_with("ldr x28"))
                        || (line.starts_with("mov x") && line.ends_with(", x28"))
                        || (line.starts_with("add x") && line.contains(", x28, #"))
                }
            }
            Arch::X86_64 => {
                if line == "push r14" { pushed = true; true }
                else if line == "pop r14" { pushed }
                else if let Some(address) = line.strip_prefix("mov QWORD PTR ").and_then(|tail| tail.strip_suffix(", r14")).filter(|address| is_frame(address)) {
                    slots.insert(address.to_owned());
                    true
                } else if let Some(address) = line.strip_prefix("mov r14, QWORD PTR ") {
                    switching || slots.contains(address)
                } else {
                    line == "mov r14, rdi"
                        || line.starts_with("lea r14, [rip + _rt_ctx]")
                        || line.ends_with(", r14")
                        || (line.contains("[r14 +") && !line.starts_with("mov r14,") && !line.starts_with("lea r14,"))
                }
            }
        };
        if !sanctioned { offenders.push(format!("{helper}: {line}")); }
    }
    offenders
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Correct frame pairs pass, but a frame slot containing another register is not a ctx restore.
    #[test]
    fn context_audit_distinguishes_paired_frame_saves_from_borrowed_registers() {
        let x86 = ".globl __rt_probe\n__rt_probe:\nmov QWORD PTR [rbp - 8], r14\nmov r14, QWORD PTR [rbp - 8]\n";
        assert!(offenders(x86, Arch::X86_64, "r14", "#").is_empty());
        let wrong_slot = ".globl __rt_probe\n__rt_probe:\nmov QWORD PTR [rbp - 88], r15\nmov r14, QWORD PTR [rbp - 88]\n";
        assert_eq!(offenders(wrong_slot, Arch::X86_64, "r14", "#").len(), 1);
        let other_helper = format!("{x86}.globl __rt_other\n__rt_other:\nmov r14, QWORD PTR [rbp - 8]\n");
        assert_eq!(offenders(&other_helper, Arch::X86_64, "r14", "#").len(), 1);
        let arm = ".globl __rt_probe\n__rt_probe:\nstp x19, x28, [sp, #16]\nldp x19, x28, [sp, #16]\nstp x28, x29, [sp, #224]\nldp x28, x29, [sp, #224]\n";
        assert!(offenders(arm, Arch::AArch64, "x28", "//").is_empty());
        let wrong_pair = ".globl __rt_probe\n__rt_probe:\nstp x19, x20, [sp, #16]\nldp x19, x28, [sp, #16]\nmov x28, x19\n";
        assert_eq!(offenders(wrong_pair, Arch::AArch64, "x28", "//").len(), 2);
        let disguised_scratch = ".globl __rt_probe\n__rt_probe:\nmov r14, QWORD PTR [r14 + 8]\n";
        assert_eq!(offenders(disguised_scratch, Arch::X86_64, "r14", "#").len(), 1);
        let wrong_entry = ".globl __rt_fiber_entry\n__rt_fiber_entry:\nadrp x28, _rt_ctx\nadd x28, x28, _rt_ctx@PAGEOFF\n";
        assert_eq!(offenders(wrong_entry, Arch::AArch64, "x28", "//").len(), 2);
        let wrong_entry_x86 = ".globl __rt_fiber_entry\n__rt_fiber_entry:\nlea r14, [rip + _rt_ctx]\n";
        assert_eq!(offenders(wrong_entry_x86, Arch::X86_64, "r14", "#").len(), 1);
    }

    /// Every supported target's all-feature runtime respects the same reserved-register contract.
    #[test]
    fn context_register_audit_covers_every_supported_target() {
        use crate::codegen_support::{driver_support::generate_runtime_with_features, platform::Target, RuntimeFeatures};
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let asm = generate_runtime_with_features(8 * 1024 * 1024, target, RuntimeFeatures::all());
            let context = if target.arch == Arch::AArch64 { "x28" } else { "r14" };
            let failures = offenders(&asm, target.arch, context, target.line_comment_prefix());
            assert!(failures.is_empty(), "{name}: {}", failures.join("\n"));
        }
    }

    /// Legacy-state detection includes non-x9 loads and GOT references, without matching helper names.
    #[test]
    fn legacy_symbol_audit_is_register_and_relocation_independent() {
        let arm = "adrp x19, _heap_small_bins\nldr x0, [x28, #8] // _heap_small_bins\n.globl __rt_heap_small_bins_helper\n";
        assert_eq!(symbol_references(arm, "_heap_small_bins", "//"), ["adrp x19, _heap_small_bins"]);
        let x86 = "mov r8, QWORD PTR _heap_small_bins@GOTPCREL[rip]\nlea r9, [rip + _heap_small_bins]\n# _heap_small_bins\n";
        assert_eq!(symbol_references(x86, "_heap_small_bins", "#").len(), 2);
    }
}
