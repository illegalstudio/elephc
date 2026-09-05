//! Purpose:
//! Stages and executes `pcntl_exec` arguments through the shared native bridge.
//!
//! Called from:
//! - The shared PCNTL evaluated-argument dispatcher.
//!
//! Key details:
//! - The bridge copies every path, argument, key, and value before `execve`.
//! - Omitted environments inherit the process; explicit empty arrays clear it.

use super::*;

/// Releases an unconsumed native exec builder on early conversion failure.
struct ExecBuilderGuard(*mut libc::c_void);

impl Drop for ExecBuilderGuard {
    /// Frees the bridge builder unless ownership was transferred to `exec_run`.
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { elephc_pcntl::elephc_pcntl_exec_free(self.0) };
        }
    }
}

/// Evaluates `pcntl_exec` and returns false on its only returning bridge path.
pub(super) fn eval_pcntl_exec_result(
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = values.string_bytes(eval_pcntl_required_arg(args, 0)?.value)?;
    if path.contains(&0) {
        return eval_throw_builtin_value_error(
            "pcntl_exec(): Argument #1 ($path) must not contain any null bytes",
            context,
            values,
        );
    }
    let arguments = eval_pcntl_arg(args, 1);
    let environment = eval_pcntl_arg(args, 2);
    let builder = unsafe {
        elephc_pcntl::elephc_pcntl_exec_new(
            path.as_ptr(),
            path.len(),
            libc::c_int::from(environment.is_some()),
        )
    };
    if builder.is_null() {
        return values.bool_value(false);
    }
    let mut guard = ExecBuilderGuard(builder);
    if let Some(arguments) = arguments {
        eval_pcntl_exec_add_arguments(builder, arguments.value, context, values)?;
    }
    if let Some(environment) = environment {
        eval_pcntl_exec_add_environment(builder, environment.value, context, values)?;
    }
    guard.0 = std::ptr::null_mut();
    let success = unsafe { elephc_pcntl::elephc_pcntl_exec_run(builder) };
    if success == 0 {
        values.warning(&elephc_pcntl::pcntl_last_error_warning(
            elephc_pcntl::PCNTL_WARNING_EXEC,
        ))?;
    }
    values.bool_value(success != 0)
}

/// Copies one PHP argument array into the opaque bridge builder.
fn eval_pcntl_exec_add_arguments(
    builder: *mut libc::c_void,
    arguments: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.is_array_like(arguments)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    for position in 0..values.array_len(arguments)? {
        let key = values.array_iter_key(arguments, position)?;
        let argument = values.array_get(arguments, key)?;
        let bytes = eval_pcntl_exec_string_bytes(argument, context, values)?;
        if bytes.contains(&0) {
            let _: RuntimeCellHandle = eval_throw_builtin_value_error(
                "pcntl_exec(): Argument #2 ($args) individual argument must not contain null bytes",
                context,
                values,
            )?;
        }
        if unsafe {
            elephc_pcntl::elephc_pcntl_exec_add_arg(builder, bytes.as_ptr(), bytes.len())
        } == 0
        {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Copies one PHP environment array into the opaque bridge builder.
fn eval_pcntl_exec_add_environment(
    builder: *mut libc::c_void,
    environment: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.is_array_like(environment)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    for position in 0..values.array_len(environment)? {
        let key = values.array_iter_key(environment, position)?;
        let value = values.array_get(environment, key)?;
        let value = eval_pcntl_exec_string_bytes(value, context, values)?;
        if value.contains(&0) {
            let _: RuntimeCellHandle = eval_throw_builtin_value_error(
                "pcntl_exec(): Argument #3 ($env_vars) value for environment variable must not contain null bytes",
                context,
                values,
            )?;
        }
        let (key_low, key_high, key_bytes) = match values.type_tag(key)? {
            EVAL_TAG_INT => (eval_int_value(key, values)? as u64, -1, None),
            EVAL_TAG_STRING => {
                let bytes = values.string_bytes(key)?;
                if bytes.contains(&0) {
                    let _: RuntimeCellHandle = eval_throw_builtin_value_error(
                        "pcntl_exec(): Argument #3 ($env_vars) name for environment variable must not contain null bytes",
                        context,
                        values,
                    )?;
                }
                (bytes.as_ptr() as u64, bytes.len() as i64, Some(bytes))
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        let success = unsafe {
            elephc_pcntl::elephc_pcntl_exec_add_env(
                builder,
                key_low,
                key_high,
                value.as_ptr(),
                value.len(),
            )
        };
        drop(key_bytes);
        if success == 0 {
            return Err(EvalStatus::RuntimeFatal);
        }
    }
    Ok(())
}

/// Applies PHP string-context conversion to one exec array entry.
fn eval_pcntl_exec_string_bytes(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        values.warning("Warning: Array to string conversion\n")?;
        return Ok(b"Array".to_vec());
    }
    let value = eval_string_context_value(value, context, values)?;
    values.string_bytes(value)
}
