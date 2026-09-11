//! Purpose:
//! Parses shared encoding lists for detection, conversion, and request settings.
//!
//! Called from:
//! - mb_detect_order, mb_detect_encoding, mb_convert_encoding, and mb_convert_variables.
//!
//! Key details:
//! - Comma-separated lists accept abbreviated auto tokens, whereas arrays require auto.
//! - Only the first auto token expands; other duplicate encoding entries remain intact.
//! - Adapters convert and append one array element at a time, stopping at the first failure.
//! - Each auto token receives the language defaults active after that element's callback.

use super::Encoding;
use crate::error::{MbError, MbResult};

/// Caller representation retained because PHP applies different trimming and alias rules.
#[derive(Clone, Copy)]
pub enum EncodingList<'a> { CommaSeparated(&'a [u8]), Array(&'a [Vec<u8>]) }

/// Retains resolved entries and the first-auto marker while a host converts successive elements.
pub struct EncodingListBuilder<'a> {
    function: &'a str,
    number: usize,
    parameter: &'a str,
    output: Vec<Encoding>,
    auto: bool,
    error: Option<MbError>,
}

impl<'a> EncodingListBuilder<'a> {
    /// Starts an empty list without capturing mutable request defaults across host callbacks.
    pub fn new(function: &'a str, number: usize, parameter: &'a str) -> Self {
        Self { function, number, parameter, output: Vec::new(), auto: false, error: None }
    }

    /// Appends one already coerced array element using the defaults active at this exact step.
    pub fn push_array(&mut self, name: &[u8], defaults: &[Encoding]) -> MbResult<()> {
        self.push(name, defaults, false)
    }

    /// Publishes a complete list or preserves the first error instead of exposing a partial result.
    pub fn finish(self) -> MbResult<Vec<Encoding>> {
        match self.error { Some(error) => Err(error), None => Ok(self.output) }
    }

    /// Resolves one name under array or comma-list rules without repeating auto expansion.
    fn push(&mut self, name: &[u8], defaults: &[Encoding], comma_separated: bool) -> MbResult<()> {
        if let Some(error) = &self.error { return Err(error.clone()); }
        let is_auto = if comma_separated {
            name.len() <= 4 && b"auto"[..name.len()].eq_ignore_ascii_case(name)
        } else { name.eq_ignore_ascii_case(b"auto") };
        if is_auto {
            if !self.auto { self.output.extend_from_slice(defaults); self.auto = true; }
        } else {
            let encoding = if comma_separated { Encoding::lookup(name) } else { Encoding::lookup_c_string(name) };
            let Some(encoding) = encoding else {
                let error = if self.number == 0 {
                    let mut message = format!("{}: INI setting contains invalid encoding \"", self.function).into_bytes();
                    message.extend_from_slice(name.split(|&byte| byte == 0).next().unwrap_or_default());
                    message.push(b'"');
                    MbError::ValueBytes(message)
                } else { MbError::argument_value(self.function, self.number, self.parameter, "contains invalid encoding ", name, "") };
                self.error = Some(error.clone());
                return Err(error);
            };
            self.output.push(encoding);
        }
        Ok(())
    }
}

/// Resolves a list without state changes or codec deprecations; argument zero takes a complete INI diagnostic context.
pub fn parse_encoding_list(input: EncodingList<'_>, defaults: &[Encoding], function: &str, number: usize, parameter: &str) -> MbResult<Vec<Encoding>> {
    let (parts, comma_separated): (Vec<&[u8]>, bool) = match input {
        EncodingList::CommaSeparated(input) => {
            if input.is_empty() { return Ok(Vec::new()); }
            let input = if input.len() > 2 && input[0] == b'"' && input[input.len() - 1] == b'"' {
                &input[1..input.len() - 1]
            } else { input };
            (input.split(|&byte| byte == b',').map(trim).collect(), true)
        }
        EncodingList::Array(input) => (input.iter().map(Vec::as_slice).collect(), false),
    };
    let mut output = EncodingListBuilder::new(function, number, parameter);
    for name in parts { output.push(name, defaults, comma_separated)?; }
    output.finish()
}

/// Trims only ASCII spaces and tabs, preserving newline and other invalid-name bytes.
fn trim(mut input: &[u8]) -> &[u8] {
    while input.first().is_some_and(|byte| matches!(byte, b' ' | b'\t')) { input = &input[1..]; }
    while input.last().is_some_and(|byte| matches!(byte, b' ' | b'\t')) { input = &input[..input.len() - 1]; }
    input
}
