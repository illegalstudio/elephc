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
use elephc::php_version::PhpVersion;

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

/// P2-f: `PDO::prepare("")` throws a `ValueError` before any driver call at all,
/// matching php-src's `zend_argument_must_not_be_empty_error`.
#[test]
fn test_pdo_prepare_empty_query_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->prepare("");
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
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
///
/// F-STMT-01: the class/object TARGET reaches the statement through
/// `setFetchMode()`, never through `fetch()`. php-src's stub is `fetch(int $mode =
/// PDO::FETCH_DEFAULT, int $cursorOrientation = PDO::FETCH_ORI_NEXT, int
/// $cursorOffset = 0)` — position 2 is an INT ORIENTATION — so the idiom this test
/// used to assert, `fetch(PDO::FETCH_CLASS, Row::class)`, is a TypeError on real PHP
/// 8.4. It was a fabricated `mixed $classOrObject` parameter, and the test locked it
/// in; both are gone.
///
/// This still exercises a path the sibling `setFetchMode` test below does NOT: the
/// mode is passed EXPLICITLY to `fetch()` while only the target comes from
/// `setFetchMode()`, so it pins that an explicit `$mode` argument does not discard
/// the separately-configured target.
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
$stmt->setFetchMode(PDO::FETCH_CLASS, Row::class);
$row = $stmt->fetch(PDO::FETCH_CLASS);
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;

$stmt2 = $db->query("SELECT id, name FROM t WHERE id = 2");
$into = new Row();
$stmt2->setFetchMode(PDO::FETCH_INTO, $into);
$same = $stmt2->fetch(PDO::FETCH_INTO);
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

/// `bindParam()` reads the referenced variable at each execute rather than at bind time.
#[test]
fn test_pdo_bind_param() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");
$n = 42;
$ins = $db->prepare("INSERT INTO t (n) VALUES (?)");
$ins->bindParam(1, $n, PDO::PARAM_INT);
$n = 43;
$ins->execute();
$n = 44;
$ins->execute();
$rows = $db->query("SELECT n FROM t ORDER BY rowid")->fetchAll(PDO::FETCH_COLUMN, 0);
echo $rows[0] . ":" . $rows[1];
"#,
    );
    assert_eq!(out, "43:44");
}

/// P1-c: `execute($params)` REPLACES prior `bindValue()`/`bindParam()` bindings
/// rather than layering `$params` on top of them (matching php-src, which
/// destroys and rebuilds `bound_params` from `$input_params`). Slot 2 is
/// pre-bound to a stale 99 via `bindValue()`; `execute([0 => 7])` only supplies
/// slot 1 (array key 0 -> the first `?`), so slot 2 must come back NULL
/// (unbound for this call) instead of the stale 99 a buggy merge would keep.
#[test]
fn test_pdo_execute_params_replaces_prior_binds() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b INTEGER)");
$ins = $db->prepare("INSERT INTO t (a, b) VALUES (?, ?)");
$ins->bindValue(2, 99, PDO::PARAM_INT);
$ins->execute([0 => 7]);
$row = $db->query("SELECT a, b FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["a"] . ":" . ($row["b"] === null ? "null" : $row["b"]);
"#,
    );
    assert_eq!(out, "7:null");
}

/// P2 (this slice): php-src's `pdo_stmt_bind_input_params` DESTROYS
/// `stmt->bound_params` and REBUILDS it from `$input_params`, so a LATER
/// no-arg `execute()` replays THAT array, not whatever `bindValue()`/
/// `bindParam()` call preceded it. Slot 1 is pre-bound to a stale `"a"` via
/// `bindValue()`; `execute(["b"])` rebinds it to `"b"` for its own call AND
/// records `"b"` as the new replay bindings, so the immediately-following
/// no-arg `execute()` must insert `"b"` again — not replay the stale `"a"`
/// a buggy implementation (recording only the ORIGINAL bindValue() call)
/// would keep.
#[test]
fn test_pdo_execute_params_persists_as_new_bindings_for_next_execute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (val TEXT)");
$ins = $db->prepare("INSERT INTO t (val) VALUES (?)");
$ins->bindValue(1, "a");
$ins->execute(["b"]);
$ins->execute();
$rows = $db->query("SELECT val FROM t ORDER BY rowid")->fetchAll(PDO::FETCH_NUM);
echo count($rows) . ":" . $rows[0][0] . ":" . $rows[1][0];
"#,
    );
    assert_eq!(out, "2:b:b");
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

/// P2-d: `setFetchMode(PDO::FETCH_COLUMN, $n)` with a negative column index
/// throws a `ValueError` and leaves the statement's previous fetch mode
/// untouched, mirroring php-src's `pdo_stmt_setup_fetch_mode`.
#[test]
fn test_pdo_set_fetch_mode_negative_column_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_COLUMN, -1);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw";
}
// The prior default fetch mode (FETCH_BOTH, from the connection default) must
// still be in effect — the rejected call did not partially apply.
$row = $stmt->fetch();
echo ":" . (isset($row["id"]) && isset($row[0]) ? "both" : "other");
"#,
    );
    assert_eq!(out, "threw:both");
}

/// P2-d: an out-of-range base fetch mode passed to `setFetchMode()` throws a
/// `ValueError` instead of silently behaving like `FETCH_BOTH`.
#[test]
fn test_pdo_set_fetch_mode_unknown_mode_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(999);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// P3: `setFetchMode(PDO::FETCH_COLUMN)` with no column argument throws —
/// mirroring php-src's `pdo_stmt_setup_fetch_mode`, which raises an
/// `ArgumentCountError` here (elephc has no `ArgumentCountError` class, so
/// this raises the closest `ValueError`, using php-src's exact message text).
#[test]
fn test_pdo_set_fetch_mode_column_missing_arg_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_COLUMN);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given"
    );
}

/// P3: `setFetchMode(PDO::FETCH_CLASS)` with no class-name argument throws the
/// same way (php-src's ArgumentCountError says "at least 2" here, since a
/// third constructor-args argument is also optional).
#[test]
fn test_pdo_set_fetch_mode_class_missing_arg_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_CLASS);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::setFetchMode() expects at least 2 arguments for the fetch mode provided, 1 given"
    );
}

/// P3: `setFetchMode(PDO::FETCH_INTO)` with no target-object argument throws.
#[test]
fn test_pdo_set_fetch_mode_into_missing_arg_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_INTO);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given"
    );
}

/// P3: `setFetchMode(PDO::FETCH_FUNC)` is rejected — php-src's
/// `pdo_stmt_verify_mode` only allows `FETCH_FUNC` as `fetchAll()`'s first
/// argument, not `setFetchMode()`'s (same message both call sites use).
#[test]
fn test_pdo_set_fetch_mode_func_rejected() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_FUNC);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()"
    );
}

/// P3: the negative-`FETCH_COLUMN`-index `ValueError` names the real
/// underlying parameter php-src's arginfo carries for this position — the
/// variadic `$args`, not elephc's own `$colno` parameter name (verified
/// against php-src's `pdo_stmt_setup_fetch_mode`, whose
/// `zend_argument_value_error(arg1_arg_num, ...)` resolves the name from
/// `setFetchMode(int $mode, mixed ...$args)`'s arginfo).
#[test]
fn test_pdo_set_fetch_mode_negative_column_message_names_args_param() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_COLUMN, -1);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::setFetchMode(): Argument #2 ($args) must be greater than or equal to 0"
    );
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

/// P1-h: `setAttribute(ATTR_ERRMODE, ...)` rejects a value that is not one of the
/// PDO::ERRMODE_* constants with a `ValueError`, and leaves the error mode
/// unchanged (still reads back as EXCEPTION == 2 afterward).
#[test]
fn test_pdo_set_attribute_errmode_rejects_invalid() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->setAttribute(PDO::ATTR_ERRMODE, 42);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw";
}
echo ":" . $db->getAttribute(PDO::ATTR_ERRMODE);
"#,
    );
    assert_eq!(out, "threw:2");
}

/// `ATTR_PERSISTENT` is a constructor-only choice. A later `setAttribute()` is
/// rejected and cannot change the live handle's persistent status.
#[test]
fn test_pdo_persistent_attribute_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
echo $db->getAttribute(PDO::ATTR_PERSISTENT) ? "1" : "0";
$changed = $db->setAttribute(PDO::ATTR_PERSISTENT, false);
echo ":" . (($changed === false) ? "rejected" : "changed")
    . ":" . ($db->getAttribute(PDO::ATTR_PERSISTENT) ? "1" : "0");
"#,
    );
    assert_eq!(out, "1:rejected:1");
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

/// P1-2: SQLite's `rowCount()` always reports `0` after a SELECT-style
/// (column-returning) statement — never the stale write count of a prior DML on
/// the same connection. Verified against a real PHP 8.5 CLI with the same
/// fixture (three INSERTs then a SELECT).
#[test]
fn test_pdo_sqlite_row_count_zero_after_select() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$db->exec("INSERT INTO t (id) VALUES (2)");
$db->exec("INSERT INTO t (id) VALUES (3)");
$stmt = $db->query("SELECT id FROM t");
echo $stmt->rowCount();
"#,
    );
    assert_eq!(out, "0");
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

/// Verifies fresh connections/statements expose php-src's uninitialized error state,
/// while a successful prepare initializes only the owning connection to `"00000"`.
#[test]
fn test_pdo_error_state_is_uninitialized_before_first_operation() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$dbInfo = $db->errorInfo();
$stmt = $db->prepare("SELECT 1");
$stmtInfo = $stmt->errorInfo();
echo ($dbInfo[0] === "" ? "empty" : "set") . ":" . ($dbInfo[1] === null ? "n" : "x") . "|";
echo ($stmt->errorCode() === null ? "null" : "set") . ":" . ($stmtInfo[0] === "" ? "empty" : "set") . "|";
echo $db->errorCode();
"#,
    );
    assert_eq!(out, "empty:n|null:empty|00000");
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

/// P1-g: `inTransaction()` reads the driver's LIVE state, not just a PHP-side
/// flag — SQLite's `sqlite3_get_autocommit` reports a transaction started by a
/// raw `exec("BEGIN")` (bypassing `beginTransaction()`) as active, and reports
/// it to the ordinary `PDO::commit()` guard, which clears it successfully.
#[test]
fn test_pdo_in_transaction_reflects_raw_begin() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("BEGIN");
echo $db->inTransaction() ? "1" : "0";
$db->commit();
echo $db->inTransaction() ? "1" : "0";
"#,
    );
    assert_eq!(out, "10");
}

/// P1-g: `beginTransaction()`'s already-active guard also consults the live
/// state, so it raises PHP's clean "already an active transaction" error even
/// when the transaction was started by a raw `exec("BEGIN")` rather than by
/// `beginTransaction()` itself (which never ran, so `$inTxn` alone would have
/// missed it).
#[test]
fn test_pdo_begin_after_raw_begin_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("BEGIN");
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

/// P2-7: a fresh SQLite connection seeds a 60s (60000ms) busy-timeout by default
/// (matching real PHP, verified against a PHP 8.5 CLI: `PRAGMA busy_timeout`
/// reports `60000` right after `new PDO(...)`), rather than the pre-fix `0`
/// (immediate `SQLITE_BUSY` on any lock contention).
#[test]
fn test_pdo_sqlite_default_busy_timeout_is_60000() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->query("PRAGMA busy_timeout")->fetchColumn();
"#,
    );
    assert_eq!(out, "60000");
}

/// `ATTR_TIMEOUT` set via `setAttribute()` changes SQLite's live busy-timeout. Like
/// php-src's SQLite driver, the write-only attribute is not readable via PDO.
#[test]
fn test_pdo_attr_timeout_set_attribute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$ok = $db->setAttribute(PDO::ATTR_TIMEOUT, 5);
echo ($ok ? "set" : "failed") . "|" . $db->query("PRAGMA busy_timeout")->fetchColumn();
"#,
    );
    assert_eq!(out, "set|5000");
}

/// `ATTR_TIMEOUT` passed as a constructor option is applied after the open.
#[test]
fn test_pdo_attr_timeout_constructor_option() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_TIMEOUT => 3]);
echo $db->query("PRAGMA busy_timeout")->fetchColumn();
"#,
    );
    assert_eq!(out, "3000");
}

/// P1-10: `Pdo\Sqlite::ATTR_OPEN_FLAGS` (a constructor option) threads through to
/// the bridge open — `OPEN_READONLY` opens a connection that rejects a write
/// (`exec()` returns false) against a file a prior read-write connection created.
#[test]
fn test_pdo_sqlite_attr_open_flags_readonly_rejects_write() {
    let out = compile_and_run(
        r#"<?php
$path = tempnam(sys_get_temp_dir(), "elephc_pdo_open_flags_");
unlink($path);
$rw = new \Pdo\Sqlite("sqlite:" . $path);
$rw->exec("CREATE TABLE t (n INTEGER)");

$ro = new \Pdo\Sqlite("sqlite:" . $path, null, null, [
    \Pdo\Sqlite::ATTR_OPEN_FLAGS => \Pdo\Sqlite::OPEN_READONLY,
    PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT,
]);
$result = $ro->exec("INSERT INTO t VALUES (1)");
echo ($result === false) ? "rejected" : "allowed";
unlink($path);
"#,
    );
    assert_eq!(out, "rejected");
}

/// P2-9: a `sqlite:file:...?mode=ro` DSN body enables `SQLITE_OPEN_URI`, so
/// `mode=ro` is honored — opening a nonexistent database this way throws
/// (read-only cannot create the file) instead of silently creating a new file
/// at the literal, unparsed `file:...?mode=ro` path. Verified against a real
/// PHP 8.5 CLI with the same DSN shape.
#[test]
fn test_pdo_sqlite_file_uri_dsn_mode_ro_nonexistent_throws() {
    let out = compile_and_run(
        r#"<?php
$path = tempnam(sys_get_temp_dir(), "elephc_pdo_uri_ro_");
unlink($path);
try {
    $db = new PDO("sqlite:file:" . $path . "?mode=ro");
    echo "opened";
} catch (\PDOException $e) {
    echo "threw";
}
echo ":" . (file_exists($path) ? "created" : "absent");
"#,
    );
    assert_eq!(out, "threw:absent");
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

/// F-CORE-18: `lastInsertId()`'s success path still returns a plain string
/// (compares `===` equal, not a boxed/coerced value) now that its return type is
/// `string|bool` — SQLite always succeeds after an insert, so this is a
/// regression guard that widening the signature left the success arm untouched.
#[test]
fn test_pdo_last_insert_id_success_strict_string() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('a')");
echo ($db->lastInsertId() === "1") ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies the default PHP 8.4 compatibility mode retains the historical high-bit fetch
/// constants and the remaining core constant values.
#[test]
fn test_pdo_constants_present() {
    let out = compile_and_run(
        r#"<?php
echo PDO::FETCH_KEY_PAIR . "," . PDO::FETCH_GROUP . "," . PDO::FETCH_UNIQUE . "," . PDO::ATTR_DEFAULT_FETCH_MODE . "," . PDO::ATTR_EMULATE_PREPARES . "," . PDO::CURSOR_SCROLL;
"#,
    );
    assert_eq!(out, "12,65536,196608,19,20,1");
}

/// Verifies PHP 8.5 selects the compact fetch-flag values and decodes a grouped column fetch
/// with the matching low-nibble base-mode mask.
#[test]
fn test_pdo_php85_compact_fetch_flags_and_grouping() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (category TEXT, value TEXT)");
$db->exec("INSERT INTO t VALUES ('a', 'x'), ('a', 'y')");
$rows = $db->query("SELECT category, value FROM t ORDER BY rowid")
    ->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN);
echo PDO::FETCH_GROUP . "," . PDO::FETCH_UNIQUE . "," . PDO::FETCH_CLASSTYPE . ","
    . PDO::FETCH_PROPS_LATE . "," . PDO::FETCH_SERIALIZE . "|"
    . $rows["a"][0] . $rows["a"][1];
"#,
        PhpVersion::Php85,
    );
    assert_eq!(out, "32,64,128,256,512|xy");
}

/// Verifies a pre-8.4 target keeps the legacy driver-extension methods usable
/// even though namespaced driver classes and `PDO::connect()` are not generated.
#[test]
fn test_pdo_php83_legacy_sqlite_surface_executes() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->sqliteCreateFunction("double_it", function($value) { return $value * 2; }, 1);
echo $db->query("SELECT double_it(6)")->fetchColumn();
"#,
        PhpVersion::Php83,
    );
    assert_eq!(out, "12");
}

/// Verifies PHP 8.5 exposes and executes the new SQLite connection and statement attributes.
#[test]
fn test_pdo_php85_sqlite_transaction_busy_and_explain_attributes() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new Pdo\Sqlite("sqlite::memory:");
echo Pdo\Sqlite::ATTR_BUSY_STATEMENT . "," . Pdo\Sqlite::ATTR_EXPLAIN_STATEMENT . ","
    . Pdo\Sqlite::ATTR_TRANSACTION_MODE . "," . Pdo\Sqlite::OK . ","
    . Pdo\Sqlite::DENY . "," . Pdo\Sqlite::IGNORE . "|";
echo $db->getAttribute(Pdo\Sqlite::ATTR_TRANSACTION_MODE);
echo $db->setAttribute(Pdo\Sqlite::ATTR_TRANSACTION_MODE, Pdo\Sqlite::TRANSACTION_MODE_IMMEDIATE) ? "T" : "F";
echo $db->getAttribute(Pdo\Sqlite::ATTR_TRANSACTION_MODE) . "|";
$stmt = $db->prepare("SELECT 1 AS value");
echo $stmt->getAttribute(Pdo\Sqlite::ATTR_BUSY_STATEMENT) ? "T" : "F";
echo $stmt->setAttribute(Pdo\Sqlite::ATTR_EXPLAIN_STATEMENT, Pdo\Sqlite::EXPLAIN_MODE_EXPLAIN) ? "T" : "F";
echo $stmt->getAttribute(Pdo\Sqlite::ATTR_EXPLAIN_STATEMENT);
$stmt->execute();
echo $stmt->getAttribute(Pdo\Sqlite::ATTR_BUSY_STATEMENT) ? "T" : "F";
"#,
        PhpVersion::Php85,
    );
    assert_eq!(out, "1003,1004,1005,0,1,2|0T1|FT1T");
}

/// PHP 8.5 SQLite driver attributes supplied in the constructor must affect the
/// newly opened native handle just like later `setAttribute()` calls; their
/// numeric collision with MySQL options must not divert them into MySQL config.
#[test]
fn test_pdo_php85_sqlite_constructor_attributes_reach_native_handle() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new Pdo\Sqlite("sqlite::memory:", null, null, [
    Pdo\Sqlite::ATTR_TRANSACTION_MODE => Pdo\Sqlite::TRANSACTION_MODE_IMMEDIATE,
    Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES => true,
]);
$db->exec("CREATE TABLE t (id INTEGER UNIQUE)");
$db->exec("INSERT INTO t VALUES (1)");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$db->exec("INSERT INTO t VALUES (1)");
echo $db->getAttribute(Pdo\Sqlite::ATTR_TRANSACTION_MODE) . ":" . $db->errorInfo()[1];
"#,
        PhpVersion::Php85,
    );
    assert_eq!(out, "1:2067");
}

/// PHP 8.6 adopts pdo_pgsql's persistent-disconnect `DISCARD ALL`: the final
/// live PDO owner resets session state, while releasing one of two simultaneous
/// owners must not disrupt the still-live object sharing that pooled handle.
#[test]
#[ignore]
fn test_pdo_php86_pgsql_persistent_release_discards_session_state() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$dsn = (string) getenv("ELEPHC_PG_DSN");
$a = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "php86-reset"]);
$b = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "php86-reset"]);
$pid = $a->query("SELECT pg_backend_pid()::text")->fetchColumn();
$a->exec("SET application_name = 'elephc-dirty'");
$a = null;
$stillDirty = $b->query("SHOW application_name")->fetchColumn();
$b = null;
$c = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "php86-reset"]);
$samePid = $c->query("SELECT pg_backend_pid()::text")->fetchColumn();
$reset = $c->query("SHOW application_name")->fetchColumn();
echo $stillDirty . ":" . (($pid === $samePid) ? "same" : "new") . ":[" . $reset . "]";
"#,
        PhpVersion::Php86,
    );
    assert_eq!(out, "elephc-dirty:same:[]");
}

/// Releasing a dynamically allocated PDOStatement drops its rooted PDO owner, so
/// overwriting the connection's last userland reference runs its destructor.
#[test]
fn test_pdo_statement_release_drops_connection_owner() {
    let out = compile_and_run(
        r#"<?php
class TrackedPDO extends PDO {
    public static int $alive = 0;
    public function __construct(string $dsn = "sqlite::memory:", ?string $username = null, ?string $password = null, ?array $options = null) {
        parent::__construct("sqlite::memory:");
        TrackedPDO::$alive = 1;
    }
    public function __destruct() {
        TrackedPDO::$alive = 0;
    }
}
$db = new TrackedPDO();
$stmt = $db->query("SELECT 1");
unset($stmt);
$db = null;
echo TrackedPDO::$alive;
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies PHP 8.5's SQLite authorizer receives the five php-src arguments,
/// controls statement preparation, and can be removed with a nullable reset.
#[test]
fn test_pdo_php85_sqlite_authorizer_callback_and_reset() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new Pdo\Sqlite("sqlite::memory:");
$db->setAuthorizer(function($action, $arg1, $arg2, $arg3, $arg4) {
    echo $action . ":" . $arg1 . ":" . $arg2 . ":" . $arg3 . ":" . $arg4 . ";";
    return Pdo\Sqlite::OK;
});
echo $db->query("SELECT 7")->fetchColumn() . "|";
$db->setAuthorizer(function($action, $arg1, $arg2, $arg3, $arg4) {
    return Pdo\Sqlite::DENY;
});
try {
    $db->exec("CREATE TABLE denied (value INTEGER)");
    echo "allowed|";
} catch (PDOException $error) {
    echo $error->errorInfo[1] . "|";
}
$db->setAuthorizer(function() { return "FAIL"; });
try {
    $db->query("SELECT 1");
} catch (Error $error) {
    $_message = $error->getMessage();
    echo $_message;
    echo "|";
}
$db->setAuthorizer(function() { return 4200; });
try {
    $db->query("SELECT 1");
} catch (Error $error) {
    $_message = $error->getMessage();
    echo $_message;
    echo "|";
}
$db->setAuthorizer(null);
echo $db->exec("CREATE TABLE t (value INTEGER)");
"#,
        PhpVersion::Php85,
    );
    assert_eq!(
        out,
        "21::::;7|23|PDO::query(): Return value of the authorizer callback must be of type int, string returned|PDO::query(): Return value of the authorizer callback must be one of Pdo\\Sqlite::OK, Pdo\\Sqlite::DENY, or Pdo\\Sqlite::IGNORE|0"
    );
}

/// Verifies SQLite callback registration normalizes every PHP callable form
/// through a rooted closure descriptor for scalar, collation, and aggregate hooks.
#[test]
fn test_pdo_sqlite_callbacks_accept_all_callable_forms() {
    let out = compile_and_run(
        r#"<?php
function pdo_named_twice($value) { return $value * 2; }
function pdo_reverse_compare($left, $right) { return strcmp($right, $left); }
function pdo_sum_step($context, $rowNumber, $value) {
    if ($context === null) { $context = 0; }
    return $context + $value;
}
function pdo_sum_final($context, $rowNumber) { return $context; }

class PdoSqliteCallbackForms {
    public static function triple($value) { return $value * 3; }
    public function quadruple($value) { return $value * 4; }
}
class PdoSqliteInvoker {
    public function __invoke($value) { return $value * 5; }
}

$db = new Pdo\Sqlite("sqlite::memory:");
$handlers = new PdoSqliteCallbackForms();
$db->createFunction("named_twice", "pdo_named_twice", 1);
$db->createFunction("static_triple", [PdoSqliteCallbackForms::class, "triple"], 1);
$db->createFunction("instance_four", [$handlers, "quadruple"], 1);
$db->createFunction("invoke_five", new PdoSqliteInvoker(), 1);
$db->createCollation("reverse_named", "pdo_reverse_compare");
$db->createAggregate("named_sum", "pdo_sum_step", "pdo_sum_final", 1);
$row = $db->query("SELECT named_twice(2), static_triple(2), instance_four(2), invoke_five(2)")->fetch(PDO::FETCH_NUM);
echo $row[0] . $row[1] . $row[2] . $row[3] . ":";
$values = $db->query("SELECT 'a' AS value UNION ALL SELECT 'b' ORDER BY value COLLATE reverse_named")->fetchAll(PDO::FETCH_COLUMN);
echo $values[0] . $values[1] . ":";
echo $db->query("SELECT named_sum(value) FROM (SELECT 2 AS value UNION ALL SELECT 3)")->fetchColumn();
"#,
    );
    assert_eq!(out, "46810:ba:5");
}

/// Verifies replacement roots use SQLite's case-insensitive `(name, arity)` key:
/// replacing one scalar arity releases only that descriptor while another arity
/// remains callable, including receiver-bound descriptors created from arrays.
#[test]
fn test_pdo_sqlite_callback_replacement_preserves_other_arities() {
    let out = compile_and_run(
        r#"<?php
class PdoReplacementHandler {
    public function twice($value) { return $value * 2; }
}

$db = new Pdo\Sqlite("sqlite::memory:");
$handler = new PdoReplacementHandler();
$db->createFunction("Calc", [$handler, "twice"], 1);
$db->createFunction("calc", function($left, $right) { return $left + $right; }, 2);
echo $db->query("SELECT calc(3), CALC(3, 4)")->fetchColumn(0) . ":";
$db->createFunction("CALC", function($value) { return $value * 3; }, 1);
$row = $db->query("SELECT calc(3), calc(3, 4)")->fetch(PDO::FETCH_NUM);
echo $row[0] . ":" . $row[1];
"#,
    );
    assert_eq!(out, "6:9:7");
}

/// Verifies PDO teardown unregisters native callbacks before a persistent SQLite
/// handle is reused, so the next object cannot call a descriptor whose PHP root
/// belonged to the previous object. The destructor is invoked explicitly to make
/// the teardown point deterministic while a second handle is opened in one fixture.
#[test]
fn test_pdo_persistent_sqlite_callbacks_are_cleared_before_pool_reuse() {
    let out = compile_and_run(
        r#"<?php
function install_persistent_callback(): void {
    $db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
    $db->sqliteCreateFunction("temporary_callback", function() { return 41; }, 0);
    $stmt = $db->query("SELECT temporary_callback() + 1");
    echo $stmt->fetchColumn() . ":";
    unset($stmt);
    $db->__destruct();
    unset($db);
}

install_persistent_callback();
$reused = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
try {
    $reused->query("SELECT temporary_callback()");
    echo "dangling";
} catch (PDOException $error) {
    echo "cleared";
}
"#,
    );
    assert_eq!(out, "42:cleared");
}

/// Verifies PHP 8.5's tightened fetch validation rejects class-only flags on
/// other modes and rejects FETCH_INTO from fetchAll(), while PHP 8.4 keeps its
/// historical acceptance in the existing default-version regressions.
#[test]
fn test_pdo_php85_fetch_flag_and_fetch_into_validation() {
    let out = compile_and_run_with_php_version(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1 AS value");
try {
    $stmt->setFetchMode(PDO::FETCH_ASSOC | PDO::FETCH_PROPS_LATE);
} catch (ValueError $error) {
    echo "set|";
}
try {
    $stmt->fetchAll(PDO::FETCH_INTO, new stdClass());
} catch (ValueError $error) {
    echo "all|";
}
try {
    $stmt->fetch(PDO::FETCH_NUM | PDO::FETCH_SERIALIZE);
} catch (ValueError $error) {
    echo "fetch";
}
"#,
        PhpVersion::Php85,
    );
    assert_eq!(out, "set|all|fetch");
}

/// The `Pdo\Mysql::ATTR_SSL_*` constants that drive MySQL TLS carry their PHP-8.4
/// (mysqlnd) values, and referencing them compiles.
#[test]
fn test_pdo_mysql_ssl_constants_present() {
    let out = compile_and_run(
        r#"<?php
echo Pdo\Mysql::ATTR_SSL_KEY . "," . Pdo\Mysql::ATTR_SSL_CERT . "," . Pdo\Mysql::ATTR_SSL_CA . "," . Pdo\Mysql::ATTR_SSL_CAPATH . "," . Pdo\Mysql::ATTR_SSL_CIPHER . "," . Pdo\Mysql::ATTR_SSL_VERIFY_SERVER_CERT;
"#,
    );
    assert_eq!(out, "1007,1008,1009,1010,1011,1014");
}

/// A MySQL SSL option in the constructor `$options` array is collected and packed
/// by `PDO::__construct`, then ignored by the bridge for a `sqlite:` DSN — the open
/// must still succeed (proving the packed-config building is inert, not fatal, for
/// the other drivers).
#[test]
fn test_pdo_mysql_ssl_options_inert_for_sqlite() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [Pdo\Mysql::ATTR_SSL_CA => "/nonexistent/ca.pem", Pdo\Mysql::ATTR_SSL_VERIFY_SERVER_CERT => false]);
echo $db->query("SELECT 42")->fetchColumn();
"#,
    );
    assert_eq!(out, "42");
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

/// P3: `setAttribute(ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_CLASS)` (and
/// `FETCH_INTO`) is ACCEPTED — mirroring php-src's `pdo_dbh.c` exactly
/// (verified against php-src): the "PDO::FETCH_INTO and PDO::FETCH_CLASS
/// cannot be set as the default fetch mode" rejection only fires when the
/// given value is an ARRAY whose element `[0]` is one of those modes (the
/// `setAttribute(ATTR_DEFAULT_FETCH_MODE, [PDO::FETCH_CLASS, 'Foo'])` idiom);
/// a BARE int is accepted and stored like any other mode. elephc's
/// `setAttribute()` only ever narrows its `mixed $value` with `(int) $value`,
/// so the array form never reaches this check and has no elephc analogue —
/// only `PDO::FETCH_USE_DEFAULT` (i.e. `PDO::FETCH_DEFAULT`, 0) is still
/// rejected. Supersedes the old `test_pdo_default_fetch_mode_rejects_class`,
/// which asserted the opposite (a real php-src divergence this slice fixes).
#[test]
fn test_pdo_default_fetch_mode_accepts_bare_class_and_into() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_CLASS);
echo $db->getAttribute(PDO::ATTR_DEFAULT_FETCH_MODE);
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_INTO);
echo ":" . $db->getAttribute(PDO::ATTR_DEFAULT_FETCH_MODE);
try {
    $db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_DEFAULT);
    echo ":no-throw";
} catch (\ValueError $e) {
    echo ":threw-default";
}
"#,
    );
    assert_eq!(out, "8:9:threw-default");
}

/// P3 regression: after accepting a bare `FETCH_CLASS` default (see above),
/// `PDO::prepare()` must still succeed — the connection-wide default is
/// propagated to the new statement via a raw, unvalidated field copy
/// (mirroring php-src's `stmt->default_fetch_type = dbh->default_fetch_type`)
/// rather than through `setFetchMode()`'s own argument-count validation,
/// which would otherwise wrongly reject this now-legal stored default the
/// moment ANY statement is prepared on the connection. With no class ever
/// registered, `fetch()` falls back to `stdClass` (its own pre-existing,
/// documented behavior for a target-less `FETCH_CLASS`).
#[test]
fn test_pdo_default_fetch_mode_bare_class_survives_prepare() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_CLASS);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$stmt = $db->prepare("SELECT id, name FROM t");
$stmt->execute();
$row = $stmt->fetch();
echo (($row instanceof stdClass) ? "stdClass" : "other") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "stdClass:1:Ada");
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

/// P3: the `FETCH_KEY_PAIR` wrong-column-count message matches php-src's exact
/// wording (verified against php-src's `pdo_stmt.c`: emitted via
/// `pdo_raise_impl_error(stmt->dbh, stmt, "HY000", ...)`), including the
/// trailing period — not elephc's previous, shorter invented text.
#[test]
fn test_pdo_fetch_key_pair_wrong_column_count_message_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b INTEGER, c INTEGER)");
$db->exec("INSERT INTO t VALUES (1, 2, 3)");
try {
    $db->query("SELECT a, b, c FROM t")->fetchAll(PDO::FETCH_KEY_PAIR);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns."
    );
}

/// P2-b: the `FETCH_KEY_PAIR` wrong-column-count error is errMode-aware, mirroring
/// php-src's `pdo_raise_impl_error` (SQLSTATE "HY000") rather than an
/// unconditional throw — under `ERRMODE_SILENT` a 3-column `fetch(FETCH_KEY_PAIR)`
/// returns `false` instead of throwing.
#[test]
fn test_pdo_fetch_key_pair_wrong_column_count_respects_silent() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$db->exec("CREATE TABLE t (a INTEGER, b INTEGER, c INTEGER)");
$db->exec("INSERT INTO t VALUES (1, 2, 3)");
$stmt = $db->query("SELECT a, b, c FROM t");
$row = $stmt->fetch(PDO::FETCH_KEY_PAIR);
echo ($row === false) ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// F-STMT-03: `fetchAll(PDO::FETCH_LAZY)` is the one place real PHP forbids
/// FETCH_LAZY, and it is a `ValueError` with php-src's verbatim message.
/// `pdo_stmt_verify_mode` takes a `fetch_all` flag and refuses FETCH_LAZY on that arm
/// ALONE — because a lazy PDORow is a view onto the CURRENT row, so a list of them
/// would all alias the last one.
///
/// This prelude used to have the restriction exactly BACKWARDS (it rejected LAZY in
/// `fetch()`, where php-src allows it, and accepted it here, where php-src does not),
/// and the old `test_pdo_fetch_lazy_unsupported_throws` locked the inversion in.
#[test]
fn test_pdo_fetch_all_lazy_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
try {
    $db->query("SELECT id FROM t")->fetchAll(PDO::FETCH_LAZY);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::fetchAll(): Argument #1 ($mode) cannot be PDO::FETCH_LAZY"
    );
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

/// `PDO::connect()` preserves late-static driver subclasses and rejects a
/// driver mismatch before attempting a connection, with php-src's exact guidance.
#[test]
fn test_pdo_connect_late_static_driver_compatibility() {
    let out = compile_and_run(
        r#"<?php
class AppSqlite extends \Pdo\Sqlite {}
$db = AppSqlite::connect("sqlite::memory:");
echo get_class($db), "|";
try {
    AppSqlite::connect("mysql:host=127.0.0.1");
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "AppSqlite|AppSqlite::connect() cannot be used for connecting to the \"mysql\" driver, either call Pdo\\Mysql::connect() or PDO::connect() instead"
    );
}

/// A generic PDO subclass is constructor-compatible for legacy code but cannot
/// select a driver-specific class through the PHP 8.4 static factory.
#[test]
fn test_pdo_connect_rejects_generic_pdo_subclass_and_unknown_driver_scope() {
    let out = compile_and_run(
        r#"<?php
class AppPdo extends PDO {}
try {
    AppPdo::connect("sqlite::memory:");
} catch (PDOException $e) {
    echo $e->getMessage(), "|";
}
try {
    \Pdo\Sqlite::connect("unknown:anything");
} catch (PDOException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "AppPdo::connect() cannot be used for connecting to the \"sqlite\" driver, either call Pdo\\Sqlite::connect() or PDO::connect() instead|Pdo\\Sqlite::connect() cannot be used for connecting to an unknown driver, call PDO::connect() instead"
    );
}

/// Driver subclasses declare their own PHP 8.4 constants (not just inherited base
/// PDO ones): SQLite DETERMINISTIC / ATTR_OPEN_FLAGS, MySQL ATTR_LOCAL_INFILE,
/// PostgreSQL ATTR_DISABLE_PREPARES / TRANSACTION_INERROR. Static constant access
/// errors on an undefined member at compile time, so a passing result proves each
/// namespaced subclass carries its own declared constants.
#[test]
fn test_pdo_driver_subclass_own_constants() {
    let out = compile_and_run(
        r#"<?php
echo \Pdo\Sqlite::DETERMINISTIC . ":" . \Pdo\Sqlite::ATTR_OPEN_FLAGS . ":"
    . \Pdo\Mysql::ATTR_LOCAL_INFILE . ":" . \Pdo\Pgsql::ATTR_DISABLE_PREPARES . ":"
    . \Pdo\Pgsql::TRANSACTION_INERROR;
"#,
    );
    assert_eq!(out, "2048:1000:1001:1000:3");
}

/// PDOStatement PHP 8.4 surface additions: the public `$queryString` property is
/// threaded from prepare(), and nextRowset() is false (P2-c: SQLite has no
/// further rowset, so this now raises IM001 under an exception-raising errMode
/// — see test_pdo_statement_nextrowset_raises_im001 for that case — and falls
/// quietly back to `false` here since the connection uses ERRMODE_SILENT).
/// P1-i/P3: no driver here registers a statement attribute hook, so both
/// setAttribute() and getAttribute() on an unsupported attribute now raise
/// IM001 instead of the old unconditional per-statement store — exercised here
/// under ERRMODE_SILENT so they fail quietly and getAttribute() falls back to
/// `false` (mirroring php-src's `RETURN_FALSE`, not `null`; see
/// test_pdo_statement_set_attribute_unsupported_throws for the
/// ERRMODE_EXCEPTION case).
#[test]
fn test_pdo_statement_querystring_attributes_nextrowset() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('Ada')");
$stmt = $db->prepare("SELECT name FROM t WHERE id = 1");
$qs = $stmt->queryString;
$set = $stmt->setAttribute(19, 5) ? "T" : "F";
$missing = $stmt->getAttribute(999) === false ? "false" : "?";
$stmt->execute();
$more = $stmt->nextRowset() ? "1" : "0";
echo $qs . "|" . $set . "|" . $missing . "|" . $more;
"#,
    );
    assert_eq!(out, "SELECT name FROM t WHERE id = 1|F|false|0");
}

/// P2-c/P3: `nextRowset()` raises IM001 ("driver does not support multiple
/// rowsets", php-src's exact wording) for a SQLite statement instead of
/// silently returning `false`, mirroring php-src's `pdo_raise_impl_error` for
/// a driver with no further-rowset primitive — errMode-aware like every other
/// statement failure: SILENT swallows it and still returns `false` (checked on
/// a separate connection above in
/// test_pdo_statement_querystring_attributes_nextrowset), EXCEPTION throws a
/// `PDOException` carrying the SQLSTATE.
#[test]
fn test_pdo_statement_nextrowset_raises_im001() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
$threw = "no";
try {
    $stmt->nextRowset();
} catch (PDOException $e) {
    $threw = $e->errorInfo[0];
}
echo $threw;
"#,
    );
    assert_eq!(out, "IM001");
}

/// P3: the `nextRowset()` IM001 message matches php-src's exact wording
/// (verified against php-src's `pdo_stmt.c`) — "driver does not support
/// multiple rowsets", not elephc's previous invented "This driver doesn't
/// support multiple rowsets" prefix.
#[test]
fn test_pdo_statement_nextrowset_message_matches_php_src() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->nextRowset();
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:SQLSTATE[IM001]: Driver does not support this function: driver does not support multiple rowsets"
    );
}

/// P1-j: a `PDOStatement` keeps its owning `PDO` (and therefore its bridge
/// connection) alive for as long as the statement itself is reachable, even
/// after the local variable holding the `PDO` goes out of scope. `PDO::query()`
/// (via `prepare()`) passes `$this` into the new statement's `setOwner()`,
/// stored in a private `?PDO $owner` property — a plain object-typed property
/// reference is enough for elephc's refcounting GC to keep the referenced
/// object (and, transitively, its bridge connection) alive. No reference cycle
/// is created: `PDO` does not hold a reference back to any of its statements.
///
/// `q()` is deliberately left without a declared return type: `PDO::query()`'s
/// real signature is `PDOStatement|bool`, and elephc's checker does not
/// flow-narrow that union down to `PDOStatement` even behind an `=== false`
/// guard, so a `: PDOStatement` return type on `q()` fails to type-check here
/// — an unrelated checker limitation, not part of what this test verifies.
#[test]
fn test_pdo_statement_keeps_connection_alive() {
    let out = compile_and_run(
        r#"<?php
function q() {
    $db = new PDO("sqlite::memory:");
    $db->exec("CREATE TABLE t(n)");
    $db->exec("INSERT INTO t VALUES(42)");
    return $db->query("SELECT n FROM t");
}
echo q()->fetchColumn();
"#,
    );
    assert_eq!(out, "42");
}

/// P2-o: constructing a `PDOStatement` directly (not via `PDO::prepare()` /
/// `query()`) throws a `PDOException` — mirroring php-src's "You should not
/// create a PDOStatement manually" — because the given `$connection` is not a
/// real, currently-open connection handle (`elephc_pdo_driver_name()` returns
/// `""` for an unknown id). This is the only way to reach the constructor
/// directly, since elephc never exposes a valid handle to PHP code.
#[test]
fn test_pdo_statement_direct_construction_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $_stmt = new PDOStatement(0, 0);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "threw:You should not create a PDOStatement manually");
}

/// P1-i: `PDOStatement::setAttribute()`/`getAttribute()` on an unsupported
/// attribute raise IM001 under `ERRMODE_EXCEPTION` (the connection default) —
/// mirroring php-src's `pdo_raise_impl_error(stmt->dbh, stmt, "IM001", ...)`,
/// since no driver here registers a statement attribute hook.
#[test]
fn test_pdo_statement_set_attribute_unsupported_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->setAttribute(12345, "x");
    echo "no-throw-set";
} catch (PDOException $e) {
    echo "threw-set:" . $e->errorInfo[0];
}
try {
    $stmt->getAttribute(12345);
    echo ":no-throw-get";
} catch (PDOException $e) {
    echo ":threw-get:" . $e->errorInfo[0];
}
"#,
    );
    assert_eq!(out, "threw-set:IM001:threw-get:IM001");
}

/// PDOStatement::getColumnMeta (P1-8): native_type is always the runtime
/// storage-class name (never the raw declared DDL text), the declared type
/// lives under the separate "sqlite:decl_type" key, and an out-of-range column
/// index returns false. Verified against a real PHP 8.5 CLI with the same
/// schema and fixture row.
#[test]
fn test_pdo_statement_get_column_meta() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)");
$db->exec("INSERT INTO t (id, name) VALUES (7, 'Zed')");
$stmt = $db->query("SELECT id, name FROM t");
$stmt->fetch();
$meta0 = $stmt->getColumnMeta(0);
$meta1 = $stmt->getColumnMeta(1);
$bad = $stmt->getColumnMeta(9) === false ? "F" : "?";
echo $meta0["name"] . ":" . $meta0["native_type"] . ":" . $meta0["sqlite:decl_type"] . ","
    . $meta0["table"] . ":" . $meta0["len"] . ":" . $meta0["precision"] . ","
    . $meta1["name"] . ":" . $meta1["native_type"] . ":" . $meta1["sqlite:decl_type"] . "," . $bad;
"#,
    );
    assert_eq!(out, "id:integer:INTEGER,t:-1:0,name:string:TEXT,F");
}

/// v43 REGRESSION GUARD: driver-specific MySQL and PostgreSQL metadata must not leak
/// into SQLite, while SQLite's common PDO descriptor fields and native source table
/// retain their php-src values.
///
/// The exact KEY COUNT is the load-bearing assertion: it is what proves no `pgsql:*` key
/// leaked into a SQLite column's metadata. A SQLite column carries exactly 8 keys (name,
/// native_type, pdo_type, len, precision, flags, table, sqlite:decl_type); the pg branch
/// would add `pgsql:oid` and `pgsql:table_oid` and drop `sqlite:decl_type`.
///
/// `len` and `precision` come from PDO's common column descriptor (`-1`/`0` here), while
/// `table` comes from SQLite's optional native column metadata API.
#[test]
fn test_pdo_get_column_meta_sqlite_shape_unchanged_by_the_mysql_and_pg_wiring() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, blob_col BLOB)");
$db->exec("INSERT INTO t (id, name, blob_col) VALUES (7, 'Zed', X'00FF')");
$stmt = $db->query("SELECT id, name, blob_col FROM t");
$stmt->fetch();

$m = $stmt->getColumnMeta(0);
// 8 keys exactly: no pgsql:oid / pgsql:table_oid leaked in from the pg branch.
// Counted with foreach, NOT count($m): getColumnMeta() is declared `array|bool` (php-src's
// `array|false`), and count() on that union is a compile-time error here ("count() argument
// must be array or Countable object") — the checker will not narrow it. Indexing the union
// is fine, which is why every other read below is a plain subscript.
$keys = 0;
foreach ($m as $k => $v) {
    $keys = $keys + 1;
}
echo $keys . ":" . $m["native_type"] . ":" . $m["pdo_type"]
    . ":" . $m["len"] . ":" . $m["precision"] . ":[" . $m["table"] . "]"
    . ":" . count($m["flags"]);

// A BLOB still reports native_type "string" with "blob" pushed into flags (never its own
// native_type/pdo_type) — pdo_sqlite's storage-class rule, untouched by the MySQL override.
$b = $stmt->getColumnMeta(2);
echo "|" . $b["native_type"] . ":" . $b["pdo_type"] . ":" . implode(",", $b["flags"]);
"#,
    );
    assert_eq!(out, "8:integer:1:-1:0:[t]:0|string:2:blob");
}

/// P2-h: `getColumnMeta()` on a prepared (not yet executed) statement returns
/// `false` — there is no result set at all to describe yet, distinct from the
/// existing out-of-range-column `false` case.
#[test]
fn test_pdo_get_column_meta_before_execute_returns_false() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$stmt = $db->prepare("SELECT id, name FROM t");
$before = $stmt->getColumnMeta(0) === false ? "F" : "?";
$stmt->execute();
$after = $stmt->getColumnMeta(0) === false ? "F" : "array";
echo $before . ":" . $after;
"#,
    );
    assert_eq!(out, "F:array");
}

/// P3: `getColumnMeta($column)` with a negative `$column` throws a `ValueError`
/// — mirroring php-src's exact message and ordering (verified against
/// php-src's `PHP_METHOD(PDOStatement, getColumnMeta)`: the negative check is
/// pure argument validation that runs BEFORE any executed-state or
/// driver-dispatch check) — distinct from the `false` returned for a merely
/// out-of-range-HIGH column index (`test_pdo_statement_get_column_meta`
/// above) or a not-yet-executed statement
/// (`test_pdo_get_column_meta_before_execute_returns_false` above). Exercised
/// on a statement that hasn't been executed yet, so a wrong check order (the
/// `!executed` guard running first) would return `false` instead of throwing.
#[test]
fn test_pdo_get_column_meta_negative_column_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->prepare("SELECT id FROM t");
try {
    $stmt->getColumnMeta(-1);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::getColumnMeta(): Argument #1 ($column) must be greater than or equal to 0"
    );
}

/// P1-4: `getColumnMeta()` called BEFORE the first explicit `fetch()` still
/// reports the real column types of the first row, not "no row yet" — elephc's
/// execute() eagerly pre-steps a SELECT-style statement, mirroring php-src's
/// pdo_sqlite `pre_fetched` behavior, and the subsequent explicit fetch() still
/// sees that same first row (verified against a real PHP 8.5 CLI with the same
/// schema and fixture rows).
#[test]
fn test_pdo_statement_get_column_meta_before_first_fetch() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT, val REAL)");
$db->exec("INSERT INTO t VALUES (1, 'a', 1.5)");
$db->exec("INSERT INTO t VALUES (2, 'b', 2.5)");
$stmt = $db->query("SELECT id, name, val FROM t ORDER BY id");
$m0 = $stmt->getColumnMeta(0);
$m1 = $stmt->getColumnMeta(1);
$m2 = $stmt->getColumnMeta(2);
$before = $m0["native_type"] . "," . $m1["native_type"] . "," . $m2["native_type"];
// The row that getColumnMeta() saw pre-fetched above must still be the first
// row an explicit fetch() returns — nothing was skipped.
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo $before . "|" . $row["id"] . ":" . $row["name"] . ":" . $row["val"];
"#,
    );
    assert_eq!(out, "integer,string,double|1:a:1.5");
}

/// P1-4: `getColumnMeta()` before the first fetch against an EMPTY result set
/// still reports "null" (there is no row, pre-fetched or otherwise, to derive a
/// real type from) — matching a real PHP 8.5 CLI.
#[test]
fn test_pdo_statement_get_column_meta_before_first_fetch_empty_result() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$stmt = $db->query("SELECT id FROM t");
$m0 = $stmt->getColumnMeta(0);
echo $m0["native_type"] . ":" . ($stmt->fetch() === false ? "false" : "other");
"#,
    );
    assert_eq!(out, "null:false");
}

/// PDOStatement::getColumnMeta (P1-8): a BLOB column reports native_type
/// "string" (not "blob"), pushes "blob" into flags, and reports pdo_type
/// PARAM_STR (2, not PARAM_LOB) — matching pdo_sqlite's sqlite_statement.c
/// exactly (verified against a real PHP 8.5 CLI).
#[test]
fn test_pdo_statement_get_column_meta_blob_triple() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (data BLOB)");
$db->exec("INSERT INTO t (data) VALUES (x'414243')");
$stmt = $db->query("SELECT data FROM t");
$stmt->fetch();
$meta = $stmt->getColumnMeta(0);
echo $meta["native_type"] . ":" . $meta["pdo_type"] . ":" . $meta["flags"][0] . ":" . $meta["sqlite:decl_type"];
"#,
    );
    assert_eq!(out, "string:2:blob:BLOB");
}

/// PDOStatement::getColumnMeta (P1-8): an expression column with no declared
/// type omits the "sqlite:decl_type" key entirely, matching PHP (verified
/// against a real PHP 8.5 CLI).
#[test]
fn test_pdo_statement_get_column_meta_expression_no_decltype() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1 + 1 AS expr");
$stmt->fetch();
$meta = $stmt->getColumnMeta(0);
echo $meta["native_type"] . ":" . (isset($meta["sqlite:decl_type"]) ? "Y" : "N")
    . ":" . (isset($meta["table"]) ? "Y" : "N") . ":" . $meta["len"];
"#,
    );
    assert_eq!(out, "integer:N:N:-1");
}

/// P2-16: `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` is a
/// live `sqlite3_stmt_readonly()` read — true for a SELECT, false for an INSERT
/// on the very same connection. Verified against a real PHP 8.5 CLI.
#[test]
fn test_pdo_sqlite_attr_readonly_statement_is_live() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");
$sel = $db->prepare("SELECT n FROM t");
$ins = $db->prepare("INSERT INTO t VALUES (1)");
$selRo = $sel->getAttribute(\Pdo\Sqlite::ATTR_READONLY_STATEMENT) ? "T" : "F";
$insRo = $ins->getAttribute(\Pdo\Sqlite::ATTR_READONLY_STATEMENT) ? "T" : "F";
echo $selRo . ":" . $insRo;
"#,
    );
    assert_eq!(out, "T:F");
}

/// `Pdo\Sqlite::loadExtension()` throws a PDOException when an extension cannot be
/// loaded (a nonexistent path here), exercising the method and its error path
/// without needing a real extension library.
#[test]
fn test_pdo_sqlite_load_extension_error() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
try {
    $db->loadExtension("/nonexistent/elephc_missing_ext.so");
    echo "no-throw";
} catch (\PDOException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Pdo\Sqlite::openBlob reads a BLOB cell through bounded incremental slices. The
/// fixture stores a 3-byte BLOB with an embedded NUL (`x'610062'` = "a\0b") directly
/// through SQL so the read path is exercised independently of parameter binding,
/// then asserts the streamed bytes match and opening a missing row returns false.
#[test]
fn test_pdo_sqlite_open_blob() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE imgs (id INTEGER PRIMARY KEY, body BLOB)");
$db->exec("INSERT INTO imgs (id, body) VALUES (1, x'610062')");
$s = $db->openBlob("imgs", "body", 1);
$content = stream_get_contents($s);
$ok = (strlen($content) === 3 && $content === ("a" . chr(0) . "b")) ? "ok" : "bad";
$missing = $db->openBlob("imgs", "body", 999);
echo $ok . ":" . (($missing === false) ? "false" : "leak");
"#,
    );
    assert_eq!(out, "ok:false");
}

/// `Pdo\Sqlite::openBlob()` exposes the native fixed-size stream semantics: the
/// default handle is read-only, OPEN_READWRITE permits in-place writes that are
/// immediately visible to SQL, seeking/stat report the BLOB cursor and size, and
/// an extending write fails without changing the cell.
#[test]
fn test_pdo_sqlite_open_blob_readwrite_seek_and_fixed_size() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE imgs (id INTEGER PRIMARY KEY, body BLOB)");
$db->exec("INSERT INTO imgs (id, body) VALUES (1, x'544553542054455354')");

$ro = $db->openBlob("imgs", "body", 1);
$readOnly = fwrite($ro, "X") === false;
fclose($ro);

$rw = $db->openBlob("imgs", "body", 1, "main", \Pdo\Sqlite::OPEN_READWRITE);
$written = fwrite($rw, "ABCD");
$tell = ftell($rw);
$seek = fseek($rw, 0);
$body = stream_get_contents($rw);
$size = fstat($rw)["size"];
$extend = fwrite($rw, "!") === false ? "fixed" : "bad";
$stored = $db->query("SELECT hex(body) FROM imgs")->fetchColumn();
fclose($rw);
echo ($readOnly ? "ro" : "bad") . ":" . $written . ":" . $tell . ":" . $seek . ":" . $body
    . ":" . $size . ":" . $extend . ":" . $stored;
"#,
    );
    assert_eq!(out, "ro:4:4:0:ABCD TEST:9:fixed:414243442054455354");
}

/// PDOStatement::debugDumpParams writes the SQL (with its byte length) and the
/// bound-parameter count to stdout.
#[test]
fn test_pdo_statement_debug_dump_params() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT 1");
$stmt->debugDumpParams();
"#,
    );
    assert_eq!(out, "SQL: [8] SELECT 1\nParams:  0\n");
}

/// F-STMT-12: a POSITIONAL bind emits php-src's `Key: Position #<paramno>:` block.
/// The expected bytes are derived from php-src's own format strings (pdo_stmt.c:
/// `"Key: Position #" ZEND_ULONG_FMT ":\n"` then `"paramno=" ZEND_LONG_FMT
/// "\nname=[%zd] \"%.*s\"\nis_param=%d\nparam_type=%d\n"`), NOT from elephc's output:
/// `paramno` is 0-based (php stores `paramno - 1`), a positional bind has an EMPTY name
/// (`name=[0] ""`), `is_param` is 1, and `param_type` echoes the caller's type verbatim
/// (PARAM_INT = 1). Note the two spaces after `Params:` — php-src's literal spacing.
#[test]
fn test_pdo_statement_debug_dump_params_positional_bind() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT ?");
$stmt->bindValue(1, 5, PDO::PARAM_INT);
$stmt->debugDumpParams();
"#,
    );
    assert_eq!(
        out,
        "SQL: [8] SELECT ?\nParams:  1\nKey: Position #0:\nparamno=0\nname=[0] \"\"\nis_param=1\nparam_type=1\n"
    );
}

/// F-STMT-12: a NAMED bind emits php-src's `Key: Name: [<bytes>] :name` block, and the
/// `name=` line repeats the placeholder QUOTED with its byte length. `:b` is 2 bytes, and
/// php-src leaves a named param's `paramno` at -1 until the first execute-time
/// normalization hook resolves it; this dump intentionally happens before execute().
#[test]
fn test_pdo_statement_debug_dump_params_named_bind() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT :b");
$stmt->bindValue(':b', 'x');
$stmt->debugDumpParams();
"#,
    );
    assert_eq!(
        out,
        "SQL: [9] SELECT :b\nParams:  1\nKey: Name: [2] :b\nparamno=-1\nname=[2] \":b\"\nis_param=1\nparam_type=2\n"
    );
}

/// Rebinding one PDO parameter replaces its visible debug entry, matching the
/// `bound_params` hash used by php-src while preserving last-value-wins execution.
#[test]
fn test_pdo_statement_debug_dump_params_rebind_replaces_entry() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT :v");
$stmt->bindValue("v", "first", PDO::PARAM_STR);
$stmt->bindValue(":v", 7, PDO::PARAM_INT);
$stmt->execute();
$stmt->debugDumpParams();
"#,
    );
    assert_eq!(
        out,
        "SQL: [9] SELECT :v\nParams:  1\nKey: Name: [2] :v\nparamno=0\nname=[2] \":v\"\nis_param=1\nparam_type=1\n"
    );
}

/// F-STMT-12: php-src stamps PDO_PARAM_STR (2) on EVERY element of an `execute($params)`
/// array, whatever the PHP value's type — an integer bound this way still dumps as
/// `param_type=2`. This pins the split between the type elephc DISPATCHES on (recorded
/// separately, 1 for an int) and the type php REPORTS here, so the internal dispatch tag
/// can never leak back into the dump.
#[test]
fn test_pdo_statement_debug_dump_params_execute_array_is_param_str() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT ?");
$stmt->execute([5]);
$stmt->debugDumpParams();
"#,
    );
    assert_eq!(
        out,
        "SQL: [8] SELECT ?\nParams:  1\nKey: Position #0:\nparamno=0\nname=[0] \"\"\nis_param=1\nparam_type=2\n"
    );
}

/// F-STMT-13: `$stmt->queryString` is readable but never overwritable — php-src guards it
/// with a custom property-write handler (`dbstmt_prop_write`), so an assignment is a
/// catchable Error rather than a silent overwrite of the SQL the statement reports. elephc
/// declares it `readonly` (assigned once in the constructor) to get there.
///
/// This pins both the concrete and `PDOStatement|bool` receiver shapes. Both raise a
/// catchable Error; the text is PHP's generic readonly message rather than pdo_stmt.c's
/// custom wording, but the exception class and write rejection match.
///
/// In both shapes the SQL is protected, which is the point of the finding. The test also
/// proves `readonly` does not break the constructor's OWN write — a regression there would
/// break EVERY PDOStatement construction, not just this assignment.
#[test]
fn test_pdo_statement_query_string_is_readonly() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT 1");
echo $stmt->queryString;
// Union receiver (PDOStatement|bool): readonly validation still applies.
try {
    $stmt->queryString = "DROP TABLE t";
    echo "|union:no-error";
} catch (Error $e) {
    echo "|union:" . $e->getMessage();
}
echo "|" . $stmt->queryString;
// Narrowed receiver: catchable Error, value preserved.
if ($stmt instanceof PDOStatement) {
    try {
        $stmt->queryString = "DROP TABLE t";
        echo "|narrowed:no-error";
    } catch (Error $e) {
        echo "|narrowed:" . $e->getMessage();
    }
    echo "|" . $stmt->queryString;
}
"#,
    );
    assert_eq!(
        out,
        "SELECT 1|union:Cannot modify readonly property PDOStatement::$queryString|SELECT 1\
         |narrowed:Cannot modify readonly property PDOStatement::$queryString|SELECT 1"
    );
}

/// F-STMT-17: setFetchMode(FETCH_COLUMN, <non-int>) raises a TypeError BEFORE the `< 0`
/// range check, mirroring php-src (pdo_stmt.c:1767-70). The check is STRICT — zend never
/// juggles this variadic argument, so a numeric string throws just like an array does.
///
/// The message carries NO argument name: zend cannot name a variadic parameter, so php
/// prints `Argument #2 must be of type int, string given`, not `Argument #2 ($args)`.
#[test]
fn test_pdo_statement_set_fetch_mode_column_rejects_non_int() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT 1");
try {
    $stmt->setFetchMode(PDO::FETCH_COLUMN, "abc");
    echo "no-error";
} catch (TypeError $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "PDOStatement::setFetchMode(): Argument #2 must be of type int, string given"
    );
}

/// Verifies PDOException::getCode() reports the SQLSTATE string while errorInfo retains
/// the native driver code, matching php-src's PDO-specific exception initialization.
#[test]
fn test_pdo_exception_get_code_is_sqlstate() {
    let out = compile_and_run(
        r#"<?php
$db = new \PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT UNIQUE)");
$db->exec("INSERT INTO t (v) VALUES ('a')");
try {
    $db->exec("INSERT INTO t (v) VALUES ('a')");
    echo "no-error";
} catch (PDOException $e) {
    echo "code=" . $e->getCode();
    echo "|sqlstate=" . $e->errorInfo[0];
    echo "|native=" . $e->errorInfo[1];
    echo "|prev=" . ($e->previous === null ? "null" : "set");
}
"#,
    );
    assert_eq!(out, "code=23000|sqlstate=23000|native=19|prev=null");
}

/// Verifies PDOException preserves and returns its previous Throwable chain entry.
#[test]
fn test_pdo_exception_get_previous_returns_stored_throwable() {
    let out = compile_and_run(
        r#"<?php
$previous = new Exception("root");
$error = new PDOException("outer", 17, $previous);
$actual = $error->getPrevious();
echo ($actual === $previous ? "same" : "different") . "|" . $error->getCode();
"#,
    );
    assert_eq!(out, "same|17");
}

/// Pdo\Pgsql::escapeIdentifier is a pure string transform (PQescapeIdentifier
/// semantics: double interior double-quotes, wrap in double-quotes) that touches no
/// connection, so it is exercised via a non-connecting Pdo\Pgsql subclass — proving
/// both the transform and dispatch of an own method declared on a namespaced
/// subclass, without a live PostgreSQL server.
#[test]
fn test_pdo_pgsql_escape_identifier() {
    let out = compile_and_run(
        r#"<?php
class FakePg extends \Pdo\Pgsql {
    // Bypass the connecting parent constructor AND destructor so the pure,
    // connection-independent escapeIdentifier() can be exercised without a live
    // server: the inherited PDO::__destruct reads $this->inTxn/$this->conn, which
    // the empty constructor never initialized.
    public function __construct() {}
    public function __destruct() {}
}
$pg = new FakePg();
echo $pg->escapeIdentifier('my"col') . "|" . $pg->escapeIdentifier('plain');
"#,
    );
    assert_eq!(out, "\"my\"\"col\"|\"plain\"");
}

/// Tier-D `Pdo\Sqlite::createCollation`: a compiled-PHP closure comparator drives a
/// custom `COLLATE` ordering. Here the comparator reverses the natural string order,
/// so `ORDER BY name COLLATE REV` returns the rows descending — proving the whole
/// path: the callable is decomposed into (descriptor, adapter) pointers, registered
/// via SQLite `pApp`, and re-entered by `__rt_pdo_call_collation` for each comparison.
#[test]
fn test_pdo_sqlite_create_collation_reverse_order() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createCollation("REV", function($a, $b) {
    return strcmp($b, $a);
});
$db->exec("CREATE TABLE t (name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('banana'), ('apple'), ('cherry')");
$rows = $db->query("SELECT name FROM t ORDER BY name COLLATE REV")->fetchAll(PDO::FETCH_NUM);
$out = "";
foreach ($rows as $r) { $out .= $r[0] . ","; }
echo $out;
"#,
    );
    assert_eq!(out, "cherry,banana,apple,");
}

/// Tier-D `Pdo\Sqlite::createCollation`: TWO collations registered on one connection
/// coexist and each keeps its own comparator. This is the direct disproof of "problem
/// C" (the old single process-global callback slot, last-write-wins): if both
/// registrations shared one slot, the reverse query would sort ascending (the
/// last-registered comparator). Each registration threads its own descriptor through
/// SQLite `pApp`, so `REV` stays descending while `NAT` sorts ascending.
#[test]
fn test_pdo_sqlite_create_collation_two_coexist() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createCollation("REV", function($a, $b) {
    return strcmp($b, $a);
});
$db->createCollation("NAT", function($a, $b) {
    return strcmp($a, $b);
});
$db->exec("CREATE TABLE t (name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('banana'), ('apple'), ('cherry')");
$rev = "";
foreach ($db->query("SELECT name FROM t ORDER BY name COLLATE REV")->fetchAll(PDO::FETCH_NUM) as $r) {
    $rev .= $r[0] . ",";
}
$nat = "";
foreach ($db->query("SELECT name FROM t ORDER BY name COLLATE NAT")->fetchAll(PDO::FETCH_NUM) as $r) {
    $nat .= $r[0] . ",";
}
echo $rev . "|" . $nat;
"#,
    );
    assert_eq!(out, "cherry,banana,apple,|apple,banana,cherry,");
}

/// Tier-D exception firewall: a collation comparator that `throw`s must not unwind
/// past the C boundary (SQLite's VDBE + the Rust bridge frame). The adapter's
/// `setjmp` firewall catches the `throw`, treats the comparison as equal (SQLite's
/// `xCompare` has no error channel), and lets the query complete. The program must
/// finish (no deadlock/hang/crash from a `longjmp` over C frames) and the connection
/// must remain usable — a following query still returns the correct count.
#[test]
fn test_pdo_sqlite_create_collation_throwing_comparator_does_not_hang() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createCollation("BOOM", function($a, $b) {
    throw new Exception("boom");
});
$db->exec("CREATE TABLE t (name TEXT)");
$db->exec("INSERT INTO t (name) VALUES ('x'), ('y'), ('z')");
$rows = $db->query("SELECT name FROM t ORDER BY name COLLATE BOOM")->fetchAll(PDO::FETCH_NUM);
$count = $db->query("SELECT COUNT(*) FROM t")->fetchColumn();
echo count($rows) . ":" . $count;
"#,
    );
    assert_eq!(out, "3:3");
}

/// Tier-D `Pdo\Sqlite::createFunction`: a compiled-PHP closure drives a scalar SQL
/// function. Integer arguments box as Mixed ints, cross into the callable, and the
/// integer return is decoded back through `__rt_pdo_call_scalar` into
/// `sqlite3_result_int64` — proving the whole path (decompose → pApp → adapter → box
/// args → invoke → decode int return).
#[test]
fn test_pdo_sqlite_create_function_int_args() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("myadd", function($a, $b) {
    return $a + $b;
}, 2);
echo $db->query("SELECT myadd(3, 4)")->fetchColumn();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies PHP 8.4's legacy pdo_sqlite methods remain installed directly on
/// `PDO`, including callback rooting for scalar, aggregate, and collation hooks.
#[test]
fn test_pdo_sqlite_legacy_driver_extension_methods() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->sqliteCreateFunction("twice", function($value) { return $value * 2; }, 1);
$db->sqliteCreateAggregate(
    "mysum",
    function($context, $row, $value) { return $context === null ? $value : $context + $value; },
    function($context, $row) { return $context; },
    1
);
$db->sqliteCreateCollation("REVERSE", function($left, $right) { return strcmp($right, $left); });
$db->exec("CREATE TABLE t(v TEXT)");
$db->exec("INSERT INTO t VALUES ('a'), ('c'), ('b')");
echo $db->query("SELECT twice(4)")->fetchColumn(), "|";
echo $db->query("SELECT mysum(length(v)) FROM t")->fetchColumn(), "|";
echo $db->query("SELECT v FROM t ORDER BY v COLLATE REVERSE LIMIT 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "8|3|c");
}

/// Tier-D `Pdo\Sqlite::createFunction`: a TEXT argument round-trips byte-exactly (the
/// adapter deep-copies SQLite's transient buffer while boxing tag-1 strings), and a
/// string return is staged through `elephc_pdo_udf_stash_bytes` and handed back to
/// SQLite with `sqlite3_result_text` — proving the string arg + string result path.
#[test]
fn test_pdo_sqlite_create_function_string_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("myecho", function($s) {
    return $s . "!";
}, 1);
echo $db->query("SELECT myecho('hi')")->fetchColumn();
"#,
    );
    assert_eq!(out, "hi!");
}

/// Tier-D `Pdo\Sqlite::createFunction`: a zero-argument function exercises the empty
/// (header-only) args-array path, and a float return proves the f64 bit-pattern
/// survives the box/unbox round-trip (carried in the integer lo register, written to
/// `ElephcResult.f`, dispatched to `sqlite3_result_double`).
#[test]
fn test_pdo_sqlite_create_function_float_and_zero_args() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("myval", function() {
    return 2.5;
}, 0);
echo $db->query("SELECT myval()")->fetchColumn();
"#,
    );
    assert_eq!(out, "2.5");
}

/// Tier-D `Pdo\Sqlite::createFunction`: TWO scalar functions on one connection coexist,
/// each keeping its own callable. This is the direct disproof of "problem C" (a single
/// process-global callback slot, last-write-wins) for scalar functions: if both shared
/// one slot, `f1()` would return 22 (the last-registered body). Each registration
/// threads its own descriptor through SQLite `pApp`, so `f1` stays 11 and `f2` is 22.
#[test]
fn test_pdo_sqlite_create_function_two_coexist() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("f1", function() { return 11; }, 0);
$db->createFunction("f2", function() { return 22; }, 0);
echo $db->query("SELECT f1()")->fetchColumn() . "," . $db->query("SELECT f2()")->fetchColumn();
"#,
    );
    assert_eq!(out, "11,22");
}

/// SQLite callbacks may register another callback and prepare/step a nested
/// statement on the same connection without deadlocking bridge handle tables.
#[test]
fn test_pdo_sqlite_callback_allows_nested_query_and_registration() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("outer_fn", function($value) use ($db) {
    $db->createFunction("inner_fn", function() { return 40; }, 0);
    $nested = $db->query("SELECT inner_fn()");
    return $nested->fetchColumn() + $value;
}, 1);
echo $db->query("SELECT outer_fn(2)")->fetchColumn(), "|";
echo $db->query("SELECT inner_fn()")->fetchColumn();
"#,
    );
    assert_eq!(out, "42|40");
}

/// Tier-D `Pdo\Sqlite::createFunction`: SQLite passes a different storage class per row,
/// so one registration must re-box each argument by its per-row type. An identity
/// function over a column holding an INTEGER then a TEXT value must return each with its
/// original type preserved (int→`result_int64`, text→`result_text`), yielding `5,str,`.
#[test]
fn test_pdo_sqlite_create_function_per_row_dynamic_typing() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("ident", function($x) { return $x; }, 1);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (5), ('str')");
$out = "";
foreach ($db->query("SELECT ident(v) FROM t ORDER BY rowid")->fetchAll(PDO::FETCH_NUM) as $r) {
    $out .= $r[0] . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "5,str,");
}

/// Tier-D `Pdo\Sqlite::createFunction`: a SQL NULL argument boxes as PHP null (Mixed
/// tag 8), and a PHP null return decodes to `sqlite3_result_null`. Verified in SQL
/// (`ident(v) IS NULL`) so the round-trip is checked without a PHP-side null compare:
/// the null survives the box → invoke → decode path, so the count is 1.
#[test]
fn test_pdo_sqlite_create_function_null_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("ident", function($x) { return $x; }, 1);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (NULL)");
echo $db->query("SELECT COUNT(*) FROM t WHERE ident(v) IS NULL")->fetchColumn();
"#,
    );
    assert_eq!(out, "1");
}

/// Tier-D exception firewall (scalar path): a user function that `throw`s must not
/// unwind past the C boundary (SQLite's VDBE + the Rust bridge frame). The adapter's
/// `setjmp` firewall catches the `throw` and reports `ElephcResult.tag = -1`, which the
/// bridge turns into a `sqlite3_result_error`; the statement fails but the program must
/// finish (no deadlock/hang/crash from a `longjmp` over C frames) and the connection
/// must remain usable — a following query still evaluates correctly.
#[test]
fn test_pdo_sqlite_create_function_throwing_does_not_hang() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("boom", function() {
    throw new Exception("bang");
}, 0);
try {
    $db->exec("SELECT boom()");
} catch (\Exception $e) {
}
echo "done:" . $db->query("SELECT 1 + 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "done:2");
}

/// Tier-D `Pdo\Sqlite::createAggregate`: an integer-accumulating aggregate (sum). The
/// step callback receives the running accumulator + row number + row value and returns
/// the new accumulator (row-number-seeded on the first row, so no null arithmetic); the
/// finalize callback returns it. Proves the whole aggregate path: per-group
/// accumulator threaded through `sqlite3_aggregate_context`, `__rt_pdo_call_agg_step`
/// per row, `__rt_pdo_call_agg_final` once, and correct row-number threading (PHP
/// bug-for-bug parity: the shared row counter is pre-incremented, so `$rownumber` is
/// `1` on the first step, per `sqlite_driver.c`'s `++agg_context->row`).
#[test]
fn test_pdo_sqlite_create_aggregate_sum() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("mysum",
    function($ctx, $row, $v) { if ($row == 1) { return $v; } return $ctx + $v; },
    function($ctx, $row) { return $ctx; }
);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (1), (2), (3), (4)");
echo $db->query("SELECT mysum(v) FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "10");
}

/// Tier-D `Pdo\Sqlite::createAggregate`: a STRING-accumulating aggregate (concat). This
/// is the refcount-critical test — the accumulator is a boxed-Mixed PHP string that
/// must survive being stored in the group slot and passed back into the next step
/// across every row (incref-new-before-release-old). A refcount error frees it early
/// (corrupt output) or leaks it. Correct output proves the ownership protocol.
#[test]
fn test_pdo_sqlite_create_aggregate_string_concat() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("myconcat",
    function($ctx, $row, $v) { if ($row == 1) { return $v; } return $ctx . $v; },
    function($ctx, $row) { return $ctx; }
);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES ('a'), ('b'), ('c'), ('d')");
echo $db->query("SELECT myconcat(v) FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "abcd");
}

/// Tier-D `Pdo\Sqlite::createAggregate`: `GROUP BY` gives each group its own
/// accumulator. SQLite allocates a distinct `sqlite3_aggregate_context` per group, so
/// the two groups accumulate independently — the direct proof that the per-group slot
/// (not a single shared accumulator) carries the state. Expects `a=3,b=30,`.
#[test]
fn test_pdo_sqlite_create_aggregate_group_by() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("mysum",
    function($ctx, $row, $v) { if ($row == 1) { return $v; } return $ctx + $v; },
    function($ctx, $row) { return $ctx; }
);
$db->exec("CREATE TABLE t (g, v)");
$db->exec("INSERT INTO t (g, v) VALUES ('a', 1), ('a', 2), ('b', 10), ('b', 20)");
$out = "";
foreach ($db->query("SELECT g, mysum(v) FROM t GROUP BY g ORDER BY g")->fetchAll(PDO::FETCH_NUM) as $r) {
    $out .= $r[0] . "=" . $r[1] . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "a=3,b=30,");
}

/// Tier-D `Pdo\Sqlite::createAggregate`: an empty group calls finalize exactly once
/// with NO prior step (`sqlite3_aggregate_context(ctx, 0)` returns NULL → a null
/// accumulator and a shared row counter that starts at 0). PHP's finalize call
/// pre-increments that same shared counter (`++agg_context->row`) even though `xStep`
/// never ran, so the finalize here — which returns the row number — yields `1`, not
/// `0`, proving the empty-group path and the bug-for-bug row-count threading.
#[test]
fn test_pdo_sqlite_create_aggregate_empty_group() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("mycount",
    function($ctx, $row, $v) { return $ctx; },
    function($ctx, $row) { return $row; }
);
$db->exec("CREATE TABLE t (v)");
echo $db->query("SELECT mycount(v) FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "1");
}

/// Tier-D `Pdo\Sqlite::createAggregate` — VALUE-PIN the `$rownumber` sequence
/// (v1 §6 open gap). The other aggregate tests only branch on `$row == 1` to
/// detect the first step; none captures the actual value `$row` takes each call.
/// Here the accumulator records every `$row` it is handed, so the output is the
/// literal sequence: SQLite pre-increments the shared `agg_context->row` at the
/// START of each `xStep` AND once more at `xFinal` (mirroring php-src's
/// `++agg_context->row`), so four rows yield step values 1,2,3,4 and finalize
/// sees 5 — never 0-based, and finalize is one past the last step. This is the
/// exact bug-for-bug row threading `test_..._empty_group` pins for the 0-step
/// case, extended to the multi-step case.
#[test]
fn test_pdo_sqlite_create_aggregate_rownumber_sequence() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("rowseq",
    function($ctx, $row, $v) { if ($row == 1) { return "" . $row; } return $ctx . "-" . $row; },
    function($ctx, $row) { return $ctx . "|" . $row; }
);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (10), (20), (30), (40)");
echo $db->query("SELECT rowseq(v) FROM t")->fetchColumn();
"#,
    );
    assert_eq!(out, "1-2-3-4|5");
}

/// Tier-D exception firewall (aggregate step): a step callback that `throw`s must not
/// unwind across SQLite's VDBE + the Rust bridge frame. The step adapter's firewall
/// catches the longjmp, preserves the accumulator (so finalize still frees it), and
/// signals the throw; the bridge raises a SQL error. The program must finish and the
/// connection stay usable.
#[test]
fn test_pdo_sqlite_create_aggregate_throwing_step_does_not_hang() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("boom",
    function($ctx, $row, $v) { throw new Exception("step boom"); },
    function($ctx, $row) { return $ctx; }
);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (1), (2)");
try {
    $db->exec("SELECT boom(v) FROM t");
} catch (\Exception $e) {
}
echo "done:" . $db->query("SELECT 1 + 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "done:2");
}

/// Tier-D exception firewall (aggregate finalize): a finalize callback that `throw`s is
/// caught by the finalize adapter's firewall, reported as an error result, and — since
/// finalize is terminal — the accumulator is still freed (no leak/dangling before
/// SQLite frees the group block). The program must finish and the connection stay
/// usable.
#[test]
fn test_pdo_sqlite_create_aggregate_throwing_final_does_not_hang() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createAggregate("boomf",
    function($ctx, $row, $v) { if ($row == 1) { return $v; } return $ctx + $v; },
    function($ctx, $row) { throw new Exception("final boom"); }
);
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (1), (2)");
try {
    $db->exec("SELECT boomf(v) FROM t");
} catch (\Exception $e) {
}
echo "done:" . $db->query("SELECT 1 + 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "done:2");
}

/// P0-5: `prepare($sql, $options)`'s two-arg form compiles and runs; `$options` is
/// stored into the statement's attribute map with no behavioral effect, so the
/// prepared statement still binds and fetches normally.
#[test]
fn test_pdo_prepare_with_options() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$stmt = $db->prepare("SELECT name FROM t WHERE id = ?", []);
$stmt->execute([1]);
echo $stmt->fetch(PDO::FETCH_ASSOC)["name"];
"#,
    );
    assert_eq!(out, "Ada");
}

/// P0-6: `query($sql, PDO::FETCH_ASSOC)` applies the fetch mode to the returned
/// statement via `setFetchMode()`, so a mode-less `fetch()` on it returns assoc rows.
#[test]
fn test_pdo_query_with_fetch_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t", PDO::FETCH_ASSOC)->fetch();
echo $row["id"] . ":" . $row["name"] . "|" . (isset($row[0]) ? "both" : "assoc");
"#,
    );
    assert_eq!(out, "1:Ada|assoc");
}

/// F-STMT-01: the 3-arg form is php-src's real one — `fetch(int $mode, int
/// $cursorOrientation, int $cursorOffset)` — and `fetch(FETCH_ASSOC,
/// PDO::FETCH_ORI_NEXT, 0)` behaves exactly like the 1-arg `fetch(FETCH_ASSOC)`.
/// (This test used to pass `null` into position 2 under the fabricated
/// `$classOrObject` signature, which is a TypeError against real PDO.)
///
/// Both trailing arguments are accepted and inert: every driver here opens a
/// FORWARD-ONLY cursor (`PDO::CURSOR_FWDONLY`; `ATTR_CURSOR` is inert), and php-src
/// likewise ignores the orientation on one. On a `CURSOR_SCROLL` statement real PHP
/// WOULD honor `FETCH_ORI_FIRST`/`LAST`/`PRIOR`/`ABS`/`REL` and seek — that divergence
/// is a property of the cursor, not of this signature.
#[test]
fn test_pdo_fetch_three_arg_form() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$row = $stmt->fetch(PDO::FETCH_ASSOC, PDO::FETCH_ORI_NEXT, 0);
echo $row["name"];

// Identical to the 1-arg form: the orientation does not consume or skip a row, so
// the SECOND fetch still lands on the next row rather than re-reading the first.
$next = $stmt->fetch(PDO::FETCH_ASSOC);
echo "|" . $next["name"];
"#,
    );
    assert_eq!(out, "Ada|Bob");
}

/// P1-6: the common legacy `bindParam($p, $v, PDO::PARAM_STR, $maxLength[, $driverOptions])`
/// 4- and 5-arg idioms compile and run; the extra length/driver-option hints are
/// accepted and ignored.
#[test]
fn test_pdo_bind_param_extended_arity() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$ins = $db->prepare("INSERT INTO t (id, name) VALUES (?, ?)");
$id = 1;
$name = "Ada";
$ins->bindParam(1, $id, PDO::PARAM_INT, 0);
$ins->bindParam(2, $name, PDO::PARAM_STR, 4000, null);
$ins->execute();
echo $db->query("SELECT name FROM t WHERE id = 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies `fetchAll(PDO::FETCH_CLASS, Row::class, [...])` forwards the complete
/// constructor-argument array through dynamic construction for every fetched row.
#[test]
fn test_pdo_fetch_all_class_with_ctor_args() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
    public string $prefix;
    public function __construct(string $prefix) { $this->prefix = $prefix; }
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");
$rows = $db->query("SELECT id, name FROM t ORDER BY id")->fetchAll(PDO::FETCH_CLASS, Row::class, ["row-"]);
echo count($rows) . ":" . $rows[0]->prefix . $rows[0]->name . ":" . $rows[1]->prefix . $rows[1]->name;
"#,
    );
    assert_eq!(out, "2:row-Ada:row-Bob");
}

/// Verifies `fetchObject()` forwards arbitrary constructor arguments instead of
/// silently constructing the dynamic class with an empty argument list.
#[test]
fn test_pdo_fetch_object_forwards_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class FetchObjectCtorRow {
    public mixed $name;
    public string $label;
    public function __construct(string $prefix, int $id) {
        $this->label = $prefix . $id;
    }
}
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 'Ada' AS name");
$row = $stmt->fetchObject(FetchObjectCtorRow::class, ["row-", 7]);
echo $row->label . ":" . $row->name;
"#,
    );
    assert_eq!(out, "row-7:Ada");
}

/// Verifies default `FETCH_CLASS` hydrates properties before construction while
/// `FETCH_PROPS_LATE` deliberately runs the constructor first.
#[test]
fn test_pdo_fetch_class_hydration_order_matches_props_late_flag() {
    let out = compile_and_run(
        r#"<?php
class HydrationOrderRow {
    public mixed $name = "initial";
    public string $seen;
    public function __construct(string $prefix) {
        $this->seen = $prefix . $this->name;
        $this->name = "constructor";
    }
}
$db = new PDO("sqlite::memory:");
$early = $db->query("SELECT 'Ada' AS name")->fetchAll(PDO::FETCH_CLASS, HydrationOrderRow::class, ["default:"])[0];
$late = $db->query("SELECT 'Ada' AS name")->fetchAll(PDO::FETCH_CLASS | PDO::FETCH_PROPS_LATE, HydrationOrderRow::class, ["late:"])[0];
echo $early->seen . "/" . $early->name . "|" . $late->seen . "/" . $late->name;
"#,
    );
    assert_eq!(out, "default:Ada/constructor|late:initial/Ada");
}

/// Verifies `PDO::query()` forwards its complete variadic fetch-mode tail, including
/// a heterogeneous constructor-argument array, into the statement fetch configuration.
#[test]
fn test_pdo_query_forwards_variadic_fetch_mode_arguments() {
    let out = compile_and_run(
        r#"<?php
class QueryCtorRow {
    public mixed $name;
    public string $label;
    public function __construct(string $prefix, int $number) {
        $this->label = $prefix . $number . ":" . $this->name;
    }
}
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 'Ada' AS name", PDO::FETCH_CLASS, QueryCtorRow::class, ["row-", 9]);
$row = $stmt->fetch();
echo $row->label;
"#,
    );
    assert_eq!(out, "row-9:Ada");
}

/// Verifies `getIterator()` returns the adapter used by PDOStatement's IteratorAggregate contract.
#[test]
fn test_pdo_statement_get_iterator() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
$out = "";
foreach ($stmt->getIterator() as $row) {
    $out .= $row["name"] . ",";
}
echo $out;
"#,
    );
    assert_eq!(out, "Ada,Bob,");
}

/// Verifies PDOStatement exposes PHP 8.4's IteratorAggregate relationship, not Iterator.
#[test]
fn test_pdo_statement_iterator_aggregate_relationship() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1");
echo ($stmt instanceof IteratorAggregate ? "aggregate" : "no"), "|";
echo ($stmt instanceof Iterator ? "iterator" : "no");
"#,
    );
    assert_eq!(out, "aggregate|no");
}

/// P1-13: `Pdo\Sqlite::createFunction`'s parameters were renamed to match the PHP
/// stub (`$function_name`, `$num_args`), so a PHP-valid named-argument call now
/// resolves (it previously failed against `$name`/`$numArgs`).
#[test]
fn test_pdo_sqlite_create_function_named_args() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction(function_name: "myadd", callback: function ($a, $b) {
    return $a + $b;
}, num_args: 2);
echo $db->query("SELECT myadd(3, 4)")->fetchColumn();
"#,
    );
    assert_eq!(out, "7");
}

/// P0-2: `FETCH_NAMED` groups duplicate-named columns into a numerically-indexed
/// array under that one key instead of the last value silently overwriting the
/// first (verified against real PHP: `SELECT 1 a, 2 a` => `["a" => [1, 2]]`, no
/// numeric keys injected). A uniquely-named column stays a plain scalar.
#[test]
fn test_pdo_fetch_named_groups_duplicate_columns() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$row = $db->query("SELECT 1 a, 2 a")->fetch(PDO::FETCH_NAMED);
echo count($row) . ":" . implode(",", $row["a"]) . ";";
$row2 = $db->query("SELECT 1 a, 2 b")->fetch(PDO::FETCH_NAMED);
echo count($row2) . ":" . $row2["a"] . ":" . $row2["b"];
"#,
    );
    assert_eq!(out, "1:1,2;2:1:2");
}

/// P0-2: a third occurrence of the same column name keeps appending to the
/// group instead of only ever holding two entries.
#[test]
fn test_pdo_fetch_named_groups_three_duplicates() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$row = $db->query("SELECT 1 a, 2 a, 3 a")->fetch(PDO::FETCH_NAMED);
echo count($row) . ":" . implode(",", $row["a"]);
"#,
    );
    assert_eq!(out, "1:1,2,3");
}

/// P0-3/P2: `fetch(PDO::FETCH_FUNC)` is rejected — real PHP restricts
/// FETCH_FUNC to `fetchAll()` and this prelude fails the same way (as a
/// `ValueError`, matching php-src's `zend_value_error("Can only use
/// PDO::FETCH_FUNC in PDOStatement::fetchAll()")`) instead of returning the
/// silent BOTH-shaped fallthrough.
#[test]
fn test_pdo_fetch_func_on_fetch_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->fetch(PDO::FETCH_FUNC);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()"
    );
}

/// Verifies `FETCH_FUNC` invokes the callback once per row with positional column arguments.
#[test]
fn test_pdo_fetch_func_on_fetch_all_invokes_callback() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'x'), (2, 'y')");
$_rows = $db->query("SELECT a, b FROM t")->fetchAll(PDO::FETCH_FUNC, function ($a, $b) {
    return $a . ":" . $b;
});
foreach ($_rows as $_row) {
    echo $_row, ";";
}
"#,
    );
    assert_eq!(out, "1:x;2:y;");
}

/// Verifies `FETCH_FUNC` resolves a function name carried through its boxed Mixed callback slot.
#[test]
fn test_pdo_fetch_func_accepts_string_callable() {
    let out = compile_and_run(
        r#"<?php
function pdo_fetch_func_label($value) {
    return "value=" . $value;
}

$db = new PDO("sqlite::memory:");
$rows = $db->query("SELECT 'x' UNION ALL SELECT 'yz'")
    ->fetchAll(PDO::FETCH_FUNC, "pdo_fetch_func_label");
foreach ($rows as $row) {
    echo $row . ";";
}
"#,
    );
    assert_eq!(out, "value=x;value=yz;");
}

/// Verifies `FETCH_FUNC` accepts both instance and static callable-array forms in Mixed.
#[test]
fn test_pdo_fetch_func_accepts_callable_arrays() {
    let out = compile_and_run(
        r#"<?php
class PdoFetchFuncFormatter {
    public function instanceLabel(string $value): string {
        return "instance=" . $value;
    }

    public static function staticLabel(string $value): string {
        return "static=" . $value;
    }
}

$db = new PDO("sqlite::memory:");
$formatter = new PdoFetchFuncFormatter();
$instanceCallback = [$formatter, "instanceLabel"];
$staticCallback = [PdoFetchFuncFormatter::class, "staticLabel"];
$instanceRows = $db->query("SELECT 'a'")
    ->fetchAll(PDO::FETCH_FUNC, $instanceCallback);
$staticRows = $db->query("SELECT 'b'")
    ->fetchAll(PDO::FETCH_FUNC, $staticCallback);
echo $instanceRows[0] . "|" . $staticRows[0]
    . "|" . $instanceCallback[1] . "|" . $staticCallback[0];
"#,
    );
    assert_eq!(
        out,
        "instance=a|static=b|instanceLabel|PdoFetchFuncFormatter"
    );
}

/// Verifies `FETCH_FUNC` resolves an invokable object carried through its Mixed callback slot.
#[test]
fn test_pdo_fetch_func_accepts_invokable_object() {
    let out = compile_and_run(
        r#"<?php
class PdoFetchFuncInvoker {
    public function __invoke(string $value): string {
        return "invoked=" . $value;
    }
}

$db = new PDO("sqlite::memory:");
$rows = $db->query("SELECT 'object'")
    ->fetchAll(PDO::FETCH_FUNC, new PdoFetchFuncInvoker());
echo $rows[0];
"#,
    );
    assert_eq!(out, "invoked=object");
}

/// Verifies `FETCH_LAZY` exposes a reusable statement-backed `PDORow` with property and offset reads.
#[test]
fn test_pdo_fetch_lazy_returns_reused_pdo_row() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1 AS id, 'a' AS label UNION ALL SELECT 2, 'b'");
$first = $stmt->fetch(PDO::FETCH_LAZY);
if (!($first instanceof PDORow)) { throw new Exception("missing first row"); }
PDORow $typedFirst = $first;
echo "PDORow:", $typedFirst->id, ":", $typedFirst[1], ":", $typedFirst->queryString, "|";
$second = $stmt->fetch(PDO::FETCH_LAZY);
if (!($second instanceof PDORow)) { throw new Exception("missing second row"); }
PDORow $typedSecond = $second;
echo ($typedFirst === $typedSecond ? "same" : "different"), ":", $typedFirst->id, ":", $typedSecond[1], "|";
"#,
    );
    assert_eq!(
        out,
        "PDORow:1:a:SELECT 1 AS id, 'a' AS label UNION ALL SELECT 2, 'b'|same:2:b|"
    );
}

/// Verifies the PDORow refresh hook is private outside PDOStatement::fetch().
#[test]
fn test_pdo_row_refresh_hook_is_not_public() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$row = $db->query("SELECT 1 AS id")->fetch(PDO::FETCH_LAZY);
if (!($row instanceof PDORow)) { throw new Exception("missing row"); }
PDORow $typedRow = $row;
try {
    $typedRow->__elephcRefresh([], []);
    echo "public";
} catch (Error $error) {
    echo "private";
}
"#,
    );
    assert_eq!(out, "private");
}

/// P2-11: `fetchColumn()` with an index at or beyond `columnCount()` throws a
/// `ValueError` once a row actually exists to check the index against
/// (verified against real PHP; an out-of-range index against an EMPTY result
/// still just returns `false`, matching the "no more rows" case).
#[test]
fn test_pdo_fetch_column_out_of_range_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$stmt = $db->query("SELECT a FROM t");
try {
    $stmt->fetchColumn(1);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "threw:Invalid column index");
}

/// P2-11: a negative column index gets its own distinct PHP-matching message.
#[test]
fn test_pdo_fetch_column_negative_index_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$stmt = $db->query("SELECT a FROM t");
try {
    $stmt->fetchColumn(-1);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "threw:Column index must be greater than or equal to 0");
}

/// P2-11: an out-of-range index against an EMPTY result set still just returns
/// `false` (no row exists to validate the index against), matching real PHP.
#[test]
fn test_pdo_fetch_column_out_of_range_on_empty_result_returns_false() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$stmt = $db->query("SELECT a FROM t");
$v = $stmt->fetchColumn(5);
echo $v === false ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies `bindColumn()` retains caller storage and writes each successful fetch.
#[test]
fn test_pdo_bind_column_updates_durable_reference() {
    let out = compile_and_run(
        r#"<?php
function run(int $id = 0, string $label = ""): void {
    $db = new PDO("sqlite::memory:");
    $db->exec("CREATE TABLE t (id INTEGER, label TEXT)");
    $db->exec("INSERT INTO t VALUES (1, 'a'), (2, 'b')");
    $stmt = $db->query("SELECT id, label FROM t ORDER BY id");
    $stmt->bindColumn(1, $id, PDO::PARAM_INT);
    $stmt->bindColumn("label", $label);
    echo ($stmt->fetch(PDO::FETCH_BOUND) ? "row" : "none") . ":" . $id . ":" . $label;
    $stmt->fetch(PDO::FETCH_ASSOC);
    echo "|" . $id . ":" . $label;
}
run();
"#,
    );
    assert_eq!(out, "row:1:a|2:b");
}

/// Verifies rebinding one output column replaces its previous destination like php-src's hash.
#[test]
fn test_pdo_bind_column_replaces_existing_destination() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 42 AS answer");
mixed $first = "first";
mixed $second = "second";
$stmt->bindColumn(1, $first, PDO::PARAM_INT);
$stmt->bindColumn(1, $second, PDO::PARAM_INT);
$stmt->fetch(PDO::FETCH_BOUND);
echo $first . "|" . $second;
"#,
    );
    assert_eq!(out, "first|42");
}

/// `fetch(PDO::FETCH_BOUND)` advances and reports availability with no bindings too.
#[test]
fn test_pdo_fetch_bound_advances_cursor_without_throwing() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$db->exec("INSERT INTO t VALUES (2)");
$stmt = $db->query("SELECT a FROM t");
$first = $stmt->fetch(PDO::FETCH_BOUND);
$second = $stmt->fetch(PDO::FETCH_BOUND);
$third = $stmt->fetch(PDO::FETCH_BOUND);
echo ($first === true ? "true" : "other") . ":"
    . ($second === true ? "true" : "other") . ":"
    . ($third === false ? "false" : "other");
"#,
    );
    assert_eq!(out, "true:true:false");
}

/// P1-3: `fetch(PDO::FETCH_BOUND)` against an EMPTY result set returns `false`
/// on the very first call (no row is ever available to advance to).
#[test]
fn test_pdo_fetch_bound_on_empty_result_returns_false() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$stmt = $db->query("SELECT a FROM t");
$r = $stmt->fetch(PDO::FETCH_BOUND);
echo $r === false ? "false" : "other";
"#,
    );
    assert_eq!(out, "false");
}

/// P1-1: `FETCH_CLASS | FETCH_PROPS_LATE` is honored, not rejected — verified
/// against php-src's `pdo_stmt_verify_mode`: PROPS_LATE is never even tested in
/// that function (it is not a rejection reason for ANY base mode), and a base
/// mode of FETCH_CLASS jumps straight to its own switch case regardless.
/// elephc's FETCH_CLASS is already unconditionally ctor-first, so the flag is
/// free to accept.
///
/// F-STMT-09: this now ALSO pins that flag masking does not OVER-reject or silently
/// drop the target. `setFetchMode()`'s gates used to test the RAW `$mode`, which is
/// false the moment any high-bit flag is OR-ed in, so this exact call matched no gate
/// at all and its class name was dropped on the floor by the storage block — leaving a
/// statement in FETCH_CLASS mode with NO target, which then quietly fetched `stdClass`
/// rows. Asserting `instanceof Row` (not merely "an object") is what catches that.
#[test]
fn test_pdo_fetch_class_with_props_late_flag_works() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$stmt = $db->query("SELECT id, name FROM t");
$stmt->setFetchMode(PDO::FETCH_CLASS | PDO::FETCH_PROPS_LATE, Row::class);
$row = $stmt->fetch();
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// P1-1: `FETCH_PROPS_LATE` combined with a non-CLASS base mode is likewise
/// accepted (silently a no-op, since only FETCH_CLASS's hydration order could
/// possibly be affected by it) rather than rejected.
#[test]
fn test_pdo_fetch_props_late_flag_with_other_mode_is_not_rejected() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
$row = $stmt->fetch(PDO::FETCH_ASSOC | PDO::FETCH_PROPS_LATE);
echo $row["id"];
"#,
    );
    assert_eq!(out, "1");
}

/// F-STMT-02: `FETCH_CLASS | FETCH_CLASSTYPE` is accepted (php-src's
/// `pdo_stmt_verify_mode` switches directly to the FETCH_CLASS case, skipping the
/// CLASSTYPE rejection check for that base mode) — and it is now REAL, where this test
/// used to assert a fabricated version of it.
///
/// CLASSTYPE means the class is NOT the one configured on the statement: it is READ
/// FROM COLUMN 0'S RUNTIME VALUE, row by row, so ONE result set can hydrate a DIFFERENT
/// class per row. php-src (`pdo_stmt.c:805-829`) does three things the old code did none
/// of: it `fetch_value()`s column 0, it `zend_lookup_class()`es that string, and it
/// hydrates from COLUMN 1 ONWARD — column 0 was CONSUMED as the type tag and must not
/// also land in a property (`fetch_value(stmt, &val, i++, NULL)` literally advances the
/// column cursor past it).
///
/// The old test passed a literal `Row::class`, asserted the literal won, and asserted
/// column 0 was still assigned as a property — i.e. it pinned the flag being IGNORED.
/// An explicit class argument is now a `ValueError` (see the setFetchMode test below),
/// so the class can ONLY come from the data. Here two rows name two different classes
/// and each is instantiated from its own row's column 0, which no "literal class wins"
/// implementation could produce.
/// The consumption half is asserted by DECLARING the column-0 property (`$kind`) on both
/// classes and proving it never receives column 0's value. Both properties are given an
/// explicit `= null` DEFAULT, and that default is what makes the assertion legal: PDO never
/// assigns `$kind` (column 0 having been consumed as the type tag), so without a default it
/// would be an UNINITIALIZED TYPED PROPERTY, and reading one is an `Error` — "Typed property
/// Cat::$kind must not be accessed before initialization" — in real PHP 8.4 exactly as in
/// elephc. (An earlier draft of this test omitted the defaults and died on that Error, which
/// looked like a PDO bug and was not one.) Defaulted, the slot reads back as null, so
/// comparing against the class-name STRING distinguishes precisely one thing: column 0
/// having leaked into a property, which is the defect under test.
#[test]
fn test_pdo_fetch_class_with_classtype_flag_reads_class_from_column_zero() {
    let out = compile_and_run(
        r#"<?php
class Cat {
    public mixed $id = null;
    public mixed $kind = null;
}
class Dog {
    public mixed $id = null;
    public mixed $kind = null;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, id INTEGER)");
$db->exec("INSERT INTO t VALUES ('Cat', 1), ('Dog', 2)");
$stmt = $db->query("SELECT kind, id FROM t ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_CLASS | PDO::FETCH_CLASSTYPE);

$first = $stmt->fetch();
$second = $stmt->fetch();

// Column 0 ('kind') is CONSUMED as the class name: its value must NOT also be
// assigned to the same-named property. `id` (column 1) is the only column hydrated.
$catKind = ($first->kind === "Cat") ? "leaked" : "consumed";
$dogKind = ($second->kind === "Dog") ? "leaked" : "consumed";

echo (($first instanceof Cat) ? "Cat" : "not-cat") . ":" . $first->id . ":" . $catKind
    . "|" . (($second instanceof Dog) ? "Dog" : "not-dog") . ":" . $second->id . ":" . $dogKind;
"#,
    );
    assert_eq!(out, "Cat:1:consumed|Dog:2:consumed");
}

/// F-STMT-02, the fallback arm: a column-0 value naming NO class hydrates a `stdClass`
/// instead of failing — php-src's `zend_lookup_class()`-found-nothing branch resolves to
/// `zend_standard_class_def` (`pdo_stmt.c:805-829`). Column 0 is STILL consumed: the
/// bogus type tag does not become a property of the fallback object either.
///
/// The stdClass fallback is implemented WITHOUT `class_exists()`: elephc's
/// `class_exists()` is an AOT constant-fold (`lower_class_like_exists` requires a CONST
/// STRING operand) and does not compile against a runtime string — the only kind that can
/// ever reach here. The dynamic `new $name()` IS the existence probe instead: it lowers
/// to `DynamicObjectNewMixed`, whose runtime miss path (`__rt_new_by_name`) returns PHP
/// `null` for a name in no class table. This test is what proves that miss path is
/// actually reached rather than, say, constructing a garbage object.
///
/// Column 0 is consumed here too, but that is NOT asserted through this fixture: the
/// fallback is a `stdClass`, whose columns arrive as DYNAMIC properties, so probing for
/// the absence of `kind` would mean reading a never-set dynamic property (or reaching for
/// `isset()`/`get_object_vars()`, neither of which this suite exercises anywhere). The
/// exclusion is pinned by the typed-class sibling above — both arms call the same
/// `assignColumnsFrom($obj, 1, $count)` with the same start index — and, independently, by
/// the FETCH_GROUP/FETCH_UNIQUE shape tests, which assert the consumed key column is
/// absent from every row.
#[test]
fn test_pdo_fetch_classtype_unknown_class_falls_back_to_stdclass() {
    let out = compile_and_run(
        r#"<?php
class Cat {
    public mixed $id;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, id INTEGER)");
$db->exec("INSERT INTO t VALUES ('NoSuchClass', 7)");
$stmt = $db->query("SELECT kind, id FROM t");
$stmt->setFetchMode(PDO::FETCH_CLASS | PDO::FETCH_CLASSTYPE);
$row = $stmt->fetch();

echo (($row instanceof stdClass) ? "stdClass" : "other")
    . ":" . (($row instanceof Cat) ? "cat" : "not-cat")
    . ":" . $row->id;
"#,
    );
    assert_eq!(out, "stdClass:not-cat:7");
}

/// P1-1: `FETCH_CLASSTYPE` combined with any OTHER base mode is rejected with a
/// `ValueError`, matching php-src's `pdo_stmt_verify_mode` default-case check
/// (`zend_argument_value_error(1, "must use PDO::FETCH_CLASSTYPE with
/// PDO::FETCH_CLASS")`) verified against the PHP-8.4 branch of php-src.
#[test]
fn test_pdo_fetch_classtype_flag_throws_with_non_class_mode() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->fetch(PDO::FETCH_ASSOC | PDO::FETCH_CLASSTYPE);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::fetch(): Argument #1 ($mode) must use PDO::FETCH_CLASSTYPE with PDO::FETCH_CLASS"
    );
}

/// F-STMT-09: `setFetchMode(FETCH_CLASS|FETCH_CLASSTYPE, 'Foo')` is REJECTED. Under
/// CLASSTYPE the class name comes from column 0's VALUE at fetch time, so an explicit
/// class argument is not merely redundant — it is a CONTRADICTION, and php-src rejects
/// the combination outright (`pdo_stmt.c:1783-1790`: the CLASSTYPE arm takes its class
/// from the data and raises `zend_argument_count_error` the moment a variadic class
/// argument accompanies it). This prelude used to ACCEPT the combo and quietly discard
/// the argument.
///
/// elephc has no `ArgumentCountError` class, so — per this file's existing convention for
/// the sibling arity gates — the rejection is a `ValueError` carrying php-src's literal
/// message text.
#[test]
fn test_pdo_set_fetch_mode_classtype_with_explicit_class_is_rejected() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public mixed $id;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->setFetchMode(PDO::FETCH_CLASS | PDO::FETCH_CLASSTYPE, Foo::class);
    echo "no-throw";
} catch (ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, 2 given"
    );
}

/// F-STMT-09, the OVER-rejection negative control: masking the flags off must not make
/// the gates reject a LEGITIMATE flagged call. `setFetchMode(FETCH_CLASS|FETCH_PROPS_LATE,
/// 'Row')` is accepted, returns true, AND the class name is actually STORED — the bug this
/// pins is that the old raw-`$mode` storage test silently DROPPED the target for any
/// flagged mode, leaving the statement in FETCH_CLASS mode with no class and quietly
/// fetching `stdClass` rows. Only fetching a row and checking `instanceof Row` catches it,
/// so the round-trip is asserted rather than just the return value.
///
/// (PROPS_LATE is never a rejection reason in php-src's `pdo_stmt_verify_mode` for ANY
/// base mode — unlike CLASSTYPE above, which is one for every base mode except
/// FETCH_CLASS.)
#[test]
fn test_pdo_set_fetch_mode_props_late_flag_keeps_the_class_target() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (5)");
$stmt = $db->query("SELECT id FROM t");
$ok = $stmt->setFetchMode(PDO::FETCH_CLASS | PDO::FETCH_PROPS_LATE, Row::class);
$row = $stmt->fetch();
echo (($ok === true) ? "true" : "false")
    . ":" . (($row instanceof Row) ? "Row" : "not-row")
    . ":" . $row->id;
"#,
    );
    assert_eq!(out, "true:Row:5");
}

/// F-STMT-04: `fetch(PDO::FETCH_INTO)` with NO object configured used to hand back a
/// fresh, anonymous `stdClass` — a silent success that threw the caller's row into an
/// object they never see. php-src raises HY000 "No fetch-into object specified."
/// (`pdo_stmt.c:864-871`, via `pdo_raise_impl_error`, hence errMode-aware). FETCH_INTO
/// without a target is not a mode, it is a mistake: the target is the entire point of the
/// mode.
#[test]
fn test_pdo_fetch_into_without_target_raises_hy000_under_exception() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
try {
    $stmt->fetch(PDO::FETCH_INTO);
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw:" . $e->getMessage() . "|" . $e->errorInfo[0];
}
"#,
    );
    assert_eq!(
        out,
        "threw:SQLSTATE[HY000]: General error: No fetch-into object specified.|HY000"
    );
}

/// F-STMT-04, the errMode-aware other half: the same targetless `FETCH_INTO` is QUIET
/// under `ERRMODE_SILENT` and returns `false` — `pdo_raise_impl_error` respects the error
/// mode. `false` (not an object) is the load-bearing assertion: the old code's silent
/// fresh-stdClass is exactly what this must never be again.
///
/// `errorCode()` is deliberately NOT asserted here. `failCode()` mirrors
/// `pdo_raise_impl_error`'s errMode dispatch but does NOT write the statement's driver
/// error slots (there was no driver-level failure to read one from), so `errorCode()`
/// still reads the driver's "00000" — a synthetic-error/driver-error asymmetry that is a
/// property of `failCode()` generally, not of this fetch mode, and is not this test's
/// subject.
#[test]
fn test_pdo_fetch_into_without_target_returns_false_under_silent() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
$row = $stmt->fetch(PDO::FETCH_INTO);
echo ($row === false) ? "false" : "object";
"#,
    );
    assert_eq!(out, "false");
}

/// F-STMT-15: `FETCH_GROUP` with `FETCH_ASSOC`. Column 0 is CONSUMED as the grouping key
/// and each key maps to a LIST of every row that carried it, in result order (php-src's
/// `do_fetch` with a non-NULL `return_all`: `add_next_index_zval` into the group's array,
/// `pdo_stmt.c:1072-1086`). These used to throw "not yet supported".
///
/// `count($r)` is the consumption assertion and it is the whole point: the query selects
/// THREE columns and every grouped row must contain exactly TWO. A row that still carried
/// its own key column would be the classic silently-wrong result — plausible-looking, and
/// wrong in the way nobody notices.
///
/// Non-numeric keys throughout: elephc's array keeps an integer-LOOKING group key a STRING
/// key where PHP folds it back to an int (the one documented divergence in
/// `fetchAllGrouped()`), so a numeric key would be pinning an array-semantics gap rather
/// than PDO behavior.
#[test]
fn test_pdo_fetch_all_group_with_assoc_consumes_column_zero() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT, n INTEGER)");
$db->exec("INSERT INTO t VALUES ('fruit', 'apple', 1), ('fruit', 'banana', 2), ('veg', 'carrot', 3)");
$out = $db->query("SELECT kind, name, n FROM t ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_ASSOC);

$s = "";
foreach ($out as $k => $rows) {
    $s .= $k . "[" . count($rows) . "]=";
    foreach ($rows as $r) {
        // count($r) == 2: 'kind' (column 0) became the KEY and is gone from the row.
        $s .= count($r) . ":" . $r["name"] . ":" . $r["n"] . ",";
    }
    $s .= ";";
}
echo $s;
"#,
    );
    assert_eq!(out, "fruit[2]=2:apple:1,2:banana:2,;veg[1]=2:carrot:3,;");
}

/// Verifies FETCH_GROUP applies PHP array-key normalization after converting the
/// grouping column to string: canonical in-range decimal integers become int keys,
/// while leading zeros, `-0`, and overflowing values remain string keys.
#[test]
fn test_pdo_fetch_all_group_normalizes_integer_looking_keys() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$rows = $db->query("SELECT '1' AS k, 'a' AS v UNION ALL SELECT '01', 'b' UNION ALL SELECT '-1', 'c' UNION ALL SELECT '-0', 'd' UNION ALL SELECT '9223372036854775808', 'e'")
    ->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN);
foreach ($rows as $key => $values) {
    echo gettype($key) . ":" . $key . "=" . $values[0] . ";";
}
"#,
    );
    assert_eq!(
        out,
        "integer:1=a;string:01=b;integer:-1=c;string:-0=d;string:9223372036854775808=e;"
    );
}

/// F-STMT-15: `FETCH_GROUP` with `FETCH_NUM`. Same consumption, plus the subtler half:
/// the surviving columns are RE-INDEXED FROM 0, not left at their original offsets.
/// php-src walks the row with TWO cursors — the column index `i` (which starts at 1, the
/// key having been taken) and the output index `idx` (which starts at 0) — so the first
/// column AFTER the key lands at `[0]`. A row that kept its original offsets would start
/// at `[1]` and have NO `[0]` at all, which is why `$r[0]` is read here rather than
/// counted.
#[test]
fn test_pdo_fetch_all_group_with_num_reindexes_from_zero() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT, n INTEGER)");
$db->exec("INSERT INTO t VALUES ('fruit', 'apple', 1), ('veg', 'carrot', 3)");
$out = $db->query("SELECT kind, name, n FROM t ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_NUM);

$s = "";
foreach ($out as $k => $rows) {
    foreach ($rows as $r) {
        $s .= $k . "=" . count($r) . ":" . $r[0] . ":" . $r[1] . ";";
    }
}
echo $s;
"#,
    );
    assert_eq!(out, "fruit=2:apple:1;veg=2:carrot:3;");
}

/// F-STMT-15: `FETCH_UNIQUE` maps each key to ONE row, LAST WRITE WINS on a duplicate key
/// — php-src uses `zend_symtable_update`, a plain overwrite that neither detects nor
/// complains about a duplicate (`pdo_stmt.c:1072-1086`). FETCH_UNIQUE is 0x30000 and thus
/// a SUPERSET of FETCH_GROUP (0x10000), not a sibling of it: a `& 0x10000` test is true
/// for BOTH, so getting the last-wins shape (rather than a one-element LIST per key) is
/// what proves the 0x30000 mask is actually being applied.
///
/// 'fruit' appears twice; the SECOND row must win outright, and the value must be the ROW
/// ITSELF, not a list containing it.
#[test]
fn test_pdo_fetch_all_unique_is_last_write_wins() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT)");
$db->exec("INSERT INTO t VALUES ('fruit', 'apple'), ('veg', 'carrot'), ('fruit', 'banana')");
$out = $db->query("SELECT kind, name FROM t ORDER BY rowid")->fetchAll(PDO::FETCH_UNIQUE | PDO::FETCH_ASSOC);

$s = count($out) . "|";
foreach ($out as $k => $r) {
    // $r is the ROW (a 1-element assoc array), NOT a list of rows: count($r) == 1 and
    // $r["name"] reads directly. 'kind' is consumed as the key, so it is not in $r.
    $s .= $k . "=" . count($r) . ":" . $r["name"] . ";";
}
echo $s;
"#,
    );
    assert_eq!(out, "2|fruit=1:banana;veg=1:carrot;");
}

/// F-STMT-15: the classic `FETCH_GROUP|FETCH_COLUMN` idiom — `[kind => [name, name, …]]`
/// — and the defaulting rule that makes it work. php-src's `fetchAll()` spells it out:
/// `stmt->fetch.column = arg2 ? … : (how & PDO_FETCH_GROUP ? 1 : 0)`, i.e. with NO explicit
/// index and GROUP set, the VALUE column defaults to **1**, not the usual 0. Column 0 is
/// already spoken for as the grouping key, so defaulting the value to it too would return
/// the useless `[kind => [kind, kind, …]]`.
///
/// The second half pins that an EXPLICIT index still overrides the default (column 2 here),
/// so the defaulting is a fallback and not a hardcode.
#[test]
fn test_pdo_fetch_all_group_column_idiom_defaults_value_column_to_one() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT, n INTEGER)");
$db->exec("INSERT INTO t VALUES ('fruit', 'apple', 1), ('fruit', 'banana', 2), ('veg', 'carrot', 3)");

$byName = $db->query("SELECT kind, name, n FROM t ORDER BY n")
    ->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN);
$s = "";
foreach ($byName as $k => $names) {
    $s .= $k . "=" . implode(",", $names) . ";";
}

// An explicit index overrides the GROUP default of 1: take column 2 ('n') instead.
$byN = $db->query("SELECT kind, name, n FROM t ORDER BY n")
    ->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN, 2);
$s .= "|";
foreach ($byN as $k => $ns) {
    $s .= $k . "=" . implode(",", $ns) . ";";
}
echo $s;
"#,
    );
    assert_eq!(
        out,
        "fruit=apple,banana;veg=carrot;|fruit=1,2;veg=3;"
    );
}

/// O(n^2)->O(n) regression: `FETCH_GROUP`'s append branch used to read the bucket out of
/// `$_groups`, push onto the local copy, then write it back — with the bucket sitting at
/// refcount 2 (the map slot + the local) across every one of those pushes, so each one
/// COW-cloned the whole bucket. The fix `unset()`s the map slot before the push so the
/// bucket is refcount 1 and mutates in place. This drives ONE key through 60 rows — enough
/// to have made the O(n^2) clone-per-row path expensive — interleaved with two single-row
/// keys, so a bug in the unset()/reinsert sequence (wrong bucket, dropped rows, corrupted
/// key order) would show up as either a short/garbled 'big' group or a scrambled key order.
///
/// Asserts, precisely: (1) the 60-row group contains ALL 60 rows, in RESULT order; (2) the
/// two incidental single-row groups are untouched by the big group's growth; (3) all three
/// keys come out in FIRST-SEEN order ('a', then 'big', then 'mid', then 'b' — 'mid' is
/// inserted in the MIDDLE of the 'big' run, after 'big' is already first-seen, so its
/// presence there also proves the interleaved unset()/reinsert of 'big' never disturbs an
/// unrelated key's own bucket).
#[test]
fn test_pdo_fetch_all_group_large_group_is_on_via_unset_reinsert() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT, n INTEGER)");
$ins = $db->prepare("INSERT INTO t (kind, name, n) VALUES (?, ?, ?)");
$ins->execute(["a", "afirst", 0]);
for ($i = 1; $i <= 30; $i++) {
    $ins->execute(["big", "r" . $i, $i]);
}
$ins->execute(["mid", "midrow", 31]);
for ($i = 31; $i <= 60; $i++) {
    $ins->execute(["big", "r" . $i, $i + 1]);
}
$ins->execute(["b", "blast", 62]);

$out = $db->query("SELECT kind, name FROM t ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN);

$s = "";
foreach ($out as $k => $vals) {
    $s .= $k . "[" . count($vals) . "]=" . implode(",", $vals) . ";";
}
echo $s;
"#,
    );
    let big_rows: Vec<String> = (1..=60).map(|i| format!("r{i}")).collect();
    let expected = format!(
        "a[1]=afirst;big[60]={};mid[1]=midrow;b[1]=blast;",
        big_rows.join(",")
    );
    assert_eq!(out, expected);
}

/// F-STMT-15: the two combinations that are REFUSED LOUDLY rather than faked, both because
/// column 0 is already consumed as the grouping key and something else wants it too.
///
/// FETCH_CLASSTYPE also reads column 0 (as the class name); php-src resolves the collision
/// by consuming TWO columns (key from 0, class from 1, properties from 2) — a shape no
/// caller of this prelude has ever been able to ask for, so it is refused rather than
/// invented. FETCH_NAMED under GROUP has no meaningful per-group row either (its
/// duplicate-column-name grouping is a second, orthogonal reshaping). Loud beats silently
/// wrong: the caller gets an error naming the combination, not a plausible array of the
/// wrong shape.
#[test]
fn test_pdo_fetch_all_group_refuses_classtype_and_named() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (kind TEXT, name TEXT)");
$db->exec("INSERT INTO t VALUES ('fruit', 'apple')");

try {
    $db->query("SELECT kind, name FROM t")
        ->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_CLASS | PDO::FETCH_CLASSTYPE);
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->getMessage();
}

echo "|";

try {
    $db->query("SELECT kind, name FROM t")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_NAMED);
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "PDO::FETCH_CLASSTYPE is not supported with PDO::FETCH_GROUP or PDO::FETCH_UNIQUE",
            "|",
            "PDO::FETCH_GROUP and PDO::FETCH_UNIQUE are not supported with this fetch mode"
        )
    );
}

/// P1-11 (best-effort): `ATTR_STRINGIFY_FETCHES` stringifies INTEGER and FLOAT
/// columns but leaves NULL untouched, matching real PHP, and is threaded from
/// the connection to a statement the same way `defaultFetchMode` already is (a
/// `prepare()`-time snapshot).
#[test]
fn test_pdo_stringify_fetches() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_STRINGIFY_FETCHES, true);
$db->exec("CREATE TABLE t (a INTEGER, b REAL, c TEXT)");
$db->exec("INSERT INTO t VALUES (1, 2.5, 'x')");
$row = $db->query("SELECT a, b, c FROM t")->fetch(PDO::FETCH_ASSOC);
echo (is_string($row["a"]) ? "str" : "notstr") . ":" . $row["a"] . ",";
echo (is_string($row["b"]) ? "str" : "notstr") . ":" . $row["b"] . ",";
echo (is_string($row["c"]) ? "str" : "notstr") . ":" . $row["c"] . ",";
$row2 = $db->query("SELECT NULL a")->fetch(PDO::FETCH_ASSOC);
echo $row2["a"] === null ? "null" : "notnull";
"#,
    );
    assert_eq!(out, "str:1,str:2.5,str:x,null");
}

/// P1-11: `ATTR_STRINGIFY_FETCHES` defaults to off — an ordinary connection
/// still returns native int/float types, so this slice's addition is opt-in.
#[test]
fn test_pdo_stringify_fetches_off_by_default() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$row = $db->query("SELECT a FROM t")->fetch(PDO::FETCH_ASSOC);
echo is_int($row["a"]) ? "int" : "notint";
"#,
    );
    assert_eq!(out, "int");
}

/// P1-4: a connect failure (here, `sqlite:` pointed at a directory that does not
/// exist, so SQLite cannot create the database file) throws with a populated
/// 3-element `errorInfo` and a `"SQLSTATE[<state>]: ..."`-prefixed message, matching
/// the standard try/catch-around-`new PDO` classification idiom
/// (`$e->errorInfo[0]`). Verified against a real PHP 8.5 CLI: sqlite connect
/// failures report SQLSTATE `HY000`.
#[test]
fn test_pdo_connect_failure_populates_error_info() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new PDO("sqlite:/nonexistent_dir_elephc_test_xyz/foo.db");
    echo "no-throw";
} catch (PDOException $e) {
    $info = $e->errorInfo;
    echo (str_starts_with($e->getMessage(), "SQLSTATE[HY000]:") ? "prefixed" : "unprefixed") . ",";
    echo $info[0] . "," . ($info[1] === null ? "null" : "notnull") . ",";
    echo (strlen($info[2]) > 0 ? "has-message" : "no-message");
}
"#,
    );
    assert_eq!(out, "prefixed,HY000,null,has-message");
}

/// A driver error's `getMessage()` carries the php-src "SQLSTATE[%s]: %s: %d %s"
/// shape — the SQLSTATE class DESCRIPTION ("General error", "Integrity constraint
/// violation") and the native driver code, not just the raw driver message.
/// Verified byte-for-byte against a real PHP 8.4 CLI + pdo_sqlite. errorInfo keeps
/// the raw [state, native, message] triple, unchanged.
#[test]
fn test_pdo_driver_error_message_has_description_and_native_code() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
$db->exec("CREATE TABLE t (id INT PRIMARY KEY)");
$db->exec("INSERT INTO t VALUES (1)");
try { $db->query("SELECT * FROM nope"); } catch (PDOException $e) { echo $e->getMessage(), "\n"; }
try { $db->exec("INSERT INTO t VALUES (1)"); }
catch (PDOException $e) { echo $e->getMessage(), "|", $e->errorInfo[0], "|", $e->errorInfo[1], "|", $e->errorInfo[2]; }
"#,
    );
    assert_eq!(
        out,
        "SQLSTATE[HY000]: General error: 1 no such table: nope\n\
         SQLSTATE[23000]: Integrity constraint violation: 19 UNIQUE constraint failed: t.id\
         |23000|19|UNIQUE constraint failed: t.id"
    );
}

/// P1-4 (unrecognized-driver case, kept distinct from a known-driver connect
/// failure): `new PDO("bogus:...")` still throws PHP's bare "could not find
/// driver" shape — no `SQLSTATE[...]` prefix — because no driver ever attempted
/// the connection. Verified against a real PHP 8.5 CLI. The source explicitly
/// passes `errorInfo: null` at this throw site too (see the constructor's
/// comment), but asserting `$e->errorInfo === null` here is NOT reliable: a
/// pre-existing, general elephc bug (reproduced in isolation, unrelated to this
/// fix — outside PDO entirely) corrupts an untyped/Mixed constructor parameter's
/// `null` call sites when OTHER call sites for the same parameter, elsewhere in
/// the same class, pass array literals whose element types differ (PDOException's
/// `$errorInfo` sees both `[string, null, string]` here and `[string, int,
/// string]` in `fail()`/`errorInfo()` elsewhere in this very class) — so this test
/// only pins the message shape, not the errorInfo value.
#[test]
fn test_pdo_connect_unrecognized_driver_error_info_stays_null() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new PDO("bogus:host=localhost");
    echo "no-throw";
} catch (PDOException $e) {
    echo (str_starts_with($e->getMessage(), "SQLSTATE[") ? "prefixed" : "unprefixed");
}
"#,
    );
    assert_eq!(out, "unprefixed");
}

/// P2-17: `PDO::__clone()` throws — PHP marks `PDO` uncloneable so two Zend
/// objects never share one bridge connection handle (whichever is destructed
/// first would otherwise close it out from under the survivor). elephc has no
/// `clone` operator yet (confirmed: the lexer/parser have no `clone` keyword at
/// all — `clone $x` fails to parse, and `--check` reports "Undefined function:
/// clone"), so this pins the guard method directly by invoking the magic method
/// like any other, exactly as codegen would dispatch it once `clone $pdo` support
/// lands.
#[test]
fn test_pdo_clone_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->__clone();
    echo "no-throw";
} catch (\Error $e) {
    // Read into a local first: elephc has a pre-existing, unrelated bug where
    // concatenating a string LITERAL with a caught exception's getMessage()
    // result corrupts the output ONLY when that message was itself built by
    // concatenation at throw time (reproducible with plain
    // `throw new Exception("a" . $b);` outside any PDO code) — an intermediate
    // variable sidesteps it.
    $msg = $e->getMessage();
    echo "threw:" . $msg;
}
"#,
    );
    assert_eq!(out, "threw:Trying to clone an uncloneable object of class PDO");
}

/// P2-17: `PDOStatement::__clone()` throws for the same reason as `PDO::__clone()`
/// — a shallow clone would produce a second owner of the statement handle. See
/// `test_pdo_clone_throws` for why this invokes the magic method directly rather
/// than through a `clone` expression.
#[test]
fn test_pdo_statement_clone_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1");
try {
    $stmt->__clone();
    echo "no-throw";
} catch (\Error $e) {
    // See test_pdo_clone_throws for why this reads into a local first.
    $msg = $e->getMessage();
    echo "threw:" . $msg;
}
"#,
    );
    assert_eq!(
        out,
        "threw:Trying to clone an uncloneable object of class PDOStatement"
    );
}

/// P2-17: cloning a driver subclass instance reports the RUNTIME class in the
/// message (`Pdo\Sqlite`, not the base `PDO`), matching a real PHP CLI exactly —
/// `__clone` is inherited from the base `PDO` class and reads `get_class($this)`.
#[test]
fn test_pdo_clone_throws_with_subclass_name() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
try {
    $db->__clone();
    echo "no-throw";
} catch (\Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Trying to clone an uncloneable object of class Pdo\\Sqlite"
    );
}

/// SQLite exposes the same embedded library version as client and server, while
/// rejecting server-info and connection-status exactly like php-src's driver hook.
#[test]
fn test_pdo_get_attribute_client_version_and_connection_status() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$clientVersion = $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
$serverVersion = $db->getAttribute(PDO::ATTR_SERVER_VERSION);
$serverInfo = $db->getAttribute(PDO::ATTR_SERVER_INFO);
$connStatus = $db->getAttribute(PDO::ATTR_CONNECTION_STATUS);
echo ($clientVersion !== null && strlen((string) $clientVersion) > 0) ? "has-client-version" : "null-client-version";
echo ",";
echo ($clientVersion === $serverVersion) ? "same-version" : "different-version";
echo ",";
echo ($serverInfo === false) ? "unsupported-info" : "has-server-info";
echo ",";
echo ($connStatus === false) ? "unsupported-status" : "has-connection-status";
"#,
    );
    assert_eq!(
        out,
        "has-client-version,same-version,unsupported-info,unsupported-status"
    );
}

/// SQLite's attribute hook has no connection-status case and therefore reaches IM001
/// under exception mode.
#[test]
fn test_pdo_get_attribute_connection_status_throws_for_sqlite() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->getAttribute(PDO::ATTR_CONNECTION_STATUS);
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->errorInfo[0];
}
"#,
    );
    assert_eq!(out, "IM001");
}

/// `prepare($sql, $options)` with two live statements taking different option-array
/// shapes must not corrupt the heap (a `foreach ($options ...)` in this ordinary frame
/// tripped the pre-existing interior-frame iterator miscompile; options are accepted
/// and ignored instead). Regression pin for the parity-audit fix.
#[test]
fn test_pdo_prepare_options_two_live_statements_no_crash() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (v)");
$db->exec("INSERT INTO t (v) VALUES (1)");
$s1 = $db->prepare("SELECT v FROM t");
$s2 = $db->prepare("SELECT v FROM t", [PDO::ATTR_CURSOR => PDO::CURSOR_FWDONLY]);
$s1->execute();
$s2->execute();
echo $s1->fetchColumn() . "," . $s2->fetchColumn();
"#,
    );
    assert_eq!(out, "1,1");
}

/// `fetchAll(PDO::FETCH_COLUMN, $n)` must return column `$n`, not always column 0.
/// Regression pin: the existing FETCH_COLUMN test only used the no-argument form.
#[test]
fn test_pdo_fetch_all_column_honors_index() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a, b, c)");
$db->exec("INSERT INTO t (a, b, c) VALUES (1, 10, 100), (2, 20, 200)");
$col2 = $db->query("SELECT a, b, c FROM t ORDER BY a")->fetchAll(PDO::FETCH_COLUMN, 2);
echo implode(",", $col2);
"#,
    );
    assert_eq!(out, "100,200");
}

/// `fetchAll(PDO::FETCH_COLUMN)` with no index must NOT reuse a stale index left
/// on `$this->fetchColumn` by a PRIOR `fetchAll(FETCH_COLUMN, $n)` call on the
/// same statement — it must reset to column 0, matching php-src's
/// `stmt->fetch.column = arg2 ? Z_LVAL(arg2) : (how & PDO_FETCH_GROUP ? 1 : 0)`.
/// Regression pin: the prelude's FETCH_COLUMN branch previously had no `else`,
/// leaking whatever index the earlier explicit call left behind.
#[test]
fn test_pdo_fetch_all_column_no_index_does_not_leak_prior_index() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a, b)");
$db->exec("INSERT INTO t (a, b) VALUES (1, 10), (2, 20)");
$stmt = $db->prepare("SELECT a, b FROM t ORDER BY a");
$stmt->execute();
$leaked = $stmt->fetchAll(PDO::FETCH_COLUMN, 1);
$stmt2 = $db->prepare("SELECT a, b FROM t ORDER BY a");
$stmt2->execute();
$reset = $stmt2->fetchAll(PDO::FETCH_COLUMN);
echo implode(",", $leaked) . "|" . implode(",", $reset);
"#,
    );
    assert_eq!(out, "10,20|1,2");
}

/// A caught `PDOException` exposes a usable `errorInfo`: `null` for an unrecognized
/// driver (matching PHP), and an indexable `[SQLSTATE, code, message]` triple for a
/// server error so the standard `$e->errorInfo[0]` idiom works. Regression pin for the
/// `?array`-typed property fix (an untyped property corrupted across the array/null
/// call sites).
#[test]
fn test_pdo_exception_error_info_usable() {
    let out = compile_and_run(
        r#"<?php
$msg = "";
try {
    $bad = new PDO("nosuchdriver:x");
} catch (\PDOException $e) {
    $msg .= ($e->errorInfo === null) ? "unrec-null" : "unrec-notnull";
}
$db = new PDO("sqlite::memory:");
try {
    $db->query("THIS IS NOT SQL");
} catch (\PDOException $e) {
    $msg .= "," . $e->errorInfo[0];
}
echo $msg;
"#,
    );
    assert_eq!(out, "unrec-null,HY000");
}

/// P0-A regression: `PDO::PARAM_STR` (the default `bindValue()` type) preserves an
/// embedded NUL byte end to end. Before the v20 ABI fix, `elephc_pdo_bind_text`
/// bound via SQLite's strlen-based `-1` sentinel, so `"AB\x00CD"` (5 bytes)
/// silently truncated to `"AB"` at the first NUL; the v20 bridge threads the
/// value's true byte length through instead. Also pins that a NUL-safe text bind
/// keeps SQLite's TEXT affinity/typeof (routed through `bind_text`, not
/// `bind_blob`).
#[test]
fn test_pdo_bind_param_str_preserves_embedded_nul() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (data TEXT)");
$ins = $db->prepare("INSERT INTO t (data) VALUES (?)");
$ins->bindValue(1, "AB\x00CD");
$ins->execute();
$row = $db->query("SELECT typeof(data) AS ty, data FROM t")->fetch(PDO::FETCH_ASSOC);
$back = $row["data"];
echo $row["ty"] . ":" . strlen($back) . ":" . bin2hex($back);
"#,
    );
    assert_eq!(out, "text:5:4142004344");
}

/// P0-A regression: `PDO::PARAM_LOB` now reaches `elephc_pdo_bind_blob` — declared
/// in the prelude and wired into `execute()`'s bind loop for the first time in
/// this slice, even though the bridge-side function has existed since v7. Raw
/// bytes `"\x00\xff\x01"` (an embedded NUL plus a non-UTF-8 0xFF byte) round-trip
/// unchanged through a BLOB column, proven via `bin2hex()`.
#[test]
fn test_pdo_bind_param_lob_preserves_binary() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (data BLOB)");
$ins = $db->prepare("INSERT INTO t (data) VALUES (?)");
$ins->bindValue(1, "\x00\xff\x01", PDO::PARAM_LOB);
$ins->execute();
$row = $db->query("SELECT typeof(data) AS ty, data FROM t")->fetch(PDO::FETCH_ASSOC);
$back = $row["data"];
echo $row["ty"] . ":" . strlen($back) . ":" . bin2hex($back);
"#,
    );
    assert_eq!(out, "blob:3:00ff01");
}

/// P1-e: the SQLite (default) `quote()` branch still ignores `$type` entirely —
/// unlike the mysql/pgsql branches, which now special-case `PDO::PARAM_LOB` — a
/// regression guard mirroring php-src's own sqlite quoter, which never consults
/// the type argument either. The pgsql `'\xDEADBEEF...'` bytea-hex-literal branch
/// and the mysql `_binary'...'` branch (both added in this slice) need a live
/// server to exercise through a real connection's `elephc_pdo_driver_name()`
/// dispatch, so they are covered by `tests/codegen/pdo_mysql.rs`'s `#[ignore]`
/// live fixtures instead (`mysql_quote_param_lob_binary_prefix`); no live pgsql
/// fixture file exists in this slice's scope to add an equivalent there.
#[test]
fn test_pdo_quote_sqlite_ignores_param_lob_type() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->quote("O'Brien", PDO::PARAM_LOB);
"#,
    );
    assert_eq!(out, "'O''Brien'");
}

/// P2-e: `setAttribute(PDO::ATTR_CASE, PDO::CASE_UPPER)` folds every FETCH_ASSOC
/// column-name key to uppercase (the original-case key is gone); the values are
/// untouched. `fetch()` returns `mixed`, so `array_keys()` (which needs a
/// statically-typed `array`) cannot be used here — `isset()` on both spellings
/// proves the fold happened instead.
#[test]
fn test_pdo_attr_case_upper_folds_keys() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_CASE, PDO::CASE_UPPER);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_ASSOC);
$hasLower = isset($row["id"]) ? "yes" : "no";
echo $hasLower . ":" . $row["ID"] . ":" . $row["NAME"];
"#,
    );
    assert_eq!(out, "no:1:Ada");
}

/// P3: `ATTR_CASE` folding is not FETCH_ASSOC-only — `setAttribute(PDO::ATTR_CASE,
/// PDO::CASE_UPPER)` also uppercases the dynamic PROPERTY name `FETCH_OBJ`
/// assigns on the fetched `stdClass` (via the same `columnName()` helper
/// `assignColumns()` uses), not just an array key.
#[test]
fn test_pdo_attr_case_upper_folds_fetch_obj_property() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_CASE, PDO::CASE_UPPER);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_OBJ);
$hasLower = isset($row->id) ? "yes" : "no";
echo $hasLower . ":" . $row->ID . ":" . $row->NAME;
"#,
    );
    assert_eq!(out, "no:1:Ada");
}

/// P3: `ATTR_CASE` folding also applies to `FETCH_BOTH`'s STRING-keyed half —
/// the numerically-indexed half (`$row[0]`, `$row[1]`) is untouched, only the
/// column-name string key is folded, confirming the fold is keyed off
/// `columnName()` (shared by every fetch style) rather than something
/// FETCH_ASSOC-specific.
#[test]
fn test_pdo_attr_case_upper_folds_fetch_both_string_key() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_CASE, PDO::CASE_UPPER);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_BOTH);
$hasLower = isset($row["id"]) ? "yes" : "no";
$hasNumeric = isset($row[0]) && isset($row[1]) ? "yes" : "no";
echo $hasLower . ":" . $hasNumeric . ":" . $row["ID"] . ":" . $row[0] . ":" . $row["NAME"] . ":" . $row[1];
"#,
    );
    assert_eq!(out, "no:yes:1:1:Ada:Ada");
}

/// P2-e: `setAttribute(PDO::ATTR_CASE, PDO::CASE_LOWER)` folds every FETCH_ASSOC
/// column-name key to lowercase, even when the SQL's own column aliases are
/// uppercase (sqlite3_column_name reports the alias exactly as written).
#[test]
fn test_pdo_attr_case_lower_folds_keys() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_CASE, PDO::CASE_LOWER);
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id AS ID, name AS NAME FROM t")->fetch(PDO::FETCH_ASSOC);
$hasUpper = isset($row["ID"]) ? "yes" : "no";
echo $hasUpper . ":" . $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "no:1:Ada");
}

/// P2-e regression guard: with no `ATTR_CASE` ever set (the default
/// `PDO::CASE_NATURAL`), FETCH_ASSOC column-name keys are left exactly as the
/// driver reports them (no uppercase spelling appears).
#[test]
fn test_pdo_attr_case_natural_default() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada')");
$row = $db->query("SELECT id, name FROM t")->fetch(PDO::FETCH_ASSOC);
$hasUpper = isset($row["ID"]) ? "yes" : "no";
echo $hasUpper . ":" . $row["id"] . ":" . $row["name"];
"#,
    );
    assert_eq!(out, "no:1:Ada");
}

/// P2-e: `setAttribute(PDO::ATTR_CASE, ...)` rejects a value outside
/// `{CASE_NATURAL, CASE_UPPER, CASE_LOWER}` with a `ValueError`, using the exact
/// message php-src's `pdo_dbh.c` uses ("Case folding mode must be one of the
/// PDO::CASE_* constants", confirmed against php-src).
#[test]
fn test_pdo_attr_case_rejects_invalid() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->setAttribute(PDO::ATTR_CASE, 99);
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:Case folding mode must be one of the PDO::CASE_* constants"
    );
}

/// `Pdo\Sqlite::ATTR_OPEN_FLAGS`-style constructor option: `PDO::ATTR_CASE` set
/// through the constructor's `$options` array (rather than `setAttribute()`)
/// takes effect too, threaded the same way `ATTR_TIMEOUT`'s constructor-option
/// test already proves for a different attribute.
#[test]
fn test_pdo_attr_case_constructor_option() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_CASE => PDO::CASE_UPPER]);
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t VALUES (1)");
$row = $db->query("SELECT id FROM t")->fetch(PDO::FETCH_ASSOC);
$hasLower = isset($row["id"]) ? "yes" : "no";
echo $row["ID"] . ":" . $hasLower;
"#,
    );
    assert_eq!(out, "1:no");
}

/// `getAttribute`/`setAttribute` round-trip both `ATTR_CASE` and
/// `ATTR_ORACLE_NULLS`; the default for each is the `*_NATURAL` value (0).
#[test]
fn test_pdo_get_set_attr_case_and_oracle_nulls() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo $db->getAttribute(PDO::ATTR_CASE) . ":" . $db->getAttribute(PDO::ATTR_ORACLE_NULLS);
$db->setAttribute(PDO::ATTR_CASE, PDO::CASE_UPPER);
$db->setAttribute(PDO::ATTR_ORACLE_NULLS, PDO::NULL_EMPTY_STRING);
echo ":" . $db->getAttribute(PDO::ATTR_CASE) . ":" . $db->getAttribute(PDO::ATTR_ORACLE_NULLS);
"#,
    );
    assert_eq!(out, "0:0:1:1");
}

/// P2-e: `setAttribute(PDO::ATTR_ORACLE_NULLS, PDO::NULL_EMPTY_STRING)` converts
/// an empty-string TEXT column value to `null` on fetch, mirroring php-src's
/// `fetch_value()` (`IS_STRING && Z_STRLEN_P(dest) == 0 && oracle_nulls ==
/// PDO_NULL_EMPTY_STRING`).
#[test]
fn test_pdo_oracle_nulls_empty_string_to_null() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ORACLE_NULLS, PDO::NULL_EMPTY_STRING);
$db->exec("CREATE TABLE t (name TEXT)");
$db->exec("INSERT INTO t VALUES ('')");
$row = $db->query("SELECT name FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["name"] === null ? "null" : "not-null";
"#,
    );
    assert_eq!(out, "null");
}

/// P2-e sibling: `setAttribute(PDO::ATTR_ORACLE_NULLS, PDO::NULL_TO_STRING)`
/// converts a `NULL` column value to `""` on fetch, mirroring php-src's
/// `fetch_value()` (`IS_NULL && oracle_nulls == PDO_NULL_TO_STRING`).
#[test]
fn test_pdo_oracle_nulls_null_to_empty_string() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ORACLE_NULLS, PDO::NULL_TO_STRING);
$db->exec("CREATE TABLE t (name TEXT)");
$db->exec("INSERT INTO t (name) VALUES (NULL)");
$row = $db->query("SELECT name FROM t")->fetch(PDO::FETCH_ASSOC);
echo $row["name"] === "" ? "empty" : ($row["name"] === null ? "still-null" : "other");
"#,
    );
    assert_eq!(out, "empty");
}

/// F-CORE-21: `PDO::exec("")` throws a `ValueError` before any driver call, exactly
/// like the `prepare("")` guard above it — php-src's `PHP_METHOD(PDO, exec)` raises
/// `zend_argument_must_not_be_empty_error(1)` from its own argument check, and its
/// `$statement` parameter name is what the message carries. Until this guard existed
/// the empty string reached the bridge, which cheerfully "executed" it.
#[test]
fn test_pdo_exec_empty_statement_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->exec("");
    echo "no-throw";
} catch (\ValueError $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "threw:PDO::exec(): Argument #1 ($statement) must not be empty"
    );
}

/// F-CORE-22: `PDO::query("")` carries its OWN empty-statement guard. php-src's
/// `PHP_METHOD(PDO, query)` validates its argument itself, so the failure names
/// `PDO::query()`; elephc used to let the empty string fall through to the internal
/// `prepare()` delegation, which reported the error under the wrong method name
/// ("PDO::prepare(): ...") — a message a caller matching on it would never expect.
/// The parameter spelling inside the parentheses is deliberately NOT pinned here
/// (php-src derives it from arginfo; see the report), only the method name, the
/// "must not be empty" wording, and the absence of any `prepare` leakage.
#[test]
fn test_pdo_query_empty_statement_names_query_not_prepare() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->query("");
    echo "no-throw";
} catch (\ValueError $e) {
    $m = $e->getMessage();
    echo (str_starts_with($m, "PDO::query(): Argument #1 (") ? "query" : "wrong-method");
    echo ":" . (str_contains($m, "must not be empty") ? "empty" : "wrong-text");
    echo ":" . (str_contains($m, "prepare") ? "leaked-prepare" : "no-prepare");
}
"#,
    );
    assert_eq!(out, "query:empty:no-prepare");
}

/// F-CORE-03 (the sharpest edge of the finding): php-src's `pdo_get_long_param()`
/// checks the SHAPE of an attribute value before any range check and raises a
/// `TypeError` for a non-int/bool/integer-numeric-string. elephc used to blind-cast
/// with `(int) $value` — and `(int) "banana"` is `0`, i.e. `PDO::ERRMODE_SILENT`,
/// which `checkErrMode()` happily accepted. So a typo'd attribute value silently
/// switched the connection into SILENT and swallowed every subsequent error. This
/// pins all three halves: the TypeError, the error mode surviving unchanged, and —
/// the part that actually bit — that errors still THROW afterwards.
#[test]
fn test_pdo_set_attribute_errmode_rejects_non_int_value() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->setAttribute(PDO::ATTR_ERRMODE, "banana");
    echo "no-throw";
} catch (\TypeError $e) {
    echo "threw:" . $e->getMessage();
}
echo "|" . $db->getAttribute(PDO::ATTR_ERRMODE);
try {
    $db->query("THIS IS NOT SQL");
    echo "|swallowed";
} catch (PDOException $e) {
    echo "|still-throws";
}
"#,
    );
    assert_eq!(
        out,
        "threw:Attribute value must be of type int for selected attribute, string given|2|still-throws"
    );
}

/// F-CORE-03, bool half: php-src's `pdo_get_bool_param()` accepts only
/// `IS_TRUE`/`IS_FALSE`/`IS_LONG` (its `case IS_STRING:` deliberately falls through
/// to the TypeError), so an array — or any other shape — passed to a bool-typed
/// attribute like `ATTR_STRINGIFY_FETCHES` is a `TypeError`, not a `(bool)` cast.
#[test]
fn test_pdo_set_attribute_bool_attribute_rejects_non_bool_value() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->setAttribute(PDO::ATTR_STRINGIFY_FETCHES, []);
    echo "no-throw";
} catch (\TypeError $e) {
    echo "threw:" . $e->getMessage();
}
echo "|" . ($db->getAttribute(PDO::ATTR_STRINGIFY_FETCHES) ? "1" : "0");
"#,
    );
    assert_eq!(
        out,
        "threw:Attribute value must be of type bool for selected attribute, array given|0"
    );
}

/// F-CORE-03 regression guard: the new shape check must not narrow what php-src
/// ACCEPTS. `pdo_get_long_param()` takes an int, a bool, and a string that
/// `is_numeric_str_function()` reports as `IS_LONG` — so a genuinely-int error mode
/// and the numeric string `"2"` both still set it, and an int still satisfies a
/// bool-typed attribute (php-src's `IS_LONG` case for `pdo_get_bool_param()`).
#[test]
fn test_pdo_set_attribute_accepts_int_and_numeric_string() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
echo $db->getAttribute(PDO::ATTR_ERRMODE);
$db->setAttribute(PDO::ATTR_ERRMODE, "2");
echo ":" . $db->getAttribute(PDO::ATTR_ERRMODE);
$db->setAttribute(PDO::ATTR_STRINGIFY_FETCHES, 1);
echo ":" . ($db->getAttribute(PDO::ATTR_STRINGIFY_FETCHES) ? "1" : "0");
"#,
    );
    assert_eq!(out, "0:2:1");
}

/// F-CORE-03, constructor half: php-src runs the constructor's `$options` array
/// through the very same `pdo_get_long_param()`/`pdo_get_bool_param()` helpers as
/// `setAttribute()`, and elephc's `$options` loop had the identical blind-cast hole —
/// so `new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => "banana"])` used to open the
/// connection in ERRMODE_SILENT. The TypeError must be raised before the connection
/// is opened, so the object is never handed back at all.
#[test]
fn test_pdo_constructor_options_reject_bad_attribute_shape() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => "banana"]);
    echo "no-throw";
} catch (\TypeError $e) {
    echo "int:" . $e->getMessage();
}
try {
    $db2 = new PDO("sqlite::memory:", null, null, [PDO::ATTR_STRINGIFY_FETCHES => "yes"]);
    echo "|no-throw";
} catch (\TypeError $e) {
    echo "|bool:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "int:Attribute value must be of type int for selected attribute, string given|bool:Attribute value must be of type bool for selected attribute, string given"
    );
}

/// F-SQLT-01: php-src's `pdo_sqlite` registers its driver constants against the BASE
/// `PDO` class (`pdo_dbh_ce`) in parallel with the class-scoped `Pdo\Sqlite::*`
/// spellings added in 8.1 — `PDO::SQLITE_ATTR_OPEN_FLAGS` and friends are the
/// pre-8.1 API surface a great deal of real-world code still uses, and elephc had
/// none of them. The two spellings are aliases: same value, both live.
#[test]
fn test_pdo_legacy_sqlite_constants_alias_pdo_sqlite() {
    let out = compile_and_run(
        r#"<?php
echo (PDO::SQLITE_DETERMINISTIC === \Pdo\Sqlite::DETERMINISTIC ? "1" : "0")
    . (PDO::SQLITE_ATTR_OPEN_FLAGS === \Pdo\Sqlite::ATTR_OPEN_FLAGS ? "1" : "0")
    . (PDO::SQLITE_OPEN_READONLY === \Pdo\Sqlite::OPEN_READONLY ? "1" : "0")
    . (PDO::SQLITE_OPEN_READWRITE === \Pdo\Sqlite::OPEN_READWRITE ? "1" : "0")
    . (PDO::SQLITE_OPEN_CREATE === \Pdo\Sqlite::OPEN_CREATE ? "1" : "0")
    . (PDO::SQLITE_ATTR_READONLY_STATEMENT === \Pdo\Sqlite::ATTR_READONLY_STATEMENT ? "1" : "0")
    . (PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES === \Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES ? "1" : "0");
echo ":" . PDO::SQLITE_DETERMINISTIC . "," . PDO::SQLITE_ATTR_OPEN_FLAGS . "," . PDO::SQLITE_OPEN_READONLY
    . "," . PDO::SQLITE_OPEN_READWRITE . "," . PDO::SQLITE_OPEN_CREATE . "," . PDO::SQLITE_ATTR_READONLY_STATEMENT
    . "," . PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES;
"#,
    );
    assert_eq!(out, "1111111:2048,1000,1,2,4,1001,1002");
}

/// F-STMT-05: php-src's `PHP_METHOD(PDOStatement, bindValue)` validates Argument #1
/// BEFORE recording anything — a positional slot below 1 is a `ValueError` ("must be
/// greater than or equal to 1") and an empty named placeholder is
/// `zend_argument_must_not_be_empty_error(1)`. elephc used to cast blindly and report
/// success for all three, so `bindValue(0, 'x')` bound nothing and said it worked.
#[test]
fn test_pdo_bind_value_rejects_invalid_parameter() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT ?");
try {
    $stmt->bindValue(0, "x");
    echo "no-throw";
} catch (\ValueError $e) {
    echo "zero:" . $e->getMessage();
}
try {
    $stmt->bindValue(-1, "x");
    echo "|no-throw";
} catch (\ValueError $e) {
    echo "|neg";
}
try {
    $stmt->bindValue("", "x");
    echo "|no-throw";
} catch (\ValueError $e) {
    echo "|empty:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "zero:PDOStatement::bindValue(): Argument #1 ($param) must be greater than or equal to 1|neg|empty:PDOStatement::bindValue(): Argument #1 ($param) must not be empty"
    );
}

/// F-STMT-05, sibling methods: php-src validates `bindParam()`'s and `bindColumn()`'s
/// own Argument #1 with the same two checks. `bindParam()` must raise under its OWN
/// name (not the `bindValue()` it delegates to), and `bindColumn()`'s parameter
/// validation runs ahead of registration, so malformed keys raise the PHP-matching
/// ValueError while a valid named output column is accepted.
#[test]
fn test_pdo_bind_param_and_bind_column_validate_argument_one() {
    let out = compile_and_run(
        r#"<?php
function run(mixed $col = null): void {
    $db = new PDO("sqlite::memory:");
    $stmt = $db->prepare("SELECT ?");
    $v = "x";
    try {
        $stmt->bindParam(0, $v);
        echo "no-throw";
    } catch (\ValueError $e) {
        echo "param:" . $e->getMessage();
    }
    try {
        $stmt->bindColumn(0, $col);
        echo "|no-throw";
    } catch (\ValueError $e) {
        echo "|col-zero";
    }
    try {
        $stmt->bindColumn("", $col);
        echo "|no-throw";
    } catch (\ValueError $e) {
        echo "|col-empty";
    }
    try {
        $stmt->bindColumn("c", $col);
        echo "|col-supported";
    } catch (PDOException $e) {
        echo "|unexpected";
    }
}
run();
"#,
    );
    assert_eq!(
        out,
        "param:PDOStatement::bindParam(): Argument #1 ($param) must be greater than or equal to 1|col-zero|col-empty|col-supported"
    );
}

/// F-STMT-06: a named placeholder the prepared SQL never declares resolves to bind
/// index `0` (`sqlite3_bind_parameter_index()`'s "unknown" answer), and neither
/// `execute()` replay loop used to check it — the value simply vanished while
/// `execute()` reported success. php-src raises HY093 "Invalid parameter number:
/// parameter was not defined" instead. Both binding paths are covered: the recorded
/// `bindValue()` replay, and the `execute($params)` array with an unknown key.
/// `errorInfo[0]` is what frameworks parse, so the SQLSTATE is asserted there.
#[test]
fn test_pdo_execute_unknown_named_placeholder_raises_hy093() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$a = $db->prepare("SELECT ?");
$a->bindValue(":nope", 1);
try {
    $a->execute();
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->errorInfo[0] . ":" . $e->getMessage();
}
$b = $db->prepare("SELECT ?");
try {
    $b->execute([":nope" => 1]);
    echo "|no-throw";
} catch (PDOException $e) {
    echo "|" . $e->errorInfo[0];
}
"#,
    );
    assert_eq!(
        out,
        "HY093:SQLSTATE[HY093]: Invalid parameter number: parameter was not defined|HY093"
    );
}

/// F-PARSE-06: every `elephc_pdo_bind_*` already returned `0` for an out-of-range
/// slot (SQLite's `SQLITE_RANGE`), but `execute()` checked no return code at all — so
/// `bindValue(5, ...)` on a 2-placeholder statement was a silent no-op and the
/// statement ran anyway with whatever the other slots held. php-src raises HY093
/// "Invalid parameter number". The row count pins the other half: the statement must
/// NOT have been executed. Under ERRMODE_SILENT the same failure returns `false`
/// rather than reporting a phantom success.
#[test]
fn test_pdo_execute_out_of_range_slot_raises_hy093() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b INTEGER)");
$stmt = $db->prepare("INSERT INTO t (a, b) VALUES (?, ?)");
$stmt->bindValue(1, 1);
$stmt->bindValue(2, 2);
$stmt->bindValue(5, "x");
try {
    $stmt->execute();
    echo "no-throw";
} catch (PDOException $e) {
    echo $e->errorInfo[0] . ":" . $e->getMessage();
}
echo "|" . $db->query("SELECT COUNT(*) FROM t")->fetchColumn();

$silent = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$silent->exec("CREATE TABLE t (a INTEGER, b INTEGER)");
$st = $silent->prepare("INSERT INTO t (a, b) VALUES (?, ?)");
$st->bindValue(5, "x");
echo "|" . (($st->execute() === false) ? "false" : "true");
"#,
    );
    assert_eq!(out, "HY093:SQLSTATE[HY093]: Invalid parameter number|0|false");
}

/// F-STMT-08: php-src ALWAYS reduces a bound type to its base type before
/// dispatching on it — `PDO_PARAM_TYPE(x)` is `((x) & ~PDO_PARAM_FLAGS)` with
/// `PDO_PARAM_FLAGS = 0xFFFF0000`, the high half where `PARAM_INPUT_OUTPUT` lives.
/// Dispatching on the RAW value made `PDO::PARAM_INT|PDO::PARAM_INPUT_OUTPUT` match
/// no branch and fall through to the generic TEXT one, binding an int as a string.
/// SQLite's `typeof()` reports the bound value's real storage class, so it catches
/// exactly that: an unmasked dispatch answers `text`, not `integer`. (A column with
/// INTEGER affinity would have silently coerced the string back and hidden the bug,
/// hence the bare `SELECT typeof(?)`.)
#[test]
fn test_pdo_bind_param_int_with_input_output_flag_binds_int() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT typeof(?) AS t, ? AS v");
$stmt->bindValue(1, 42, PDO::PARAM_INT | PDO::PARAM_INPUT_OUTPUT);
$stmt->bindValue(2, 42, PDO::PARAM_INT | PDO::PARAM_INPUT_OUTPUT);
$stmt->execute();
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo $row["t"] . ":" . $row["v"] . ":" . (is_int($row["v"]) ? "int" : "notint");
"#,
    );
    assert_eq!(out, "integer:42:int");
}

/// F-STMT-07: `PDO::PARAM_BOOL` now takes the driver's own boolean bind
/// (php-src's `PDO_PARAM_BOOL` case) instead of being folded into `PARAM_INT` —
/// which is what lets PostgreSQL send a real `'t'`/`'f'` for a BOOL column. SQLite
/// binds 0/1 either way, so the discriminator that proves the bool branch is taken
/// is php-src's `zval_is_true()` reduction: a bound `5` must arrive as `1`, whereas
/// the PARAM_INT branch would have bound `5` verbatim.
#[test]
fn test_pdo_bind_value_param_bool_reduces_to_truthiness() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->prepare("SELECT typeof(?) AS t, ? AS a, ? AS b, ? AS c");
$stmt->bindValue(1, true, PDO::PARAM_BOOL);
$stmt->bindValue(2, true, PDO::PARAM_BOOL);
$stmt->bindValue(3, false, PDO::PARAM_BOOL);
$stmt->bindValue(4, 5, PDO::PARAM_BOOL);
$stmt->execute();
$row = $stmt->fetch(PDO::FETCH_ASSOC);
echo $row["t"] . ":" . $row["a"] . ":" . $row["b"] . ":" . $row["c"];
"#,
    );
    assert_eq!(out, "integer:1:0:1");
}

/// F-QUAL-01: `columnValue()` — the single dispatch point every fetch path goes
/// through — used to copy a TEXT/BLOB value out of the bridge ONE BYTE AT A TIME
/// (`chr(elephc_pdo_column_data_byte(...))` in a loop: N FFI calls, each locking and
/// unlocking the bridge's statement table, plus N string concatenations, so an N-byte
/// column cost O(N) FFI calls and built its string in O(N²)). It now copies the value
/// in ONE call through `ptr_read_string(elephc_pdo_column_data_ptr(...), $_len)`.
///
/// The regression that rewrite risks is byte-exactness: the byte loop existed
/// precisely because embedded NUL bytes must survive (php-src's PDO hands back a
/// length-counted `zend_string`, never a C string — `pdo_stmt.c`'s `fetch_value()`
/// uses the driver's reported byte length). `column_data_ptr`/`column_data_len` are
/// the length-counted pair and `__rt_ptr_read_string` copies an EXACT byte count with
/// no NUL-termination semantics, so this must hold at a size where the old loop would
/// have been ruinous: a ~5 KB value with two embedded NULs, compared byte-for-byte.
#[test]
fn test_pdo_fetch_multi_kb_text_with_embedded_nuls_is_byte_exact() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (data TEXT)");
$big = str_repeat("A", 2000) . "\x00" . str_repeat("B", 2000) . "\x00" . str_repeat("C", 1000);
$ins = $db->prepare("INSERT INTO t (data) VALUES (?)");
$ins->bindValue(1, $big);
$ins->execute();
$back = (string) $db->query("SELECT data FROM t")->fetchColumn();
echo strlen($big) . ":" . strlen($back)
    . ":" . ((bin2hex($back) === bin2hex($big)) ? "same" : "diff")
    . ":" . ord($back[2000]) . ":" . ord($back[4001]) . ":" . ord($back[4002]);
"#,
    );
    assert_eq!(out, "5002:5002:same:0:0:67");
}

/// F-QUAL-01, the NULL-pointer edge the rewrite HAD to guard. The bridge's
/// `store_bytes` reports an EMPTY buffer as a NULL data pointer, and `ptr_read_string`
/// lowers to `__rt_ptr_check_nonnull` BEFORE it ever looks at the length — so an
/// unguarded `ptr_read_string(column_data_ptr(...), 0)` on an empty TEXT column or a
/// zero-length BLOB would hard-ABORT the process, where the old byte loop simply ran
/// zero iterations and yielded `""`. A regression here is a crash, not a wrong value,
/// which is why it gets its own fixture. php-src fetches both as an empty string
/// (`ATTR_ORACLE_NULLS` is `NULL_NATURAL` by default, so no ""→null conversion).
#[test]
fn test_pdo_fetch_empty_text_and_zero_length_blob() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (txt TEXT, bin BLOB)");
$db->exec("INSERT INTO t (txt, bin) VALUES ('', x'')");
$row = $db->query("SELECT txt, bin FROM t")->fetch(PDO::FETCH_ASSOC);
echo "[" . $row["txt"] . "]:" . strlen($row["txt"])
    . ":[" . $row["bin"] . "]:" . strlen($row["bin"]);
"#,
    );
    assert_eq!(out, "[]:0:[]:0");
}

/// F-QUAL-01, `blobStream()` half: bounded BLOB reads still copy each returned slice
/// through `elephc_pdo_blob_data_ptr()` in one `ptr_read_string`, never one FFI call
/// per byte. Two risks are pinned at BLOB scale: a ~3 KB body with an embedded NUL
/// must arrive byte-identical, and a ZERO-LENGTH blob (whose buffer the bridge reports
/// as a NULL pointer) must yield an empty stream rather than aborting on
/// `__rt_ptr_check_nonnull`. The zero-length read is a SUCCESS (0 bytes), which is a
/// different answer from the `false` a missing row returns — that distinction is
/// covered by `test_pdo_sqlite_open_blob`.
#[test]
fn test_pdo_sqlite_open_blob_multi_kb_and_zero_length() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE imgs (id INTEGER PRIMARY KEY, body BLOB)");
$big = str_repeat("x", 1500) . "\x00" . str_repeat("y", 1500);
$ins = $db->prepare("INSERT INTO imgs (id, body) VALUES (1, ?)");
$ins->bindValue(1, $big, PDO::PARAM_LOB);
$ins->execute();
$db->exec("INSERT INTO imgs (id, body) VALUES (2, x'')");

$s = $db->openBlob("imgs", "body", 1);
$content = (string) stream_get_contents($s);
$e = $db->openBlob("imgs", "body", 2);
$empty = (string) stream_get_contents($e);
echo strlen($content) . ":" . ((bin2hex($content) === bin2hex($big)) ? "same" : "diff")
    . ":" . ord($content[1500]) . "|" . strlen($empty);
"#,
    );
    assert_eq!(out, "3001:same:0|0");
}

/// F-SQLT-02: `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` (1002) used to be a
/// complete no-op. php-src's `pdo_sqlite_set_attribute`
/// calls `sqlite3_extended_result_codes(H->db, lval)`, which widens what
/// `sqlite3_errcode()` reports — and PDO surfaces that value verbatim as
/// `errorInfo[1]` — from the coarse primary code (`SQLITE_CONSTRAINT` = 19, "some
/// constraint broke") to the extended code naming WHICH constraint
/// (`SQLITE_CONSTRAINT_UNIQUE` = 2067). It is a live toggle, so turning it back off
/// restores the primary code.
///
/// `errorInfo[0]` deliberately degrades to "HY000" while the attribute is on, and that
/// is php-src parity, not a bug: `pdo_sqlite_error()` switches on the SAME unmasked
/// `sqlite3_errcode()` value, so its `case SQLITE_CONSTRAINT: → "23000"` no longer
/// matches once the code is 2067 and it falls through to its `default: → "HY000"`.
/// Masking back to the primary code would have DIVERGED from php-src, so the SQLSTATE
/// is pinned here alongside the native code to keep that trade-off honest.
#[test]
fn test_pdo_sqlite_extended_result_codes_widen_error_info() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$db->exec("CREATE TABLE t (id INTEGER PRIMARY KEY, u TEXT UNIQUE)");
$db->exec("INSERT INTO t (id, u) VALUES (1, 'a')");

$db->exec("INSERT INTO t (id, u) VALUES (2, 'a')");
$plain = $db->errorInfo();

$db->setAttribute(PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES, true);
$db->exec("INSERT INTO t (id, u) VALUES (3, 'a')");
$ext = $db->errorInfo();

$db->setAttribute(PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES, false);
$db->exec("INSERT INTO t (id, u) VALUES (4, 'a')");
$off = $db->errorInfo();

echo $plain[0] . "/" . $plain[1] . "|" . $ext[0] . "/" . $ext[1] . "|" . $off[0] . "/" . $off[1];
"#,
    );
    assert_eq!(out, "23000/19|HY000/2067|23000/19");
}

/// F-SQLT-02, shape check: php-src reads this attribute through `pdo_get_bool_param()`
/// (`zend_parse_arg_bool`), so a non-bool, non-int value is a TypeError, not a silent
/// truthiness cast. The new `setAttribute()` branch therefore routes 1002 through the
/// same `attrBoolValue()` helper `ATTR_STRINGIFY_FETCHES` uses, and
/// the message must match theirs byte for byte.
#[test]
fn test_pdo_sqlite_extended_result_codes_rejects_non_bool() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->setAttribute(PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES, "banana");
    echo "no-throw";
} catch (\TypeError $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Attribute value must be of type bool for selected attribute, string given"
    );
}

/// F-SQLT-02: `ATTR_EXTENDED_RESULT_CODES` is write-only in real PHP. The setter
/// succeeds, while the getter follows IM001 instead of echoing a retained value.
#[test]
fn test_pdo_sqlite_extended_result_codes_get_attribute_is_unsupported() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$ok = $db->setAttribute(PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES, true);
$after = $db->getAttribute(PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES);
echo ($ok ? "set" : "failed") . ":" . (($after === false) ? "unsupported" : "echoed");
"#,
    );
    assert_eq!(out, "set:unsupported");
}

/// F-CORE-15: php-src marks `class PDO` `/** @not-serializable */` in
/// `ext/pdo/pdo.stub.php`, which installs `zend_class_serialize_deny` as the class's
/// serialize handler, so `serialize($pdo)` throws
/// `Exception: Serialization of 'PDO' is not allowed` — a plain `Exception`, because
/// `zend_class_serialize_deny` passes a NULL class entry to `zend_throw_exception_ex`
/// (so NOT a `PDOException`, and NOT a `ValueError`).
///
/// elephc has no per-class engine flag for that, and its `serialize()` used to simply
/// WALK THE PROPERTIES: the blob it emitted carried `PDO::$conn`, the raw integer
/// bridge handle, and `unserialize()` handed back a zombie PDO whose handle indexes
/// nothing. The guard is therefore implemented in the prelude as a `__serialize()`
/// override that throws — elephc's `__rt_serialize_object` consults the per-class
/// `_class_serialize_ptrs` table before walking properties
/// (`src/codegen_support/runtime/data/user.rs:289`), so the magic method fires and the
/// throw unwinds out of the runtime's serialize frame.
#[test]
fn test_pdo_serialize_is_denied() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $blob = serialize($db);
    echo "no-throw:" . $blob;
} catch (\Exception $e) {
    // Read into a local first (see test_pdo_clone_throws): concatenating a literal
    // with a caught exception's getMessage() corrupts the output when the message
    // was itself built by concatenation at throw time, as this one is (get_class()).
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(out, "Serialization of 'PDO' is not allowed");
}

/// F-CORE-15: `PDOStatement` carries the same `/** @not-serializable */` annotation in
/// `ext/pdo/pdo.stub.php`, for the same reason (its blob would leak the bridge's
/// statement handle), and reports its own class name in the message.
#[test]
fn test_pdo_statement_serialize_is_denied() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stmt = $db->query("SELECT 1");
try {
    $blob = serialize($stmt);
    echo "no-throw:" . $blob;
} catch (\Exception $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(out, "Serialization of 'PDOStatement' is not allowed");
}

/// F-CORE-15: the deny guard names the RUNTIME class, not the class that declares
/// `__serialize()` — php-src's `zend_class_serialize_deny` formats `ZSTR_VAL(ce->name)`
/// of the object being serialized, and the driver subclasses inherit the deny handler
/// from `PDO`. The prelude's `get_class($this)` reproduces that, so a `Pdo\Sqlite`
/// reports its own name. (This also proves the guard is inherited at all: without it,
/// the subclass would fall through to the property walk that `PDO` no longer takes.)
#[test]
fn test_pdo_serialize_denied_reports_subclass_name() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
try {
    $blob = serialize($db);
    echo "no-throw:" . $blob;
} catch (\Exception $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(out, "Serialization of 'Pdo\\Sqlite' is not allowed");
}

/// F-CORE-01: php-src's `create_driver_specific_pdo_object` (`pdo_dbh.c:222-299`)
/// compares the DSN's driver against the driver-specific subclass being constructed and
/// refuses a mismatch, BEFORE any connection is attempted — so this needs no live
/// server. `Pdo\Mysql` had no constructor at all here, so `new Pdo\Mysql("sqlite:…")`
/// used to open a SQLite database behind a `Pdo\Mysql` object: an object whose class
/// lies about what it is, and whose MySQL-only methods then fail deep in the bridge.
/// The message is php-src's, byte for byte.
#[test]
fn test_pdo_mysql_subclass_ctor_rejects_sqlite_dsn() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new \Pdo\Mysql("sqlite::memory:");
    echo "no-throw";
} catch (\PDOException $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(
        out,
        "Pdo\\Mysql::__construct() cannot be used for connecting to the \"sqlite\" driver, \
         either call Pdo\\Sqlite::__construct() or PDO::__construct() instead"
    );
}

/// F-CORE-01, the mirror case: a `pgsql:` DSN handed to `Pdo\Sqlite`. The guard runs
/// ahead of `parent::__construct()`, so no PostgreSQL server is contacted (and none is
/// running in CI) — the throw is purely a DSN-prefix comparison.
#[test]
fn test_pdo_sqlite_subclass_ctor_rejects_pgsql_dsn() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new \Pdo\Sqlite("pgsql:host=localhost;dbname=nope");
    echo "no-throw";
} catch (\PDOException $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(
        out,
        "Pdo\\Sqlite::__construct() cannot be used for connecting to the \"pgsql\" driver, \
         either call Pdo\\Pgsql::__construct() or PDO::__construct() instead"
    );
}

/// F-CORE-01, the negative control the guard must not break: the CORRECT pairing still
/// connects and queries. Without this, a guard that rejected everything would pass both
/// tests above.
#[test]
fn test_pdo_sqlite_subclass_ctor_accepts_sqlite_dsn() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->exec("CREATE TABLE t (n INTEGER)");
$db->exec("INSERT INTO t VALUES (9)");
echo $db->query("SELECT n FROM t")->fetchColumn() . "|" . $db->getAttribute(PDO::ATTR_DRIVER_NAME);
"#,
    );
    assert_eq!(out, "9|sqlite");
}

/// F-CORE-04, **CORRECTED**. An earlier pass implemented the finalization spec's version
/// of this finding, and the spec was WRONG: `PDO_FINALIZATION_SPEC_V3.md:168` specifies a
/// LOUD rejection (exception under `ERRMODE_EXCEPTION`), and this test used to pin it.
///
/// Real PHP's `PDO::setAttribute()` on an unknown attribute returns **false SILENTLY**. It
/// raises NOTHING — no exception, no error state — not even under `ERRMODE_EXCEPTION`.
/// VERIFIED against a real PHP 8.5.6 CLI: `$pdo->setAttribute(9999, 1)` on an
/// `ERRMODE_EXCEPTION` handle returns `bool(false)` and `$pdo->errorCode()` still reads
/// `"00000"`.
///
/// WHY, in php-src's own terms: `pdo_dbh_attribute_set()` only reaches
/// `pdo_raise_impl_error(…, "IM001", "driver does not support setting attributes")` on the
/// `!dbh->methods->set_attribute` arm — a driver with NO `set_attribute` hook AT ALL. All
/// three drivers here (pdo_sqlite, pdo_mysql, pdo_pgsql) HAVE one, and each simply
/// `return 0`s for an attribute it does not recognize WITHOUT setting an error, so the
/// `PDO_HANDLE_DBH_ERR()` that follows finds SQLSTATE `00000` and raises nothing. The
/// IM001 arm is unreachable for every driver this bridge implements.
///
/// So all three are asserted together: the return is `false`, the error mode is
/// irrelevant (no throw under EXCEPTION), and `errorCode()` is untouched. What SURVIVES
/// from the original finding is that NOTHING IS STORED — store-and-return-true was wrong
/// under any reading. `getAttribute()`'s IM001 is genuinely asymmetric and still raises
/// (see below); that asymmetry looks like a php-src bug, but it is the behavior.
#[test]
fn test_pdo_set_attribute_unknown_returns_false_silently_under_exception() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
try {
    $ok = $db->setAttribute(99999, 1);
    echo (($ok === false) ? "false" : "true") . "|" . $db->errorCode();
} catch (\PDOException $e) {
    echo "threw:" . $e->getMessage();
}
"#,
    );
    assert_eq!(out, "false|00000");
}

/// F-CORE-04: under `ERRMODE_SILENT` the rejected attribute is likewise quiet and returns
/// `false` — which, after the correction above, is now the SAME behavior as under
/// EXCEPTION rather than the quiet half of an errMode-aware raise. It must also store
/// NOTHING — a rejected attribute that still landed in the bag would read back out of
/// `getAttribute()` and defeat the whole finding — which is why the read-back is asserted
/// here rather than trusted.
#[test]
fn test_pdo_set_attribute_unknown_returns_false_under_silent() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$ok = $db->setAttribute(99999, 1);
$back = $db->getAttribute(99999);
echo (($ok === false) ? "false" : "true") . "|" . (($back === false) ? "false" : "stored");
"#,
    );
    assert_eq!(out, "false|false");
}

/// F-CORE-05: `getAttribute()` on a number that is not a PDO attribute raises IM001
/// "driver does not support that attribute" and returns php-src's literal `RETURN_FALSE`
/// (not NULL). Unlike `setAttribute`'s IM001 this one IS exactly what real PHP does:
/// `pdo_sqlite_get_attribute` returning 0 lands on an explicit `pdo_raise_impl_error`
/// in `pdo_dbh.c`'s `case 0:` arm. elephc used to return NULL, indistinguishable from a
/// known attribute nobody had set.
#[test]
fn test_pdo_get_attribute_unknown_throws_im001_under_exception() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
try {
    $db->getAttribute(99999);
    echo "no-throw";
} catch (\PDOException $e) {
    $msg = $e->getMessage();
    $info = $e->errorInfo;
    echo $msg . "|" . $info[0];
}
"#,
    );
    assert_eq!(
        out,
        "SQLSTATE[IM001]: Driver does not support this function: driver does not support that attribute|IM001"
    );
}

/// F-CORE-05: under `ERRMODE_SILENT`, `getAttribute()` on an unsupported attribute is
/// quiet and yields `false`, whether the number names a generic PDO constant or is
/// completely unknown. Driver support, not membership in a numeric range, is decisive.
#[test]
fn test_pdo_get_attribute_unknown_returns_false_under_silent() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$unknown = $db->getAttribute(99999);
$unsupported = $db->getAttribute(PDO::ATTR_PREFETCH);
echo (($unknown === false) ? "false" : "other") . "|" . (($unsupported === false) ? "false" : "other");
"#,
    );
    assert_eq!(out, "false|false");
}

/// F-CORE-04/F-CORE-05: constants that the active SQLite driver does not implement are
/// rejected instead of being stored in a generic echo bag. This matches the SQLite
/// driver hook in php-src for cursor, emulated-prepare and default-string attributes.
#[test]
fn test_pdo_sqlite_rejects_unsupported_known_attributes() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT]);
$cursor = $db->setAttribute(PDO::ATTR_CURSOR, PDO::CURSOR_SCROLL);
$emulate = $db->setAttribute(PDO::ATTR_EMULATE_PREPARES, true);
$strParam = $db->setAttribute(PDO::ATTR_DEFAULT_STR_PARAM, PDO::PARAM_STR);
echo (($cursor === false) ? "rejected" : "stored") . "|"
    . (($emulate === false) ? "rejected" : "stored") . "|"
    . (($strParam === false) ? "rejected" : "stored");
"#,
    );
    assert_eq!(out, "rejected|rejected|rejected");
}

/// SQLite rejects a non-forward prepare-time cursor option by returning false,
/// even under exception mode, matching `sqlite_handle_preparer()`.
#[test]
fn test_pdo_sqlite_rejects_scroll_cursor_prepare_option() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
$stmt = $db->prepare("SELECT 1", [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
echo ($stmt === false) ? "rejected" : "accepted";
"#,
    );
    assert_eq!(out, "rejected");
}

/// A driver-specific constant from another driver is rejected by SQLite rather than
/// silently retained. The overlapping numeric ranges are interpreted only by the active
/// driver's hook, exactly as in php-src.
#[test]
fn test_pdo_sqlite_rejects_foreign_driver_attribute() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$stored = $db->setAttribute(Pdo\Mysql::ATTR_LOCAL_INFILE_DIRECTORY, "/var/lib/import");
echo ($stored ? "stored" : "rejected");
"#,
    );
    assert_eq!(out, "rejected");
}

/// F-CORE-11: php-src's `dsn_from_uri` (`pdo_dbh.c:208-220`, called from the constructor
/// at `pdo_dbh.c:346-358`) treats a `uri:` DSN as INDIRECT — it opens the referenced
/// stream and takes the real DSN from its FIRST LINE, so a credentials-bearing DSN can
/// live outside the source tree. elephc had no `uri:` handling at all, so such a DSN
/// reached the bridge verbatim and failed as an unknown driver. A file whose first line
/// is `sqlite::memory:` must therefore produce a working in-memory SQLite connection,
/// indistinguishable from `new PDO("sqlite::memory:")`.
///
/// The `file://` spelling is PHP's own documented one for this feature. elephc's
/// `fopen()` has no `file://` stream wrapper, so the prelude strips the scheme and opens
/// the remainder as a plain path — a divergence in mechanism that is invisible here,
/// which is exactly the point of pinning the documented spelling rather than the bare
/// path.
#[test]
fn test_pdo_uri_dsn_resolves_first_line_of_file() {
    let out = compile_and_run(
        r#"<?php
$path = tempnam(sys_get_temp_dir(), "elephc_pdo_uri_dsn_");
file_put_contents($path, "sqlite::memory:\n");
$db = new PDO("uri:file://" . $path);
$db->exec("CREATE TABLE t (n INTEGER)");
$db->exec("INSERT INTO t VALUES (13)");
echo $db->query("SELECT n FROM t")->fetchColumn() . "|" . $db->getAttribute(PDO::ATTR_DRIVER_NAME);
unlink($path);
"#,
    );
    assert_eq!(out, "13|sqlite");
}

/// F-CORE-11: a `uri:` DSN whose stream cannot be opened is an argument error, not a
/// connect failure — php-src's `dsn_from_uri` returns NULL and the constructor raises
/// `zend_argument_error(pdo_exception_ce, 1, "must be a valid data source URI")`. Note
/// the exception CLASS: `zend_argument_error`'s first parameter is the class entry, so
/// this is an argument-error MESSAGE SHAPE thrown as a **PDOException**, not a
/// `ValueError` (verified against a real PHP 8.5.6 CLI — reading only the
/// `zend_argument_*` call name gives the wrong class here).
#[test]
fn test_pdo_uri_dsn_unreadable_throws_argument_error_shape() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new PDO("uri:file:///nonexistent/elephc-pdo-no-such-dsn-file");
    echo "no-throw";
} catch (\PDOException $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(
        out,
        "PDO::__construct(): Argument #1 ($dsn) must be a valid data source URI"
    );
}

/// F-CORE-13: the constructor and the `PDO::connect()` factory now report an unknown
/// driver with ONE message, php-src's bare `"could not find driver"`. The constructor
/// used to let the bridge fail the open and surfaced ITS text — `"could not find driver
/// (only sqlite:, pgsql:, and mysql: DSNs are supported)"` — while `connect()` already
/// threw php-src's, so one failure had two messages inside one class. php-src
/// deliberately keeps the DSN (which may carry a password) out of that text; the helpful
/// driver list now lives in a code comment and in `docs/php/pdo.md`, not in a message
/// callers may match on.
///
/// Equality of the two messages is asserted directly, so this cannot drift back apart
/// silently.
#[test]
fn test_pdo_unknown_driver_message_identical_for_ctor_and_connect() {
    let out = compile_and_run(
        r#"<?php
$ctorMsg = "";
try {
    $db = new PDO("oracle:x");
} catch (\PDOException $e) {
    $ctorMsg = $e->getMessage();
}
$connectMsg = "";
try {
    $db2 = \PDO::connect("oracle:x");
} catch (\PDOException $e) {
    $connectMsg = $e->getMessage();
}
echo (($ctorMsg === $connectMsg) ? "same" : "diff") . "|" . $ctorMsg;
"#,
    );
    assert_eq!(out, "same|could not find driver");
}

/// F-CORE-13: a DSN with NO COLON AT ALL is a different failure with a different
/// message — php-src validates the colon first (`pdo_dbh.c:346-372`) and raises the
/// argument-error shape `"PDO::__construct(): Argument #1 ($dsn) must be a valid data
/// source name"`, reaching `"could not find driver"` only for a colon-prefixed DSN whose
/// driver is unregistered. elephc used to give a colonless DSN neither message.
///
/// The exception is a **PDOException**, NOT a `ValueError` — `zend_argument_error`'s
/// first argument is the exception class entry, and php-src passes `pdo_exception_ce`
/// (verified on a real PHP 8.5.6 CLI: `get_class($e)` on `new PDO("nocolon")` is
/// "PDOException"). This test would not compile-and-pass if the throw were a ValueError,
/// which is the point of catching the narrow class here.
#[test]
fn test_pdo_colonless_dsn_throws_argument_error_shape() {
    let out = compile_and_run(
        r#"<?php
try {
    $db = new PDO("nocolon");
    echo "no-throw";
} catch (\PDOException $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(
        out,
        "PDO::__construct(): Argument #1 ($dsn) must be a valid data source name"
    );
}

/// F-SQLT-05: php-src validates the extension name as an ARGUMENT, before any driver
/// dispatch — `pdo_sqlite.c:80-87` is `if (ZSTR_LEN(extension) == 0) {
/// zend_argument_must_not_be_empty_error(1); RETURN_THROWS(); }`, a `ValueError`. elephc
/// used to hand `""` straight to `sqlite3_load_extension()` and surface its failure as a
/// generic `PDOException`: the wrong exception class, raised at the wrong stage. No
/// extension file is touched by this test — the guard fires first, which is the finding.
#[test]
fn test_pdo_sqlite_load_extension_empty_name_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Sqlite("sqlite::memory:");
try {
    $db->loadExtension("");
    echo "no-throw";
} catch (\ValueError $e) {
    $msg = $e->getMessage();
    echo $msg;
}
"#,
    );
    assert_eq!(
        out,
        "Pdo\\Sqlite::loadExtension(): Argument #1 ($name) must not be empty"
    );
}

/// F-SURF-01: `ext/pdo/pdo.stub.php` declares a GLOBAL `pdo_drivers(): array` alongside
/// the class surface — the procedural spelling of `PDO::getAvailableDrivers()`, and the
/// one most capability probes still reach for (`in_array('pgsql', pdo_drivers(), true)`).
/// It was absent entirely, so such a probe failed to COMPILE rather than reporting the
/// drivers this build has. The two spellings must agree exactly (the prelude duplicates
/// the list rather than delegating, so this equality is the thing keeping them in
/// lockstep).
#[test]
fn test_pdo_drivers_function_matches_get_available_drivers() {
    let out = compile_and_run(
        r#"<?php
$procedural = pdo_drivers();
$static = PDO::getAvailableDrivers();
echo (($procedural === $static) ? "same" : "diff") . "|" . implode(",", $procedural);
"#,
    );
    assert_eq!(out, "same|mysql,pgsql,sqlite");
}

/// Verifies the prelude carries PHP 8.4's complete SQLSTATE descriptions rather
/// than only the generic states commonly produced by SQLite.
#[test]
fn test_pdo_sqlstate_description_table_includes_postgresql_states() {
    let out = compile_and_run(
        r#"<?php
$drivers = pdo_drivers();
echo __elephc_pdo_sqlstate_description("00000"), "|";
echo __elephc_pdo_sqlstate_description("23505"), "|";
echo __elephc_pdo_sqlstate_description("42P01"), "|";
echo __elephc_pdo_sqlstate_description("ZZZZZ");
"#,
    );
    assert_eq!(
        out,
        "No error|Unique violation|Undefined table|<<Unknown error>>"
    );
}

/// F-SURF-03: the 7 `PDO::PARAM_EVT_*` constants. Their values are the DECLARATION ORDER
/// of `enum pdo_param_event` in php-src's `ext/pdo/php_pdo_driver.h` (the enum carries no
/// explicit values, so the order is the only thing that fixes them): ALLOC=0, FREE=1,
/// EXEC_PRE=2, EXEC_POST=3, FETCH_PRE=4, FETCH_POST=5, NORMALIZE=6.
///
/// They back native PDO-DRIVER authorship — a driver's `param_hook` fires once per event
/// — and are entirely INERT in elephc, which implements the drivers natively in Rust and
/// exposes no param-hook seam to PHP. They exist so code that references them (portable
/// driver shims, suites enumerating the class surface) still compiles, which is exactly
/// what this test proves.
#[test]
fn test_pdo_param_evt_constants_present() {
    let out = compile_and_run(
        r#"<?php
echo PDO::PARAM_EVT_ALLOC . "," . PDO::PARAM_EVT_FREE . "," . PDO::PARAM_EVT_EXEC_PRE . "," . PDO::PARAM_EVT_EXEC_POST . "," . PDO::PARAM_EVT_FETCH_PRE . "," . PDO::PARAM_EVT_FETCH_POST . "," . PDO::PARAM_EVT_NORMALIZE;
"#,
    );
    assert_eq!(out, "0,1,2,3,4,5,6");
}

/// `ATTR_STATEMENT_CLASS` stores the connection default, constructs the selected
/// PDOStatement subclass without calling the base bridge constructor, initializes
/// queryString first, and then invokes an inherited private constructor with its args.
#[test]
fn test_pdo_statement_class_default_and_prepare_override() {
    let out = compile_and_run(
        r#"<?php
class PrivateStatement extends PDOStatement {
    public string $marker = "unset";
    private function __construct(string $marker) {
        $this->marker = $marker . ":" . $this->queryString;
        echo "ctor:" . $this->marker . "|";
    }
}
class InheritedStatement extends PrivateStatement {}

$db = new PDO("sqlite::memory:");
$initial = $db->getAttribute(PDO::ATTR_STATEMENT_CLASS);
echo $initial[0] . "|" . count($initial) . "|";
echo ($db->setAttribute(PDO::ATTR_STATEMENT_CLASS, [InheritedStatement::class, ["default"]]) ? "set" : "no") . "|";
$stored = $db->getAttribute(PDO::ATTR_STATEMENT_CLASS);
echo $stored[0] . "|" . $stored[1][0] . "|";
$first = $db->prepare("SELECT 1");
echo (($first instanceof InheritedStatement) ? "InheritedStatement" : "wrong") . "|";
$second = $db->prepare("SELECT 2", [PDO::ATTR_STATEMENT_CLASS => [PDOStatement::class]]);
echo (($second instanceof PDOStatement) ? "PDOStatement" : "wrong") . "|" . $second->queryString . "|";
echo $db->getAttribute(PDO::ATTR_STATEMENT_CLASS)[0];
"#,
    );
    assert_eq!(
        out,
        "PDOStatement|1|set|InheritedStatement|default|ctor:default:SELECT 1|InheritedStatement|PDOStatement|SELECT 2|InheritedStatement"
    );
}

/// `ATTR_STATEMENT_CLASS` rejects malformed values, unrelated classes, and public
/// constructors with php-src's distinct diagnostics instead of silently storing them.
#[test]
fn test_pdo_statement_class_validation_errors() {
    let out = compile_and_run(
        r#"<?php
class NotAStatement {}
class PublicStatement extends PDOStatement {
    public function __construct() {}
}
$db = new PDO("sqlite::memory:");
$cases = [
    "scalar" => "bad",
    "empty" => [],
    "null-class" => [null],
    "unknown" => ["NoSuchStatement"],
    "parent" => [NotAStatement::class],
    "public" => [PublicStatement::class],
    "args" => [PDOStatement::class, "bad"],
    "null-args" => [PDOStatement::class, null],
];
foreach ($cases as $name => $value) {
    try {
        $db->setAttribute(PDO::ATTR_STATEMENT_CLASS, $value);
        echo $name . ":none|";
    } catch (Throwable $e) {
        echo $name . ":" . get_class($e) . ":" . $e->getMessage() . "|";
    }
}
"#,
    );
    assert_eq!(
        out,
        "scalar:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS value must be of type array, string given|empty:ValueError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS value must be an array with the format array(classname, constructor_args)|null-class:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be a valid class|unknown:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be a valid class|parent:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement|public:TypeError:PDO::setAttribute(): Argument #2 ($value) User-supplied statement class cannot have a public constructor|args:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given|null-args:TypeError:PDO::setAttribute(): Argument #2 ($value) PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given|"
    );
}

/// Abstract statement classes are accepted as an attribute value but fail when prepare()
/// reaches instantiation, while constructor arguments for a class without a user
/// constructor fail with PDO's dedicated runtime Error.
#[test]
fn test_pdo_statement_class_instantiation_errors() {
    let out = compile_and_run(
        r#"<?php
abstract class AbstractStatement extends PDOStatement {}
class NoConstructorStatement extends PDOStatement {}
$db = new PDO("sqlite::memory:");
echo ($db->setAttribute(PDO::ATTR_STATEMENT_CLASS, [AbstractStatement::class]) ? "abstract-set" : "abstract-no") . "|";
try {
    $db->prepare("SELECT 1");
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . "|";
}
try {
    $db->prepare("SELECT 2", [PDO::ATTR_STATEMENT_CLASS => [NoConstructorStatement::class, []]]);
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "abstract-set|Error:Cannot instantiate abstract class AbstractStatement|Error:User-supplied statement does not accept constructor arguments"
    );
}

/// Persistent PDO handles reject a connection-level ATTR_STATEMENT_CLASS both through
/// setAttribute() and constructor options, while php-src still permits a prepare-local
/// override because it is not retained on the pooled connection.
#[test]
fn test_pdo_statement_class_persistent_connection_rules() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:", null, null, [PDO::ATTR_PERSISTENT => true]);
try {
    $db->setAttribute(PDO::ATTR_STATEMENT_CLASS, [PDOStatement::class]);
    echo "set:none|";
} catch (PDOException $e) {
    echo "set:" . $e->getMessage() . "|";
}
$stmt = $db->prepare("SELECT 1", [PDO::ATTR_STATEMENT_CLASS => [PDOStatement::class]]);
echo (($stmt instanceof PDOStatement) ? "local-ok" : "local-bad") . "|";
try {
    $other = new PDO("sqlite::memory:", null, null, [
        PDO::ATTR_STATEMENT_CLASS => [PDOStatement::class],
        PDO::ATTR_PERSISTENT => true,
    ]);
    echo "ctor:none";
} catch (PDOException $e) {
    echo "ctor:" . $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "set:SQLSTATE[HY000]: General error: PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances|local-ok|ctor:SQLSTATE[HY000]: General error: PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances"
    );
}
