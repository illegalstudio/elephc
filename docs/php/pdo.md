---
title: "PDO (Databases)"
description: "PDO database access with the SQLite, PostgreSQL, and MySQL/MariaDB drivers: connections, prepared statements, fetch modes, and transactions."
sidebar:
  order: 17
---

elephc supports a practical subset of PHP's PDO database layer, with the
**SQLite**, **PostgreSQL**, and **MySQL / MariaDB** drivers. `PDO`,
`PDOStatement`, and `PDOException` behave like their PHP counterparts for everyday
use: connect, execute, prepare/bind, fetch, and run transactions. The DSN prefix
selects the driver, so the same code works against any of the databases.

Every driver is linked statically (SQLite is bundled; PostgreSQL and MySQL use
pure-Rust clients), so a compiled PDO binary has **no system database-client
dependency** — it runs anywhere the elephc binary runs. SQLite runs in-process;
PostgreSQL and MySQL connect to a running server over the network.

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

The DSN must start with `sqlite:`, `pgsql:`, or `mysql:`. For SQLite, the
`$username` and `$password` arguments are accepted for signature compatibility
but ignored; constructor options still seed PDO attributes. For PostgreSQL and
MySQL, `$username` / `$password` are folded into the connection (other keys like
`host`, `port`, `dbname`, and — for MySQL — `unix_socket` come from the
`key=value;…` DSN). A failed connection throws a `PDOException`.

Constructor options may include `PDO::ATTR_PERSISTENT => true`. Persistent PDO
instances use a process-local pool keyed by the fully materialized DSN, so a later
PDO constructed with the same DSN and persistent option reuses the existing
connection inside the same compiled program. Non-persistent connections are
opened independently.

## Executing statements

```php
<?php
// exec() runs a statement with no result set and returns the affected row count.
$db->exec("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
$n = $db->exec("INSERT INTO users (name, score) VALUES ('Ada', 9.5)");

echo $db->lastInsertId();   // "1"
```

## Prepared statements and binding

`execute()` accepts an array of parameters. Positional (`?`) placeholders bind by
position; named (`:name`) placeholders bind by key (with or without the leading
colon). Bound values are typed automatically (int, float, string, null, bool).

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

`query()` prepares and immediately executes a statement, returning the
`PDOStatement` ready to fetch.

Parameters can also be bound individually with `bindValue()` (and `bindParam()`),
then applied by an argument-less `execute()`:

```php
<?php
$stmt = $db->prepare("INSERT INTO users (name, score) VALUES (:name, :score)");
$stmt->bindValue(":name", "Dee");
$stmt->bindValue(":score", 5, PDO::PARAM_INT);
$stmt->execute();
```

`bindParam()` binds the variable's *current* value (it does not defer a
by-reference read to `execute()` time), so bind immediately before `execute()`.

Prefer prepared statements over interpolation. When you must embed a string,
`PDO::quote()` wraps it in single quotes and escapes embedded quotes:

```php
<?php
$db->quote("O'Brien");  // 'O''Brien'
```

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

$row = $db->query("SELECT id, name FROM users")->fetch(PDO::FETCH_CLASS, UserRow::class);

$target = new UserRow();
$same = $db->query("SELECT id, name FROM users")->fetch(PDO::FETCH_INTO, $target);

$all = $db->query("SELECT id FROM users")->fetchAll(PDO::FETCH_NUM);
$one = $db->query("SELECT name FROM users")->fetchColumn();  // first column of next row

// FETCH_COLUMN yields one column per row as a scalar:
$ids = $db->query("SELECT id FROM users")->fetchAll(PDO::FETCH_COLUMN);  // [1, 2, …]
```

`fetch()` returns `false` when the result set is exhausted. `FETCH_OBJ` creates a
real `stdClass` and assigns dynamic properties directly, including numeric column
names such as `"0"`. `FETCH_CLASS` creates the requested class and assigns column
values to matching declared or dynamic properties; `FETCH_INTO` fills and returns
the object instance passed as the second argument.

Column values are returned with their native scalar shape: integer → int, real /
floating point → float, text → string, binary/BLOB/`bytea` → string with embedded
NUL bytes preserved, and `NULL` → null. `FETCH_BOTH` is the default mode.

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

The cursor is forward-only: each row is consumed as it is yielded, so a statement
can be iterated once.

## PostgreSQL notes

The PostgreSQL driver behaves like the SQLite one, with a few database-specific
points:

- **Placeholders.** PDO `?` and `:name` placeholders are translated to
  PostgreSQL's native `$1, $2, …` at prepare time, so you write the same
  portable SQL for either driver.
- **`lastInsertId()`.** PostgreSQL has no rowid; `lastInsertId()` returns the
  session's last sequence value (`lastval()`), or `lastInsertId($sequence)`
  returns `currval($sequence)`. Use a `SERIAL`/`IDENTITY` column or `RETURNING`.
- **Types.** `integer`/`bigint` → int, `real`/`double precision` → float,
  `boolean` → `0`/`1`, text types → string, `NULL` → null. The rich types are
  returned as their text representation: `numeric`/`decimal` (scale preserved),
  `date` / `time` / `timestamp` / `timestamptz`, `uuid`, and `json`/`jsonb`. The
  same values bind as parameters (text is coerced to the column type). `bytea`
  is returned as a PHP string with embedded NUL bytes preserved. `json` / `jsonb`
  are re-serialized compactly, so whitespace may differ from the server's text
  output, but the value is equivalent. Other types (arrays, network types) are
  best read with an explicit `::text` cast.

## MySQL / MariaDB notes

The MySQL driver behaves like the others, with a few database-specific points:

- **Placeholders.** MySQL uses positional `?` natively; PDO `:name` placeholders
  are rewritten to `?` at prepare time (a name reused in the statement binds the
  same value to each position), so you write the same portable SQL for either
  driver. As in PHP, a single statement uses either `?` or `:name`, not both.
- **`lastInsertId()`.** Returns the last `AUTO_INCREMENT` value; the sequence-name
  argument (a PostgreSQL/Oracle concept) is ignored.
- **Transactions.** Wrap DML on transactional (InnoDB) tables. MySQL implicitly
  commits around DDL (`CREATE`/`DROP TABLE`, …), so a `beginTransaction()` cannot
  roll those back.
- **Types.** `INT`/`BIGINT`/`BOOLEAN` (a `TINYINT(1)`, so `0`/`1`) → int,
  `FLOAT`/`DOUBLE` → float, text types → string, `NULL` → null. The rich types are
  returned as their text representation: `DECIMAL` (scale preserved), `DATE`,
  `DATETIME` / `TIMESTAMP`, and `TIME`. The same values bind as parameters (text
  is coerced to the column type by the server). Binary and BLOB columns are
  returned as PHP strings with embedded NUL bytes preserved.
- **Driver name.** `getAttribute(PDO::ATTR_DRIVER_NAME)` reports `"mysql"`.

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

## Errors

The default error mode is `PDO::ERRMODE_EXCEPTION`: a failed `exec()`, `prepare()`,
or connection throws a `PDOException` (which extends `RuntimeException`).

```php
<?php
try {
    $db->exec("NOT VALID SQL");
} catch (PDOException $e) {
    echo $e->getMessage();
}
```

`PDO::errorCode()` returns the 5-character `SQLSTATE` for the last operation
(`"00000"` on success) and `PDO::errorInfo()` returns
`[SQLSTATE, driver-specific code, message]`, with `["00000", null, null]` on
success. Every driver surfaces a real `SQLSTATE`: SQLite through a
php-src-matching table, MySQL from the `ERR` packet's `#`-marked field, and
PostgreSQL from the `ErrorResponse` `C` field. `PDOStatement` tracks its own
error state through the same `errorCode()` / `errorInfo()` pair, and a thrown
`PDOException` carries the triple on its public `$errorInfo` property — read as
`$e->errorInfo[0]` for the `SQLSTATE` (see Limitations for the `getCode()`
divergence).

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
- `ERRMODE_SILENT` suppresses it: `exec()`, `query()`, and `prepare()` all return
  `false` on error (check with `=== false`).
- `ERRMODE_WARNING` writes the message to `STDERR` and returns the same failure
  value as `SILENT`.

The mode can also be seeded from the constructor's options array:
`new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_SILENT])`.
Prepared statements inherit the connection's current error mode when they are
created. `getAttribute()` reads attributes back; `ATTR_DRIVER_NAME` reports the
active driver (`"sqlite"`, `"pgsql"`, or `"mysql"`). `ATTR_PERSISTENT` can be set
in the constructor options to use the process-local DSN pool; setting it later
with `setAttribute()` updates the reported attribute but does not reopen an
already-created connection. Persistent connections are local to the running
native process; there is no cross-process pool.

## Under `--web`

Each prefork worker holds its own connections: N workers means N independent
SQLite handles on the same database file, so concurrent writes contend. For a
write-heavy `--web` app, open the database in WAL mode and set a busy timeout so a
contended write waits instead of failing immediately:

```php
<?php
$db = new PDO("sqlite:/var/data/app.db", null, null, [PDO::ATTR_TIMEOUT => 5]);
$db->exec("PRAGMA journal_mode=WAL");
```

`ATTR_TIMEOUT` is expressed in seconds (mapped to SQLite's millisecond
busy-timeout). `ATTR_PERSISTENT` connections live in a per-worker pool keyed by
DSN, so they persist across requests handled by the same worker but are never
shared across workers or across a worker respawn. The bridge's connection and
result state lives outside the per-request PHP heap, so it is unaffected by the
per-request heap reset the web runtime performs between requests.

## Supported surface

- **PDO**: `__construct`, `exec`, `query`, `prepare`, `quote`, `lastInsertId`,
  `beginTransaction`, `commit`, `rollBack`, `inTransaction`, `errorCode`,
  `errorInfo`, `getAttribute`, `setAttribute`, `getAvailableDrivers` (static),
  `connect` (static factory), `__destruct`. Starting a nested transaction, or committing / rolling back with
  none active, throws a `PDOException`; `__destruct` rolls back an open
  transaction before closing.
- **PDOStatement**: `execute`, `bindValue`, `bindParam`, `setFetchMode`, `fetch`,
  `fetchAll`, `fetchColumn`, `fetchObject`, `closeCursor`, `errorCode`,
  `errorInfo`, `rowCount`, `columnCount`, `getColumnMeta`, `getAttribute`,
  `setAttribute`, `nextRowset`, `debugDumpParams`, `__destruct`, plus the public
  `$queryString` property (the prepared SQL); Traversable, so a statement can be
  walked with `foreach`. `fetch*()` on a statement that has not been `execute()`d
  (or after `closeCursor()`) returns `false` rather than stepping the query.

Connections and prepared statements release their underlying bridge resources
automatically through `__destruct`: a `PDO` closes its connection (finalizing any
remaining statements) and a `PDOStatement` finalizes itself when the object is
released — at the end of its scope, when its variable is reassigned or `unset()`,
or at program exit. You do not need to close them explicitly.
- **Fetch modes**: `FETCH_ASSOC`, `FETCH_NUM`, `FETCH_BOTH`, `FETCH_OBJ`,
  `FETCH_COLUMN` (a single column as a scalar; the column index is the second
  argument to `setFetchMode(PDO::FETCH_COLUMN, $col)`), `FETCH_CLASS`,
  `FETCH_INTO`, and `FETCH_KEY_PAIR` (a two-column result as a `[col0 => col1]`
  map). `ATTR_DEFAULT_FETCH_MODE` sets the mode used when `fetch()` is called with
  no argument. Unsupported modes (`FETCH_LAZY`, `FETCH_GROUP`, `FETCH_UNIQUE`)
  fail loudly with a `PDOException` rather than silently returning wrong data.
- **Parameters**: positional `?` and named `:name` (the leading `:` is optional in
  the `execute([...])` array); `PARAM_INT` / `PARAM_STR` / `PARAM_NULL` /
  `PARAM_BOOL` constants.
- **Constants**: the full PHP 8.4 set — fetch-mode (base modes plus the OR-able
  `FETCH_GROUP` / `FETCH_UNIQUE` / `FETCH_PROPS_LATE` / … flags), parameter,
  cursor, case, null-handling, and `ATTR_*` constants (including `ATTR_TIMEOUT`,
  `ATTR_DEFAULT_FETCH_MODE`, `ATTR_STRINGIFY_FETCHES`, `ATTR_EMULATE_PREPARES`),
  plus `ERR_NONE` (`"00000"`).
- **Driver subclasses**: `Pdo\Sqlite`, `Pdo\Mysql`, and `Pdo\Pgsql` (PHP 8.4)
  extend `PDO` and inherit its full base surface, so `new \Pdo\Sqlite("sqlite::…")`
  works like `new \PDO(...)` and the instance is `instanceof \PDO`. A program that
  names only a subclass — never the base `PDO` — still injects the prelude, and the
  PHP 8.4 `PDO::connect($dsn, …)` factory returns the matching subclass for the
  DSN's driver prefix (an unknown prefix throws `PDOException`). Each subclass also
  declares its PHP 8.4 driver-specific constants (`Pdo\Sqlite::DETERMINISTIC` /
  `OPEN_*` / `ATTR_*`, `Pdo\Mysql::ATTR_*`, `Pdo\Pgsql::ATTR_*` / `TRANSACTION_*`).
  Driver-specific methods: `Pdo\Pgsql::escapeIdentifier()` (identifier quoting),
  `Pdo\Pgsql::getPid()` (backend process id), `Pdo\Mysql::getWarningCount()`
  (warnings from the last statement), `Pdo\Pgsql::lobCreate()` / `lobUnlink()`
  (large-object create/delete), `Pdo\Pgsql::copyFromArray()` / `copyFromFile()` /
  `copyToArray()` / `copyToFile()` (COPY), `Pdo\Sqlite::loadExtension()` (load a
  SQLite extension by path), `Pdo\Pgsql::getNotify()` (poll LISTEN/NOTIFY), and the
  stream-returning `Pdo\Sqlite::openBlob()` / `Pdo\Pgsql::lobOpen()` (read the whole
  BLOB / large object into a `php://memory` stream). The callback methods are not yet
  provided (see Limitations).

## Limitations

- **SQLite, PostgreSQL, and MySQL / MariaDB.** Other PDO drivers (Oracle, SQL
  Server, …) are not implemented; the bridge is structured to add more behind the
  same prelude.
- **`PDO::quote()`** is driver-aware: SQLite and PostgreSQL double single quotes
  (PostgreSQL switches to the `E'…'` form when a backslash is present) and MySQL
  backslash-escapes quotes, backslashes, and control bytes. Prepared statements
  remain the recommended path for every driver.
- **`PDOException::getCode()`** returns the base `Exception` integer code, not the
  `SQLSTATE` string PHP puts there — elephc's built-in `Exception::$code` is
  `int`-typed. Read the `SQLSTATE` from `$e->errorInfo[0]` (which frameworks do
  and which is always populated).
- **`errorCode()` before the first operation** returns `"00000"` rather than
  PHP's `null` (the bridge reports a fresh handle as success).
- **`fetch()`'s second argument** is a class/target (as with `setFetchMode`),
  not PHP 8.4's cursor-orientation parameter; forward-only cursors make the
  difference moot in practice.
- **Namespaced driver subclasses** `Pdo\Sqlite`, `Pdo\Mysql`, and `Pdo\Pgsql`
  (PHP 8.4) exist and extend `PDO`: they are auto-detected (a program that names
  only a subclass still injects the prelude), are directly instantiable, inherit
  the full base connection surface, are what `PDO::connect()` returns, and declare
  their driver-specific constants. Implemented driver methods:
  `Pdo\Pgsql::escapeIdentifier()`, `getPid()`, `lobCreate()` / `lobUnlink()`,
  `copyFromArray()` / `copyFromFile()` / `copyToArray()` / `copyToFile()`,
  `Pdo\Sqlite::loadExtension()`, `Pdo\Pgsql::getNotify()`,
  `Pdo\Mysql::getWarningCount()` (which reflects a preceding direct `exec()`/DML
  statement; the pure-Rust client does not surface a SELECT's EOF-packet warnings),
  and the stream-returning `Pdo\Sqlite::openBlob()` / `Pdo\Pgsql::lobOpen()`.
  `copyToArray()` returns an empty array both for an empty table and a transport
  error (check `errorInfo()`); `loadExtension()` runs native code from the named
  library, weakening the standalone-binary guarantee; `getNotify()` returns a
  numerically-indexed `[channel, pid, payload]` array (an empty array, not `false`,
  when none is pending — elephc's EIR array backend cannot mix a string-keyed and an
  empty array across one method's return paths). `openBlob()` / `lobOpen()` are
  **read-whole**: they read the entire BLOB / large object (NUL bytes preserved) into
  a rewound `php://memory` stream and return it (or `false` on a missing row/OID), so
  reads work fully but writes are not flushed back to storage, and the `$flags` /
  `$mode` argument is accepted only for signature compatibility. Still missing: the
  **callback** methods (`Pdo\Sqlite::createFunction` / `createAggregate` /
  `createCollation`, `Pdo\Pgsql::setNoticeCallback`), which need a PHP-callable-to-C
  trampoline elephc's FFI does not yet provide. `PDO::connect()` selects the subclass
  from the
  DSN prefix, so a subclass-qualified call with a mismatched DSN
  (`Pdo\Sqlite::connect("mysql:…")`) is not rejected as PHP would.
- **`FETCH_GROUP` / `FETCH_UNIQUE`** result shaping and **`createFunction()`** are
  not yet implemented — their constants exist but the behaviors either fail loudly
  or are absent.
- **`bindParam()`** binds the current value, not a deferred by-reference read.
- **`getAttribute` / `setAttribute`** act on `ATTR_ERRMODE`, `ATTR_DRIVER_NAME`,
  `ATTR_PERSISTENT`, `ATTR_TIMEOUT` (SQLite busy-timeout, in seconds),
  `ATTR_DEFAULT_FETCH_MODE`, and `ATTR_SERVER_VERSION`; other attributes are
  stored and read back but have no effect.
- Avoid `new PDOStatement(...)` directly — statements are created by `query()` /
  `prepare()`.
