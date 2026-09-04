//! Purpose:
//! A structural gate over the whole emitted runtime: every path through a helper must leave the
//! stack pointer exactly where it found it by the time it returns.
//!
//! Called from:
//! - `cargo test -p elephc --lib` through `emitters::frame_balance_tests`.
//!
//! Key details:
//! - `stream_filter_register` released its 48-byte frame on two of its three exits and then
//!   branched to a shared epilogue that released it AGAIN, so `ret` jumped to whatever the
//!   caller's stack happened to hold. Every call to it segfaulted, and 28 filter tests failed for
//!   a reason none of them was about. It was the SECOND defect of this shape here, which is why
//!   it is worth a gate rather than a fix.
//! - The property needs no understanding of what a helper computes: walk its control-flow graph
//!   carrying one number — the signed distance sp has moved since entry — and check every `ret`
//!   sees zero. Run against the pre-fix emitter this reports exactly
//!   "`__rt_stream_filter_register`: ret with sp+48" on AArch64 and "sp+56" on x86_64.
//! - Anything the walk cannot read makes a helper UNANALYSABLE rather than passing, and the
//!   unanalysable set is pinned BY NAME. A gate that silently skips what it cannot model is how
//!   the first instance survived; a new name appearing there is a helper nothing is checking.

use super::*;
use crate::codegen_support::platform::{Arch, Platform, Target};
use std::collections::{HashMap, HashSet};

/// Helpers whose stack pointer moves by a run-time amount, by design rather than by accident.
///
/// Each one is here because its stack depth is not a static property, so "sp is where it started"
/// is not the contract it keeps. They are named individually so that a helper that becomes
/// dynamically framed by MISTAKE has to be added here deliberately, with a reason.
const DYNAMIC_STACK_HELPERS: &[(&str, &str)] = &[
    (
        "__rt_sprintf",
        "pops the caller-owned argument records, so it returns on a stack its CALLER pushed",
    ),
    (
        "__rt_vsprintf",
        "pushes one 16-byte record per argument, then tail-calls __rt_sprintf to pop them",
    ),
    (
        "__rt_fiber_switch",
        "switches to another fiber's stack; leaving sp where it found it is the one thing it must not do",
    ),
    (
        "__rt_bcmath_call_free",
        "an indirect trampoline: the branch target is a register",
    ),
    (
        "__rt_bcmath_call_last_error",
        "an indirect trampoline: the branch target is a register",
    ),
    (
        "__rt_mixed_count",
        "an indirect dispatch: the branch target is a register",
    ),
    (
        "__rt_closure_bind",
        "realigns the stack with `and rsp, -16` before a C call",
    ),
    (
        "__rt_curl_version",
        "a real variable-length alloca: it reserves `curl_version_info`'s blob by the byte \
         count the bridge reports (`sub rsp, r10` after `shl r10, 4`, `sub sp, sp, x9`), so \
         the movement CANNOT be read from the text. VERIFIED balanced by hand on both \
         arches, through the frame pointer rather than by matching the subtraction: x86 \
         unwinds with `mov rsp, rbp` + `pop rbp`, aarch64 with `sub sp, x29, #16` + \
         `add sp, sp, #32`",
    ),
    (
        "__rt_exception_matches",
        "realigns the stack with `and rsp, -16` before a C call",
    ),
];

/// One instruction's effect on the stack pointer.
enum SpEffect {
    /// Moves it by a known number of bytes.
    Delta(i64),
    /// `leave`, or `mov rsp, rbp`: back to where the frame pointer was pinned.
    ToFramePointer,
    /// The walk cannot read it.
    Opaque,
}

/// Reads one AArch64 instruction's effect on sp.
fn sp_effect_aarch64(inst: &str) -> SpEffect {
    if let Some(rest) = inst.strip_prefix("sub sp, sp, #") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            return SpEffect::Delta(-n);
        }
    }
    if let Some(rest) = inst.strip_prefix("add sp, sp, #") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            return SpEffect::Delta(n);
        }
    }
    // `stp x0, x1, [sp, #-16]!` — a pre-indexed push.
    if let Some(idx) = inst.find("[sp, #-") {
        if inst.ends_with("]!") {
            let tail = &inst[idx + "[sp, #-".len()..inst.len() - 2];
            if let Ok(n) = tail.parse::<i64>() {
                return SpEffect::Delta(-n);
            }
        }
    }
    // `ldp x0, x1, [sp], #16` — a post-indexed pop.
    if let Some(idx) = inst.find("[sp], #") {
        let tail = &inst[idx + "[sp], #".len()..];
        if let Ok(n) = tail.trim().parse::<i64>() {
            return SpEffect::Delta(n);
        }
    }
    if inst.starts_with("mov sp,") || inst.starts_with("add sp,") || inst.starts_with("sub sp,") {
        return SpEffect::Opaque;
    }
    SpEffect::Delta(0)
}

/// Reads one x86_64 instruction's effect on rsp.
fn sp_effect_x86_64(inst: &str) -> SpEffect {
    if let Some(rest) = inst.strip_prefix("sub rsp, ") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            return SpEffect::Delta(-n);
        }
    }
    if let Some(rest) = inst.strip_prefix("add rsp, ") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            return SpEffect::Delta(n);
        }
    }
    if inst.starts_with("push ") {
        return SpEffect::Delta(-8);
    }
    if inst.starts_with("pop ") {
        return SpEffect::Delta(8);
    }
    if inst == "leave" || inst == "mov rsp, rbp" {
        return SpEffect::ToFramePointer;
    }
    // Any other write to rsp — `and rsp, -16`, `lea rsp, [rsp + rcx + 16]`, a load from memory.
    let writes_rsp = inst
        .split_once(' ')
        .map(|(_, operands)| operands.trim_start().starts_with("rsp"))
        .unwrap_or(false);
    if writes_rsp {
        return SpEffect::Opaque;
    }
    SpEffect::Delta(0)
}

/// Where control can go from one instruction: whether it falls through, and its branch target.
///
/// A call is NOT a transfer of control — it comes back. An indirect branch is, and cannot be
/// followed, so it is reported as `None` and makes the helper unanalysable.
fn successors(arch: Arch, inst: &str) -> (bool, Option<Option<String>>) {
    let (op, rest) = match inst.split_once(char::is_whitespace) {
        Some((op, rest)) => (op, rest.trim()),
        None => return (true, None),
    };
    let target = |rest: &str| rest.rsplit(',').next().unwrap_or(rest).trim().to_string();
    // `1f` / `1b` are GAS local labels, not indirect targets.
    let is_label = |s: &str| {
        let named = s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$')
            && s.starts_with(|c: char| c.is_alphabetic() || c == '_');
        let numeric = s.len() >= 2
            && s[..s.len() - 1].chars().all(|c| c.is_ascii_digit())
            && (s.ends_with('f') || s.ends_with('b'));
        named || numeric
    };
    match arch {
        Arch::AArch64 => match op {
            "bl" | "blr" => (true, None),
            "br" => (false, Some(None)),
            "b" => (false, Some(Some(target(rest)))),
            _ if op.starts_with("b.") || op.starts_with("cb") || op.starts_with("tb") => {
                (true, Some(Some(target(rest))))
            }
            _ => (true, None),
        },
        Arch::X86_64 => match op {
            "call" => (true, None),
            "jmp" if is_label(rest) => (false, Some(Some(rest.to_string()))),
            "jmp" => (false, Some(None)),
            _ if op.starts_with('j') && is_label(rest) => (true, Some(Some(rest.to_string()))),
            _ if op.starts_with('j') => (true, Some(None)),
            _ => (true, None),
        },
    }
}

/// One helper: its instruction stream and the offsets its local labels name.
struct Helper {
    instructions: Vec<String>,
    label_at: HashMap<String, usize>,
    /// GAS numeric local labels — `1:` reached as `1f` (the next one forward) or `1b` (the last
    /// one back). They repeat within a helper, so they are a LIST rather than a map.
    numeric_labels: Vec<(u32, usize)>,
}

impl Helper {
    /// Resolves a branch target to an instruction offset, if it names one inside this helper.
    fn resolve(&self, target: &str, from: usize) -> Option<usize> {
        if let Some(&at) = self.label_at.get(target) {
            return Some(at);
        }
        let (digits, dir) = target.split_at(target.len().saturating_sub(1));
        let number: u32 = digits.parse().ok()?;
        match dir {
            "f" => self
                .numeric_labels
                .iter()
                .filter(|(n, at)| *n == number && *at > from)
                .map(|(_, at)| *at)
                .min(),
            "b" => self
                .numeric_labels
                .iter()
                .filter(|(n, at)| *n == number && *at <= from)
                .map(|(_, at)| *at)
                .max(),
            _ => None,
        }
    }
}

/// Splits a runtime dump into helpers keyed by their real ENTRY points.
///
/// A `.globl` label is not automatically one. `__rt_csv_parse_buffer` carries `.globl` and is
/// reached only by `b` from two helpers that have already established a 112-byte frame — it is a
/// shared TAIL, and analysing it as an entry reports a `ret` that is perfectly correct. An entry
/// is a label something CALLS, or a global nothing branches to, which is how a compiled program
/// enters the runtime.
fn split_into_helpers(asm: &str) -> HashMap<String, Helper> {
    let mut globals = HashSet::new();
    let mut called = HashSet::new();
    let mut branched = HashSet::new();
    for line in asm.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix(".globl ") {
            globals.insert(name.trim().to_string());
        } else if let Some(name) = line.strip_prefix("bl ").or_else(|| line.strip_prefix("call ")) {
            called.insert(name.trim().to_string());
        } else if let Some(name) = line.strip_prefix("b ").or_else(|| line.strip_prefix("jmp ")) {
            branched.insert(name.trim().to_string());
        }
    }
    let entries: HashSet<&String> = globals
        .iter()
        .filter(|g| called.contains(*g) || !branched.contains(*g))
        .collect();

    let mut helpers: HashMap<String, Helper> = HashMap::new();
    let mut current: Option<String> = None;
    for line in asm.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('.') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_suffix(':') {
            let name = name.to_string();
            if entries.contains(&name) {
                current = Some(name.clone());
                helpers.insert(
                    name,
                    Helper {
                        instructions: Vec::new(),
                        label_at: HashMap::new(),
                        numeric_labels: Vec::new(),
                    },
                );
                continue;
            }
            if let Some(helper) = current.as_ref().and_then(|c| helpers.get_mut(c)) {
                let at = helper.instructions.len();
                match name.parse::<u32>() {
                    Ok(number) => helper.numeric_labels.push((number, at)),
                    Err(_) => {
                        helper.label_at.insert(name, at);
                    }
                }
            }
            continue;
        }
        if let Some(helper) = current.as_ref().and_then(|c| helpers.get_mut(c)) {
            helper.instructions.push(line.to_string());
        }
    }
    helpers
}

/// What the walk concluded about one helper.
enum Verdict {
    Balanced,
    Unbalanced(String),
    Unanalysable(String),
}

/// Walks every path from the helper's entry, carrying the distance sp has moved.
fn analyse(helper: &Helper, arch: Arch) -> Verdict {
    if helper.instructions.is_empty() {
        return Verdict::Balanced;
    }
    let mut seen: HashMap<usize, i64> = HashMap::new();
    let mut work = vec![(0usize, 0i64)];
    let mut unanalysable: Option<String> = None;

    while let Some((mut pc, mut delta)) = work.pop() {
        loop {
            // Running off the end is either a noreturn call or a physical fallthrough into the
            // next helper. Neither is this gate's question, which is about RETURNS.
            if pc >= helper.instructions.len() {
                break;
            }
            match seen.get(&pc) {
                Some(&previous) => {
                    // Two depths at one instruction means the frame size is not static. That is a
                    // property of the helper, not a defect on its own — the argument-spill loops
                    // do it deliberately — so it makes the helper unanalysable.
                    if previous != delta {
                        unanalysable = Some(format!("two depths at `{}`", helper.instructions[pc]));
                    }
                    break;
                }
                None => {
                    seen.insert(pc, delta);
                }
            }
            let inst = &helper.instructions[pc];

            match if arch == Arch::AArch64 {
                sp_effect_aarch64(inst)
            } else {
                sp_effect_x86_64(inst)
            } {
                SpEffect::Opaque => {
                    unanalysable = Some(format!("opaque `{inst}`"));
                    break;
                }
                SpEffect::ToFramePointer => {
                    // The canonical `push rbp; mov rbp, rsp` prologue pins rbp at entry-8.
                    delta = -8;
                    if inst == "leave" {
                        delta = 0; // `leave` also pops rbp
                    }
                    pc += 1;
                    continue;
                }
                SpEffect::Delta(d) => delta += d,
            }

            if inst == "ret" {
                if delta != 0 {
                    return Verdict::Unbalanced(format!(
                        "`ret` with sp{delta:+} — the frame is released a different number of \
                         times on different paths"
                    ));
                }
                break;
            }

            let (falls_through, branch) = successors(arch, inst);
            match branch {
                Some(None) => unanalysable = Some(format!("indirect `{inst}`")),
                Some(Some(target)) => {
                    // A branch OUT of the helper is another helper's problem.
                    if let Some(at) = helper.resolve(&target, pc) {
                        work.push((at, delta));
                    }
                }
                None => {}
            }
            if !falls_through {
                break;
            }
            pc += 1;
        }
    }

    match unanalysable {
        Some(why) => Verdict::Unanalysable(why),
        None => Verdict::Balanced,
    }
}

/// Verifies every runtime helper returns on the stack it was entered with, on both arches.
///
/// The pre-fix `stream_filter_register` reports here as "`ret` with sp+48" (AArch64) and "sp+56"
/// (x86_64) — the defect that made every `stream_filter_register()` call segfault, found without
/// running anything.
#[test]
fn every_runtime_helper_returns_on_a_balanced_stack() {
    let exempt: HashMap<&str, &str> = DYNAMIC_STACK_HELPERS.iter().copied().collect();
    let mut complaints = Vec::new();
    let mut unanalysable_found: HashSet<String> = HashSet::new();

    for (platform, arch) in [
        (Platform::MacOS, Arch::AArch64),
        (Platform::Linux, Arch::X86_64),
    ] {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_runtime(&mut emitter, RuntimeFeatures::all());
        let asm = emitter.output();
        let helpers = split_into_helpers(&asm);
        assert!(
            helpers.len() > 500,
            "{arch:?}: only {} helpers were found — the split stopped reading the runtime",
            helpers.len()
        );

        for (name, helper) in &helpers {
            match analyse(helper, arch) {
                Verdict::Balanced => {}
                Verdict::Unanalysable(why) => {
                    unanalysable_found.insert(format!("{name} ({why})"));
                }
                Verdict::Unbalanced(why) => {
                    if !exempt.contains_key(name.as_str()) {
                        complaints.push(format!("{arch:?} {name}: {why}"));
                    }
                }
            }
        }
    }

    assert!(
        complaints.is_empty(),
        "these runtime helpers do not return on the stack they were entered with, so `ret` jumps \
         to whatever the caller's stack happens to hold:\n  {}",
        complaints.join("\n  ")
    );

    // A helper the walk cannot read is a helper nothing is checking, so the set is pinned by name.
    // Every entry is dynamically framed BY DESIGN and says why in `DYNAMIC_STACK_HELPERS`; a new
    // name here has to be added deliberately, after deciding which kind it is.
    let unexpected: Vec<&String> = unanalysable_found
        .iter()
        .filter(|name| {
            let bare = name.split(" (").next().unwrap_or(name);
            !exempt.contains_key(bare)
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "these runtime helpers moved their stack pointer by an amount this gate cannot read, so \
         it is NOT checking them. Either the movement is by design — add it to \
         DYNAMIC_STACK_HELPERS with the reason — or it is the defect this gate exists to \
         find: {unexpected:?}"
    );
}
