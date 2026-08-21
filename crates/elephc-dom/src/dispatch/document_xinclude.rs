//! Purpose:
//! Implements legacy and modern XML-document XInclude processing.
//! Preserves PHP wrapper invalidation and libxml diagnostic behavior.
//!
//! Called from:
//! - `super::reentrant::dispatch()` for callback-capable public calls.
//! - `super::routes::dispatch()` for direct bridge-level dispatch.
//!
//! Key details:
//! - Native execution runs without retaining the per-thread context borrow.
//! - Every wrapper whose native node may be destroyed becomes invalid-state.

use std::rc::Rc;

use crate::abi::{STATUS_ABI_ERROR, STATUS_THROW};
use crate::context::Context;
use crate::objects::{DocumentFamily, DocumentGraph};
use crate::request::Request;

use super::{document, libxml, DispatchResult};

/// One XInclude request detached from the mutable bridge context.
pub(super) struct PreparedXInclude {
    graph: Rc<DocumentGraph>,
    flags: i32,
    method: &'static [u8],
    family: DocumentFamily,
    generic_errors: bool,
}

/// Either one runnable XInclude request or an already complete PHP result.
pub(super) enum XIncludePreparation {
    Ready(PreparedXInclude),
    Complete(DispatchResult),
}

/// Native XInclude output retained with the authoritative document graph.
pub(super) struct ExecutedXInclude {
    prepared: PreparedXInclude,
    outcome: crate::native::XIncludeOutcome,
}

/// Appends one exact legacy method warning to a completed result.
fn with_invalid_flags_warning(result: DispatchResult) -> DispatchResult {
    result.with_warning(
        b"Warning: DOMDocument::xinclude(): Invalid flags\n",
    )
}

/// Validates and snapshots one legacy or modern XInclude invocation.
pub(super) fn prepare(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<XIncludePreparation, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let (method, family) = match operation_key {
        "method:domdocument::xinclude" => (
            b"DOMDocument::xinclude".as_slice(),
            DocumentFamily::Legacy,
        ),
        "method:dom\\xmldocument::xinclude" => (
            b"Dom\\XMLDocument::xinclude".as_slice(),
            DocumentFamily::ModernXml,
        ),
        _ => return Err(()),
    };
    let flags = if request.values.is_empty() {
        0
    } else {
        let flags = request.integer(0)?;
        match i32::try_from(flags) {
            Ok(flags) => flags,
            Err(_) if family == DocumentFamily::Legacy => {
                return Ok(XIncludePreparation::Complete(
                    with_invalid_flags_warning(DispatchResult::boolean(false)),
                ));
            }
            Err(_) => {
                let mut message = method.to_vec();
                message.extend_from_slice(
                    b"(): Argument #1 ($options) is too large",
                );
                return Ok(XIncludePreparation::Complete(
                    DispatchResult::value_error(&message),
                ));
            }
        }
    };
    let target = document(context, request.header.receiver)?;
    if target.family() != family {
        return Err(());
    }
    Ok(XIncludePreparation::Ready(PreparedXInclude {
        graph: target.graph(),
        flags,
        method,
        family,
        generic_errors: !context.internal_errors,
    }))
}

/// Runs XInclude without retaining the context across PHP resource callbacks.
pub(super) fn execute(
    context_id: u64,
    prepared: PreparedXInclude,
) -> ExecutedXInclude {
    let outcome = crate::native::document_xinclude(
        prepared.graph.pointer(),
        prepared.flags,
        prepared.generic_errors,
        context_id,
    );
    ExecutedXInclude { prepared, outcome }
}

/// Publishes invalid wrappers, errors, and the family-specific PHP result.
pub(super) fn finish(
    context: &mut Context,
    executed: ExecutedXInclude,
) -> Result<DispatchResult, ()> {
    context.invalidate_node_pointers(&executed.outcome.invalidated);
    match u32::try_from(executed.outcome.host_status).map_err(|_| ())? {
        0 => {}
        STATUS_THROW => return Ok(DispatchResult::pending_host_throwable()),
        STATUS_ABI_ERROR => return Err(()),
        _ => return Err(()),
    }
    if executed.outcome.allocation_failed {
        return Err(());
    }
    libxml::record_errors(context, &executed.outcome.errors);
    let mut result = match executed.prepared.family {
        DocumentFamily::Legacy if executed.outcome.substitutions == 0 => {
            DispatchResult::boolean(false)
        }
        DocumentFamily::Legacy => {
            DispatchResult::integer(executed.outcome.substitutions.into())
        }
        DocumentFamily::ModernXml
            if executed.outcome.substitutions < 0 =>
        {
            DispatchResult::dom_exception(
                13,
                b"Invalid Modification Error",
            )
        }
        DocumentFamily::ModernXml => {
            DispatchResult::integer(executed.outcome.substitutions.into())
        }
        DocumentFamily::ModernHtml => return Err(()),
    };
    if executed.prepared.generic_errors {
        result = result.with_libxml_warnings(
            executed.prepared.method,
            &executed.outcome.errors,
        );
    }
    Ok(result)
}

/// Executes a non-reentrant bridge-level XInclude path for direct route tests.
pub(super) fn direct(
    context: &mut Context,
    operation_key: &str,
    request: &Request,
) -> Result<DispatchResult, ()> {
    match prepare(context, operation_key, request)? {
        XIncludePreparation::Ready(prepared) => {
            let executed = execute(0, prepared);
            finish(context, executed)
        }
        XIncludePreparation::Complete(result) => Ok(result),
    }
}
