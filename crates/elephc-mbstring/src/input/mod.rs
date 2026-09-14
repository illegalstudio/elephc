//! Purpose:
//! Prepares HTTP query bytes for shared encoding detection and PHP variable registration.
//!
//! Called from:
//! - The shared HTTP parsing compatibility tests.
//!
//! Key details:
//! - Query splitting uses raw C-string boundaries before binary-safe URL decoding.
//! - Detection examines all names and values together, without candidate-order weighting.
//! - Conversion and registration remain separate so host filters and diagnostics can reenter.
//! - Native/eval public bindings consume these stages when their live-output adapters are added.

mod variables;
mod names;

pub use variables::Variables;
pub use names::{registration, RegistrationStep};
use crate::encoding::Encoding;
use crate::state::{OutputEncoding, State};

/// One URL-decoded name/value pair, before PHP name normalization or array insertion.
#[derive(Debug, PartialEq, Eq)]
pub struct Pair { pub name: Vec<u8>, pub value: Vec<u8> }

impl Pair {
    /// Converts both byte ranges independently and accounts for rejected conversion units.
    /// The destination is captured at parser entry; substitution uses the current request state.
    pub fn convert(self, from: OutputEncoding, to: Encoding, state: &mut State) -> Self {
        let OutputEncoding::Convert(from) = from else { return self; };
        let (name, name_errors) = crate::text::convert_encoding_with_errors(&self.name, from, to, state.substitute());
        state.record_illegal_chars(name_errors);
        let (value, value_errors) = crate::text::convert_encoding_with_errors(&self.value, from, to, state.substitute());
        state.record_illegal_chars(value_errors);
        Self { name, value }
    }
}

/// Decoded source fields, retaining whether the raw query was actually empty.
pub struct Query { empty: bool, pairs: Vec<Pair> }

impl Query {
    /// Splits and decodes all fields before enforcing the whole-query variable limit.
    /// Exceeding the limit discards every field, including names that registration would ignore.
    pub fn decode(input: &[u8], separators: &[u8], max_variables: i64) -> Result<Self, InputLimit> {
        let input = c_string(input);
        let separators = c_string(separators);
        let pairs = input.split(|byte| separators.contains(byte)).filter(|field| !field.is_empty()).map(|field| {
            let equals = field.iter().position(|&byte| byte == b'=');
            let (name, value) = equals.map_or((field, &b""[..]), |index| (&field[..index], &field[index + 1..]));
            Pair { name: url_decode(name), value: url_decode(value) }
        }).collect::<Vec<_>>();
        if !input.is_empty() && (max_variables < 0 || pairs.len() as u128 > max_variables as u128) {
            return Err(InputLimit { maximum: max_variables });
        }
        Ok(Self { empty: input.is_empty(), pairs })
    }

    /// Identifies one encoding for all fields without altering request introspection yet.
    /// Hosts emit a detection warning before converting with the returned pass fallback.
    pub fn identify(&self, candidates: &[OutputEncoding], strict: bool) -> Identification {
        if self.empty { return Identification { encoding: None, warning: false }; }
        match candidates {
            [] => Identification { encoding: Some(OutputEncoding::Pass), warning: false },
            [encoding] => Identification { encoding: Some(*encoding), warning: false },
            candidates => {
                let encodings = candidates.iter().filter_map(|encoding| match encoding {
                    OutputEncoding::Convert(encoding) => Some(*encoding), OutputEncoding::Pass => None,
                }).collect::<Vec<_>>();
                let strings = self.pairs.iter().flat_map(|pair| [pair.name.as_slice(), pair.value.as_slice()]).collect::<Vec<_>>();
                let encoding = crate::detect::guess_many(&strings, &encodings, strict, false);
                Identification { encoding: Some(encoding.map_or(OutputEncoding::Pass, OutputEncoding::Convert)), warning: encoding.is_none() }
            }
        }
    }

    /// Transfers fields in source order for per-pair conversion, host filtering, and registration.
    pub fn into_pairs(self) -> impl Iterator<Item = Pair> { self.pairs.into_iter() }
}

/// Selected HTTP input identity and the independent unable-to-detect warning condition.
#[derive(Debug, PartialEq, Eq)]
pub struct Identification { pub encoding: Option<OutputEncoding>, pub warning: bool }

/// A whole-query limit failure emitted before input detection or any variable registration.
#[derive(Debug)]
pub struct InputLimit { pub maximum: i64 }

impl InputLimit {
    /// Formats the parser-owned PHP diagnostic; the host attaches the current function name.
    pub fn message(&self) -> String {
        format!("Input variables exceeded {}. To increase the limit change max_input_vars in php.ini.", self.maximum)
    }
}

/// Applies the C-string boundary used for raw query bytes, separators, and registered names.
fn c_string(bytes: &[u8]) -> &[u8] { &bytes[..bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len())] }

/// Decodes valid percent pairs and plus signs while preserving malformed escapes and binary NULs.
fn url_decode(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'%' && cursor + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[cursor + 1]), hex(bytes[cursor + 2])) {
                output.push(high * 16 + low);
                cursor += 3;
                continue;
            }
        }
        output.push(if byte == b'+' { b' ' } else { byte });
        cursor += 1;
    }
    output
}

/// Reads one ASCII hexadecimal digit without interpreting non-ASCII character classes.
fn hex(byte: u8) -> Option<u8> {
    match byte { b'0'..=b'9' => Some(byte - b'0'), b'a'..=b'f' => Some(byte - b'a' + 10), b'A'..=b'F' => Some(byte - b'A' + 10), _ => None }
}
