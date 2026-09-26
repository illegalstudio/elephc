//! Purpose:
//! Reads HTTP input configuration and independently recorded input identifications.
//!
//! Called from:
//! - The shared mb_http_input dispatcher and request parser integration.
//!
//! Key details:
//! - Configured input encodings are independent of internal encoding and detection order.
//! - Aggregate and per-source identification are separate, including an explicit pass identity.

use super::{OutputEncoding, State};
use crate::error::{MbError, MbResult};

/// A request source whose identification can be queried independently of the last parsed input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource { Get, Post, Cookie, String }

/// A PHP HTTP input result, distinguishing an unidentified input from an empty encoding list.
#[derive(Debug, PartialEq, Eq)]
pub enum InputInformation { Unidentified, String(Vec<u8>), List(Vec<Vec<u8>>) }

impl State {
    /// Validates a one-byte selector and reads current identification or configured input names.
    pub fn http_input(&self, selector: Option<&[u8]>) -> MbResult<InputInformation> {
        let identified = match selector {
            None => self.http_input,
            Some([b'G' | b'g']) => self.http_input_sources[0],
            Some([b'P' | b'p']) => self.http_input_sources[1],
            Some([b'C' | b'c']) => self.http_input_sources[2],
            Some([b'S' | b's']) => self.http_input_sources[3],
            Some([b'I' | b'i']) => return Ok(InputInformation::List(self.http_input_encodings.iter()
                .map(|encoding| encoding.name().as_bytes().to_vec()).collect())),
            Some([b'L' | b'l']) => return Ok(if self.http_input_encodings.is_empty() { InputInformation::Unidentified }
                else { InputInformation::String(self.http_input_encodings.iter().map(|encoding| encoding.name()).collect::<Vec<_>>().join(",").into_bytes()) }),
            Some(_) => return Err(MbError::argument("mb_http_input", 1, "type", "must be one of \"G\", \"P\", \"C\", \"S\", \"I\", or \"L\"")),
        };
        Ok(identified.map_or(InputInformation::Unidentified, |encoding| InputInformation::String(encoding.name().as_bytes().to_vec())))
    }

    /// Stores the accepted configured input list without changing any previous identification.
    pub fn set_http_input_encodings(&mut self, encodings: &[OutputEncoding]) { self.http_input_encodings = encodings.to_vec(); }

    /// Borrows input detection candidates for a request parser, retaining order and repetitions.
    pub fn http_input_encodings(&self) -> &[OutputEncoding] { &self.http_input_encodings }

    /// Records or clears one source, leaving aggregate identification for the caller to update.
    pub fn set_http_input_source_identification(&mut self, source: InputSource, encoding: Option<OutputEncoding>) {
        let index = match source { InputSource::Get => 0, InputSource::Post => 1, InputSource::Cookie => 2, InputSource::String => 3 };
        self.http_input_sources[index] = encoding;
    }
}
