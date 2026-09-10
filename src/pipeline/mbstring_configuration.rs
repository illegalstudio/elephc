//! Purpose:
//! Carries mbstring startup defaults and supplied settings into the native shared engine.
//!
//! Called from:
//! - Backend configuration after optional mbstring capability selection.
//!
//! Key details:
//! - Effective core encodings follow php_get_*_encoding inheritance and C-string boundaries.
//! - The bridge owns directive validation, registration order, and duplicate-key handling.
//! - Default-only programs initialize shared state without selecting the optional MIME provider.

/// Selects relevant startup overrides and resolves the three core encoding fallback chains.
pub(super) fn arguments(overrides: &[(String, String)]) -> Vec<Vec<u8>> {
    let directive = |name: &str| {
        use elephc_builtin_contract::mbstring_abi::ini::{catalog, core};
        catalog::lookup(name.as_bytes()).is_some() || core::lookup(name.as_bytes()).is_some()
            || matches!(name, "default_mimetype" | "default_charset")
    };
    let core = ["default_charset", "internal_encoding", "input_encoding", "output_encoding"];
    let effective = |name: &str| overrides.iter().rev().find(|(key, _)| key == name)
        .map(|(_, value)| value.as_bytes().split(|byte| *byte == 0).next().unwrap())
        .filter(|value| !value.is_empty());
    let raw_charset = overrides.iter().rev().find(|(name, _)| name == "default_charset").map(|(_, value)| value.as_bytes());
    let charset = raw_charset.filter(|value| !value.is_empty() && !value.iter().any(|byte| matches!(byte, 0 | b'\r' | b'\n')))
        .unwrap_or(b"UTF-8");
    let mut args: Vec<_> = core[1..].iter().map(|name| effective(name).unwrap_or(charset).to_vec()).collect();
    for (name, value) in overrides.iter().filter(|(name, _)| directive(name)) {
        args.push(name.as_bytes().to_vec());
        args.push(value.as_bytes().to_vec());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves raw mbstring values while resolving last-wins core defaults independently.
    #[test]
    fn mbstring_startup_argument_inheritance() {
        let settings = [("default_charset", "SJIS"), ("input_encoding", "ASCII"),
            ("input_encoding", ""), ("internal_encoding", "8bit\0ignored"),
            ("mbstring.language", "Japanese"), ("mbstring.language", "neutral"),
            ("opcache.enable_cli", "1")].map(|(key, value)| (key.into(), value.into()));
        let expected: Vec<_> = ["8bit", "SJIS", "SJIS", "default_charset", "SJIS", "mbstring.language", "Japanese",
            "mbstring.language", "neutral"].map(|value| value.as_bytes().to_vec()).into();
        assert_eq!(arguments(&settings), expected);
    }

    /// Initializes default-only programs while excluding unrelated and unknown startup directives.
    #[test]
    fn mbstring_startup_configuration_includes_defaults() {
        let defaults = [b"UTF-8".to_vec(), b"UTF-8".to_vec(), b"UTF-8".to_vec()].to_vec();
        assert_eq!(arguments(&[]), defaults);
        assert_eq!(arguments(&[("opcache.enable_cli".into(), "1".into())]), defaults);
        assert_eq!(arguments(&[("mbstring.unknown".into(), "1".into())]), defaults);
        assert_eq!(arguments(&[("MBSTRING.language".into(), "Japanese".into())]), defaults);
        assert_eq!(arguments(&[("default_charset".into(), "\0SJIS".into())]),
            [b"UTF-8".as_slice(), b"UTF-8", b"UTF-8", b"default_charset", b"\0SJIS"].map(<[u8]>::to_vec).to_vec());
    }

    /// Carries raw Core query overrides to the same native configuration as mbstring directives.
    #[test]
    fn mbstring_startup_core_query_directives() {
        let settings = [("arg_separator.input", ";&"), ("max_input_vars", "2K"),
            ("max_input_nesting_level", "0"), ("display_errors", "stderr"),
            ("display_errors", "256"), ("DISPLAY_ERRORS", "0"), ("max_input_vars\0", "0")]
            .map(|(key, value)| (key.into(), value.into()));
        let expected: Vec<_> = ["UTF-8", "UTF-8", "UTF-8", "arg_separator.input", ";&",
            "max_input_vars", "2K", "max_input_nesting_level", "0", "display_errors", "stderr",
            "display_errors", "256"].map(|value| value.as_bytes().to_vec()).into();
        assert_eq!(arguments(&settings), expected);
    }

    /// Carries raw MIME defaults and rejects a malformed charset before resolving encoding inheritance.
    #[test]
    fn mbstring_startup_response_defaults() {
        let settings = [("default_mimetype", "application/json"), ("default_charset", "bad\ncharset")]
            .map(|(key, value)| (key.into(), value.into()));
        let expected: Vec<_> = ["UTF-8", "UTF-8", "UTF-8", "default_mimetype", "application/json", "default_charset", "bad\ncharset"]
            .map(|value| value.as_bytes().to_vec()).into();
        assert_eq!(arguments(&settings), expected);
        assert_eq!(arguments(&[("default_mimetype".into(), "".into())]),
            ["UTF-8", "UTF-8", "UTF-8", "default_mimetype", ""].map(|value| value.as_bytes().to_vec()).to_vec());
    }
}
