---
title: "PDO (Databases)"
description: "PDO database access with the SQLite, PostgreSQL, and MySQL/MariaDB drivers: connections, prepared statements, fetch modes, transactions, and the divergences from php-src worth knowing about."
sidebar:
  order: 17
---

elephc implements a large, practical subset of PHP 8.4's PDO database layer, with the
**SQLite**, **PostgreSQL**, and **MySQL / MariaDB** drivers. `PDO`, `PDOStatement`,
and `PDOException` behave like their PHP counterparts for everyday use: connect,
execute, prepare/bind, fetch, and run transactions. The DSN prefix selects the driver,
so the same code works against any of the databases.

Every driver is linked statically (SQLite is bundled; PostgreSQL and MySQL use
pure-Rust clients), so a compiled PDO binary has **no system database-client
dependency** — it runs anywhere the elephc binary runs. SQLite runs in-process;
PostgreSQL and MySQL connect to a running server over the network.

The surface is deliberately honest: where a feature is not implemented, it fails
loudly (a `PDOException`, a `ValueError`, a `TypeError`) rather than silently
returning wrong data. The [Divergences from php-src](#divergences-from-php-src) and
[Limitations](#limitations) sections below enumerate what is different and why —
read them before porting security-sensitive or data-loading code.

## Connecting

```php
<?php
// SQLite — file-backed (created if missing) or in-memory.
$db = new PDO("sqlite:/path/to/app.db");
$mem = new PDO("sqlite::memory:");

// PostgreSQL — credentials in the DSN or as constructor arguments.
$pg = new PDO("pgsql:host=localhost;port=5432;dbname=app;user=me;password=secret");
$pg = new PDO("pgsql:host=localhost;dbname=app", "me", "secret");

// MySQL / MariaDB — credentials in the DSN or as constructor arguments.
$my = new PDO("mysql:host=127.0.0.1;port=3306;dbname=app;user=me;password=secret");
$my = new PDO("mysql:host=127.0.0.1;dbname=app", "me", "secret");
```

The DSN must start with `sqlite:`, `pgsql:`, or `mysql:`. A DSN with no colon at all
throws a `PDOException` naming the argument (`PDO::__construct(): Argument #1 ($dsn)
must be a valid data source name`); a colon-bearing DSN with an unknown prefix throws
`PDOException("could not find driver")` — both before any connection is attempted,
matching php-src.

For SQLite, the `$username` / `$password` arguments are accepted for signature
compatibility but ignored; constructor options still seed PDO attributes. A failed
connection throws a `PDOException` whose message carries a `SQLSTATE[…]:` prefix and
whose `$errorInfo` is a real triple (`HY000` for SQLite, `08006` for the network
drivers).

### The `uri:` DSN

`new PDO("uri:/etc/app/db.dsn")` reads the real DSN from the **first line** of the
referenced file, as in php-src (which deprecated the form but still supports it).
Divergences: elephc has no `file://` stream wrapper, so a `uri:file:///path` DSN has
the scheme stripped and the remainder opened as a plain path (any other scheme simply
fails to open); the trailing newline is trimmed; and no `E_DEPRECATED` is raised
(elephc has no deprecation channel). An unreadable/empty file or a first line with no
colon throws a `PDOException`.

### Credential precedence (asymmetric, by driver — matches php-src)

- **PostgreSQL**: a `user=` / `password=` **in the DSN wins** over the constructor
  arguments; the constructor's values are only folded in when the DSN does not carry
  that key. (libpq's conninfo parsing is last-wins, and php-src assembles the DSN's
  keys last.)
- **MySQL / MariaDB**: the **constructor argument wins** over a DSN key. `new
  PDO("mysql:host=h;user=readonly", "admin", $pw)` connects as `admin`, exactly as in
  real PHP.

### Persistent connections

`PDO::ATTR_PERSISTENT` in the constructor options selects a **process-local** pool.
The pool key is the fully materialized DSN **plus** the `ATTR_PERSISTENT` value when
that value is a non-numeric, non-empty string — so two persistent connections to the
same DSN under different key strings stay distinct handles, exactly as in php-src.
Anything else (a bool, an int, a numeric string, `""`) is a plain numeric coercion and
uses the unkeyed pool; none of those forms is an error.

```php
<?php
$a = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => true]);      // unkeyed pool
$b = new PDO($dsn, null, null, [PDO::ATTR_PERSISTENT => "reports"]); // separate pool
```

Setting `ATTR_PERSISTENT` later with `setAttribute()` updates the reported attribute
but does not reopen an already-created connection. Persistent connections are local to
the running native process; there is no cross-process pool, and **no liveness check is
performed on reuse** (see [Limitations](#limitations)).

## Executing statements

```php
<?php
// exec() runs a statement with no result set and returns the affected row count.
$db->exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
$n = $db->exec("INSERT INTO users (name, score) VALUES ('Ada', 9.5)");

echo $db->lastInsertId();   // "1"
```

`exec("")`, `query("")`, and `prepare("")` each throw a `ValueError` before any driver
call, naming their own method (php-src does the same).

## Prepared statements and binding

`execute()` accepts an array of parameters. Positional (`?`) placeholders bind by
position; named (`:name`) placeholders bind by key (with or without the leading colon).
Bound values are typed automatically (int, float, string, null, bool).

```php
<?php
// Positional
$stmt = $db->prepare("SELECT name FROM users WHERE id = ?");
$stmt->execute([1]);

// Named
$ins = $db->prepare("INSERT INTO users (name, score) VALUES (:name, :score)");
$ins->execute([":name" => "Bob", ":score" => 7.25]);
$ins->execute(["name" => "Cyd", "score" => 3.0]);  // colon optional
```

`query()` prepares and immediately executes a statement, returning the `PDOStatement`
ready to fetch.

Parameters can also be bound individually with `bindValue()` (and `bindParam()`), then
applied by an argument-less `execute()`:

```php
<?php
$stmt = $db->prepare("INSERT INTO users (name, score) VALUES (:name, :score)");
$stmt->bindValue(":name", "Dee");
$stmt->bindValue(":score", 5, PDO::PARAM_INT);
$stmt->execute();
```

`bindParam()` binds the variable's *current* value (it does not defer a by-reference
read to `execute()` time), so bind immediately before `execute()`.

**`execute($params)` REPLACES the recorded bindings**, it does not layer on top of
them — php-src rebuilds its bound-parameter table from the array, so a slot bound
earlier with `bindValue()` but absent from `$params` does not keep a stale value, and a
later argument-less `execute()` replays *that array*, not the earlier `bindValue()`
calls.

### Bind validation

- `bindValue()` / `bindParam()` / `bindColumn()` reject a positional index below 1
  (`ValueError`) and an empty named placeholder (`ValueError`), before recording
  anything.
- A named placeholder the SQL never declares, or a positional index past the
  placeholder count, fails `execute()` with **SQLSTATE HY093** ("Invalid parameter
  number") — errmode-aware, and the statement is left un-executed so a later `fetch()`
  cannot step it.
- `PDO::PARAM_*` **flags** are masked off before dispatch, so
  `PDO::PARAM_INT|PDO::PARAM_INPUT_OUTPUT` still binds an integer.
- `PDO::PARAM_BOOL` uses the driver's **real boolean bind** (PostgreSQL sends `t`/`f`,
  not an integer literal a `BOOL` column would refuse); the value is truthiness-reduced
  first, as php-src does.
- `PDO::PARAM_LOB` binds raw bytes (embedded NUL preserved).
- `PDO::PARAM_NULL` / `PARAM_INT` / `PARAM_STR` behave as expected; `PARAM_STR` binds
  with a measured byte length, so a value containing a NUL byte binds in full.

### Quoting

Prefer prepared statements over interpolation. When you must embed a string,
`PDO::quote()` is driver-aware:

```php
<?php
$db->quote("O'Brien");                  // 'O''Brien'
$db->quote($bytes, PDO::PARAM_LOB);     // _binary'…' (MySQL) / '\x…' (pgsql bytea hex)
```

- **SQLite**: `''`-doubling. Binary-safe for an embedded NUL byte — a deliberate
  improvement over php-src, whose own SQLite quoter truncates at the first NUL.
- **PostgreSQL**: `''`-doubling, switching to the `E'…'` form when a backslash is
  present. `PARAM_LOB` produces a `bytea` hex literal.
- **MySQL**: backslash-escapes quotes, backslashes, and control bytes — **unless** the
  session's `sql_mode` has `NO_BACKSLASH_ESCAPES`, which the bridge reads live; under
  that mode backslash-escaping is actively unsafe, so `quote()` falls back to
  `''`-doubling only, mirroring mysqlnd. `PARAM_LOB` adds the `_binary` introducer.

## Fetching results

```php
<?php
$stmt = $db->query("SELECT id, name FROM users");

$stmt->fetch(PDO::FETCH_ASSOC);  // ["id" => 1, "name" => "Ada"]
$stmt->fetch(PDO::FETCH_NUM);    // [0 => 1, 1 => "Ada"]
$stmt->fetch(PDO::FETCH_BOTH);   // both numeric and string keys
$stmt->fetch(PDO::FETCH_OBJ);    // stdClass { id: 1, name: "Ada" }

class UserRow {
    public mixed $id;
    public mixed $name;
}

// Class / object targets are configured on the STATEMENT, never passed to fetch().
$stmt = $db->query("SELECT id, name FROM users");
$stmt->setFetchMode(PDO::FETCH_CLASS, UserRow::class);
$row = $stmt->fetch();

$target = new UserRow();
$stmt = $db->query("SELECT id, name FROM users");
$stmt->setFetchMode(PDO::FETCH_INTO, $target);
$same = $stmt->fetch();          // fills and returns $target

$all = $db->query("SELECT id FROM users")->fetchAll(PDO::FETCH_NUM);
$one = $db->query("SELECT name FROM users")->fetchColumn();  // first column of next row

// FETCH_COLUMN yields one column per row as a scalar:
$ids = $db->query("SELECT id FROM users")->fetchAll(PDO::FETCH_COLUMN);  // [1, 2, …]
```

`fetch()` has **php-src's signature** — `fetch(int $mode = PDO::FETCH_DEFAULT, int
$cursorOrientation = PDO::FETCH_ORI_NEXT, int $cursorOffset = 0)`. Its second parameter
is an int **cursor orientation**, *not* a class/object target. (An earlier elephc
release accepted `fetch(PDO::FETCH_CLASS, Row::class)`; that idiom is a `TypeError` in
real PHP and no longer works here. Use `setFetchMode()` or `fetchObject()`.) The
orientation is accepted and every value behaves as `FETCH_ORI_NEXT`: all cursors here
are forward-only.

`fetch()` returns `false` when the result set is exhausted. `FETCH_OBJ` creates a real
`stdClass` and assigns dynamic properties directly, including numeric column names such
as `"0"`. `FETCH_CLASS` builds the configured class (or `stdClass` when none is
configured) and assigns column values to matching declared or dynamic properties;
`FETCH_INTO` fills and returns the configured object, and raises **HY000** ("No
fetch-into object specified.") when there is none.

Column values are returned with their native scalar shape: integer → int, real /
floating point → float, text → string, binary/BLOB/`bytea` → string with embedded NUL
bytes preserved, and `NULL` → null. `FETCH_BOTH` is the default mode.

### Fetch modes

| Mode | Notes |
| --- | --- |
| `FETCH_ASSOC` / `FETCH_NUM` / `FETCH_BOTH` / `FETCH_OBJ` | Fully supported. |
| `FETCH_COLUMN` | Column index is the 2nd argument to `setFetchMode()` / `fetchAll()`. |
| `FETCH_CLASS` | Target class configured on the statement. Always **constructor-first** hydration. |
| `FETCH_INTO` | Target object configured on the statement; HY000 without one. |
| `FETCH_KEY_PAIR` | Two-column result as `[col0 => col1]`; HY000 if the result has ≠ 2 columns. |
| `FETCH_NAMED` | Assoc-only; duplicate column names group into a list under one key. |
| `FETCH_BOUND` | Advances the cursor and returns `true`/`false`, but `bindColumn()` is unsupported, so nothing is written back. |
| `FETCH_CLASSTYPE` | **Real**: the class name comes from column 0's *value*, per row; column 0 is consumed; an unknown class falls back to `stdClass`. |
| `FETCH_GROUP` / `FETCH_UNIQUE` | **Implemented** (see below). |
| `FETCH_PROPS_LATE` | Accepted, but inert — hydration is always constructor-first here. |
| `FETCH_LAZY` | Rejected in `fetchAll()` (as in php-src) and, unlike php-src, also in `fetch()` — elephc has no `PDORow` class. |
| `FETCH_FUNC` | Rejected with a `PDOException` (see [Limitations](#limitations)). |

`setFetchMode()` validates before storing anything, so a rejected call leaves the
statement's previous mode intact: an out-of-range base mode is a `ValueError`,
`FETCH_COLUMN` with a non-int index is a `TypeError` (a numeric *string* is rejected too,
as in php-src, where the argument is variadic and never juggled) and with a negative index
a `ValueError`, `FETCH_COLUMN`/`FETCH_CLASS`/`FETCH_INTO` with no second argument raises a
`ValueError` carrying php-src's ArgumentCountError text (elephc has no
`ArgumentCountError` class), and `FETCH_CLASS|FETCH_CLASSTYPE` *with* an explicit class
argument is rejected as the contradiction it is. The OR-able high-bit flags are masked off
before every one of those gates, so `FETCH_CLASS|FETCH_PROPS_LATE` is checked (and stores
its class) exactly like a bare `FETCH_CLASS`.

`fetchColumn($n)` throws a `ValueError` for a negative index, and for an index past the
column count of the row it just fetched — but an exhausted result set still just returns
`false`, as in real PHP.

### Column metadata

`getColumnMeta($i)` returns `false` for a statement that has not been executed and for an
index at or past the column count; a **negative** index is a `ValueError` (php-src
validates the argument before any driver dispatch).

- **SQLite** reports the runtime **storage class**, exactly as php-src's `pdo_sqlite`
  does: `native_type` is `"integer"` / `"double"` / `"string"` / `"null"`, a BLOB column
  reports `native_type` `"string"` with `"blob"` pushed into `flags` and `pdo_type`
  `PARAM_STR`. The column's *declared* type (`sqlite3_column_decltype`) is a separate
  **`sqlite:decl_type`** key, present only when the column has one. `len`, `precision` and
  `table` have no source in `pdo_sqlite` and are present with neutral values (`0`, `0`,
  `""`) rather than omitted, so a caller reading them never errors.
- **PostgreSQL** and **MySQL** report their own real metadata — see their sections below.

### `FETCH_GROUP` and `FETCH_UNIQUE`

Both consume **column 0** as the key and exclude it from the row:

```php
<?php
// [type => [ [name…], [name…] ], …] — every row that carried that key
$byType = $db->query("SELECT type, name, id FROM t")->fetchAll(PDO::FETCH_GROUP|PDO::FETCH_ASSOC);

// [type => [name, name, …]] — FETCH_COLUMN under GROUP defaults the value column to 1
$names  = $db->query("SELECT type, name FROM t")->fetchAll(PDO::FETCH_GROUP|PDO::FETCH_COLUMN);

// [id => row] — last write wins, exactly like php-src's plain overwrite
$byId   = $db->query("SELECT id, name FROM t")->fetchAll(PDO::FETCH_UNIQUE|PDO::FETCH_ASSOC);
```

Groups come out in **first-seen key order**, and `FETCH_NUM`/`FETCH_BOTH` rows are
re-indexed from 0 after the key column is removed — both matching php-src.

Supported base modes under GROUP/UNIQUE: `FETCH_ASSOC`, `FETCH_NUM`, `FETCH_BOTH`,
`FETCH_OBJ`, `FETCH_COLUMN`, `FETCH_CLASS`. Any other base mode, and the
`FETCH_CLASSTYPE` combination (which would want column 0 too), raise a `PDOException`
rather than returning a plausible array of the wrong shape.

**One divergence:** php-src casts the grouping key to a string and PHP's array then
folds an integer-*looking* string key back to an int key. elephc's arrays do not, so a
grouping column holding `1` reads back as `$out["1"]`, not `$out[1]`. Non-numeric keys
(type names, statuses, categories — the overwhelmingly common case) are identical.

## Iterating a statement

A `PDOStatement` is Traversable, so `foreach` walks the result set forward with
sequential integer keys, yielding each row in the statement's current fetch mode:

```php
<?php
$stmt = $db->query("SELECT id, name FROM users");
$stmt->setFetchMode(PDO::FETCH_ASSOC);

foreach ($stmt as $i => $row) {
    echo $i, ": ", $row["name"], "\n";
}
```

The cursor is forward-only: each row is consumed as it is yielded, so a statement can
be iterated once. `getIterator()` returns the statement itself.

Note that elephc's `PDOStatement` implements `Iterator` directly, where php-src's
implements `IteratorAggregate` with an internal, userland-invisible iterator. `foreach`
behaves identically and `$stmt instanceof \Traversable` is true in both, but
`instanceof \Iterator` / `instanceof \IteratorAggregate` and
`method_exists($stmt, 'rewind')` answer differently. This is deliberate — see the
F-STMT-11 note in `src/pdo_prelude.rs`.

## Attributes

`getAttribute()` / `setAttribute()` act on:

| Attribute | Behavior |
| --- | --- |
| `ATTR_ERRMODE` | Silent / Warning / Exception (default). |
| `ATTR_DRIVER_NAME` | `"sqlite"`, `"pgsql"`, or `"mysql"`. |
| `ATTR_PERSISTENT` | Pool selection (constructor only, in practice). |
| `ATTR_TIMEOUT` | Seconds. SQLite: busy-timeout. pgsql/mysql: folded into the DSN as `connect_timeout`, bounding the initial connect. |
| `ATTR_DEFAULT_FETCH_MODE` | Mode used by a no-argument `fetch()`; inherited by statements at `prepare()` time. |
| `ATTR_SERVER_VERSION` | The server's version string. |
| `ATTR_CLIENT_VERSION` | Same value (the bridge links each client statically and has no separate client-library version; for SQLite this is exact PHP parity). |
| `ATTR_CONNECTION_STATUS` | The fixed string `"Connection OK; waiting to send."`. |
| `ATTR_CASE` | Folds fetched column-name keys upper/lowercase. |
| `ATTR_ORACLE_NULLS` | Folds `NULL` ↔ `""` in fetched scalar values. |
| `ATTR_STRINGIFY_FETCHES` | Stringifies fetched INTEGER/FLOAT values. |
| `Pdo\Sqlite::ATTR_OPEN_FLAGS` | Raw `sqlite3_open_v2` flags at open time. A `file:` DSN body always gets `SQLITE_OPEN_URI` OR-ed in. |
| `Pdo\Sqlite::ATTR_READONLY_STATEMENT` | Live `sqlite3_stmt_readonly()` read (statement-level). |
| `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` | Wired: with it on, `errorInfo()[1]` is the *extended* code (`2067` SQLITE_CONSTRAINT_UNIQUE, not the coarse `19`). Write-only, exactly as in php-src — `getAttribute()` returns `null`. |
| `Pdo\Mysql::ATTR_INIT_COMMAND` | One SQL statement run right after authentication. |
| `Pdo\Mysql::ATTR_FOUND_ROWS` | Wired: negotiates `CLIENT_FOUND_ROWS`, so an UPDATE's `rowCount()` reports rows *matched*, not rows *changed*. |
| `Pdo\Mysql::ATTR_SSL_KEY` / `ATTR_SSL_CERT` / `ATTR_SSL_CA` / `ATTR_SSL_VERIFY_SERVER_CERT` | Drive MySQL TLS (see the TLS section). |

`ATTR_CASE`, `ATTR_ORACLE_NULLS`, `ATTR_STRINGIFY_FETCHES`, and
`ATTR_DEFAULT_FETCH_MODE` are **snapshotted onto each statement at `prepare()` time**,
not re-read on every fetch: a `setAttribute()` call after a statement is prepared does
not retroactively affect it (real PHP re-checks the connection attribute per fetch).

Every other `ATTR_*` in the generic range (0..21) and the driver-specific range
(1000..1015) is **stored and read back** but has no effect.

### Attribute value validation

- The **shape** of the value is checked before any range check, exactly as php-src's
  `pdo_get_long_param()` / `pdo_get_bool_param()` do: `setAttribute(PDO::ATTR_ERRMODE,
  "banana")` raises a `TypeError` instead of casting to `0` and silently switching the
  connection to `ERRMODE_SILENT`. The same check runs on the constructor's `$options`
  array.
- `ATTR_ERRMODE` outside 0/1/2, and `ATTR_CASE` outside `CASE_NATURAL`/`UPPER`/`LOWER`,
  raise a `ValueError` and leave the current value untouched. `ATTR_DEFAULT_FETCH_MODE`
  rejects `0`.
- An **unknown attribute number** (a negative, 22..999, or above 1015):
  `setAttribute()` returns **`false` silently** — no exception, no error state, not even
  under `ERRMODE_EXCEPTION` — while `getAttribute()` raises **IM001** ("driver does not
  support that attribute") and returns `false`. That asymmetry looks like a php-src bug;
  it is nonetheless php-src's behavior, verified against a real 8.5.6 CLI, and mirroring
  it is the point of this surface.

`PDOStatement::getAttribute()` answers `Pdo\Sqlite::ATTR_READONLY_STATEMENT` (live) and
`ATTR_EMULATE_PREPARES` (from the prepare-time snapshot); every other statement
attribute raises **IM001**, and `PDOStatement::setAttribute()` raises IM001 for
everything (no driver here registers a statement-attribute hook).

## PostgreSQL notes

The PostgreSQL driver behaves like the SQLite one, with a few database-specific points:

- **Placeholders.** PDO `?` and `:name` placeholders are translated to PostgreSQL's
  native `$1, $2, …` at prepare time, so you write the same portable SQL for either
  driver. The scanner skips `--` / `/* */` comments, `'…'` / `"…"` quoted regions,
  `$tag$…$tag$` dollar-quoted strings (including **non-ASCII tags**, e.g. `$café$…$café$`),
  the `::type` cast operator (including greedy runs of three or more colons), and the
  `??` jsonb operator.
- **`lastInsertId()`.** PostgreSQL has no rowid; `lastInsertId()` returns the session's
  last sequence value (`lastval()`), or `lastInsertId($sequence)` returns
  `currval($sequence)`. Use a `SERIAL`/`IDENTITY` column or `RETURNING`.
- **Types.** `integer`/`bigint` → int, `real`/`double precision` → float, `boolean` →
  `0`/`1`, text types → string, `NULL` → null. The rich types are returned as their text
  representation: `numeric`/`decimal` (scale preserved), `date` / `time` / `timestamp` /
  `timestamptz`, `uuid`, and `json`/`jsonb`. The same values bind as parameters (text is
  coerced to the column type). `bytea` is returned as a PHP string with embedded NUL
  bytes preserved. `json` / `jsonb` are re-serialized compactly, so whitespace may differ
  from the server's text output, but the value is equivalent. Other types (arrays, network
  types) are best read with an explicit `::text` cast.
- **`getColumnMeta()`** reports PostgreSQL's real per-column metadata, read off the
  prepared statement's column descriptors (so it is valid before any row is fetched):
  `native_type` (the server's `pg_type.typname`: `int4`, `bool`, `bytea`, `text`, …),
  `pdo_type` (php-src's OID switch exactly: `BOOL`→`PARAM_BOOL`, `INT2`/`INT4`/`INT8`→
  `PARAM_INT`, `BYTEA`/`OID`→`PARAM_LOB`, everything else `PARAM_STR`), `pgsql:oid`
  (`PQftype`), `pgsql:table_oid` (`PQftable`, emitted unconditionally — `0` is
  `InvalidOid`, the server's own answer for an expression/literal/aggregate column),
  `len` (`PQfsize`) and `precision` (`PQfmod`). The last two are **raw and
  counter-intuitive, exactly as in real PDO**: `len` is the type's fixed byte width
  (`int4` → 4, `uuid` → 16) and **`-1` for any VARLENA** (text, varchar, numeric, bytea,
  json, arrays) — a `VARCHAR(20)` reports `len` `-1`, not `20`; its declared 20 surfaces
  in `precision` as the **undecoded atttypmod** `24` (20 + VARHDRSZ), and `NUMERIC(10,2)`
  is `655366`. The `table` **name** is the one key left empty: it would need a `pg_class`
  catalog lookup this bridge does not perform.
- **`getNotify()`.** `getNotify(PDO::FETCH_ASSOC, $timeoutMs)` shapes a pending
  `LISTEN`/`NOTIFY` message as `["message" => $channel, "pid" => $pid, "payload" =>
  $payload]`; any other `$fetchMode` (the default) keeps the numerically-indexed
  `[$channel, $pid, $payload]` shape. Both return `[]` rather than `false` when no
  notification arrives within the timeout.
- **`COPY`.** `copyFromArray()` / `copyFromFile()` emit `COPY … FROM STDIN`;
  `copyToArray()` / `copyToFile()` emit `COPY … TO STDOUT`. The `$separator` is
  **truncated to its first byte** (PostgreSQL's COPY grammar admits only a one-byte
  delimiter, and php-src's builders dereference exactly one byte), so
  `copyFromArray(…, "::")` copies with `:` rather than failing — matching real PDO.
  `copyToArray()` distinguishes an empty table (`[]`) from a transport error (`false`).
- **Connect timeout.** A `connect_timeout` the DSN (or `PDO::ATTR_TIMEOUT`) supplies
  wins; when neither does, **30 s** is appended — php-src's own default. Without it the
  pure-Rust client has no application-level connect timeout and hangs for minutes on a
  black-holed host.

## MySQL / MariaDB notes

The MySQL driver behaves like the others, with a few database-specific points:

- **Placeholders.** MySQL uses positional `?` natively; PDO `:name` placeholders are
  rewritten to `?` at prepare time (a name reused in the statement binds the same value
  to each position), so you write the same portable SQL for either driver. As in PHP, a
  single statement uses either `?` or `:name`, not both. The scanner skips `--` / `#` /
  `/* */` comments and `'…'` / `"…"` / `` `…` `` quoted regions, and is handed the
  connection's **live `NO_BACKSLASH_ESCAPES`** state so its idea of where a string
  literal ends always agrees with the server's.
- **`lastInsertId()`.** Returns the last `AUTO_INCREMENT` value; the sequence-name
  argument (a PostgreSQL/Oracle concept) is ignored.
- **Transactions.** Wrap DML on transactional (InnoDB) tables. MySQL implicitly commits
  around DDL (`CREATE`/`DROP TABLE`, …), so a `beginTransaction()` cannot roll those back.
- **Types.** `INT`/`BIGINT`/`BOOLEAN` (a `TINYINT(1)`, so `0`/`1`) → int, `FLOAT`/`DOUBLE`
  → float, text types → string, `NULL` → null. The rich types are returned as their text
  representation: `DECIMAL` (scale preserved), `DATE`, `DATETIME` / `TIMESTAMP`, and
  `TIME`. The same values bind as parameters (text is coerced to the column type by the
  server). Binary and BLOB columns are returned as PHP strings with embedded NUL bytes
  preserved. A `BIGINT UNSIGNED` value above `PHP_INT_MAX` is returned as its exact
  decimal numeric string (matching PHP) rather than wrapping negative.
- **`getColumnMeta()`'s `native_type`** is MySQL's own wire-type name, as in php-src
  (`type_to_name_native`): an `INT` column is `"LONG"`, a `VARCHAR` is `"VAR_STRING"`, a
  `DECIMAL` is `"NEWDECIMAL"`, a `BLOB`/`TEXT` is `"BLOB"`. `pdo_type`, `len`,
  `precision` and `flags` still come from the generic storage-class derivation.
- **`Pdo\Mysql::ATTR_INIT_COMMAND`.** Passed as a constructor option, this SQL statement
  runs on the server immediately after authentication (e.g. `SET NAMES utf8mb4`).
- **`Pdo\Mysql::ATTR_FOUND_ROWS`.** Wired: negotiates `CLIENT_FOUND_ROWS` in the
  handshake, so an `UPDATE` writing a value a row already holds reports `rowCount()` 1
  (matched) rather than 0 (changed).
- **`charset` DSN key.** A `mysql:…;charset=utf8mb4` DSN key becomes its own `SET NAMES
  utf8mb4` statement at connect time, run before `ATTR_INIT_COMMAND` (so an explicit init
  command can still issue its own `SET NAMES`). Only plain identifier characters
  (`[A-Za-z0-9_]`) are honored; anything else is silently dropped.
- **`unix_socket` DSN key.** Honored **only** when the DSN names no host or names exactly
  `localhost` — php-src's own condition (a literal `strcmp("localhost", …)`), so
  `mysql:host=127.0.0.1;unix_socket=/tmp/mysql.sock` deliberately connects over TCP.
- **Connect timeout.** Defaults to **30 s** (php-src's `PDO::ATTR_TIMEOUT` default) when
  neither the DSN's `connect_timeout` key nor `ATTR_TIMEOUT` supplies one.

## TLS / encrypted connections

Both network drivers connect over TLS with [rustls](https://github.com/rustls/rustls).
SQLite is in-process and unaffected.

**PostgreSQL** — ships in the default build (ring provider, no aws-lc-rs). Configure it
with the usual libpq DSN keys:

```php
<?php
// Encrypt and verify the server against the bundled public trust roots:
$db = new PDO("pgsql:host=db.example.com;dbname=app;sslmode=require;user=u;password=p");

// Verify against a specific CA (e.g. a managed provider's root):
$db = new PDO(
    "pgsql:host=db.example.com;dbname=app;sslmode=verify-full;sslrootcert=/path/ca.pem;user=u;password=p"
);
```

- `sslmode`: `disable` (plaintext), `prefer` (the default — try TLS, allow plaintext),
  or `require`/`verify-ca`/`verify-full` (require TLS).
- `sslrootcert`: a PEM CA bundle to trust instead of the bundled webpki roots.
- `sslcert` + `sslkey`: a client certificate + private key for mutual TLS (both required
  together).

Unlike libpq's bare `require` (which encrypts without verifying), elephc always
validates the server certificate once TLS is negotiated — the safer default — so
`require`, `verify-ca`, and `verify-full` all verify against the trust roots. For a
server with a self-signed certificate, pass its CA via `sslrootcert`.

**MySQL / MariaDB** — opt-in at build time (`cargo build -p elephc-pdo --features
mysql-tls`; see [Limitations](#limitations) for why). Configure it with the
`Pdo\Mysql::ATTR_SSL_*` constructor options:

```php
<?php
$db = new PDO("mysql:host=db.example.com;dbname=app", "u", "p", [
    Pdo\Mysql::ATTR_SSL_CA   => "/path/ca.pem",   // trust this CA
    // Pdo\Mysql::ATTR_SSL_CERT => "/path/client.pem",  // mutual TLS (with SSL_KEY)
    // Pdo\Mysql::ATTR_SSL_KEY  => "/path/client.key",
    // Pdo\Mysql::ATTR_SSL_VERIFY_SERVER_CERT => false, // skip verification (insecure)
]);
```

- `ATTR_SSL_CA`: a PEM CA bundle to trust (in addition to the bundled webpki roots).
- `ATTR_SSL_CERT` + `ATTR_SSL_KEY`: client certificate + key for mutual TLS.
- `ATTR_SSL_VERIFY_SERVER_CERT`: `false` disables certificate and hostname checking.
- `ATTR_SSL_CAPATH` and `ATTR_SSL_CIPHER` have no rustls equivalent and are ignored.

Presence of any `ATTR_SSL_*` option enables TLS. In a build without `mysql-tls`,
requesting TLS raises a `PDOException` at connect time rather than silently falling back
to plaintext.

## Transactions

```php
<?php
$db->beginTransaction();
try {
    $db->exec("INSERT INTO users (name, score) VALUES ('Dee', 1.0)");
    $db->commit();
} catch (PDOException $e) {
    $db->rollBack();
}
```

Starting a nested transaction, or committing / rolling back with none active, throws a
`PDOException` regardless of the error mode. `__destruct` rolls back an open transaction
before closing.

`inTransaction()` (and `beginTransaction()`'s already-active guard) consult the
driver's **live** transaction state where one exists: for SQLite this is
`sqlite3_get_autocommit()`, so a transaction started by a raw `exec("BEGIN")` — bypassing
`beginTransaction()` — is seen too. Neither pure-Rust client this bridge uses exposes a
live transaction-status accessor for PostgreSQL/MySQL, so those fall back to the
`beginTransaction()`/`commit()`/`rollBack()` flag.

## Errors

The default error mode is `PDO::ERRMODE_EXCEPTION`: a failed `exec()`, `prepare()`, or
connection throws a `PDOException` (which extends `RuntimeException`).

```php
<?php
try {
    $db->exec("NOT VALID SQL");
} catch (PDOException $e) {
    echo $e->getMessage();
    echo $e->errorInfo[0];   // the SQLSTATE
}
```

`PDO::errorCode()` returns the 5-character `SQLSTATE` for the last operation (`"00000"`
on success) and `PDO::errorInfo()` returns `[SQLSTATE, driver-specific code, message]`,
with `["00000", null, null]` on success. Every driver surfaces a real `SQLSTATE`: SQLite
through a php-src-matching table, MySQL from the `ERR` packet's `#`-marked field, and
PostgreSQL from the `ErrorResponse` `C` field. `PDOStatement` tracks its own error state
through the same `errorCode()` / `errorInfo()` pair.

The error mode is configurable through `ATTR_ERRMODE`:

```php
<?php
$db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT);
$rows = $db->exec("UPDATE …");       // false on error instead of throwing
if ($db->exec("BAD SQL") === false) {
    echo $db->errorInfo()[2];
}
```

- `ERRMODE_EXCEPTION` (default) throws a `PDOException`.
- `ERRMODE_SILENT` suppresses it: `exec()`, `query()`, and `prepare()` all return `false`
  on error (check with `=== false`).
- `ERRMODE_WARNING` writes the message to `STDERR` and returns the same failure value as
  `SILENT`.

Every synthetic (non-driver) failure — HY093 bind errors, IM001 unsupported attributes,
HY000 fetch-mode errors, `nextRowset()` — is **errmode-aware** in the same way: it throws
under EXCEPTION, warns under WARNING, and is quiet under SILENT, always returning the
method's own failure value.

The mode can also be seeded from the constructor's options array: `new PDO($dsn, null,
null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT])`. Prepared statements inherit the
connection's current error mode when they are created.

### The bridge never aborts your program

Every `extern "C"` entry point of the native bridge runs its body inside a `catch_unwind`
panic firewall, and takes every handle-table lock through a poison-recovering helper. An
internal panic — an `unwrap` in the bridge, an unexpected panic out of the `postgres` /
`mysql` client crates — therefore degrades into the same well-defined failure sentinel the
entry point already promises for an unknown handle, which the prelude turns into a
catchable `PDOException`. Without the pair, the unwind out of a plain `extern "C"` function
would abort the whole compiled process, and one panic taken under a table lock would poison
that mutex and brick PDO for every later call in the process.

### `PDOException` shape

elephc's `PDOException` diverges from PHP's inherited `Exception` signature, and the
divergences are forced by elephc's type system rather than chosen:

- **The 2nd constructor parameter is `?array $errorInfo`, not `int $code`**:
  `new PDOException($message, $errorInfo, $previous)`. The inherited PHP form
  `new PDOException($msg, $code, $prev)` is a type error here.
- **`$e->errorInfo`** is a real `[SQLSTATE, driver-code, message]` array for a server
  error (so `$e->errorInfo[0]` works — which is what frameworks read) and `null` when
  there is no structured info, matching PHP.
- **`getCode()`** returns the **driver-specific integer code** (i.e. `errorInfo[1]`), not
  the `SQLSTATE` string PHP puts there — elephc's built-in `Exception::$code` is
  `int`-typed and cannot hold a 5-character SQLSTATE. Read the SQLSTATE from
  `$e->errorInfo[0]`.
- **`getPrevious()` always returns `null`**: every Throwable "standard method" call is
  intercepted in codegen before user-method dispatch, so an override would be dead code.
  The chain is exposed through a public property instead — `$e->previous` (elephc) ==
  `$e->getPrevious()` (PHP).

## Under `--web`

Each prefork worker holds its own connections: N workers means N independent SQLite
handles on the same database file, so concurrent writes contend. For a write-heavy
`--web` app, open the database in WAL mode and set a busy timeout so a contended write
waits instead of failing immediately:

```php
<?php
$db = new PDO("sqlite:/var/data/app.db", null, null, [PDO::ATTR_TIMEOUT => 5]);
$db->exec("PRAGMA journal_mode=WAL");
```

`ATTR_TIMEOUT` is expressed in seconds (mapped to SQLite's millisecond busy-timeout).
`ATTR_PERSISTENT` connections live in a per-worker pool keyed by DSN, so they persist
across requests handled by the same worker but are never shared across workers or across
a worker respawn. The bridge's connection and result state lives outside the per-request
PHP heap, so it is unaffected by the per-request heap reset the web runtime performs
between requests.

## Supported surface

- **`pdo_drivers(): array`** — the global, procedural spelling of
  `PDO::getAvailableDrivers()`. Known gap: the prelude is only injected for a program
  that *names* a PDO class, so a program whose only PDO reference is a bare
  `pdo_drivers()` call needs `--with-pdo` to force injection.
- **PDO**: `__construct`, `exec`, `query`, `prepare`, `quote`, `lastInsertId`,
  `beginTransaction`, `commit`, `rollBack`, `inTransaction`, `errorCode`, `errorInfo`,
  `getAttribute`, `setAttribute`, `getAvailableDrivers` (static), `connect` (static
  factory), `__destruct`. `clone $pdo` throws (PHP forbids it too), and
  `serialize($pdo)` throws `Exception: Serialization of 'PDO' is not allowed` —
  php-src marks the class `@not-serializable`, and without the guard elephc's
  property-walking `serialize()` would emit the raw bridge handle into the blob and
  hand back a zombie object on `unserialize()`.
- **PDOStatement**: `execute`, `bindValue`, `bindParam`, `bindColumn` (throws — see
  Limitations), `setFetchMode`, `fetch`, `fetchAll`, `fetchColumn`, `fetchObject`,
  `closeCursor`, `errorCode`, `errorInfo`, `rowCount`, `columnCount`, `getColumnMeta`,
  `getAttribute`, `setAttribute`, `nextRowset`, `debugDumpParams`, `getIterator`,
  `__destruct`, plus the public **`readonly`** `$queryString` property (the prepared
  SQL — it can never be overwritten; see "readonly `$queryString`" under Divergences for
  how the rejection surfaces). `fetch*()` on a statement that has not been
  `execute()`d (or after `closeCursor()`) returns `false` rather than stepping the query.
  `clone $stmt` and `serialize($stmt)` throw, like `PDO`'s.
  `new PDOStatement(...)` constructed directly throws a `PDOException` ("You should not
  create a PDOStatement manually").
- **`debugDumpParams()`** reproduces php-src's full line shapes (`SQL: [n] …`,
  `Params:  n`, then a `Key:`/`paramno=`/`name=`/`is_param=`/`param_type=` block per
  bind), including php-src's reported `param_type` (an `execute($params)` array stamps
  every element `PARAM_STR`, whatever the PHP value's type). `Sent SQL:` is correctly
  absent — php-src prints it only for an emulated prepare, and elephc never emulates.
  Two divergences: re-binding the same parameter prints two blocks (php-src's
  bound-params table is keyed, so it prints one), and a named parameter's `paramno` is
  the resolved slot from the start (php-src shows `-1` until the first `execute()`).
- **Constants**: the full PHP 8.4 set — fetch-mode (base modes plus the OR-able
  `FETCH_GROUP` / `FETCH_UNIQUE` / `FETCH_CLASSTYPE` / `FETCH_PROPS_LATE` / … flags),
  parameter (including `PARAM_STR_NATL` / `PARAM_STR_CHAR` / `PARAM_INPUT_OUTPUT`),
  cursor, case, null-handling, and `ATTR_*` constants, plus `ERR_NONE` (`"00000"`), the
  parameter-lifecycle `PARAM_EVT_*` constants (declared so code enumerating the class
  surface compiles — they are entirely inert here, since elephc's drivers are native Rust
  and expose no `param_hook` seam to PHP), and the **legacy `PDO::SQLITE_*` aliases**
  (`PDO::SQLITE_ATTR_OPEN_FLAGS`, `PDO::SQLITE_OPEN_*`, `PDO::SQLITE_DETERMINISTIC`,
  `PDO::SQLITE_ATTR_READONLY_STATEMENT`, `PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES`),
  which php-src registers on the base class alongside the 8.1+ class-scoped spellings.
- **Driver subclasses**: `Pdo\Sqlite`, `Pdo\Mysql`, and `Pdo\Pgsql` (PHP 8.4) extend
  `PDO` and inherit its full base surface, so `new \Pdo\Sqlite("sqlite::…")` works like
  `new \PDO(...)` and the instance is `instanceof \PDO`. A program that names only a
  subclass — never the base `PDO` — still injects the prelude, and `PDO::connect($dsn, …)`
  returns the matching subclass for the DSN's driver prefix (an unknown prefix throws).
  Each subclass declares its PHP 8.4 driver-specific constants.

  **Opening a foreign DSN through a subclass is rejected**: `new Pdo\Sqlite("mysql:…")`
  throws before any connection is attempted, matching php-src. (The *static* form,
  `Pdo\Sqlite::connect("mysql:…")`, is not — elephc has no late static binding, so an
  inherited static cannot see which subclass it was called through, and `connect()` still
  dispatches on the DSN prefix alone.)

  Driver methods: `Pdo\Pgsql::escapeIdentifier()`, `getPid()`, `lobCreate()` /
  `lobUnlink()` / `lobOpen()`, `copyFromArray()` / `copyFromFile()` / `copyToArray()` /
  `copyToFile()`, `getNotify()`, `setNoticeCallback()`; `Pdo\Mysql::getWarningCount()`;
  `Pdo\Sqlite::loadExtension()`, `openBlob()`, `createCollation()`, `createFunction()`,
  `createAggregate()`.

Connections and prepared statements release their underlying bridge resources
automatically through `__destruct`: a `PDO` closes its connection (finalizing any
remaining statements) and a `PDOStatement` finalizes itself when the object is released
— at the end of its scope, when its variable is reassigned or `unset()`, or at program
exit. You do not need to close them explicitly. A statement also **roots its owning
`PDO`**, so `return $db->query(...)` from inside a function whose local `$db` then goes
out of scope keeps the connection alive.

## SQLite user-defined functions and collations

`Pdo\Sqlite` runs compiled-PHP closures as SQLite callbacks:

- `createCollation(string $name, callable $comparator): bool` — registers a custom
  `COLLATE` ordering; `$comparator($a, $b)` returns `<0` / `0` / `>0`.
- `createFunction(string $function_name, callable $callback, int $num_args = -1, int $flags = 0): bool`
  — registers a scalar SQL function invoked once per row; `$flags` may be
  `Pdo\Sqlite::DETERMINISTIC`.
- `createAggregate(string $name, callable $step, callable $finalize, int $numArgs = -1): bool`
  — registers an aggregate: `$step($context, $rownumber, ...$values)` runs per row and
  returns the running accumulator (`null` before the first row), and
  `$finalize($context, $rownumber)` returns the group result.

```php
$db = new \Pdo\Sqlite("sqlite::memory:");
$db->createFunction("shout", fn($s) => strtoupper($s) . "!");
$db->createAggregate("joined",
    fn($acc, $n, $v) => $n === 0 ? $v : $acc . "," . $v,
    fn($acc, $n) => $acc);
echo $db->query("SELECT shout('hi')")->fetchColumn();          // HI!
```

Only closures and first-class callables are accepted — a string or `[$obj, 'method']`
callable is rejected at compile time. A callback that throws never crashes or unwinds
across SQLite's engine: the exception is caught at the C boundary and the statement fails
with a `PDOException` (a throwing *collation* comparator is instead treated as "equal",
since SQLite's comparison has no error channel). A UDF/aggregate returning a **non-scalar**
value (an array or object) yields SQL `NULL` silently. Each callback runs through elephc's
dynamic-dispatch path, which currently retains a small amount of heap per invocation, so a
callback applied across a very large result set accumulates memory until the program exits.
Registering a callback inside another SQLite callback on the same connection is not
supported — a nested query returns no rows rather than re-entering.

`Pdo\Pgsql::setNoticeCallback(callable $callback): void` registers a callback invoked with
the text of each PostgreSQL server `NOTICE` (e.g. from `RAISE NOTICE`):

```php
$pg = new \Pdo\Pgsql("pgsql:host=localhost;dbname=app");
$pg->setNoticeCallback(fn($msg) => error_log("PG NOTICE: $msg"));
$pg->exec("DO $$ BEGIN RAISE NOTICE 'migrated'; END $$");   // callback fires with "migrated"
```

Two divergences from PHP: the parameter is a non-nullable `callable` (to stop delivery,
register a no-op closure rather than passing `null`), and delivery is **poll-based** — the
driver buffers notices as they arrive and dispatches them right after each `exec()` /
`query()` on the connection, so a `NOTICE` raised by a prepared-statement `execute()` is
delivered on the next `exec()`/`query()`.

## Divergences from php-src

These are behavioral differences you can *observe* — not missing features. Read them
before assuming php-src semantics.

### MySQL is always NATIVE-prepared; php-src emulates by default

This is the single largest divergence, and it is easy to trip over because it inverts a
default:

- **php-src's `pdo_mysql` defaults to client-side prepare EMULATION** (`ATTR_EMULATE_PREPARES
  = true`). elephc **always** uses real server-side prepares. elephc's default therefore
  equals php-src's *opt-out*, not its default.
- `PDO::ATTR_EMULATE_PREPARES` is **inert**: it is stored and read back (including from
  `PDOStatement::getAttribute()`), but there is no emulated code path to switch to.
- There is **no automatic native→emulated fallback** on MySQL **errno 1295** ("this
  command is not supported in the prepared statement protocol yet"). A multi-statement
  `prepare()`/`query()`, and admin statements such as `USE`, `LOCK TABLES` and some
  `SHOW` forms, therefore **fail** here where real PHP quietly emulates them.
- php-src's **emulated-mode bind-COUNT `HY093` pre-check** (raised client-side when the
  number of bound parameters does not match the number of placeholders) **does not exist
  here**. elephc raises HY093 for an *unresolvable* placeholder and for an out-of-range
  slot, from the driver's own answer — not from a client-side count comparison.
- `CALL` behaves like a genuine prepared `CALL` rather than an emulated one.

### Other divergences

- **PostgreSQL `boolean` and `bytea` do not map to PHP's native representations.** A
  `boolean` column returns the integer `0`/`1` rather than a real `bool`, so
  `$row['flag'] === true` is always `false` — compare with `== true` or cast with
  `(bool) $row['flag']`. A `bytea` column returns a plain PHP string (embedded NUL bytes
  preserved) rather than a stream resource, so `fread()` / `fseek()` do not apply. Both are
  deliberate, ubiquitous divergences from php-src's `pdo_pgsql`, not bugs.
- **`errorCode()` before the first operation** returns `"00000"` rather than PHP's `null`
  (the bridge reports a fresh handle as success).
- **readonly `$queryString`: the write is always rejected, but it is not always *loud*.**
  php-src throws an `Error` on `$stmt->queryString = 'x'`. elephc declares the property
  `readonly`, so the prepared SQL can never be overwritten — but *how* the rejection
  surfaces depends on the receiver's static type:

  ```php
  $stmt = $db->prepare("SELECT 1");   // static type: PDOStatement|bool
  $stmt->queryString = "DROP TABLE t";        // silently ignored — NO Error, value kept
  echo $stmt->queryString;                    // "SELECT 1"

  if ($stmt instanceof PDOStatement) {        // narrowed to a concrete PDOStatement
      $stmt->queryString = "DROP TABLE t";    // catchable Error, as in php
  }
  ```

  The silent case is a **compiler limitation, not a PDO one**: a `readonly` write through a
  union-typed receiver is not checked (it reproduces on any user class whose factory
  returns `Box|bool`), and `prepare()`/`query()` return `PDOStatement|bool`. The message in
  the narrowed case is PHP's generic readonly text, not php-src's custom "Property
  queryString is read only" — same class (`Error`), same catchability.
- **`FETCH_GROUP` integer-looking keys stay strings** (see the FETCH_GROUP section).
- **`FETCH_CLASS` / `FETCH_INTO` hydration is always constructor-first.** php-src's
  *default* is properties-BEFORE-constructor (`FETCH_PROPS_LATE` is the flag that asks for
  constructor-first). So elephc's default equals php-src's `FETCH_PROPS_LATE`, and passing
  `FETCH_PROPS_LATE` explicitly is accepted and changes nothing. A constructor that
  overwrites a property will therefore clobber the fetched column here.
- **`nextRowset()`** raises `IM001` ("driver does not support multiple rowsets") for SQLite
  and PostgreSQL statements — errmode-aware, like every other statement failure — instead
  of silently returning `false`. A `mysql:` statement returns `false` *without* raising:
  MySQL genuinely supports more rowsets over the wire, but this bridge only materializes
  the first one per prepared statement, so there is no second rowset to advance to and no
  "driver can't do this" to report.
- **`PDO::connect()` on a subclass** does not reject a mismatched DSN (no late static
  binding) — the `new Pdo\Sqlite("mysql:…")` spelling does.
- **The placeholder scanners diverge from php-src only on already-malformed SQL.** An
  unterminated string literal or unterminated `/* … */` comment consumes to end-of-input
  (php-src's scanner instead backtracks and treats the opening quote / `/*` as a lone
  character), so a `?` appearing after an unbalanced quote or comment is not recognized as
  a placeholder. Well-formed SQL matches php-src exactly.
- **UPSTREAM php-src bug, deliberately NOT reproduced.** php-src 8.4's `copyToArray()` /
  `copyToFile()` build `COPY … TO STDIN` (`pgsql_driver.c:882,884,973,975`) — an invalid
  direction in PostgreSQL's COPY grammar. elephc correctly emits `TO STDOUT`. Do not "fix"
  this to match php-src.

## Limitations

### Not implemented (fails loudly)

- **Other PDO drivers.** Only SQLite, PostgreSQL, and MySQL / MariaDB; the bridge is
  structured to add more behind the same prelude.
- **`bindColumn()`** throws a `PDOException` ("not supported") after validating its
  arguments. PHP stores a *reference* to the variable and writes each fetched column into
  it on every `fetch(PDO::FETCH_BOUND)`; capturing that escaping reference needs
  `$this->boundColumns[$c] = &$var;`, which does not parse in elephc. `FETCH_BOUND` itself
  still advances the cursor and reports whether a row was available.
- **`FETCH_LAZY`** — elephc has no `PDORow` class (a lazily-materializing row object needs
  a `__get` that reaches back into a live cursor), so `fetch(PDO::FETCH_LAZY)` throws
  rather than substituting an eagerly-built row of some other shape. (php-src allows LAZY
  in `fetch()` and forbids it in `fetchAll()`; elephc forbids it in both.)
- **`FETCH_FUNC`** throws a `PDOException`: the callback would have to arrive through
  `fetchAll()`'s `mixed` second parameter, and elephc's checker refuses to invoke a Mixed
  value or pass one to a `callable`-typed parameter.
- **`Pdo\Sqlite::setAuthorizer()`**, the legacy `sqliteCreateFunction()` /
  `sqliteCreateAggregate()` / `sqliteCreateCollation()` aliases, `php.ini`-based DSN
  aliasing (`pdo.dsn.*`), and the `#[\SensitiveParameter]` attribute.
- **`PDO::PARAM_STR_NATL`** (the national-character-set string bind) and
  **`PDO::ATTR_DEFAULT_STR_PARAM`**: the constants exist but neither is implemented. There
  is no `_utf8`/`N''` introducer path, so `PARAM_STR|PARAM_STR_NATL` masks its flag off and
  binds as a plain `PARAM_STR` string, and `ATTR_DEFAULT_STR_PARAM` is stored and ignored.

### Silently inert (accepted, no effect)

- **`Pdo\Mysql::ATTR_LOCAL_INFILE` is inert — and this is a correctness trap, not a
  cosmetic no-op.** The bridge installs no local-infile handler, so the `mysql` client
  answers the server's file request with an **empty packet**: `LOAD DATA LOCAL INFILE`
  **loads ZERO ROWS and reports NO ERROR**. A data-loading script ported to elephc will
  appear to succeed and import nothing. Use `COPY` (PostgreSQL), a server-side
  `LOAD DATA INFILE`, or batched `INSERT`s instead. `ATTR_LOCAL_INFILE_DIRECTORY` is
  likewise inert.
- **Inert `Pdo\Mysql::ATTR_*` constants** (declared, stored, read back, no effect):
  `ATTR_USE_BUFFERED_QUERY` (1000), `ATTR_LOCAL_INFILE` (1001, see above), `ATTR_COMPRESS`
  (1003), `ATTR_DIRECT_QUERY` (1004), `ATTR_IGNORE_SPACE` (1006), `ATTR_SSL_CAPATH` (1010),
  `ATTR_SSL_CIPHER` (1011), `ATTR_SERVER_PUBLIC_KEY` (1012), `ATTR_MULTI_STATEMENTS`
  (1013), `ATTR_LOCAL_INFILE_DIRECTORY` (1015). The **wired** ones are `ATTR_INIT_COMMAND`
  (1002), `ATTR_FOUND_ROWS` (1005), and `ATTR_SSL_KEY`/`ATTR_SSL_CERT`/`ATTR_SSL_CA`/
  `ATTR_SSL_VERIFY_SERVER_CERT` (1007/1008/1009/1014).
- **`Pdo\Pgsql::ATTR_DISABLE_PREPARES` is inert**, and "inert" undersells it. Callers reach
  for it precisely because **some statements cannot be server-side PREPAREd at all** —
  multi-command strings (`BEGIN; …; COMMIT;` in one call), certain admin/utility statements,
  and anything the extended-query protocol refuses. In real PHP this attribute switches
  those to a simple-query execute-only path; elephc has no such path, so those statements
  simply **fail** here. Split them into separate `exec()` calls.
- **`Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE`** is inert.
- **`PDO::ATTR_SERVER_INFO`** is always `null` (unless a caller has explicitly
  `setAttribute()`'d a value for it). php-src only answers this for MySQL, from mysqlnd's
  live `mysql_stat()` admin string; neither the `mysql` crate nor `mysql_common` this bridge
  links exposes a `mysql_stat()`/`COM_STATISTICS` accessor.
- **`PDO::ATTR_CURSOR` / `CURSOR_SCROLL`** — every cursor is forward-only, so `fetch()`'s
  `$cursorOrientation` / `$cursorOffset` are accepted and ignored. On a real
  `CURSOR_SCROLL` statement php-src would honor `FETCH_ORI_FIRST`/`LAST`/`PRIOR`/`ABS`/`REL`.
- **`ATTR_STATEMENT_CLASS`**, `ATTR_AUTOCOMMIT`, `ATTR_PREFETCH`, `ATTR_MAX_COLUMN_LEN`,
  `ATTR_FETCH_TABLE_NAMES`, `ATTR_FETCH_CATALOG_NAMES`, `ATTR_CURSOR_NAME` — stored and read
  back, no effect.

### The PostgreSQL DSN allow-list silently DROPS real libpq keys

`tokio-postgres`'s connection-string parser hard-fails with `UnknownOption` on any key it
does not know, which would turn a perfectly good libpq DSN into a connection that never
happens. The bridge therefore forwards **only** the keys that parser accepts and
**silently drops the rest**. Forwarded: `user`, `password`, `dbname`, `options`,
`application_name`, `sslnegotiation`, `host`, `hostaddr`, `port`, `connect_timeout`,
`tcp_user_timeout`, `keepalives`, `keepalives_idle`, `keepalives_interval`,
`keepalives_retries`, `target_session_attrs`, `channel_binding`, `load_balance_hosts`
— plus `sslmode` / `sslrootcert` / `sslcert` / `sslkey`, which are consumed separately and
applied to the rustls connector.

**Dropped without a word** (present in a libpq DSN, ignored here): `service`, `passfile`,
`client_encoding`, `gssencmode`, `krbsrvname`, `gsslib`, `requiressl`, `sslcrl`,
`sslcrldir`, `sslpassword`, `sslsni`, `sslcompression`, `ssl_min_protocol_version`,
`ssl_max_protocol_version`, `replication`, `fallback_application_name`. The connection
still opens — just without whatever that key would have configured. If your DSN relies on
one of these (a `service` file, a non-UTF-8 `client_encoding`, a CRL), it will not do what
you expect.

### Resource and lifetime caveats

- **Persistent connections are reused with NO liveness check.** The pool hands back the
  pooled handle if it is still registered; it does not ping the server. A connection the
  server has since closed (idle timeout, restart, failover) is handed back dead, and the
  failure surfaces on the next statement rather than at `new PDO(...)`.
- **PostgreSQL and MySQL results are FULLY BUFFERED client-side** at the first `step()`:
  the driver materializes every row of the result set into memory before the first
  `fetch()` returns. A `SELECT` over a very large table is a memory cliff — there is no
  streaming/unbuffered mode (`Pdo\Mysql::ATTR_USE_BUFFERED_QUERY` is inert). Paginate, or
  push the aggregation into SQL. SQLite steps its cursor lazily and is unaffected.
- **`openBlob()` / `lobOpen()` are read-whole**: the entire BLOB / large object is read
  (NUL bytes preserved) into a rewound `php://memory` stream. Reads work fully; **writes
  are not flushed back to storage**, and the `$flags` / `$mode` argument is accepted only
  for signature compatibility. Both return `false` on a missing row/OID.
- **`Pdo\Sqlite::loadExtension()`** runs native code from the named library, weakening the
  standalone-binary guarantee. An empty name is a `ValueError`.
- **`Pdo\Mysql::getWarningCount()`** reflects a preceding direct `exec()`/DML statement;
  the pure-Rust client does not surface a SELECT's EOF-packet warnings.
- **SQLite bridge threadsafety invariant.** The bridge keeps its connection table and its
  statement table under **two separate mutexes**, so two overlapping calls can touch the
  same `sqlite3*` — which is only defined under a **serialized** SQLite build
  (`SQLITE_THREADSAFE=1`). The bundled amalgamation is built that way and
  `assert_sqlite_threadsafe()` pins the invariant at the first open (it panics rather than
  corrupting memory if a future build ever flips it). Do not link a
  `SQLITE_THREADSAFE=0` amalgamation on a threaded target.
- **MySQL TLS is opt-in at build time.** PostgreSQL TLS ships in the default build; MySQL /
  MariaDB TLS is behind the `mysql-tls` Cargo feature. The reason is dependency hygiene: the
  `mysql` crate's rustls backend pulls rustls with its default `aws-lc-rs` provider (a C/asm
  library needing a build toolchain), whereas the rest of elephc's TLS — PostgreSQL, `ssl://`
  streams — uses the pure-Rust `ring` provider and stays musl-friendly. Building without
  `mysql-tls` keeps every PDO binary aws-lc-free; a MySQL connection that requests TLS then
  fails loudly rather than silently downgrading to plaintext. Rebuild with `cargo build -p
  elephc-pdo --features mysql-tls` to reach a MySQL server that *mandates* TLS (AWS
  RDS/Aurora, PlanetScale, …).

### Compiler limitations that shape this surface

These are elephc compiler gaps, not PDO design choices. They are worth knowing because
they are what you will actually hit:

- **No by-reference `bindParam(&$var)` deferred read.** `$this->prop = &$var;` does not
  parse, so `bindParam()` records the variable's *current* value. Bind immediately before
  `execute()`, or use `bindValue()`. (Same root cause as `bindColumn()`'s unsupported
  write-back.)
- **No constructor args through `query()` / `fetchAll()` / `fetchObject()`.** PHP's real
  signatures take a heterogeneous variadic tail (`mixed ...$args`); elephc's checker
  mis-derives both the minimum arity and the variadic element type for a variadic tail on a
  *method* with a leading non-variadic parameter. So `query()` accepts up to two extra
  arguments (the 2nd is forwarded to `setFetchMode()`, the 3rd is not), and
  `fetchAll(PDO::FETCH_CLASS, 'Row', [$a, $b])` / `fetchObject('Row', [$a, $b])` accept the
  constructor-argument array but **do not forward it** — the target class is always built
  with no arguments.
- **`FETCH_CLASS` / `FETCH_INTO` target classes should declare TYPED properties.**
  Populating a class whose properties are *untyped* (`public $id;`) can corrupt a column
  whose value type differs from another column's (a compiler limitation in dynamically-named
  property writes); declare them `public mixed $id;` (or a concrete type) and every column
  populates correctly. `FETCH_OBJ` (`stdClass`) is unaffected.
- **Hydration is constructor-first** (see Divergences) because that is the order the
  dynamic-property write path supports.
- **`FETCH_GROUP` integer-looking keys stay strings** because elephc's arrays do not fold a
  numeric string key back to an int key.
- **`prepare()`'s `$options` array is accepted and ignored** — iterating it inside the
  method body trips a known EIR miscompile, and no supported prepare option has a
  behavioral effect anyway.
