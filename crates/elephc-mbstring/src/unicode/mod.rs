//! Purpose:
//! Implements encoding-independent Unicode casing and PHP display width semantics.
//!
//! Called from:
//! - The shared mbstring engine after decoding input into Unicode codepoints.
//!
//! Key details:
//! - Full mappings expand to at most three codepoints; simple mappings stay one-to-one.
//! - Only ISO-8859-9 uses Turkish casing; the process locale is irrelevant.
//! - Invalid decoder markers survive casing and are substituted by the encoder.

mod tables;
pub(crate) mod kana;

/// Version of the generated Unicode Character Database, matching PHP 8.5.10.
pub const UNICODE_VERSION: &str = "17.0.0";

/// PHP's eight `MB_CASE_*` values in their public numeric order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CaseMode {
    Upper = 0,
    Lower = 1,
    Title = 2,
    Fold = 3,
    UpperSimple = 4,
    LowerSimple = 5,
    TitleSimple = 6,
    FoldSimple = 7,
}

impl CaseMode {
    /// Resolves a PHP case-mode integer, rejecting values outside the eight constants.
    pub fn from_php(mode: i64) -> Option<Self> {
        Some(match mode {
            0 => Self::Upper,
            1 => Self::Lower,
            2 => Self::Title,
            3 => Self::Fold,
            4 => Self::UpperSimple,
            5 => Self::LowerSimple,
            6 => Self::TitleSimple,
            7 => Self::FoldSimple,
            _ => return None,
        })
    }
}

/// Reports the Unicode Cased property using the baseline's versioned table.
pub fn is_cased(code: u32) -> bool {
    tables::contains(tables::CASED, code)
}

/// Reports the Unicode Case_Ignorable property used by title and sigma rules.
pub fn is_case_ignorable(code: u32) -> bool {
    tables::contains(tables::IGNORABLE, code)
}

/// Returns PHP's display width: two for East Asian Wide/Fullwidth, otherwise one.
pub fn character_width(code: u32) -> usize {
    1 + usize::from(tables::contains(tables::WIDE, code))
}

/// Converts codepoints using the 64-character decoder batches used by PHP's UTF encodings.
pub fn convert_case(input: &[u32], mode: CaseMode, turkish: bool) -> Vec<u32> {
    convert_case_chunks(input.chunks(64), mode, turkish)
}

/// Converts decoder batches while preserving PHP's bounded sigma lookbehind behavior.
///
/// Batches contain at most 64 codepoints. Encodings that emit multiple codepoints for
/// one character can use shorter batches, matching their decoder's boundaries.
pub fn convert_case_chunks<'a>(
    chunks: impl Iterator<Item = &'a [u32]> + Clone,
    mode: CaseMode,
    turkish: bool,
) -> Vec<u32> {
    convert_case_buffers(chunks, mode, turkish).concat()
}

/// Retains transformed buffer boundaries for encoders with observable invocation state.
pub(crate) fn convert_case_buffers<'a>(
    mut chunks: impl Iterator<Item = &'a [u32]> + Clone,
    mode: CaseMode,
    turkish: bool,
) -> Vec<Vec<u32>> {
    let simple = mode as u8 >= 4;
    let mode = mode as u8 % 4;
    let mut output = Vec::new();
    let mut buffers = Vec::new();
    let mut title_lower = false;
    let mut previous = [0u32; 192];
    let mut previous_len = 0;
    while let Some(chunk) = chunks.next() {
        assert!(chunk.len() <= 64, "mbstring decoder batch exceeds 64 codepoints");
        output.clear();
        let output_start = output.len();
        for (index, &code) in chunk.iter().enumerate() {
            let current_len = output.len() - output_start;
            let operation = if mode == 2 && title_lower { 1 } else { mode };
            if code > 0xffffff {
                output.push(code);
            } else if operation == 1 && !simple && code == 0x3a3
                && sigma_has_preceding_case(chunk, index, &previous, current_len, previous_len)
                && !sigma_has_following_case(&chunk[index + 1..], chunks.clone())
            {
                output.push(0x3c2);
            } else if turkish && append_turkish(code, operation, &mut output) {
                // ISO-8859-9's dotted/dotless I overrides the ordinary Unicode table.
            } else {
                let table = match operation {
                    0 => tables::UPPER,
                    1 => tables::LOWER,
                    2 => tables::TITLE,
                    _ => tables::FOLD,
                };
                tables::append_case(table, code, simple, &mut output);
            }
            if code <= 0xffffff && !is_case_ignorable(code) {
                title_lower = is_cased(code);
            }
            let new_len = output.len() - output_start;
            previous[current_len..new_len]
                .copy_from_slice(&output[output_start + current_len..]);
        }
        previous_len = output.len() - output_start;
        buffers.push(output.clone());
    }
    buffers
}

/// Searches the current input batch and the unoverwritten tail of PHP's prior output.
fn sigma_has_preceding_case(
    input: &[u32],
    index: usize,
    previous: &[u32],
    current_len: usize,
    previous_len: usize,
) -> bool {
    if let Some(&code) = input[..index].iter().rev().find(|&&code| !is_case_ignorable(code)) {
        return is_cased(code);
    }
    if current_len >= previous_len {
        return false;
    }
    previous[current_len..previous_len].iter().rev()
        .find(|&&code| is_cased(code) || !is_case_ignorable(code))
        .is_some_and(|&code| is_cased(code))
}

/// Searches forward with PHP's distinct in-batch and subsequent-batch property order.
fn sigma_has_following_case<'a>(
    input: &[u32],
    chunks: impl Iterator<Item = &'a [u32]>,
) -> bool {
    if let Some(&code) = input.iter().find(|&&code| !is_case_ignorable(code)) {
        return is_cased(code);
    }
    chunks.flatten().find(|&&code| is_cased(code) || !is_case_ignorable(code))
        .is_some_and(|&code| is_cased(code))
}

/// Appends ISO-8859-9's exceptional I mapping, returning whether it applied.
fn append_turkish(code: u32, mode: u8, output: &mut Vec<u32>) -> bool {
    let converted = match (mode, code) {
        (0 | 2, 0x69) => 0x130,
        (1 | 3, 0x49) => 0x131,
        (1 | 3, 0x130) => 0x69,
        _ => return false,
    };
    output.push(converted);
    true
}
