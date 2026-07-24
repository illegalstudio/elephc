//! Unit tests for the Win32 shim emitters (mirrors the assembly assertions
//! used throughout the `shims_*` submodules).

use super::*;
use crate::codegen::platform::Target;

/// Verifies x86_64 stat consumers read the portable layout written by Windows shims.
#[test]
fn test_windows_stat_consumers_use_portable_layout_offsets() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_stat(&mut emitter);
    crate::codegen_support::runtime::io::emit_stat_ext(&mut emitter);
    crate::codegen_support::runtime::io::emit_stat_array(&mut emitter);
    let asm = emitter.output();

    assert!(asm.contains("mov r9d, DWORD PTR [rsp + 16]"));
    assert!(asm.contains("mov eax, DWORD PTR [rsp + 16]"));
    assert!(asm.contains("mov eax, DWORD PTR [rbp - 136]"));
    assert!(!asm.contains("mov r9d, DWORD PTR [rsp + 24]"));
}

/// Verifies that Win32 shims emit the expected symbols for windows-x86_64.
#[test]
fn test_win32_shims_emit_expected_symbols() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    for sym in [
        "__rt_sys_write",
        "__rt_sys_read",
        "__rt_sys_exit",
        "__rt_sys_close",
        "__rt_sys_mmap",
        "__rt_sys_open",
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", sym)),
            "Win32 shim missing global symbol {}",
            sym
        );
    }
}

/// Verifies that Win32 imports are declared.
#[test]
fn test_win32_imports_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".extern GetStdHandle"));
    assert!(asm.contains(".extern WriteFile"));
    assert!(asm.contains(".extern ExitProcess"));
    assert!(asm.contains(".extern HeapAlloc"));
    assert!(asm.contains(".extern WSASend"));
    assert!(asm.contains(".extern WSARecv"));
    assert!(asm.contains(".extern PeekNamedPipe"));
    assert!(asm.contains(".extern GetTempFileNameW"));
    assert!(asm.contains(".extern getprotobyname"));
    assert!(asm.contains(".extern getprotobynumber"));
    assert!(asm.contains(".extern getservbyname"));
    assert!(asm.contains(".extern getservbyport"));
}

/// Verifies Windows netdb helpers use Winsock shims rather than Unix database files.
#[test]
fn test_win32_netdb_runtime_helpers_call_winsock() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_getprotobyname(&mut emitter);
    crate::codegen_support::runtime::io::emit_getprotobynumber(&mut emitter);
    crate::codegen_support::runtime::io::emit_getservbyname(&mut emitter);
    crate::codegen_support::runtime::io::emit_getservbyport(&mut emitter);
    let asm = emitter.output();

    assert!(asm.contains("call __rt_sys_getprotobyname"));
    assert!(asm.contains("call __rt_sys_getprotobynumber"));
    assert!(asm.contains("call __rt_sys_getservbyname"));
    assert!(asm.contains("call __rt_sys_getservbyport"));
    assert!(asm.contains("movsx rax, WORD PTR [rax + 16]"));
    assert!(asm.contains("test rcx, rcx"));
    assert!(asm.contains("jz __rt_gsbn_windows_missing"));
    assert!(asm.contains("movzx eax, WORD PTR [rax + 24]"));
    assert!(asm.contains("call __rt_str_persist"));
    assert!(asm.matches("sub rsp, 8").count() >= 2);
    assert!(asm.matches("add rsp, 8").count() >= 4);
    assert!(!asm.contains("__rt_protoent_load"));
    assert!(!asm.contains("__rt_servent_load"));
}

/// Verifies that fd-to-HANDLE conversion consistently uses the CRT descriptor table.
#[test]
fn test_fd_to_handle_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_fd_to_handle(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call _get_osfhandle"));
    assert!(!asm.contains("call GetStdHandle"));
}

/// Verifies opaque Windows stream descriptors are mapped to bounded slots before
/// EOF/filter state is indexed, and that close releases the mapping and state.
#[test]
fn test_stream_state_uses_bounded_opaque_handle_slots() {
    let mut registry = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win_stream_slot_registry(&mut registry);
    emit_win_stream_slot_clear(&mut registry);
    emit_win_stream_mark_timed_out(&mut registry);
    let registry_asm = registry.output();
    assert!(registry_asm.contains(".globl __rt_win_stream_slot\n"));
    assert!(registry_asm.contains("cmp ecx, 256"));
    assert!(registry_asm.contains("_win_stream_slot_handles"));
    assert!(registry_asm.contains("_win_stream_slot_used"));
    assert!(registry_asm.contains("_win_stream_timed_out"));
    assert!(registry_asm.contains("_stream_read_filters"));
    assert!(registry_asm.contains("_stream_write_filters"));

    let mut close = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_close(&mut close);
    let close_asm = close.output();
    assert_eq!(close_asm.matches("call __rt_win_stream_slot_clear").count(), 2);

    let mut feof = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_feof(&mut feof);
    let feof_asm = feof.output();
    assert!(feof_asm.contains("call __rt_win_stream_slot"));
    assert!(feof_asm.contains("mov rdi, rax"));
    assert!(feof_asm.contains("BYTE PTR [r10 + rdi]"));

    let mut consumers = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_fread(&mut consumers);
    crate::codegen_support::runtime::io::emit_fgets(&mut consumers);
    crate::codegen_support::runtime::io::emit_fwrite(&mut consumers);
    crate::codegen_support::runtime::io::emit_fopen(&mut consumers);
    crate::codegen_support::runtime::io::emit_stream_get_line(&mut consumers);
    crate::codegen_support::runtime::io::emit_stream_get_meta_data(&mut consumers);
    crate::codegen_support::runtime::io::emit_streams_ext(&mut consumers);
    crate::codegen_support::runtime::io::emit_stream_filter_attach_user(&mut consumers);
    crate::codegen_support::runtime::io::emit_apply_user_stream_filter(&mut consumers);
    crate::codegen_support::runtime::io::emit_user_filter_release_fd(&mut consumers);
    let consumers_asm = consumers.output();
    assert!(consumers_asm.matches("call __rt_win_stream_slot").count() >= 12);
    assert!(consumers_asm.contains("_stream_read_filters"));
    assert!(consumers_asm.contains("_stream_write_filters"));
    assert!(consumers_asm.contains("_eof_flags"));
    assert!(consumers_asm.contains("_win_stream_timed_out"));
    assert!(consumers_asm.contains("__rt_win_stream_mark_timed_out"));
    assert!(consumers_asm.contains("cmp rax, -3"));
    assert!(consumers_asm.contains("jne __rt_fread_would_block_x86"));
    assert!(consumers_asm.contains("mov rax, -1"));

    let mut timeout = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_stream_set_timeout(&mut timeout);
    let timeout_asm = timeout.output();
    assert!(timeout_asm.contains("mov rdx, 20"));
    assert!(timeout_asm.contains("mov rdx, 21"));
    assert_eq!(timeout_asm.matches("call __rt_sys_setsockopt").count(), 2);
    assert!(timeout_asm.contains("_win_stream_timed_out"));
}

/// Verifies optional write-filter handle tables also use compact slots on Windows.
#[test]
fn test_optional_filter_handles_use_windows_stream_slots() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::stream_filters::zlib::emit_x86_64(
        &mut emitter,
        "zlib_write",
        "zlib_close",
        "zlib_skip",
        6,
    );
    crate::codegen_support::stream_filters::bzip2::emit_compress_x86_64(
        &mut emitter,
        "bz2_write",
        "bz2_close",
        "bz2_skip",
        9,
        0,
    );
    crate::codegen_support::stream_filters::iconv_write::emit_iconv_write_attach_with_labels(
        &mut emitter,
        "_iconv_from",
        "_iconv_to",
        |prefix| format!("{}_slot_test", prefix),
    );
    let asm = emitter.output();
    assert!(asm.matches("call __rt_win_stream_slot").count() >= 12);
    assert!(asm.contains("_zstream_handles"));
    assert!(asm.contains("_bzstream_handles"));
    assert!(asm.contains("_iconv_handles"));
    assert!(!asm.contains("QWORD PTR [r9 + rdi*8]"));
}

/// Verifies that file close releases the CRT descriptor rather than bypassing its table.
#[test]
fn test_close_shim_uses_crt_descriptor_contract() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_close(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call _close"));
    assert!(!asm.contains("call CloseHandle"));
}

/// Verifies that the main wrapper shuffles MSx64 args to SysV and initializes Winsock.
#[test]
fn test_main_wrapper_shuffles_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_main_wrapper(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call __rt_winsock_init"), "main wrapper must call winsock init");
    assert!(asm.contains("call __elephc_main"));
    // argc/argv are spilled to the stack across the winsock init call (rcx/rdx
    // are volatile on MSx64 and clobbered by WSAStartup) and reloaded into the
    // SysV arg registers rdi/rsi before __elephc_main.
    assert!(asm.contains("mov QWORD PTR [rsp + 0], rcx"), "argc must be spilled before the init call");
    assert!(asm.contains("mov QWORD PTR [rsp + 8], rdx"), "argv must be spilled before the init call");
    assert!(asm.contains("mov rdi, QWORD PTR [rsp + 0]"), "argc must be reloaded into rdi after the init call");
    assert!(asm.contains("mov rsi, QWORD PTR [rsp + 8]"), "argv must be reloaded into rsi after the init call");
    // The winsock init call must occur before __elephc_main so sockets work.
    let init_pos = asm.find("call __rt_winsock_init");
    let main_pos = asm.find("call __elephc_main");
    assert!(init_pos.is_some() && main_pos.is_some() && init_pos < main_pos);
}

/// Verifies that newly added shims for previously-missing syscalls are emitted.
#[test]
fn test_new_shims_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    for sym in [
        "__rt_sys_lseek",
        "__rt_sys_socketpair",
        "__rt_sys_statfs",
        "__rt_sys_pselect6",
        "__rt_sys_sendmsg",
        "__rt_sys_recvmsg",
        "__rt_sys_getdents",
        "__rt_sys_creat",
        "__rt_sys_clock_getres",
        "__rt_sys_newfstatat",
        "__rt_sys_dup",
        "__rt_sys_dup2",
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", sym)),
            "Win32 shim missing global symbol {}",
            sym
        );
    }
}

/// Verifies that fcntl preserves F_GETFL state and applies F_SETFL through FIONBIO.
#[test]
fn test_fcntl_tracks_flags_and_applies_socket_nonblocking_mode() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_fcntl(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("__rt_win_fd_status_find"));
    assert!(asm.contains("__rt_win_fd_status_upsert"));
    assert!(asm.contains("mov edx, 0x8004667e"));
    assert!(asm.contains("call ioctlsocket"));
}

/// Verifies Windows ownership shims reject even existing paths like php-src.
#[test]
fn test_libc_chown_shims_return_failure_without_path_probe() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_libc_chown_unsupported(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_libc_chown\n"), "chown shim label missing");
    assert!(asm.contains(".globl __rt_sys_libc_lchown\n"), "lchown shim label missing");
    assert_eq!(
        asm.matches("mov rax, -1").count(),
        2,
        "both chown and lchown must return failure"
    );
    assert!(
        !asm.contains("__rt_sys_access"),
        "Windows chown/lchown must not probe or mutate the path"
    );
}

/// Verifies argv bootstrap delegates quote/backslash parsing to shell32 and converts every
/// resulting Unicode argument into independently owned strict UTF-8 storage.
#[test]
fn test_init_argv_uses_native_unicode_quoting_and_balanced_ownership() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_sys_init_argv(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_init_argv");
    assert!(section.contains("call GetCommandLineW"));
    assert!(section.contains("call CommandLineToArgvW"));
    assert!(!section.contains("call GetCommandLineA"));
    assert_eq!(section.matches("call __rt_win_utf16_to_utf8").count(), 2);
    assert!(section.contains("call LocalFree"));
    assert!(section.contains(".Linit_argv_w_cleanup"));
    assert!(section.matches("call __rt_heap_free").count() >= 3);
    assert!(section.contains("mov QWORD PTR [rip + _global_argc], rax"));
    assert!(section.contains("mov QWORD PTR [rip + _global_argv], rax"));
}

/// Verifies process argv cleanup releases each converted string before its
/// pointer table and clears the globals so exit may invoke it defensively.
#[test]
fn test_free_argv_releases_runtime_heap_ownership_and_is_idempotent() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_sys_free_argv(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_free_argv");
    assert_eq!(section.matches("call __rt_heap_free").count(), 2);
    assert!(section.contains("mov rax, QWORD PTR [r10 + rcx * 8]"));
    assert!(section.contains("mov QWORD PTR [rip + _global_argc], 0"));
    assert!(section.contains("mov QWORD PTR [rip + _global_argv], 0"));
}

/// Verifies that open shim handles PHP fopen creation flags, including exclusive create.
#[test]
fn test_open_shim_handles_flags() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_open(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("0x40"), "O_CREAT check missing");
    assert!(asm.contains("0x80"), "O_EXCL check missing");
    assert!(asm.contains("0x200"), "O_TRUNC check missing");
    assert!(asm.contains("0x400"), "O_APPEND check missing");
}

/// Verifies Windows `fopen` parses PHP's full base-mode and suffix-mode surface.
#[test]
fn test_windows_fopen_mode_parser_emits_full_php_modes() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    crate::codegen_support::runtime::io::emit_fopen(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("__rt_fopen_check_x_x86"));
    assert!(asm.contains("__rt_fopen_check_c_x86"));
    assert!(asm.contains("__rt_fopen_modifier_next_x86"));
    assert!(asm.contains("__rt_fopen_modifier_t_x86"));
    assert!(asm.contains("0x80"), "x mode must retain O_EXCL");
    assert!(asm.contains("0x4000"), "t mode must retain _O_TEXT");
}

/// Verifies `open` converts Unicode/extended paths once and stages the seven-argument
/// `CreateFileW` call in non-overlapping MSx64 stack slots.
#[test]
fn test_open_shim_uses_wide_path_and_windows_call_layout() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_open(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_open");
    assert!(section.contains("sub rsp, 88"));
    assert_eq!(section.matches("call __rt_win_utf8_to_utf16").count(), 1);
    assert!(section.contains("mov rcx, QWORD PTR [rsp + 56]"));
    assert!(section.contains("mov QWORD PTR [rsp + 32], r10"));
    assert!(section.contains("mov QWORD PTR [rsp + 40], 0"));
    assert!(section.contains("mov QWORD PTR [rsp + 48], 0"));
    assert!(section.contains("call CreateFileW"));
    assert!(section.contains("call _open_osfhandle"));
    assert!(section.contains("cmp eax, -1"));
    assert!(section.contains("mov edx, 0x8000"));
    assert!(section.contains("test DWORD PTR [rsp + 64], 0x4000"));
    assert!(section.contains("mov edx, 0x4000"));
    assert!(!section.contains("call CreateFileA"));
    assert!(section.contains("call __rt_heap_free"));
    assert!(section.contains("call GetLastError"));
    assert!(section.contains("__rt_win32_last_error"));
    assert!(section.contains("__rt_errno"));
}

/// Verifies path-based stat uses the Unicode attribute and handle APIs while preserving
/// the documented Windows structure offsets and releasing its converted long path.
#[test]
fn test_stat_shim_uses_wide_metadata_and_windows_layouts() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_stat(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_stat");
    assert!(section.contains("sub rsp, 168"));
    assert!(section.contains("add rsp, 168"));
    assert_eq!(section.matches("call __rt_win_utf8_to_utf16").count(), 1);
    assert!(section.contains("call GetFileAttributesExW"));
    assert!(!section.contains("call GetFileAttributesExA"));
    assert!(section.contains("mov eax, DWORD PTR [rsp + 64]"));
    assert!(section.contains("mov edx, DWORD PTR [rsp + 60]"));
    assert!(section.contains("lea rdi, [rsp + 88]"));
    assert!(section.contains("mov eax, DWORD PTR [rsp + 132]"));
    assert!(section.contains("mov ecx, DWORD PTR [rsp + 136]"));
    assert!(section.contains("call CreateFileW"));
    assert!(!section.contains("call CreateFileA"));
    assert!(section.contains("call GetFileInformationByHandle"));
    assert!(section.contains("call __rt_heap_free"));
    assert!(section.contains("call GetLastError"));
    assert!(section.contains("__rt_errno"));
}

/// Verifies that getrandom caps BCrypt's 32-bit length and reports the partial count.
#[test]
fn test_getrandom_returns_count() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_getrandom(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("BCryptGenRandom"));
    assert!(asm.contains("mov rax, 4294967295"));
    assert!(asm.contains("cmova rsi, rax"));
    assert!(asm.contains("mov QWORD PTR [rsp + 32], rsi"));
    assert!(asm.contains(".Lgetrandom_fail"));
}

/// Verifies that kill shim uses OpenProcess+TerminateProcess for SIGKILL.
#[test]
fn test_kill_uses_terminate_process() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_kill(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("OpenProcess"));
    assert!(asm.contains("TerminateProcess"));
}

/// Verifies that writev shim saves iov pointer on stack (not in rsi).
#[test]
fn test_writev_saves_iov_ptr() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_writev(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("[rsp + 8]"), "iov pointer should be saved on stack");
}

/// Verifies that _dup and _dup2 are in the imports list.
#[test]
fn test_dup_imports_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".extern _dup"));
    assert!(asm.contains(".extern _dup2"));
}

/// Verifies that the 6 previously-missing __rt_sys_* shims are now emitted.
#[test]
fn test_missing_sys_shims_now_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    for sym in [
        "__rt_sys_link",
        "__rt_sys_symlink",
        "__rt_sys_readlink",
        "__rt_sys_chown",
        "__rt_sys_fchown",
        "__rt_sys_lchown",
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", sym)),
            "Missing __rt_sys_* shim: {}",
            sym
        );
    }
}

/// Verifies realtime conversion keeps its divisor out of `rdx`, while monotonic
/// requests use the performance counter and write through the real second
/// `clock_gettime` argument (`rsi`), not the clock id in `rdi`.
#[test]
fn test_clock_gettime_realtime_and_monotonic_contracts() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_clock_gettime(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("xor rdx, rdx"), "RDX must be cleared before div");
    assert!(asm.contains("mov r11, 10000000"), "Divisor should be in r11");
    assert!(asm.contains("div r11"), "Should divide by r11, not rdx");
    assert!(!asm.contains("div rdx"), "Must NOT divide by rdx (crash bug)");
    assert!(asm.contains("cmp edi, 1"), "CLOCK_MONOTONIC must be recognized");
    assert!(asm.contains("call QueryPerformanceCounter"));
    assert!(asm.contains("call QueryPerformanceFrequency"));
    assert!(asm.contains("mov QWORD PTR [rsi], rax"));
    assert!(asm.contains("mov QWORD PTR [rsi + 8], rax"));
    assert!(
        !asm.contains("mov QWORD PTR [rdi], rax"),
        "clock_id must never be mistaken for the timespec pointer"
    );
}

/// Verifies every `localtime` exit returns elephc's extended struct-tm buffer,
/// including the no-bridge/unresolvable-zone fallback and UCRT's out-of-range
/// null result, so date formatters can safely read `tm_gmtoff` at offset 40.
#[test]
fn test_localtime_fallback_synthesizes_extended_tm_layout() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_localtime(&mut emitter);
    let asm = emitter.output();
    let fallback = asm
        .split(".Lsys_localtime_fallback:")
        .nth(1)
        .expect("localtime fallback label");
    assert!(fallback.contains("call localtime"));
    assert!(fallback.contains("test rax, rax"));
    assert!(fallback.contains("call _mkgmtime"));
    assert!(fallback.contains("lea r8, [rip + _win_tz_tm_buf]"));
    assert!(fallback.contains("mov QWORD PTR [r8 + 40], r11"));
    assert!(fallback.contains("call __rt_win_safe_gmtime"));
    assert_eq!(
        fallback.matches("mov rax, r8").count(),
        2,
        "both the msvcrt and out-of-range fallback paths must return the extended buffer"
    );
}

/// Verifies localtime publishes the bridge-owned transition abbreviation as `tm_zone`.
#[test]
fn test_localtime_resolves_transition_abbreviation() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_localtime(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("_elephc_tz_abbreviation_fn"));
    assert!(asm.contains("mov QWORD PTR [rbp - 40], rax"));
    assert!(asm.contains("mov QWORD PTR [r8 + 48], r11"));
}

/// Verifies Windows `mktime` resolves the IANA offset again at the first UTC
/// candidate, preventing the naive wall-clock lookup from selecting the wrong
/// side of a nearby DST transition.
#[test]
fn test_mktime_rechecks_offset_at_candidate_instant() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_mktime(&mut emitter);
    let asm = emitter.output();
    assert_eq!(
        asm.matches("call r11").count(),
        2,
        "mktime must perform an initial and candidate-instant bridge lookup"
    );
    assert!(asm.contains("mov QWORD PTR [rbp - 32], r11"));
    assert!(asm.contains(".Lsys_mktime_use_first_candidate"));
}

/// Verifies the Win32 mktime boundary shifts UCRT-rejected 1900..1969 years by one cycle.
#[test]
fn test_mktime_shifts_pre_1970_for_ucrt() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_mktime(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("cmp eax, 70"));
    assert!(asm.contains("add DWORD PTR [rdi + 20], 400"));
    assert!(asm.contains("sub DWORD PTR [r10 + 20], 400"));
    assert!(asm.contains(&format!("movabs r10, {}", 146_097_i64 * 86_400)));
}

/// Verifies that utimensat sets all seven `CreateFileW` arguments.
#[test]
fn test_utimensat_has_all_createfile_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("utimensat"));
    assert!(asm.contains("call __rt_win_utf8_to_utf16"));
    assert!(asm.contains("call CreateFileW"));
    // arg 5: dwCreationDisposition at [rsp+32]
    assert!(asm.contains("[rsp + 32], 3"));
    // arg 6: dwFlagsAndAttributes at [rsp+40] (FILE_FLAG_BACKUP_SEMANTICS, so directories open too)
    assert!(asm.contains("[rsp + 40], 0x2000000"));
    // arg 7: hTemplateFile at [rsp+48]
    assert!(asm.contains("[rsp + 48], 0"));
    // SetFileTime actually receives the requested atime/mtime FILETIMEs, not NULL
    assert!(asm.contains("call SetFileTime"));
    assert!(asm.contains("lea r8, [rsp + 72]"));
    assert!(asm.contains("lea r9, [rsp + 80]"));
}

/// Verifies the rename shim translates the Win32 `MoveFileExW` BOOL result
/// (nonzero = success) to the POSIX convention (`0` = success, `-1` =
/// failure) that `__rt_rename` tests with `cmp eax, 0` — without it a
/// successful rename would be reported as a failure and vice versa.
#[test]
fn test_rename_translates_bool_to_posix() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_rename(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call MoveFileExW"), "rename must overwrite via MoveFileExW");
    assert_eq!(asm.matches("call __rt_win_utf8_to_utf16").count(), 2);
    assert!(asm.contains("mov r8d, 3"), "MOVEFILE_REPLACE_EXISTING | MOVEFILE_COPY_ALLOWED");
    assert!(asm.contains("test eax, eax"), "must test the Win32 BOOL result");
    assert!(asm.contains(".Lrename_fail"), "must branch to the POSIX failure path");
    assert!(asm.contains("xor eax, eax"), "success translates to POSIX 0");
    assert!(asm.contains("mov rax, -1"), "failure translates to POSIX -1");
}

/// Verifies that symlink shim retries with ALLOW_UNPRIVILEGED_CREATE.
#[test]
fn test_symlink_unprivileged_retry() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("mov r8, 2"), "Should set ALLOW_UNPRIVILEGED_CREATE");
    assert!(asm.contains(".Lsymlink_ok"));
    assert!(asm.contains(".Lsymlink_fail"));
}

/// Verifies that readlink keeps CreateFile stack arguments separate from owned paths.
#[test]
fn test_readlink_clean_stack_layout() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "readlink");
    assert!(section.contains("mov QWORD PTR [rsp + 88], rsi"));
    assert!(section.contains("mov QWORD PTR [rsp + 96], rdx"));
    assert!(section.contains("mov QWORD PTR [rsp + 40], 0x2200000"));
    assert!(!asm.contains("Wait"), "No leftover debugging comments");
}

/// Verifies that all shims use 16-byte aligned stack frames (sub rsp, 40 not 32).
///
/// A bare `sub rsp, 32` directly off a shim's own entry would misalign the stack
/// (32 is a multiple of 16, but such shims are entered at rsp ≡ 8 mod 16, so they
/// need `sub rsp, K` with K ≡ 8 mod 16 — 40 or 56 — to re-align before the call).
/// Two patterns legitimately use `sub rsp, 32` anyway, both re-aligned by a
/// *different* instruction first: `__rt_sys_exit`, which executes `and rsp, -16`
/// to force 16-byte alignment (safe because the shim never returns) before its
/// `sub rsp, 32` ExitProcess shadow space; and `Emitter::emit_native_bridge_call`
/// (a ≤4-arg native-bridge call — see its own unit tests), whose `sub rsp, 32`
/// MSx64 shadow space follows a `push rbp; mov rbp, rsp; sub rsp, <multiple of
/// 16>` frame in the calling shim (e.g. WF9's `__rt_sys_localtime`/
/// `__rt_sys_mktime`, which call the elephc-tz bridge through it) — that
/// established rsp ≡ 0 mod 16 already, which `sub rsp, 32` preserves, and its
/// immediately-preceding emitted line is always `mov r11, <reg>` (the fn-ptr
/// relocation, skipped only when the pointer is already in r11). Permit
/// `sub rsp, 32` only when the immediately-preceding emitted line is
/// `and rsp, -16` or starts with `mov r11, `; reject it anywhere else.
#[test]
fn test_stack_alignment_16_bytes() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    let lines: Vec<&str> = asm.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("sub rsp, 32") {
            let prev = if i > 0 { lines[i - 1].trim() } else { "" };
            assert!(
                prev.starts_with("and rsp, -16") || prev.starts_with("mov r11, "),
                "Only the force-aligned exit shim or a native-bridge call's fn-ptr \
                 relocation (`Emitter::emit_native_bridge_call`) may use sub rsp, 32; \
                 found a bare sub rsp, 32 (misaligned off a shim's own rsp ≡ 8 mod 16 \
                 entry) not preceded by `and rsp, -16` or `mov r11, ...`. Use 40 or 56."
            );
        }
    }
}

/// Verifies that sendto passes all 6 arguments.
#[test]
fn test_sendto_passes_6_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socket_shims(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_sendto");
    assert!(section.contains("mov QWORD PTR [rsp + 32], rax"), "sendto must stage translated/native dest_addr as the fifth MSx64 argument");
    assert!(section.contains("mov QWORD PTR [rsp + 40], rax"), "sendto must stage translated/native addrlen as the sixth MSx64 argument");
    assert!(section.contains("mov r9, QWORD PTR [rsp + 56]"), "sendto must preserve flags before registry lookups");
}

/// Verifies that Windows rejects AF_UNIX before consulting the native provider.
#[test]
fn test_af_unix_is_rejected_for_php_windows_parity() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socket_shims(&mut emitter);
    let asm = emitter.output();
    let socket = shim_section(&asm, "__rt_sys_socket");
    assert!(
        socket.contains("cmp edi, 1")
            && socket.contains("je .Lsocket_unix_unsupported")
            && socket.contains("mov DWORD PTR [rip + __rt_errno], 97"),
        "AF_UNIX must fail with EAFNOSUPPORT before Winsock dispatch"
    );
}

/// Verifies that Windows socketpair uses php-src's AF_INET loopback emulation.
#[test]
fn test_socketpair_emulates_php_windows_af_inet_surface() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socketpair(&mut emitter);
    let asm = emitter.output();
    let socketpair = shim_section(&asm, "__rt_sys_socketpair");
    assert!(
        socketpair.contains("cmp edi, 2")
            && socketpair.contains("call socket")
            && socketpair.contains("call bind")
            && socketpair.contains("call connect")
            && socketpair.contains("call accept"),
        "Windows socketpair must create an AF_INET loopback pair"
    );
    assert!(
        socketpair.contains("mov DWORD PTR [rip + __rt_errno], 92"),
        "non-AF_INET families must fail with ENOPROTOOPT"
    );
}

/// Verifies that recvfrom passes all 6 arguments.
#[test]
fn test_recvfrom_passes_6_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socket_shims(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_recvfrom");
    assert!(section.contains("mov QWORD PTR [rsp + 32], rax"), "recvfrom must stage native/temporary src_addr as the fifth MSx64 argument");
    assert!(section.contains("mov QWORD PTR [rsp + 40], rax"), "recvfrom must stage native/temporary addrlen as the sixth MSx64 argument");
    assert!(section.contains("call __rt_win_unix_write_sockaddr"), "recvfrom must restore AF_UNIX source addresses after loopback reception");
}

/// Verifies that setsockopt passes all 5 arguments.
#[test]
fn test_setsockopt_passes_5_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("[rsp + 32], r8"), "setsockopt: optlen should be at [rsp+32]");
    assert!(asm.contains("mov r9, r10"), "setsockopt: optval should go to r9");
}

/// Verifies that getsockopt passes all 5 arguments.
#[test]
fn test_getsockopt_passes_5_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_getsockopt(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("[rsp + 32], r8"), "getsockopt: &optlen should be at [rsp+32]");
    assert!(asm.contains("mov r9, r10"), "getsockopt: optval should go to r9");
}

/// F5 regression: `setsockopt` translates POSIX `SOL_SOCKET` (1) to Winsock's `0xFFFF`
/// before calling Winsock `setsockopt`, gated on the ORIGINAL (pre-translation) level.
#[test]
fn test_setsockopt_translates_sol_socket_level() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("cmp rsi, 1"),
        "must test the original POSIX SOL_SOCKET level"
    );
    assert!(
        section.contains("mov rsi, 0xffff"),
        "POSIX SOL_SOCKET(1) must translate to Winsock 0xFFFF"
    );
}

/// F5 regression: `setsockopt` translates POSIX `SO_RCVTIMEO`(20)/`SO_SNDTIMEO`(21) to
/// Winsock's `0x1006`/`0x1005`, and other `SOL_SOCKET` optnames (`SO_REUSEADDR`,
/// `SO_KEEPALIVE`, `SO_BROADCAST`) to their Winsock numeric values.
#[test]
fn test_setsockopt_translates_sol_socket_optnames() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("mov rdx, 0x1006"),
        "SO_RCVTIMEO(20) must translate to Winsock 0x1006"
    );
    assert!(
        section.contains("mov rdx, 0x1005"),
        "SO_SNDTIMEO(21) must translate to Winsock 0x1005"
    );
    assert!(
        section.contains("    mov rdx, 4\n"),
        "SO_REUSEADDR(2) must translate to Winsock 4"
    );
    assert!(
        section.contains("    mov rdx, 8\n"),
        "SO_KEEPALIVE(9) must translate to Winsock 8"
    );
    assert!(
        section.contains("mov rdx, 0x20"),
        "SO_BROADCAST(6) must translate to Winsock 0x20"
    );
}

/// F5 regression: an unmapped `SOL_SOCKET` optname (e.g. POSIX `SO_LINGER`=13, which
/// elephc never emits and the shim has no Winsock translation for) must pass through
/// UNCHANGED rather than being silently dropped or corrupted — the shim falls through to
/// the unconditional "pass through unchanged" jump instead of matching a known `je` case.
#[test]
fn test_setsockopt_unknown_optname_falls_through_unchanged() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        !section.contains("cmp rdx, 13"),
        "no known translation exists for SO_LINGER(13): it must not be special-cased"
    );
    assert!(
        section.contains("jmp .Lsetsockopt_after_optname"),
        "unmatched SOL_SOCKET optnames must fall through to the untranslated path"
    );
}

/// F5 regression: php-src's Windows trap — `SO_RCVTIMEO`/`SO_SNDTIMEO` take a plain
/// `DWORD` millisecond count on Winsock, not a `struct timeval`. The shim must convert
/// the POSIX timeval at `[r10]` (`tv_sec`@+0, `tv_usec`@+8) into milliseconds and
/// redirect `optval`/`optlen` (`optlen=4`) at that DWORD before calling Winsock.
#[test]
fn test_setsockopt_converts_timeval_to_ms_for_timeout_options() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("mov rax, QWORD PTR [r10 + 8]"),
        "must read tv_usec from the POSIX timeval"
    );
    assert!(
        section.contains("mov rax, QWORD PTR [r10]"),
        "must read tv_sec from the POSIX timeval"
    );
    assert!(
        section.contains("idiv r11"),
        "must divide tv_usec by 1000 to get whole milliseconds"
    );
    assert!(
        section.contains("imul rax, rax, 1000"),
        "must scale tv_sec to milliseconds"
    );
    assert!(
        section.contains("add rax, r11"),
        "ms = tv_sec*1000 + tv_usec/1000"
    );
    assert!(
        section.contains("mov DWORD PTR [rsp + 48], eax"),
        "must stash the computed ms as a 32-bit DWORD"
    );
    assert!(
        section.contains("lea r10, [rsp + 48]"),
        "optval must be redirected to the ms DWORD"
    );
    assert!(
        section.contains("    mov r8, 4\n"),
        "optlen must be overridden to sizeof(DWORD) = 4 for the timeout payload"
    );
}

/// Register-clobber regression: the ms-conversion's `cqo`/`idiv` pair destructively
/// overwrites `rdx` (`cqo` sign-extends `rax` into `rdx:rax`; `idiv` then leaves the
/// division remainder in `rdx`), but `rdx` is exactly where the Winsock optname
/// (`0x1006`/`0x1005`) was staged just before entering the timeout payload. A prior
/// version of this shim let `idiv` silently replace the optname with
/// `tv_usec % 1000`, so Winsock received a garbage optname on every SO_RCVTIMEO/
/// SO_SNDTIMEO call. The shim must stash `rdx` to the unused `[rsp + 40]` pad slot
/// BEFORE `cqo` and restore it AFTER `idiv` completes, strictly in that order.
#[test]
fn test_setsockopt_preserves_optname_across_timeout_division() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("mov QWORD PTR [rsp + 40], rdx"),
        "optname must be stashed to the unused [rsp + 40] pad slot before the division"
    );
    assert!(
        section.contains("mov rdx, QWORD PTR [rsp + 40]"),
        "optname must be restored from [rsp + 40] after the division"
    );
    let stash_pos = section
        .find("mov QWORD PTR [rsp + 40], rdx")
        .expect("optname stash missing");
    let cqo_pos = section.find("cqo").expect("cqo missing");
    let idiv_pos = section.find("idiv r11").expect("idiv missing");
    let restore_pos = section
        .find("mov rdx, QWORD PTR [rsp + 40]")
        .expect("optname restore missing");
    assert!(
        stash_pos < cqo_pos && cqo_pos < idiv_pos && idiv_pos < restore_pos,
        "optname must be stashed before cqo and restored only after idiv completes \
         (stash={stash_pos}, cqo={cqo_pos}, idiv={idiv_pos}, restore={restore_pos})"
    );
    // The restored optname must actually reach the Winsock call's 3rd argument (r8),
    // not just sit in rdx unused — `mov r8, rdx` in `.Lsetsockopt_after_optname` must
    // come after the restore, otherwise the fix stashes/restores a value nothing reads.
    let stage_optname_pos = section
        .rfind("mov r8, rdx")
        .expect("optname staging into r8 missing");
    assert!(
        restore_pos < stage_optname_pos,
        "restored optname must flow to `mov r8, rdx` before the Winsock call"
    );
}

/// F5 regression: non-`SOL_SOCKET` levels (e.g. POSIX `IPPROTO_TCP`=6, used by
/// `TCP_NODELAY`) must skip optname/level translation entirely — both are already
/// numerically identical to their Winsock counterparts.
#[test]
fn test_setsockopt_non_sol_socket_level_skips_translation() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("cmp rsi, 1") && section.contains("je .Lsetsockopt_sol_socket"),
        "optname translation must be gated on level == POSIX SOL_SOCKET"
    );
    assert!(
        section.contains("jne .Lsetsockopt_after_level"),
        "level translation must also be gated so non-SOL_SOCKET levels stay unchanged"
    );
}

/// F5 regression: elephc's IPv6 stream-server emits `IPV6_V6ONLY` at level
/// `IPPROTO_IPV6`=41 with the Linux optname 26; Winsock uses 27 (level 41 is identical
/// on both). The shim must gate on the original level==41 and translate optname 26 → 27.
#[test]
fn test_setsockopt_translates_ipv6_v6only_optname() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("cmp rsi, 41") && section.contains("je .Lsetsockopt_ipv6"),
        "IPPROTO_IPV6 optname translation must be gated on the original level == 41"
    );
    assert!(
        section.contains("    mov rdx, 27\n"),
        "IPV6_V6ONLY(26) must translate to Winsock 27"
    );
}

/// F5 regression: Windows has no `SO_REUSEPORT`; forwarding POSIX optname 15 raw makes
/// Winsock reject it with `WSAENOPROTOOPT`. The shim maps `SO_REUSEPORT`(15) → Winsock
/// `SO_REUSEADDR`(4), the substitution php-src uses on Windows for address-reuse.
#[test]
fn test_setsockopt_maps_reuseport_to_reuseaddr() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_setsockopt(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_setsockopt");
    assert!(
        section.contains("cmp rdx, 15") && section.contains("je .Lsetsockopt_is_reuseport"),
        "must recognize POSIX SO_REUSEPORT (optname 15)"
    );
    // The SO_REUSEPORT target block sets rdx to Winsock SO_REUSEADDR (4). Assert against
    // the instruction immediately under the reuseport label — the bare `mov rdx, 4` is
    // shared with the separate SO_REUSEADDR(2) target, so the label anchor is what scopes
    // this to the reuseport case (Rust `//` comments never reach the emitted asm).
    assert!(
        section.contains(".Lsetsockopt_is_reuseport:\n    mov rdx, 4\n"),
        "SO_REUSEPORT(15) must map to Winsock SO_REUSEADDR(4)"
    );
}

/// Returns the assembly slice for the shim labeled `label`, from its `.globl`
/// declaration up to (but excluding) the next `.globl` declaration — used to scope
/// `cdqe` presence/absence assertions to a single shim's body instead of the whole
/// (possibly multi-shim) emitter output.
fn shim_section<'a>(asm: &'a str, label: &str) -> &'a str {
    let marker = format!(".globl {}\n", label);
    let start = asm
        .find(&marker)
        .unwrap_or_else(|| panic!("shim {} not found in emitted asm", label));
    let after = &asm[start + marker.len()..];
    match after.find(".globl ") {
        Some(next) => &after[..next],
        None => after,
    }
}

/// Sign-extension (Classe 3) regression suite: every `__rt_sys_*` shim that returns a
/// 32-bit int status (0/`SOCKET_ERROR`=-1) and has a sign-testing consumer must `cdqe`
/// before returning, so a -1 failure reads as a 64-bit negative instead of
/// `0x00000000_FFFFFFFF` (positive, a missed failure). `socket`/`accept` return a
/// 64-bit `SOCKET` handle and must NOT `cdqe` (it would corrupt a handle with bit 31
/// set). See `emit_shim_socket_shims`'s docblock for the full rationale.
mod sign_extension {
    use super::*;

    /// Verifies that all six int-status socket shims — `bind`, `listen`, `connect`,
    /// `shutdown`, `getsockname`, `getpeername` — are emitted as dedicated blocks AFTER
    /// the shared `shims` loop (which now contains ONLY `socket`/`accept`), each ending
    /// with `cdqe` before `ret`, matching the connect gabarit this class of fix is
    /// copied from. `accept` is the last shared-loop entry, so every dedicated block
    /// must appear strictly after it — proving they are no longer loop-driven.
    #[test]
    fn test_int_status_socket_shims_are_dedicated_cdqe_blocks() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();

        let accept_pos = asm
            .find(".globl __rt_sys_accept\n")
            .expect("accept shim missing");
        for label in [
            "__rt_sys_bind",
            "__rt_sys_listen",
            "__rt_sys_connect",
            "__rt_sys_shutdown",
            "__rt_sys_getsockname",
            "__rt_sys_getpeername",
        ] {
            let pos = asm
                .find(&format!(".globl {}\n", label))
                .unwrap_or_else(|| panic!("{} shim missing", label));
            assert!(
                pos > accept_pos,
                "{} must be emitted as a dedicated block after the shared loop (found before accept)",
                label
            );
            let section = shim_section(&asm, label);
            assert!(
                section.contains("cdqe"),
                "{} must sign-extend its Winsock int return with cdqe",
                label
            );
        }
    }

    /// Verifies the shared `shims` loop is now reduced to ONLY `socket`/`accept` — the
    /// two 64-bit SOCKET-handle returns that must NOT be sign-extended (`cdqe` would
    /// corrupt a handle with bit 31 set). Every other former loop entry
    /// (shutdown/getsockname/getpeername) was extracted into a dedicated cdqe block.
    #[test]
    fn test_socket_and_accept_have_no_cdqe() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();
        for label in ["__rt_sys_socket", "__rt_sys_accept"] {
            let section = shim_section(&asm, label);
            assert!(
                !section.contains("cdqe"),
                "{} returns a 64-bit SOCKET handle and must NOT cdqe",
                label
            );
        }
    }

    /// Verifies the three int-status shims extracted out of the shared loop in the
    /// correction loop — `shutdown`, `getsockname`, `getpeername` — each sign-extend
    /// their Winsock int return. These had ZERO cdqe coverage before this test:
    /// shutdown's consumer is stream_socket_shutdown.rs:47 (`test rax,rax; js`),
    /// getsockname/getpeername share stream_socket_get_name.rs:310 (`cmp rax,0; jl`).
    #[test]
    fn test_shutdown_getsockname_getpeername_sign_extend() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();
        for label in [
            "__rt_sys_shutdown",
            "__rt_sys_getsockname",
            "__rt_sys_getpeername",
        ] {
            let section = shim_section(&asm, label);
            assert!(section.contains("cdqe"), "{} must cdqe before returning", label);
        }
    }

    /// Verifies sendmsg/recvmsg translate Linux iovecs to Win64 WSABUFs and
    /// route synchronous scatter/gather I/O through Winsock.
    #[test]
    fn test_sendmsg_recvmsg_use_winsock_scatter_gather() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_sendmsg(&mut emitter);
        emit_shim_recvmsg(&mut emitter);
        let asm = emitter.output();
        for (label, native_call) in [
            ("__rt_sys_sendmsg", "call WSASend"),
            ("__rt_sys_recvmsg", "call WSARecv"),
        ] {
            let section = shim_section(&asm, label);
            assert!(section.contains(native_call), "{label} must use {native_call}");
            assert!(section.contains("QWORD PTR [r10 + 16]"));
            assert!(section.contains("QWORD PTR [rsi + 24]"));
            assert!(section.contains("shl r9, 4"));
            assert!(section.contains("QWORD PTR [r10 + r9 + 8]"));
            assert!(section.contains("DWORD PTR [rax + r9], r11d"));
            assert!(section.contains("QWORD PTR [rax + r9 + 8], r11"));
            assert!(section.contains("call WSAGetLastError"));
            assert!(section.contains("call __rt_win32_errno_from_code"));
            assert!(section.contains("call __rt_heap_free"));
        }
        let recv = shim_section(&asm, "__rt_sys_recvmsg");
        assert!(recv.contains("DWORD PTR [r10 + 48], eax"));
    }

    /// Verifies `sendto`/`recvfrom` sign-extend their Winsock byte-count-or-error
    /// return (consumers at stream_socket_sendto.rs:415 / stream_socket_recvfrom.rs:204
    /// sign-test with `cmp rax,0; jl`).
    #[test]
    fn test_sendto_recvfrom_sign_extend() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_socket_shims(&mut emitter);
        let asm = emitter.output();
        for label in ["__rt_sys_sendto", "__rt_sys_recvfrom"] {
            let section = shim_section(&asm, label);
            assert!(section.contains("cdqe"), "{} must cdqe before returning", label);
        }
    }

    /// Verifies `setsockopt` sign-extends its Winsock int-status return (consumer
    /// stream_set_timeout.rs:75 sign-tests with `cmp rax,0; jl`).
    #[test]
    fn test_setsockopt_sign_extends() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_setsockopt(&mut emitter);
        let asm = emitter.output();
        let section = shim_section(&asm, "__rt_sys_setsockopt");
        assert!(section.contains("cdqe"), "setsockopt must cdqe before returning");
    }

    /// Verifies latent `getsockopt` still reports a correctly signed status and errno.
    #[test]
    fn test_getsockopt_sign_extends_and_captures_errno() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_getsockopt(&mut emitter);
        let asm = emitter.output();
        let section = shim_section(&asm, "__rt_sys_getsockopt");
        assert!(section.contains("cdqe"));
        assert!(section.contains("call __rt_wsa_capture_errno"));
    }

    /// Verifies `_dup`/`_dup2` sign-extend their msvcrt int-status return (-1 on
    /// failure).
    #[test]
    fn test_dup_dup2_sign_extend() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_dup_shims(&mut emitter);
        let asm = emitter.output();
        for label in ["__rt_sys_dup", "__rt_sys_dup2"] {
            let section = shim_section(&asm, label);
            assert!(section.contains("cdqe"), "{} must cdqe before returning", label);
        }
    }

    /// Verifies `ioctl` (→ ioctlsocket) sign-extends its int-status return; reached
    /// from `stream_set_blocking()` via the fcntl F_SETFL/FIONBIO delegation, which
    /// sign-tests with `test rax,rax; js` at stream_set_blocking.rs:94/114.
    #[test]
    fn test_ioctl_shim_sign_extends() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_ioctl(&mut emitter);
        let asm = emitter.output();
        let section = shim_section(&asm, "__rt_sys_ioctl");
        assert!(section.contains("cdqe"), "ioctl shim must cdqe before returning");
    }

    /// Verifies `lseek` uses the 64-bit `SetFilePointerEx` contract and maps
    /// native failures instead of truncating offsets through a DWORD return.
    #[test]
    fn test_lseek_shim_preserves_full_width_positions() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_lseek(&mut emitter);
        let asm = emitter.output();
        let section = shim_section(&asm, "__rt_sys_lseek");
        assert!(section.contains("call SetFilePointerEx"));
        assert!(section.contains("lea r8, [rsp + 48]"));
        assert!(section.contains("mov rax, QWORD PTR [rsp + 48]"));
        assert!(section.contains("call __rt_win32_errno_from_code"));
        assert!(!section.contains("call SetFilePointer\n"));
    }

    /// Verifies `pselect6` tests Winsock's 32-bit SOCKET_ERROR sentinel in EAX
    /// and widens the result before saving it for the shared return path.
    #[test]
    fn test_pselect6_success_return_sign_extends() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_shim_pselect6(&mut emitter);
        let asm = emitter.output();
        let select_section = shim_section(&asm, "__rt_sys_pselect6");
        assert!(select_section.contains("call select\n    movsxd rax, eax"));
        assert!(select_section.contains("cmp eax, -1"));
        assert!(!select_section.contains("cmp rax, -1"));
        let reload_pos = asm
            .find("mov rax, QWORD PTR [rsp + 72]")
            .expect("pselect6 must reload the saved select result from [rsp+72]");
        let cdqe_pos = asm
            .find("cdqe")
            .expect("pselect6 success return must contain cdqe");
        let ret_pos = asm[cdqe_pos..]
            .find("ret")
            .map(|p| p + cdqe_pos)
            .expect("no ret found after cdqe");
        assert!(
            reload_pos < cdqe_pos && cdqe_pos < ret_pos,
            "cdqe must sit between the [rsp+72] reload ({}) and the following ret ({}), \
             found at {}",
            reload_pos,
            ret_pos,
            cdqe_pos
        );
    }
}

/// Verifies that flock supplies a real OVERLAPPED to both whole-file lock APIs.
#[test]
fn test_flock_stack_alignment_for_6_args() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("flock"));
    let section = shim_section(&asm, "flock");
    assert!(section.contains("sub rsp, 104"));
    assert!(section.contains("movdqu XMMWORD PTR [rsp + 48], xmm0"));
    assert!(section.contains("mov QWORD PTR [rsp + 40], rax"));
    assert!(section.contains("test QWORD PTR [rsp + 88], 8"));
    assert!(section.contains("call __rt_win32_errno_from_code"));
}

/// Verifies WriteFile shim uses the MSx64-correct 5th-arg layout: lpOverlapped=NULL
/// at [rsp+32] (arg5) and the &bytesWritten output pointer at [rsp+40], never
/// colliding on the arg5 slot.
#[test]
fn test_write_shim_overlapped_and_output_offsets() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_write(&mut emitter);
    let asm = emitter.output();
    assert!(
        asm.contains("mov QWORD PTR [rsp + 32], 0"),
        "lpOverlapped NULL must be at the arg5 slot [rsp+32]"
    );
    assert!(
        asm.contains("lea r9, [rsp + 40]"),
        "&bytesWritten (arg4) must point at [rsp+40], off the arg5 slot"
    );
    assert!(
        asm.contains("mov eax, DWORD PTR [rsp + 40]"),
        "bytesWritten must be read back as a 4-byte DWORD from [rsp+40]"
    );
    assert!(
        !asm.contains("lea r9, [rsp + 32]"),
        "output pointer must not alias the arg5 (lpOverlapped) slot"
    );
    // `len` must be spilled to the stack and reloaded across the intervening
    // `call __rt_fd_to_handle` — r8 is volatile in MSx64 and may be clobbered.
    assert!(
        asm.contains("mov QWORD PTR [rsp + 48], rdx"),
        "len must be spilled to [rsp+48] before the handle-conversion call"
    );
    assert!(
        asm.contains("mov r8, QWORD PTR [rsp + 48]"),
        "len must be reloaded from [rsp+48] into r8 after the handle-conversion call"
    );
    assert!(
        !asm.contains("mov r8, rdx"),
        "len must not be parked in volatile r8 across the call (regression guard)"
    );
    assert!(asm.contains("test eax, eax"));
    assert!(asm.contains("jz .Lsys_write_file_fail"));
    assert!(asm.contains("call GetLastError"));
    assert!(asm.contains("call __rt_win32_errno_from_code"));
    assert!(asm.contains("mov DWORD PTR [rip + __rt_errno], eax"));
    assert!(asm.contains("mov rax, -1"));
}

/// Verifies ReadFile shim uses the MSx64-correct 5th-arg layout: lpOverlapped=NULL
/// at [rsp+32] (arg5), &bytesRead at [rsp+48], and the preserved length at [rsp+72].
#[test]
fn test_read_shim_overlapped_and_output_offsets() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_read(&mut emitter);
    let asm = emitter.output();
    assert!(
        asm.contains("mov QWORD PTR [rsp + 32], 0"),
        "lpOverlapped NULL must be at the arg5 slot [rsp+32]"
    );
    assert!(
        asm.contains("lea r9, [rsp + 48]"),
        "&bytesRead (arg4) must point at [rsp+48], off the arg5 slot"
    );
    assert!(
        asm.contains("mov eax, DWORD PTR [rsp + 48]"),
        "bytesRead must be read back as a 4-byte DWORD from [rsp+48]"
    );
    assert!(
        !asm.contains("lea r9, [rsp + 32]"),
        "output pointer must not alias the arg5 (lpOverlapped) slot"
    );
    // `len` must be spilled to the stack and reloaded across the intervening
    // `call __rt_fd_to_handle` — r8 is volatile in MSx64 and may be clobbered.
    assert!(
        asm.contains("mov QWORD PTR [rsp + 72], rdx"),
        "len must be spilled to [rsp+72] before the handle-conversion call"
    );
    assert!(
        asm.contains("mov r8, QWORD PTR [rsp + 72]"),
        "len must be reloaded from [rsp+72] into r8 after the handle-conversion call"
    );
    assert!(
        !asm.contains("mov r8, rdx"),
        "len must not be parked in volatile r8 across the call (regression guard)"
    );
}

/// Verifies the lseek shim preserves both full-width distance and whence across
/// the intervening handle conversion before loading the MS x64 arguments.
#[test]
fn test_lseek_shim_spills_distance_and_whence_across_call() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_lseek(&mut emitter);
    let asm = emitter.output();
    assert!(
        asm.contains("mov QWORD PTR [rsp + 32], rsi"),
        "distance must be spilled to [rsp+32] before the handle-conversion call"
    );
    assert!(
        asm.contains("mov QWORD PTR [rsp + 40], rdx"),
        "whence must be spilled to [rsp+40] before the handle-conversion call"
    );
    assert!(
        asm.contains("mov rdx, QWORD PTR [rsp + 32]"),
        "distance must be reloaded as the full-width LARGE_INTEGER argument"
    );
    assert!(
        asm.contains("mov r9d, DWORD PTR [rsp + 40]"),
        "whence must be reloaded into the fourth MS x64 argument"
    );
}

/// Verifies exact readlink spills the reparse buffer, handle, and byte result across calls.
#[test]
fn test_readlink_shim_spills_handle_and_length_across_calls() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "readlink");
    assert!(section.contains("mov QWORD PTR [rsp + 72], rax"));
    assert!(section.contains("mov QWORD PTR [rsp + 80], rax"));
    assert!(section.contains("mov rcx, QWORD PTR [rsp + 80]"));
    assert!(section.contains("mov QWORD PTR [rsp + 136], rax"));
    assert!(
        !asm.contains("mov rcx, r10"),
        "readlink handle must not survive a call in volatile r10 (regression guard)"
    );
}

/// Verifies the remaining filesystem path shims use strict wide APIs, normalize extended
/// drive/UNC prefixes on returned paths, and balance owned conversions with errno paths.
#[test]
fn test_final_path_family_uses_unicode_and_extended_prefix_normalization() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    for (label, api) in [
        ("symlink", "CreateSymbolicLinkW"),
        ("link", "CreateHardLinkW"),
        ("readlink", "DeviceIoControl"),
        ("utimensat", "CreateFileW"),
        ("realpath", "GetFinalPathNameByHandleW"),
    ] {
        let section = shim_section(&asm, label);
        assert!(section.contains("call __rt_win_utf8_to_utf16"), "{label}");
        assert!(section.contains(&format!("call {api}")), "{label}");
        assert!(section.contains("__rt_errno"), "{label}");
        assert!(section.contains("call __rt_heap_free"), "{label}");
    }
    let readlink = shim_section(&asm, "readlink");
    assert!(readlink.contains("0x2200000"));
    assert!(readlink.contains("0x900A8"));
    assert!(readlink.contains("0xA000000C"));
    assert!(readlink.contains("0xA0000003"));
    assert!(readlink.contains("WORD PTR [r10 + 14]"));
    assert!(readlink.contains("WORD PTR [r10 + 10]"));
    assert!(readlink.contains("cmp DWORD PTR [rsp + 104], 20"));
    assert!(readlink.contains("cmp DWORD PTR [rsp + 104], 16"));
    assert!(readlink.contains("lea r8, [rdx + rax]"));
    assert!(readlink.contains("cmova rax, rcx"));
    assert!(readlink.contains("rep movsb"));
    assert!(readlink.contains("0x005c003f005c005c"));
    assert!(readlink.contains("0x005c003f003f005c"));
    assert!(readlink.contains("0x005c0043004e0055"));
    assert!(readlink.contains("call __rt_win_utf16_to_utf8"));
    let realpath = shim_section(&asm, "realpath");
    assert!(realpath.contains("call CreateFileW"));
    assert!(realpath.contains("mov r8d, 7"));
    assert!(realpath.contains("mov QWORD PTR [rsp + 32], 3"));
    assert!(realpath.contains("mov QWORD PTR [rsp + 40], 0x2000080"));
    assert!(realpath.contains("mov QWORD PTR [rsp + 48], 0"));
    assert!(realpath.contains("call GetFinalPathNameByHandleW"));
    assert!(realpath.contains("call CloseHandle"));
    assert!(!realpath.contains("call GetFullPathNameW"));
    assert!(realpath.contains("0x005c003f005c005c"));
    assert!(realpath.contains("0x005c0043004e0055"));
    assert!(realpath.contains("call __rt_win_utf16_to_utf8"));
    let lstat_probe = shim_section(&asm, "__rt_lstat_is_symlink");
    assert!(lstat_probe.contains("mov QWORD PTR [rsp + 40], 0x2200000"));
    assert!(lstat_probe.contains("call DeviceIoControl"));
    assert!(lstat_probe.contains("cmp DWORD PTR [rax], 0xA000000C"));
    for ansi in [
        "CreateSymbolicLinkA",
        "CreateHardLinkA",
        "GetFinalPathNameByHandleA",
        "GetFullPathNameA",
    ] {
        assert!(!asm.contains(&format!("call {ansi}")), "ANSI path call remains: {ansi}");
    }
}

/// Verifies `lstat` preserves php-src's tri-state reparse semantics and link modes.
#[test]
fn test_lstat_reparse_ioctl_failure_is_not_silently_followed() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let probe = shim_section(&asm, "__rt_lstat_is_symlink");
    assert!(probe.contains("call GetFileAttributesW"));
    assert!(probe.contains("test eax, 0x400"));
    assert!(probe.contains("jz .Llstat_probe_cleanup_wide_false"));
    assert!(probe.contains("call DeviceIoControl"));
    assert!(probe.contains("call GetLastError"));
    assert!(probe.contains(".Llstat_probe_cleanup_error:"));
    assert!(probe.contains("mov rax, -1"));
    assert!(probe.contains("call __rt_win32_errno_from_code"));

    let lstat = shim_section(&asm, "lstat");
    assert!(lstat.contains("js .Llstat_probe_fail"));
    assert!(lstat.contains(".Llstat_probe_fail:"));
    assert!(lstat.contains("mov eax, 0xA1B6"));
    assert!(lstat.contains("mov eax, 0xA124"));
    assert!(lstat.contains("jz .Llstat_followed_fallback"));
    assert!(
        lstat.contains(".Llstat_followed_fallback:\n    mov rdi, QWORD PTR [rsp + 144]\n    mov rsi, QWORD PTR [rsp + 64]\n    call __rt_sys_stat"),
        "ordinary files and directories must reach the followed stat fallback"
    );
}

/// Verifies uname retrieves the computer name as UTF-16 and converts it into the fixed
/// UTF-8 nodename field without retaining the ANSI hostname API.
#[test]
fn test_uname_uses_unicode_computer_name_layout() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_uname(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_uname");
    assert!(section.contains("sub rsp, 232"));
    assert!(section.contains("call GetComputerNameW"));
    assert!(!section.contains("call GetComputerNameA"));
    assert!(section.contains("lea rdi, [rsp + 40]"));
    assert!(section.contains("call __rt_win_utf16_to_utf8"));
    assert!(section.contains("lea rcx, [rsp + 168]"));
}

/// Verifies that `emit_win32_shims` unconditionally emits the
/// `__rt_unsupported_syscall` diagnostic helper (the target of the transform's
/// unmapped-syscall path), so it is always present for the transform to call.
#[test]
fn test_unsupported_syscall_helper_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(
        asm.contains(".globl __rt_unsupported_syscall\n"),
        "emit_win32_shims must emit the __rt_unsupported_syscall diagnostic helper"
    );
    assert!(
        asm.contains("call ExitProcess"),
        "the unsupported-syscall helper must terminate via ExitProcess"
    );
}

/// Verifies that Winsock init/cleanup shims are emitted with the right Win32 calls.
#[test]
fn test_winsock_init_and_cleanup_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_winsock_init\n"), "winsock init shim missing");
    assert!(asm.contains("call WSAStartup"), "winsock init must call WSAStartup");
    assert!(asm.contains("0x0202"), "winsock init must load MAKEWORD(2,2)");
    assert!(asm.contains(".globl __rt_winsock_cleanup\n"), "winsock cleanup shim missing");
    assert!(asm.contains("call WSACleanup"), "winsock cleanup must call WSACleanup");
}

/// Verifies the disabled AF_UNIX registry owns no state and main only balances Winsock.
#[test]
fn test_main_wrapper_has_no_unix_loopback_teardown() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_unix_loopback_registry(&mut emitter);
    emit_main_wrapper(&mut emitter);
    let asm = emitter.output();
    let close_all = asm
        .split(".globl __rt_win_unix_close_all\n")
        .nth(1)
        .expect("close-all helper missing");
    assert!(!close_all.contains("call closesocket"));
    assert!(!close_all.contains("_win_unix_endpoint_records"));
    let main = asm
        .split(".globl main\n")
        .nth(1)
        .expect("Windows main wrapper missing");
    assert!(!main.contains("call __rt_win_unix_close_all"));
    assert!(main.contains("call __rt_winsock_cleanup"));
}

/// Verifies that `__rt_sys_exit` calls `__rt_winsock_cleanup` before `ExitProcess`
/// so Winsock resources are released on process termination.
#[test]
fn test_exit_calls_winsock_cleanup_before_exit_process() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_exit(&mut emitter);
    let asm = emitter.output();
    let cleanup_pos = asm.find("call __rt_winsock_cleanup");
    let exit_pos = asm.find("call ExitProcess");
    assert!(cleanup_pos.is_some(), "exit shim must call winsock cleanup");
    assert!(exit_pos.is_some(), "exit shim must call ExitProcess");
    assert!(cleanup_pos < exit_pos, "winsock cleanup must run before ExitProcess");
}

/// Regression test for the Windows exit-crash class: `emit_shim_exit` starts with
/// `and rsp, -16` (forced alignment, since the shim can be reached at any
/// alignment), so the following `sub rsp, <N>` MUST have `N ≡ 0 mod 16` to keep
/// rsp ≡ 0 at the `call __rt_winsock_cleanup` and `call ExitProcess` sites.
/// Using `N ≡ 8 mod 16` (e.g. 40) would leave rsp ≡ 8 at both call sites — the
/// exact SSE #GP crash class that the original `and rsp, -16` fix was added for
/// (Wine's process-exit path reads aligned SSE registers). This test parses the
/// emitted asm, finds the `and rsp, -16` line, reads the next `sub rsp, <N>`,
/// and asserts `N % 16 == 0`, locking the invariant so it can't regress.
#[test]
fn test_exit_shim_stack_alignment() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_exit(&mut emitter);
    let asm = emitter.output();
    let lines: Vec<&str> = asm.lines().collect();
    // Find the `and rsp, -16` line, then the next `sub rsp, <N>` line.
    let and_pos = lines
        .iter()
        .position(|l| l.trim().starts_with("and rsp, -16"))
        .expect("exit shim must start with `and rsp, -16`");
    let sub_line = lines[and_pos + 1..]
        .iter()
        .find(|l| l.trim().starts_with("sub rsp, "))
        .expect("exit shim must have a `sub rsp, <N>` after `and rsp, -16`");
    let n_str = sub_line
        .trim()
        .strip_prefix("sub rsp, ")
        .and_then(|rest| rest.split_whitespace().next())
        .expect("`sub rsp, <N>` must have a numeric operand");
    let n: u64 = n_str
        .parse()
        .unwrap_or_else(|_| panic!("`sub rsp, <N>` operand `{}` is not an integer", n_str));
    assert_eq!(
        n % 16,
        0,
        "exit shim: after `and rsp, -16` (forces rsp ≡ 0), `sub rsp, {}` must be ≡ 0 mod 16 \
         so rsp stays ≡ 0 at the Win32 call sites; got N ≡ {} mod 16 (misaligned → SSE #GP)",
        n,
        n % 16
    );
    // Both Win32 calls must appear after the aligned prologue.
    assert!(
        asm.contains("call __rt_winsock_cleanup"),
        "exit shim must call __rt_winsock_cleanup"
    );
    assert!(
        asm.contains("call ExitProcess"),
        "exit shim must call ExitProcess"
    );
}

/// Verifies `access` implements PHP's Windows existence, readonly, and executable checks.
#[test]
fn test_access_shim_uses_get_file_attributes() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_access(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_access\n"));
    assert!(asm.contains("call __rt_win_utf8_to_utf16"));
    assert!(asm.contains("call GetFileAttributesW"));
    assert!(asm.contains("0xFFFFFFFF"), "access must check INVALID_FILE_ATTRIBUTES");
    assert!(asm.contains("test edx, 2"), "W_OK must inspect readonly attributes");
    assert!(
        asm.contains(
            "test eax, 1\n    jnz .Laccess_eacces\n    jmp .Laccess_success\n.Laccess_check_execute:"
        ),
        "a writable non-executable file must satisfy W_OK without falling through to X_OK"
    );
    assert!(asm.contains("mov DWORD PTR [rip + __rt_errno], 13"));
    assert!(asm.contains("test edx, 1"), "X_OK must use Windows executable detection");
    assert!(asm.contains("call GetBinaryTypeW"));
    assert!(asm.contains(".Laccess_fail"));
}

/// Verifies `stat` mirrors php-src's Windows mode, hard-link, and special-handle
/// synthesis rather than collapsing every handle to a writable regular file.
#[test]
fn test_stat_shim_preserves_php_windows_modes_and_metadata() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_stat(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_stat");
    assert!(section.contains("call GetBinaryTypeW"));
    assert!(section.contains("test edx, 0x10"));
    assert!(section.contains("mov eax, 0x41FF"));
    assert!(section.contains("or DWORD PTR [rsp + 156], 0x49"));
    assert!(section.contains("and eax, 0xFF6D"));
    assert!(section.contains("mov eax, DWORD PTR [rsp + 128]"));
    assert!(section.contains("mov edx, DWORD PTR [rsp + 88]"));
    assert!(section.contains(".Lstat_handle_mode_readonly"));
    assert!(section.contains("mov rax, QWORD PTR [rsp + 100]"));
    assert!(section.contains("mov rax, QWORD PTR [rsp + 108]"));
    assert!(section.contains("mov rax, QWORD PTR [rsp + 92]"));
    assert!(section.contains("call GetFileType"));
    assert!(section.contains(".Lstat_pipe_mode"));
    assert!(section.contains("mov eax, 0x11B6"));
    assert!(section.contains("mov eax, 0x21B6"));
    assert!(section.contains("sub rsp, 168"));
    assert!(section.contains("add rsp, 168"));
}

/// Verifies `access(X_OK | W_OK)` branches to the executable probe before
/// querying readonly attributes, matching php-src's X_OK short-circuit.
#[test]
fn test_access_shim_short_circuits_x_ok_before_w_ok() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_access(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_access");
    let execute_branch = section
        .find("jnz .Laccess_check_execute")
        .expect("X_OK branch must be emitted");
    let attributes_call = section
        .find("call GetFileAttributesW")
        .expect("non-X_OK access must still probe attributes");
    assert!(
        execute_branch < attributes_call,
        "X_OK must bypass readonly W_OK handling before GetFileAttributesW"
    );
    assert!(section.contains("call GetBinaryTypeW"));
    assert!(section.contains("mov edx, DWORD PTR [rsp + 40]"));
}

/// Verifies `fstat` emits full handle metadata and pipe/character fallbacks.
#[test]
fn test_fstat_uses_by_handle_information_and_file_type_fallbacks() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_fstat(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call __rt_fd_to_handle"));
    assert!(asm.contains("call GetFileInformationByHandle"));
    assert!(asm.contains("call GetFileType"));
    assert!(asm.contains("mov eax, 0x11B6"), "pipes must be S_IFIFO");
    assert!(asm.contains("mov eax, 0x21B6"), "console/device handles must be S_IFCHR");
    assert!(asm.contains("mov eax, DWORD PTR [rsp + 72]"), "native link count must be loaded");
    assert!(asm.contains("test edx, 1"), "readonly attributes must clear write bits");
}

/// Verifies that `__rt_sys_ftruncate` converts CRT descriptors and preserves position.
#[test]
fn test_ftruncate_shim_uses_set_file_pointer_ex_and_set_end_of_file() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_ftruncate(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_ftruncate\n"));
    assert!(asm.contains("call __rt_fd_to_handle"));
    assert_eq!(asm.matches("call SetFilePointerEx").count(), 3);
    assert!(asm.contains("call SetEndOfFile"));
    assert!(asm.contains("mov r9d, 1"), "the current position must be queried first");
    assert!(asm.contains("mov QWORD PTR [rsp + 40], rax"), "the converted HANDLE must be retained");
    assert!(asm.contains("mov rdx, QWORD PTR [rsp + 48]"), "the original position must be restored");
    assert!(asm.contains(".Lftruncate_fail"));
    assert!(asm.contains(".Lftruncate_bad_fd"));
    assert!(asm.contains(".Lftruncate_invalid_size"));
    assert!(asm.contains("call GetLastError"));
    assert!(asm.contains("call __rt_win32_errno_from_code"));
    assert!(asm.contains("mov DWORD PTR [rip + __rt_errno], eax"));
}

/// Verifies that the C-symbol stubs for `access`, `ftruncate`, and `umask` are
/// emitted so direct `call <name>` sites in the shared runtime resolve on Windows.
#[test]
fn test_c_symbol_stubs_for_access_ftruncate_umask() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl access\n"), "access C-symbol stub missing");
    assert!(asm.contains("call __rt_sys_access"));
    assert!(asm.contains(".globl ftruncate\n"), "ftruncate C-symbol stub missing");
    assert!(asm.contains("call __rt_sys_ftruncate"));
    assert!(asm.contains(".globl umask\n"), "umask C-symbol stub missing");
    let umask_section = shim_section(&asm, "umask");
    assert!(umask_section.contains("mov eax, DWORD PTR [rip + __rt_win_umask]"));
    assert!(umask_section.contains("mov DWORD PTR [rip + __rt_win_umask], edi"));
    assert!(!umask_section.contains("xor eax, eax"), "umask must preserve process-local state");
}

/// Verifies that WSAStartup, WSACleanup, SetFilePointerEx, and SetEndOfFile are
/// declared as Win32 imports so the MinGW linker resolves them against ws2_32/kernel32.
#[test]
fn test_new_win32_imports_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".extern WSAStartup"));
    assert!(asm.contains(".extern WSACleanup"));
    assert!(asm.contains(".extern SetFilePointerEx"));
    assert!(asm.contains(".extern SetEndOfFile"));
}

/// Verifies that the `sleep` and `usleep` C-symbol stubs are emitted (so direct
/// `call sleep`/`call usleep` sites from the shared `lower_sleep`/`lower_usleep`
/// lowering resolve on Windows) and that both delegate to `Sleep`.
#[test]
fn test_sleep_usleep_c_symbol_stubs_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl sleep\n"), "sleep C-symbol stub missing");
    assert!(asm.contains(".globl usleep\n"), "usleep C-symbol stub missing");
    // Both stubs convert to milliseconds and call Win32 Sleep.
    assert!(asm.contains("call Sleep"), "sleep/usleep must call Win32 Sleep");
    // sleep: seconds → ms via imul rcx, rdi, 1000.
    let sleep_section = asm.split(".globl sleep\n").nth(1).unwrap_or("");
    assert!(
        sleep_section.contains("imul rcx, rdi, 1000"),
        "sleep must convert seconds→ms with imul rcx, rdi, 1000"
    );
    // usleep: microseconds → ms via div by 1000.
    let usleep_section = asm.split(".globl usleep\n").nth(1).unwrap_or("");
    assert!(
        usleep_section.contains("div rcx"),
        "usleep must convert usec→ms with a div"
    );
}

/// Verifies that `__rt_sys_getrusage` is emitted, calls `GetProcessTimes`, uses
/// the current-process pseudo-handle (`mov rcx, -1`), and lays out the 5th
/// argument (lpUserTime) in the MSx64 stack-arg slot `[rsp + 32]`.
#[test]
fn test_getrusage_shim_uses_get_process_times() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_getrusage(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_getrusage\n"));
    assert!(asm.contains("call GetProcessTimes"));
    assert!(
        asm.contains("mov rcx, -1"),
        "getrusage must use the current-process pseudo-handle (HANDLE)-1"
    );
    // 5th arg (lpUserTime) goes in the MSx64 stack-arg slot at [rsp+32].
    assert!(
        asm.contains("[rsp + 32], rax"),
        "getrusage must pass lpUserTime via the [rsp+32] stack-arg slot"
    );
    // FILETIME→timeval conversion uses the 10_000_000 divisor (100ns units per second).
    assert!(
        asm.contains("mov ecx, 10000000"),
        "getrusage must divide FILETIME by 10_000_000 to get tv_sec"
    );
    // RUSAGE_SELF guard branches and the two terminal paths.
    assert!(asm.contains(".Lgetrusage_zero"));
    assert!(asm.contains(".Lgetrusage_fail"));
    assert!(asm.contains("rep stosq"), "getrusage must zero rusage fields with rep stosq");
    assert!(
        asm.contains("mov rcx, 14"),
        "getrusage success path must zero 14 qwords (ru_maxrss..ru_nivcsw, offsets 32..144)"
    );
}

/// Verifies that `Sleep` and `GetProcessTimes` are declared as Win32 imports so
/// the MinGW linker resolves them against kernel32.
#[test]
fn test_sleep_getprocesstimes_imports_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".extern Sleep"));
    assert!(asm.contains(".extern GetProcessTimes"));
}

/// Verifies that the `__rt_sys_getrusage` shim is registered in the full Win32
/// shim set emitted by `emit_win32_shims` (so the syscall-98 transform target
/// resolves at link time).
#[test]
fn test_getrusage_shim_registered_in_full_set() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_getrusage\n"));
}

/// Verifies that the `popen`, `pclose`, `fileno`, `fgetc`, and `system`
/// C-symbol stubs are emitted with their `.globl` labels and that each body
/// calls the corresponding msvcrt import (`_popen`, `_pclose`, `_fileno`,
/// `fgetc`, `system`) — never the libc-name self-recursion form.
#[test]
fn test_popen_pclose_fileno_fgetc_system_stubs_emitted() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    for (name, import) in [
        ("popen", "call _popen"),
        ("pclose", "call _pclose"),
        ("fileno", "call _fileno"),
        ("__rt_sys_fgetc", "call QWORD PTR [rip + __imp_fgetc]"),
        ("__rt_sys_system", "call QWORD PTR [rip + __imp_system]"),
    ] {
        assert!(
            asm.contains(&format!(".globl {}\n", name)),
            "{} C-symbol stub missing",
            name
        );
        let section = asm
            .split(&format!(".globl {}\n", name))
            .nth(1)
            .unwrap_or("");
        assert!(
            section.contains(import),
            "{} stub must call {} (msvcrt import)",
            name,
            import
        );
    }
}

/// Verifies that the msvcrt imports `_popen`, `_pclose`, `_fileno`, `fgetc`,
/// `system` and the ws2_32 `select` import are declared as `.extern` so the
/// MinGW linker resolves them against msvcrt/ws2_32.
#[test]
fn test_popen_msvcrt_and_select_imports_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".extern _popen"), "missing .extern _popen");
    assert!(asm.contains(".extern _pclose"), "missing .extern _pclose");
    assert!(asm.contains(".extern _fileno"), "missing .extern _fileno");
    assert!(asm.contains(".extern fgetc"), "missing .extern fgetc");
    assert!(asm.contains(".extern system"), "missing .extern system");
    assert!(asm.contains(".extern select"), "missing .extern select");
}

/// Verifies that the `__rt_sys_pselect6` shim calls ws2_32 `select` instead
/// of being the old `-1`/`ret` ENOSYS stub.
#[test]
fn test_pselect6_shim_calls_ws2_32_select() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_pselect6(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_sys_pselect6\n"));
    assert!(
        asm.contains("call select"),
        "pselect6 shim must call ws2_32 select"
    );
}

/// Verifies that the `__rt_sys_pselect6` shim's `sub rsp, <N>` frame size
/// satisfies `N % 16 == 8`, keeping rsp 16-byte aligned at the inner
/// `call select` (the shim is entered via `call` with rsp ≡ 8 mod 16).
#[test]
fn test_pselect6_shim_frame_stack_alignment() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_pselect6(&mut emitter);
    let asm = emitter.output();
    let section = asm
        .split(".globl __rt_sys_pselect6\n")
        .nth(1)
        .unwrap_or("");
    // First `sub rsp, <N>` after the label is the frame allocation.
    let sub_line = section
        .lines()
        .find(|l| l.trim_start().starts_with("sub rsp,"))
        .unwrap_or_else(|| panic!("no `sub rsp,` in pselect6 shim"));
    let n: i64 = sub_line
        .trim()
        .trim_start_matches("sub rsp,")
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("could not parse frame size from: {}", sub_line));
    assert_eq!(
        n % 16,
        8,
        "pselect6 frame size {} must satisfy N % 16 == 8",
        n
    );
}

/// Regression guard: with `RuntimeFeatures::none()`, `emit_win32_shims` must not
/// reference any zlib/bzip2/pcre2/iconv third-party symbol — those libraries are
/// linked only when the program actually uses them (`-lz -lbz2 -lpcre2-* -liconv`),
/// so an unconditional `call` to their symbols breaks the base MinGW link. A base
/// shim (`__rt_sys_write`) must still be present so gating did not eat the whole set.
#[test]
fn test_third_party_shims_gated_off_when_features_absent() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::none());
    let asm = emitter.output();
    for call in [
        "call compress2",
        "call BZ2_bzCompress",
        "call pcre2_regcomp",
        "call iconv_open",
    ] {
        assert!(!asm.contains(call), "features::none() must not emit `{}`", call);
    }
    for label in [
        "__rt_sys_compress2",
        "__rt_sys_BZ2_bzCompress",
        "__rt_sys_pcre2_regcomp",
        "__rt_sys_iconv_open",
    ] {
        assert!(
            !asm.contains(&format!(".globl {}\n", label)),
            "features::none() must not emit shim label `{}`",
            label
        );
    }
    assert!(
        asm.contains(".globl __rt_sys_write\n"),
        "base shims must remain emitted when third-party features are gated off"
    );
}

/// Verifies that sysinfo initializes `MEMORYSTATUSEX.dwLength` before calling
/// `GlobalMemoryStatusEx` (required or the call always fails with
/// `ERROR_INVALID_PARAMETER`), and that the BOOL result is honored: success
/// returns 0, failure returns -1 (POSIX `sysinfo(2)` convention) instead of
/// the previous unconditional success.
#[test]
fn test_sysinfo_sets_dwlength_and_honors_bool_result() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_sysinfo(&mut emitter);
    let asm = emitter.output();
    assert!(
        asm.contains("mov DWORD PTR [rcx], 64"),
        "dwLength must be initialised to sizeof(MEMORYSTATUSEX) before the call"
    );
    assert!(asm.contains("call GlobalMemoryStatusEx"));
    assert!(asm.contains("test eax, eax"), "must test the Win32 BOOL result");
    assert!(asm.contains(".Lsysinfo_ok"), "must branch to the success path");
    assert!(asm.contains("mov rax, -1"), "failure must translate to POSIX -1");
    assert!(
        !asm.contains("xor rax, rax"),
        "unconditional success return must be gone (now conditional on the BOOL)"
    );
}

/// Verifies that kill returns -1 (not silent 0) for both the OpenProcess-failed
/// SIGKILL path and any unsupported (non-SIGKILL) signal — Windows has no
/// `posix_kill` equivalent, and a loud failure is safer than reporting success
/// for a signal that was never delivered.
#[test]
fn test_kill_returns_failure_for_unsupported_signal_and_open_process_failure() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_kill(&mut emitter);
    let asm = emitter.output();
    // Locate the LABEL DEFINITIONS (`name:`), not the earlier jump-target
    // references (`jne .Lsys_kill_noop`) — those share the bare name and
    // would otherwise make `find` return the wrong (earlier) offset.
    let fail_label_pos = asm
        .find(".Lsys_kill_fail:\n")
        .expect("fail label definition missing");
    let noop_label_pos = asm
        .find(".Lsys_kill_noop:\n")
        .expect("noop label definition missing");
    assert!(fail_label_pos < noop_label_pos, "fail block must precede noop block");
    let fail_section = &asm[fail_label_pos..noop_label_pos];
    let noop_section = &asm[noop_label_pos..];
    assert!(
        fail_section.contains("mov rax, -1"),
        "OpenProcess-failed path must return -1, not silent success"
    );
    assert!(
        noop_section.contains("mov rax, -1"),
        "unsupported-signal path must return -1, not silent success"
    );
    assert!(
        !noop_section.contains("xor rax, rax"),
        "no bare success return may remain on the unsupported-signal path"
    );
}

/// Verifies the bare C `kill` symbol delegates to the same strict Win32 helper
/// instead of returning false success for unsupported signals or API failures.
#[test]
fn test_c_kill_symbol_delegates_to_strict_runtime_shim() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "kill");
    assert!(section.contains("call __rt_sys_kill"));
    assert!(!section.contains("call OpenProcess"));
    assert!(!section.contains("xor rax, rax"));
}

/// Verifies that clock_getres writes `tv_nsec = 1` (matching the "1ns
/// best-effort resolution" docblock) into the caller's `struct timespec`, and
/// NULL-guards the write since POSIX permits `res == NULL`.
#[test]
fn test_clock_getres_writes_resolution_and_null_guards() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_clock_getres(&mut emitter);
    let asm = emitter.output();
    assert!(
        asm.contains("test rsi, rsi"),
        "must NULL-guard the res pointer before writing"
    );
    assert!(
        asm.contains(".Lclock_getres_done"),
        "must skip the write when res is NULL"
    );
    assert!(
        asm.contains("mov QWORD PTR [rsi + 8], 1"),
        "tv_nsec must be written as 1 (1ns best-effort resolution)"
    );
}

/// Verifies `GetLastError` is declared as a Win32 import — required by
/// `__rt_sys_read`'s failure path (finding F3, the Windows errno layer).
#[test]
fn test_get_last_error_declared_for_errno_translation() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(
        asm.contains(".extern GetLastError"),
        "GetLastError must be declared for __rt_sys_read's failure path"
    );
}

/// Verifies the full Win32 shim emission wires in the errno translation
/// helper (called from `emit_shim_c_symbols`, not registered separately in
/// `emit_win32_shims`).
#[test]
fn test_full_win32_shim_emission_includes_errno_translate_helper() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    assert!(asm.contains(".globl __rt_win32_errno_from_code\n"));
}

/// Windows errno layer (finding F3): `__rt_win32_errno_from_code` translates a
/// Win32/WSA `GetLastError`/`WSAGetLastError` code into the POSIX errno value
/// the `fgets`/`fread` nonblocking-read check compares against 11 (EAGAIN) —
/// see `io/fgets.rs:271-274` and `io/fread.rs:227-230`.
mod errno_translation {
    use super::*;

    /// Verifies the three behaviorally-critical would-block codes all map to
    /// EAGAIN(11): this is what lets fgets/fread distinguish a nonblocking
    /// would-block miss from real EOF/error on Windows.
    #[test]
    fn test_win32_errno_translate_would_block_codes_map_to_eagain() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_errno_translate(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".globl __rt_win32_errno_from_code\n"));
        for code in ["10035", "232", "997"] {
            assert!(
                asm.contains(&format!("cmp ecx, {}", code)),
                "translate helper must compare against Win32/WSA code {}",
                code
            );
        }
        // All three would-block codes branch to the same EAGAIN(11) terminal.
        let eagain_pos = asm
            .find(".Lw32err_eagain:\n")
            .expect("EAGAIN terminal label missing");
        assert!(
            asm[eagain_pos..].contains("mov eax, 11"),
            "EAGAIN terminal must return 11"
        );
    }

    /// Verifies the common Win32/WSA -> POSIX errno mappings are present as
    /// distinct compare targets, that ERROR_HANDLE_EOF(38) maps to 0 (not an
    /// error), and that an unrecognized code falls through to EIO(5) rather
    /// than a garbage value.
    #[test]
    fn test_win32_errno_translate_common_codes_and_unknown_default() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_win32_errno_translate(&mut emitter);
        let asm = emitter.output();
        for code in ["2", "3", "5", "6", "109", "38", "10004", "10054", "10060"] {
            assert!(
                asm.contains(&format!("cmp ecx, {}", code)),
                "translate helper must compare against Win32/WSA code {}",
                code
            );
        }
        let eof_pos = asm
            .find(".Lw32err_eof:\n")
            .expect("EOF terminal label missing");
        assert!(
            asm[eof_pos..].contains("xor eax, eax"),
            "ERROR_HANDLE_EOF must translate to 0 (not an error)"
        );
        // The straight-line fallthrough before the first labeled terminal is
        // the "no compare matched" default path -> EIO(5).
        let first_label_pos = asm
            .find(".Lw32err_eagain:\n")
            .expect("first terminal label missing");
        let default_zone = &asm[..first_label_pos];
        assert!(
            default_zone.contains("mov eax, 5"),
            "unrecognized codes must default to EIO(5)"
        );
        assert!(
            default_zone.trim_end().ends_with("ret"),
            "the default EIO path must return immediately"
        );
    }
}

/// Verifies `__rt_sys_read` (the ReadFile shim) stores a translated errno on
/// failure and returns -1, but leaves errno untouched and still returns the
/// byte count on success — the fgets/fread EAGAIN check needs both halves of
/// this contract (`io/fgets.rs:271-274`, `io/fread.rs:227-230`).
#[test]
fn test_sys_read_stores_errno_on_readfile_failure_only() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_read(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_read");
    assert!(section.contains("call ReadFile"));
    assert!(section.contains("test eax, eax"), "must test the ReadFile BOOL result");
    assert!(
        section.contains("jz .Lsys_read_fail"),
        "must branch to the failure path when the BOOL result is 0"
    );
    let fail_pos = section.find(".Lsys_read_fail:\n").expect("failure label missing");
    let (success_zone, fail_zone) = section.split_at(fail_pos);
    let readfile_pos = success_zone
        .find(".Lsys_read_file:\n")
        .expect("ReadFile path label missing");
    let socket_pos = success_zone
        .find(".Lsys_read_socket:\n")
        .expect("socket path label missing");
    let readfile_success_zone = &success_zone[readfile_pos..socket_pos];
    assert!(
        !readfile_success_zone.contains("__rt_errno") && !readfile_success_zone.contains("GetLastError"),
        "the successful ReadFile path must not touch errno (POSIX does not clear errno on success)"
    );
    assert!(
        readfile_success_zone.contains("mov eax, DWORD PTR [rsp + 48]"),
        "success path must still return the ReadFile byte count"
    );
    assert!(
        fail_zone.contains("call GetLastError"),
        "failure path must fetch GetLastError()"
    );
    assert!(
        fail_zone.contains("call __rt_win32_errno_from_code"),
        "failure path must translate the Win32 code to a POSIX errno"
    );
    assert!(
        fail_zone.contains("mov DWORD PTR [rip + __rt_errno], eax"),
        "failure path must publish the translated errno"
    );
    assert!(
        fail_zone.contains("mov rax, -1"),
        "failure path must return -1 like POSIX read()"
    );
}

/// Verifies a cached `O_NONBLOCK` CRT pipe polls availability like php-src
/// before the synchronous ReadFile call, while a POSIX zero-length read
/// returns zero without observing readiness or publishing EAGAIN.
#[test]
fn test_sys_read_uses_peek_named_pipe_for_nonblocking_crt_pipes() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_read(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_read");
    assert!(section.contains("call __rt_win_fd_status_find"));
    assert!(section.contains("test QWORD PTR [rax + 16], 0x800"));
    assert!(section.contains("call PeekNamedPipe"));
    assert!(section.contains(".Lsys_read_nonblocking_pipe_probe:"));
    assert!(section.contains("inc QWORD PTR [rsp + 96]"));
    assert!(section.contains("cmp QWORD PTR [rsp + 96], 32000"));
    assert!(section.contains("call Sleep"));
    assert!(section.contains("jmp .Lsys_read_nonblocking_pipe_probe"));
    let ready_pos = section
        .find(".Lsys_read_nonblocking_pipe_ready:\n")
        .expect("nonblocking pipe ready label missing");
    let readfile_pos = section
        .find(".Lsys_read_file:\n")
        .expect("ReadFile path label missing");
    let ready_zone = &section[ready_pos..readfile_pos];
    assert!(ready_zone.contains("mov eax, DWORD PTR [rsp + 56]"));
    assert!(ready_zone.contains("cmova rdx, rax"));
    assert!(ready_zone.contains("mov QWORD PTR [rsp + 72], rdx"));
    let zero_pos = section
        .find(".Lsys_read_nonempty:\n")
        .expect("zero-length read fast path missing");
    let zero_path = &section[..zero_pos];
    assert!(zero_path.contains("test rdx, rdx"));
    assert!(zero_path.contains("xor eax, eax"));
    assert!(
        !zero_path.contains("PeekNamedPipe") && !zero_path.contains("ReadFile"),
        "zero-length reads must complete before any Win32 pipe operation"
    );
}

/// Verifies `__rt_sys_recvfrom` stores a translated errno on `SOCKET_ERROR`
/// and still returns -1 (sign-extended), while the success path is untouched
/// and still `cdqe`s the byte count (Class-3 sign-extension requirement).
#[test]
fn test_recvfrom_stores_errno_on_socket_error() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socket_shims(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_recvfrom");
    assert!(
        section.contains("cmp eax, -1"),
        "must test for SOCKET_ERROR after recvfrom"
    );
    assert!(section.contains("je .Lrecvfrom_fail"));
    let fail_pos = section.find(".Lrecvfrom_fail:\n").expect("failure label missing");
    let (success_zone, fail_zone) = section.split_at(fail_pos);
    assert!(
        success_zone.contains("cdqe"),
        "success path must still sign-extend the byte count"
    );
    assert!(fail_zone.contains("call __rt_wsa_capture_errno"));
}

/// Regression test for WF10b BUG A: `__rt_sys_snprintf_double` (used by
/// `__rt_sprintf`'s `%f`/`%e`/`%g`/`%E`/`%G` path, whose
/// `snprintf(buf, size, fmt, double)` call has NO leading integer
/// precision argument, unlike `__rt_ftoa`'s `snprintf(buf, size, "%.*e", p, x)`
/// shape handled by `__rt_sys_snprintf`) must stage buf/size/fmt into
/// rcx/rdx/r8 and duplicate the double into BOTH r9 (the MSx64 positional
/// argument-4 integer register, required for a variadic callee's `va_arg`
/// home-slot read) and xmm3 (the positional argument-4 float register) before
/// calling MinGW `snprintf` — NOT the `[rsp+32]` 5th-argument stack slot
/// `__rt_sys_snprintf` uses, since here the double is positional argument 4,
/// not 5.
#[test]
fn test_snprintf_double_shim_duplicates_double_into_r9_and_xmm3() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_snprintf_double(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_snprintf_double");
    assert!(section.contains("mov r8, rdx"), "fmt -> arg3 (r8)");
    assert!(section.contains("mov rdx, rsi"), "size -> arg2 (rdx)");
    assert!(section.contains("mov rcx, rdi"), "buf -> arg1 (rcx)");
    assert!(
        section.contains("movq r9, xmm0"),
        "the double must be duplicated into r9 (arg4 integer register)"
    );
    assert!(
        section.contains("movaps xmm3, xmm0"),
        "the double must be duplicated into xmm3 (arg4 float register)"
    );
    assert!(section.contains("call __mingw_snprintf"));
    assert!(
        !section.contains("[rsp + 32]"),
        "unlike __rt_sys_snprintf, the double here is positional arg4 (register-only), not a 5th stack arg"
    );
}

/// Verifies the precision-shaped snprintf bridge puts its integer variadic
/// argument in MSx64 position four and its double in the position-five stack
/// slot, rather than incorrectly duplicating the stack argument into XMM3/R9.
#[test]
fn test_snprintf_precision_shim_places_double_in_fifth_stack_slot() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_snprintf(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_snprintf");
    assert!(section.contains("mov r9, r10"), "precision -> arg4 (r9)");
    assert!(
        section.contains("movsd QWORD PTR [rsp + 32], xmm0"),
        "the positional arg5 double must occupy the stack slot above shadow space"
    );
    assert!(!section.contains("movaps xmm3, xmm0"));
    assert!(section.contains("call __mingw_snprintf"));
}

/// Verifies negative msvcrt `strtol` results are widened from Windows' 32-bit
/// `long` representation before 64-bit runtime callers inspect the sign.
#[test]
fn test_strtol_shim_sign_extends_windows_long() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_strtol(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_strtol");
    let call = section.find("call strtol").expect("missing strtol import call");
    let widen = section.find("cdqe").expect("missing signed-long widening");
    assert!(call < widen);
}

/// Verifies native filesystem shims import strict UTF conversion and wide Win32 APIs.
#[test]
fn test_unicode_filesystem_imports_are_declared() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_shims(&mut emitter, RuntimeFeatures::all());
    let asm = emitter.output();
    for symbol in [
        "MultiByteToWideChar",
        "WideCharToMultiByte",
        "FindFirstFileExW",
        "FindNextFileW",
        "CreateFileW",
        "GetFinalPathNameByHandleW",
        "GetFileAttributesExW",
        "GetCurrentDirectoryW",
        "GetDiskFreeSpaceExW",
        "GetTempPathW",
        "GetCommandLineW",
        "CommandLineToArgvW",
        "LocalFree",
        "SetFileAttributesW",
    ] {
        assert!(asm.contains(&format!(".extern {symbol}\n")), "missing import {symbol}");
    }
}

/// Verifies the reverse conversion stages all eight `WideCharToMultiByte` arguments.
#[test]
fn test_utf16_to_utf8_helper_uses_widechar_msx64_layout() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_encoding_helpers(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_win_utf16_to_utf8");
    assert!(section.contains("mov ecx, 65001"));
    assert!(section.contains("mov edx, 128"));
    assert!(section.contains("mov r8, rdi"));
    assert!(section.contains("mov r9d, -1"));
    assert!(section.contains("mov QWORD PTR [rsp + 32], rsi"));
    assert!(section.contains("mov QWORD PTR [rsp + 40], rdx"));
    assert!(section.contains("call WideCharToMultiByte"));
}

/// Verifies directory enumeration retains a wide pattern and converts each name to UTF-8.
#[test]
fn test_directory_shims_use_unicode_enumeration() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_opendir(&mut emitter);
    emit_shim_readdir(&mut emitter);
    emit_shim_rewinddir(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call __rt_win_utf8_to_utf16"));
    assert_eq!(asm.matches("call FindFirstFileExW").count(), 2);
    assert!(asm.contains("call FindNextFileW"));
    assert!(asm.contains("call __rt_win_utf16_to_utf8"));
    assert!(!asm.contains("call FindFirstFileA"));
    assert!(!asm.contains("call FindNextFileA"));
}

/// Verifies directory EOF remains distinct from native enumeration/conversion
/// failures and `closedir` reports a failed `FindClose` after releasing memory.
#[test]
fn test_directory_shims_publish_native_errors() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_readdir(&mut emitter);
    emit_shim_closedir(&mut emitter);
    let asm = emitter.output();
    let readdir = shim_section(&asm, "__rt_sys_readdir");
    assert!(readdir.contains("call GetLastError"));
    assert!(readdir.contains("cmp eax, 18"));
    assert!(readdir.contains(".Lreaddir_native_error:"));
    assert!(readdir.contains("call __rt_win32_errno_from_code"));
    let closedir = shim_section(&asm, "__rt_sys_closedir");
    assert!(closedir.contains("call FindClose"));
    assert!(closedir.contains("call GetLastError"));
    assert!(closedir.contains("call __rt_win32_errno_from_code"));
    assert!(closedir.contains("mov rax, -1"));
}

/// Verifies glob uses wide matching while fnmatch keeps PHP's POSIX flag semantics.
#[test]
fn test_glob_and_fnmatch_use_wide_windows_apis() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let glob = shim_section(&asm, "glob");
    assert!(glob.contains("call __rt_win_utf8_to_utf16"));
    assert!(glob.contains("call FindFirstFileExW"));
    assert!(glob.contains("call FindNextFileW"));
    assert!(glob.contains("call __rt_win_utf16_to_utf8"));
    assert!(!glob.contains("call FindFirstFileA"));
    let fnmatch = shim_section(&asm, "fnmatch");
    assert!(!fnmatch.contains("PathMatchSpec"));
    assert!(fnmatch.contains("test QWORD PTR [rsp], 1"));
    assert!(fnmatch.contains("test QWORD PTR [rsp], 2"));
    assert!(fnmatch.contains("test QWORD PTR [rsp + 8], 4"));
    assert!(fnmatch.contains("test QWORD PTR [rsp], 16"));
}

/// Verifies Windows `glob` grows its result vector without a fixed match ceiling
/// and completely releases partial ownership when growth cannot be represented.
#[test]
fn test_glob_result_vector_grows_dynamically_and_cleans_up_on_enomem() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    let glob = shim_section(&asm, "glob");
    assert!(glob.contains("mov QWORD PTR [rsp + 704], 16"));
    assert!(glob.contains("lea rax, [rcx + rcx]"));
    assert!(glob.contains(".Lglob_grow_copy:"));
    assert!(glob.contains("mov QWORD PTR [rsp + 696], rax"));
    assert!(!glob.contains("cmp rax, 1024"));
    assert!(!glob.contains("silently drop matches"));
    assert!(glob.contains("cmp rcx, 0x10000000"));
    assert!(glob.contains("shr r10, 32"));
    assert!(glob.contains(".Lglob_nospace_path_loop:"));
    assert!(glob.contains("call FindClose"));
    assert!(glob.contains("mov DWORD PTR [rip + __rt_errno], 12"));
    assert!(glob.contains("mov QWORD PTR [rcx + 8], 0"));
}

/// Verifies `mkstemp` atomically creates and attributes the generated Unicode path.
#[test]
fn test_mkstemp_uses_wide_atomic_creation() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_mkstemp(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("call __rt_win_utf8_to_utf16"));
    assert!(asm.contains("call CreateFileW"));
    assert!(asm.contains("call SetFileAttributesW"));
    assert!(asm.contains("call _open_osfhandle"));
    assert!(asm.contains("cmp eax, -1"));
    assert!(asm.contains("mov edx, 0x8002"));
    assert!(asm.contains("mov QWORD PTR [rsp + 32], 1"));
    assert!(!asm.contains("call CreateFileA"));
}

/// Verifies path-mutating shims preserve UTF-8 at the runtime edge and call only `W` APIs.
#[test]
fn test_unicode_path_mutation_shims_use_strict_wide_boundary() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_unlink(&mut emitter);
    emit_shim_chdir(&mut emitter);
    emit_shim_mkdir(&mut emitter);
    emit_shim_rmdir(&mut emitter);
    emit_shim_chmod(&mut emitter);
    emit_shim_access(&mut emitter);
    emit_shim_rename(&mut emitter);
    let asm = emitter.output();
    for (label, api) in [
        ("__rt_sys_unlink", "DeleteFileW"),
        ("__rt_sys_chdir", "SetCurrentDirectoryW"),
        ("__rt_sys_mkdir", "CreateDirectoryW"),
        ("__rt_sys_rmdir", "RemoveDirectoryW"),
        ("__rt_sys_chmod", "SetFileAttributesW"),
        ("__rt_sys_access", "GetFileAttributesW"),
        ("__rt_sys_rename", "MoveFileExW"),
    ] {
        let section = shim_section(&asm, label);
        assert!(section.contains("call __rt_win_utf8_to_utf16"), "{label} must preserve UTF-8/UTF-16 boundary");
        assert!(section.contains(&format!("call {api}")), "{label} must call {api}");
        assert!(section.contains("__rt_errno"), "{label} must publish conversion/native failure");
    }
    for label in [
        "__rt_sys_unlink",
        "__rt_sys_chdir",
        "__rt_sys_mkdir",
        "__rt_sys_rmdir",
    ] {
        let section = shim_section(&asm, label);
        assert!(
            section.contains("xor eax, eax") && section.contains("mov eax, -1"),
            "{label} must translate Win32 BOOL into POSIX 0/-1 status"
        );
        assert!(
            section.matches("cdqe").count() >= 2,
            "{label} must sign-extend native and UTF-8 conversion failure sentinels"
        );
    }
    for ansi in [
        "DeleteFileA",
        "SetCurrentDirectoryA",
        "CreateDirectoryA",
        "RemoveDirectoryA",
        "SetFileAttributesA",
        "GetFileAttributesA",
        "MoveFileExA",
    ] {
        assert!(!asm.contains(&format!("call {ansi}")), "ANSI path call remains: {ansi}");
    }
}

/// Verifies the strict conversion boundary does not normalize UNC or extended path prefixes.
#[test]
fn test_utf8_path_helper_delegates_unc_and_long_path_bytes_without_rewrite() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_encoding_helpers(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_win_utf8_to_utf16");
    assert!(section.contains("call MultiByteToWideChar"));
    assert!(!section.contains("GetFullPathName"));
    assert!(!section.contains("PathCch"));
    assert!(!section.contains("PathCanonicalize"));
}

/// Verifies `getcwd` dynamically queries a wide long path, converts it into the caller's
/// UTF-8 buffer, returns that buffer, and reports native/range/allocation failures.
#[test]
fn test_getcwd_uses_dynamic_wide_buffer_and_utf8_result() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_getcwd(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_getcwd");
    assert_eq!(section.matches("call GetCurrentDirectoryW").count(), 2);
    assert!(!section.contains("call GetCurrentDirectoryA"));
    assert!(section.contains("xor ecx, ecx"));
    assert!(section.contains("call __rt_heap_alloc"));
    assert!(section.contains("call __rt_win_utf16_to_utf8"));
    assert!(section.contains("mov rax, QWORD PTR [rsp + 32]"));
    assert!(section.contains("call __rt_heap_free"));
    assert!(section.contains("mov DWORD PTR [rip + __rt_errno], 34"));
    assert!(section.contains("mov DWORD PTR [rip + __rt_errno], 12"));
}

/// Verifies `statfs` converts Unicode/extended paths and preserves the Windows output
/// qwords separately from its owned path and error locals.
#[test]
fn test_statfs_uses_wide_disk_space_layout_and_errno() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_statfs(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_statfs");
    assert!(section.contains("sub rsp, 88"));
    assert!(section.contains("call __rt_win_utf8_to_utf16"));
    assert!(section.contains("lea rdx, [rsp + 32]"));
    assert!(section.contains("lea r8, [rsp + 40]"));
    assert!(section.contains("mov QWORD PTR [rsp + 48], rax"));
    assert!(section.contains("mov QWORD PTR [rsp + 64], rsi"));
    assert!(section.contains("mov rsi, QWORD PTR [rsp + 64]"));
    assert!(section.contains("call GetDiskFreeSpaceExW"));
    assert!(!section.contains("call GetDiskFreeSpaceExA"));
    assert!(section.contains("mov DWORD PTR [rsi + 8], 1"));
    assert!(section.contains("mov QWORD PTR [rsi + 16], rax"));
    assert!(section.contains("mov QWORD PTR [rsi + 32], rax"));
    assert!(section.contains("call __rt_heap_free"));
    assert!(section.contains("__rt_errno"));
}

/// Verifies the temp-directory helper queries arbitrary wide lengths, performs a strict
/// two-pass UTF-8 conversion, trims the trailing separator, and balances both allocations.
#[test]
fn test_temp_dir_uses_dynamic_wide_utf8_pipeline() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_sys_get_temp_dir(&mut emitter);
    let asm = emitter.output();
    let section = shim_section(&asm, "__rt_sys_get_temp_dir");
    assert_eq!(section.matches("call GetTempPathW").count(), 2);
    assert!(!section.contains("call GetTempPathA"));
    assert!(!section.contains("mov ecx, 260"));
    assert_eq!(section.matches("call __rt_win_utf16_to_utf8").count(), 2);
    assert_eq!(section.matches("call __rt_heap_alloc").count(), 2);
    assert!(section.matches("call __rt_heap_free").count() >= 4);
    assert!(section.contains("cmp BYTE PTR [r10 + rax - 1], 92"));
    assert!(section.contains("mov BYTE PTR [r10 + rax], 0"));
    assert!(section.contains("__rt_win32_last_error"));
    assert!(section.contains("__rt_errno"));
}

/// Verifies Win32 and Winsock native error states remain distinct from POSIX errno.
#[test]
fn test_native_error_slots_are_separate() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_c_symbols(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("__rt_errno:\n"));
    assert!(asm.contains("__rt_win32_last_error:\n"));
    assert!(asm.contains("__rt_wsa_last_error:\n"));
}

/// Verifies PHP-visible native error messages originate as Unicode system messages.
#[test]
fn test_win32_error_message_uses_format_message_w_and_utf8_output() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_error_message(&mut emitter);
    let asm = emitter.output();
    assert!(asm.contains("mov ecx, 0x1200"));
    assert!(asm.contains("call FormatMessageW"));
    assert!(asm.contains("call WideCharToMultiByte"));
    assert!(asm.contains("call __rt_win_utf16_to_utf8"));
    assert!(asm.contains("call __rt_heap_alloc"));
    assert!(asm.contains("call __rt_heap_free"));
}

/// Verifies failure-only capture keeps Win32 and Winsock native errors separate.
#[test]
fn test_native_errno_capture_helpers_translate_without_success_side_effects() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_native_errno_capture_helpers(&mut emitter);
    let asm = emitter.output();
    let win32 = shim_section(&asm, "__rt_win32_capture_errno");
    assert!(win32.contains("call GetLastError"));
    assert!(win32.contains("__rt_win32_last_error"));
    assert!(win32.contains("call __rt_win32_errno_from_code"));
    let wsa = shim_section(&asm, "__rt_wsa_capture_errno");
    assert!(wsa.contains("call WSAGetLastError"));
    assert!(wsa.contains("__rt_wsa_last_error"));
    assert!(wsa.contains("call __rt_win32_errno_from_code"));
}

/// Verifies all reachable core Winsock shims route failures through errno capture.
#[test]
fn test_reachable_socket_shims_capture_errno() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_socket_shims(&mut emitter);
    emit_shim_accept4(&mut emitter);
    emit_shim_setsockopt(&mut emitter);
    emit_shim_getsockopt(&mut emitter);
    let asm = emitter.output();
    for label in [
        "__rt_sys_socket",
        "__rt_sys_accept",
        "__rt_sys_bind",
        "__rt_sys_listen",
        "__rt_sys_connect",
        "__rt_sys_shutdown",
        "__rt_sys_getsockname",
        "__rt_sys_getpeername",
        "__rt_sys_closesocket",
        "__rt_sys_sendto",
        "__rt_sys_recvfrom",
        "__rt_sys_setsockopt",
        "__rt_sys_getsockopt",
    ] {
        assert!(
            shim_section(&asm, label).contains("call __rt_wsa_capture_errno"),
            "{label} must capture Winsock errno on failure"
        );
    }
    assert!(
        shim_section(&asm, "__rt_sys_accept4").contains("call __rt_sys_accept"),
        "accept4 must delegate to the errno-capturing accept shim"
    );
}

/// Verifies high-impact filesystem and connection failures map to POSIX errno values.
#[test]
fn test_extended_errno_mapping_includes_connection_and_path_failures() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_win32_errno_translate(&mut emitter);
    let asm = emitter.output();
    for (code, label, errno) in [
        ("10061", ".Lw32err_econnrefused", "111"),
        ("10048", ".Lw32err_eaddrinuse", "98"),
        ("112", ".Lw32err_enospc", "28"),
        ("183", ".Lw32err_eexist", "17"),
        ("206", ".Lw32err_enametoolong", "36"),
        ("122", ".Lw32err_erange", "34"),
        ("1113", ".Lw32err_eilseq", "84"),
    ] {
        assert!(asm.contains(&format!("cmp ecx, {code}")), "missing native code {code}");
        let terminal = asm.find(&format!("{label}:\n")).expect("mapping terminal missing");
        assert!(asm[terminal..].contains(&format!("mov eax, {errno}")));
    }
}
/// Verifies the unsupported futex syscall fails loudly instead of claiming
/// synchronization succeeded without sleeping or waking any waiter.
#[test]
fn test_futex_never_reports_false_success() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_futex(&mut emitter);
    let section = emitter.output();

    assert!(section.contains("mov DWORD PTR [rip + __rt_errno], 38"));
    assert!(section.contains("mov rax, -1"));
    assert!(!section.contains("xor rax, rax"));
}

/// Verifies PROT_NONE becomes a real inaccessible Windows guard page.
#[test]
fn test_mprotect_translates_prot_none_to_page_noaccess() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_mprotect(&mut emitter);
    let section = emitter.output();

    assert!(section.contains("test edx, edx"));
    assert!(section.contains("mov r8, 0x01"));
    assert!(section.contains("call VirtualProtect"));
}

/// Verifies mmap reserves and commits a region that VirtualProtect can guard.
#[test]
fn test_mmap_reserves_and_commits_protectable_region() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_shim_mmap(&mut emitter);
    let section = emitter.output();

    assert!(section.contains("mov r8, 0x3000"));
    assert!(section.contains("call VirtualAlloc"));
}

/// Verifies the low-stack Fiber abort path performs only a minimal aligned
/// `ExitProcess` call and cannot return into Wine's exception dispatcher.
#[test]
fn test_fiber_stack_overflow_abort_exits_without_cleanup() {
    let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
    emit_fiber_stack_overflow_abort(&mut emitter);
    let section = emitter.output();

    assert!(section.contains("and rsp, -16"));
    assert!(section.contains("sub rsp, 32"));
    assert!(section.contains("mov ecx, 134"));
    assert!(section.contains("call ExitProcess"));
    assert!(section.contains("ud2"));
    assert!(!section.contains("__rt_sys_free_argv"));
    assert!(!section.contains("__rt_winsock_cleanup"));
}
