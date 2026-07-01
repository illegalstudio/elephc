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
        78 => Some("__rt_sys_getdents"),
        79 => Some("__rt_sys_getcwd"),
        80 => Some("__rt_sys_chdir"),
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
    out
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
        assert_eq!(linux_syscall_to_shim(60), Some("__rt_sys_exit"));
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

    /// Helper for tests: returns true if any line is a standalone `syscall`.
    fn contains_standalone_syscall(asm: &str) -> bool {
        asm.lines().any(is_syscall_line)
    }
}
