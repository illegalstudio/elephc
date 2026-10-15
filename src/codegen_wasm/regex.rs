//! Purpose:
//! `preg_match()` on `wasm32-wasi`: the compile-time pattern compiler and the bytecode its
//! runtime VM executes.
//!
//! Called from:
//! - `builtins` compiles the LITERAL pattern here and refuses what the VM cannot run.
//! - `plan` lays the compiled program out in static data.
//! - `regex_vm` is the WAT backtracking interpreter that runs it.
//!
//! Key details:
//! - The pattern is compiled when the MODULE is built, not at runtime. php requires a string
//!   here and the checker already demands a literal in AOT mode, so the delimiters, the flags
//!   and the whole syntax tree are settled before any WAT is written — which is what makes a
//!   PARTIAL engine honest. A construct this module does not implement is a compile error
//!   naming it, never a wrong answer at runtime.
//! - The subset is the one that can be executed by a backtracking VM with no capture stack:
//!   literals, `.`, character classes, the `\d \w \s` shorthands and their negations, `^ $`
//!   `\A \z \b \B`, alternation, groups, and the greedy and lazy forms of `* + ? {n,m}`.
//!   Backreferences, lookaround, named and conditional groups, possessive quantifiers,
//!   recursion and unicode properties are each refused by name.
//! - `$matches` is refused rather than ignored. Capturing is the one feature whose absence a
//!   program cannot detect from the return value, so accepting the argument and leaving the
//!   array untouched would be the worst available answer.
//! - Bytes, not codepoints. php's `u` flag switches PCRE to UTF-8 mode, where `.` consumes a
//!   whole codepoint and classes match by property; this engine walks bytes, so `u` is
//!   refused. Without it PCRE is also byte-oriented, which is the common case.

/// One VM instruction. Laid out as three little-endian `i32`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Inst {
    /// Match one exact byte.
    Char(u8),
    /// Match one byte, case-folded. The operand is already lowercased.
    CharFolded(u8),
    /// Match any byte; under `s` a newline counts, otherwise it does not.
    Any { dot_all: bool },
    /// Match against the 256-bit bitmap at this index.
    Class(u32),
    /// Try the first target; on failure resume at the second.
    Split(u32, u32),
    /// Continue at this address.
    Jump(u32),
    /// Start of subject, or just after a newline under `m`.
    Bol { multiline: bool },
    /// End of subject, before a trailing newline, or just before a newline under `m`.
    Eol { multiline: bool },
    /// End of subject, with no allowance for a trailing newline. This is `\z`, which php
    /// distinguishes from `$`: `preg_match('/c\z/', "c\n")` is 0 where `/c$/` is 1.
    EndOfSubject,
    /// A word boundary, or its negation.
    WordBoundary { negated: bool },
    /// Record the current offset in mark slot `k`, at the head of a loop body.
    Mark(u32),
    /// Fail unless the offset has moved since mark slot `k` was set.
    ///
    /// This is what stops a loop whose body can match nothing from iterating forever:
    /// `(a?)*` would otherwise return to the top at the same offset and do it again.
    Advanced(u32),
    /// The pattern matched.
    Match,
}

impl Inst {
    /// The three words this instruction occupies in the emitted program.
    fn words(self) -> [i32; 3] {
        match self {
            Inst::Char(byte) => [0, i32::from(byte), 0],
            Inst::CharFolded(byte) => [1, i32::from(byte), 0],
            Inst::Any { dot_all } => [2, i32::from(dot_all), 0],
            Inst::Class(index) => [3, index as i32, 0],
            Inst::Split(first, second) => [4, first as i32, second as i32],
            Inst::Jump(target) => [5, target as i32, 0],
            Inst::Bol { multiline } => [6, i32::from(multiline), 0],
            Inst::Eol { multiline } => [7, i32::from(multiline), 0],
            Inst::EndOfSubject => [8, 0, 0],
            Inst::WordBoundary { negated } => [9, i32::from(negated), 0],
            Inst::Mark(slot) => [10, slot as i32, 0],
            Inst::Advanced(slot) => [11, slot as i32, 0],
            Inst::Match => [12, 0, 0],
        }
    }
}

/// A compiled pattern: the instruction list and the class bitmaps it refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Program {
    pub(super) insts: Vec<Inst>,
    pub(super) classes: Vec<[u8; 32]>,
    /// How many progress marks the VM has to carry per backtrack frame.
    pub(super) marks: u32,
}

impl Program {
    /// The instruction words, ready for a data segment.
    pub(super) fn inst_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.insts.len() * 12);
        for inst in &self.insts {
            for word in inst.words() {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        bytes
    }

    /// The class bitmaps, concatenated in index order.
    pub(super) fn class_bytes(&self) -> Vec<u8> {
        self.classes.iter().flatten().copied().collect()
    }
}

/// Largest total repetition a bounded quantifier may expand to.
///
/// `{n,m}` is compiled by repeating the body, so a pattern like `(a{100}){100}` would expand
/// to ten thousand instructions and a nested one far past that. The bound is on the whole
/// program rather than on any one quantifier, so it cannot be evaded by nesting.
const MAX_PROGRAM: usize = 4096;

/// Progress marks the VM carries per backtrack frame.
///
/// One is needed per LOOP WHOSE BODY CAN MATCH NOTHING, and only those: an ordinary `a+` can
/// never return to its own head without advancing, so it needs no check and no slot. Real
/// patterns have none or one; the fuzzer that found this produces them constantly, which is
/// why the bound is stated rather than assumed.
pub(super) const MAX_MARKS: u32 = 8;

/// Compiles a php pattern — delimiters, body and flags — or says what it cannot run.
pub(super) fn compile(pattern: &str) -> Result<Program, String> {
    let (body, flags) = split_delimiters(pattern)?;
    let mut parser = Parser {
        bytes: body.as_bytes(),
        at: 0,
        flags,
        insts: Vec::new(),
        classes: Vec::new(),
        marks: 0,
    };
    parser.parse_alternation()?;
    if parser.at < parser.bytes.len() {
        return Err(format!(
            "unbalanced `)` at offset {} in the pattern",
            parser.at
        ));
    }
    parser.insts.push(Inst::Match);
    if parser.insts.len() > MAX_PROGRAM {
        return Err(format!(
            "the pattern compiles to {} instructions, past the {MAX_PROGRAM} this target runs",
            parser.insts.len()
        ));
    }
    if parser.marks > MAX_MARKS {
        return Err(format!(
            "the pattern has {} loops whose body can match nothing, past the {MAX_MARKS} this \
             VM tracks progress for",
            parser.marks
        ));
    }
    Ok(Program {
        insts: parser.insts,
        classes: parser.classes,
        marks: parser.marks,
    })
}

/// Every LITERAL `preg_match` pattern in the module, in first-seen order.
///
/// Only a literal is collected, because only a literal can be compiled here — and the audit
/// refuses a computed one, so a pattern missing from this list is a pattern no call reaches.
pub(super) fn module_patterns(module: &crate::ir::Module) -> Vec<String> {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let mut patterns = Vec::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
    {
        for inst in &function.instructions {
            if inst.op != Op::RuntimeCall {
                continue;
            }
            let reaches = matches!(
                inst.immediate.as_ref(),
                Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::Function(RuntimeFnId::PregMatch)
                        | RuntimeCallTarget::ProfiledFunction {
                            target: RuntimeFnId::PregMatch,
                            ..
                        }
                ))
            );
            if !reaches {
                continue;
            }
            let Some(value) = inst.operands.first() else {
                continue;
            };
            let Some(bytes) = super::builtins::sprintf_literal_format(function, module, *value)
            else {
                continue;
            };
            let Ok(pattern) = String::from_utf8(bytes) else {
                continue;
            };
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
            }
        }
    }
    patterns
}

/// Lays every pattern's compiled program out in static data.
///
/// Returns the map the lowering looks an address up in, keyed by the pattern text, and the
/// advanced data cursor. A pattern that does not compile is SKIPPED rather than reported here:
/// the capability audit has already refused that call with the compiler's own message, and
/// failing twice would report it twice.
pub(super) fn emit_pattern_programs(
    wm: &mut super::wat::WatModule,
    module: &crate::ir::Module,
    mut cursor: u32,
) -> (std::collections::HashMap<String, (u32, u32)>, u32) {
    let mut placed = std::collections::HashMap::new();
    for pattern in module_patterns(module) {
        let Ok(program) = compile(&pattern) else {
            continue;
        };
        cursor = (cursor + 3) & !3;
        let insts = cursor;
        let inst_bytes = program.inst_bytes();
        cursor += inst_bytes.len() as u32;
        wm.add_data(super::wat::DataSegment {
            offset: insts,
            bytes: inst_bytes,
        });
        let classes = cursor;
        let class_bytes = program.class_bytes();
        cursor += class_bytes.len() as u32;
        if !class_bytes.is_empty() {
            wm.add_data(super::wat::DataSegment {
                offset: classes,
                bytes: class_bytes,
            });
        }
        placed.insert(pattern, (insts, classes));
    }
    (placed, cursor)
}

/// The flags a compiled pattern carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Flags {
    case_insensitive: bool,
    multiline: bool,
    dot_all: bool,
}

/// Splits `/body/flags` into its two halves, whatever delimiter php was given.
///
/// php accepts any non-alphanumeric, non-backslash, non-whitespace byte as the delimiter, and
/// pairs the four bracket styles rather than repeating them. Only the flags this engine can
/// honour are accepted; `u`, `x` and the rest are refused by name rather than ignored, because
/// ignoring a flag changes what the pattern MEANS.
fn split_delimiters(pattern: &str) -> Result<(&str, Flags), String> {
    let bytes = pattern.as_bytes();
    let Some(&open) = bytes.first() else {
        return Err("the pattern is empty; php requires delimiters".to_string());
    };
    if open.is_ascii_alphanumeric() || open == b'\\' || open.is_ascii_whitespace() {
        return Err(format!(
            "{:?} cannot be a php pattern delimiter",
            open as char
        ));
    }
    let close = match open {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        other => other,
    };
    let Some(end) = bytes.iter().rposition(|byte| *byte == close) else {
        return Err(format!(
            "no closing {:?} delimiter in the pattern",
            close as char
        ));
    };
    if end == 0 {
        return Err("no closing delimiter in the pattern".to_string());
    }
    let mut flags = Flags::default();
    for byte in &bytes[end + 1..] {
        match byte {
            b'i' => flags.case_insensitive = true,
            b'm' => flags.multiline = true,
            b's' => flags.dot_all = true,
            b'u' => {
                return Err(
                    "the `u` flag puts PCRE in UTF-8 mode, where `.` consumes a codepoint and \
                     classes match by property; this engine walks bytes"
                        .to_string(),
                )
            }
            b'x' => {
                return Err(
                    "the `x` flag changes how whitespace and `#` are read, which this \
                     compiler does not implement"
                        .to_string(),
                )
            }
            other => {
                return Err(format!(
                    "the `{}` flag is not one this target honours (i, m, s are)",
                    *other as char
                ))
            }
        }
    }
    Ok((&pattern[1..end], flags))
}

/// The recursive-descent pattern parser, emitting VM instructions as it goes.
struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    flags: Flags,
    insts: Vec<Inst>,
    classes: Vec<[u8; 32]>,
    marks: u32,
}

impl Parser<'_> {
    /// `alternation := concat ('|' concat)*`
    ///
    /// Each alternative is emitted with a `Split` before it and a `Jump` past the rest after
    /// it, both patched once the following branch's address is known.
    fn parse_alternation(&mut self) -> Result<(), String> {
        let start = self.insts.len();
        self.parse_concat()?;
        let mut jumps = Vec::new();
        while self.peek() == Some(b'|') {
            self.at += 1;
            // Make room for the split that has to precede the branch already emitted.
            self.insts.insert(start, Inst::Split(0, 0));
            for inst in &mut self.insts[start + 1..] {
                shift(inst, start, 1);
            }
            for jump in &mut jumps {
                if *jump >= start {
                    *jump += 1;
                }
            }
            self.insts.push(Inst::Jump(0));
            jumps.push(self.insts.len() - 1);
            let second = self.insts.len() as u32;
            self.insts[start] = Inst::Split(start as u32 + 1, second);
            self.parse_concat()?;
        }
        let end = self.insts.len() as u32;
        for jump in jumps {
            self.insts[jump] = Inst::Jump(end);
        }
        Ok(())
    }

    /// `concat := repeat*`, stopping at `|` or `)`.
    fn parse_concat(&mut self) -> Result<(), String> {
        while let Some(byte) = self.peek() {
            if byte == b'|' || byte == b')' {
                break;
            }
            self.parse_repeat()?;
        }
        Ok(())
    }

    /// `repeat := atom quantifier?`
    fn parse_repeat(&mut self) -> Result<(), String> {
        let start = self.insts.len();
        self.parse_atom()?;
        let Some(byte) = self.peek() else {
            return Ok(());
        };
        let (min, max) = match byte {
            b'*' => (0, None),
            b'+' => (1, None),
            b'?' => (0, Some(1)),
            b'{' => match self.parse_bound()? {
                Some(bound) => bound,
                None => return Ok(()), // a literal `{`, already consumed as an atom
            },
            _ => return Ok(()),
        };
        if matches!(byte, b'*' | b'+' | b'?') {
            self.at += 1;
        }
        let lazy = match self.peek() {
            Some(b'?') => {
                self.at += 1;
                true
            }
            Some(b'+') => {
                return Err(
                    "a possessive quantifier never gives back what it matched, which this \
                     backtracking VM has no way to express"
                        .to_string(),
                )
            }
            _ => false,
        };
        self.apply_quantifier(start, min, max, lazy)
    }

    /// Reads `{n}`, `{n,}` or `{n,m}`, or `None` when the brace was a literal.
    fn parse_bound(&mut self) -> Result<Option<(usize, Option<usize>)>, String> {
        let save = self.at;
        self.at += 1; // the '{'
        let min = self.parse_number();
        let bound = match self.peek() {
            Some(b'}') if min.is_some() => (min.unwrap(), Some(min.unwrap())),
            Some(b',') => {
                self.at += 1;
                let max = self.parse_number();
                if self.peek() != Some(b'}') {
                    self.at = save;
                    return Ok(None);
                }
                (min.unwrap_or(0), max)
            }
            _ => {
                self.at = save;
                return Ok(None);
            }
        };
        self.at += 1; // the '}'
        if let Some(max) = bound.1 {
            if max < bound.0 {
                return Err(format!(
                    "the quantifier {{{},{max}}} counts down, which php rejects too",
                    bound.0
                ));
            }
        }
        Ok(Some(bound))
    }

    /// Parses a run of ASCII digits at the cursor and advances past it, answering `None` when there
    /// is none.
    fn parse_number(&mut self) -> Option<usize> {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        (self.at > start).then(|| {
            std::str::from_utf8(&self.bytes[start..self.at])
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
        })
    }

    /// Wraps the instructions from `start` in the loops a quantifier needs.
    ///
    /// `{n,m}` is expanded by COPYING the body, which is why the program has a size bound: the
    /// VM has no repetition counter, so the count has to live in the instruction list.
    fn apply_quantifier(
        &mut self,
        start: usize,
        min: usize,
        max: Option<usize>,
        lazy: bool,
    ) -> Result<(), String> {
        let body: Vec<Inst> = self.insts[start..].to_vec();
        self.insts.truncate(start);
        let unbounded = max.is_none();
        let copies = max.unwrap_or(min);
        // The mandatory copies first: plain repetitions with no split at all.
        for _ in 0..min {
            self.append_shifted(&body, start);
            if self.insts.len() > MAX_PROGRAM {
                return Err(bound_error(self.insts.len()));
            }
        }
        if unbounded {
            // Then a star loop, whatever the minimum was: `a{2,}` is `aa` followed by `a*`,
            // not `aaa` followed by one. Emitting the entry split unconditionally is what
            // keeps the third copy optional.
            //
            // A body that can match NOTHING gets a progress check around it. Without one the
            // loop returns to its own head at the same offset and goes round again forever,
            // which a step budget turns into a silent non-match rather than a hang — and it
            // takes the whole search with it, including the alternatives that would have
            // matched. A differential fuzz against php-src found fifteen of those in four
            // hundred patterns.
            let guard = is_nullable(&body, start).then(|| {
                let slot = self.marks;
                self.marks += 1;
                slot
            });
            let entry = self.insts.len();
            self.insts.push(Inst::Split(0, 0));
            let body_at = self.insts.len();
            if let Some(slot) = guard {
                self.insts.push(Inst::Mark(slot));
            }
            self.append_shifted(&body, start);
            if self.insts.len() > MAX_PROGRAM {
                return Err(bound_error(self.insts.len()));
            }
            if let Some(slot) = guard {
                self.insts.push(Inst::Advanced(slot));
            }
            self.insts.push(Inst::Jump(entry as u32));
            let after = self.insts.len() as u32;
            self.insts[entry] = if lazy {
                Inst::Split(after, body_at as u32)
            } else {
                Inst::Split(body_at as u32, after)
            };
        } else {
            // The optional copies, each guarded by its own split to the end.
            let mut splits = Vec::new();
            for _ in min..copies {
                splits.push(self.insts.len());
                self.insts.push(Inst::Split(0, 0));
                self.append_shifted(&body, start);
                if self.insts.len() > MAX_PROGRAM {
                    return Err(bound_error(self.insts.len()));
                }
            }
            let after = self.insts.len() as u32;
            for split in splits {
                let next = split as u32 + 1;
                self.insts[split] = if lazy {
                    Inst::Split(after, next)
                } else {
                    Inst::Split(next, after)
                };
            }
        }
        Ok(())
    }

    /// Appends a copy of `body`, moving its addresses to where the copy now sits.
    ///
    /// The body was compiled in place at `origin` and every address inside it is ABSOLUTE, so
    /// a copy landing elsewhere has to move each one by the same delta. Getting this wrong is
    /// not a crash: the copy jumps into the original's instructions and matches a different
    /// language, which is why `a{3}` and `(?:ab){2}` are both in the measured table.
    fn append_shifted(&mut self, body: &[Inst], origin: usize) {
        let delta = self.insts.len() as i64 - origin as i64;
        for inst in body {
            let mut inst = *inst;
            rebase(&mut inst, delta);
            self.insts.push(inst);
        }
    }

    /// One atom: a group, a class, an escape, a dot, an anchor, or a literal byte.
    fn parse_atom(&mut self) -> Result<(), String> {
        let byte = self.bytes[self.at];
        match byte {
            b'(' => self.parse_group(),
            b'[' => self.parse_class(),
            b'.' => {
                self.at += 1;
                self.insts.push(Inst::Any {
                    dot_all: self.flags.dot_all,
                });
                Ok(())
            }
            b'^' => {
                self.at += 1;
                self.insts.push(Inst::Bol {
                    multiline: self.flags.multiline,
                });
                Ok(())
            }
            b'$' => {
                self.at += 1;
                self.insts.push(Inst::Eol {
                    multiline: self.flags.multiline,
                });
                Ok(())
            }
            b'\\' => self.parse_escape(),
            b'*' | b'+' | b'?' => Err(format!(
                "nothing precedes the `{}` quantifier at offset {}",
                byte as char, self.at
            )),
            _ => {
                self.at += 1;
                self.push_literal(byte);
                Ok(())
            }
        }
    }

    /// `(` ... `)`, capturing or not. Both compile to the same instructions.
    ///
    /// A capturing group is accepted and compiled as a plain group, which is sound only
    /// because `$matches` is refused: with nowhere for a capture to be observed, the two forms
    /// cannot be told apart.
    fn parse_group(&mut self) -> Result<(), String> {
        self.at += 1;
        if self.peek() == Some(b'?') {
            let next = self.bytes.get(self.at + 1).copied();
            match next {
                Some(b':') => self.at += 2,
                Some(b'=') | Some(b'!') => {
                    return Err(
                        "lookahead needs the VM to match a subpattern without consuming, which \
                         this one cannot do"
                            .to_string(),
                    )
                }
                Some(b'<') => {
                    return Err(
                        "a named group or lookbehind is not implemented by this engine"
                            .to_string(),
                    )
                }
                Some(b'P') => return Err("a named group is not implemented by this engine".to_string()),
                Some(b'>') => {
                    return Err("an atomic group is not implemented by this engine".to_string())
                }
                Some(b'#') => return Err("an inline comment group is not implemented".to_string()),
                _ => {
                    return Err(
                        "an inline flag or conditional group is not implemented by this engine"
                            .to_string(),
                    )
                }
            }
        }
        self.parse_alternation()?;
        if self.peek() != Some(b')') {
            return Err("an unclosed `(` in the pattern".to_string());
        }
        self.at += 1;
        Ok(())
    }

    /// `[` ... `]`, with ranges, negation and the shorthand escapes.
    fn parse_class(&mut self) -> Result<(), String> {
        self.at += 1;
        let negated = self.peek() == Some(b'^');
        if negated {
            self.at += 1;
        }
        let mut set = [0u8; 32];
        let mut first = true;
        loop {
            let Some(byte) = self.peek() else {
                return Err("an unclosed `[` in the pattern".to_string());
            };
            if byte == b']' && !first {
                self.at += 1;
                break;
            }
            first = false;
            // A `]` immediately after `[` or `[^` is a literal, which is php's rule too.
            let low = if byte == b'\\' {
                match self.class_escape()? {
                    ClassAtom::Byte(value) => value,
                    ClassAtom::Set(shorthand) => {
                        for index in 0..32 {
                            set[index] |= shorthand[index];
                        }
                        continue;
                    }
                }
            } else {
                self.at += 1;
                byte
            };
            // a range, unless the `-` is the last byte before the close
            if self.peek() == Some(b'-') && self.bytes.get(self.at + 1).copied() != Some(b']') {
                self.at += 1;
                let Some(next) = self.peek() else {
                    return Err("an unclosed `[` in the pattern".to_string());
                };
                let high = if next == b'\\' {
                    match self.class_escape()? {
                        ClassAtom::Byte(value) => value,
                        ClassAtom::Set(_) => {
                            return Err(
                                "a shorthand class cannot be the end of a range".to_string()
                            )
                        }
                    }
                } else {
                    self.at += 1;
                    next
                };
                if high < low {
                    return Err(format!(
                        "the range {:?}-{:?} runs backwards, which php rejects too",
                        low as char, high as char
                    ));
                }
                for value in low..=high {
                    add(&mut set, value, self.flags.case_insensitive);
                }
            } else {
                add(&mut set, low, self.flags.case_insensitive);
            }
        }
        if negated {
            for byte in &mut set {
                *byte = !*byte;
            }
        }
        self.push_class(set);
        Ok(())
    }

    /// An escape outside a class.
    fn parse_escape(&mut self) -> Result<(), String> {
        self.at += 1;
        let Some(byte) = self.peek() else {
            return Err("the pattern ends in a bare `\\`".to_string());
        };
        self.at += 1;
        match byte {
            b'd' | b'D' | b'w' | b'W' | b's' | b'S' => {
                let set = shorthand(byte);
                self.push_class(set);
                Ok(())
            }
            b'b' => {
                self.insts.push(Inst::WordBoundary { negated: false });
                Ok(())
            }
            b'B' => {
                self.insts.push(Inst::WordBoundary { negated: true });
                Ok(())
            }
            b'A' => {
                self.insts.push(Inst::Bol { multiline: false });
                Ok(())
            }
            b'z' => {
                self.insts.push(Inst::EndOfSubject);
                Ok(())
            }
            b'Z' => Err("`\\Z` allows a trailing newline, which this engine does not model \
                         separately from `$`"
                .to_string()),
            b'0'..=b'9' => Err(format!(
                "`\\{}` is a backreference, which needs the capture stack this VM does not keep",
                byte as char
            )),
            b'p' | b'P' | b'X' => Err(
                "a unicode property needs UTF-8 mode, and this engine walks bytes".to_string(),
            ),
            b'G' | b'K' | b'R' | b'Q' | b'E' | b'C' => Err(format!(
                "`\\{}` is a PCRE extension this engine does not implement",
                byte as char
            )),
            other => {
                let value = literal_escape(other);
                self.push_literal(value);
                Ok(())
            }
        }
    }

    /// An escape inside a class: either one byte or a whole shorthand set.
    fn class_escape(&mut self) -> Result<ClassAtom, String> {
        self.at += 1;
        let Some(byte) = self.peek() else {
            return Err("the class ends in a bare `\\`".to_string());
        };
        self.at += 1;
        match byte {
            b'd' | b'D' | b'w' | b'W' | b's' | b'S' => Ok(ClassAtom::Set(shorthand(byte))),
            b'p' | b'P' => Err(
                "a unicode property needs UTF-8 mode, and this engine walks bytes".to_string(),
            ),
            other => Ok(ClassAtom::Byte(literal_escape(other))),
        }
    }

    /// Emits one literal byte, as a folded comparison when `i` is in force.
    fn push_literal(&mut self, byte: u8) {
        if self.flags.case_insensitive && byte.is_ascii_alphabetic() {
            self.insts.push(Inst::CharFolded(byte.to_ascii_lowercase()));
        } else {
            self.insts.push(Inst::Char(byte));
        }
    }

    /// Emits a class, reusing an identical bitmap rather than laying it out twice.
    fn push_class(&mut self, set: [u8; 32]) {
        let index = match self.classes.iter().position(|existing| *existing == set) {
            Some(index) => index,
            None => {
                self.classes.push(set);
                self.classes.len() - 1
            }
        };
        self.insts.push(Inst::Class(index as u32));
    }

    /// The byte at the cursor without consuming it, or `None` at the end of the pattern.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
}

/// What a `\x` inside a class stands for.
enum ClassAtom {
    Byte(u8),
    Set([u8; 32]),
}

/// The diagnostic for a pattern whose expansion of `len` instructions exceeds `MAX_PROGRAM`.
fn bound_error(len: usize) -> String {
    format!("the pattern expands to {len} instructions, past the {MAX_PROGRAM} this target runs")
}

/// Whether `body` has a path from its first instruction to its end that consumes no input.
///
/// The body is a slice of the program that was compiled in place at `origin`, so its addresses
/// are absolute and have to be rebased to index it. Everything that consumes a byte is a dead
/// end for this walk; everything else — splits, jumps, anchors — is an edge.
///
/// Reaching one past the last instruction means the body can match the empty string, which is
/// exactly the condition under which a loop around it needs a progress check.
fn is_nullable(body: &[Inst], origin: usize) -> bool {
    let end = body.len();
    let mut seen = vec![false; end + 1];
    let mut queue = vec![0usize];
    while let Some(at) = queue.pop() {
        if at == end {
            return true;
        }
        if at > end || seen[at] {
            continue;
        }
        seen[at] = true;
        let local = |address: u32| (address as usize).saturating_sub(origin);
        match body[at] {
            Inst::Char(_) | Inst::CharFolded(_) | Inst::Any { .. } | Inst::Class(_) => {}
            Inst::Split(first, second) => {
                queue.push(local(first));
                queue.push(local(second));
            }
            Inst::Jump(target) => queue.push(local(target)),
            _ => queue.push(at + 1),
        }
    }
    false
}

/// Adds a byte to a set, and its case partner when `i` is in force.
fn add(set: &mut [u8; 32], byte: u8, fold: bool) {
    set[(byte >> 3) as usize] |= 1 << (byte & 7);
    if fold && byte.is_ascii_alphabetic() {
        let other = byte ^ 0x20;
        set[(other >> 3) as usize] |= 1 << (other & 7);
    }
}

/// The bitmap for one of the six shorthand classes.
fn shorthand(byte: u8) -> [u8; 32] {
    let mut set = [0u8; 32];
    let negated = byte.is_ascii_uppercase();
    let base = byte.to_ascii_lowercase();
    for value in 0u8..=255 {
        let member = match base {
            b'd' => value.is_ascii_digit(),
            b'w' => value.is_ascii_alphanumeric() || value == b'_',
            _ => matches!(value, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c),
        };
        if member != negated {
            set[(value >> 3) as usize] |= 1 << (value & 7);
        }
    }
    set
}

/// The byte a non-shorthand escape stands for.
fn literal_escape(byte: u8) -> u8 {
    match byte {
        b'n' => b'\n',
        b't' => b'\t',
        b'r' => b'\r',
        b'f' => 0x0c,
        b'v' => 0x0b,
        b'e' => 0x1b,
        b'a' => 0x07,
        other => other,
    }
}

/// Moves an instruction's absolute addresses by `by`, for addresses at or after `from`.
fn shift(inst: &mut Inst, from: usize, by: u32) {
    let bump = |address: &mut u32| {
        if *address >= from as u32 {
            *address += by;
        }
    };
    match inst {
        Inst::Split(first, second) => {
            bump(first);
            bump(second);
        }
        Inst::Jump(target) => bump(target),
        _ => {}
    }
}

/// Moves a copied body's absolute addresses by `delta`.
fn rebase(inst: &mut Inst, delta: i64) {
    let moved = |address: &mut u32| *address = (i64::from(*address) + delta) as u32;
    match inst {
        Inst::Split(first, second) => {
            moved(first);
            moved(second);
        }
        Inst::Jump(target) => moved(target),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference VM, used only by the tests below.
    ///
    /// The WAT interpreter executes the same bytecode; running it here first is what lets the
    /// SEMANTICS be checked against php-src in bulk without building a module per pattern.
    pub(super) fn matches(program: &Program, subject: &[u8]) -> bool {
        for start in 0..=subject.len() {
            if run(program, subject, start) {
                return true;
            }
        }
        false
    }

    /// Runs `program` against `subject` from `start` with a backtracking stack, answering whether it
    /// matches. The step count is capped so a pathological pattern fails rather than hangs.
    fn run(program: &Program, subject: &[u8], start: usize) -> bool {
        let mut marks = vec![usize::MAX; program.marks as usize];
        let mut stack = vec![(0usize, start, marks.clone())];
        let mut steps = 0u32;
        while let Some((mut pc, mut at, saved)) = stack.pop() {
            marks = saved;
            loop {
                steps += 1;
                if steps > 2_000_000 {
                    return false;
                }
                match program.insts[pc] {
                    Inst::Match => return true,
                    Inst::Char(byte) => {
                        if subject.get(at) != Some(&byte) {
                            break;
                        }
                        at += 1;
                        pc += 1;
                    }
                    Inst::CharFolded(byte) => {
                        match subject.get(at) {
                            Some(found) if found.to_ascii_lowercase() == byte => {}
                            _ => break,
                        }
                        at += 1;
                        pc += 1;
                    }
                    Inst::Any { dot_all } => {
                        match subject.get(at) {
                            Some(b'\n') if !dot_all => break,
                            Some(_) => {}
                            None => break,
                        }
                        at += 1;
                        pc += 1;
                    }
                    Inst::Class(index) => {
                        let Some(&byte) = subject.get(at) else { break };
                        let set = &program.classes[index as usize];
                        if set[(byte >> 3) as usize] & (1 << (byte & 7)) == 0 {
                            break;
                        }
                        at += 1;
                        pc += 1;
                    }
                    Inst::Split(first, second) => {
                        stack.push((second as usize, at, marks.clone()));
                        pc = first as usize;
                    }
                    Inst::Mark(slot) => {
                        marks[slot as usize] = at;
                        pc += 1;
                    }
                    Inst::Advanced(slot) => {
                        if at == marks[slot as usize] {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::Jump(target) => pc = target as usize,
                    Inst::Bol { multiline } => {
                        let ok = at == 0 || (multiline && subject[at - 1] == b'\n');
                        if !ok {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::Eol { multiline } => {
                        let ok = at == subject.len()
                            || (at + 1 == subject.len() && subject[at] == b'\n')
                            || (multiline && subject[at] == b'\n');
                        if !ok {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::EndOfSubject => {
                        if at != subject.len() {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::WordBoundary { negated } => {
                        let word = |index: usize| {
                            subject
                                .get(index)
                                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                        };
                        let before = at > 0 && word(at - 1);
                        let after = word(at);
                        if (before != after) == negated {
                            break;
                        }
                        pc += 1;
                    }
                }
            }
        }
        false
    }

    /// The delimiter and flag rules are php's, and an unhonourable flag is refused by name.
    #[test]
    fn delimiters_and_flags_follow_php() {
        assert!(compile("/abc/").is_ok());
        assert!(compile("#abc#").is_ok(), "any punctuation delimits");
        assert!(compile("(abc)").is_ok(), "bracket styles pair rather than repeat");
        assert!(compile("{abc}").is_ok());
        assert!(compile("/abc/ims").is_ok());
        assert!(compile("aabca").is_err(), "an alphanumeric cannot delimit");
        assert!(compile("").is_err());
        assert!(compile("/abc").is_err(), "no closing delimiter");

        // A flag that changes what the pattern MEANS is refused rather than ignored.
        assert!(compile("/abc/u").is_err_and(|why| why.contains("UTF-8")));
        assert!(compile("/abc/x").is_err_and(|why| why.contains("whitespace")));
        assert!(compile("/abc/D").is_err_and(|why| why.contains("not one this target honours")));
    }

    /// Every construct outside the subset is refused by NAME, not by a generic failure.
    ///
    /// This is the property that makes a partial engine honest: the pattern is compiled when
    /// the module is built, so an unimplemented construct is a compile error a developer can
    /// act on, never a wrong answer at runtime.
    #[test]
    fn unimplemented_constructs_are_refused_by_name() {
        for (pattern, expected) in [
            (r"/(a)\1/", "backreference"),
            (r"/(?=a)/", "lookahead"),
            (r"/(?<=a)/", "named group or lookbehind"),
            (r"/(?P<x>a)/", "named group"),
            (r"/(?>a)/", "atomic group"),
            (r"/(?i)a/", "inline flag or conditional"),
            (r"/a*+/", "possessive"),
            (r"/\p{L}/", "unicode property"),
            (r"/\K/", "PCRE extension"),
            (r"/\Z/", "trailing newline"),
            (r"/(a/", "unclosed `(`"),
            (r"/a)/", "unbalanced `)`"),
            (r"/[a/", "unclosed `[`"),
            (r"/*a/", "nothing precedes"),
            (r"/a{3,1}/", "counts down"),
            (r"/[z-a]/", "runs backwards"),
            (r"/a\/", "bare `\\`"),
        ] {
            let why = compile(pattern).expect_err(&format!("{pattern} must be refused"));
            assert!(
                why.contains(expected),
                "{pattern} was refused as {why:?}, which does not mention {expected:?}"
            );
        }
    }

    /// A bounded quantifier is expanded, so the program size is bounded too.
    #[test]
    fn a_bounded_quantifier_cannot_expand_without_limit() {
        assert!(compile("/a{10}/").is_ok());
        // Four instructions per copy, so a thousand copies still fit and eleven hundred do not.
        assert!(compile("/(?:abcd){1000}/").is_ok());
        assert!(compile("/(?:abcd){1100}/").is_err_and(|why| why.contains("past the")));
        // Nesting multiplies, which is the case a per-quantifier bound would have missed:
        // each factor is modest and the product is not.
        assert!(compile("/(?:a{70}){70}/").is_err_and(|why| why.contains("past the")));
        assert!(compile("/(?:a{10}){10}/").is_ok(), "a hundred copies is still reasonable");
    }

    /// The compiled bytecode matches what php matches, over a table of measured cases.
    ///
    /// The expectations are php-src 8.5.10's, taken by running each pair through
    /// `preg_match`. They are the semantics, not this engine's opinion of them: `$` before a
    /// trailing newline, `.` stopping at one, `\b` at both ends of the subject, and a lazy
    /// quantifier preferring the shortest match all behave the way php does because they were
    /// checked against it.
    #[test]
    fn the_compiled_program_matches_what_php_matches() {
        let cases: &[(&str, &str, bool)] = &[
            ("/^hello preg$/", "hello preg", true),
            ("/^hello preg$/", "hello pregx", false),
            ("/^hello preg$/", "say hello preg", false),
            ("/hello/", "say hello there", true),
            ("/^$/", "", true),
            ("/a/", "", false),
            // `.` and the s flag
            ("/a.c/", "abc", true),
            ("/a.c/", "a\nc", false),
            ("/a.c/s", "a\nc", true),
            // anchors, including php's trailing-newline rule
            ("/^b/", "a\nb", false),
            ("/^b/m", "a\nb", true),
            ("/c$/", "c\n", true),
            ("/c$/", "c\nd", false),
            ("/c$/m", "c\nd", true),
            ("/\\Ac/", "c", true),
            ("/c\\z/", "c\n", false),
            // classes
            ("/[abc]+/", "xxbxx", true),
            ("/[^abc]/", "abc", false),
            ("/[a-c]/", "b", true),
            ("/[a-c]/", "d", false),
            ("/^[]]$/", "]", true),
            ("/^[a-]$/", "-", true),
            ("/\\d+/", "abc123", true),
            ("/^\\d+$/", "abc123", false),
            ("/^\\w+$/", "a_1", true),
            ("/^\\s$/", "\t", true),
            ("/^\\D$/", "5", false),
            ("/^[\\d.]+$/", "1.5", true),
            // alternation and groups
            ("/^(?:cat|dog)$/", "dog", true),
            ("/^(?:cat|dog)$/", "cow", false),
            ("/^(cat|dog)s$/", "cats", true),
            ("/^a|b$/", "a", true),
            ("/^(?:a|b|c)+$/", "abcba", true),
            // quantifiers
            ("/^a*$/", "", true),
            ("/^a*$/", "aaa", true),
            ("/^a+$/", "", false),
            ("/^ab?c$/", "ac", true),
            ("/^ab?c$/", "abc", true),
            ("/^a{3}$/", "aaa", true),
            ("/^a{3}$/", "aa", false),
            ("/^a{2,}$/", "aaaa", true),
            ("/^a{2,3}$/", "a", false),
            ("/^a{2,3}$/", "aaa", true),
            ("/^a{2,3}$/", "aaaa", false),
            ("/^(?:ab)+$/", "ababab", true),
            ("/^.*b/", "aabab", true),
            ("/^.*?b/", "aabab", true),
            ("/^a??a$/", "a", true),
            // case folding
            ("/^HeLLo$/i", "hello", true),
            ("/^HeLLo$/i", "hellO", true),
            ("/^[a-z]+$/i", "ABC", true),
            ("/^abc$/", "ABC", false),
            // word boundaries
            ("/\\bcat\\b/", "the cat sat", true),
            ("/\\bcat\\b/", "concatenate", false),
            ("/\\Bcat\\B/", "concatenate", true),
            ("/^\\ba/", "a", true),
            // escapes
            ("/^a\\.c$/", "a.c", true),
            ("/^a\\.c$/", "abc", false),
            ("/^a\\tb$/", "a\tb", true),
            ("/^\\$5$/", "$5", true),
            // A loop whose body can match NOTHING. Without a progress check the VM returns to
            // the loop head at the same offset forever, burns its step budget and answers 0 —
            // taking the alternatives that would have matched down with it. A differential
            // fuzz against php-src found fifteen of these in four hundred random patterns.
            ("/^(?:a?)*$/", "aaa", true),
            ("/^(?:a?)*$/", "", true),
            ("/(?:x?)*b/", "b", true),
            ("/^(?:a?b?)*$/", "ba", true),
            ("/^(?:a?)*x|y$/", "y", true),
            ("/^(?:(?:a?)*)*$/", "aa", true),
            ("/^a(?:b?)*a$/", "aa", true),
            ("/^(?:a(?:b?)*)*$/", "aa", true),
            ("/^(?:a*)+$/", "aaa", true),
            ("/^(?:a*)+$/", "b", false),
            ("/^\\\\$/", "\\", true),
        ];
        for (pattern, subject, expected) in cases {
            let program = compile(pattern)
                .unwrap_or_else(|why| panic!("{pattern} did not compile: {why}"));
            assert_eq!(
                matches(&program, subject.as_bytes()),
                *expected,
                "{pattern} against {subject:?}"
            );
        }
    }

    /// The emitted bytes are three little-endian words per instruction, and classes are
    /// laid out once each however often they are used.
    #[test]
    fn the_emitted_layout_is_what_the_vm_reads() {
        assert!(
            crate::codegen_wasm::capability::runtime_function_is_supported(
                crate::ir::RuntimeFnId::PregMatch
            ),
            "preg_match must be supported on this target"
        );

        let program = compile(r"/^\d\d$/").expect("compiles");
        assert_eq!(program.inst_bytes().len(), program.insts.len() * 12);
        assert_eq!(
            program.classes.len(),
            1,
            "the same shorthand twice is one bitmap"
        );
        assert_eq!(program.class_bytes().len(), 32);

        // The first word of each triple is the opcode, in the order `Inst::words` assigns.
        let bytes = program.inst_bytes();
        let opcode = |index: usize| i32::from_le_bytes(bytes[index * 12..index * 12 + 4].try_into().unwrap());
        assert_eq!(opcode(0), 6, "^ is Bol");
        assert_eq!(opcode(1), 3, "\\d is a Class");
        assert_eq!(opcode(3), 7, "$ is Eol");
        assert_eq!(opcode(4), 12, "the program ends in Match");
    }
}
