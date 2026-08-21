//! Purpose:
//! Implements DOM document parsing and serialization through filesystem paths.
//! Owns path validation, file-URI normalization, and PHP file-result contracts.
//!
//! Called from:
//! - `super::routes::dispatch()` for legacy and modern document file methods.
//!
//! Key details:
//! - Supported targets preserve Unix path bytes without forcing UTF-8.
//! - A successful file parse records one canonical `file://` document URI.
//! - Registered wrapper reads and writes use the re-entrant no-unwind host ABI.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use crate::context::Context;
use crate::host::{
    emit_warning, flush_stream, open_stream, read_stream_lease,
    register_stream_lease, release_stream_lease, write_stream_chunk, HostCallError,
    StreamOpenFailure, StreamOpenResult, StreamWriteResult,
};
use crate::objects::{
    DocumentFamily, DocumentObject, NativeObject, HANDLE_DOCUMENT,
};
use crate::request::Request;

use super::{
    document, document_mut, dom_exception, libxml::record_errors,
    require_no_receiver, DispatchResult,
};

const MODERN_HTML_OPTIONS: u64 =
    32 | 65_536 | 8_192 | 2_147_483_648;
const MODERN_XML_OPTIONS: u64 = 1
    | 2
    | 4
    | 8
    | 16
    | 32
    | 64
    | 128
    | 256
    | 1_024
    | 2_048
    | 8_192
    | 16_384
    | 65_536
    | 524_288
    | 4_194_304
    | 8_388_608;

/// Converts PHP path bytes into one native path on supported Unix targets.
#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

/// Converts PHP path bytes losslessly when the host path representation is UTF-8 based.
#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// Borrows native path bytes for URI construction on supported Unix targets.
#[cfg(unix)]
fn path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

/// Borrows native path bytes for URI construction on non-Unix fallback targets.
#[cfg(not(unix))]
fn path_bytes(path: &Path) -> &[u8] {
    path.to_str().unwrap_or_default().as_bytes()
}

/// Decodes percent escapes used by `file://` paths while rejecting malformed escapes.
fn percent_decode(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = *bytes.get(index + 1)?;
        let low = *bytes.get(index + 2)?;
        let value = hex_digit(high)?.checked_mul(16)? + hex_digit(low)?;
        if value == 0 {
            return None;
        }
        decoded.push(value);
        index += 3;
    }
    Some(decoded)
}

/// Converts one ASCII hexadecimal digit into its numeric value.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Resolves plain paths and local `file://` URLs into filesystem paths.
pub(super) fn resolve_path(bytes: &[u8]) -> Option<PathBuf> {
    if bytes.contains(&0) {
        return None;
    }
    if let Some(rest) = bytes.strip_prefix(b"file://") {
        let local = rest
            .strip_prefix(b"localhost/")
            .map(|path| [&b"/"[..], path].concat())
            .unwrap_or_else(|| rest.to_vec());
        return percent_decode(&local).map(|path| path_from_bytes(&path));
    }
    if let Some(rest) = bytes.strip_prefix(b"file:") {
        return percent_decode(rest).map(|path| path_from_bytes(&path));
    }
    if bytes.windows(3).any(|window| window == b"://") {
        return None;
    }
    Some(path_from_bytes(bytes))
}

/// Percent-encodes one canonical native path as PHP's local file document URI.
fn canonical_file_uri(path: &Path) -> Vec<u8> {
    let canonical = std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf());
    let mut uri = b"file://".to_vec();
    for &byte in path_bytes(&canonical) {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~')
        {
            uri.push(byte);
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            uri.push(b'%');
            uri.push(HEX[(byte >> 4) as usize]);
            uri.push(HEX[(byte & 0x0f) as usize]);
        }
    }
    uri
}

/// Reads one local path or registered PHP stream and returns its URI and parser name.
pub(super) fn read_source(
    host: crate::context::Host,
    stream_context: Option<u64>,
    path: &[u8],
    method: &'static str,
) -> Result<Option<(Vec<u8>, Vec<u8>, Vec<u8>)>, HostCallError> {
    if let Some(resolved) = resolve_path(path) {
        let Some(source) = std::fs::read(&resolved).ok() else {
            return Ok(None);
        };
        let canonical = std::fs::canonicalize(&resolved)
            .unwrap_or_else(|_| resolved.clone());
        let uri = canonical_file_uri(&canonical);
        let input_name = path_bytes(&canonical).to_vec();
        return Ok(Some((source, uri, input_name)));
    }
    let stream = match open_stream(
        host,
        path,
        b"rb",
        stream_context,
        true,
    )? {
        StreamOpenResult::Opened(stream) => stream,
        StreamOpenResult::Failed(failure) => {
            emit_stream_open_warning(host, method, path, &failure)?;
            return Ok(None);
        }
    };
    let lease_id = register_stream_lease(stream);
    let mut source = Vec::new();
    loop {
        let chunk = match read_stream_lease(lease_id, 8_192) {
            Ok(chunk) => chunk,
            Err(error) => {
                let _ = release_stream_lease(lease_id);
                return Err(error);
            }
        };
        if chunk.is_empty() {
            break;
        }
        source.extend_from_slice(&chunk);
    }
    release_stream_lease(lease_id)?;
    Ok(Some((source, path.to_vec(), path.to_vec())))
}

/// Writes one serialized document through a plain or local-file-URL path.
fn write_output(path: &[u8], bytes: &[u8]) -> bool {
    resolve_path(path)
        .is_some_and(|resolved| std::fs::write(resolved, bytes).is_ok())
}

/// One validated serialized file payload ready for local or host-backed output.
pub(super) struct PreparedFile {
    pub(super) path: Vec<u8>,
    pub(super) bytes: Vec<u8>,
    pub(super) method: &'static str,
}

/// Either a serialized file payload or an already complete PHP-visible result.
pub(super) enum FilePreparation {
    Ready(PreparedFile),
    Complete(DispatchResult),
}

/// Returns whether one PHP path requires a registered stream-wrapper callback.
pub(super) fn requires_host_stream(path: &[u8]) -> bool {
    resolve_path(path).is_none()
}

/// Writes one prepared payload through PHP's stream layer without holding the DOM context borrow.
pub(super) fn write_host_stream(
    host: crate::context::Host,
    stream_context: Option<u64>,
    prepared: PreparedFile,
) -> Result<DispatchResult, ()> {
    let stream = match open_stream(
        host,
        &prepared.path,
        b"wb",
        stream_context,
        false,
    ) {
        Ok(stream) => stream,
        Err(error) => return host_failure(error),
    };
    let stream = match stream {
        StreamOpenResult::Opened(stream) => stream,
        StreamOpenResult::Failed(failure) => {
            if let Err(error) = emit_stream_open_warning(
                host,
                prepared.method,
                &prepared.path,
                &failure,
            ) {
                return host_failure(error);
            }
            return Ok(DispatchResult::boolean(false));
        }
    };
    let mut written = 0_usize;
    while written < prepared.bytes.len() {
        let outcome = match write_stream_chunk(&stream, &prepared.bytes[written..]) {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = stream.release();
                return host_failure(error);
            }
        };
        let count = match outcome {
            StreamWriteResult::Failed => {
                if let Err(error) = flush_stream(&stream) {
                    let _ = stream.release();
                    return host_failure(error);
                }
                if let Err(error) = stream.release() {
                    return host_failure(error);
                }
                return Ok(DispatchResult::boolean(false));
            }
            StreamWriteResult::Written(count) => count,
            StreamWriteResult::Oversized(reported) => {
                let remaining = prepared.bytes.len().checked_sub(written).ok_or(())?;
                let warning = oversized_write_warning(
                    prepared.method,
                    &stream.class_name,
                    reported,
                    remaining,
                )?;
                if let Err(error) = emit_warning(host, &warning) {
                    let _ = stream.release();
                    return host_failure(error);
                }
                remaining
            }
        };
        if count == 0 {
            if written != 0 {
                if let Err(error) = flush_stream(&stream) {
                    let _ = stream.release();
                    return host_failure(error);
                }
            }
            if let Err(error) = stream.release() {
                return host_failure(error);
            }
            return Ok(DispatchResult::integer(
                i64::try_from(written).map_err(|_| ())?,
            ));
        }
        written = written.checked_add(count).ok_or(())?;
    }
    if let Err(error) = flush_stream(&stream) {
        let _ = stream.release();
        return host_failure(error);
    }
    if let Err(error) = stream.release() {
        return host_failure(error);
    }
    Ok(DispatchResult::integer(
        i64::try_from(written).map_err(|_| ())?,
    ))
}

/// Formats php-src's warning when a wrapper reports more bytes than requested.
fn oversized_write_warning(
    method: &str,
    class_name: &[u8],
    reported: usize,
    requested: usize,
) -> Result<Vec<u8>, ()> {
    let excess = reported.checked_sub(requested).ok_or(())?;
    let mut warning = Vec::new();
    warning.extend_from_slice(b"Warning: ");
    warning.extend_from_slice(method.as_bytes());
    warning.extend_from_slice(b"(): ");
    warning.extend_from_slice(class_name);
    warning.extend_from_slice(b"::stream_write wrote ");
    warning.extend_from_slice(excess.to_string().as_bytes());
    warning.extend_from_slice(b" bytes more data than requested (");
    warning.extend_from_slice(reported.to_string().as_bytes());
    warning.extend_from_slice(b" written, ");
    warning.extend_from_slice(requested.to_string().as_bytes());
    warning.extend_from_slice(b" max)\n");
    Ok(warning)
}

/// Emits php-src's registered-wrapper warning for one failed stream open.
fn emit_stream_open_warning(
    host: crate::context::Host,
    method: &str,
    path: &[u8],
    failure: &StreamOpenFailure,
) -> Result<(), HostCallError> {
    let (class_name, suffix, include_path) = match failure {
        StreamOpenFailure::Silent => return Ok(()),
        StreamOpenFailure::MissingUrlStat(class_name) => (
            class_name.as_slice(),
            b"::url_stat is not implemented!\n".as_slice(),
            false,
        ),
        StreamOpenFailure::MissingStreamOpen(class_name) => (
            class_name.as_slice(),
            b"::stream_open\" is not implemented\n".as_slice(),
            true,
        ),
        StreamOpenFailure::StreamOpenFailed(class_name) => (
            class_name.as_slice(),
            b"::stream_open\" call failed\n".as_slice(),
            true,
        ),
    };
    let mut warning = Vec::new();
    warning.extend_from_slice(b"Warning: ");
    warning.extend_from_slice(method.as_bytes());
    warning.push(b'(');
    if include_path {
        warning.extend_from_slice(&sanitize_stream_warning_path(path));
    }
    warning.extend_from_slice(b"): ");
    if include_path {
        warning.extend_from_slice(b"Failed to open stream: \"");
    }
    warning.extend_from_slice(class_name);
    warning.extend_from_slice(suffix);
    emit_warning(host, &warning)
}

/// Removes URL credentials from a stream path using php-src's warning transform.
fn sanitize_stream_warning_path(path: &[u8]) -> Vec<u8> {
    let Some(marker) = path.windows(3).position(|window| window == b"://") else {
        return path.to_vec();
    };
    let credentials_start = marker + 3;
    let Some(relative_at) = path[credentials_start..]
        .iter()
        .position(|byte| *byte == b'@')
    else {
        return path.to_vec();
    };
    let at = credentials_start + relative_at;
    let mut sanitized = path[..credentials_start].to_vec();
    sanitized.extend(
        std::iter::repeat(b'.')
            .take(3.min(at.saturating_sub(credentials_start))),
    );
    sanitized.extend_from_slice(&path[at..]);
    sanitized
}

/// Maps host callback failures onto the bridge's re-entrant dispatch contract.
fn host_failure(error: HostCallError) -> Result<DispatchResult, ()> {
    match error {
        HostCallError::Abi => Err(()),
        HostCallError::PendingThrowable => {
            Ok(DispatchResult::pending_host_throwable())
        }
    }
}

/// Formats one scoped PHP argument `ValueError` message.
fn argument_value_error(
    method: &str,
    argument: &str,
    detail: &str,
) -> DispatchResult {
    let message = format!("{method}(): Argument {argument} {detail}");
    DispatchResult::value_error(message.as_bytes())
}

/// Builds PHP's base exception for a modern document file-open failure.
fn cannot_open_file(path: &[u8]) -> DispatchResult {
    let mut message = b"Cannot open file '".to_vec();
    message.extend_from_slice(path);
    message.push(b'\'');
    DispatchResult::exception(&message)
}

/// Inserts one newly parsed authoritative document graph into the bridge context.
fn insert_document(
    context: &mut Context,
    pointer: usize,
    family: DocumentFamily,
) -> DispatchResult {
    let handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(DocumentObject::new(pointer, family)),
    );
    context.document_handles.insert(pointer, handle);
    DispatchResult::bridge_handle(handle)
}

/// Replaces one legacy receiver graph after a successful file load.
fn replace_legacy_document(
    context: &mut Context,
    receiver: u64,
    pointer: usize,
) -> Result<(), ()> {
    let document = document_mut(context, receiver)?;
    let previous_pointer = document.pointer();
    document.replace_pointer(pointer);
    context.document_handles.remove(&previous_pointer);
    context.document_handles.insert(pointer, receiver);
    Ok(())
}

/// One registered-wrapper file-read operation prepared under the DOM context borrow.
#[derive(Clone, Copy)]
enum HostFileReadKind {
    ModernXml,
    ModernHtml,
    LegacyXml { receiver: u64 },
    LegacyHtml { receiver: u64 },
}

impl HostFileReadKind {
    /// Returns the PHP method name used by stream-open diagnostics.
    fn method(self) -> &'static str {
        match self {
            Self::ModernXml => "Dom\\XMLDocument::createFromFile",
            Self::ModernHtml => "Dom\\HTMLDocument::createFromFile",
            Self::LegacyXml { .. } => "DOMDocument::load",
            Self::LegacyHtml { .. } => "DOMDocument::loadHTMLFile",
        }
    }

    /// Returns the byte method name used by retained libxml diagnostics.
    fn warning_method(self) -> &'static [u8] {
        self.method().as_bytes()
    }

    /// Returns whether a failed open must become the modern base `Exception`.
    fn is_modern(self) -> bool {
        matches!(self, Self::ModernXml | Self::ModernHtml)
    }
}

/// Validated callback-capable file-read inputs that no longer borrow a request or context.
pub(super) struct PreparedHostFileRead {
    kind: HostFileReadKind,
    path: Vec<u8>,
    options: i64,
    override_encoding: Option<Vec<u8>>,
}

/// Preparation result for one registered-wrapper document read.
pub(super) enum HostFileReadPreparation {
    Ready(PreparedHostFileRead),
    Complete(DispatchResult),
}

/// Parsed registered-wrapper document graph ready to re-enter the bridge context.
pub(super) enum HostFileReadExecution {
    Parsed {
        prepared: PreparedHostFileRead,
        uri: Vec<u8>,
        outcome: crate::native::DocumentParseOutcome,
    },
    Complete(DispatchResult),
}

/// Validates one callback-capable document file operation while the context is borrowed.
pub(super) fn prepare_host_file_read(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<HostFileReadPreparation, ()> {
    let kind = match operation_key {
        "method:dom\\xmldocument::createfromfile" => {
            require_no_receiver(request)?;
            if request.values.is_empty() || request.values.len() > 3 {
                return Err(());
            }
            HostFileReadKind::ModernXml
        }
        "method:dom\\htmldocument::createfromfile" => {
            require_no_receiver(request)?;
            if request.values.is_empty() || request.values.len() > 3 {
                return Err(());
            }
            HostFileReadKind::ModernHtml
        }
        "method:domdocument::load" => {
            if request.values.is_empty() || request.values.len() > 2 {
                return Err(());
            }
            let target = document(context, request.header.receiver)?;
            if target.family() != DocumentFamily::Legacy {
                return Err(());
            }
            HostFileReadKind::LegacyXml {
                receiver: request.header.receiver,
            }
        }
        "method:domdocument::loadhtmlfile" => {
            if request.values.is_empty() || request.values.len() > 2 {
                return Err(());
            }
            let target = document(context, request.header.receiver)?;
            if target.family() != DocumentFamily::Legacy {
                return Err(());
            }
            HostFileReadKind::LegacyHtml {
                receiver: request.header.receiver,
            }
        }
        _ => return Err(()),
    };
    let path = request.byte_string(0)?.to_vec();
    if path.is_empty() {
        let result = match kind {
            HostFileReadKind::ModernHtml => {
                DispatchResult::value_error(b"Path must not be empty")
            }
            HostFileReadKind::ModernXml => argument_value_error(
                kind.method(),
                "#1 ($path)",
                "must not be empty",
            ),
            HostFileReadKind::LegacyXml { .. } => argument_value_error(
                kind.method(),
                "#1 ($filename)",
                "must not be empty",
            ),
            HostFileReadKind::LegacyHtml { .. } => argument_value_error(
                kind.method(),
                "#1 ($filename)",
                "must not be empty",
            ),
        };
        return Ok(HostFileReadPreparation::Complete(result));
    }
    if matches!(kind, HostFileReadKind::ModernXml)
        && path.windows(3).any(|window| window == b"%00")
    {
        return Ok(HostFileReadPreparation::Complete(argument_value_error(
            kind.method(),
            "#1 ($path)",
            "must not contain percent-encoded NUL bytes",
        )));
    }
    if matches!(kind, HostFileReadKind::ModernHtml)
        && path.windows(3).any(|window| window == b"%00")
    {
        return Ok(HostFileReadPreparation::Complete(argument_value_error(
            kind.method(),
            "#1 ($path)",
            "must not contain percent-encoded NUL bytes",
        )));
    }
    if matches!(kind, HostFileReadKind::LegacyHtml { .. })
        && path.contains(&0)
    {
        return Ok(HostFileReadPreparation::Complete(argument_value_error(
            kind.method(),
            "#1 ($filename)",
            "must not contain any null bytes",
        )));
    }

    let explicit_options = if request.values.len() > 1 {
        request.integer(1)?
    } else {
        0
    };
    let options = match kind {
        HostFileReadKind::ModernXml => {
            if (explicit_options as u64) & !MODERN_XML_OPTIONS != 0 {
                return Ok(HostFileReadPreparation::Complete(
                    argument_value_error(
                        kind.method(),
                        "#2 ($options)",
                        "contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)",
                    ),
                ));
            }
            explicit_options
        }
        HostFileReadKind::ModernHtml => {
            if (explicit_options as u64) & !MODERN_HTML_OPTIONS != 0 {
                return Ok(HostFileReadPreparation::Complete(
                    argument_value_error(
                        kind.method(),
                        "#2 ($options)",
                        "contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\\HTML_NO_DEFAULT_NS)",
                    ),
                ));
            }
            explicit_options
        }
        HostFileReadKind::LegacyXml { receiver } => {
            let target = document(context, receiver)?;
            i64::from(
                i32::try_from(explicit_options).map_err(|_| ())?
                    | target.legacy_parser_options(),
            )
        }
        HostFileReadKind::LegacyHtml { .. } => {
            i64::from(i32::try_from(explicit_options).map_err(|_| ())?)
        }
    };

    let override_encoding = if request.values.len() > 2 {
        request.optional_byte_string(2)?.map(<[u8]>::to_vec)
    } else {
        None
    };
    if matches!(kind, HostFileReadKind::ModernXml)
        && override_encoding
            .as_deref()
            .is_some_and(|encoding| !crate::native::encoding_is_valid(encoding))
    {
        return Ok(HostFileReadPreparation::Complete(argument_value_error(
            kind.method(),
            "#3 ($overrideEncoding)",
            "must be a valid document encoding",
        )));
    }
    if matches!(kind, HostFileReadKind::ModernHtml)
        && override_encoding
            .as_deref()
            .is_some_and(|encoding| !crate::native::html_encoding_is_valid(encoding))
    {
        return Ok(HostFileReadPreparation::Complete(argument_value_error(
            kind.method(),
            "#3 ($overrideEncoding)",
            "must be a valid document encoding",
        )));
    }
    Ok(HostFileReadPreparation::Ready(PreparedHostFileRead {
        kind,
        path,
        options,
        override_encoding,
    }))
}

/// Opens and parses one prepared registered-wrapper input without a DOM context borrow.
pub(super) fn execute_host_file_read(
    host: crate::context::Host,
    stream_context: Option<u64>,
    prepared: PreparedHostFileRead,
) -> Result<HostFileReadExecution, ()> {
    let (source, uri, input_name) = match read_source(
        host,
        stream_context,
        &prepared.path,
        prepared.kind.method(),
    ) {
        Ok(Some(source)) => source,
        Ok(None) => {
            let result = if prepared.kind.is_modern() {
                cannot_open_file(&prepared.path)
            } else {
                DispatchResult::boolean(false)
            };
            return Ok(HostFileReadExecution::Complete(result));
        }
        Err(HostCallError::PendingThrowable) => {
            return Ok(HostFileReadExecution::Complete(
                DispatchResult::pending_host_throwable(),
            ));
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let outcome = match prepared.kind {
        HostFileReadKind::ModernXml | HostFileReadKind::LegacyXml { .. } => {
            crate::native::document_parse_xml(
                &source,
                i32::try_from(prepared.options).map_err(|_| ())?,
                prepared.override_encoding.as_deref(),
                Some(&input_name),
            )?
        }
        HostFileReadKind::ModernHtml => {
            match crate::native::document_parse_html5(
                &source,
                prepared.options as u32,
                prepared.override_encoding.as_deref(),
                &prepared.path,
            ) {
                Ok(outcome) => outcome,
                Err(crate::native::HtmlParseError::InvalidEncoding) => {
                    return Ok(HostFileReadExecution::Complete(
                        argument_value_error(
                            prepared.kind.method(),
                            "#3 ($overrideEncoding)",
                            "must be a valid document encoding",
                        ),
                    ));
                }
                Err(crate::native::HtmlParseError::Allocation) => {
                    return Ok(HostFileReadExecution::Complete(dom_exception(11)));
                }
            }
        }
        HostFileReadKind::LegacyHtml { .. } => {
            crate::native::document_parse_html4(
                &source,
                i32::try_from(prepared.options).map_err(|_| ())?,
                Some(&input_name),
            )?
        }
    };
    Ok(HostFileReadExecution::Parsed {
        prepared,
        uri,
        outcome,
    })
}

/// Publishes one parsed registered-wrapper document after reacquiring the DOM context.
pub(super) fn finish_host_file_read(
    context: &mut Context,
    execution: HostFileReadExecution,
) -> Result<DispatchResult, ()> {
    let HostFileReadExecution::Parsed {
        prepared,
        uri,
        outcome,
    } = execution
    else {
        return Err(());
    };
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = if matches!(prepared.kind, HostFileReadKind::ModernXml) {
            dom_exception(12)
        } else if matches!(prepared.kind, HostFileReadKind::ModernHtml) {
            return Err(());
        } else {
            DispatchResult::boolean(false)
        };
        return Ok(if emit_warnings {
            if matches!(prepared.kind, HostFileReadKind::ModernHtml) {
                result.with_libxml_warnings(
                    prepared.kind.warning_method(),
                    &outcome.errors,
                )
            } else {
                result.with_libxml_parser_warnings(
                    prepared.kind.warning_method(),
                    &outcome.errors,
                    i32::try_from(prepared.options).map_err(|_| ())?,
                )
            }
        } else {
            result
        });
    };

    let result = match prepared.kind {
        HostFileReadKind::ModernXml => {
            let encoding =
                prepared.override_encoding.as_deref().unwrap_or(b"UTF-8");
            if crate::native::document_encoding(pointer).is_none()
                && crate::native::document_set_encoding(pointer, encoding) != 1
            {
                unsafe {
                    crate::native::document_free(pointer);
                }
                return Ok(dom_exception(11));
            }
            if !crate::native::document_convert_modern_xml(pointer)
                || !crate::native::document_set_url(pointer, &uri)
            {
                unsafe {
                    crate::native::document_free(pointer);
                }
                return Ok(dom_exception(11));
            }
            insert_document(context, pointer, DocumentFamily::ModernXml)
        }
        HostFileReadKind::ModernHtml => {
            if !crate::native::document_set_url(pointer, &uri) {
                unsafe {
                    crate::native::document_free(pointer);
                }
                return Ok(dom_exception(11));
            }
            insert_document(context, pointer, DocumentFamily::ModernHtml)
        }
        HostFileReadKind::LegacyXml { receiver }
        | HostFileReadKind::LegacyHtml { receiver } => {
            if !crate::native::document_set_url(pointer, &uri) {
                unsafe {
                    crate::native::document_free(pointer);
                }
                return Ok(DispatchResult::boolean(false));
            }
            replace_legacy_document(context, receiver, pointer)?;
            DispatchResult::boolean(true)
        }
    };
    Ok(if emit_warnings {
        if matches!(prepared.kind, HostFileReadKind::ModernHtml) {
            result.with_libxml_warnings(
                prepared.kind.warning_method(),
                &outcome.errors,
            )
        } else {
            result.with_libxml_parser_warnings(
                prepared.kind.warning_method(),
                &outcome.errors,
                i32::try_from(prepared.options).map_err(|_| ())?,
            )
        }
    } else {
        result
    })
}

/// Frees an uncommitted parsed document when the context cannot be reacquired.
pub(super) fn free_host_file_read(execution: HostFileReadExecution) {
    if let HostFileReadExecution::Parsed { outcome, .. } = execution {
        if let Some(pointer) = outcome.document {
            unsafe {
                crate::native::document_free(pointer);
            }
        }
    }
}

/// Parses one modern XML document from a filesystem path.
pub(super) fn create_modern_xml_from_file(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 3 {
        return Err(());
    }
    let path = request.byte_string(0)?;
    if path.is_empty() {
        return Ok(argument_value_error(
            "Dom\\XMLDocument::createFromFile",
            "#1 ($path)",
            "must not be empty",
        ));
    }
    if path.windows(3).any(|window| window == b"%00") {
        return Ok(argument_value_error(
            "Dom\\XMLDocument::createFromFile",
            "#1 ($path)",
            "must not contain percent-encoded NUL bytes",
        ));
    }
    let options = if request.values.len() > 1 {
        request.integer(1)? as u64
    } else {
        0
    };
    if options & !MODERN_XML_OPTIONS != 0 {
        return Ok(argument_value_error(
            "Dom\\XMLDocument::createFromFile",
            "#2 ($options)",
            "contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)",
        ));
    }
    let override_encoding = if request.values.len() > 2 {
        request.optional_byte_string(2)?
    } else {
        None
    };
    if override_encoding
        .is_some_and(|encoding| !crate::native::encoding_is_valid(encoding))
    {
        return Ok(argument_value_error(
            "Dom\\XMLDocument::createFromFile",
            "#3 ($overrideEncoding)",
            "must be a valid document encoding",
        ));
    }
    let (source, uri, input_name) = match read_source(
        context.host,
        context.stream_context,
        path,
        "Dom\\XMLDocument::createFromFile",
    ) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(cannot_open_file(path)),
        Err(HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let outcome = crate::native::document_parse_xml(
        &source,
        options as i32,
        override_encoding,
        Some(&input_name),
    )?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = dom_exception(12);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"Dom\\XMLDocument::createFromFile",
                &outcome.errors,
                options as i32,
            )
        } else {
            result
        });
    };
    let encoding = override_encoding.unwrap_or(b"UTF-8");
    if crate::native::document_encoding(pointer).is_none()
        && crate::native::document_set_encoding(pointer, encoding) != 1
    {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(dom_exception(11));
    }
    if !crate::native::document_convert_modern_xml(pointer)
        || !crate::native::document_set_url(pointer, &uri)
    {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(dom_exception(11));
    }
    let result = insert_document(
        context,
        pointer,
        DocumentFamily::ModernXml,
    );
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"Dom\\XMLDocument::createFromFile",
            &outcome.errors,
            options as i32,
        )
    } else {
        result
    })
}

/// Parses one modern HTML document from a filesystem path.
pub(super) fn create_modern_html_from_file(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 3 {
        return Err(());
    }
    let path = request.byte_string(0)?;
    if path.is_empty() {
        return Ok(DispatchResult::value_error(b"Path must not be empty"));
    }
    if path.windows(3).any(|window| window == b"%00") {
        return Ok(argument_value_error(
            "Dom\\HTMLDocument::createFromFile",
            "#1 ($path)",
            "must not contain percent-encoded NUL bytes",
        ));
    }
    let options = if request.values.len() > 1 {
        request.integer(1)? as u64
    } else {
        0
    };
    if options & !MODERN_HTML_OPTIONS != 0 {
        return Ok(argument_value_error(
            "Dom\\HTMLDocument::createFromFile",
            "#2 ($options)",
            "contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\\HTML_NO_DEFAULT_NS)",
        ));
    }
    let override_encoding = if request.values.len() > 2 {
        request.optional_byte_string(2)?
    } else {
        None
    };
    if override_encoding
        .is_some_and(|encoding| !crate::native::html_encoding_is_valid(encoding))
    {
        return Ok(argument_value_error(
            "Dom\\HTMLDocument::createFromFile",
            "#3 ($overrideEncoding)",
            "must be a valid document encoding",
        ));
    }
    let (source, uri, _input_name) = match read_source(
        context.host,
        context.stream_context,
        path,
        "Dom\\HTMLDocument::createFromFile",
    ) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(cannot_open_file(path)),
        Err(HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let outcome = match crate::native::document_parse_html5(
        &source,
        options as u32,
        override_encoding,
        path,
    ) {
        Ok(outcome) => outcome,
        Err(crate::native::HtmlParseError::InvalidEncoding) => {
            return Ok(argument_value_error(
                "Dom\\HTMLDocument::createFromFile",
                "#3 ($overrideEncoding)",
                "must be a valid document encoding",
            ));
        }
        Err(crate::native::HtmlParseError::Allocation) => {
            return Ok(dom_exception(11));
        }
    };
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let pointer = outcome.document.ok_or(())?;
    if !crate::native::document_set_url(pointer, &uri) {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(dom_exception(11));
    }
    let result = insert_document(
        context,
        pointer,
        DocumentFamily::ModernHtml,
    );
    Ok(if emit_warnings {
        result.with_libxml_warnings(
            b"Dom\\HTMLDocument::createFromFile",
            &outcome.errors,
        )
    } else {
        result
    })
}

/// Loads one XML file into an existing legacy `DOMDocument`.
pub(super) fn load_legacy_xml_file(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let path = request.byte_string(0)?;
    if path.is_empty() {
        return Ok(argument_value_error(
            "DOMDocument::load",
            "#1 ($filename)",
            "must not be empty",
        ));
    }
    let explicit_options = if request.values.len() > 1 {
        i32::try_from(request.integer(1)?).map_err(|_| ())?
    } else {
        0
    };
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let options = explicit_options | target.legacy_parser_options();
    let (source, uri, input_name) = match read_source(
        context.host,
        context.stream_context,
        path,
        "DOMDocument::load",
    ) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(DispatchResult::boolean(false)),
        Err(HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let outcome = crate::native::document_parse_xml(
        &source,
        options,
        None,
        Some(&input_name),
    )?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = DispatchResult::boolean(false);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"DOMDocument::load",
                &outcome.errors,
                options,
            )
        } else {
            result
        });
    };
    if !crate::native::document_set_url(pointer, &uri) {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(DispatchResult::boolean(false));
    }
    replace_legacy_document(context, request.header.receiver, pointer)?;
    let result = DispatchResult::boolean(true);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"DOMDocument::load",
            &outcome.errors,
            options,
        )
    } else {
        result
    })
}

/// Loads one HTML file into an existing legacy `DOMDocument`.
pub(super) fn load_legacy_html_file(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let path = request.byte_string(0)?;
    if path.is_empty() {
        return Ok(argument_value_error(
            "DOMDocument::loadHTMLFile",
            "#1 ($filename)",
            "must not be empty",
        ));
    }
    if path.contains(&0) {
        return Ok(argument_value_error(
            "DOMDocument::loadHTMLFile",
            "#1 ($filename)",
            "must not contain any null bytes",
        ));
    }
    let options = if request.values.len() > 1 {
        i32::try_from(request.integer(1)?).map_err(|_| ())?
    } else {
        0
    };
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let (source, uri, input_name) = match read_source(
        context.host,
        context.stream_context,
        path,
        "DOMDocument::loadHTMLFile",
    ) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(DispatchResult::boolean(false)),
        Err(HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(HostCallError::Abi) => return Err(()),
    };
    let outcome = crate::native::document_parse_html4(
        &source,
        options,
        Some(&input_name),
    )?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = DispatchResult::boolean(false);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"DOMDocument::loadHTMLFile",
                &outcome.errors,
                options,
            )
        } else {
            result
        });
    };
    if !crate::native::document_set_url(pointer, &uri) {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(DispatchResult::boolean(false));
    }
    replace_legacy_document(context, request.header.receiver, pointer)?;
    let result = DispatchResult::boolean(true);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"DOMDocument::loadHTMLFile",
            &outcome.errors,
            options,
        )
    } else {
        result
    })
}

/// Serializes a legacy or modern document as XML into one filesystem path.
pub(super) fn save_xml_file(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    match prepare_xml_file(context, request)? {
        FilePreparation::Ready(prepared) => {
            if !write_output(&prepared.path, &prepared.bytes) {
                return Ok(DispatchResult::boolean(false));
            }
            Ok(DispatchResult::integer(
                i64::try_from(prepared.bytes.len()).map_err(|_| ())?,
            ))
        }
        FilePreparation::Complete(result) => Ok(result),
    }
}

/// Validates and serializes one XML file operation without starting output callbacks.
pub(super) fn prepare_xml_file(
    context: &Context,
    request: &Request,
) -> Result<FilePreparation, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let path = request.byte_string(0)?.to_vec();
    let target = document(context, request.header.receiver)?;
    let method = match target.family() {
        DocumentFamily::Legacy => "DOMDocument::save",
        DocumentFamily::ModernXml => "Dom\\XMLDocument::saveXmlFile",
        DocumentFamily::ModernHtml => "Dom\\HTMLDocument::saveXmlFile",
    };
    if path.is_empty() {
        return Ok(FilePreparation::Complete(argument_value_error(
            method,
            "#1 ($filename)",
            "must not be empty",
        )));
    }
    let options = if request.values.len() > 1 {
        request.integer(1)? as i32
    } else {
        0
    };
    let mode = match target.family() {
        DocumentFamily::Legacy => 0,
        DocumentFamily::ModernXml => 1,
        DocumentFamily::ModernHtml => 2,
    };
    let Some(mut bytes) = crate::native::document_serialize(
        target.pointer(),
        None,
        target.format_output(),
        mode,
        options & crate::native::XML_SAVE_NO_EMPTY,
    ) else {
        return Ok(FilePreparation::Complete(DispatchResult::boolean(false)));
    };
    if target.family() != DocumentFamily::Legacy
        && crate::native::document_element(target.pointer()).is_some()
        && bytes.last() == Some(&b'\n')
    {
        bytes.pop();
    }
    Ok(FilePreparation::Ready(PreparedFile {
        path,
        bytes,
        method,
    }))
}

/// Serializes a legacy or modern HTML document into one filesystem path.
pub(super) fn save_html_file(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    match prepare_html_file(context, request)? {
        FilePreparation::Ready(prepared) => {
            if !write_output(&prepared.path, &prepared.bytes) {
                return Ok(DispatchResult::boolean(false));
            }
            Ok(DispatchResult::integer(
                i64::try_from(prepared.bytes.len()).map_err(|_| ())?,
            ))
        }
        FilePreparation::Complete(result) => Ok(result),
    }
}

/// Validates and serializes one HTML file operation without starting output callbacks.
pub(super) fn prepare_html_file(
    context: &Context,
    request: &Request,
) -> Result<FilePreparation, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let path = request.byte_string(0)?.to_vec();
    let target = document(context, request.header.receiver)?;
    let method = match target.family() {
        DocumentFamily::Legacy => "DOMDocument::saveHTMLFile",
        DocumentFamily::ModernHtml => "Dom\\HTMLDocument::saveHtmlFile",
        DocumentFamily::ModernXml => return Err(()),
    };
    if path.is_empty() {
        return Ok(FilePreparation::Complete(argument_value_error(
            method,
            "#1 ($filename)",
            "must not be empty",
        )));
    }
    let bytes = match target.family() {
        DocumentFamily::Legacy => crate::native::document_serialize_html4(
            target.pointer(),
            None,
            target.format_output(),
        ),
        DocumentFamily::ModernHtml => crate::native::document_serialize_html5(
            target.pointer(),
            None,
        ),
        DocumentFamily::ModernXml => None,
    };
    let Some(bytes) = bytes else {
        return Ok(FilePreparation::Complete(DispatchResult::boolean(false)));
    };
    Ok(FilePreparation::Ready(PreparedFile {
        path,
        bytes,
        method,
    }))
}
