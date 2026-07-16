//! Purpose:
//! End-to-end surface and live-server tests for the optional PDO_ODBC backend.
//!
//! Called from:
//! - `cargo test --features pdo-odbc --test codegen_tests`.
//!
//! Key details:
//! - Surface tests require unixODBC at link time but no configured database driver.
//! - The ignored live test uses a direct DSN from `ELEPHC_ODBC_DSN`.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Exposes PDO_ODBC's manager type, registry entry, aliases, and PHP 8.4 class.
#[test]
fn test_pdo_odbc_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo PDO_ODBC_TYPE . "|";
echo (class_exists("Pdo\\Odbc") ? "class" : "missing") . "|";
echo Pdo\Odbc::ATTR_USE_CURSOR_LIBRARY . ":" . Pdo\Odbc::ATTR_ASSUME_UTF8 . "|";
echo Pdo\Odbc::SQL_USE_IF_NEEDED . ":" . Pdo\Odbc::SQL_USE_ODBC . ":" . Pdo\Odbc::SQL_USE_DRIVER . "|";
echo PDO::ODBC_ATTR_USE_CURSOR_LIBRARY . ":" . PDO::ODBC_SQL_USE_DRIVER;
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "odbc,mysql,pgsql,sqlite|unixODBC|class|1000:1001|0:1:2|1000:2");
}

/// Keeps PDO_ODBC's global type and legacy aliases before namespaced classes exist.
#[test]
fn test_pdo_odbc_surface_php83() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo PDO_ODBC_TYPE . "|";
echo (class_exists("Pdo\\Odbc") ? "class" : "missing") . "|";
echo PDO::ODBC_ATTR_ASSUME_UTF8 . ":" . PDO::ODBC_SQL_USE_ODBC;
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "unixODBC|missing|1001:1");
}

/// Exercises native binds, text typing, attributes, rowsets, transactions, and diagnostics.
#[test]
#[ignore]
fn test_pdo_odbc_live_round_trip() {
    let dsn = std::env::var("ELEPHC_ODBC_DSN")
        .expect("ELEPHC_ODBC_DSN is required for the ignored PDO_ODBC live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, Pdo\Odbc::ATTR_ASSUME_UTF8 => true]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo PDO_ODBC_TYPE . ":" . (($db->getAttribute(PDO::ATTR_SERVER_VERSION) !== "" && $db->getAttribute(PDO::ATTR_SERVER_INFO) !== "") ? "server" : "missing") . ":" . $db->getAttribute(PDO::ATTR_CLIENT_VERSION) . "|";
echo ($db->getAttribute(Pdo\Odbc::ATTR_ASSUME_UTF8) ? "utf8" : "raw") . "|";
try {{ $db->quote("O'Brien"); }} catch (PDOException $e) {{ echo $e->errorInfo[0]; }}
echo "|";
$db->exec("CREATE TEMP TABLE elephc_odbc_test (id INTEGER, name VARCHAR(40))");
$stmt = $db->prepare("INSERT INTO elephc_odbc_test (id, name) VALUES (:id, :name)");
$stmt->execute(["id" => 7, "name" => "Éléphant"]);
echo $stmt->rowCount() . "|";
try {{
    $db->prepare("SELECT id FROM elephc_odbc_test", [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
}} catch (PDOException $e) {{
    echo $e->errorInfo[0] . "|";
}}
$stmt = $db->prepare("SELECT id, name FROM elephc_odbc_test ORDER BY id");
$stmt->execute();
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo gettype($row["id"]) . ":" . $row["id"] . ":" . $row["name"] . "|";
$meta = $stmt->getColumnMeta(0);
echo count($meta) . ":" . $meta["pdo_type"] . "|";
$db->beginTransaction();
$db->exec("INSERT INTO elephc_odbc_test (id, name) VALUES (8, 'rollback')");
$db->rollBack();
echo $db->query("SELECT COUNT(*) FROM elephc_odbc_test")->fetchColumn() . "|";
$sets = $db->query("SELECT 1 AS value; SELECT 2 AS value");
echo $sets->fetchColumn() . ":" . ($sets->nextRowset() ? $sets->fetchColumn() : "missing") . "|";
try {{ $db->query("SELECT * FROM elephc_missing_odbc_table"); }} catch (PDOException $e) {{ echo $e->errorInfo[0] . ":" . (($e->errorInfo[1] !== 0) ? "native" : "zero"); }}
}} catch (Throwable $fatal) {{
echo "FAIL:" . $fatal->getMessage();
}}
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(out, "odbc|unixODBC:server:ODBC-unixODBC|utf8|IM001|1|HYC00|string:7:Éléphant|1:2|1|1:2|42P01:native");
}
