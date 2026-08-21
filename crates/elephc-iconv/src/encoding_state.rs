//! Purpose:
//! Owns the process-wide `input_encoding` / `output_encoding` / `internal_encoding`
//! trio that `iconv_get_encoding()` reports and `iconv_set_encoding()` updates.
//!
//! Called from:
//! - `crate::search` and `crate::mime` when a call omits its `$encoding` argument.
//! - `crate::abi` and Magician's `iconv_get_encoding` / `iconv_set_encoding` bindings.
//!
//! Key details:
//! - php-src stores these as ini entries that fall back to `default_charset`, so all
//!   three start at `UTF-8` and `iconv_set_encoding()` never validates the new value.
//! - Type names are matched case-insensitively, exactly like the ini lookup php-src does.
//! - The state is shared by the whole program, so a mutex keeps concurrent Magician
//!   callers from observing a torn value.

use std::sync::Mutex;

/// PHP's effective default for all three encodings, inherited from `default_charset`.
pub const DEFAULT_ENCODING: &str = "UTF-8";

/// One of the three encoding slots the extension exposes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncodingKind {
    /// `input_encoding`.
    Input,
    /// `output_encoding`.
    Output,
    /// `internal_encoding`, the fallback charset of the character-oriented functions.
    Internal,
}

impl EncodingKind {
    /// Returns the PHP-visible key used by `iconv_get_encoding()`'s array result.
    pub fn key(self) -> &'static str {
        match self {
            EncodingKind::Input => "input_encoding",
            EncodingKind::Output => "output_encoding",
            EncodingKind::Internal => "internal_encoding",
        }
    }

    /// Resolves one PHP `$type` argument, accepting any ASCII casing like php-src.
    pub fn parse(name: &[u8]) -> Option<Self> {
        for kind in [
            EncodingKind::Input,
            EncodingKind::Output,
            EncodingKind::Internal,
        ] {
            if name.eq_ignore_ascii_case(kind.key().as_bytes()) {
                return Some(kind);
            }
        }
        None
    }

    /// Returns every slot in the order `iconv_get_encoding("all")` reports them.
    pub fn all() -> [Self; 3] {
        [
            EncodingKind::Input,
            EncodingKind::Output,
            EncodingKind::Internal,
        ]
    }
}

/// The three encoding slots as currently configured.
struct EncodingState {
    input: String,
    output: String,
    internal: String,
}

/// Process-wide encoding configuration shared by every iconv entry point.
static STATE: Mutex<Option<EncodingState>> = Mutex::new(None);

/// Runs `action` against the live encoding state, initializing it on first use.
fn with_state<T>(action: impl FnOnce(&mut EncodingState) -> T) -> T {
    let mut guard = match STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let state = guard.get_or_insert_with(|| EncodingState {
        input: DEFAULT_ENCODING.to_string(),
        output: DEFAULT_ENCODING.to_string(),
        internal: DEFAULT_ENCODING.to_string(),
    });
    action(state)
}

/// Returns the charset currently configured for one slot.
pub fn get(kind: EncodingKind) -> String {
    with_state(|state| match kind {
        EncodingKind::Input => state.input.clone(),
        EncodingKind::Output => state.output.clone(),
        EncodingKind::Internal => state.internal.clone(),
    })
}

/// Replaces one slot's charset and reports success the way php-src does.
///
/// php-src writes the ini entry without validating the charset name, so any value is
/// accepted; only an unrecognized `$type` fails.
pub fn set(kind: EncodingKind, value: &[u8]) -> bool {
    let value = String::from_utf8_lossy(value).into_owned();
    with_state(|state| {
        match kind {
            EncodingKind::Input => state.input = value,
            EncodingKind::Output => state.output = value,
            EncodingKind::Internal => state.internal = value,
        }
        true
    })
}

/// Resolves the charset one call actually converts through.
///
/// php-src distinguishes two kinds of "absent": an omitted or `null` `$encoding`
/// falls back to `iconv.internal_encoding`, while an explicitly empty string reaches
/// the generic engine and resolves to `default_charset`. elephc has no ini surface,
/// so `default_charset` is the fixed `UTF-8` PHP ships with.
pub fn effective_charset(explicit: Option<&[u8]>) -> Vec<u8> {
    match explicit {
        None => get(EncodingKind::Internal).into_bytes(),
        Some(charset) if charset.is_empty() => DEFAULT_ENCODING.as_bytes().to_vec(),
        Some(charset) => charset.to_vec(),
    }
}
