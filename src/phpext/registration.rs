//! Purpose:
//! Generates the host-owned registration and lifecycle shell of a PHP extension.
//!
//! Called from:
//! - The producer adapter generator and real-PHP registration integration tests.
//!
//! Key details:
//! - Uses the selected PHP SDK's module layout, build ID, and TSRM macros.
//! - Native runtime activation and function entries are added by the producer invocation layer.

/// Renders an empty module registration shell with real host lifecycle and MINFO callbacks.
pub fn render_registration(name: &str, version: &str) -> Result<String, String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        || name.as_bytes()[0].is_ascii_digit()
    {
        return Err("PHP module name must be a lowercase C identifier".to_string());
    }
    if version.is_empty() || !version.bytes().all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\') {
        return Err("PHP module version must be a nonempty printable ASCII token without quotes or backslashes".to_string());
    }
    Ok(include_str!("registration.c").replace("@MODULE@", name).replace("@VERSION@", version)
        .replace("@IDENT@", &format!("elephc_generated_{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects metadata that would create invalid C symbols or inject generated source.
    #[test]
    fn validates_module_metadata() {
        for name in ["", "9module", "Demo", "foo-bar", "foo\nbar", "foo\"bar"] {
            assert!(render_registration(name, "1.0").is_err());
        }
        for version in ["", "bad\"version", "bad\\version", "bad\nversion", "a b"] {
            assert!(render_registration("demo", version).is_err());
        }
        assert!(render_registration("demo_2", "1.0.0-dev+abc").is_ok());
    }
}
