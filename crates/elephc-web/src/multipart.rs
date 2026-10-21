//! Purpose:
//! A minimal, binary-safe `multipart/form-data` (RFC 7578) body parser. Splits a
//! request body into named parts — text fields and file uploads — backing `$_POST`
//! and `$_FILES` under `--web`.
//!
//! Called from:
//! - `crate::request_state` (lazily, on the first `elephc_web_multipart_*` getter
//!   call for the current request).
//!
//! Key details:
//! - The body is treated as raw bytes (file uploads are binary); only the part
//!   headers are interpreted as ASCII.
//! - The boundary comes from the `Content-Type: multipart/form-data; boundary=…`
//!   header. A part with a `filename` is a file upload; otherwise it is a field.

/// One parsed multipart part.
pub(crate) struct Part {
    /// The `name="…"` from Content-Disposition.
    pub name: String,
    /// The `filename="…"` from Content-Disposition, if present (file upload).
    pub filename: Option<String>,
    /// The part's `Content-Type`, or empty.
    pub content_type: String,
    /// The raw part body (binary-safe).
    pub content: Vec<u8>,
}

/// Parses `body` as `multipart/form-data` using the boundary in `content_type`.
/// Returns an empty vec if the content type is not multipart or has no boundary.
pub(crate) fn parse(body: &[u8], content_type: &str) -> Vec<Part> {
    let Some(boundary) = extract_boundary(content_type) else {
        return Vec::new();
    };
    let delim = format!("--{}", boundary).into_bytes();
    let positions = find_all(body, &delim);
    let mut parts = Vec::new();
    for pair in positions.windows(2) {
        let seg = &body[pair[0] + delim.len()..pair[1]];
        // Each segment starts with CRLF (after the delimiter) and ends with CRLF
        // (before the next delimiter); strip both before parsing the part.
        let seg = strip_prefix(seg, b"\r\n");
        let seg = strip_suffix(seg, b"\r\n");
        if let Some(part) = parse_part(seg) {
            parts.push(part);
        }
    }
    parts
}

/// Extracts the `boundary=…` value from a multipart Content-Type, unquoting it.
fn extract_boundary(content_type: &str) -> Option<String> {
    if !content_type.to_ascii_lowercase().contains("multipart/form-data") {
        return None;
    }
    for attr in content_type.split(';') {
        let attr = attr.trim();
        if let Some(rest) = attr.strip_prefix("boundary=").or_else(|| attr.strip_prefix("boundary =")) {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Parses one part (its header block + body) into a `Part`.
fn parse_part(seg: &[u8]) -> Option<Part> {
    let split = find(seg, b"\r\n\r\n")?;
    let header_bytes = &seg[..split];
    let content = seg[split + 4..].to_vec();
    let headers = String::from_utf8_lossy(header_bytes);
    let mut name: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut content_type = String::new();
    for line in headers.split("\r\n") {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-disposition:") {
            name = extract_quoted(line, "name=");
            filename = extract_quoted(line, "filename=");
        } else if lower.starts_with("content-type:") {
            content_type = line[line.find(':').map(|i| i + 1).unwrap_or(0)..].trim().to_string();
        }
    }
    Some(Part {
        name: name?,
        filename,
        content_type,
        content,
    })
}

/// Extracts a `key=value` attribute from a `Content-Disposition` line using
/// php-src's `php_ap_getword_conf` rules: the value may be double-quoted,
/// single-quoted, or unquoted; `\\` and `\<quote>` are unescaped; an unquoted
/// value ends at the next `;`.
///
/// This is shared by the request parser and the upload-progress tracker so a
/// filename cannot mean different things to `$_FILES` and to the progress
/// snapshot written into the session file.
pub(crate) fn extract_quoted(line: &str, key: &str) -> Option<String> {
    let at = find_attribute(line, key)?;
    let rest = line[at + key.len()..].trim_start_matches([' ', '\t']);
    let quote = match rest.as_bytes().first() {
        Some(&b'"') => Some('"'),
        Some(&b'\'') => Some('\''),
        _ => None,
    };
    let body = match quote {
        Some(q) => &rest[q.len_utf8()..],
        None => {
            let end = rest.find(';').unwrap_or(rest.len());
            &rest[..end]
        }
    };
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if Some(ch) == quote {
            return Some(out);
        }
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '\\' || Some(next) == quote {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(ch);
    }
    // A quoted value without a closing quote is malformed; an unquoted value
    // simply consumed the rest of the line.
    if quote.is_none() {
        Some(out)
    } else {
        None
    }
}

/// Finds `key` in a header line only at an attribute boundary, so `name=` does
/// not match inside `filename=`.
fn find_attribute(line: &str, key: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(relative) = line[from..].find(key) {
        let at = from + relative;
        let boundary = at == 0 || matches!(bytes[at - 1], b';' | b' ' | b'\t');
        if boundary {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Returns the byte offsets of every occurrence of `needle` in `haystack`.
fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    memchr::memmem::find_iter(haystack, needle).collect()
}

/// Returns the byte offset of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    memchr::memmem::find(haystack, needle)
}

/// Removes a leading `prefix` from `data` if present.
fn strip_prefix<'a>(data: &'a [u8], prefix: &[u8]) -> &'a [u8] {
    data.strip_prefix(prefix).unwrap_or(data)
}

/// Removes a trailing `suffix` from `data` if present.
fn strip_suffix<'a>(data: &'a [u8], suffix: &[u8]) -> &'a [u8] {
    data.strip_suffix(suffix).unwrap_or(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a text field + a file upload parse into the expected parts.
    #[test]
    fn parses_field_and_file() {
        let ct = "multipart/form-data; boundary=X";
        let body = b"--X\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nhello\r\n--X\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.txt\"\r\nContent-Type: text/plain\r\n\r\nFILEDATA\r\n--X--\r\n";
        let parts = parse(body, ct);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "a");
        assert!(parts[0].filename.is_none());
        assert_eq!(parts[0].content, b"hello");
        assert_eq!(parts[1].name, "f");
        assert_eq!(parts[1].filename.as_deref(), Some("x.txt"));
        assert_eq!(parts[1].content_type, "text/plain");
        assert_eq!(parts[1].content, b"FILEDATA");
    }

    /// Verifies a non-multipart content type yields no parts.
    #[test]
    fn non_multipart_is_empty() {
        assert!(parse(b"x=1", "application/x-www-form-urlencoded").is_empty());
    }

    /// Matches php-src's `php_ap_getword_conf`: quoted values unescape `\\` and
    /// `\<quote>`, unquoted values run to the next `;`, and `name=` must not
    /// match inside `filename=`.
    #[test]
    fn parses_escaped_and_unquoted_attribute_values() {
        let ct = "multipart/form-data; boundary=X";
        let body = b"--X\r\nContent-Disposition: form-data; name=\"f\"; filename=\"a\\\"b\\\\c.txt\"\r\n\r\nDATA\r\n--X\r\nContent-Disposition: form-data; filename=x.txt; name=f2\r\n\r\nDATA2\r\n--X--\r\n";
        let parts = parse(body, ct);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "f");
        assert_eq!(
            parts[0].filename.as_deref(),
            Some("a\"b\\c.txt"),
            "escaped quote and backslash must be unescaped"
        );
        assert_eq!(parts[1].name, "f2", "name= must not match inside filename=");
        assert_eq!(parts[1].filename.as_deref(), Some("x.txt"));
    }

    /// Guards against reintroducing the adversarial `O(body × boundary)` scan
    /// while functional multipart fixtures continue to pin parser semantics.
    #[test]
    fn multipart_boundary_search_uses_a_linear_time_primitive() {
        let source = include_str!("multipart.rs");
        let forbidden = ["&haystack[i..i + ", "needle.len()] == needle"].concat();
        assert!(
            !source.contains(&forbidden),
            "multipart boundary search must use a linear-time byte-search primitive"
        );
    }
}
