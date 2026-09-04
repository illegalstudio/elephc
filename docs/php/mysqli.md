---
title: "mysqli (MySQL / MariaDB)"
description: "The mysqli subset: connections, buffered queries and results, prepared statements, multi_query, mysqli_report error handling, and the documented divergences from php-src."
sidebar:
  order: 18
---

elephc ships a documented **mysqli subset** for MySQL and MariaDB, implemented
as its own PHP surface over the same pure-Rust MySQL client that backs
[PDO](./pdo.md) (`crates/elephc-pdo`). A mysqli program links no system MySQL
client library, and it never declares the PDO classes: `mysqli`,
`mysqli_stmt`, `mysqli_result`, and `mysqli_sql_exception` are their own
types, and a mysqli failure never throws `PDOException`.

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
- `real_connect()` accepts the `MYSQLI_CLIENT_FOUND_ROWS`,
  `MYSQLI_CLIENT_COMPRESS`, and `MYSQLI_CLIENT_IGNORE_SPACE` flags.
  `MYSQLI_CLIENT_SSL` is **rejected with a clear error** — elephc's mysqli has
  no TLS path; use PDO MySQL's `Pdo\Mysql::ATTR_SSL_*` attributes when you
  need TLS.
- `mysqli_options()` honors `MYSQLI_OPT_CONNECT_TIMEOUT`,
  `MYSQLI_INIT_COMMAND`, and `MYSQLI_SET_CHARSET_NAME` (collected before
  `real_connect`, applied at connect time).
- Calling `real_connect()` on an already-connected object closes the previous
  connection first (php-src's mysqlnd reconnect semantics), so a reconnect
  never strands the old handle.
- Connect-time failures populate `connect_errno` / `connect_error` (distinct
  from `errno` / `error`), and the no-argument procedural
  `mysqli_connect_errno()` / `mysqli_connect_error()` read the process-wide
  last connect attempt, exactly like PHP.

Every public method has a `mysqli_*` procedural alias
(`mysqli_connect`, `mysqli_query`, `mysqli_prepare`, …), so
`function_exists('mysqli_query')` is true once the surface is compiled in.
elephc always requires the explicit link argument, including under
`--php-version=8.0` (PHP 8.0's implicit last-opened-link fallback is not
implemented; PHP 8.1+ requires the object anyway).

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
`fetch_fields()`, `fetch_field_direct()`, the `$lengths` property, and
`free()` / `free_result()` / `close()`. `fetch_*` return `null` when the
cursor is exhausted (`fetch_column()` returns `false`).

Field metadata comes from the wire protocol: `name`, `table`, `type`
(`MYSQLI_TYPE_*`), `flags`, `length`, and `decimals` are real; `orgname` and
`orgtable` mirror `name` and `table` (the bridge exposes no original-name
accessors, so an aliased column reports its alias in both); metadata the
bridge does not expose (`def`, `db`, `max_length`, `charsetnr`) reads
`0` / `""`.

Non-select statements return `true` and refresh `affected_rows` and
`insert_id` on the connection. `real_query()` runs the statement and leaves
the result pending for `store_result()`; `use_result()` is an alias of
`store_result()` (results are always buffered — see divergences).

## Prepared statements

`mysqli::prepare()` uses real server-side (non-emulated) `?` placeholders.

```php
$ins = $db->prepare("INSERT INTO users (name, score) VALUES (?, ?)");
$name = "Ada";
$score = 1.5;
$ins->bind_param("sd", $name, $score);
$ins->execute();

$sel = $db->prepare("SELECT id, name FROM users WHERE name = ?");
$sel->execute(["Ada"]);            // PHP 8.1+ shape: binds as strings
$result = $sel->get_result();
$row = $result->fetch_assoc();
```

- `bind_param($types, ...$vars)` accepts the `i` / `d` / `s` / `b` type
  characters and validates that the variable count matches the type string.
  **The variable values are captured when `bind_param()` is called** — see
  divergences below.
- `execute(?array $params = null)` optionally binds an array per execution
  (every element as a string, `null` as SQL NULL), replacing prior binds for
  that execution.
- `get_result()` drains the result set into an independent `mysqli_result`
  and leaves the statement re-executable.
- `store_result()` consumes the pending rows so `$stmt->num_rows` is valid.
- `mysqli::execute_query($sql, $params)` (PHP 8.2+) is
  prepare + execute + get_result in one call.
- Statement properties: `affected_rows`, `errno`, `error`, `field_count`,
  `insert_id`, `num_rows`, `param_count`, `sqlstate`.

`bind_result()` and `fetch()` are **not provided** — see divergences.

## multi_query

`multi_query()` sends the whole batch in one server round-trip; the retained
result sets are walked with `store_result()` / `more_results()` /
`next_result()`:

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

Each produced `mysqli_result` is independent and stays valid while the batch
advances.

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
`stat()`, or a transaction control — fails with errno 2014, php-src's
"Commands out of sync; you can't run this command now". The two-step statement
form honors the same guard: `mysqli_stmt::prepare()` and
`mysqli_stmt::execute()` on a busy link fail with 2014 recorded on the
statement. `close()` discards any unconsumed result with the connection, so a
later `real_connect()` on the same object starts clean.

## Errors, mysqli_report, and mysqli_sql_exception

`mysqli_report()` mirrors PHP 8.1+: the default is
`MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT` (under `--php-version=8.0` the
default is `MYSQLI_REPORT_OFF`).

| Mode | Failure behavior |
|---|---|
| `MYSQLI_REPORT_STRICT` set | throw `mysqli_sql_exception` (extends `RuntimeException`) |
| `MYSQLI_REPORT_ERROR` only | write the message to STDERR, return `false` |
| `MYSQLI_REPORT_OFF` | silent `false` |

After a failure, `errno`, `error`, `sqlstate`, and `error_list` are populated
on the connection (and on the statement). `mysqli_sql_exception` provides both
`getSqlState()` (php 8.1+) and a public `$sqlstate` property (php-src keeps the
property protected behind the getter; elephc exposes both).

## Escaping

`real_escape_string()` / `escape_string()` return the escaped payload
**without** wrapping quotes. Escaping is **charset-aware**, using the
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

## Divergences from php-src

- **`bind_param()` captures values at bind time.** PHP binds *references* and
  reads the current variable values at each `execute()`; elephc cannot alias
  caller variables past the call, so the values are snapshotted when
  `bind_param()` runs. Re-executing with fresh values means re-calling
  `bind_param()` or passing `execute($params)`. Passing a literal instead of
  a variable is tolerated (PHP rejects it).
- **`bind_result()` / `fetch()` are not declared.** Writing fetched rows back
  into caller variables needs the same cross-call aliasing; a silently inert
  binding would be worse than an honest absence (`method_exists` stays
  truthful). Use `get_result()` and the `mysqli_result` fetch family.
- **`MYSQLI_USE_RESULT` is accepted but still buffered.** True unbuffered
  `use_result()` streaming is out of scope; `use_result()` behaves like
  `store_result()`.
- **A connection statement while a prepared result is pending is permitted**,
  not rejected. A `mysqli_stmt::execute()` whose result set has not yet been
  drained by `get_result()`/`store_result()` leaves those rows buffered on the
  bridge (the connection wire is free), so a `query()` on the same connection
  runs and the statement's rows stay fully readable afterward. Real mysqlnd
  raises errno 2014 ("Commands out of sync") for that interleaving unless the
  statement was explicitly `store_result()`-ed; elephc's always-buffered model
  is the more lenient side of that divergence. (The 2014 guard still fires for
  an unconsumed `query()`/`multi_query()`/`real_query()` result — those DO hold
  connection-level state.)
- **`MYSQLI_CLIENT_SSL` is rejected** with
  `"elephc mysqli does not support MYSQLI_CLIENT_SSL; use PDO MySQL TLS attributes"`.
- **No `mysqlnd`**: `extension_loaded('mysqlnd')` is `false`, and
  `mysqli_get_client_info()` reports the bridge's own client string, not a
  mysqlnd version.
- **No implicit PHP 8.0 last-link**: procedural functions always require the
  explicit `mysqli` argument.
- **Procedural aliases validate the link/result argument at runtime** with a
  `TypeError` naming the expected class, so passing a `false` query result
  onward fails loudly.
- **Property writes stick**: the public `mysqli` / `mysqli_result` /
  `mysqli_stmt` properties are refreshed after operations but are not
  write-barriered; assigning to them is not rejected.
- **`ping()` runs `SELECT 1`** rather than the wire-protocol ping packet.
- **Operations on a never-connected object raise `Error`.** php 8 raises
  `Error: mysqli object is not fully initialized` for any operation on a
  `mysqli_init()` / argument-less `new mysqli()` object; elephc matches this
  (including `real_escape_string()` and `character_set_name()`, which used to
  return a value with no signal). `close()` on such an object still returns
  `true`.
- **`MYSQLI_REPORT_ERROR` (without STRICT) writes to STDERR**, not a real
  `E_WARNING`: php raises an `E_WARNING` visible to `set_error_handler()`,
  `error_get_last()`, and log routing, and its message carries the SQLSTATE,
  errno, and method name. elephc's `fwrite(STDERR, "mysqli error: …")` is
  invisible to php's error hooks and omits those fields. `STRICT` (the default)
  is unaffected — it throws `mysqli_sql_exception`.
- **Argument-error classes differ.** `bind_param()` records errno 2031 and
  returns `false` when the type-string length does not match the variable
  count; php throws `ArgumentCountError`. `mysqli_result::data_seek(-1)` and
  `fetch_column()` out of range return `false` / throw `ValueError` per php,
  but a negative `data_seek` returns `false` where php throws `ValueError`.
- **`query()` rejects multi-statement strings client-side** (errno 1064 with
  an elephc-worded message) instead of php-src's server-side rejection: the
  bridge keeps multi-statements enabled for the whole connection (php's
  mysqlnd toggles them per `multi_query()` call via `COM_SET_OPTION`, which
  the bridge does not expose), so without the client-side scan a classic
  `"1; DROP TABLE …"` injection would execute. The scan is **charset-aware**
  (it consults the connection's live charset, the same per-charset table the
  escape uses, over the raw query bytes — never a lossy-UTF-8 copy), so a
  GBK-family `<lead><0x5C>` inside a string literal is lexed as one character
  exactly like the server, and cannot hide the real closing quote to smuggle a
  second statement past the guard. The scan skips string literals, backtick
  identifiers, and comments; a trailing `;` is fine. Executable comments are
  **not** skipped: MySQL's `/*! … */` (bare or versioned) and MariaDB's
  `/*M! … */` are live SQL to the server, so a `;` inside them counts as a
  statement separator (the marker is case-sensitive, like the server's —
  a lowercase `/*m!` is an ordinary inert comment). There
  is no exemption for compound-body DDL: `CREATE PROCEDURE … BEGIN …; … END`
  through `query()` is rejected too (telling body semicolons apart from a
  statement separator safely needs a full BEGIN/END parser, and any cheaper
  heuristic would let an `… END; DROP …` tail execute) — run compound DDL
  through `multi_query()`, which handles it as one statement. php-src's
  `mysqli_query()` accepts such DDL, so this is a (loud) divergence.
- **`$info` is always `""`** (and `mysqli_info()` always `null`): the bridge
  does not expose the OK-packet info string ("Rows matched: … Changed: …").
- **`insert_id` is an `int`**: an `AUTO_INCREMENT` value beyond
  `PHP_INT_MAX` (e.g. `BIGINT UNSIGNED`) wraps, where PHP returns a numeric
  string.
- **`fetch_object()` constructs before assigning properties** (the
  `PDO::FETCH_PROPS_LATE` order); php-src assigns the row first and calls the
  constructor afterwards.
- **Capability probes do not inject the surface**: like PDO,
  `class_exists('mysqli')` in a program with no other mysqli reference
  reports `false`; build with `--with-mysqli` to force the surface. A bare
  `MYSQLI_*` constant reference *does* inject it.
- **`eval()` never sees mysqli** (same rule as PDO):
  `extension_loaded('mysqli')` is `false` inside `eval()`.
- **`get_charset()` reports only the charset name.** The returned object's
  `charset` field is exact; `collation`, `number`, `state`, `dir`,
  `min_length`, `max_length`, and `comment` are `0` / `""` (the bridge does not
  expose the charset table).

## Not implemented (fails loudly)

The v1 subset deliberately omits — calls fail to compile
(undeclared symbol) rather than silently pretending:

- `mysqli_poll` / `mysqli_reap_async_query` / async connect
- `change_user`, `kill`, `dump_debug_info`, `refresh`, embedded server
- `mysqli_ssl_set` (and `MYSQLI_CLIENT_SSL`, rejected at connect)
- true unbuffered `use_result`
- the `mysqli_driver` and `mysqli_warning` objects
- `bind_result` / `fetch` (see divergences)
- `mysqli_stmt_data_seek` and `mysqli_stmt_result_metadata` (the statement does
  not buffer its own rows in this model — use `get_result()` and the
  `mysqli_result` fetch family, which do)
- the legacy `mysql_*` API

Provided procedural surface beyond the earlier list now also includes
`mysqli_savepoint` / `mysqli_release_savepoint`, `mysqli_stmt_init` /
`mysqli_stmt_prepare` / `mysqli_execute`, `mysqli_stmt_sqlstate` /
`mysqli_stmt_field_count` / `mysqli_stmt_insert_id` / `mysqli_stmt_error_list` /
`mysqli_stmt_free_result`, `mysqli_fetch_lengths`, `mysqli_field_seek` /
`mysqli_field_tell`, `mysqli_get_charset`, and `mysqli_thread_safe`
(`mysqli_thread_safe()` reports `false` — this build's client is not
thread-safe).

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`mysqli_affected_rows()`](./builtins/database/mysqli_affected_rows.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_autocommit()`](./builtins/database/mysqli_autocommit.md) | `(mixed $mysql, bool $enable): bool` | `bool` | ✓ | — |
| [`mysqli_begin_transaction()`](./builtins/database/mysqli_begin_transaction.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_character_set_name()`](./builtins/database/mysqli_character_set_name.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_close()`](./builtins/database/mysqli_close.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_commit()`](./builtins/database/mysqli_commit.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_connect()`](./builtins/database/mysqli_connect.md) | `(string $hostname = null, string $username = null, string $password = null, string $database = null, int $port = null, string $socket = null): mixed` | `mixed` | ✓ | — |
| [`mysqli_connect_errno()`](./builtins/database/mysqli_connect_errno.md) | `(): int` | `int` | ✓ | — |
| [`mysqli_connect_error()`](./builtins/database/mysqli_connect_error.md) | `(): string` | `string` | ✓ | — |
| [`mysqli_data_seek()`](./builtins/database/mysqli_data_seek.md) | `(mixed $result, int $offset): bool` | `bool` | ✓ | — |
| [`mysqli_errno()`](./builtins/database/mysqli_errno.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_error()`](./builtins/database/mysqli_error.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_error_list()`](./builtins/database/mysqli_error_list.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_escape_string()`](./builtins/database/mysqli_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | — |
| [`mysqli_execute()`](./builtins/database/mysqli_execute.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_execute_query()`](./builtins/database/mysqli_execute_query.md) | `(mixed $mysql, string $query, mixed $params = null): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_all()`](./builtins/database/mysqli_fetch_all.md) | `(mixed $result, int $mode = 2): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_array()`](./builtins/database/mysqli_fetch_array.md) | `(mixed $result, int $mode = 3): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_assoc()`](./builtins/database/mysqli_fetch_assoc.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_column()`](./builtins/database/mysqli_fetch_column.md) | `(mixed $result, int $column = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_field()`](./builtins/database/mysqli_fetch_field.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_field_direct()`](./builtins/database/mysqli_fetch_field_direct.md) | `(mixed $result, int $index): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_fields()`](./builtins/database/mysqli_fetch_fields.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_lengths()`](./builtins/database/mysqli_fetch_lengths.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_object()`](./builtins/database/mysqli_fetch_object.md) | `(mixed $result, string $class = 'stdClass', mixed $constructor_args = []): mixed` | `mixed` | ✓ | — |
| [`mysqli_fetch_row()`](./builtins/database/mysqli_fetch_row.md) | `(mixed $result): mixed` | `mixed` | ✓ | — |
| [`mysqli_field_count()`](./builtins/database/mysqli_field_count.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_field_seek()`](./builtins/database/mysqli_field_seek.md) | `(mixed $result, int $index): bool` | `bool` | ✓ | — |
| [`mysqli_field_tell()`](./builtins/database/mysqli_field_tell.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_free_result()`](./builtins/database/mysqli_free_result.md) | `(mixed $result): void` | `void` | ✓ | — |
| [`mysqli_get_charset()`](./builtins/database/mysqli_get_charset.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_get_client_info()`](./builtins/database/mysqli_get_client_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_client_version()`](./builtins/database/mysqli_get_client_version.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_get_host_info()`](./builtins/database/mysqli_get_host_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_proto_info()`](./builtins/database/mysqli_get_proto_info.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_get_server_info()`](./builtins/database/mysqli_get_server_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_get_server_version()`](./builtins/database/mysqli_get_server_version.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_info()`](./builtins/database/mysqli_info.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_init()`](./builtins/database/mysqli_init.md) | `(): mixed` | `mixed` | ✓ | — |
| [`mysqli_insert_id()`](./builtins/database/mysqli_insert_id.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_more_results()`](./builtins/database/mysqli_more_results.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_multi_query()`](./builtins/database/mysqli_multi_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_next_result()`](./builtins/database/mysqli_next_result.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_num_fields()`](./builtins/database/mysqli_num_fields.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_num_rows()`](./builtins/database/mysqli_num_rows.md) | `(mixed $result): int` | `int` | ✓ | — |
| [`mysqli_options()`](./builtins/database/mysqli_options.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | — |
| [`mysqli_ping()`](./builtins/database/mysqli_ping.md) | `(mixed $mysql): bool` | `bool` | ✓ | — |
| [`mysqli_prepare()`](./builtins/database/mysqli_prepare.md) | `(mixed $mysql, string $query): mixed` | `mixed` | ✓ | — |
| [`mysqli_query()`](./builtins/database/mysqli_query.md) | `(mixed $mysql, string $query, int $result_mode = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_real_connect()`](./builtins/database/mysqli_real_connect.md) | `(mixed $mysql, string $hostname = null, string $username = null, string $password = null, string $database = null, int $port = null, string $socket = null, int $flags = 0): bool` | `bool` | ✓ | — |
| [`mysqli_real_escape_string()`](./builtins/database/mysqli_real_escape_string.md) | `(mixed $mysql, string $string): string` | `string` | ✓ | — |
| [`mysqli_real_query()`](./builtins/database/mysqli_real_query.md) | `(mixed $mysql, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_release_savepoint()`](./builtins/database/mysqli_release_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | — |
| [`mysqli_report()`](./builtins/database/mysqli_report.md) | `(int $flags): bool` | `bool` | ✓ | — |
| [`mysqli_rollback()`](./builtins/database/mysqli_rollback.md) | `(mixed $mysql, int $flags = 0, string $name = null): bool` | `bool` | ✓ | — |
| [`mysqli_savepoint()`](./builtins/database/mysqli_savepoint.md) | `(mixed $mysql, string $name): bool` | `bool` | ✓ | — |
| [`mysqli_select_db()`](./builtins/database/mysqli_select_db.md) | `(mixed $mysql, string $database): bool` | `bool` | ✓ | — |
| [`mysqli_set_charset()`](./builtins/database/mysqli_set_charset.md) | `(mixed $mysql, string $charset): bool` | `bool` | ✓ | — |
| [`mysqli_set_opt()`](./builtins/database/mysqli_set_opt.md) | `(mixed $mysql, int $option, mixed $value): bool` | `bool` | ✓ | — |
| [`mysqli_sqlstate()`](./builtins/database/mysqli_sqlstate.md) | `(mixed $mysql): string` | `string` | ✓ | — |
| [`mysqli_stat()`](./builtins/database/mysqli_stat.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_affected_rows()`](./builtins/database/mysqli_stmt_affected_rows.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_bind_param()`](./builtins/database/mysqli_stmt_bind_param.md) | `(mixed $statement, string $types, ...$vars): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_close()`](./builtins/database/mysqli_stmt_close.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_errno()`](./builtins/database/mysqli_stmt_errno.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_error()`](./builtins/database/mysqli_stmt_error.md) | `(mixed $statement): string` | `string` | ✓ | — |
| [`mysqli_stmt_error_list()`](./builtins/database/mysqli_stmt_error_list.md) | `(mixed $statement): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_execute()`](./builtins/database/mysqli_stmt_execute.md) | `(mixed $statement, mixed $params = null): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_field_count()`](./builtins/database/mysqli_stmt_field_count.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_free_result()`](./builtins/database/mysqli_stmt_free_result.md) | `(mixed $statement): void` | `void` | ✓ | — |
| [`mysqli_stmt_get_result()`](./builtins/database/mysqli_stmt_get_result.md) | `(mixed $statement): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_init()`](./builtins/database/mysqli_stmt_init.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_stmt_insert_id()`](./builtins/database/mysqli_stmt_insert_id.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_num_rows()`](./builtins/database/mysqli_stmt_num_rows.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_param_count()`](./builtins/database/mysqli_stmt_param_count.md) | `(mixed $statement): int` | `int` | ✓ | — |
| [`mysqli_stmt_prepare()`](./builtins/database/mysqli_stmt_prepare.md) | `(mixed $statement, string $query): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_reset()`](./builtins/database/mysqli_stmt_reset.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_stmt_sqlstate()`](./builtins/database/mysqli_stmt_sqlstate.md) | `(mixed $statement): string` | `string` | ✓ | — |
| [`mysqli_stmt_store_result()`](./builtins/database/mysqli_stmt_store_result.md) | `(mixed $statement): bool` | `bool` | ✓ | — |
| [`mysqli_store_result()`](./builtins/database/mysqli_store_result.md) | `(mixed $mysql, int $mode = 0): mixed` | `mixed` | ✓ | — |
| [`mysqli_thread_id()`](./builtins/database/mysqli_thread_id.md) | `(mixed $mysql): int` | `int` | ✓ | — |
| [`mysqli_thread_safe()`](./builtins/database/mysqli_thread_safe.md) | `(): bool` | `bool` | ✓ | — |
| [`mysqli_use_result()`](./builtins/database/mysqli_use_result.md) | `(mixed $mysql): mixed` | `mixed` | ✓ | — |
| [`mysqli_warning_count()`](./builtins/database/mysqli_warning_count.md) | `(mixed $mysql): int` | `int` | ✓ | — |

Classes: `mysqli`, `mysqli_result`, `mysqli_sql_exception`, `mysqli_stmt`.

Constants: `MYSQLI_ASSOC`, `MYSQLI_BOTH`, `MYSQLI_CLIENT_COMPRESS`, `MYSQLI_CLIENT_FOUND_ROWS`, `MYSQLI_CLIENT_IGNORE_SPACE`, `MYSQLI_CLIENT_INTERACTIVE`, `MYSQLI_CLIENT_SSL`, `MYSQLI_INIT_COMMAND`, `MYSQLI_NUM`, `MYSQLI_OPT_CONNECT_TIMEOUT`, `MYSQLI_REPORT_ALL`, `MYSQLI_REPORT_ERROR`, `MYSQLI_REPORT_INDEX`, `MYSQLI_REPORT_OFF`, `MYSQLI_REPORT_STRICT`, `MYSQLI_SET_CHARSET_NAME`, `MYSQLI_STORE_RESULT`, `MYSQLI_TRANS_COR_AND_CHAIN`, `MYSQLI_TRANS_COR_AND_NO_CHAIN`, `MYSQLI_TRANS_COR_NO_RELEASE`, `MYSQLI_TRANS_COR_RELEASE`, `MYSQLI_TRANS_START_READ_ONLY`, `MYSQLI_TRANS_START_READ_WRITE`, `MYSQLI_TRANS_START_WITH_CONSISTENT_SNAPSHOT`, `MYSQLI_TYPE_BIT`, `MYSQLI_TYPE_BLOB`, `MYSQLI_TYPE_DATE`, `MYSQLI_TYPE_DATETIME`, `MYSQLI_TYPE_DECIMAL`, `MYSQLI_TYPE_DOUBLE`, `MYSQLI_TYPE_ENUM`, `MYSQLI_TYPE_FLOAT`, `MYSQLI_TYPE_GEOMETRY`, `MYSQLI_TYPE_INT24`, `MYSQLI_TYPE_JSON`, `MYSQLI_TYPE_LONG`, `MYSQLI_TYPE_LONGLONG`, `MYSQLI_TYPE_LONG_BLOB`, `MYSQLI_TYPE_MEDIUM_BLOB`, `MYSQLI_TYPE_NEWDATE`, `MYSQLI_TYPE_NEWDECIMAL`, `MYSQLI_TYPE_NULL`, `MYSQLI_TYPE_SET`, `MYSQLI_TYPE_SHORT`, `MYSQLI_TYPE_STRING`, `MYSQLI_TYPE_TIME`, `MYSQLI_TYPE_TIMESTAMP`, `MYSQLI_TYPE_TINY`, `MYSQLI_TYPE_TINY_BLOB`, `MYSQLI_TYPE_VAR_STRING`, `MYSQLI_TYPE_YEAR`, `MYSQLI_USE_RESULT`.

<!-- elephc:generated:symbols:end -->
