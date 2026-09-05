//! Purpose:
//! Defines target-aware integer constants exposed by PHP's PCNTL extension.
//!
//! Called from:
//! - The compiler constant registry and Magician's predefined-constant evaluator.
//!
//! Key details:
//! - Values follow the target libc and intentionally differ between macOS and Linux.

/// Integer constants exposed by `ext/pcntl` when targeting macOS.
pub const MACOS_PCNTL_INT_CONSTANTS: &[(&str, i64)] = &[
    ("PCNTL_E2BIG", 7), ("PCNTL_EACCES", 13), ("PCNTL_EAGAIN", 35),
    ("PCNTL_ECHILD", 10), ("PCNTL_EFAULT", 14), ("PCNTL_EINTR", 4),
    ("PCNTL_EINVAL", 22), ("PCNTL_EIO", 5), ("PCNTL_EISDIR", 21),
    ("PCNTL_ELOOP", 62), ("PCNTL_EMFILE", 24), ("PCNTL_ENAMETOOLONG", 63),
    ("PCNTL_ENFILE", 23), ("PCNTL_ENOENT", 2), ("PCNTL_ENOEXEC", 8),
    ("PCNTL_ENOMEM", 12), ("PCNTL_ENOSPC", 28), ("PCNTL_ENOTDIR", 20),
    ("PCNTL_EPERM", 1), ("PCNTL_ESRCH", 3), ("PCNTL_ETXTBSY", 26),
    ("PCNTL_EUSERS", 68), ("PRIO_DARWIN_BG", 4_096), ("PRIO_DARWIN_THREAD", 3),
    ("PRIO_PGRP", 1), ("PRIO_PROCESS", 0), ("PRIO_USER", 2),
    ("P_ALL", 0), ("P_PGID", 2), ("P_PID", 1),
    ("SIGABRT", 6), ("SIGALRM", 14), ("SIGBABY", 12),
    ("SIGBUS", 10), ("SIGCHLD", 20), ("SIGCONT", 19),
    ("SIGFPE", 8), ("SIGHUP", 1), ("SIGILL", 4),
    ("SIGINFO", 29), ("SIGINT", 2), ("SIGIO", 23),
    ("SIGIOT", 6), ("SIGKILL", 9), ("SIGPIPE", 13),
    ("SIGPROF", 27), ("SIGQUIT", 3), ("SIGSEGV", 11),
    ("SIGSTOP", 17), ("SIGSYS", 12), ("SIGTERM", 15),
    ("SIGTRAP", 5), ("SIGTSTP", 18), ("SIGTTIN", 21),
    ("SIGTTOU", 22), ("SIGURG", 16), ("SIGUSR1", 30),
    ("SIGUSR2", 31), ("SIGVTALRM", 26), ("SIGWINCH", 28),
    ("SIGXCPU", 24), ("SIGXFSZ", 25), ("SIG_BLOCK", 1),
    ("SIG_DFL", 0), ("SIG_ERR", -1), ("SIG_IGN", 1),
    ("SIG_SETMASK", 3), ("SIG_UNBLOCK", 2), ("WCONTINUED", 16),
    ("WEXITED", 4), ("WNOHANG", 1), ("WNOWAIT", 32),
    ("WSTOPPED", 8), ("WUNTRACED", 2),
];

/// Integer constants exposed by `ext/pcntl` when targeting Linux.
pub const LINUX_PCNTL_INT_CONSTANTS: &[(&str, i64)] = &[
    ("BUS_ADRALN", 1), ("BUS_ADRERR", 2), ("BUS_OBJERR", 3),
    ("CLD_CONTINUED", 6), ("CLD_DUMPED", 3), ("CLD_EXITED", 1),
    ("CLD_KILLED", 2), ("CLD_STOPPED", 5), ("CLD_TRAPPED", 4),
    ("CLONE_NEWCGROUP", 33_554_432), ("CLONE_NEWIPC", 134_217_728),
    ("CLONE_NEWNET", 1_073_741_824), ("CLONE_NEWNS", 131_072),
    ("CLONE_NEWPID", 536_870_912), ("CLONE_NEWUSER", 268_435_456),
    ("CLONE_NEWUTS", 67_108_864), ("FPE_FLTDIV", 3), ("FPE_FLTINV", 7),
    ("FPE_FLTOVF", 4), ("FPE_FLTRES", 6), ("FPE_FLTSUB", 8),
    ("FPE_FLTUND", 5), ("FPE_INTDIV", 1), ("FPE_INTOVF", 2),
    ("ILL_BADSTK", 8), ("ILL_COPROC", 7), ("ILL_ILLADR", 3),
    ("ILL_ILLOPC", 1), ("ILL_ILLOPN", 2), ("ILL_ILLTRP", 4),
    ("ILL_PRVOPC", 5), ("ILL_PRVREG", 6), ("PCNTL_E2BIG", 7),
    ("PCNTL_EACCES", 13), ("PCNTL_EAGAIN", 11), ("PCNTL_ECHILD", 10),
    ("PCNTL_EFAULT", 14), ("PCNTL_EINTR", 4), ("PCNTL_EINVAL", 22),
    ("PCNTL_EIO", 5), ("PCNTL_EISDIR", 21), ("PCNTL_ELIBBAD", 80),
    ("PCNTL_ELOOP", 40), ("PCNTL_EMFILE", 24), ("PCNTL_ENAMETOOLONG", 36),
    ("PCNTL_ENFILE", 23), ("PCNTL_ENOENT", 2), ("PCNTL_ENOEXEC", 8),
    ("PCNTL_ENOMEM", 12), ("PCNTL_ENOSPC", 28), ("PCNTL_ENOTDIR", 20),
    ("PCNTL_EPERM", 1), ("PCNTL_ESRCH", 3), ("PCNTL_ETXTBSY", 26),
    ("PCNTL_EUSERS", 87), ("POLL_ERR", 4), ("POLL_HUP", 6),
    ("POLL_IN", 1), ("POLL_MSG", 3), ("POLL_OUT", 2), ("POLL_PRI", 5),
    ("PRIO_PGRP", 1), ("PRIO_PROCESS", 0), ("PRIO_USER", 2),
    ("P_ALL", 0), ("P_PGID", 2), ("P_PID", 1), ("P_PIDFD", 3),
    ("SEGV_ACCERR", 2), ("SEGV_MAPERR", 1), ("SIGABRT", 6),
    ("SIGALRM", 14), ("SIGBABY", 31), ("SIGBUS", 7),
    ("SIGCHLD", 17), ("SIGCLD", 17), ("SIGCONT", 18),
    ("SIGFPE", 8), ("SIGHUP", 1), ("SIGILL", 4),
    ("SIGINT", 2), ("SIGIO", 29), ("SIGIOT", 6),
    ("SIGKILL", 9), ("SIGPIPE", 13), ("SIGPOLL", 29),
    ("SIGPROF", 27), ("SIGPWR", 30), ("SIGQUIT", 3),
    ("SIGRTMAX", 64), ("SIGRTMIN", 34), ("SIGSEGV", 11),
    ("SIGSTKFLT", 16), ("SIGSTOP", 19), ("SIGSYS", 31),
    ("SIGTERM", 15), ("SIGTRAP", 5), ("SIGTSTP", 20),
    ("SIGTTIN", 21), ("SIGTTOU", 22), ("SIGURG", 23),
    ("SIGUSR1", 10), ("SIGUSR2", 12), ("SIGVTALRM", 26),
    ("SIGWINCH", 28), ("SIGXCPU", 24), ("SIGXFSZ", 25),
    ("SIG_BLOCK", 0), ("SIG_DFL", 0), ("SIG_ERR", -1),
    ("SIG_IGN", 1), ("SIG_SETMASK", 2), ("SIG_UNBLOCK", 1),
    ("SI_ASYNCIO", -4), ("SI_KERNEL", 128), ("SI_MESGQ", -3),
    ("SI_QUEUE", -1), ("SI_SIGIO", -5), ("SI_TIMER", -2),
    ("SI_TKILL", -6), ("SI_USER", 0), ("TRAP_BRKPT", 1),
    ("TRAP_TRACE", 2), ("WCONTINUED", 8), ("WEXITED", 4),
    ("WNOHANG", 1), ("WNOWAIT", 16_777_216), ("WSTOPPED", 2),
    ("WUNTRACED", 2),
];

/// Looks up one PCNTL constant using the build host's supported target table.
pub fn host_pcntl_int_constant(name: &str) -> Option<i64> {
    let constants = if cfg!(target_os = "macos") {
        MACOS_PCNTL_INT_CONSTANTS
    } else {
        LINUX_PCNTL_INT_CONSTANTS
    };
    constants
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
}

/// Reports whether a name belongs to PCNTL on at least one supported target.
pub fn is_pcntl_int_constant(name: &str) -> bool {
    MACOS_PCNTL_INT_CONSTANTS
        .iter()
        .chain(LINUX_PCNTL_INT_CONSTANTS.iter())
        .any(|(candidate, _)| *candidate == name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use super::*;

    /// Looks up one named constant in a selected target table.
    fn value(constants: &[(&str, i64)], name: &str) -> Option<i64> {
        constants.iter().find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    }

    /// Verifies platform-dependent values and target isolation.
    #[test]
    fn values_and_availability_follow_the_target() {
        assert_eq!(value(MACOS_PCNTL_INT_CONSTANTS, "SIGCHLD"), Some(20));
        assert_eq!(value(LINUX_PCNTL_INT_CONSTANTS, "SIGCHLD"), Some(17));
        assert_eq!(value(MACOS_PCNTL_INT_CONSTANTS, "PCNTL_EAGAIN"), Some(35));
        assert_eq!(value(LINUX_PCNTL_INT_CONSTANTS, "PCNTL_EAGAIN"), Some(11));
        assert_eq!(value(LINUX_PCNTL_INT_CONSTANTS, "CLONE_NEWNS"), Some(131_072));
        assert_eq!(value(MACOS_PCNTL_INT_CONSTANTS, "CLONE_NEWNS"), None);
        assert!(is_pcntl_int_constant("CLONE_NEWNS"));
        assert!(!is_pcntl_int_constant("NOT_A_PCNTL_CONSTANT"));
    }

    /// Verifies neither target table contains duplicate names.
    #[test]
    fn target_tables_have_unique_names() {
        for constants in [MACOS_PCNTL_INT_CONSTANTS, LINUX_PCNTL_INT_CONSTANTS] {
            let names = constants.iter().map(|(name, _)| *name).collect::<HashSet<_>>();
            assert_eq!(names.len(), constants.len());
        }
    }
}
