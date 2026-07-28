//! Purpose:
//! Declarative eval registry entry and process-resource implementation for `proc_open`.
//!
//! Called from:
//! - Eval builtin registry dispatch and the source-sensitive call dispatcher.
//!
//! Key details:
//! - The pipes argument is written by reference and process ownership remains in
//!   `EvalStreamResources` until `proc_close` waits for it.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "proc_open",
    area: Filesystem,
    params: [
        command,
        descriptor_spec,
        pipes: by_ref,
        cwd = EvalBuiltinDefaultValue::Null,
        env_vars = EvalBuiltinDefaultValue::Null,
        options = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [pipes],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_resources::EvalProcDescriptor;

/// Evaluates positional `proc_open` calls that cannot preserve a writable pipes target.
pub(in crate::interpreter) fn eval_proc_open_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    values.warning("proc_open(): Argument #3 ($pipes) must be passed by reference, value given")?;
    eval_proc_open_values(&evaluated, None, context, values)
}

/// Evaluates materialized `proc_open` arguments without a caller lvalue.
pub(in crate::interpreter) fn eval_proc_open_declared_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_proc_open_values(args, None, context, values)
}

/// Evaluates source call metadata while retaining the by-reference pipes lvalue.
pub(in crate::interpreter) fn eval_builtin_proc_open_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["command", "descriptor_spec", "pipes", "cwd", "env_vars", "options"],
        &evaluated,
        false,
    )?;
    let command = required_evaluated_ref_arg(&bound, 0)?;
    let descriptor = required_evaluated_ref_arg(&bound, 1)?;
    let pipes = required_evaluated_ref_arg(&bound, 2)?;
    let target = pipes.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?;
    let mut selected = vec![command.value, descriptor.value, pipes.value];
    for index in 3..=5 {
        if let Some(arg) = optional_evaluated_ref_arg(&bound, index) {
            selected.push(arg.value);
        }
    }
    eval_proc_open_values(&selected, Some(&target), context, values)
}

/// Starts one shell process and returns its eval-local process resource.
fn eval_proc_open_values(
    args: &[RuntimeCellHandle],
    pipes_target: Option<&EvalReferenceTarget>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=6).contains(&args.len()) || !values.is_array_like(args[1])? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let command = eval_path_string(args[0], values)?;
    let descriptors = eval_proc_descriptors(args[1], values)?;
    let cwd = match args.get(3).copied() {
        Some(value) if values.type_tag(value)? != EVAL_TAG_NULL => {
            Some(eval_path_string(value, values)?)
        }
        _ => None,
    };
    let env = match args.get(4).copied() {
        Some(value) if values.type_tag(value)? != EVAL_TAG_NULL => {
            Some(eval_proc_environment(value, values)?)
        }
        _ => None,
    };
    let bypass_shell = match args.get(5).copied() {
        Some(value) if values.type_tag(value)? != EVAL_TAG_NULL => {
            let key = values.string("bypass_shell")?;
            let option = values.array_get(value, key)?;
            values.type_tag(option)? != EVAL_TAG_NULL && values.truthy(option)?
        }
        _ => false,
    };
    match context.stream_resources_mut().open_process(
        &command,
        &descriptors,
        cwd.as_deref(),
        env.as_deref(),
        bypass_shell,
    ) {
        Some(result) => {
            let mut pipes = values.array_new(result.pipes.len())?;
            for (descriptor, pipe_id) in result.pipes {
                let key = values.int(descriptor)?;
                let pipe = values.resource(pipe_id)?;
                pipes = values.array_set(pipes, key, pipe)?;
            }
            if let Some(target) = pipes_target {
                eval_write_direct_ref_target(
                    target,
                    pipes,
                    context,
                    values,
                    Some(ScopeCellOwnership::Owned),
                )?;
            }
            values.resource(result.process_id)
        }
        None => values.bool_value(false),
    }
}

/// Parses PHP's descriptor specification into the three child stdio slots.
fn eval_proc_descriptors(
    spec: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<[Option<EvalProcDescriptor>; 3], EvalStatus> {
    let mut descriptors = std::array::from_fn(|_| None);
    for position in 0..values.array_len(spec)? {
        let key = values.array_iter_key(spec, position)?;
        let descriptor = usize::try_from(eval_int_value(key, values)?)
            .ok()
            .filter(|descriptor| *descriptor < descriptors.len())
            .ok_or(EvalStatus::RuntimeFatal)?;
        let entry = values.array_get(spec, key)?;
        if values.type_tag(entry)? == EVAL_TAG_INT {
            let target = usize::try_from(eval_int_value(entry, values)?)
                .ok()
                .filter(|target| *target < descriptors.len())
                .ok_or(EvalStatus::RuntimeFatal)?;
            descriptors[descriptor] = Some(EvalProcDescriptor::Redirect(target));
            continue;
        }
        if !values.is_array_like(entry)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let kind = eval_proc_descriptor_string(entry, 0, values)?;
        descriptors[descriptor] = Some(match kind.as_str() {
            "pipe" => {
                let mode = eval_proc_descriptor_string(entry, 1, values)?;
                match mode.as_str() {
                    "r" => EvalProcDescriptor::Pipe { child_reads: true },
                    "w" => EvalProcDescriptor::Pipe { child_reads: false },
                    _ => return Err(EvalStatus::RuntimeFatal),
                }
            }
            "file" => EvalProcDescriptor::File {
                path: eval_proc_descriptor_string(entry, 1, values)?,
                mode: eval_proc_descriptor_string(entry, 2, values)?,
            },
            _ => return Err(EvalStatus::RuntimeFatal),
        });
    }
    Ok(descriptors)
}

/// Reads one string field from an indexed descriptor tuple.
fn eval_proc_descriptor_string(
    descriptor: RuntimeCellHandle,
    index: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let key = values.int(index)?;
    let value = values.array_get(descriptor, key)?;
    if values.type_tag(value)? != EVAL_TAG_STRING {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_path_string(value, values)
}

/// Converts a PHP environment array into the exact child environment mapping.
fn eval_proc_environment(
    env: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(String, String)>, EvalStatus> {
    if !values.is_array_like(env)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut result = Vec::with_capacity(values.array_len(env)?);
    for position in 0..values.array_len(env)? {
        let key = values.array_iter_key(env, position)?;
        let value = values.array_get(env, key)?;
        result.push((eval_path_string(key, values)?, eval_path_string(value, values)?));
    }
    Ok(result)
}
