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
    BuiltinContract {
        id: BuiltinId::from_canonical_name(name),
        name,
        area: Area::System,
        kind: BuiltinKind::Function,
        params,
        variadic: None,
        min_args: None,
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

/// PCNTL contracts whose typed bridge implementations are available to AOT.
pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    pcntl_contract(
        "pcntl_alarm",
        &[ParamSpec {
            name: "seconds",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
        }],
        TypeSpec::Int,
        "Schedules a SIGALRM and returns the prior alarm's remaining seconds.",
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
        "pcntl_getpriority",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
            },
            ParamSpec {
                name: "mode",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
            },
        ],
        TypeSpec::Mixed,
        "Returns a process, process-group, or user scheduling priority, or false on failure.",
    ),
    pcntl_contract(
        "pcntl_setpriority",
        &[
            ParamSpec {
                name: "priority",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
            },
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Null),
                by_ref: false,
            },
            ParamSpec {
                name: "mode",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
            },
        ],
        TypeSpec::Bool,
        "Changes a process, process-group, or user scheduling priority.",
    ),
    pcntl_contract(
        "pcntl_strerror",
        &[ParamSpec {
            name: "error_code",
            ty: TypeSpec::Int,
            default: None,
            by_ref: false,
        }],
        TypeSpec::Str,
        "Returns the system message for a PCNTL errno value.",
    ),
    pcntl_contract(
        "pcntl_wait",
        &[
            ParamSpec {
                name: "status",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: true,
            },
            ParamSpec {
                name: "flags",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
            },
            ParamSpec {
                name: "resource_usage",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
            },
        ],
        TypeSpec::Int,
        "Waits for any child process and writes its target-native status.",
    ),
    pcntl_contract(
        "pcntl_waitpid",
        &[
            ParamSpec {
                name: "process_id",
                ty: TypeSpec::Int,
                default: None,
                by_ref: false,
            },
            ParamSpec {
                name: "status",
                ty: TypeSpec::Mixed,
                default: None,
                by_ref: true,
            },
            ParamSpec {
                name: "flags",
                ty: TypeSpec::Int,
                default: Some(DefaultSpec::Int(0)),
                by_ref: false,
            },
            ParamSpec {
                name: "resource_usage",
                ty: TypeSpec::Mixed,
                default: Some(DefaultSpec::EmptyArray),
                by_ref: true,
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
        }],
        TypeSpec::Mixed,
        "Returns the terminating signal encoded in a child wait status.",
    ),
];
