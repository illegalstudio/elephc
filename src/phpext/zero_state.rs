//! Purpose:
//! Lays out zero-initialized native globals without storing their bytes in extension files.
//!
//! Called from:
//! - `crate::phpext::state::combine()` for commons and explicit BSS declarations.
//!
//! Key details:
//! - One zero-fill allocation keeps every alias at a stable address on ELF and Mach-O.
//! - Unknown BSS directives fail closed rather than escaping request isolation.

use crate::codegen_support::platform::{Arch, Platform, Target};

#[derive(Default)]
pub(super) struct ZeroState {
    size: usize,
    alignment: usize,
    symbols: Vec<(String, usize)>,
}

impl ZeroState {
    /// Reserves an aligned common symbol while detecting layout arithmetic overflow.
    pub(super) fn allocate(&mut self, name: &str, size: usize, alignment: usize) -> Result<(), String> {
        self.align(alignment)?;
        self.symbols.push((name.to_string(), self.size));
        self.advance(size)
    }

    /// Advances storage without materializing the zero bytes in assembly or the object file.
    fn advance(&mut self, size: usize) -> Result<(), String> {
        self.size = self.size.checked_add(size).ok_or("PHP extension zero-state size overflow")?;
        Ok(())
    }

    /// Preserves each global's alignment relative to an equally aligned base allocation.
    fn align(&mut self, alignment: usize) -> Result<(), String> {
        if !alignment.is_power_of_two() { return Err("invalid PHP extension zero-state alignment".into()); }
        self.alignment = self.alignment.max(alignment).max(16);
        self.size = self.size.checked_add(alignment - 1)
            .ok_or("PHP extension zero-state alignment overflow")? & !(alignment - 1);
        Ok(())
    }

    /// Classifies the compiler's explicit BSS subset, refusing nonzero or unknown storage.
    pub(super) fn bss_line(&mut self, line: &str, target: Target) -> Result<(), String> {
        let line = line.split("//").next().unwrap_or("").split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(".globl ") || line.starts_with(".global ") { return Ok(()); }
        if let Some(label) = line.strip_suffix(':') {
            return self.allocate(label, 0, 1);
        }
        let (directive, values) = line.split_once(char::is_whitespace)
            .ok_or_else(|| format!("unclassified PHP extension BSS directive: {line}"))?;
        let values = values.split(',').map(str::trim).collect::<Vec<_>>();
        let number = |value: &str| -> Result<usize, String> {
            value.strip_prefix("0x").map_or_else(|| value.parse(), |hex| usize::from_str_radix(hex, 16))
                .map_err(|_| format!("invalid PHP extension BSS integer: {value}"))
        };
        match directive {
            ".zero" | ".space" | ".skip" | ".balign" | ".p2align" | ".align" => {
                if values.len() > 2 || values.get(1).is_some_and(|value| number(value) != Ok(0)) {
                    return Err(format!("unsupported PHP extension BSS fill: {line}"));
                }
                let value = number(values[0])?;
                if matches!(directive, ".zero" | ".space" | ".skip") { return self.advance(value); }
                let exponent = directive == ".p2align" || (directive == ".align" && target.arch != Arch::X86_64);
                let alignment = if exponent {
                    1usize.checked_shl(u32::try_from(value).map_err(|_| "BSS alignment overflow")?)
                        .ok_or("BSS alignment overflow")?
                } else { value };
                self.align(alignment)
            }
            ".byte" | ".short" | ".long" | ".quad" => {
                if values.iter().any(|value| number(value) != Ok(0)) {
                    return Err(format!("nonzero PHP extension BSS initializer: {line}"));
                }
                let width = match directive { ".byte" => 1, ".short" => 2, ".long" => 4, _ => 8 };
                self.advance(values.len().checked_mul(width).ok_or("BSS size overflow")?)
            }
            _ => Err(format!("unclassified PHP extension BSS directive: {line}")),
        }
    }

    /// Emits one sparse allocation and aliases, including a stable one-past-end boundary.
    pub(super) fn render(&self, target: Target) -> String {
        let begin = target.extern_symbol("elephc_state_zero_begin");
        let end = target.extern_symbol("elephc_state_zero_end");
        let alignment = self.alignment.max(16);
        let mut out = if target.platform == Platform::MacOS {
            format!(".globl {begin}\n.zerofill __DATA,__elephc_zero,{begin},{size},{power}\n",
                size = self.size, power = alignment.trailing_zeros())
        } else {
            format!(".section .elephc_zero,\"aw\",@nobits\n.balign {alignment}\n.globl {begin}\n{begin}:\n.zero {}\n", self.size)
        };
        for (name, offset) in self.symbols.iter().map(|(name, offset)| (name.as_str(), *offset))
            .chain(std::iter::once((end.as_str(), self.size))) {
            out.push_str(&format!(".globl {name}\n"));
            if target.platform == Platform::MacOS { out.push_str(&format!(".alt_entry {name}\n")); }
            out.push_str(&format!(".set {name},{begin}+{offset}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sparse sections retain aligned symbol addresses without putting large buffers in file data.
    #[test]
    fn sparse_state_preserves_alignment_on_all_hosts() {
        for target in [Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Linux, Arch::AArch64), Target::new(Platform::MacOS, Arch::AArch64)] {
            let mut state = ZeroState::default();
            state.allocate("_first", 9, 8).unwrap();
            state.allocate("_heap", 134_217_728, 16).unwrap();
            let result = state.render(target);
            let begin = target.extern_symbol("elephc_state_zero_begin");
            assert!(result.contains(&format!(".set _heap,{begin}+16\n")));
            assert!(result.contains(&format!(".set {},{begin}+134217744\n", target.extern_symbol("elephc_state_zero_end"))));
            if target.platform == Platform::MacOS {
                assert!(result.contains(".zerofill __DATA,__elephc_zero,"));
                assert!(!result.contains(".zero "));
            } else { assert!(result.contains(".section .elephc_zero,\"aw\",@nobits")); }
        }
    }

    /// BSS parsing rejects nonzero fills, unknown directives and overflowing layouts.
    #[test]
    fn bss_storage_is_zero_and_bounded() {
        let target = Target::new(Platform::Linux, Arch::X86_64);
        let mut state = ZeroState::default();
        for line in [".p2align 3", ".globl _value", "_value:", ".quad 0, 0", ".zero 8"] {
            state.bss_line(line, target).unwrap();
        }
        assert_eq!(state.size, 24);
        assert!(state.bss_line(".quad 7", target).is_err());
        assert!(state.bss_line(".zero 8, 1", target).is_err());
        assert!(state.bss_line(".new_storage 8", target).is_err());
        state.allocate("_too_large", usize::MAX - 24, 1).unwrap();
        assert!(state.allocate("_overflow", 8, 16).is_err());
    }
}
