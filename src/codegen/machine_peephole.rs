//! Purpose:
//! Late, target-aware machine peepholes over one finished function's assembly text.
//! Removes redundant moves, forwards stored frame values into adjacent reloads,
//! drops jumps to the immediately following label, and shortens AArch64 wide branches.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_blocks()` once per small or loop-carrying function body.
//!
//! Key details:
//! - Every rewrite is local and only looks through comment and blank lines. A label is a
//!   join point and any directive is opaque, so both stop a pattern.
//! - Register patterns only match full-width (64-bit) registers: 32-bit forms zero-extend.
//! - AArch64 conditional branches reach only ±1 MiB, so lowering emits far branches as an
//!   inverted short branch over an unconditional `b`. That wide form is folded back into a
//!   single conditional branch only when the target is a label of the same region and the
//!   whole region is provably smaller than the conditional range.
//! - Functions can reach millions of lines (eval-heavy programs), so lines borrow the input
//!   text, deletions are tombstones compacted once per round, and every pattern rejects an
//!   instruction by its first bytes before parsing it.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::codegen::platform::{Arch, Target};

/// Upper bound on the bytes one line of region text can assemble to, used for range proofs.
const MAX_BYTES_PER_LINE: usize = 8;
/// AArch64 `b.cond` / `cbz` / `cbnz` reach (±1 MiB), minus a safety margin.
const AARCH64_COND_BRANCH_REACH: usize = 1_000_000;
/// Bound on whole-region rewrite rounds; a second round catches patterns a first one exposed.
const MAX_ROUNDS: usize = 2;
/// Regions larger than this (about 8k lines) are left untouched. Such bodies are generated
/// dispatch code (an eval-heavy program's top level reaches millions of lines), not hot user
/// loops; skipping them keeps compile time flat. The bound is in bytes so the check is free.
const MAX_REGION_BYTES: usize = 256 << 10;
/// Loop-free regions up to this size (about 250 lines) are still optimized: small functions
/// are cheap to rewrite and are often the callees of hot loops.
pub(super) const SMALL_REGION_BYTES: usize = 8 << 10;

/// One classified assembly line, borrowing the region text unless a rewrite replaced it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line<'a> {
    /// Blank or comment-only line (transparent to every pattern).
    Trivia(&'a str),
    /// A label definition, without the trailing colon.
    Label(&'a str),
    /// One instruction, trimmed of indentation.
    Inst(Cow<'a, str>),
    /// Anything else, such as a directive; opaque to every pattern.
    Other(&'a str),
    /// A line deleted by a rewrite; skipped by every pattern and dropped on compaction.
    Removed,
}

impl Line<'_> {
    /// Renders the line back to assembly text with the emitter's indentation.
    fn render(&self, out: &mut String) {
        match self {
            Line::Trivia(text) | Line::Other(text) => out.push_str(text),
            Line::Label(name) => {
                out.push_str(name);
                out.push(':');
            }
            Line::Inst(text) => {
                out.push_str("    ");
                out.push_str(text);
            }
            Line::Removed => return,
        }
        out.push('\n');
    }
}

/// Runs every machine peephole over one function region and returns the rewritten text.
#[cfg(test)]
fn optimize(text: &str, target: Target) -> String {
    optimize_region(text, target).unwrap_or_else(|| text.to_string())
}

/// Runs every machine peephole over one region; `None` means nothing changed.
///
/// Each round is one walk that dispatches on an instruction's first byte, so lines that no
/// pattern can start with cost a byte compare. Deletions are tombstones compacted per round.
pub(super) fn optimize_region(text: &str, target: Target) -> Option<String> {
    if text.len() > MAX_REGION_BYTES {
        return None;
    }
    let arch = target.arch;
    let comment = target.line_comment_prefix();
    let mut lines: Vec<Line<'_>> = text.lines().map(|line| classify(line, comment)).collect();
    let region_labels = (arch == Arch::AArch64).then(|| shortening_labels(&lines)).flatten();
    let mut any_change = false;
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for index in 0..lines.len() {
            let Some(first) = inst_at(&lines, index).and_then(|inst| inst.bytes().next()) else {
                continue;
            };
            changed |= match first {
                b'm' | b'f' => {
                    try_remove_redundant_move(&mut lines, index, arch)
                        || try_forward_store(&mut lines, index, arch)
                }
                b's' => try_forward_store(&mut lines, index, arch),
                b'b' | b'c' | b'j' => {
                    region_labels
                        .as_ref()
                        .is_some_and(|labels| try_shorten_wide_branch(&mut lines, index, labels))
                        || try_remove_jump_to_next_label(&mut lines, index, arch)
                }
                _ => false,
            };
        }
        if !changed {
            break;
        }
        any_change = true;
        lines.retain(|line| *line != Line::Removed);
    }
    if !any_change {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    for line in &lines {
        line.render(&mut out);
    }
    Some(out)
}

/// Classifies one raw assembly line.
fn classify<'a>(line: &'a str, comment: &str) -> Line<'a> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(comment) {
        return Line::Trivia(line);
    }
    let indented = line.starts_with(' ') || line.starts_with('\t');
    if !indented {
        if let Some(name) = trimmed.strip_suffix(':') {
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                return Line::Label(name);
            }
        }
        return Line::Other(line);
    }
    if trimmed.starts_with('.') {
        return Line::Other(line);
    }
    Line::Inst(Cow::Borrowed(trimmed))
}

/// Returns the index of the next non-trivia, non-removed line after `index`.
fn next_significant(lines: &[Line<'_>], index: usize) -> Option<usize> {
    (index + 1..lines.len()).find(|&i| !matches!(lines[i], Line::Trivia(_) | Line::Removed))
}

/// Returns the instruction text at `index`, if that line is an instruction.
fn inst_at<'l>(lines: &'l [Line<'_>], index: usize) -> Option<&'l str> {
    match &lines[index] {
        Line::Inst(text) => Some(text),
        _ => None,
    }
}

/// Splits `mnemonic a, b` into its mnemonic and two operands.
fn two_operands(inst: &str) -> Option<(&str, &str, &str)> {
    let (mnemonic, rest) = inst.split_once(' ')?;
    let (first, second) = rest.split_once(',')?;
    Some((mnemonic, first.trim(), second.trim()))
}

/// Returns true for a 64-bit general-purpose register name the backend may move freely.
fn is_gpr64(arch: Arch, reg: &str) -> bool {
    match arch {
        Arch::AArch64 => reg
            .strip_prefix('x')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| n <= 30),
        Arch::X86_64 => matches!(
            reg,
            "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "r8" | "r9" | "r10" | "r11" | "r12"
                | "r13" | "r14" | "r15"
        ),
    }
}

/// Returns true for a scalar double register name.
fn is_fpr(arch: Arch, reg: &str) -> bool {
    match arch {
        Arch::AArch64 => reg
            .strip_prefix('d')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| n <= 31),
        Arch::X86_64 => reg
            .strip_prefix("xmm")
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| n <= 15),
    }
}

/// Parses a full-width register-to-register move as `(dst, src)`.
fn reg_move(arch: Arch, inst: &str) -> Option<(&str, &str)> {
    if !(inst.starts_with("mov") || inst.starts_with("fmov")) {
        return None;
    }
    let (mnemonic, dst, src) = two_operands(inst)?;
    let valid = match (arch, mnemonic) {
        (_, "mov") => is_gpr64(arch, dst) && is_gpr64(arch, src),
        (Arch::AArch64, "fmov") => is_fpr(arch, dst) && is_fpr(arch, src),
        (Arch::X86_64, "movsd") => is_fpr(arch, dst) && is_fpr(arch, src),
        _ => false,
    };
    valid.then_some((dst, src))
}

/// Deletes a self-move at `index`, or the second move of a `mov a, b` / `mov b, a` pair.
fn try_remove_redundant_move(lines: &mut [Line<'_>], index: usize, arch: Arch) -> bool {
    let Some((dst, src)) = inst_at(lines, index).and_then(|inst| reg_move(arch, inst)) else {
        return false;
    };
    if dst == src {
        lines[index] = Line::Removed;
        return true;
    }
    let move_back = next_significant(lines, index).filter(|&next| {
        inst_at(lines, next).and_then(|inst| reg_move(arch, inst)) == Some((src, dst))
    });
    let Some(next) = move_back else {
        return false;
    };
    // After `dst = src`, copying `dst` back into `src` changes nothing.
    lines[next] = Line::Removed;
    true
}

/// Register class of a forwarded memory value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemClass {
    Gpr,
    Fpr,
}

/// Parses a full-width store as `(value register, address, class)`.
fn parse_store(arch: Arch, inst: &str) -> Option<(&str, &str, MemClass)> {
    match arch {
        Arch::AArch64 => {
            if !inst.starts_with("st") {
                return None;
            }
            let (mnemonic, first, second) = split_memory_operands(inst)?;
            if !matches!(mnemonic, "str" | "stur") || !is_plain_address(second) {
                return None;
            }
            reg_class(arch, first).map(|class| (first, second, class))
        }
        Arch::X86_64 => {
            if !inst.starts_with("mov") || !inst.contains(" QWORD PTR [") {
                return None;
            }
            let (mnemonic, first, second) = split_memory_operands(inst)?;
            let address = qword_address(first)?;
            let class = match mnemonic {
                "mov" if is_gpr64(arch, second) => MemClass::Gpr,
                "movsd" if is_fpr(arch, second) => MemClass::Fpr,
                _ => return None,
            };
            Some((second, address, class))
        }
    }
}

/// Parses a full-width load as `(destination register, address, class)`.
fn parse_load(arch: Arch, inst: &str) -> Option<(&str, &str, MemClass)> {
    match arch {
        Arch::AArch64 => {
            if !inst.starts_with("ld") {
                return None;
            }
            let (mnemonic, first, second) = split_memory_operands(inst)?;
            if !matches!(mnemonic, "ldr" | "ldur") || !is_plain_address(second) {
                return None;
            }
            reg_class(arch, first).map(|class| (first, second, class))
        }
        Arch::X86_64 => {
            if !inst.starts_with("mov") {
                return None;
            }
            let (mnemonic, first, second) = split_memory_operands(inst)?;
            let address = qword_address(second)?;
            let class = match mnemonic {
                "mov" if is_gpr64(arch, first) => MemClass::Gpr,
                "movsd" if is_fpr(arch, first) => MemClass::Fpr,
                _ => return None,
            };
            Some((first, address, class))
        }
    }
}

/// Splits `mnemonic a, b` where one operand may itself contain commas inside brackets.
fn split_memory_operands(inst: &str) -> Option<(&str, &str, &str)> {
    let (mnemonic, rest) = inst.split_once(' ')?;
    let rest = rest.trim();
    let split = if rest.starts_with('[') || rest.starts_with("QWORD") {
        rest.find(']').and_then(|close| rest[close..].find(',').map(|c| close + c))?
    } else {
        rest.find(',')?
    };
    Some((mnemonic, rest[..split].trim(), rest[split + 1..].trim()))
}

/// Returns the register class of a full-width AArch64 or x86_64 register.
fn reg_class(arch: Arch, reg: &str) -> Option<MemClass> {
    if is_gpr64(arch, reg) {
        Some(MemClass::Gpr)
    } else if is_fpr(arch, reg) {
        Some(MemClass::Fpr)
    } else {
        None
    }
}

/// Accepts a pre-indexed-free, post-index-free AArch64 address such as `[x29, #-16]`.
fn is_plain_address(address: &str) -> bool {
    address.starts_with('[') && address.ends_with(']') && !address.contains('!')
}

/// Returns the bracketed address of an x86_64 `QWORD PTR [..]` memory operand.
fn qword_address(operand: &str) -> Option<&str> {
    let address = operand.strip_prefix("QWORD PTR ")?;
    (address.starts_with('[') && address.ends_with(']')).then_some(address)
}

/// Replaces a reload of the value just stored at `index` with a register move (or nothing).
fn try_forward_store(lines: &mut [Line<'_>], index: usize, arch: Arch) -> bool {
    let Some((value, address, class)) = inst_at(lines, index).and_then(|inst| parse_store(arch, inst)) else {
        return false;
    };
    let Some(next) = next_significant(lines, index) else {
        return false;
    };
    let Some((dst, load_address, load_class)) = inst_at(lines, next).and_then(|inst| parse_load(arch, inst)) else {
        return false;
    };
    if load_address != address || load_class != class {
        return false;
    }
    let replacement = if dst == value {
        Line::Removed
    } else {
        let mnemonic = match (arch, class) {
            (_, MemClass::Gpr) => "mov",
            (Arch::AArch64, MemClass::Fpr) => "fmov",
            (Arch::X86_64, MemClass::Fpr) => "movsd",
        };
        Line::Inst(Cow::Owned(format!("{mnemonic} {dst}, {value}")))
    };
    lines[next] = replacement;
    true
}

/// Returns the target of an unconditional jump instruction.
fn unconditional_jump_target(arch: Arch, inst: &str) -> Option<&str> {
    let target = match arch {
        Arch::AArch64 => inst.strip_prefix("b ")?,
        Arch::X86_64 => inst.strip_prefix("jmp ")?,
    };
    let target = target.trim();
    (!target.is_empty() && !target.contains(char::is_whitespace)).then_some(target)
}

/// Resolves a branch operand (`name`, or a numeric `Nf` forward reference) to a label name.
fn forward_label_name(target: &str) -> &str {
    match target.strip_suffix('f') {
        Some(number) if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => number,
        _ => target,
    }
}

/// Returns true for a numeric backward reference such as `1b`.
fn is_numeric_backward(target: &str) -> bool {
    target
        .strip_suffix('b')
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
}

/// Deletes the unconditional jump at `index` when its target label is reached by falling through.
fn try_remove_jump_to_next_label(lines: &mut [Line<'_>], index: usize, arch: Arch) -> bool {
    let Some(target) = inst_at(lines, index).and_then(|inst| unconditional_jump_target(arch, inst)) else {
        return false;
    };
    if is_numeric_backward(target) {
        return false;
    }
    let wanted = forward_label_name(target);
    let mut falls_into_target = false;
    for line in &lines[index + 1..] {
        match line {
            Line::Trivia(_) | Line::Removed => {}
            Line::Label(name) if *name == wanted => {
                falls_into_target = true;
                break;
            }
            Line::Label(_) => {}
            Line::Inst(_) | Line::Other(_) => break,
        }
    }
    if falls_into_target {
        lines[index] = Line::Removed;
    }
    falls_into_target
}

/// Returns the AArch64 condition code that is true exactly when `cond` is false.
fn invert_aarch64_condition(cond: &str) -> Option<&'static str> {
    Some(match cond {
        "eq" => "ne",
        "ne" => "eq",
        "hs" | "cs" => "lo",
        "lo" | "cc" => "hs",
        "mi" => "pl",
        "pl" => "mi",
        "vs" => "vc",
        "vc" => "vs",
        "hi" => "ls",
        "ls" => "hi",
        "ge" => "lt",
        "lt" => "ge",
        "gt" => "le",
        "le" => "gt",
        _ => return None,
    })
}

/// Parses a short AArch64 conditional skip (`b.cc Nf`, `cbz r, Nf`, `cbnz r, Nf`) and returns
/// the inverted branch prefix (ready to append a target) together with the skipped label.
fn parse_short_skip(inst: &str) -> Option<(String, &str)> {
    if let Some(rest) = inst.strip_prefix("b.") {
        let (cond, target) = rest.split_once(' ')?;
        let label = numeric_forward(target.trim())?;
        return Some((format!("b.{} ", invert_aarch64_condition(cond)?), label));
    }
    let (inverse, rest) = if let Some(rest) = inst.strip_prefix("cbz ") {
        ("cbnz", rest)
    } else if let Some(rest) = inst.strip_prefix("cbnz ") {
        ("cbz", rest)
    } else {
        return None;
    };
    let (reg, target) = rest.split_once(',')?;
    let reg = reg.trim();
    let valid_reg = reg
        .strip_prefix(['x', 'w'])
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| n <= 30);
    if !valid_reg {
        return None;
    }
    let label = numeric_forward(target.trim())?;
    Some((format!("{inverse} {reg}, "), label))
}

/// Returns `N` for a numeric forward reference `Nf`.
fn numeric_forward(target: &str) -> Option<&str> {
    let number = target.strip_suffix('f')?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(number)
}

/// Returns true for a directive whose assembled size the per-line bound cannot cover.
fn may_emit_unbounded_bytes(line: &Line<'_>) -> bool {
    let Line::Other(text) = line else {
        return false;
    };
    let directive = text.trim_start();
    [".ascii", ".asciz", ".string", ".zero", ".space", ".skip", ".fill", ".align", ".balign", ".p2align", ".incbin", ".ltorg", ".pool"]
        .iter()
        .any(|prefix| directive.starts_with(prefix))
}

/// Returns the region's labels when AArch64 wide branches may be shortened in it at all.
///
/// Shortening is only sound when every label of the region is within conditional reach of
/// every branch, which the conservative per-line byte bound proves for the whole region.
fn shortening_labels(lines: &[Line<'_>]) -> Option<HashSet<String>> {
    let code_lines = lines
        .iter()
        .filter(|line| matches!(line, Line::Inst(_) | Line::Other(_)))
        .count();
    if code_lines.saturating_mul(MAX_BYTES_PER_LINE) >= AARCH64_COND_BRANCH_REACH
        || lines.iter().any(may_emit_unbounded_bytes)
    {
        return None;
    }
    Some(
        lines
            .iter()
            .filter_map(|line| match line {
                Line::Label(name) => Some((*name).to_string()),
                _ => None,
            })
            .collect(),
    )
}

/// Folds `b.cc 1f; b L; 1:` at `index` into `b.!cc L; 1:` when `L` is a region label.
fn try_shorten_wide_branch(lines: &mut [Line<'_>], index: usize, region_labels: &HashSet<String>) -> bool {
    let Some((inverted, skipped)) = inst_at(lines, index).and_then(parse_short_skip) else {
        return false;
    };
    let skipped = skipped.to_string();
    let Some(jump) = next_significant(lines, index) else {
        return false;
    };
    let target = inst_at(lines, jump)
        .and_then(|inst| unconditional_jump_target(Arch::AArch64, inst))
        .filter(|target| region_labels.contains(*target))
        .map(str::to_string);
    let Some(target) = target else {
        return false;
    };
    let lands_on_skip = next_significant(lines, jump)
        .is_some_and(|after| matches!(lines[after], Line::Label(name) if name == skipped));
    if !lands_on_skip {
        return false;
    }
    lines[index] = Line::Inst(Cow::Owned(format!("{inverted}{target}")));
    lines[jump] = Line::Removed;
    true
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the late machine peephole rewrites on both supported architectures.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Inputs mirror the emitter's exact line format (4-space indented instructions).

    use super::optimize;
    use crate::codegen::platform::{Arch, Platform, Target};

    /// Returns the Linux target for one architecture.
    fn linux(arch: Arch) -> Target {
        Target::new(Platform::Linux, arch)
    }

    /// Verifies self-moves and move-backs disappear, while 32-bit zero-extending moves stay.
    #[test]
    fn removes_redundant_register_moves() {
        let x86 = "    mov rax, rax\n    mov rsi, rax\n    # note\n    mov rax, rsi\n    mov eax, eax\n";
        assert_eq!(
            optimize(x86, linux(Arch::X86_64)),
            "    mov rsi, rax\n    # note\n    mov eax, eax\n"
        );
        let arm = "    mov x0, x0\n    fmov d1, d1\n    mov w0, w0\n    mov x12, x0\n    mov x0, x12\n";
        assert_eq!(optimize(arm, linux(Arch::AArch64)), "    mov w0, w0\n    mov x12, x0\n");
    }

    /// Verifies an adjacent reload of a just-stored frame slot is forwarded from the register.
    #[test]
    fn forwards_store_to_adjacent_reload() {
        let x86 = "    mov QWORD PTR [rbp - 64], rax\n    # @src line=1\n    mov rax, QWORD PTR [rbp - 64]\n    mov QWORD PTR [rbp - 8], rax\n    mov rsi, QWORD PTR [rbp - 8]\n";
        assert_eq!(
            optimize(x86, linux(Arch::X86_64)),
            "    mov QWORD PTR [rbp - 64], rax\n    # @src line=1\n    mov QWORD PTR [rbp - 8], rax\n    mov rsi, rax\n"
        );
        let arm = "    stur x0, [x29, #-16]\n    ldur x1, [x29, #-16]\n    stur d0, [x29, #-24]\n    ldur d0, [x29, #-24]\n";
        assert_eq!(
            optimize(arm, linux(Arch::AArch64)),
            "    stur x0, [x29, #-16]\n    mov x1, x0\n    stur d0, [x29, #-24]\n"
        );
    }

    /// Verifies a label between the store and the reload blocks forwarding (another path joins).
    #[test]
    fn store_forwarding_stops_at_labels_and_width_changes() {
        let x86 = "    mov QWORD PTR [rbp - 8], rax\n.L1:\n    mov rax, QWORD PTR [rbp - 8]\n    mov QWORD PTR [rbp - 16], rax\n    mov rax, QWORD PTR [rbp - 24]\n";
        assert_eq!(optimize(x86, linux(Arch::X86_64)), x86);
        let arm = "    stur x0, [x29, #-16]\n    ldur w0, [x29, #-16]\n    str x0, [x9, #8]!\n    ldr x0, [x9, #8]!\n";
        assert_eq!(optimize(arm, linux(Arch::AArch64)), arm);
    }

    /// Verifies jumps to the label that immediately follows are removed, other jumps kept.
    #[test]
    fn removes_jump_to_fallthrough_label() {
        let x86 = "    jmp .L_b\n    # @block name=b\n.L_b:\n    jmp .L_c\n    nop\n.L_c:\n";
        assert_eq!(
            optimize(x86, linux(Arch::X86_64)),
            "    # @block name=b\n.L_b:\n    jmp .L_c\n    nop\n.L_c:\n"
        );
        let arm = "    b 1f\n1:\n    b 1b\n";
        assert_eq!(optimize(arm, linux(Arch::AArch64)), "1:\n    b 1b\n");
    }

    /// Verifies a directive between a jump and its label keeps the jump.
    #[test]
    fn directives_block_jump_removal() {
        let x86 = "    jmp .L_b\n.section .text.other,\"ax\",@progbits\n.L_b:\n";
        assert_eq!(optimize(x86, linux(Arch::X86_64)), x86);
    }

    /// Verifies AArch64 wide skips fold into one conditional branch to an in-region label.
    #[test]
    fn shortens_wide_aarch64_branches_to_region_labels() {
        let arm = ".L_t:\n    cmp x0, x10\n    b.ge 1f\n    b .L_t\n1:\n    cbz x0, 1f\n    b .L_t\n1:\n    cbnz x0, 1f\n    b __rt_far\n1:\n";
        assert_eq!(
            optimize(arm, linux(Arch::AArch64)),
            ".L_t:\n    cmp x0, x10\n    b.lt .L_t\n1:\n    cbnz x0, .L_t\n1:\n    cbnz x0, 1f\n    b __rt_far\n1:\n"
        );
    }

    /// Verifies the region size cap alone keeps shortened branches within conditional reach.
    ///
    /// Every instruction line is at least as long as the 4-byte encoding it assembles to, and
    /// directives of unbounded size disable shortening, so a capped region cannot outgrow it.
    #[test]
    fn region_cap_is_within_conditional_reach() {
        const { assert!(super::MAX_REGION_BYTES < super::AARCH64_COND_BRANCH_REACH) };
        let arm = ".L_t:\n    b.eq 1f\n    b .L_t\n1:\n    .p2align 4\n";
        assert_eq!(optimize(arm, linux(Arch::AArch64)), arm);
    }

    /// Verifies regions over the size cap are returned unchanged.
    #[test]
    fn skips_regions_over_the_size_cap() {
        let mut x86 = String::from("    mov rax, rax\n");
        while x86.len() <= super::MAX_REGION_BYTES {
            x86.push_str("    nop\n");
        }
        assert_eq!(optimize(&x86, linux(Arch::X86_64)), x86);
    }
}
