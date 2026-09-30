//! Purpose:
//! Declares the dependency-neutral PCNTL function contracts implemented by both compiler backends.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative shared builtin catalog.
//!
//! Key details:
//! - Platform availability and backend behavior remain in typed semantic descriptors, not this PHP surface.

use crate::{Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, TypeSpec};

/// Builds one ordinary public PCNTL function contract with shared fixed metadata.
const fn pcntl_contract(
    name: &'static str,
    params: &'static [ParamSpec],
    returns: TypeSpec,
    summary: &'static str,
) -> BuiltinContract {
    pcntl_contract_with_min_args(name, params, returns, summary, None)
}

/// Builds a PCNTL contract whose PHP signature permits fewer arguments than its
/// dependency-neutral parameter defaults can express.
const fn pcntl_contract_with_min_args(
    name: &'static str,
    params: &'static [ParamSpec],
    returns: TypeSpec,
    summary: &'static str,
    min_args: Option<usize>,
) -> BuiltinContract {
    BuiltinContract {
        id: BuiltinId::from_canonical_name(name),
        name,
        area: Area::System,
        module: crate::PhpModule::Pcntl,
        since: None,
        kind: BuiltinKind::Function,
        params,
        variadic: None,
        variadic_by_ref: false,
        variadic_writes: None,
        min_args,
        max_args: None,
        arity_error: None,
        returns,
        by_ref_return: false,
        summary,
        examples: &[],
        php_manual: None,
        deprecation: None,
        extension: false,
        internal: false,
        requirements: &[],
    }
}

/// Builds one Elephc-only PCNTL extension contract hidden by `--strict-php`.
const fn pcntl_extension_contract(
    name: &'static str,
    params: &'static [ParamSpec],
    returns: TypeSpec,
    summary: &'static str,
) -> BuiltinContract {
    let mut contract = pcntl_contract(name, params, returns, summary);
    contract.extension = true;
    contract
}

/// Attributes the POSIX session helpers implemented by the PCNTL bridge to ext/posix.
const fn posix_contract(
    name: &'static str,
    params: &'static [ParamSpec],
    returns: TypeSpec,
    summary: &'static str,
) -> BuiltinContract {
    let mut contract = pcntl_contract(name, params, returns, summary);
    contract.module = crate::PhpModule::Posix;
    contract
}

/// PCNTL contracts whose typed bridge implementations are available to AOT.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    pcntl_contract(
        "pcntl_alarm",
        &[ParamSpec {
            name: "seconds",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Int,
        "Schedules a SIGALRM and returns the prior alarm's remaining seconds.",
    ),
    pcntl_contract(
        "pcntl_async_signals",
        &[ParamSpec {
            name: "enable",
            ty: TypeSpec::Bool,
            default: Some(DefaultSpec::Null),
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Enables or queries automatic dispatch of pending signal callbacks.",
    ),
    pcntl_extension_contract(
        "pcntl_daemon",
        &[
            ParamSpec {
                name: "no_chdir",
                ty: TypeSpec::Bool,
                default: Some(DefaultSpec::Bool(false)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "no_close",
                ty: TypeSpec::Bool,
                default: Some(DefaultSpec::Bool(false)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Detaches the surviving child into a background daemon process.",
    ),
    pcntl_contract(
        "pcntl_exec",
        &[
            ParamSpec {
                name: "path",
                ty: TypeSpec::Str,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "args",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "env_vars",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Replaces the current process image with a program and optional arguments and environment.",
    ),
    pcntl_contract(
        "pcntl_errno",
        &[],
        TypeSpec::Int,
        "Returns the errno recorded by the most recent failing PCNTL operation.",
    ),
    pcntl_contract(
        "pcntl_fork",
        &[],
        TypeSpec::Int,
        "Forks the current process and returns the child or parent process identifier.",
    ),
    pcntl_contract(
        "pcntl_get_last_error",
        &[],
        TypeSpec::Int,
        "Returns the errno recorded by the most recent failing PCNTL operation.",
    ),
    pcntl_contract(
        "pcntl_getcpu",
        &[],
        TypeSpec::Int,
        "Returns the logical CPU on which the calling thread is executing.",
    ),
    pcntl_contract(
        "pcntl_getcpuaffinity",
        &[ParamSpec {
            name: "process_id",
            ty: TypeSpec::Int,
            default: Some(DefaultSpec::Null),
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Mixed,
        "Returns the CPU affinity mask for a Linux process, or false on failure.",
    ),
    pcntl_contract(
        "pcntl_getpriority",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "mode",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Mixed,
        "Returns a process, process-group, or user scheduling priority, or false on failure.",
    ),
    pcntl_contract(
        "pcntl_getqos_class",
        &[],
        TypeSpec::Mixed,
        "Returns the current macOS thread quality-of-service class.",
    ),
    pcntl_contract(
        "pcntl_setcpuaffinity",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "cpu_ids",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Changes the CPU affinity mask for a Linux process.",
    ),
    pcntl_contract(
        "pcntl_setns",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "nstype",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0x4000_0000)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Joins one Linux namespace of the selected process.",
    ),
    pcntl_contract(
        "pcntl_setpriority",
        &[
            ParamSpec {
                name: "priority",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "mode",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Changes a process, process-group, or user scheduling priority.",
    ),
    pcntl_contract(
        "pcntl_setqos_class",
        &[ParamSpec {
            name: "qos_class",
            ty: TypeSpec::Mixed,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Void,
        "Changes the current macOS thread quality-of-service class.",
    ),
    pcntl_contract(
        "pcntl_signal",
        &[
            ParamSpec {
                name: "signal",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "handler",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "restart_syscalls",
                ty: TypeSpec::Bool,
                default: Some(DefaultSpec::Bool(true)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Installs a callable, default, or ignored disposition for one signal.",
    ),
    pcntl_contract(
        "pcntl_signal_dispatch",
        &[],
        TypeSpec::Bool,
        "Invokes callbacks for every signal currently pending in PCNTL's queue.",
    ),
    pcntl_contract(
        "pcntl_signal_get_handler",
        &[ParamSpec {
            name: "signal",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Mixed,
        "Returns the callable or integer disposition registered for one signal.",
    ),
    pcntl_contract(
        "pcntl_sigprocmask",
        &[
            ParamSpec {
                name: "mode",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "signals",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "old_signals",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Changes the signal mask and optionally writes the prior blocked signals.",
    ),
    pcntl_contract(
        "pcntl_sigtimedwait",
        &[
            ParamSpec {
                name: "signals",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "info",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
            ParamSpec {
                name: "seconds",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "nanoseconds",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Mixed,
        "Waits up to a timeout for one selected Linux signal and returns its number or false.",
    ),
    pcntl_contract(
        "pcntl_sigwaitinfo",
        &[
            ParamSpec {
                name: "signals",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "info",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
        ],
        TypeSpec::Mixed,
        "Waits synchronously for one selected Linux signal and returns its number or false.",
    ),
    pcntl_contract(
        "pcntl_strerror",
        &[ParamSpec {
            name: "error_code",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Str,
        "Returns the system message for a PCNTL errno value.",
    ),
    pcntl_contract(
        "pcntl_unshare",
        &[ParamSpec {
            name: "flags",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Disassociates selected Linux process execution contexts.",
    ),
    pcntl_contract(
        "pcntl_wait",
        &[
            ParamSpec {
                name: "status",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: true,
                writes: None,
            },
            ParamSpec {
                name: "flags",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "resource_usage",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
        ],
        TypeSpec::Int,
        "Waits for any child process and writes its target-native status.",
    ),
    pcntl_contract(
        "pcntl_waitid",
        &[
            ParamSpec {
                name: "idtype",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "info",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
            ParamSpec {
                name: "flags",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(4)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "resource_usage",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Waits for a child state change and writes signal information plus optional PHP 8.5 resource usage.",
    ),
    pcntl_contract(
        "pcntl_waitpid",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "status",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: true,
                writes: None,
            },
            ParamSpec {
                name: "flags",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "resource_usage",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
                writes: None,
            },
        ],
        TypeSpec::Int,
        "Waits for a selected child process and writes its target-native status.",
    ),
    pcntl_contract(
        "pcntl_wexitstatus",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Mixed,
        "Returns the exit code encoded in a child wait status.",
    ),
    pcntl_contract(
        "pcntl_wifcontinued",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Reports whether a child wait status represents continued execution.",
    ),
    pcntl_contract(
        "pcntl_wifexited",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Reports whether a child wait status represents normal termination.",
    ),
    pcntl_contract(
        "pcntl_wifsignaled",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Reports whether a child wait status represents signal termination.",
    ),
    pcntl_contract(
        "pcntl_wifstopped",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Bool,
        "Reports whether a child wait status represents a stopped process.",
    ),
    pcntl_contract(
        "pcntl_wstopsig",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Mixed,
        "Returns the stopping signal encoded in a child wait status.",
    ),
    pcntl_contract(
        "pcntl_wtermsig",
        &[ParamSpec {
            name: "status",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
            writes: None,
        }],
        TypeSpec::Mixed,
        "Returns the terminating signal encoded in a child wait status.",
    ),
    posix_contract(
        "posix_setpgid",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
            ParamSpec {
                name: "process_group_id",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
                writes: None,
            },
        ],
        TypeSpec::Bool,
        "Moves a process into a process group for job control.",
    ),
    posix_contract(
        "posix_setsid",
        &[],
        TypeSpec::Int,
        "Creates a new session and makes the current process its leader.",
    ),
];
