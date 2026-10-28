//! Purpose:
//! End-to-end codegen test binary for extensions and I/O: streams, curl, PDO, mysqli, image, XML, fileinfo, JSON and system.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext` through Rust's test harness.
//!
//! Key details:
//! - The codegen suite is split into six `codegen_tests_*` binaries by domain so a library edit
//!   recompiles them in parallel; test names keep the `codegen::<module>::` path, and CI selects
//!   the family with `binary(/^codegen_tests_/)`.
//! - Each binary mounts the shared runner helpers; the ones a domain never calls are expected.

#[allow(dead_code)]
#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

#[allow(dead_code, unused_imports, unused_macros)]
#[path = "codegen/support/mod.rs"]
mod support;

#[allow(dead_code)]
#[path = "support/locals_retype_fixtures.rs"]
mod locals_retype_fixtures;

mod codegen {
    mod include_paths;
    mod curl;
    pub(crate) mod io;
    mod mysqli;
    mod mysqli_mysql;
    mod pdo;
    #[cfg(feature = "pdo-dblib")]
    mod pdo_dblib;
    #[cfg(feature = "pdo-firebird")]
    mod pdo_firebird;
    #[cfg(feature = "pdo-odbc")]
    mod pdo_odbc;
    #[cfg(feature = "pdo-informix")]
    mod pdo_informix;
    #[cfg(feature = "pdo-ibm")]
    mod pdo_ibm;
    #[cfg(feature = "pdo-sqlsrv")]
    mod pdo_sqlsrv;
    #[cfg(feature = "pdo-oci")]
    mod pdo_oci;
    #[cfg(feature = "pdo-cubrid")]
    mod pdo_cubrid;
    mod pdo_mysql;
    mod pdo_pgsql;
    mod image;
    mod pcntl;
    mod xml;
    mod tokenizer;
    mod calendar;
    mod system;
    mod json;
    mod serialize;
    mod random;
    mod dom_html;
    mod zval;
    mod fileinfo;
    mod fileinfo_autoload;
    mod fileinfo_reflection;
    mod fileinfo_validation;
    mod fileinfo_stream_diagnostics;
    mod fileinfo_wrapper_stat;
    mod fileinfo_stream_magic;
}
