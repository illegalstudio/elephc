//! Purpose:
//! End-to-end surface and live-server tests for the optional PDO_INFORMIX backend.
//!
//! Called from:
//! - `cargo test --features pdo-informix --test codegen_tests`.
//!
//! Key details:
//! - Surface tests need unixODBC at link time but no Informix installation.
//! - The ignored live test requires IBM/HCL Client SDK plus `ELEPHC_INFORMIX_DSN`.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Exposes the current PECL driver without inventing a namespaced subclass.
#[test]
fn test_pdo_informix_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo (class_exists("Pdo\\Informix") ? "class" : "missing");
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "informix,mysql,pgsql,sqlite|missing");
}

/// Keeps PDO_INFORMIX available on the oldest maintained elephc PHP profile.
#[test]
fn test_pdo_informix_surface_php82() {
    let out = compile_and_run_with_php_version(
        r#"<?php echo in_array("informix", pdo_drivers(), true) ? "yes" : "no";"#,
        PhpVersion::Php82,
    );
    assert_eq!(out, "yes");
}

/// Exercises CLI binds, case folding, transactions, diagnostics, and metadata.
#[test]
#[ignore]
fn test_pdo_informix_live_round_trip() {
    let dsn = std::env::var("ELEPHC_INFORMIX_DSN")
        .expect("ELEPHC_INFORMIX_DSN is required for the ignored PDO_INFORMIX live test");
    let source = format!(
        r#"<?php
try {{
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo $db->getAttribute(PDO::ATTR_CLIENT_VERSION) . ":" . $db->getAttribute(PDO::ATTR_SERVER_INFO) . "|";
$db->exec("CREATE TEMP TABLE elephc_informix_test (id INTEGER, name VARCHAR(40)) WITH NO LOG");
$stmt = $db->prepare("INSERT INTO elephc_informix_test (id, name) VALUES (:id, :name)");
$stmt->execute(["id" => 7, "name" => "Éléphant"]);
echo $stmt->rowCount() . "|";
$stmt = $db->query("SELECT id, name FROM elephc_informix_test ORDER BY id");
$meta = $stmt->getColumnMeta(0);
echo $meta["scale"] . ":" . $meta["native_type"] . ":";
echo (array_key_exists("not_null", $meta["flags"]) && array_key_exists("unsigned", $meta["flags"])
    && array_key_exists("auto_increment", $meta["flags"]) ? "flags" : "missing") . ":";
echo $meta["pdo_type"] . ":";
echo (array_key_exists("name", $meta) && array_key_exists("len", $meta)
    && array_key_exists("precision", $meta) ? "core" : "missing") . "|";
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo implode(",", array_keys($row)) . ":" . gettype($row["ID"]) . ":" . $row["ID"] . ":" . $row["NAME"] . "|";
$db->beginTransaction();
$db->exec("INSERT INTO elephc_informix_test (id, name) VALUES (8, 'rollback')");
$db->rollBack();
echo $db->query("SELECT COUNT(*) FROM elephc_informix_test")->fetchColumn() . "|";
try {{ $db->query("SELECT * FROM elephc_missing_table"); }} catch (PDOException $e) {{
    echo $e->errorInfo[0] . ":" . (($e->errorInfo[1] !== 0) ? "native" : "zero");
}}
}} catch (Throwable $fatal) {{
echo "FAIL:" . $fatal->getMessage();
}}
"#
    );
    let out = compile_and_run(&source);
    assert!(out.starts_with("informix|1.3.7:"), "unexpected PDO_INFORMIX output: {out}");
    assert!(out.contains("|1|0:INTEGER:flags:2:core|ID,NAME:string:7:Éléphant|1|"), "unexpected PDO_INFORMIX output: {out}");
    assert!(out.ends_with(":native"), "unexpected PDO_INFORMIX output: {out}");
}
