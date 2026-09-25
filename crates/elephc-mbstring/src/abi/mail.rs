//! Purpose:
//! Formats and submits multibyte mail with the current request language settings.
//!
//! Called from:
//! - The shared mbstring ABI after contract coercion and array snapshotting.
//!
//! Key details:
//! - MIME conversion uses the existing codec catalog with PHP's mail substitution policy.
//! - The transport receives arguments directly, so header and parameter bytes never enter a shell.
//! - Failed process creation and nonzero exits return false with a PHP warning.

use std::ffi::OsString;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Stdio};

use elephc_builtin_contract::mbstring_abi::array::{Key, Value};

use super::{ARG_ARRAY, Arguments, MbError, Outcome, State};
use crate::encoding::{Encoding, Substitute};

/// Formats the message before opening a transport and returns the delivery status.
pub(super) fn dispatch(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let message = match prepare(args, state) {
        Ok(message) => message,
        Err(error) => return Outcome::error(error),
    };
    let mut command = match command(state, args.nullable_string(4)) {
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
    if let Some(additional) = additional {
        no_nul(additional, 5, "additional_params")?;
        command.args(additional.split(|byte| byte.is_ascii_whitespace()).filter(|part| !part.is_empty())
            .map(|part| OsString::from_vec(part.to_vec())));
    }
    Ok(command)
}

/// Encodes PHP's subject and body and appends absent MIME headers in source order.
fn prepare(args: &Arguments<'_>, state: &State) -> Result<Vec<u8>, MbError> {
    let to = args.string(0);
    let subject = args.string(1);
    let body = args.string(2);
    for (index, name, bytes) in [(1, "to", to), (2, "subject", subject), (3, "message", body)] {
        no_nul(bytes, index, name)?;
    }
    let mut headers = headers(args)?;
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
            charset = Encoding::lookup(name).unwrap_or_else(|| Encoding::lookup(b"ASCII").expect("ASCII codec"));
        }
    }
    let transfer = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case(b"content-transfer-encoding"));
    if let Some((_, value)) = transfer {
        body_encoding = Encoding::lookup(value.trim_ascii()).filter(|encoding|
            ["BASE64", "7bit", "8bit"].iter().any(|name| encoding.name().eq_ignore_ascii_case(name)))
            .unwrap_or_else(|| Encoding::lookup(b"8bit").expect("8bit codec"));
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
    let mut result = Vec::new();
    result.extend_from_slice(b"To: ");
    result.extend(to.trim_ascii_end().iter().map(|&byte| if byte.is_ascii_control() { b' ' } else { byte }));
    result.extend_from_slice(b"\r\nSubject: ");
    result.extend_from_slice(&crate::mime::encode_header(subject, state.internal_encoding(), charset,
        header_encoding.name().eq_ignore_ascii_case("BASE64"), b"\r\n", 29));
    result.extend_from_slice(b"\r\n");
    for (name, value) in headers {
        result.extend_from_slice(&name);
        result.extend_from_slice(b": ");
        result.extend_from_slice(&value);
        result.extend_from_slice(b"\r\n");
    }
    result.extend_from_slice(b"\r\n");
    let text = charset.encode_conversion(body, state.internal_encoding(), Substitute::default());
    let raw = Encoding::lookup(b"8bit").expect("8bit codec");
    result.extend_from_slice(&body_encoding.encode_conversion(&text, raw, Substitute::default()));
    result.extend_from_slice(b"\r\n");
    Ok(result)
}

/// Copies string and array header forms without allowing embedded NUL bytes.
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
    graph.arrays()[graph.root()].iter().map(|(key, value)| {
        let Value::String(value) = value else {
            return Err(MbError::argument("mb_send_mail", 4, "additional_headers", "must contain only string values"));
        };
        no_nul(value, 4, "additional_headers")?;
        let name = match key {
            Key::String(name) => name.clone(),
            Key::Int(_) => {
                let Some((name, _)) = split_once_byte(value, b':') else {
                    return Err(MbError::argument("mb_send_mail", 4, "additional_headers", "contains a header without a colon"));
                };
                name.to_vec()
            }
        };
        let value = if matches!(key, Key::Int(_)) {
            split_once_byte(value, b':').expect("validated header").1.trim_ascii().to_vec()
        } else { value.clone() };
        no_nul(&name, 4, "additional_headers")?;
        Ok((name, value))
    }).collect()
}

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
    use elephc_builtin_contract::{RuntimeBuiltinId, mbstring_abi::MbArgV1};

    /// Matches PHP's neutral-language MIME bytes for non-ASCII subject and body text.
    #[test]
    fn neutral_mail_matches_php_mime_output() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string("Café".as_bytes()),
            MbArgV1::string("Crème".as_bytes())];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        let message = prepare(&args, &State::default()).expect("MIME message");
        assert_eq!(message, b"To: a@example.test\r\nSubject: =?UTF-8?B?Q2Fmw6k=?=\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: BASE64\r\n\r\nQ3LDqG1l\r\n");
    }

    /// Rejects embedded NULs before opening a mail transport.
    #[test]
    fn mail_rejects_nul_subject() {
        let slots = [MbArgV1::string(b"a@example.test"), MbArgV1::string(b"a\0b"), MbArgV1::string(b"body")];
        let args = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, &slots) }.expect("mail arguments");
        assert_eq!(prepare(&args, &State::default()), Err(MbError::argument("mb_send_mail", 2,
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
