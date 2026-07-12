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
/// it cleared again once a raw `exec("COMMIT")` runs.
#[test]
fn test_pdo_in_transaction_reflects_raw_begin() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("BEGIN");
echo $db->inTransaction() ? "1" : "0";
$db->exec("COMMIT");
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

/// W6: the added PHP 8.4 constants resolve to their documented numeric values,
/// including the OR-able fetch flags (hex).
///
/// 8.5-READINESS: these FETCH_GROUP/UNIQUE values (0x10000/0x30000) are the PHP-8.4
/// values and are CORRECT for elephc's 8.4 target. php-src master (8.5+) renumbered
/// the OR-able fetch flags to single bits — GROUP=(1<<5)=32, UNIQUE=(1<<6)=64,
/// CLASSTYPE=(1<<7)=128, PROPS_LATE=(1<<8)=256, SERIALIZE=(1<<9)=512 — with the base
/// mask 0xFFFFFFF0. When elephc's PHP target moves to 8.5, renumber those five
/// constants in the prelude, switch the `$mode & 0xFFFF`/`& 0x10000`/`& 0x40000`/
/// `& 0x100000` base-mode masks accordingly, and update this test's expected string.
#[test]
fn test_pdo_constants_present() {
    let out = compile_and_run(
        r#"<?php
echo PDO::FETCH_KEY_PAIR . "," . PDO::FETCH_GROUP . "," . PDO::FETCH_UNIQUE . "," . PDO::ATTR_DEFAULT_FETCH_MODE . "," . PDO::ATTR_EMULATE_PREPARES . "," . PDO::CURSOR_SCROLL;
"#,
    );
    assert_eq!(out, "12,65536,196608,19,20,1");
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
        "threw:SQLSTATE[HY000]: PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns."
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

/// W4/W6: an unsupported base fetch mode fails loudly instead of returning wrong
/// data (FETCH_LAZY). P2: php-src's `pdo_stmt_verify_mode` raises every
/// mode-validation failure as a `ValueError`, not a `PDOException`.
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
} catch (ValueError $e) {
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
        "threw:SQLSTATE[IM001]: driver does not support multiple rowsets"
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
    . $meta1["name"] . ":" . $meta1["native_type"] . ":" . $meta1["sqlite:decl_type"] . "," . $bad;
"#,
    );
    assert_eq!(out, "id:integer:INTEGER,name:string:TEXT,F");
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
echo $meta["native_type"] . ":" . (isset($meta["sqlite:decl_type"]) ? "Y" : "N");
"#,
    );
    assert_eq!(out, "integer:N");
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

/// Pdo\Sqlite::openBlob reads a BLOB cell whole into a rewound php://memory stream
/// (the read-whole resource shape). The fixture stores a 3-byte BLOB with an embedded
/// NUL (`x'610062'` = "a\0b") directly through SQL so the read path is exercised
/// independently of parameter binding, then asserts the streamed bytes match
/// (NUL-preserving) and that opening a missing row returns false.
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

/// P1-6: the 3-arg `fetch($mode, $classOrObject, $cursorOffset)` form compiles and
/// runs; the cursor offset is accepted and ignored (the bridge's cursor is
/// forward-only, matching every driver here).
#[test]
fn test_pdo_fetch_three_arg_form() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");
$stmt = $db->query("SELECT id, name FROM t ORDER BY id");
$row = $stmt->fetch(PDO::FETCH_ASSOC, null, 0);
echo $row["name"];
"#,
    );
    assert_eq!(out, "Ada");
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

/// P1-6: `fetchAll(PDO::FETCH_CLASS, Row::class, [...])`'s 3-arg form compiles and
/// runs; the constructor-argument array is accepted but not forwarded, the same
/// documented divergence as `fetchObject()`'s `$constructorArgs`.
#[test]
fn test_pdo_fetch_all_class_with_ctor_args() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER, name TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'Ada'), (2, 'Bob')");
$rows = $db->query("SELECT id, name FROM t ORDER BY id")->fetchAll(PDO::FETCH_CLASS, Row::class, []);
echo count($rows) . ":" . $rows[0]->name . ":" . $rows[1]->name;
"#,
    );
    assert_eq!(out, "2:Ada:Bob");
}

/// P1-12: `getIterator()` returns the statement itself, which `foreach` can walk
/// (`PDOStatement` already `implements Iterator`).
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

/// P0-3: `fetchAll(PDO::FETCH_FUNC, $callback)` fails loudly (elephc cannot
/// invoke a callback threaded through the bounded `mixed $classOrObject` slot —
/// see fetchAll()'s comment) rather than silently returning garbage rows.
#[test]
fn test_pdo_fetch_func_on_fetch_all_throws() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (a INTEGER, b TEXT)");
$db->exec("INSERT INTO t VALUES (1, 'x')");
try {
    $db->query("SELECT a, b FROM t")->fetchAll(PDO::FETCH_FUNC, function ($a, $b) {
        return $a . ":" . $b;
    });
    echo "no-throw";
} catch (PDOException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
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

/// P0-4: `bindColumn()` exists (so real-world code calling it at least compiles)
/// but fails loudly — elephc cannot store a by-reference write-back target
/// (`$this->x = &$v;` does not parse), so accepting the call and silently doing
/// nothing was the alternative this slice rejects.
#[test]
fn test_pdo_bind_column_unsupported() {
    let out = compile_and_run(
        r#"<?php
function run(mixed $col = null): void {
    $db = new PDO("sqlite::memory:");
    $db->exec("CREATE TABLE t (a INTEGER)");
    $db->exec("INSERT INTO t VALUES (1)");
    $stmt = $db->query("SELECT a FROM t");
    try {
        $stmt->bindColumn(1, $col);
        echo "no-throw-bind";
    } catch (PDOException $e) {
        echo "threw-bind";
    }
}
run();
"#,
    );
    assert_eq!(out, "threw-bind");
}

/// P1-3: `fetch(PDO::FETCH_BOUND)` no longer fails loudly — real php-src's
/// `do_fetch` just advances the cursor and reports whether a row was available
/// (`RETVAL_TRUE` once `do_fetch_common` has stepped); bindColumn()'s
/// unimplemented write-back (see its own test) means there is nothing further
/// to do with no bound columns, so this is a plain advance-and-return-bool.
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
$row = $stmt->fetch(PDO::FETCH_CLASS | PDO::FETCH_PROPS_LATE, Row::class);
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

/// P1-1: `FETCH_CLASS | FETCH_CLASSTYPE` is likewise accepted — php-src's
/// `pdo_stmt_verify_mode` switches directly to the FETCH_CLASS case, skipping
/// the CLASSTYPE rejection check entirely for that base mode.
#[test]
fn test_pdo_fetch_class_with_classtype_flag_works() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
}

$db = new PDO("sqlite::memory:");
$db->exec("CREATE TABLE t (id INTEGER)");
$db->exec("INSERT INTO t (id) VALUES (1)");
$stmt = $db->query("SELECT id FROM t");
$row = $stmt->fetch(PDO::FETCH_CLASS | PDO::FETCH_CLASSTYPE, Row::class);
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id;
"#,
    );
    assert_eq!(out, "Row:1");
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

/// P2-13: `getAttribute(ATTR_CLIENT_VERSION)` and `getAttribute(ATTR_CONNECTION_STATUS)`
/// return real, non-null values instead of falling through to the generic
/// unknown-attribute `null`.
#[test]
fn test_pdo_get_attribute_client_version_and_connection_status() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
$clientVersion = $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
$connStatus = $db->getAttribute(PDO::ATTR_CONNECTION_STATUS);
echo ($clientVersion !== null && strlen((string) $clientVersion) > 0) ? "has-client-version" : "null-client-version";
echo ",";
echo ($connStatus !== null && strlen((string) $connStatus) > 0) ? "has-connection-status" : "null-connection-status";
"#,
    );
    assert_eq!(out, "has-client-version,has-connection-status");
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
