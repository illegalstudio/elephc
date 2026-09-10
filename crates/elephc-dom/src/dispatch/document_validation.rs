//! Purpose:
//! Implements DTD, W3C XML Schema, and Relax NG validation for DOM documents.
//! Maps pinned-libxml outcomes to PHP booleans, warnings, and internal errors.
//!
//! Called from:
//! - `super::routes::dispatch()` for legacy and modern validation methods.
//!
//! Key details:
//! - Modern namespace declarations are temporarily relinked inside the native adapter.
//! - Invalid grammar warnings remain visible even when libxml internal errors are enabled.

use std::rc::Rc;

use crate::abi::{STATUS_ABI_ERROR, STATUS_THROW};
use crate::context::Context;
use crate::objects::DocumentGraph;
use crate::request::Request;

use super::{document, libxml, DispatchResult};

/// Identifies one grammar engine and its optional XML Schema flags.
#[derive(Clone, Copy)]
pub(super) enum ValidationKind {
    Schema { flags: i32 },
    RelaxNg,
}

/// One callback-capable validation request detached from the DOM context borrow.
pub(super) struct PreparedFileValidation {
    graph: Rc<DocumentGraph>,
    path: Vec<u8>,
    method: &'static [u8],
    kind: ValidationKind,
    generic_errors: bool,
}

/// Either one runnable validation request or an already complete PHP result.
pub(super) enum FileValidationPreparation {
    Ready(PreparedFileValidation),
    Complete(DispatchResult),
}

/// Native validation output whose graph remains retained until dispatch finishes.
pub(super) struct ExecutedFileValidation {
    prepared: PreparedFileValidation,
    outcome: crate::native::ValidationOutcome,
}

/// One callback-capable in-memory validation request detached from the context.
pub(super) struct PreparedSourceValidation {
    graph: Rc<DocumentGraph>,
    source: Vec<u8>,
    method: &'static [u8],
    kind: ValidationKind,
    generic_errors: bool,
}

/// Either one runnable source validation or an already complete PHP result.
pub(super) enum SourceValidationPreparation {
    Ready(PreparedSourceValidation),
    Complete(DispatchResult),
}

/// Native source-validation output retained with its authoritative graph.
pub(super) struct ExecutedSourceValidation {
    prepared: PreparedSourceValidation,
    outcome: crate::native::ValidationOutcome,
}

/// Finalizes one native validation outcome using PHP's diagnostic policy.
fn finish_validation(
    context: &mut Context,
    outcome: crate::native::ValidationOutcome,
    method: &[u8],
    invalid_grammar_warning: Option<&[u8]>,
    invalid_file_warning: Option<&[u8]>,
    invalid_context_error: Option<&[u8]>,
) -> Result<DispatchResult, ()> {
    match u32::try_from(outcome.host_status).map_err(|_| ())? {
        0 => {}
        STATUS_THROW => return Ok(DispatchResult::pending_host_throwable()),
        STATUS_ABI_ERROR => return Err(()),
        _ => return Err(()),
    }
    libxml::record_errors(context, &outcome.errors);
    if outcome.status == 2 {
        return invalid_context_error
            .map(DispatchResult::error)
            .ok_or(());
    }
    if outcome.status == 3 {
        return invalid_file_warning
            .map(|warning| {
                with_method_warning(
                    DispatchResult::boolean(false),
                    method,
                    warning,
                )
            })
            .ok_or(());
    }
    if outcome.status != 0 && outcome.status != 1 {
        return Err(());
    }

    let emit_warnings = !context.internal_errors;
    let mut result = DispatchResult::boolean(outcome.valid);
    if emit_warnings {
        result = result.with_libxml_warnings(method, &outcome.errors);
    }
    if outcome.status == 1 {
        let warning = invalid_grammar_warning.ok_or(())?;
        result = with_method_warning(result, method, warning);
    }
    Ok(result)
}

/// Appends one canonical method-scoped PHP warning to an existing result.
fn with_method_warning(
    result: DispatchResult,
    method: &[u8],
    warning: &[u8],
) -> DispatchResult {
    let mut message = b"Warning: ".to_vec();
    message.extend_from_slice(method);
    message.extend_from_slice(b"(): ");
    message.extend_from_slice(warning);
    message.push(b'\n');
    result.with_warning(&message)
}

/// Validates a non-empty grammar path and builds PHP's exact `ValueError`.
fn validation_path<'a>(
    request: &'a Request,
    method: &[u8],
    maximum_arguments: usize,
) -> Result<Result<&'a [u8], DispatchResult>, ()> {
    if request.values.is_empty() || request.values.len() > maximum_arguments {
        return Err(());
    }
    let path = request.byte_string(0)?;
    let detail = if path.is_empty() {
        Some(b"must not be empty".as_slice())
    } else if path.contains(&0) {
        Some(b"must not contain any null bytes".as_slice())
    } else {
        None
    };
    if let Some(detail) = detail {
        let mut message = method.to_vec();
        message.extend_from_slice(b"(): Argument #1 ($filename) ");
        message.extend_from_slice(detail);
        return Ok(Err(DispatchResult::value_error(&message)));
    }
    Ok(Ok(path))
}

/// Validates a document against the DTD subset attached to its native graph.
pub(super) fn validate(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if !request.values.is_empty() {
        return Err(());
    }
    let pointer = document(context, request.header.receiver)?.pointer();
    let outcome = crate::native::document_validate(pointer)?;
    finish_validation(context, outcome, method, None, None, None)
}

/// Validates a document against one in-memory W3C XML Schema.
pub(super) fn schema_validate_source(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let source = request.byte_string(0)?;
    if source.is_empty() {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($source) must not be empty",
        );
        return Ok(DispatchResult::value_error(&message));
    }
    let flags = if request.values.len() == 2 {
        request.integer(1)? & 1
    } else {
        0
    };
    let pointer = document(context, request.header.receiver)?.pointer();
    let outcome = crate::native::document_schema_validate_source_with_host(
        pointer,
        source,
        flags as i32,
        !context.internal_errors,
        0,
    )?;
    finish_validation(
        context,
        outcome,
        method,
        Some(b"Invalid Schema"),
        None,
        Some(b"Invalid Schema Validation Context"),
    )
}

/// Validates a document against one local or PHP-stream W3C XML Schema.
pub(super) fn schema_validate_file(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    let path = match validation_path(request, method, 2)? {
        Ok(path) => path,
        Err(result) => return Ok(result),
    };
    let flags = if request.values.len() == 2 {
        request.integer(1)? & 1
    } else {
        0
    };
    let pointer = document(context, request.header.receiver)?.pointer();
    let outcome = crate::native::document_schema_validate_file(
        pointer,
        path,
        flags as i32,
        !context.internal_errors,
        0,
    )?;
    finish_validation(
        context,
        outcome,
        method,
        Some(b"Invalid Schema"),
        Some(b"Invalid Schema file source"),
        Some(b"Invalid Schema Validation Context"),
    )
}

/// Validates a document against one in-memory Relax NG grammar.
pub(super) fn relaxng_validate_source(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let source = request.byte_string(0)?;
    if source.is_empty() {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($source) must not be empty",
        );
        return Ok(DispatchResult::value_error(&message));
    }
    let pointer = document(context, request.header.receiver)?.pointer();
    let outcome = crate::native::document_relaxng_validate_source_with_host(
        pointer,
        source,
        !context.internal_errors,
        0,
    )?;
    finish_validation(
        context,
        outcome,
        method,
        Some(b"Invalid RelaxNG"),
        None,
        Some(b"Invalid RelaxNG Validation Context"),
    )
}

/// Validates a document against one local or PHP-stream Relax NG grammar.
pub(super) fn relaxng_validate_file(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    let path = match validation_path(request, method, 1)? {
        Ok(path) => path,
        Err(result) => return Ok(result),
    };
    let pointer = document(context, request.header.receiver)?.pointer();
    let outcome = crate::native::document_relaxng_validate_file(
        pointer,
        path,
        !context.internal_errors,
        0,
    )?;
    finish_validation(
        context,
        outcome,
        method,
        Some(b"Invalid RelaxNG"),
        Some(b"Invalid RelaxNG file source"),
        Some(b"Invalid RelaxNG Validation Context"),
    )
}

/// Snapshots one file-validation receiver before any PHP callback can re-enter DOM.
pub(super) fn prepare_file_validation(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<FileValidationPreparation, ()> {
    let (method, kind, maximum_arguments) = match operation_key {
        "method:domdocument::schemavalidate" => (
            b"DOMDocument::schemaValidate".as_slice(),
            ValidationKind::Schema {
                flags: if request.values.len() == 2 {
                    (request.integer(1)? & 1) as i32
                } else {
                    0
                },
            },
            2,
        ),
        "method:dom\\document::schemavalidate" => (
            b"Dom\\Document::schemaValidate".as_slice(),
            ValidationKind::Schema {
                flags: if request.values.len() == 2 {
                    (request.integer(1)? & 1) as i32
                } else {
                    0
                },
            },
            2,
        ),
        "method:domdocument::relaxngvalidate" => (
            b"DOMDocument::relaxNGValidate".as_slice(),
            ValidationKind::RelaxNg,
            1,
        ),
        "method:dom\\document::relaxngvalidate" => (
            b"Dom\\Document::relaxNgValidate".as_slice(),
            ValidationKind::RelaxNg,
            1,
        ),
        _ => return Err(()),
    };
    let path = match validation_path(request, method, maximum_arguments)? {
        Ok(path) => path.to_vec(),
        Err(result) => return Ok(FileValidationPreparation::Complete(result)),
    };
    let target = document(context, request.header.receiver)?;
    Ok(FileValidationPreparation::Ready(PreparedFileValidation {
        graph: target.graph(),
        path,
        method,
        kind,
        generic_errors: !context.internal_errors,
    }))
}

/// Runs callback-capable grammar loading without retaining a DOM context borrow.
pub(super) fn execute_file_validation(
    context_id: u64,
    prepared: PreparedFileValidation,
) -> Result<ExecutedFileValidation, ()> {
    let pointer = prepared.graph.pointer();
    let outcome = match prepared.kind {
        ValidationKind::Schema { flags } => {
            crate::native::document_schema_validate_file(
                pointer,
                &prepared.path,
                flags,
                prepared.generic_errors,
                context_id,
            )?
        }
        ValidationKind::RelaxNg => {
            crate::native::document_relaxng_validate_file(
                pointer,
                &prepared.path,
                prepared.generic_errors,
                context_id,
            )?
        }
    };
    Ok(ExecutedFileValidation { prepared, outcome })
}

/// Records errors and maps one callback-capable file validation to PHP.
pub(super) fn finish_file_validation(
    context: &mut Context,
    executed: ExecutedFileValidation,
) -> Result<DispatchResult, ()> {
    let (invalid_warning, context_error) = match executed.prepared.kind {
        ValidationKind::Schema { .. } => (
            b"Invalid Schema".as_slice(),
            b"Invalid Schema Validation Context".as_slice(),
        ),
        ValidationKind::RelaxNg => (
            b"Invalid RelaxNG".as_slice(),
            b"Invalid RelaxNG Validation Context".as_slice(),
        ),
    };
    finish_validation(
        context,
        executed.outcome,
        executed.prepared.method,
        Some(invalid_warning),
        Some(match executed.prepared.kind {
            ValidationKind::Schema { .. } => {
                b"Invalid Schema file source".as_slice()
            }
            ValidationKind::RelaxNg => {
                b"Invalid RelaxNG file source".as_slice()
            }
        }),
        Some(context_error),
    )
}

/// Snapshots one source-validation receiver before an external loader callback.
pub(super) fn prepare_source_validation(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<SourceValidationPreparation, ()> {
    let (method, kind, maximum_arguments) = match operation_key {
        "method:domdocument::schemavalidatesource" => (
            b"DOMDocument::schemaValidateSource".as_slice(),
            ValidationKind::Schema {
                flags: if request.values.len() == 2 {
                    (request.integer(1)? & 1) as i32
                } else {
                    0
                },
            },
            2,
        ),
        "method:dom\\document::schemavalidatesource" => (
            b"Dom\\Document::schemaValidateSource".as_slice(),
            ValidationKind::Schema {
                flags: if request.values.len() == 2 {
                    (request.integer(1)? & 1) as i32
                } else {
                    0
                },
            },
            2,
        ),
        "method:domdocument::relaxngvalidatesource" => (
            b"DOMDocument::relaxNGValidateSource".as_slice(),
            ValidationKind::RelaxNg,
            1,
        ),
        "method:dom\\document::relaxngvalidatesource" => (
            b"Dom\\Document::relaxNgValidateSource".as_slice(),
            ValidationKind::RelaxNg,
            1,
        ),
        _ => return Err(()),
    };
    if request.values.is_empty() || request.values.len() > maximum_arguments {
        return Err(());
    }
    let source = request.byte_string(0)?;
    if source.is_empty() {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($source) must not be empty",
        );
        return Ok(SourceValidationPreparation::Complete(
            DispatchResult::value_error(&message),
        ));
    }
    let target = document(context, request.header.receiver)?;
    Ok(SourceValidationPreparation::Ready(
        PreparedSourceValidation {
            graph: target.graph(),
            source: source.to_vec(),
            method,
            kind,
            generic_errors: !context.internal_errors,
        },
    ))
}

/// Runs in-memory grammar parsing without retaining a DOM context borrow.
pub(super) fn execute_source_validation(
    context_id: u64,
    prepared: PreparedSourceValidation,
) -> Result<ExecutedSourceValidation, ()> {
    let pointer = prepared.graph.pointer();
    let outcome = match prepared.kind {
        ValidationKind::Schema { flags } => {
            crate::native::document_schema_validate_source_with_host(
                pointer,
                &prepared.source,
                flags,
                prepared.generic_errors,
                context_id,
            )?
        }
        ValidationKind::RelaxNg => {
            crate::native::document_relaxng_validate_source_with_host(
                pointer,
                &prepared.source,
                prepared.generic_errors,
                context_id,
            )?
        }
    };
    Ok(ExecutedSourceValidation { prepared, outcome })
}

/// Records errors and maps one callback-capable source validation to PHP.
pub(super) fn finish_source_validation(
    context: &mut Context,
    executed: ExecutedSourceValidation,
) -> Result<DispatchResult, ()> {
    let (invalid_warning, context_error) = match executed.prepared.kind {
        ValidationKind::Schema { .. } => (
            b"Invalid Schema".as_slice(),
            b"Invalid Schema Validation Context".as_slice(),
        ),
        ValidationKind::RelaxNg => (
            b"Invalid RelaxNG".as_slice(),
            b"Invalid RelaxNG Validation Context".as_slice(),
        ),
    };
    finish_validation(
        context,
        executed.outcome,
        executed.prepared.method,
        Some(invalid_warning),
        None,
        Some(context_error),
    )
}
