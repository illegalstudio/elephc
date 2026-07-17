//! Purpose:
//! End-to-end version-surface and live-server tests for the optional PDO_IBM backend.
//!
//! Called from:
//! - `cargo test --features pdo-ibm --test codegen_tests`.
//!
//! Key details:
//! - Surface tests need unixODBC at link time but no IBM driver installation.
//! - The ignored live test requires an IBM CLI/ODBC driver plus `ELEPHC_IBM_DSN`.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Keeps legacy PDO_IBM constants while omitting the PHP 8.4 namespaced class on PHP 8.3.
#[test]
fn test_pdo_ibm_surface_php83() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo PDO::SQL_ATTR_INFO_USERID . ":" . PDO::SQL_ATTR_USE_TRUSTED_CONTEXT . "|";
echo class_exists("Pdo\\Ibm") ? "class" : "missing";
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "ibm,mysql,pgsql,sqlite|1281:2561|missing");
}

/// Exposes PDO_IBM 1.7.0's `Pdo\Ibm` class and constants on PHP 8.4.
#[test]
fn test_pdo_ibm_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo Pdo\Ibm::ATTR_INFO_USERID . ":" . Pdo\Ibm::ATTR_INFO_ACCTSTR . ":";
echo Pdo\Ibm::ATTR_INFO_APPLNAME . ":" . Pdo\Ibm::ATTR_INFO_WRKSTNNAME . "|";
echo Pdo\Ibm::ATTR_USE_TRUSTED_CONTEXT . ":" . Pdo\Ibm::ATTR_TRUSTED_CONTEXT_USERID . ":" . Pdo\Ibm::ATTR_TRUSTED_CONTEXT_PASSWORD . "|";
try { $unused = new Pdo\Ibm("sqlite::memory:"); } catch (PDOException $e) { echo str_contains($e->getMessage(), '"sqlite" driver') ? "guard" : "wrong"; }
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "1281:1282:1283:1284|2561:2562:2563|guard");
}

/// Keeps the PHP 8.5 alias values coherent with the namespaced PDO_IBM constants.
#[test]
fn test_pdo_ibm_surface_php85() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo (PDO::SQL_ATTR_INFO_USERID === Pdo\Ibm::ATTR_INFO_USERID ? "same" : "different") . "|";
echo (PDO::SQL_ATTR_TRUSTED_CONTEXT_PASSWORD === Pdo\Ibm::ATTR_TRUSTED_CONTEXT_PASSWORD ? "same" : "different");
"#,
        PhpVersion::Php85,
    );
    assert_eq!(out, "same|same");
}

/// Exercises Db2 scalar fetching, cursor names, metadata, and connection information.
#[test]
#[ignore]
fn test_pdo_ibm_live_round_trip() {
    let dsn = std::env::var("ELEPHC_IBM_DSN")
        .expect("ELEPHC_IBM_DSN is required for the ignored PDO_IBM live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo $db->getAttribute(PDO::ATTR_CLIENT_VERSION) . ":" . $db->getAttribute(PDO::ATTR_SERVER_INFO) . "|";
$stmt = $db->prepare("SELECT CAST(42 AS INTEGER) AS answer, CAST('elephc' AS VARCHAR(20)) AS label FROM SYSIBM.SYSDUMMY1", [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
$stmt->setAttribute(PDO::ATTR_CURSOR_NAME, "ELEPHC_IBM_CURSOR");
$stmt->execute();
$meta = $stmt->getColumnMeta(0);
echo $meta["name"] . ":" . $meta["native_type"] . ":" . $meta["len"] . ":" . $meta["precision"] . ":";
echo (array_key_exists("not_null", $meta["flags"]) && array_key_exists("unsigned", $meta["flags"]) && array_key_exists("auto_increment", $meta["flags"]) ? "flags" : "missing") . "|";
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo gettype($row["ANSWER"]) . ":" . $row["ANSWER"] . ":" . $row["LABEL"] . "|";
echo $stmt->getAttribute(PDO::ATTR_CURSOR_NAME);
}} catch (Throwable $fatal) {{ echo "FAIL:" . $fatal->getMessage(); }}
"#
    );
    let out = compile_and_run(&source);
    assert!(out.starts_with("ibm|1.7.0:"), "unexpected PDO_IBM output: {out}");
    assert!(out.contains(":flags|string:42:elephc|ELEPHC_IBM_CURSOR"), "unexpected PDO_IBM output: {out}");
}
