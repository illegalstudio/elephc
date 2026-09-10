//! Purpose:
//! Plans output conversion against host header metadata and converts one buffer phase.
//!
//! Called from:
//! - Shared output-handler adapters and focused PHP compatibility tests.
//!
//! Key details:
//! - Header publication occurs between planning and conversion, outside the request borrow.
//! - Decoder state spans calls; encoder state belongs only to one call's 128-word batches.
//! - Pass mode returns before phase transitions, matching PHP's early return.

use super::{OutputEncoding, State};

/// Trims PHP's MIME directive whitespace before applying its separate C-string boundary.
pub(crate) fn mime_pattern(value: &[u8]) -> Option<&[u8]> {
    // PHP trim includes NUL and excludes form feed, unlike ASCII whitespace trimming.
    let is_trim = |byte: &u8| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0 | 11);
    let start = value.iter().position(|byte| !is_trim(byte))?;
    let end = value.iter().rposition(|byte| !is_trim(byte)).unwrap() + 1;
    let pattern = &value[start..end];
    Some(&pattern[..pattern.iter().position(|byte| *byte == 0).unwrap_or(pattern.len())])
}

/// Header facts supplied by the host without transferring response ownership to mbstring.
pub struct OutputHeaders<'a> {
    pub mimetype: Option<&'a [u8]>,
    pub default_mimetype: Option<&'a [u8]>,
    pub send_default_content_type: bool,
    pub in_handler: bool,
}

/// Captured output encoding and deferred header action for one prepared PHP call.
pub struct OutputPlan {
    encoding: OutputEncoding,
    phase: i64,
    activate: bool,
    pub header: Option<Vec<u8>>,
}

/// Request-global conversion activation and the source decoder's packed state.
#[derive(Clone, Default)]
pub(super) struct OutputState { enabled: bool, decoder: u32 }

impl State {
    /// Plans START handling using the host's MIME match, without emitting headers or invoking PHP.
    pub fn prepare_output(&self, phase: i64, headers: OutputHeaders<'_>, mime_matches: bool) -> OutputPlan {
        let mut plan = OutputPlan { encoding: self.http_output(), phase, activate: false, header: None };
        let OutputEncoding::Convert(encoding) = plan.encoding else { return plan; };
        if phase & 1 == 0 { return plan; }
        let matched = headers.mimetype.filter(|_| mime_matches);
        let mimetype = if let Some(mimetype) = matched {
            &mimetype[..mimetype.iter().position(|byte| *byte == b';').unwrap_or(mimetype.len())]
        } else {
            headers.default_mimetype.unwrap_or(b"text/html")
        };
        if headers.send_default_content_type || matched.is_some() {
            plan.activate = true;
            if !headers.in_handler {
                if let Some(charset) = encoding.mime_name() {
                    let mut line = b"Content-Type: ".to_vec();
                    line.extend_from_slice(mimetype);
                    line.extend_from_slice(b"; charset=");
                    line.extend_from_slice(charset.as_bytes());
                    plan.header = Some(line);
                }
            }
        }
        plan
    }

    /// Converts after header publication using the live internal encoding and substitution settings.
    pub fn output_chunk(&mut self, input: &[u8], plan: OutputPlan) -> Vec<u8> {
        let OutputEncoding::Convert(encoding) = plan.encoding else { return input.to_vec(); };
        self.output_conversion.enabled |= plan.activate;
        if !self.output_conversion.enabled { return input.to_vec(); }
        let mut remaining = input;
        let mut chunks = Vec::new();
        while !remaining.is_empty() {
            chunks.push(self.internal.decode_next(&mut remaining, 128, &mut self.output_conversion.decoder).points);
        }
        let final_feed = plan.phase & 8 != 0;
        let (output, errors) = crate::encoding::errors::measure(||
            encoding.encode_output_chunks(&chunks, self.substitute, final_feed));
        self.record_illegal_chars(errors);
        if final_feed { self.output_conversion = OutputState::default(); }
        output
    }
}
