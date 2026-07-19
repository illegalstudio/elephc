//! Purpose:
//! Integration tests for the PDO MySQL / MariaDB driver. Each fixture compiles a
//! PHP program that drives a live MySQL/MariaDB server through `PDO`/`PDOStatement`
//! and asserts the produced stdout.
//!
//! Called from:
//! - `cargo test` through Rust's test harness. These tests are `#[ignore]`d
//!   because, unlike the SQLite fixtures, they require a running MySQL/MariaDB
//!   server. Run them opt-in with the DSN in the `ELEPHC_MY_DSN` environment
//!   variable (inherited by the compiled test binary), e.g.:
//!     docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
//!         -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
//!         -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
//!     ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
//!         cargo test --test codegen_tests -- --ignored mysql
//!
//! Key details:
//! - Each fixture opens its connection from `getenv("ELEPHC_MY_DSN")` and uses
//!   `DROP TABLE IF EXISTS` on a fixture-specific table so reruns are idempotent.
//! - The same prelude drives every driver; these tests exercise the MySQL
//!   specifics: `?`-placeholder binding (and `:name` rewritten to `?`),
//!   `AUTO_INCREMENT`/`lastInsertId`, the `mysql` driver name, and decoding of
//!   integer/double/boolean/text/NULL plus the rich `DECIMAL`/`DATE`/`DATETIME`/
//!   `TIME` types.

use crate::support::*;

/// Wraps a PHP body that opens `$db` from `ELEPHC_MY_DSN`, so each fixture only
/// writes the database logic under test.
fn my_program(body: &str) -> String {
    // getenv() is typed string|false; the env var is always set when these
    // ignored tests run, so a string cast is safe and keeps the DSN a string.
    format!(
        "<?php\n$db = new PDO((string) getenv(\"ELEPHC_MY_DSN\"));\n{}\n",
        body
    )
}

/// Generic connection-information attributes expose the linked client, live
/// server statistics, and the actual MySQL transport description.
#[test]
#[ignore]
fn test_mysql_connection_information_attributes() {
    let out = compile_and_run(&my_program(
        r#"
$client = (string) $db->getAttribute(PDO::ATTR_CLIENT_VERSION);
$server = (string) $db->getAttribute(PDO::ATTR_SERVER_VERSION);
$info = (string) $db->getAttribute(PDO::ATTR_SERVER_INFO);
$status = (string) $db->getAttribute(PDO::ATTR_CONNECTION_STATUS);
echo (strpos($client, "mysql ") === 0 ? "client" : "bad-client") . "|";
echo (strlen($server) > 0 ? "server" : "bad-server") . "|";
echo (strpos($info, "Uptime: ") === 0 && strpos($info, "Questions: ") !== false ? "info" : "bad-info") . "|";
echo (strpos($status, " via TCP/IP") !== false || $status === "Localhost via UNIX socket" ? "status" : "bad-status");
"#,
    ));
    assert_eq!(out, "client|server|info|status");
}

/// MySQL's live `ATTR_FETCH_TABLE_NAMES` setting prefixes fetched keys with the
/// protocol table label and affects statements prepared after either constructor
/// or runtime configuration.
#[test]
#[ignore]
fn test_mysql_fetch_table_names_attribute() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_names");
$db->exec("CREATE TABLE my_names (id INT)");
$db->exec("INSERT INTO my_names VALUES (7)");
echo ($db->getAttribute(PDO::ATTR_FETCH_TABLE_NAMES) ? "on" : "off") . "|";
$db->setAttribute(PDO::ATTR_FETCH_TABLE_NAMES, true);
$row = $db->query("SELECT id FROM my_names")->fetch(PDO::FETCH_ASSOC);
echo $row["my_names.id"] . "|" . ($db->getAttribute(PDO::ATTR_FETCH_TABLE_NAMES) ? "on" : "off");
$db->setAttribute(PDO::ATTR_FETCH_TABLE_NAMES, false);
$plain = $db->query("SELECT id FROM my_names")->fetch(PDO::FETCH_ASSOC);
echo "|" . $plain["id"];
$db->exec("DROP TABLE my_names");
"#,
    ));
    assert_eq!(out, "off|7|on|7");
}

/// MySQL unbuffered mode reports zero SELECT rowCount, blocks a second query
/// until the active cursor is closed, and can be toggled live through the PDO
/// attribute. MULTI_STATEMENTS=false rejects real second statements while
/// allowing a semicolon embedded in a string literal.
#[test]
#[ignore]
fn test_mysql_buffered_and_multi_statement_options() {
    let out = compile_and_run(
        r#"<?php
$dsn = (string) getenv("ELEPHC_MY_DSN");
$db = new \Pdo\Mysql($dsn, null, null, [
    \Pdo\Mysql::ATTR_USE_BUFFERED_QUERY => false,
    \Pdo\Mysql::ATTR_MULTI_STATEMENTS => false,
    \Pdo\Mysql::ATTR_IGNORE_SPACE => true,
]);
$stmt = $db->query("SELECT 1 AS n UNION ALL SELECT 2");
$first = $stmt->fetchColumn();
try {
    $db->query("SELECT 3");
    $busy = "bad";
} catch (\PDOException $error) {
    $busy = ($error->errorInfo[1] === 2014) ? "busy" : "wrong";
}
$stmt->closeCursor();
$after = $db->query("SELECT ';' AS value;")->fetchColumn();
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$multi = $db->query("SELECT 1; SELECT 2") === false ? "blocked" : "bad";
echo ($db->getAttribute(\Pdo\Mysql::ATTR_USE_BUFFERED_QUERY) ? "buffered" : "unbuffered")
    . ":" . $first . ":" . $busy . ":" . $after . ":" . $multi;
"#,
    );
    assert_eq!(out, "unbuffered:1:busy:;:blocked");
}

/// Unbuffered MySQL execution returns after the first wire row instead of waiting
/// for a deliberately delayed second row, proving rows are not materialized first.
#[test]
#[ignore]
fn test_mysql_unbuffered_fetch_is_demand_driven() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Mysql((string) getenv("ELEPHC_MY_DSN"), null, null, [
    \Pdo\Mysql::ATTR_USE_BUFFERED_QUERY => false,
    \PDO::ATTR_EMULATE_PREPARES => false,
]);
$db->exec("DROP TABLE IF EXISTS elephc_stream_probe");
$db->exec("CREATE TABLE elephc_stream_probe (id INT PRIMARY KEY)");
$db->exec("INSERT INTO elephc_stream_probe VALUES (1), (2)");
$stmt = $db->prepare("SELECT IF(id = 1, REPEAT('x', 1048576), CONCAT(SLEEP(3), 'done')) AS payload FROM elephc_stream_probe");
$start = microtime(true);
$stmt->execute();
$executeElapsed = microtime(true) - $start;
$first = $stmt->fetchColumn();
$second = $stmt->fetchColumn();
$totalElapsed = microtime(true) - $start;
$stmt->closeCursor();
$db->exec("DROP TABLE elephc_stream_probe");
echo ($executeElapsed < 2.0 ? "early" : "late") . ":" . strlen($first) . ":" . $second . ":" . ($totalElapsed >= 2.5 ? "waited" : "too-fast");
"#,
    );
    assert_eq!(out, "early:1048576:0done:waited");
}

/// Unbuffered stored-procedure result sets remain separated and `nextRowset()`
/// discards unread rows before advancing, including MySQL's trailing OK set.
#[test]
#[ignore]
fn test_mysql_unbuffered_next_rowset_is_demand_driven() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Mysql((string) getenv("ELEPHC_MY_DSN"), null, null, [
    \Pdo\Mysql::ATTR_USE_BUFFERED_QUERY => false,
]);
$db->exec("DROP PROCEDURE IF EXISTS elephc_stream_sets");
$db->exec("CREATE PROCEDURE elephc_stream_sets() BEGIN SELECT 1 AS n UNION ALL SELECT 99; SELECT 2 AS n; END");
$stmt = $db->query("CALL elephc_stream_sets()");
$first = $stmt->fetchColumn();
$stmt->nextRowset();
$second = $stmt->fetchColumn();
while ($stmt->nextRowset()) {}
$ready = $db->query("SELECT 3")->fetchColumn();
$stmt->closeCursor();
$db->exec("DROP PROCEDURE elephc_stream_sets");
echo $first . ":" . $second . ":" . $ready;
"#,
    );
    assert_eq!(out, "1:2:3");
}

/// With LOCAL_INFILE explicitly enabled, the native client uploads the exact
/// requested file and enforces ATTR_LOCAL_INFILE_DIRECTORY's canonical path
/// boundary. Requires a server configured with `local_infile=ON`.
#[test]
#[ignore]
fn test_mysql_local_infile_directory_upload() {
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . "/elephc-pdo-local-infile.tsv";
file_put_contents($path, "1\tAda\n2\tBob\n");
$db = new \Pdo\Mysql((string) getenv("ELEPHC_MY_DSN"), null, null, [
    \Pdo\Mysql::ATTR_LOCAL_INFILE => true,
    \Pdo\Mysql::ATTR_LOCAL_INFILE_DIRECTORY => sys_get_temp_dir(),
]);
$db->exec("DROP TABLE IF EXISTS elephc_local_infile");
$db->exec("CREATE TABLE elephc_local_infile (id INT, name VARCHAR(20))");
$count = $db->exec("LOAD DATA LOCAL INFILE " . $db->quote($path) . " INTO TABLE elephc_local_infile");
$names = $db->query("SELECT GROUP_CONCAT(name ORDER BY id) FROM elephc_local_infile")->fetchColumn();
$db->exec("DROP TABLE elephc_local_infile");
unlink($path);
echo $count . ":" . $names;
"#,
    );
    assert_eq!(out, "2:Ada,Bob");
}

/// Round-trip: create, insert through named placeholders (rewritten to `?`), and
/// read a row back keyed by column name.
#[test]
#[ignore]
fn test_mysql_named_bind_round_trip() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_rt");
$db->exec("CREATE TABLE my_rt (id INTEGER PRIMARY KEY AUTO_INCREMENT, name TEXT, score DOUBLE)");
$ins = $db->prepare("INSERT INTO my_rt (name, score) VALUES (:name, :score)");
$ins->execute([":name" => "Ada", ":score" => 9.5]);
$row = $db->query("SELECT id, name, score FROM my_rt")->fetch(PDO::FETCH_ASSOC);
echo $row["id"] . ":" . $row["name"] . ":" . $row["score"];
$db->exec("DROP TABLE my_rt");
"#,
    ));
    assert_eq!(out, "1:Ada:9.5");
}

/// Positional `?` placeholders bind by position.
#[test]
#[ignore]
fn test_mysql_positional_bind() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_pos");
$db->exec("CREATE TABLE my_pos (a INTEGER, b TEXT)");
$ins = $db->prepare("INSERT INTO my_pos (a, b) VALUES (?, ?)");
$ins->execute([7, "seven"]);
$sel = $db->prepare("SELECT b FROM my_pos WHERE a = ?");
$sel->execute([7]);
echo $sel->fetchColumn();
$db->exec("DROP TABLE my_pos");
"#,
    ));
    assert_eq!(out, "seven");
}

/// MySQL defaults to client-side emulated prepares, quotes bound text without losing
/// apostrophes, and switches subsequent statements to native protocol when disabled.
#[test]
#[ignore]
fn test_mysql_emulated_prepare_default_and_native_opt_out() {
    let out = compile_and_run(&my_program(
        r#"
$default = $db->getAttribute(PDO::ATTR_EMULATE_PREPARES);
$emulated = $db->prepare("SELECT ? AS value");
$emulated->execute(["O'Reilly"]);
$quoted = $emulated->fetchColumn();
$disabled = $db->setAttribute(PDO::ATTR_EMULATE_PREPARES, false);
$native = $db->prepare("SELECT ? AS value");
$native->execute([17]);
echo ($default ? "emulated" : "native") . "|" . $quoted . "|"
    . ($disabled ? "disabled" : "failed") . "|"
    . ($native->getAttribute(PDO::ATTR_EMULATE_PREPARES) ? "emulated" : "native") . "|"
    . $native->fetchColumn();
"#,
    ));
    assert_eq!(out, "emulated|O'Reilly|disabled|native|17");
}

/// Emulated execution rejects a missing bind client-side with HY093 instead of
/// silently substituting SQL NULL for a parameter the caller never supplied.
#[test]
#[ignore]
fn test_mysql_emulated_prepare_rejects_missing_bind() {
    let out = compile_and_run(&my_program(
        r#"
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$stmt = $db->prepare("SELECT ? + ?");
$ok = $stmt->execute([1]);
echo (($ok === false) ? "false" : "true") . "|" . $stmt->errorCode();
"#,
    ));
    assert_eq!(out, "false|HY093");
}

/// `debugDumpParams()` exposes the exact SQL rendered by the emulated text-protocol
/// path, including client-side string quoting.
#[test]
#[ignore]
fn test_mysql_emulated_prepare_debug_dump_prints_sent_sql() {
    let out = compile_and_run(&my_program(
        r#"
$stmt = $db->prepare("SELECT ? AS value");
$stmt->execute(["O'Reilly"]);
$stmt->debugDumpParams();
"#,
    ));
    assert_eq!(
        out,
        "SQL: [17] SELECT ? AS value\nSent SQL: [27] SELECT 'O\\'Reilly' AS value\nParams:  1\nKey: Position #0:\nparamno=0\nname=[0] \"\"\nis_param=1\nparam_type=2\n"
    );
}

/// `AUTO_INCREMENT` columns drive `lastInsertId()`.
#[test]
#[ignore]
fn test_mysql_last_insert_id() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_seq");
$db->exec("CREATE TABLE my_seq (id INTEGER PRIMARY KEY AUTO_INCREMENT, n INTEGER)");
$db->exec("INSERT INTO my_seq (n) VALUES (10)");
$db->exec("INSERT INTO my_seq (n) VALUES (20)");
echo $db->lastInsertId();
$db->exec("DROP TABLE my_seq");
"#,
    ));
    assert_eq!(out, "2");
}

/// Column types decode to PHP scalars: integer, double, boolean (0/1), text, and
/// SQL NULL.
#[test]
#[ignore]
fn test_mysql_type_decoding() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_types");
$db->exec("CREATE TABLE my_types (i INTEGER, d DOUBLE, flag BOOLEAN, t TEXT, n TEXT)");
$db->exec("INSERT INTO my_types VALUES (42, 3.5, true, 'hi', NULL)");
$row = $db->query("SELECT i, d, flag, t, n FROM my_types")->fetch(PDO::FETCH_ASSOC);
echo $row["i"] . "|" . $row["d"] . "|" . $row["flag"] . "|" . $row["t"] . "|" . (is_null($row["n"]) ? "NULL" : "x");
$db->exec("DROP TABLE my_types");
"#,
    ));
    assert_eq!(out, "42|3.5|1|hi|NULL");
}

/// A `PDOStatement` is Traversable: `foreach` walks the result set in the current
/// fetch mode.
#[test]
#[ignore]
fn test_mysql_foreach() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_iter");
$db->exec("CREATE TABLE my_iter (id INTEGER, name TEXT)");
$db->exec("INSERT INTO my_iter VALUES (1, 'a'), (2, 'b'), (3, 'c')");
$stmt = $db->query("SELECT id, name FROM my_iter ORDER BY id");
$stmt->setFetchMode(PDO::FETCH_ASSOC);
foreach ($stmt as $k => $row) {
    echo $k . ":" . $row["id"] . "=" . $row["name"] . ";";
}
$db->exec("DROP TABLE my_iter");
"#,
    ));
    assert_eq!(out, "0:1=a;1:2=b;2:3=c;");
}

/// Committed work persists; a rolled-back transaction does not (InnoDB). DDL runs
/// outside the transaction because MySQL implicitly commits around it.
#[test]
#[ignore]
fn test_mysql_transactions() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_tx");
$db->exec("CREATE TABLE my_tx (n INTEGER) ENGINE=InnoDB");
$db->beginTransaction();
$db->exec("INSERT INTO my_tx VALUES (1)");
$db->rollBack();
$db->beginTransaction();
$db->exec("INSERT INTO my_tx VALUES (2)");
$db->commit();
echo $db->query("SELECT COUNT(*) FROM my_tx")->fetchColumn() . ":" . $db->query("SELECT n FROM my_tx")->fetchColumn();
$db->exec("DROP TABLE my_tx");
"#,
    ));
    assert_eq!(out, "1:2");
}

/// A raw MySQL `START TRANSACTION` bypasses PDO::beginTransaction() but remains
/// visible through PDO::inTransaction() and accepted by PDO::rollBack().
#[test]
#[ignore]
fn test_mysql_raw_transaction_is_visible() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("START TRANSACTION");
echo ($db->inTransaction() ? "in" : "out") . ":";
$db->rollBack();
echo ($db->inTransaction() ? "in" : "out");
"#,
    ));
    assert_eq!(out, "in:out");
}

/// Rich MySQL types decode to their text representation: `DECIMAL` keeps its
/// scale, `DATE` drops the time, `DATETIME` keeps it, and `TIME` renders as
/// `HH:MM:SS`. The values bind through text parameters (coerced by the server to
/// the column type).
#[test]
#[ignore]
fn test_mysql_rich_types() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_rich");
$db->exec("CREATE TABLE my_rich (money DECIMAL(10,2), d DATE, ts DATETIME, t TIME)");
$ins = $db->prepare("INSERT INTO my_rich VALUES (:m, :d, :ts, :t)");
$ins->execute([
    ":m"  => "1234.50",
    ":d"  => "2024-01-15",
    ":ts" => "2024-01-15 10:30:00",
    ":t"  => "10:30:00",
]);
$r = $db->query("SELECT money, d, ts, t FROM my_rich")->fetch(PDO::FETCH_ASSOC);
echo $r["money"] . "|" . $r["d"] . "|" . $r["ts"] . "|" . $r["t"];
$db->exec("DROP TABLE my_rich");
"#,
    ));
    assert_eq!(out, "1234.50|2024-01-15|2024-01-15 10:30:00|10:30:00");
}

/// `getAttribute(PDO::ATTR_DRIVER_NAME)` reports the active driver.
#[test]
#[ignore]
fn test_mysql_driver_name() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->getAttribute(PDO::ATTR_DRIVER_NAME);
"#,
    ));
    assert_eq!(out, "mysql");
}

/// The default exception error mode throws a catchable `PDOException` on bad SQL,
/// and `ERRMODE_SILENT` makes `exec()` return `false` instead.
#[test]
#[ignore]
fn test_mysql_error_modes() {
    let out = compile_and_run(&my_program(
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

/// P2-2: a `BIGINT UNSIGNED` value above `i64::MAX` round-trips as an exact
/// decimal numeric string rather than wrapping negative through a lossy `as
/// i64` cast (`my.rs::decode_value`'s `Value::UInt` branch). Driven against the
/// live server.
#[test]
#[ignore]
fn test_mysql_bigint_unsigned_above_i64_max_round_trips() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_bigint_unsigned");
$db->exec("CREATE TABLE my_bigint_unsigned (n BIGINT UNSIGNED)");
$db->exec("INSERT INTO my_bigint_unsigned VALUES (18446744073709551615)");
echo $db->query("SELECT n FROM my_bigint_unsigned")->fetchColumn();
$db->exec("DROP TABLE my_bigint_unsigned");
"#,
    ));
    assert_eq!(out, "18446744073709551615");
}

/// P1-9 (minimal wiring): `Pdo\Mysql::ATTR_INIT_COMMAND` runs its SQL statement
/// right after authentication, so a session variable it sets is already visible
/// to the very first query issued on the connection. Driven against the live
/// server as the driver subclass directly (the constructor option).
#[test]
#[ignore]
fn test_mysql_attr_init_command_runs_on_connect() {
    let out = compile_and_run(
        r#"<?php
$db = new \Pdo\Mysql((string) getenv("ELEPHC_MY_DSN"), null, null, [
    \Pdo\Mysql::ATTR_INIT_COMMAND => "SET @elephc_init_probe = 42",
]);
echo $db->query("SELECT @elephc_init_probe")->fetchColumn();
"#,
    );
    assert_eq!(out, "42");
}

/// P2-3: a `charset=utf8mb4` DSN key becomes a `SET NAMES utf8mb4` statement at
/// connect time, so `SHOW VARIABLES LIKE 'character_set_connection'` reports it
/// without the caller issuing any SQL itself. Driven against the live server.
#[test]
#[ignore]
fn test_mysql_charset_dsn_key_sets_connection_charset() {
    let out = compile_and_run(
        r#"<?php
$dsn = ((string) getenv("ELEPHC_MY_DSN")) . ";charset=utf8mb4";
$db = new \Pdo\Mysql($dsn);
$row = $db->query("SHOW VARIABLES LIKE 'character_set_connection'")->fetch(PDO::FETCH_NUM);
echo $row[1];
"#,
    );
    assert_eq!(out, "utf8mb4");
}

/// P2-1: `PDO::ATTR_TIMEOUT` folds into the DSN as the `connect_timeout` key
/// (mapped to `OptsBuilder::tcp_connect_timeout` in `my.rs::build_opts`), so a
/// connection attempt against an unreachable host fails within a bounded time
/// instead of hanging on the OS's own (much longer) TCP connect timeout. Uses a
/// non-routable TEST-NET-1 address (RFC 5737, `192.0.2.0/24`) so the connect
/// attempt reliably blackholes rather than getting an immediate "connection
/// refused". Driven without any live server (the point is that the connection
/// never completes).
#[test]
#[ignore]
fn test_mysql_attr_timeout_fails_fast() {
    let out = compile_and_run(
        r#"<?php
class MysqlTimeoutClock { public static float $start = 0.0; }
MysqlTimeoutClock::$start = microtime(true);
try {
    $conn = new \Pdo\Mysql("mysql:host=192.0.2.1;port=3306;dbname=testdb", null, null, [PDO::ATTR_TIMEOUT => 2]);
    echo "connected";
} catch (PDOException $e) {
    $elapsed = microtime(true) - MysqlTimeoutClock::$start;
    echo ($elapsed < 10.0) ? "fast" : "slow";
}
"#,
    );
    assert_eq!(out, "fast");
}

/// `Pdo\Mysql::getWarningCount()` reports the warning count of the last statement,
/// cached from that statement's terminal OK packet. `CREATE TABLE IF NOT EXISTS`
/// on an existing table raises one "table already exists" warning (an OK-terminated
/// DDL statement). A lossy SELECT cast then pins EOF/OK warning capture on a prepared
/// row-producing statement too. Driven against the live server as the driver subclass.
#[test]
#[ignore]
fn test_mysql_get_warning_count() {
    let out = compile_and_run(
        "<?php\n$db = new \\Pdo\\Mysql((string) getenv(\"ELEPHC_MY_DSN\"));\n$db->exec(\"DROP TABLE IF EXISTS elephc_warn_probe\");\n$db->exec(\"CREATE TABLE elephc_warn_probe (id INT)\");\n$db->exec(\"CREATE TABLE IF NOT EXISTS elephc_warn_probe (id INT)\");\n$ddl = $db->getWarningCount();\n$stmt = $db->query(\"SELECT CAST('not-a-number' AS UNSIGNED)\");\n$stmt->fetchColumn();\n$select = $db->getWarningCount();\n$db->exec(\"DROP TABLE elephc_warn_probe\");\necho $ddl . ':' . (($select > 0) ? 'warn' : 'none');\n",
    );
    assert_eq!(out, "1:warn");
}

/// Live TLS round-trip. Opens a MySQL/MariaDB connection with `Pdo\Mysql::ATTR_SSL_CA`
/// set to the server CA bundle (path in `ELEPHC_MY_TLS_CA`) and confirms a query
/// returns over the encrypted connection. mysql 28's ring-backed TLS ships in the
/// default bridge build; a custom build without `mysql-tls` fails loudly.
/// `#[ignore]` — needs a TLS-serving MySQL. Example:
///   docker run -d --name mytls -e MYSQL_ROOT_PASSWORD=test -e MYSQL_DATABASE=testdb \
///       -e MYSQL_USER=test -e MYSQL_PASSWORD=test -p 33062:3306 mysql:8 \
///       --require-secure-transport=ON
///   docker cp mytls:/var/lib/mysql/ca.pem ./ca.pem   # server-generated CA
///   cargo build -p elephc-pdo                         # TLS staticlib (ring)
///   ELEPHC_MY_TLS_DSN='mysql:host=127.0.0.1;port=33062;dbname=testdb;user=test;password=test' \
///       ELEPHC_MY_TLS_CA="$PWD/ca.pem" \
///       cargo test --test codegen_tests -- --ignored mysql_tls_round_trip
#[test]
#[ignore]
fn mysql_tls_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO(
    (string) getenv("ELEPHC_MY_TLS_DSN"),
    null,
    null,
    [Pdo\Mysql::ATTR_SSL_CA => (string) getenv("ELEPHC_MY_TLS_CA")]
);
echo $db->query("SELECT 'tls-ok'")->fetchColumn();
"#,
    );
    assert_eq!(out, "tls-ok");
}

/// `ATTR_SSL_CAPATH` trusts the PEM certificates in a directory after the bridge
/// adapts them into rustls's multi-certificate bundle representation.
#[test]
#[ignore]
fn mysql_tls_capath_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO(
    (string) getenv("ELEPHC_MY_TLS_DSN"),
    null,
    null,
    [Pdo\Mysql::ATTR_SSL_CAPATH => (string) getenv("ELEPHC_MY_TLS_CAPATH")]
);
echo $db->query("SELECT 'capath-ok'")->fetchColumn();
"#,
    );
    assert_eq!(out, "capath-ok");
}

/// A caller-supplied caching_sha2_password RSA key is used for a non-TLS login,
/// matching mysqlnd's MYSQL_SERVER_PUBLIC_KEY connection option.
#[test]
#[ignore]
fn mysql_server_public_key_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO(
    (string) getenv("ELEPHC_MY_DSN"),
    null,
    null,
    [Pdo\Mysql::ATTR_SERVER_PUBLIC_KEY => (string) getenv("ELEPHC_MY_SERVER_PUBLIC_KEY")]
);
echo $db->query("SELECT 'rsa-ok'")->fetchColumn();
"#,
    );
    assert_eq!(out, "rsa-ok");
}

/// ATTR_SSL_CIPHER constrains rustls to a modern TLS 1.2 suite understood under
/// both OpenSSL's MySQL spelling and rustls's IANA spelling.
#[test]
#[ignore]
fn mysql_tls_cipher_round_trip() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO(
    (string) getenv("ELEPHC_MY_TLS_DSN"),
    null,
    null,
    [
        Pdo\Mysql::ATTR_SSL_CA => (string) getenv("ELEPHC_MY_TLS_CA"),
        Pdo\Mysql::ATTR_SSL_CIPHER => "ECDHE-RSA-AES128-GCM-SHA256",
    ]
);
$row = $db->query("SHOW STATUS LIKE 'Ssl_cipher'")->fetch(PDO::FETCH_NUM);
echo str_contains((string) $row[1], "AES128-GCM-SHA256") ? "cipher-ok" : (string) $row[1];
"#,
    );
    assert_eq!(out, "cipher-ok");
}

/// P0-B: `PDO::exec()` must return the real affected-row count for INSERT,
/// UPDATE, and DELETE, not always `0`. Regression for `my.rs::MyConn::exec()`
/// reading `affected_rows()` after draining the query result, at which point
/// the crate's `QueryResult` state machine has already advanced past the OK
/// packet that carries the count. Asserts the return values directly (not via
/// a follow-up `SELECT COUNT(*)`, which would pass even with the bug). Docker:
///   docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
///       -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
///       -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_exec_returns_affected_row_count
#[test]
#[ignore]
fn mysql_exec_returns_affected_row_count() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_exec_counts");
$db->exec("CREATE TABLE my_exec_counts (id INTEGER PRIMARY KEY, n INTEGER)");
$inserted = $db->exec("INSERT INTO my_exec_counts (id, n) VALUES (1, 1), (2, 1), (3, 2)");
$updated = $db->exec("UPDATE my_exec_counts SET n = 9 WHERE n = 1");
$deleted = $db->exec("DELETE FROM my_exec_counts WHERE n = 9");
echo $inserted . ":" . $updated . ":" . $deleted;
$db->exec("DROP TABLE my_exec_counts");
"#,
    ));
    assert_eq!(out, "3:2:2");
}

/// P0-D: a `BIT(8)` column holding a high-bit value must round-trip its raw
/// byte unchanged. Regression for `my.rs::ColKind::from_column_type()` routing
/// `MYSQL_TYPE_BIT` through the lossy `String::from_utf8_lossy` path (the
/// `Other` bucket) instead of the byte-preserving `Cell::Bytes` path (the
/// `Binary` bucket): `0xFF` is not valid UTF-8, so the lossy path replaces it
/// with a 3-byte U+FFFD ("\u{FFFD}") in the decoded string. Asserted via
/// `bin2hex()`, not a printable-ASCII value, since a printable value would
/// happen to survive the lossy path and mask the bug. Docker: same as
/// `mysql_exec_returns_affected_row_count` above.
#[test]
#[ignore]
fn mysql_bit_column_round_trip() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_bit_col");
$db->exec("CREATE TABLE my_bit_col (b BIT(8))");
$db->exec("INSERT INTO my_bit_col VALUES (b'11111111')");
$val = $db->query("SELECT b FROM my_bit_col")->fetchColumn();
echo bin2hex($val);
$db->exec("DROP TABLE my_bit_col");
"#,
    ));
    assert_eq!(out, "ff");
}

/// P1: a `VARBINARY` column holding a high-bit, non-UTF-8 byte sequence must
/// round-trip its raw bytes unchanged. Regression for `my.rs::ColKind::
/// from_column` classifying `VARBINARY`/`BINARY` correctly: both arrive on the
/// wire as the exact same `ColumnType` as `VARCHAR`/`CHAR`
/// (`MYSQL_TYPE_VAR_STRING`), and only the column's character set (63, the
/// `binary` collation) tells them apart. Before the fix, a `VARBINARY` column
/// fell to `ColKind::Other` and was decoded through the lossy
/// `String::from_utf8_lossy` path, turning `0xC3FF00` (not valid UTF-8) into a
/// U+FFFD-corrupted value. Asserted via `bin2hex()`, not a printable value,
/// since a printable value would happen to survive the lossy path and mask the
/// bug. Docker: same as `mysql_exec_returns_affected_row_count` above.
#[test]
#[ignore]
fn mysql_varbinary_round_trip() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_varbinary_col");
$db->exec("CREATE TABLE my_varbinary_col (b VARBINARY(16))");
$db->exec("INSERT INTO my_varbinary_col VALUES (x'C3FF00')");
$val = $db->query("SELECT b FROM my_varbinary_col")->fetchColumn();
echo bin2hex($val);
$db->exec("DROP TABLE my_varbinary_col");
"#,
    ));
    assert_eq!(out, "c3ff00");
}

/// P0-C: `CALL`ing a stored procedure through a prepared statement must return
/// its rows. Regression for `my.rs::MyStmt::execute()` gating row
/// materialization on the PREPARE-time column count (`self.col_kinds`):
/// `COM_STMT_PREPARE` reports zero columns for `CALL proc()` (the result shape
/// is only known once the procedure actually runs), so the old code silently
/// dropped the procedure's rows off the wire. Docker: same server as above,
/// e.g.:
///   docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
///       -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
///       -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_call_stored_procedure_returns_rows
#[test]
#[ignore]
fn mysql_call_stored_procedure_returns_rows() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_call_src");
$db->exec("CREATE TABLE my_call_src (id INTEGER, name TEXT)");
$db->exec("INSERT INTO my_call_src VALUES (1, 'a'), (2, 'b')");
$db->exec("DROP PROCEDURE IF EXISTS my_call_sp");
$db->exec("CREATE PROCEDURE my_call_sp() BEGIN SELECT id, name FROM my_call_src ORDER BY id; END");
$stmt = $db->prepare("CALL my_call_sp()");
$stmt->execute();
$rows = $stmt->fetchAll(PDO::FETCH_ASSOC);
echo count($rows) . ":" . $rows[0]["id"] . "=" . $rows[0]["name"] . ";" . $rows[1]["id"] . "=" . $rows[1]["name"];
$db->exec("DROP PROCEDURE my_call_sp");
$db->exec("DROP TABLE my_call_src");
"#,
    ));
    assert_eq!(out, "2:1=a;2=b");
}

/// P1-f (SECURITY): under the `NO_BACKSLASH_ESCAPES` `sql_mode`, backslash is a
/// literal character inside a MySQL string literal, so `PDO::quote()`'s usual
/// backslash-escaping is unsafe there — an escaped quote (`\'`) does not
/// actually escape and lets a crafted string break out of the literal. mysqlnd
/// itself switches to quote-doubling-only in that mode; `elephc_pdo_no_backslash_escapes`
/// (bridge v21) mirrors that via a live `sql_mode` read, so `quote()` must
/// return the doubled-quote form (`'O''Brien'`), not the backslash form. Docker:
/// same server as the other MySQL fixtures in this file, e.g.:
///   docker run -d --name my -e MARIADB_ROOT_PASSWORD=rootpw \
///       -e MARIADB_DATABASE=testdb -e MARIADB_USER=test \
///       -e MARIADB_PASSWORD=test -p 33060:3306 mariadb:11
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_quote_no_backslash_escapes_mode
#[test]
#[ignore]
fn mysql_quote_no_backslash_escapes_mode() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'");
echo $db->quote("O'Brien");
"#,
    ));
    assert_eq!(out, "'O''Brien'");
}

/// Sibling of `mysql_quote_no_backslash_escapes_mode`: the connection's default
/// session mode (no `NO_BACKSLASH_ESCAPES`) keeps `PDO::quote()`'s
/// backslash-escaped form, proving the branch genuinely depends on the live
/// `elephc_pdo_no_backslash_escapes` read rather than one path always winning.
/// Docker: same server as above (see `mysql_quote_no_backslash_escapes_mode`).
#[test]
#[ignore]
fn mysql_quote_normal_mode_backslash_escapes() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->quote("O'Brien");
"#,
    ));
    assert_eq!(out, "'O\\'Brien'");
}

/// P1-e: `PDO::quote($string, PDO::PARAM_LOB)` on the mysql driver prefixes the
/// escaped literal with the `_binary` charset introducer, mirroring php-src's
/// `mysql_handle_quoter`, so a binary/LOB value is not reinterpreted under the
/// connection's charset. Docker: same server as above (see
/// `mysql_quote_no_backslash_escapes_mode`).
#[test]
#[ignore]
fn mysql_quote_param_lob_binary_prefix() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->quote("ab", PDO::PARAM_LOB);
"#,
    ));
    assert_eq!(out, "_binary'ab'");
}

/// F-MY-05 (P0-C follow-up): a `CALL` behind a LEADING COMMENT is still a `CALL`.
/// This is the single highest-value live fixture of the wave: it pins the
/// data-loss half of the P0-C regression rather than its detection half.
/// `my.rs::sql_is_call_statement()` used to test only past leading WHITESPACE, so
/// `/* hint */ CALL p()` (an optimizer-hint prefix, which real applications and
/// ORMs emit routinely) and `-- note\nCALL p()` were classified as ordinary
/// statements. A non-`CALL` statement's row materialization is gated on the
/// PREPARE-time column count, and `COM_STMT_PREPARE` reports ZERO columns for a
/// `CALL` (the result shape only exists once the procedure runs) — so the
/// procedure's rows were silently dropped off the wire and `fetchAll()` returned
/// an empty set with NO error. The fix skips `/* … */`, `-- …` and `# …` runs
/// ahead of the keyword. `$db->query()` routes through `prepare()` + `execute()`
/// in the prelude, so it exercises exactly that path.
///
/// Both comment spellings are asserted (block and line), and the row is read
/// through a `count() > 0` guard so a REGRESSION reports a readable `0-:0-`
/// rather than crashing on an out-of-range index. Docker: same server as
/// `mysql_call_stored_procedure_returns_rows` above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_call_behind_leading_comment_returns_rows
#[test]
#[ignore]
fn mysql_call_behind_leading_comment_returns_rows() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_call_cmt_src");
$db->exec("CREATE TABLE my_call_cmt_src (id INTEGER, name TEXT)");
$db->exec("INSERT INTO my_call_cmt_src VALUES (1, 'a')");
$db->exec("DROP PROCEDURE IF EXISTS my_call_cmt_sp");
$db->exec("CREATE PROCEDURE my_call_cmt_sp() BEGIN SELECT id, name FROM my_call_cmt_src; END");
$blockRows = $db->query("/* hint */ CALL my_call_cmt_sp()")->fetchAll(PDO::FETCH_ASSOC);
$lineRows = $db->query("-- note\nCALL my_call_cmt_sp()")->fetchAll(PDO::FETCH_ASSOC);
$block = (count($blockRows) > 0) ? $blockRows[0]["name"] : "-";
$line = (count($lineRows) > 0) ? $lineRows[0]["name"] : "-";
echo count($blockRows) . $block . ":" . count($lineRows) . $line;
$db->exec("DROP PROCEDURE my_call_cmt_sp");
$db->exec("DROP TABLE my_call_cmt_src");
"#,
    ));
    assert_eq!(out, "1a:1a");
}

/// F-CORE-02: on the MYSQL driver the CONSTRUCTOR's `$username`/`$password` WIN
/// over a `user=`/`password=` the DSN already carries. php-src's handle factory
/// consults the DSN key only as a fallback for an ABSENT constructor argument
/// (`if (!dbh->username && vars[5].optval) …`, `mysql_driver.c:948-953`), the
/// opposite of pgsql's last-wins conninfo (`pgsql_driver.c:1377-1378`) — and this
/// prelude used to apply the pgsql rule to both, so
/// `new PDO("mysql:host=h;user=readonly", "admin", $pw)` connected as `readonly`:
/// a SILENT PRIVILEGE SWAP.
///
/// Runnable against the standard test container without knowing its credentials:
/// the real ones are lifted out of `ELEPHC_MY_DSN` itself, a bogus pair is
/// APPENDED to that DSN (the bridge's `build_opts` parser is last-wins, so the
/// appended pair overrides the env DSN's own), and they are then handed back as
/// the constructor arguments. The first `new PDO($bogus)` — no constructor
/// arguments at all — is the NEGATIVE CONTROL: it must be REJECTED by the server,
/// proving the bogus credentials really are bogus and that the second connect
/// succeeds because the constructor arguments displaced them, not because the
/// server would have let anyone in. Docker: same server as above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_ctor_credentials_override_dsn_credentials
#[test]
#[ignore]
fn mysql_ctor_credentials_override_dsn_credentials() {
    let out = compile_and_run(
        r#"<?php
$dsn = (string) getenv("ELEPHC_MY_DSN");
$user = "";
$pass = "";
$parts = explode(";", $dsn);
foreach ($parts as $part) {
    if (str_starts_with($part, "user=")) {
        $user = substr($part, 5);
    } elseif (str_starts_with($part, "password=")) {
        $pass = substr($part, 9);
    }
}
if ($user === "") {
    echo "dsn-carries-no-user";
} else {
    $bogus = $dsn . ";user=elephc_no_such_user;password=elephc_wrong_password";
    $control = "connected";
    try {
        $rejectMe = new PDO($bogus);
    } catch (PDOException $e) {
        $control = "rejected";
    }
    $db = new PDO($bogus, $user, $pass);
    echo $control . ":" . $db->query("SELECT 1")->fetchColumn();
}
"#,
    );
    assert_eq!(out, "rejected:1");
}

/// Contrast half of `mysql_ctor_credentials_override_dsn_credentials`, pinning the
/// behavior that was already correct and that the F-CORE-02 fix must not break: a
/// DSN-ONLY credential set still connects. The constructor arguments are passed
/// EXPLICITLY as `null` here (rather than omitted) because that is the exact
/// condition the fix's append is gated on — `$username !== null` — so a future
/// change that appended an empty `;user=` for a null argument would clobber the
/// DSN's own `user=` (the parser is last-wins) and fail here, not silently in
/// production. Docker: same server as above.
#[test]
#[ignore]
fn mysql_dsn_only_credentials_still_connect() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO((string) getenv("ELEPHC_MY_DSN"), null, null);
echo $db->query("SELECT 1")->fetchColumn();
"#,
    );
    assert_eq!(out, "1");
}

/// F-MY-06: `Pdo\Mysql::ATTR_FOUND_ROWS` switches what the server reports as an
/// UPDATE's affected-row count — and therefore what `PDOStatement::rowCount()`
/// returns — from "rows actually CHANGED" to "rows MATCHED by the WHERE clause".
/// The attribute ORs `CLIENT_FOUND_ROWS` into the HANDSHAKE capability flags
/// (php-src `mysql_driver.c:776-778`), so it is a per-CONNECTION property that
/// must be known before authentication: it cannot be set after the fact, and the
/// only observable difference is on an UPDATE that matches a row but changes
/// nothing.
///
/// The fixture therefore UPDATEs a row TO ITS OWN CURRENT VALUE (`n = 5` where it
/// already is 5) over two connections to the same server: the plain one reports
/// 0 (nothing changed), the `ATTR_FOUND_ROWS` one reports 1 (one row matched).
/// The plain connection doubles as the fixture's setup/teardown connection.
/// Docker: same server as above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_attr_found_rows_reports_matched_rows
#[test]
#[ignore]
fn mysql_attr_found_rows_reports_matched_rows() {
    let out = compile_and_run(
        r#"<?php
$dsn = (string) getenv("ELEPHC_MY_DSN");
$plain = new \Pdo\Mysql($dsn);
$plain->exec("DROP TABLE IF EXISTS my_found_rows");
$plain->exec("CREATE TABLE my_found_rows (id INTEGER PRIMARY KEY, n INTEGER)");
$plain->exec("INSERT INTO my_found_rows (id, n) VALUES (1, 5)");

$changedStmt = $plain->prepare("UPDATE my_found_rows SET n = 5 WHERE id = 1");
$changedStmt->execute();
$changed = $changedStmt->rowCount();

$found = new \Pdo\Mysql($dsn, null, null, [\Pdo\Mysql::ATTR_FOUND_ROWS => true]);
$matchedStmt = $found->prepare("UPDATE my_found_rows SET n = 5 WHERE id = 1");
$matchedStmt->execute();
$matched = $matchedStmt->rowCount();

$plain->exec("DROP TABLE my_found_rows");
echo $changed . ":" . $matched;
"#,
    );
    assert_eq!(out, "0:1");
}

/// F-CORE-10: the DEFAULT connect timeout. `my.rs::build_opts()` now applies
/// `DEFAULT_CONNECT_TIMEOUT_SECS` (30 s, php-src's own pdo_mysql default)
/// UNCONDITIONALLY, so a DSN naming neither a `connect_timeout` key nor (through
/// the prelude) `PDO::ATTR_TIMEOUT` no longer waits out the OS's TCP connect
/// timeout — roughly 130 s on Linux (`tcp_syn_retries=6`) and ~75 s on macOS.
/// Sibling of `test_mysql_attr_timeout_fails_fast`, which pins the EXPLICIT
/// attribute; this one deliberately passes NO options at all, which is the
/// configuration that used to hang.
///
/// Uses a non-routable TEST-NET-1 address (RFC 5737, `192.0.2.0/24`) so the SYN
/// blackholes rather than drawing an immediate "connection refused" — the same
/// address family the sibling test relies on. Needs no live server: the point is
/// that the connection NEVER completes.
///
/// The compile+run is driven from a worker thread behind a `recv_timeout` so a
/// regression FAILS this test rather than parking the whole suite on that OS
/// timeout (the in-PHP `< 35.0` assertion alone cannot bound a hang). The
/// warm-up run first pays for any lazy `cargo build -p elephc-pdo` of the bridge
/// staticlib and the cached SDK/runtime-object lookups OFF the clock, so the
/// guarded run is only ever a small compile plus the connect attempt itself, and
/// the 120 s budget has no legitimate way to be reached.
#[test]
#[ignore]
fn mysql_default_connect_timeout_bounds_blackholed_connect() {
    use std::sync::mpsc;
    use std::time::Duration;

    // Warm-up: an in-process SQLite PDO program links the very same bridge
    // staticlib, so it forces every lazy build/OnceLock the guarded run would
    // otherwise be billed for. No server needed.
    let warm = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo "warm";
"#,
    );
    assert_eq!(warm, "warm");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let out = compile_and_run(
            r#"<?php
class MysqlDefaultTimeoutClock { public static float $start = 0.0; }
MysqlDefaultTimeoutClock::$start = microtime(true);
try {
    $conn = new \Pdo\Mysql("mysql:host=192.0.2.1;port=3306;dbname=testdb");
    echo "connected";
} catch (PDOException $e) {
    $elapsed = microtime(true) - MysqlDefaultTimeoutClock::$start;
    echo ($elapsed < 35.0) ? "fast" : "slow";
}
"#,
        );
        let _ = tx.send(out);
    });

    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(out) => assert_eq!(out, "fast"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!(
            "connecting to a blackholed address with no ATTR_TIMEOUT was still running after \
             120 s: build_opts() is no longer applying the default connect timeout"
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("the compile/run worker panicked; its output is above")
        }
    }
}

/// F-CORE-16: the persistent pool is keyed on the (DSN, ATTR_PERSISTENT key) PAIR,
/// not on the DSN alone. php-src builds the persistent hashkey from both
/// (`pdo_dbh.c:389-404`): an `ATTR_PERSISTENT` that is a non-numeric, non-empty
/// STRING is a user-supplied POOL KEY, and separating one DSN into several
/// independent pooled connections is the entire point of that named form. Keying
/// on the DSN alone silently collapsed them onto ONE shared server session.
///
/// `SELECT CONNECTION_ID()` is the observation: it is the server's own id for the
/// session, so two handles that are really one connection cannot disagree on it.
/// The third open REUSES the first key and is the NECESSARY CONTROL — without it
/// a "distinct" result would also be produced by persistence being broken outright
/// (every open making a fresh connection), which is the opposite bug. Sharing one
/// pooled handle between two live PDO objects is safe: `elephc_pdo_close()`
/// no-ops on a persistent id (`lib.rs:600-604`), so neither destructor closes the
/// session out from under the other. Docker: same server as above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_persistent_key_separates_pooled_connections
#[test]
#[ignore]
fn mysql_persistent_key_separates_pooled_connections() {
    let out = compile_and_run(
        r#"<?php
$dsn = (string) getenv("ELEPHC_MY_DSN");
$a = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "elephc_key_a"]);
$b = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "elephc_key_b"]);
$again = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "elephc_key_a"]);
$idA = (string) $a->query("SELECT CONNECTION_ID()")->fetchColumn();
$idB = (string) $b->query("SELECT CONNECTION_ID()")->fetchColumn();
$idAgain = (string) $again->query("SELECT CONNECTION_ID()")->fetchColumn();
echo (($idA !== $idB) ? "distinct" : "same") . ":" . (($idA === $idAgain) ? "reuse" : "new");
"#,
    );
    assert_eq!(out, "distinct:reuse");
}

/// F-MY-08 / v43: `getColumnMeta()` on a `mysql:` statement reports MySQL's
/// wire type, PDO parameter type, source table, declared size/precision, and native
/// column flags rather than the SQLite storage-class vocabulary
/// ("integer"/"double"/"string") the prelude used to hand every driver. php-src
/// builds the key from `type_to_name_native()`, whose `PDO_MYSQL_NATIVE_TYPE_NAME`
/// macro simply stringifies the `MYSQL_TYPE_` suffix — so an `INT` is `LONG`, a
/// `VARCHAR` is `VAR_STRING`, a `DECIMAL` is `NEWDECIMAL`, a `BLOB`/`TEXT` is
/// `BLOB`, a `BIGINT` is `LONGLONG`, a `DATETIME` is `DATETIME`. That vocabulary is
/// the whole point of the key: the storage class cannot tell a `VARCHAR` from a
/// `BLOB` from a `NEWDECIMAL` (MySQL hands all three over as strings), which is
/// exactly what a caller reading `native_type` is asking about.
///
/// The six expectations below were cross-read against `my.rs::native_type_name()`,
/// the mapping actually implemented, and against php-src's switch: every arm agrees
/// (php-src's list is STRING, VAR_STRING, TINY, SHORT, LONG, LONGLONG, INT24, FLOAT,
/// DOUBLE, DECIMAL, NEWDECIMAL, GEOMETRY, TIMESTAMP, YEAR, SET, ENUM, DATE, NEWDATE,
/// TIME, DATETIME, TINY_BLOB, MEDIUM_BLOB, LONG_BLOB, BLOB, NULL, BIT, JSON, and a
/// `default:` that OMITS the key; `native_type_name` spells the same 27 names and
/// returns `""` — the bridge's "no metadata" value — for the default). No name
/// disagrees, so there is nothing to flag here.
///
/// The second half re-reads the SAME columns through a statement whose result set is
/// EMPTY (`WHERE 1 = 0`). It must report the identical names: `column_native_type`
/// reads the PREPARE-time column descriptor, never a live cell, so the DECLARED type
/// survives a result set with no row to inspect — the property the storage-class
/// derivation (which would report "null" for every column here) structurally cannot
/// have. `JSON` is deliberately absent from the fixture: MariaDB aliases it to
/// `LONGTEXT` and would report `BLOB`, pinning the server's alias rather than the
/// mapping. Docker: same server as the other MySQL fixtures, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_get_column_meta_native_types
#[test]
#[ignore]
fn mysql_get_column_meta_native_types() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_native_meta");
$db->exec("CREATE TABLE my_native_meta (i INT NOT NULL PRIMARY KEY, v VARCHAR(20) UNIQUE, m DECIMAL(10,2), b BLOB, big BIGINT, ts DATETIME)");
$db->exec("INSERT INTO my_native_meta VALUES (42, 'ada', '1234.50', 'bin', 9000000000, '2024-01-15 10:30:00')");

$rowed = $db->query("SELECT i, v, m, b, big, ts FROM my_native_meta");
$withRow = [];
for ($c = 0; $c < 6; $c++) {
    $meta = $rowed->getColumnMeta($c);
    $withRow[] = (string) $meta["native_type"] . ":" . $meta["pdo_type"];
}
$mi = $rowed->getColumnMeta(0);
$mv = $rowed->getColumnMeta(1);
$mm = $rowed->getColumnMeta(2);
$mb = $rowed->getColumnMeta(3);

$empty = $db->query("SELECT i, v, m, b, big, ts FROM my_native_meta WHERE 1 = 0");
$noRow = [];
for ($c = 0; $c < 6; $c++) {
    $meta = $empty->getColumnMeta($c);
    $noRow[] = (string) $meta["native_type"];
}

echo implode(",", $withRow) . "|" . implode(",", $noRow)
    . "|" . $mi["table"] . ":" . implode(",", $mi["flags"])
    . "|" . implode(",", $mv["flags"])
    . "|" . implode(",", $mb["flags"])
    . "|" . (($mv["len"] >= 20) ? "len-y" : "len-n") . ":" . $mm["precision"];
$db->exec("DROP TABLE my_native_meta");
"#,
    ));
    assert_eq!(
        out,
        "LONG:1,VAR_STRING:2,NEWDECIMAL:2,BLOB:2,LONGLONG:1,DATETIME:2|\
         LONG,VAR_STRING,NEWDECIMAL,BLOB,LONGLONG,DATETIME|\
         my_native_meta:not_null,primary_key|unique_key|blob|len-y:2"
    );
}

/// F-MY-03: under the `NO_BACKSLASH_ESCAPES` `sql_mode`, backslash is an ORDINARY
/// BYTE inside a MySQL string literal — doubling is the only escape left — so the
/// placeholder scanner has to stop assuming backslash-escaping there, or it
/// disagrees with the SERVER about where a literal ENDS and therefore about how many
/// placeholders the statement has.
///
/// A string literal ending in a BACKSLASH, with a placeholder just past it, is the
/// minimal statement that exposes it — `CONCAT('a\', txt) … WHERE txt = ?`. In this
/// mode the server closes the literal at the quote right after the backslash (its
/// value being the two bytes `a\`) and sees ONE placeholder. The old scanner read the
/// `\'` as an ESCAPED QUOTE, ran off the end of the SQL looking for a close that was
/// never coming, and swallowed the `?` as string content — so `translate_placeholders`
/// allocated ZERO slots while the server's own prepare of that same text reported one,
/// and the `execute()` below bound into a slot map with no slot 1. (A backslash in the
/// MIDDLE of a literal, `'C:\path'`, is NOT a witness: the old scanner consumed the `p`
/// and still found the real closing quote, so both modes agree. Only a TRAILING
/// backslash moves the literal's end.) `my.rs::MyConn::prepare()` now threads the
/// connection's LIVE `no_backslash_escape()` session state into the scan — the only
/// place that flag can be read, `translate_placeholders` being a free function with no
/// connection.
///
/// The literal is round-tripped through `CONCAT` against a real column, so the bound
/// parameter sits in a `WHERE` like every other fixture here and the result column is a
/// genuine string expression, not a bare `?` whose PREPARE-time type the server has not
/// yet inferred. The value is asserted through `bin2hex()` (`615c78` = `a\x`), not as a
/// printable string: the point is that the byte after `a` really is the backslash the
/// server kept, so a regression that dropped it or re-escaped it cannot pass. Both
/// placeholder spellings are driven, since `:name` and `?` share the string-literal scan
/// but not the slot bookkeeping. Docker: same server as above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_no_backslash_escapes_placeholder_scan
#[test]
#[ignore]
fn mysql_no_backslash_escapes_placeholder_scan() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'");
$db->exec("DROP TABLE IF EXISTS my_nbe");
$db->exec("CREATE TABLE my_nbe (txt VARCHAR(32))");
$db->exec("INSERT INTO my_nbe VALUES ('x'), ('y')");

// PHP "\\" is ONE backslash, so the SQL really is:
//     SELECT CONCAT('a\', txt) AS lit FROM my_nbe WHERE txt = ?
$pos = $db->prepare("SELECT CONCAT('a\\', txt) AS lit FROM my_nbe WHERE txt = ?");
$pos->execute(["x"]);
$posLit = $pos->fetchColumn();

$named = $db->prepare("SELECT CONCAT('a\\', txt) AS lit FROM my_nbe WHERE txt = :v");
$named->execute([":v" => "y"]);
$namedLit = $named->fetchColumn();

echo bin2hex($posLit) . ":" . bin2hex($namedLit);
$db->exec("DROP TABLE my_nbe");
"#,
    ));
    assert_eq!(out, "615c78:615c79");
}

/// F-STMT-15 on a NON-SQLITE driver: `FETCH_GROUP` and `FETCH_UNIQUE` (which used to
/// throw "not yet supported") reshape a live MySQL result set around a key taken from
/// COLUMN 0, which both modes CONSUME — the key is excluded from the row, and the row
/// is built from columns 1..n-1. `FETCH_GROUP` maps each key to a LIST of every row
/// that carried it, in result order; `FETCH_UNIQUE` maps it to ONE row, LAST WRITE
/// WINS (php-src overwrites with `zend_symtable_update` and never complains about the
/// duplicate). This is the driver-independence proof for the prelude's new
/// `fetchAllGrouped()`: it reads rows through `stepCursor()`/`columnValue()` like every
/// other fetch path, so nothing about it is SQLite-specific, and this fixture is what
/// says so rather than assuming it.
///
/// Four shapes, all on `('fruit','apple'), ('fruit','banana'), ('veg','carrot')`:
///  - `GROUP|COLUMN` with NO explicit index is the classic `[kind => [name, …]]` idiom.
///    It only works because php-src defaults the VALUE column to 1 when GROUP is set
///    (column 0 is already the key, so defaulting it to 0 would return the key again);
///    a regression there yields `[fruit => [fruit, fruit]]`, which this pins.
///  - `GROUP|ASSOC` maps to a list of column-name rows with the key column absent.
///  - `GROUP|NUM` proves the RE-INDEXING: the first column AFTER the key lands at [0],
///    not at its original offset [1] (php-src walks the row with a separate output
///    cursor). A row that kept its offsets would have no [0] at all.
///  - `UNIQUE|ASSOC` proves last-wins: 'fruit' appears twice, and the surviving row is
///    'banana', the LAST one.
///
/// Every key here is NON-NUMERIC on purpose. The one documented divergence in
/// `fetchAllGrouped()` is that elephc's array keeps an integer-LOOKING group key a
/// STRING key where PHP folds it back to an int; using kind names keeps this fixture
/// about PDO's grouping semantics instead of about that array-semantics gap. Docker:
/// same server as above, e.g.:
///   ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=33060;dbname=testdb;user=test;password=test' \
///       cargo test --test codegen_tests -- --ignored mysql_fetch_all_group_and_unique
#[test]
#[ignore]
fn mysql_fetch_all_group_and_unique() {
    let out = compile_and_run(&my_program(
        r#"
$db->exec("DROP TABLE IF EXISTS my_group");
$db->exec("CREATE TABLE my_group (kind VARCHAR(16), name VARCHAR(16), n INTEGER)");
$db->exec("INSERT INTO my_group VALUES ('fruit', 'apple', 1), ('fruit', 'banana', 2), ('veg', 'carrot', 3)");

$byCol = $db->query("SELECT kind, name FROM my_group ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_COLUMN);
$col = count($byCol["fruit"]) . ":" . $byCol["fruit"][0] . "," . $byCol["fruit"][1] . "/" . $byCol["veg"][0];

$byAssoc = $db->query("SELECT kind, name, n FROM my_group ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_ASSOC);
$assoc = count($byAssoc["fruit"]) . ":" . $byAssoc["fruit"][1]["name"] . "=" . $byAssoc["fruit"][1]["n"];

$byNum = $db->query("SELECT kind, name, n FROM my_group ORDER BY n")->fetchAll(PDO::FETCH_GROUP | PDO::FETCH_NUM);
$num = $byNum["veg"][0][0] . "=" . $byNum["veg"][0][1];

$uniq = $db->query("SELECT kind, name FROM my_group ORDER BY n")->fetchAll(PDO::FETCH_UNIQUE | PDO::FETCH_ASSOC);
$last = $uniq["fruit"]["name"] . "/" . $uniq["veg"]["name"];

echo $col . "|" . $assoc . "|" . $num . "|" . $last;
$db->exec("DROP TABLE my_group");
"#,
    ));
    assert_eq!(out, "2:apple,banana/carrot|2:banana=2|carrot=3|banana/carrot");
}

/// MySQL's two generic driver hooks are live rather than stored echo values:
/// AUTOCOMMIT reaches the server session, and DEFAULT_STR_PARAM controls the
/// national-string marker used by emulated prepared statements.
#[test]
#[ignore]
fn mysql_autocommit_and_default_string_parameter_attributes() {
    let out = compile_and_run(&my_program(
        r#"
echo $db->getAttribute(PDO::ATTR_AUTOCOMMIT) ? "1" : "0";
echo $db->setAttribute(PDO::ATTR_AUTOCOMMIT, false) ? "1" : "0";
echo $db->getAttribute(PDO::ATTR_AUTOCOMMIT) ? "1" : "0";
echo $db->setAttribute(PDO::ATTR_AUTOCOMMIT, true) ? "1" : "0";
echo "|";
echo $db->setAttribute(PDO::ATTR_DEFAULT_STR_PARAM, PDO::PARAM_STR_NATL) ? "1" : "0";
echo ($db->getAttribute(PDO::ATTR_DEFAULT_STR_PARAM) === PDO::PARAM_STR_NATL) ? "N" : "C";
$stmt = $db->prepare("SELECT ?");
$stmt->execute(["café"]);
echo $stmt->fetchColumn();
echo $db->setAttribute(PDO::ATTR_DEFAULT_STR_PARAM, PDO::PARAM_STR_CHAR) ? "1" : "0";
echo ($db->getAttribute(PDO::ATTR_DEFAULT_STR_PARAM) === PDO::PARAM_STR_CHAR) ? "C" : "N";
"#,
    ));
    assert_eq!(out, "1101|1Ncafé1C");
}

/// Verifies emulated MySQL multi-statements retain every wire result set and
/// `nextRowset()` refreshes the active rows/metadata until it returns false.
#[test]
#[ignore]
fn mysql_next_rowset_traverses_multi_statement_results() {
    let out = compile_and_run(&my_program(
        r#"
$stmt = $db->query("SELECT 1 AS value; SELECT 2 AS value; SELECT 3 AS value");
echo $stmt->fetchColumn() . ":" . $stmt->columnCount() . "|";
echo ($stmt->nextRowset() ? "next" : "done") . ":" . $stmt->fetchColumn() . "|";
echo ($stmt->nextRowset() ? "next" : "done") . ":" . $stmt->fetchColumn() . "|";
echo $stmt->nextRowset() ? "next" : "done";
"#,
    ));
    assert_eq!(out, "1:1|next:2|next:3|done");
}
