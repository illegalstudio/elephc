//! Purpose:
//! Offline mysqli prelude tests that do not need a live MySQL server: surface
//! injection, no PDO class leak, and (in later tasks) connect-failure paths and
//! escaping that need no live row.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the test harness.
//!
//! Key details:
//! - Live query/fetch fixtures live in `mysqli_mysql.rs` and are `#[ignore]`d
//!   (they need `ELEPHC_MY_DSN`, same as `pdo_mysql.rs`).
//! - The class-leak assertions are the point: a mysqli-only program must declare
//!   `mysqli` but not `PDO`, and a PDO-only program must not grow `mysqli`.

use crate::support::*;

/// mysqli's internal constructor rejects surplus arguments as a catchable runtime error.
#[test]
fn test_mysqli_constructor_rejects_extra_arguments_at_runtime() {
    let out = compile_and_run(
        r#"<?php
try {
    new mysqli("h", "u", "p", "d", 3306, null, 999);
} catch (ArgumentCountError $error) {
    echo $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "mysqli::__construct() expects at most 6 arguments, 7 given"
    );
}

/// A bare `MYSQLI_*` constant is enough to inject the surface (issue: a config
/// array or helper that only forwards the constant must not fail with
/// "undefined constant"), and `mysqli_sql_exception::getSqlState()` (php 8.1+)
/// reads the SQLSTATE.
#[test]
fn test_mysqli_bare_constant_and_getsqlstate() {
    let out = compile_and_run(
        r#"<?php
$cfg = ['mode' => MYSQLI_ASSOC, 'flags' => MYSQLI_REPORT_STRICT];
echo $cfg['mode'], $cfg['flags'];
$e = new mysqli_sql_exception("boom");
$e->sqlstate = "42S02";
echo "|", $e->getSqlState();
"#,
    );
    assert_eq!(out, "12|42S02");
}

/// The completed procedural surface is declared: the savepoint API, the
/// two-step statement API, the statement introspectors, the result field
/// cursor, and get_charset / thread_safe all pass `function_exists`, and the
/// new `MYSQLI_TRANS_COR_*` constants are defined.
#[test]
fn test_mysqli_completed_surface_exists() {
    let out = compile_and_run(
        r#"<?php
$fns = [
    'mysqli_savepoint', 'mysqli_release_savepoint',
    'mysqli_stmt_init', 'mysqli_stmt_prepare', 'mysqli_execute',
    'mysqli_stmt_sqlstate', 'mysqli_stmt_field_count', 'mysqli_stmt_insert_id',
    'mysqli_stmt_error_list', 'mysqli_stmt_free_result',
    'mysqli_fetch_lengths', 'mysqli_field_seek', 'mysqli_field_tell',
    'mysqli_get_charset', 'mysqli_thread_safe',
];
$missing = 0;
foreach ($fns as $f) { if (!function_exists($f)) { $missing++; } }
echo $missing === 0 ? "all-fns" : ("missing:" . $missing);
echo "|", method_exists('mysqli', 'savepoint') && method_exists('mysqli', 'stmt_init') ? "methods" : "no";
echo "|", method_exists('mysqli_result', 'field_seek') ? "field_seek" : "no";
echo "|", defined('MYSQLI_TRANS_COR_AND_CHAIN') && defined('MYSQLI_TRANS_COR_RELEASE') ? "cor" : "no";
echo "|", mysqli_thread_safe() === false ? "not-ts" : "ts";
"#,
    );
    assert_eq!(out, "all-fns|methods|field_seek|cor|not-ts");
}

/// The `__elephc*` instance internals are compiler plumbing, not part of PHP's
/// mysqli surface: user code calling them hits normal private-member
/// visibility (a catchable `Error` at runtime), exactly like the PDO prelude's
/// factories. (The static `__elephcInit` factory is rejected at compile time —
/// see `error_tests/mysqli.rs`.)
#[test]
fn test_mysqli_internal_helpers_are_private() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$stmt = new mysqli_stmt();
try {
    $stmt->__elephcBindParamValues("i", [1]);
    echo "callable";
} catch (Error $e) {
    echo "blocked";
}
"#,
    );
    assert_eq!(out, "blocked");
}

/// The transaction-name allowlist rule needs no server: an empty name is a
/// `ValueError` (php's exact message), and a name carrying an executable-comment
/// injection payload is silently sanitized rather than throwing (php strips it),
/// so the call is accepted — the actual neutralization is asserted live.
#[test]
fn test_mysqli_transaction_name_validation_offline() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = mysqli_init();
// Empty name throws before any connection is even required.
try { $db->begin_transaction(0, ""); echo "no"; }
catch (ValueError $e) {
    echo strpos($e->getMessage(), "must not be empty") !== false ? "empty-ve" : "ve?";
}
// A malicious name does NOT throw (php strips, does not reject); on an
// unconnected object the strip happens first, then the connection Error.
try { $db->begin_transaction(0, "!50000 ; DROP"); echo "|no-strip-throw"; }
catch (ValueError $e) { echo "|stripped-threw-ve"; }
catch (Error $e) { echo "|conn-error"; }
// Unlike begin_transaction, commit/rollback do NOT throw on an empty name
// (php sends COMMIT /**/); the empty check is skipped, so on an unconnected
// object they reach the connection Error, not a ValueError.
try { $db->commit(0, ""); echo "|no"; }
catch (ValueError $e) { echo "|commit-ve"; }
catch (Error $e) { echo "|commit-conn"; }
try { $db->rollback(0, ""); echo "|no"; }
catch (ValueError $e) { echo "|rollback-ve"; }
catch (Error $e) { echo "|rollback-conn"; }
"#,
    );
    // begin: empty→ValueError, malicious→stripped→conn Error; commit/rollback:
    // empty name is NOT a ValueError (reaches the connection Error instead).
    assert_eq!(out, "empty-ve|conn-error|commit-conn|rollback-conn");
}

/// A mysqli-only program declares the mysqli surface (class, procedural alias,
/// constants) and does NOT leak the PDO classes. The `new mysqli()` is the
/// detection trigger: capability probes are string literals and deliberately
/// never inject (same rule as PDO; `--with-mysqli` forces probe-only programs).
#[test]
fn test_mysqli_class_exists_and_does_not_leak_pdo() {
    let out = compile_and_run(
        r#"<?php
$db = new mysqli();
echo class_exists('mysqli') ? '1' : '0';
echo class_exists('PDO') ? '1' : '0';
echo function_exists('mysqli_connect') ? '1' : '0';
echo defined('MYSQLI_ASSOC') ? '1' : '0';
"#,
    );
    assert_eq!(out, "1011");
}

/// A PDO-only program does not grow the mysqli surface.
#[test]
fn test_pdo_program_does_not_grow_mysqli() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
echo class_exists('mysqli') ? '1' : '0';
echo class_exists('PDO') ? '1' : '0';
"#,
    );
    assert_eq!(out, "01");
}

/// A failed constructor connect under `MYSQLI_REPORT_OFF` leaves a usable
/// object with `connect_errno` / `connect_error` populated (no exception, no
/// PDO types). Port 1 on localhost refuses immediately, so this needs no
/// server and cannot hang.
#[test]
fn test_mysqli_connect_failure_sets_connect_errno() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = @new mysqli("127.0.0.1", "nope", "nope", "nope", 1);
echo $db->connect_errno > 0 ? "err" : "ok";
echo "|";
echo $db->connect_error !== "" ? "msg" : "empty";
"#,
    );
    assert_eq!(out, "err|msg");
}

/// Procedural `mysqli_connect()` returns `false` on failure under REPORT_OFF,
/// and the no-argument `mysqli_connect_errno()` / `mysqli_connect_error()`
/// read the process-wide last-connect failure.
#[test]
fn test_mysqli_connect_procedural_failure_returns_false() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = mysqli_connect("127.0.0.1", "nope", "nope", "nope", 1);
echo $db === false ? "F" : "obj";
echo "|", mysqli_connect_errno() > 0 ? "err" : "ok";
echo "|", mysqli_connect_error() !== null ? "msg" : "null";
"#,
    );
    assert_eq!(out, "F|err|msg");
}

/// SECURITY: a `;` in the host, database, or socket argument is rejected at
/// connect time (errno 2002) rather than injected as a second DSN directive
/// that would redirect the connection to an attacker-chosen server. Needs no
/// live server — the rejection happens before any bridge open.
#[test]
fn test_mysqli_connect_rejects_dsn_separator_injection() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = new mysqli("127.0.0.1;host=192.0.2.1", "u", "p", "d", 3306);
echo $db->connect_errno === 2002 ? "host-rejected" : ("host:" . $db->connect_errno);
$db2 = new mysqli("127.0.0.1", "u", "p", "app;host=192.0.2.1", 3306);
echo "|", $db2->connect_errno === 2002 ? "db-rejected" : ("db:" . $db2->connect_errno);
$db3 = new mysqli("localhost", "u", "p", "d", 3306, "/tmp/x;host=192.0.2.1");
echo "|", $db3->connect_errno === 2002 ? "sock-rejected" : ("sock:" . $db3->connect_errno);
"#,
    );
    assert_eq!(out, "host-rejected|db-rejected|sock-rejected");
}

/// Under `MYSQLI_REPORT_STRICT` a failed connect throws `mysqli_sql_exception`
/// — never `PDOException` — with the SQLSTATE on the public property.
#[test]
fn test_mysqli_connect_failure_strict_throws_mysqli_sql_exception() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
try {
    new mysqli("127.0.0.1", "nope", "nope", "nope", 1);
    echo "no-throw";
} catch (mysqli_sql_exception $e) {
    echo "caught|", strlen($e->getMessage()) > 0 ? "msg" : "empty";
    echo "|", $e->sqlstate !== "" ? "state" : "none";
}
"#,
    );
    assert_eq!(out, "caught|msg|state");
}

/// Operations on an unconnected (`mysqli_init()`) object raise php 8's
/// `Error: mysqli object is not fully initialized` — including
/// `real_escape_string`, which previously returned a value with no signal at
/// all. `options()` is the exception: it just records state and succeeds.
#[test]
fn test_mysqli_unconnected_ops_raise_error() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = mysqli_init();
$probe = function(callable $fn): string {
    try { $fn(); return "no-err"; }
    catch (Error $e) {
        return strpos($e->getMessage(), "not fully initialized") !== false ? "err" : "err?";
    }
};
echo $probe(fn() => $db->ping());
echo "|", $probe(fn() => $db->select_db("nope"));
echo "|", $probe(fn() => $db->real_escape_string("a'b"));
echo "|", $probe(fn() => $db->character_set_name());
echo "|", $db->options(MYSQLI_OPT_CONNECT_TIMEOUT, 1) ? "opt-T" : "opt-F";
"#,
    );
    assert_eq!(out, "err|err|err|err|opt-T");
}

/// An empty query string throws `ValueError` (php-src's message); a query on an
/// unconnected object raises php 8's not-fully-initialized `Error`.
#[test]
fn test_mysqli_query_empty_string_and_unconnected() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = mysqli_init();
try {
    $db->query("");
    echo "no";
} catch (ValueError $e) {
    echo "ve";
}
try {
    $db->query("SELECT 1");
    echo "|no";
} catch (Error $e) {
    echo "|", strpos($e->getMessage(), "not fully initialized") !== false ? "err" : "err?";
}
"#,
    );
    assert_eq!(out, "ve|err");
}

/// Procedural aliases validate their link/result argument at runtime with a
/// `TypeError` naming the expected class (PHP's own behavior), so the classic
/// `mysqli_query(...)` → `mysqli_num_rows(...)` pipeline fails loudly on a
/// `false` result instead of reading garbage.
#[test]
fn test_mysqli_procedural_alias_type_errors() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
try {
    mysqli_num_rows(false);
    echo "no";
} catch (TypeError $e) {
    echo strpos($e->getMessage(), "mysqli_result") !== false ? "te-result" : "te-?";
}
try {
    mysqli_ping("not-a-link");
    echo "|no";
} catch (TypeError $e) {
    echo "|", strpos($e->getMessage(), "must be of type mysqli") !== false ? "te-link" : "te-?";
}
"#,
    );
    assert_eq!(out, "te-result|te-link");
}

/// `bind_param` validates offline: a type character outside `i`/`d`/`s`/`b`
/// throws `ValueError`; a types-vs-variables count mismatch reports and
/// returns `false` under REPORT_OFF; execute on an unprepared statement fails
/// with `errno` set.
#[test]
fn test_mysqli_stmt_bind_param_validation() {
    // The statement comes from a `mysqli_stmt|false`-typed helper, the same
    // union shape `mysqli::prepare` returns: statement method calls dispatch
    // dynamically on the union receiver (a concretely-typed receiver would
    // instead hit the checker's by-ref storage rule at compile time — loud,
    // and documented).
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
function make_stmt(): mysqli_stmt|false {
    return new mysqli_stmt();
}
$stmt = make_stmt();
$v = 1;
$w = 2;
try {
    $stmt->bind_param("x", $v);
    echo "no";
} catch (ValueError $e) {
    echo "ve";
}
echo "|", $stmt->bind_param("is", $v) ? "T" : "F";
echo "|", $stmt->bind_param("i", $v, $w) ? "T" : "F";
echo "|", $stmt->execute() ? "T" : "F";
echo "|", $stmt->errno;
"#,
    );
    assert_eq!(out, "ve|F|F|F|2006");
}

/// The mysqli exception hierarchy is mysqli's own: `mysqli_sql_exception`
/// extends `RuntimeException`, and the locked `MYSQLI_*` constants carry
/// php-src's values.
#[test]
fn test_mysqli_exception_and_constants() {
    let out = compile_and_run(
        r#"<?php
$e = new mysqli_sql_exception("boom");
echo $e instanceof RuntimeException ? 'rt' : 'no';
echo '|', MYSQLI_ASSOC, MYSQLI_NUM, MYSQLI_BOTH;
echo '|', MYSQLI_REPORT_OFF, MYSQLI_REPORT_ERROR, MYSQLI_REPORT_STRICT;
echo '|', MYSQLI_CLIENT_SSL;
echo '|', $e->sqlstate;
"#,
    );
    assert_eq!(out, "rt|123|012|2048|00000");
}
