//! Purpose:
//! Builds MySQL TLS options and maps PDO cipher strings to upstream mysql's typed API.
//!
//! Called from:
//! - `super::apply_ssl_opts()` before opening a MySQL connection.
//!
//! Key details:
//! - Preserve OpenSSL aliases, provider ordering, and errors for unsupported selections.
//! - Never pass an empty selection to upstream, where it restores the default suites.

use mysql::{ClientIdentity, SslOpts};
use rustls::SupportedCipherSuite;
use std::path::PathBuf;

/// Parses packed CA, client-identity and verification options, plus PDO cipher names.
pub(super) fn parse_ssl_config(
    ssl_config: &str,
    cipher: Option<&str>,
) -> Result<SslOpts, String> {
    install_crypto_provider();
    let mut ca: Option<String> = None;
    let mut cert: Option<String> = None;
    let mut key: Option<String> = None;
    let mut verify = true;
    for pair in ssl_config.split(';') {
        let Some((k, v)) = pair.trim().split_once('=') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim() {
            "ca" => ca = Some(v),
            "cert" => cert = Some(v),
            "key" => key = Some(v),
            "verify" => verify = v != "0",
            _ => {}
        }
    }

    let mut ssl = SslOpts::default();
    if let Some(ca) = ca {
        ssl = ssl.with_root_cert_path(Some(PathBuf::from(ca)));
    }
    if let (Some(cert), Some(key)) = (cert, key) {
        ssl = ssl.with_client_identity(Some(ClientIdentity::new(
            PathBuf::from(cert),
            PathBuf::from(key),
        )));
    }
    if !verify {
        ssl = ssl
            .with_danger_skip_domain_validation(true)
            .with_danger_accept_invalid_certs(true);
    }
    if let Some(cipher) = cipher {
        ssl = ssl.with_cipher_suites(&select_cipher_suites(cipher)?);
    }
    Ok(ssl)
}

/// Resolves IANA and OpenSSL names against ring without broadening an empty match.
fn select_cipher_suites(cipher: &str) -> Result<Vec<SupportedCipherSuite>, String> {
    let requested: Vec<String> = cipher
        .split([':', ','])
        .map(|name| name.trim().to_ascii_uppercase().replace('-', "_"))
        .filter(|name| !name.is_empty())
        .collect();
    let suites: Vec<_> = rustls::crypto::ring::default_provider()
        .cipher_suites
        .into_iter()
        .filter(|suite| {
            let iana = format!("{:?}", suite.suite());
            let alias = match iana.as_str() {
                "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256" => "ECDHE_RSA_AES128_GCM_SHA256",
                "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384" => "ECDHE_RSA_AES256_GCM_SHA384",
                "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256" => "ECDHE_RSA_CHACHA20_POLY1305",
                "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256" => "ECDHE_ECDSA_AES128_GCM_SHA256",
                "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384" => "ECDHE_ECDSA_AES256_GCM_SHA384",
                "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256" => "ECDHE_ECDSA_CHACHA20_POLY1305",
                _ => iana.as_str(),
            };
            requested.iter().any(|name| name == &iana || name == alias)
        })
        .collect();
    if suites.is_empty() {
        return Err("no requested MySQL TLS cipher is supported by rustls".to_string());
    }
    Ok(suites)
}

/// Installs ring once for upstream mysql's provider-less rustls configuration builder.
fn install_crypto_provider() {
    use std::sync::Once;
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // Another TLS path may already have installed the same process default.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use super::parse_ssl_config;

    /// Every previously accepted OpenSSL TLS 1.2 alias selects its exact IANA suite.
    #[test]
    fn cipher_aliases_match_iana_suites() {
        for (alias, iana) in [
            ("ECDHE-RSA-AES128-GCM-SHA256", "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"),
            ("ECDHE-RSA-AES256-GCM-SHA384", "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"),
            ("ECDHE-RSA-CHACHA20-POLY1305", "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256"),
            ("ECDHE-ECDSA-AES128-GCM-SHA256", "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256"),
            ("ECDHE-ECDSA-AES256-GCM-SHA384", "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384"),
            ("ECDHE-ECDSA-CHACHA20-POLY1305", "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256"),
        ] {
            for name in [alias, iana] {
                let ssl = parse_ssl_config("", Some(name)).unwrap();
                assert_eq!(ssl.cipher_suites().unwrap(), &[iana.to_string()], "{name}");
            }
        }
    }

    /// Case, whitespace, both separators and duplicates retain one copy of each suite.
    #[test]
    fn cipher_lists_preserve_provider_order() {
        let ssl = parse_ssl_config(
            "",
            Some(" ecdhe-rsa-aes128-gcm-sha256 : tls13_aes_128_gcm_sha256, ECDHE_RSA_AES128_GCM_SHA256 ,"),
        ).unwrap();
        assert_eq!(ssl.cipher_suites().unwrap(), &[
            "TLS13_AES_128_GCM_SHA256".to_string(),
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".to_string(),
        ]);
    }

    /// A mixed list keeps the supported intersection without restoring default suites.
    #[test]
    fn cipher_lists_ignore_unsupported_names_when_a_supported_suite_remains() {
        let ssl = parse_ssl_config(
            "",
            Some("AES128-SHA:ECDHE-RSA-AES128-GCM-SHA256:UNKNOWN"),
        ).unwrap();
        assert_eq!(ssl.cipher_suites().unwrap(), &[
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".to_string(),
        ]);
    }

    /// Empty and wholly unsupported lists must fail instead of resetting upstream defaults.
    #[test]
    fn cipher_lists_without_supported_suites_are_rejected() {
        for cipher in ["", " ", ":,,", "AES128-SHA", "TLS_NULL_WITH_NULL_NULL"] {
            let error = super::super::apply_ssl_opts(mysql::OptsBuilder::new(), "", Some(cipher))
                .err().expect("unsupported cipher selection must fail before connecting");
            assert!(error.contains("no requested MySQL TLS cipher"), "{cipher:?}: {error}");
        }
    }

    /// A connection without an explicit cipher restriction retains provider defaults.
    #[test]
    fn absent_cipher_uses_provider_defaults() {
        assert!(parse_ssl_config("", None).unwrap().cipher_suites().is_none());
    }
}
