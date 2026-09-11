//! Purpose:
//! Reads mbstring request information from the same settings used by text operations.
//!
//! Called from:
//! - The shared mb_get_info dispatcher and host request configuration adapters.
//!
//! Key details:
//! - Selectors compare complete bytes, including embedded NULs, without changing the encoding cache.
//! - The all snapshot omits absent HTTP identification and preserves PHP insertion order.

use super::{OutputEncoding, State};
use crate::encoding::SubstituteMode;
use crate::arrays::ArrayGraph;
use elephc_builtin_contract::mbstring_abi::array::{Key, Value};

/// A successful information result, preserving PHP scalar and array distinctions.
#[derive(Debug, PartialEq, Eq)]
pub enum Information { Null, Integer(i64), String(Vec<u8>), Strings(Vec<Vec<u8>>), All(ArrayGraph) }

const SELECTORS: [&[u8]; 13] = [b"internal_encoding", b"http_input", b"http_output",
    b"http_output_conv_mimetypes", b"mail_charset", b"mail_header_encoding", b"mail_body_encoding",
    b"illegal_chars", b"encoding_translation", b"language", b"detect_order", b"substitute_character", b"strict_detection"];

impl State {
    /// Returns a setting or ordered snapshot, with None reserved for an invalid selector.
    pub fn info(&self, selector: &[u8]) -> Option<Information> {
        if selector.eq_ignore_ascii_case(b"all") { return Some(Information::All(self.all_info())); }
        let index = SELECTORS.iter().position(|name| selector.eq_ignore_ascii_case(name))?;
        Some(self.info_at(index))
    }

    /// Records the last HTTP input encoding identified by the request parser, including pass.
    pub fn set_http_input_identification(&mut self, encoding: Option<OutputEncoding>) { self.http_input = encoding; }

    /// Returns the last HTTP input identification without performing new detection.
    pub fn http_input_identification(&self) -> Option<OutputEncoding> { self.http_input }

    /// Stores the host's accepted MIME selection expression with its original byte spelling.
    pub fn set_http_output_conv_mimetypes(&mut self, expression: &[u8]) { self.output_mimetypes = expression.to_vec(); }

    /// Borrows the MIME expression used to select HTTP output conversion.
    pub fn http_output_conv_mimetypes(&self) -> &[u8] { &self.output_mimetypes }

    /// Applies the host's request input-translation setting before parsing request data.
    pub fn set_encoding_translation(&mut self, enabled: bool) { self.encoding_translation = enabled; }

    /// Reports whether request input encoding translation is enabled.
    pub fn encoding_translation(&self) -> bool { self.encoding_translation }

    /// Reads one known selector without creating unrelated arrays or changing request state.
    fn info_at(&self, index: usize) -> Information {
        let string = |value: &str| Information::String(value.as_bytes().to_vec());
        match index {
            0 => string(self.internal_encoding().name()),
            1 => self.http_input.map_or(Information::Null, |encoding| string(encoding.name())),
            2 => string(self.http_output().name()),
            3 => Information::String(self.output_mimetypes.clone()),
            4..=6 => string(self.language().mail_encodings()[index - 4].name()),
            7 => Information::Integer(self.illegal_chars() as i64),
            8 => string(if self.encoding_translation { "On" } else { "Off" }),
            9 => string(self.language().name()),
            10 => if self.detect_order().is_empty() { Information::Null } else {
                Information::Strings(self.detect_order().iter().map(|encoding| encoding.name().as_bytes().to_vec()).collect())
            },
            11 => match self.substitute().mode {
                SubstituteMode::Character => Information::Integer(self.substitute().character as i64),
                SubstituteMode::None => string("none"),
                SubstituteMode::Long => string("long"),
                SubstituteMode::Entity => string("entity"),
            },
            12 => string(if self.strict_detection() { "On" } else { "Off" }),
            _ => unreachable!("validated information selector"),
        }
    }

    /// Builds a fresh associative root and an independently owned detection-order child.
    fn all_info(&self) -> ArrayGraph {
        let mut arrays = vec![Vec::new()];
        for (index, name) in SELECTORS.iter().enumerate() {
            let value = match self.info_at(index) {
                Information::Null => continue,
                Information::Integer(value) => Value::Int(value),
                Information::String(value) => Value::String(value),
                Information::Strings(values) => {
                    let index = arrays.len();
                    arrays.push(values.into_iter().enumerate().map(|(index, value)| (Key::Int(index as i64), Value::String(value))).collect());
                    Value::Array(index)
                },
                Information::All(_) => unreachable!("single selectors cannot return a snapshot"),
            };
            arrays[0].push((Key::String(name.to_vec()), value));
        }
        ArrayGraph::new(0, arrays).expect("unique information keys and valid detection child")
    }
}
