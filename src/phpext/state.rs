//! Purpose:
//! Places every native writable datum in one request snapshot region.
//!
//! Called from:
//! - The producer linker before assembling its combined user/runtime object.
//!
//! Key details:
//! - Native pointers retain stable addresses while requests exchange complete snapshots under a lock.
//! - Commons become ordinary storage, so no allocator or exception global can escape the snapshot.

use crate::codegen_support::platform::{Platform, Target};

/// Combines assembly units into a single, explicitly bounded writable state region.
pub fn combine(units: &[&str], target: Target) -> Result<String, String> {
    let section = match target.platform {
        Platform::Linux => ".section .elephc_state,\"aw\",@progbits",
        Platform::MacOS => ".section __DATA,__elephc_state",
        _ => return Err("unsupported PHP extension state target".to_string()),
    };
    let mut out = format!("{section}\n.p2align 4\n.globl {}\n{}:\n",
        target.extern_symbol("elephc_state_begin"), target.extern_symbol("elephc_state_begin"));
    let mut commons = String::new();
    for unit in units {
        // AArch64 user units rely on the assembler's default text section.
        out.push_str(".text\n");
        let mut in_state = false;
        for line in unit.lines() {
            let trimmed = line.trim();
            if let Some(comm) = trimmed.strip_prefix(".comm ") {
                let fields = comm.split(',').map(str::trim).collect::<Vec<_>>();
                if fields.len() != 3 || fields[1].parse::<usize>().is_err() {
                    return Err(format!("unsupported native common declaration: {line}"));
                }
                let align = fields[2].parse::<u32>()
                    .map_err(|_| format!("invalid native common alignment: {line}"))?;
                let bytes = if target.platform == Platform::MacOS {
                    1usize.checked_shl(align).ok_or("native common alignment overflow")?
                } else { align as usize };
                if !bytes.is_power_of_two() {
                    return Err(format!("invalid native common alignment: {line}"));
                }
                let alternate = if target.platform == Platform::MacOS {
                    format!(".alt_entry {}\n", fields[0])
                } else { String::new() };
                commons.push_str(&format!(".balign {bytes}\n.globl {symbol}\n{alternate}{symbol}:\n.zero {size}\n",
                    symbol = fields[0], size = fields[1]));
            } else if matches!(trimmed, ".data" | ".bss") {
                in_state = true;
                out.push_str(section);
                out.push('\n');
            } else if trimmed.starts_with(".section ") && !trimmed.starts_with(".section .text")
                && !trimmed.starts_with(".section .note.GNU-stack")
            {
                return Err(format!("unclassified native section in PHP extension: {line}"));
            } else if trimmed == ".subsections_via_symbols" {
                // Re-emitted once after grouping state labels into the initial region atom.
            } else {
                if trimmed == ".text" || trimmed.starts_with(".section ") { in_state = false; }
                if target.platform == Platform::MacOS && in_state && trimmed.ends_with(':') {
                    let symbol = trimmed.trim_end_matches(':');
                    if !symbol.starts_with('L') && !symbol.bytes().all(|b| b.is_ascii_digit()) {
                        out.push_str(&format!(".alt_entry {symbol}\n"));
                    }
                }
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out.push_str(section);
    out.push('\n');
    out.push_str(&commons);
    let end = target.extern_symbol("elephc_state_end");
    if target.platform == Platform::MacOS { out.push_str(&format!(".alt_entry {end}\n")); }
    out.push_str(&format!(".p2align 4\n.globl {}\n{}:\n",
        target.extern_symbol("elephc_state_end"), target.extern_symbol("elephc_state_end")));
    if target.platform == Platform::MacOS { out.push_str(".subsections_via_symbols\n"); }
    if target.platform == Platform::Linux { out.push_str(".section .note.GNU-stack,\"\",@progbits\n"); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Arch;

    /// Every mutable source kind participates in the bounded region on all producer targets.
    #[test]
    fn phpext_state_captures_data_and_commons() {
        for target in [Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Linux, Arch::AArch64), Target::new(Platform::MacOS, Arch::AArch64)] {
            let align = if target.platform == Platform::MacOS { 3 } else { 8 };
            let source = format!(".text\nf:\n ret\n.comm _heap, 128, {align}\n.data\nx:\n.quad 7\n");
            let result = combine(&[&source], target).unwrap();
            assert!(!result.contains(".comm "));
            assert!(result.contains("_heap:\n.zero 128"));
            assert!(result.contains(".balign 8\n.globl _heap\n"));
            if target.platform == Platform::MacOS {
                assert!(result.contains(".alt_entry _heap\n_heap:"));
                assert!(result.contains(".alt_entry x\nx:"));
                assert!(result.ends_with(".subsections_via_symbols\n"));
            }
            assert!(result.contains("elephc_state_begin:"));
            assert!(result.contains("elephc_state_end:"));
            assert!(!result.contains("\n.data\n"));
        }
    }

    /// New storage sections fail closed until the producer explicitly classifies them.
    #[test]
    fn phpext_state_refuses_unclassified_storage() {
        assert!(combine(&[".section .new_mutable,\"aw\"\n"], Target::detect_host()).is_err());
    }

    /// Each unit starts in executable text even when its first function has no section directive.
    #[test]
    fn phpext_aarch64_user_units_begin_in_text() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let asm = combine(&[".globl _function\n_function:\nret\n.data\nvalue:\n.quad 1\n"], target).unwrap();
        assert!(asm.contains(".text\n.globl _function\n_function:"));
        assert!(!asm.contains(".alt_entry _function"));
        assert!(asm.contains(".alt_entry value"));
    }
}
