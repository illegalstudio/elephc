//! Purpose:
//! The `mysqli_*` procedural aliases, as an elephc-PHP source fragment. Every
//! alias is an ordinary PHP function forwarding to the `mysqli` /
//! `mysqli_result` object, so `function_exists('mysqli_query')` is true once
//! the prelude is injected.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - Link/result parameters are declared `mixed` and validated with an inline
//!   `instanceof` guard that throws `TypeError` (PHP's own runtime behavior).
//!   This is deliberate: `mysqli_query()` returns `mysqli_result|bool`, and an
//!   `instanceof`-narrowed union local passed to a typed object PARAMETER is
//!   miscompiled by the current checker/lowering split (the box is passed, not
//!   the pointer) — while a narrowed value used as a method RECEIVER lowers
//!   correctly. The guard keeps the classic procedural pipeline
//!   (`mysqli_query` → `mysqli_num_rows`) working and failing loudly.
//! - elephc always requires the explicit link argument, including under
//!   `--php-version=8.0` (PHP 8.0's implicit last-link is a documented
//!   divergence; PHP 8.1+ requires the object anyway).
//! - `mysqli_connect_errno()` / `mysqli_connect_error()` take no link and read
//!   the process-wide last-connect statics on `mysqli`, exactly like PHP.
//! - `mysqli_report()` lives in `exception.rs` next to the flag store.

/// The procedural `mysqli_*` alias functions (fragment without a `<?php`
/// header).
pub(super) const SRC: &str = r##"
// -- mysqli procedural aliases --

// Validation helpers cannot RETURN the narrowed object (a narrowed value only
// lowers correctly as a method receiver, not as a typed value), so every alias
// guards inline: instanceof-narrow, throw TypeError otherwise, then forward on
// the narrowed receiver.

function mysqli_connect(
    ?string $hostname = null,
    ?string $username = null,
    ?string $password = null,
    ?string $database = null,
    ?int $port = null,
    ?string $socket = null
): mysqli|false {
    // Unlike the argument-less constructor, procedural mysqli_connect() always
    // attempts the connection (php-src behavior; null arguments take their
    // defaults inside real_connect).
    $_link = new mysqli();
    if ($_link->real_connect($hostname, $username, $password, $database, $port, $socket, 0)) {
        return $_link;
    }
    return false;
}

// php declares `mysqli|false`, but the object is always created; the narrower
// `mysqli` keeps `mysqli_init()` usable as a typed argument.
function mysqli_init(): mysqli {
    return new mysqli();
}

function mysqli_real_connect(
    mixed $mysql,
    ?string $hostname = null,
    ?string $username = null,
    ?string $password = null,
    ?string $database = null,
    ?int $port = null,
    ?string $socket = null,
    int $flags = 0
): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_real_connect(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->real_connect($hostname, $username, $password, $database, $port, $socket, $flags);
}

function mysqli_close(mixed $mysql): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_close(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->close();
}

function mysqli_ping(mixed $mysql): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_ping(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->ping();
}

function mysqli_select_db(mixed $mysql, string $database): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_select_db(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->select_db($database);
}

function mysqli_set_charset(mixed $mysql, string $charset): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_set_charset(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->set_charset($charset);
}

function mysqli_character_set_name(mixed $mysql): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_character_set_name(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->character_set_name();
}

function mysqli_real_escape_string(mixed $mysql, string $string): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_real_escape_string(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->real_escape_string($string);
}

function mysqli_escape_string(mixed $mysql, string $string): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_escape_string(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->real_escape_string($string);
}

// -- elephc PHP >= 8.6 mysqli quote_string begin --

function mysqli_quote_string(mixed $mysql, string $string): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_quote_string(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return "'" . $mysql->real_escape_string($string) . "'";
}

// -- elephc PHP >= 8.6 mysqli quote_string end --

function mysqli_begin_transaction(mixed $mysql, int $flags = 0, ?string $name = null): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_begin_transaction(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->begin_transaction($flags, $name);
}

function mysqli_commit(mixed $mysql, int $flags = 0, ?string $name = null): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_commit(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->commit($flags, $name);
}

function mysqli_rollback(mixed $mysql, int $flags = 0, ?string $name = null): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_rollback(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->rollback($flags, $name);
}

function mysqli_savepoint(mixed $mysql, string $name): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_savepoint(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->savepoint($name);
}

function mysqli_release_savepoint(mixed $mysql, string $name): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_release_savepoint(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->release_savepoint($name);
}

function mysqli_autocommit(mixed $mysql, bool $enable): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_autocommit(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->autocommit($enable);
}

function mysqli_options(mixed $mysql, int $option, mixed $value): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_options(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->options($option, $value);
}

function mysqli_set_opt(mixed $mysql, int $option, mixed $value): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_set_opt(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->options($option, $value);
}

function mysqli_ssl_set(mixed $mysql, ?string $key, ?string $certificate, ?string $ca_certificate, ?string $ca_path, ?string $cipher_algos): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_ssl_set(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->ssl_set($key, $certificate, $ca_certificate, $ca_path, $cipher_algos);
}

function mysqli_change_user(mixed $mysql, string $username, string $password, ?string $database): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_change_user(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->change_user($username, $password, $database);
}

function mysqli_kill(mixed $mysql, int $process_id): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_kill(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->kill($process_id);
}

function mysqli_refresh(mixed $mysql, int $flags): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_refresh(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->refresh($flags);
}

function mysqli_dump_debug_info(mixed $mysql): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_dump_debug_info(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->dump_debug_info();
}

// mysqlnd debug tracing is compiled out (MYSQLI_DEBUG_TRACE_ENABLED is 0).
function mysqli_debug(string $options): bool {
    $_ignoredOptions = $options;
    return true;
}

function mysqli_get_server_info(mixed $mysql): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_server_info(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->get_server_info();
}

// php: the client library version; the optional link argument is accepted
// (deprecated since 8.1) but never needed.
function mysqli_get_client_info(mixed $mysql = null): string {
    if ($mysql !== null && !($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_client_info(): Argument #1 (\$mysql) must be of type ?mysqli, " . gettype($mysql) . " given");
    }
    return elephc_pdo_mysql_client_info();
}

// php: major*10000 + minor*100 + patch of the client library, no link needed.
function mysqli_get_client_version(): int {
    $_version = elephc_pdo_mysql_client_info();
    if (str_starts_with($_version, "mysql ")) {
        $_version = substr($_version, 6);
    }
    $_parts = explode(".", $_version);
    $_major = (int) $_parts[0];
    $_minor = count($_parts) > 1 ? (int) $_parts[1] : 0;
    $_patch = count($_parts) > 2 ? (int) $_parts[2] : 0;
    return $_major * 10000 + $_minor * 100 + $_patch;
}

function mysqli_get_host_info(mixed $mysql): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_host_info(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->host_info;
}

function mysqli_get_proto_info(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_proto_info(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->protocol_version;
}

function mysqli_get_server_version(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_server_version(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->server_version;
}

function mysqli_get_connection_stats(mixed $mysql): array {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_connection_stats(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    // Rebuilt key by key so the alias returns the associative shape directly.
    $_out = [];
    foreach ($mysql->get_connection_stats() as $_key => $_value) {
        $_out[(string) $_key] = (string) $_value;
    }
    return $_out;
}

// Process-wide mysqlnd client statistics, every key php reports.
function mysqli_get_client_stats(): array {
    $_counters = mysqli::$clientStats;
    $_out = [];
    foreach (mysqli::__elephcStatKeys() as $_key) {
        $_name = (string) $_key;
        $_out[$_name] = mysqli::__elephcWireStat(-1, $_name) ?? (string) ($_counters[$_name] ?? 0);
    }
    return $_out;
}

// php: total open links plus the persistent-link pool counters.
function mysqli_get_links_stats(): array {
    return ["total" => mysqli::$openLinks, "active_plinks" => mysqli::$openPersistentLinks, "cached_plinks" => 0];
}

function mysqli_get_warnings(mixed $mysql): mysqli_warning|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_warnings(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->get_warnings();
}

function mysqli_stat(mixed $mysql): string|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_stat(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->stat();
}

function mysqli_thread_id(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_thread_id(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->thread_id;
}

function mysqli_connect_errno(): int {
    return mysqli::$lastConnectErrno;
}

function mysqli_connect_error(): ?string {
    if (mysqli::$lastConnectErrno == 0) {
        return null;
    }
    return mysqli::$lastConnectError;
}

function mysqli_errno(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_errno(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->errno;
}

function mysqli_error(mixed $mysql): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_error(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->error;
}

function mysqli_error_list(mixed $mysql): array {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_error_list(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->error_list;
}

function mysqli_sqlstate(mixed $mysql): string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_sqlstate(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->sqlstate;
}

function mysqli_affected_rows(mixed $mysql): int|string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_affected_rows(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->affected_rows;
}

function mysqli_insert_id(mixed $mysql): int|string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_insert_id(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->insert_id;
}

function mysqli_field_count(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_field_count(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->field_count;
}

function mysqli_warning_count(mixed $mysql): int {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_warning_count(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->warning_count;
}

function mysqli_info(mixed $mysql): ?string {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_info(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->info;
}

function mysqli_query(mixed $mysql, string $query, int $result_mode = 0): mysqli_result|bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_query(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->query($query, $result_mode);
}

function mysqli_real_query(mixed $mysql, string $query): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_real_query(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->real_query($query);
}

function mysqli_multi_query(mixed $mysql, string $query): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_multi_query(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->multi_query($query);
}

function mysqli_more_results(mixed $mysql): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_more_results(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->more_results();
}

function mysqli_next_result(mixed $mysql): bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_next_result(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->next_result();
}

function mysqli_store_result(mixed $mysql, int $mode = 0): mysqli_result|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_store_result(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->store_result($mode);
}

function mysqli_use_result(mixed $mysql): mysqli_result|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_use_result(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->use_result();
}

function mysqli_reap_async_query(mixed $mysql): mysqli_result|bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_reap_async_query(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->reap_async_query();
}

// Same contract as mysqli::poll() (the link arrays are rewritten in place).
function mysqli_poll(?array &$read, ?array &$error, array &$reject, int $seconds, int $microseconds = 0): int|false {
    return mysqli::poll($read, $error, $reject, $seconds, $microseconds);
}

function mysqli_fetch_assoc(mixed $result): ?array {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_assoc(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_assoc();
}

function mysqli_fetch_row(mixed $result): ?array {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_row(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_row();
}

function mysqli_fetch_array(mixed $result, int $mode = 3): ?array {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_array(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_array($mode);
}

function mysqli_fetch_object(mixed $result, string $class = "stdClass", array $constructor_args = []): mixed {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_object(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_object($class, $constructor_args);
}

function mysqli_fetch_all(mixed $result, int $mode = 2): array {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_all(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_all($mode);
}

// -- elephc PHP >= 8.1 mysqli fetch_column begin --

function mysqli_fetch_column(mixed $result, int $column = 0): mixed {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_column(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_column($column);
}

// -- elephc PHP >= 8.1 mysqli fetch_column end --

function mysqli_num_rows(mixed $result): int|string {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_num_rows(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    if ($result->type == 1) {
        throw new Error("mysqli_num_rows() cannot be used in MYSQLI_USE_RESULT mode");
    }
    return $result->num_rows;
}

function mysqli_num_fields(mixed $result): int {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_num_fields(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->field_count;
}

function mysqli_data_seek(mixed $result, int $offset): bool {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_data_seek(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->data_seek($offset);
}

function mysqli_fetch_field(mixed $result): mixed {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_field(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_field();
}

function mysqli_fetch_fields(mixed $result): array {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_fields(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_fields();
}

function mysqli_fetch_field_direct(mixed $result, int $index): mixed {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_field_direct(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->fetch_field_direct($index);
}

function mysqli_fetch_lengths(mixed $result): array|false {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_fetch_lengths(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    $_lengths = $result->lengths;
    if ($_lengths === null) {
        return false;
    }
    return $_lengths;
}

function mysqli_field_seek(mixed $result, int $index): bool {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_field_seek(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->field_seek($index);
}

function mysqli_field_tell(mixed $result): int {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_field_tell(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    return $result->current_field;
}

function mysqli_free_result(mixed $result): void {
    if (!($result instanceof mysqli_result)) {
        throw new TypeError("mysqli_free_result(): Argument #1 (\$result) must be of type mysqli_result, " . gettype($result) . " given");
    }
    $result->free();
}

function mysqli_prepare(mixed $mysql, string $query): mysqli_stmt|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_prepare(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->prepare($query);
}

// -- elephc PHP >= 8.2 mysqli execute_query begin --

function mysqli_execute_query(mixed $mysql, string $query, ?array $params = null): mysqli_result|bool {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_execute_query(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->execute_query($query, $params);
}

// -- elephc PHP >= 8.2 mysqli execute_query end --

function mysqli_stmt_init(mixed $mysql): mysqli_stmt|false {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_stmt_init(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->stmt_init();
}

function mysqli_stmt_bind_param(mixed $statement, string $types, mixed &...$vars): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_bind_param(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    // A by-ref variadic cannot be spread-forwarded into bind_param, so the
    // alias builds the same by-reference reader closure itself; the checker's
    // mysqli friend channel lets it reach the statement's private helper.
    return $statement->__elephcBindParamReader("mysqli_stmt_bind_param", 2, $types, count($vars), function () use (&$vars): array {
        $_values = [];
        $_n = count($vars);
        for ($_i = 0; $_i < $_n; $_i++) {
            $_values[] = $vars[$_i];
        }
        return $_values;
    });
}

function mysqli_stmt_bind_result(mixed $statement, mixed &...$vars): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_bind_result(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->__elephcBindResultWriter(count($vars), function (array $row) use (&$vars): void {
        $_n = count($vars);
        for ($_i = 0; $_i < $_n; $_i++) {
            $vars[$_i] = $row[$_i];
        }
    });
}

function mysqli_stmt_execute(mixed $statement, ?array $params = null): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_execute(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->execute($params);
}

// Deprecated php alias of mysqli_stmt_execute.
function mysqli_execute(mixed $statement, ?array $params = null): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_execute(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->execute($params);
}

function mysqli_stmt_prepare(mixed $statement, string $query): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_prepare(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->prepare($query);
}

function mysqli_stmt_fetch(mixed $statement): ?bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_fetch(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->fetch();
}

function mysqli_stmt_get_result(mixed $statement): mysqli_result|false {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_get_result(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->get_result();
}

function mysqli_stmt_store_result(mixed $statement): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_store_result(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->store_result();
}

function mysqli_stmt_result_metadata(mixed $statement): mysqli_result|false {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_result_metadata(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->result_metadata();
}

function mysqli_stmt_data_seek(mixed $statement, int $offset): void {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_data_seek(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    $statement->data_seek($offset);
}

function mysqli_stmt_more_results(mixed $statement): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_more_results(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->more_results();
}

function mysqli_stmt_next_result(mixed $statement): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_next_result(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->next_result();
}

function mysqli_stmt_send_long_data(mixed $statement, int $param_num, string $data): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_send_long_data(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->send_long_data($param_num, $data);
}

function mysqli_stmt_attr_get(mixed $statement, int $attribute): int {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_attr_get(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->attr_get($attribute);
}

function mysqli_stmt_attr_set(mixed $statement, int $attribute, int $value): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_attr_set(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->attr_set($attribute, $value);
}

function mysqli_stmt_get_warnings(mixed $statement): mysqli_warning|false {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_get_warnings(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->get_warnings();
}

function mysqli_stmt_reset(mixed $statement): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_reset(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->reset();
}

function mysqli_stmt_close(mixed $statement): bool {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_close(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->close();
}

function mysqli_stmt_free_result(mixed $statement): void {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_free_result(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    $statement->free_result();
}

function mysqli_stmt_affected_rows(mixed $statement): int|string {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_affected_rows(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->affected_rows;
}

function mysqli_stmt_errno(mixed $statement): int {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_errno(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->errno;
}

function mysqli_stmt_error(mixed $statement): string {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_error(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->error;
}

function mysqli_stmt_error_list(mixed $statement): array {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_error_list(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->error_list;
}

function mysqli_stmt_num_rows(mixed $statement): int|string {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_num_rows(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->num_rows();
}

function mysqli_stmt_param_count(mixed $statement): int {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_param_count(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->param_count;
}

function mysqli_stmt_field_count(mixed $statement): int {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_field_count(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->field_count;
}

function mysqli_stmt_insert_id(mixed $statement): int|string {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_insert_id(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->insert_id;
}

function mysqli_stmt_sqlstate(mixed $statement): string {
    if (!($statement instanceof mysqli_stmt)) {
        throw new TypeError("mysqli_stmt_sqlstate(): Argument #1 (\$statement) must be of type mysqli_stmt, " . gettype($statement) . " given");
    }
    return $statement->sqlstate;
}

function mysqli_get_charset(mixed $mysql): mixed {
    if (!($mysql instanceof mysqli)) {
        throw new TypeError("mysqli_get_charset(): Argument #1 (\$mysql) must be of type mysqli, " . gettype($mysql) . " given");
    }
    return $mysql->get_charset();
}

// The client library is not thread-safe in this build; php returns a bool.
function mysqli_thread_safe(): bool {
    return false;
}
"##;
