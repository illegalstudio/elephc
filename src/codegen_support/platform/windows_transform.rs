//! Purpose:
//! Transforms Linux x86_64 assembly into Windows-compatible assembly by replacing
//! raw `syscall` sequences with calls to Win32 shim wrappers. This is the central
//! mechanism that lets the existing x86_64 runtime code emit SysV-convention argument
//! setup while running on Windows — the shims convert SysV registers to MSx64 ABI
//! before calling Win32 API functions.
//!
//! Called from:
//! - `crate::pipeline::compile()` on the emitted user assembly (Windows target only).
//! - `crate::runtime_cache::prepare_runtime_object()` on the runtime assembly
//!   before hashing/assembling (Windows target only).
//! - `crate::codegen::platform::target::Target::transform_assembly()` (test harness).
//!
//! Key details:
//! - Each `mov eax, <N>\n    syscall` pair is replaced by `call __rt_sys_<name>`.
//! - The syscall number N is the Linux x86_64 syscall number.
//! - Unsupported syscall numbers are rewritten to keep the number in `eax` and
//!   `call __rt_unsupported_syscall`, which prints `unsupported syscall: <N>` to
//!   stderr and exits — instead of a silent `int3` that would just crash.
//! - Runtime-internal calls (`call __rt_*`) are left unchanged — they are not syscalls.

/// Maps a Linux x86_64 syscall number to its Win32 shim function name.
///
/// Returns `None` for syscalls that do not yet have a Win32 equivalent.
/// Unsupported syscalls are routed to `__rt_unsupported_syscall`, which reports
/// the number on stderr and exits (see `transform_for_windows`).
fn linux_syscall_to_shim(linux_num: u32) -> Option<&'static str> {
    match linux_num {
        0 => Some("__rt_sys_read"),
        1 => Some("__rt_sys_write"),
        2 => Some("__rt_sys_open"),
        3 => Some("__rt_sys_close"),
        8 => Some("__rt_sys_lseek"),
        9 => Some("__rt_sys_mmap"),
        10 => Some("__rt_sys_mprotect"),
        11 => Some("__rt_sys_munmap"),
        12 => Some("__rt_sys_brk"),
        16 => Some("__rt_sys_ioctl"),
        20 => Some("__rt_sys_writev"),
        21 => Some("__rt_sys_access"),
        28 => Some("__rt_sys_accept4"),
        32 => Some("__rt_sys_dup2"),
        33 => Some("__rt_sys_dup"),
        41 => Some("__rt_sys_socket"),
        42 => Some("__rt_sys_connect"),
        43 => Some("__rt_sys_accept"),
        44 => Some("__rt_sys_sendto"),
        45 => Some("__rt_sys_recvfrom"),
        46 => Some("__rt_sys_sendmsg"),
        47 => Some("__rt_sys_recvmsg"),
        48 => Some("__rt_sys_shutdown"),
        49 => Some("__rt_sys_bind"),
        50 => Some("__rt_sys_listen"),
        51 => Some("__rt_sys_getsockname"),
        52 => Some("__rt_sys_getpeername"),
        53 => Some("__rt_sys_socketpair"),
        54 => Some("__rt_sys_setsockopt"),
        55 => Some("__rt_sys_getsockopt"),
        59 => Some("__rt_sys_execve"),
        60 => Some("__rt_sys_exit"),
        62 => Some("__rt_sys_kill"),
        63 => Some("__rt_sys_uname"),
        72 => Some("__rt_sys_fcntl"),
        77 => Some("__rt_sys_ftruncate"),
        78 => Some("__rt_sys_getdents"),
        79 => Some("__rt_sys_getcwd"),
        80 => Some("__rt_sys_chdir"),
        82 => Some("__rt_sys_rename"),
        83 => Some("__rt_sys_mkdir"),
        84 => Some("__rt_sys_rmdir"),
        85 => Some("__rt_sys_creat"),
        86 => Some("__rt_sys_link"),
        87 => Some("__rt_sys_unlink"),
        88 => Some("__rt_sys_symlink"),
        89 => Some("__rt_sys_readlink"),
        90 => Some("__rt_sys_chmod"),
        92 => Some("__rt_sys_chown"),
        93 => Some("__rt_sys_fchown"),
        94 => Some("__rt_sys_lchown"),
        96 => Some("__rt_sys_getpriority"),
        97 => Some("__rt_sys_setpriority"),
        98 => Some("__rt_sys_getrusage"),
        102 => Some("__rt_sys_getuid"),
        104 => Some("__rt_sys_getgid"),
        105 => Some("__rt_sys_setuid"),
        106 => Some("__rt_sys_setgid"),
        110 => Some("__rt_sys_getppid"),
        116 => Some("__rt_sys_sysinfo"),
        137 => Some("__rt_sys_statfs"),
        160 => Some("__rt_sys_uname"),
        202 => Some("__rt_sys_futex"),
        228 => Some("__rt_sys_clock_gettime"),
        230 => Some("__rt_sys_clock_getres"),
        262 => Some("__rt_sys_newfstatat"),
        270 => Some("__rt_sys_pselect6"),
        318 => Some("__rt_sys_getrandom"),
        _ => None,
    }
}

/// Transforms Linux x86_64 assembly to Windows-compatible assembly.
///
/// Replaces each `mov eax, <N>` followed by `syscall` with `call __rt_sys_<name>`.
/// The argument setup (in SysV registers rdi/rsi/rdx/r10/rcx/r8/r9) is left intact —
/// the Win32 shim wrappers read arguments from SysV registers and convert them to
/// MSx64 ABI before calling the corresponding Win32 API function.
///
/// Unsupported syscall numbers are rewritten to keep the number in `eax` and
/// `call __rt_unsupported_syscall`, the runtime helper that prints
/// `unsupported syscall: <N>` to stderr and exits — a visible diagnostic instead
/// of a silent `int3` crash.
pub fn transform_for_windows(asm: &str) -> String {
    let lines: Vec<&str> = asm.lines().collect();
    let mut out = String::with_capacity(asm.len() + 256);
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if let Some(num) = parse_linux_syscall(trimmed) {
            if i + 1 < lines.len() && is_syscall_line(lines[i + 1]) {
                match linux_syscall_to_shim(num) {
                    Some(shim) => {
                        out.push_str("    call ");
                        out.push_str(shim);
                        out.push('\n');
                    }
                    None => {
                        // No Win32 shim for this syscall: keep the number in eax
                        // and route to the runtime diagnostic helper, which prints
                        // "unsupported syscall: <N>" to stderr and exits — instead
                        // of a silent int3 that would just crash.
                        out.push_str("    mov eax, ");
                        out.push_str(&num.to_string());
                        out.push('\n');
                        out.push_str("    call __rt_unsupported_syscall\n");
                    }
                }
                i += 2;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
        i += 1;
    }
    let promoted = promote_independent_frame_regions(&out);
    add_safe_frame_unwind_metadata(&promoted)
}

/// Promotes independent internal frame starts to synthetic COFF-local function symbols.
///
/// Runtime emitters often place a frameless fast path or a completed helper before
/// an internal slow-path label that establishes its own RBP frame. PE requires a
/// distinct Function Table range for that later prologue. This pass inserts a
/// synthetic `.L` label only when the preceding linear region has balanced every
/// RBP push, so a still-active outer frame is never split accidentally. GNU as
/// discards `.L` symbols from the COFF symbol table, preventing runtime and user
/// objects from exporting colliding synthetic names.
fn promote_independent_frame_regions(asm: &str) -> String {
    let lines = asm.lines().collect::<Vec<_>>();
    let mut out = String::with_capacity(asm.len() + 512);
    let mut rbp_depth = 0usize;
    let mut synthetic_index = 0usize;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(".globl ") {
            rbp_depth = 0;
        }
        let is_canonical_start = trimmed == "push rbp"
            && lines.get(index + 1).is_some_and(|line| line.trim() == "mov rbp, rsp")
            && lines
                .get(index + 2)
                .is_some_and(|line| line.trim().starts_with("sub rsp, "));
        if is_canonical_start && rbp_depth == 0 && !global_label_immediately_precedes(&lines, index) {
            let symbol = format!(".L__elephc_unwind_region_{synthetic_index}");
            out.push_str(&symbol);
            out.push_str(":\n");
            synthetic_index += 1;
        }
        out.push_str(line);
        out.push('\n');
        if trimmed == "push rbp" {
            rbp_depth += 1;
        } else if matches!(trimmed, "pop rbp" | "leave") {
            rbp_depth = rbp_depth.saturating_sub(1);
        }
    }
    out
}

/// Returns whether a global symbol and its label directly own this prologue.
fn global_label_immediately_precedes(lines: &[&str], push_index: usize) -> bool {
    let significant = lines[..push_index]
        .iter()
        .rev()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .take(2)
        .map(|line| line.trim())
        .collect::<Vec<_>>();
    significant.len() == 2
        && significant[0].ends_with(':')
        && significant[1].starts_with(".globl ")
        && significant[1].trim_start_matches(".globl ").trim()
            == significant[0].trim_end_matches(':')
}

/// Adds PE/COFF unwind directives around exactly recognized x86_64 frames.
///
/// EIR codegen brackets its function-like regions with `@fn`/`@endfn` markers.
/// Within those regions every global body using the standard x86_64 frame
/// prologue gets a `.seh_proc` record describing the saved frame pointer, fixed
/// stack allocation, and register-allocator callee saves. Hand-written runtime
/// helpers are accepted only when their entire body passes the stricter runtime
/// audit: no unrepresented stack-pointer mutation, push/pop, ambiguous return,
/// or live-frame tail call is allowed.
fn add_safe_frame_unwind_metadata(asm: &str) -> String {
    let lines = asm.lines().collect::<Vec<_>>();
    let mut out = String::with_capacity(asm.len() + 512);
    let mut in_eir_region = false;
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with("# @fn ") {
            in_eir_region = true;
        }
        if let Some(symbol) = frame_region_symbol(trimmed) {
            let segment_end = function_segment_end(&lines, index);
            let segment = &lines[index..segment_end];
            let metadata = standard_frame_metadata(segment, symbol);
            let is_generated = in_eir_region;
            let is_safe_runtime = metadata
                .as_ref()
                .is_some_and(|metadata| audit_runtime_frame(segment, metadata).is_ok());
            if (is_generated || is_safe_runtime) && metadata.is_some() {
                let metadata = metadata.expect("frame metadata checked above");
                out.push_str(".seh_proc ");
                out.push_str(symbol);
                out.push('\n');
                for (relative, line) in lines[index..segment_end].iter().enumerate() {
                    out.push_str(line);
                    out.push('\n');
                    if relative == metadata.push_index {
                        out.push_str("    .seh_pushreg rbp\n");
                    }
                    if relative == metadata.stack_alloc_index {
                        out.push_str("    .seh_stackalloc ");
                        out.push_str(&metadata.stack_bytes.to_string());
                        out.push('\n');
                    }
                    if metadata
                        .frame_pointer
                        .is_some_and(|frame_pointer| frame_pointer.line_index == relative)
                    {
                        let frame_pointer = metadata.frame_pointer.expect("frame pointer checked");
                        out.push_str("    .seh_setframe rbp, ");
                        out.push_str(&frame_pointer.rsp_offset.to_string());
                        out.push('\n');
                    }
                    for save in metadata.saves.iter().filter(|save| save.line_index == relative) {
                        out.push_str("    .seh_savereg ");
                        out.push_str(save.register);
                        out.push_str(", ");
                        out.push_str(&save.rsp_offset.to_string());
                        out.push('\n');
                    }
                    if relative == metadata.prologue_end_index {
                        out.push_str("    .seh_endprologue\n");
                    }
                }
                out.push_str(".seh_endproc\n");
                index = segment_end;
                continue;
            }
        }
        out.push_str(lines[index]);
        out.push('\n');
        if trimmed.starts_with("# @endfn ") {
            in_eir_region = false;
        }
        index += 1;
    }
    out
}

/// Finds the end of one global text symbol without absorbing a following data section.
fn function_segment_end(lines: &[&str], start: usize) -> usize {
    lines[start + 1..]
        .iter()
        .position(|line| {
            let line = line.trim();
            frame_region_symbol(line).is_some()
                || line.starts_with("# @endfn ")
                || matches!(line, ".data" | ".bss" | ".text")
                || line.starts_with(".section ")
        })
        .map_or(lines.len(), |offset| start + 1 + offset)
}

/// Returns the function symbol introduced by a global directive or synthetic local label.
fn frame_region_symbol(line: &str) -> Option<&str> {
    line.strip_prefix(".globl ")
        .map(str::trim)
        .or_else(|| {
            line.strip_suffix(':')
                .filter(|symbol| symbol.starts_with(".L__elephc_unwind_region_"))
        })
}

/// Returns whether a hand-written runtime frame is fully described by `metadata`.
///
/// This deliberately recognizes a small subset. A rejected helper simply remains
/// without PE unwind data; accepting an ambiguous body would be worse because the
/// OS could restore a fabricated stack or register state during `RtlUnwindEx`.
fn audit_runtime_frame(
    lines: &[&str],
    metadata: &GeneratedFrameMetadata<'_>,
) -> Result<(), RuntimeFrameRejection> {
    let mut saw_return = false;
    let mut saw_safe_tail_exit = false;
    let mut saw_terminal_trap = false;
    let local_labels = lines
        .iter()
        .filter_map(|line| line.trim().strip_suffix(':'))
        .collect::<std::collections::HashSet<_>>();

    for (index, raw) in lines.iter().enumerate().skip(metadata.prologue_end_index + 1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') || line.ends_with(':') {
            continue;
        }

        if line.starts_with("push ") || (line.starts_with("pop ") && line != "pop rbp") {
            return Err(RuntimeFrameRejection::AdditionalPushOrPop);
        }
        if writes_stack_or_frame_pointer(line) {
            let is_epilogue = matches!(line, "mov rsp, rbp" | "pop rbp" | "leave")
                || is_fixed_frame_deallocation(lines, index, metadata.stack_bytes);
            let is_dynamic_rsp_with_frame_pointer = metadata.has_encodable_frame_pointer()
                && first_operand(line).is_some_and(|operand| {
                    matches!(operand, "rsp" | "esp" | "sp" | "spl")
                });
            if !is_epilogue && !is_dynamic_rsp_with_frame_pointer {
                return Err(RuntimeFrameRejection::UnencodableStackMutation);
            }
        }

        if line == "ret" {
            saw_return = true;
            let Some(previous) = previous_instruction(lines, index) else {
                return Err(RuntimeFrameRejection::AmbiguousReturn);
            };
            if previous != "leave"
                && previous != "pop rbp"
                && !metadata.has_encodable_frame_pointer()
            {
                return Err(RuntimeFrameRejection::AmbiguousReturn);
            }
        }
        if line == "ud2" {
            saw_terminal_trap = true;
        }

        if let Some(target) = line.strip_prefix("jmp ").map(str::trim) {
            if !local_labels.contains(target) {
                if !basic_block_released_frame(lines, index)
                    && !is_audited_runtime_tail(metadata, target)
                {
                    return Err(RuntimeFrameRejection::LiveFrameTailCall);
                }
                saw_safe_tail_exit = true;
            }
        }
    }

    if saw_return || saw_safe_tail_exit || saw_terminal_trap {
        Ok(())
    } else {
        Err(RuntimeFrameRejection::NoReturn)
    }
}

/// Recognizes runtime tail edges whose destination frame contract was audited explicitly.
fn is_audited_runtime_tail(
    metadata: &GeneratedFrameMetadata<'_>,
    target: &str,
) -> bool {
    matches!(
        (metadata.symbol, target, metadata.stack_bytes),
        (
            "__rt_mixed_numeric_add" | "__rt_mixed_numeric_sub",
            "__rt_mixed_numeric_common_linux_x86_64",
            80
        ) | ("__rt_opendir", "__rt_opendir_native_x86", 16)
    )
}

/// Returns whether the current basic block tore down its RBP frame before a tail exit.
fn basic_block_released_frame(lines: &[&str], before: usize) -> bool {
    lines[..before]
        .iter()
        .rev()
        .take_while(|line| !line.trim().ends_with(':'))
        .any(|line| matches!(line.trim(), "pop rbp" | "leave"))
}

/// Reason a canonical runtime frame cannot truthfully receive one unwind record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RuntimeFrameRejection {
    AdditionalPushOrPop,
    UnencodableStackMutation,
    AmbiguousReturn,
    LiveFrameTailCall,
    NoReturn,
}

/// Returns whether an `add rsp, frame_size` starts a canonical pop/return epilogue.
fn is_fixed_frame_deallocation(
    lines: &[&str],
    index: usize,
    stack_bytes: usize,
) -> bool {
    if lines[index].trim() != format!("add rsp, {stack_bytes}") {
        return false;
    }
    let Some((pop_index, pop)) = next_instruction(lines, index) else {
        return false;
    };
    if pop != "pop rbp" {
        return false;
    }
    next_instruction(lines, pop_index).is_some_and(|(_, instruction)| {
        instruction == "ret" || instruction.starts_with("jmp ")
    })
}

/// Returns the next non-comment, non-directive assembly instruction and its index.
fn next_instruction<'a>(lines: &[&'a str], after: usize) -> Option<(usize, &'a str)> {
    lines.iter().enumerate().skip(after + 1).find_map(|(index, line)| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') || line.ends_with(':') {
            None
        } else {
            Some((index, line))
        }
    })
}

/// Returns the previous non-comment, non-directive assembly instruction.
fn previous_instruction<'a>(lines: &[&'a str], before: usize) -> Option<&'a str> {
    lines[..before].iter().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') || line.ends_with(':') {
            None
        } else {
            Some(line)
        }
    })
}

/// Detects instructions that directly change `rsp` or `rbp`.
fn writes_stack_or_frame_pointer(line: &str) -> bool {
    let Some((mnemonic, operands)) = line.split_once(char::is_whitespace) else {
        return line == "leave";
    };
    if !instruction_writes_first_operand(mnemonic) {
        return false;
    }
    let destination = operands.split(',').next().unwrap_or(operands).trim();
    matches!(destination, "rsp" | "esp" | "sp" | "spl" | "rbp" | "ebp" | "bp" | "bpl")
}

/// Returns the first Intel-syntax operand of an instruction.
fn first_operand(line: &str) -> Option<&str> {
    let (_, operands) = line.split_once(char::is_whitespace)?;
    Some(operands.split(',').next().unwrap_or(operands).trim())
}

/// Returns whether a runtime segment is a true MS x64 leaf needing no unwind table.
#[cfg(test)]
fn is_leaf_without_unwind_state(lines: &[&str]) -> bool {
    lines.iter().all(|raw| {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('.') || line.ends_with(':') {
            return true;
        }
        !line.starts_with("call ")
            && !line.starts_with("push ")
            && !line.starts_with("pop ")
            && !writes_stack_or_frame_pointer(line)
            && !writes_windows_nonvolatile_register(line)
    })
}

/// Detects an explicit or string-instruction write to an MS x64 nonvolatile register.
#[cfg(test)]
fn writes_windows_nonvolatile_register(line: &str) -> bool {
    if line.starts_with("rep ")
        || line.starts_with("movs")
        || line.starts_with("stos")
        || line.starts_with("lods")
        || line.starts_with("scas")
    {
        return true;
    }
    let Some((mnemonic, operands)) = line.split_once(char::is_whitespace) else {
        return false;
    };
    if mnemonic == "xchg" {
        return operands
            .split(',')
            .any(|operand| is_windows_nonvolatile_register(operand.trim()));
    }
    instruction_writes_first_operand(mnemonic)
        && first_operand(line).is_some_and(is_windows_nonvolatile_register)
}

/// Returns whether an exact operand names an MS x64 nonvolatile register.
#[cfg(test)]
fn is_windows_nonvolatile_register(operand: &str) -> bool {
    matches!(
        operand,
        "rbx" | "ebx" | "bx" | "bl"
            | "rsi" | "esi" | "si" | "sil"
            | "rdi" | "edi" | "di" | "dil"
            | "r12" | "r12d" | "r12w" | "r12b"
            | "r13" | "r13d" | "r13w" | "r13b"
            | "r14" | "r14d" | "r14w" | "r14b"
            | "r15" | "r15d" | "r15w" | "r15b"
            | "xmm6" | "xmm7" | "xmm8" | "xmm9" | "xmm10" | "xmm11"
            | "xmm12" | "xmm13" | "xmm14" | "xmm15"
    )
}

/// Returns whether a mnemonic writes its first Intel-syntax operand.
fn instruction_writes_first_operand(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "mov"
            | "movabs"
            | "movsx"
            | "movsxd"
            | "movzx"
            | "lea"
            | "add"
            | "adc"
            | "sub"
            | "sbb"
            | "imul"
            | "and"
            | "or"
            | "xor"
            | "not"
            | "neg"
            | "inc"
            | "dec"
            | "shl"
            | "shr"
            | "sar"
            | "rol"
            | "ror"
            | "pop"
            | "cvtsi2sd"
            | "cvttsd2si"
            | "movq"
            | "movd"
            | "movsd"
            | "movss"
            | "movaps"
            | "movups"
            | "movdqa"
            | "movdqu"
    ) || mnemonic.starts_with("cmov")
        || mnemonic.starts_with("set")
}


/// Unwind-relevant indices and offsets for one standard generated frame.
struct GeneratedFrameMetadata<'a> {
    symbol: &'a str,
    push_index: usize,
    stack_alloc_index: usize,
    stack_bytes: usize,
    frame_pointer: Option<GeneratedFramePointer>,
    saves: Vec<GeneratedRegisterSave<'a>>,
    prologue_end_index: usize,
}

impl GeneratedFrameMetadata<'_> {
    /// Returns whether PE can use `rbp` to recover the fixed frame across dynamic RSP changes.
    fn has_encodable_frame_pointer(&self) -> bool {
        self.frame_pointer.is_some()
    }
}

/// One PE-encodable RBP frame-pointer declaration and its instruction position.
#[derive(Clone, Copy)]
struct GeneratedFramePointer {
    line_index: usize,
    rsp_offset: usize,
}

/// One nonvolatile register save expressed relative to the post-prologue RSP.
struct GeneratedRegisterSave<'a> {
    line_index: usize,
    register: &'a str,
    rsp_offset: usize,
}

/// Recognizes the standard generated frame prologue and derives safe SEH codes.
fn standard_frame_metadata<'a>(
    lines: &[&'a str],
    symbol: &'a str,
) -> Option<GeneratedFrameMetadata<'a>> {
    let label_index = lines.iter().position(|line| line.trim() == format!("{symbol}:"))?;
    let push_index = next_generated_prologue_instruction(lines, label_index)?;
    if lines[push_index].trim() != "push rbp" {
        return None;
    }
    let frame_pointer_index = next_generated_prologue_instruction(lines, push_index)?;
    if lines[frame_pointer_index].trim() != "mov rbp, rsp" {
        return None;
    }
    let stack_alloc_index = next_generated_prologue_instruction(lines, frame_pointer_index)?;
    if !lines[stack_alloc_index].trim().starts_with("sub rsp, ") {
        return None;
    }
    let stack_bytes = lines[stack_alloc_index]
        .trim()
        .trim_start_matches("sub rsp, ")
        .parse::<usize>()
        .ok()?;
    let frame_pointer = if stack_bytes <= 240 && stack_bytes % 16 == 0 {
        Some(GeneratedFramePointer {
            line_index: stack_alloc_index,
            rsp_offset: stack_bytes,
        })
    } else {
        next_generated_prologue_instruction(lines, stack_alloc_index).and_then(|adjustment_index| {
            let adjustment = lines[adjustment_index]
                .trim()
                .strip_prefix("sub rbp, ")?
                .parse::<usize>()
                .ok()?;
            let rsp_offset = stack_bytes.checked_sub(adjustment)?;
            (rsp_offset <= 240 && rsp_offset % 16 == 0).then_some(GeneratedFramePointer {
                line_index: adjustment_index,
                rsp_offset,
            })
        })
    };
    let save_marker = lines[stack_alloc_index + 1..]
        .iter()
        .position(|line| line.trim() == "# save callee-saved registers used by the register allocator")
        .map(|offset| stack_alloc_index + 1 + offset);
    let mut saves = Vec::new();
    if let Some(marker) = save_marker {
        for (line_index, line) in lines.iter().enumerate().skip(marker + 1) {
            if line.trim().starts_with("movsd QWORD PTR [rbp - ") {
                // The allocator currently has no callee-saved XMM pool on
                // x86_64. Refuse to annotate a future scalar-only XMM save:
                // Windows unwind metadata restores the full 128-bit register,
                // which would not match `movsd`'s 64-bit frame write.
                return None;
            }
            let Some((register, frame_offset)) = parse_generated_register_save(line) else {
                break;
            };
            if !matches!(register, "rbx" | "rsi" | "rdi" | "r12" | "r13" | "r14" | "r15") {
                return None;
            }
            let rsp_offset = stack_bytes.checked_sub(frame_offset)?;
            if rsp_offset % 8 != 0 {
                return None;
            }
            saves.push(GeneratedRegisterSave {
                line_index,
                register,
                rsp_offset,
            });
        }
    }
    let prologue_end_index = saves
        .last()
        .map_or(stack_alloc_index, |save| save.line_index)
        .max(frame_pointer.map_or(stack_alloc_index, |frame_pointer| frame_pointer.line_index));
    Some(GeneratedFrameMetadata {
        symbol,
        push_index,
        stack_alloc_index,
        stack_bytes,
        frame_pointer,
        saves,
        prologue_end_index,
    })
}

/// Finds the next real instruction in a generated prologue, skipping only
/// blank lines and emitter comments; encountering a directive is not ignored.
fn next_generated_prologue_instruction(lines: &[&str], after: usize) -> Option<usize> {
    lines.iter().enumerate().skip(after + 1).find_map(|(index, line)| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            None
        } else {
            Some(index)
        }
    })
}

/// Parses a register-allocator save of the form `[rbp - offset], register`.
fn parse_generated_register_save(line: &str) -> Option<(&str, usize)> {
    let line = line.trim();
    let line = line.strip_prefix("mov QWORD PTR [rbp - ")?;
    let (offset, register) = line.split_once("], ")?;
    Some((register.trim(), offset.parse().ok()?))
}

/// Extracts the syscall number from a `mov eax, <N>` instruction line.
/// Returns `Some(N)` if the line is a syscall number load, `None` otherwise.
fn parse_linux_syscall(line: &str) -> Option<u32> {
    let line = line.trim();
    if !line.starts_with("mov eax, ") {
        return None;
    }
    let rest = &line["mov eax, ".len()..];
    if rest.starts_with('#') || rest.starts_with("//") {
        return None;
    }
    let num_part = rest.split_whitespace().next()?;
    num_part.parse::<u32>().ok()
}

/// Returns `true` if the line is a standalone `syscall` instruction.
///
/// Tolerates leading/trailing whitespace and an optional trailing assembler
/// comment (`#` for the x86_64 emitter) so a `syscall` immediately following a
/// `mov eax, <N>` is still recognized even if a comment is ever appended. A
/// longer mnemonic that merely starts with `syscall` (e.g. `syscallfoo`) is not
/// matched.
fn is_syscall_line(line: &str) -> bool {
    let trimmed = line.trim();
    match trimmed.strip_prefix("syscall") {
        Some("") => true,
        Some(rest) => rest.starts_with(char::is_whitespace) || rest.starts_with('#'),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a `mov eax, 1; syscall` sequence is replaced by `call __rt_sys_write`.
    #[test]
    fn test_write_syscall_replacement() {
        let input = "    mov eax, 1\n    syscall\n";
        let output = transform_for_windows(input);
        assert!(output.contains("call __rt_sys_write"));
        assert!(!output.contains("syscall"));
    }

    /// Verifies that `mov eax, 60; syscall` (exit) is replaced by `call __rt_sys_exit`.
    #[test]
    fn test_exit_syscall_replacement() {
        let input = "    mov eax, 60\n    syscall\n";
        let output = transform_for_windows(input);
        assert!(output.contains("call __rt_sys_exit"));
        assert!(!output.contains("syscall"));
    }

    /// Verifies that an unsupported syscall number is routed to the runtime
    /// diagnostic helper (`call __rt_unsupported_syscall` with the number kept in
    /// eax) rather than a silent `int3`.
    #[test]
    fn test_unsupported_syscall_produces_diagnostic_call() {
        let input = "    mov eax, 999\n    syscall\n";
        let output = transform_for_windows(input);
        assert!(output.contains("mov eax, 999"));
        assert!(output.contains("call __rt_unsupported_syscall"));
        assert!(!output.contains("int3"));
        // The raw `    syscall` instruction line must be gone (the helper name
        // ending in "syscall" is a call operand, not a standalone instruction).
        assert!(!output.lines().any(|l| l.trim() == "syscall"));
    }

    /// Verifies that non-syscall `mov eax` lines are preserved.
    #[test]
    fn test_non_syscall_mov_eax_preserved() {
        let input = "    mov eax, 1\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains("mov eax, 1"));
        assert!(output.contains("ret"));
    }

    /// Verifies that syscall with comment after the number is still parsed.
    #[test]
    fn test_syscall_with_comment() {
        let input = "    mov eax, 1\n    syscall\n";
        let output = transform_for_windows(input);
        assert!(output.contains("call __rt_sys_write"));
    }

    /// Verifies the full mapping of common syscalls.
    #[test]
    fn test_linux_syscall_to_shim_mapping() {
        assert_eq!(linux_syscall_to_shim(0), Some("__rt_sys_read"));
        assert_eq!(linux_syscall_to_shim(1), Some("__rt_sys_write"));
        assert_eq!(linux_syscall_to_shim(3), Some("__rt_sys_close"));
        assert_eq!(linux_syscall_to_shim(9), Some("__rt_sys_mmap"));
        assert_eq!(linux_syscall_to_shim(21), Some("__rt_sys_access"));
        assert_eq!(linux_syscall_to_shim(60), Some("__rt_sys_exit"));
        assert_eq!(linux_syscall_to_shim(77), Some("__rt_sys_ftruncate"));
        assert_eq!(linux_syscall_to_shim(82), Some("__rt_sys_rename"));
        assert_eq!(linux_syscall_to_shim(98), Some("__rt_sys_getrusage"));
        assert_eq!(linux_syscall_to_shim(999), None);
    }

    /// Verifies that munmap (syscall 11) maps to its VirtualFree shim so it never
    /// degrades to `int3` if an x86_64 code path ever emits it inline.
    #[test]
    fn test_munmap_syscall_mapped() {
        assert_eq!(linux_syscall_to_shim(11), Some("__rt_sys_munmap"));
    }

    /// Verifies that a `syscall` line carrying a trailing assembler comment is
    /// still recognized and rewritten (hardening for comment-annotated output).
    #[test]
    fn test_syscall_with_trailing_comment_is_transformed() {
        let input = "    mov eax, 1\n    syscall  # write bytes\n";
        let output = transform_for_windows(input);
        assert!(output.contains("call __rt_sys_write"));
        assert!(!contains_standalone_syscall(&output));
    }

    /// Verifies the `is_syscall_line` predicate: bare, comment-suffixed, and
    /// whitespace-suffixed `syscall` match; a longer mnemonic does not.
    #[test]
    fn test_is_syscall_line_predicate() {
        assert!(is_syscall_line("    syscall"));
        assert!(is_syscall_line("syscall"));
        assert!(is_syscall_line("    syscall  # comment"));
        assert!(is_syscall_line("    syscall\t"));
        assert!(!is_syscall_line("    syscallfoo"));
        assert!(!is_syscall_line("    mov eax, 1"));
    }

    /// Verifies standard EIR frames receive PE unwind directives while their
    /// existing instruction stream stays intact.
    #[test]
    fn test_generated_eir_frame_receives_unwind_metadata() {
        let input = r#"    # @fn name=demo symbol=_fn_demo
.globl _fn_demo
_fn_demo:
    # prologue
    push rbp
    mov rbp, rsp
    sub rsp, 64
    ret
    # @endfn name=demo
"#;
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_proc _fn_demo"));
        assert!(output.contains("push rbp\n    .seh_pushreg rbp"));
        assert!(output.contains(
            "sub rsp, 64\n    .seh_stackalloc 64\n    .seh_setframe rbp, 64\n    .seh_endprologue"
        ));
        assert!(output.contains(".seh_endproc\n    # @endfn name=demo"));
    }

    /// Verifies a generated frame after a frameless fiber guard gets its own PE range.
    #[test]
    fn test_generated_eir_frame_after_fiber_guard_uses_local_unwind_region() {
        let input = r#"    # @fn name=demo symbol=_fn_demo
.globl _fn_demo
_fn_demo:
    mov r11, QWORD PTR [rip + _fiber_current]
    test r11, r11
    jz 8f
    call __rt_win_fiber_stack_overflow_abort
    ud2
8:
    push rbp
    mov rbp, rsp
    sub rsp, 64
    add rsp, 64
    pop rbp
    ret
    # @endfn name=demo
"#;
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_proc .L__elephc_unwind_region_0"));
        assert!(output.contains(".L__elephc_unwind_region_0:\n    push rbp"));
        assert!(!output.contains(".seh_proc _fn_demo"));
        assert!(output.contains("push rbp\n    .seh_pushreg rbp"));
        assert!(output.contains(".seh_stackalloc 64"));
        assert!(output.contains(".seh_endproc\n    # @endfn name=demo"));
    }

    /// Verifies integer register-allocator saves are represented relative to
    /// the final stack pointer so Windows unwinding restores them exactly.
    #[test]
    fn test_generated_eir_frame_describes_nonvolatile_register_saves() {
        let input = r#"    # @fn name=demo symbol=_fn_demo
.globl _fn_demo
_fn_demo:
    push rbp
    mov rbp, rsp
    sub rsp, 96
    mov r10, QWORD PTR [rip + _concat_off]
    mov QWORD PTR [rbp - 88], r10
    # save callee-saved registers used by the register allocator
    mov QWORD PTR [rbp - 80], rbx
    ret
    # @endfn name=demo
"#;
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_savereg rbx, 16"));
        assert!(output.contains(".seh_savereg rbx, 16\n    .seh_endprologue"));
    }

    /// Verifies a frame with a scalar XMM save is left unannotated because PE
    /// unwind records restore full 128-bit XMM state and would not match it.
    #[test]
    fn test_generated_eir_frame_rejects_partial_xmm_save() {
        let input = r#"    # @fn name=demo symbol=_fn_demo
.globl _fn_demo
_fn_demo:
    push rbp
    mov rbp, rsp
    sub rsp, 96
    # save callee-saved registers used by the register allocator
    movsd QWORD PTR [rbp - 72], xmm6
    ret
    # @endfn name=demo
"#;
        let output = transform_for_windows(input);
        assert!(!output.contains(".seh_"));
    }

    /// Verifies a merely similar stack sequence without the standard rbp frame
    /// shape is not assigned misleading generated-frame unwind metadata.
    #[test]
    fn test_generated_eir_frame_rejects_nonstandard_prologue() {
        let input = r#"    # @fn name=demo symbol=_fn_demo
.globl _fn_demo
_fn_demo:
    push rbp
    sub rsp, 64
    ret
    # @endfn name=demo
"#;
        let output = transform_for_windows(input);
        assert!(!output.contains(".seh_"));
    }

    /// Verifies a canonical hand-written runtime frame receives exact unwind metadata.
    #[test]
    fn test_standard_runtime_frame_receives_unwind_support() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    xor eax, eax\n    leave\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_proc __rt_demo"));
        assert!(output.contains(".seh_stackalloc 32"));
        assert!(output.contains(".seh_endproc"));
    }

    /// Verifies a frameless fast path followed by an independent slow frame gets its own PE range.
    #[test]
    fn test_independent_internal_frame_is_promoted_and_described() {
        let input = ".globl __rt_demo\n__rt_demo:\n    test eax, eax\n    jz __rt_demo_slow\n    ret\n__rt_demo_slow:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    leave\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains(".L__elephc_unwind_region_0:"));
        assert!(output.contains(".seh_proc .L__elephc_unwind_region_0"));
        assert!(!output.contains(".globl __elephc_unwind_region_0"));
    }

    /// Verifies independently transformed runtime and program objects link without
    /// exporting colliding synthetic unwind-region symbols.
    #[test]
    fn test_synthetic_unwind_regions_do_not_collide_between_coff_objects() {
        for tool in ["x86_64-w64-mingw32-as", "x86_64-w64-mingw32-gcc"] {
            if std::process::Command::new(tool).arg("--version").output().is_err() {
                eprintln!("skipping synthetic unwind link test: {tool} not found");
                return;
            }
        }

        let runtime = transform_for_windows(
            ".intel_syntax noprefix\n.text\n.globl __rt_runtime\n__rt_runtime:\n    test eax, eax\n    jz __rt_runtime_slow\n    ret\n__rt_runtime_slow:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    leave\n    ret\n",
        );
        let program = transform_for_windows(
            ".intel_syntax noprefix\n.text\n.globl _fn_program\n_fn_program:\n    test eax, eax\n    jz _fn_program_slow\n    ret\n_fn_program_slow:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    leave\n    ret\n",
        );
        let directory = std::env::temp_dir().join(format!(
            "elephc-local-unwind-link-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("create local unwind link directory");
        let runtime_assembly = directory.join("runtime.s");
        let runtime_object = directory.join("runtime.o");
        let program_assembly = directory.join("program.s");
        let program_object = directory.join("program.o");
        let executable = directory.join("combined.exe");
        std::fs::write(&runtime_assembly, runtime).expect("write transformed runtime assembly");
        std::fs::write(&program_assembly, program).expect("write transformed program assembly");

        for (assembly, object) in [
            (&runtime_assembly, &runtime_object),
            (&program_assembly, &program_object),
        ] {
            let assembled = std::process::Command::new("x86_64-w64-mingw32-as")
                .arg("-o")
                .arg(object)
                .arg(assembly)
                .output()
                .expect("run MinGW assembler for local unwind link test");
            assert!(
                assembled.status.success(),
                "MinGW rejected transformed assembly:\n{}",
                String::from_utf8_lossy(&assembled.stderr)
            );
        }

        let linked = std::process::Command::new("x86_64-w64-mingw32-gcc")
            .arg("-nostdlib")
            .arg("-Wl,-e,_fn_program")
            .arg(&runtime_object)
            .arg(&program_object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("link runtime and program COFF objects");
        let _ = std::fs::remove_dir_all(&directory);
        assert!(
            linked.status.success(),
            "synthetic unwind symbols collided across COFF objects:\n{}",
            String::from_utf8_lossy(&linked.stderr)
        );
    }

    /// Verifies an ambiguous runtime stack adjustment is never described as a fixed frame.
    #[test]
    fn test_runtime_frame_rejects_dynamic_stack_adjustment() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 256\n    sub rsp, 16\n    call __rt_other\n    add rsp, 16\n    leave\n    ret\n";
        let output = transform_for_windows(input);
        assert!(!output.contains(".seh_"));
    }

    /// Verifies an unrepresented nonvolatile-register push refuses runtime metadata.
    #[test]
    fn test_runtime_frame_rejects_unrepresented_nonvolatile_push() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    push rsi\n    pop rsi\n    leave\n    ret\n";
        let output = transform_for_windows(input);
        assert!(!output.contains(".seh_"));
    }

    /// Verifies an encodable frame pointer makes transient shadow-space changes unwindable.
    #[test]
    fn test_runtime_frame_describes_dynamic_rsp_with_frame_pointer() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    sub rsp, 32\n    call __rt_other\n    add rsp, 32\n    add rsp, 32\n    pop rbp\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_setframe rbp, 32"));
        assert!(output.contains(".seh_proc __rt_demo"));
    }

    /// Verifies an internal callable label can return while the outer RBP frame stays active.
    #[test]
    fn test_runtime_frame_describes_internal_subroutine_with_frame_pointer() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    call __rt_demo_inner\n    add rsp, 32\n    pop rbp\n    ret\n__rt_demo_inner:\n    xor eax, eax\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_setframe rbp, 32"));
        assert!(output.contains(".seh_proc __rt_demo"));
    }

    /// Verifies a live-frame tail call is rejected while a torn-down tail call stays harmless.
    #[test]
    fn test_runtime_frame_rejects_live_frame_tail_call() {
        let input = ".globl __rt_demo\n__rt_demo:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 32\n    jmp __rt_other\n";
        let output = transform_for_windows(input);
        assert!(!output.contains(".seh_"));
    }

    /// Verifies the audited mixed add entry can share the identical mul/common frame record.
    #[test]
    fn test_mixed_numeric_equivalent_frame_tail_is_described() {
        let input = ".globl __rt_mixed_numeric_add\n__rt_mixed_numeric_add:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 80\n    mov r10, 0\n    jmp __rt_mixed_numeric_common_linux_x86_64\n.globl __rt_mixed_numeric_mul\n__rt_mixed_numeric_mul:\n    push rbp\n    mov rbp, rsp\n    sub rsp, 80\n__rt_mixed_numeric_common_linux_x86_64:\n    add rsp, 80\n    pop rbp\n    ret\n";
        let output = transform_for_windows(input);
        assert!(output.contains(".seh_proc __rt_mixed_numeric_add"));
        assert!(output.contains(".seh_proc __rt_mixed_numeric_mul"));
    }

    /// Measures the all-features runtime audit and keeps both accepted and refused
    /// hand-written frame populations non-empty.
    #[test]
    fn test_full_windows_runtime_has_conservative_unwind_coverage() {
        let target = crate::codegen::platform::Target::new(
            crate::codegen::platform::Platform::Windows,
            crate::codegen::platform::Arch::X86_64,
        );
        let raw = crate::codegen::generate_runtime_with_features_pic(
            8 * 1024 * 1024,
            target,
            crate::codegen::RuntimeFeatures::all(),
            false,
        );
        let transformed = transform_for_windows(&raw);
        let canonical_frames = raw
            .match_indices("    push rbp\n    mov rbp, rsp\n    sub rsp, ")
            .count();
        let described_runtime_frames = transformed.matches(".seh_proc ").count();
        let raw_lines = raw.lines().collect::<Vec<_>>();
        let mut runtime_functions = 0usize;
        let mut leaf_functions = 0usize;
        let mut rejection_counts = std::collections::HashMap::new();
        let mut rejected_symbols = Vec::new();
        for (index, line) in raw_lines.iter().enumerate() {
            let Some(symbol) = line.trim().strip_prefix(".globl ") else {
                continue;
            };
            if !symbol.starts_with("__rt_") {
                continue;
            }
            runtime_functions += 1;
            let end = function_segment_end(&raw_lines, index);
            let segment = &raw_lines[index..end];
            if is_leaf_without_unwind_state(segment) {
                leaf_functions += 1;
            }
            if let Some(metadata) = standard_frame_metadata(segment, symbol) {
                if let Err(reason) = audit_runtime_frame(segment, &metadata) {
                    *rejection_counts.entry(reason).or_insert(0usize) += 1;
                    rejected_symbols.push((symbol, reason));
                }
            }
        }
        let functions_requiring_unwind = runtime_functions - leaf_functions;
        let rejected_canonical_frames = rejection_counts.values().sum::<usize>();
        eprintln!(
            "Windows runtime unwind audit: {runtime_functions} functions, {leaf_functions} true leaves, {functions_requiring_unwind} requiring unwind state; {described_runtime_frames}/{canonical_frames} canonical frame regions described, {rejected_canonical_frames} global frames rejected {rejection_counts:?}: {rejected_symbols:?}"
        );
        assert!(canonical_frames > 0, "runtime audit fixture has no canonical frames");
        assert!(
            rejection_counts.is_empty(),
            "runtime globals with rejected canonical frames remain: {rejected_symbols:?}"
        );
        assert_eq!(
            described_runtime_frames, canonical_frames,
            "every canonical stack-mutating region must have exactly one PE unwind record"
        );
        assert!(leaf_functions > 0, "runtime audit found no true leaf helpers");
        assert!(
            described_runtime_frames <= functions_requiring_unwind,
            "described more frames than the non-leaf population"
        );
    }

    /// Assembles the all-features runtime with MinGW and verifies decoded PE unwind tables.
    #[test]
    fn test_full_windows_runtime_unwind_metadata_assembles_and_decodes() {
        if std::process::Command::new("x86_64-w64-mingw32-as")
            .arg("--version")
            .output()
            .is_err()
            || std::process::Command::new("x86_64-w64-mingw32-objdump")
                .arg("--version")
                .output()
                .is_err()
        {
            eprintln!("skipping runtime unwind object test: MinGW tools not found");
            return;
        }

        let target = crate::codegen::platform::Target::new(
            crate::codegen::platform::Platform::Windows,
            crate::codegen::platform::Arch::X86_64,
        );
        let transformed = transform_for_windows(
            &crate::codegen::generate_runtime_with_features_pic(
                8 * 1024 * 1024,
                target,
                crate::codegen::RuntimeFeatures::all(),
                false,
            ),
        );
        let directory = std::env::temp_dir().join(format!(
            "elephc-runtime-unwind-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("create runtime unwind test directory");
        let assembly_path = directory.join("runtime.s");
        let object_path = directory.join("runtime.o");
        std::fs::write(&assembly_path, transformed).expect("write transformed runtime assembly");

        let assembled = std::process::Command::new("x86_64-w64-mingw32-as")
            .arg("-o")
            .arg(&object_path)
            .arg(&assembly_path)
            .output()
            .expect("run MinGW assembler");
        assert!(
            assembled.status.success(),
            "MinGW rejected runtime unwind metadata:\n{}",
            String::from_utf8_lossy(&assembled.stderr)
        );
        let inspected = std::process::Command::new("x86_64-w64-mingw32-objdump")
            .arg("-x")
            .arg(&object_path)
            .output()
            .expect("run MinGW objdump");
        let headers = String::from_utf8_lossy(&inspected.stdout);
        let _ = std::fs::remove_dir_all(&directory);
        assert!(inspected.status.success(), "objdump failed to inspect runtime object");
        assert!(headers.contains(".pdata"), "runtime object has no .pdata section");
        assert!(headers.contains(".xdata"), "runtime object has no .xdata section");
        assert!(
            headers.contains("The Function Table") && headers.contains("UnwindData"),
            "objdump did not decode the runtime function table"
        );
    }

    /// Helper for tests: returns true if any line is a standalone `syscall`.
    fn contains_standalone_syscall(asm: &str) -> bool {
        asm.lines().any(is_syscall_line)
    }
}
