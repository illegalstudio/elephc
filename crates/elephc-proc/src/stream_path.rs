//! Purpose:
//! Opens the path of a `['file', $path, $mode]` descriptor spec the way php-src's
//! `php_stream_open_wrapper()` + `php_stream_cast(PHP_STREAM_AS_FD)` pair does, so a
//! stream URL reaches the child as a real descriptor instead of a literal filename.
//!
//! Called from:
//! - `crate::spawn::Builder::file`.
//!
//! Key details:
//! - The wrapper is located like `php_stream_locate_url_wrapper()`: a scheme of two or more
//!   `[A-Za-z0-9+.-]` bytes followed by `://` (or the case-sensitive `data:` shorthand),
//!   looked up case-insensitively.
//! - Descriptor-backed streams cast: `file://`, `php://stdin|stdout|stderr|fd/N` (a dup),
//!   and `php://temp` / `data:` (an unlinked temporary file holding the decoded payload,
//!   which is what php-src's temp-stream cast produces).
//! - Streams php-src opens but cannot cast (`php://memory|output|input`, `compress.zlib://`,
//!   `php://filter`) and the open errors of each wrapper answer php-src's warning texts.
//!   An unregistered scheme warns and falls back to a plain open of the whole path.
//! - The network and archive wrappers (`http`, `https`, `ftp`, `ftps`, `phar`, `zip`,
//!   `compress.bzip2`) are not emulated: their path is opened as a plain file.
//! - Several warnings for one spec are joined with [`WARNING_SEPARATOR`].

use std::ffi::{CStr, CString};
use std::os::raw::c_int;

use crate::spawn::{close_quietly, dup_cloexec, last_errno, set_cloexec, strerror};

/// Separates the warnings one descriptor spec raises, in emission order.
pub const WARNING_SEPARATOR: char = '\u{1f}';

/// A file spec opened to a close-on-exec descriptor, with the warning php-src emits even
/// though the open succeeded (an unregistered wrapper's fallback).
pub(crate) struct Opened {
    pub(crate) fd: c_int,
    pub(crate) warning: Option<String>,
}

/// Opens `path` for `['file', $path, $mode]`, or answers php-src's warning text(s).
pub(crate) fn open_file_spec(path: &CStr, mode: &str) -> Result<Opened, String> {
    let display = path.to_string_lossy().into_owned();
    let failed = |reason: &str| format!("proc_open({display}): Failed to open stream: {reason}");
    let bytes = path.to_bytes();
    let opened = |fd| Ok(Opened { fd, warning: None });
    let Some(scheme_len) = scheme_length(bytes) else {
        return opened(open_plain(path, mode, &display)?);
    };
    let scheme = String::from_utf8_lossy(&bytes[..scheme_len]).to_ascii_lowercase();
    let rest = &bytes[scheme_len + 1..];
    match scheme.as_str() {
        "file" => {
            let target = rest.strip_prefix(b"//").unwrap_or(rest);
            let target = if target.len() >= 10 && target[..10].eq_ignore_ascii_case(b"localhost/") {
                &target[9..]
            } else {
                target
            };
            if !target.is_empty() && target[0] != b'/' {
                return Err(join(&[
                    format!("proc_open(): Remote host file access not supported, {display}"),
                    failed("no suitable wrapper could be found"),
                ]));
            }
            let target = CString::new(target).unwrap_or_default();
            opened(open_plain(&target, mode, &display)?)
        }
        "php" => opened(open_php(rest.strip_prefix(b"//").unwrap_or(rest), &display, &failed)?),
        "data" => {
            let payload = decode_data_url(rest.strip_prefix(b"//").unwrap_or(rest))
                .map_err(|reason| failed(reason))?;
            opened(temp_descriptor(&payload).map_err(|errno| failed(&strerror(errno)))?)
        }
        "compress.zlib" => {
            let inner = CString::new(rest.strip_prefix(b"//").unwrap_or(rest)).unwrap_or_default();
            let fd = open_flags(mode)
                .map(|flags| unsafe { libc::open(inner.as_ptr(), flags, 0o666 as libc::c_uint) })
                .unwrap_or(-1);
            if fd < 0 {
                return Err(failed("operation failed"));
            }
            close_quietly(fd);
            Err(cannot_represent("ZLIB"))
        }
        "glob" => Err(failed("wrapper does not support stream open")),
        "http" | "https" | "ftp" | "ftps" | "phar" | "zip" | "compress.bzip2" => {
            opened(open_plain(path, mode, &display)?)
        }
        _ => {
            let warning = format!(
                "proc_open(): Unable to find the wrapper \"{}\" - did you forget to enable it when you configured PHP?",
                String::from_utf8_lossy(&bytes[..scheme_len])
            );
            match open_plain(path, mode, &display) {
                Ok(fd) => Ok(Opened { fd, warning: Some(warning) }),
                Err(error) => Err(join(&[warning, error])),
            }
        }
    }
}

/// Returns the scheme length when `path` names a stream wrapper (`scheme://` or `data:`).
fn scheme_length(path: &[u8]) -> Option<usize> {
    let length = path
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        .count();
    let marked = path.get(length) == Some(&b':')
        && length > 1
        && (path[length + 1..].starts_with(b"//") || (length == 4 && path.starts_with(b"data:")));
    marked.then_some(length)
}

/// Joins several warnings into one bridge error text.
fn join(warnings: &[String]) -> String {
    warnings.join(&WARNING_SEPARATOR.to_string())
}

/// php-src's refusal to cast a non-descriptor stream.
fn cannot_represent(kind: &str) -> String {
    format!("proc_open(): Cannot represent a stream of type {kind} as a File Descriptor")
}

/// Translates a PHP `fopen()` mode into `open(2)` flags, or `None` for an invalid mode.
fn open_flags(mode: &str) -> Option<c_int> {
    let mut chars = mode.chars();
    let base = chars.next()?;
    let plus = chars.as_str().contains('+');
    let access = if plus { libc::O_RDWR } else if base == 'r' { libc::O_RDONLY } else { libc::O_WRONLY };
    let extra = match base {
        'r' => 0,
        'w' => libc::O_CREAT | libc::O_TRUNC,
        'a' => libc::O_CREAT | libc::O_APPEND,
        'x' => libc::O_CREAT | libc::O_EXCL,
        'c' => libc::O_CREAT,
        _ => return None,
    };
    Some(access | extra | libc::O_CLOEXEC)
}

/// Opens a filesystem path with fopen-mode semantics.
fn open_plain(path: &CStr, mode: &str, display: &str) -> Result<c_int, String> {
    let Some(flags) = open_flags(mode) else {
        return Err(format!(
            "proc_open({display}): Failed to open stream: {}",
            strerror(libc::EINVAL)
        ));
    };
    let fd = unsafe { libc::open(path.as_ptr(), flags, 0o666 as libc::c_uint) };
    if fd < 0 {
        return Err(format!(
            "proc_open({display}): Failed to open stream: {}",
            strerror(last_errno())
        ));
    }
    Ok(fd)
}

/// Opens a `php://` target (the text after `php://`), case-insensitively like php-src.
fn open_php(target: &[u8], display: &str, failed: &dyn Fn(&str) -> String) -> Result<c_int, String> {
    let named = |name: &str| target.eq_ignore_ascii_case(name.as_bytes());
    let prefixed = |prefix: &str| {
        target.len() >= prefix.len() && target[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    };
    let standard = [("stdin", 0), ("stdout", 1), ("stderr", 2)]
        .into_iter()
        .find(|(name, _)| named(name))
        .map(|(_, fd)| fd);
    if let Some(fd) = standard {
        return dup_cloexec(fd, 0).map_err(|errno| failed(&strerror(errno)));
    }
    if prefixed("fd/") {
        let original = parse_long(&target[3..]).ok_or_else(|| {
            failed("php://fd/ stream must be specified in the form php://fd/<orig fd>")
        })?;
        let table = i64::from(unsafe { libc::getdtablesize() });
        if original < 0 || original >= table {
            return Err(failed(&format!(
                "The file descriptors must be non-negative numbers smaller than {table}"
            )));
        }
        return dup_cloexec(original as c_int, 0).map_err(|errno| {
            failed(&format!(
                "Error duping file descriptor {original}; possibly it doesn't exist: [{errno}]: {}",
                strerror(errno)
            ))
        });
    }
    if named("temp") || prefixed("temp/maxmemory:") {
        return temp_descriptor(&[]).map_err(|errno| failed(&strerror(errno)));
    }
    if named("memory") {
        return Err(cannot_represent("MEMORY"));
    }
    if named("output") {
        return Err(cannot_represent("Output"));
    }
    if named("input") {
        return Err(cannot_represent("Input"));
    }
    if prefixed("filter/") {
        return Err("proc_open(): Cannot cast a filtered stream on this system".to_string());
    }
    Err(join(&[
        "proc_open(): Invalid php:// URL specified".to_string(),
        format!("proc_open({display}): Failed to open stream: operation failed"),
    ]))
}

/// `ZEND_STRTOL(text, &end, 10)` that must consume the whole text: leading whitespace, an
/// optional sign and at least one digit, saturating like `strtol`.
fn parse_long(text: &[u8]) -> Option<i64> {
    let text = &text[text.iter().take_while(|byte| byte.is_ascii_whitespace()).count()..];
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let magnitude = digits.iter().fold(0i64, |value, digit| {
        value.saturating_mul(10).saturating_add(i64::from(digit - b'0'))
    });
    Some(if negative { magnitude.saturating_neg() } else { magnitude })
}

/// An unlinked, close-on-exec temporary file holding `content`, positioned at its start.
fn temp_descriptor(content: &[u8]) -> Result<c_int, c_int> {
    let directory = std::env::temp_dir();
    let template = format!("{}/elephc-proc-XXXXXX", directory.to_string_lossy().trim_end_matches('/'));
    let mut template = CString::new(template).map_err(|_| libc::EINVAL)?.into_bytes_with_nul();
    let fd = unsafe { libc::mkstemp(template.as_mut_ptr().cast()) };
    if fd < 0 {
        return Err(last_errno());
    }
    unsafe {
        libc::unlink(template.as_ptr().cast());
    }
    set_cloexec(fd);
    let mut written = 0;
    while written < content.len() {
        let count = unsafe {
            libc::write(fd, content[written..].as_ptr().cast(), content.len() - written)
        };
        if count < 0 {
            let errno = last_errno();
            close_quietly(fd);
            return Err(errno);
        }
        written += count as usize;
    }
    if unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
        let errno = last_errno();
        close_quietly(fd);
        return Err(errno);
    }
    Ok(fd)
}

/// Decodes an RFC 2397 `data:` URL body (the text after `data:` or `data://`) like
/// php-src's `php_stream_url_wrap_rfc2397()`, answering its `rfc2397:` error reasons.
fn decode_data_url(url: &[u8]) -> Result<Vec<u8>, &'static str> {
    let comma = url.iter().position(|&byte| byte == b',').ok_or("rfc2397: no comma in URL")?;
    let (mut meta, body) = (&url[..comma], &url[comma + 1..]);
    let mut base64 = false;
    if !meta.is_empty() {
        let semi = meta.iter().position(|&byte| byte == b';');
        let sep = meta.iter().position(|&byte| byte == b'/');
        match (semi, sep) {
            (None, None) => return Err("rfc2397: illegal media type"),
            (None, Some(_)) => meta = &[],
            (Some(semi), Some(sep)) if sep < semi => meta = &meta[semi..],
            _ if meta == b";base64" => {}
            _ => return Err("rfc2397: illegal media type"),
        }
        // `;name=value` parameters, then an optional final `;base64`.
        while let Some(params) = meta.strip_prefix(b";") {
            let equals = params.iter().position(|&byte| byte == b'=');
            let next = params.iter().position(|&byte| byte == b';');
            match (equals, next) {
                (Some(equals), next) if next.map_or(true, |next| equals < next) => {
                    meta = &params[next.unwrap_or(params.len())..];
                }
                _ if params == b"base64" => {
                    base64 = true;
                    meta = &[];
                }
                _ => return Err("rfc2397: illegal parameter"),
            }
        }
        if !meta.is_empty() {
            return Err("rfc2397: illegal URL");
        }
    }
    if base64 {
        decode_base64_strict(body).ok_or("rfc2397: unable to decode")
    } else {
        Ok(url_decode(body))
    }
}

/// `php_base64_decode_ex(..., strict = true)`: whitespace is skipped, any other byte outside
/// the alphabet, data after padding, or a malformed tail fails.
fn decode_base64_strict(text: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(text.len() / 4 * 3);
    let (mut accumulator, mut bits, mut count, mut padding) = (0u32, 0u32, 0usize, 0usize);
    for &byte in text {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        if padding > 0 {
            return None;
        }
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        count += 1;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    if count % 4 == 1 || (padding > 0 && (padding > 2 || (count + padding) % 4 != 0)) {
        return None;
    }
    Some(output)
}

/// `php_url_decode()`: `+` is a space and `%XX` a byte when both digits are hexadecimal.
fn url_decode(text: &[u8]) -> Vec<u8> {
    let hex = |byte: u8| (byte as char).to_digit(16);
    let mut output = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        match text[index] {
            b'+' => output.push(b' '),
            b'%' if index + 2 < text.len() => {
                match (hex(text[index + 1]), hex(text[index + 2])) {
                    (Some(high), Some(low)) => {
                        output.push((high * 16 + low) as u8);
                        index += 2;
                    }
                    _ => output.push(b'%'),
                }
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrapper detection follows `php_stream_locate_url_wrapper()`.
    #[test]
    fn scheme_detection_matches_php() {
        assert_eq!(scheme_length(b"php://stdout"), Some(3));
        assert_eq!(scheme_length(b"compress.zlib://x"), Some(13));
        assert_eq!(scheme_length(b"data:,hi"), Some(4));
        assert_eq!(scheme_length(b"DATA:,hi"), None);
        assert_eq!(scheme_length(b"x://y"), None);
        assert_eq!(scheme_length(b"php:/stdout"), None);
        assert_eq!(scheme_length(b"/tmp/a://b"), None);
    }

    /// `data:` bodies decode, and malformed metadata answers php-src's reasons.
    #[test]
    fn data_urls_decode_like_php() {
        assert_eq!(decode_data_url(b",a+b%20c%zz").unwrap(), b"a b c%zz");
        assert_eq!(decode_data_url(b"text/plain;charset=utf-8;base64,aGkK").unwrap(), b"hi\n");
        assert_eq!(decode_data_url(b";base64,aGk").unwrap(), b"hi");
        assert_eq!(decode_data_url(b"text/plain;x=y,hi").unwrap(), b"hi");
        assert_eq!(decode_data_url(b"hi").unwrap_err(), "rfc2397: no comma in URL");
        assert_eq!(decode_data_url(b"text,hi").unwrap_err(), "rfc2397: illegal media type");
        assert_eq!(decode_data_url(b"text/plain;x,hi").unwrap_err(), "rfc2397: illegal parameter");
        assert_eq!(decode_data_url(b";base64,@@").unwrap_err(), "rfc2397: unable to decode");
    }

    /// `php://fd/` numbers parse like `strtol` over the whole text.
    #[test]
    fn fd_numbers_parse_like_strtol() {
        assert_eq!(parse_long(b"01"), Some(1));
        assert_eq!(parse_long(b"-1"), Some(-1));
        assert_eq!(parse_long(b""), None);
        assert_eq!(parse_long(b"1x"), None);
    }
}
