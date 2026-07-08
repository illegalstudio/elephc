//! Purpose:
//! Declarative eval registry entries and dispatch adapters for network,
//! environment, process, and system-information builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated network/env dispatch.
//!
//! Key details:
//! - Runtime behavior stays in the focused sibling helper modules; this module
//!   owns registry metadata and small hook adapters only.

use super::super::super::*;
use super::*;

mod exec;
mod getenv;
mod gethostbyaddr;
mod gethostbyname;
mod gethostname;
mod getprotobyname;
mod getprotobynumber;
mod getservbyname;
mod getservbyport;
mod inet_ntop;
mod inet_pton;
mod ip2long;
mod long2ip;
mod passthru;
mod php_uname;
mod phpversion;
mod putenv;
mod shell_exec;
mod system;

/// Dispatches direct expression-level calls for declaratively migrated network/env builtins.
pub(in crate::interpreter) fn eval_builtin_network_env_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "exec" | "shell_exec" | "system" | "passthru" => {
            eval_builtin_process_command(name, args, context, scope, values)
        }
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "gethostbyaddr" => eval_builtin_gethostbyaddr(args, context, scope, values),
        "gethostbyname" => eval_builtin_gethostbyname(args, context, scope, values),
        "gethostname" => eval_builtin_gethostname(args, values),
        "getprotobyname" => eval_builtin_getprotobyname(args, context, scope, values),
        "getprotobynumber" => eval_builtin_getprotobynumber(args, context, scope, values),
        "getservbyname" => eval_builtin_getservbyname(args, context, scope, values),
        "getservbyport" => eval_builtin_getservbyport(args, context, scope, values),
        "inet_ntop" => eval_builtin_inet_ntop(args, context, scope, values),
        "inet_pton" => eval_builtin_inet_pton(args, context, scope, values),
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
        "long2ip" => eval_builtin_long2ip(args, context, scope, values),
        "php_uname" => super::super::eval_builtin_php_uname(args, context, scope, values),
        "phpversion" => super::super::eval_builtin_phpversion(args, values),
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated network/env builtins.
pub(in crate::interpreter) fn eval_network_env_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "php_uname" => match evaluated_args {
            [] => super::super::eval_php_uname_result(None, values),
            [mode] => super::super::eval_php_uname_result(Some(*mode), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "gethostbyaddr" => {
            let [ip] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyaddr_result(*ip, values)
        }
        "gethostbyname" => {
            let [hostname] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyname_result(*hostname, values)
        }
        "gethostname" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_gethostname_result(values)
        }
        "getprotobyname" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobyname_result(*protocol, values)
        }
        "getprotobynumber" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobynumber_result(*protocol, values)
        }
        "getservbyname" => {
            let [service, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyname_result(*service, *protocol, values)
        }
        "getservbyport" => {
            let [port, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyport_result(*port, *protocol, values)
        }
        "getenv" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getenv_result(*name, values)
        }
        "exec" | "shell_exec" | "system" | "passthru" => {
            let [command] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_process_command_result(name, *command, values)
        }
        "inet_ntop" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_ntop_result(*value, values)
        }
        "inet_pton" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_pton_result(*value, values)
        }
        "ip2long" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ip2long_result(*value, values)
        }
        "phpversion" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            super::super::eval_phpversion_result(values)
        }
        "putenv" => {
            let [assignment] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_putenv_result(*assignment, values)
        }
        "long2ip" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_long2ip_result(*value, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
