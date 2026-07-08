//! Purpose:
//! Eval registry entry and implementation for `exec` plus shared shell runner helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - `shell_exec`, `system`, and `passthru` call the runner owned by this file.

use std::process::Command;

use super::*;

eval_builtin! {
    name: "exec",
    area: NetworkEnv,
    params: [command],
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates `exec($command)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_exec(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_process_command("exec", args, context, scope, values)
}

/// Evaluates already materialized `exec()` command arguments.
pub(in crate::interpreter) fn eval_exec_result(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_process_command_result("exec", command, values)
}

/// Evaluates one eval process-control builtin over a command expression.
pub(in crate::interpreter) fn eval_builtin_process_command(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [command] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let command = eval_expr(command, context, scope, values)?;
    eval_process_command_result(name, command, values)
}

/// Evaluates one already materialized process-control command argument.
pub(in crate::interpreter) fn eval_process_command_result(
    name: &str,
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let command = eval_shell_command_string(command, values)?;
    let output = eval_shell_command_output(&command);
    match name {
        "exec" | "shell_exec" => values.string_bytes_value(&output),
        "system" => {
            eval_echo_process_output(&output, values)?;
            values.string("")
        }
        "passthru" => {
            eval_echo_process_output(&output, values)?;
            values.null()
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Converts a PHP command cell into the host shell string accepted by `Command`.
fn eval_shell_command_string(
    command: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let command = values.string_bytes(command)?;
    Ok(String::from_utf8_lossy(&command).into_owned())
}

/// Executes a shell command and returns stdout bytes, mapping spawn failures to an empty string.
fn eval_shell_command_output(command: &str) -> Vec<u8> {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .map(|output| output.stdout)
        .unwrap_or_default()
}

/// Echoes captured process output through the eval runtime value hooks.
fn eval_echo_process_output(
    output: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if output.is_empty() {
        return Ok(());
    }
    let output = values.string_bytes_value(output)?;
    values.echo(output)
}
