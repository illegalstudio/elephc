//! Purpose:
//! Encodes one AArch64 instruction line of a generated listing into the 32-bit word Apple's `as`
//! produces for it, plus the fixup the in-tree assembler must resolve or turn into a relocation
//! when an operand names a label or symbol.
//!
//! Called from:
//! - `crate::linker::inhouse::assemble` for every instruction line of a slice.
//!
//! Key details:
//! - The word must be exactly the one `as` emits, including its alias choices (`mov` to or from
//!   `sp` is `add #0`, a negative `add` immediate is `sub`, a bitmask `mov` is `orr`, an
//!   unaligned `ldr` offset is `ldur`). The oracle test over `encode_cases` holds a word `as`
//!   produced for every form the compiler emits.
//! - A symbolic operand leaves its field zero in the word and is described by the [`Fixup`]; the
//!   assembler patches it for a target it defines in the same object, as `as` does, and emits a
//!   relocation otherwise.
//! - `None` means "not handled" (or "`as` would reject it"): the caller hands the whole slice
//!   back to `as`, so a gap here costs speed, never correctness.
//! - This runs on ~28 million lines per build: operands live in fixed arrays and nothing is
//!   allocated per line.

/// How a symbolic operand's field is filled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FixupKind {
    /// `b`/`bl`: imm26 = (target - pc) / 4.
    Branch26,
    /// `b.cond`, `cbz`/`cbnz`, `ldr` literal: imm19 at bits 5..24 = (target - pc) / 4.
    Branch19,
    /// `tbz`/`tbnz`: imm14 at bits 5..19 = (target - pc) / 4.
    Branch14,
    /// `adr`: immhi:immlo = target - pc.
    Adr21,
    /// `adrp Rd, sym@PAGE` (ARM64_RELOC_PAGE21).
    Page21,
    /// `adrp Rd, sym@GOTPAGE` (ARM64_RELOC_GOT_LOAD_PAGE21).
    GotPage21,
    /// `add`/`ldr`/`str ... sym@PAGEOFF` (ARM64_RELOC_PAGEOFF12); the linker scales a load or
    /// store's offset by its access size itself.
    PageOff12,
    /// `ldr Xt, [Xn, sym@GOTPAGEOFF]` (ARM64_RELOC_GOT_LOAD_PAGEOFF12).
    GotPageOff12,
}

/// A symbolic operand of one instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Fixup<'a> {
    pub(crate) kind: FixupKind,
    /// The label or symbol, as spelled (`L_xx_3`, `_main`, `1f`/`1b` for numeric locals).
    pub(crate) target: &'a str,
    /// Constant added to the target address (`sym+8@PAGEOFF`); usually 0.
    pub(crate) addend: i64,
}

/// One encoded instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Encoded<'a> {
    /// The instruction word with every symbolic field zero.
    pub(crate) word: u32,
    pub(crate) fixup: Option<Fixup<'a>>,
}

/// One general-purpose register operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Gp {
    /// 0..=30, or 31 for `sp`/`xzr`/`wzr`.
    num: u32,
    /// `x`/`sp`/`xzr` rather than `w`/`wsp`/`wzr`.
    wide: bool,
    /// The stack pointer (encoded as 31 where the instruction allows it).
    sp: bool,
    /// The zero register (encoded as 31 where the instruction allows it).
    zr: bool,
}

impl Gp {
    /// A register that is neither `sp` nor the zero register.
    fn plain(self) -> bool {
        !self.sp && !self.zr
    }

    /// The zero register of the same width.
    fn zero_of(self) -> Gp {
        Gp {
            num: 31,
            wide: self.wide,
            sp: false,
            zr: true,
        }
    }

    /// `sf` bit position value (bit 31) for a 64-bit form.
    fn sf(self) -> u32 {
        u32::from(self.wide) << 31
    }
}

/// Parses a general-purpose register name (`x0`, `w30`, `sp`, `wzr`, `fp`, `lr`, ...).
fn gp(text: &str) -> Option<Gp> {
    let named = |num, wide, sp, zr| Some(Gp { num, wide, sp, zr });
    match text {
        "sp" => return named(31, true, true, false),
        "wsp" => return named(31, false, true, false),
        "xzr" => return named(31, true, false, true),
        "wzr" => return named(31, false, false, true),
        "fp" => return named(29, true, false, false),
        "lr" => return named(30, true, false, false),
        _ => {}
    }
    let wide = match text.as_bytes().first()? {
        b'x' => true,
        b'w' => false,
        _ => return None,
    };
    let num = reg_number(&text[1..])?;
    (num <= 30).then_some(Gp {
        num,
        wide,
        sp: false,
        zr: false,
    })
}

/// Parses the digits of a register name: one or two digits, no leading zero.
fn reg_number(digits: &str) -> Option<u32> {
    let bytes = digits.as_bytes();
    if bytes.is_empty() || bytes.len() > 2 || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() == 2 && bytes[0] == b'0' {
        return None;
    }
    digits.parse().ok()
}

/// Width class of a scalar floating-point register: `b`, `h`, `s`, `d` or `q`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FpSize {
    B,
    H,
    S,
    D,
    Q,
}

impl FpSize {
    /// `ftype` field of the scalar floating-point instructions (`s` = 0, `d` = 1, `h` = 3).
    fn ftype(self) -> Option<u32> {
        match self {
            FpSize::S => Some(0),
            FpSize::D => Some(1),
            FpSize::H => Some(3),
            _ => None,
        }
    }
}

/// A scalar floating-point register operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Fp {
    num: u32,
    size: FpSize,
}

/// Parses a scalar floating-point register name (`d0`, `s31`, `q7`, `h1`, `b2`).
fn fp(text: &str) -> Option<Fp> {
    let size = match text.as_bytes().first()? {
        b'b' => FpSize::B,
        b'h' => FpSize::H,
        b's' => FpSize::S,
        b'd' => FpSize::D,
        b'q' => FpSize::Q,
        _ => return None,
    };
    let num = reg_number(&text[1..])?;
    (num <= 31).then_some(Fp { num, size })
}

/// Parses `#123`, `#-16`, `#0x5b78` (the `#` is optional, as in `cmp x0, 6`).
fn imm(text: &str) -> Option<i64> {
    let text = text.strip_prefix('#').unwrap_or(text);
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
            digits.parse::<u64>().ok()? as i64
        }
        None => return None,
    };
    Some(if negative { value.wrapping_neg() } else { value })
}

/// Parses a condition code name to its 4-bit encoding.
fn cond(text: &str) -> Option<u32> {
    Some(match text {
        "eq" => 0,
        "ne" => 1,
        "cs" | "hs" => 2,
        "cc" | "lo" => 3,
        "mi" => 4,
        "pl" => 5,
        "vs" => 6,
        "vc" => 7,
        "hi" => 8,
        "ls" => 9,
        "ge" => 10,
        "lt" => 11,
        "gt" => 12,
        "le" => 13,
        "al" => 14,
        "nv" => 15,
        _ => return None,
    })
}

/// A shift or extend applied to the last register operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Modifier {
    Lsl,
    Lsr,
    Asr,
    Ror,
    /// An extend; the payload is the 3-bit `option` field (`uxtb` 0 .. `sxtx` 7).
    Ext(u32),
}

/// A parsed modifier and its optional `#amount`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Shift {
    kind: Modifier,
    amount: Option<u32>,
}

/// Parses `lsl #3`, `sxtw #2`, `uxtw`, ...
fn shift(text: &str) -> Option<Shift> {
    let (name, rest) = match text.split_once(' ') {
        Some((name, rest)) => (name, Some(rest.trim_ascii_start())),
        None => (text, None),
    };
    let kind = match name {
        "lsl" => Modifier::Lsl,
        "lsr" => Modifier::Lsr,
        "asr" => Modifier::Asr,
        "ror" => Modifier::Ror,
        "uxtb" => Modifier::Ext(0),
        "uxth" => Modifier::Ext(1),
        "uxtw" => Modifier::Ext(2),
        "uxtx" => Modifier::Ext(3),
        "sxtb" => Modifier::Ext(4),
        "sxth" => Modifier::Ext(5),
        "sxtw" => Modifier::Ext(6),
        "sxtx" => Modifier::Ext(7),
        _ => return None,
    };
    let amount = match rest {
        Some(rest) => Some(u32::try_from(imm(rest)?).ok()?),
        None => None,
    };
    Some(Shift { kind, amount })
}

/// Operands split at top-level commas (never inside `[...]`), in a fixed array: this runs on
/// every line of every slice.
struct Operands<'a> {
    items: [&'a str; 6],
    len: usize,
}

impl<'a> Operands<'a> {
    /// The operands present.
    fn as_slice(&self) -> &[&'a str] {
        &self.items[..self.len]
    }
}

/// Splits an operand list; `None` when there are more than six operands.
fn operands(text: &str) -> Option<Operands<'_>> {
    let mut ops = Operands {
        items: [""; 6],
        len: 0,
    };
    let text = text.trim_ascii();
    if text.is_empty() {
        return Some(ops);
    }
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

/// A symbolic operand: `name`, `name+8`, `name@PAGEOFF`, `name-4@PAGE`, `1f`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Symbol<'a> {
    target: &'a str,
    addend: i64,
    /// The text after `@` (`PAGE`, `PAGEOFF`, `GOTPAGE`, `GOTPAGEOFF`), or empty.
    suffix: &'a str,
}

/// Parses a symbolic operand (anything that is not a register or an immediate).
fn symbol(text: &str) -> Option<Symbol<'_>> {
    let first = *text.as_bytes().first()?;
    if !(first.is_ascii_alphanumeric() || first == b'_' || first == b'.' || first == b'$') {
        return None;
    }
    let (name, suffix) = match text.split_once('@') {
        Some((name, suffix)) => (name, suffix),
        None => (text, ""),
    };
    // A trailing `+N`/`-N` is the addend; the name itself never contains either.
    let split = name
        .bytes()
        .enumerate()
        .skip(1)
        .find(|&(_, b)| b == b'+' || b == b'-')
        .map(|(index, _)| index);
    let (target, addend) = match split {
        Some(index) => {
            let tail = &name[index..];
            (&name[..index], imm(tail.strip_prefix('+').unwrap_or(tail))?)
        }
        None => (name, 0),
    };
    Some(Symbol {
        target,
        addend,
        suffix,
    })
}

/// Wraps a plain word.
fn plain(word: u32) -> Option<Encoded<'static>> {
    Some(Encoded { word, fixup: None })
}

/// Wraps a word with the fixup of a symbolic operand.
fn with_fixup<'a>(word: u32, kind: FixupKind, sym: Symbol<'a>) -> Option<Encoded<'a>> {
    Some(Encoded {
        word,
        fixup: Some(Fixup {
            kind,
            target: sym.target,
            addend: sym.addend,
        }),
    })
}

/// Encodes one trimmed instruction line (`mnemonic operands`), or `None` when the form is not
/// handled.
pub(crate) fn encode_instruction(line: &str) -> Option<Encoded<'_>> {
    let line = line.trim_ascii();
    let (mnemonic, rest) = match line.split_once(' ') {
        Some((mnemonic, rest)) => (mnemonic, rest),
        None => (line, ""),
    };
    let ops = operands(rest)?;
    let ops = ops.as_slice();
    match mnemonic {
        "b" | "bl" => encode_branch(mnemonic, ops),
        "cbz" | "cbnz" | "tbz" | "tbnz" => encode_compare_branch(mnemonic, ops),
        "adr" | "adrp" => encode_adr(mnemonic, ops),
        "add" if ops.len() == 3 && ops[2].contains('@') => encode_add_symbol(ops),
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" | "ldrsb" | "ldrsh" | "ldrsw" | "ldur"
        | "stur" | "ldurb" | "sturb" | "ldurh" | "sturh" | "ldursb" | "ldursh" | "ldursw" => {
            encode_load_store(mnemonic, ops)
        }
        _ if mnemonic.starts_with("b.") => {
            let [target] = ops else { return None };
            let condition = cond(&mnemonic[2..])?;
            with_fixup(0x5400_0000 | condition, FixupKind::Branch19, symbol(target)?)
        }
        _ => encode_word(mnemonic, ops).and_then(plain),
    }
}

/// Encodes every instruction that has no symbolic operand.
fn encode_word(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    match mnemonic {
        "mov" => encode_mov(ops),
        "movz" | "movk" | "movn" => encode_wide_move(mnemonic, ops),
        "add" | "sub" | "adds" | "subs" => encode_add_sub_op(mnemonic, ops),
        "cmp" | "cmn" => encode_cmp(mnemonic == "cmp", ops),
        "neg" | "negs" => encode_neg(mnemonic == "negs", ops),
        "ngc" | "ngcs" => encode_ngc(mnemonic == "ngcs", ops),
        "and" | "orr" | "eor" | "ands" | "bic" | "orn" | "eon" | "bics" => {
            encode_logical_op(mnemonic, ops)
        }
        "tst" => encode_tst(ops),
        "mvn" => encode_mvn(ops),
        "lsl" | "lsr" | "asr" | "ror" => encode_shift(mnemonic, ops),
        "ubfx" | "sbfx" | "bfxil" | "ubfiz" | "sbfiz" | "bfi" | "bfc" | "ubfm" | "sbfm" | "bfm" => {
            encode_bitfield(mnemonic, ops)
        }
        "extr" => encode_extr(ops),
        "sxtb" | "sxth" | "sxtw" | "uxtb" | "uxth" | "uxtw" => encode_extend(mnemonic, ops),
        "rev" | "rev16" | "rev32" | "clz" | "cls" | "rbit" => encode_unary_int(mnemonic, ops),
        "mul" | "mneg" | "sdiv" | "udiv" | "smulh" | "umulh" | "smull" | "umull" | "smnegl"
        | "umnegl" => encode_mul3(mnemonic, ops),
        "madd" | "msub" | "smaddl" | "smsubl" | "umaddl" | "umsubl" => encode_mul4(mnemonic, ops),
        "csel" | "csinc" | "csinv" | "csneg" => encode_cond_select(mnemonic, ops),
        "cset" | "csetm" => encode_cset(mnemonic == "csetm", ops),
        "cinc" | "cinv" | "cneg" => encode_cond_inc(mnemonic, ops),
        "ccmp" | "ccmn" => encode_cond_compare(mnemonic == "ccmp", ops),
        "ldp" | "stp" | "ldnp" | "stnp" | "ldpsw" => encode_pair(mnemonic, ops),
        "br" | "blr" | "ret" => encode_branch_register(mnemonic, ops),
        "svc" | "udf" | "brk" | "hlt" => encode_exception(mnemonic, ops),
        "nop" | "yield" | "wfe" | "wfi" | "sev" => {
            if !ops.is_empty() {
                return None;
            }
            Some(match mnemonic {
                "nop" => 0xD503_201F,
                "yield" => 0xD503_203F,
                "wfe" => 0xD503_205F,
                "wfi" => 0xD503_207F,
                _ => 0xD503_209F,
            })
        }
        "dmb" | "dsb" | "isb" => encode_barrier(mnemonic, ops),
        "mrs" | "msr" => encode_system_register(mnemonic == "mrs", ops),
        _ => encode_float(mnemonic, ops),
    }
}

/// `b label` and `bl label`.
fn encode_branch<'a>(mnemonic: &str, ops: &[&'a str]) -> Option<Encoded<'a>> {
    let [target] = ops else { return None };
    let base = if mnemonic == "bl" { 0x9400_0000 } else { 0x1400_0000 };
    with_fixup(base, FixupKind::Branch26, symbol(target)?)
}

/// `cbz`/`cbnz Rt, label` and `tbz`/`tbnz Rt, #bit, label`.
fn encode_compare_branch<'a>(mnemonic: &str, ops: &[&'a str]) -> Option<Encoded<'a>> {
    if mnemonic.starts_with('c') {
        let [reg, target] = ops else { return None };
        let reg = gp(reg)?;
        if reg.sp {
            return None;
        }
        let base = if mnemonic == "cbz" { 0x3400_0000 } else { 0x3500_0000 };
        return with_fixup(
            base | reg.sf() | reg.num,
            FixupKind::Branch19,
            symbol(target)?,
        );
    }
    let [reg, bit, target] = ops else { return None };
    let reg = gp(reg)?;
    let bit = u32::try_from(imm(bit)?).ok()?;
    if reg.sp || bit >= if reg.wide { 64 } else { 32 } {
        return None;
    }
    let base = if mnemonic == "tbz" { 0x3600_0000 } else { 0x3700_0000 };
    let word = base | (bit >> 5) << 31 | (bit & 31) << 19 | reg.num;
    with_fixup(word, FixupKind::Branch14, symbol(target)?)
}

/// `adr Rd, label` and `adrp Rd, sym@PAGE` / `sym@GOTPAGE`.
fn encode_adr<'a>(mnemonic: &str, ops: &[&'a str]) -> Option<Encoded<'a>> {
    let [dst, target] = ops else { return None };
    let dst = gp(dst)?;
    if !dst.wide || !dst.plain() {
        return None;
    }
    let sym = symbol(target)?;
    if mnemonic == "adr" {
        return with_fixup(0x1000_0000 | dst.num, FixupKind::Adr21, sym);
    }
    let kind = match sym.suffix {
        "PAGE" => FixupKind::Page21,
        "GOTPAGE" => FixupKind::GotPage21,
        _ => return None,
    };
    with_fixup(0x9000_0000 | dst.num, kind, sym)
}

/// `add Rd, Rn, sym@PAGEOFF`.
fn encode_add_symbol<'a>(ops: &[&'a str]) -> Option<Encoded<'a>> {
    let [dst, first, target] = ops else { return None };
    let (dst, first) = (gp(dst)?, gp(first)?);
    let sym = symbol(target)?;
    if !dst.wide || !first.wide || dst.zr || first.zr || sym.suffix != "PAGEOFF" {
        return None;
    }
    with_fixup(
        0x9100_0000 | first.num << 5 | dst.num,
        FixupKind::PageOff12,
        sym,
    )
}

/// `mov Rd, Rm` (ORR, or ADD #0 with SP) and `mov Rd, #imm` (MOVZ, MOVN or ORR).
fn encode_mov(ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    let dst = gp(dst)?;
    if let Some(src) = gp(src) {
        if src.wide != dst.wide {
            return None;
        }
        // A move to or from SP is ADD (immediate) #0.
        if dst.sp || src.sp {
            if dst.zr || src.zr {
                return None;
            }
            return Some(dst.sf() | 0x1100_0000 | src.num << 5 | dst.num);
        }
        if dst.zr {
            return None;
        }
        return Some(dst.sf() | 0x2A00_03E0 | src.num << 16 | dst.num);
    }
    let value = imm(src)?;
    let bits = if dst.wide { 64 } else { 32 };
    let mask = if dst.wide { u64::MAX } else { u64::from(u32::MAX) };
    if !dst.wide && !(-(1i64 << 32)..(1i64 << 32)).contains(&value) {
        return None;
    }
    let value = (value as u64) & mask;
    // The assembler's order: MOVZ whenever one chunk holds the value, MOVN when one chunk holds
    // its complement, else a bitmask immediate (ORR, which cannot name the zero register).
    if let Some((hw, chunk)) = single_chunk(value, bits) {
        return Some(wide_move(0x5280_0000, dst, hw, chunk));
    }
    if let Some((hw, chunk)) = single_chunk(!value & mask, bits) {
        return Some(wide_move(0x1280_0000, dst, hw, chunk));
    }
    if dst.zr {
        return None;
    }
    let (n, immr, imms) = logical_immediate(value, bits)?;
    Some(dst.sf() | 0x3200_0000 | n << 22 | immr << 16 | imms << 10 | 0x1F << 5 | dst.num)
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

/// A wide-move word; `w_base` is the 32-bit encoding, the 64-bit one adds the `sf` bit.
fn wide_move(w_base: u32, dst: Gp, hw: u32, chunk: u32) -> u32 {
    dst.sf() | w_base | hw << 21 | chunk << 5 | dst.num
}

/// `(N, immr, imms)` of a bitmask immediate, or `None` when `value` is not one.
///
/// `value` is the register-width pattern (a 32-bit value for `bits` = 32).
fn logical_immediate(value: u64, bits: u32) -> Option<(u32, u32, u32)> {
    let value = if bits == 32 {
        (value & 0xFFFF_FFFF) | value << 32
    } else {
        value
    };
    if value == 0 || value == u64::MAX {
        return None;
    }
    // The smallest element that tiles the pattern.
    let mut size = 64u32;
    while size > 2 {
        let half = size / 2;
        let mask = (1u64 << half) - 1;
        if value & mask != (value >> half) & mask {
            break;
        }
        size = half;
    }
    let element = if size == 64 {
        value
    } else {
        value & ((1u64 << size) - 1)
    };
    let ones = element.count_ones();
    let run = (1u64 << ones) - 1;
    let element_mask = if size == 64 { u64::MAX } else { (1u64 << size) - 1 };
    // `element` must be a rotation of `ones` contiguous low bits; immr is the right rotation.
    let rotl = |v: u64, r: u32| {
        if r == 0 {
            v
        } else {
            ((v << r) | (v >> (size - r))) & element_mask
        }
    };
    let immr = (0..size).find(|&r| rotl(element, r) == run)?;
    let imms = ((!(size - 1) << 1) | (ones - 1)) & 0x3F;
    Some((u32::from(size == 64), immr, imms))
}

/// `movz`/`movn`/`movk Rd, #imm{, lsl #shift}`.
fn encode_wide_move(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let (dst, value, shift_text) = match ops {
        [dst, value] => (dst, value, None),
        [dst, value, shift_text] => (dst, value, Some(shift_text)),
        _ => return None,
    };
    let dst = gp(dst)?;
    if !dst.plain() {
        return None;
    }
    let shift_amount = match shift_text {
        Some(text) => {
            let parsed = shift(text)?;
            if parsed.kind != Modifier::Lsl {
                return None;
            }
            parsed.amount?
        }
        None => 0,
    };
    let value = imm(value)?;
    let limit = if dst.wide { 48 } else { 16 };
    if !(0..=0xFFFF).contains(&value) || shift_amount % 16 != 0 || shift_amount > limit {
        return None;
    }
    let base = match mnemonic {
        "movz" => 0x5280_0000,
        "movn" => 0x1280_0000,
        _ => 0x7280_0000,
    };
    Some(wide_move(base, dst, shift_amount / 16, value as u32))
}

/// Splits the operand after the registers from its optional shift/extend.
fn operand_and_shift<'a>(rest: &[&'a str]) -> Option<(&'a str, Option<Shift>)> {
    match rest {
        [operand] => Some((operand, None)),
        [operand, modifier] => Some((operand, Some(shift(modifier)?))),
        _ => None,
    }
}

/// `add`/`sub`/`adds`/`subs Rd, Rn, <imm | Rm{, shift}>`.
fn encode_add_sub_op(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, first, rest @ ..] = ops else { return None };
    let sub = mnemonic.starts_with("sub");
    let flags = mnemonic.ends_with('s');
    add_sub(sub, flags, gp(dst)?, gp(first)?, rest)
}

/// `cmp`/`cmn Rn, <imm | Rm{, shift}>` (SUBS/ADDS into the zero register).
fn encode_cmp(sub: bool, ops: &[&str]) -> Option<u32> {
    let [first, rest @ ..] = ops else { return None };
    let first = gp(first)?;
    add_sub(sub, true, first.zero_of(), first, rest)
}

/// `neg`/`negs Rd, Rm{, shift}` (SUB/SUBS from the zero register).
fn encode_neg(flags: bool, ops: &[&str]) -> Option<u32> {
    let [dst, rest @ ..] = ops else { return None };
    let dst = gp(dst)?;
    add_sub(true, flags, dst, dst.zero_of(), rest)
}

/// `ngc`/`ngcs Rd, Rm` (SBC/SBCS from the zero register).
fn encode_ngc(flags: bool, ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    let (dst, src) = (gp(dst)?, gp(src)?);
    if dst.wide != src.wide || !dst.plain() && !dst.zr || src.sp || dst.sp {
        return None;
    }
    let base = 0x5A00_03E0 | u32::from(flags) << 29;
    Some(dst.sf() | base | src.num << 16 | dst.num)
}

/// The shared ADD/SUB core. `rest` is the third operand and its optional modifier.
fn add_sub(sub: bool, flags: bool, dst: Gp, first: Gp, rest: &[&str]) -> Option<u32> {
    let (operand, modifier) = operand_and_shift(rest)?;
    if dst.wide != first.wide {
        return None;
    }
    let op_bits = u32::from(sub) << 30 | u32::from(flags) << 29;
    if let Some(register) = gp(operand) {
        return add_sub_register(op_bits, flags, dst, first, register, modifier);
    }
    // Immediate form: `Rn` may be SP but not the zero register.
    if first.zr || (flags && dst.sp) || (!flags && dst.zr) {
        return None;
    }
    let value = imm(operand)?;
    let explicit = match modifier {
        None => None,
        Some(Shift {
            kind: Modifier::Lsl,
            amount: Some(amount @ (0 | 12)),
        }) => Some(amount),
        Some(_) => return None,
    };
    // A negative value flips ADD/SUB; the magnitude must fit 12 bits, optionally shifted by 12.
    let (flip, magnitude) = if value < 0 {
        if explicit.is_some() {
            return None;
        }
        (true, value.checked_neg()? as u64)
    } else {
        (false, value as u64)
    };
    let (imm12, shift12) = match explicit {
        Some(12) if magnitude <= 0xFFF => (magnitude, 1),
        Some(12) => return None,
        _ if magnitude <= 0xFFF => (magnitude, 0),
        _ if magnitude <= 0xFFF => (magnitude, 0),
        _ if magnitude & 0xFFF == 0 && magnitude <= 0xFF_F000 => (magnitude >> 12, 1),
        _ => return None,
    };
    let op_bits = if flip { op_bits ^ (1 << 30) } else { op_bits };
    Some(
        dst.sf()
            | 0x1100_0000
            | op_bits
            | shift12 << 22
            | (imm12 as u32) << 10
            | first.num << 5
            | dst.num,
    )
}

/// The register forms of ADD/SUB: shifted-register, or extended-register when `sp` is involved
/// or an extend is written.
fn add_sub_register(
    op_bits: u32,
    flags: bool,
    dst: Gp,
    first: Gp,
    register: Gp,
    modifier: Option<Shift>,
) -> Option<u32> {
    let extended = dst.sp
        || first.sp
        || matches!(modifier, Some(Shift { kind: Modifier::Ext(_), .. }));
    if register.sp {
        return None;
    }
    if !extended {
        if register.wide != dst.wide {
            return None;
        }
        let (kind, amount) = match modifier {
            None => (0, 0),
            Some(Shift { kind, amount }) => (
                match kind {
                    Modifier::Lsl => 0,
                    Modifier::Lsr => 1,
                    Modifier::Asr => 2,
                    _ => return None,
                },
                amount.unwrap_or(0),
            ),
        };
        if amount >= if dst.wide { 64 } else { 32 } {
            return None;
        }
        return Some(
            dst.sf()
                | 0x0B00_0000
                | op_bits
                | kind << 22
                | register.num << 16
                | amount << 10
                | first.num << 5
                | dst.num,
        );
    }
    // Extended register: `Rd` is SP (or ZR with flags), `Rn` is SP, never the zero register.
    if first.zr || (flags && dst.sp) || (!flags && dst.zr) {
        return None;
    }
    let (option, amount) = match modifier {
        None => (if dst.wide { 3 } else { 2 }, 0),
        Some(Shift {
            kind: Modifier::Lsl,
            amount,
        }) => (if dst.wide { 3 } else { 2 }, amount.unwrap_or(0)),
        Some(Shift {
            kind: Modifier::Ext(option),
            amount,
        }) => (option, amount.unwrap_or(0)),
        Some(_) => return None,
    };
    // The index register is 64-bit exactly for the `x` extends (uxtx/sxtx, and lsl).
    let wide_index = option & 3 == 3;
    if amount > 4 || register.wide != wide_index {
        return None;
    }
    if matches!(modifier, Some(Shift { kind: Modifier::Lsl, .. })) || modifier.is_none() {
        // `lsl` spells uxtx (64-bit) or uxtw (32-bit) and needs the same width as `Rd`.
        if register.wide != dst.wide {
            return None;
        }
    }
    Some(
        dst.sf()
            | 0x0B20_0000
            | op_bits
            | register.num << 16
            | option << 13
            | amount << 10
            | first.num << 5
            | dst.num,
    )
}

/// `and`/`orr`/`eor`/`ands` and `bic`/`orn`/`eon`/`bics` with a register or bitmask immediate.
fn encode_logical_op(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, first, rest @ ..] = ops else { return None };
    let (opc, invert) = match mnemonic {
        "and" => (0, false),
        "orr" => (1, false),
        "eor" => (2, false),
        "ands" => (3, false),
        "bic" => (0, true),
        "orn" => (1, true),
        "eon" => (2, true),
        _ => (3, true),
    };
    logical(opc, invert, gp(dst)?, gp(first)?, rest)
}

/// The shared logical core. `rest` is the third operand and its optional modifier.
fn logical(opc: u32, invert: bool, dst: Gp, first: Gp, rest: &[&str]) -> Option<u32> {
    let (operand, modifier) = operand_and_shift(rest)?;
    if dst.wide != first.wide || first.sp {
        return None;
    }
    if let Some(register) = gp(operand) {
        if dst.sp || register.sp || register.wide != dst.wide {
            return None;
        }
        let (kind, amount) = match modifier {
            None => (0, 0),
            Some(Shift { kind, amount }) => (
                match kind {
                    Modifier::Lsl => 0,
                    Modifier::Lsr => 1,
                    Modifier::Asr => 2,
                    Modifier::Ror => 3,
                    Modifier::Ext(_) => return None,
                },
                amount.unwrap_or(0),
            ),
        };
        if amount >= if dst.wide { 64 } else { 32 } {
            return None;
        }
        return Some(
            dst.sf()
                | 0x0A00_0000
                | opc << 29
                | kind << 22
                | u32::from(invert) << 21
                | register.num << 16
                | amount << 10
                | first.num << 5
                | dst.num,
        );
    }
    if modifier.is_some() || (opc == 3 && dst.sp) || (opc != 3 && dst.zr) {
        return None;
    }
    let value = imm(operand)?;
    let bits = if dst.wide { 64 } else { 32 };
    if !dst.wide && !(-(1i64 << 32)..(1i64 << 32)).contains(&value) {
        return None;
    }
    let mut pattern = value as u64;
    if invert {
        pattern = !pattern;
    }
    let (n, immr, imms) = logical_immediate(pattern & if dst.wide { u64::MAX } else { 0xFFFF_FFFF }, bits)?;
    Some(
        dst.sf()
            | 0x1200_0000
            | opc << 29
            | n << 22
            | immr << 16
            | imms << 10
            | first.num << 5
            | dst.num,
    )
}

/// `tst Rn, <Rm{, shift} | #bitmask>` (ANDS into the zero register).
fn encode_tst(ops: &[&str]) -> Option<u32> {
    let [first, rest @ ..] = ops else { return None };
    let first = gp(first)?;
    logical(3, false, first.zero_of(), first, rest)
}

/// `mvn Rd, Rm{, shift}` (ORN from the zero register).
fn encode_mvn(ops: &[&str]) -> Option<u32> {
    let [dst, rest @ ..] = ops else { return None };
    let dst = gp(dst)?;
    logical(1, true, dst, dst.zero_of(), rest)
}

/// `lsl`/`lsr`/`asr`/`ror Rd, Rn, <#shift | Rm>`.
fn encode_shift(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, src, amount] = ops else { return None };
    let (dst, src) = (gp(dst)?, gp(src)?);
    if dst.sp || src.sp || dst.wide != src.wide {
        return None;
    }
    let bits: i64 = if dst.wide { 64 } else { 32 };
    if let Some(amount) = gp(amount) {
        if amount.sp || amount.wide != dst.wide {
            return None;
        }
        let base = match mnemonic {
            "lsl" => 0x1AC0_2000,
            "lsr" => 0x1AC0_2400,
            "asr" => 0x1AC0_2800,
            _ => 0x1AC0_2C00,
        };
        return Some(dst.sf() | base | amount.num << 16 | src.num << 5 | dst.num);
    }
    let amount = imm(amount)?;
    if !(0..bits).contains(&amount) {
        return None;
    }
    let n = u32::from(dst.wide) << 22;
    let last = (bits - 1) as u32;
    let amount_u = amount as u32;
    match mnemonic {
        "lsl" => {
            let immr = ((bits - amount) % bits) as u32;
            let imms = (bits - 1 - amount) as u32;
            Some(dst.sf() | 0x5300_0000 | n | immr << 16 | imms << 10 | src.num << 5 | dst.num)
        }
        "lsr" => Some(dst.sf() | 0x5300_0000 | n | amount_u << 16 | last << 10 | src.num << 5 | dst.num),
        "asr" => Some(dst.sf() | 0x1300_0000 | n | amount_u << 16 | last << 10 | src.num << 5 | dst.num),
        _ => Some(
            dst.sf()
                | 0x1380_0000
                | n
                | src.num << 16
                | amount_u << 10
                | src.num << 5
                | dst.num,
        ),
    }
}

/// `ubfx`/`sbfx`/`bfxil`/`ubfiz`/`sbfiz`/`bfi`/`bfc` and the raw `ubfm`/`sbfm`/`bfm`.
fn encode_bitfield(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let (dst, src, first, second) = match (mnemonic, ops) {
        ("bfc", [dst, first, second]) => (gp(dst)?, None, *first, *second),
        (_, [dst, src, first, second]) if mnemonic != "bfc" => {
            (gp(dst)?, Some(gp(src)?), *first, *second)
        }
        _ => return None,
    };
    let src = src.unwrap_or(dst.zero_of());
    if dst.sp || src.sp || dst.wide != src.wide {
        return None;
    }
    let bits = if dst.wide { 64i64 } else { 32 };
    let (a, b) = (imm(first)?, imm(second)?);
    let (immr, imms, base) = match mnemonic {
        "ubfm" | "sbfm" | "bfm" => {
            let base = match mnemonic {
                "ubfm" => 0x5300_0000,
                "sbfm" => 0x1300_0000,
                _ => 0x3300_0000,
            };
            if !(0..bits).contains(&a) || !(0..bits).contains(&b) {
                return None;
            }
            (a, b, base)
        }
        "ubfx" | "sbfx" | "bfxil" => {
            if !(0..bits).contains(&a) || b < 1 || a + b > bits {
                return None;
            }
            let base = match mnemonic {
                "ubfx" => 0x5300_0000,
                "sbfx" => 0x1300_0000,
                _ => 0x3300_0000,
            };
            (a, a + b - 1, base)
        }
        _ => {
            // ubfiz / sbfiz / bfi / bfc: insert `width` bits at `lsb`.
            if !(0..bits).contains(&a) || b < 1 || a + b > bits {
                return None;
            }
            let base = match mnemonic {
                "ubfiz" => 0x5300_0000,
                "sbfiz" => 0x1300_0000,
                _ => 0x3300_0000,
            };
            ((bits - a) % bits, b - 1, base)
        }
    };
    let n = u32::from(dst.wide) << 22;
    Some(dst.sf() | base | n | (immr as u32) << 16 | (imms as u32) << 10 | src.num << 5 | dst.num)
}

/// `extr Rd, Rn, Rm, #lsb`.
fn encode_extr(ops: &[&str]) -> Option<u32> {
    let [dst, first, second, lsb] = ops else { return None };
    let (dst, first, second) = (gp(dst)?, gp(first)?, gp(second)?);
    if dst.sp || first.sp || second.sp || dst.wide != first.wide || dst.wide != second.wide {
        return None;
    }
    let lsb = imm(lsb)?;
    if !(0..if dst.wide { 64 } else { 32 }).contains(&lsb) {
        return None;
    }
    let n = u32::from(dst.wide) << 22;
    Some(dst.sf() | 0x1380_0000 | n | second.num << 16 | (lsb as u32) << 10 | first.num << 5 | dst.num)
}

/// `sxtb`/`sxth`/`sxtw`/`uxtb`/`uxth`/`uxtw Rd, Wn`.
fn encode_extend(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    let (dst, src) = (gp(dst)?, gp(src)?);
    if dst.sp || src.sp || src.wide {
        return None;
    }
    let last = match &mnemonic[3..] {
        "b" => 7,
        "h" => 15,
        _ => 31,
    };
    let signed = mnemonic.starts_with('s');
    if mnemonic == "uxtw" {
        // `uxtw Xd, Wn` is `ubfm Xd, Xn, #0, #31`.
        if !dst.wide {
            return None;
        }
        return Some(0xD340_0000 | 31 << 10 | src.num << 5 | dst.num);
    }
    if mnemonic == "sxtw" && !dst.wide {
        return None;
    }
    // `uxtb`/`uxth` always write the 32-bit form, whatever the destination is spelled.
    let wide = signed && dst.wide;
    let base = if signed { 0x1300_0000 } else { 0x5300_0000 };
    let n = u32::from(wide) << 22;
    Some(u32::from(wide) << 31 | base | n | last << 10 | src.num << 5 | dst.num)
}

/// `rev`/`rev16`/`rev32`/`clz`/`cls`/`rbit Rd, Rn`.
fn encode_unary_int(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    let (dst, src) = (gp(dst)?, gp(src)?);
    if !dst.plain() && !dst.zr || dst.sp || src.sp || dst.wide != src.wide {
        return None;
    }
    let opcode = match (mnemonic, dst.wide) {
        ("rbit", _) => 0,
        ("rev16", _) => 1,
        ("rev", false) => 2,
        ("rev", true) => 3,
        ("rev32", true) => 2,
        ("clz", _) => 4,
        ("cls", _) => 5,
        _ => return None,
    };
    Some(dst.sf() | 0x5AC0_0000 | opcode << 10 | src.num << 5 | dst.num)
}

/// Three-register arithmetic: `mul`/`mneg`/`sdiv`/`udiv`/`smulh`/`umulh`/`smull`/`umull`/...
fn encode_mul3(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, first, second] = ops else { return None };
    let (dst, first, second) = (gp(dst)?, gp(first)?, gp(second)?);
    if dst.sp || first.sp || second.sp {
        return None;
    }
    let long = matches!(mnemonic, "smull" | "umull" | "smnegl" | "umnegl");
    let high = matches!(mnemonic, "smulh" | "umulh");
    if long {
        if !dst.wide || first.wide || second.wide {
            return None;
        }
    } else if dst.wide != first.wide || dst.wide != second.wide || high && !dst.wide {
        return None;
    }
    let base = match mnemonic {
        "mul" => 0x1B00_7C00,
        "mneg" => 0x1B00_FC00,
        "sdiv" => 0x1AC0_0C00,
        "udiv" => 0x1AC0_0800,
        "smulh" => 0x9B40_7C00,
        "umulh" => 0x9BC0_7C00,
        "smull" => 0x9B20_7C00,
        "umull" => 0x9BA0_7C00,
        "smnegl" => 0x9B20_FC00,
        _ => 0x9BA0_FC00,
    };
    Some((if long || high { 0 } else { dst.sf() }) | base | second.num << 16 | first.num << 5 | dst.num)
}

/// Four-register multiply-accumulate: `madd`/`msub`/`smaddl`/`smsubl`/`umaddl`/`umsubl`.
fn encode_mul4(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, first, second, acc] = ops else { return None };
    let (dst, first, second, acc) = (gp(dst)?, gp(first)?, gp(second)?, gp(acc)?);
    if dst.sp || first.sp || second.sp || acc.sp {
        return None;
    }
    let long = !matches!(mnemonic, "madd" | "msub");
    if long {
        if !dst.wide || first.wide || second.wide || !acc.wide {
            return None;
        }
    } else if dst.wide != first.wide || dst.wide != second.wide || dst.wide != acc.wide {
        return None;
    }
    let base = match mnemonic {
        "madd" => 0x1B00_0000,
        "msub" => 0x1B00_8000,
        "smaddl" => 0x9B20_0000,
        "smsubl" => 0x9B20_8000,
        "umaddl" => 0x9BA0_0000,
        _ => 0x9BA0_8000,
    };
    Some((if long { 0 } else { dst.sf() }) | base | second.num << 16 | acc.num << 10 | first.num << 5 | dst.num)
}

/// `csel`/`csinc`/`csinv`/`csneg Rd, Rn, Rm, cond`.
fn encode_cond_select(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, first, second, condition] = ops else { return None };
    let (dst, first, second) = (gp(dst)?, gp(first)?, gp(second)?);
    if dst.sp || first.sp || second.sp || dst.wide != first.wide || dst.wide != second.wide {
        return None;
    }
    let base = match mnemonic {
        "csel" => 0x1A80_0000,
        "csinc" => 0x1A80_0400,
        "csinv" => 0x5A80_0000,
        _ => 0x5A80_0400,
    };
    Some(dst.sf() | base | second.num << 16 | cond(condition)? << 12 | first.num << 5 | dst.num)
}

/// `cset`/`csetm Rd, cond` (CSINC/CSINV of the zero register on the inverted condition).
fn encode_cset(mask: bool, ops: &[&str]) -> Option<u32> {
    let [dst, condition] = ops else { return None };
    let dst = gp(dst)?;
    let condition = cond(condition)?;
    if dst.sp || condition >= 14 {
        return None;
    }
    let base = if mask { 0x5A9F_03E0 } else { 0x1A9F_07E0 };
    Some(dst.sf() | base | (condition ^ 1) << 12 | dst.num)
}

/// `cinc`/`cinv`/`cneg Rd, Rn, cond`.
fn encode_cond_inc(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [dst, src, condition] = ops else { return None };
    let (dst, src) = (gp(dst)?, gp(src)?);
    let condition = cond(condition)?;
    if dst.sp || src.sp || dst.wide != src.wide || condition >= 14 {
        return None;
    }
    let base = match mnemonic {
        "cinc" => 0x1A80_0400,
        "cinv" => 0x5A80_0000,
        _ => 0x5A80_0400,
    };
    Some(dst.sf() | base | src.num << 16 | (condition ^ 1) << 12 | src.num << 5 | dst.num)
}

/// `ccmp`/`ccmn Rn, <Rm | #imm5>, #nzcv, cond`.
fn encode_cond_compare(compare: bool, ops: &[&str]) -> Option<u32> {
    let [first, second, nzcv, condition] = ops else { return None };
    let first = gp(first)?;
    let nzcv = u32::try_from(imm(nzcv)?).ok().filter(|&n| n < 16)?;
    let condition = cond(condition)?;
    if first.sp {
        return None;
    }
    let base = first.sf() | if compare { 0x7A40_0000 } else { 0x3A40_0000 };
    let tail = condition << 12 | first.num << 5 | nzcv;
    if let Some(second) = gp(second) {
        if second.sp || second.wide != first.wide {
            return None;
        }
        return Some(base | second.num << 16 | tail);
    }
    let value = u32::try_from(imm(second)?).ok().filter(|&n| n < 32)?;
    Some(base | 0x800 | value << 16 | tail)
}

/// The addressing modes of a load or store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Addr<'a> {
    /// `[Xn]` or `[Xn, #off]`.
    Offset(Gp, i64),
    /// `[Xn, #off]!`.
    Pre(Gp, i64),
    /// `[Xn], #off`.
    Post(Gp, i64),
    /// `[Xn, Rm{, extend #amount}]`.
    Index(Gp, Gp, Option<Shift>),
    /// `[Xn, sym@PAGEOFF]` or `sym@GOTPAGEOFF`.
    Symbol(Gp, Symbol<'a>),
}

/// Parses the memory operand (and a trailing post-index immediate) from the operand tail.
fn address<'a>(tail: &[&'a str]) -> Option<Addr<'a>> {
    let (text, post) = match tail {
        [text] => (*text, None),
        [text, post] => (*text, Some(*post)),
        _ => return None,
    };
    let (inner, after) = text.strip_prefix('[')?.split_once(']')?;
    let mut parts = inner.split(',');
    let base = gp(parts.next()?.trim_ascii())?;
    if !base.wide || base.zr {
        return None;
    }
    let second = parts.next().map(str::trim_ascii);
    let third = parts.next().map(str::trim_ascii);
    if parts.next().is_some() {
        return None;
    }
    let writeback = match after.trim_ascii() {
        "" => false,
        "!" => true,
        _ => return None,
    };
    if let Some(post) = post {
        if second.is_some() || writeback {
            return None;
        }
        return Some(Addr::Post(base, imm(post)?));
    }
    let Some(second) = second else {
        return (!writeback).then_some(Addr::Offset(base, 0));
    };
    if second.starts_with('#') {
        if third.is_some() {
            return None;
        }
        let offset = imm(second)?;
        return Some(if writeback {
            Addr::Pre(base, offset)
        } else {
            Addr::Offset(base, offset)
        });
    }
    if writeback {
        return None;
    }
    if let Some(index) = gp(second) {
        if index.sp {
            return None;
        }
        let modifier = match third {
            Some(text) => Some(shift(text)?),
            None => None,
        };
        return Some(Addr::Index(base, index, modifier));
    }
    if third.is_some() {
        return None;
    }
    Some(Addr::Symbol(base, symbol(second)?))
}

/// The size class of a load/store transfer register.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Transfer {
    /// The `size` field.
    size: u32,
    /// SIMD&FP register (`V` bit).
    vector: bool,
    /// `opc` field for a store; a load adds one (and `ldrs*` use 2 or 3).
    opc: u32,
    /// log2 of the access size in bytes.
    scale: u32,
    /// Register number.
    rt: u32,
}

/// Loads and stores of one register.
fn encode_load_store<'a>(mnemonic: &str, ops: &[&'a str]) -> Option<Encoded<'a>> {
    let [target, tail @ ..] = ops else { return None };
    let unscaled = mnemonic.contains("ur");
    let load = mnemonic.starts_with("ld");
    let suffix = mnemonic.trim_start_matches("ldur").trim_start_matches("stur");
    let suffix = suffix.trim_start_matches("ldr").trim_start_matches("str");
    let signed = suffix.starts_with('s');
    let transfer = if let Some(fp) = fp(target) {
        if !suffix.is_empty() {
            return None;
        }
        let (size, opc, scale) = match fp.size {
            FpSize::B => (0, 0, 0),
            FpSize::H => (1, 0, 1),
            FpSize::S => (2, 0, 2),
            FpSize::D => (3, 0, 3),
            FpSize::Q => (0, 2, 4),
        };
        Transfer {
            size,
            vector: true,
            opc: opc + u32::from(load),
            scale,
            rt: fp.num,
        }
    } else {
        let reg = gp(target)?;
        if reg.sp {
            return None;
        }
        let (size, opc, needs_w) = match suffix {
            "" => (if reg.wide { 3 } else { 2 }, u32::from(load), false),
            "b" => (0, u32::from(load), true),
            "h" => (1, u32::from(load), true),
            "sb" => (0, if reg.wide { 2 } else { 3 }, false),
            "sh" => (1, if reg.wide { 2 } else { 3 }, false),
            "sw" => (2, 2, false),
            _ => return None,
        };
        if (needs_w && reg.wide) || (suffix == "sw" && !reg.wide) || (signed && !load) {
            return None;
        }
        Transfer {
            size,
            vector: false,
            opc,
            scale: size,
            rt: reg.num,
        }
    };
    let head = transfer.size << 30 | u32::from(transfer.vector) << 26 | transfer.opc << 22 | transfer.rt;
    // `ldr Xt, label` / `ldr Dt, label`.
    if !unscaled && matches!(mnemonic, "ldr" | "ldrsw") && tail.len() == 1 && !tail[0].starts_with('[') {
        let sym = symbol(tail[0])?;
        let word = match (transfer.vector, transfer.scale, mnemonic) {
            (false, 2, "ldrsw") => 0x9800_0000 | transfer.rt,
            (false, 2, _) => 0x1800_0000 | transfer.rt,
            (false, 3, _) => 0x5800_0000 | transfer.rt,
            (true, 2, _) => 0x1C00_0000 | transfer.rt,
            (true, 3, _) => 0x5C00_0000 | transfer.rt,
            (true, 4, _) => 0x9C00_0000 | transfer.rt,
            _ => return None,
        };
        return with_fixup(word, FixupKind::Branch19, sym);
    }
    let scale = 1i64 << transfer.scale;
    match address(tail)? {
        Addr::Offset(base, offset) => {
            if !unscaled && offset >= 0 && offset % scale == 0 && offset / scale <= 0xFFF {
                let word = 0x3900_0000 | head | ((offset / scale) as u32) << 10 | base.num << 5;
                return plain(word);
            }
            if (-256..256).contains(&offset) {
                let word = 0x3800_0000 | head | ((offset as u32) & 0x1FF) << 12 | base.num << 5;
                return plain(word);
            }
            None
        }
        Addr::Pre(base, offset) | Addr::Post(base, offset) => {
            if unscaled || !(-256..256).contains(&offset) || (!base.sp && base.num == transfer.rt && !transfer.vector) {
                return None;
            }
            let index = if matches!(address(tail)?, Addr::Pre(..)) { 0b11 } else { 0b01 };
            let word = 0x3800_0000 | head | ((offset as u32) & 0x1FF) << 12 | index << 10 | base.num << 5;
            plain(word)
        }
        Addr::Index(base, index, modifier) => {
            if unscaled {
                return None;
            }
            let (option, amount) = match modifier {
                None => (3, None),
                Some(Shift { kind: Modifier::Lsl, amount }) => (3, amount),
                Some(Shift { kind: Modifier::Ext(option @ (2 | 3 | 6 | 7)), amount }) => (option, amount),
                Some(_) => return None,
            };
            if index.wide != (option & 1 == 1) {
                return None;
            }
            let set = match amount {
                None | Some(0) if transfer.scale != 0 => 0,
                None => 0,
                Some(a) if a == transfer.scale => 1,
                Some(0) => 1,
                Some(_) => return None,
            };
            let word = 0x3820_0800 | head | index.num << 16 | option << 13 | set << 12 | base.num << 5;
            plain(word)
        }
        Addr::Symbol(base, sym) => {
            if unscaled {
                return None;
            }
            let kind = match sym.suffix {
                "PAGEOFF" => FixupKind::PageOff12,
                "GOTPAGEOFF" if mnemonic == "ldr" && transfer.size == 3 && !transfer.vector => {
                    FixupKind::GotPageOff12
                }
                _ => return None,
            };
            with_fixup(0x3900_0000 | head | base.num << 5, kind, sym)
        }
    }
}

/// `ldp`/`stp`/`ldnp`/`stnp`/`ldpsw` of two registers.
fn encode_pair(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [first, second, tail @ ..] = ops else { return None };
    let load = mnemonic.starts_with("ld");
    let (opc, vector, scale, rt, rt2) = if let (Some(a), Some(b)) = (fp(first), fp(second)) {
        if a.size != b.size || mnemonic == "ldpsw" {
            return None;
        }
        let (opc, scale) = match a.size {
            FpSize::S => (0, 2),
            FpSize::D => (1, 3),
            FpSize::Q => (2, 4),
            _ => return None,
        };
        (opc, true, scale, a.num, b.num)
    } else {
        let (a, b) = (gp(first)?, gp(second)?);
        if a.wide != b.wide || a.sp || b.sp {
            return None;
        }
        match (mnemonic, a.wide) {
            ("ldpsw", true) => (1, false, 2, a.num, b.num),
            ("ldpsw", false) => return None,
            (_, true) => (2, false, 3, a.num, b.num),
            (_, false) => (0, false, 2, a.num, b.num),
        }
    };
    let (mode, base, offset) = match address(tail)? {
        Addr::Offset(base, offset) => (0b010u32, base, offset),
        Addr::Pre(base, offset) => (0b011, base, offset),
        Addr::Post(base, offset) => (0b001, base, offset),
        _ => return None,
    };
    let non_temporal = mnemonic.ends_with("np");
    if non_temporal && mode != 0b010 {
        return None;
    }
    let step = 1i64 << scale;
    if offset % step != 0 || !(-64..64).contains(&(offset / step)) {
        return None;
    }
    // Writing back into a transferred register, or loading one register twice, is
    // UNPREDICTABLE; `as` refuses it.
    if load && rt == rt2 && (vector || mode == 0b010) && !non_temporal {
        return None;
    }
    if mode != 0b010 && !vector && !base.sp && (base.num == rt || base.num == rt2) {
        return None;
    }
    let imm7 = ((offset / step) as u32) & 0x7F;
    let mode = if non_temporal { 0 } else { mode };
    Some(
        opc << 30
            | 0x2800_0000
            | u32::from(vector) << 26
            | mode << 23
            | u32::from(load) << 22
            | imm7 << 15
            | rt2 << 10
            | base.num << 5
            | rt,
    )
}

/// `br`/`blr Xn` and `ret {Xn}`.
fn encode_branch_register(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let target = match ops {
        [] if mnemonic == "ret" => 30,
        [reg] => {
            let reg = gp(reg)?;
            if !reg.wide || !reg.plain() {
                return None;
            }
            reg.num
        }
        _ => return None,
    };
    let base = match mnemonic {
        "br" => 0xD61F_0000,
        "blr" => 0xD63F_0000,
        _ => 0xD65F_0000,
    };
    Some(base | target << 5)
}

/// `svc`/`udf`/`brk`/`hlt #imm16`.
fn encode_exception(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let [value] = ops else { return None };
    let value = u32::try_from(imm(value)?).ok().filter(|&v| v <= 0xFFFF)?;
    Some(match mnemonic {
        "svc" => 0xD400_0001 | value << 5,
        "brk" => 0xD420_0000 | value << 5,
        "hlt" => 0xD440_0000 | value << 5,
        _ => value,
    })
}

/// `dmb`/`dsb`/`isb` with an optional barrier option.
fn encode_barrier(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    let option = match ops {
        [] if mnemonic == "isb" => 15,
        [name] => match *name {
            "oshld" => 1,
            "oshst" => 2,
            "osh" => 3,
            "nshld" => 5,
            "nshst" => 6,
            "nsh" => 7,
            "ishld" => 9,
            "ishst" => 10,
            "ish" => 11,
            "ld" => 13,
            "st" => 14,
            "sy" => 15,
            _ => return None,
        },
        _ => return None,
    };
    let base = match mnemonic {
        "dmb" => 0xD503_30BF,
        "dsb" => 0xD503_309F,
        _ => 0xD503_30DF,
    };
    Some(base | option << 8)
}

/// `mrs Xt, sysreg` / `msr sysreg, Xt` for the registers the runtime reads.
fn encode_system_register(read: bool, ops: &[&str]) -> Option<u32> {
    let (reg, name) = match ops {
        [reg, name] if read => (*reg, *name),
        [name, reg] => (*reg, *name),
        _ => return None,
    };
    let reg = gp(reg)?;
    if !reg.wide || reg.sp {
        return None;
    }
    // (op0, op1, crn, crm, op2)
    let (op0, op1, crn, crm, op2): (u32, u32, u32, u32, u32) = match name {
        "nzcv" => (3, 3, 4, 2, 0),
        "fpcr" => (3, 3, 4, 4, 0),
        "fpsr" => (3, 3, 4, 4, 1),
        "tpidr_el0" => (3, 3, 13, 0, 2),
        "cntvct_el0" => (3, 3, 14, 0, 2),
        "cntfrq_el0" => (3, 3, 14, 0, 0),
        _ => return None,
    };
    Some(
        0xD510_0000
            | u32::from(read) << 21
            | (op0 - 2) << 19
            | op1 << 16
            | crn << 12
            | crm << 8
            | op2 << 5
            | reg.num,
    )
}

/// Scalar floating-point arithmetic, comparison and conversion.
fn encode_float(mnemonic: &str, ops: &[&str]) -> Option<u32> {
    match mnemonic {
        "fmov" => encode_fmov(ops),
        "fabs" | "fneg" | "fsqrt" | "frintn" | "frintp" | "frintm" | "frintz" | "frinta"
        | "frintx" | "frinti" => {
            let [dst, src] = ops else { return None };
            let (dst, src) = (fp(dst)?, fp(src)?);
            if dst.size != src.size {
                return None;
            }
            let opcode = match mnemonic {
                "fabs" => 1,
                "fneg" => 2,
                "fsqrt" => 3,
                "frintn" => 8,
                "frintp" => 9,
                "frintm" => 10,
                "frintz" => 11,
                "frinta" => 12,
                "frintx" => 14,
                _ => 15,
            };
            Some(0x1E20_4000 | dst.size.ftype()? << 22 | opcode << 15 | src.num << 5 | dst.num)
        }
        "fadd" | "fsub" | "fmul" | "fdiv" | "fmax" | "fmin" | "fmaxnm" | "fminnm" | "fnmul" => {
            let [dst, first, second] = ops else { return None };
            let (dst, first, second) = (fp(dst)?, fp(first)?, fp(second)?);
            if dst.size != first.size || dst.size != second.size {
                return None;
            }
            let base = match mnemonic {
                "fmul" => 0x1E20_0800,
                "fdiv" => 0x1E20_1800,
                "fadd" => 0x1E20_2800,
                "fsub" => 0x1E20_3800,
                "fmax" => 0x1E20_4800,
                "fmin" => 0x1E20_5800,
                "fmaxnm" => 0x1E20_6800,
                "fminnm" => 0x1E20_7800,
                _ => 0x1E20_8800,
            };
            Some(base | dst.size.ftype()? << 22 | second.num << 16 | first.num << 5 | dst.num)
        }
        "fmadd" | "fmsub" | "fnmadd" | "fnmsub" => {
            let [dst, first, second, acc] = ops else { return None };
            let (dst, first, second, acc) = (fp(dst)?, fp(first)?, fp(second)?, fp(acc)?);
            if dst.size != first.size || dst.size != second.size || dst.size != acc.size {
                return None;
            }
            let base = match mnemonic {
                "fmadd" => 0x1F00_0000,
                "fmsub" => 0x1F00_8000,
                "fnmadd" => 0x1F20_0000,
                _ => 0x1F20_8000,
            };
            Some(base | dst.size.ftype()? << 22 | second.num << 16 | acc.num << 10 | first.num << 5 | dst.num)
        }
        "fcmp" | "fcmpe" => {
            let [first, second] = ops else { return None };
            let first = fp(first)?;
            let signalling = if mnemonic == "fcmpe" { 0x10 } else { 0 };
            let base = 0x1E20_2000 | first.size.ftype()? << 22 | first.num << 5 | signalling;
            if let Some(second) = fp(second) {
                if second.size != first.size {
                    return None;
                }
                return Some(base | second.num << 16);
            }
            let zero = matches!(*second, "#0.0");
            zero.then_some(base | 0x8)
        }
        "fccmp" | "fccmpe" => {
            let [first, second, nzcv, condition] = ops else { return None };
            let (first, second) = (fp(first)?, fp(second)?);
            let nzcv = u32::try_from(imm(nzcv)?).ok().filter(|&n| n < 16)?;
            if first.size != second.size {
                return None;
            }
            let signalling = if mnemonic == "fccmpe" { 0x10 } else { 0 };
            Some(
                0x1E20_0400
                    | first.size.ftype()? << 22
                    | second.num << 16
                    | cond(condition)? << 12
                    | first.num << 5
                    | signalling
                    | nzcv,
            )
        }
        "fcsel" => {
            let [dst, first, second, condition] = ops else { return None };
            let (dst, first, second) = (fp(dst)?, fp(first)?, fp(second)?);
            if dst.size != first.size || dst.size != second.size {
                return None;
            }
            Some(
                0x1E20_0C00
                    | dst.size.ftype()? << 22
                    | second.num << 16
                    | cond(condition)? << 12
                    | first.num << 5
                    | dst.num,
            )
        }
        "fcvt" => {
            let [dst, src] = ops else { return None };
            let (dst, src) = (fp(dst)?, fp(src)?);
            if dst.size == src.size {
                return None;
            }
            Some(0x1E22_4000 | src.size.ftype()? << 22 | dst.size.ftype()? << 15 | src.num << 5 | dst.num)
        }
        "scvtf" | "ucvtf" => {
            let [dst, src] = ops else { return None };
            let (dst, src) = (fp(dst)?, gp(src)?);
            if src.sp {
                return None;
            }
            let base = if mnemonic == "scvtf" { 0x1E22_0000 } else { 0x1E23_0000 };
            Some(src.sf() | base | dst.size.ftype()? << 22 | src.num << 5 | dst.num)
        }
        "fcvtzs" | "fcvtzu" | "fcvtas" | "fcvtau" | "fcvtms" | "fcvtmu" | "fcvtps" | "fcvtpu"
        | "fcvtns" | "fcvtnu" => {
            let [dst, src] = ops else { return None };
            let (dst, src) = (gp(dst)?, fp(src)?);
            if dst.sp {
                return None;
            }
            let base = match mnemonic {
                "fcvtns" => 0x1E20_0000,
                "fcvtnu" => 0x1E21_0000,
                "fcvtps" => 0x1E28_0000,
                "fcvtpu" => 0x1E29_0000,
                "fcvtms" => 0x1E30_0000,
                "fcvtmu" => 0x1E31_0000,
                "fcvtas" => 0x1E24_0000,
                "fcvtau" => 0x1E25_0000,
                "fcvtzs" => 0x1E38_0000,
                _ => 0x1E39_0000,
            };
            Some(dst.sf() | base | src.size.ftype()? << 22 | src.num << 5 | dst.num)
        }
        _ => None,
    }
}

/// `fmov` between FP registers, between FP and general registers, and of an 8-bit immediate.
fn encode_fmov(ops: &[&str]) -> Option<u32> {
    let [dst, src] = ops else { return None };
    if let (Some(dst), Some(src)) = (fp(dst), fp(src)) {
        if dst.size != src.size {
            return None;
        }
        return Some(0x1E20_4000 | dst.size.ftype()? << 22 | src.num << 5 | dst.num);
    }
    if let (Some(dst), Some(src)) = (fp(dst), gp(src)) {
        // `fmov Dd, Xn` / `fmov Sd, Wn`.
        let matches = (dst.size == FpSize::D && src.wide) || (dst.size == FpSize::S && !src.wide);
        if !matches || src.sp {
            return None;
        }
        return Some(src.sf() | 0x1E27_0000 | u32::from(src.wide) << 22 | src.num << 5 | dst.num);
    }
    if let (Some(dst), Some(src)) = (gp(dst), fp(src)) {
        let matches = (src.size == FpSize::D && dst.wide) || (src.size == FpSize::S && !dst.wide);
        if !matches || dst.sp {
            return None;
        }
        return Some(dst.sf() | 0x1E26_0000 | u32::from(dst.wide) << 22 | src.num << 5 | dst.num);
    }
    let dst = fp(dst)?;
    let text = src.strip_prefix('#')?;
    let value: f64 = text.parse().ok()?;
    if value == 0.0 && value.is_sign_positive() {
        // `fmov Dd, #0.0` is not an 8-bit immediate: `as` writes `fmov Dd, xzr`.
        return match dst.size {
            FpSize::D => Some(0x9E67_03E0 | dst.num),
            FpSize::S => Some(0x1E27_03E0 | dst.num),
            _ => None,
        };
    }
    let imm8 = float_imm8(value)?;
    Some(0x1E20_1000 | dst.size.ftype()? << 22 | imm8 << 13 | dst.num)
}

/// The 8-bit floating-point immediate (`±n/16 * 2^e`, n in 16..=31, e in -3..=4) of `value`.
fn float_imm8(value: f64) -> Option<u32> {
    let bits = value.to_bits();
    let sign = (bits >> 63) as u32;
    let exponent = ((bits >> 52) & 0x7FF) as i32 - 1023;
    let mantissa = bits & ((1 << 52) - 1);
    if mantissa & ((1 << 48) - 1) != 0 || !(-3..=4).contains(&exponent) {
        return None;
    }
    let frac = (mantissa >> 48) as u32;
    let biased = (exponent + 3) as u32;
    let bcd = ((!(biased >> 2) & 1) << 2) | (biased & 3);
    Some(sign << 7 | bcd << 4 | frac)
}

#[cfg(test)]
mod tests {
    use super::encode_instruction;

    /// The `(target, addend)` a case line names, computed independently of the encoder.
    fn expected_target(line: &str) -> Option<(&str, i64)> {
        let operand = if line.contains('@') {
            line.split(|c| c == ' ' || c == ',' || c == '[' || c == ']')
                .find(|piece| piece.contains('@'))?
        } else {
            line.rsplit(' ').next()?
        };
        let name = operand.split('@').next()?;
        let cut = name.bytes().skip(1).position(|b| b == b'+' || b == b'-').map(|p| p + 1);
        match cut {
            Some(at) => Some((&name[..at], name[at..].trim_start_matches('+').parse().ok()?)),
            None => Some((name, 0)),
        }
    }

    /// Every word and fixup here is what Apple's `as` produced for the same line.
    #[test]
    fn encodings_match_the_assembler() {
        let mut wrong = Vec::new();
        for (line, word, kind) in super::tests_table::CASES {
            let got = encode_instruction(line);
            let ok = match got {
                Some(encoded) => {
                    encoded.word == *word
                        && encoded.fixup.map(|f| f.kind) == *kind
                        && match (encoded.fixup, expected_target(line)) {
                            (Some(fixup), Some((target, addend))) => {
                                fixup.target == target && fixup.addend == addend
                            }
                            (None, _) => true,
                            _ => false,
                        }
                }
                None => false,
            };
            if !ok {
                let got = got.map(|e| format!("{:08x} {:?}", e.word, e.fixup));
                wrong.push(format!("{line}: as {word:08x} {kind:?}, encode {got:?}"));
            }
        }
        assert!(wrong.is_empty(), "{} of {} mismatches:\n{}", wrong.len(), super::tests_table::CASES.len(), wrong.join("\n"));
    }

    /// Lines `as` rejects must not get a word of our own invention.
    #[test]
    fn lines_the_assembler_rejects_stay_unhandled() {
        let invented: Vec<String> = super::tests_table::REJECTED
            .iter()
            .filter_map(|line| encode_instruction(line).map(|e| format!("{line}: {:08x}", e.word)))
            .collect();
        assert!(invented.is_empty(), "{} invented:\n{}", invented.len(), invented.join("\n"));
    }

    /// Forms `as` rejects must not be encoded.
    #[test]
    fn invalid_forms_are_refused() {
        for line in [
            "add x0, x1, #4097",
            "ldr x0, [x1, #-257]",
            "ldp x1, x2, [x3, #4]",
            "ldr x0, [x0, #8]!",
            "mov x0, #0x12345",
            "b",
            "frob x0",
            "ldr x0, [x1",
            "cset x0, al",
        ] {
            assert_eq!(encode_instruction(line), None, "{line}");
        }
    }
}

#[cfg(test)]
#[path = "encode_cases.rs"]
mod tests_table;
