//! Purpose:
//! Integration tests for the PDO (SQLite driver) standard-library surface.
//! Each fixture compiles a PHP program that drives an in-memory SQLite database
//! through `PDO`/`PDOStatement` and asserts the produced stdout.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The PDO prelude is injected by the compiler when a program references PDO,
//!   and the program links the `elephc-pdo` bridge staticlib (built as a
//!   workspace default-member, located in `target/<profile>/`). No external
//!   database is required for these SQLite fixtures: `sqlite::memory:` runs
//!   in-process, so they are not `#[ignore]`d. PostgreSQL fixtures (which need a
//!   live server) live in `tests/codegen/pdo_pgsql.rs` and are `#[ignore]`d.

use crate::support::*;

/// `new PDO("sqlite::memory:")` opens a database and `exec()` + a SELECT through
/// `fetch(PDO::FETCH_ASSOC)` round-trips a row keyed by column name.
#[test]
fn test_pdo_exec_and_assoc_fetch() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO users (name) VALUES ('Ada')");
$stmt = $db->query("SELECT id, name FROM users");
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "1:Ada");
}

/// A prepared statement with a positional `?` placeholder binds through
/// `execute([...])` and selects the matching row.
#[test]
fn test_pdo_prepared_positional_bind() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'one'), (2, 'two')");
$stmt = $db->prepare("SELECT name FROM t WHERE id = ?");
$stmt->execute([2]);
echo $stmt->fetch(PDO::FETCH_ASSOC)["name"];
"#,
    );
    assert_eq!(out, "two");
}

/// Named placeholders (`:name`) bind through `execute([":name" => ...])`.
#[test]
fn test_pdo_named_bind() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b TEXT)");
$ins = $db->prepare("INSERT INTO t (a, b) VALUES (:a, :b)");
$ins->execute([":a" => 7, ":b" => "seven"]);
$sel = $db->prepare("SELECT b FROM t WHERE a = :a");
$sel->execute(["a" => 7]);
echo $sel->fetchColumn();
"#,
    );
    assert_eq!(out, "seven");
}

/// `FETCH_NUM` returns a 0-indexed numeric array.
#[test]
fn test_pdo_fetch_num() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (5, 'five')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_NUM);
echo $row[0] . "/" . $row[1];
"#,
    );
    assert_eq!(out, "5/five");
}

/// `FETCH_BOTH` returns each column under both its numeric index and its name.
#[test]
fn test_pdo_fetch_both() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (9, 'nine')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_BOTH);
echo $row[0] . "=" . $row["id"] . "," . $row[1] . "=" . $row["name"];
"#,
    );
    assert_eq!(out, "9=9,nine=nine");
}

/// `FETCH_OBJ` returns a stdClass whose properties are the result columns.
#[test]
fn test_pdo_fetch_obj() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT, score REAL)");
$db->exec("INSERT INTO t VALUES (1, 'Ada', 9.5)");
$o = $db->query("SELECT id, name, score FROM t")->fetch(PDO::FETCH_OBJ);
echo $o->id . ":" . $o->name . ":" . $o->score;
"#,
    );
    assert_eq!(out, "1:Ada:9.5");
}

/// `FETCH_OBJ` uses real stdClass dynamic properties, so numeric column aliases
/// remain object properties instead of degrading through a JSON list round-trip.
#[test]
fn test_pdo_fetch_obj_numeric_property_name() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$o = $db->query("SELECT 7 AS \"0\", 'Ada' AS name")->fetch(PDO::FETCH_OBJ);
echo gettype($o) . ":" . $o->{"0"} . ":" . $o->name;
"#,
    );
    assert_eq!(out, "object:7:Ada");
}

/// `FETCH_CLASS` creates the requested class and assigns matching columns to
/// declared properties; `FETCH_INTO` fills an existing object instance.
#[test]
fn test_pdo_fetch_class_and_fetch_into() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");

$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$row = $stmt->fetch(PDO::FETCH_CLASS, Row::class);
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;

$stmt2 = $db->query("SELECT id, name FROM t WHERE id = 2");
$into = new Row();
$same = $stmt2->fetch(PDO::FETCH_INTO, $into);
echo "|" . (($same === $into) ? "same" : "different") . ":" . $into->id . ":" . $into->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada|same:2:Bob");
}

/// `setFetchMode()` stores the class/object target used by later no-argument
/// `fetch()` calls for `FETCH_CLASS` and `FETCH_INTO`.
#[test]
fn test_pdo_set_fetch_mode_class_and_into_targets() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");

$stmt = $db->query("SELECT id, name FROM t WHERE id = 1");
$stmt->setFetchMode(PDO::FETCH_CLASS, Row::class);
$row = $stmt->fetch();
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;

$stmt2 = $db->query("SELECT id, name FROM t WHERE id = 2");
$into = new Row();
$stmt2->setFetchMode(PDO::FETCH_INTO, $into);
$same = $stmt2->fetch();
echo "|" . (($same === $into) ? "same" : "different") . ":" . $into->id . ":" . $into->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada|same:2:Bob");
}

/// `fetchAll()` drains every row into an array, and `count()` reports the total.
#[test]
fn test_pdo_fetch_all() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");
$db->exec("INSERT INTO t VALUES (1), (2), (3)");
$rows = $db->query("SELECT n FROM t ORDER BY n")->fetchAll(PDO::FETCH_NUM);
$sum = 0;
foreach ($rows as $r) { $sum += $r[0]; }
echo count($rows) . ":" . $sum;
"#,
    );
    assert_eq!(out, "3:6");
}

/// `columnCount()` reports the number of result columns and `lastInsertId()`
/// returns the rowid of the last INSERT as a string.
#[test]
fn test_pdo_column_count_and_last_insert_id() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT, c REAL)");
$db->exec("INSERT INTO t (b, c) VALUES ('x', 1.0)");
$db->exec("INSERT INTO t (b, c) VALUES ('y', 2.0)");
$stmt = $db->query("SELECT a, b, c FROM t");
echo $stmt->columnCount() . ":" . $db->lastInsertId();
"#,
    );
    assert_eq!(out, "3:2");
}

/// A committed transaction persists its writes; a rolled-back one does not.
#[test]
fn test_pdo_transactions() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");

$db->beginTransaction();
$db->exec("INSERT INTO t VALUES (1)");
$db->rollBack();

$db->beginTransaction();
$db->exec("INSERT INTO t VALUES (2)");
$db->commit();

$rows = $db->query("SELECT n FROM t")->fetchAll(PDO::FETCH_NUM);
echo count($rows) . ":" . $rows[0][0];
"#,
    );
    assert_eq!(out, "1:2");
}

/// A failing `exec()` throws a catchable `PDOException` under the default
/// (exception) error mode.
#[test]
fn test_pdo_exception_on_bad_sql() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->exec("THIS IS NOT VALID SQL");
    echo "no-throw";
} catch (PDOException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// `bindValue()` with positional `?` placeholders binds typed values, which
/// survive `execute()` (reset keeps bindings; bindValue is applied at execute).
#[test]
fn test_pdo_bind_value_positional() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$ins = $db->prepare("INSERT INTO t (id, name) VALUES (?, ?)");
$ins->bindValue(1, 7, PDO::PARAM_INT);
$ins->bindValue(2, "seven");
$ins->execute();
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "7:seven");
}

/// `bindValue()` with named `:name` placeholders binds by parameter name.
#[test]
fn test_pdo_bind_value_named() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$ins = $db->prepare("INSERT INTO t (id, name) VALUES (:id, :name)");
$ins->bindValue(":id", 3, PDO::PARAM_INT);
$ins->bindValue(":name", "Cyd");
$ins->execute();
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "3:Cyd");
}

/// A statement that mixes a positional `?` and a named `:name` placeholder binds
/// both correctly. Regression for a parameter-inference bug that previously lost
/// the positional binding.
#[test]
fn test_pdo_mixed_positional_named_bind() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$ins = $db->prepare("INSERT INTO t (id, name) VALUES (?, :name)");
$ins->bindValue(1, 10, PDO::PARAM_INT);
$ins->bindValue(":name", "Ada");
$ins->execute();
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "10:Ada");
}

/// `bindParam()` binds the current value of the passed variable.
#[test]
fn test_pdo_bind_param() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");
$n = 42;
$ins = $db->prepare("INSERT INTO t (n) VALUES (?)");
$ins->bindParam(1, $n, PDO::PARAM_INT);
$ins->execute();
echo $db->query("SELECT n FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "42");
}

/// `setFetchMode()` sets the default mode used by an argument-less `fetch()` /
/// `fetchAll()`.
#[test]
fn test_pdo_set_fetch_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'a'), (2, 'b')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_NUM);
$out = "";
foreach ($stmt->fetchAll() as $r) { $out .= $r[0] . $r[1] . " "; }
echo trim($out);
"#,
    );
    assert_eq!(out, "1a 2b");
}

/// A SQL NULL column fetches as PHP null, and `rowCount()` reports rows affected
/// by a DML statement.
#[test]
fn test_pdo_null_value_and_row_count() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, note TEXT)");
$db->exec("INSERT INTO t (id, note) VALUES (1, NULL)");
$row = $db->query("SELECT note FROM t")->fetch(PDO::FETCH_ASSOC);
echo is_null($row["note"]) ? "null" : "notnull";
$stmt = $db->prepare("UPDATE t SET note = 'set' WHERE id = ?");
$stmt->execute([1]);
echo ":" . $stmt->rowCount();
"#,
    );
    assert_eq!(out, "null:1");
}

/// `foreach` over a `PDOStatement` (it is Traversable) walks the result set with
/// sequential integer keys, yielding each row in the statement's fetch mode.
#[test]
fn test_pdo_foreach_assoc_with_keys() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'alice'), (2, 'bob'), (3, 'carol')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $k => $row) {
    echo $k . ":" . $row["id"] . "=" . $row["name"] . ";";
}
"#,
    );
    assert_eq!(out, "0:1=alice;1:2=bob;2:3=carol;");
}

/// `foreach` honors `FETCH_NUM`, yielding positionally-keyed rows.
#[test]
fn test_pdo_foreach_num_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'one'), (2, 'two')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_NUM);
foreach ($stmt as $row) {
    echo $row[0] . "/" . $row[1] . ";";
}
"#,
    );
    assert_eq!(out, "1/one;2/two;");
}

/// `foreach` over a prepared statement walks the rows produced by `execute()`.
#[test]
fn test_pdo_foreach_prepared_statement() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'alice'), (2, 'bob'), (3, 'carol')");
$stmt = $db->prepare("SELECT name FROM t WHERE id >= ? ORDER BY id");
$stmt->execute([2]);
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $row) {
    echo $row["name"] . ";";
}
"#,
    );
    assert_eq!(out, "bob;carol;");
}

/// `foreach` over an empty result set runs zero iterations.
#[test]
fn test_pdo_foreach_empty_result() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->query("SELECT id FROM t");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
$n = 0;
foreach ($stmt as $row) {
    $n = $n + 1;
}
echo "rows=" . $n;
"#,
    );
    assert_eq!(out, "rows=0");
}

/// `PDO::quote()` wraps a string in single quotes and doubles embedded single
/// quotes, matching the SQLite driver, and the quoted literal round-trips through
/// a query.
#[test]
fn test_pdo_quote_and_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->quote("plain") . "|" . $db->quote("O'Brien") . "\n";
$db->exec("CREATE TABLE t (name TEXT)");
$name = "Tim O'Reilly";
$db->exec("INSERT INTO t (name) VALUES (" . $db->quote($name) . ")");
echo $db->query("SELECT name FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "'plain'|'O''Brien'\nTim O'Reilly");
}

/// `fetchAll(PDO::FETCH_COLUMN)` returns a flat array of the first column, and
/// `fetch(PDO::FETCH_COLUMN)` returns one scalar.
#[test]
fn test_pdo_fetch_column_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'a'), (2, 'b'), (3, 'c')");
$names = $db->query("SELECT name FROM t ORDER BY id")->fetchAll(PDO::FETCH_COLUMN);
echo implode(",", $names) . ":" . $db->query("SELECT id FROM t ORDER BY id")->fetch(PDO::FETCH_COLUMN);
"#,
    );
    assert_eq!(out, "a,b,c:1");
}

/// BLOB values are read through a byte-counted bridge path, preserving embedded
/// NUL bytes that would be truncated by C-string marshaling.
#[test]
fn test_pdo_blob_fetch_preserves_embedded_nul() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE blobs (data BLOB)");
$db->exec("INSERT INTO blobs VALUES (x'410042')");
$s = (string) $db->query("SELECT data FROM blobs")->fetchColumn();
echo strlen($s) . ":" . ord($s[0]) . ":" . ord($s[1]) . ":" . ord($s[2]);
"#,
    );
    assert_eq!(out, "3:65:0:66");
}

/// `foreach` honors `FETCH_COLUMN` with the column index set through
/// `setFetchMode(PDO::FETCH_COLUMN, $col)`, yielding that column's scalar per row.
#[test]
fn test_pdo_foreach_fetch_column_index() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'x'), (2, 'y'), (3, 'z')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_COLUMN, 1);
foreach ($stmt as $v) {
    echo $v . ";";
}
"#,
    );
    assert_eq!(out, "x;y;z;");
}

/// `getAttribute`/`setAttribute` round-trip `ATTR_ERRMODE`; the default mode is
/// `ERRMODE_EXCEPTION` (2) and `ATTR_DRIVER_NAME` reports the SQLite driver.
#[test]
fn test_pdo_get_set_attribute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->getAttribute(PDO::ATTR_ERRMODE);
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
echo ":" . $db->getAttribute(PDO::ATTR_ERRMODE);
echo ":" . $db->getAttribute(PDO::ATTR_DRIVER_NAME);
"#,
    );
    assert_eq!(out, "2:0:sqlite");
}

/// `ATTR_PERSISTENT` is accepted through constructor options and setAttribute(),
/// can be read back, and constructor-level truthy values opt into the
/// process-local DSN pool.
#[test]
fn test_pdo_persistent_attribute_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
echo $db->getAttribute(PDO::ATTR_PERSISTENT) ? "1" : "0";
$db->setAttribute(PDO::ATTR_PERSISTENT, false);
echo ":" . ($db->getAttribute(PDO::ATTR_PERSISTENT) ? "1" : "0");
"#,
    );
    assert_eq!(out, "1:0");
}

/// Constructor-level `ATTR_PERSISTENT` opens through the process-local DSN pool:
/// two `sqlite::memory:` handles with the option share the same in-memory DB.
#[test]
fn test_pdo_persistent_sqlite_memory_reuses_connection() {
    let out = compile_and_run(
        r#"<?php
$a = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
$a->exec("CREATE TABLE pdo_persist (n INTEGER)");
$a->exec("INSERT INTO pdo_persist VALUES (42)");

$b = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
echo $b->query("SELECT n FROM pdo_persist")->fetchColumn();
"#,
    );
    assert_eq!(out, "42");
}

/// Non-persistent `sqlite::memory:` connections stay isolated, so the persistent
/// pool only applies when requested by constructor options.
#[test]
fn test_pdo_nonpersistent_sqlite_memory_stays_isolated() {
    let out = compile_and_run(
        r#"<?php
$a = new PDO("sqlite::memory:");
$a->exec("CREATE TABLE pdo_isolated (n INTEGER)");
$a->exec("INSERT INTO pdo_isolated VALUES (7)");

$b = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
echo ($b->query("SELECT n FROM pdo_isolated") === false) ? "isolated" : "shared";
"#,
    );
    assert_eq!(out, "isolated");
}

/// `ERRMODE_SILENT` suppresses exceptions: `exec()`, `query()`, and `prepare()`
/// all return `false` (a real `false`, matched with `=== false`) on a SQL error
/// instead of throwing.
#[test]
fn test_pdo_errmode_silent_returns_false() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$r = $db->exec("THIS IS NOT SQL");
$stmt = $db->query("SELECT * FROM does_not_exist");
$prep = $db->prepare("ALSO NOT SQL");
echo (($r === false) ? "1" : "0")
    . (($stmt === false) ? "1" : "0")
    . (($prep === false) ? "1" : "0");
"#,
    );
    assert_eq!(out, "111");
}

/// A statement inherits the PDO error mode: DML failures during `execute()`
/// return `false` in silent mode instead of throwing.
#[test]
fn test_pdo_statement_execute_uses_silent_error_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY)");
$stmt = $db->prepare("INSERT INTO t (id) VALUES (?)");
$stmt->execute([1]);
$again = $stmt->execute([1]);
echo ($again === false) ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// The default `ERRMODE_EXCEPTION` still throws a `PDOException` on a SQL error.
#[test]
fn test_pdo_errmode_exception_default_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->getAttribute(PDO::ATTR_ERRMODE);
try {
    $db->exec("BAD SQL");
    echo ":no";
} catch (PDOException $e) {
    echo ":caught";
}
"#,
    );
    assert_eq!(out, "2:caught");
}

/// A driver-options array passed to the constructor seeds attributes, e.g.
/// `ATTR_ERRMODE`, so `exec()` returns `false` instead of throwing.
#[test]
fn test_pdo_constructor_options_errmode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
echo $db->getAttribute(PDO::ATTR_ERRMODE);
echo ":" . (($db->exec("BAD") === false) ? "false" : "other");
"#,
    );
    assert_eq!(out, "0:false");
}

/// `rowCount()` is snapshotted per statement at execute() time: a later DML on
/// the same connection (which moves the connection-wide change counter) must not
/// change an earlier statement's reported count. Here the UPDATE affects 3 rows
/// and the later DELETE affects 0; each statement keeps its own count.
#[test]
fn test_pdo_row_count_snapshot_is_per_statement() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1), (2), (3)");
$upd = $db->prepare("UPDATE t SET id = id + 10");
$upd->execute();
$del = $db->prepare("DELETE FROM t WHERE id < 0");
$del->execute();
echo $upd->rowCount() . ":" . $del->rowCount();
"#,
    );
    assert_eq!(out, "3:0");
}

/// An aliased import (`use PDO as Db;`) still injects the prelude and resolves to
/// PDO. The program references PDO only through the alias, so prelude detection
/// must inspect the import name — `new Db()` carries the alias, not "PDO".
#[test]
fn test_pdo_aliased_import() {
    let out = compile_and_run(
        r#"<?php
use PDO as Db;
$db = new Db("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (7)");
$row = $db->query("SELECT id FROM t")->fetch();
echo $row["id"];
"#,
    );
    assert_eq!(out, "7");
}

/// W1: after a successful operation, `errorCode()` reports the `"00000"` success
/// SQLSTATE rather than a native integer code.
#[test]
fn test_pdo_error_code_success() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
echo $db->errorCode();
"#,
    );
    assert_eq!(out, "00000");
}

/// W1: `errorInfo()` on success is the PHP-shaped triple `["00000", null, null]`.
#[test]
fn test_pdo_error_info_success() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$info = $db->errorInfo();
echo $info[0] . "|" . ($info[1] === null ? "n" : "x") . "|" . ($info[2] === null ? "n" : "x");
"#,
    );
    assert_eq!(out, "00000|n|n");
}

/// W1: a constraint violation surfaces the real SQLSTATE `23000` (php-src's
/// SQLite mapping) through `errorInfo()[0]` in silent mode.
#[test]
fn test_pdo_error_info_on_constraint() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$info = $db->errorInfo();
echo $info[0];
"#,
    );
    assert_eq!(out, "23000");
}

/// W1: in the default EXCEPTION mode a failed statement throws a `PDOException`
/// whose `errorInfo[0]` carries the SQLSTATE frameworks parse.
#[test]
fn test_pdo_exception_carries_error_info() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY)");
$db->exec("INSERT INTO t (id) VALUES (1)");
try {
    $db->exec("INSERT INTO t (id) VALUES (1)");
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->errorInfo[0];
}
"#,
    );
    assert_eq!(out, "23000");
}

/// W1: a statement tracks its own error state independently of the connection.
#[test]
fn test_pdo_statement_error_info() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->prepare("INSERT INTO t (id) VALUES (1)");
$stmt->execute();
$info = $stmt->errorInfo();
echo $info[0];
"#,
    );
    assert_eq!(out, "23000");
}

/// W2: `inTransaction()` tracks the transaction lifecycle across begin/commit.
#[test]
fn test_pdo_in_transaction_flag() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->inTransaction() ? "1" : "0";
$db->beginTransaction();
echo $db->inTransaction() ? "1" : "0";
$db->commit();
echo $db->inTransaction() ? "1" : "0";
"#,
    );
    assert_eq!(out, "010");
}

/// W2: a committed transaction persists its writes.
#[test]
fn test_pdo_transaction_commit_persists() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->beginTransaction();
$db->exec("INSERT INTO t (id) VALUES (42)");
$db->commit();
$row = $db->query("SELECT id FROM t")->fetch();
echo $row["id"];
"#,
    );
    assert_eq!(out, "42");
}

/// W2: starting a nested transaction is a logic error and throws.
#[test]
fn test_pdo_double_begin_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->beginTransaction();
try {
    $db->beginTransaction();
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "There is already an active transaction");
}

/// W2: committing with no active transaction is a logic error and throws.
#[test]
fn test_pdo_commit_without_transaction_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->commit();
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(out, "There is no active transaction");
}

/// W2: `PDO::getAvailableDrivers()` is a static returning the dispatchable drivers.
#[test]
fn test_pdo_get_available_drivers() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", PDO::getAvailableDrivers());
"#,
    );
    assert_eq!(out, "mysql,pgsql,sqlite");
}

/// W5: SQLite `quote()` wraps in single quotes and doubles embedded single quotes.
#[test]
fn test_pdo_quote_sqlite_doubles_single_quotes() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->quote("O'Brien");
"#,
    );
    assert_eq!(out, "'O''Brien'");
}

/// W3: `fetch()` on a statement that was never executed returns false instead of
/// silently stepping the query with NULL binds.
#[test]
fn test_pdo_fetch_before_execute_returns_false() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->prepare("SELECT id FROM t");
$r = $stmt->fetch();
echo $r === false ? "false" : "row";
"#,
    );
    assert_eq!(out, "false");
}

/// W3: `closeCursor()` requires a re-execute before the next fetch succeeds.
#[test]
fn test_pdo_close_cursor() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (5)");
$stmt = $db->query("SELECT id FROM t");
$first = $stmt->fetch();
$stmt->closeCursor();
$after = $stmt->fetch();
echo $first["id"] . "|" . ($after === false ? "false" : "row");
"#,
    );
    assert_eq!(out, "5|false");
}

/// W3: `fetchObject()` builds a stdClass with one property per column.
#[test]
fn test_pdo_fetch_object() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t (id, name) VALUES (5, 'Zoe')");
$o = $db->query("SELECT id, name FROM t")->fetchObject();
echo $o->id . ":" . $o->name;
"#,
    );
    assert_eq!(out, "5:Zoe");
}

/// W5: `ATTR_TIMEOUT` set via setAttribute() is stored and read back (seconds).
#[test]
fn test_pdo_attr_timeout_set_attribute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_TIMEOUT, 5);
echo $db->getAttribute(PDO::ATTR_TIMEOUT);
"#,
    );
    assert_eq!(out, "5");
}

/// W5: `ATTR_TIMEOUT` passed as a constructor option is applied after the open.
#[test]
fn test_pdo_attr_timeout_constructor_option() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_TIMEOUT => 3]);
echo $db->getAttribute(PDO::ATTR_TIMEOUT);
"#,
    );
    assert_eq!(out, "3");
}

/// W5: `lastInsertId()` returns the last auto-increment id via the text bridge.
#[test]
fn test_pdo_last_insert_id_text_bridge() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('a')");
$db->exec("INSERT INTO t (name) VALUES ('b')");
echo $db->lastInsertId();
"#,
    );
    assert_eq!(out, "2");
}

/// W6: the added PHP 8.4 constants resolve to their documented numeric values,
/// including the OR-able fetch flags (hex).
#[test]
fn test_pdo_constants_present() {
    let out = compile_and_run(
        r#"<?php
echo PDO::FETCH_KEY_PAIR . "," . PDO::FETCH_GROUP . "," . PDO::FETCH_UNIQUE . "," . PDO::ATTR_DEFAULT_FETCH_MODE . "," . PDO::ATTR_EMULATE_PREPARES . "," . PDO::CURSOR_SCROLL;
"#,
    );
    assert_eq!(out, "12,65536,196608,19,20,1");
}

/// W4: `ATTR_DEFAULT_FETCH_MODE` set via setAttribute() governs a no-mode fetch().
#[test]
fn test_pdo_default_fetch_mode_set_attribute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_ASSOC);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t (id, name) VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t")->fetch();
echo $row["name"] . "|" . (isset($row[0]) ? "both" : "assoc");
"#,
    );
    assert_eq!(out, "Ada|assoc");
}

/// W4: `ATTR_DEFAULT_FETCH_MODE` passed as a constructor option is honored.
#[test]
fn test_pdo_default_fetch_mode_constructor() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_NUM]);
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (9)");
$row = $db->query("SELECT id FROM t")->fetch();
echo $row[0];
"#,
    );
    assert_eq!(out, "9");
}

/// W4: `getAttribute(ATTR_DEFAULT_FETCH_MODE)` reads back the stored default.
#[test]
fn test_pdo_get_default_fetch_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_OBJ);
echo $db->getAttribute(PDO::ATTR_DEFAULT_FETCH_MODE);
"#,
    );
    assert_eq!(out, "5");
}

/// W4: `fetchAll(FETCH_KEY_PAIR)` maps a 2-column result to `[col0 => col1]`.
#[test]
fn test_pdo_fetch_all_key_pair() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$db->exec("INSERT INTO t VALUES (2, 'Bob')");
$pairs = $db->query("SELECT id, name FROM t ORDER BY id")->fetchAll(PDO::FETCH_KEY_PAIR);
$out = count($pairs) . "|";
foreach ($pairs as $k => $v) { $out .= $k . ":" . $v . ";"; }
echo $out;
"#,
    );
    assert_eq!(out, "2|1:Ada;2:Bob;");
}

/// W4: `FETCH_KEY_PAIR` on a result without exactly two columns throws.
#[test]
fn test_pdo_fetch_key_pair_wrong_column_count_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b INTEGER, c INTEGER)");
$db->exec("INSERT INTO t VALUES (1, 2, 3)");
try {
    $db->query("SELECT a, b, c FROM t")->fetchAll(PDO::FETCH_KEY_PAIR);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// W4/W6: an unsupported base fetch mode fails loudly instead of returning wrong
/// data (FETCH_LAZY).
#[test]
fn test_pdo_fetch_lazy_unsupported_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->fetch(PDO::FETCH_LAZY);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// W4: an OR-able reshaping flag (FETCH_GROUP) that is not yet implemented fails
/// loudly rather than silently returning a flat list.
#[test]
fn test_pdo_fetch_group_unsupported_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
try {
    $db->query("SELECT id FROM t")->fetchAll(PDO::FETCH_GROUP);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// W7 namespace prerequisite (smoke test, no prelude change): a user-defined
/// class in a block namespace can `extends \PDO`, inherit the real prelude
/// `PDO::__construct` (a genuine emitted body, not a synthesized stub), dispatch
/// inherited methods (`exec`/`query`/`fetch`) through the vtable, and satisfy
/// `instanceof \PDO`. This proves the single unproven capability the shipped
/// `Pdo\Sqlite`/`Mysql`/`Pgsql` subclasses depend on: a namespaced class whose
/// method symbols mangle `\` to `_N_` links and inherits from the flat prelude
/// class it extends.
#[test]
fn test_pdo_namespaced_subclass_extends_prelude_pdo() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    class MyPdo extends \PDO {}
}
namespace {
    $db = new \App\MyPdo("sqlite::memory:");
    $db->exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
    $db->exec("INSERT INTO users (name) VALUES ('Ada')");
    $row = $db->query("SELECT id, name FROM users")->fetch(\PDO::FETCH_ASSOC);
    $is_pdo = $db instanceof \PDO ? "1" : "0";
    echo $row["id"] . ":" . $row["name"] . ":" . $is_pdo;
}
"#,
    );
    assert_eq!(out, "1:Ada:1");
}

/// W7 prelude subclasses: a program that references only `Pdo\Sqlite` — never the
/// base `PDO` name — still injects the prelude (driver-subclass detection) and the
/// subclass drives an in-memory database through its inherited base methods.
#[test]
fn test_pdo_driver_subclass_sqlite_alone_triggers_injection() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('Zed')");
echo $db->query("SELECT name FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "Zed");
}

/// W7 prelude subclasses: a `Pdo\Sqlite` instance satisfies `instanceof` for both
/// its own namespaced class and the base `\PDO` it extends, confirming the
/// inheritance edge survives injection, name resolution, and `\`->`_N_` mangling.
#[test]
fn test_pdo_driver_subclass_instanceof_edges() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$is_sqlite = $db instanceof \Pdo\Sqlite ? "1" : "0";
$is_pdo = $db instanceof \PDO ? "1" : "0";
echo $is_sqlite . $is_pdo;
"#,
    );
    assert_eq!(out, "11");
}

/// W7 prelude subclasses: `Pdo\Mysql` and `Pdo\Pgsql` are real, resolvable classes
/// that inherit the base `PDO` constant surface. Static constant access errors on
/// an undefined class at compile time (unlike `instanceof`, which the checker does
/// not validate), so a passing result proves both subclasses exist and flatten
/// `PDO`'s constants — without needing a live MySQL/PostgreSQL server.
#[test]
fn test_pdo_driver_subclass_mysql_pgsql_inherit_constants() {
    let out = compile_and_run(
        r#"<?php
$mysql_errmode = \Pdo\Mysql::ERRMODE_EXCEPTION;
$pgsql_fetch = \Pdo\Pgsql::FETCH_ASSOC;
echo $mysql_errmode . ":" . $pgsql_fetch;
"#,
    );
    assert_eq!(out, "2:2");
}

/// `PDO::connect()` (PHP 8.4 static factory): a `sqlite:` DSN dispatches to a
/// working `Pdo\Sqlite`. The returned object opens the connection and drives a
/// query through its inherited base methods, and it satisfies `instanceof` for
/// both the concrete `\Pdo\Sqlite` it is and the `\PDO` base it extends — proving
/// the factory returns the real subclass instance, not a bare base `PDO`.
#[test]
fn test_pdo_connect_sqlite_returns_working_subclass() {
    let out = compile_and_run(
        r#"<?php
$db = \PDO::connect("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('Ada')");
$name = $db->query("SELECT name FROM t")->fetchColumn();
$is_sqlite = $db instanceof \Pdo\Sqlite ? "1" : "0";
$is_pdo = $db instanceof \PDO ? "1" : "0";
echo $name . ":" . $is_sqlite . $is_pdo;
"#,
    );
    assert_eq!(out, "Ada:11");
}

/// `PDO::connect()` with a DSN whose prefix matches no known driver throws a
/// `PDOException` ("could not find driver"), matching PHP's factory behavior.
#[test]
fn test_pdo_connect_unknown_driver_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    \PDO::connect("bogus:host=localhost");
    echo "no-throw";
} catch (\PDOException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// The `PDO` return type of `PDO::connect()` threads through a `\PDO`-typed
/// parameter: the subclass instance is accepted where a base `\PDO` is expected
/// and its inherited methods dispatch, confirming the subclass-to-base upcast on
/// the declared return type is sound across a function boundary.
#[test]
fn test_pdo_connect_result_threads_as_base_pdo() {
    let out = compile_and_run(
        r#"<?php
function seed(\PDO $db): int {
    $db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER)");
    $db->exec("INSERT INTO t (n) VALUES (7)");
    return (int) $db->query("SELECT n FROM t")->fetchColumn();
}
echo seed(\PDO::connect("sqlite::memory:"));
"#,
    );
    assert_eq!(out, "7");
}
