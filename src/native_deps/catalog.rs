//! Purpose:
//! Owns the trusted, compiled-in native package catalog and immutable recipes.
//!
//! Called from:
//! - Manifest validation, lock expansion, installation, and compilation resolution.
//!
//! Key details:
//! - Project files select only catalogued names and exact versions; they never supply executable data.

use crate::codegen_support::platform::Target;

use super::error::{NativeError, NativeErrorKind};

/// Verified upstream source metadata embedded in the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceArchive {
    pub https_url: &'static str,
    pub sha256: &'static str,
    pub exact_size: u64,
    pub body_limit: u64,
}

/// One immutable version and recipe in the trusted catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageVersion {
    pub version: &'static str,
    pub source: SourceArchive,
    pub recipe_revision: u32,
    pub dependencies: &'static [&'static str],
    pub supported_targets: &'static [&'static str],
    pub ordered_link_outputs: &'static [&'static str],
    pub retained_headers: &'static [&'static str],
    pub provides: &'static [&'static str],
}

/// A named package and its default exact version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageSpec {
    pub name: &'static str,
    pub default_version: &'static str,
    pub versions: &'static [PackageVersion],
}

const TARGETS: &[&str] = &["macos-aarch64", "linux-aarch64", "linux-x86_64"];
const PCRE2_ARCHIVES: &[&str] = &[
    "lib/libelephc_pcre2_shim.a",
    "lib/libpcre2-posix.a",
    "lib/libpcre2-8.a",
];
const PCRE2_HEADERS: &[&str] = &["include/pcre2.h", "include/pcre2posix.h"];
const ZLIB_ARCHIVES: &[&str] = &["lib/libz.a"];
const ZLIB_HEADERS: &[&str] = &["include/zlib.h", "include/zconf.h"];
const OPENSSL_ARCHIVES: &[&str] = &["lib/libssl.a", "lib/libcrypto.a"];
/// Every public OpenSSL 3.5.7 header curl's TLS backend transitively includes: the static headers
/// shipped in the release tarball plus the ones OpenSSL's own `Configure`/`make build_libs`
/// generates from `.h.in` templates. Verified against a real `no-shared no-legacy` build (see
/// `openssl_catalog_snapshot_is_exact`).
const OPENSSL_HEADERS: &[&str] = &[
    "include/openssl/aes.h", "include/openssl/asn1.h", "include/openssl/asn1err.h", "include/openssl/asn1t.h",
    "include/openssl/async.h", "include/openssl/asyncerr.h", "include/openssl/bio.h", "include/openssl/bioerr.h",
    "include/openssl/blowfish.h", "include/openssl/bn.h", "include/openssl/bnerr.h", "include/openssl/buffer.h",
    "include/openssl/buffererr.h", "include/openssl/byteorder.h", "include/openssl/camellia.h", "include/openssl/cast.h",
    "include/openssl/cmac.h", "include/openssl/cmp_util.h", "include/openssl/cmp.h", "include/openssl/cmperr.h",
    "include/openssl/cms.h", "include/openssl/cmserr.h", "include/openssl/comp.h", "include/openssl/comperr.h",
    "include/openssl/conf_api.h", "include/openssl/conf.h", "include/openssl/conferr.h", "include/openssl/configuration.h",
    "include/openssl/conftypes.h", "include/openssl/core_dispatch.h", "include/openssl/core_names.h", "include/openssl/core_object.h",
    "include/openssl/core.h", "include/openssl/crmf.h", "include/openssl/crmferr.h", "include/openssl/crypto.h",
    "include/openssl/cryptoerr_legacy.h", "include/openssl/cryptoerr.h", "include/openssl/ct.h", "include/openssl/cterr.h",
    "include/openssl/decoder.h", "include/openssl/decodererr.h", "include/openssl/des.h", "include/openssl/dh.h",
    "include/openssl/dherr.h", "include/openssl/dsa.h", "include/openssl/dsaerr.h", "include/openssl/dtls1.h",
    "include/openssl/e_os2.h", "include/openssl/e_ostime.h", "include/openssl/ebcdic.h", "include/openssl/ec.h",
    "include/openssl/ecdh.h", "include/openssl/ecdsa.h", "include/openssl/ecerr.h", "include/openssl/encoder.h",
    "include/openssl/encodererr.h", "include/openssl/engine.h", "include/openssl/engineerr.h", "include/openssl/err.h",
    "include/openssl/ess.h", "include/openssl/esserr.h", "include/openssl/evp.h", "include/openssl/evperr.h",
    "include/openssl/fips_names.h", "include/openssl/fipskey.h", "include/openssl/hmac.h", "include/openssl/hpke.h",
    "include/openssl/http.h", "include/openssl/httperr.h", "include/openssl/idea.h", "include/openssl/indicator.h",
    "include/openssl/kdf.h", "include/openssl/kdferr.h", "include/openssl/lhash.h", "include/openssl/macros.h",
    "include/openssl/md2.h", "include/openssl/md4.h", "include/openssl/md5.h", "include/openssl/mdc2.h",
    "include/openssl/ml_kem.h", "include/openssl/modes.h", "include/openssl/obj_mac.h", "include/openssl/objects.h",
    "include/openssl/objectserr.h", "include/openssl/ocsp.h", "include/openssl/ocsperr.h", "include/openssl/opensslconf.h",
    "include/openssl/opensslv.h", "include/openssl/ossl_typ.h", "include/openssl/param_build.h", "include/openssl/params.h",
    "include/openssl/pem.h", "include/openssl/pem2.h", "include/openssl/pemerr.h", "include/openssl/pkcs12.h",
    "include/openssl/pkcs12err.h", "include/openssl/pkcs7.h", "include/openssl/pkcs7err.h", "include/openssl/prov_ssl.h",
    "include/openssl/proverr.h", "include/openssl/provider.h", "include/openssl/quic.h", "include/openssl/rand.h",
    "include/openssl/randerr.h", "include/openssl/rc2.h", "include/openssl/rc4.h", "include/openssl/rc5.h",
    "include/openssl/ripemd.h", "include/openssl/rsa.h", "include/openssl/rsaerr.h", "include/openssl/safestack.h",
    "include/openssl/seed.h", "include/openssl/self_test.h", "include/openssl/sha.h", "include/openssl/srp.h",
    "include/openssl/srtp.h", "include/openssl/ssl.h", "include/openssl/ssl2.h", "include/openssl/ssl3.h",
    "include/openssl/sslerr_legacy.h", "include/openssl/sslerr.h", "include/openssl/stack.h", "include/openssl/store.h",
    "include/openssl/storeerr.h", "include/openssl/symhacks.h", "include/openssl/thread.h", "include/openssl/tls1.h",
    "include/openssl/trace.h", "include/openssl/ts.h", "include/openssl/tserr.h", "include/openssl/txt_db.h",
    "include/openssl/types.h", "include/openssl/ui.h", "include/openssl/uierr.h", "include/openssl/whrlpool.h",
    "include/openssl/x509_acert.h", "include/openssl/x509_vfy.h", "include/openssl/x509.h", "include/openssl/x509err.h",
    "include/openssl/x509v3.h", "include/openssl/x509v3err.h",
];
const CURL_ARCHIVES: &[&str] = &["lib/libcurl.a"];
const CURL_HEADERS: &[&str] = &[
    "include/curl/curl.h",
    "include/curl/curlver.h",
    "include/curl/easy.h",
    "include/curl/header.h",
    "include/curl/mprintf.h",
    "include/curl/multi.h",
    "include/curl/options.h",
    "include/curl/stdcheaders.h",
    "include/curl/system.h",
    "include/curl/typecheck-gcc.h",
    "include/curl/urlapi.h",
    "include/curl/websockets.h",
];
const PCRE2_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "10.47",
    source: SourceArchive {
        https_url: "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.47/pcre2-10.47.tar.gz",
        sha256: "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16",
        exact_size: 2_792_969,
        body_limit: 32 * 1024 * 1024,
    },
    recipe_revision: 2,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: PCRE2_ARCHIVES,
    retained_headers: PCRE2_HEADERS,
    provides: &["pcre2"],
}];
const ZLIB_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "1.3.2",
    source: SourceArchive {
        https_url:
            "https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz",
        sha256: "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
        exact_size: 1_502_830,
        body_limit: 16 * 1024 * 1024,
    },
    recipe_revision: 1,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: ZLIB_ARCHIVES,
    retained_headers: ZLIB_HEADERS,
    provides: &["zlib"],
}];
/// Frozen source identity copied verbatim from `scripts/docs/curl_surface.json` (Task 1). OpenSSL
/// is used only as libcurl's TLS backend; `openssl_encrypt`/`hash()` stay on `elephc-crypto`.
const OPENSSL_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "3.5.7",
    source: SourceArchive {
        https_url:
            "https://github.com/openssl/openssl/releases/download/openssl-3.5.7/openssl-3.5.7.tar.gz",
        sha256: "a8c0d28a529ca480f9f36cf5792e2cd21984552a3c8e4aa11a24aa31aeac98e8",
        exact_size: 53_153_930,
        body_limit: 128 * 1024 * 1024,
    },
    recipe_revision: 1,
    dependencies: &[],
    supported_targets: TARGETS,
    ordered_link_outputs: OPENSSL_ARCHIVES,
    retained_headers: OPENSSL_HEADERS,
    provides: &["openssl"],
}];
/// Frozen source identity copied verbatim from `scripts/docs/curl_surface.json` (Task 1). Statically
/// linked against the managed `openssl` and `zlib` packages; never a system libcurl/OpenSSL.
const CURL_VERSIONS: &[PackageVersion] = &[PackageVersion {
    version: "8.21.0",
    source: SourceArchive {
        https_url: "https://curl.se/download/curl-8.21.0.tar.gz",
        sha256: "d9b327997999045a24cda50f3983e69e51c516bd8be6ef9842fc7f99135e33bb",
        exact_size: 4_298_225,
        body_limit: 32 * 1024 * 1024,
    },
    recipe_revision: 1,
    dependencies: &["openssl", "zlib"],
    supported_targets: TARGETS,
    ordered_link_outputs: CURL_ARCHIVES,
    retained_headers: CURL_HEADERS,
    provides: &["curl"],
}];
const PACKAGES: &[PackageSpec] = &[
    PackageSpec {
        name: "pcre2",
        default_version: "10.47",
        versions: PCRE2_VERSIONS,
    },
    PackageSpec {
        name: "zlib",
        default_version: "1.3.2",
        versions: ZLIB_VERSIONS,
    },
    PackageSpec {
        name: "openssl",
        default_version: "3.5.7",
        versions: OPENSSL_VERSIONS,
    },
    PackageSpec {
        name: "curl",
        default_version: "8.21.0",
        versions: CURL_VERSIONS,
    },
];

/// Returns every package in deterministic catalog order.
pub fn packages() -> &'static [PackageSpec] {
    PACKAGES
}

/// Looks up a package and reports the complete known-name set on failure.
pub fn package(name: &str) -> Result<&'static PackageSpec, NativeError> {
    PACKAGES.iter().find(|package| package.name == name).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Catalog,
            format!("unknown native package '{name}'; known packages: {}", known_names()),
        )
    })
}

/// Resolves an exact catalog version, using the package default when omitted.
pub fn version(name: &str, requested: Option<&str>) -> Result<&'static PackageVersion, NativeError> {
    let package = package(name)?;
    let selected = requested.unwrap_or(package.default_version);
    package.versions.iter().find(|version| version.version == selected).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Catalog,
            format!("native package '{name}' has no catalogued exact version '{selected}'"),
        )
    })
}

/// Validates that a package recipe supports the selected compiler backend target.
pub fn ensure_target(version: &PackageVersion, target: Target) -> Result<(), NativeError> {
    if !target.supports_current_backend()
        || !version.supported_targets.iter().any(|candidate| *candidate == target.as_str())
    {
        return Err(NativeError::new(
            NativeErrorKind::Catalog,
            format!("native package does not support target '{}'", target.as_str()),
        ));
    }
    Ok(())
}

/// Returns catalog package names as a stable comma-separated diagnostic list.
pub fn known_names() -> String {
    PACKAGES.iter().map(|package| package.name).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the official PCRE2 source identity and immutable archive order.
    #[test]
    fn pcre2_catalog_snapshot_is_exact() {
        let version = version("pcre2", None).expect("catalogue entry");
        assert_eq!(version.version, "10.47");
        assert_eq!(version.source.exact_size, 2_792_969);
        assert_eq!(version.source.sha256, "c08ae2388ef333e8403e670ad70c0a11f1eed021fd88308d7e02f596fcd9dc16");
        assert_eq!(version.ordered_link_outputs, PCRE2_ARCHIVES);
        assert_eq!(version.supported_targets, TARGETS);
    }

    /// Verifies the official zlib source identity and static archive contract.
    #[test]
    fn zlib_catalog_snapshot_is_exact() {
        let version = version("zlib", None).expect("catalogue entry");
        assert_eq!(version.version, "1.3.2");
        assert_eq!(version.source.exact_size, 1_502_830);
        assert_eq!(
            version.source.sha256,
            "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16"
        );
        assert_eq!(version.ordered_link_outputs, ZLIB_ARCHIVES);
        assert_eq!(version.retained_headers, ZLIB_HEADERS);
        assert_eq!(version.supported_targets, TARGETS);
    }

    /// Verifies unknown package and version inputs fail closed.
    #[test]
    fn catalog_rejects_unknown_selection() {
        assert!(package("libfoo")
            .unwrap_err()
            .to_string()
            .contains("known packages: pcre2, zlib, openssl, curl"));
        assert!(version("pcre2", Some("10.46")).is_err());
    }

    /// Verifies the official OpenSSL source identity, TLS-only static archive contract, and the
    /// exact header set curl's build compiles against.
    #[test]
    fn openssl_catalog_snapshot_is_exact() {
        let version = version("openssl", None).expect("catalogue entry");
        assert_eq!(version.version, "3.5.7");
        assert_eq!(version.source.exact_size, 53_153_930);
        assert_eq!(
            version.source.sha256,
            "a8c0d28a529ca480f9f36cf5792e2cd21984552a3c8e4aa11a24aa31aeac98e8"
        );
        assert_eq!(version.ordered_link_outputs, OPENSSL_ARCHIVES);
        assert_eq!(version.ordered_link_outputs, &["lib/libssl.a", "lib/libcrypto.a"]);
        assert_eq!(version.retained_headers.len(), 142);
        assert!(version.retained_headers.contains(&"include/openssl/ssl.h"));
        assert!(version.retained_headers.contains(&"include/openssl/crypto.h"));
        assert!(version.dependencies.is_empty());
        assert_eq!(version.supported_targets, TARGETS);
    }

    /// Verifies the official curl source identity, static archive contract, and the transitive
    /// `openssl`/`zlib` dependency declaration that `elephc native add curl` must materialize.
    #[test]
    fn curl_catalog_snapshot_is_exact() {
        let version = version("curl", None).expect("catalogue entry");
        assert_eq!(version.version, "8.21.0");
        assert_eq!(version.source.exact_size, 4_298_225);
        assert_eq!(
            version.source.sha256,
            "d9b327997999045a24cda50f3983e69e51c516bd8be6ef9842fc7f99135e33bb"
        );
        assert_eq!(version.ordered_link_outputs, CURL_ARCHIVES);
        assert_eq!(version.ordered_link_outputs, &["lib/libcurl.a"]);
        assert_eq!(version.retained_headers, CURL_HEADERS);
        assert_eq!(version.dependencies, &["openssl", "zlib"]);
        assert_eq!(version.supported_targets, TARGETS);
    }
}
