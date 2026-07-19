//! Purpose:
//! End-to-end surface and live-server tests for the optional FreeTDS PDO_DBLIB backend.
//!
//! Called from:
//! - `cargo test --features pdo-dblib --test codegen_tests`.
//!
//! Key details:
//! - Surface tests require FreeTDS at link time but no database server.
//! - Ignored live tests read `ELEPHC_DBLIB_DSN` and exercise SQL Server/Sybase through libsybdb.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Exposes DBLIB in the compiled-driver registry and PHP 8.4 namespaced class.
#[test]
fn test_pdo_dblib_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo (class_exists("Pdo\\Dblib") ? "class" : "missing") . "|";
echo Pdo\Dblib::ATTR_CONNECTION_TIMEOUT . ":" . Pdo\Dblib::ATTR_DATETIME_CONVERT . "|";
echo PDO::DBLIB_ATTR_CONNECTION_TIMEOUT . ":" . PDO::DBLIB_ATTR_DATETIME_CONVERT;
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "dblib,mysql,pgsql,sqlite|class|1000:1006|1000:1006");
}

/// Keeps the legacy PDO constants while withholding `Pdo\Dblib` before PHP 8.4.
#[test]
fn test_pdo_dblib_surface_php83() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo (class_exists("Pdo\\Dblib") ? "class" : "missing") . "|";
echo PDO::DBLIB_ATTR_QUERY_TIMEOUT . ":" . PDO::DBLIB_ATTR_TDS_VERSION;
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "missing|1001:1004");
}

/// Connects through FreeTDS and verifies scalar types, named/positional binds,
/// multi-rowsets, driver attributes, and native error propagation.
#[test]
#[ignore]
fn test_pdo_dblib_live_round_trip() {
    let dsn = std::env::var("ELEPHC_DBLIB_DSN")
        .expect("ELEPHC_DBLIB_DSN is required for the ignored PDO_DBLIB live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo ($db->getAttribute(PDO::ATTR_EMULATE_PREPARES) ? "emulated" : "native") . ":";
echo ($db->setAttribute(PDO::ATTR_EMULATE_PREPARES, false) ? "mutable" : "fixed") . ":";
echo ($db->setAttribute(Pdo\Dblib::ATTR_QUERY_TIMEOUT, 5) ? "timeout" : "no-timeout") . "|";
$db->setAttribute(Pdo\Dblib::ATTR_STRINGIFY_UNIQUEIDENTIFIER, true);
$db->setAttribute(Pdo\Dblib::ATTR_SKIP_EMPTY_ROWSETS, true);
echo ($db->getAttribute(Pdo\Dblib::ATTR_STRINGIFY_UNIQUEIDENTIFIER) ? "uuid" : "binary") . ":";
echo ($db->getAttribute(Pdo\Dblib::ATTR_SKIP_EMPTY_ROWSETS) ? "skip" : "keep") . ":";
echo (($db->getAttribute(Pdo\Dblib::ATTR_VERSION) !== "" && $db->getAttribute(Pdo\Dblib::ATTR_TDS_VERSION) !== "") ? "versions" : "missing") . "|";
echo ($db->setAttribute(Pdo\Dblib::ATTR_CONNECTION_TIMEOUT, 1) ? "mutable-connect" : "fixed-connect") . ":";
try {{
    $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
}} catch (PDOException $e) {{
    echo $e->errorInfo[0];
}}
echo "|";
echo $db->quote("O'Brien") . ":";
$db->setAttribute(PDO::ATTR_DEFAULT_STR_PARAM, PDO::PARAM_STR_NATL);
echo $db->quote("O'Brien") . "|";
$stmt = $db->prepare("SELECT :n AS n, :name AS name");
$stmt->execute(["n" => 7, "name" => "Ada"]);
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo gettype($row["n"]) . ":" . $row["n"] . ":" . $row["name"] . "|";
$meta = $stmt->getColumnMeta(0);
echo $meta["native_type"] . ":" . $meta["native_type_id"] . ":" . $meta["pdo_type"] . ":" . $meta["max_length"] . "|";
$types = $db->query("SELECT CAST('00112233-4455-6677-8899-AABBCCDDEEFF' AS uniqueidentifier) AS uuid, CAST('2024-02-03T04:05:06' AS datetime2) AS moment")->fetch(PDO::FETCH_NUM);
echo $types[0] . ":" . $types[1] . "|";
$sets = $db->query("SELECT 1 AS value; SELECT 2 AS value");
echo $sets->fetchColumn() . ":";
echo ($sets->nextRowset() ? $sets->fetchColumn() : "missing") . "|";
try {{
    $db->query("SELECT * FROM elephc_missing_dblib_table");
}} catch (PDOException $e) {{
    echo $e->errorInfo[0] . ":" . (($e->errorInfo[1] !== 0) ? "native" : "zero") . ":" . (isset($e->errorInfo[4]) ? "extended" : "short");
}}
echo "|";
try {{
    $db->prepare("SELECT ? AS positional, :named AS named");
}} catch (PDOException $e) {{
    echo $e->errorInfo[0];
}}
}} catch (Throwable $fatal) {{
    echo "FAIL:" . $fatal->getMessage();
}}
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "dblib|emulated:fixed:timeout|uuid:skip:versions|fixed-connect:IM001|'O''Brien':N'O''Brien'|integer:7:Ada|int:56:1:4|00112233-4455-6677-8899-AABBCCDDEEFF:2024-02-03 04:05:06|1:2|HY000:native:extended|HY093");
}
