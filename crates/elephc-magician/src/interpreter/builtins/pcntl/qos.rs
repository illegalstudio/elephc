//! Purpose:
//! Implements macOS `Pcntl\QosClass` getter and setter adapters for Magician.
//!
//! Called from:
//! - The shared PCNTL evaluated-argument dispatcher on macOS.
//!
//! Key details:
//! - Native QoS values map to generated enum singletons through class-constant hooks.
//! - pthread failures become catchable PHP `Error` objects like php-src and AOT code.

use super::*;
#[cfg(target_os = "macos")]
use crate::runtime_hooks::owner_frames::{StagedRuntimeOwners, with_runtime_owners};

/// Evaluates a macOS QoS PCNTL function when the current target supports it.
#[cfg(target_os = "macos")]
pub(super) fn eval_pcntl_qos_result(
    name: &str,
    args: &[Option<EvaluatedCallArg>],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "pcntl_getqos_class" => {
            if args.iter().any(Option::is_some) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let ordinal = elephc_pcntl::elephc_pcntl_getqos_class();
            if ordinal < 0 {
                return eval_throw_error("invalid QOS class", context, values);
            }
            let case = match ordinal {
                0 => "UserInteractive",
                1 => "UserInitiated",
                2 => "Default",
                3 => "Utility",
                4 => "Background",
                _ => return eval_throw_error("invalid QOS class", context, values),
            };
            values
                .class_constant_get("Pcntl\\QosClass", case)?
                .ok_or(EvalStatus::RuntimeFatal)?
        }
        "pcntl_setqos_class" => {
            if args.len() != 1 || eval_pcntl_arg(args, 0).is_none() {
                return Err(EvalStatus::RuntimeFatal);
            }
            let qos = eval_pcntl_required_arg(args, 0)?;
            let owners = StagedRuntimeOwners::default();
            let success = with_runtime_owners(&owners, || {
                let result = (|| {
                    let name = owners.stage(values.property_get(qos.value, "name")?);
                    let name = values.string_bytes(name.borrowed())?;
                    Ok(unsafe {
                        elephc_pcntl::elephc_pcntl_setqos_class(name.as_ptr(), name.len())
                    })
                })();
                owners.complete(result, |cell| values.release(cell))
            })?;
            if success == 0 {
                return eval_throw_error("pcntl_setqos_class failed", context, values);
            }
            values.null()?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Rejects QoS dispatch on non-macOS targets at the internal adapter boundary.
#[cfg(not(target_os = "macos"))]
pub(super) fn eval_pcntl_qos_result(
    _name: &str,
    _args: &[Option<EvaluatedCallArg>],
    _context: &mut ElephcEvalContext,
    _values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    Ok(None)
}
