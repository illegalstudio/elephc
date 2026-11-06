---
title: "mysqli (MySQL / MariaDB)"
description: "The complete mysqli extension: connections (TLS included), queries and results, prepared statements with reference binding, multi_query, async queries, warnings, statistics, mysqli_report error handling, and the documented divergences from php-src."
sidebar:
  order: 18
---

elephc ships the **complete mysqli extension** for MySQL and MariaDB — every
function, class, method, and constant php-src's mysqli declares (mysqlnd
build) for the selected `--php-version` — implemented
as its own PHP surface over the same pure-Rust MySQL client that backs
[PDO](./pdo.md) (`crates/elephc-pdo`). A mysqli program links no system MySQL
client library, and it never declares the PDO classes: `mysqli`,
`mysqli_stmt`, `mysqli_result`, `mysqli_driver`, `mysqli_warning`, and
`mysqli_sql_exception` are their own types, and a mysqli failure never throws
`PDOException`.

The prelude is injected automatically when a program references a mysqli
class or calls a `mysqli_*` function. `--with-mysqli` force-injects the
surface (and links the bridge) for programs with no statically visible usage.
`extension_loaded('mysqli')` is true exactly when the surface is compiled in;
`mysqlnd` is never reported (elephc is not mysqlnd).

## Connecting

```php
<?php
mysqli_report(MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT);
$db = new mysqli("127.0.0.1", "user", "secret", "appdb", 3306);
echo $db->host_info, "\n";     // "127.0.0.1 via TCP/IP"
echo $db->server_info, "\n";   // e.g. "8.4.6"
$db->close();
```

- `new mysqli()` with no arguments behaves like `mysqli_init()`: no connection
  is attempted until `real_connect()`.
- A host beginning with `p:` selects a persistent connection; the remainder is
  the real host (`new mysqli("p:127.0.0.1", …)`).
- The socket argument is honored only when the host is empty or exactly
  `localhost`; any other host connects over TCP. The default port is `3306`.
- `real_connect()` honors `MYSQLI_CLIENT_FOUND_ROWS`, `MYSQLI_CLIENT_COMPRESS`,
  `MYSQLI_CLIENT_IGNORE_SPACE`, `MYSQLI_CLIENT_SSL`,
  `MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT`, and
  `MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT`.
- `mysqli_options()` / `mysqli_set_opt()` honor `MYSQLI_OPT_CONNECT_TIMEOUT`,
  `MYSQLI_INIT_COMMAND` (several commands run in order, like mysqlnd's list),
  `MYSQLI_SET_CHARSET_NAME`, `MYSQLI_OPT_LOCAL_INFILE`,
  `MYSQLI_OPT_LOAD_DATA_LOCAL_DIR`, `MYSQLI_OPT_SSL_VERIFY_SERVER_CERT`,
  `MYSQLI_SERVER_PUBLIC_KEY`, `MYSQLI_OPT_INT_AND_FLOAT_NATIVE`, and
  `MYSQLI_OPT_READ_TIMEOUT` (the socket read and write timeout, in seconds: a
  stalled server fails the statement with errno 2006 "MySQL server has gone
  away", like mysqlnd); the other
  mysqlnd option ids are accepted (`true`) and unknown ids return `false`.
- Calling `real_connect()` (or `connect()`) on an already-connected object
  closes the previous connection implicitly and starts over on a fresh mysqlnd
  handle, as php-src does: every option and `ssl_set()` value set before goes
  back to its default (set them again after reconnecting), and the connection
  statistics restart. A reconnect that fails leaves a link that is still
  "connected" but has no connection: its operations fail with errno 2014.
- The link's state follows php-src: a failed constructor (or
  `mysqli_connect()`) closes the object; a failed `real_connect()` on a
  `mysqli_init()` object leaves it unconnected (`… is not fully initialized`);
  after `close()`, every method — `real_connect()`, `options()`, `close()`
  itself, and the procedural accessors such as `mysqli_errno()` — throws
  `Error: mysqli object is already closed`, while `connect()` reopens the
  object on a new link.
- Connect-time failures populate `connect_errno` / `connect_error` (distinct
  from `errno` / `error`), and the no-argument procedural
  `mysqli_connect_errno()` / `mysqli_connect_error()` read the process-wide
  last connect attempt, exactly like PHP.

### TLS

`ssl_set()` (or `mysqli_ssl_set()`) collects the key, certificate, CA, CA
path, and cipher list for the next `real_connect()`, which then negotiates TLS
through the bridge's rustls backend; `MYSQLI_CLIENT_SSL` enables TLS without
files. Peer verification follows mysqlnd: an explicit
`MYSQLI_CLIENT_SSL_VERIFY_SERVER_CERT` / `MYSQLI_CLIENT_SSL_DONT_VERIFY_SERVER_CERT`
flag wins, then `MYSQLI_OPT_SSL_VERIFY_SERVER_CERT`, then the default — verify
when `ssl_set()` named any file or cipher, otherwise not.

```php
$db = mysqli_init();
$db->ssl_set(null, null, "/etc/ssl/certs/ca.pem", null, null);
$db->real_connect("db.example.com", "user", "secret", "appdb", 3306, null, MYSQLI_CLIENT_SSL);
```

Every procedural function php declares has a `mysqli_*` alias (`mysqli_connect`,
`mysqli_query`, `mysqli_prepare`, …), so `function_exists('mysqli_query')` is
true once the surface is compiled in. elephc always requires the explicit link
argument, including under `--php-version=8.0` (PHP 8.0's implicit
last-opened-link fallback is not implemented; PHP 8.1+ requires the object
anyway).

## Queries and results

`mysqli::query()` returns a **fully buffered** `mysqli_result` that owns its
rows: a later query on the same connection never invalidates an earlier
result.

```php
$r1 = $db->query("SELECT name FROM users ORDER BY id");
$r2 = $db->query("SELECT COUNT(*) AS c FROM users");   // $r1 stays valid
$r1->data_seek(0);
foreach ($r1 as $i => $row) {          // assoc rows, integer keys
    echo $i, ": ", $row["name"], "\n";
}
echo $r1->num_rows, " users\n";
```

The fetch family matches PHP: `fetch_assoc()`, `fetch_row()`,
`fetch_array(MYSQLI_ASSOC|MYSQLI_NUM|MYSQLI_BOTH)`, `fetch_object()`,
`fetch_all()`, `fetch_column()` (PHP 8.1+), `data_seek()`, `fetch_field()`,
`fetch_fields()`, `fetch_field_direct()`, `field_seek()`, the `$lengths`,
`$current_field`, and `$type` properties, and `free()` / `free_result()` /
`close()`. `fetch_*` return `null` when the cursor is exhausted
(`fetch_column()` returns `false`); out-of-range `data_seek()` /
`field_seek()` / `fetch_field_direct()` / `fetch_column()` arguments throw
php's `ValueError`s.

Text-protocol (`query()`) values are strings, like php; with
`MYSQLI_OPT_INT_AND_FLOAT_NATIVE` integer and float columns come back as PHP
`int` / `float`. Prepared-statement values keep their native types. Cells are
decoded the way mysqlnd decodes them: a `BIT` column reads as its decimal
string in the text protocol and as an `int` in the binary protocol (or with
`MYSQLI_OPT_INT_AND_FLOAT_NATIVE`); `ZEROFILL` integers (`YEAR` included) stay
zero-padded strings in both protocols, even with the native option; and
unsigned values past `PHP_INT_MAX` stay decimal strings.

Field metadata comes from the wire protocol: `name`, `table`, `type`
(`MYSQLI_TYPE_*`), `flags`, `length`, and `decimals` are real (`flags`
includes the `MYSQLI_NUM_FLAG` mysqlnd adds to numeric columns); `orgname` and
`orgtable` mirror `name` and `table`; `max_length` is `0` (as in mysqlnd); the
metadata the bridge does not expose (`def`, `db`, `charsetnr`) reads `0` / `""`.

`query($sql, MYSQLI_USE_RESULT)` and `use_result()` return an **unbuffered**
result: rows stay on the server and every fetch reads the next one. Like
mysqlnd, the link answers every other command (`query()`, `prepare()`, …) with
errno 2014 "Commands out of sync" until the set is exhausted or the result is
freed (or destroyed), `num_rows` reads `0` until the last row was fetched
(`affected_rows` is `-1` meanwhile), and `data_seek()` / `mysqli_num_rows()`
throw. A row read that fails midway (a read timeout, a lost connection) is
reported like mysqli's fetch path (errno on the link, `mysqli_sql_exception`
under `MYSQLI_REPORT_STRICT`, a warning under `MYSQLI_REPORT_ERROR`) instead of
ending the set silently. Difference from PHP: with reporting off that fetch
returns `null` where PHP returns `false`; both end a `while ($row = ...)` loop.

Non-select statements return `true` and refresh `affected_rows`,
`insert_id`, `warning_count`, and `info` (the server's "Records: … Duplicates:
… Warnings: …" string). `real_query()` runs the statement and leaves the result
on the server for `store_result()` (buffer it) / `use_result()` (stream it) /
`new mysqli_result($db, $mode)`.

## Prepared statements

`mysqli::prepare()` (or `new mysqli_stmt($db, $sql)`) uses real server-side
(non-emulated) `?` placeholders.

```php
$ins = $db->prepare("INSERT INTO users (name, score) VALUES (?, ?)");
$name = "";
$score = 0.0;
$ins->bind_param("sd", $name, $score);
foreach ([["Ada", 1.5], ["Ben", 2.0]] as [$name, $score]) {
    $ins->execute();               // reads $name / $score NOW
}

$sel = $db->prepare("SELECT id, name FROM users WHERE score > ?");
$min = 1.0;
$sel->bind_param("d", $min);
$sel->bind_result($id, $userName);
$sel->execute();
while ($sel->fetch()) {            // writes the row into $id / $userName
    echo $id, " ", $userName, "\n";
}
```

- `bind_param($types, ...$vars)` and `bind_result(...$vars)` bind **by
  reference**, like php: every `execute()` reads the bound variables' current
  values and every `fetch()` writes the row back into the result variables.
  The `i` / `d` / `s` / `b` type characters are validated, and the counts throw
  php's `ArgumentCountError`s.
- A `b` parameter is sent through `send_long_data()` chunks: each one goes to the
  server at once (`COM_STMT_SEND_LONG_DATA`) and the next `execute()` leaves that
  value out of `COM_STMT_EXECUTE`; without chunks it sends an empty string (or
  NULL for a NULL variable), like mysqlnd.
- `execute(?array $params = null)` optionally binds an array per execution
  (every element as a string, `null` as SQL NULL); a non-list array or a wrong
  element count throws php's `ValueError`.
- `fetch()` returns `true` per row, `null` when exhausted, `false` on error.
- Like mysqlnd, `execute()` reads only the result set's metadata: the rows stay
  on the server and `fetch()` reads them one at a time. Until they are fetched
  to the end, stored, freed (`free_result()` / `reset()` / `close()`), or the
  statement is re-executed, any other command on the link (a query, another
  statement) fails with errno 2014 "Commands out of sync" (unless the
  statement reads them through a cursor, see below).
- `store_result()` buffers the rows client-side so `num_rows` / `num_rows()`
  are exact and `data_seek()` repositions `fetch()`.
- `get_result()` drains the result set into an independent `mysqli_result`
  and leaves the statement re-executable.
- `result_metadata()` returns a row-less `mysqli_result` describing the columns
  (available right after `prepare()`, like `field_count` and `param_count`).
- `more_results()` / `next_result()` walk the result sets of a `CALL`.
- `reset()` sends `COM_STMT_RESET` on a ready link (while another result owns the
  link it resets locally, like mysqlnd) and returns `false` without `mysqli_report()`
  when the connection is lost.
- `attr_get()` / `attr_set()` handle `MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH` and
  `MYSQLI_STMT_ATTR_CURSOR_TYPE` (plus `MYSQLI_STMT_ATTR_PREFETCH_ROWS` before
  PHP 8.4), with php's `ValueError`s and mysqlnd's "Not implemented" for the
  cursor types it does not support.
- With `MYSQLI_CURSOR_TYPE_READ_ONLY`, `execute()` asks the server for a
  read-only cursor, like mysqlnd: the rows stay in the cursor and each
  `fetch()` reads one with `COM_STMT_FETCH`, so the link stays free for other
  queries and statements in between. `get_result()` / `store_result()` read
  every remaining row with one fetch, and `free_result()`, `reset()`,
  `close()` or a re-execution drain the rest. A statement without a result
  set runs as usual. While another result owns the link (an unbuffered
  query, an async query, another statement's unread set), a cursor's
  `fetch()`, `get_result()` or `store_result()` fails with 2014 on the
  statement and the link, and the cursor stays readable once that result is
  consumed. Difference from mysqlnd: when that happens on the cursor's very
  first `fetch()`, mysqlnd returns `NULL` without an error and abandons the
  cursor; elephc answers 2014 there too.
- `mysqli::execute_query($sql, $params)` (PHP 8.2+) is prepare + execute +
  get_result in one call.
- Statement properties: `affected_rows`, `errno`, `error`, `error_list`,
  `field_count`, `id`, `insert_id`, `num_rows`, `param_count`, `sqlstate`.
  Methods on an unprepared statement raise `Error: mysqli_stmt object is not
  fully initialized`, and on a closed one `… is already closed`.

## multi_query

`multi_query()` sends the whole batch in one server round-trip; the retained
result sets are walked with `store_result()` / `more_results()` /
`next_result()`. Like a mysqlnd link, a mysqli session has multi-statements
off: `multi_query()` turns them on with `COM_SET_OPTION` and the next
`query()` / `real_query()` turns them off again, so a multi-statement string
through `query()` gets the server's own 1064 syntax error (compound-body DDL
such as `CREATE PROCEDURE … BEGIN …; … END` is one statement and runs):

```php
$db->multi_query("SELECT 1 AS a; INSERT INTO t (v) VALUES (5); SELECT 2 AS b");
$first = $db->store_result();          // buffered result for SELECT 1
while ($db->more_results()) {
    $db->next_result();
    $set = $db->store_result();        // false for the INSERT's OK packet
    if ($set === false) {
        echo $db->affected_rows, " rows\n";
    }
}
```

A `store_result()` result is independent and stays valid while the batch
advances; with `use_result()` each set streams and must be consumed (or freed)
before `next_result()`, which — like mysqlnd — returns `false` while the current
set is unread.

A stored `query()` of a `CALL` buffers only its first set, like mysqlnd's
`store_result()`: the later sets stay on the server until `next_result()` reads
them, and the link answers other commands with 2014 meanwhile.

## Async queries

`query($sql, MYSQLI_ASYNC)` returns `true` once the query is sent;
`mysqli_poll()` / `mysqli::poll()` reports which links have a query to reap,
and `reap_async_query()` returns its result (or `true` for a non-select):

```php
$db->query("SELECT SLEEP(1), 42", MYSQLI_ASYNC);
$read = [$db];
$error = [$db];
$reject = [];
if (mysqli_poll($read, $error, $reject, 5) > 0) {
    $row = $db->reap_async_query()->fetch_row();
}
```

The query runs on a bridge worker thread, so `query()` returns at once and
several links can work concurrently. `poll()` waits up to `$seconds` +
`$microseconds` for one of the `$read` links to become readable (links with no
query in flight land in `$reject`; links passed only in `$error` keep it waiting
for its timeout, and `$error` comes back empty), and `reap_async_query()`
collects the result (blocking if the server has not answered yet). Until it is
reaped the link answers other commands with errno 2014, like mysqlnd, and a
failing async query is reported by `reap_async_query()`, exactly where mysqlnd
surfaces it. The worker reads only the first result set's metadata, so `poll()`
reports the link as soon as the server starts answering. A buffered result reads
the rows of that set when it is reaped; the later sets of a `CALL` are read by
`next_result()`, like mysqlnd. With `MYSQLI_ASYNC | MYSQLI_USE_RESULT` the reaped
result is unbuffered: each fetch reads the next row from the server, and the link
stays busy until it is read or freed.

## Transactions and savepoints

```php
$db->begin_transaction(MYSQLI_TRANS_START_READ_WRITE, "job42");   // START TRANSACTION READ WRITE /*job42*/
$db->query("UPDATE accounts SET balance = balance - 10 WHERE id = 1");
$db->savepoint("half");                                           // SAVEPOINT `half`
$db->query("UPDATE accounts SET balance = balance + 10 WHERE id = 2");
$db->commit(MYSQLI_TRANS_COR_AND_CHAIN, "job42");                 // COMMIT AND CHAIN /*job42*/
```

The `$name` argument of `begin_transaction()` / `commit()` / `rollback()` is a
SQL **comment** (`/*name*/`), exactly as php-src emits it — it is *not* a
savepoint. Like php, the name is **stripped to `[A-Za-z0-9 -_=]`** before it is
wrapped: this is a security measure, not cosmetics — an unfiltered name
beginning with `!` (or MariaDB's `M!`) would open an executable `/*! … */`
comment and a `;` inside it would run a second statement. An empty name is a
`ValueError` for `begin_transaction()` only (php throws there but not for
`commit()`/`rollback()`, which send `COMMIT /**/` / `ROLLBACK /**/`). php raises
an `E_WARNING` when it truncates; elephc strips silently (no `E_WARNING` channel
— documented divergence, identical security behavior).
Savepoints are the separate `savepoint()` / `release_savepoint()` methods (and
`mysqli_savepoint()` / `mysqli_release_savepoint()`), which emit `SAVEPOINT` /
`RELEASE SAVEPOINT` with backtick quoting. The `$flags` are composed into the
SQL: `MYSQLI_TRANS_START_*` on `begin_transaction()` (`READ ONLY`, `READ WRITE`,
`WITH CONSISTENT SNAPSHOT`) and `MYSQLI_TRANS_COR_*` on `commit()` / `rollback()`
(`AND [NO] CHAIN`, `[NO] RELEASE`).

While result sets remain unconsumed (including a `real_query()` result not yet
picked up by `store_result()`), issuing a new statement — `query()`,
`prepare()`, `multi_query()`, `ping()`, `select_db()`, `set_charset()`,
`stat()`, `change_user()`, `kill()`, `refresh()`, or a transaction control —
fails with errno 2014, php-src's "Commands out of sync; you can't run this
command now". The two-step statement form honors the same guard:
`mysqli_stmt::prepare()` and `mysqli_stmt::execute()` on a busy link fail with
2014 recorded on the statement. Closing or reconnecting discards any
unconsumed result with the connection, so the next link starts clean.

## Connection maintenance

- `ping()` sends a real `COM_PING`, `select_db()` a real `COM_INIT_DB`, and
  `change_user($user, $password, $database)` a real `COM_CHANGE_USER` (the
  session is reset: an open transaction is rolled back, temporary tables and
  session variables are dropped).
- `kill($process_id)` sends a real `COM_PROCESS_KILL` (`ValueError` for a
  non-positive id). Killing the connection's own `thread_id` returns `true`
  without waiting for the reply, and the link is gone from then on: every
  command fails with 2006 "MySQL server has gone away", like mysqlnd.
- `refresh($flags)` sends a real `COM_REFRESH` with the `MYSQLI_REFRESH_*`
  bitmask, and `dump_debug_info()` a real `COM_DEBUG` (the server writes its
  debug information to its error log). Neither goes through `mysqli_report`: a
  refused `refresh()` returns `false` with `errno` / `error` set, while
  `dump_debug_info()` returns `true` whatever the server answers (a refusal is
  only recorded in `errno` / `error`), as mysqlnd does.
- MySQL 8.4 removed `COM_PROCESS_KILL` and `COM_REFRESH`; as with php, `kill()`
  and `refresh()` then fail with the server's 1047 "Unknown command" (use a
  `KILL` / `FLUSH` statement instead).
- After these protocol commands (`ping()`, `select_db()`, `kill()`,
  `refresh()`, `dump_debug_info()`, `stat()`) `affected_rows` reads `-1` and an
  OK reply clears `info`, following mysqlnd's upsert state.
- `stat()` returns MySQL's "Uptime: … Questions: …" status line.
- `get_warnings()` (and `mysqli_stmt::get_warnings()`) return a
  `mysqli_warning` chain built from `SHOW WARNINGS` — `$message`, `$sqlstate`,
  `$errno`, and `next()` — or `false` when the last statement left none.
  `mysqli_warning` cannot be constructed directly.
- `get_connection_stats()`, `mysqli_get_client_stats()`, and
  `mysqli_get_links_stats()` return mysqlnd's statistics arrays with every key
  php reports (values as numeric strings). The wire statistics follow mysqlnd's
  own accounting and match PHP 8.3 mysqlnd on the same server: `bytes_sent` /
  `packets_sent` / `protocol_overhead_out` (each command counts its physical
  packets plus one, as mysqlnd does), `packets_sent_command`, `bytes_received`
  (the bytes consumed — each packet's header and payload, or under
  compression each compressed frame — after TLS decryption),
  `packets_received`, `protocol_overhead_in`, and the per-packet-type
  `bytes_received_*_packet` / `packets_received_*` pairs, charged through
  php-src's own (shifted) reader tables: result headers show up under
  `rset_row`, field definitions under `prepare_response`, rows under
  `change_user`, EOFs under `rset_header`. A mysqli session negotiates the same
  protocol features mysqlnd does (no MariaDB extended capabilities, no client
  statement cache: every `prepare()` reaches the server and `close()` sends
  `COM_STMT_CLOSE`), and reads results when the program consumes them, like
  mysqlnd: `multi_query()` / `real_query()` / `use_result()` / a statement's
  `execute()` read a set's header and metadata, its rows are read by
  `store_result()` / `get_result()` or one per fetch, and `next_result()`
  reads the next set — so the counters move at the same calls as php's. The
  `mem_*` counters are `"0"`, as in php with its default
  `mysqlnd.collect_memory_statistics=Off`. The decode-time and lifecycle
  counters follow mysqlnd too: `proto_text_fetched_*` /
  `proto_binary_fetched_*` (charged per decoded field when a row is handed to
  the program, through php-src's shifted type-to-name tables) and
  `bytes_received_real_data_*`, buffered / unbuffered / flushed set counts,
  `rows_fetched_from_*` / `rows_buffered_from_client_*` / `rows_skipped_*`,
  `explicit_free_result` / `implicit_free_result`, statement
  `explicit_stmt_close` / `implicit_stmt_close`, `stmt_close_*_executed`,
  `result_set_queries` / `non_result_set_queries` with the index flags, and the
  connection-level counters (connections, closes, `COM_*` commands). The
  counters mysqlnd records only process-wide (buffered client-side row fetches,
  statement closes) move only in `mysqli_get_client_stats()`, as in php. The
  connection counters keep mysqlnd's own accounting quirks too: a
  `mysqli_init()` handle counts as persistent (`pconnect_success`,
  `active_persistent_connections`), closing a connection — `close()` or the
  object's destruction, both `explicit_close` — drops `active_connections`
  twice, a pooled `p:` link enters and leaves the pool without touching them,
  and a counter driven below zero wraps like mysqlnd's `uint64_t`
  (`"18446744073709551615"`). `mysqli_get_links_stats()` reports
  `cached_plinks` for links parked in the pool.
- `debug()` returns `true`: mysqlnd's client-side debug tracing is compiled out
  of release builds (`MYSQLI_DEBUG_TRACE_ENABLED` is `0`), where php does the same.

## mysqli_driver

`new mysqli_driver()` exposes the process-wide client state through live
properties: `report_mode` reads **and writes** the same mode `mysqli_report()`
sets, `client_info` / `client_version` describe the linked client library, and
`driver_version` is php's `MYSQLI_VERSION_ID` (`101009`).

## Errors, mysqli_report, and mysqli_sql_exception

`mysqli_report()` mirrors PHP 8.1+: the default is
`MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT` (under `--php-version=8.0` the
default is `MYSQLI_REPORT_OFF`).

| Mode | Failure behavior |
|---|---|
| `MYSQLI_REPORT_STRICT` set | throw `mysqli_sql_exception` (extends `RuntimeException`) |
| `MYSQLI_REPORT_ERROR` only | raise an `E_WARNING`, return `false` |
| `MYSQLI_REPORT_OFF` | silent `false` |

A connect failure is the exception: like php-src it throws under
`MYSQLI_REPORT_STRICT` and is otherwise always an `E_WARNING`
(`mysqli::real_connect(): (HY000/1045): Access denied …`), even with
`MYSQLI_REPORT_OFF`.

`MYSQLI_REPORT_INDEX` (part of `MYSQLI_REPORT_ALL`) reads the server status
flags of each statement's final OK/EOF packet: a query or prepared statement
the server ran without a (good) index raises php's "No index used in
query/prepared statement …" / "Bad index used …" — a `mysqli_sql_exception`
(code 0, SQLSTATE `00000`) under `STRICT`, an `E_WARNING` otherwise.

Those warnings are real `E_WARNING`s with php's exact text — the entry point
the program called, then the detail: `mysqli::query(): (42S02/1146): Table
'db.t' doesn't exist`, `mysqli_connect(): (HY000/1045): Access denied …`,
`mysqli::query(): No index used in query/prepared statement …`. They reach
`set_error_handler()`, honour `error_reporting()` and `@`, and are reported at
the user's call-site line.

After a failure, `errno`, `error`, `sqlstate`, and `error_list` are populated
on the connection (and on the statement). `mysqli_sql_exception` is `final`,
keeps the SQLSTATE in a protected `$sqlstate` property, and exposes it through
`getSqlState()`, exactly like php-src.

## Escaping

`real_escape_string()` / `escape_string()` return the escaped payload
**without** wrapping quotes (PHP 8.6's `quote_string()` / `mysqli_quote_string()`
add them). Escaping is **charset-aware**, using the
connection's own live charset — tracked in the bridge from the handshake and
from every statement that changes the client charset: `SET NAMES`,
`SET CHARACTER SET`, and even a raw `SET character_set_client = …` (bare, `@@`,
or `SESSION` spellings). This goes **beyond php**, which only follows
`mysqli_set_charset()` and stays on `utf8mb4` after a raw SQL charset change (so
php's own `real_escape_string()` is unsafe there); elephc tracks it, so the
escape is never fooled into treating a multibyte session as `utf8mb4`.
`character_set_connection` / `character_set_results` and `GLOBAL` scopes are not
tracked, because they do not change how the server lexes the string literals
being escaped. For an ASCII-incompatible multibyte charset (`gbk`, `big5`,
`sjis`, `cp932`, `euckr`, `ujis`, `gb2312`, `gb18030`) the escape uses that
charset's real lead/trail byte ranges: a byte that completes a valid character
is copied opaquely, and a lead byte that does not is itself escaped — closing the
classic trailing-byte breakout for the whole family, not just GBK. Verified
byte-for-byte against php over a 221-input × 9-charset differential: identical
for gbk/big5/sjis/cp932/gb2312/ujis, and **`euckr` follows MySQL's own
`ctype-euc_kr` table** (server-faithful: the server is what parses the escaped
result) rather than mysqlnd's slightly wider table, so a handful of `0x8E`/`0x8F`
`euckr` sequences escape differently from php — always in the safe direction,
never leaving a quote reachable. Under `NO_BACKSLASH_ESCAPES` only `'` is doubled
(backslash is a literal there; this mirrors mysqlnd).

## Version profiles

The surface follows `--php-version` exactly as php-src's stubs do:
`fetch_column` (method and alias), `MYSQLI_IS_MARIADB`,
`MYSQLI_OPT_LOAD_DATA_LOCAL_DIR`, and `MYSQLI_REFRESH_REPLICA` exist from 8.1;
`execute_query` from 8.2; PHP 8.4 removes `MYSQLI_CURSOR_TYPE_FOR_UPDATE`,
`MYSQLI_CURSOR_TYPE_SCROLLABLE`, `MYSQLI_SET_CHARSET_DIR`,
`MYSQLI_STMT_ATTR_PREFETCH_ROWS`, and `MYSQLI_TYPE_INTERVAL` (and the matching
statement attributes) and adds `MYSQLI_TYPE_VECTOR`; `quote_string` (method and
alias) and `MYSQLI_OPT_COMPRESS` exist from 8.6. Functions php deprecates
(`mysqli_ping`, `mysqli_kill`, `mysqli_refresh` in 8.4, `mysqli_execute` in
8.5, `mysqli_get_charset` / `mysqli_stmt_init` in 8.6) are flagged with php's
deprecation message in the function reference; like the rest of the surface
they keep working (elephc raises no runtime `E_DEPRECATED`).

## Divergences from php-src

- **No `mysqlnd`**: `extension_loaded('mysqlnd')` is `false`, and
  `mysqli_get_client_info()` reports the bridge's own client string, not a
  mysqlnd version.
- **No implicit PHP 8.0 last-link**: procedural functions always require the
  explicit `mysqli` argument.
- **Procedural aliases validate the link/result argument at runtime** with a
  `TypeError` naming the expected class (the message spells the value's type
  with `gettype()`), and their `ValueError` messages name the method form
  (`mysqli_result::data_seek(): Argument #1`) rather than the procedural one.
- **Property writes stick**: the public `mysqli` / `mysqli_result` /
  `mysqli_stmt` properties are refreshed after operations but are not
  write-barriered; assigning to them is not rejected. Likewise, reading them
  on a closed or never-connected `mysqli` returns their last value where php
  throws (`… is already closed`, `Property access is not allowed yet`); the
  methods and procedural accessors do throw. (On a link whose reconnect
  failed, php itself crashes reading `host_info` / `server_info` or calling
  `character_set_name()`; elephc returns the previous connection's values.)
- **`insert_id` / `affected_rows` / `num_rows` are `int|string`**, as php
  declares them. elephc type-checks calls statically, so passing one straight
  to an `int` parameter needs an `(int)` cast where php would coerce at run
  time.
- **Capability probes do not inject the surface**: like PDO,
  `class_exists('mysqli')` in a program with no other mysqli reference
  reports `false`; build with `--with-mysqli` to force the surface. A bare
  `MYSQLI_*` constant reference *does* inject it. Code inside `eval()` uses
  the surface the program linked — classes, methods, `mysqli_*` functions,
  and `extension_loaded('mysqli')` all behave as in compiled code — but an
  `eval()` string is not scanned, so a program that uses mysqli *only* there
  needs `--with-mysqli`.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`mysqli_affected_rows()`](./builtins/database/mysqli_affected_rows.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_autocommit()`](./builtins/database/mysqli_autocommit.md) | `(mixed $mysql, bool $enable): bool` | `bool` | ✓ | - |
| [`mysqli_begin_transaction()`](./builtins/database/mysqli_begin_transaction.md) | `(mixed $mysql, int $flags = 0, ?string $name = null): bool` | `bool` | ✓ | - |
| [`mysqli_change_user()`](./builtins/database/mysqli_change_user.md) | `(mixed $mysql, string $username, string $password, ?string $database): bool` | `bool` | ✓ | - |
| [`mysqli_character_set_name()`](./builtins/database/mysqli_character_set_name.md) | `(mixed $mysql): string` | `string` | ✓ | - |
| [`mysqli_close()`](./builtins/database/mysqli_close.md) | `(mixed $mysql): bool` | `bool` | ✓ | - |
| [`mysqli_commit()`](./builtins/database/mysqli_commit.md) | `(mixed $mysql, int $flags = 0, ?string $name = null): bool` | `bool` | ✓ | - |
| [`mysqli_connect()`](./builtins/database/mysqli_connect.md) | `(?string $hostname = null, ?string $username = null, ?string $password = null, ?string $database = null, ?int $port = null, ?string $socket = null): mixed` | `mixed` | ✓ | - |
| [`mysqli_connect_errno()`](./builtins/database/mysqli_connect_errno.md) | `(): int` | `int` | ✓ | - |
| [`mysqli_connect_error()`](./builtins/database/mysqli_connect_error.md) | `(): ?string` | `?string` | ✓ | - |
| [`mysqli_data_seek()`](./builtins/database/mysqli_data_seek.md) | `(mixed $result, int $offset): bool` | `bool` | ✓ | - |
| [`mysqli_debug()`](./builtins/database/mysqli_debug.md) | `(string $options): bool` | `bool` | ✓ | - |
| [`mysqli_dump_debug_info()`](./builtins/database/mysqli_dump_debug_info.md) | `(mixed $mysql): bool` | `bool` | ✓ | - |
| [`mysqli_errno()`](./builtins/database/mysqli_errno.md) | `(mixed $mysql): int` | `int` | ✓ | - |
| [`mysqli_error()`](./builtins/database/mysqli_error.md) | `(mixed $mysql): string` | `string` | ✓ | - |
| [`mysqli_error_list()`](./builtins/database/mysqli_error_list.md) | `(mixed $mysql): array` | `array` | ✓ | - |
| [`mysqli_escape_string()`](./builtins/database/mysqli_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | - |
| [`mysqli_execute()`](./builtins/database/mysqli_execute.md) | `(mixed $statement, ?array $params = null): bool` | `bool` | ✓ | - |
| [`mysqli_execute_query()`](./builtins/database/mysqli_execute_query.md) | `(mixed $mysql, string $query, ?array $params = null): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_all()`](./builtins/database/mysqli_fetch_all.md) | `(mixed $result, int $mode = 2): array` | `array` | ✓ | - |
| [`mysqli_fetch_array()`](./builtins/database/mysqli_fetch_array.md) | `(mixed $result, int $mode = 3): ?array` | `?array` | ✓ | - |
| [`mysqli_fetch_assoc()`](./builtins/database/mysqli_fetch_assoc.md) | `(mixed $result): ?array` | `?array` | ✓ | - |
| [`mysqli_fetch_column()`](./builtins/database/mysqli_fetch_column.md) | `(mixed $result, int $column = 0): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_field()`](./builtins/database/mysqli_fetch_field.md) | `(mixed $result): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_field_direct()`](./builtins/database/mysqli_fetch_field_direct.md) | `(mixed $result, int $index): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_fields()`](./builtins/database/mysqli_fetch_fields.md) | `(mixed $result): array` | `array` | ✓ | - |
| [`mysqli_fetch_lengths()`](./builtins/database/mysqli_fetch_lengths.md) | `(mixed $result): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_object()`](./builtins/database/mysqli_fetch_object.md) | `(mixed $result, string $class = 'stdClass', array $constructor_args = []): mixed` | `mixed` | ✓ | - |
| [`mysqli_fetch_row()`](./builtins/database/mysqli_fetch_row.md) | `(mixed $result): ?array` | `?array` | ✓ | - |
| [`mysqli_field_count()`](./builtins/database/mysqli_field_count.md) | `(mixed $mysql): int` | `int` | ✓ | - |
| [`mysqli_field_seek()`](./builtins/database/mysqli_field_seek.md) | `(mixed $result, int $index): bool` | `bool` | ✓ | - |
| [`mysqli_field_tell()`](./builtins/database/mysqli_field_tell.md) | `(mixed $result): int` | `int` | ✓ | - |
| [`mysqli_free_result()`](./builtins/database/mysqli_free_result.md) | `(mixed $result): void` | `void` | ✓ | - |
| [`mysqli_get_charset()`](./builtins/database/mysqli_get_charset.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_get_client_info()`](./builtins/database/mysqli_get_client_info.md) | `(mixed $mysql = null): string` | `string` | ✓ | - |
| [`mysqli_get_client_stats()`](./builtins/database/mysqli_get_client_stats.md) | `(): array` | `array` | ✓ | - |
| [`mysqli_get_client_version()`](./builtins/database/mysqli_get_client_version.md) | `(): int` | `int` | ✓ | - |
| [`mysqli_get_connection_stats()`](./builtins/database/mysqli_get_connection_stats.md) | `(mixed $mysql): array` | `array` | ✓ | - |
| [`mysqli_get_host_info()`](./builtins/database/mysqli_get_host_info.md) | `(mixed $mysql): string` | `string` | ✓ | - |
| [`mysqli_get_links_stats()`](./builtins/database/mysqli_get_links_stats.md) | `(): array` | `array` | ✓ | - |
| [`mysqli_get_proto_info()`](./builtins/database/mysqli_get_proto_info.md) | `(mixed $mysql): int` | `int` | ✓ | - |
| [`mysqli_get_server_info()`](./builtins/database/mysqli_get_server_info.md) | `(mixed $mysql): string` | `string` | ✓ | - |
| [`mysqli_get_server_version()`](./builtins/database/mysqli_get_server_version.md) | `(mixed $mysql): int` | `int` | ✓ | - |
| [`mysqli_get_warnings()`](./builtins/database/mysqli_get_warnings.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_info()`](./builtins/database/mysqli_info.md) | `(mixed $mysql): ?string` | `?string` | ✓ | - |
| [`mysqli_init()`](./builtins/database/mysqli_init.md) | `(): mixed` | `mixed` | ✓ | - |
| [`mysqli_insert_id()`](./builtins/database/mysqli_insert_id.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_kill()`](./builtins/database/mysqli_kill.md) | `(mixed $mysql, int $process_id): bool` | `bool` | ✓ | - |
| [`mysqli_more_results()`](./builtins/database/mysqli_more_results.md) | `(mixed $mysql): bool` | `bool` | ✓ | - |
| [`mysqli_multi_query()`](./builtins/database/mysqli_multi_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | - |
| [`mysqli_next_result()`](./builtins/database/mysqli_next_result.md) | `(mixed $mysql): bool` | `bool` | ✓ | - |
| [`mysqli_num_fields()`](./builtins/database/mysqli_num_fields.md) | `(mixed $result): int` | `int` | ✓ | - |
| [`mysqli_num_rows()`](./builtins/database/mysqli_num_rows.md) | `(mixed $result): mixed` | `mixed` | ✓ | - |
| [`mysqli_options()`](./builtins/database/mysqli_options.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | - |
| [`mysqli_ping()`](./builtins/database/mysqli_ping.md) | `(mixed $mysql): bool` | `bool` | ✓ | - |
| [`mysqli_poll()`](./builtins/database/mysqli_poll.md) | `(?array $read, ?array $error, array $reject, int $seconds, int $microseconds = 0): mixed` | `mixed` | ✓ | - |
| [`mysqli_prepare()`](./builtins/database/mysqli_prepare.md) | `(mixed $mysql, string $query): mixed` | `mixed` | ✓ | - |
| [`mysqli_query()`](./builtins/database/mysqli_query.md) | `(mixed $mysql, string $query, int $result_mode = 0): mixed` | `mixed` | ✓ | - |
| [`mysqli_quote_string()`](./builtins/database/mysqli_quote_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | - |
| [`mysqli_real_connect()`](./builtins/database/mysqli_real_connect.md) | `(mixed $mysql, ?string $hostname = null, ?string $username = null, ?string $password = null, ?string $database = null, ?int $port = null, ?string $socket = null, int $flags = 0): bool` | `bool` | ✓ | - |
| [`mysqli_real_escape_string()`](./builtins/database/mysqli_real_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | - |
| [`mysqli_real_query()`](./builtins/database/mysqli_real_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | - |
| [`mysqli_reap_async_query()`](./builtins/database/mysqli_reap_async_query.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_refresh()`](./builtins/database/mysqli_refresh.md) | `(mixed $mysql, int $flags): bool` | `bool` | ✓ | - |
| [`mysqli_release_savepoint()`](./builtins/database/mysqli_release_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | - |
| [`mysqli_report()`](./builtins/database/mysqli_report.md) | `(int $flags): bool` | `bool` | ✓ | - |
| [`mysqli_rollback()`](./builtins/database/mysqli_rollback.md) | `(mixed $mysql, int $flags = 0, ?string $name = null): bool` | `bool` | ✓ | - |
| [`mysqli_savepoint()`](./builtins/database/mysqli_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | - |
| [`mysqli_select_db()`](./builtins/database/mysqli_select_db.md) | `(mixed $mysql, string $database): bool` | `bool` | ✓ | - |
| [`mysqli_set_charset()`](./builtins/database/mysqli_set_charset.md) | `(mixed $mysql, string $charset): bool` | `bool` | ✓ | - |
| [`mysqli_set_opt()`](./builtins/database/mysqli_set_opt.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | - |
| [`mysqli_sqlstate()`](./builtins/database/mysqli_sqlstate.md) | `(mixed $mysql): string` | `string` | ✓ | - |
| [`mysqli_ssl_set()`](./builtins/database/mysqli_ssl_set.md) | `(mixed $mysql, ?string $key, ?string $certificate, ?string $ca_certificate, ?string $ca_path, ?string $cipher_algos): bool` | `bool` | ✓ | - |
| [`mysqli_stat()`](./builtins/database/mysqli_stat.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_affected_rows()`](./builtins/database/mysqli_stmt_affected_rows.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_attr_get()`](./builtins/database/mysqli_stmt_attr_get.md) | `(mixed $statement, int $attribute): int` | `int` | ✓ | - |
| [`mysqli_stmt_attr_set()`](./builtins/database/mysqli_stmt_attr_set.md) | `(mixed $statement, int $attribute, int $value): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_bind_param()`](./builtins/database/mysqli_stmt_bind_param.md) | `(mixed $statement, string $types, ...$vars): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_bind_result()`](./builtins/database/mysqli_stmt_bind_result.md) | `(mixed $statement, ...$vars): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_close()`](./builtins/database/mysqli_stmt_close.md) | `(mixed $statement): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_data_seek()`](./builtins/database/mysqli_stmt_data_seek.md) | `(mixed $statement, int $offset): void` | `void` | ✓ | - |
| [`mysqli_stmt_errno()`](./builtins/database/mysqli_stmt_errno.md) | `(mixed $statement): int` | `int` | ✓ | - |
| [`mysqli_stmt_error()`](./builtins/database/mysqli_stmt_error.md) | `(mixed $statement): string` | `string` | ✓ | - |
| [`mysqli_stmt_error_list()`](./builtins/database/mysqli_stmt_error_list.md) | `(mixed $statement): array` | `array` | ✓ | - |
| [`mysqli_stmt_execute()`](./builtins/database/mysqli_stmt_execute.md) | `(mixed $statement, ?array $params = null): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_fetch()`](./builtins/database/mysqli_stmt_fetch.md) | `(mixed $statement): ?bool` | `?bool` | ✓ | - |
| [`mysqli_stmt_field_count()`](./builtins/database/mysqli_stmt_field_count.md) | `(mixed $statement): int` | `int` | ✓ | - |
| [`mysqli_stmt_free_result()`](./builtins/database/mysqli_stmt_free_result.md) | `(mixed $statement): void` | `void` | ✓ | - |
| [`mysqli_stmt_get_result()`](./builtins/database/mysqli_stmt_get_result.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_get_warnings()`](./builtins/database/mysqli_stmt_get_warnings.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_init()`](./builtins/database/mysqli_stmt_init.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_insert_id()`](./builtins/database/mysqli_stmt_insert_id.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_more_results()`](./builtins/database/mysqli_stmt_more_results.md) | `(mixed $statement): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_next_result()`](./builtins/database/mysqli_stmt_next_result.md) | `(mixed $statement): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_num_rows()`](./builtins/database/mysqli_stmt_num_rows.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_param_count()`](./builtins/database/mysqli_stmt_param_count.md) | `(mixed $statement): int` | `int` | ✓ | - |
| [`mysqli_stmt_prepare()`](./builtins/database/mysqli_stmt_prepare.md) | `(mixed $statement, string $query): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_reset()`](./builtins/database/mysqli_stmt_reset.md) | `(mixed $statement): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_result_metadata()`](./builtins/database/mysqli_stmt_result_metadata.md) | `(mixed $statement): mixed` | `mixed` | ✓ | - |
| [`mysqli_stmt_send_long_data()`](./builtins/database/mysqli_stmt_send_long_data.md) | `(mixed $statement, int $param_num, string $data): bool` | `bool` | ✓ | - |
| [`mysqli_stmt_sqlstate()`](./builtins/database/mysqli_stmt_sqlstate.md) | `(mixed $statement): string` | `string` | ✓ | - |
| [`mysqli_stmt_store_result()`](./builtins/database/mysqli_stmt_store_result.md) | `(mixed $statement): bool` | `bool` | ✓ | - |
| [`mysqli_store_result()`](./builtins/database/mysqli_store_result.md) | `(mixed $mysql, int $mode = 0): mixed` | `mixed` | ✓ | - |
| [`mysqli_thread_id()`](./builtins/database/mysqli_thread_id.md) | `(mixed $mysql): int` | `int` | ✓ | - |
| [`mysqli_thread_safe()`](./builtins/database/mysqli_thread_safe.md) | `(): bool` | `bool` | ✓ | - |
| [`mysqli_use_result()`](./builtins/database/mysqli_use_result.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | - |
| [`mysqli_warning_count()`](./builtins/database/mysqli_warning_count.md) | `(mixed $mysql): int` | `int` | ✓ | - |

Classes: `mysqli`, `mysqli_driver`, `mysqli_result`, `mysqli_sql_exception`, `mysqli_stmt`, `mysqli_warning`.

Constants: 111 constants: `MYSQLI_*` (111).

<!-- elephc:generated:symbols:end -->
