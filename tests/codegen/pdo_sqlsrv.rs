//! Purpose:
//! End-to-end version-surface and live SQL Server tests for optional PDO_SQLSRV.
//!
//! Called from:
//! - `cargo test --features pdo-sqlsrv --test codegen_tests`.
//!
//! Key details:
//! - PDO_SQLSRV 5.13.1 supports PHP 8.3-8.5 and declares constants on `PDO` only.
//! - The ignored live test requires Microsoft ODBC Driver 18/17 and `ELEPHC_SQLSRV_DSN`.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Verifies PDO_SQLSRV 5.13.1's legacy-only PHP 8.3 constant surface.
#[test]
fn test_pdo_sqlsrv_surface_php83() {
    assert_sqlsrv_surface(PhpVersion::Php83);
}

/// Verifies PHP 8.4 does not invent a `Pdo\Sqlsrv` class absent upstream.
#[test]
fn test_pdo_sqlsrv_surface_php84() {
    assert_sqlsrv_surface(PhpVersion::Php84);
}

/// Verifies PHP 8.5 keeps PDO_SQLSRV constants non-deprecated and on `PDO`.
#[test]
fn test_pdo_sqlsrv_surface_php85() {
    assert_sqlsrv_surface(PhpVersion::Php85);
}

/// Verifies PDO_SQLSRV 5.13.1 is hidden from its unsupported PHP 8.2 target.
#[test]
fn test_pdo_sqlsrv_unavailable_php82() {
    assert_sqlsrv_unavailable(PhpVersion::Php82);
}

/// Verifies the profile stays hidden until Microsoft publishes PHP 8.6 support.
#[test]
fn test_pdo_sqlsrv_unavailable_php86() {
    assert_sqlsrv_unavailable(PhpVersion::Php86);
}

/// Compiles the shared PDO_SQLSRV constants/class-presence probe for one PHP version.
fn assert_sqlsrv_surface(version: PhpVersion) {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo PDO::SQLSRV_ATTR_ENCODING . ":" . PDO::SQLSRV_ATTR_DATA_CLASSIFICATION . "|";
echo PDO::SQLSRV_ENCODING_DEFAULT . ":" . PDO::SQLSRV_ENCODING_BINARY . ":" . PDO::SQLSRV_ENCODING_SYSTEM . ":" . PDO::SQLSRV_ENCODING_UTF8 . "|";
echo PDO::SQLSRV_CURSOR_KEYSET . ":" . PDO::SQLSRV_CURSOR_DYNAMIC . ":" . PDO::SQLSRV_CURSOR_STATIC . ":" . PDO::SQLSRV_CURSOR_BUFFERED . "|";
echo PDO::SQLSRV_PARAM_OUT_DEFAULT_SIZE . ":" . PDO::SQLSRV_TXN_SNAPSHOT . "|";
echo class_exists("Pdo\\Sqlsrv") ? "class" : "missing";
"#,
        version,
    );
    assert_eq!(
        out,
        "sqlsrv,mysql,pgsql,sqlite|1000:1009|1:2:3:65001|1:2:3:42|-1:SNAPSHOT|missing"
    );
}

/// Compiles the unsupported-version capability probe without referencing absent constants.
fn assert_sqlsrv_unavailable(version: PhpVersion) {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo class_exists("Pdo\\Sqlsrv") ? "class" : "missing";
"#,
        version,
    );
    assert_eq!(out, "mysql,pgsql,sqlite|missing");
}

/// Exercises SQLSRV connection info, attributes, Unicode quoting, types, metadata, and identities.
#[test]
#[ignore]
fn test_pdo_sqlsrv_live_round_trip() {
    let dsn = std::env::var("ELEPHC_SQLSRV_DSN")
        .expect("ELEPHC_SQLSRV_DSN is required for the ignored PDO_SQLSRV live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
$client = $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
$server = $db->getAttribute(PDO::ATTR_SERVER_INFO);
echo $client["ExtensionVer"] . ":" . (isset($client["DriverVer"]) ? "client" : "missing") . ":" . (isset($server["SQLServerVersion"]) ? "server" : "missing") . "|";
echo $db->quote("é'", PDO::PARAM_STR) . "|";
$db->exec("CREATE TABLE #elephc_pdo_sqlsrv (id INT IDENTITY(1,1), amount DECIMAL(10,2), happened DATETIME2, label NVARCHAR(40))");
$insert = $db->prepare("INSERT INTO #elephc_pdo_sqlsrv(amount, happened, label) VALUES (?, ?, ?)");
$insert->execute([12.5, "2026-07-17 12:34:56", "éléphant"]);
echo $db->lastInsertId() . "|";
$stmt = $db->prepare(
    "SELECT id, amount, happened, label FROM #elephc_pdo_sqlsrv",
    [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL, PDO::SQLSRV_ATTR_CURSOR_SCROLL_TYPE => PDO::SQLSRV_CURSOR_BUFFERED, PDO::SQLSRV_ATTR_FETCHES_NUMERIC_TYPE => true, PDO::SQLSRV_ATTR_FETCHES_DATETIME_TYPE => true]
);
echo "prepared|";
$stmt->execute();
echo "executed|";
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo "fetched|";
echo gettype($row["id"]) . ":";
echo $row["id"] . ":";
echo get_class($row["happened"]) . ":";
echo $row["label"] . "|";
$meta = $stmt->getColumnMeta(0);
echo $meta["native_type"] . ":" . $meta["sqlsrv:decl_type"] . ":" . $meta["pdo_type"] . ":" . $meta["name"];
}} catch (Throwable $fatal) {{ echo "FAIL:" . $fatal->getMessage(); }}
"#
    );
    let out = compile_and_run(&source);
    assert!(
        out.starts_with("sqlsrv|5.13.1:client:server|N'"),
        "unexpected PDO_SQLSRV output: {out}"
    );
    assert!(
        out.contains("|1|prepared|executed|fetched|integer:1:DateTime:éléphant|string:"),
        "unexpected PDO_SQLSRV output: {out}"
    );
}
