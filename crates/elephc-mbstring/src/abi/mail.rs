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
use std::process::{Command, Stdio};

use elephc_builtin_contract::mbstring_abi::array::{Key, Value};

use super::{ARG_ARRAY, Arguments, MbError, Outcome, State};
use crate::encoding::{Encoding, Substitute};

#[cfg_attr(not(windows), allow(dead_code))]
mod smtp;

/// Message bytes and PHP warnings determined before the transport runs.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) struct PreparedMail {
    pub(super) bytes: Vec<u8>,
    pub(super) smtp_bytes: Vec<u8>,
    pub(super) recipients: Vec<Vec<u8>>,
    pub(super) sender: Option<Vec<u8>>,
    pub(super) has_date: bool,
    pub(super) warnings: Vec<Vec<u8>>,
}

impl PreparedMail {
    /// Adds the authoritative PHP-timezone Date header when the caller supplied none.
    pub(super) fn add_date(&mut self, date: &[u8]) {
        if self.has_date { return; }
        let mut bytes = Vec::with_capacity(self.smtp_bytes.len() + date.len() + 8);
        bytes.extend_from_slice(b"Date: ");
        bytes.extend_from_slice(date);
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&self.smtp_bytes);
        self.smtp_bytes = bytes;
        self.has_date = true;
    }

    /// Adds a configured From header when the caller supplied no sender field.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn add_from(&mut self, sender: &[u8]) {
        if self.sender.is_some() || sender.is_empty() { return; }
        let mut bytes = Vec::with_capacity(self.smtp_bytes.len() + sender.len() + 8);
        bytes.extend_from_slice(b"From: ");
        bytes.extend_from_slice(sender);
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&self.smtp_bytes);
        self.smtp_bytes = bytes;
        self.sender = Some(sender.to_vec());
    }
}

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
    let mut result = send_prepared(prepared, state, args.nullable_string(4));
    diagnostics.append(&mut result.diagnostics);
    result.diagnostics = diagnostics;
    result
}

/// Selects the configured process or native Windows SMTP transport.
pub(super) fn send_prepared(message: PreparedMail, state: &State, additional: Option<&[u8]>) -> Outcome {
    if let Some(additional) = additional {
        if let Err(error) = no_nul(additional, 5, "additional_params") { return Outcome::error(error); }
    }
    #[cfg(windows)]
    if state.mail_sendmail_path().is_none() {
        let mut message = message;
        let sender = state.sendmail_from().or(message.sender.as_deref()).unwrap_or_default().to_vec();
        if sender.is_empty() {
            return mail_warning(b"\"sendmail_from\" not set in php.ini or custom \"From:\" header missing");
        }
        message.add_from(&sender);
        let result = smtp::deliver(&message, smtp::SmtpConfig {
            host: state.smtp_host(), port: state.smtp_port(), sender: &sender,
        });
        return match result {
            Ok(()) => Outcome::boolean(true),
            Err(error) => smtp_warning(error, state.smtp_host(), state.smtp_port()),
        };
    }
    send_process(&message.bytes, state, additional)
}

/// Runs an already formatted message through the configured platform process transport.
fn send_process(message: &[u8], state: &State, additional: Option<&[u8]>) -> Outcome {
    let mut command = match command(state, additional) {
        Ok(command) => command,
        Err(error) => return Outcome::error(error),
    };
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    let code = match command.spawn() {
        Ok(mut child) => {
            let written = child.stdin.take().is_some_and(|mut input| input.write_all(message).is_ok());
            match child.wait() {
                Ok(status) if written && status.success() => return Outcome::boolean(true),
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        }
        Err(_) => {
            let mut warning = b"Could not execute mail delivery program '".to_vec();
            warning.extend_from_slice(state.mail_command());
            warning.push(b'\'');
            return mail_warning(&warning);
        }
    };
    Outcome {
        diagnostics: format!("Warning: mb_send_mail(): Sendmail exited with non-zero exit code {code}\n").into_bytes(),
        ..Outcome::boolean(false)
    }
}

/// Formats one PHP mb_send_mail warning without losing server-response bytes.
fn mail_warning(message: &[u8]) -> Outcome {
    let mut diagnostics = b"Warning: mb_send_mail(): ".to_vec();
    diagnostics.extend_from_slice(message);
    diagnostics.push(b'\n');
    Outcome { diagnostics, ..Outcome::boolean(false) }
}

/// Maps native Windows SMTP failures to PHP 8.5's exact warning families.
#[cfg_attr(not(windows), allow(dead_code))]
fn smtp_warning(error: smtp::SmtpError, host: &[u8], port: u16) -> Outcome {
    let message = match error {
        smtp::SmtpError::Connect => {
            let mut value = b"Failed to connect to mailserver at \"".to_vec();
            value.extend_from_slice(host);
            value.extend_from_slice(b"\" port ");
            value.extend_from_slice((if port == 0 { 25 } else { port }).to_string().as_bytes());
            value.extend_from_slice(b", verify your \"SMTP\" and \"smtp_port\" setting in php.ini or use ini_set()");
            value
        }
        smtp::SmtpError::Response(response) => [b"SMTP server response: ".as_slice(), response.as_slice()].concat(),
        smtp::SmtpError::Send => b"Failed to Send".to_vec(),
        smtp::SmtpError::Receive => b"Failed to Receive".to_vec(),
        smtp::SmtpError::Protocol => b"Server Error".to_vec(),
    };
    mail_warning(&message)
}

/// Validates transport parameters and creates the platform's PHP-compatible process command.
fn command(state: &State, additional: Option<&[u8]>) -> Result<Command, MbError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        if let Some(additional) = additional { no_nul(additional, 5, "additional_params")?; }
        let path = state.mail_command();
        no_nul(path, 0, "sendmail_path")?;
        if path.is_empty() { return Err(MbError::Runtime("mb_send_mail(): No mail transport is configured".into())); }
        let mut line = path.to_vec();
        if let Some(parameters) = state.mail_force_extra_parameters().or(additional) {
            line.push(b' ');
            line.extend_from_slice(&windows_escape_shell_cmd(parameters));
        }
        let tail = command_argument(&windows_command_tail(&line))?;
        let mut command = Command::new("cmd.exe");
        command.raw_arg(tail);
        return Ok(command);
    }
    #[cfg(not(windows))]
    {
    let mut parts = state.mail_command().split(|byte| byte.is_ascii_whitespace()).filter(|part| !part.is_empty());
    let Some(path) = parts.next() else { return Err(MbError::Runtime("mb_send_mail(): No mail transport is configured".into())); };
    let mut command = Command::new(command_argument(path)?);
    for argument in parts {
        command.arg(command_argument(argument)?);
    }
    if let Some(additional) = additional { no_nul(additional, 5, "additional_params")?; }
    if let Some(parameters) = state.mail_force_extra_parameters().or(additional) {
        for argument in parameters.split(|byte| byte.is_ascii_whitespace()).filter(|part| !part.is_empty()) {
            command.arg(command_argument(argument)?);
        }
    }
    Ok(command)
    }
}

/// Reproduces php-src `popen_ex()`'s raw command tail without Rust argv quoting.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_command_tail(command: &[u8]) -> Vec<u8> {
    let mut tail = Vec::with_capacity(command.len() + b"/s /c \"\"".len());
    tail.extend_from_slice(b"/s /c \"");
    tail.extend_from_slice(command);
    tail.push(b'"');
    tail
}

/// Applies PHP's Windows `php_escape_shell_cmd` metacharacter mapping.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_escape_shell_cmd(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        if byte == 0xff { continue; }
        if matches!(byte, b'#' | b'&' | b';' | b'`' | b'|' | b'*' | b'?' | b'~' | b'<' | b'>' | b'^' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'$' | b'\\' | b'\n' | b'%' | b'!' | b'"' | b'\'') {
            escaped.push(b'^');
        }
        escaped.push(byte);
    }
    escaped
}

/// Converts PHP command bytes to the host process API's string representation.
///
/// Unix process arguments are byte strings and remain exact. Windows process arguments are
/// UTF-16. The Windows implementation mirrors php-src's UTF-8-first conversion with its legacy
/// ANSI-code-page fallback instead of applying Rust's lossy Unicode replacement.
#[cfg(unix)]
fn command_argument(bytes: &[u8]) -> Result<OsString, MbError> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(bytes.to_vec()))
}

/// Converts one PHP byte string to a Windows process argument without Unix-only APIs.
#[cfg(windows)]
fn command_argument(bytes: &[u8]) -> Result<OsString, MbError> {
    use std::os::windows::ffi::OsStringExt;

    if let Ok(argument) = std::str::from_utf8(bytes) {
        return Ok(OsString::from(argument));
    }

    // PHP 8.5.10's `php_win32_cp_any_to_w()` first tries strict UTF-8 when the
    // process code page is UTF-8, then deliberately falls back to the Windows
    // ANSI code page for legacy script bytes. Elephc's Windows runtime is
    // UTF-8-first, so mirror that compatibility fallback rather than rejecting
    // byte strings that PHP passes to `MultiByteToWideChar(GetACP(), ...)`.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetACP() -> u32;
        fn MultiByteToWideChar(code_page: u32, flags: u32, input: *const u8,
            input_len: i32, output: *mut u16, output_len: i32) -> i32;
    }
    let input_len = i32::try_from(bytes.len()).map_err(|_| {
        MbError::Runtime("mb_send_mail(): Mail transport command is too long on Windows".into())
    })?;
    let code_page = unsafe { GetACP() };
    let length = unsafe {
        MultiByteToWideChar(code_page, 0, bytes.as_ptr(), input_len, std::ptr::null_mut(), 0)
    };
    if length <= 0 {
        return Err(MbError::Runtime(
            "mb_send_mail(): Mail transport command could not be converted on Windows".into(),
        ));
    }
    let mut wide = vec![0_u16; length as usize];
    let converted = unsafe {
        MultiByteToWideChar(code_page, 0, bytes.as_ptr(), input_len, wide.as_mut_ptr(), length)
    };
    if converted != length {
        return Err(MbError::Runtime(
            "mb_send_mail(): Mail transport command could not be converted on Windows".into(),
        ));
    }
    Ok(OsString::from_wide(&wide))
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
    let mut recipients = mailbox_list(to);
    let mut sender = None;
    let has_date = headers.iter().any(|(name, _)| name.eq_ignore_ascii_case(b"date"));
    let mut smtp_headers = Vec::new();
    for (name, value) in &headers {
        if name.eq_ignore_ascii_case(b"cc") || name.eq_ignore_ascii_case(b"bcc") {
            recipients.extend(mailbox_list(value));
        }
        if name.eq_ignore_ascii_case(b"from") && sender.is_none() {
            sender = mailbox_list(value).into_iter().next();
        }
        if !name.eq_ignore_ascii_case(b"bcc") { smtp_headers.push((name.clone(), value.clone())); }
    }
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
    let mut smtp_bytes = Vec::new();
    smtp_bytes.extend_from_slice(b"To: "); smtp_bytes.extend_from_slice(&safe_to(to.trim_ascii_end())); smtp_bytes.extend_from_slice(b"\r\nSubject: ");
    smtp_bytes.extend_from_slice(&crate::mime::encode_header(subject, state.internal_encoding(), charset,
        header_encoding.name().eq_ignore_ascii_case("BASE64"), b"\r\n", 28));
    smtp_bytes.extend_from_slice(b"\r\n");
    for (name, value) in smtp_headers { smtp_bytes.extend_from_slice(&name); smtp_bytes.extend_from_slice(b": "); smtp_bytes.extend_from_slice(&value); smtp_bytes.extend_from_slice(b"\r\n"); }
    smtp_bytes.extend_from_slice(b"\r\n"); smtp_bytes.extend_from_slice(&body_encoding.encode_conversion(&text, raw, Substitute::default()));
    Ok(PreparedMail { bytes: result, smtp_bytes, recipients, sender, has_date, warnings })
}

/// Extracts envelope addresses while respecting quoted display-name commas.
fn mailbox_list(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    let (mut start, mut quoted, mut angle) = (0, false, 0u32);
    for (index, &byte) in bytes.iter().enumerate() {
        match byte { b'"' => quoted = !quoted, b'<' if !quoted => angle += 1, b'>' if !quoted => angle = angle.saturating_sub(1), b',' if !quoted && angle == 0 => { push_mailbox(&mut result, &bytes[start..index]); start = index + 1; }, _ => {} }
    }
    push_mailbox(&mut result, &bytes[start..]);
    result
}

/// Normalizes one display-name mailbox into its envelope addr-spec.
fn push_mailbox(result: &mut Vec<Vec<u8>>, value: &[u8]) {
    let value = value.trim_ascii();
    let address = value.iter().position(|&b| b == b'<').and_then(|start| value.iter().rposition(|&b| b == b'>').map(|end| &value[start + 1..end])).unwrap_or(value).trim_ascii();
    if !address.is_empty() { result.push(address.to_vec()); }
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
#[path = "mail/tests.rs"]
mod tests;
