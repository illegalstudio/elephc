//! Purpose:
//! Dispatches PHP libxml compatibility functions used by the DOM extension.
//! Encodes structured errors as detached PHP `LibXMLError` value objects.
//!
//! Called from:
//! - `super::dispatch()` for public libxml functions.
//! - Document parsing paths when native structured errors must be retained.
//!
//! Key details:
//! - Error and loader state is isolated per bridge context.
//! - Host-owned callable references are balanced through context callbacks.

use crate::abi::{
    Value, VALUE_BOOL, VALUE_BYTES, VALUE_CALLABLE, VALUE_INT, VALUE_NULL,
    VALUE_RESOURCE,
};
use crate::context::Context;
use crate::host::HostCallError;
use crate::objects::LibxmlErrorObject;
use crate::request::Request;

use super::{require_no_receiver, require_no_values, DispatchResult};

/// Returns or updates the per-context libxml structured-error collection mode.
pub(super) fn use_internal_errors(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    let previous = context.internal_errors;
    if let Some(value) = request.values.first() {
        match value.tag {
            VALUE_NULL => {}
            VALUE_BOOL => {
                if value.payload0 > 1 {
                    return Err(());
                }
                context.internal_errors = value.payload0 == 1;
                if !context.internal_errors {
                    context.errors.clear();
                }
            }
            _ => return Err(()),
        }
    }
    Ok(DispatchResult::boolean(previous))
}

/// Returns the retained structured-error list as detached PHP value-object descriptors.
pub(super) fn get_errors(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    require_no_values(request)?;
    Ok(DispatchResult::libxml_errors(context.errors.clone()))
}

/// Returns a detached PHP value object for the last libxml error or false when none exists.
pub(super) fn get_last_error(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    require_no_values(request)?;
    let Some(error) = context.last_error.clone() else {
        return Ok(DispatchResult::boolean(false));
    };
    Ok(DispatchResult::libxml_error(error))
}

/// Clears per-context libxml structured and last-error state.
pub(super) fn clear_errors(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    require_no_values(request)?;
    context.errors.clear();
    context.last_error = None;
    Ok(DispatchResult::null())
}

/// Updates the deprecated entity-loader compatibility flag and returns its prior value.
pub(super) fn disable_entity_loader(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    let disable = if request.values.is_empty() {
        true
    } else {
        request.boolean(0)?
    };
    let previous = context.entity_loader_disabled;
    context.entity_loader_disabled = disable;
    Ok(DispatchResult::boolean(previous))
}

/// Retains or clears the PHP callable used for external entity resolution.
pub(super) fn set_external_entity_loader(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    let value = request.value(0)?;
    let descriptor = match value.tag {
        VALUE_NULL => None,
        VALUE_CALLABLE if value.payload0 != 0 => Some(value.payload0),
        _ => return Err(()),
    };
    if request.values.len() != 1 {
        return Err(());
    }
    match context.set_external_entity_loader(descriptor) {
        Ok(Some(action)) => {
            Ok(DispatchResult::boolean(true).with_pending_host_action(action))
        }
        Ok(None) => Ok(DispatchResult::boolean(true)),
        Err(HostCallError::PendingThrowable) => {
            Ok(DispatchResult::pending_host_throwable())
        }
        Err(HostCallError::Abi) => Err(()),
    }
}

/// Returns the current external entity loader with a fresh host-owned reference.
pub(super) fn get_external_entity_loader(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    require_no_values(request)?;
    let descriptor = match context.retained_external_entity_loader() {
        Ok(descriptor) => descriptor,
        Err(HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let Some(descriptor) = descriptor else {
        return Ok(DispatchResult::null());
    };
    Ok(DispatchResult::callable(descriptor))
}

/// Records the stream-context resource applied to subsequent libxml I/O callbacks.
pub(super) fn set_streams_context(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.value(0)?;
    if value.tag != VALUE_RESOURCE {
        return Err(());
    }
    context.stream_context = Some(value.payload0);
    Ok(DispatchResult::null())
}

/// Appends the six PHP-declared `LibXMLError` properties in Reflection declaration order.
pub(super) fn append_error_fields(
    error: &LibxmlErrorObject,
    values: &mut Vec<Value>,
    bytes: &mut Vec<u8>,
) {
    values.push(integer_value(error.level));
    values.push(integer_value(error.code));
    values.push(integer_value(error.column));
    values.push(byte_value(&error.message, bytes));
    values.push(byte_value(&error.file, bytes));
    values.push(integer_value(error.line));
}

/// Encodes one signed PHP integer into a flat result value.
fn integer_value(value: i64) -> Value {
    Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: value as u64,
        payload1: 0,
    }
}

/// Appends bytes to the result buffer and returns their length-delimited value descriptor.
fn byte_value(value: &[u8], bytes: &mut Vec<u8>) -> Value {
    let offset = bytes.len();
    bytes.extend_from_slice(value);
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: value.len() as u64,
    }
}

/// Updates last-error state and, when enabled, appends every structured parse error.
pub(super) fn record_errors(
    context: &mut Context,
    errors: &[LibxmlErrorObject],
) {
    if let Some(last) = errors.last() {
        context.last_error = Some(last.clone());
    }
    if context.internal_errors {
        context.errors.extend_from_slice(errors);
    }
}
