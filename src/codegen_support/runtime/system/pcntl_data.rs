//! Purpose:
//! Emits fixed string keys used by PCNTL resource-usage and signal-info arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::data::fixed::emit_runtime_data_fixed()`.
//!
//! Key details:
//! - Symbol spellings are a private assembly ABI shared with the PCNTL runtime emitters.

/// Returns assembly data for PCNTL array keys and process-wide signal-dispatch state.
pub(crate) fn emit_pcntl_data() -> String {
    let fields = [
        ("_pcntl_rusage_oublock", "ru_oublock"),
        ("_pcntl_rusage_inblock", "ru_inblock"),
        ("_pcntl_rusage_msgsnd", "ru_msgsnd"),
        ("_pcntl_rusage_msgrcv", "ru_msgrcv"),
        ("_pcntl_rusage_maxrss", "ru_maxrss"),
        ("_pcntl_rusage_ixrss", "ru_ixrss"),
        ("_pcntl_rusage_idrss", "ru_idrss"),
        ("_pcntl_rusage_minflt", "ru_minflt"),
        ("_pcntl_rusage_majflt", "ru_majflt"),
        ("_pcntl_rusage_nsignals", "ru_nsignals"),
        ("_pcntl_rusage_nvcsw", "ru_nvcsw"),
        ("_pcntl_rusage_nivcsw", "ru_nivcsw"),
        ("_pcntl_rusage_nswap", "ru_nswap"),
        ("_pcntl_rusage_utime_usec", "ru_utime.tv_usec"),
        ("_pcntl_rusage_utime_sec", "ru_utime.tv_sec"),
        ("_pcntl_rusage_stime_usec", "ru_stime.tv_usec"),
        ("_pcntl_rusage_stime_sec", "ru_stime.tv_sec"),
    ];
    let mut out = String::new();
    for (symbol, key) in fields {
        out.push_str(&format!(".globl {symbol}\n{symbol}:\n    .ascii {key:?}\n"));
    }
    let siginfo_fields = [
        ("_pcntl_siginfo_signo", "signo"),
        ("_pcntl_siginfo_errno", "errno"),
        ("_pcntl_siginfo_code", "code"),
        ("_pcntl_siginfo_status", "status"),
        ("_pcntl_siginfo_pid", "pid"),
        ("_pcntl_siginfo_uid", "uid"),
        ("_pcntl_siginfo_utime", "utime"),
        ("_pcntl_siginfo_stime", "stime"),
        ("_pcntl_siginfo_addr", "addr"),
        ("_pcntl_siginfo_band", "band"),
        ("_pcntl_siginfo_fd", "fd"),
    ];
    for (symbol, key) in siginfo_fields {
        out.push_str(&format!(".globl {symbol}\n{symbol}:\n    .ascii {key:?}\n"));
    }
    out.push_str("    .balign 8\n");
    out.push_str(".globl __rt_pcntl_handler_kind\n__rt_pcntl_handler_kind:\n    .zero 1024\n");
    out.push_str(".globl __rt_pcntl_handler_descriptor\n__rt_pcntl_handler_descriptor:\n    .zero 1024\n");
    out.push_str(".globl __rt_pcntl_async_enabled\n__rt_pcntl_async_enabled:\n    .quad 0\n");
    out.push_str(".globl __rt_pcntl_dispatching\n__rt_pcntl_dispatching:\n    .quad 0\n");
    out.push_str(".globl __rt_pcntl_dispatch_mask\n__rt_pcntl_dispatch_mask:\n    .zero 128\n");
    out.push_str(".globl __rt_pcntl_signal_fn\n__rt_pcntl_signal_fn:\n    .quad 0\n");
    out.push_str(".globl __rt_pcntl_signal_next_fn\n__rt_pcntl_signal_next_fn:\n    .quad 0\n");
    out.push_str(".globl __rt_pcntl_dispatch_begin_fn\n__rt_pcntl_dispatch_begin_fn:\n    .quad 0\n");
    out.push_str(".globl __rt_pcntl_dispatch_end_fn\n__rt_pcntl_dispatch_end_fn:\n    .quad 0\n");
    out
}
