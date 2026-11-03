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

/// A bare `MYSQLI_*` constant is enough to inject the surface (issue: a config
/// array or helper that only forwards the constant must not fail with
/// "undefined constant"), and `mysqli_sql_exception::getSqlState()` reads the
/// protected SQLSTATE (php's default "00000" for a hand-built exception).
#[test]
fn test_mysqli_bare_constant_and_getsqlstate() {
    let out = compile_and_run(
        r#"<?php
$cfg = ['mode' => MYSQLI_ASSOC, 'flags' => MYSQLI_REPORT_STRICT];
echo $cfg['mode'], $cfg['flags'];
$e = new mysqli_sql_exception("boom");
$previous = new Exception("root");
$chained = new mysqli_sql_exception("outer", 0, $previous);
echo "|", $e->getSqlState(), "|", $chained->getPrevious()->getMessage();
"#,
    );
    assert_eq!(out, "12|00000|root");
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
/// factories. (The static factories are rejected at compile time — see
/// `error_tests/mysqli.rs`.)
#[test]
fn test_mysqli_internal_helpers_are_private() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = new mysqli();
try {
    $db->__elephcConn();
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
    echo "|", $e->getSqlState() !== "" ? "state" : "none";
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

/// A statement or result needs a connected link, exactly like php: building
/// either on a `mysqli_init()` object raises `Error: mysqli object is not fully
/// initialized` (both constructors are public, as in php), and so do the
/// procedural `mysqli_stmt_init()` and `mysqli_prepare()`.
#[test]
fn test_mysqli_stmt_and_result_constructors_need_a_connection() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = new mysqli();
$probe = function(callable $fn): string {
    try { $fn(); return "no-err"; }
    catch (Error $e) { return $e->getMessage() === "mysqli object is not fully initialized" ? "err" : "err?"; }
};
echo $probe(fn() => new mysqli_stmt($db));
echo "|", $probe(fn() => new mysqli_stmt($db, "SELECT 1"));
echo "|", $probe(fn() => mysqli_stmt_init($db));
echo "|", $probe(fn() => mysqli_prepare($db, "SELECT 1"));
echo "|", $probe(fn() => new mysqli_result($db));
"#,
    );
    assert_eq!(out, "err|err|err|err|err");
}

/// The mysqli exception hierarchy is mysqli's own: `mysqli_sql_exception`
/// extends `RuntimeException`, and the `MYSQLI_*` constants carry php-src's
/// values.
#[test]
fn test_mysqli_exception_and_constants() {
    let out = compile_and_run(
        r#"<?php
$e = new mysqli_sql_exception("boom");
echo $e instanceof RuntimeException ? 'rt' : 'no';
echo '|', MYSQLI_ASSOC, MYSQLI_NUM, MYSQLI_BOTH;
echo '|', MYSQLI_REPORT_OFF, MYSQLI_REPORT_ERROR, MYSQLI_REPORT_STRICT;
echo '|', MYSQLI_CLIENT_SSL;
echo '|', $e->getSqlState();
"#,
    );
    assert_eq!(out, "rt|123|012|2048|00000");
}

/// Every procedural function php 8.5's mysqli declares exists (106 of them),
/// every class exists with php's method set, and the methods elephc used to add
/// on top of php (`get_host_info`, `field_tell`, …) are gone, so `method_exists`
/// answers exactly like php.
#[test]
fn test_mysqli_full_php_surface_exists() {
    let out = compile_and_run(
        r#"<?php
// A bare constant injects the surface (string probes alone never do).
$probe = MYSQLI_ASSOC;
$fns = [
    'mysqli_affected_rows', 'mysqli_autocommit', 'mysqli_begin_transaction',
    'mysqli_change_user', 'mysqli_character_set_name', 'mysqli_close', 'mysqli_commit',
    'mysqli_connect', 'mysqli_connect_errno', 'mysqli_connect_error', 'mysqli_data_seek',
    'mysqli_dump_debug_info', 'mysqli_debug', 'mysqli_errno', 'mysqli_error',
    'mysqli_error_list', 'mysqli_stmt_execute', 'mysqli_execute', 'mysqli_execute_query',
    'mysqli_fetch_field', 'mysqli_fetch_fields', 'mysqli_fetch_field_direct',
    'mysqli_fetch_lengths', 'mysqli_fetch_all', 'mysqli_fetch_array', 'mysqli_fetch_assoc',
    'mysqli_fetch_object', 'mysqli_fetch_row', 'mysqli_fetch_column', 'mysqli_field_count',
    'mysqli_field_seek', 'mysqli_field_tell', 'mysqli_free_result',
    'mysqli_get_connection_stats', 'mysqli_get_client_stats', 'mysqli_get_charset',
    'mysqli_get_client_info', 'mysqli_get_client_version', 'mysqli_get_links_stats',
    'mysqli_get_host_info', 'mysqli_get_proto_info', 'mysqli_get_server_info',
    'mysqli_get_server_version', 'mysqli_get_warnings', 'mysqli_init', 'mysqli_info',
    'mysqli_insert_id', 'mysqli_kill', 'mysqli_more_results', 'mysqli_multi_query',
    'mysqli_next_result', 'mysqli_num_fields', 'mysqli_num_rows', 'mysqli_options',
    'mysqli_set_opt', 'mysqli_ping', 'mysqli_poll', 'mysqli_prepare', 'mysqli_report',
    'mysqli_query', 'mysqli_real_connect', 'mysqli_real_escape_string',
    'mysqli_escape_string', 'mysqli_real_query', 'mysqli_reap_async_query',
    'mysqli_release_savepoint', 'mysqli_rollback', 'mysqli_savepoint', 'mysqli_select_db',
    'mysqli_set_charset', 'mysqli_stmt_affected_rows', 'mysqli_stmt_attr_get',
    'mysqli_stmt_attr_set', 'mysqli_stmt_bind_param', 'mysqli_stmt_bind_result',
    'mysqli_stmt_close', 'mysqli_stmt_data_seek', 'mysqli_stmt_errno', 'mysqli_stmt_error',
    'mysqli_stmt_error_list', 'mysqli_stmt_fetch', 'mysqli_stmt_field_count',
    'mysqli_stmt_free_result', 'mysqli_stmt_get_result', 'mysqli_stmt_get_warnings',
    'mysqli_stmt_init', 'mysqli_stmt_insert_id', 'mysqli_stmt_more_results',
    'mysqli_stmt_next_result', 'mysqli_stmt_num_rows', 'mysqli_stmt_param_count',
    'mysqli_stmt_prepare', 'mysqli_stmt_reset', 'mysqli_stmt_result_metadata',
    'mysqli_stmt_send_long_data', 'mysqli_stmt_store_result', 'mysqli_stmt_sqlstate',
    'mysqli_sqlstate', 'mysqli_ssl_set', 'mysqli_stat', 'mysqli_store_result',
    'mysqli_thread_id', 'mysqli_thread_safe', 'mysqli_use_result', 'mysqli_warning_count',
    'mysqli_refresh',
];
$missing = [];
foreach ($fns as $f) { if (!function_exists($f)) { $missing[] = $f; } }
echo count($fns), ":", count($missing) === 0 ? "all-fns" : implode(",", $missing);
$classes = class_exists('mysqli') && class_exists('mysqli_result') && class_exists('mysqli_stmt')
    && class_exists('mysqli_driver') && class_exists('mysqli_warning') && class_exists('mysqli_sql_exception');
echo "|classes:", $classes ? 0 : 1;
// method_exists() needs literal names in AOT mode, so each probe is spelled out.
$absentMethods = 0;
if (!method_exists('mysqli', 'autocommit')) { $absentMethods++; }
if (!method_exists('mysqli', 'begin_transaction')) { $absentMethods++; }
if (!method_exists('mysqli', 'change_user')) { $absentMethods++; }
if (!method_exists('mysqli', 'character_set_name')) { $absentMethods++; }
if (!method_exists('mysqli', 'close')) { $absentMethods++; }
if (!method_exists('mysqli', 'commit')) { $absentMethods++; }
if (!method_exists('mysqli', 'connect')) { $absentMethods++; }
if (!method_exists('mysqli', 'dump_debug_info')) { $absentMethods++; }
if (!method_exists('mysqli', 'debug')) { $absentMethods++; }
if (!method_exists('mysqli', 'get_charset')) { $absentMethods++; }
if (!method_exists('mysqli', 'execute_query')) { $absentMethods++; }
if (!method_exists('mysqli', 'get_client_info')) { $absentMethods++; }
if (!method_exists('mysqli', 'get_connection_stats')) { $absentMethods++; }
if (!method_exists('mysqli', 'get_server_info')) { $absentMethods++; }
if (!method_exists('mysqli', 'get_warnings')) { $absentMethods++; }
if (!method_exists('mysqli', 'init')) { $absentMethods++; }
if (!method_exists('mysqli', 'kill')) { $absentMethods++; }
if (!method_exists('mysqli', 'multi_query')) { $absentMethods++; }
if (!method_exists('mysqli', 'more_results')) { $absentMethods++; }
if (!method_exists('mysqli', 'next_result')) { $absentMethods++; }
if (!method_exists('mysqli', 'ping')) { $absentMethods++; }
if (!method_exists('mysqli', 'poll')) { $absentMethods++; }
if (!method_exists('mysqli', 'prepare')) { $absentMethods++; }
if (!method_exists('mysqli', 'query')) { $absentMethods++; }
if (!method_exists('mysqli', 'real_connect')) { $absentMethods++; }
if (!method_exists('mysqli', 'real_escape_string')) { $absentMethods++; }
if (!method_exists('mysqli', 'reap_async_query')) { $absentMethods++; }
if (!method_exists('mysqli', 'escape_string')) { $absentMethods++; }
if (!method_exists('mysqli', 'real_query')) { $absentMethods++; }
if (!method_exists('mysqli', 'release_savepoint')) { $absentMethods++; }
if (!method_exists('mysqli', 'rollback')) { $absentMethods++; }
if (!method_exists('mysqli', 'savepoint')) { $absentMethods++; }
if (!method_exists('mysqli', 'select_db')) { $absentMethods++; }
if (!method_exists('mysqli', 'set_charset')) { $absentMethods++; }
if (!method_exists('mysqli', 'options')) { $absentMethods++; }
if (!method_exists('mysqli', 'set_opt')) { $absentMethods++; }
if (!method_exists('mysqli', 'ssl_set')) { $absentMethods++; }
if (!method_exists('mysqli', 'stat')) { $absentMethods++; }
if (!method_exists('mysqli', 'stmt_init')) { $absentMethods++; }
if (!method_exists('mysqli', 'store_result')) { $absentMethods++; }
if (!method_exists('mysqli', 'thread_safe')) { $absentMethods++; }
if (!method_exists('mysqli', 'use_result')) { $absentMethods++; }
if (!method_exists('mysqli', 'refresh')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'close')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'free')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'data_seek')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_field')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_fields')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_field_direct')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_all')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_array')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_assoc')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_object')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_row')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'fetch_column')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'field_seek')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'free_result')) { $absentMethods++; }
if (!method_exists('mysqli_result', 'getIterator')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'attr_get')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'attr_set')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'bind_param')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'bind_result')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'close')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'data_seek')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'execute')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'fetch')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'get_warnings')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'result_metadata')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'more_results')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'next_result')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'num_rows')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'send_long_data')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'free_result')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'reset')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'prepare')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'store_result')) { $absentMethods++; }
if (!method_exists('mysqli_stmt', 'get_result')) { $absentMethods++; }
if (!method_exists('mysqli_warning', 'next')) { $absentMethods++; }
if (!method_exists('mysqli_sql_exception', 'getSqlState')) { $absentMethods++; }
echo "|methods:", $absentMethods === 0 ? "all" : $absentMethods;
$extra = 0;
if (method_exists('mysqli', 'get_host_info')) { $extra++; }
if (method_exists('mysqli', 'get_proto_info')) { $extra++; }
if (method_exists('mysqli', 'get_server_version')) { $extra++; }
if (method_exists('mysqli', 'get_client_version')) { $extra++; }
if (method_exists('mysqli_result', 'field_tell')) { $extra++; }
echo "|extra:", $extra;
"#,
    );
    assert_eq!(out, "106:all-fns|classes:0|methods:all|extra:0");
}

/// The constant surface carries php-src's exact values across the families
/// (options, client flags, field flags, field types, refresh, statement
/// attributes, async), and `MYSQLI_IS_MARIADB` is the always-false bool.
#[test]
fn test_mysqli_constant_values_match_php() {
    let out = compile_and_run(
        r#"<?php
echo MYSQLI_READ_DEFAULT_GROUP, ",", MYSQLI_READ_DEFAULT_FILE, ",", MYSQLI_OPT_LOCAL_INFILE, ",",
    MYSQLI_OPT_LOAD_DATA_LOCAL_DIR, ",", MYSQLI_OPT_READ_TIMEOUT, ",", MYSQLI_OPT_NET_CMD_BUFFER_SIZE, ",",
    MYSQLI_OPT_NET_READ_BUFFER_SIZE, ",", MYSQLI_OPT_INT_AND_FLOAT_NATIVE, ",", MYSQLI_OPT_SSL_VERIFY_SERVER_CERT, ",",
    MYSQLI_SERVER_PUBLIC_KEY, ",", MYSQLI_OPT_CAN_HANDLE_EXPIRED_PASSWORDS, "\n";
echo MYSQLI_CLIENT_NO_SCHEMA, ",", MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT, ",", MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT, ",",
    MYSQLI_CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS, ",", MYSQLI_ASYNC, ",", MYSQLI_STORE_RESULT_COPY_DATA, "\n";
echo MYSQLI_NOT_NULL_FLAG, ",", MYSQLI_PRI_KEY_FLAG, ",", MYSQLI_UNIQUE_KEY_FLAG, ",", MYSQLI_MULTIPLE_KEY_FLAG, ",",
    MYSQLI_BLOB_FLAG, ",", MYSQLI_UNSIGNED_FLAG, ",", MYSQLI_ZEROFILL_FLAG, ",", MYSQLI_BINARY_FLAG, ",", MYSQLI_ENUM_FLAG, ",",
    MYSQLI_AUTO_INCREMENT_FLAG, ",", MYSQLI_TIMESTAMP_FLAG, ",", MYSQLI_SET_FLAG, ",", MYSQLI_NO_DEFAULT_VALUE_FLAG, ",",
    MYSQLI_ON_UPDATE_NOW_FLAG, ",", MYSQLI_PART_KEY_FLAG, ",", MYSQLI_GROUP_FLAG, ",", MYSQLI_NUM_FLAG, "\n";
echo MYSQLI_TYPE_CHAR, ",", MYSQLI_TYPE_VECTOR, ",", MYSQLI_NO_DATA, ",", MYSQLI_DATA_TRUNCATED, ",", MYSQLI_DEBUG_TRACE_ENABLED, ",",
    MYSQLI_SERVER_QUERY_NO_GOOD_INDEX_USED, ",", MYSQLI_SERVER_QUERY_NO_INDEX_USED, ",", MYSQLI_SERVER_QUERY_WAS_SLOW, ",",
    MYSQLI_SERVER_PS_OUT_PARAMS, "\n";
echo MYSQLI_REFRESH_GRANT, ",", MYSQLI_REFRESH_LOG, ",", MYSQLI_REFRESH_TABLES, ",", MYSQLI_REFRESH_HOSTS, ",",
    MYSQLI_REFRESH_STATUS, ",", MYSQLI_REFRESH_THREADS, ",", MYSQLI_REFRESH_REPLICA, ",", MYSQLI_REFRESH_SLAVE, ",",
    MYSQLI_REFRESH_MASTER, ",", MYSQLI_REFRESH_BACKUP_LOG, "\n";
echo MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH, ",", MYSQLI_STMT_ATTR_CURSOR_TYPE, ",", MYSQLI_CURSOR_TYPE_NO_CURSOR, ",",
    MYSQLI_CURSOR_TYPE_READ_ONLY, "\n";
var_dump(MYSQLI_IS_MARIADB);
"#,
    );
    assert_eq!(
        out,
        "5,4,8,43,11,202,203,201,21,35,37\n16,1073741824,64,4194304,8,16\n1,2,4,8,16,32,64,128,256,512,1024,2048,4096,8192,16384,32768,32768\n1,242,100,101,0,16,32,2048,4096\n1,2,4,8,16,32,64,64,128,2097152\n0,1,0,1\nbool(false)\n"
    );
}

/// The surface follows `--php-version`: PHP 8.0 has no `MYSQLI_IS_MARIADB`,
/// PHP 8.3 still has the five constants 8.4 removed (and no
/// `MYSQLI_TYPE_VECTOR`), and only PHP 8.6 has `mysqli_quote_string` /
/// `MYSQLI_OPT_COMPRESS`.
#[test]
fn test_mysqli_surface_follows_php_version() {
    let probe = r#"<?php
$probe = MYSQLI_ASSOC;
echo defined('MYSQLI_IS_MARIADB') ? "M" : "-";
echo defined('MYSQLI_TYPE_INTERVAL') && defined('MYSQLI_CURSOR_TYPE_SCROLLABLE') && defined('MYSQLI_STMT_ATTR_PREFETCH_ROWS') && defined('MYSQLI_SET_CHARSET_DIR') && defined('MYSQLI_CURSOR_TYPE_FOR_UPDATE') ? "L" : "-";
echo defined('MYSQLI_TYPE_VECTOR') ? "V" : "-";
echo function_exists('mysqli_quote_string') && defined('MYSQLI_OPT_COMPRESS') ? "Q" : "-";
"#;
    let php80 = compile_and_run_with_php_version(probe, elephc::php_version::PhpVersion::Php80);
    let php83 = compile_and_run_with_php_version(probe, elephc::php_version::PhpVersion::Php83);
    let php85 = compile_and_run_with_php_version(probe, elephc::php_version::PhpVersion::Php85);
    let php86 = compile_and_run_with_php_version(probe, elephc::php_version::PhpVersion::Php86);
    assert_eq!(
        [php80.as_str(), php83.as_str(), php85.as_str(), php86.as_str()],
        ["-L--", "ML--", "M-V-", "M-VQ"]
    );
}

/// `mysqli_driver`'s properties are live views like php's property handlers:
/// `report_mode` reads and WRITES the same mode `mysqli_report()` sets, and the
/// client fields describe the linked client library.
#[test]
fn test_mysqli_driver_report_mode_is_live() {
    let out = compile_and_run(
        r#"<?php
$driver = new mysqli_driver();
mysqli_report(MYSQLI_REPORT_ERROR);
echo $driver->report_mode;
$driver->report_mode = MYSQLI_REPORT_OFF;
$db = mysqli_init();
// OFF now: a failing connect returns false instead of throwing.
echo "|", $db->real_connect("127.0.0.1", "u", "p", "d", 1) ? "connected" : "quiet-false";
$driver->report_mode = MYSQLI_REPORT_STRICT;
try {
    $db->real_connect("127.0.0.1", "u", "p", "d", 1);
    echo "|no-throw";
} catch (mysqli_sql_exception $e) {
    echo "|strict-throw";
}
echo "|", $driver->client_version === mysqli_get_client_version() ? "cv" : "cv?";
echo "|", $driver->client_info === mysqli_get_client_info() ? "ci" : "ci?";
echo "|", $driver->driver_version;
"#,
    );
    assert_eq!(out, "1|quiet-false|strict-throw|cv|ci|101009");
}

/// Every mysqlnd option id the surface honors or accepts returns `true` from
/// `options()` before connecting (MYSQLI_OPT_READ_TIMEOUT included), and an
/// unknown id returns `false`, like php.
#[test]
fn test_mysqli_options_accept_mysqlnd_ids() {
    let out = compile_and_run(
        r#"<?php
$db = mysqli_init();
$ids = [MYSQLI_OPT_CONNECT_TIMEOUT, MYSQLI_OPT_READ_TIMEOUT, MYSQLI_INIT_COMMAND, MYSQLI_SET_CHARSET_NAME,
    MYSQLI_OPT_LOCAL_INFILE, MYSQLI_OPT_LOAD_DATA_LOCAL_DIR, MYSQLI_OPT_SSL_VERIFY_SERVER_CERT,
    MYSQLI_OPT_INT_AND_FLOAT_NATIVE, MYSQLI_OPT_NET_CMD_BUFFER_SIZE, MYSQLI_OPT_NET_READ_BUFFER_SIZE];
$ok = 0;
foreach ($ids as $id) { if ($db->options($id, 1)) { $ok++; } }
echo $ok, "/", count($ids), "|", $db->options(9999, 1) ? "T" : "F";
"#,
    );
    assert_eq!(out, "10/10|F");
}

/// The raw protocol commands still guard the link like php: `refresh()` and
/// `dump_debug_info()` on an unconnected object raise the not-fully-initialized
/// `Error` before any COM_REFRESH / COM_DEBUG is sent, and `MYSQLI_REPORT_INDEX`
/// stays part of `MYSQLI_REPORT_ALL`.
#[test]
fn test_mysqli_raw_protocol_commands_need_a_connection() {
    let out = compile_and_run(
        r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = mysqli_init();
$probe = function(callable $fn): string {
    try { $fn(); return "no-err"; }
    catch (Error $e) {
        return $e->getMessage() === "mysqli object is not fully initialized" ? "err" : "err?";
    }
};
echo $probe(fn() => $db->refresh(MYSQLI_REFRESH_TABLES));
echo "|", $probe(fn() => $db->dump_debug_info());
echo "|", $probe(fn() => mysqli_refresh($db, MYSQLI_REFRESH_GRANT));
echo "|", (MYSQLI_REPORT_ALL & MYSQLI_REPORT_INDEX) === MYSQLI_REPORT_INDEX ? "all" : "?";
"#,
    );
    assert_eq!(out, "err|err|err|all");
}

/// php frees mysqli objects in C, so `mysqli`, `mysqli_stmt`, and `mysqli_result`
/// expose no `__destruct` (nor elephc's private helpers) to `method_exists()`,
/// `get_class_methods()`, or reflection — also through a user subclass — and the
/// method lists match php-src's.
#[test]
fn test_mysqli_implementation_methods_are_hidden() {
    let out = compile_and_run(
        r#"<?php
$mode = MYSQLI_ASSOC;
class AppDb extends mysqli {}
echo method_exists("mysqli", "__destruct") ? "D" : "-";
echo method_exists("mysqli_stmt", "__destruct") ? "D" : "-";
echo method_exists("mysqli_result", "__destruct") ? "D" : "-";
echo method_exists("AppDb", "__destruct") ? "D" : "-";
echo method_exists("mysqli", "__elephcReportIndex") ? "H" : "-";
$db = mysqli_init();
echo method_exists($db, "__destruct") ? "D" : "-";
foreach (["mysqli", "mysqli_stmt", "mysqli_result", "AppDb"] as $class) {
    echo in_array("__destruct", get_class_methods($class)) ? "D" : "-";
}
echo "|";
echo (new ReflectionClass("mysqli"))->hasMethod("__destruct") ? "D" : "-";
echo (new ReflectionClass("mysqli_stmt"))->hasMethod("__destruct") ? "D" : "-";
echo "|", count(get_class_methods("mysqli_stmt")), ",", count(get_class_methods("mysqli_result"));
echo ",", method_exists("AppDb", "query") ? "q" : "?";
"#,
    );
    assert_eq!(out, "----------|--|20,16,q");
}

/// The counters php declares `int|string` (`insert_id`, `affected_rows`, `num_rows`)
/// keep that type on the properties and through the procedural aliases.
#[test]
fn test_mysqli_counters_are_int_or_numeric_string() {
    let out = compile_and_run(
        r#"<?php
$mode = MYSQLI_ASSOC;
echo (new ReflectionProperty("mysqli", "insert_id"))->getType();
echo "|", (new ReflectionProperty("mysqli_stmt", "num_rows"))->getType();
echo "|", (new ReflectionFunction("mysqli_affected_rows"))->getReturnType();
"#,
    );
    assert_eq!(out, "string|int|string|int|string|int");
}
