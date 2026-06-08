//! Purpose:
//! Integration tests for the PDO PostgreSQL driver. Each fixture compiles a PHP
//! program that drives a live PostgreSQL server through `PDO`/`PDOStatement` and
//! asserts the produced stdout.
//!
//! Called from:
//! - `cargo test` through Rust's test harness. These tests are `#[ignore]`d
//!   because, unlike the SQLite fixtures, they require a running PostgreSQL
//!   server. Run them opt-in with the DSN in the `ELEPHC_PG_DSN` environment
//!   variable (inherited by the compiled test binary), e.g.:
//!     docker run -d --name pg -e POSTGRES_PASSWORD=test -e POSTGRES_USER=test \
//!         -e POSTGRES_DB=testdb -p 55432:5432 postgres:16-alpine
//!     ELEPHC_PG_DSN='pgsql:host=localhost;port=55432;dbname=testdb;user=test;password=test' \
//!         cargo test --test codegen_tests -- --ignored pgsql
//!
//! Key details:
//! - Each fixture opens its connection from `getenv("ELEPHC_PG_DSN")` and uses
//!   `DROP TABLE IF EXISTS` on a fixture-specific table so reruns are idempotent.
//! - The same prelude drives both drivers; these tests exercise the PostgreSQL
//!   specifics: `$1`-placeholder translation, `SERIAL`/`lastInsertId`, and
//!   bool/float/null type decoding.

use crate::support::*;

/// Wraps a PHP body that opens `$db` from `ELEPHC_PG_DSN`, so each fixture only
/// writes the database logic under test.
fn pg_program(body: &str) -> String {
    // getenv() is typed string|false; the env var is always set when these
    // ignored tests run, so a string cast is safe and keeps the DSN a string.
    format!(
        "<?php\n$db = new PDO((string) getenv(\"ELEPHC_PG_DSN\"));\n{}\n",
        body
    )
}

/// Round-trip: create, insert through named placeholders, and read a row back
/// keyed by column name.
#[test]
#[ignore]
fn test_pgsql_named_bind_round_trip() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_rt");
$db->exec("CREATE TABLE pg_rt (id SERIAL PRIMARY KEY, name TEXT, score DOUBLE PRECISION)");
$ins = $db->prepare("INSERT INTO pg_rt (name, score) VALUES (:name, :score)");
$ins->execute([":name" => "Ada", ":score" => 9.5]);
$row = $db->query("SELECT id, name, score FROM pg_rt")->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"] . ":" . $row["score"];
$db->exec("DROP TABLE pg_rt");
"#,
    ));
    assert_eq!(out, "1:Ada:9.5");
}

/// Positional `?` placeholders translate to `$1, $2` and bind by position.
#[test]
#[ignore]
fn test_pgsql_positional_bind() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_pos");
$db->exec("CREATE TABLE pg_pos (a INTEGER, b TEXT)");
$ins = $db->prepare("INSERT INTO pg_pos (a, b) VALUES (?, ?)");
$ins->execute([7, "seven"]);
$sel = $db->prepare("SELECT b FROM pg_pos WHERE a = ?");
$sel->execute([7]);
echo $sel->fetchColumn();
$db->exec("DROP TABLE pg_pos");
"#,
    ));
    assert_eq!(out, "seven");
}

/// `SERIAL` columns drive `lastInsertId()` (via `lastval()`).
#[test]
#[ignore]
fn test_pgsql_last_insert_id() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_seq");
$db->exec("CREATE TABLE pg_seq (id SERIAL PRIMARY KEY, n INTEGER)");
$db->exec("INSERT INTO pg_seq (n) VALUES (10)");
$db->exec("INSERT INTO pg_seq (n) VALUES (20)");
echo $db->lastInsertId();
$db->exec("DROP TABLE pg_seq");
"#,
    ));
    assert_eq!(out, "2");
}

/// Column types decode to PHP scalars: integer, double, boolean (0/1), text, and
/// SQL NULL.
#[test]
#[ignore]
fn test_pgsql_type_decoding() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_types");
$db->exec("CREATE TABLE pg_types (i INTEGER, d DOUBLE PRECISION, flag BOOLEAN, t TEXT, n TEXT)");
$db->exec("INSERT INTO pg_types VALUES (42, 3.5, true, 'hi', NULL)");
$row = $db->query("SELECT i, d, flag, t, n FROM pg_types")->fetch(PDO::FETCH_ASSOC);
echo $row["i"] . "|" . $row["d"] . "|" . $row["flag"] . "|" . $row["t"] . "|" . (is_null($row["n"]) ? "NULL" : "x");
$db->exec("DROP TABLE pg_types");
"#,
    ));
    assert_eq!(out, "42|3.5|1|hi|NULL");
}

/// A `PDOStatement` is Traversable: `foreach` walks the result set in the current
/// fetch mode.
#[test]
#[ignore]
fn test_pgsql_foreach() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_iter");
$db->exec("CREATE TABLE pg_iter (id INTEGER, name TEXT)");
$db->exec("INSERT INTO pg_iter VALUES (1, 'a'), (2, 'b'), (3, 'c')");
$stmt = $db->query("SELECT id, name FROM pg_iter ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $k => $row) {
    echo $k . ":" . $row["id"] . "=" . $row["name"] . ";";
}
$db->exec("DROP TABLE pg_iter");
"#,
    ));
    assert_eq!(out, "0:1=a;1:2=b;2:3=c;");
}

/// Committed work persists; a rolled-back transaction does not.
#[test]
#[ignore]
fn test_pgsql_transactions() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_tx");
$db->exec("CREATE TABLE pg_tx (n INTEGER)");
$db->beginTransaction();
$db->exec("INSERT INTO pg_tx VALUES (1)");
$db->rollBack();
$db->beginTransaction();
$db->exec("INSERT INTO pg_tx VALUES (2)");
$db->commit();
echo $db->query("SELECT COUNT(*) FROM pg_tx")->fetchColumn() . ":" . $db->query("SELECT n FROM pg_tx")->fetchColumn();
$db->exec("DROP TABLE pg_tx");
"#,
    ));
    assert_eq!(out, "1:2");
}

/// Rich PostgreSQL types decode to their text representation: `numeric` keeps its
/// scale, date/time/timestamp use PostgreSQL's text format, `uuid` and `json`
/// round-trip, and a `numeric` value binds through a parameter (coerced from
/// text).
#[test]
#[ignore]
fn test_pgsql_rich_types() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_rich");
$db->exec("CREATE TABLE pg_rich (money NUMERIC(10,2), d DATE, ts TIMESTAMP, u UUID, j JSON)");
$ins = $db->prepare("INSERT INTO pg_rich VALUES (:m, :d, :ts, :u, :j)");
$ins->execute([
    ":m"  => "1234.50",
    ":d"  => "2024-01-15",
    ":ts" => "2024-01-15 10:30:00",
    ":u"  => "550e8400-e29b-41d4-a716-446655440000",
    ":j"  => '{"a":1}',
]);
$r = $db->query("SELECT money, d, ts, u, j FROM pg_rich")->fetch(PDO::FETCH_ASSOC);
echo $r["money"] . "|" . $r["d"] . "|" . $r["ts"] . "|" . $r["u"] . "|" . $r["j"];
$db->exec("DROP TABLE pg_rich");
"#,
    ));
    assert_eq!(
        out,
        "1234.50|2024-01-15|2024-01-15 10:30:00|550e8400-e29b-41d4-a716-446655440000|{\"a\":1}"
    );
}

/// The default exception error mode throws a catchable `PDOException` on bad SQL,
/// and `ERRMODE_SILENT` makes `exec()` return `false` instead.
#[test]
#[ignore]
fn test_pgsql_error_modes() {
    let out = compile_and_run(&pg_program(
        r#"
try {
    $db->exec("THIS IS NOT VALID SQL");
    echo "no";
} catch (PDOException $e) {
    echo "caught";
}
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
echo ":" . (($db->exec("ALSO BAD") === false) ? "false" : "other");
"#,
    ));
    assert_eq!(out, "caught:false");
}
