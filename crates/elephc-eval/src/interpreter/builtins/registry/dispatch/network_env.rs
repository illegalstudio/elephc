//! Purpose:
//! Dispatches already evaluated network, environment, and stream-introspection builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated network, environment, and stream-introspection builtins.
pub(in crate::interpreter) fn eval_network_env_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "php_uname" => match evaluated_args {
            [] => eval_php_uname_result(None, values)?,
            [mode] => eval_php_uname_result(Some(*mode), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "gethostbyaddr" => {
            let [ip] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyaddr_result(*ip, values)?
        }
        "gethostbyname" => {
            let [hostname] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyname_result(*hostname, values)?
        }
        "gethostname" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_gethostname_result(values)?
        }
        "getprotobyname" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobyname_result(*protocol, values)?
        }
        "getprotobynumber" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobynumber_result(*protocol, values)?
        }
        "getservbyname" => {
            let [service, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyname_result(*service, *protocol, values)?
        }
        "getservbyport" => {
            let [port, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyport_result(*port, *protocol, values)?
        }
        "getenv" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getenv_result(*name, values)?
        }
        "inet_ntop" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_ntop_result(*value, values)?
        }
        "inet_pton" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_pton_result(*value, values)?
        }
        "ip2long" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ip2long_result(*value, values)?
        }
        "phpversion" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_phpversion_result(values)?
        }
        "putenv" => {
            let [assignment] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_putenv_result(*assignment, values)?
        }
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_introspection_result(name, values)?
        }
        "long2ip" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_long2ip_result(*value, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
