//! Purpose:
//! Finds every runtime instruction that writes a process-global data symbol instead of `_rt_ctx`.
//!
//! Called from:
//! - The `global_write_audit` tests below, over each supported target's all-feature runtime.
//!
//! Key details:
//! - A symbol's address is tracked through the register that materialized it: `adrp` + `add
//!   @PAGEOFF`/`:lo12:` or a GOT load on AArch64, `lea [rip + sym]` or a GOT load on x86_64.
//!   A store through that register (or a direct `[rip + sym]` destination) is a global write.
//! - Any other instruction that redefines the register forgets the symbol, so a load of the
//!   pointer stored AT the symbol (heap tables) is not mistaken for writing the symbol itself.
//! - This is a name-independent audit: it reads the emitted assembly, so a hand-rolled access
//!   that bypasses the `abi::` accessors is found as surely as an accessor call.

use super::Arch;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Strips one relocation decoration and returns the bare symbol, if the operand names one.
fn relocated_symbol(operand: &str) -> Option<String> {
    let operand = operand.trim().trim_start_matches('#');
    let operand = operand.strip_prefix(":lo12:").unwrap_or(operand);
    let operand = operand.strip_prefix(":got_lo12:").unwrap_or(operand);
    let operand = operand.strip_prefix(":got:").unwrap_or(operand);
    let symbol = operand.split('@').next().unwrap_or(operand).trim();
    (symbol.starts_with('_') && symbol.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then(|| symbol.to_owned())
}

/// Maps a sub-register spelling to the 64-bit register it aliases.
fn canonical(register: &str, arch: Arch) -> String {
    let register = register.trim();
    match arch {
        Arch::AArch64 => match register.strip_prefix('w') {
            Some(number) if number.bytes().all(|b| b.is_ascii_digit()) => format!("x{number}"),
            _ => register.to_owned(),
        },
        Arch::X86_64 => {
            const FAMILIES: [(&str, &[&str]); 8] = [
                ("rax", &["eax", "ax", "al"]), ("rbx", &["ebx", "bx", "bl"]),
                ("rcx", &["ecx", "cx", "cl"]), ("rdx", &["edx", "dx", "dl"]),
                ("rsi", &["esi", "si", "sil"]), ("rdi", &["edi", "di", "dil"]),
                ("rbp", &["ebp", "bp", "bpl"]), ("rsp", &["esp", "sp", "spl"]),
            ];
            for (full, aliases) in FAMILIES {
                if aliases.contains(&register) { return full.to_owned(); }
            }
            // r8d / r8w / r8b alias r8.
            let trimmed = register.trim_end_matches(['d', 'w', 'b']);
            if register.starts_with('r') && trimmed.len() > 1
                && trimmed[1..].bytes().all(|b| b.is_ascii_digit()) {
                return trimmed.to_owned();
            }
            register.to_owned()
        }
    }
}

/// Splits `mnemonic op1, op2, ...` keeping bracketed operands whole.
fn operands(line: &str) -> (String, Vec<String>) {
    let (mnemonic, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in rest.chars() {
        match ch {
            '[' => { depth += 1; current.push(ch); }
            ']' => { depth -= 1; current.push(ch); }
            ',' if depth == 0 => { parts.push(current.trim().to_owned()); current.clear(); }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() { parts.push(current.trim().to_owned()); }
    (mnemonic.to_ascii_lowercase(), parts)
}

/// The base register (or direct symbol) of a memory operand such as `[x9, #8]` or `[rip + _s]`.
enum Base { Register(String), Symbol(String) }

/// Reads the base of a bracketed memory operand.
fn memory_base(operand: &str, arch: Arch) -> Option<Base> {
    let open = operand.find('[')?;
    let close = operand.rfind(']')?;
    let inside = &operand[open + 1..close];
    if arch == Arch::X86_64 {
        if inside.replace(' ', "").starts_with("rip+") {
            let symbol = inside.split('+').nth(1)?.trim();
            return relocated_symbol(symbol).map(Base::Symbol);
        }
        let base = inside.split(['+', '-', '*']).next()?.trim();
        return Some(Base::Register(canonical(base, arch)));
    }
    let mut fields = inside.split(',');
    let base = fields.next()?.trim();
    if let Some(offset) = fields.next() {
        if offset.contains("@PAGEOFF") || offset.contains(":lo12:") {
            if let Some(symbol) = relocated_symbol(offset) { return Some(Base::Symbol(symbol)); }
        }
    }
    Some(Base::Register(canonical(base, arch)))
}

/// AArch64 mnemonics whose FIRST operand is a source and whose memory operand is written.
fn aarch64_store(mnemonic: &str) -> bool {
    mnemonic.starts_with("st") && !mnemonic.starts_with("sta")
        || mnemonic.starts_with("swp") || mnemonic.starts_with("cas")
        || mnemonic.starts_with("ldadd") || mnemonic.starts_with("ldclr")
        || mnemonic.starts_with("ldset") || mnemonic.starts_with("ldeor")
}

/// x86_64 mnemonics that only read a memory first operand.
fn x86_reads_only(mnemonic: &str) -> bool {
    matches!(mnemonic, "cmp" | "test" | "push" | "bt" | "ucomisd" | "comisd" | "ptest"
        | "prefetcht0" | "prefetchnta" | "call" | "jmp")
}

/// Returns symbol -> helpers for every global data symbol the runtime writes.
pub(super) fn global_writes(asm: &str, arch: Arch, comment: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut helper = String::new();
    let mut held: HashMap<String, String> = HashMap::new();
    let mut writes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut record = |symbol: String, helper: &str| {
        writes.entry(symbol).or_default().insert(helper.to_owned());
    };
    for raw in asm.lines() {
        let line = raw.split(comment).next().unwrap_or(raw).trim();
        if let Some(global) = line.strip_prefix(".globl ") {
            helper = global.trim().to_owned();
            held.clear();
            continue;
        }
        if line.is_empty() || line.starts_with('.') || line.ends_with(':') { continue; }
        let line = line.strip_prefix("lock ").unwrap_or(line);
        let (mnemonic, ops) = operands(line);
        let Some(first) = ops.first() else { continue };
        match arch {
            Arch::AArch64 => {
                if aarch64_store(&mnemonic) {
                    if let Some(memory) = ops.iter().find(|op| op.contains('[')) {
                        match memory_base(memory, arch) {
                            Some(Base::Symbol(symbol)) => record(symbol, &helper),
                            Some(Base::Register(base)) => {
                                if let Some(symbol) = held.get(&base) { record(symbol.clone(), &helper); }
                            }
                            None => {}
                        }
                    }
                    // Exclusive/atomic forms also write a status or old-value register.
                    if mnemonic.starts_with("stx") || mnemonic.starts_with("stlx") {
                        held.remove(&canonical(first, arch));
                    }
                    continue;
                }
                let destination = canonical(first, arch);
                let tracked = match mnemonic.as_str() {
                    "adrp" => ops.get(1).and_then(|op| relocated_symbol(op)),
                    "add" if ops.get(2).is_some_and(|op| op.contains("@PAGEOFF") || op.contains(":lo12:")) => {
                        ops.get(2).and_then(|op| relocated_symbol(op))
                    }
                    "ldr" if ops.get(1).is_some_and(|op| op.contains("@GOTPAGEOFF") || op.contains(":got_lo12:")) => {
                        ops.get(1).and_then(|op| {
                            let inside = op.split(',').nth(1)?;
                            relocated_symbol(inside.trim_end_matches(']'))
                        })
                    }
                    _ => None,
                };
                match tracked {
                    Some(symbol) => { held.insert(destination, symbol); }
                    None => {
                        held.remove(&destination);
                        if mnemonic == "ldp" {
                            if let Some(second) = ops.get(1) { held.remove(&canonical(second, arch)); }
                        }
                    }
                }
            }
            Arch::X86_64 => {
                if first.contains('[') {
                    if x86_reads_only(&mnemonic) { continue; }
                    match memory_base(first, arch) {
                        Some(Base::Symbol(symbol)) => record(symbol, &helper),
                        Some(Base::Register(base)) => {
                            if let Some(symbol) = held.get(&base) { record(symbol.clone(), &helper); }
                        }
                        None => {}
                    }
                    continue;
                }
                let destination = canonical(first, arch);
                let tracked = match mnemonic.as_str() {
                    "lea" => ops.get(1).and_then(|op| match memory_base(op, arch) {
                        Some(Base::Symbol(symbol)) => Some(symbol),
                        _ => None,
                    }),
                    "mov" if ops.get(1).is_some_and(|op| op.contains("@GOTPCREL")) => {
                        ops.get(1).and_then(|op| op.split_whitespace().find(|word| word.contains("@GOTPCREL"))
                            .and_then(relocated_symbol))
                    }
                    _ => None,
                };
                match tracked {
                    Some(symbol) => { held.insert(destination, symbol); }
                    None if mnemonic == "xchg" => {
                        held.remove(&destination);
                        if let Some(second) = ops.get(1) { held.remove(&canonical(second, arch)); }
                    }
                    None if !x86_reads_only(&mnemonic) => { held.remove(&destination); }
                    None => {}
                }
            }
        }
    }
    writes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tracker follows each target's address materialization and forgets redefined registers.
    #[test]
    fn global_write_tracker_follows_address_registers_on_both_architectures() {
        let arm = ".globl __rt_a\nadrp x9, _g@PAGE\nadd x9, x9, _g@PAGEOFF\nstr x0, [x9]\n\
                   adrp x10, _h@PAGE\nadd x10, x10, _h@PAGEOFF\nldr x10, [x10]\nstr x1, [x10]\n\
                   str xzr, [x11, _k@PAGEOFF]\nldr x12, [x13, _p@GOTPAGEOFF]\nstrb w2, [x12, #3]\n";
        let found = global_writes(arm, Arch::AArch64, "//");
        assert_eq!(found.keys().cloned().collect::<Vec<_>>(), ["_g", "_k", "_p"], "{found:?}");
        let x86 = ".globl __rt_b\nlea r10, [rip + _g]\nmov QWORD PTR [r10 + 8], rax\n\
                   mov QWORD PTR [rip + _k], 0\ncmp QWORD PTR [rip + _c], 0\n\
                   mov r11, QWORD PTR _p@GOTPCREL[rip]\ninc DWORD PTR [r11]\n\
                   lea rax, [rip + _h]\nmov rax, QWORD PTR [rax]\nmov QWORD PTR [rax], 1\n";
        let found = global_writes(x86, Arch::X86_64, "#");
        assert_eq!(found.keys().cloned().collect::<Vec<_>>(), ["_g", "_k", "_p"], "{found:?}");
    }

    /// Process-wide state by design: the context pool itself, POSIX signal dispatch (signals are
    /// delivered to the process), eval hook pointers installed once at startup, and the resource
    /// id registry, which serializes every mutation behind `_resource_id_lock`.
    const PROCESS_WIDE: &[&str] = &[
        "__rt_pcntl_async_enabled", "__rt_pcntl_dispatch_handler", "__rt_pcntl_dispatching",
        "__rt_pcntl_handler_descriptor", "__rt_pcntl_handler_kind", "__rt_pcntl_handler_value",
        "_elephc_eval_array_reference_retire_fn", "_elephc_eval_closure_bind_ctx_fn",
        "_elephc_eval_closure_bind_fn", "_elephc_eval_dynamic_object_destruct_fn",
        "_elephc_eval_ob_handler_fn", "_elephc_eval_ob_release_fn", "_elephc_eval_object_clone_fn",
        "_elephc_eval_object_gc_child_fn", "_elephc_eval_object_release_fn", "_resource_id_keys",
        "_resource_id_lock", "_resource_id_next", "_resource_id_vals", "_rt_ctx", "_rt_ctx_pool",
        "_rt_ctx_state",
    ];

    /// Execution state still written as a process global: a Parallel worker and the coordinator
    /// race on every one of these. Migrate each into `_rt_ctx` and delete it here; this list may
    /// only shrink (tracked in beads elephc-u3p7.2).
    const PER_CONTEXT_DEBT: &[&str] = &[
        "_accept_peer_len", "_accept_peer_ptr", "_debuginfo_call_line", "_debuginfo_guard",
        "_debuginfo_pad", "_fgc_url_addr", "_fgc_url_retr", "_float_spill_index", "_fsockopen_addr",
        "_ftp_cmd_scratch", "_ftp_data_addr", "_ftp_use_tls", "_hash_write_guard_top",
        "_heap_consolidated_frees", "_heap_fail_request",
        "_http_active_host_len", "_http_active_host_ptr", "_http_active_ignore_errors",
        "_http_active_max_redirects", "_http_active_proxy_len", "_http_active_proxy_ptr",
        "_http_active_timeout_seconds", "_http_redirect_path_buf", "_http_redirect_path_len",
        "_itoa_spill_index", "_json_active_depth", "_json_active_flags", "_json_depth_limit",
        "_json_error_candidate_ptr", "_json_error_column", "_json_error_line",
        "_json_error_location_active", "_json_error_scan_bytes", "_json_error_scan_count",
        "_json_error_source_len", "_json_error_source_ptr", "_json_indent_depth", "_json_last_error",
        "_json_validate_idx", "_json_validate_len", "_json_validate_ptr", "_ob_flushing",
        "_ob_implicit_flush", "_ob_in_handler", "_phar_write_len",
        "_phar_write_out", "_phar_write_tpl_len", "_phar_write_url_len", "_phar_write_url_ptr",
        "_php_default_tz_len", "_php_tz_env", "_php_tz_save", "_preg_exec_error", "_preg_last_error",
        "_recvfrom_addr_len", "_recvfrom_addr_ptr", "_rt_diag_warning_message_len",
        "_rt_reference_validation_pins_top", "_ser_legacy_active", "_ser_stream",
        "_simplexml_unwind_stream", "_sleep_warning_suffix", "_stream_close_silent",
        "_stream_context_options", "_stream_error_code", "_stream_error_tls", "_stream_grow_scratch",
        "_stream_open_opened_path_scratch", "_strtotime_clock", "_strtotime_iso_buf",
        "_uncaught_report_mode", "_unser_call_line", "_unser_legacy_active", "_url_stat_matched",
        "_user_wrapper_drain_buf", "_vd_indent", "_vd_seen", "_vd_seen_n",
    ];

    /// No runtime helper may write a new process global, and migrated debt cannot linger listed.
    #[test]
    fn runtime_global_writes_only_shrink_on_every_supported_target() {
        use crate::codegen_support::{driver_support::generate_runtime_with_features, platform::Target, RuntimeFeatures};
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let asm = generate_runtime_with_features(8 * 1024 * 1024, target, RuntimeFeatures::all());
            let found = global_writes(&asm, target.arch, target.line_comment_prefix());
            let unexpected: Vec<String> = found.iter()
                .filter(|(symbol, _)| !PROCESS_WIDE.contains(&symbol.as_str())
                    && !PER_CONTEXT_DEBT.contains(&symbol.as_str()))
                .map(|(symbol, helpers)| format!("{symbol} (written by {})",
                    helpers.iter().cloned().collect::<Vec<_>>().join(", ")))
                .collect();
            assert!(unexpected.is_empty(),
                "{name}: runtime helpers write process-global state; route it through `_rt_ctx`:\n{}",
                unexpected.join("\n"));
            seen.extend(found.into_keys());
        }
        let stale: Vec<&str> = PER_CONTEXT_DEBT.iter().copied()
            .filter(|symbol| !seen.contains(*symbol)).collect();
        assert!(stale.is_empty(), "no longer written as globals; delete from PER_CONTEXT_DEBT: {stale:?}");
    }
}
