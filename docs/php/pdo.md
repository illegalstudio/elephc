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
- **`getNotify()`.** `getNotify(PDO::FETCH_ASSOC, $timeoutMs)` shapes a pending
  `LISTEN`/`NOTIFY` message as `["message" => $channel, "pid" => $pid, "payload"
  => $payload]`; any other `$fetchMode` (the default) keeps the numerically-indexed
  `[$channel, $pid, $payload]` shape. Both return `[]` rather than `false` when no
  notification arrives within the timeout.
- **Credentials and `ATTR_TIMEOUT`.** The constructor's `$username`/`$password`
  are folded into the DSN as `user=`/`password=` only when the DSN does not
  already carry that key (a DSN-embedded credential wins, matching PHP).
  `PDO::ATTR_TIMEOUT` (seconds) is folded in the same way as a `connect_timeout`
  conninfo key, bounding the initial socket connect — not just the SQLite-style
  busy-wait `setAttribute()` applies after the connection already exists.

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
  returned as PHP strings with embedded NUL bytes preserved. A `BIGINT UNSIGNED`
  value above `PHP_INT_MAX` is returned as its exact decimal numeric string
  (matching PHP) rather than wrapping negative.
- **Driver name.** `getAttribute(PDO::ATTR_DRIVER_NAME)` reports `"mysql"`.
- **Credentials and `ATTR_TIMEOUT`.** Same DSN-precedence and connect-time
  timeout behavior as PostgreSQL's (see above): a DSN-embedded `user=`/`password=`
  wins over the constructor arguments, and `PDO::ATTR_TIMEOUT` bounds the initial
  TCP connect via a `connect_timeout` DSN key.
- **`Pdo\Mysql::ATTR_INIT_COMMAND`.** Passed as a constructor option, this SQL
  statement runs on the server immediately after authentication (e.g. `SET NAMES
  utf8mb4`). `ATTR_SSL_CA` / `ATTR_SSL_CERT` / `ATTR_SSL_KEY` /
  `ATTR_SSL_VERIFY_SERVER_CERT` drive TLS (see the TLS section below); the
  remaining `Pdo\Mysql::ATTR_*` constants (`ATTR_COMPRESS`, `ATTR_SSL_CAPATH`,
  `ATTR_SSL_CIPHER`, …) are accepted and stored but have no effect.
- **`charset` DSN key.** A `mysql:…;charset=utf8mb4` DSN key becomes its own `SET
  NAMES utf8mb4` statement at connect time, run before `ATTR_INIT_COMMAND` (so an
  explicit init command can still issue its own `SET NAMES` afterwards). Only
  plain identifier characters (`[A-Za-z0-9_]`) are honored; anything else is
  silently dropped.

## TLS / encrypted connections

Both network drivers connect over TLS with [rustls](https://github.com/rustls/rustls).
SQLite is in-process and unaffected.

**PostgreSQL** — ships in the default build (ring provider, no aws-lc-rs). Configure
it with the usual libpq DSN keys:

```php
<?php
// Encrypt and verify the server against the bundled public trust roots:
$db = new PDO("pgsql:host=db.example.com;dbname=app;sslmode=require;user=u;password=p");

// Verify against a specific CA (e.g. a managed provider's root):
$db = new PDO(
    "pgsql:host=db.example.com;dbname=app;sslmode=verify-full;sslrootcert=/path/ca.pem;user=u;password=p"
);
```

- `sslmode`: `disable` (plaintext), `prefer` (the default — try TLS, allow
  plaintext), or `require`/`verify-ca`/`verify-full` (require TLS).
- `sslrootcert`: a PEM CA bundle to trust instead of the bundled webpki roots.
- `sslcert` + `sslkey`: a client certificate + private key for mutual TLS (both
  required together).

Unlike libpq's bare `require` (which encrypts without verifying), elephc always
validates the server certificate once TLS is negotiated — the safer default — so
`require`, `verify-ca`, and `verify-full` all verify against the trust roots. For a
server with a self-signed certificate, pass its CA via `sslrootcert`.

**MySQL / MariaDB** — opt-in at build time (`cargo build -p elephc-pdo --features
mysql-tls`; see Limitations for why). Configure it with the `Pdo\Mysql::ATTR_SSL_*`
constructor options:

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
requesting TLS raises a `PDOException` at connect time rather than silently falling
back to plaintext.

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
  BLOB / large object into a `php://memory` stream), the SQLite user-callback methods
  `Pdo\Sqlite::createCollation()` / `createFunction()` / `createAggregate()` (see
  below), and `Pdo\Pgsql::setNoticeCallback()` (dispatch a callback for each server
  NOTICE, poll-based — see below).

## SQLite user-defined functions and collations

`Pdo\Sqlite` runs compiled-PHP closures as SQLite callbacks:

- `createCollation(string $name, callable $comparator): bool` — registers a custom
  `COLLATE` ordering; `$comparator($a, $b)` returns `<0` / `0` / `>0`.
- `createFunction(string $name, callable $callback, int $numArgs = -1, int $flags = 0): bool`
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
across SQLite's engine: the exception is caught at the C boundary and the statement
fails with a `PDOException` (a throwing *collation* comparator is instead treated as
"equal", since SQLite's comparison has no error channel). Each callback runs through
elephc's dynamic-dispatch path, which currently retains a small amount of heap per
invocation, so a callback applied across a very large result set accumulates memory
until the program exits. Registering a callback (any driver) inside another SQLite
callback on the same connection is not supported — a nested query returns no rows
rather than re-entering.

`Pdo\Pgsql::setNoticeCallback(callable $callback): void` registers a callback invoked
with the text of each PostgreSQL server `NOTICE` (e.g. from `RAISE NOTICE`):

```php
$pg = new \Pdo\Pgsql("pgsql:host=localhost;dbname=app");
$pg->setNoticeCallback(fn($msg) => error_log("PG NOTICE: $msg"));
$pg->exec("DO $$ BEGIN RAISE NOTICE 'migrated'; END $$");   // callback fires with "migrated"
```

Two divergences from PHP: the parameter is a non-nullable `callable` (to stop delivery,
register a no-op closure rather than passing `null`), and delivery is **poll-based** —
the driver buffers notices as they arrive and dispatches them right after each `exec()`
/ `query()` on the connection, so a `NOTICE` raised by a prepared-statement `execute()`
is delivered on the next `exec()`/`query()`.

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
- **MySQL TLS is opt-in at build time.** PostgreSQL TLS ships in the default build
  (see the TLS section above); MySQL/MariaDB TLS is behind the `mysql-tls` Cargo
  feature. The reason is dependency hygiene: the `mysql` crate's rustls backend
  pulls rustls with its default `aws-lc-rs` provider (a C/asm library needing a
  build toolchain), whereas the rest of elephc's TLS — PostgreSQL, `ssl://`
  streams — uses the pure-Rust `ring` provider and stays musl-friendly. Building
  without `mysql-tls` (the default) keeps every PDO binary aws-lc-free; a MySQL
  connection that requests TLS then fails loudly rather than silently downgrading
  to plaintext. Rebuild the bridge with `cargo build -p elephc-pdo --features
  mysql-tls` to reach a MySQL server that *mandates* TLS (AWS RDS/Aurora,
  PlanetScale, …).
- **`FETCH_CLASS` / `FETCH_INTO` target classes should declare typed properties.**
  Populating a class whose properties are *untyped* (`public $id;`) can corrupt a
  column whose value type differs from another column's (a compiler limitation in
  dynamically-named property writes); declare them `public mixed $id;` (or a concrete
  type) and every column populates correctly. `FETCH_OBJ` (`stdClass`) is unaffected.
- **`PDOException`'s second constructor argument** is `?array $errorInfo`
  (`new PDOException($message, $errorInfo)`), not PHP's `int $code` — the SQLSTATE
  triple lives in `errorInfo`, and `getCode()` is the base default (see above). A
  caught exception's `$e->errorInfo` is a real `[SQLSTATE, driver-code, message]`
  array for a server error (so `$e->errorInfo[0]` works) and `null` when there is no
  structured info (an unrecognized-driver connect failure), matching PHP.
- **`clone $pdo` / `clone $stmt` and a bare `fetch(PDO::FETCH_LAZY)`** are not
  supported: cloning a connection/statement is rejected (PHP forbids it too), and
  `FETCH_LAZY` — which returns PHP's lazy `PDORow`, a class elephc does not provide —
  fails loudly rather than returning a wrong-shaped row.
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
  numerically-indexed `[channel, pid, payload]` array by default, or the assoc
  `["message" => channel, "pid" => pid, "payload" => payload]` shape when called
  with `PDO::FETCH_ASSOC` (an empty array, not `false`, either way when none is
  pending). `openBlob()` / `lobOpen()` are
  **read-whole**: they read the entire BLOB / large object (NUL bytes preserved) into
  a rewound `php://memory` stream and return it (or `false` on a missing row/OID), so
  reads work fully but writes are not flushed back to storage, and the `$flags` /
  `$mode` argument is accepted only for signature compatibility. The SQLite
  user-callback methods are all implemented (see above): `Pdo\Sqlite::createCollation`
  / `createFunction` / `createAggregate`, and `Pdo\Pgsql::setNoticeCallback` (poll-based,
  with a non-nullable `callable` parameter). `PDO::connect()` selects the subclass
  from the
  DSN prefix, so a subclass-qualified call with a mismatched DSN
  (`Pdo\Sqlite::connect("mysql:…")`) is not rejected as PHP would.
- **`FETCH_GROUP` / `FETCH_UNIQUE`** result shaping is not yet implemented — the
  constants exist but the behavior fails loudly rather than reshaping the result.
- **`bindParam()`** binds the current value, not a deferred by-reference read.
- **`getAttribute` / `setAttribute`** act on `ATTR_ERRMODE`, `ATTR_DRIVER_NAME`,
  `ATTR_PERSISTENT`, `ATTR_TIMEOUT` (SQLite busy-timeout, in seconds),
  `ATTR_DEFAULT_FETCH_MODE`, `ATTR_SERVER_VERSION`, `ATTR_CASE` (folds fetched
  column-name keys upper/lowercase), `ATTR_ORACLE_NULLS` (folds NULL<->`""` in
  fetched scalar values), and `ATTR_STRINGIFY_FETCHES` (stringifies fetched
  INTEGER/FLOAT values); other attributes are stored and read back but have no
  effect.
- **`PDO::ATTR_SERVER_INFO`** is inert (always `null`, unless a caller has
  explicitly `setAttribute()`'d a value for it) for every driver. php-src itself
  only answers this for MySQL, from mysqlnd's live `mysql_stat()` admin string;
  neither the `mysql` crate nor `mysql_common` this bridge links exposes a
  `mysql_stat()`/`COM_STATISTICS` accessor to reproduce that.
- **PostgreSQL `boolean` and `bytea` do not map to PHP's native representations.**
  A `boolean` column returns the integer `0`/`1` rather than PHP's real `bool`, so
  `$row['flag'] === true` is always `false` — compare with `== true` or cast with
  `(bool) $row['flag']` instead. A `bytea` column returns a plain PHP string (with
  embedded NUL bytes preserved) rather than a stream resource, so `fread()` /
  `fseek()` do not apply to a fetched value — read it as a string directly. Both
  are deliberate, ubiquitous divergences from php-src's `pdo_pgsql`, not bugs.
- **`PDO::ATTR_EMULATE_PREPARES` is accepted but inert for MySQL — elephc always
  uses native server-side prepares**, whereas real PHP's `pdo_mysql` *emulates*
  prepares by default. Consequences: a multi-statement `prepare()`/`query()` is
  not supported, some admin statements (`USE`, `LOCK TABLES`, certain `SHOW`
  forms) fail with MySQL errno 1295 ("this command is not supported in the
  prepared statement protocol yet"), and `CALL` behaves like a genuine prepared
  `CALL` rather than an emulated one. There is no attribute to switch to
  emulation.
- **MySQL multiple rowsets are not exposed.** A `CALL` returning several result
  sets, or a multi-statement query, only makes its first rowset available
  through this bridge; `nextRowset()` always returns `false` for a `mysql:`
  statement without raising an error (MySQL genuinely supports more rowsets —
  see `nextRowset()`'s SQLite/PostgreSQL `IM001` behavior below).
- **The `?` / `:name` placeholder scanner diverges from php-src only on already-
  malformed SQL.** An unterminated string literal or unterminated `/* … */`
  comment consumes to end-of-input (php-src's scanner instead backtracks and
  treats the opening quote/`/*` as a lone character), so a `?` that appears
  after an unbalanced quote or comment is not recognized as a placeholder. `::`
  runs of three or more colons (chained PostgreSQL type casts) and non-ASCII
  dollar-quote tags (`$tag$…$tag$`) are also not special-cased. Well-formed SQL
  matches php-src exactly; these differences are only observable on SQL that is
  already broken.
- **`Pdo\Pgsql::ATTR_DISABLE_PREPARES`** is accepted but inert — there is no
  execute-only, non-prepared code path to switch to. **A SQLite user-defined
  function or aggregate that returns a non-scalar value** (an array or object)
  yields SQL `NULL` silently rather than raising an error. **`Pdo\Sqlite::setAuthorizer()`,
  the legacy `sqliteCreateFunction()` / `sqliteCreateAggregate()` /
  `sqliteCreateCollation()` aliases, the `uri:` DSN prefix, `php.ini`-based DSN
  aliasing (`pdo.dsn.*`), and the `#[\SensitiveParameter]` attribute** are not
  supported.
- **`PDO::quote()` on SQLite is binary-safe for an embedded NUL byte** — a
  deliberate improvement over php-src, whose own SQLite quoter
  (`ext/pdo_sqlite`) truncates the quoted literal at the first NUL.
- **`nextRowset()`** raises `IM001` ("driver does not support multiple
  rowsets") for SQLite and PostgreSQL statements — errMode-aware, like every
  other statement failure — instead of silently returning `false`; a `mysql:`
  statement returns `false` without raising (see above).
- `new PDOStatement(...)` constructed directly (not via `query()` / `prepare()`)
  throws a `PDOException` ("You should not create a PDOStatement manually") as
  long as the `$connection` argument is not a real, currently-open connection
  handle — the only way to call this constructor at all, since elephc never
  exposes a valid handle to PHP code. It cannot detect a caller who happens to
  guess a live handle (elephc's handles are small sequential integers), unlike
  php-src's real ownership check.
