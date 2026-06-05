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
