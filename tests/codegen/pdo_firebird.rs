//! Purpose:
//! End-to-end surface and live-server tests for the optional PDO_FIREBIRD backend.
//!
//! Called from:
//! - `cargo test --features pdo-firebird --test codegen_tests`.
//!
//! Key details:
//! - Surface tests need no server and verify the PHP-version-dependent aliases/classes.
//! - The ignored live test reads `ELEPHC_FIREBIRD_DSN` and exercises Firebird over its wire protocol.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Exposes the namespaced Firebird class, constants, API level, and legacy aliases on PHP 8.4.
#[test]
fn test_pdo_firebird_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo (in_array("firebird", PDO::getAvailableDrivers(), true) ? "driver" : "missing") . "|";
echo (class_exists("Pdo\\Firebird") ? "class" : "missing") . "|";
echo Pdo\Firebird::ATTR_DATE_FORMAT . ":" . Pdo\Firebird::WRITABLE_TRANSACTION . "|";
echo Pdo\Firebird::getApiVersion() . "|";
echo PDO::FB_ATTR_DATE_FORMAT . ":" . PDO::FB_ATTR_TIMESTAMP_FORMAT;
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "driver|class|1000:1007|40|1000:1002");
}

/// Keeps only the historical PDO aliases before driver-specific classes exist.
#[test]
fn test_pdo_firebird_surface_php83() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo (class_exists("Pdo\\Firebird") ? "class" : "missing") . "|";
echo PDO::FB_ATTR_TIME_FORMAT . ":" . PDO::FB_ATTR_TIMESTAMP_FORMAT;
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "missing|1001:1002");
}

/// Verifies binding, scalar/date conversion, metadata, transactions, attributes,
/// quoting, and native diagnostics against a real Firebird server.
#[test]
#[ignore]
fn test_pdo_firebird_live_round_trip() {
    let dsn = std::env::var("ELEPHC_FIREBIRD_DSN")
        .expect("ELEPHC_FIREBIRD_DSN is required for the ignored PDO_FIREBIRD live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo (($db->getAttribute(PDO::ATTR_SERVER_VERSION) !== "" && $db->getAttribute(PDO::ATTR_CLIENT_VERSION) !== "") ? "versions" : "missing") . "|";
echo ($db->getAttribute(PDO::ATTR_CONNECTION_STATUS) ? "connected" : "closed") . "|";
$db->setAttribute(Pdo\Firebird::ATTR_DATE_FORMAT, "%d/%m/%Y");
$db->setAttribute(Pdo\Firebird::TRANSACTION_ISOLATION_LEVEL, Pdo\Firebird::READ_COMMITTED);
$db->setAttribute(Pdo\Firebird::WRITABLE_TRANSACTION, true);
echo $db->getAttribute(Pdo\Firebird::ATTR_DATE_FORMAT) . ":" . $db->getAttribute(Pdo\Firebird::TRANSACTION_ISOLATION_LEVEL) . ":" . ($db->getAttribute(Pdo\Firebird::WRITABLE_TRANSACTION) ? "rw" : "ro") . "|";
echo $db->quote("O'Brien") . "|";
$stmt = $db->prepare("SELECT CAST(? AS INTEGER) AS n, CAST(:name AS VARCHAR(20)) AS name FROM RDB\$DATABASE");
try {{
    $stmt->execute([7, "name" => "Ada"]);
}} catch (PDOException $mixed) {{
    echo $mixed->errorInfo[0] . "|";
}}
$stmt = $db->prepare("SELECT CAST(:n AS INTEGER) AS n, CAST(:name AS VARCHAR(20)) AS name FROM RDB\$DATABASE");
$stmt->execute(["n" => 7, "name" => "Ada"]);
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo gettype($row["N"]) . ":" . $row["N"] . ":" . trim($row["NAME"]) . "|";
$meta = $stmt->getColumnMeta(0);
echo count($meta) . ":" . $meta["pdo_type"] . "|";
$date = $db->query("SELECT CAST('2024-02-03' AS DATE) AS d FROM RDB\$DATABASE")->fetchColumn();
echo $date . "|";
$db->exec("RECREATE GLOBAL TEMPORARY TABLE ELEPHC_PDO_FB_TEST (VALUE INTEGER) ON COMMIT PRESERVE ROWS");
$db->beginTransaction();
$db->exec("INSERT INTO ELEPHC_PDO_FB_TEST (VALUE) VALUES (42)");
$db->rollBack();
echo $db->query("SELECT COUNT(*) FROM ELEPHC_PDO_FB_TEST")->fetchColumn() . "|";
try {{
    $db->query("SELECT * FROM ELEPHC_MISSING_FIREBIRD_TABLE");
}} catch (PDOException $e) {{
    echo $e->errorInfo[0] . ":" . (($e->errorInfo[1] !== 0) ? "native" : "zero");
}}
}} catch (Throwable $fatal) {{
echo "FAIL:" . $fatal->getMessage();
}}
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "firebird|versions|connected|%d/%m/%Y:1004:rw|'O''Brien'|HY093|integer:7:Ada|1:1|03/02/2024|0|42S02:native");
}
