//! Purpose:
//! Encodes the most frequent symbol-free AArch64 instructions of a generated slice into the
//! 32-bit words `as` would produce, so a slice can hand them over as `.inst` directives.
//!
//! Called from:
//! - `crate::linker::asm_split::SplitPlan::render_slice()` for AArch64 slices.
//!
//! Key details:
//! - Apple's assembler spends microseconds matching each instruction against its aliases:
//!   1.1 million `mov x0, x20` took 51 s, and the same number of `.word`s 0.2 s. The generated
//!   Symfony program is 37.5 million instructions, which made `as` 250 CPU-seconds of every
//!   build. An encoded word skips the matcher and leaves the object byte-for-byte the same.
//! - Only forms with a single possible encoding are handled. Anything the assembler might spell
//!   differently -- `sp` in a register form, a negative `add` immediate (which becomes `sub`), a
//!   value two wide-move encodings could both hold -- returns `None` and stays text.
//! - `encode` sees one line of the slice. A line it does not recognise is copied unchanged, so a
//!   gap here costs speed, never correctness.

/// One general-purpose register operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Reg {
    /// 0..=30, or 31 for `sp`/`xzr`/`wzr`.
    num: u32,
    /// `x`/`sp`/`xzr` rather than `w`/`wsp`/`wzr`.
    wide: bool,
    /// The stack pointer (encoded as 31 where the instruction allows it).
    sp: bool,
    /// The zero register (encoded as 31 where the instruction allows it).
    zr: bool,
}

impl Reg {
    /// A register that is neither `sp` nor the zero register.
    fn plain(self) -> bool {
        !self.sp && !self.zr
    }
}

/// Parses a general-purpose register name.
fn reg(text: &str) -> Option<Reg> {
    let text = text.trim_ascii();
    let named = |num, wide, sp, zr| Some(Reg { num, wide, sp, zr });
    match text {
        "sp" => return named(31, true, true, false),
        "wsp" => return named(31, false, true, false),
        "xzr" => return named(31, true, false, true),
        "wzr" => return named(31, false, false, true),
        "fp" => return named(29, true, false, false),
        "lr" => return named(30, true, false, false),
        _ => {}
    }
    let (wide, digits) = match text.as_bytes().first()? {
        b'x' => (true, &text[1..]),
        b'w' => (false, &text[1..]),
        _ => return None,
    };
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() == 2 && digits.starts_with('0') {
        return None;
    }
    let num: u32 = digits.parse().ok()?;
    (num <= 30).then_some(Reg {
        num,
        wide,
        sp: false,
        zr: false,
    })
}

/// Parses `#123`, `#-16` or `#0x5b78`.
fn imm(text: &str) -> Option<i64> {
    let text = text.trim_ascii().strip_prefix('#')?;
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits.strip_prefix("0x") {
        // Parsed unsigned: `#0xffff000000000000` is a valid 64-bit bit pattern.
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            u64::from_str_radix(hex, 16).ok()? as i64
        }
        Some(_) => return None,
        None if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            digits.parse::<i64>().ok()?
        }
        None => return None,
    };
    Some(if negative { -value } else { value })
}

/// A memory operand: `[base]`, `[base, #off]`, `[base, #off]!`, or post-index `[base], #off`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mem {
    Offset(Reg, i64),
    Pre(Reg, i64),
    Post(Reg, i64),
}

/// Parses the memory operand that closes an operand list (`rest` starts at `[`).
fn mem(rest: &str) -> Option<Mem> {
    let rest = rest.trim_ascii();
    let inner_end = rest.find(']')?;
    let inner = rest.strip_prefix('[')?.get(..inner_end - 1)?;
    let after = rest[inner_end + 1..].trim_ascii();
    let (base, offset) = match inner.split_once(',') {
        Some((base, offset)) => (reg(base)?, imm(offset)?),
        None => (reg(inner)?, 0),
    };
    if !base.wide {
        return None;
    }
    if after.is_empty() {
        return Some(Mem::Offset(base, offset));
    }
    if after == "!" {
        return inner.contains(',').then_some(Mem::Pre(base, offset));
    }
    let post = after.strip_prefix(',')?;
    if inner.contains(',') {
        return None;
    }
    Some(Mem::Post(base, imm(post)?))
}

/// Up to four operands, split at top-level commas (never inside `[...]`).
///
/// A fixed array rather than a `Vec`: this runs on every line of every slice.
struct Operands<'a> {
    items: [&'a str; 4],
    len: usize,
}

impl<'a> Operands<'a> {
    fn as_slice(&self) -> &[&'a str] {
        &self.items[..self.len]
    }
}

fn operands(text: &str) -> Option<Operands<'_>> {
    let mut ops = Operands {
        items: [""; 4],
        len: 0,
    };
    let mut depth = 0usize;
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                *ops.items.get_mut(ops.len)? = text[start..index].trim_ascii();
                ops.len += 1;
                start = index + 1;
            }
            _ => {}
        }
    }
    *ops.items.get_mut(ops.len)? = text[start..].trim_ascii();
    ops.len += 1;
    Some(ops)
}

/// Returns the word `as` assembles `line` into, when `line` is one of the handled forms.
pub(super) fn encode(line: &str) -> Option<u32> {
    let line = line.trim_ascii();
    let (mnemonic, rest) = line.split_once(' ')?;
    match mnemonic {
        "mov" => encode_mov(operands(rest)?.as_slice()),
        "movz" | "movk" => encode_wide_move(mnemonic, operands(rest)?.as_slice()),
        "add" | "sub" => encode_add_sub(mnemonic == "sub", operands(rest)?.as_slice()),
        "cmp" => encode_cmp(operands(rest)?.as_slice()),
        "lsl" => encode_lsl(operands(rest)?.as_slice()),
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" => encode_load_store(mnemonic, rest),
        "ldp" | "stp" => encode_pair(mnemonic == "ldp", rest),
        "blr" => {
            let ops = operands(rest)?;
            let [target] = ops.as_slice() else { return None };
            let target = reg(target)?;
            (target.plain() && target.wide).then_some(0xD63F_0000 | target.num << 5)
        }
        _ => None,
    }
}

/// `mov Rd, Rm` (ORR with the zero register) and `mov Rd, #imm` (a single MOVZ or MOVN).
fn encode_mov(ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    let dst = reg(dst)?;
    if dst.sp {
        return None;
    }
    if let Some(src) = reg(src) {
        // A move to or from SP is ADD (immediate); only plain registers are ORR.
        if src.sp || src.wide != dst.wide {
            return None;
        }
        let base = if dst.wide { 0xAA00_03E0 } else { 0x2A00_03E0 };
        return Some(base | src.num << 16 | dst.num);
    }
    let value = imm(src)?;
    let bits = if dst.wide { 64 } else { 32 };
    let mask = if bits == 64 { u64::MAX } else { u32::MAX as u64 };
    let value = (value as u64) & mask;
    // The assembler's order: MOVZ whenever one chunk holds the value, else MOVN when one chunk
    // holds its complement. Anything else (a bitmask immediate) is left for it to choose.
    if let Some((hw, chunk)) = single_chunk(value, bits) {
        return Some(wide_move(0xD280_0000, 0x5280_0000, dst, hw, chunk));
    }
    let (hw, chunk) = single_chunk(!value & mask, bits)?;
    Some(wide_move(0x9280_0000, 0x1280_0000, dst, hw, chunk))
}

/// `(hw, imm16)` when `value` is one 16-bit chunk at one of the register's four (or two) slots.
fn single_chunk(value: u64, bits: u32) -> Option<(u32, u32)> {
    if value == 0 {
        return Some((0, 0));
    }
    (0..bits / 16).find_map(|hw| {
        let shift = hw * 16;
        (value & !(0xFFFF_u64 << shift) == 0).then(|| (hw, ((value >> shift) & 0xFFFF) as u32))
    })
}

fn wide_move(x_base: u32, w_base: u32, dst: Reg, hw: u32, chunk: u32) -> u32 {
    (if dst.wide { x_base } else { w_base }) | hw << 21 | chunk << 5 | dst.num
}

/// `movz Rd, #imm` and `movk Rd, #imm, lsl #shift`.
fn encode_wide_move(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let (dst, value, shift) = match ops {
        [dst, value] => (dst, value, 0),
        [dst, value, shift] => (dst, value, imm(shift.strip_prefix("lsl")?.trim_ascii_start())?),
        _ => return None,
    };
    let dst = reg(dst)?;
    if !dst.plain() {
        return None;
    }
    let value = imm(value)?;
    let limit = if dst.wide { 48 } else { 16 };
    if !(0..=0xFFFF).contains(&value) || shift % 16 != 0 || !(0..=limit).contains(&shift) {
        return None;
    }
    let (x_base, w_base) = if mnemonic == "movz" {
        (0xD280_0000, 0x5280_0000)
    } else {
        (0xF280_0000, 0x7280_0000)
    };
    Some(wide_move(x_base, w_base, dst, (shift / 16) as u32, value as u32))
}

/// `add`/`sub Rd, Rn, #imm12` and `add`/`sub Rd, Rn, Rm`.
fn encode_add_sub(sub: bool, ops: &[&str]) -> Option<u32> {
    let [dst, first, second] = ops else { return None };
    let (dst, first) = (reg(dst)?, reg(first)?);
    if dst.wide != first.wide || dst.zr || first.zr {
        return None;
    }
    if let Some(second) = reg(second) {
        // With SP anywhere the assembler uses the extended-register form.
        if !dst.plain() || !first.plain() || second.sp || second.wide != dst.wide {
            return None;
        }
        let base = match (sub, dst.wide) {
            (false, true) => 0x8B00_0000,
            (true, true) => 0xCB00_0000,
            (false, false) => 0x0B00_0000,
            (true, false) => 0x4B00_0000,
        };
        return Some(base | second.num << 16 | first.num << 5 | dst.num);
    }
    let value = imm(second)?;
    // A negative immediate flips the mnemonic, and a large one is shifted: both left to `as`.
    if !(0..=0xFFF).contains(&value) {
        return None;
    }
    let base = match (sub, dst.wide) {
        (false, true) => 0x9100_0000,
        (true, true) => 0xD100_0000,
        (false, false) => 0x1100_0000,
        (true, false) => 0x5100_0000,
    };
    Some(base | (value as u32) << 10 | first.num << 5 | dst.num)
}

/// `cmp Rn, #imm12` (SUBS to the zero register) and `cmp Rn, Rm`.
fn encode_cmp(ops: &[&str]) -> Option<u32> {
    let [first, second] = ops else { return None };
    let first = reg(first)?;
    if first.zr {
        return None;
    }
    if let Some(second) = reg(second) {
        if !first.plain() || second.sp || second.wide != first.wide {
            return None;
        }
        let base = if first.wide { 0xEB00_001F } else { 0x6B00_001F };
        return Some(base | second.num << 16 | first.num << 5);
    }
    let value = imm(second)?;
    if !(0..=0xFFF).contains(&value) {
        return None;
    }
    let base = if first.wide { 0xF100_001F } else { 0x7100_001F };
    Some(base | (value as u32) << 10 | first.num << 5)
}

/// `lsl Rd, Rn, #shift` (UBFM).
fn encode_lsl(ops: &[&str]) -> Option<u32> {
    let [dst, src, shift] = ops else { return None };
    let (dst, src) = (reg(dst)?, reg(src)?);
    if !dst.plain() || src.sp || dst.wide != src.wide {
        return None;
    }
    let bits: i64 = if dst.wide { 64 } else { 32 };
    let shift = imm(shift)?;
    if !(1..bits).contains(&shift) {
        return None;
    }
    let immr = ((bits - shift) % bits) as u32;
    let imms = (bits - 1 - shift) as u32;
    let base = if dst.wide { 0xD340_0000 } else { 0x5300_0000 };
    Some(base | immr << 16 | imms << 10 | src.num << 5 | dst.num)
}

/// Single-register loads and stores of bytes, halves, words and doublewords.
fn encode_load_store(mnemonic: &str, rest: &str) -> Option<u32> {
    let (target, address) = rest.split_once(',')?;
    let target = reg(target)?;
    if target.sp {
        return None;
    }
    let load = mnemonic.starts_with("ldr");
    // `size` field and access width in bytes.
    let (size, scale) = match mnemonic {
        "ldrb" | "strb" if !target.wide => (0u32, 1i64),
        "ldrh" | "strh" if !target.wide => (1, 2),
        "ldr" | "str" if target.wide => (3, 8),
        "ldr" | "str" => (2, 4),
        _ => return None,
    };
    let opc = u32::from(load);
    let rt = target.num;
    match mem(address)? {
        Mem::Offset(base, offset) => {
            let rn = base.num;
            if offset >= 0 && offset % scale == 0 && offset / scale <= 0xFFF {
                // Unsigned scaled offset.
                let imm12 = (offset / scale) as u32;
                Some(size << 30 | 0x3900_0000 | opc << 22 | imm12 << 10 | rn << 5 | rt)
            } else if (-256..256).contains(&offset) {
                // Unscaled (LDUR/STUR), which `as` picks for a negative or unaligned offset.
                let imm9 = (offset as u32) & 0x1FF;
                Some(size << 30 | 0x3800_0000 | opc << 22 | imm9 << 12 | rn << 5 | rt)
            } else {
                None
            }
        }
        Mem::Pre(base, offset) | Mem::Post(base, offset) => {
            // Writing back into the transferred register is UNPREDICTABLE; let `as` rule on it.
            if !(-256..256).contains(&offset) || (!base.sp && base.num == rt) {
                return None;
            }
            let index = if matches!(mem(address)?, Mem::Pre(..)) { 0b11u32 } else { 0b01 };
            let imm9 = (offset as u32) & 0x1FF;
            Some(
                size << 30
                    | 0x3800_0000
                    | opc << 22
                    | imm9 << 12
                    | index << 10
                    | base.num << 5
                    | rt,
            )
        }
    }
}

/// `ldp`/`stp` of two X registers: signed offset, pre-index and post-index.
fn encode_pair(load: bool, rest: &str) -> Option<u32> {
    let (first, tail) = rest.split_once(',')?;
    let (second, address) = tail.split_once(',')?;
    let (first, second) = (reg(first)?, reg(second)?);
    if !first.wide || !second.wide || first.sp || second.sp || (load && first.num == second.num) {
        return None;
    }
    let (mode, base, offset) = match mem(address)? {
        Mem::Offset(base, offset) => (0b010u32, base, offset),
        Mem::Pre(base, offset) => (0b011, base, offset),
        Mem::Post(base, offset) => (0b001, base, offset),
    };
    if offset % 8 != 0 || !(-512..=504).contains(&offset) {
        return None;
    }
    // Writing back into a transferred register is UNPREDICTABLE; let `as` rule on it.
    if mode != 0b010 && !base.sp && (base.num == first.num || base.num == second.num) {
        return None;
    }
    let imm7 = ((offset / 8) as u32) & 0x7F;
    Some(
        0xA800_0000
            | mode << 23
            | u32::from(load) << 22
            | imm7 << 15
            | second.num << 10
            | base.num << 5
            | first.num,
    )
}

#[cfg(test)]
mod tests {
    use super::encode;

    /// Every word here is what Apple's `as` (clang 21) produced for the same line.
    #[test]
    fn encodings_match_the_assembler() {
        let mut wrong = Vec::new();
        for (line, word) in super::tests_table::CASES {
            let got = encode(line);
            if got != Some(*word) {
                wrong.push(format!("{line}: as {word:08x}, encode {got:08x?}"));
            }
        }
        assert!(wrong.is_empty(), "{} mismatches:\n{}", wrong.len(), wrong.join("\n"));
    }

    /// Forms whose encoding the assembler chooses are left as text.
    #[test]
    fn ambiguous_forms_stay_text() {
        for line in [
            "mov sp, x0",
            "mov x0, sp",
            "add x0, x0, #-8",
            "add sp, sp, x1",
            "cmp x0, #-1",
            "mov x0, #0xffff0000ffff",
            "mov x0, #0x5555555555555555",
            "ldr x0, [x1, #40960]",
            "ldr x0, [x0, #8]!",
            "ldr d0, [x1]",
            "bl __rt_incref",
            "add x1, x1, _str_3@PAGEOFF",
            "ldr x9, [x9, x10, lsl #3]",
        ] {
            assert_eq!(encode(line), None, "{line}");
        }
    }
}

#[cfg(test)]
#[path = "aarch64_encode_cases.rs"]
mod tests_table;
