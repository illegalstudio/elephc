//! Purpose:
//! Owns mutable mbstring settings shared by native and interpreted request adapters.
//!
//! Called from:
//! - mbstring setting operations and explicit/default encoding resolution.
//!
//! Key details:
//! - Each request owns its state; constructing a new state clears the lookup cache.
//! - Failed language changes reset the language but retain the previous auto defaults.
//! - Host adapters are responsible for request lifecycle and PHP diagnostic emission.

mod language;
mod language_data;
mod info;
mod http_input;
mod ini;
mod core_ini;
mod output;
mod response;

pub use language::Language;
pub use info::Information;
pub use http_input::{InputInformation, InputSource};
pub use ini::{CoreEncodingDefaults, IniRequest, IniString, IniUpdate, MimeRegexError};
pub use core_ini::CoreIni;
pub use output::{OutputHeaders, OutputPlan};
pub use response::Response;
pub(crate) use output::mime_pattern;
use crate::encoding::{parse_encoding_list, Encoding, EncodingList, EncodingListBuilder, Substitute, SubstituteMode};
use crate::error::{MbError, MbResult};

/// HTTP output can bypass conversion even though pass is not a public encoding identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputEncoding { Pass, Convert(Encoding) }

impl OutputEncoding {
    /// Returns the spelling used by mb_http_output and request introspection.
    pub fn name(self) -> &'static str { match self { Self::Pass => "pass", Self::Convert(encoding) => encoding.name() } }
}

/// Result of ordinary encoding resolution, with an optional diagnostic to emit at the caller.
pub struct ResolvedEncoding { pub encoding: Encoding, pub deprecation: Option<&'static str> }

/// Encoding, language, and replacement settings for one request's mbstring operations.
#[derive(Clone)]
pub struct State {
    ini: ini::IniData,
    pub(crate) core_ini: CoreIni,
    pub(crate) response: Response,
    language: Language,
    auto_language: Language,
    internal: Encoding,
    output: OutputEncoding,
    http_input: Option<OutputEncoding>,
    http_input_sources: [Option<OutputEncoding>; 4],
    http_input_encodings: Vec<OutputEncoding>,
    output_mimetypes: Vec<u8>,
    output_conversion: output::OutputState,
    encoding_translation: bool,
    detect_order: Vec<Encoding>,
    substitute: Substitute,
    strict_detection: bool,
    illegal_chars: u64,
    cached_encoding: Option<(Vec<u8>, Encoding)>,
}

impl Default for State {
    /// Creates the PHP baseline defaults; host configuration can apply overrides before execution.
    fn default() -> Self {
        let language = Language::default();
        let internal = Encoding::lookup(b"UTF-8").expect("default encoding");
        Self { ini: ini::IniData::default(), core_ini: CoreIni::default(), response: Response::default(), language, auto_language: language, internal, output: OutputEncoding::Convert(internal),
            http_input: None, output_mimetypes: br"^(text/|application/xhtml\+xml)".to_vec(),
            output_conversion: output::OutputState::default(), encoding_translation: false,
            http_input_sources: [None; 4], http_input_encodings: vec![OutputEncoding::Convert(internal)],
            detect_order: language.detect_order(), substitute: Substitute::default(), strict_detection: false, illegal_chars: 0, cached_encoding: None }
    }
}

impl State {
    /// Returns the current language without altering auto expansion or active detection order.
    pub fn language(&self) -> Language { self.language }

    /// Changes the language and future auto expansion, preserving the active detection order.
    pub fn set_language(&mut self, name: &[u8]) -> MbResult<()> {
        self.begin_language_ini_change();
        let Some(language) = Language::lookup(name) else {
            self.language = Language::default();
            return Err(MbError::argument_value("mb_language", 1, "language", "must be a valid language, ", name, " given"));
        };
        self.language = language;
        self.auto_language = language;
        self.commit_language_ini_change(name);
        Ok(())
    }

    /// Returns the encoding used when an ordinary text operation omits its encoding argument.
    pub fn internal_encoding(&self) -> Encoding { self.internal }

    /// Updates the internal encoding without changing the explicit-lookup cache or emitting deprecations.
    pub fn set_internal_encoding(&mut self, name: &[u8]) -> MbResult<()> {
        self.internal = encoding_argument(name, "mb_internal_encoding", 1, "encoding")?;
        self.ini.explicit_internal = true;
        Ok(())
    }

    /// Returns the current output conversion setting.
    pub fn http_output(&self) -> OutputEncoding { self.output }

    /// Resolves the output encoding, including PHP's case-sensitive abbreviated pass values.
    pub fn set_http_output(&mut self, name: &[u8]) -> MbResult<()> {
        if name.contains(&0) { return Err(MbError::argument("mb_http_output", 1, "encoding", "must not contain any null bytes")); }
        self.output = if b"pass".starts_with(name) { OutputEncoding::Pass }
            else { OutputEncoding::Convert(encoding_argument(name, "mb_http_output", 1, "encoding")?) };
        self.ini.explicit_output = true;
        Ok(())
    }

    /// Returns the active detection order, including intentional repeated entries.
    pub fn detect_order(&self) -> &[Encoding] { &self.detect_order }

    /// Parses a new detection order and commits it only after validation succeeds.
    pub fn set_detect_order(&mut self, input: EncodingList<'_>) -> MbResult<()> {
        let order = self.parse_encodings(input, "mb_detect_order", 1, "encoding")?;
        if order.is_empty() { return Err(MbError::argument("mb_detect_order", 1, "encoding", "must specify at least one encoding")); }
        self.detect_order = order;
        Ok(())
    }

    /// Parses lists for every conversion/detection consumer using the same current auto defaults.
    pub fn parse_encodings(&self, input: EncodingList<'_>, function: &str, number: usize, parameter: &str) -> MbResult<Vec<Encoding>> {
        parse_encoding_list(input, &self.auto_language.detect_order(), function, number, parameter)
    }

    /// Resolves one host-coerced array element after callbacks have updated the request language.
    pub fn push_array_encoding(&self, list: &mut EncodingListBuilder<'_>, name: &[u8]) -> MbResult<()> {
        list.push_array(name, &self.auto_language.detect_order())
    }

    /// Returns the request's default strictness for detection and automatic conversion.
    pub fn strict_detection(&self) -> bool { self.strict_detection }

    /// Applies the boolean value supplied by the host's mbstring.strict_detection INI adapter.
    pub fn set_strict_detection(&mut self, strict: bool) { self.strict_detection = strict; }

    /// Resolves candidates and detects text with PHP's empty-list, transfer, and default rules.
    pub fn detect_encoding(&self, input: &[u8], candidates: Option<EncodingList<'_>>, strict: Option<bool>, order_significant: bool) -> MbResult<Option<Encoding>> {
        let mut candidates = match candidates {
            Some(list) => self.parse_encodings(list, "mb_detect_encoding", 2, "encodings")?,
            None => self.detect_order.clone(),
        };
        if candidates.is_empty() { return Err(MbError::argument("mb_detect_encoding", 2, "encodings", "must specify at least one encoding")); }
        candidates.retain(|encoding| encoding.supports_detection());
        Ok(crate::detect::guess(input, &candidates, strict.unwrap_or(self.strict_detection), order_significant))
    }

    /// Prepares mb_convert_encoding sources after the adapter resolves the destination argument.
    pub fn conversion_sources(&self, input: Option<EncodingList<'_>>) -> MbResult<crate::text::ConversionSources> {
        let encodings = match input {
            Some(list) => self.parse_encodings(list, "mb_convert_encoding", 3, "from_encoding")?,
            None => vec![self.internal],
        };
        crate::text::ConversionSources::new(encodings)
    }

    /// Converts a string with prepared sources and records only actual conversion rejections.
    pub fn convert_string(&mut self, input: &[u8], to: Encoding, sources: &crate::text::ConversionSources) -> Option<Vec<u8>> {
        let (output, errors) = crate::encoding::errors::measure(|| sources.convert(input, to, self.strict_detection, self.substitute));
        self.record_illegal_chars(errors);
        output
    }

    /// Converts an array graph and accounts for visited strings even if a later traversal fails.
    pub fn convert_array(&mut self, input: &crate::arrays::ArrayGraph, to: Encoding, sources: &crate::text::ConversionSources) -> crate::arrays::ArrayConversion {
        let output = crate::arrays::convert_encoding(input, to, sources, self.strict_detection, self.substitute);
        self.record_illegal_chars(output.illegal_chars);
        output
    }

    /// Returns rejected units accumulated by request conversion and output operations.
    pub fn illegal_chars(&self) -> u64 { self.illegal_chars }

    /// Accumulates a measured conversion count with PHP's unsigned counter behavior.
    pub fn record_illegal_chars(&mut self, count: u64) {
        self.illegal_chars = self.illegal_chars.wrapping_add(count);
    }

    /// Replaces malformed units and records rejections without counting unrelated text operations.
    pub fn scrub(&mut self, input: &[u8], encoding: Encoding) -> Vec<u8> {
        let (output, errors) = crate::text::convert_encoding_with_errors(input, encoding, encoding, self.substitute);
        self.record_illegal_chars(errors);
        output
    }

    /// Returns both the active replacement mode and its remembered scalar character.
    pub fn substitute(&self) -> Substitute { self.substitute }

    /// Selects a Unicode scalar replacement and resets the mode to character substitution.
    pub fn set_substitute_codepoint(&mut self, code: i64) -> MbResult<()> {
        if !(0..=0x10ffff).contains(&code) || (0xd800..=0xdfff).contains(&code) {
            return Err(MbError::argument("mb_substitute_character", 1, "substitute_character", "is not a valid codepoint"));
        }
        self.substitute = Substitute { mode: SubstituteMode::Character, character: code as u32 };
        Ok(())
    }

    /// Selects a textual substitution mode while retaining the remembered replacement character.
    pub fn set_substitute_mode(&mut self, name: &[u8]) -> MbResult<()> {
        self.substitute.mode = if name.eq_ignore_ascii_case(b"none") { SubstituteMode::None }
            else if name.eq_ignore_ascii_case(b"long") { SubstituteMode::Long }
            else if name.eq_ignore_ascii_case(b"entity") { SubstituteMode::Entity }
            else { return Err(MbError::argument("mb_substitute_character", 1, "substitute_character", "must be \"none\", \"long\", \"entity\" or a valid codepoint")); };
        Ok(())
    }

    /// Resolves an ordinary optional argument and reproduces PHP's last-name deprecation cache.
    pub fn resolve_encoding(&mut self, name: Option<&[u8]>, function: &str, number: usize, parameter: &str) -> MbResult<ResolvedEncoding> {
        let Some(name) = name else { return Ok(ResolvedEncoding { encoding: self.internal, deprecation: None }); };
        if let Some((cached, encoding)) = &self.cached_encoding {
            if name.eq_ignore_ascii_case(cached) { return Ok(ResolvedEncoding { encoding: *encoding, deprecation: None }); }
        }
        let encoding = encoding_argument(name, function, number, parameter)?;
        self.cached_encoding = Some((name.to_vec(), encoding));
        Ok(ResolvedEncoding { encoding, deprecation: encoding.deprecation() })
    }
}

/// Validates an ordinary C-string encoding name with the caller's exact PHP argument diagnostic.
fn encoding_argument(name: &[u8], function: &str, number: usize, parameter: &str) -> MbResult<Encoding> {
    Encoding::lookup_c_string(name).ok_or_else(|| MbError::argument_value(function, number, parameter, "must be a valid encoding, ", name, " given"))
}
