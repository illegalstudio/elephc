//! Purpose:
//! Formats and submits multibyte mail with the current request language settings.
//!
//! Called from:
//! - The shared mbstring ABI after contract coercion and array snapshotting.
//!
//! Key details:
//! - MIME conversion uses the existing codec catalog with PHP's mail substitution policy.
//! - Array headers use PHP's field-name, value, and key validation before delivery.
//! - The transport receives arguments directly, so header and parameter bytes never enter a shell.
//! - Failed process creation and nonzero exits return false with a PHP warning.

use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Stdio};

use elephc_builtin_contract::mbstring_abi::array::{Key, Value};

use super::{ARG_ARRAY, Arguments, MbError, Outcome, State};
use crate::encoding::{Encoding, Substitute};

/// Message bytes and PHP warnings determined before the transport runs.
pub(super) struct PreparedMail { pub(super) bytes: Vec<u8>, pub(super) warnings: Vec<Vec<u8>> }

/// Formats the message before opening a transport and returns the delivery status.
pub(super) fn dispatch(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let prepared = match prepare(args, state) {
        Ok(prepared) => prepared,
        Err(error) => return Outcome::error(error),
    };
    let mut diagnostics = Vec::new();
    for warning in &prepared.warnings {
        diagnostics.extend_from_slice(b"Warning: ");
        diagnostics.extend_from_slice(warning);
        diagnostics.push(b'\n');
    }
    let mut result = send(prepared.bytes, state, args.nullable_string(4));
    diagnostics.append(&mut result.diagnostics);
    result.diagnostics = diagnostics;
    result
}

/// Runs an already formatted message using direct process arguments.
pub(super) fn send(message: Vec<u8>, state: &State, additional: Option<&[u8]>) -> Outcome {
    let mut command = match command(state, additional) {
        Ok(command) => command,
        Err(error) => return Outcome::error(error),
    };
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    let code = match command.spawn() {
        Ok(mut child) => {
            let written = child.stdin.take().is_some_and(|mut input| input.write_all(&message).is_ok());
            match child.wait() {
                Ok(status) if written && status.success() => return Outcome::boolean(true),
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        }
        Err(_) => 127,
    };
    Outcome {
        diagnostics: format!("Warning: mb_send_mail(): Sendmail exited with non-zero exit code {code}\n").into_bytes(),
        ..Outcome::boolean(false)
    }
}

/// Validates path-like PHP parameters and creates a process without shell interpretation.
fn command(state: &State, additional: Option<&[u8]>) -> Result<Command, MbError> {
    let mut parts = state.mail_command().split(|byte| byte.is_ascii_whitespace()).filter(|part| !part.is_empty());
    let Some(path) = parts.next() else { return Err(MbError::Runtime("mb_send_mail(): No mail transport is configured".into())); };
    let mut command = Command::new(OsString::from_vec(path.to_vec()));
    command.args(parts.map(|part| OsString::from_vec(part.to_vec())));
    if let Some(additional) = additional { no_nul(additional, 5, "additional_params")?; }
    if let Some(parameters) = state.mail_force_extra_parameters().or(additional) {
        command.args(parameters.split(|byte| byte.is_ascii_whitespace()).filter(|part| !part.is_empty())
            .map(|part| OsString::from_vec(part.to_vec())));
    }
    Ok(command)
}

/// Encodes PHP's subject and body and appends absent MIME headers in source order.
pub(super) fn prepare(args: &Arguments<'_>, state: &State) -> Result<PreparedMail, MbError> {
    let to = args.string(0);
    let subject = args.string(1);
    let body = args.string(2);
    for (index, name, bytes) in [(1, "to", to), (2, "subject", subject), (3, "message", body)] {
        no_nul(bytes, index, name)?;
    }
    let mut headers = headers(args)?;
    let mut warnings = Vec::new();
    let [mut charset, header_encoding, mut body_encoding] = state.language().mail_encodings();
    let has_content_type = headers.iter().any(|(name, _)| name.eq_ignore_ascii_case(b"content-type"));
    let has_transfer = headers.iter().any(|(name, _)| name.eq_ignore_ascii_case(b"content-transfer-encoding"));
    let content_type = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case(b"content-type"));
    if let Some((_, value)) = content_type {
        if let Some(name) = value.split(|&byte| byte == b';').skip(1).find_map(|piece| {
            let (key, value) = split_once_byte(piece, b'=')?;
            key.trim_ascii().eq_ignore_ascii_case(b"charset").then_some(value.trim_ascii())
        }) {
            let name = name.strip_prefix(b"\"").unwrap_or(name);
            let name = name.strip_suffix(b"\"").unwrap_or(name);
            if !name.is_empty() {
                charset = Encoding::lookup(name).unwrap_or_else(|| {
                    warnings.push(unsupported(b"charset", name, b"ascii"));
                    Encoding::lookup(b"ASCII").expect("ASCII codec")
                });
            }
        }
    }
    let transfer = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case(b"content-transfer-encoding"));
    if let Some((_, value)) = transfer {
        body_encoding = Encoding::lookup(value.trim_ascii()).filter(|encoding|
            ["BASE64", "7bit", "8bit"].iter().any(|name| encoding.name().eq_ignore_ascii_case(name)))
            .unwrap_or_else(|| {
                warnings.push(unsupported(b"transfer encoding", value.trim_ascii(), b"8bit"));
                Encoding::lookup(b"8bit").expect("8bit codec")
            });
    }
    if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case(b"mime-version")) {
        headers.push((b"MIME-Version".to_vec(), b"1.0".to_vec()));
    }
    if !has_content_type {
        let mut value = b"text/plain".to_vec();
        if let Some(name) = charset.mime_name() {
            value.extend_from_slice(b"; charset=");
            value.extend_from_slice(name.as_bytes());
        }
        headers.push((b"Content-Type".to_vec(), value));
    }
    if !has_transfer {
        headers.push((b"Content-Transfer-Encoding".to_vec(),
            body_encoding.mime_name().unwrap_or("7bit").as_bytes().to_vec()));
    }
    let line_sep = state.mail_line_separator();
    let mut result = Vec::new();
    result.extend_from_slice(b"To: ");
    result.extend_from_slice(&safe_to(to.trim_ascii_end()));
    result.extend_from_slice(line_sep);
    result.extend_from_slice(b"Subject: ");
    result.extend_from_slice(&crate::mime::encode_header(subject, state.internal_encoding(), charset,
        header_encoding.name().eq_ignore_ascii_case("BASE64"), line_sep,
        (b"Subject: [PHP-jp nnnnnnnn]".len() + line_sep.len()) as i64));
    result.extend_from_slice(line_sep);
    for (name, value) in headers {
        result.extend_from_slice(&name);
        result.extend_from_slice(b": ");
        result.extend_from_slice(&value);
        result.extend_from_slice(line_sep);
    }
    result.extend_from_slice(line_sep);
    let text = charset.encode_conversion(body, state.internal_encoding(), Substitute::default());
    let raw = Encoding::lookup(b"8bit").expect("8bit codec");
    result.extend_from_slice(&body_encoding.encode_conversion(&text, raw, Substitute::default()));
    result.extend_from_slice(line_sep);
    Ok(PreparedMail { bytes: result, warnings })
}

/// Formats PHP's unsupported mail header warning without changing non-UTF-8 bytes.
fn unsupported(kind: &[u8], value: &[u8], fallback: &[u8]) -> Vec<u8> {
    let mut warning = b"mb_send_mail(): Unsupported ".to_vec();
    warning.extend_from_slice(kind);
    warning.extend_from_slice(b" \"");
    warning.extend_from_slice(value);
    warning.extend_from_slice(b"\" - will be regarded as ");
    warning.extend_from_slice(fallback);
    warning
}

/// Keeps RFC 822 folded continuations while neutralizing other control bytes.
fn safe_to(to: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(to.len());
    let mut index = 0;
    while index < to.len() {
        if to[index..].starts_with(b"\r\n") && to.get(index + 2).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
            result.extend_from_slice(b"\r\n");
            index += 2;
            while to.get(index).is_some_and(|byte| matches!(byte, b' ' | b'\t')) {
                result.push(to[index]);
                index += 1;
            }
        } else {
            result.push(if to[index].is_ascii_control() { b' ' } else { to[index] });
            index += 1;
        }
    }
    result
}

/// Parses string headers and applies PHP's structured array-header checks.
fn headers(args: &Arguments<'_>) -> Result<Vec<(Vec<u8>, Vec<u8>)>, MbError> {
    if !args.supplied(3) { return Ok(Vec::new()); }
    if args.kind(3) != ARG_ARRAY {
        let bytes = args.string(3);
        no_nul(bytes, 4, "additional_headers")?;
        return bytes.trim_ascii().split(|&byte| byte == b'\n').filter(|line| !line.is_empty()).map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let Some((name, value)) = split_once_byte(line, b':') else {
                return Err(MbError::argument("mb_send_mail", 4, "additional_headers", "contains a header without a colon"));
            };
            Ok((name.trim_ascii().to_vec(), value.trim_ascii().to_vec()))
        }).collect();
    }
    let graph = args.array(3).expect("validated mail header array");
    let mut result = Vec::new();
    for (key, value) in &graph.arrays()[graph.root()] {
        let name = match key {
            Key::String(name) => name,
            Key::Int(index) => return Err(MbError::TypeBytes(
                format!("Header name cannot be numeric, {index} given").into_bytes())),
        };
        if name.eq_ignore_ascii_case(b"to") || name.eq_ignore_ascii_case(b"subject") {
            let reserved = if name.eq_ignore_ascii_case(b"to") { "To" } else { "Subject" };
            return Err(MbError::Value(format!("The additional headers cannot contain the \"{reserved}\" header")));
        }
        match value {
            Value::String(value) => checked_header(&mut result, name, value)?,
            Value::Array(index) => {
                if let Some(standard) = single_value_header(name) {
                    return Err(MbError::TypeBytes(format!("Header \"{standard}\" must be of type string, array given").into_bytes()));
                }
                for (nested_key, nested_value) in &graph.arrays()[*index] {
                    if let Key::String(nested_name) = nested_key {
                        return Err(MbError::TypeBytes(named_header(b"Header ", name,
                            &[b"\" must only contain numeric keys, \"", php_c_string(nested_name), b"\" found"].concat())));
                    }
                    let Value::String(nested_value) = nested_value else {
                        return Err(MbError::TypeBytes(named_header(b"Header ", name,
                            &[b"\" must only contain values of type string, ", php_value_name(nested_value), b" found"].concat())));
                    };
                    checked_header(&mut result, name, nested_value)?;
                }
            }
            other => return Err(MbError::TypeBytes(named_header(b"Header ", name,
                &[b"\" must be of type array|string, ", php_value_name(other), b" given"].concat()))),
        }
    }
    Ok(result)
}

/// Appends one already typed header after checking its name and folded value.
fn checked_header(headers: &mut Vec<(Vec<u8>, Vec<u8>)>, name: &[u8], value: &[u8]) -> Result<(), MbError> {
    if name.iter().any(|byte| *byte < 33 || *byte > 126 || *byte == b':') {
        return Err(MbError::ValueBytes(named_header(b"Header name ", name, b"\" contains invalid characters")));
    }
    let mut index = 0;
    while index < value.len() {
        match value[index] {
            b'\r' if value.get(index + 1) != Some(&b'\n') => return Err(header_value_error(name,
                b" contains CR character that is not allowed in the header")),
            b'\r' if matches!(value.get(index + 2), Some(b' ' | b'\t')) => index += 3,
            b'\r' => return Err(header_value_error(name,
                b" contains CRLF characters that are used as a line separator and are not allowed in the header")),
            b'\n' if matches!(value.get(index + 1), Some(b' ' | b'\t')) => index += 2,
            b'\n' => return Err(header_value_error(name,
                b" contains LF character that is not allowed in the header")),
            0 => return Err(header_value_error(name,
                b" contains NULL character that is not allowed in the header")),
            _ => index += 1,
        }
    }
    headers.push((name.to_vec(), value.to_vec()));
    Ok(())
}

/// Identifies standard fields for which PHP forbids a list of repeated values.
fn single_value_header(name: &[u8]) -> Option<&'static str> {
    ["orig-date", "from", "sender", "reply-to", "cc", "bcc", "message-id", "references", "in-reply-to"]
        .into_iter().find(|field| name.eq_ignore_ascii_case(field.as_bytes()))
}

/// Builds a binary-safe PHP error around a NUL-terminated displayed header name.
fn named_header(prefix: &[u8], name: &[u8], suffix: &[u8]) -> Vec<u8> {
    [prefix, b"\"", php_c_string(name), suffix].concat()
}

/// Formats an invalid header value without losing non-UTF-8 field-name bytes.
fn header_value_error(name: &[u8], suffix: &[u8]) -> MbError {
    MbError::ValueBytes(named_header(b"Header ", name, &[b"\"", suffix].concat()))
}

/// Matches the PHP value names used in structured-header TypeErrors.
fn php_value_name(value: &Value) -> &'static [u8] {
    match value {
        Value::Null => b"null", Value::Bool(false) => b"false", Value::Bool(true) => b"true",
        Value::Int(_) => b"int", Value::Float(_) => b"float", Value::String(_) => b"string",
        Value::Array(_) => b"array", Value::Unsupported => b"object",
    }
}

/// Stops `%s`-style PHP diagnostics at the first embedded NUL byte.
fn php_c_string(value: &[u8]) -> &[u8] { value.split(|byte| *byte == 0).next().unwrap_or_default() }

/// Emits PHP's path/string NUL diagnostic before any transport is opened.
fn no_nul(bytes: &[u8], index: usize, name: &str) -> Result<(), MbError> {
    if bytes.contains(&0) { Err(MbError::argument("mb_send_mail", index, name, "must not contain any null bytes")) }
    else { Ok(()) }
}

/// Splits a byte header at its first delimiter without relying on unstable slice APIs.
fn split_once_byte(bytes: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|&byte| byte == delimiter)?;
    Some((&bytes[..index], &bytes[index + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::{MbArgV1, array::ArrayGraph}};

    /// Prepares one structured header graph without opening a configured transport.
    fn array_mail(graph: ArrayGraph) -> Result<PreparedMail, MbError> {
        let encoded = graph.encode();
        let slots = [MbArgV1::string(b"to@example.test"), MbArgV1::string(b"S"),
            MbArgV1::string(b"body"), MbArgV1::array(&encoded)];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        prepare(&args, &State::default())
    }

    /// Rejects each forbidden line break and NUL with PHP's field-specific ValueError.
    #[test]
    fn mail_array_header_values_match_php_validation() {
        for (value, detail) in [
            (b"hello\r\nBcc: attacker@example.test".as_slice(),
                b"CRLF characters that are used as a line separator and are not allowed in the header".as_slice()),
            (b"hello\nBcc: attacker@example.test", b"LF character that is not allowed in the header"),
            (b"hello\rBcc: attacker@example.test", b"CR character that is not allowed in the header"),
            (b"hello\0Bcc: attacker@example.test", b"NULL character that is not allowed in the header"),
        ] {
            let graph = ArrayGraph::new(0, vec![vec![(Key::String(b"Reply-To".to_vec()),
                Value::String(value.to_vec()))]]).expect("header graph");
            let expected = [b"Header \"Reply-To\" contains ".as_slice(), detail].concat();
            assert_eq!(array_mail(graph).err(), Some(MbError::ValueBytes(expected)));
        }
        let graph = ArrayGraph::new(0, vec![vec![(Key::String(b"X-Test".to_vec()),
            Value::String(b"hello\r\n folded\n\tmore".to_vec()))]]).expect("folded header");
        assert!(array_mail(graph).expect("folded header").bytes.windows(b"X-Test: hello\r\n folded\n\tmore".len())
            .any(|part| part == b"X-Test: hello\r\n folded\n\tmore"));
    }

    /// Rejects invalid names, numeric keys, reserved fields, and typed array members.
    #[test]
    fn mail_array_header_names_and_types_match_php_validation() {
        for (key, value, error) in [
            (Key::String(b"Bad Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad Name\" contains invalid characters".to_vec())),
            (Key::String(b"Bad:Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad:Name\" contains invalid characters".to_vec())),
            (Key::String(b"Bad\0Name".to_vec()), Value::String(b"value".to_vec()),
                MbError::ValueBytes(b"Header name \"Bad\" contains invalid characters".to_vec())),
            (Key::Int(42), Value::String(b"X-Test: value".to_vec()),
                MbError::TypeBytes(b"Header name cannot be numeric, 42 given".to_vec())),
            (Key::String(b"To".to_vec()), Value::String(b"other@example.test".to_vec()),
                MbError::Value("The additional headers cannot contain the \"To\" header".into())),
            (Key::String(b"SUBJECT".to_vec()), Value::String(b"new".to_vec()),
                MbError::Value("The additional headers cannot contain the \"Subject\" header".into())),
            (Key::String(b"X-Test".to_vec()), Value::Int(12),
                MbError::TypeBytes(b"Header \"X-Test\" must be of type array|string, int given".to_vec())),
            (Key::String(b"Cc".to_vec()), Value::Array(1),
                MbError::TypeBytes(b"Header \"cc\" must be of type string, array given".to_vec())),
        ] {
            let graph = ArrayGraph::new(0, vec![vec![(key, value)], vec![]]).expect("header graph");
            assert_eq!(array_mail(graph).err(), Some(error));
        }
        let invalid_key = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::String(b"named".to_vec()), Value::String(b"one".to_vec()))],
        ]).expect("nested header");
        assert_eq!(array_mail(invalid_key).err(), Some(MbError::TypeBytes(
            b"Header \"X-Test\" must only contain numeric keys, \"named\" found".to_vec())));
        let invalid_value = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::Int(12))],
        ]).expect("nested header");
        assert_eq!(array_mail(invalid_value).err(), Some(MbError::TypeBytes(
            b"Header \"X-Test\" must only contain values of type string, int found".to_vec())));
        let nested_injection = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::String(b"one\r\nBcc: attacker@example.test".to_vec()))],
        ]).expect("nested header");
        assert_eq!(array_mail(nested_injection).err(), Some(MbError::ValueBytes(
            b"Header \"X-Test\" contains CRLF characters that are used as a line separator and are not allowed in the header".to_vec())));
        let repeated = ArrayGraph::new(0, vec![
            vec![(Key::String(b"X-Test".to_vec()), Value::Array(1))],
            vec![(Key::Int(0), Value::String(b"one".to_vec())),
                (Key::Int(1), Value::String(b"two".to_vec()))],
        ]).expect("nested header");
        assert!(array_mail(repeated).expect("repeated header").bytes.windows(b"X-Test: one\r\nX-Test: two\r\n".len())
            .any(|part| part == b"X-Test: one\r\nX-Test: two\r\n"));
    }

    /// Matches PHP's neutral-language MIME bytes for non-ASCII subject and body text.
    #[test]
    fn neutral_mail_matches_php_mime_output() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string("Crème".as_bytes())];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let message = prepare(&args, &State::default()).expect("MIME message");
        assert_eq!(message.bytes, b"To: a@example.test\r\nSubject: =?UTF-8?B?Q2Fmw6k=?=\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: BASE64\r\n\r\nQ3LDqG1l\r\n");
        assert!(message.warnings.is_empty());
    }

    /// Preserves valid RFC 822 folding while replacing other control bytes in To.
    #[test]
    fn mail_to_keeps_folded_continuations() {
        let slots = [MbArgV1::string(b"a@example.test\r\n \tCc: b@example.test\nInjected: z"),
            MbArgV1::string(b"S"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let message = prepare(&args, &State::default()).expect("MIME message");
        assert!(message.bytes.starts_with(b"To: a@example.test\r\n \tCc: b@example.test Injected: z\r\nSubject: S\r\n"));
    }

    /// Applies PHP's legacy mixed line-feed setting to the complete mail envelope.
    #[test]
    fn mail_mixed_line_endings_match_php() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string("Crème".as_bytes())];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let (state, diagnostics) = State::with_ini_configuration(&[(b"mail.mixed_lf_and_crlf".to_vec(), b"1".to_vec())],
            crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        assert!(diagnostics.is_empty());
        let message = prepare(&args, &state).expect("MIME message");
        assert_eq!(message.bytes, b"To: a@example.test\nSubject: =?UTF-8?B?Q2Fmw6k=?=\nMIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: BASE64\n\nQ3LDqG1l\n");
    }

    /// Gives the configured extra parameters precedence without interpreting shell syntax.
    #[test]
    fn mail_force_parameters_replace_call_arguments() {
        let (state, diagnostics) = State::with_ini_configuration(&[
            (b"sendmail_path".to_vec(), b"/bin/true -t".to_vec()),
            (b"mail.force_extra_parameters".to_vec(), b"-f forced@example.test".to_vec()),
        ], crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        assert!(diagnostics.is_empty());
        let configured = command(&state, Some(b"-f caller@example.test")).expect("mail command");
        assert_eq!(configured.get_program(), "/bin/true");
        assert_eq!(configured.get_args().collect::<Vec<_>>(), ["-t", "-f", "forced@example.test"]);
        assert_eq!(command(&state, Some(b"bad\0parameter")).err(),
            Some(MbError::argument("mb_send_mail", 5, "additional_params", "must not contain any null bytes")));
    }

    /// Keeps shell punctuation literal in forced parameters rather than invoking a shell.
    #[test]
    fn mail_force_parameters_use_literal_argv() {
        let (state, _) = State::with_ini_configuration(&[
            (b"sendmail_path".to_vec(), b"/bin/true".to_vec()),
            (b"mail.force_extra_parameters".to_vec(), b"'-f quoted@example.test' ; echo".to_vec()),
        ], crate::state::CoreEncodingDefaults::default(), |_| Ok(()));
        let command = command(&state, None).expect("mail command");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["'-f", "quoted@example.test'", ";", "echo"]);
    }

    /// Matches both PHP warnings and the fallback MIME conversion order.
    #[test]
    fn mail_unsupported_headers_warn_before_delivery() {
        use elephc_builtin_contract::mbstring_abi::array::ArrayGraph;
        let graph = ArrayGraph::new(0, vec![vec![
            (Key::String(b"Content-Type".to_vec()), Value::String(b"text/plain; charset=X-NOPE".to_vec())),
            (Key::String(b"Content-Transfer-Encoding".to_vec()), Value::String(b"X-NOPE".to_vec())),
        ]]).expect("headers");
        let encoded = graph.encode();
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string(b"body"), MbArgV1::array(&encoded)];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let prepared = prepare(&args, &State::default()).expect("MIME message");
        assert_eq!(prepared.warnings, [
            b"mb_send_mail(): Unsupported charset \"X-NOPE\" - will be regarded as ascii".to_vec(),
            b"mb_send_mail(): Unsupported transfer encoding \"X-NOPE\" - will be regarded as 8bit".to_vec(),
        ]);
        assert!(prepared.bytes.starts_with(b"To: a@example.test\r\nSubject: =?US-ASCII?B?Q2FmPw==?=\r\n"));
    }

    /// Rejects embedded NULs before opening a mail transport.
    #[test]
    fn mail_rejects_nul_subject() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"a\0b"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        assert!(matches!(prepare(&args, &State::default()), Err(error) if error == MbError::argument("mb_send_mail", 2,
            "subject", "must not contain any null bytes")));
    }

    /// Delivers through a configured harmless process and preserves a true result.
    #[test]
    fn mail_uses_configured_transport() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"subject"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let mut state = State::with_ini_configuration(&[(b"sendmail_path".to_vec(), b"/bin/cat".to_vec())],
            crate::state::CoreEncodingDefaults::default(), |_| Ok(())).0;
        let result = dispatch(&args, &mut state);
        assert_eq!(result.kind, super::super::RESULT_BOOL);
        assert_eq!(result.value, 1);
        assert!(result.diagnostics.is_empty());
    }
}
