//! Purpose:
//! A whole-runtime audit of ONE x86_64 ABI rule: System V requires `rsp % 16 == 0` at every
//! `call`, so the callee is entered with `rsp % 16 == 8`. This module walks every emitted
//! `linux-x86_64` runtime helper, tracks the stack pointer through its control-flow graph,
//! and fails when a `call` sits on the wrong boundary.
//!
//! Called from:
//! - `cargo test --bin elephc` only. The file is `#[cfg(test)]`-gated at its `mod`
//!   declaration in `super`, so it is never compiled into a shipped binary.
//!
//! Key details:
//! - WHY THIS EXISTS. `__rt_array_free_deep` reserved a 24-byte frame after `push rbp` had
//!   already landed rsp on a 16-byte boundary, so every `call` in its body ran 8 bytes off.
//!   That was invisible for as long as the callees were hand-written assembly touching only
//!   integer registers — and then curl arrived. A `CurlHandle` released as an ARRAY ELEMENT
//!   walks `__rt_array_free_deep` -> `__rt_decref_any` -> `__rt_mixed_free_deep` ->
//!   `__rt_curl_easy_free` -> `call r9`, the `elephc_curl` bridge, and libcurl faults on its
//!   first aligned SSE spill onto that stack. It cost a red linux-x86_64 CI shard
//!   (`codegen::curl::multi::multi_get_handles_reports_add_order_and_tracks_removal`,
//!   SIGSEGV after printing every expected line) and a full day to find, because the fault
//!   is libcurl-build-dependent: a distro libcurl tolerates the same misaligned stack.
//! - THE RULE IS ABOUT THE CLASS, NOT THAT ONE HELPER. Any new runtime helper that reserves
//!   an odd multiple of 8 after a single `push` re-arms the same landmine for whatever
//!   bridge is called next. That is what this audit prevents.
//! - THE ALLOWLIST SHRINKS ONLY. Every helper that violates the rule today is named below
//!   WITH the reason it is still there. An allowlisted helper that becomes compliant must be
//!   deleted from the list — [`the_misaligned_call_allowlist_shrinks_only`] fails otherwise,
//!   the same discipline the builtin-parity allowlist uses. The list may never grow without
//!   a deliberate edit that has to justify itself in review.
//! - AARCH64 IS NOT AUDITED HERE and does not need to be: its emitters allocate frames with
//!   `sub sp, sp, #N`, and AArch64 faults on a misaligned SP at the point of use rather than
//!   silently corrupting a callee, so the same class of bug cannot lie dormant there. Both
//!   aarch64 targets passed the curl fixture this audit was written for.

use std::collections::HashMap;

use crate::codegen_support::platform::Target;
use crate::codegen_support::runtime_features::RuntimeFeatures;

/// Helpers whose `call` sites are misaligned TODAY, each with the reason it has not been
/// fixed. Ordered by the shape of the problem, not alphabetically, so a reader can see the
/// two distinct causes.
///
/// NOTHING HERE REACHES THE `elephc_curl` BRIDGE — that was checked, not assumed: none of
/// these helpers calls a decref/release helper, so none of them can reach
/// `__rt_mixed_free_deep`'s resource ladder. The three entries that DO reach code outside
/// the hand-written runtime (`__rt_usort`, `__rt_array_udiff_uintersect`, `__rt_fiber_entry`)
/// are called out individually below and are the ones worth fixing first.
const ALLOWED_MISALIGNED_CALLS: &[(&str, &str)] = &[
    // -- No frame at all: the helper calls without adjusting rsp, so the callee is entered
    //    8 bytes off. Every callee here is hand-written assembly that touches only integer
    //    registers, so nothing observes it. The fix is a `sub rsp, 8` / `add rsp, 8` pair
    //    around each call, which is why these are cheap but not free: several are hot leaves.
    ("__rt_strtolower", "frameless: calls __rt_strcopy, integer-only assembly"),
    ("__rt_hash_key_hash", "frameless: calls __rt_hash_fnv1a, integer-only assembly"),
    ("__rt_hash_key_eq", "frameless: calls __rt_str_eq, integer-only assembly"),
    ("__rt_array_rand", "frameless: calls __rt_random_uniform, integer-only assembly"),
    ("__rt_mixed_is_empty", "frameless: calls __rt_mixed_unbox, integer-only assembly"),
    (
        "__rt_report_uncaught_exception",
        "frameless: calls __rt_itoa on the fatal path, integer-only assembly",
    ),
    (
        "__rt_incref",
        "frameless: calls __rt_heap_debug_check_live, and only in --heap-debug builds. \
         The hottest leaf in the runtime; a two-instruction pad here is measurable",
    ),
    (
        "__rt_spl_dll_top",
        "frameless: calls __rt_incref, integer-only assembly",
    ),
    (
        "__rt_spl_dll_bottom",
        "frameless: calls __rt_incref, integer-only assembly",
    ),
    (
        "__rt_spl_dll_current",
        "frameless: calls __rt_incref, integer-only assembly",
    ),
    (
        "__rt_heap_alloc",
        "calls __rt_heap_debug_validate_free_list (--heap-debug builds only), integer-only",
    ),
    (
        "__rt_heap_free",
        "calls __rt_object_handle_release and __rt_heap_debug_validate_free_list, both \
         integer-only. NOTE this is one frame below __rt_mixed_free_deep, but it is reached \
         AFTER the resource destructor has already run, never before it",
    ),
    // -- PRIVATE SUBROUTINES sharing the exported helper's `rbp` frame: they take no frame of
    //    their own (they read the caller's `[rbp - N]` spills directly), so the `call` between
    //    two of them is always 8 bytes off. Only reachable through their own section's
    //    exported entry, and every callee is integer-only digit/cursor assembly.
    (
        "__rt_date",
        "private frameless subroutines (__rt_date_write_num -> __rt_date_write_2digit and \
         friends) call each other while sharing __rt_date's rbp frame; all integer-only",
    ),
    (
        "__rt_strtotime",
        "private frameless subroutines (-> __rt_strtotime_lc_cursor_linux_x86_64) call each \
         other while sharing __rt_strtotime's rbp frame; all integer-only",
    ),
    // -- A real frame that lands on the wrong parity because the pushes and the `sub` do not
    //    add up to a 16-byte multiple. Mechanically fixable the same way the seven already
    //    corrected helpers were, but each needs its spill-slot offsets re-read first.
    (
        "__rt_wordwrap",
        "multi-push frame off by 8: calls __rt_concat_reserve / __rt_wordwrap_cpy_x86_64, \
         integer-only assembly",
    ),
    (
        "__rt_array_filter",
        "multi-push frame off by 8: calls __rt_heap_alloc / __rt_object_handle_acquire, \
         integer-only assembly",
    ),
    (
        "__rt_array_filter_refcounted",
        "multi-push frame off by 8: calls __rt_heap_alloc / __rt_object_handle_acquire, \
         integer-only assembly",
    ),
    // -- The three that reach code this runtime did not write. FIX THESE FIRST.
    (
        "__rt_usort",
        "REACHES NON-RUNTIME CODE: `call r12` is the user's comparator, i.e. COMPILED PHP. \
         It has survived because codegen spills floats with `movsd`/`movq` (alignment-\
         tolerant) rather than `movaps`, which is luck, not design",
    ),
    (
        "__rt_array_udiff_uintersect",
        "REACHES NON-RUNTIME CODE: same `call r12` user-callback shape as __rt_usort",
    ),
    (
        "__rt_fiber_entry",
        "REACHES LIBC: `call setjmp` on a misaligned stack, plus `call r13` into the fiber's \
         PHP body. glibc's x86_64 setjmp is integer-only so it survives. Deliberately NOT \
         fixed here: this is coroutine entry, the frame is load-bearing for \
         __rt_fiber_switch's stack hand-off, and it needs fiber-focused tests in hand first",
    ),
];

/// Helpers this audit cannot model, each with the construct that defeats it. These are NOT
/// assertions of correctness — they are honest holes, and naming them is what keeps the
/// audit from quietly reporting a clean sweep it never performed.
const NOT_STATICALLY_ANALYZABLE: &[(&str, &str)] = &[
    ("__rt_curl_version", "dynamic frame: `sub rsp, r10`"),
    ("__rt_sprintf", "dynamic frame: `lea rsp, [rsp + rcx + 16]`"),
    (
        "__rt_vsprintf",
        "two paths reach one instruction with different frames (-72 / -88)",
    ),
    ("__rt_closure_bind", "realigns explicitly with `and rsp, -16`"),
    (
        "__rt_gc_mark_reachable",
        "shares a tail between the framed body and a frameless early-out",
    ),
    (
        "__rt_gc_collect_cycles",
        "shares a tail between the framed body and a frameless early-out",
    ),
    (
        "__rt_mb_strlen",
        "shares a tail between the framed body and a frameless early-out",
    ),
];

/// The release chain a curl handle travels into the C bridge. These may NEVER appear in
/// either allowlist above: allowlisting one of them would re-open exactly the bug this
/// module was written for, and would do it silently.
const CURL_RELEASE_CHAIN: &[&str] = &[
    "__rt_array_free_deep",
    "__rt_hash_free_deep",
    "__rt_object_free_deep",
    "__rt_mixed_free_deep",
    "__rt_decref_any",
    "__rt_decref_array",
    "__rt_decref_mixed",
    "__rt_decref_object",
    "__rt_curl_easy_free",
    "__rt_curl_multi_free",
    "__rt_curl_share_free",
];

/// One emitted helper: its name and its (line number, text) instruction stream.
struct Function {
    name: String,
    body: Vec<(usize, String)>,
}

/// What the walk found in one helper.
struct Analysis {
    /// Set when the helper contains a construct this audit cannot model.
    unanalyzable: Option<String>,
    /// `(line, target, rsp offset from entry)` for every `call` on the wrong boundary.
    misaligned: Vec<(usize, String, i64)>,
}

/// Splits the emitted assembly into helpers, one per `.section .text.<name>` block.
///
/// Directives, comments and blank lines are dropped; labels are kept because the walk needs
/// them as branch targets.
fn functions_of(asm: &str) -> Vec<Function> {
    let mut functions: Vec<Function> = Vec::new();
    let mut inside = false;
    for (number, raw) in asm.lines().enumerate() {
        let text = raw.trim();
        if let Some(rest) = text.strip_prefix(".section .text.") {
            let name = rest.split(',').next().unwrap_or_default().to_string();
            functions.push(Function { name, body: Vec::new() });
            inside = true;
            continue;
        }
        if text.starts_with(".section") {
            inside = false;
            continue;
        }
        if !inside || text.is_empty() || text.starts_with('#') || text.starts_with('.') {
            continue;
        }
        if let Some(function) = functions.last_mut() {
            function.body.push((number + 1, text.to_string()));
        }
    }
    functions
}

/// Returns the label a line declares, if it declares one.
fn label_of(text: &str) -> Option<&str> {
    let name = text.strip_suffix(':')?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some(name)
}

/// The conditional and unconditional branch mnemonics the emitters use.
fn is_branch(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "jmp" | "je" | "jne" | "jz" | "jnz" | "jl" | "jle" | "jg" | "jge" | "jb" | "jbe"
            | "ja" | "jae" | "js" | "jns" | "jo" | "jno" | "jc" | "jnc" | "jp" | "jnp"
            | "jpe" | "jpo" | "jrcxz" | "jecxz"
    )
}

/// True when an instruction writes `rsp` in a way the walk models explicitly elsewhere or
/// cannot model at all.
fn writes_rsp(mnemonic: &str, operands: &str) -> bool {
    matches!(
        mnemonic,
        "mov" | "lea" | "and" | "or" | "xor" | "add" | "sub" | "xchg" | "adc" | "sbb"
    ) && operands.starts_with("rsp,")
}

/// Resolves a GAS local branch target (`1f` / `1b`) to an index in `body`.
fn resolve_local(body: &[(usize, String)], from: usize, target: &str) -> Option<usize> {
    let (digits, direction) = target.split_at(target.len() - 1);
    let wanted = format!("{digits}:");
    match direction {
        "f" => (from + 1..body.len()).find(|&i| body[i].1 == wanted),
        "b" => (0..from).rev().find(|&i| body[i].1 == wanted),
        _ => None,
    }
}


/// Walks ONE control-flow path set, starting at `start`, tracking `rsp`'s offset from that
/// entry point. Returns the construct that defeated it, if any.
///
/// At entry `rsp % 16 == 8` (the caller's `call` pushed a return address onto an aligned
/// stack), so a `call` is correct exactly when the offset is `8 mod 16`. A path ends at
/// `ret`, at a branch that leaves the helper (a tail call), or at an unmodellable construct.
/// Reaching one instruction with two different offsets means the walk lost track, and the
/// helper is reported as unanalyzable rather than silently half-checked.
fn walk(
    body: &[(usize, String)],
    labels: &HashMap<&str, usize>,
    start: usize,
    seen: &mut HashMap<usize, i64>,
    misaligned: &mut Vec<(usize, String, i64)>,
) -> Option<String> {
    let mut work = vec![(start, 0i64)];
    while let Some((index, offset)) = work.pop() {
        let Some((line, text)) = body.get(index) else {
            continue;
        };
        match seen.get(&index) {
            Some(&previous) if previous == offset => continue,
            Some(&previous) => {
                return Some(format!(
                    "line {line}: reached with rsp offsets {previous} and {offset}"
                ));
            }
            None => {
                seen.insert(index, offset);
            }
        }

        if label_of(text).is_some() {
            work.push((index + 1, offset));
            continue;
        }

        let (mnemonic, operands) = match text.split_once(' ') {
            Some((head, rest)) => (head, rest.trim()),
            None => (text.as_str(), ""),
        };

        match mnemonic {
            "push" | "pushfq" => work.push((index + 1, offset - 8)),
            "pop" | "popfq" => work.push((index + 1, offset + 8)),
            "ret" | "hlt" | "ud2" | "iret" => {}
            "call" => {
                if offset.rem_euclid(16) != 8 {
                    misaligned.push((*line, operands.to_string(), offset));
                }
                work.push((index + 1, offset));
            }
            _ if writes_rsp(mnemonic, operands) => {
                let rest = operands.trim_start_matches("rsp,").trim();
                let literal = rest.parse::<i64>().ok();
                match (mnemonic, literal) {
                    ("sub", Some(n)) => work.push((index + 1, offset - n)),
                    ("add", Some(n)) => work.push((index + 1, offset + n)),
                    ("mov", _) if rest == "rbp" => work.push((index + 1, -8)),
                    ("lea", _)
                        if rest.starts_with("[rbp - ") && rest.ends_with(']') =>
                    {
                        let inner = rest
                            .trim_start_matches("[rbp - ")
                            .trim_end_matches(']')
                            .trim()
                            .parse::<i64>()
                            .ok();
                        match inner {
                            Some(n) => work.push((index + 1, -8 - n)),
                            None => {
                                return Some(format!(
                                    "line {line}: unmodelled rsp write `{text}`"
                                ))
                            }
                        }
                    }
                    _ => return Some(format!("line {line}: unmodelled rsp write `{text}`")),
                }
            }
            _ if is_branch(mnemonic) => {
                let target = operands;
                let resolved = if let Some(&target_index) = labels.get(target) {
                    Some(target_index)
                } else if target
                    .strip_suffix(['f', 'b'])
                    .is_some_and(|digits| {
                        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                    })
                {
                    match resolve_local(body, index, target) {
                        Some(target_index) => Some(target_index),
                        None => {
                            return Some(format!(
                                "line {line}: unresolved local branch `{text}`"
                            ))
                        }
                    }
                } else if is_register(target) {
                    // A BRIDGE-SLOT TRAMPOLINE is a tail call, not a jump table: the runtime's
                    // only indirect branches are `mov <reg>, QWORD PTR [rip + _..._fn]`
                    // immediately followed by `jmp <reg>` (bcmath's two entry points). The
                    // preceding-load check is what keeps this from silently swallowing a real
                    // computed jump, which would hide every call site behind it.
                    let loads_slot = index
                        .checked_sub(1)
                        .and_then(|previous| body.get(previous))
                        .is_some_and(|(_, previous)| {
                            previous.starts_with(&format!("mov {target}, QWORD PTR [rip + "))
                        });
                    if !loads_slot {
                        return Some(format!("line {line}: indirect branch `{text}`"));
                    }
                    None
                } else {
                    // A branch to a label outside this helper is a TAIL CALL: the callee runs
                    // on this stack with the original return address still at [rsp], so the
                    // path simply ends here.
                    None
                };
                if let Some(target_index) = resolved {
                    work.push((target_index, offset));
                }
                if mnemonic != "jmp" {
                    work.push((index + 1, offset));
                }
            }
            _ => work.push((index + 1, offset)),
        }
    }
    None
}

/// True for a bare register operand (`rcx`, `r10`, `eax`).
fn is_register(operand: &str) -> bool {
    !operand.is_empty()
        && operand.len() <= 4
        && operand.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && operand.starts_with(['r', 'e'])
}

/// Audits every helper emitted in one `.section .text.<name>` block.
///
/// MULTI-ENTRY ON PURPOSE, AND EACH ENTRY WALKS IN ISOLATION. A section is not one function:
/// the emitters lay private subroutines out behind the exported helper — `__rt_date` alone
/// trails nine of them (`__rt_date_write_2digit_linux_x86_64` and friends), each entered by
/// `call` and each sharing the caller's `rbp` frame; `__rt_bcmath_binary` sits behind
/// `__rt_bcmath_call_last_error` the same way. Walking only the section's own name would
/// leave every call site inside those bodies unaudited, which is how they escaped notice
/// until this audit was written.
///
/// The offsets are tracked per walk rather than per section because two entry points into
/// one section legitimately see different frames; sharing one map would report that ordinary
/// arrangement as "the walk lost track". A conflict WITHIN a single walk still means exactly
/// that, and is reported.
fn analyze(function: &Function) -> Analysis {
    let mut labels: HashMap<&str, usize> = HashMap::new();
    for (index, (_, text)) in function.body.iter().enumerate() {
        if let Some(name) = label_of(text) {
            labels.insert(name, index);
        }
    }

    // A helper with no `call` anywhere has nothing for a CALL-SITE audit to check. That is
    // what makes the bcmath bridge trampolines (`mov rcx, [slot]` + `jmp rcx`) a non-event.
    let has_call = function
        .body
        .iter()
        .any(|(_, text)| text == "call" || text.starts_with("call "));
    if !has_call {
        return Analysis { unanalyzable: None, misaligned: Vec::new() };
    }

    let mut visited: HashMap<usize, i64> = HashMap::new();
    let mut misaligned: Vec<(usize, String, i64)> = Vec::new();
    let mut entry = labels.get(function.name.as_str()).copied();

    loop {
        let Some(start) = entry else { break };
        let mut seen: HashMap<usize, i64> = HashMap::new();
        if let Some(reason) = walk(&function.body, &labels, start, &mut seen, &mut misaligned) {
            return Analysis { unanalyzable: Some(reason), misaligned: Vec::new() };
        }
        visited.extend(seen);
        entry = function
            .body
            .iter()
            .enumerate()
            .find(|(index, (_, text))| label_of(text).is_some() && !visited.contains_key(index))
            .map(|(index, _)| index);
    }

    // One call site can be reached by two entry walks; report it once.
    misaligned.sort_by_key(|(line, _, _)| *line);
    misaligned.dedup_by_key(|(line, _, _)| *line);
    Analysis { unanalyzable: None, misaligned }
}

/// Renders the `linux-x86_64` runtime for one feature set.
fn runtime_asm(features: RuntimeFeatures) -> String {
    crate::codegen_support::generate_runtime_with_features(
        8_388_608,
        Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target"),
        features,
    )
}

/// Audits both feature profiles and returns, per helper name, whether it was seen at all,
/// whether it violated, and whether it was unanalyzable.
fn audit() -> (HashMap<String, Vec<(usize, String, i64)>>, HashMap<String, String>, Vec<String>) {
    let mut violations: HashMap<String, Vec<(usize, String, i64)>> = HashMap::new();
    let mut unanalyzable: HashMap<String, String> = HashMap::new();
    let mut seen: Vec<String> = Vec::new();
    for features in [RuntimeFeatures::none(), RuntimeFeatures::all()] {
        let asm = runtime_asm(features);
        for function in functions_of(&asm) {
            let result = analyze(&function);
            seen.push(function.name.clone());
            if let Some(reason) = result.unanalyzable {
                unanalyzable.entry(function.name).or_insert(reason);
            } else if !result.misaligned.is_empty() {
                violations.entry(function.name).or_insert(result.misaligned);
            }
        }
    }
    (violations, unanalyzable, seen)
}

/// EVERY `call` IN THE x86_64 RUNTIME MUST LEAVE THE CALLEE A 16-BYTE-ALIGNED STACK.
///
/// A failure here means a helper reserved a frame that is not a 16-byte multiple (counting
/// its pushes), and every `call` below it hands the callee a stack System V forbids. That is
/// invisible until something the runtime calls is real C — which is what turned
/// `__rt_array_free_deep`'s 24-byte frame into a SIGSEGV inside libcurl.
#[test]
fn every_x86_64_runtime_call_site_is_sysv_aligned() {
    let (violations, _, _) = audit();
    let allowed: Vec<&str> = ALLOWED_MISALIGNED_CALLS.iter().map(|(name, _)| *name).collect();
    let mut unexpected: Vec<String> = violations
        .iter()
        .filter(|(name, _)| !allowed.contains(&name.as_str()))
        .map(|(name, calls)| {
            let (line, target, offset) = &calls[0];
            format!(
                "{name} calls {target} at line {line} with rsp {offset} bytes past its entry \
                 ({} mod 16, needs 8)",
                offset.rem_euclid(16)
            )
        })
        .collect();
    unexpected.sort();
    assert!(
        unexpected.is_empty(),
        "these x86_64 runtime helpers call on a stack System V forbids. Round the frame up \
         to a 16-byte multiple (counting pushes) rather than adding the helper to \
         ALLOWED_MISALIGNED_CALLS — a bridge called from here faults:\n  {}",
        unexpected.join("\n  ")
    );
}

/// THE ALLOWLIST SHRINKS ONLY. A helper that has been fixed must leave the list, or the list
/// stops describing reality and the next reader trusts it anyway.
#[test]
fn the_misaligned_call_allowlist_shrinks_only() {
    let (violations, unanalyzable, seen) = audit();
    let mut stale = Vec::new();
    for (name, reason) in ALLOWED_MISALIGNED_CALLS {
        if !seen.iter().any(|emitted| emitted == name) {
            stale.push(format!("{name} is no longer emitted at all ({reason})"));
        } else if !violations.contains_key(*name) && !unanalyzable.contains_key(*name) {
            stale.push(format!("{name} is aligned now — delete it ({reason})"));
        }
    }
    stale.sort();
    assert!(
        stale.is_empty(),
        "ALLOWED_MISALIGNED_CALLS must shrink as helpers are fixed:\n  {}",
        stale.join("\n  ")
    );
}

/// The same discipline for the holes: a helper this audit learns to model must leave the
/// unanalyzable list, so the list keeps naming exactly what is NOT being checked.
#[test]
fn the_unanalyzable_allowlist_shrinks_only() {
    let (_, unanalyzable, seen) = audit();
    let mut stale = Vec::new();
    for (name, reason) in NOT_STATICALLY_ANALYZABLE {
        if !seen.iter().any(|emitted| emitted == name) {
            stale.push(format!("{name} is no longer emitted at all ({reason})"));
        } else if !unanalyzable.contains_key(*name) {
            stale.push(format!("{name} is analyzable now — delete it ({reason})"));
        }
    }
    let mut unlisted: Vec<String> = unanalyzable
        .iter()
        .filter(|(name, _)| {
            !NOT_STATICALLY_ANALYZABLE
                .iter()
                .any(|(listed, _)| listed == &name.as_str())
        })
        .map(|(name, reason)| format!("{name}: {reason}"))
        .collect();
    stale.sort();
    unlisted.sort();
    assert!(
        stale.is_empty(),
        "NOT_STATICALLY_ANALYZABLE must shrink:\n  {}",
        stale.join("\n  ")
    );
    assert!(
        unlisted.is_empty(),
        "these helpers defeated the walk and are not named as holes, so \
         every_x86_64_runtime_call_site_is_sysv_aligned silently skipped them:\n  {}",
        unlisted.join("\n  ")
    );
}

/// THE CURL RELEASE CHAIN IS NEVER ALLOWLISTED. Muting one of these is how the original bug
/// would come back, so the prohibition is a test rather than a comment.
#[test]
fn the_curl_release_chain_is_never_allowlisted() {
    for helper in CURL_RELEASE_CHAIN {
        assert!(
            !ALLOWED_MISALIGNED_CALLS.iter().any(|(name, _)| name == helper),
            "{helper} is on the path a CurlHandle takes into the elephc_curl bridge; it must \
             be FIXED, never allowlisted"
        );
        assert!(
            !NOT_STATICALLY_ANALYZABLE.iter().any(|(name, _)| name == helper),
            "{helper} is on the path a CurlHandle takes into the elephc_curl bridge; the \
             audit must keep being able to check it"
        );
    }
}
