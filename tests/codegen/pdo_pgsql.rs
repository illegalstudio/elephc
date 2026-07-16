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
//!   specifics: `$1`-placeholder translation (including the cast-run and
//!   dollar-quote-tag scanner rules), `SERIAL`/`lastInsertId`, bool/float/null type
//!   decoding, `PARAM_BOOL`'s real `'t'`/`'f'` bind, the full `getColumnMeta()` column
//!   description (type OID, table OID, raw `PQfsize`/`PQfmod`), the `COPY` methods, and
//!   the libpq `connect_timeout` default.

use crate::support::*;

/// Verifies PHP 8.4's nullable notice callback signature without requiring a live server.
#[test]
fn test_pdo_pgsql_notice_callback_accepts_null() {
    let out = compile_and_run(
        r#"<?php
function disable_notices(\Pdo\Pgsql $connection): void {
    $connection->setNoticeCallback(null);
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies every PHP 8.4 legacy pdo_pgsql method signature lowers without a live server.
#[test]
fn test_pdo_pgsql_legacy_method_signatures_compile() {
    let out = compile_and_run(
        r#"<?php
function exercise_legacy_pgsql(PDO $connection, int $guard): void {
    if ($guard < 0) {
        $connection->pgsqlCopyFromArray("items", ["1\tAda"]);
        $connection->pgsqlCopyFromFile("items", "input.tsv");
        $connection->pgsqlCopyToArray("items");
        $connection->pgsqlCopyToFile("items", "output.tsv");
        $oid = $connection->pgsqlLOBCreate();
        $connection->pgsqlLOBOpen((string) $oid);
        $connection->pgsqlLOBUnlink((string) $oid);
        $connection->pgsqlGetNotify(PDO::FETCH_ASSOC, 0);
        $connection->pgsqlGetPid();
    }
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

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

/// Generic connection-information attributes expose the linked client, live
/// libpq-equivalent status, and PostgreSQL session parameters.
#[test]
#[ignore]
fn test_pgsql_connection_information_attributes() {
    let out = compile_and_run(&pg_program(
        r#"
$client = (string) $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
$server = (string) $db->getAttribute(PDO::ATTR_SERVER_VERSION);
$info = (string) $db->getAttribute(PDO::ATTR_SERVER_INFO);
$status = (string) $db->getAttribute(PDO::ATTR_CONNECTION_STATUS);
echo (strpos($client, "postgres ") === 0 ? "client" : "bad-client") . "|";
echo (strlen($server) > 0 ? "server" : "bad-server") . "|";
echo (strpos($info, "PID: ") === 0 && strpos($info, "; Client Encoding: ") !== false ? "info" : "bad-info") . "|";
echo ($status === "Connection OK; waiting to send." ? "status" : "bad-status");
"#,
    ));
    assert_eq!(out, "client|server|info|status");
}

/// PostgreSQL scroll cursors honor every PDO fetch orientation, including
/// one-based/negative absolute positions and relative movement.
#[test]
#[ignore]
fn test_pgsql_scroll_cursor_orientations() {
    let out = compile_and_run(&pg_program(
        r#"
$stmt = $db->prepare("SELECT n FROM generate_series(1, 4) AS n", [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]);
$stmt->execute();
$first = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_FIRST);
$next = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_NEXT);
$last = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_LAST);
$prior = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_PRIOR);
$absolute = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_ABS, 2);
$relative = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_REL, 1);
$back = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_REL, -2);
$negative = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_ABS, -1);
$before = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_ABS, 0);
$restart = $stmt->fetch(PDO::FETCH_NUM, PDO::FETCH_ORI_NEXT);
echo $first[0] . $next[0] . $last[0] . $prior[0] . $absolute[0]
    . $relative[0] . $back[0] . $negative[0] . ($before === false ? "F" : "T") . $restart[0];
"#,
    ));
    assert_eq!(out, "12432314F1");
}

/// PostgreSQL exposes statement-owned result memory after execution, grows with
/// the result, and records HY000 while returning null before execution.
#[test]
#[ignore]
fn test_pgsql_result_memory_size_attribute() {
    let out = compile_and_run(&pg_program(
        r#"
$small = $db->query("SELECT 1")->getAttribute(Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE);
$large = $db->query("SELECT generate_series(1, 1000)")->getAttribute(Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE);
$pending = $db->prepare("SELECT 1");
$none = $pending->getAttribute(Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE);
$error = $pending->errorInfo();
echo (is_int($small) && $small > 0 ? "small" : "bad-small") . "|";
echo (is_int($large) && $large > $small ? "large" : "bad-large") . "|";
echo (is_null($none) ? "null" : "bad-null") . "|" . $error[0];
"#,
    ));
    assert_eq!(out, "small|large|null|HY000");
}

/// PostgreSQL `ATTR_PREFETCH=false` selects single-row/unbuffered semantics:
/// SELECT rowCount is not known up front and starting another query closes the
/// older cursor. A prepare-local true override remains buffered.
#[test]
#[ignore]
fn test_pgsql_prefetch_connection_and_statement_modes() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->setAttribute(PDO::ATTR_PREFETCH, false);
$streamed = $db->prepare("SELECT generate_series(1, 3) AS n");
$streamed->execute();
$first = $streamed->fetchColumn();
$rowCount = $streamed->rowCount();
$db->query("SELECT 99")->fetchColumn();
$closed = $streamed->fetch() === false ? "closed" : "bad";

$buffered = $db->prepare("SELECT generate_series(1, 2) AS n", [PDO::ATTR_PREFETCH => true]);
$buffered->execute();
$bufferedCount = $buffered->rowCount();
$db->query("SELECT 100")->fetchColumn();
$stillReadable = $buffered->fetchColumn();
echo $first . ":" . $rowCount . ":" . $closed . ":" . $bufferedCount . ":" . $stillReadable;
"#,
    );
    assert_eq!(out, "1:0:closed:2:1");
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

/// P2-j: a multi-statement `exec()` string (rejected by the single-command
/// `execute()` path, so it falls back to the simple-query protocol) returns the
/// LAST command's affected-row count, mirroring php-src's `PQexec` — not `0`,
/// which the old `batch_execute`-based fallback always reported.
#[test]
#[ignore]
fn test_pgsql_exec_multi_statement_returns_last_command_count() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_multi");
$db->exec("CREATE TABLE pg_multi (id INT)");
$n = $db->exec("INSERT INTO pg_multi VALUES (1); INSERT INTO pg_multi VALUES (2), (3);");
$db->exec("DROP TABLE pg_multi");
echo $n;
"#,
    ));
    assert_eq!(out, "2");
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

/// PostgreSQL's emulation flag selects the simple-query protocol, including SQL that
/// contains multiple commands and therefore cannot be server-side prepared as one unit.
#[test]
#[ignore]
fn test_pgsql_emulated_prepare_executes_multi_command_sql() {
    let out = compile_and_run(&pg_program(
        r#"
$enabled = $db->setAttribute(PDO::ATTR_EMULATE_PREPARES, true);
$stmt = $db->prepare("SELECT ? AS value; SELECT ? AS value");
$stmt->execute([3, 9]);
echo ($enabled ? "enabled" : "failed") . "|"
    . ($stmt->getAttribute(PDO::ATTR_EMULATE_PREPARES) ? "emulated" : "native") . "|"
    . $stmt->fetchColumn();
"#,
    ));
    assert_eq!(out, "enabled|emulated|9");
}

/// `Pdo\Pgsql::ATTR_DISABLE_PREPARES` independently selects execute-only simple-query
/// mode while the generic emulation attribute remains false.
#[test]
#[ignore]
fn test_pgsql_disable_prepares_executes_multi_command_sql() {
    let out = compile_and_run(&pg_program(
        r#"
$disabled = $db->setAttribute(Pdo\Pgsql::ATTR_DISABLE_PREPARES, true);
$stmt = $db->prepare("SELECT 11 AS value; SELECT 13 AS value");
$stmt->execute();
echo ($disabled ? "disabled" : "failed") . "|"
    . ($db->getAttribute(Pdo\Pgsql::ATTR_DISABLE_PREPARES) ? "simple" : "prepared") . "|"
    . $stmt->fetchColumn();
"#,
    ));
    assert_eq!(out, "disabled|simple|13");
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

/// F-CORE-18: on a FRESH connection (a new session — `lastval()` is
/// session-scoped, so this only holds because `pg_program` opens a new `PDO`
/// per test), `lastInsertId()` before any INSERT/`nextval()` fails — pg's
/// `lastval()` errors with SQLSTATE 55000 ("currval of sequence ... is not yet
/// defined in this session") rather than returning a fabricated `"0"` — and the
/// default EXCEPTION errmode surfaces that as a catchable `PDOException` whose
/// `errorInfo[0]` carries the real (non-success) SQLSTATE. A subsequent real
/// `SERIAL` insert still returns the real id, unaffected by the earlier failure.
#[test]
#[ignore]
fn test_pgsql_last_insert_id_no_sequence_throws() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_seq_fresh");
$db->exec("CREATE TABLE pg_seq_fresh (id SERIAL PRIMARY KEY, n INTEGER)");
$code = "no-throw";
try {
    $db->lastInsertId();
} catch (PDOException $e) {
    $code = $e->errorInfo[0];
}
$db->exec("INSERT INTO pg_seq_fresh (n) VALUES (42)");
$id = $db->lastInsertId();
$db->exec("DROP TABLE pg_seq_fresh");
echo (strlen($code) === 5 && $code !== "00000") ? "err-ok" : $code;
echo ":" . $id;
"#,
    ));
    assert_eq!(out, "err-ok:1");
}

/// Column types decode to PHP scalars: integer, double, native boolean, text, and
/// SQL NULL; bytea is exposed as the read stream returned by php-src.
#[test]
#[ignore]
fn test_pgsql_type_decoding() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_types");
$db->exec("CREATE TABLE pg_types (i INTEGER, d DOUBLE PRECISION, flag BOOLEAN, t TEXT, n TEXT, b BYTEA)");
$db->exec("INSERT INTO pg_types VALUES (42, 3.5, true, 'hi', NULL, decode('410042', 'hex'))");
$row = $db->query("SELECT i, d, flag, t, n, b FROM pg_types")->fetch(PDO::FETCH_ASSOC);
echo $row["i"] . "|" . $row["d"] . "|" . (is_bool($row["flag"]) ? "bool:" : "not-bool:") . ($row["flag"] ? "1" : "0")
    . "|" . $row["t"] . "|" . (is_null($row["n"]) ? "NULL" : "x")
    . "|" . (is_resource($row["b"]) ? bin2hex(stream_get_contents($row["b"])) : "not-resource");
$db->exec("DROP TABLE pg_types");
"#,
    ));
    assert_eq!(out, "42|3.5|bool:1|hi|NULL|410042");
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

/// A raw PostgreSQL BEGIN is visible to PDO::inTransaction() and the ordinary
/// PDO::commit() guard even though beginTransaction() bookkeeping was bypassed.
#[test]
#[ignore]
fn test_pgsql_raw_transaction_is_visible() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("BEGIN");
echo ($db->inTransaction() ? "in" : "out") . ":";
$db->commit();
echo ($db->inTransaction() ? "in" : "out");
"#,
    ));
    assert_eq!(out, "in:out");
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

/// `Pdo\Pgsql::getPid()` returns the live PostgreSQL backend process id (a positive
/// integer). Constructed as the driver subclass directly, since `getPid` is not on
/// the base `PDO`, and driven against the live server.
#[test]
#[ignore]
fn test_pgsql_get_pid() {
    let out = compile_and_run(
        "<?php\n$db = new \\Pdo\\Pgsql((string) getenv(\"ELEPHC_PG_DSN\"));\necho $db->getPid() > 0 ? \"pid-ok\" : \"pid-bad\";\n",
    );
    assert_eq!(out, "pid-ok");
}

/// Verifies PHP 8.4's legacy pdo_pgsql extension methods remain installed on a
/// base `PDO` connection and share the modern bridge behavior.
#[test]
#[ignore]
fn test_pgsql_legacy_driver_extension_methods() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_legacy");
$db->exec("CREATE TABLE pg_legacy (id INT, name TEXT)");
$copied = $db->pgsqlCopyFromArray("pg_legacy", ["1\tAda", "2\tBob"]);
$rows = $db->pgsqlCopyToArray("pg_legacy", "\t", "\\N", "id, name");
$pid = $db->pgsqlGetPid();
$none = $db->pgsqlGetNotify(PDO::FETCH_NUM, 0);
$db->beginTransaction();
$oid = $db->pgsqlLOBCreate();
$opened = $oid === false ? false : $db->pgsqlLOBOpen((string) $oid);
$unlinked = $oid === false ? false : $db->pgsqlLOBUnlink((string) $oid);
$db->rollBack();
$db->exec("DROP TABLE pg_legacy");
echo ($copied ? "copy" : "bad") . ":" . count($rows) . ":";
echo ($pid > 0 ? "pid" : "bad") . ":" . ($none === false ? "none" : "bad") . ":";
echo (($opened !== false && $unlinked) ? "lob" : "bad");
"#,
    ));
    assert_eq!(out, "copy:2:pid:none:lob");
}

/// `Pdo\Pgsql::lobCreate()` returns a new large object's OID (a numeric string) and
/// `lobUnlink()` deletes it, both driven against the live server.
#[test]
#[ignore]
fn test_pgsql_lob_create_unlink() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->beginTransaction();
$oid = $db->lobCreate();
$ok = ($oid !== false && is_numeric($oid)) ? "1" : "0";
$unlinked = $db->lobUnlink((string) $oid) ? "1" : "0";
$db->commit();
echo $ok . $unlinked;
"#,
    );
    assert_eq!(out, "11");
}

/// `Pdo\Pgsql::copyFromArray()` streams rows into a table via COPY FROM STDIN and
/// `copyToArray()` reads them back via COPY TO STDOUT (default tab/`\N` format).
///
/// `copyToArray()` is typed `array|false` (P2-i), so `$rows` must be narrowed before
/// `count()`/`implode()` accept it as an array. The checker's flow-sensitive guard
/// narrowing (`src/types/checker/stmt_check/narrowing.rs`) recognizes `is_array($rows)`
/// (alongside `is_bool`/`is_null`/`instanceof` and the `=== false`/`!== false` /
/// `=== null` comparisons), narrowing the union down to `array` for `count()`/`implode()`.
#[test]
#[ignore]
fn test_pgsql_copy_from_to_array() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("DROP TABLE IF EXISTS elephc_copy");
$db->exec("CREATE TABLE elephc_copy (id INT, name TEXT)");
$ok = $db->copyFromArray("elephc_copy", ["1\tAda", "2\tBob"]) ? "1" : "0";
$rows = $db->copyToArray("elephc_copy");
$db->exec("DROP TABLE elephc_copy");
if (is_array($rows)) {
    $joined = implode("", $rows);
    echo $ok . ":" . count($rows) . ":" . (strpos($joined, "Ada") !== false ? "y" : "n") . (strpos($joined, "Bob") !== false ? "y" : "n");
} else {
    echo $ok . ":err";
}
"#,
    );
    assert_eq!(out, "1:2:yy");
}

/// P2-i: `copyToArray()` distinguishes a genuinely empty table (`[]`) from a
/// failed COPY (`false`, widened from the old `array`-only return type) —
/// `COPY` against a nonexistent table fails at the server, and must not read
/// back as an empty result.
///
/// The empty-vs-array check uses `$empty === []` directly: the deep array strict-equality
/// helper (`__rt_array_strict_eq`) compares a `Union(Array, Bool)`-typed value (what
/// `array|false` lowers to) against an array literal by structure rather than heap-pointer
/// identity, so an empty result reads back as strictly equal to `[]` while a `false`
/// failure does not. `$error === false` compares the `false` alternative directly.
#[test]
#[ignore]
fn test_pgsql_copy_to_array_distinguishes_empty_from_error() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("DROP TABLE IF EXISTS elephc_copy_empty");
$db->exec("CREATE TABLE elephc_copy_empty (id INT)");
$empty = $db->copyToArray("elephc_copy_empty");
$db->exec("DROP TABLE elephc_copy_empty");
$error = $db->copyToArray("elephc_copy_does_not_exist");
echo ($empty === [] ? "empty-ok" : "empty-bad") . ":" . ($error === false ? "error-ok" : "error-bad");
"#,
    );
    assert_eq!(out, "empty-ok:error-ok");
}

/// `Pdo\Pgsql::getNotify()` receives a LISTEN/NOTIFY notification: the session
/// listens on a channel, notifies it, and getNotify returns [channel, pid, payload]
/// without truncating an embedded tab in the payload. Driven against the live server.
#[test]
#[ignore]
fn test_pgsql_get_notify() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("LISTEN elephc_ch");
$db->exec("SELECT pg_notify('elephc_ch', E'hi\\tthere')");
$n = $db->getNotify(\PDO::FETCH_NUM, 1000);
echo (count($n) === 0) ? "none" : ($n[0] . ":" . $n[2]);
"#,
    );
    assert_eq!(out, "elephc_ch:hi\tthere");
}

/// `Pdo\Pgsql::getNotify(PDO::FETCH_ASSOC)` shapes the notification as
/// `["message"=>channel, "pid"=>pid, "payload"=>payload]` instead of the default
/// numerically-indexed triple (P2-5). Driven against the live server.
#[test]
#[ignore]
fn test_pgsql_get_notify_assoc() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("LISTEN elephc_ch_assoc");
$db->exec("NOTIFY elephc_ch_assoc, 'hi'");
$n = $db->getNotify(\PDO::FETCH_ASSOC, 1000);
echo (count($n) === 0) ? "none" : ($n["message"] . ":" . $n["payload"] . ":" . ($n["pid"] > 0 ? "pid-ok" : "pid-bad"));
"#,
    );
    assert_eq!(out, "elephc_ch_assoc:hi:pid-ok");
}

/// P2-1: `PDO::ATTR_TIMEOUT` folds into the DSN as libpq's `connect_timeout`
/// conninfo key, so a connection attempt against an unreachable host fails
/// within a bounded time instead of hanging on the OS's own (much longer) TCP
/// connect timeout. Uses a non-routable TEST-NET-1 address (RFC 5737,
/// `192.0.2.0/24`) so the connect attempt reliably blackholes rather than
/// getting an immediate "connection refused". Driven without any live server
/// (the point is that the connection never completes).
#[test]
#[ignore]
fn test_pgsql_attr_timeout_fails_fast() {
    let out = compile_and_run(
        r#"<?php
$start = microtime(true);
try {
    $conn = new \Pdo\Pgsql("pgsql:host=192.0.2.1;port=5432;dbname=testdb", null, null, [PDO::ATTR_TIMEOUT => 2]);
    echo "connected";
} catch (PDOException $e) {
    $elapsed = microtime(true) - $start;
    echo ($elapsed < 10.0) ? "fast" : "slow";
}
"#,
    );
    assert_eq!(out, "fast");
}

/// `Pdo\Pgsql::lobOpen()` returns a transaction-scoped seekable stream. The live
/// fixture reads an existing object, overwrites and extends it (including a seek
/// beyond EOF), verifies the write through SQL, and rejects a nonexistent OID.
#[test]
#[ignore]
fn test_pgsql_lob_open() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->beginTransaction();
$oid = $db->query("SELECT lo_from_bytea(0, 'elephc-lo'::bytea)")->fetchColumn();
$s = $db->lobOpen((string) $oid, "w+b");
$content = stream_get_contents($s);
$seek = fseek($s, 7);
$written = fwrite($s, "LOB");
fseek($s, 12);
fwrite($s, "!");
$stored = $db->query("SELECT encode(lo_get(" . (string) $oid . "), 'hex')")->fetchColumn();
$missing = $db->lobOpen("999999999") === false ? "false" : "leak";
$db->lobUnlink((string) $oid);
$db->commit();
echo $content . ":" . $seek . ":" . $written . ":" . $stored . ":" . $missing;
"#,
    );
    assert_eq!(out, "elephc-lo:0:3:656c657068632d4c4f42000021:false");
}

/// Live TLS round-trip. Opens `ELEPHC_PG_TLS_DSN` — a DSN carrying `sslmode=require`
/// (or `sslmode=verify-full;sslrootcert=<ca.pem>`) against a TLS-enabled PostgreSQL —
/// and confirms a query returns over the encrypted rustls (ring) connection. The
/// default `tls` feature is compiled into the linked staticlib, so no extra build
/// flag is needed. `#[ignore]` — needs a TLS-serving PostgreSQL. Example:
///   # server.crt/server.key must be owned by the postgres uid inside the container
///   docker run -d --name pgtls -e POSTGRES_PASSWORD=test -e POSTGRES_USER=test \
///       -e POSTGRES_DB=testdb -p 55433:5432 -v "$PWD/certs":/certs postgres:16-alpine \
///       -c ssl=on -c ssl_cert_file=/certs/server.crt -c ssl_key_file=/certs/server.key
///   ELEPHC_PG_TLS_DSN='pgsql:host=localhost;port=55433;dbname=testdb;user=test;password=test;sslmode=require' \
///       cargo test --test codegen_tests -- --ignored pgsql_tls_round_trip
#[test]
#[ignore]
fn pgsql_tls_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO((string) getenv("ELEPHC_PG_TLS_DSN"));
echo $db->query("SELECT 'tls-ok'")->fetchColumn();
"#,
    );
    assert_eq!(out, "tls-ok");
}

/// P2-k: `getColumnMeta()` on a `pgsql:` statement reports the column's REAL
/// PostgreSQL type — the server native_type name (`int4`/`bool`/`bytea`/`text`),
/// the correct pdo_type (INT4→PARAM_INT=1, BOOL→PARAM_BOOL=5, BYTEA→PARAM_LOB=3,
/// TEXT→PARAM_STR=2), and the `pgsql:oid` type OID (23/16/17/25) — instead of the
/// generic SQLite storage-class metadata elephc emitted for every driver before
/// ABI v23. The name/OID are threaded from the prepared statement's
/// `postgres::types::Type`, so they describe the DECLARED column type even though
/// the table is empty (no row is fetched here).
#[test]
#[ignore]
fn test_pgsql_get_column_meta_native_types() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_meta");
$db->exec("CREATE TABLE pg_meta (id INT4, flag BOOL, payload BYTEA, label TEXT)");
$stmt = $db->query("SELECT id, flag, payload, label FROM pg_meta");
$parts = [];
for ($i = 0; $i < 4; $i++) {
    $m = $stmt->getColumnMeta($i);
    $parts[] = $m["native_type"] . ":" . $m["pdo_type"] . ":" . $m["pgsql:oid"];
}
echo implode(",", $parts);
$db->exec("DROP TABLE pg_meta");
"#,
    ));
    assert_eq!(out, "int4:1:23,bool:5:16,bytea:3:17,text:2:25");
}

/// v1 §6 gap: `Pdo\Pgsql::setNoticeCallback()` end-to-end. Registers a callback,
/// then runs a `DROP TABLE IF EXISTS` on a missing table — which the server
/// answers with a `NOTICE: ... does not exist, skipping` — and confirms the
/// buffered notice is drained and dispatched to the callback right after the
/// exec() (delivery is poll-based, not fired mid-protocol). Uses a plain
/// `DROP ... IF EXISTS` rather than a `DO $$ RAISE NOTICE $$` block so the fixture
/// needs no dollar-quoting. The callback asserts inside itself (echoing "got" on a
/// substring match, tolerant of libpq's severity prefix / trailing whitespace)
/// rather than accumulating into a `use (&$var)` by-reference capture: a by-ref
/// capture stored on the statement's callback property and invoked from another
/// method (drainNotices) trips a separate, pre-existing closure limitation, which
/// is orthogonal to whether the notice is delivered.
#[test]
#[ignore]
fn test_pgsql_set_notice_callback_e2e() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->setNoticeCallback(function($msg) { echo str_contains($msg, "does not exist") ? "got" : ("other:" . $msg); });
$db->exec("DROP TABLE IF EXISTS pg_notice_probe_zzz");
"#,
    );
    assert_eq!(out, "got");
}

/// P2-a end-to-end: preparing a statement that mixes a positional `?` with a
/// named `:name` placeholder is rejected with SQLSTATE HY093 BEFORE the server is
/// asked to prepare it (from the placeholder scanner's `mixed` flag, whose unit
/// is `pg_translate_placeholders_mixed_flag`). Handles both dispositions — under
/// the default EXCEPTION errmode prepare() throws a PDOException carrying
/// errorInfo[0] = "HY093"; a silent errmode would instead return false with the
/// same SQLSTATE on the connection.
#[test]
#[ignore]
fn test_pgsql_mixed_placeholder_styles_reject_hy093() {
    let out = compile_and_run(&pg_program(
        r#"
try {
    $r = $db->prepare("SELECT * FROM (VALUES (1)) v WHERE ? = :a");
    echo ($r === false) ? $db->errorInfo()[0] : "no-error";
} catch (\PDOException $e) {
    echo $e->errorInfo[0];
}
"#,
    ));
    assert_eq!(out, "HY093");
}

/// F-PG-01 / F-PG-02 (v26): `getColumnMeta()` on a `pgsql:` statement now reports the
/// three fields it used to hardcode — `pgsql:table_oid` (PQftable), `len` (PQfsize) and
/// `precision` (PQfmod) — and reports them RAW, exactly as php-src's
/// `pgsql_stmt_describe` copies them off the wire
/// (`ext/pdo_pgsql/pgsql_statement.c:496-497` for len/precision; the `pgsql:table_oid`
/// key is added unconditionally in `pgsql_stmt_get_column_meta`).
///
/// Three counter-intuitive semantics are pinned here on purpose, because each one is a
/// place a well-meaning "fix" would silently diverge from real PDO:
///
/// * `pgsql:table_oid` is present on EVERY column, `0` included. `0` is `InvalidOid`,
///   the server's own answer for a column that is not a plain table column — here the
///   computed `id + 1` expression. php-src emits the key with no test at all, so
///   suppressing it on `0` would break `isset($meta['pgsql:table_oid'])` on exactly the
///   columns a caller is most likely to probe. A real table column reports the table's
///   `pg_class` OID, which is non-zero (it is assigned per-database, so only its sign
///   can be asserted).
/// * `len` is the TYPE's byte width when it has a fixed one and `-1` for any varlena.
///   `int4` reports 4; `VARCHAR(20)` reports **-1**, NOT 20; `NUMERIC(10,2)` reports
///   -1 too. Both are varlena types, and `PQfsize()` is `pg_type.typlen`.
/// * `precision` is the RAW `atttypmod`, undecoded. `VARCHAR(20)`'s declared 20 surfaces
///   HERE, as **24** (20 + `VARHDRSZ`), and `NUMERIC(10,2)` as **655366**
///   (`((10 << 16) | 2) + 4`). A type carrying no modifier (`int4`) reports -1. The
///   values were read back off a live pg16 with
///   `SELECT atttypmod FROM pg_attribute …` and match byte for byte.
#[test]
#[ignore]
fn test_pgsql_get_column_meta_table_oid_len_precision() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_meta_full");
$db->exec("CREATE TABLE pg_meta_full (id INT4, label VARCHAR(20), money NUMERIC(10,2))");
$stmt = $db->query("SELECT id, label, money, id + 1 AS expr FROM pg_meta_full");
$id = $stmt->getColumnMeta(0);
$label = $stmt->getColumnMeta(1);
$money = $stmt->getColumnMeta(2);
$expr = $stmt->getColumnMeta(3);
$db->exec("DROP TABLE pg_meta_full");
echo ((((int) $id["pgsql:table_oid"]) > 0) ? "tbl-y" : "tbl-n")
    . ":" . $id["table"] . ":" . (isset($expr["table"]) ? "expr-table-y" : "expr-table-n")
    . ":" . (isset($expr["pgsql:table_oid"]) ? "key-y" : "key-n")
    . ":" . $expr["pgsql:table_oid"]
    . "|" . $id["len"] . "," . $label["len"] . "," . $money["len"]
    . "|" . $id["precision"] . "," . $label["precision"] . "," . $money["precision"];
"#,
    ));
    assert_eq!(
        out,
        "tbl-y:pg_meta_full:expr-table-n:key-y:0|4,-1,-1|-1,24,655366"
    );
}

/// F-PG-04 (Wave 1): an `oid` column is `PDO::PARAM_LOB` (3), not `PDO::PARAM_INT`.
/// php-src's pdo_pgsql type switch pairs the two cases literally —
/// `case OIDOID: case BYTEAOID:` (`ext/pdo_pgsql/pgsql_statement.c:690-706`) — because
/// to pdo_pgsql an OID is a large-object HANDLE, not an integer value: it is what you
/// feed to `lobOpen()`. Grouping it with INT2/INT4/INT8 (as elephc did before Wave 1)
/// made `getColumnMeta()` advertise a LOB handle as a plain integer.
///
/// `len` is asserted alongside as 4 — `oid` is one of PostgreSQL's fixed-width 4-byte
/// types (`pg_type.typlen` = 4), so it does NOT take the varlena `-1` that `bytea`, its
/// partner in that same switch arm, reports.
#[test]
#[ignore]
fn test_pgsql_get_column_meta_oid_column_is_param_lob() {
    let out = compile_and_run(&pg_program(
        r#"
$m = $db->query("SELECT '1'::oid AS o")->getColumnMeta(0);
echo $m["native_type"] . ":" . $m["pdo_type"] . ":" . $m["pgsql:oid"] . ":" . $m["len"];
"#,
    ));
    assert_eq!(out, "oid:3:26:4");
}

/// F-PG-05: a MULTI-character COPY separator is TRUNCATED TO ITS FIRST BYTE and the
/// COPY succeeds. PostgreSQL's COPY grammar admits only a one-byte `DELIMITER`, and all
/// four of php-src's COPY builders dereference exactly one byte of the argument —
/// `(pg_delim_len ? *pg_delim : '\t')` (`ext/pdo_pgsql/pgsql_driver.c:654,773,882,973`) —
/// silently dropping the rest. elephc used to interpolate the WHOLE string, so
/// `copyFromArray(…, "::")` emitted `DELIMITER '::'` and the SERVER rejected the
/// statement, where real PHP quietly copies with `:`.
///
/// Round-tripped through `copyToArray()` with the same `"::"` so the truncation is
/// proved to be consistent in both directions: the rows go in split on `:` and come
/// back joined on `:`.
#[test]
#[ignore]
fn test_pgsql_copy_multi_char_separator_truncates_to_first_byte() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("DROP TABLE IF EXISTS pg_copy_delim");
$db->exec("CREATE TABLE pg_copy_delim (id INT, name TEXT)");
$ok = $db->copyFromArray("pg_copy_delim", ["1:Ada", "2:Bob"], "::") ? "1" : "0";
$rows = $db->copyToArray("pg_copy_delim", "::");
$db->exec("DROP TABLE pg_copy_delim");
if (is_array($rows)) {
    $joined = implode("", $rows);
    echo $ok . ":" . count($rows) . ":" . (strpos($joined, "1:Ada") !== false ? "y" : "n") . (strpos($joined, "2:Bob") !== false ? "y" : "n");
} else {
    echo $ok . ":err";
}
"#,
    );
    assert_eq!(out, "1:2:yy");
}

/// `Pdo\Pgsql::copyFromFile()` / `copyToFile()` round-trip through client-side files —
/// the two COPY methods no other fixture exercised at all. The file written by
/// `copyToFile()` must be byte-identical to the one `copyFromFile()` consumed (default
/// tab delimiter, `\N` NULL marker, trailing newline per row).
///
/// php-src's own 8.4 source builds `COPY … TO STDIN` for `copyToArray`/`copyToFile`
/// (`ext/pdo_pgsql/pgsql_driver.c:882,884,973,975`) — an INVALID direction in
/// PostgreSQL's COPY grammar, which only knows `FROM STDIN` and `TO STDOUT`. elephc
/// correctly emits `TO STDOUT`. That divergence is DELIBERATE and must NOT be "aligned"
/// with php-src: aligning it would make every `copyTo*` call fail at the server.
#[test]
#[ignore]
fn test_pgsql_copy_from_file_to_file_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Pgsql((string) getenv("ELEPHC_PG_DSN"));
$db->exec("DROP TABLE IF EXISTS pg_copy_file");
$db->exec("CREATE TABLE pg_copy_file (id INT, name TEXT)");
$src = tempnam(sys_get_temp_dir(), "elephc_pg_copy_in_");
file_put_contents($src, "1\tAda\n2\tBob\n");
$in = $db->copyFromFile("pg_copy_file", $src) ? "1" : "0";
$dst = tempnam(sys_get_temp_dir(), "elephc_pg_copy_out_");
$out = $db->copyToFile("pg_copy_file", $dst) ? "1" : "0";
$back = (string) file_get_contents($dst);
unlink($src);
unlink($dst);
$db->exec("DROP TABLE pg_copy_file");
echo $in . $out . ":" . (($back === "1\tAda\n2\tBob\n") ? "same" : "diff");
"#,
    );
    assert_eq!(out, "11:same");
}

/// F-PARSE-01 end-to-end, in the only two dispositions a live PostgreSQL admits.
///
/// The first half is the executable one: a CHAINED cast (`:v::int::text`, two separate
/// two-colon runs) prepares and executes with its named bind intact — the `::` runs are
/// emitted verbatim and only `:v` becomes `$1`.
///
/// The second half pins the odd-run rule, and it cannot be an "it executes" assertion:
/// PostgreSQL's lexer has no token for a 3+-colon run (`typecast` is exactly `"::"` and
/// a bare `:` is a self char legal only inside an array subscript), so `SELECT 1 :::c`
/// is a syntax error on a real server — verified against a live pg16, which answers
/// SQLSTATE **42601**. There is therefore NO valid SQL text carrying a 3+-colon run for
/// the placeholder scanner to translate, and "prepares and executes correctly" is
/// unattainable for one by construction.
///
/// What the fix changes is WHOSE error it is. php-src's `MULTICHAR = [:]{2,}`
/// (`pgsql_sql_parser.re:35`) is greedy, so the whole run is one verbatim text token.
/// elephc's scanner used to eat colons PAIRWISE, leaving the third colon of an odd run
/// to be re-scanned as a fresh `:c` — a named bind php-src never emits. Next to the `?`
/// in the same statement that phantom bind set the `mixed` flag, and elephc rejected the
/// statement ITSELF with HY093 before the server ever saw it. Post-fix no named bind is
/// allocated, the text reaches the server unchanged, and the server delivers its own
/// verdict. So `42601` (and specifically NOT `HY093`) is the assertion that discriminates
/// the fixed scanner from the broken one.
#[test]
#[ignore]
fn test_pgsql_multi_colon_run_no_phantom_bind() {
    let out = compile_and_run(&pg_program(
        r#"
$st = $db->prepare("SELECT :v::int::text AS a");
$st->execute([":v" => 41]);
$a = $st->fetchColumn();
$code = "";
try {
    $bad = $db->prepare("SELECT 1 :::c , ? FROM (VALUES (1)) v(x)");
    $code = ($bad === false) ? $db->errorInfo()[0] : "prepared";
} catch (\PDOException $e) {
    $code = $e->errorInfo[0];
}
echo $a . ":" . $code;
"#,
    ));
    assert_eq!(out, "41:42601");
}

/// F-PARSE-02 end-to-end: a dollar-quote TAG may carry non-ASCII bytes, so
/// `$café$ … $café$` is a real dollar-quoted string — php-src spells the classes
/// `DOLQ_START = [A-Za-z\200-\377_]` / `DOLQ_CONT = [A-Za-z\200-\377_0-9]`
/// (`pgsql_sql_parser.re:32-33`), matching PostgreSQL's own lexer.
///
/// Gating the tag on `is_ascii_alphabetic()` meant the quote never opened: the body fell
/// through to the ordinary scanner and the `?` INSIDE the string literal was rewritten
/// into a positional bind. That did two things at once, and both are pinned here — it
/// corrupted the SQL text the server received, and, alongside the real `:n` named bind,
/// it set the `mixed` flag and got the statement rejected with HY093 before the server
/// saw it. Post-fix the body is copied through untouched (the `?` survives as literal
/// text in the result) and `:n` is the statement's only parameter.
///
/// The SQL is single-quoted in PHP so `$café$` is not read as a variable interpolation.
#[test]
#[ignore]
fn test_pgsql_non_ascii_dollar_quote_tag_executes() {
    let out = compile_and_run(&pg_program(
        r#"
$st = $db->prepare('SELECT $café$a ? b$café$ AS dq, :n::text AS n');
$st->execute([":n" => "ok"]);
$row = $st->fetch(PDO::FETCH_ASSOC);
echo $row["dq"] . "|" . $row["n"];
"#,
    ));
    assert_eq!(out, "a ? b|ok");
}

/// F-PG-03: a connection with NO `PDO::ATTR_TIMEOUT` still fails in bounded time.
/// php-src's pgsql handle factory defaults libpq's `connect_timeout` to 30 s
/// (`ext/pdo_pgsql/pgsql_driver.c:1350,1373,1381`), and elephc now appends
/// `connect_timeout='30'` to the conninfo whenever the DSN supplies none — so a
/// blackholed address fails at ~30 s instead of waiting out the OS's own TCP connect
/// timeout (75 s+ on Linux, longer on macOS).
///
/// `10.255.255.1` is a non-routable RFC 1918 address that silently drops packets rather
/// than answering `ECONNREFUSED`, which is what makes this test measure the TIMEOUT and
/// not the round trip. The 45 s bound is the guard: it is comfortably above the 30 s
/// default (leaving room for DNS/TLS setup on a loaded CI box) and comfortably below
/// every OS default, so the assertion can only fail if the default went missing. The
/// libpq timeout itself is what keeps the fixture from hanging the suite — the test
/// cannot outlive it.
#[test]
#[ignore]
fn test_pgsql_default_connect_timeout_bounds_unreachable_host() {
    let out = compile_and_run(
        r#"<?php
$start = microtime(true);
try {
    $conn = new \Pdo\Pgsql("pgsql:host=10.255.255.1;port=5432;dbname=testdb;user=test;password=test");
    echo "connected";
} catch (PDOException $e) {
    $elapsed = microtime(true) - $start;
    echo ($elapsed < 45.0) ? "bounded" : "unbounded";
}
"#,
    );
    assert_eq!(out, "bounded");
}

/// F-STMT-07 (Wave 1) against a real `BOOL` column: `PDO::PARAM_BOOL` takes the driver's
/// own boolean bind (php-src's `PDO_PARAM_BOOL` case in `pdo_stmt.c`'s bind dispatch)
/// instead of being folded into `PARAM_INT`. PostgreSQL is the driver that needs it —
/// the dedicated `elephc_pdo_bind_bool` extern exists so pg can send a real `'t'`/`'f'`
/// for a `bool` parameter, where SQLite and MySQL are happy with 0/1.
///
/// Both polarities are round-tripped, and each is also used as a `WHERE flag = ?`
/// PREDICATE rather than only as an INSERT value: that is the half a stringified or
/// integer bind cannot fake, because the parameter's type is inferred as `bool` from the
/// comparison and the value has to arrive as a boolean to satisfy it.
#[test]
#[ignore]
fn test_pgsql_param_bool_round_trip() {
    let out = compile_and_run(&pg_program(
        r#"
$db->exec("DROP TABLE IF EXISTS pg_bool");
$db->exec("CREATE TABLE pg_bool (id INT, flag BOOL)");
$ins = $db->prepare("INSERT INTO pg_bool (id, flag) VALUES (?, ?)");
$ins->bindValue(1, 1, PDO::PARAM_INT);
$ins->bindValue(2, true, PDO::PARAM_BOOL);
$ins->execute();
$ins2 = $db->prepare("INSERT INTO pg_bool (id, flag) VALUES (?, ?)");
$ins2->bindValue(1, 2, PDO::PARAM_INT);
$ins2->bindValue(2, false, PDO::PARAM_BOOL);
$ins2->execute();
$selTrue = $db->prepare("SELECT id FROM pg_bool WHERE flag = ?");
$selTrue->bindValue(1, true, PDO::PARAM_BOOL);
$selTrue->execute();
$trueId = $selTrue->fetchColumn();
$selFalse = $db->prepare("SELECT id FROM pg_bool WHERE flag = ?");
$selFalse->bindValue(1, false, PDO::PARAM_BOOL);
$selFalse->execute();
$falseId = $selFalse->fetchColumn();
$db->exec("DROP TABLE pg_bool");
echo $trueId . ":" . $falseId;
"#,
    ));
    assert_eq!(out, "1:2");
}
