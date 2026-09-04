//! Purpose:
//! Defines the integer option flags exposed by PHP's OpenSSL cipher API.
//!
//! Called from:
//! - The checker, name resolver, and constant prescan when resolving `OPENSSL_*` names.
//!
//! Key details:
//! - Values match PHP's ext/openssl constants and the elephc-crypto bridge option bits.

/// Tuple of `(name, value)` pairs for the supported OpenSSL cipher flags.
pub(crate) use elephc_builtin_contract::php_constants::OPENSSL_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the option bits match PHP and remain independent powers of two.
    #[test]
    fn openssl_option_bits_match_php() {
        assert_eq!(OPENSSL_INT_CONSTANTS, &[
            ("OPENSSL_RAW_DATA", 1),
            ("OPENSSL_ZERO_PADDING", 2),
            ("OPENSSL_DONT_ZERO_PAD_KEY", 4),
        ]);
    }
}
