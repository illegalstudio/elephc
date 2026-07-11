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
$start = microtime(true);
try {
    $conn = new \Pdo\Mysql("mysql:host=192.0.2.1;port=3306;dbname=testdb", null, null, [PDO::ATTR_TIMEOUT => 2]);
    echo "connected";
} catch (PDOException $e) {
    $elapsed = microtime(true) - $start;
    echo ($elapsed < 10.0) ? "fast" : "slow";
}
"#,
    );
    assert_eq!(out, "fast");
}

/// `Pdo\Mysql::getWarningCount()` reports the warning count of the last statement,
/// cached from that statement's terminal OK packet. `CREATE TABLE IF NOT EXISTS`
/// on an existing table raises one "table already exists" warning (an OK-terminated
/// DDL statement, so the count is surfaced — unlike a SELECT warning, which sits in
/// an EOF packet the pure-Rust client does not expose). Driven against the live
/// server as the driver subclass directly.
#[test]
#[ignore]
fn test_mysql_get_warning_count() {
    let out = compile_and_run(
        "<?php\n$db = new \\Pdo\\Mysql((string) getenv(\"ELEPHC_MY_DSN\"));\n$db->exec(\"DROP TABLE IF EXISTS elephc_warn_probe\");\n$db->exec(\"CREATE TABLE elephc_warn_probe (id INT)\");\n$db->exec(\"CREATE TABLE IF NOT EXISTS elephc_warn_probe (id INT)\");\n$n = $db->getWarningCount();\n$db->exec(\"DROP TABLE elephc_warn_probe\");\necho $n;\n",
    );
    assert_eq!(out, "1");
}

/// Live TLS round-trip. Opens a MySQL/MariaDB connection with `Pdo\Mysql::ATTR_SSL_CA`
/// set to the server CA bundle (path in `ELEPHC_MY_TLS_CA`) and confirms a query
/// returns over the encrypted connection. UNLIKE pg, MySQL TLS is opt-in: the linked
/// staticlib must be rebuilt with the `mysql-tls` feature first (it pulls aws-lc-rs),
/// otherwise the bridge fails loud with a "requires the opt-in `mysql-tls` feature"
/// error. `#[ignore]` — needs a TLS-serving MySQL. Example:
///   docker run -d --name mytls -e MYSQL_ROOT_PASSWORD=test -e MYSQL_DATABASE=testdb \
///       -e MYSQL_USER=test -e MYSQL_PASSWORD=test -p 33062:3306 mysql:8 \
///       --require-secure-transport=ON
///   docker cp mytls:/var/lib/mysql/ca.pem ./ca.pem   # server-generated CA
///   cargo build -p elephc-pdo --features mysql-tls    # TLS staticlib (aws-lc-rs)
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
