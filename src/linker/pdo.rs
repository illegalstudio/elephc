//! Purpose:
//! Centralizes optional PDO bridge profile selection for archive builds and final links.
//!
//! Called from:
//! - `crate::linker::bridges` when materializing `libelephc_pdo.a`.
//! - `crate::link_planning` when adding native driver-manager dependencies.
//!
//! Key details:
//! - Cargo features and their CI environment equivalents select the same bridge profile.
//! - Only libpq, FreeTDS, and ODBC-family profiles add direct final-link libraries.

/// Returns whether an optional profile is selected by a Cargo feature or CI environment flag.
fn selected(feature_enabled: bool, environment: &str) -> bool {
    feature_enabled || std::env::var_os(environment).is_some()
}

/// Returns Cargo feature names required when rebuilding the PDO bridge in this process.
pub(super) fn cargo_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    for (enabled, feature) in [
        (
            selected(cfg!(feature = "pdo-libpq-gss"), "ELEPHC_PDO_LIBPQ"),
            "libpq-gss",
        ),
        (
            selected(cfg!(feature = "pdo-dblib"), "ELEPHC_PDO_DBLIB"),
            "dblib",
        ),
        (
            selected(cfg!(feature = "pdo-firebird"), "ELEPHC_PDO_FIREBIRD"),
            "firebird",
        ),
        (
            selected(cfg!(feature = "pdo-odbc"), "ELEPHC_PDO_ODBC"),
            "odbc",
        ),
        (
            selected(cfg!(feature = "pdo-informix"), "ELEPHC_PDO_INFORMIX"),
            "informix",
        ),
        (
            selected(cfg!(feature = "pdo-ibm"), "ELEPHC_PDO_IBM"),
            "ibm",
        ),
        (
            selected(cfg!(feature = "pdo-sqlsrv"), "ELEPHC_PDO_SQLSRV"),
            "sqlsrv",
        ),
        (
            selected(cfg!(feature = "pdo-oci"), "ELEPHC_PDO_OCI"),
            "oci",
        ),
        (
            selected(cfg!(feature = "pdo-cubrid"), "ELEPHC_PDO_CUBRID"),
            "cubrid",
        ),
    ] {
        if enabled {
            features.push(feature);
        }
    }
    features
}

/// Returns whether the default PDO archive must be replaced by an optional profile build.
pub(super) fn profile_selected() -> bool {
    !cargo_features().is_empty()
}

/// Returns native libraries required by the selected PDO archive profile.
pub(super) fn system_libraries() -> Vec<&'static str> {
    let mut libraries = Vec::new();
    if selected(cfg!(feature = "pdo-libpq-gss"), "ELEPHC_PDO_LIBPQ") {
        libraries.push("pq");
    }
    if selected(cfg!(feature = "pdo-dblib"), "ELEPHC_PDO_DBLIB") {
        libraries.push("sybdb");
    }
    if selected(cfg!(feature = "pdo-odbc"), "ELEPHC_PDO_ODBC")
        || selected(cfg!(feature = "pdo-informix"), "ELEPHC_PDO_INFORMIX")
        || selected(cfg!(feature = "pdo-ibm"), "ELEPHC_PDO_IBM")
        || selected(cfg!(feature = "pdo-sqlsrv"), "ELEPHC_PDO_SQLSRV")
    {
        libraries.push("odbc");
    }
    libraries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the selected feature list never repeats a Cargo feature.
    #[test]
    fn selected_cargo_features_are_unique() {
        let features = cargo_features();
        let unique = features.iter().copied().collect::<crate::fast_hash::FastSet<_>>();
        assert_eq!(features.len(), unique.len());
    }
}
