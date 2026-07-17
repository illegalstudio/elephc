//! Purpose:
//! End-to-end version-surface and live CCI tests for the optional PDO_CUBRID backend.
//!
//! Called from:
//! - `cargo test --features pdo-cubrid --test codegen_tests`.
//!
//! Key details:
//! - Surface tests never require CCI or a running server.
//! - The ignored live test requires `CUBRID_CCI_LIBRARY` and `ELEPHC_CUBRID_DSN`.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Keeps the external extension's historical PDO-only surface on every PHP target.
#[test]
fn test_pdo_cubrid_surface_all_php_versions() {
    for version in PhpVersion::ALL {
        let out = compile_and_run_with_php_version(
            r#"<?php
echo implode(",", PDO::getAvailableDrivers()) . "|";
echo PDO::CUBRID_ATTR_ISOLATION_LEVEL . ":" . PDO::CUBRID_ATTR_LOCK_TIMEOUT . ":" . PDO::CUBRID_ATTR_MAX_STRING_LENGTH . "|";
echo PDO::TRAN_REP_CLASS_COMMIT_INSTANCE . ":" . PDO::TRAN_REP_CLASS_REP_INSTANCE . ":" . PDO::TRAN_SERIALIZABLE . "|";
echo PDO::CUBRID_SCH_TABLE . ":" . PDO::CUBRID_SCH_PRIMARY_KEY . ":" . PDO::CUBRID_SCH_ATTR_WITH_SYNONYM . "|";
echo class_exists("Pdo\\Cubrid") ? "class" : "missing";
function bind_cubrid_types(PDOStatement $statement, mixed &$status, mixed &$tags): void {
    $statement->bindParam(1, $status, PDO::PARAM_STR, 0, "ENUM");
    $statement->bindParam(2, $tags, PDO::PARAM_STR, 0, "STRING");
}
"#,
            version,
        );
        assert_eq!(
            out,
            "cubrid,mysql,pgsql,sqlite|1000:1001:1002|4:5:6|1:16:20|missing"
        );
    }
}

/// Exercises CCI prepares, binds, scrolling, metadata, schema rows, and rollback.
#[test]
#[ignore]
fn test_pdo_cubrid_live_round_trip() {
    let dsn = std::env::var("ELEPHC_CUBRID_DSN")
        .expect("ELEPHC_CUBRID_DSN is required for the ignored PDO_CUBRID live test");
    let source = format!(
        r#"<?php
$stage = "connect";
try {{
$available = implode(",", pdo_drivers());
$db = new PDO({dsn:?}, "dba", "", [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME) . "|";
echo (($db->getAttribute(PDO::ATTR_SERVER_VERSION) !== "" && $db->getAttribute(PDO::ATTR_CLIENT_VERSION) !== "") ? "versions" : "missing") . "|";
$db->setAttribute(PDO::CUBRID_ATTR_LOCK_TIMEOUT, 4);
$db->setAttribute(PDO::CUBRID_ATTR_ISOLATION_LEVEL, PDO::TRAN_REP_CLASS_REP_INSTANCE);
$db->setAttribute(PDO::ATTR_TIMEOUT, 3);
echo (($db->getAttribute(PDO::CUBRID_ATTR_LOCK_TIMEOUT) == 4
    && $db->getAttribute(PDO::CUBRID_ATTR_ISOLATION_LEVEL) == PDO::TRAN_REP_CLASS_REP_INSTANCE
    && $db->getAttribute(PDO::ATTR_TIMEOUT) == 3
    && is_int($db->getAttribute(PDO::CUBRID_ATTR_MAX_STRING_LENGTH))) ? "attrs" : "bad-attrs") . "|";
$stage = "ddl";
$db->exec("DROP TABLE IF EXISTS elephc_pdo_cubrid");
$db->exec("CREATE TABLE elephc_pdo_cubrid (id INTEGER AUTO_INCREMENT PRIMARY KEY, label VARCHAR(80), amount DOUBLE, status ENUM('ready', 'done'), tags SET(VARCHAR(20)), payload BLOB, note CLOB)");
$stage = "bind";
$insert = $db->prepare("INSERT INTO elephc_pdo_cubrid(label, amount, status, tags, payload, note) VALUES (:label, :amount, :status, :tags, :payload, :note)");
$insert->bindValue(":label", "éléphant", PDO::PARAM_STR);
$insert->bindValue(":amount", 12.5);
$status = "ready";
$tags = ["one", "two"];
$payload = fopen("php://memory", "r+");
fwrite($payload, "blob-data");
rewind($payload);
$note = fopen("php://memory", "r+");
fwrite($note, "clob-data");
rewind($note);
$insert->bindParam(":status", $status, PDO::PARAM_STR, 0, "ENUM");
$insert->bindParam(":tags", $tags, PDO::PARAM_STR, 0, "STRING");
$insert->bindParam(":payload", $payload, PDO::PARAM_LOB, 0, "BLOB");
$insert->bindParam(":note", $note, PDO::PARAM_LOB, 0, "CLOB");
echo "inputs:" . ftell($payload) . ":" . ftell($note) . "|";
$stage = "execute";
$insert->execute();
echo $insert->rowCount() . ":" . $db->lastInsertId() . "|";
fclose($payload);
fclose($note);
$stage = "select-prepare";
$select = $db->prepare("SELECT id, label, amount, status, payload, note FROM elephc_pdo_cubrid WHERE id = :id ORDER BY id", [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
$stage = "select-bind";
$select->bindValue(":id", 1, PDO::PARAM_INT);
$stage = "select-execute";
$select->execute();
$stage = "select-fetch";
$row = $select->fetch(PDO::FETCH_ASSOC, PDO::FETCH_ORI_FIRST);
echo $row["id"] . ":" . $row["label"] . ":" . $row["amount"] . ":" . $row["status"] . "|";
$fetchedPayload = $row["payload"];
$fetchedNote = $row["note"];
echo (is_resource($fetchedPayload) ? stream_get_contents($fetchedPayload) : "not-resource") . ":";
echo (is_resource($fetchedNote) ? stream_get_contents($fetchedNote) : "not-resource") . "|";
fclose($fetchedPayload);
fclose($fetchedNote);
$select->execute();
echo $select->fetchColumn() . "|";
$absolute = $select->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_ABS, 1);
echo $absolute[0] . "|";
$stage = "metadata";
$meta = $select->getColumnMeta(0);
echo $meta["name"] . ":" . $meta["primary_key"] . ":" . $meta["auto_increment"] . "|";
$schema = $db->cubrid_schema(PDO::CUBRID_SCH_TABLE, "elephc_pdo_cubrid");
echo ($schema !== false && isset($schema[0]) ? "schema" : "missing") . "|";
$stage = "transaction";
$db->beginTransaction();
$db->exec("INSERT INTO elephc_pdo_cubrid(label, amount) VALUES ('rollback', 1.0)");
$db->rollBack();
$countStatement = $db->query("SELECT COUNT(*) FROM elephc_pdo_cubrid");
echo $countStatement->fetchColumn() . ":";
echo $db->getAttribute(PDO::ATTR_AUTOCOMMIT) . ":";
echo $db->quote("a'b");
$select->closeCursor();
$countStatement->closeCursor();
$select->__destruct();
$countStatement->__destruct();
$insert->__destruct();
$db->exec("DROP TABLE elephc_pdo_cubrid");
echo "|dropped";
$db->__destruct();
echo "|closed";
}} catch (Throwable $error) {{
    echo "error@" . $stage . "[" . $available . "]:" . $error->getMessage();
}}
"#
    );
    let out = compile_and_run(&source);
    assert_eq!(
        out,
        "cubrid|versions|attrs|inputs:0:0|1:1|1:éléphant:12.5000000000000000:ready|blob-data:clob-data|1|1|id::|schema|1:1:a''b|dropped|closed"
    );
}
