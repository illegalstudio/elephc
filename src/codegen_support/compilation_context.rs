//! Purpose:
//! Stores per-thread compile profile, autoload metadata, and linked extension state.
//!
//! Called from:
//! - "crate::pipeline" before lowering and code generation.
//!
//! Key details:
//! - Thread-local storage isolates parallel compiler tests while keeping deep lowering APIs narrow.

use std::cell::{Cell, RefCell};

thread_local! {
    /// Number of `spl_autoload_register` closure rules extracted before code generation.
    static AUTOLOAD_RULE_COUNT: Cell<usize> = const { Cell::new(0) };
    /// Canonical PHP extension names supplied by linked bridge static libraries.
    static LINKED_EXTENSIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Selected PHP language profile and whether the current compilation uses web SAPI.
    static COMPILE_PROFILE: Cell<(crate::web_prelude::PhpVersion, bool)> =
        const { Cell::new((crate::web_prelude::PhpVersion::Php85, false)) };
    /// The compile-time `--ini KEY=VALUE` directive overrides of this compilation.
    static INI_OVERRIDES: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    /// Whether `opcache.restrict_api` denies this binary's calls into the OPcache API.
    static OPCACHE_API_RESTRICTED: Cell<bool> = const { Cell::new(false) };
    /// How many scripts the compile-time OPcache manifest holds.
    static OPCACHE_MANIFEST_LEN: Cell<usize> = const { Cell::new(0) };
}

/// Records how many scripts this binary's compile-time OPcache manifest holds.
///
/// They occupy hash slots in php-src like any cached script — the entry script and every static
/// `require` are compiled into the cache before a dynamic include runs — so the runtime tier's
/// capacity is the prime MINUS these. Read via [`opcache_manifest_len`].
pub fn set_opcache_manifest_len(len: usize) {
    OPCACHE_MANIFEST_LEN.with(|cell| cell.set(len));
}

/// How many scripts this compilation's OPcache manifest holds.
pub(crate) fn opcache_manifest_len() -> usize {
    OPCACHE_MANIFEST_LEN.with(Cell::get)
}

/// Records whether `opcache.restrict_api` denies this binary's OPcache API calls.
///
/// Decided ONCE, in the pipeline, by `opcache_prelude::restrict_api_denies` — the same verdict
/// the injected native bodies bake. Carried here because the eval bridge needs it too: an
/// OPcache call the compiler cannot see (a runtime-provided `eval()` source) never gets a
/// native body, and the interpreter's own handler must refuse it just the same. Read via
/// [`opcache_api_restricted`].
pub fn set_opcache_api_restricted(restricted: bool) {
    OPCACHE_API_RESTRICTED.with(|cell| cell.set(restricted));
}

/// Whether `opcache.restrict_api` denies this compilation's OPcache API calls.
pub(crate) fn opcache_api_restricted() -> bool {
    OPCACHE_API_RESTRICTED.with(Cell::get)
}

/// Records the PHP language profile and SAPI mode of the current compilation.
///
/// Called once from `pipeline::compile` before any prelude runs, so every later
/// phase (prescan constants, EIR lowering, per-instruction lowering) observes the
/// same pair. Read via [`compile_php_version`] / [`compile_is_web_sapi`].
pub fn set_compile_profile(php_version: crate::web_prelude::PhpVersion, web: bool) {
    COMPILE_PROFILE.with(|profile| profile.set((php_version, web)));
}

/// Returns the PHP language profile this compilation targets (default: the newest
/// maintained profile, matching `--php-version`'s own default).
pub(crate) fn compile_php_version() -> crate::web_prelude::PhpVersion {
    COMPILE_PROFILE.with(|profile| profile.get().0)
}

/// Returns whether this compilation is a `--web` build (default: `false`, i.e. CLI).
pub(crate) fn compile_is_web_sapi() -> bool {
    COMPILE_PROFILE.with(|profile| profile.get().1)
}

/// Sets the number of autoload rules registered.
pub fn set_autoload_rule_count(n: usize) {
    AUTOLOAD_RULE_COUNT.with(|c| c.set(n));
}

/// Returns the number of autoload rules registered.
pub fn autoload_rule_count() -> usize {
    AUTOLOAD_RULE_COUNT.with(|c| c.get())
}

/// Records the canonical PHP extension names of the bridge staticlibs linked
/// into the current compilation, so `extension_loaded()` / `get_loaded_extensions()`
/// report them in addition to the always-present core set. Set from the pipeline
/// before codegen; read via [`linked_extensions`]. Passing an empty vec (the
/// default) reports only the core extensions, matching a bridge-free program.
pub fn set_linked_extensions(extensions: Vec<String>) {
    LINKED_EXTENSIONS.with(|names| *names.borrow_mut() = extensions);
}

/// Returns the canonical PHP extension names of the bridges linked into the
/// current compilation (empty unless [`set_linked_extensions`] ran for it).
pub(crate) fn linked_extensions() -> Vec<String> {
    LINKED_EXTENSIONS.with(|names| names.borrow().clone())
}

/// Parses a PHP `memory_limit` quantity into bytes.
///
/// `-1` and `0` mean no lower limit (php-src refuses zero while memory is already in use and
/// keeps the previous value); `K`, `M`, and `G` use PHP's 1024-based multipliers.
#[cfg(test)]
pub fn parse_memory_limit_bytes(value: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed == "-1" || trimmed == "0" {
        return Ok(None);
    }
    let (digits, multiplier) = match trimmed.as_bytes().last() {
        Some(b'K' | b'k') => (&trimmed[..trimmed.len() - 1], 1024u64),
        Some(b'M' | b'm') => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024),
        Some(b'G' | b'g') => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024 * 1024),
        _ => (trimmed, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid memory_limit value `{value}`"));
    }
    let number = digits
        .parse::<u64>()
        .map_err(|_| format!("invalid memory_limit value `{value}`"))?;
    let bytes = number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("memory_limit value `{value}` overflows"))?;
    usize::try_from(bytes)
        .map(Some)
        .map_err(|_| format!("memory_limit value `{value}` overflows"))
}

/// Returns whether a positive PHP quantity exceeds `u64::MAX` before the signed mirror wraps it.
fn unsigned_quantity_overflows_u64(value: &str) -> bool {
    let bytes = value.trim().as_bytes();
    let mut cursor = 0;
    if bytes.get(cursor).is_some_and(|byte| matches!(byte, b'+' | b'-')) {
        cursor += 1;
    }
    if cursor >= bytes.len() {
        return false;
    }
    let radix = if bytes[cursor] == b'0' && cursor + 1 < bytes.len() {
        match bytes[cursor + 1] | 0x20 {
            b'x' => 16,
            b'b' => 2,
            _ => 8,
        }
    } else if bytes[cursor] == b'0' {
        8
    } else {
        10
    };
    if matches!(radix, 16 | 2) && cursor + 1 < bytes.len() {
        cursor += 2;
    }
    let digit = |byte: u8, radix: u32| -> Option<u32> {
        match (radix, byte) {
            (16, byte @ b'0'..=b'9') => Some(u32::from(byte - b'0')),
            (16, byte @ b'a'..=b'f') => Some(u32::from(byte - b'a' + 10)),
            (16, byte @ b'A'..=b'F') => Some(u32::from(byte - b'A' + 10)),
            (8, byte @ b'0'..=b'7') => Some(u32::from(byte - b'0')),
            (2, b'0'..=b'1') => Some(u32::from(byte - b'0')),
            (10, byte @ b'0'..=b'9') => Some(u32::from(byte - b'0')),
            _ => None,
        }
    };
    let mut magnitude = 0_u128;
    let mut digits = 0_usize;
    while cursor < bytes.len() {
        let Some(value) = digit(bytes[cursor], radix) else {
            break;
        };
        magnitude = magnitude * u128::from(radix) + u128::from(value);
        if magnitude > u128::from(u64::MAX) {
            return true;
        }
        cursor += 1;
        digits += 1;
    }
    if digits == 0 {
        return false;
    }
    while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }
    let last = bytes[bytes.len() - 1] | 0x20;
    let multiplier = match last {
        b'k' => 1024_u128,
        b'm' => 1024 * 1024,
        b'g' => 1024 * 1024 * 1024,
        _ => return false,
    };
    magnitude
        .checked_mul(multiplier)
        .is_none_or(|scaled| scaled > u128::from(u64::MAX))
}

/// Returns the unsigned `memory_limit` byte result for a positive quantity, including a
/// `K`/`M`/`G` suffix applied modulo 2^64.
///
/// The signed `zend_ini_parse_quantity` mirror reports a positive magnitude at or above 2^63 as
/// a negative sentinel BEFORE applying the suffix. `memory_limit` uses the unsigned parser
/// instead, where the factor wraps: `9223372036854784000K` is `(2^63 + 8192) * 1024`, which is
/// 8 MiB modulo 2^64. Returns `None` for a negative, malformed, or base-overflowed value; those
/// keep the signed path's existing unlimited/refusal result.
fn unsigned_scaled_quantity(value: &str) -> Option<u64> {
    let bytes = value.trim().as_bytes();
    if bytes.first() == Some(&b'-') {
        return None;
    }
    let cursor = usize::from(bytes.first() == Some(&b'+'));
    if cursor >= bytes.len() {
        return None;
    }
    let (radix, mut cursor) = if bytes[cursor] == b'0' && cursor + 1 < bytes.len() {
        match bytes[cursor + 1] | 0x20 {
            b'x' => (16_u32, cursor + 2),
            b'b' => (2_u32, cursor + 2),
            _ => (8_u32, cursor),
        }
    } else if bytes[cursor] == b'0' {
        (8_u32, cursor)
    } else {
        (10_u32, cursor)
    };
    let digit = |byte: u8, radix: u32| -> Option<u32> {
        match (radix, byte) {
            (16, byte @ b'0'..=b'9') => Some(u32::from(byte - b'0')),
            (16, byte @ b'a'..=b'f') => Some(u32::from(byte - b'a' + 10)),
            (16, byte @ b'A'..=b'F') => Some(u32::from(byte - b'A' + 10)),
            (8, byte @ b'0'..=b'7') => Some(u32::from(byte - b'0')),
            (2, b'0'..=b'1') => Some(u32::from(byte - b'0')),
            (10, byte @ b'0'..=b'9') => Some(u32::from(byte - b'0')),
            _ => None,
        }
    };
    let mut magnitude: u128 = 0;
    let mut digits = 0_usize;
    while cursor < bytes.len() {
        let Some(value) = digit(bytes[cursor], radix) else {
            break;
        };
        magnitude = magnitude * u128::from(radix) + u128::from(value);
        if magnitude > u128::from(u64::MAX) {
            return None;
        }
        cursor += 1;
        digits += 1;
    }
    if digits == 0 {
        return None;
    }
    // Only a spaced suffix (`12 M`) has bytes between the digits and the final multiplier
    // character; any other trailing data keeps the signed parser's warn-and-prefix result.
    while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }
    let factor = match bytes.last().copied().map(|byte| byte | 0x20) {
        Some(b'k') if cursor + 1 == bytes.len() => 1024_u64,
        Some(b'm') if cursor + 1 == bytes.len() => 1024 * 1024,
        Some(b'g') if cursor + 1 == bytes.len() => 1024 * 1024 * 1024,
        _ if cursor == bytes.len() => 1,
        _ => return None,
    };
    Some((magnitude as u64).wrapping_mul(factor))
}

/// Parses one startup `memory_limit` override with PHP's INI quantity rules.
///
/// `parse_ini_quantity` is the verified `zend_ini_parse_quantity` mirror used by the OPcache
/// directives; `memory_limit` accepts the same bases and suffixes (`0x800000`, `010`,
/// `0b1000`, `12 M`) and stores the parsed value even when it warns. The signed mirror
/// reports a positive value at or above `2^63` as a negative sentinel; startup treats that,
/// and any overflow result, as unlimited because the fixed arena already uses its capacity.
fn startup_memory_limit_quantity(value: &str) -> (Option<usize>, Option<String>) {
    let (parsed, mut diagnostic) = crate::opcache::directives::parse_ini_quantity(value);
    let negative = value.trim_start().starts_with('-');
    if negative && parsed > 0 {
        // The signed mirror wrapped a negative magnitude above 2^63 back into the positive
        // range. php-src's unsigned parser keeps that magnitude and treats the high result as
        // the full arena; install an 8 MiB ceiling only if the wrap is ignored.
        if diagnostic.is_none() {
            diagnostic = Some(format!(
                "Invalid quantity \"{value}\": value is out of range, using overflow result for backwards compatibility"
            ));
        }
        return (None, diagnostic);
    }
    if parsed == -1 {
        // `-1` is PHP's unlimited spelling; the signed mirror also lands here for a positive
        // quantity that wrapped to `u64::MAX`, which php-src accepts silently. Keep the
        // diagnostic for a genuinely negative overflow or a positive value beyond `u64::MAX`.
        return (
            None,
            if negative || unsigned_quantity_overflows_u64(value) {
                diagnostic
            } else {
                None
            },
        );
    }
    if parsed < 0 {
        if !negative {
            // A positive magnitude between 2^63 and u64::MAX wraps negative in the signed
            // mirror. The unsigned parser applies a suffix modulo 2^64, which can bring the
            // result back below 2^63 (`9223372036854784000K` is 8 MiB); only a value that
            // stays at or above 2^63 is the arena's unlimited spelling.
            if let Some(scaled) = unsigned_scaled_quantity(value) {
                if scaled < (1_u64 << 63) {
                    return (Some(scaled as usize), diagnostic);
                }
            }
            return (
                None,
                if unsigned_quantity_overflows_u64(value) {
                    diagnostic
                } else {
                    None
                },
            );
        }
        if parsed == i64::MIN {
            return (None, diagnostic);
        }
        if diagnostic.is_none() {
            diagnostic = Some(format!(
                "Invalid quantity \"{value}\": value is out of range, using overflow result for backwards compatibility"
            ));
        }
        return (Some(parsed.unsigned_abs() as usize), diagnostic);
    }
    (Some(parsed as usize), diagnostic)
}

/// Parses a startup `memory_limit` override, preserving PHP's zero-as-a-number rule.
///
/// Startup must distinguish unlimited (`-1`) from a finite zero that PHP refuses because its
/// 2 MiB startup allocation is already live. A malformed quantity follows php-src's
/// warn-and-store path instead of becoming a hard compile error.
pub fn parse_startup_memory_limit_bytes(value: &str) -> Result<Option<usize>, String> {
    let (bytes, diagnostic) = startup_memory_limit_quantity(value);
    if let Some(diagnostic) = diagnostic {
        eprintln!("Warning: Invalid \"memory_limit\" setting. {diagnostic}");
    }
    Ok(bytes)
}

/// Returns the silent parsed value of one startup `memory_limit` override.
///
/// The floor and arena sizing run several times over the same merged override list, so they
/// must not emit php-src's quantity warning themselves; the CLI calls
/// [`parse_startup_memory_limit_bytes`] once when it reports the final override list.
pub(crate) fn startup_memory_limit_value(value: &str) -> Option<usize> {
    startup_memory_limit_quantity(value).0
}

/// Returns the last compile-time `memory_limit` override in bytes, if one was supplied.
pub(crate) fn memory_limit_bytes() -> Option<usize> {
    INI_OVERRIDES.with(|entries| {
        entries
            .borrow()
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case("memory_limit"))
            .and_then(|(_, value)| startup_memory_limit_quantity(value).0)
    })
}

/// Records this compilation's `--ini` directive overrides.
///
/// Set from the pipeline alongside the compile profile, because the values are
/// consumed far below the parameter list that carries them — the OPcache runtime
/// cache configuration is baked in per-instruction lowering. Read via
/// [`ini_overrides`].
pub fn set_ini_overrides(overrides: Vec<(String, String)>) {
    INI_OVERRIDES.with(|entries| *entries.borrow_mut() = overrides);
}

/// Parses one PHP `memory_limit` value into bytes; `-1`/`0` means unlimited (`None`).
#[allow(dead_code)]
pub fn parse_memory_limit_bytes(value: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed == "-1" || trimmed == "0" {
        return Ok(None);
    }
    let (digits, multiplier) = match trimmed.as_bytes().last() {
        Some(b'K' | b'k') => (&trimmed[..trimmed.len() - 1], 1024u64),
        Some(b'M' | b'm') => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024),
        Some(b'G' | b'g') => (&trimmed[..trimmed.len() - 1], 1024u64 * 1024 * 1024),
        _ => (trimmed, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid memory_limit value `{value}`"));
    }
    let number = digits
        .parse::<u64>()
        .map_err(|_| format!("invalid memory_limit value `{value}`"))?;
    let bytes = number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("memory_limit value `{value}` overflows"))?;
    usize::try_from(bytes)
        .map(Some)
        .map_err(|_| format!("memory_limit value `{value}` overflows"))
}

/// Interprets a PHP INI boolean value with `zend_ini_parse_bool`'s spellings and fallback.
///
/// Both the compiler pipeline and the codegen test harness read `--ini` overrides through this
/// function, so a fixture cannot silently exercise a different `tidy.clean_output` value than
/// the CLI would.
pub fn ini_bool(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "true" | "yes" | "on" => true,
        "false" | "no" | "off" | "none" => false,
        _ => {
            // php-src falls back to `strtol(value, NULL, 10) != 0`: leading
            // whitespace and a sign are accepted, and parsing stops at the
            // first non-digit. An overflowing quantity is still nonzero.
            let trimmed = lower.trim_start();
            let bytes = trimmed.as_bytes();
            let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
            let digit_start = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == digit_start {
                return false;
            }
            trimmed[..index]
                .parse::<i128>()
                .map_or(true, |quantity| quantity != 0)
        }
    }
}

/// Returns this compilation's `--ini` directive overrides (empty unless
/// [`set_ini_overrides`] ran for it, which is the no-override default).
pub fn ini_overrides() -> Vec<(String, String)> {
    INI_OVERRIDES.with(|entries| entries.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::parse_memory_limit_bytes;

    /// Verifies PHP's binary suffixes and the unlimited spellings.
    #[test]
    fn memory_limit_quantities_use_binary_multipliers() {
        assert_eq!(parse_memory_limit_bytes("8M"), Ok(Some(8 * 1024 * 1024)));
        assert_eq!(parse_memory_limit_bytes("16m"), Ok(Some(16 * 1024 * 1024)));
        assert_eq!(parse_memory_limit_bytes("1G"), Ok(Some(1024 * 1024 * 1024)));
        assert_eq!(parse_memory_limit_bytes("65536"), Ok(Some(65_536)));
        assert_eq!(parse_memory_limit_bytes("-1"), Ok(None));
        assert_eq!(parse_memory_limit_bytes("0"), Ok(None));
    }

    /// Verifies malformed values and overflowing quantities are rejected rather than clamped.
    #[test]
    fn invalid_memory_limit_is_rejected() {
        assert!(parse_memory_limit_bytes("oops").is_err());
        assert!(parse_memory_limit_bytes("12MB").is_err());
        assert!(parse_memory_limit_bytes("999999999999999999999G").is_err());
    }

    /// Verifies a suffix applies to the unsigned magnitude, so a value above `i64::MAX`
    /// wraps modulo 2^64 instead of becoming unlimited before the multiplier.
    #[test]
    fn startup_memory_limit_applies_suffix_to_the_unsigned_magnitude() {
        assert_eq!(
            super::startup_memory_limit_value("9223372036854784000K"),
            Some(8 * 1024 * 1024)
        );
    }

    /// Verifies the codegen-facing byte query applies the same unsigned startup result.
    #[test]
    fn memory_limit_bytes_uses_the_unsigned_startup_quantity() {
        super::set_ini_overrides(vec![(
            "memory_limit".to_string(),
            "9223372036854784000K".to_string(),
        )]);
        assert_eq!(super::memory_limit_bytes(), Some(8 * 1024 * 1024));
        super::set_ini_overrides(Vec::new());
    }

    /// Verifies startup parsing keeps `-1` unlimited but treats zero as a finite refused value.
    #[test]
    fn startup_memory_limit_distinguishes_unlimited_from_zero() {
        assert_eq!(super::parse_startup_memory_limit_bytes("-1"), Ok(None));
        assert_eq!(super::parse_startup_memory_limit_bytes("0"), Ok(Some(0)));
        assert_eq!(super::parse_startup_memory_limit_bytes("0K"), Ok(Some(0)));
        assert_eq!(
            super::parse_startup_memory_limit_bytes("1M"),
            Ok(Some(1024 * 1024))
        );
    }

    /// Verifies startup parsing accepts php-src's hexadecimal, octal, binary, and spaced
    /// suffixes, and maps unsigned overflow to the unlimited arena.
    #[test]
    fn startup_memory_limit_uses_php_quantity_rules() {
        assert_eq!(
            super::parse_startup_memory_limit_bytes("0x800000"),
            Ok(Some(8 * 1024 * 1024))
        );
        assert_eq!(super::parse_startup_memory_limit_bytes("010"), Ok(Some(8)));
        assert_eq!(super::parse_startup_memory_limit_bytes("0b1000"), Ok(Some(8)));
        assert_eq!(
            super::parse_startup_memory_limit_bytes("12 M"),
            Ok(Some(12 * 1024 * 1024))
        );
        assert_eq!(
            super::parse_startup_memory_limit_bytes("18446744073709551615"),
            Ok(None)
        );
        assert_eq!(
            super::parse_startup_memory_limit_bytes("-18446744073701163008"),
            Ok(None)
        );
    }
}
