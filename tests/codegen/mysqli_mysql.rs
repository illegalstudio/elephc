//! Purpose:
//! Integration tests for the mysqli surface against a live MySQL / MariaDB
//! server. Each fixture compiles a PHP program that drives the server through
//! `mysqli` / `mysqli_result` / `mysqli_stmt` and asserts the produced stdout.
//!
//! Called from:
//! - `cargo test` through Rust's test harness. These tests are `#[ignore]`d
//!   because they require a running MySQL/MariaDB server. Run them opt-in with
//!   the DSN in the `ELEPHC_MY_DSN` environment variable (same variable as
//!   `pdo_mysql.rs`), e.g.:
//!     docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
//!         -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
//!         -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
//!     ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
//!         cargo test --test codegen_tests -- --ignored mysqli_mysql
//!
//! Key details:
//! - `my_program` parses the PDO-style `mysql:` DSN into `new mysqli(...)`
//!   arguments inside the PHP fixture, so CI keeps one env var for both PDO and
//!   mysqli live jobs.
//! - Fixtures use `DROP TABLE IF EXISTS` on fixture-specific tables so reruns
//!   are idempotent, and connect under `MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT`
//!   so a broken connection fails the test loudly.

use crate::support::*;

/// Wraps a PHP body with a header that parses `ELEPHC_MY_DSN` (a PDO-style
/// `mysql:` DSN) and opens `$db` as a `mysqli` connection, so each fixture only
/// writes the mysqli logic under test.
fn my_program(body: &str) -> String {
    format!(
        r#"<?php
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
$dsn = (string) getenv("ELEPHC_MY_DSN");
$host = "";
$port = 3306;
$dbname = "";
$user = "";
$pass = "";
$socket = "";
foreach (explode(";", substr($dsn, 6)) as $pair) {{
    $eq = strpos($pair, "=");
    if ($eq === false) {{
        continue;
    }}
    $key = substr($pair, 0, $eq);
    $value = substr($pair, $eq + 1);
    if ($key === "host") {{
        $host = $value;
    }} elseif ($key === "port") {{
        $port = (int) $value;
    }} elseif ($key === "dbname") {{
        $dbname = $value;
    }} elseif ($key === "user") {{
        $user = $value;
    }} elseif ($key === "password") {{
        $pass = $value;
    }} elseif ($key === "unix_socket") {{
        $socket = $value;
    }}
}}
if ($socket === "") {{
    $db = new mysqli($host, $user, $pass, $dbname, $port);
}} else {{
    $db = new mysqli($host, $user, $pass, $dbname, $port, $socket);
}}
{}
"#,
        body
    )
}

/// `real_escape_string` returns the escaped payload WITHOUT wrapping quotes,
/// with live `NO_BACKSLASH_ESCAPES` detection (default mode backslash-escapes).
#[test]
#[ignore]
fn test_mysqli_escape_and_roundtrip() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->real_escape_string("a'b\\c");
"#,
    ));
    assert_eq!(out, r"a\'b\\c");
}

/// SECURITY: charset-aware escaping is correct for the WHOLE dangerous family,
/// not just GBK — byte-for-byte against php's captured output for `BF 5C 27 41`.
/// A single lead/trail band would re-open the breakout on sjis/cp932/euckr/ujis;
/// and the charset is tracked live (a raw `SET NAMES sjis` is honored, so the
/// escape is never fooled into treating a multibyte session as utf8mb4).
#[test]
#[ignore]
fn test_mysqli_escape_is_charset_aware_per_family() {
    let out = compile_and_run(&my_program(
        r#"
$in = "\xBF\x5C\x27\x41";
$want = [
    "gbk" => "bf5c5c2741",
    "sjis" => "bf5c5c5c2741",
    "cp932" => "bf5c5c5c2741",
    "euckr" => "5cbf5c5c5c2741",
    "ujis" => "5cbf5c5c5c2741",
    "gb2312" => "5cbf5c5c5c2741",
    "utf8mb4" => "bf5c5c5c2741",
];
$ok = 0;
$bad = "";
foreach ($want as $cs => $exp) {
    $db->set_charset($cs);
    $got = bin2hex($db->real_escape_string($in));
    if ($got === $exp) { $ok++; } else { $bad = $bad . $cs . ":" . $got . " "; }
}
echo $ok === count($want) ? "all-match" : ("MISMATCH " . $bad);
// Live charset tracking: a raw SET NAMES (not set_charset) is honored too.
$db->query("SET NAMES sjis");
echo "|", $db->character_set_name();
echo "|", bin2hex($db->real_escape_string("\xBF\x5C"));
$db->set_charset("utf8mb4");
"#,
    ));
    // sjis under a raw SET NAMES: 0xBF standalone (raw) + escaped backslash.
    assert_eq!(out, "all-match|sjis|bf5c5c");
}

/// SECURITY: the escape tracks the CLIENT charset changed by a raw
/// `SET character_set_client = gbk` (not just SET NAMES), so it is never left on
/// a stale utf8mb4 table while the server reads gbk — the round-2 breakout
/// through a different door. php stays on utf8mb4 here and emits the unsafe
/// `bf5c27`; elephc goes further and emits the safe `5cbf5c27`.
#[test]
#[ignore]
fn test_mysqli_escape_tracks_character_set_client() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("SET character_set_client = gbk");
echo $db->character_set_name();
echo "|", bin2hex($db->real_escape_string("\xBF\x27"));
$db->query("SET @@character_set_client = 'sjis'");
echo "|", $db->character_set_name();
echo "|", bin2hex($db->real_escape_string("\xBF\x27"));
$db->query("SET NAMES utf8mb4");
echo "|", $db->character_set_name();
"#,
    ));
    // gbk: 0xBF is a lead → escaped, then quote escaped (5cbf 5c27).
    // sjis: 0xBF not a lead → raw, quote escaped (bf 5c27).
    assert_eq!(out, "gbk|5cbf5c27|sjis|bf5c27|utf8mb4");
}

/// SECURITY: a transaction `$name` cannot inject an executable `/*! … */`
/// comment. php strips the name to its allowlist; elephc does the same, so a
/// `!`-prefixed name carrying a `;`-separated DROP is neutralized (the table
/// survives), while an empty name is a `ValueError` before any SQL is sent.
#[test]
#[ignore]
fn test_mysqli_transaction_name_cannot_inject() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("DROP TABLE IF EXISTS victims");
$db->query("CREATE TABLE victims (i INT)");
$db->begin_transaction(0, "!50000 ; DROP TABLE victims");
$db->commit();
$db->rollback(0, "M!50000 ; DROP TABLE victims");
$r = $db->query("SELECT COUNT(*) c FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'victims'");
echo ($r instanceof mysqli_result) ? ("survived=" . $r->fetch_column(0)) : "no";
try { $db->begin_transaction(0, ""); echo "|no-throw"; }
catch (ValueError $e) { echo "|empty-ve"; }
$db->query("DROP TABLE victims");
"#,
    ));
    assert_eq!(out, "survived=1|empty-ve");
}

/// The core result-identity guarantee: `query()` returns a `mysqli_result`
/// that OWNS its rows, so a later query on the same connection leaves an
/// earlier result fully usable (`data_seek`, `fetch_assoc`, `num_rows`).
#[test]
#[ignore]
fn test_mysqli_query_assoc_and_independent_result() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mj");
$db->query("CREATE TABLE mj (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(32))");
$db->query("INSERT INTO mj (name) VALUES ('Ada'), ('Ben')");
$r1 = $db->query("SELECT name FROM mj ORDER BY id");
$r2 = $db->query("SELECT COUNT(*) AS c FROM mj");
$row = $r2->fetch_assoc();
echo $row["c"], "|";
$r1->data_seek(1);
$second = $r1->fetch_assoc();
echo $second["name"], "|";
echo $r1->num_rows;
$db->query("DROP TABLE mj");
"#,
    ));
    assert_eq!(out, "2|Ben|2");
}

/// The fetch family over one buffered result: fetch_row / fetch_array modes /
/// fetch_object / fetch_all / fetch_column / foreach, plus field metadata.
#[test]
#[ignore]
fn test_mysqli_fetch_family_and_foreach() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mf");
$db->query("CREATE TABLE mf (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(32), score DOUBLE)");
$db->query("INSERT INTO mf (name, score) VALUES ('Ada', 1.5), ('Ben', 2.25)");
$r = $db->query("SELECT id, name, score FROM mf ORDER BY id");
if (!($r instanceof mysqli_result)) {
    echo "no-result";
    exit(1);
}
$row = $r->fetch_row();
echo $row[1], "|";
$both = $r->fetch_array(MYSQLI_BOTH);
echo $both[1], "=", $both["name"], "|";
$r->data_seek(0);
$obj = $r->fetch_object();
echo $obj->name, ":", $obj->score, "|";
$r->data_seek(0);
$all = $r->fetch_all(MYSQLI_ASSOC);
echo count($all), "|";
$r->data_seek(0);
echo $r->fetch_column(1), "|";
$names = "";
foreach ($r as $i => $frow) {
    $names = $names . $i . $frow["name"];
}
echo $names, "|";
$f = $r->fetch_field();
echo $f->name, ":", $f->type == MYSQLI_TYPE_LONG || $f->type == MYSQLI_TYPE_LONGLONG ? "int" : "other";
echo "|", $r->field_count, "|", mysqli_num_rows($r);
$db->query("DROP TABLE mf");
"#,
    ));
    // fetch_row consumes row 0 (Ada), fetch_array then sees row 1 (Ben); each
    // data_seek(0) rewinds before the next family member.
    assert_eq!(out, "Ada|Ben=Ben|Ada:1.5|2|Ada|0Ada1Ben|id:int|3|2");
}

/// Non-select statements return `true` and refresh `affected_rows` /
/// `insert_id`; `real_query` + `store_result` picks up the pending result.
#[test]
#[ignore]
fn test_mysqli_non_select_and_store_result() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mn");
$db->query("CREATE TABLE mn (id INT PRIMARY KEY AUTO_INCREMENT, v INT)");
$ok = $db->query("INSERT INTO mn (v) VALUES (10)");
echo $ok === true ? "T" : "F";
echo "|", $db->affected_rows, "|", $db->insert_id;
$db->query("INSERT INTO mn (v) VALUES (20), (30)");
echo "|", $db->affected_rows;
echo "|", $db->real_query("SELECT v FROM mn ORDER BY id") ? "rq" : "no";
$r = $db->store_result();
echo "|", $r === false ? "F" : $r->num_rows;
echo "|", $db->store_result() === false ? "empty" : "again";
$db->query("DROP TABLE mn");
"#,
    ));
    assert_eq!(out, "T|1|1|2|rq|3|empty");
}

/// Prepared statements: `bind_param` + `execute` + `get_result`, with a
/// re-executable statement. Bound values are captured at bind_param time
/// (documented divergence from PHP's read-at-execute references), so fresh
/// values come from re-binding or `execute($params)`.
#[test]
#[ignore]
fn test_mysqli_stmt_bind_param_and_get_result() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS ms");
$db->query("CREATE TABLE ms (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(32), score DOUBLE)");
$ins = $db->prepare("INSERT INTO ms (name, score) VALUES (?, ?)");
echo $ins->param_count, "|";
$name = "Ada";
$score = 1.5;
$ins->bind_param("sd", $name, $score);
$ins->execute();
$name = "Ben";
$score = 2.25;
$ins->bind_param("sd", $name, $score);
$ins->execute();
echo $ins->affected_rows, "|";
$sel = $db->prepare("SELECT name, score FROM ms WHERE name = ?");
$who = "Ben";
$sel->bind_param("s", $who);
$sel->execute();
$r = $sel->get_result();
if (!($r instanceof mysqli_result)) {
    echo "no-result";
    exit(1);
}
$row = $r->fetch_assoc();
echo $row["name"], ":", $row["score"], "|";
$who = "Ada";
$sel->bind_param("s", $who);
$sel->execute();
$r2 = $sel->get_result();
if (!($r2 instanceof mysqli_result)) {
    echo "no-result2";
    exit(1);
}
$row2 = $r2->fetch_assoc();
echo $row2["name"], "|";
$sel->close();
$ins->close();
$db->query("DROP TABLE ms");
"#,
    ));
    assert_eq!(out, "2|1|Ben:2.25|Ada|");
}

/// `execute($params)` binds an array per execution (PHP 8.1+ shape), and
/// `store_result` makes `num_rows` valid on the statement.
#[test]
#[ignore]
fn test_mysqli_stmt_execute_params_and_store_result() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mb");
$db->query("CREATE TABLE mb (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(32))");
$ins = $db->prepare("INSERT INTO mb (name) VALUES (?)");
$ins->execute(["Ada"]);
$ins->execute(["Ben"]);
echo $ins->insert_id, "|";
$sel = $db->prepare("SELECT id, name FROM mb ORDER BY id");
$sel->execute();
$sel->store_result();
echo $sel->num_rows, "|";
$sel->execute();
$r = $sel->get_result();
if (!($r instanceof mysqli_result)) {
    echo "no-result";
    exit(1);
}
$out = "";
foreach ($r as $row) {
    $out = $out . $row["id"] . ":" . $row["name"] . ",";
}
echo $out;
$sel->close();
$ins->close();
$db->query("DROP TABLE mb");
"#,
    ));
    assert_eq!(out, "2|2|1:Ada,2:Ben,");
}

/// `execute_query` (PHP 8.2+) is prepare + execute + get_result in one call,
/// and the procedural statement pipeline works end to end.
#[test]
#[ignore]
fn test_mysqli_execute_query_and_procedural_stmt() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mq");
$db->query("CREATE TABLE mq (id INT PRIMARY KEY AUTO_INCREMENT, v INT)");
echo $db->execute_query("INSERT INTO mq (v) VALUES (?)", [7]) === true ? "ins" : "no";
$r = $db->execute_query("SELECT v FROM mq");
if (!($r instanceof mysqli_result)) {
    echo "no-result";
    exit(1);
}
echo "|", $r->fetch_column(0);
$stmt = mysqli_prepare($db, "SELECT v + ? FROM mq");
$delta = 5;
echo "|", mysqli_stmt_bind_param($stmt, "i", $delta) ? "bp" : "no";
echo "|", mysqli_stmt_execute($stmt) ? "ex" : "no";
$pr = mysqli_stmt_get_result($stmt);
if (!($pr instanceof mysqli_result)) {
    echo "no-presult";
    exit(1);
}
echo "|", $pr->fetch_column(0);
echo "|", mysqli_stmt_param_count($stmt);
mysqli_stmt_close($stmt);
$db->query("DROP TABLE mq");
"#,
    ));
    assert_eq!(out, "ins|7|bp|ex|12|1");
}

/// `multi_query` sends the whole batch in one round-trip; `store_result`
/// returns each set, `more_results`/`next_result` walk the retained sets, and
/// a non-select statement in the batch updates `affected_rows` instead.
#[test]
#[ignore]
fn test_mysqli_multi_query_traverses_result_sets() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mm");
$db->query("CREATE TABLE mm (v INT)");
echo $db->multi_query("SELECT 1 AS a; INSERT INTO mm (v) VALUES (5), (6); SELECT 2 AS b") ? "mq" : "no";
$r1 = $db->store_result();
if (!($r1 instanceof mysqli_result)) {
    echo "no-r1";
    exit(1);
}
echo "|", $r1->fetch_column(0);
echo "|", $db->more_results() ? "more" : "done";
echo "|", $db->next_result() ? "next" : "no-next";
echo "|", $db->store_result() === false ? "F" : "set";
echo "|", $db->affected_rows;
echo "|", $db->more_results() ? "more" : "done";
echo "|", $db->next_result() ? "next" : "no-next";
$r2 = $db->store_result();
if (!($r2 instanceof mysqli_result)) {
    echo "no-r2";
    exit(1);
}
echo "|", $r2->fetch_column(0);
echo "|", $db->more_results() ? "more" : "done";
echo "|", $db->next_result() ? "next" : "no-next";
echo "|", $r1->num_rows;
$db->query("DROP TABLE mm");
"#,
    ));
    assert_eq!(
        out,
        "mq|1|more|next|F|2|more|next|2|done|no-next|1"
    );
}

/// The procedural multi_query pipeline mirrors the object surface.
#[test]
#[ignore]
fn test_mysqli_multi_query_procedural() {
    let out = compile_and_run(&my_program(
        r#"
echo mysqli_multi_query($db, "SELECT 7 AS a; SELECT 8 AS b") ? "mq" : "no";
$r = mysqli_store_result($db);
if (!($r instanceof mysqli_result)) {
    echo "no-r";
    exit(1);
}
echo "|", $r->fetch_column(0);
echo "|", mysqli_more_results($db) ? "more" : "done";
echo "|", mysqli_next_result($db) ? "next" : "no-next";
$r2 = mysqli_store_result($db);
if (!($r2 instanceof mysqli_result)) {
    echo "no-r2";
    exit(1);
}
echo "|", $r2->fetch_column(0);
echo "|", mysqli_more_results($db) ? "more" : "done";
"#,
    ));
    assert_eq!(out, "mq|7|more|next|8|done");
}

/// `real_connect` on an already-connected object closes the previous
/// connection first (php-src reconnect semantics): the new session has a new
/// thread id and the object stays fully usable.
#[test]
#[ignore]
fn test_mysqli_reconnect_closes_previous_connection() {
    let out = compile_and_run(&my_program(
        r#"
$tid1 = $db->thread_id;
echo $tid1 > 0 ? "tid" : "notid";
echo "|", $db->real_connect($host, $user, $pass, $dbname, $port) ? "re" : "no";
echo "|", $db->thread_id > 0 && $db->thread_id != $tid1 ? "newtid" : "same";
$r = $db->query("SELECT 42");
echo "|", ($r instanceof mysqli_result) ? $r->fetch_column(0) : "no";
"#,
    ));
    assert_eq!(out, "tid|re|newtid|42");
}

/// While multi_query result sets remain unconsumed, `query()` and `prepare()`
/// fail with errno 2014 (php-src's "Commands out of sync"); after the batch is
/// fully consumed the connection accepts statements again.
#[test]
#[ignore]
fn test_mysqli_out_of_sync_guard() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->multi_query("SELECT 1 AS a; SELECT 2 AS b");
echo $db->query("SELECT 3") === false ? "F" : "ok";
echo "|", $db->errno;
$r1 = $db->store_result();
echo "|", $db->query("SELECT 3") === false ? "F" : "ok";
echo "|", $db->errno;
echo "|", $db->prepare("SELECT ?") === false ? "F" : "ok";
echo "|", $db->ping() === false ? "F" : "ok";
echo "|", $db->select_db("testdb") === false ? "F" : "ok";
echo "|", $db->begin_transaction() === false ? "F" : "ok";
echo "|", $db->errno;
$db->next_result();
$r2 = $db->store_result();
$q = $db->query("SELECT 3 AS c");
echo "|", ($q instanceof mysqli_result) ? $q->fetch_column(0) : "no";
echo "|", ($r1 instanceof mysqli_result) ? $r1->fetch_column(0) : "no";
"#,
    ));
    assert_eq!(out, "F|2014|F|2014|F|F|F|F|2014|3|1");
}

/// `query()` rejects a second top-level statement client-side (errno 1064) —
/// the anti-injection contract php-src enforces server-side — while a trailing
/// `;`, semicolons inside string literals, and `--`-commented tails stay
/// single statements, and `multi_query()` remains the batch path.
#[test]
#[ignore]
fn test_mysqli_query_rejects_multi_statement() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
echo $db->query("SELECT 1 AS a; SELECT 2 AS b") === false ? "F" : "ok";
echo "|", $db->errno;
$r = $db->query("SELECT 7;");
echo "|", ($r instanceof mysqli_result) ? $r->fetch_column(0) : "no";
$s = $db->query("SELECT '; not a statement' AS s");
echo "|", ($s instanceof mysqli_result) ? "str-ok" : "no";
$c = $db->query("SELECT 8 -- tail; SELECT 9");
echo "|", ($c instanceof mysqli_result) ? $c->fetch_column(0) : "no";
echo "|", $db->multi_query("SELECT 5 AS a; SELECT 6 AS b") ? "mq" : "no";
$m1 = $db->store_result();
$db->next_result();
$m2 = $db->store_result();
echo "|", ($m2 instanceof mysqli_result) ? $m2->fetch_column(0) : "no";
echo "|", $db->query("CREATE TABLE mms (i INT); DROP TABLE mms") === false ? "F" : "ok";
echo "|", $db->errno;
"#,
    ));
    assert_eq!(out, "F|1064|7|str-ok|8|mq|6|F|1064");
}

/// SECURITY: executable comments cannot smuggle a second statement through
/// `query()`. MySQL's `/*! ... */` and MariaDB's `/*M! ... */` (bare and
/// versioned) are live SQL where the server expands them, but a mysqli session
/// runs without server-side multi-statements (like mysqlnd), so the hidden
/// second statement is a 1064 syntax error from the server instead of being
/// executed. MySQL does not expand `/*M!` at all, so there it is an ordinary
/// comment and the query runs, as under php. The case-mismatched `/*m!` is
/// inert on both servers.
#[test]
#[ignore]
fn test_mysqli_query_rejects_executable_comment_statement() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
echo $db->query("SELECT 1/*!;DROP TABLE t*/") === false ? "F" : "ok";
echo "|", $db->errno;
echo "|", $db->query("SELECT 1/*M!;DROP TABLE t*/") === false ? "F" : "ok";
echo "|", $db->errno;
echo "|", $db->query("SELECT 1/*M!100000 ;DROP TABLE t*/") === false ? "F" : "ok";
echo "|", $db->errno;
$r = $db->query("SELECT 3 /*m! ; not a second statement */ AS v");
echo "|", ($r instanceof mysqli_result) ? $r->fetch_column(0) : ("no:" . $db->errno);
"#,
    ));
    // MySQL treats MariaDB's `/*M!` as a plain comment, so there the query runs (and
    // server-side multi-statements are off, so nothing hidden can execute either).
    assert!(
        ["F|1064|F|1064|F|1064|3", "F|1064|ok|0|ok|0|3"].contains(&out.as_str()),
        "unexpected output: {out}"
    );
}

/// Reconnecting discards an unconsumed result: `connect()` on a closed object
/// (php reopens it) and `real_connect()` on a link with a pending multi_query
/// batch both start clean instead of raising a spurious 2014. `real_connect()`
/// on a closed object is php's "already closed" Error.
#[test]
#[ignore]
fn test_mysqli_close_clears_pending_result_for_reconnect() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->real_query("SELECT 1");
$db->close();
echo $db->connect($host, $user, $pass, $dbname, $port) ? "re" : "no";
$r = $db->query("SELECT 2");
echo "|", ($r instanceof mysqli_result) ? $r->fetch_column(0) : ("no:" . $db->errno);
$db->multi_query("SELECT 3 AS a; SELECT 4 AS b");
echo "|", $db->real_connect($host, $user, $pass, $dbname, $port) ? "re" : "no";
$r2 = $db->query("SELECT 5");
echo "|", ($r2 instanceof mysqli_result) ? $r2->fetch_column(0) : ("no:" . $db->errno);
$db->close();
try {
    $db->real_connect($host, $user, $pass, $dbname, $port);
} catch (Error $e) {
    echo "|", $e->getMessage();
}
"#,
    ));
    assert_eq!(out, "re|2|re|5|mysqli object is already closed");
}

/// The two-step `stmt_init()` + `prepare()` form honors the
/// commands-out-of-sync guard exactly like one-shot `mysqli::prepare()`:
/// while an unconsumed result is pending, both `prepare()` and `execute()`
/// fail with 2014 instead of issuing wire commands on a busy connection, and
/// both succeed again once the pending result is consumed.
#[test]
#[ignore]
fn test_mysqli_stmt_two_step_out_of_sync_guard() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$n = 5;
$ready = $db->stmt_init();
echo $ready->prepare("SELECT ?") ? "prep" : "no";
$ready->bind_param("i", $n);
$db->real_query("SELECT 1");
$stmt = $db->stmt_init();
echo "|", $stmt->prepare("SELECT 2") === false ? "F" : "ok";
echo "|", $stmt->errno;
echo "|", $ready->execute() === false ? "F" : "ok";
echo "|", $ready->errno;
$pending = $db->store_result();
echo "|", ($pending instanceof mysqli_result) ? "drained" : "no";
echo "|", $stmt->prepare("SELECT 2") ? "prep2" : "no";
echo "|", $ready->execute() ? "exec" : "no";
$res = $ready->get_result();
echo "|", ($res instanceof mysqli_result) ? $res->fetch_column(0) : "no";
"#,
    ));
    assert_eq!(out, "prep|F|2014|F|2014|drained|prep2|exec|5");
}

/// A server error while producing the first result set of a multi_query batch (a scalar
/// subquery returning 3 rows, errno 1242 on both MySQL and MariaDB): like mysqlnd,
/// `multi_query()` itself succeeds — it reads only the set's header and metadata — and the
/// error surfaces when `store_result()` reads the rows (no partial result, batch closed,
/// connection still usable). Expected output is php 8.3 mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_multi_query_reports_mid_set_error() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("DROP TABLE IF EXISTS mde");
$db->query("CREATE TABLE mde (v INT)");
$db->query("INSERT INTO mde (v) VALUES (1), (2), (3)");
echo $db->multi_query("SELECT (SELECT v FROM mde) AS s FROM mde; SELECT 99") ? "mq" : "F";
echo "|", $db->errno;
echo "|", $db->store_result() === false ? "nores" : "res";
echo "|", $db->more_results() ? "more" : "nomore";
$r = $db->query("SELECT 7");
echo "|", ($r instanceof mysqli_result) ? $r->fetch_column(0) : ("no:" . $db->errno);
"#,
    ));
    assert_eq!(out, "mq|0|nores|nomore|7");
}

/// Multi-statements are off on a mysqli session, like mysqlnd's: compound-body DDL is ONE
/// statement, so `query()` creates the procedure, while a `... END; DROP ...` injection
/// tail is rejected by the server itself (1064). `multi_query()` turns them on, `CALL`
/// walks both result sets, and the next `query()` is back to single statements.
#[test]
#[ignore]
fn test_mysqli_session_runs_without_multi_statements() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("DROP PROCEDURE IF EXISTS mproc");
echo $db->query("CREATE PROCEDURE mproc() BEGIN SELECT 41; SELECT 42 AS v; END") === true ? "created" : ("F:" . $db->errno);
echo "|", $db->query("CREATE PROCEDURE mproc2() BEGIN SELECT 1; END; DROP TABLE users") === false ? "F" : "leaked";
echo "|", $db->errno, "|", $db->sqlstate;
echo "|", $db->query("SELECT 1; SELECT 2") === false ? "F" : "leaked", "|", $db->errno;
echo "|", $db->multi_query("CALL mproc()") ? "call" : "no";
$r1 = $db->store_result();
echo "|", ($r1 instanceof mysqli_result) ? $r1->fetch_column(0) : "no";
while ($db->more_results()) {
    $db->next_result();
    $rn = $db->store_result();
    if ($rn instanceof mysqli_result) {
        echo "|", $rn->fetch_column(0);
    }
}
echo "|", $db->multi_query("SELECT 5; SELECT 6") ? "mq" : "no";
do {
    $set = $db->store_result();
    if ($set instanceof mysqli_result) {
        $set->free();
    }
} while ($db->next_result());
echo "|", $db->query("SELECT 7; SELECT 8") === false ? "F" : "leaked", "|", $db->errno;
echo "|", $db->get_connection_stats()["com_stmt_set_option"];
$db->query("DROP PROCEDURE mproc");
echo "|done";
"#,
    ));
    assert_eq!(out, "created|F|1064|42000|F|1064|call|41|42|mq|F|1064|2|done");
}

/// `fetch_field_direct()->decimals` carries MySQL's wire "decimals" byte
/// (through the bridge's column_precision), so a DECIMAL(10,2) reports 2.
#[test]
#[ignore]
fn test_mysqli_fetch_field_decimals() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS md");
$db->query("CREATE TABLE md (price DECIMAL(10,2), label VARCHAR(8))");
$db->query("INSERT INTO md VALUES (12.34, 'x')");
$r = $db->query("SELECT price, label FROM md");
if (!($r instanceof mysqli_result)) {
    echo "no-result";
    exit(1);
}
$f = $r->fetch_field_direct(0);
echo $f->decimals;
$g = $r->fetch_field_direct(1);
echo "|", $g->decimals;
$db->query("DROP TABLE md");
"#,
    ));
    assert_eq!(out, "2|0");
}

/// `bind_param()` binds REFERENCES (php's semantics): mutating the variable
/// after binding and WITHOUT re-binding executes with the mutated value.
#[test]
#[ignore]
fn test_mysqli_stmt_bind_param_reads_the_variable_at_execute() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS msnap");
$db->query("CREATE TABLE msnap (name VARCHAR(32))");
$ins = $db->prepare("INSERT INTO msnap (name) VALUES (?)");
$name = "Ada";
$ins->bind_param("s", $name);
$name = "Ben";
$ins->execute();
$r = $db->query("SELECT name FROM msnap");
echo ($r instanceof mysqli_result) ? $r->fetch_column(0) : "no";
$db->query("DROP TABLE msnap");
"#,
    ));
    assert_eq!(out, "Ben");
}

/// Connection information, ping, charset, autocommit, and a commit round-trip
/// against the live server.
#[test]
#[ignore]
fn test_mysqli_connection_info_and_transactions() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->ping() ? "ping" : "dead";
echo "|", $db->server_version >= 50000 ? "ver" : "old";
echo "|", strlen($db->host_info) > 0 ? "host" : "nohost";
echo "|", $db->thread_id > 0 ? "tid" : "notid";
echo "|", $db->set_charset("utf8mb4") ? "cs" : "nocs";
echo "|", $db->character_set_name();
echo "|", $db->autocommit(true) ? "ac" : "noac";
$db->begin_transaction();
echo "|", $db->commit() ? "tx" : "notx";
$db->begin_transaction(0, "sp1");
echo "|", $db->rollback(0, "sp1") ? "sp" : "nosp";
echo "|", $db->rollback() ? "rb" : "norb";
echo "|", strlen((string) $db->stat()) > 0 ? "stat" : "nostat";
$db->close();
echo "|closed";
"#,
    ));
    assert_eq!(
        out,
        "ping|ver|host|tid|cs|utf8mb4|ac|tx|sp|rb|stat|closed"
    );
}

/// The corrected transaction semantics: a named `commit()` COMMITS (it emits
/// `COMMIT /*name*/`, not `RELEASE SAVEPOINT`), so the row persists after a
/// reconnect — the data-loss blocker. Savepoints use the separate
/// `savepoint()` / `release_savepoint()` API, and `$flags` compose into the
/// SQL. `$db->affected_rows` after commit is 0 (php resets it).
#[test]
#[ignore]
fn test_mysqli_named_commit_persists_and_savepoints() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mtx");
$db->query("CREATE TABLE mtx (id INT PRIMARY KEY AUTO_INCREMENT, v INT) ENGINE=InnoDB");
$db->begin_transaction(MYSQLI_TRANS_START_READ_WRITE, "job");
$db->query("INSERT INTO mtx (v) VALUES (10), (20)");
$db->savepoint("half");
$db->query("INSERT INTO mtx (v) VALUES (30)");
echo $db->release_savepoint("half") ? "rel" : "norel";
echo "|", $db->commit(0, "job") ? "commit" : "no";
echo "|aff=", $db->affected_rows;
// Reconnect proves the writes were committed, not rolled back on destruct.
$db2 = new mysqli($host, $user, $pass, $dbname, $port);
$r = $db2->query("SELECT COUNT(*) c FROM mtx");
echo "|rows=", ($r instanceof mysqli_result) ? $r->fetch_column(0) : "?";
// rollback to savepoint keeps the outer transaction open.
$db->begin_transaction();
$db->query("INSERT INTO mtx (v) VALUES (40)");
$db->savepoint("sp2");
$db->query("INSERT INTO mtx (v) VALUES (50)");
$db->rollback(0, "sp2");   // ROLLBACK /*sp2*/ — full rollback, not to savepoint
$rc = $db->query("SELECT COUNT(*) c FROM mtx");
echo "|after_rb=", ($rc instanceof mysqli_result) ? $rc->fetch_column(0) : "?";
$db2->query("DROP TABLE mtx");
"#,
    ));
    // 3 rows committed (10,20,30); the second transaction's rollback discards
    // its inserts, so the count stays 3.
    assert_eq!(out, "rel|commit|aff=0|rows=3|after_rb=3");
}

/// The two-step statement API (`stmt_init()` + `prepare()`), the statement
/// introspectors, and `get_charset()` over the live server.
#[test]
#[ignore]
fn test_mysqli_stmt_init_prepare_and_introspection() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS msi");
$db->query("CREATE TABLE msi (id INT PRIMARY KEY AUTO_INCREMENT, v INT)");
$stmt = $db->stmt_init();
echo $stmt->prepare("INSERT INTO msi (v) VALUES (?)") ? "prep" : "no";
echo "|params=", $stmt->param_count;
$stmt->execute([7]);
echo "|stmt_insert_id=", $stmt->insert_id;
echo "|stmt_affected=", $stmt->affected_rows;
echo "|stmt_sqlstate=", $stmt->sqlstate;
$stmt->free_result();
$stmt->close();
$cs = $db->get_charset();
echo "|charset=", $cs->charset;
$sel = $db->prepare("SELECT id, v FROM msi ORDER BY id");
$sel->execute();
$r = $sel->get_result();
if ($r instanceof mysqli_result) {
    $r->fetch_field();
    echo "|field_tell=", $r->current_field;
    $r->field_seek(0);
    echo "|after_seek=", $r->current_field;
}
$sel->close();
$db->query("DROP TABLE msi");
"#,
    ));
    assert_eq!(
        out,
        "prep|params=1|stmt_insert_id=1|stmt_affected=1|stmt_sqlstate=00000|charset=utf8mb4|field_tell=1|after_seek=0"
    );
}

/// `bind_param()` and `bind_result()` bind BY REFERENCE, exactly like php:
/// each `execute()` reads the bound variables' CURRENT values, and every
/// `fetch()` writes the row back into the result variables (`null` once the
/// set is exhausted). The same statement re-executes with fresh values.
#[test]
#[ignore]
fn test_mysqli_stmt_reference_binding_and_fetch() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS msr");
$db->query("CREATE TABLE msr (id INT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(20), score DOUBLE)");
$ins = $db->prepare("INSERT INTO msr (name, score) VALUES (?, ?)");
$name = "";
$score = 0.0;
$ins->bind_param("sd", $name, $score);
foreach ([["Ada", 1.5], ["Ben", 2.25], ["Cy", 3.0]] as $row) {
    $name = $row[0];
    $score = $row[1];
    $ins->execute();
}
echo "last=", $db->insert_id;
$sel = $db->prepare("SELECT id, name, score FROM msr WHERE id >= ? ORDER BY id");
echo "|fields=", $sel->field_count;
$min = 1;
$sel->bind_param("i", $min);
$id = null;
$n = null;
$s = null;
$sel->bind_result($id, $n, $s);
$sel->execute();
while ($sel->fetch()) {
    echo "|", $id, ":", $n, ":", $s;
}
var_dump($sel->fetch());
$min = 3;
$sel->execute();
$sel->fetch();
echo "again=", $id, ":", $n;
// Like mysqlnd, the unread EOF keeps the link out of sync until the result is freed.
$sel->free_result();
$db->query("DROP TABLE msr");
"#,
    ));
    assert_eq!(out, "last=3|fields=3|1:Ada:1.5|2:Ben:2.25|3:Cy:3NULL\nagain=3:Cy");
}

/// `store_result()` buffers the statement's rows client-side: `num_rows` /
/// `num_rows()` are exact, `data_seek()` rewinds `fetch()`, and
/// `result_metadata()` describes the columns before execution.
#[test]
#[ignore]
fn test_mysqli_stmt_store_result_data_seek_and_metadata() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mss");
$db->query("CREATE TABLE mss (id INT PRIMARY KEY, label VARCHAR(10))");
$db->query("INSERT INTO mss VALUES (1, 'a'), (2, 'b'), (3, 'c')");
$stmt = $db->prepare("SELECT id, label FROM mss ORDER BY id");
$meta = $stmt->result_metadata();
echo "meta=", $meta->field_count, ":", $meta->fetch_field()->name, ":", $meta->num_rows;
echo "|id>0=", $stmt->id > 0 ? "y" : "n";
$stmt->execute();
$stmt->store_result();
echo "|rows=", $stmt->num_rows, "/", $stmt->num_rows();
$id = 0;
$label = "";
$stmt->bind_result($id, $label);
$stmt->data_seek(2);
$stmt->fetch();
echo "|seek2=", $id, $label;
$stmt->data_seek(0);
$stmt->fetch();
echo "|seek0=", $id, $label;
$stmt->free_result();
$db->query("DROP TABLE mss");
"#,
    ));
    assert_eq!(out, "meta=2:id:0|id>0=y|rows=3/3|seek2=3c|seek0=1a");
}

/// `send_long_data()` streams a `b` parameter in chunks (sent at the next
/// `execute()`), a `b` parameter without chunks sends an empty string like
/// mysqlnd, and php's ArgumentCountError / ValueError argument checks apply.
#[test]
#[ignore]
fn test_mysqli_stmt_long_data_and_argument_errors() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS msl");
$db->query("CREATE TABLE msl (id INT PRIMARY KEY, body BLOB)");
$stmt = $db->prepare("INSERT INTO msl (id, body) VALUES (?, ?)");
$id = 1;
$blob = "ignored";
$stmt->bind_param("ib", $id, $blob);
$stmt->send_long_data(1, "hello ");
$stmt->send_long_data(1, "world");
$stmt->execute();
$id = 2;
$stmt->execute();
echo $stmt->send_long_data(5, "x") ? "T" : "F", $stmt->errno;
$r = $db->query("SELECT id, body FROM msl ORDER BY id");
while ($row = $r->fetch_row()) {
    echo "|", $row[0], "=", $row[1];
}
try {
    $stmt->bind_param("i", $id);
} catch (ArgumentCountError $e) {
    echo "|ace";
}
try {
    $stmt->execute([1]);
} catch (ValueError $e) {
    echo "|", $e->getMessage();
}
try {
    $stmt->attr_set(99, 1);
} catch (ValueError $e) {
    echo "|attr-ve";
}
echo "|", $stmt->attr_set(MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH, 1) ? "set" : "no";
echo ":", $stmt->attr_get(MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH);
$db->query("DROP TABLE msl");
"#,
    ));
    assert_eq!(
        out,
        "F2034|1=hello world|2=|ace|mysqli_stmt::execute(): Argument #1 ($params) must consist of exactly 2 elements, 1 present|attr-ve|set:1"
    );
}

/// mysqlnd's `send_long_data()` preconditions and the `store_result()` / `get_result()`
/// exclusion: no bound parameters is 2014, a parameter number past the count 2034, a
/// parameter not bound as `b` (even `s`) 2035, none of them reported (so STRICT mode does not
/// throw), and a success clears the error, as `bind_param()` and `bind_result()` do.
/// `get_result()` after `store_result()` fails with a reported 2014 and leaves the stored
/// rows fetchable. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_stmt_long_data_preconditions_and_stored_get_result() {
    let out = compile_and_run(&my_program(
        r#"
$plain = $db->prepare("SELECT ?");
echo $plain->send_long_data(0, "x") ? "T" : "F", $plain->errno;
$i = 1;
$plain->bind_param("i", $i);
echo "/", $plain->errno;
echo "|", $plain->send_long_data(0, "x") ? "T" : "F", $plain->errno, ":", $plain->error_list[0]["sqlstate"];
$s = "s";
$plain->bind_param("s", $s);
echo "|", $plain->send_long_data(0, "x") ? "T" : "F", $plain->errno;
echo "|", $plain->send_long_data(4, "x") ? "T" : "F", $plain->errno;
$plain->bind_param("b", $s);
echo "|", $plain->send_long_data(0, "x") ? "T" : "F", $plain->errno, count($plain->error_list);
$sel = $db->prepare("SELECT 1 UNION SELECT 2");
$sel->execute();
$sel->store_result();
try {
    $sel->get_result();
    echo "|no-throw";
} catch (mysqli_sql_exception $e) {
    echo "|", $e->getCode(), ":", $e->getMessage();
}
$v = 0;
$sel->bind_result($v);
$sel->fetch();
echo "|", $v, ",", $sel->num_rows, ",", $sel->errno;
"#,
    ));
    assert_eq!(
        out,
        "F2014/0|F2035:HY000|F2035|F2034|T00|2014:Commands out of sync; you can't run this command now|1,2,0"
    );
}

/// `MYSQLI_CURSOR_TYPE_READ_ONLY` opens a server cursor like mysqlnd: the link stays free while
/// its rows are fetched one `COM_STMT_FETCH` each, a fetch past the end is `false`,
/// `get_result()` / `store_result()` read the rest in one fetch, `free_result()` / a
/// re-execution / `close()` drain the cursor, and a statement without a result set runs as
/// usual. The lifecycle statistics match mysqlnd's. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_stmt_read_only_cursor() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mcur");
$db->query("CREATE TABLE mcur (id INT)");
$db->query("INSERT INTO mcur VALUES (1), (2), (3)");
$st = $db->prepare("SELECT id FROM mcur ORDER BY id");
echo $st->attr_set(MYSQLI_STMT_ATTR_CURSOR_TYPE, MYSQLI_CURSOR_TYPE_READ_ONLY) ? "set" : "unset";
$st->execute();
echo "|", $st->num_rows, "|", $db->query("SELECT 42")->fetch_row()[0];
$id = 0;
$st->bind_result($id);
$out = "";
while ($st->fetch()) {
    $out = $out . $id . ":" . $db->query("SELECT 7")->fetch_row()[0] . ",";
}
echo "|", $out, "|", $st->num_rows;
echo "|", var_export($st->fetch(), true);
$st->execute();
$st->fetch();
$st->free_result();
echo "|", $db->query("SELECT 8")->fetch_row()[0];
$st->execute();
$r = $st->get_result();
$ids = [];
foreach ($r->fetch_all() as $row) {
    $ids[] = $row[0];
}
echo "|", $r->num_rows, "=", implode(",", $ids);
$st->execute();
$st->store_result();
echo "|", $st->num_rows;
$st->fetch();
echo "=", $id;
$st->execute();
$st->fetch();
$st->execute();
$st->fetch();
echo "|re=", $id;
$st->close();
$e = $db->prepare("SELECT id FROM mcur WHERE id > 100");
$e->attr_set(MYSQLI_STMT_ATTR_CURSOR_TYPE, MYSQLI_CURSOR_TYPE_READ_ONLY);
$e->execute();
$x = 0;
$e->bind_result($x);
echo "|empty=", var_export($e->fetch(), true);
$ins = $db->prepare("INSERT INTO mcur VALUES (?)");
$ins->attr_set(MYSQLI_STMT_ATTR_CURSOR_TYPE, MYSQLI_CURSOR_TYPE_READ_ONLY);
$v = 4;
$ins->bind_param("i", $v);
$ins->execute();
echo "|ins=", $ins->affected_rows;
$s = $db->get_connection_stats();
echo "|", $s["com_stmt_fetch"], ",", $s["rows_fetched_from_server_ps"], ",", $s["rows_fetched_from_client_ps_cursor"], ",", $s["rows_skipped_ps"], ",", $s["flushed_ps_sets"], ",", $s["ps_unbuffered_sets"], ",", $s["ps_buffered_sets"];
$db->query("DROP TABLE mcur");
"#,
    ));
    assert_eq!(out, "set|0|42|1:7,2:7,3:7,|3|false|8|3=1,2,3|3=1|re=1|empty=NULL|ins=1|20,18,12,6,3,5,1");
}

/// A read-only cursor statement refuses `fetch()` and `store_result()` with a reported 2014 (on
/// the statement and the link) while an unbuffered query owns the link, then reads its remaining
/// rows once that result is freed. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_stmt_cursor_waits_for_a_busy_link() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS mcurb");
$db->query("CREATE TABLE mcurb (id INT)");
$db->query("INSERT INTO mcurb VALUES (1), (2), (3)");
$st = $db->prepare("SELECT id FROM mcurb ORDER BY id");
$st->attr_set(MYSQLI_STMT_ATTR_CURSOR_TYPE, MYSQLI_CURSOR_TYPE_READ_ONLY);
$st->execute();
$id = 0;
$st->bind_result($id);
$st->fetch();
echo $id;
$u = $db->query("SELECT 7", MYSQLI_USE_RESULT);
try {
    $st->fetch();
    echo "|no-throw";
} catch (mysqli_sql_exception $e) {
    echo "|", $e->getCode();
}
echo ":", $st->errno, ":", $db->errno;
try {
    $st->store_result();
    echo "|no-throw";
} catch (mysqli_sql_exception $e) {
    echo "|", $e->getCode();
}
echo "|u=", $u->fetch_row()[0];
$u->free();
echo "|", var_export($st->fetch(), true), ":", $id;
echo "|", var_export($st->fetch(), true), ":", $id;
echo "|", var_export($st->fetch(), true), "\n";
"#,
    ));
    assert_eq!(out, "1|2014:2014:2014|2014|u=7|true:2|true:3|NULL\n");
}

/// A buffered `MYSQLI_ASYNC` query is readable as soon as the server's first response arrives,
/// not when every row has: the first row (100 KB) is flushed before the second row's
/// `SLEEP(1)`, so `mysqli_poll()` reports the link at once and `reap_async_query()` then waits
/// for the rest. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_async_poll_ready_before_rows_complete() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("SELECT REPEAT('x', 100000) AS v UNION ALL SELECT SLEEP(1)", MYSQLI_ASYNC);
$start = microtime(true);
$read = [$db];
$error = [];
$reject = [];
$ready = mysqli_poll($read, $error, $reject, 0, 600000);
$polled = microtime(true) - $start;
$result = $db->reap_async_query();
echo $ready, "|", $polled < 0.5 ? "early" : "late", "|", $result->num_rows, "|", strlen($result->fetch_row()[0]), "\n";
"#,
    ));
    assert_eq!(out, "1|early|2|100000\n");
}

/// Closing a link with a pending `MYSQLI_ASYNC` query (buffered or `MYSQLI_USE_RESULT`) returns
/// at once, like mysqlnd, which closes the socket instead of waiting for the server to finish
/// the query. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_close_does_not_wait_for_a_pending_async_query() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$a = new mysqli($host, $user, $pass, $dbname, $port);
$a->query("SELECT SLEEP(3)", MYSQLI_ASYNC);
$start = microtime(true);
$a->close();
echo microtime(true) - $start < 1.0 ? "buffered:early" : "buffered:late";
$b = new mysqli($host, $user, $pass, $dbname, $port);
$b->query("SELECT SLEEP(3)", MYSQLI_ASYNC | MYSQLI_USE_RESULT);
$start = microtime(true);
$b->close();
echo "|", microtime(true) - $start < 1.0 ? "streaming:early" : "streaming:late";
echo "|", $db->query("SELECT 5")->fetch_row()[0], "\n";
"#,
    ));
    assert_eq!(out, "buffered:early|streaming:early|5\n");
}

/// An unbuffered result kept across a reconnect of its link does not finish the link's newer
/// stream when it is freed: the newer result still reads its remaining rows. Expected output
/// is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_old_stream_result_leaves_the_new_stream_alone() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$link = new mysqli($host, $user, $pass, $dbname, $port);
$old = $link->query("SELECT 1 UNION ALL SELECT 2", MYSQLI_USE_RESULT);
echo $old->fetch_row()[0];
$link->real_connect($host, $user, $pass, $dbname, $port);
$new = $link->query("SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9", MYSQLI_USE_RESULT);
echo "|", $new->fetch_row()[0];
$old->free();
$rest = [];
while ($row = $new->fetch_row()) {
    $rest[] = $row[0];
}
echo "|", implode(",", $rest), "|", $new->num_rows, "|", $link->errno, "\n";
"#,
    ));
    assert_eq!(out, "1|7|8,9|3|0\n");
}

/// A read that times out midway through an unbuffered result is reported (errno 2006 on the
/// link, `mysqli_sql_exception` under STRICT) instead of ending the set as if it were complete.
/// Expected output is php 8.5 + mysqlnd's, except that with reporting off elephc's fetch
/// returns `null` where php returns `false` (documented in docs/php/mysqli.md).
#[test]
#[ignore]
fn test_mysqli_unbuffered_read_failure_is_reported() {
    let out = compile_and_run(&my_program(
        r#"
$slow = mysqli_init();
$slow->options(MYSQLI_OPT_READ_TIMEOUT, 1);
$slow->real_connect($host, $user, $pass, $dbname, $port);
$r = $slow->query("SELECT REPEAT('x', 100000) AS v UNION ALL SELECT SLEEP(3)", MYSQLI_USE_RESULT);
echo strlen($r->fetch_row()[0]);
try {
    $row = $r->fetch_row();
    echo "|no-throw:", var_export($row, true);
} catch (mysqli_sql_exception $e) {
    echo "|", $e->getCode(), ":", $e->getMessage();
}
echo "|", $slow->errno, "\n";
mysqli_report(MYSQLI_REPORT_OFF);
$slow2 = mysqli_init();
$slow2->options(MYSQLI_OPT_READ_TIMEOUT, 1);
$slow2->real_connect($host, $user, $pass, $dbname, $port);
$r = $slow2->query("SELECT REPEAT('x', 100000) AS v UNION ALL SELECT SLEEP(3)", MYSQLI_USE_RESULT);
echo strlen($r->fetch_row()[0]), "|", var_export($r->fetch_row(), true), "|", $slow2->errno, "|", $slow2->error, "|", $r->num_rows, "\n";
"#,
    ));
    assert_eq!(out, "100000|2006:MySQL server has gone away|2006\n100000|NULL|2006|MySQL server has gone away|1\n");
}

/// A persistent link closed with an unreaped `MYSQLI_ASYNC` query (either result mode) closes
/// at once and does not poison the pool: the next `p:` connection with the same credentials
/// runs queries right away. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_persistent_link_closed_with_pending_async_query() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
foreach ([MYSQLI_ASYNC, MYSQLI_ASYNC | MYSQLI_USE_RESULT] as $mode) {
    $p = new mysqli("p:" . $host, $user, $pass, $dbname, $port);
    $p->query("SELECT SLEEP(2)", $mode);
    $start = microtime(true);
    $p->close();
    $closed = microtime(true) - $start;
    $q = new mysqli("p:" . $host, $user, $pass, $dbname, $port);
    $r = $q->query("SELECT 41 + 1");
    echo $closed < 1.0 ? "early" : "late", ":", $r === false ? "failed:" . $q->errno : $r->fetch_row()[0], ":", microtime(true) - $start < 1.5 ? "fast" : "slow", "|";
    $q->close();
}
echo "\n";
"#,
    ));
    assert_eq!(out, "early:42:fast|early:42:fast|\n");
}

/// Opening a second `p:` link with the same credentials while the first still has a pending
/// `MYSQLI_ASYNC` query leaves that query alone: the first link polls and reaps it. Expected
/// output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_persistent_checkout_keeps_a_busy_owners_async_query() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$a = new mysqli("p:" . $host, $user, $pass, $dbname, $port);
$a->query("SELECT 6 * 7, SLEEP(1)", MYSQLI_ASYNC);
$b = new mysqli("p:" . $host, $user, $pass, $dbname, $port);
$read = [$a];
$error = [];
$reject = [];
echo mysqli_poll($read, $error, $reject, 3), "|", $a->reap_async_query()->fetch_row()[0], "\n";
"#,
    ));
    assert_eq!(out, "1|42\n");
}

/// `mysqli_poll()` with links only in `$error` waits for its timeout (mysqlnd selects on them for
/// exceptional conditions, which never come) instead of returning at once, validates those
/// entries, and leaves readiness to `$read`. Expected output is php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_poll_waits_on_error_only_links() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$x = new mysqli($host, $user, $pass, $dbname, $port);
$x->query("SELECT SLEEP(2)", MYSQLI_ASYNC);
$start = microtime(true);
$read = null;
$error = [$x];
$reject = [];
$n = mysqli_poll($read, $error, $reject, 1);
$waited = microtime(true) - $start;
echo var_export($n, true), "|", $waited >= 0.9 && $waited < 1.8 ? "waited" : "returned", "|", var_export($read, true), "|", count($error), "|", count($reject);
try {
    $read = null;
    $bad = [1];
    mysqli_poll($read, $bad, $reject, 0);
    echo "|accepted";
} catch (TypeError $e) {
    echo "|TypeError";
}
$read = [$x];
$error = [$x];
echo "|", mysqli_poll($read, $error, $reject, 3), "|", count($read), "|", count($error), "|", $x->reap_async_query()->fetch_row()[0], "\n";
"#,
    ));
    assert_eq!(out, "0|waited|NULL|0|0|TypeError|1|1|0|0\n");
}

/// `MYSQLI_ASYNC | MYSQLI_USE_RESULT` reaps an unbuffered result like mysqlnd: the rows are
/// read from the server on fetch (`num_rows` is 0 until the end), the link answers 2014 while
/// the set is unread, and the statistics count one unbuffered set. Expected output is
/// php 8.5 + mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_async_use_result_streams_rows() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS masy");
$db->query("CREATE TABLE masy (id INT)");
$db->query("INSERT INTO masy VALUES (1), (2), (3)");
$db->query("SELECT id FROM masy ORDER BY id", MYSQLI_ASYNC | MYSQLI_USE_RESULT);
$read = [$db];
$error = [$db];
$reject = [];
echo mysqli_poll($read, $error, $reject, 5);
$r = $db->reap_async_query();
echo "|", get_class($r), "|", $r->num_rows, "|", $r->fetch_row()[0];
try {
    $db->query("SELECT 1");
    echo "|no-2014";
} catch (mysqli_sql_exception $e) {
    echo "|", $e->getCode();
}
$rest = [];
while ($row = $r->fetch_row()) {
    $rest[] = $row[0];
}
echo "|", implode(",", $rest), "|", $r->num_rows, "|", $db->query("SELECT 5")->fetch_row()[0];
$s = $db->get_connection_stats();
echo "|", $s["unbuffered_sets"], ",", $s["buffered_sets"], ",", $s["rows_fetched_from_client_normal_unbuffered"], ",", $s["rows_fetched_from_server_normal"];
$db->query("DROP TABLE masy");
"#,
    ));
    assert_eq!(out, "1|mysqli_result|0|1|2014|2,3|3|5|1,1,3,4");
}

/// Warnings come back as a `mysqli_warning` chain (`SHOW WARNINGS`), from the
/// connection and from a statement; `$info` carries the server's OK-packet
/// info string; MYSQLI_OPT_INT_AND_FLOAT_NATIVE turns text-protocol numbers into
/// PHP ints/floats.
#[test]
#[ignore]
fn test_mysqli_warnings_info_and_native_types() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("SELECT CAST('abc' AS SIGNED) AS a, CAST('1x' AS SIGNED) AS b");
echo "wc=", $db->warning_count;
$w = $db->get_warnings();
$codes = [];
do {
    $codes[] = $w->errno . "/" . $w->sqlstate;
} while ($w->next());
echo "|", implode(",", $codes);
$db->query("DROP TABLE IF EXISTS msw");
$db->query("CREATE TABLE msw (v INT)");
$db->query("INSERT INTO msw VALUES (1), (2)");
echo "|info=", $db->info;
$stmt = $db->prepare("SELECT CAST(? AS SIGNED)");
$text = "9z";
$stmt->bind_param("s", $text);
$stmt->execute();
$stmt->store_result();
// mysqlnd takes the statement's warning count from the metadata EOF execute() read (0
// here), so like php no mysqli_warning chain is built.
echo "|stmt-warn=", $stmt->get_warnings() instanceof mysqli_warning ? "y" : "n";
$plain = mysqli_init();
$plain->options(MYSQLI_OPT_INT_AND_FLOAT_NATIVE, 1);
$plain->real_connect($host, $user, $pass, $dbname, $port);
$row = $plain->query("SELECT 7 AS i, 2.5e0 AS f, 1.25 AS d")->fetch_row();
echo "|", gettype($row[0]), ",", gettype($row[1]), ",", gettype($row[2]);
$row = $db->query("SELECT 7 AS i")->fetch_row();
echo "|default=", gettype($row[0]);
$db->query("DROP TABLE msw");
"#,
    ));
    assert_eq!(
        out,
        "wc=2|1292/HY000,1292/HY000|info=Records: 2  Duplicates: 0  Warnings: 0|stmt-warn=n|integer,double,string|default=string"
    );
}

/// MYSQLI_ASYNC queries complete on send; `mysqli_poll()` reports links with an
/// unreaped query as readable and the rest as rejected, and a failing async
/// query is reported by `reap_async_query()` (where mysqlnd surfaces it).
#[test]
#[ignore]
fn test_mysqli_async_query_poll_and_reap() {
    let out = compile_and_run(&my_program(
        r#"
$other = new mysqli($host, $user, $pass, $dbname, $port);
echo $db->query("SELECT 41 + 1 AS x", MYSQLI_ASYNC) ? "sent" : "no";
$read = [$db, $other];
$error = [$db, $other];
$reject = [];
echo "|ready=", mysqli_poll($read, $error, $reject, 1), "|errors=", count($error), "|rejected=", count($reject);
$result = $db->reap_async_query();
echo "|", $result->fetch_row()[0];
$db->query("SELECT * FROM no_such_table_xyz", MYSQLI_ASYNC);
try {
    $db->reap_async_query();
    echo "|no-throw";
} catch (mysqli_sql_exception $e) {
    echo "|reap-error=", $e->getCode();
}
"#,
    ));
    assert_eq!(out, "sent|ready=1|errors=0|rejected=1|42|reap-error=1146");
}

/// The connection-maintenance API: a real COM_PING, COM_CHANGE_USER (which
/// resets session state), killing another connection, `refresh()`, the
/// public `mysqli_result` constructor over a `real_query()`, MYSQLI_USE_RESULT
/// observability, and the mysqlnd-style statistics. MySQL 8.4 removed
/// COM_PROCESS_KILL and COM_REFRESH, which mysqli sends like mysqlnd: there the
/// server answers 1047 "Unknown command" (php 8.3 behaves the same).
#[test]
#[ignore]
fn test_mysqli_connection_maintenance_and_stats() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->ping() ? "ping" : "no";
$db->query("SET @marker = 5");
echo "|change=", $db->change_user($user, $pass, $dbname) ? "ok" : "no";
$row = $db->query("SELECT @marker IS NULL AS cleared")->fetch_row();
echo "|session-reset=", $row[0];
$victim = new mysqli($host, $user, $pass, $dbname, $port);
mysqli_report(MYSQLI_REPORT_OFF);
$killed = $db->kill($victim->thread_id);
echo "|kill=", $killed ? "ok" : ($db->errno == 1047 ? "removed" : "no");
// FLUSH TABLES needs the RELOAD privilege: either it succeeds or the server's
// own 1227 "Access denied" comes back (with its plain server text).
$refreshed = $db->refresh(MYSQLI_REFRESH_TABLES);
echo "|refresh=", $refreshed || ($db->errno == 1227 && str_starts_with($db->error, "Access denied")) ? "ok" : ($db->errno == 1047 ? "removed" : "no");
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
$db->real_query("SELECT 1 AS a UNION SELECT 2");
$result = new mysqli_result($db);
echo "|ctor=", $result->num_rows, ":", $result->type;
$unbuffered = $db->query("SELECT 1 AS a UNION SELECT 2", MYSQLI_USE_RESULT);
echo "|use=", $unbuffered->type, ":", $unbuffered->num_rows;
try {
    mysqli_num_rows($unbuffered);
} catch (Error $e) {
    echo "|use-num-rows-error";
}
while ($unbuffered->fetch_row()) {
}
echo "|after=", $unbuffered->num_rows;
$stats = $db->get_connection_stats();
echo "|stats=", count($stats), ":", ((int) $stats["com_query"]) > 0 ? "q" : "-", ":", $stats["connect_success"];
$links = mysqli_get_links_stats();
echo "|links=", $links["total"] >= 1 ? "y" : "n";
"#,
    ));
    let tail = "|ctor=2:0|use=1:0|use-num-rows-error|after=2|stats=163:q:1|links=y";
    assert!(
        [
            format!("ping|change=ok|session-reset=1|kill=ok|refresh=ok{tail}"),
            format!("ping|change=ok|session-reset=1|kill=removed|refresh=removed{tail}"),
        ]
        .contains(&out),
        "unexpected output: {out}"
    );
}

/// A `CALL` returning several result sets walks them through the statement's
/// `more_results()` / `next_result()`, like mysqlnd.
#[test]
#[ignore]
fn test_mysqli_stmt_multiple_result_sets() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP PROCEDURE IF EXISTS msp_two");
$db->multi_query("CREATE PROCEDURE msp_two() BEGIN SELECT 1 AS a; SELECT 2 AS b, 3 AS c; END");
while ($db->more_results()) {
    $db->next_result();
}
$stmt = $db->prepare("CALL msp_two()");
$stmt->execute();
$sets = 0;
do {
    $result = $stmt->get_result();
    if ($result instanceof mysqli_result) {
        $sets++;
        echo "set", $sets, "=", implode(",", $result->fetch_row()), ";";
    }
} while ($stmt->more_results() && $stmt->next_result());
echo "sets=", $sets;
$stmt->close();
$db->query("DROP PROCEDURE msp_two");
"#,
    ));
    assert_eq!(out, "set1=1;set2=2,3;sets=2");
}

/// `$error` / `mysqli_error()` carry the server's own message text, like php
/// (not the bridge's `MySqlError { … }` rendering), on the connection and on a
/// statement.
#[test]
#[ignore]
fn test_mysqli_error_text_is_the_server_message() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("SELECT * FROM no_such_table_err");
echo $db->errno, "|", $db->sqlstate, "|", $db->error, "\n";
$stmt = $db->prepare("SELECT * FROM no_such_table_err2");
echo var_export($stmt, true), "|", mysqli_error($db), "\n";
mysqli_report(MYSQLI_REPORT_STRICT);
try {
    $db->query("SELECT * FROM no_such_table_err3");
} catch (mysqli_sql_exception $e) {
    echo $e->getCode(), "|", $e->getSqlState(), "|", $e->getMessage();
}
"#,
    ));
    assert_eq!(
        out,
        "1146|42S02|Table 'testdb.no_such_table_err' doesn't exist\nfalse|Table 'testdb.no_such_table_err2' doesn't exist\n1146|42S02|Table 'testdb.no_such_table_err3' doesn't exist"
    );
}

/// MYSQLI_OPT_READ_TIMEOUT bounds the socket read: a statement that outlives it
/// fails fast with mysqlnd's errno 2006 instead of waiting for the server.
#[test]
#[ignore]
fn test_mysqli_read_timeout_aborts_a_stalled_read() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$slow = mysqli_init();
echo $slow->options(MYSQLI_OPT_READ_TIMEOUT, 1) ? "opt" : "no";
$slow->real_connect($host, $user, $pass, $dbname, $port);
$started = microtime(true);
$result = $slow->query("SELECT SLEEP(4)");
$elapsed = microtime(true) - $started;
echo "|", $result === false ? "failed" : "ran", "|", $elapsed < 3.0 ? "fast" : "slow";
echo "|", $slow->errno, "|", $slow->error;
"#,
    ));
    assert_eq!(out, "opt|failed|fast|2006|MySQL server has gone away");
}

/// MYSQLI_USE_RESULT / use_result() stream the rows from the server one fetch
/// at a time: the link answers every other command with mysqlnd's 2014 until
/// the set is exhausted or freed (or the result is destroyed), num_rows reads
/// 0 until the end, and real_query() + use_result() / store_result() and
/// multi_query() batches walk the same on-demand protocol.
#[test]
#[ignore]
fn test_mysqli_use_result_streams_and_guards_the_link() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("DROP TABLE IF EXISTS msu");
$db->query("CREATE TABLE msu (id INT)");
$db->query("INSERT INTO msu VALUES (1), (2), (3)");
$r = $db->query("SELECT id FROM msu ORDER BY id", MYSQLI_USE_RESULT);
echo $r->type, ":", $r->num_rows, ":", $db->affected_rows;
echo "|first=", $r->fetch_row()[0];
echo "|busy=", $db->query("SELECT 1") === false ? $db->errno : "ran";
echo "|busy-prepare=", $db->prepare("SELECT 1") === false ? $db->errno : "ran";
$rest = [];
while ($row = $r->fetch_row()) {
    $rest[] = $row[0];
}
echo "|rest=", implode(",", $rest), "|rows=", $r->num_rows, "|affected=", $db->affected_rows;
echo "|free=", $db->query("SELECT 7")->fetch_row()[0];
$partial = $db->query("SELECT id FROM msu", MYSQLI_USE_RESULT);
$partial->fetch_row();
$partial->free();
echo "|after-free=", $db->query("SELECT 8")->fetch_row()[0];
function read_one(mysqli $db): void {
    $result = $db->query("SELECT id FROM msu", MYSQLI_USE_RESULT);
    $result->fetch_row();
}
read_one($db);
echo "|after-scope=", $db->query("SELECT 9")->fetch_row()[0];
$db->real_query("SELECT id FROM msu ORDER BY id DESC");
echo "|pending=", $db->query("SELECT 1") === false ? $db->errno : "ran";
$stream = $db->use_result();
$ids = "";
foreach ($stream as $row) {
    $ids .= $row["id"];
}
echo "|streamed=", $ids;
$db->multi_query("SELECT 1 AS a; INSERT INTO msu VALUES (4); SELECT 2 AS b");
$sets = [];
do {
    $set = $db->use_result();
    if ($set instanceof mysqli_result) {
        $sets[] = $set->fetch_row()[0];
        $set->free();
    } else {
        $sets[] = "ok" . $db->affected_rows;
    }
} while ($db->more_results() && $db->next_result());
echo "|batch=", implode(",", $sets);
$db->query("DROP TABLE msu");
"#,
    ));
    assert_eq!(
        out,
        "1:0:-1|first=1|busy=2014|busy-prepare=2014|rest=2,3|rows=3|affected=3|free=7|after-free=8|after-scope=9|pending=2014|streamed=321|batch=1,ok1,2"
    );
}

/// `mysqli_poll()` / `mysqli::poll()` take php's exact `?array &$read, ?array &$error,
/// array &$reject` signature: typed link arrays are rewritten in place, either list may be
/// `null`, and passing two `null` lists is php's ValueError.
#[test]
#[ignore]
fn test_mysqli_poll_accepts_null_link_lists() {
    let out = compile_and_run(&my_program(
        r#"
$other = new mysqli($host, $user, $pass, $dbname, $port);
$db->query("SELECT 42", MYSQLI_ASYNC);
$read = [$db, $other];
$error = null;
$reject = [];
echo mysqli_poll($read, $error, $reject, 1), "|", count($read), "|", count($reject), "|", var_export($error, true);
echo "|", $read[0]->reap_async_query()->fetch_row()[0];
$noRead = null;
$errors = [$db];
echo "|", mysqli::poll($noRead, $errors, $reject, 0), "|", var_export($noRead, true), "|", count($errors);
try {
    $a = null;
    $b = null;
    mysqli_poll($a, $b, $reject, 0);
} catch (ValueError $e) {
    echo "|", $e->getMessage();
}
"#,
    ));
    assert_eq!(out, "1|1|1|NULL|42|0|NULL|0|No stream arrays were passed");
}

/// MYSQLI_ASYNC queries really run in the background: `query()` returns at once,
/// `mysqli_poll()` waits (with its timeout) until a link's query finished, two links
/// overlap their work, and `reap_async_query()` collects each result.
#[test]
#[ignore]
fn test_mysqli_async_queries_overlap_and_poll_waits() {
    let out = compile_and_run(&my_program(
        r#"
$other = new mysqli($host, $user, $pass, $dbname, $port);
$started = microtime(true);
$db->query("SELECT SLEEP(1) AS s, 'first' AS tag", MYSQLI_ASYNC);
$other->query("SELECT SLEEP(1) AS s, 'second' AS tag", MYSQLI_ASYNC);
echo "sent-fast=", (microtime(true) - $started) < 0.5 ? "y" : "n";
$read = [$db];
$error = [];
$reject = [];
echo "|early=", mysqli_poll($read, $error, $reject, 0, 100000);
$tags = [];
$links = [$db, $other];
while (count($links) > 0) {
    $read = $links;
    $error = [];
    $reject = [];
    if (mysqli_poll($read, $error, $reject, 5) < 1) {
        continue;
    }
    foreach ($read as $link) {
        $row = $link->reap_async_query()->fetch_assoc();
        $tags[] = $row["tag"];
        $remaining = [];
        foreach ($links as $candidate) {
            if ($candidate !== $link) {
                $remaining[] = $candidate;
            }
        }
        $links = $remaining;
    }
}
sort($tags);
$elapsed = microtime(true) - $started;
echo "|tags=", implode(",", $tags), "|overlapped=", $elapsed < 1.8 ? "y" : "n";
echo "|busy=";
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("SELECT SLEEP(0.2)", MYSQLI_ASYNC);
echo $db->query("SELECT 1") === false ? $db->errno : "ran";
$db->reap_async_query();
echo "|after=", $db->query("SELECT 5")->fetch_row()[0];
"#,
    ));
    assert_eq!(out, "sent-fast=y|early=0|tags=first,second|overlapped=y|busy=2014|after=5");
}

/// MYSQLI_REPORT_INDEX reads the OK/EOF packet's server status flags: a full table scan
/// raises php's "No index used in query/prepared statement …" (an exception under STRICT),
/// an indexed lookup does not; `refresh()` and `dump_debug_info()` send real COM_REFRESH /
/// COM_DEBUG commands without mysqli_report dispatch: a refused refresh is `false` +
/// errno, while `dump_debug_info()` is `true` whatever the server answers (mysqlnd).
#[test]
#[ignore]
fn test_mysqli_report_index_and_raw_protocol_commands() {
    let out = compile_and_run(&my_program(
        r#"
$db->query("DROP TABLE IF EXISTS msx");
$db->query("CREATE TABLE msx (id INT PRIMARY KEY, v INT)");
$db->query("INSERT INTO msx VALUES (1, 10), (2, 20)");
mysqli_report(MYSQLI_REPORT_ALL);
$indexed = $db->query("SELECT v FROM msx WHERE id = 1");
echo "indexed=", $indexed->fetch_row()[0];
try {
    $db->query("SELECT v FROM msx WHERE v = 20");
    echo "|scan=quiet";
} catch (mysqli_sql_exception $e) {
    echo "|scan=", $e->getMessage(), "|code=", $e->getCode(), "|state=", $e->getSqlState();
}
$stmt = $db->prepare("SELECT v FROM msx WHERE v > ?");
$min = 0;
$stmt->bind_param("i", $min);
try {
    $stmt->execute();
    echo "|stmt=quiet";
} catch (mysqli_sql_exception $e) {
    echo "|stmt=", str_starts_with($e->getMessage(), "No index used in query/prepared statement SELECT v FROM msx") ? "reported" : $e->getMessage();
}
mysqli_report(MYSQLI_REPORT_OFF);
// The executed statement's rows are still on the server: free them first, or
// (like mysqlnd) the link answers the next command with 2014.
$stmt->free_result();
$refreshed = $db->refresh(MYSQLI_REFRESH_TABLES);
echo "|refresh=", $refreshed ? "ok" : ($db->errno == 1227 ? "denied" : "err" . $db->errno);
$dumped = $db->dump_debug_info();
echo "|debug=", $dumped ? "ok" : ($db->errno == 1227 ? "denied" : "err" . $db->errno);
$db->query("DROP TABLE msx");
"#,
    ));
    let (head, tail) = out.split_once("|refresh=").expect("refresh marker");
    assert_eq!(
        head,
        "indexed=10|scan=No index used in query/prepared statement SELECT v FROM msx WHERE v = 20|code=0|state=00000|stmt=reported"
    );
    // The test user may or may not hold RELOAD / SUPER: either the command ran or the
    // server's own 1227 came back — never a client-side substitute.
    assert!(
        // dump_debug_info() is true whatever the server answers, like mysqlnd;
        // MySQL 8.4 removed COM_REFRESH (1047).
        ["ok|debug=ok", "denied|debug=ok", "err1047|debug=ok"].contains(&tail),
        "unexpected tail: {tail}"
    );
}

/// Point-6 fidelity against a live server: `fetch_object()` assigns the columns
/// before the constructor runs (and rejects constructor args for a class without
/// one), a `BIGINT UNSIGNED` AUTO_INCREMENT id beyond `PHP_INT_MAX` comes back as a
/// numeric string, and the byte / packet counters follow the wire traffic.
#[test]
#[ignore]
fn test_mysqli_fetch_object_order_big_ids_and_wire_stats() {
    let out = compile_and_run(&my_program(
        r#"
class Seen {
    public ?string $id = null;
    public string $before = "";
    public function __construct(public string $tag = "") { $this->before = (string) $this->id; }
}
class Bare { public ?string $id = null; }
$o = $db->query("SELECT '7' AS id")->fetch_object("Seen", ["t"]);
echo "ctor-saw=", $o->before, "|tag=", $o->tag;
try {
    $db->query("SELECT '7' AS id")->fetch_object("Bare", ["x"]);
    echo "|bare=accepted";
} catch (ValueError $e) {
    echo "|bare=", get_class($e), ":", $e->getMessage();
}
$db->query("DROP TABLE IF EXISTS mbig");
$db->query("CREATE TABLE mbig (id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY, v INT)");
$db->query("INSERT INTO mbig VALUES (5, 1)");
echo "|small="; var_dump($db->insert_id);
$db->query("INSERT INTO mbig VALUES (18446744073709551610, 1)");
echo "|big="; var_dump($db->insert_id);
$stmt = $db->prepare("INSERT INTO mbig VALUES (?, 2)");
$id = "18446744073709551611";
$stmt->bind_param("s", $id);
$stmt->execute();
echo "|stmt=", $stmt->insert_id, "|proc=", mysqli_stmt_insert_id($stmt), "|link=", mysqli_insert_id($db);
$db->query("DROP TABLE mbig");
$before = $db->get_connection_stats();
$db->query("SELECT REPEAT('x', 1000)")->fetch_row();
$after = $db->get_connection_stats();
$sent = (int) $after["bytes_sent"] - (int) $before["bytes_sent"];
$received = (int) $after["bytes_received"] - (int) $before["bytes_received"];
echo "|sent=", $sent, "|recv-big=", $received > 1000 ? "y" : "n";
echo "|pkts=", (int) $after["packets_sent"] - (int) $before["packets_sent"];
echo "|overhead=", (int) $after["protocol_overhead_out"] === 4 * (int) $after["packets_sent"] ? "y" : "n";
echo "|client=", (int) mysqli_get_client_stats()["bytes_received"] >= (int) $after["bytes_received"] ? "y" : "n";
"#,
    ));
    assert_eq!(
        out,
        "ctor-saw=7|tag=t|bare=ValueError:mysqli_result::fetch_object(): Argument #2 ($constructor_args) must be empty when the specified class (Bare) does not have a constructor|small=int(5)\n|big=string(20) \"18446744073709551610\"\n|stmt=18446744073709551611|proc=18446744073709551611|link=18446744073709551611|sent=33|recv-big=y|pkts=2|overhead=y|client=y"
    );
}

/// `fetch_object()` into a class whose properties are UNTYPED and never assigned anywhere else
/// (`public $id;`): the columns land in the declared slots, like php, instead of crashing on a
/// duplicate dynamic property.
#[test]
#[ignore]
fn test_mysqli_fetch_object_fills_untyped_declared_properties() {
    let out = compile_and_run(&my_program(
        r#"
class Row { public $id; public $label; public $absent; }
$row = $db->query("SELECT 7 AS id, 'seven' AS label")->fetch_object("Row");
var_dump($row);
"#,
    ));
    assert_eq!(
        out,
        "object(Row)#2 (3) {\n  [\"id\"]=>\n  string(1) \"7\"\n  [\"label\"]=>\n  string(5) \"seven\"\n  [\"absent\"]=>\n  NULL\n}\n"
    );
}

/// `get_charset()` reports mysqlnd's charset-table entry for the session charset: the
/// default collation, its number, the character byte lengths and the comment, with
/// `state` 1 and an empty `dir`, after connecting and after each `set_charset()`.
#[test]
#[ignore]
fn test_mysqli_get_charset_reports_the_mysqlnd_table_entry() {
    let out = compile_and_run(&my_program(
        r#"
$show = function (mixed $c): string {
    return $c->charset . "/" . $c->collation . "/" . $c->number . "/" . $c->min_length . "-"
        . $c->max_length . "/" . $c->state . "/" . $c->dir . "/" . $c->comment;
};
echo $show($db->get_charset());
$db->set_charset("LATIN1");
echo "|", $show(mysqli_get_charset($db));
$db->set_charset("utf8mb4");
echo "|", $show($db->get_charset());
"#,
    ));
    assert_eq!(
        out,
        "utf8mb4/utf8mb4_general_ci/45/1-4/1//UTF-8 Unicode|latin1/latin1_swedish_ci/8/1-1/1//|utf8mb4/utf8mb4_general_ci/45/1-4/1//UTF-8 Unicode"
    );
}

/// Against a live server, `MYSQLI_REPORT_ERROR` / `MYSQLI_REPORT_INDEX` warnings are real
/// `E_WARNING`s with php's exact text: the method or procedural entry point, then
/// `(SQLSTATE/errno): message` (a failed `prepare()` reports the server's own error, not a
/// statement placeholder), or the "No index used …" notice; `@` silences them.
#[test]
#[ignore]
fn test_mysqli_report_warnings_are_php_warnings() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_ERROR);
set_error_handler(function (int $no, string $str): bool {
    echo $no === E_WARNING ? "W:" : "?:", $str, "\n";
    return true;
});
$db->query("SELECT * FROM mwarn_missing");
mysqli_query($db, "SELECT * FROM mwarn_missing");
$db->prepare("SELECT mwarn_nope FROM dual");
mysqli_report(MYSQLI_REPORT_INDEX);
$db->query("DROP TABLE IF EXISTS mwarn");
$db->query("CREATE TABLE mwarn (id INT PRIMARY KEY, v INT)");
$db->query("SELECT * FROM mwarn WHERE v = 1");
mysqli_report(MYSQLI_REPORT_ERROR);
restore_error_handler();
set_error_handler(function (int $no, string $str): bool {
    echo "silenced-but-seen:", error_reporting() === 0 ? "0" : "mask", "\n";
    return true;
});
@$db->query("SELECT * FROM mwarn_missing");
$db->query("DROP TABLE mwarn");
"#,
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "W:mysqli::query(): (42S02/1146): Table 'testdb.mwarn_missing' doesn't exist");
    assert_eq!(lines[1], "W:mysqli_query(): (42S02/1146): Table 'testdb.mwarn_missing' doesn't exist");
    assert!(
        lines[2].starts_with("W:mysqli::prepare(): (42S22/1054): Unknown column 'mwarn_nope'"),
        "{}",
        lines[2]
    );
    assert_eq!(lines[3], "W:mysqli::query(): No index used in query/prepared statement SELECT * FROM mwarn WHERE v = 1");
    assert_eq!(lines.len(), 5, "{out}");
}

/// The wire statistics follow mysqlnd's own accounting, checked against PHP 8.3 mysqlnd on
/// the same server: a command counts `physical + 1` packets and `payload + 4` bytes for
/// each, and every received packet is charged to the counter of the mysqlnd reader that
/// consumes it through php-src's shifted per-type tables — result headers under
/// `rset_row`, field definitions under `prepare_response`, rows under `change_user` (the
/// closing EOF as its 4 header bytes), EOFs under `rset_header`, OK packets nowhere.
/// `stmt->close()` really sends COM_STMT_CLOSE, and a re-prepare reaches the server.
#[test]
#[ignore]
fn test_mysqli_wire_statistics_follow_mysqlnd() {
    let out = compile_and_run(&my_program(
        r#"
$keys = ["packets_sent", "packets_sent_command", "packets_received_rset_row",
    "packets_received_prepare_response", "packets_received_rset_header",
    "packets_received_change_user", "packets_received_ok", "bytes_received_change_user_packet"];
$prev = $db->get_connection_stats();
$step = function (string $label) use ($db, $keys, &$prev): void {
    $now = $db->get_connection_stats();
    echo $label;
    foreach ($keys as $k) {
        echo " ", (int) $now[$k] - (int) $prev[$k];
    }
    echo "|";
    $prev = $now;
};
$db->query("SELECT REPEAT('x', 1000) AS a, 2 AS b")->fetch_all();
$step("query");
$db->ping();
$step("ping");
$s = $db->prepare("SELECT ? + 1 AS r, 'z' AS c");
$step("prepare");
$v = 5;
$s->bind_param("i", $v);
$s->execute();
$s->get_result()->fetch_all();
$step("execute");
$s->close();
$step("close");
$again = $db->prepare("SELECT 1");
$again->close();
$again = $db->prepare("SELECT 1");
$step("reprepare");
$out = $db->get_connection_stats();
echo (int) $out["protocol_overhead_out"] === 4 * (int) $out["packets_sent"] ? "overhead" : "?";
"#,
    ));
    assert_eq!(
        out,
        "query 2 1 1 2 1 2 0 1013|ping 2 1 0 0 0 0 0 0|prepare 2 1 0 3 2 0 0 0|execute 2 1 1 2 1 2 0 20|close 2 1 0 0 0 0 0 0|reprepare 6 3 0 2 2 0 0 0|overhead"
    );
}

/// Like mysqlnd, a prepared statement's result set stays on the server after `execute()`:
/// any other command on the link — a query, another prepare — gets errno 2014 until the
/// rows are fetched to the end, stored (`store_result()` / `get_result()`), freed, or the
/// statement is re-executed or closed. Expected output is php 8.3 mysqlnd's.
#[test]
#[ignore]
fn test_mysqli_statement_results_stay_on_the_server() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$s = $db->prepare("SELECT 1 AS a UNION SELECT 2");
$s->execute();
var_dump($db->query("SELECT 9"), $db->errno);
$t = $db->prepare("SELECT 3"); var_dump($t, $db->errno);
$a = null; $s->bind_result($a);
while ($s->fetch()) echo $a, ",";
echo "\n";
var_dump($db->query("SELECT 9")->fetch_row()[0]);
$s->execute(); $s->store_result(); var_dump($s->num_rows, $db->query("SELECT 10")->fetch_row()[0]);
$s->execute(); $r = $s->get_result(); var_dump($db->query("SELECT 11")->fetch_row()[0], count($r->fetch_all()));
$s->execute(); $s->free_result(); var_dump($db->query("SELECT 12")->fetch_row()[0]);
$s->execute(); $s->execute(); $s->store_result(); var_dump($s->num_rows);
$s->execute(); $s->close(); var_dump($db->query("SELECT 13")->fetch_row()[0]);
$u = $db->prepare("INSERT INTO nonexist VALUES (1)"); var_dump($u);
"#,
    ));
    assert_eq!(
        out,
        r#"bool(false)
int(2014)
bool(false)
int(2014)
1,2,
string(1) "9"
int(2)
string(2) "10"
string(2) "11"
int(2)
string(2) "12"
int(2)
string(2) "13"
bool(false)
"#
    );
}

/// Like mysqlnd, results are read only when the program consumes them: `multi_query()`
/// reads the first set's header and metadata, `store_result()` its rows, `next_result()`
/// the next set's header and metadata, `use_result()` nothing and each `fetch_row()` one
/// row — so the wire counters move at the same calls as php's (packets per call checked
/// against php 8.3 mysqlnd), with and without MYSQLI_CLIENT_COMPRESS.
#[test]
#[ignore]
fn test_mysqli_results_are_read_when_consumed() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
foreach ([0, MYSQLI_CLIENT_COMPRESS] as $flags) {
    $link = mysqli_init();
    $link->real_connect($host, $user, $pass, $dbname, $port, null, $flags);
    $prev = $link->get_connection_stats();
    $step = function (string $label) use ($link, &$prev): void {
        $now = $link->get_connection_stats();
        echo $label, ":", (int) $now["packets_received"] - (int) $prev["packets_received"], "|";
        $prev = $now;
    };
    $link->multi_query("SELECT 1 UNION SELECT 2; SELECT 3; DO 4; SELECT 5");
    $step("mq");
    $r = $link->store_result();
    $step("store1");
    $r->free();
    $link->next_result();
    $step("next2");
    $r = $link->use_result();
    $step("use2");
    $r->fetch_row();
    $step("fetch2");
    $r->free();
    $step("free2");
    $link->next_result();
    $step("next3");
    $link->next_result();
    $step("next4");
    $r = $link->store_result();
    $step("store4");
    echo $link->more_results() ? "more" : "done", "\n";
    $link->close();
}
"#,
    ));
    let expected = "mq:4|store1:3|next2:3|use2:0|fetch2:1|free2:1|next3:1|next4:3|store4:2|done\n";
    assert_eq!(out, format!("{expected}{expected}"));
}

/// Per-step mysqlnd statistics across the whole result and statement lifecycle
/// (decode-time protocol counters, data bytes, buffered / unbuffered / flushed
/// sets, free and close accounting), connection and process-wide. The golden
/// output was produced by php 8.3 + mysqlnd running this same program.
#[test]
#[ignore]
fn test_mysqli_lifecycle_statistics_follow_mysqlnd() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$wire = ["bytes_sent","bytes_received","packets_sent","packets_received","protocol_overhead_in","protocol_overhead_out","packets_sent_command"];
$prevC = $db->get_connection_stats();
$prevG = mysqli_get_client_stats();
$step = function (string $label) use ($db, &$prevC, &$prevG, $wire): void {
    $c = $db->get_connection_stats();
    $g = mysqli_get_client_stats();
    $out = [];
    foreach ($c as $k => $v) {
        if (in_array($k, $wire) || str_starts_with($k, "bytes_received_") && str_ends_with($k, "_packet") || str_starts_with($k, "packets_received_")) { continue; }
        $d = (int) $v - (int) $prevC[$k];
        if ($d != 0) { $out[] = "c." . $k . "=" . $d; }
    }
    foreach ($g as $k => $v) {
        if (in_array($k, $wire) || str_starts_with($k, "bytes_received_") && str_ends_with($k, "_packet") || str_starts_with($k, "packets_received_")) { continue; }
        $d = (int) $v - (int) $prevG[$k];
        if ($d != 0) { $out[] = "g." . $k . "=" . $d; }
    }
    echo $label, ": ", implode(" ", $out), "\n";
    $prevC = $c;
    $prevG = $g;
};
$db->query("DROP TABLE IF EXISTS mysqli_life_t");
$db->query("CREATE TABLE mysqli_life_t (i INT, b BIGINT, d DECIMAL(6,2), f DOUBLE, s VARCHAR(10), bl BLOB, dt DATETIME, da DATE, t TINYINT, n INT NULL, e ENUM('a','b'), y YEAR, bt BIT(3), fl FLOAT, sm SMALLINT, md MEDIUMINT, tm TIME, ts TIMESTAMP NULL)");
$step("ddl");
$db->query("INSERT INTO mysqli_life_t VALUES (1, 2, 3.5, 4.25, 'abc', 'xyz', '2024-01-02 03:04:05', '2024-01-02', 7, NULL, 'b', 2024, 5, 1.5, 9, 10, '01:02:03', '2024-01-02 03:04:05'), (2, 3, 4.5, 5.25, 'de', NULL, NULL, NULL, 8, 1, 'a', 2025, 1, 2.5, 3, 4, '05:06:07', NULL)");
$step("insert");
$r = $db->query("SELECT * FROM mysqli_life_t");
$step("query");
$r->fetch_row();
$step("fetch1");
$r->data_seek(0);
$r->fetch_assoc();
$step("refetch");
$r->fetch_all();
$step("fetchall");
$r->free();
$step("free");
$r = $db->query("SELECT i, s FROM mysqli_life_t", MYSQLI_USE_RESULT);
$step("use");
$r->fetch_row();
$step("ufetch");
$r->free();
$step("ufree");
$r = $db->query("SELECT i FROM mysqli_life_t");
unset($r);
$step("implicit");
$db->real_query("SELECT s FROM mysqli_life_t");
$r = $db->store_result();
$step("real+store");
$r->close();
$db->query("UPDATE mysqli_life_t SET i = i + 1");
$step("update");
$s = $db->prepare("SELECT * FROM mysqli_life_t WHERE i > ?");
$step("prepare");
$v = 0; $s->bind_param("i", $v);
$s->execute();
$step("execute");
$s->fetch();
$step("sfetch1");
$s->fetch();
$s->fetch();
$step("sfetchend");
$s->execute();
$s->store_result();
$step("sstore");
$s->fetch();
$step("sfetchstored");
$s->free_result();
$step("sfree");
$s->execute();
$res = $s->get_result();
$step("getresult");
$res->fetch_all();
$step("gfetchall");
$res->free();
$s->execute();
$s->fetch();
$s->close();
$step("sclose_pending");
$u = $db->prepare("UPDATE mysqli_life_t SET i = i + 1 WHERE i > ?");
$u->bind_param("i", $v);
$u->execute();
$step("supdate");
$u->close();
$step("uclose");
$never = $db->prepare("SELECT 1");
$never->close();
$step("never");
$twice = $db->prepare("SELECT 1");
$twice->execute(); $twice->store_result(); $twice->execute(); $twice->store_result();
$twice->close();
$step("twice");
$db->multi_query("SELECT 1; UPDATE mysqli_life_t SET i = i; SELECT 2");
do { if ($x = $db->store_result()) { $x->free(); } } while ($db->next_result());
$step("multi");
$db->ping();
$db->select_db("testdb");
$step("ping+select");
$db->query("DROP TABLE mysqli_life_t");
$db->close();
$g = mysqli_get_client_stats();
echo "end-close: g.explicit_close=", $g["explicit_close"] - $prevG["explicit_close"], "\n";
"#,
    ));
    assert_eq!(
        out,
        r#"ddl: c.non_result_set_queries=2 c.com_query=2 g.non_result_set_queries=2 g.com_query=2
insert: c.non_result_set_queries=1 c.rows_affected_normal=2 c.com_query=1 g.non_result_set_queries=1 g.rows_affected_normal=2 g.com_query=1
query: c.result_set_queries=1 c.no_index_used=1 c.buffered_sets=1 c.rows_fetched_from_server_normal=2 c.rows_buffered_from_client_normal=2 c.com_query=1 g.result_set_queries=1 g.no_index_used=1 g.buffered_sets=1 g.rows_fetched_from_server_normal=2 g.rows_buffered_from_client_normal=2 g.com_query=1
fetch1: c.proto_text_fetched_bit=1 c.proto_text_fetched_tinyint=1 c.proto_text_fetched_short=1 c.proto_text_fetched_int24=1 c.proto_text_fetched_int=1 c.proto_text_fetched_bigint=1 c.proto_text_fetched_decimal=1 c.proto_text_fetched_float=1 c.proto_text_fetched_double=1 c.proto_text_fetched_date=1 c.proto_text_fetched_year=1 c.proto_text_fetched_time=1 c.proto_text_fetched_datetime=1 c.proto_text_fetched_timestamp=1 c.proto_text_fetched_string=2 c.proto_text_fetched_enum=1 c.bytes_received_real_data_normal=85 g.rows_fetched_from_client_normal_buffered=1 g.proto_text_fetched_bit=1 g.proto_text_fetched_tinyint=1 g.proto_text_fetched_short=1 g.proto_text_fetched_int24=1 g.proto_text_fetched_int=1 g.proto_text_fetched_bigint=1 g.proto_text_fetched_decimal=1 g.proto_text_fetched_float=1 g.proto_text_fetched_double=1 g.proto_text_fetched_date=1 g.proto_text_fetched_year=1 g.proto_text_fetched_time=1 g.proto_text_fetched_datetime=1 g.proto_text_fetched_timestamp=1 g.proto_text_fetched_string=2 g.proto_text_fetched_enum=1 g.bytes_received_real_data_normal=85
refetch: c.proto_text_fetched_bit=1 c.proto_text_fetched_tinyint=1 c.proto_text_fetched_short=1 c.proto_text_fetched_int24=1 c.proto_text_fetched_int=1 c.proto_text_fetched_bigint=1 c.proto_text_fetched_decimal=1 c.proto_text_fetched_float=1 c.proto_text_fetched_double=1 c.proto_text_fetched_date=1 c.proto_text_fetched_year=1 c.proto_text_fetched_time=1 c.proto_text_fetched_datetime=1 c.proto_text_fetched_timestamp=1 c.proto_text_fetched_string=2 c.proto_text_fetched_enum=1 c.bytes_received_real_data_normal=85 g.rows_fetched_from_client_normal_buffered=1 g.proto_text_fetched_bit=1 g.proto_text_fetched_tinyint=1 g.proto_text_fetched_short=1 g.proto_text_fetched_int24=1 g.proto_text_fetched_int=1 g.proto_text_fetched_bigint=1 g.proto_text_fetched_decimal=1 g.proto_text_fetched_float=1 g.proto_text_fetched_double=1 g.proto_text_fetched_date=1 g.proto_text_fetched_year=1 g.proto_text_fetched_time=1 g.proto_text_fetched_datetime=1 g.proto_text_fetched_timestamp=1 g.proto_text_fetched_string=2 g.proto_text_fetched_enum=1 g.bytes_received_real_data_normal=85
fetchall: c.proto_text_fetched_bit=1 c.proto_text_fetched_tinyint=1 c.proto_text_fetched_short=1 c.proto_text_fetched_int24=1 c.proto_text_fetched_int=2 c.proto_text_fetched_bigint=1 c.proto_text_fetched_decimal=1 c.proto_text_fetched_float=1 c.proto_text_fetched_double=1 c.proto_text_fetched_year=1 c.proto_text_fetched_time=1 c.proto_text_fetched_string=2 c.bytes_received_real_data_normal=33 g.rows_fetched_from_client_normal_buffered=1 g.proto_text_fetched_bit=1 g.proto_text_fetched_tinyint=1 g.proto_text_fetched_short=1 g.proto_text_fetched_int24=1 g.proto_text_fetched_int=2 g.proto_text_fetched_bigint=1 g.proto_text_fetched_decimal=1 g.proto_text_fetched_float=1 g.proto_text_fetched_double=1 g.proto_text_fetched_year=1 g.proto_text_fetched_time=1 g.proto_text_fetched_string=2 g.bytes_received_real_data_normal=33
free: c.explicit_free_result=1 g.explicit_free_result=1
use: c.result_set_queries=1 c.no_index_used=1 c.unbuffered_sets=1 c.com_query=1 g.result_set_queries=1 g.no_index_used=1 g.unbuffered_sets=1 g.com_query=1
ufetch: c.rows_fetched_from_server_normal=1 c.rows_fetched_from_client_normal_unbuffered=1 c.proto_text_fetched_int=1 c.proto_text_fetched_string=1 c.bytes_received_real_data_normal=4 g.rows_fetched_from_server_normal=1 g.rows_fetched_from_client_normal_unbuffered=1 g.proto_text_fetched_int=1 g.proto_text_fetched_string=1 g.bytes_received_real_data_normal=4
ufree: c.flushed_normal_sets=1 c.rows_fetched_from_server_normal=1 c.rows_fetched_from_client_normal_unbuffered=1 c.rows_skipped_normal=1 c.explicit_free_result=1 g.flushed_normal_sets=1 g.rows_fetched_from_server_normal=1 g.rows_fetched_from_client_normal_unbuffered=1 g.rows_skipped_normal=1 g.explicit_free_result=1
implicit: c.result_set_queries=1 c.no_index_used=1 c.buffered_sets=1 c.rows_fetched_from_server_normal=2 c.rows_buffered_from_client_normal=2 c.explicit_free_result=1 c.com_query=1 g.result_set_queries=1 g.no_index_used=1 g.buffered_sets=1 g.rows_fetched_from_server_normal=2 g.rows_buffered_from_client_normal=2 g.explicit_free_result=1 g.com_query=1
real+store: c.result_set_queries=1 c.no_index_used=1 c.buffered_sets=1 c.rows_fetched_from_server_normal=2 c.rows_buffered_from_client_normal=2 c.com_query=1 g.result_set_queries=1 g.no_index_used=1 g.buffered_sets=1 g.rows_fetched_from_server_normal=2 g.rows_buffered_from_client_normal=2 g.com_query=1
update: c.non_result_set_queries=1 c.rows_affected_normal=2 c.explicit_free_result=1 c.com_query=1 g.non_result_set_queries=1 g.rows_affected_normal=2 g.explicit_free_result=1 g.com_query=1
prepare: c.com_stmt_prepare=1 g.com_stmt_prepare=1
execute: c.result_set_queries=1 c.no_index_used=1 c.com_stmt_execute=1 g.result_set_queries=1 g.no_index_used=1 g.com_stmt_execute=1
sfetch1: c.ps_unbuffered_sets=1 c.rows_fetched_from_server_ps=1 c.rows_fetched_from_client_ps_unbuffered=1 g.ps_unbuffered_sets=1 g.rows_fetched_from_server_ps=1 g.rows_fetched_from_client_ps_unbuffered=1
sfetchend: c.rows_fetched_from_server_ps=1 c.rows_fetched_from_client_ps_unbuffered=1 g.rows_fetched_from_server_ps=1 g.rows_fetched_from_client_ps_unbuffered=1
sstore: c.result_set_queries=1 c.no_index_used=1 c.ps_buffered_sets=1 c.rows_fetched_from_server_ps=2 c.rows_buffered_from_client_ps=2 c.com_stmt_execute=1 g.result_set_queries=1 g.no_index_used=1 g.ps_buffered_sets=1 g.rows_fetched_from_server_ps=2 g.rows_buffered_from_client_ps=2 g.com_stmt_execute=1
sfetchstored: g.rows_fetched_from_client_ps_buffered=1
sfree: 
getresult: c.result_set_queries=1 c.no_index_used=1 c.buffered_sets=1 c.rows_fetched_from_server_ps=2 c.rows_buffered_from_client_ps=2 c.com_stmt_execute=1 g.result_set_queries=1 g.no_index_used=1 g.buffered_sets=1 g.rows_fetched_from_server_ps=2 g.rows_buffered_from_client_ps=2 g.com_stmt_execute=1
gfetchall: c.proto_binary_fetched_bit=5 c.proto_binary_fetched_tinyint=2 c.proto_binary_fetched_short=2 c.proto_binary_fetched_int24=2 c.proto_binary_fetched_int=2 c.proto_binary_fetched_bigint=3 c.proto_binary_fetched_decimal=2 c.proto_binary_fetched_float=2 c.proto_binary_fetched_double=2 c.proto_binary_fetched_date=2 c.proto_binary_fetched_year=1 c.proto_binary_fetched_time=2 c.proto_binary_fetched_datetime=2 c.proto_binary_fetched_timestamp=1 c.proto_binary_fetched_string=1 c.proto_binary_fetched_json=4 c.proto_binary_fetched_blob=1 c.bytes_received_real_data_ps=160 g.rows_fetched_from_client_ps_buffered=2 g.proto_binary_fetched_bit=5 g.proto_binary_fetched_tinyint=2 g.proto_binary_fetched_short=2 g.proto_binary_fetched_int24=2 g.proto_binary_fetched_int=2 g.proto_binary_fetched_bigint=3 g.proto_binary_fetched_decimal=2 g.proto_binary_fetched_float=2 g.proto_binary_fetched_double=2 g.proto_binary_fetched_date=2 g.proto_binary_fetched_year=1 g.proto_binary_fetched_time=2 g.proto_binary_fetched_datetime=2 g.proto_binary_fetched_timestamp=1 g.proto_binary_fetched_string=1 g.proto_binary_fetched_json=4 g.proto_binary_fetched_blob=1 g.bytes_received_real_data_ps=160
sclose_pending: c.result_set_queries=1 c.no_index_used=1 c.ps_unbuffered_sets=1 c.flushed_ps_sets=1 c.rows_fetched_from_server_ps=2 c.rows_fetched_from_client_ps_unbuffered=2 c.rows_skipped_ps=1 c.explicit_free_result=1 c.implicit_free_result=1 c.com_stmt_execute=1 c.com_stmt_close=1 g.result_set_queries=1 g.no_index_used=1 g.ps_unbuffered_sets=1 g.flushed_ps_sets=1 g.rows_fetched_from_server_ps=2 g.rows_fetched_from_client_ps_unbuffered=2 g.rows_skipped_ps=1 g.explicit_free_result=2 g.implicit_free_result=1 g.explicit_stmt_close=1 g.com_stmt_execute=1 g.com_stmt_close=1
supdate: c.non_result_set_queries=1 c.com_stmt_prepare=1 c.com_stmt_execute=1 g.non_result_set_queries=1 g.com_stmt_prepare=1 g.com_stmt_execute=1
uclose: c.ps_prepared_once_executed=1 c.com_stmt_close=1 g.ps_prepared_once_executed=1 g.explicit_free_result=1 g.explicit_stmt_close=1 g.com_stmt_close=1
never: c.ps_prepared_never_executed=1 c.implicit_free_result=1 c.com_stmt_prepare=1 c.com_stmt_close=1 g.ps_prepared_never_executed=1 g.explicit_free_result=1 g.implicit_free_result=1 g.explicit_stmt_close=1 g.com_stmt_prepare=1 g.com_stmt_close=1
twice: c.result_set_queries=2 c.ps_buffered_sets=2 c.rows_fetched_from_server_ps=2 c.rows_buffered_from_client_ps=2 c.implicit_free_result=1 c.com_stmt_prepare=1 c.com_stmt_execute=2 c.com_stmt_close=1 g.result_set_queries=2 g.ps_buffered_sets=2 g.rows_fetched_from_server_ps=2 g.rows_buffered_from_client_ps=2 g.explicit_free_result=1 g.implicit_free_result=1 g.explicit_stmt_close=1 g.com_stmt_prepare=1 g.com_stmt_execute=2 g.com_stmt_close=1
multi: c.result_set_queries=2 c.non_result_set_queries=1 c.buffered_sets=2 c.rows_fetched_from_server_normal=2 c.rows_buffered_from_client_normal=2 c.explicit_free_result=2 c.com_query=1 c.com_stmt_set_option=1 g.result_set_queries=2 g.non_result_set_queries=1 g.buffered_sets=2 g.rows_fetched_from_server_normal=2 g.rows_buffered_from_client_normal=2 g.explicit_free_result=2 g.com_query=1 g.com_stmt_set_option=1
ping+select: c.com_init_db=1 c.com_ping=1 g.com_init_db=1 g.com_ping=1
end-close: g.explicit_close=1
"#
    );
}

/// mysqlnd's cell decoding: BIT becomes a decimal string (text) or an integer
/// (binary / INT_AND_FLOAT_NATIVE), ZEROFILL integers stay padded strings,
/// unsigned values past PHP_INT_MAX stay strings, and fields carry NUM_FLAG.
/// Golden output from php 8.3 + mysqlnd.
#[test]
#[ignore]
fn test_mysqli_cell_decoding_follows_mysqlnd() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$db->query("DROP TABLE IF EXISTS mysqli_decode_t");
$db->query("CREATE TABLE mysqli_decode_t (a INT(5) ZEROFILL, b TINYINT ZEROFILL, c BIGINT UNSIGNED ZEROFILL, y YEAR, bt BIT(3), b64 BIT(64), bt9 BIT(9), fz FLOAT ZEROFILL, u BIGINT UNSIGNED, d DECIMAL(4,1), ts TIMESTAMP NULL, n INT NULL)");
$db->query("INSERT INTO mysqli_decode_t VALUES (7, 3, 12, 2024, 5, b'1111111111111111111111111111111111111111111111111111111111111111', 300, 1.5, 18446744073709551615, 2.5, NULL, NULL), (0, 0, 0, 1999, 0, 0, 1, 0, 1, 0, NULL, 4)");
$show = function (string $label, ?array $row): void {
    echo $label, ":";
    foreach ((array) $row as $v) {
        echo " ", gettype($v), "(", var_export($v, true), ")";
    }
    echo "\n";
};
$q = "SELECT * FROM mysqli_decode_t ORDER BY y DESC";
$r = $db->query($q);
$show("text", $r->fetch_row());
$show("text", $r->fetch_row());
foreach ($r->fetch_fields() as $f) {
    // A TIMESTAMP's flags are the server's own (MariaDB adds UNSIGNED_FLAG).
    if ($f->name !== "ts") {
        echo $f->name, "=", $f->type, "/", $f->flags, " ";
    }
}
echo "\n";
$u = $db->query($q, MYSQLI_USE_RESULT);
$show("unbuf", $u->fetch_row());
$u->free();
$s = $db->prepare($q);
$s->execute();
$g = $s->get_result();
$show("binary", $g->fetch_row());
$show("binary", $g->fetch_row());
$a = $b = $c = $y = $bt = $b64 = $bt9 = $fz = $uu = $d = $ts = $n = null;
$s->execute();
$s->bind_result($a, $b, $c, $y, $bt, $b64, $bt9, $fz, $uu, $d, $ts, $n);
$s->fetch();
$show("bound", [$a, $b, $c, $y, $bt, $b64, $bt9, $fz, $uu, $d, $ts, $n]);
$s->close();
$native = mysqli_init();
$native->options(MYSQLI_OPT_INT_AND_FLOAT_NATIVE, 1);
$native->real_connect($host, $user, $pass, $dbname, $port);
$show("native", $native->query($q)->fetch_row());
$db->query("DROP TABLE mysqli_decode_t");
"#,
    ));
    assert_eq!(
        out,
        r#"text: string('00007') string('003') string('00000000000000000012') string('2024') string('5') string('18446744073709551615') string('300') string('0000000001.5') string('18446744073709551615') string('2.5') NULL(NULL) NULL(NULL)
text: string('00000') string('000') string('00000000000000000000') string('1999') string('0') string('0') string('1') string('000000000000') string('1') string('0.0') NULL(NULL) string('4')
a=3/32864 b=1/32864 c=8/32864 y=13/32864 bt=16/32 b64=16/32 bt9=16/32 fz=4/32864 u=8/32800 d=246/0 n=3/32768 
unbuf: string('00007') string('003') string('00000000000000000012') string('2024') string('5') string('18446744073709551615') string('300') string('0000000001.5') string('18446744073709551615') string('2.5') NULL(NULL) NULL(NULL)
binary: string('00007') string('003') string('00000000000000000012') string('2024') integer(5) string('18446744073709551615') integer(300) double(1.5) string('18446744073709551615') string('2.5') NULL(NULL) NULL(NULL)
binary: string('00000') string('000') string('00000000000000000000') string('1999') integer(0) integer(0) integer(1) double(0.0) integer(1) string('0.0') NULL(NULL) integer(4)
bound: string('00007') string('003') string('00000000000000000012') string('2024') integer(5) string('18446744073709551615') integer(300) double(1.5) string('18446744073709551615') string('2.5') NULL(NULL) NULL(NULL)
native: string('00007') string('003') string('00000000000000000012') string('2024') integer(5) string('18446744073709551615') integer(300) double(1.5) string('18446744073709551615') string('2.5') NULL(NULL) NULL(NULL)
"#
    );
}

/// The link lifecycle follows php-src's mysqli_common_connect and mysqlnd:
/// connecting a connected link closes it implicitly and starts over on a fresh
/// handle (every option back to its default), a failed reconnect leaves a
/// connected link that answers 2014, a failed constructor closes the object
/// while a failed real_connect() on mysqli_init() leaves it unconnected,
/// connect() reopens a closed object, and the connection counters move like
/// mysqlnd's (mysqli_init() handles count as persistent, active_connections
/// drops twice per close). Golden output from php 8.3 + mysqlnd.
#[test]
#[ignore]
fn test_mysqli_link_lifecycle_follows_mysqlnd() {
    let out = compile_and_run(&my_program(
        r#"
mysqli_report(MYSQLI_REPORT_OFF);
$keys = ["connect_success", "connect_failure", "implicit_close", "explicit_close", "pconnect_success", "active_connections", "active_persistent_connections", "com_quit"];
$signed = function (string $v): int {
    if (strlen($v) < 19) {
        return (int) $v;
    }
    return -((18446744073 - (int) substr($v, 0, -9)) * 1000000000 + (709551616 - (int) substr($v, -9)));
};
$prev = mysqli_get_client_stats();
$delta = function (string $label) use (&$prev, $keys, $signed): void {
    $now = mysqli_get_client_stats();
    echo $label, ":";
    foreach ($keys as $k) {
        $d = $signed((string) $now[$k]) - $signed((string) $prev[$k]);
        if ($d != 0) {
            echo " ", $k, "=", $d;
        }
    }
    echo "\n";
    $prev = $now;
};
$kind = function (mysqli $link): string {
    $row = $link->query("SELECT 1, @elephc_init")->fetch_row();
    return gettype($row[0]) . "/" . var_export($row[1], true);
};
$attempt = function (string $label, Closure $call): void {
    try {
        $result = (string) $call();
        echo $label, ": ", $result, "\n";
    } catch (Error $e) {
        echo $label, ": ", get_class($e), " ", $e->getMessage(), "\n";
    }
};
// Reconnecting a connected link starts over on a fresh handle: options gone.
$a = new mysqli($host, $user, $pass, $dbname, $port);
$a->options(MYSQLI_OPT_INT_AND_FLOAT_NATIVE, 1);
echo "before: ", $kind($a), "\n";
$a->query("SELECT 2");
$delta("connected");
$a->real_connect($host, $user, $pass, $dbname, $port);
echo "after: ", $kind($a), " q=", $a->get_connection_stats()["com_query"], "\n";
$delta("reconnected");
$b = mysqli_init();
$b->options(MYSQLI_OPT_INT_AND_FLOAT_NATIVE, 1);
$b->options(MYSQLI_INIT_COMMAND, "SET @elephc_init = 5");
$b->real_connect($host, $user, $pass, $dbname, $port);
echo "init: ", $kind($b), "\n";
$b->connect($host, $user, $pass, $dbname, $port);
echo "connect: ", $kind($b), "\n";
$delta("init+connect");
// A failed reconnect leaves a connected link with no connection: 2014.
@$a->real_connect($host, $user, "elephc-wrong", $dbname, $port);
$delta("failed reconnect");
echo "dead: ", var_export($a->ping(), true), " ", $a->errno, " ", var_export($a->query("SELECT 1"), true), " ", $a->errno, "\n";
$a->real_connect($host, $user, $pass, $dbname, $port);
echo "revived: ", $kind($a), "\n";
$delta("revived");
$a->close();
$delta("closed");
$attempt("closed real_connect", fn() => var_export($a->real_connect($host, $user, $pass, $dbname, $port), true));
$attempt("closed options", fn() => var_export($a->options(MYSQLI_OPT_CONNECT_TIMEOUT, 1), true));
$attempt("closed ping", fn() => var_export($a->ping(), true));
$attempt("closed errno", fn() => (string) mysqli_errno($a));
$attempt("closed close", fn() => var_export($a->close(), true));
$attempt("closed connect", fn() => var_export($a->connect($host, $user, $pass, $dbname, $port), true));
echo "reopened: ", $kind($a), "\n";
$delta("reopened");
unset($a);
$delta("destroyed");
// A failed constructor frees the handle; a failed real_connect on
// mysqli_init() keeps an unconnected one.
$c = @new mysqli($host, $user, "elephc-wrong", $dbname, $port);
$delta("failed constructor");
$attempt("failed ctor ping", fn() => var_export($c->ping(), true));
$e = mysqli_init();
@$e->real_connect($host, $user, "elephc-wrong", $dbname, $port);
$delta("failed init");
$attempt("failed init ping", fn() => var_export($e->ping(), true));
$attempt("failed init error", fn() => mysqli_error($e));
echo "init retry: ", var_export($e->real_connect($host, $user, $pass, $dbname, $port), true), "\n";
$delta("init retry");
$f = mysqli_init();
unset($f);
$delta("unused init");
echo "links: ", json_encode(mysqli_get_links_stats()), "\n";
"#,
    ));
    assert_eq!(
        out,
        r#"before: integer/NULL
connected: connect_success=1 active_connections=1
after: string/NULL q=1
reconnected: connect_success=1 implicit_close=1 active_connections=-1 com_quit=1
init: integer/5
connect: string/NULL
init+connect: connect_success=2 implicit_close=1 pconnect_success=1 active_persistent_connections=-1 com_quit=1
failed reconnect: connect_failure=1 implicit_close=1 active_connections=-2 com_quit=1
dead: false 2014 false 2014
revived: string/NULL
revived: connect_success=1
closed: explicit_close=1 active_connections=-2 com_quit=1
closed real_connect: Error mysqli object is already closed
closed options: Error mysqli object is already closed
closed ping: Error mysqli object is already closed
closed errno: Error mysqli object is already closed
closed close: Error mysqli object is already closed
closed connect: true
reopened: string/NULL
reopened: connect_success=1 active_connections=1
destroyed: explicit_close=1 active_connections=-2 com_quit=1
failed constructor: connect_failure=1 active_connections=-1
failed ctor ping: Error mysqli object is already closed
failed init: connect_failure=1
failed init ping: Error mysqli object is not fully initialized
failed init error: Error mysqli object is not fully initialized
init retry: true
init retry: connect_success=1 pconnect_success=1 active_connections=1 active_persistent_connections=1
unused init: active_connections=-1 active_persistent_connections=-1
links: {"total":2,"active_plinks":0,"cached_plinks":0}
"#
    );
}
