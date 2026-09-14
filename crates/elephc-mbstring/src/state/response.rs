//! Purpose:
//! Owns the response MIME metadata observed by mbstring output conversion.
//!
//! Called from:
//! - Native header publication, terminal output commitment, and output-handler metadata reads.
//!
//! Key details:
//! - PHP retains the first accepted Content-Type for MIME selection, including after replacement.
//! - Header validation precedes metadata changes; the first nonempty terminal write commits headers.
//! - Default MIME and charset preserve empty values and reject NUL or line breaks.

/// Request-local response metadata, independent of output converter activation and decoder state.
#[derive(Clone)]
pub struct Response {
    pub(crate) mimetype: Option<Vec<u8>>,
    pub(crate) default_mimetype: Vec<u8>,
    default_charset: Vec<u8>,
    pub(crate) send_default_content_type: bool,
    sent: bool,
}

impl Default for Response {
    /// Creates PHP's initial response before any explicit header or terminal output.
    fn default() -> Self {
        Self { mimetype: None, default_mimetype: b"text/html".to_vec(), default_charset: b"UTF-8".to_vec(),
            send_default_content_type: true, sent: false }
    }
}

impl Response {
    /// Clears response progress while retaining the accepted startup header defaults.
    pub(crate) fn startup_reset(&self) -> Self {
        Self { default_mimetype: self.default_mimetype.clone(), default_charset: self.default_charset.clone(), ..Self::default() }
    }

    /// Applies the last raw startup override for each Core header default, preserving rejected defaults.
    pub fn with_overrides(overrides: &[(Vec<u8>, Vec<u8>)]) -> Self {
        let mut response = Self::default();
        for (name, destination) in [(b"default_mimetype".as_slice(), &mut response.default_mimetype),
            (b"default_charset".as_slice(), &mut response.default_charset)] {
            if let Some((_, value)) = overrides.iter().rev().find(|(key, _)| key == name) {
                if !value.iter().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) { *destination = value.clone(); }
            }
        }
        response
    }

    /// Validates a header and returns its accepted wire spelling, or a nonthrowing PHP warning.
    /// None is ordinary rejection of an empty input line; callers still own header-list/status work.
    pub fn header(&mut self, line: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        if self.sent { return Err("Cannot modify header information - headers already sent"); }
        if line.is_empty() { return Ok(None); }
        let line = line.trim_ascii_end();
        for byte in line {
            if matches!(byte, b'\r' | b'\n') { return Err("Header may not contain more than a single header, new line detected"); }
            if *byte == 0 { return Err("Header may not contain NUL bytes"); }
        }
        if line.len() >= 5 && line[..5].eq_ignore_ascii_case(b"HTTP/") { return Ok(Some(line.to_vec())); }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else { return Ok(Some(line.to_vec())); };
        if !line[..colon].eq_ignore_ascii_case(b"Content-Type") { return Ok(Some(line.to_vec())); }
        let mut value = &line[colon + 1..];
        while value.first() == Some(&b' ') { value = &value[1..]; }
        let mut mime = value.to_vec();
        let append_charset = !self.default_charset.is_empty() && value.starts_with(b"text/")
            && !value.windows(b"charset=".len()).any(|window| window == b"charset=");
        if append_charset {
            mime.extend_from_slice(b";charset=");
            mime.extend_from_slice(&self.default_charset);
        }
        if self.mimetype.is_none() { self.mimetype = Some(mime.clone()); }
        self.send_default_content_type = false;
        if append_charset {
            let mut line = b"Content-type: ".to_vec();
            line.extend_from_slice(&mime);
            Ok(Some(line))
        } else { Ok(Some(line.to_vec())) }
    }

    /// Commits response defaults at the first nonempty terminal write without invoking any PHP callback.
    pub fn commit(&mut self, length: u64) {
        if length == 0 || self.sent { return; }
        if self.send_default_content_type {
            let mut mime = self.default_mimetype.clone();
            if mime.len() >= 5 && mime[..5].eq_ignore_ascii_case(b"text/") && !self.default_charset.is_empty() {
                mime.extend_from_slice(b"; charset=");
                mime.extend_from_slice(&self.default_charset);
            }
            if !mime.is_empty() { self.mimetype = Some(mime); }
            self.send_default_content_type = false;
        }
        self.sent = true;
    }

    /// Borrows the first explicit or committed default MIME type, retaining present-empty metadata.
    pub fn mimetype(&self) -> Option<&[u8]> { self.mimetype.as_deref() }

    /// Reports whether nonempty terminal output has frozen the response headers.
    pub fn headers_sent(&self) -> bool { self.sent }
}

#[cfg(test)]
mod tests {
    use crate::state::{CoreEncodingDefaults, State};

    /// Resets accepted headers and commitment while retaining configured response defaults across requests.
    #[test]
    fn response_reset_retains_configured_defaults() {
        let overrides = [(b"default_mimetype".to_vec(), b"application/custom".to_vec()),
            (b"default_charset".to_vec(), b"ASCII".to_vec())];
        let (mut state, diagnostics) = State::with_ini_configuration(&overrides, CoreEncodingDefaults::default(), |_| Ok(()));
        assert!(diagnostics.is_empty());
        state.response.header(b"Content-Type: text/plain").unwrap();
        state.response.commit(1);
        state.reset_ini_request();
        assert_eq!(state.response.mimetype(), None);
        assert!(!state.response.headers_sent());
        assert!(state.response.send_default_content_type);
        assert_eq!(state.response.default_mimetype, b"application/custom");
        assert_eq!(state.response.header(b"Content-Type: text/plain").unwrap(), Some(b"Content-type: text/plain;charset=ASCII".to_vec()));
    }
}
