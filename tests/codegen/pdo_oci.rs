//! Purpose:
//! End-to-end surface and live-server tests for the optional PDO_OCI backend.
//!
//! Called from:
//! - `cargo test --features pdo-oci --test codegen_tests`.
//!
//! Key details:
//! - Surface tests do not load Oracle Instant Client or contact a database.
//! - The ignored live test reads `ELEPHC_OCI_DSN` and exercises the real OCI client.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Exposes PDO_OCI's registry entry and unchanged legacy constants on PHP 8.4+.
#[test]
fn test_pdo_oci_surface_php84() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo PDO::OCI_ATTR_ACTION . ":" . PDO::OCI_ATTR_CLIENT_INFO . ":";
echo PDO::OCI_ATTR_CLIENT_IDENTIFIER . ":" . PDO::OCI_ATTR_MODULE . ":" . PDO::OCI_ATTR_CALL_TIMEOUT . "|";
echo class_exists("Pdo\\Oci") ? "class" : "missing";
"#,
        PhpVersion::Php84,
    );
    assert_eq!(out, "oci,mysql,pgsql,sqlite|1000:1001:1002:1003:1004|missing");
}

/// Keeps the bundled PHP 8.3 PDO_OCI constant surface byte-for-byte compatible.
#[test]
fn test_pdo_oci_surface_php83() {
    let out = compile_and_run_with_php_version(
        r#"<?php
echo PDO::OCI_ATTR_ACTION . ":" . PDO::OCI_ATTR_CALL_TIMEOUT . "|";
echo class_exists("Pdo\\Oci") ? "class" : "missing";
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "1000:1004|missing");
}

/// Exercises Oracle binds, metadata, attributes, LOB reads, and transactions.
#[test]
#[ignore]
fn test_pdo_oci_live_round_trip() {
    let dsn = std::env::var("ELEPHC_OCI_DSN")
        .expect("ELEPHC_OCI_DSN is required for the ignored PDO_OCI live test");
    let source = format!(
        r#"<?php
$db = new PDO({dsn:?}, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_PREFETCH => 7]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo (($db->getAttribute(PDO::ATTR_SERVER_VERSION) !== "" && $db->getAttribute(PDO::ATTR_CLIENT_VERSION) !== "") ? "versions" : "missing") . "|";
echo $db->getAttribute(PDO::ATTR_PREFETCH) . "|";
$db->setAttribute(PDO::OCI_ATTR_MODULE, "elephc-pdo");
$db->setAttribute(PDO::OCI_ATTR_ACTION, "live-test");
$db->exec("BEGIN EXECUTE IMMEDIATE 'DROP TABLE ELEPHC_PDO_OCI_TEST'; EXCEPTION WHEN OTHERS THEN NULL; END;");
$db->exec("CREATE TABLE ELEPHC_PDO_OCI_TEST (ID NUMBER NOT NULL, NAME VARCHAR2(80), DATA BLOB)");
$insert = $db->prepare("INSERT INTO ELEPHC_PDO_OCI_TEST (ID, NAME, DATA) VALUES (:id, :name, :data)");
$insert->bindValue(1, 7, PDO::PARAM_INT);
$insert->bindValue(2, "Éléphant", PDO::PARAM_STR);
$insert->bindValue(3, "A\0B", PDO::PARAM_LOB);
$insert->execute();
echo $insert->rowCount() . "|";
$io = $db->prepare("BEGIN :p := :p + 100; END;");
$p = -1;
$io->bindParam(":p", $p, PDO::PARAM_INT | PDO::PARAM_INPUT_OUTPUT, 10);
$io->execute();
echo gettype($p) . ":" . $p . "|";
$lobStmt = $db->prepare("BEGIN SELECT DATA INTO :data FROM ELEPHC_PDO_OCI_TEST WHERE ID = 7; END;");
$lob = null;
$lobStmt->bindParam(":data", $lob, PDO::PARAM_LOB);
$lobStmt->execute();
echo (is_resource($lob) ? stream_get_contents($lob) : "not-stream") . "|";
$select = $db->prepare("SELECT ID, NAME, DATA FROM ELEPHC_PDO_OCI_TEST ORDER BY ID", [PDO::ATTR_PREFETCH => 3, PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
$select->execute();
$row = $select->fetch(PDO::FETCH_ASSOC, PDO::FETCH_ORI_FIRST);
echo gettype($row["ID"]) . ":" . $row["ID"] . ":" . $row["NAME"] . ":" . stream_get_contents($row["DATA"]) . "|";
$meta = $select->getColumnMeta(0);
echo $meta["native_type"] . ":" . $meta["pdo_type"] . ":" . implode(",", $meta["flags"]) . "|";
$db->beginTransaction();
$db->exec("INSERT INTO ELEPHC_PDO_OCI_TEST (ID, NAME, DATA) VALUES (8, 'rollback', empty_blob())");
$db->rollBack();
echo $db->query("SELECT COUNT(*) FROM ELEPHC_PDO_OCI_TEST")->fetchColumn() . "|";
try {{ $db->query("SELECT * FROM ELEPHC_PDO_OCI_MISSING"); }} catch (PDOException $error) {{ echo $error->errorInfo[0] . ":" . (($error->errorInfo[1] !== 0) ? "native" : "zero"); }}
$db->exec("DROP TABLE ELEPHC_PDO_OCI_TEST");
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(
        out,
        "oci|versions|7|1|string:99|A\0B|string:7:Éléphant:A\0B|NUMBER:2:not_null|1|HY000:native"
    );
}
