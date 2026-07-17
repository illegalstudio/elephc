---
title: "PDO (Databases)"
description: "PDO database access with SQLite, PostgreSQL, MySQL/MariaDB, optional PDO_DBLIB, PDO_FIREBIRD, PDO_ODBC, PDO_INFORMIX, and PDO_OCI: connections, prepared statements, fetch modes, transactions, and php-src divergences."
sidebar:
  order: 17
---

elephc implements PDO for the PHP 8.0 through 8.6 compatibility targets, with the
**SQLite**, **PostgreSQL**, **MySQL / MariaDB**, optional **FreeTDS
PDO_DBLIB**, optional **PDO_FIREBIRD**, optional **PDO_ODBC**, and optional
**PDO_INFORMIX**, and optional **Oracle PDO_OCI** drivers. `PDO`, `PDOStatement`, and `PDOException` behave like their
PHP counterparts for everyday use: connect, execute, prepare/bind, fetch, and run
transactions. The DSN prefix selects the driver.

The default drivers are linked statically (SQLite is bundled; PostgreSQL and MySQL
use pure-Rust clients), so their compiled PDO binaries have **no system
database-client dependency**. The optional DBLIB profile deliberately follows PHP
and links the target platform's FreeTDS `libsybdb`; the resulting binary therefore
needs a compatible system client at build and runtime. PDO_ODBC likewise links
unixODBC and delegates database protocols to installed ODBC drivers. The Firebird
profile uses the pure-Rust wire protocol and adds no client-library runtime dependency.
PDO_OCI loads Oracle Instant Client dynamically through ODPI-C, preserving the official
extension's external Oracle-client boundary without requiring proprietary headers at build time.
PDO_INFORMIX follows PECL 1.3.7 and delegates to the IBM/HCL Client SDK through
the platform ODBC driver manager.

The surface is deliberately honest: where a feature is not implemented, it fails
loudly (a `PDOException`, a `ValueError`, a `TypeError`) rather than silently
returning wrong data. The [Divergences from php-src](#divergences-from-php-src) and
[Limitations](#limitations) sections below enumerate what is different and why —
read them before porting security-sensitive or data-loading code.

## PHP compatibility version

PDO's generated surface is selected with `--php-version=8.0` through
`--php-version=8.6`; `ELEPHC_PHP_VERSION` provides the same selection for automation.
The command-line option wins over the environment and the default remains PHP 8.4.
Patch versions and values outside this range are rejected.

| Target | PDO differences selected by elephc |
| --- | --- |
| 8.0 | Core classes and legacy SQLite/PostgreSQL driver methods; no public `queryString`, namespaced driver classes, or `PDO::connect()`. |
| 8.1 | Public `PDOStatement::$queryString` and `PDORow::$queryString`. |
| 8.2–8.3 | Password parameters carry `#[SensitiveParameter]`; otherwise the 8.1 PDO surface. |
| 8.4 | `PDO::connect()` and `Pdo\Sqlite`, `Pdo\Mysql`, `Pdo\Pgsql`, plus `Pdo\Dblib` / `Pdo\Firebird` / `Pdo\Odbc` when their profiles are enabled; historical high-bit fetch flags. |
| 8.5 | Compact fetch flags, SQLite busy/explain/transaction attributes and `setAuthorizer()`, PostgreSQL transaction-constant deprecations, and deprecations for the legacy DBLIB/Firebird/ODBC constant aliases. |
| 8.6 | The 8.5 public surface plus PostgreSQL persistent-session cleanup with `DISCARD ALL` when the final owner releases a pooled handle. |

The version switch currently governs PDO first; it does not claim that every unrelated
PHP language or standard-library difference is version-gated.

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

// SQL Server / Sybase through the optional FreeTDS PDO_DBLIB profile.
$tds = new PDO("dblib:host=127.0.0.1;port=1433;dbname=app", "sa", "secret");

// Firebird through the optional pure-Rust wire-protocol profile.
$firebird = new PDO("firebird:dbname=localhost/3050:/data/app.fdb;charset=UTF8", "SYSDBA", "secret");

// Any installed unixODBC driver through the optional PDO_ODBC profile.
$odbc = new PDO("odbc:Driver={PostgreSQL Unicode};Servername=127.0.0.1;Database=app", "me", "secret");

// Informix through the optional Client SDK CLI/ODBC profile.
$informix = new PDO("informix:Driver={IBM INFORMIX ODBC DRIVER};Server=ol_informix;Database=app", "me", "secret");

// Oracle through the optional PDO_OCI / Instant Client profile.
$oracle = new PDO("oci:dbname=//127.0.0.1:1521/FREEPDB1;charset=AL32UTF8", "me", "secret");
```

The DSN normally starts with `sqlite:`, `pgsql:`, `mysql:`, or (when enabled)
`dblib:`, `firebird:`, `odbc:`, `informix:`, or `oci:`. A colonless value may
instead name a runtime PHP configuration alias such as
`pdo.dsn.app = "pgsql:host=db;dbname=app"`; `new PDO("app")` then uses the resolved
DSN. The standalone binary loads an explicit `PHPRC` file (or `php.ini` inside an
explicit `PHPRC` directory) followed by alphabetically sorted `.ini` fragments from
`PHP_INI_SCAN_DIR`, with the last assignment winning as in PHP. Alias names are
case-sensitive. An absent alias throws the normal argument-shaped `PDOException`; an
alias whose value contains no colon throws
`PDOException("invalid data source name (via INI: pdo.dsn.<name>)")`.

A colon-bearing DSN with an unknown prefix throws `PDOException("could not find
driver")` before any connection is attempted, matching php-src. The resolved alias
value is also authoritative for driver-specific subclasses, `PDO::connect()`,
credentials, and persistent-pool identity.

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
- **DBLIB**: like php-src, constructor credentials become the DB-Library login
  credentials and take precedence over credentials embedded in the DSN.

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

Setting `ATTR_PERSISTENT` later with `setAttribute()` returns `false`; persistence is a
constructor-only choice and the live handle remains unchanged. Persistent connections are local to
the running native process; there is no cross-process pool. Checkout is serialized and
validates liveness (MySQL `COM_PING`, PostgreSQL client state), evicting a dead session
before reconnecting. Under the PHP 8.6 target, the final owner of a persistent PostgreSQL
handle also performs upstream's new disconnect-equivalent `DISCARD ALL` cleanup.

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

`bindParam()` retains a durable reference to the caller variable and reads its current
value on every `execute()`, including when a concrete scalar local must be promoted to
the compiler's boxed `Mixed` reference-cell representation. PDO_OCI also writes native
`PARAM_INPUT_OUTPUT` results back through that reference, honors `$maxLength`, and turns
output LOB locators into PHP streams.

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
orientation is honored by PostgreSQL statements prepared with
`[PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL]`. SQLite rejects a scroll cursor and MySQL
remains forward-only, matching the capabilities exposed by those drivers.

`fetch()` returns `false` when the result set is exhausted. `FETCH_OBJ` creates a real
`stdClass` and assigns dynamic properties directly, including numeric column names such
as `"0"`. `FETCH_CLASS` builds the configured class (or `stdClass` when none is
configured) and assigns column values to matching declared or dynamic properties;
`FETCH_INTO` fills and returns the configured object, and raises **HY000** ("No
fetch-into object specified.") when there is none.

Column values are returned with their native scalar shape: integer → int, real /
floating point → float, text → string, SQLite/MySQL binary values → binary-safe string,
PostgreSQL `boolean` → bool, PostgreSQL `bytea` → rewound stream resource, and `NULL` →
null. `FETCH_BOTH` is the default mode.

### Fetch modes

| Mode | Notes |
| --- | --- |
| `FETCH_ASSOC` / `FETCH_NUM` / `FETCH_BOTH` / `FETCH_OBJ` | Fully supported. |
| `FETCH_COLUMN` | Column index is the 2nd argument to `setFetchMode()` / `fetchAll()`. |
| `FETCH_CLASS` | Target class configured on the statement. Properties are assigned before the constructor by default; `FETCH_PROPS_LATE` selects constructor-first hydration. |
| `FETCH_INTO` | Target object configured on the statement; HY000 without one. |
| `FETCH_KEY_PAIR` | Two-column result as `[col0 => col1]`; HY000 if the result has ≠ 2 columns. |
| `FETCH_NAMED` | Assoc-only; duplicate column names group into a list under one key. |
| `FETCH_BOUND` | Advances the cursor, writes every `bindColumn()` destination, and returns `true`/`false`. |
| `FETCH_CLASSTYPE` | **Real**: the class name comes from column 0's *value*, per row; column 0 is consumed; an unknown class falls back to `stdClass`. |
| `FETCH_GROUP` / `FETCH_UNIQUE` | **Implemented** (see below). |
| `FETCH_PROPS_LATE` | Implements PHP's constructor-first class hydration. |
| `FETCH_LAZY` | `fetch()` returns the statement-owned reusable `PDORow`; `fetchAll()` rejects it as php-src does. |
| `FETCH_FUNC` | `fetchAll()` invokes any PHP callable shape (closure, function string, callable array, first-class descriptor, invokable object) once per row. |

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
  **`sqlite:decl_type`** key, present only when the column has one. Common descriptor
  fields report `len = -1` and `precision = 0`; `table` is present only for a column
  backed by a native SQLite table.
- **PostgreSQL** reports `pgsql:oid`, `pgsql:table_oid`, native type, PDO type, raw
  `PQfsize`/`PQfmod` equivalents, and the `pg_class` table name when applicable.
- **MySQL** reports its wire type (`LONG`, `VAR_STRING`, `NEWDECIMAL`, …), PDO type,
  source table, declared length/precision, and native flags such as `not_null`,
  `primary_key`, `multiple_key`, `unique_key`, and `blob`.
- **DBLIB** reports php-src's exact DB-Library descriptor keys: `max_length`,
  `precision`, `scale`, `column_source`, `native_type`, `native_type_id`,
  `native_usertype_id`, and `pdo_type`.

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

Grouping keys use PHP array-key normalization, including integer-looking strings.

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
be iterated once. `PDOStatement` implements `IteratorAggregate` and `getIterator()` returns
a forwarding iterator. The compiler-owned adapter is excluded from public class discovery,
has a private constructor and exposes no public helper hook; the observable interface
relationship matches PHP.

## Attributes

`getAttribute()` / `setAttribute()` act on:

| Attribute | Behavior |
| --- | --- |
| `ATTR_ERRMODE` | Silent / Warning / Exception (default). |
| `ATTR_DRIVER_NAME` | `"sqlite"`, `"pgsql"`, `"mysql"`, optional `"dblib"`, `"firebird"`, or `"odbc"`. |
| `ATTR_PERSISTENT` | Pool selection (constructor only, in practice). |
| `ATTR_TIMEOUT` | Seconds. SQLite: busy-timeout. pgsql/mysql: initial connect timeout. DBLIB: login and query timeout unless a DBLIB-specific timeout overrides it. |
| `ATTR_DEFAULT_FETCH_MODE` | Mode used by a no-argument `fetch()`; inherited by statements at `prepare()` time. |
| `ATTR_SERVER_VERSION` | The server's version string for the default drivers, Firebird, and ODBC. DBLIB follows IM001 and exposes negotiated TDS through its driver-specific attribute. |
| `ATTR_CLIENT_VERSION` | SQLite's embedded library version; PostgreSQL/MySQL/Firebird report their statically linked client; ODBC reports `ODBC-unixODBC`. DBLIB follows IM001 and exposes FreeTDS through its driver-specific attribute. |
| `ATTR_SERVER_INFO` | PostgreSQL: live PID/session parameters. MySQL: live server statistics. Firebird: version information. ODBC: DBMS name. SQLite follows IM001. |
| `ATTR_CONNECTION_STATUS` | PostgreSQL: live connected/closed status in libpq's wording. MySQL: actual TCP/socket transport description. Firebird: boolean liveness. SQLite follows IM001. |
| `ATTR_CASE` | Folds fetched column-name keys upper/lowercase. |
| `ATTR_ORACLE_NULLS` | Folds `NULL` ↔ `""` in fetched scalar values. |
| `ATTR_STRINGIFY_FETCHES` | Stringifies fetched INTEGER/FLOAT values. |
| `ATTR_EMULATE_PREPARES` | MySQL text protocol (default `true`) or PostgreSQL simple-query protocol (default `false`). DBLIB is read-only `true`, because DB-Library has no native prepare API. SQLite rejects it. |
| `ATTR_AUTOCOMMIT` | Live MySQL or ODBC autocommit state. |
| `ATTR_DEFAULT_STR_PARAM` | MySQL and DBLIB default `PARAM_STR_CHAR`/`PARAM_STR_NATL` string binding. |
| `Pdo\Dblib::ATTR_CONNECTION_TIMEOUT` | Constructor-only DB-Library login timeout in seconds. |
| `Pdo\Dblib::ATTR_QUERY_TIMEOUT` | Constructor and live query timeout in seconds; write-only, matching php-src. |
| `Pdo\Dblib::ATTR_STRINGIFY_UNIQUEIDENTIFIER` | Returns SQL Server `uniqueidentifier` values as uppercase canonical strings instead of 16 raw bytes. |
| `Pdo\Dblib::ATTR_VERSION` / `ATTR_TDS_VERSION` | FreeTDS client version and negotiated TDS protocol version (read-only). |
| `Pdo\Dblib::ATTR_SKIP_EMPTY_ROWSETS` | Omits DB-Library results without columns while traversing `nextRowset()`. |
| `Pdo\Dblib::ATTR_DATETIME_CONVERT` | Selects FreeTDS text conversion; disabled uses php-src's fixed `YYYY-MM-DD HH:MM:SS` representation. |
| `Pdo\Firebird::ATTR_DATE_FORMAT` / `ATTR_TIME_FORMAT` / `ATTR_TIMESTAMP_FORMAT` | `strftime`-style output formats for Firebird temporal values. |
| `Pdo\Firebird::TRANSACTION_ISOLATION_LEVEL` | `READ_COMMITTED`, `REPEATABLE_READ` (default), or `SERIALIZABLE` for the next manual transaction. |
| `Pdo\Firebird::WRITABLE_TRANSACTION` | Selects read/write (default) or read-only manual transactions. |
| `Pdo\Odbc::ATTR_USE_CURSOR_LIBRARY` | Constructor-only ODBC driver-manager cursor selection. |
| `Pdo\Odbc::ATTR_ASSUME_UTF8` | Live ODBC UTF-8 conversion flag. |
| `Pdo\Sqlite::ATTR_OPEN_FLAGS` | Raw `sqlite3_open_v2` flags at open time. A `file:` DSN body always gets `SQLITE_OPEN_URI` OR-ed in. |
| `Pdo\Sqlite::ATTR_READONLY_STATEMENT` | Live `sqlite3_stmt_readonly()` read (statement-level). |
| `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` | Wired: with it on, `errorInfo()[1]` is the *extended* code (`2067` SQLITE_CONSTRAINT_UNIQUE, not the coarse `19`). Write-only, exactly as in php-src — `getAttribute()` follows IM001. |
| `Pdo\Mysql::ATTR_INIT_COMMAND` | One SQL statement run right after authentication. |
| `Pdo\Mysql::ATTR_FOUND_ROWS` | Wired: negotiates `CLIENT_FOUND_ROWS`, so an UPDATE's `rowCount()` reports rows *matched*, not rows *changed*. |
| `Pdo\Mysql::ATTR_DIRECT_QUERY` | Alias of `ATTR_EMULATE_PREPARES`, as in php-src. |
| `Pdo\Mysql::ATTR_USE_BUFFERED_QUERY` | Selects buffered or observable unbuffered semantics for new statements. |
| `Pdo\Mysql::ATTR_LOCAL_INFILE` / `ATTR_LOCAL_INFILE_DIRECTORY` | Constructor-only upload permission and canonical directory sandbox. |
| `Pdo\Mysql::ATTR_COMPRESS` / `ATTR_IGNORE_SPACE` / `ATTR_MULTI_STATEMENTS` | Connection protocol/capability controls. |
| `Pdo\Mysql::ATTR_SSL_KEY` / `ATTR_SSL_CERT` / `ATTR_SSL_CA` / `ATTR_SSL_VERIFY_SERVER_CERT` | Drive MySQL TLS (see the TLS section). |
| `Pdo\Pgsql::ATTR_DISABLE_PREPARES` | Uses PostgreSQL's execute-only simple-query path, including multi-command SQL. |
| `ATTR_PREFETCH` | PostgreSQL connection/prepare option controlling buffered cursor semantics. |
| `ATTR_CURSOR` (prepare option) | PostgreSQL supports `CURSOR_SCROLL` and all `FETCH_ORI_*` movements. SQLite rejects non-forward cursors; MySQL remains forward-only. |
| `ATTR_FETCH_TABLE_NAMES` | MySQL prefixes fetched column keys with the protocol table label; other drivers reject it. |

`ATTR_CASE`, `ATTR_ORACLE_NULLS`, `ATTR_STRINGIFY_FETCHES`, and
`ATTR_DEFAULT_FETCH_MODE` are **snapshotted onto each statement at `prepare()` time**,
not re-read on every fetch: a `setAttribute()` call after a statement is prepared does
not retroactively affect it (real PHP re-checks the connection attribute per fetch).

Attributes are selected by the active driver's hook, not by numeric-range membership.
An attribute unsupported by that driver is never retained in a generic echo bag:
`setAttribute()` returns `false`, and `getAttribute()` follows the IM001 path. This is
especially important because driver-specific values overlap (`1002` is SQLite extended
result codes and MySQL init command).

### Attribute value validation

- The **shape** of the value is checked before any range check, exactly as php-src's
  `pdo_get_long_param()` / `pdo_get_bool_param()` do: `setAttribute(PDO::ATTR_ERRMODE,
  "banana")` raises a `TypeError` instead of casting to `0` and silently switching the
  connection to `ERRMODE_SILENT`. The same check runs on the constructor's `$options`
  array.
- `ATTR_ERRMODE` outside 0/1/2, and `ATTR_CASE` outside `CASE_NATURAL`/`UPPER`/`LOWER`,
  raise a `ValueError` and leave the current value untouched. `ATTR_DEFAULT_FETCH_MODE`
  rejects `0`.
- An unsupported attribute, whether it is a known constant or an unknown number,
  makes `setAttribute()` return **`false` silently**. `getAttribute()` raises **IM001**
  (or returns `false` in silent mode), matching the active php-src driver hook.

`PDOStatement::getAttribute()` answers SQLite readonly/busy/explain state,
PostgreSQL result-memory size, and `ATTR_EMULATE_PREPARES` from the prepare-time
snapshot. SQLite explain mode is writable on PHP 8.5+. Other statement attributes follow
the driver's IM001 path.

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
  bool, text types → string, `NULL` → null. The rich types are returned as their text
  representation: `numeric`/`decimal` (scale preserved), `date` / `time` / `timestamp` /
  `timestamptz`, `uuid`, and `json`/`jsonb`. The same values bind as parameters (text is
  coerced to the column type). `bytea` is returned as a rewound binary stream resource.
  `json` / `jsonb` are re-serialized compactly, so whitespace may differ
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
  is `655366`. A plain table column also carries its `pg_class` relation name under
  `table`; expression columns omit that key.
- **`getNotify()`.** `getNotify(PDO::FETCH_ASSOC, $timeoutMs)` shapes a pending
  `LISTEN`/`NOTIFY` message as `["message" => $channel, "pid" => $pid, "payload" =>
  $payload]`; any other `$fetchMode` (the default) keeps the numerically-indexed
  `[$channel, $pid, $payload]` shape. Both return `false` when no notification
  arrives within the timeout.
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

## FreeTDS PDO_DBLIB notes

PDO_DBLIB is an optional system-client profile because php-src itself delegates this
driver to DB-Library. Install FreeTDS (`brew install freetds` on macOS or
`apt install freetds-dev` on Debian/Ubuntu), then compile with:

```bash
cargo run --features pdo-dblib -- app.php
```

The profile makes `dblib:` available through `PDO::getAvailableDrivers()`, enables
the legacy `PDO::DBLIB_ATTR_*` constants on every supported PHP compatibility target,
and adds `Pdo\Dblib` on PHP 8.4+. PHP 8.5 marks the legacy aliases deprecated and
points callers to the namespaced constants, matching php-src's stubs.

- **DSN.** `host`, `dbname`, `charset`, `appname`, `user`, and `password` follow
  PDO_DBLIB. `port` is accepted as a direct FreeTDS login property for local and CI
  instances that do not use a `freetds.conf` server alias.
- **Prepared statements.** DB-Library has no native prepare API. Placeholders are
  scanned and safely rendered as T-SQL literals; mixed named/positional styles and
  missing binds fail with HY093. `ATTR_EMULATE_PREPARES` is consequently fixed at
  `true` and attempting to disable it returns `false`.
- **Results.** Every `dbresults()` result is materialized and exposed through
  `nextRowset()`. `ATTR_SKIP_EMPTY_ROWSETS` controls whether results without columns
  remain visible. Integer/float/binary types retain their PHP scalar shapes;
  `uniqueidentifier` and datetime conversion follow the driver attributes above.
- **Native diagnostics.** FreeTDS client callbacks and SQL Server/Sybase messages
  populate the same SQLSTATE/native-code/message triples as the other bridge drivers.
- **Targets.** The Rust backend is target-neutral; each supported target must provide
  a target-compatible `libsybdb`. macOS linking deliberately resolves FreeTDS before
  `libSystem`, whose unrelated Berkeley DB API exports the same `dbopen` symbol.

## PDO_FIREBIRD notes

Enable Firebird without a system `libfbclient` dependency:

```bash
cargo run --features pdo-firebird -- app.php
```

The profile registers `firebird:`, the three historical `PDO::FB_ATTR_*` format
aliases on PHP 8.0–8.6, and `Pdo\Firebird` on PHP 8.4+. PHP 8.5 deprecates only
the legacy aliases. `Pdo\Firebird::getApiVersion()` reports API level 40, matching
the Firebird 4/5 client API targeted by the backend.

- **DSN.** `dbname`, `charset`, `role`, `dialect`, `user`, and `password` match
  PDO_FIREBIRD. Both documented legacy remote names
  (`host/port:/path-or-alias`) and Firebird 3+ `inet[4|6]://` names are accepted.
- **Values and binding.** Positional and named placeholders are normalized to the
  Firebird positional protocol; mixing styles or omitting a bind fails with HY093.
  Integer, floating, boolean, binary, text, and date/time values retain PDO scalar
  shapes and embedded NUL bytes.
- **Transactions.** Manual transactions use the configured isolation/access mode.
  Firebird's connection-level format, autocommit, table-name, isolation, and
  writable attributes are live and version-independent like php-src.
- **Metadata.** Firebird follows php-src's deliberately small `getColumnMeta()`
  result and returns only `pdo_type`. Statement cursor names expose the same
  31-byte validation and nullable readback contract.
- **Client identity.** `ATTR_CLIENT_VERSION` identifies `rsfbclient-rust 0.27`
  rather than a dynamically installed `libfbclient`; server behavior and protocol
  results remain the authoritative compatibility boundary.

## PDO_ODBC notes

Install unixODBC and the database-specific ODBC driver, then enable the profile:

```bash
brew install unixodbc                 # macOS
sudo apt install unixodbc-dev         # Debian/Ubuntu build dependency
cargo run --features pdo-odbc -- app.php
```

The profile follows php-src's architecture: PDO calls the ODBC 3 driver-manager ABI,
and the installed driver owns the database protocol. It exposes `PDO_ODBC_TYPE`
(`"unixODBC"`), the historical `PDO::ODBC_*` aliases on PHP 8.0–8.6, and
`Pdo\Odbc` on PHP 8.4+. PHP 8.5 deprecates the aliases in favor of the namespaced
constants.

- **DSNs.** `odbc:<data-source-name>` calls `SQLConnect`; a body containing `=` is
  passed to `SQLDriverConnect`. Braced values preserve embedded semicolons and escaped
  closing braces. Constructor credentials are appended only when the direct string
  does not already contain `UID=` / `PWD=`, matching PDO_ODBC.
- **Values and metadata.** Non-NULL result values are fetched as strings, including
  numeric database values. `getColumnMeta()` returns only `pdo_type => PDO::PARAM_STR`,
  matching php-src's deliberately small descriptor.
- **Attributes.** Autocommit and `Pdo\Odbc::ATTR_ASSUME_UTF8` are live connection
  attributes. `ATTR_USE_CURSOR_LIBRARY` is a pre-connect option. Cursor names,
  native scroll-cursor selection, and `nextRowset()` use the statement's ODBC handle;
  fetched scroll rows are materialized before PDO orientation is applied.
- **Unsupported hooks.** PDO_ODBC has no quoter, last-insert-id hook, or connection-
  status attribute; these paths raise the same IM001 class as php-src.
- **Targets.** Every supported target links its native unixODBC library (`libodbc`).
  The selected database driver must exist for that target and be registered with the
  driver manager.

## PDO_INFORMIX notes

PDO_INFORMIX remains a PECL extension and requires IBM/HCL Client SDK. Install
the target-compatible SDK and register its ODBC driver, then enable the profile:

```bash
cargo run --features pdo-informix -- app.php
```

The implementation tracks stable PECL PDO_INFORMIX 1.3.7. That extension does
not declare driver-specific constants or a `Pdo\Informix` subclass, including
on PHP 8.4+, so elephc deliberately exposes neither one. `informix:` is present
in `pdo_drivers()` and `PDO::getAvailableDrivers()` on every supported PHP
compatibility target.

- **DSNs.** `informix:<data-source-name>` uses `SQLConnect`; a body containing
  `=` uses `SQLDriverConnect`. Constructor credentials are added only when the
  connection string does not already provide them.
- **LOB compatibility.** After connecting, the bridge enables
  `SQL_INFX_ATTR_LO_AUTOMATIC` followed by `SQL_INFX_ATTR_ODBC_TYPES_ONLY`, in
  the same order as PECL, so Informix CLOB/BLOB values are exposed through the
  standard ODBC long text/binary types.
- **Values and parameters.** Scalar result values use the Client SDK's text
  representation. Binary data preserves embedded NUL bytes. Scalar
  `PDO::PARAM_INPUT_OUTPUT` binds use native `SQL_PARAM_INPUT_OUTPUT`; Informix
  LOB parameters remain input-only, matching the extension.
- **Driver behavior.** Natural column names are upper-cased by default.
  Autocommit, transactions, native diagnostics, scroll cursors, cursor names,
  multiple rowsets, and the most recent `SERIAL` value are wired through CLI.
  `ATTR_CLIENT_VERSION` is `1.3.7`; `ATTR_SERVER_INFO` is the DBMS name.
  PDO_INFORMIX does not implement `ATTR_SERVER_VERSION` or a connection-status
  attribute, so those requests follow PDO's IM001 path.
- **Metadata.** `getColumnMeta()` follows PECL's associative shape: `scale`, the
  optional base `table`, `native_type`, boolean `not_null`/`unsigned`/
  `auto_increment` flag entries, and the PHP-version-aware `pdo_type`. Informix
  binary and long-value fetches are streams, while metadata uses `PARAM_LOB`
  only for Informix BLOB/CLOB UDTs, preserving the extension's own distinction.
- **Targets.** The Rust bridge and unixODBC ABI build on macOS ARM64, Linux ARM64,
  and Linux x86_64. A live connection additionally requires an IBM/HCL Client
  SDK and Informix ODBC driver built for that same target.

## PDO_OCI notes

PDO_OCI is optional because, like PHP's extension, it needs an Oracle client at runtime:

```bash
cargo run --features pdo-oci -- app.php
```

Install Oracle Instant Client for the target and expose its directory through the
platform loader (`LD_LIBRARY_PATH` on Linux or the corresponding macOS loader path).
The bridge uses Oracle's ODPI-C layer, which resolves `libclntsh` dynamically; compiling
elephc and the bridge itself therefore needs neither Oracle headers nor a client install.

- **Version surface.** PHP bundled PDO_OCI through 8.3 and moved it to PECL in PHP 8.4.
  The current PECL 1.2.0 surface keeps `PDO::OCI_ATTR_ACTION`, `CLIENT_INFO`,
  `CLIENT_IDENTIFIER`, `MODULE`, and `CALL_TIMEOUT`; it does not define `Pdo\Oci`.
  elephc follows that split on every compatibility target.
- **DSN and encoding.** `dbname`, `user`, `password`, and `charset` follow PDO_OCI;
  constructor credentials override DSN credentials. Compiled PHP strings cross ODPI-C as
  UTF-8, so `AL32UTF8` and `UTF8` are accepted explicitly and another client character set
  fails at connection setup rather than being ignored.
- **Execution.** Oracle native placeholders, repeated named binds, scroll orientations,
  prefetch rows, autocommit, tracked transactions, affected-row counts, ping-based
  persistent checkout, and Oracle SQLSTATE/native diagnostics are wired to the client.
  Constructor failures preserve PDO_OCI's native code and special SQLSTATE mappings.
  Scalar Oracle values and scalar input/output results remain strings like PDO_OCI.
  Input `PARAM_LOB` strings/streams use temporary Oracle BLOBs; null LOB output binds and
  fetched BLOB/CLOB/NCLOB/BFILE values are exposed as PHP streams.
- **Attributes and metadata.** Session action/module/client fields and millisecond call
  timeout are live. `getColumnMeta()` reports PDO_OCI's `oci:decl_type`, `native_type`,
  `pdo_type`, `scale`, and nullable/not-null/blob flags.
- **Unsupported official hooks.** PDO_OCI itself has no last-insert-id hook, connection-
  status attribute, or driver subclass. `PDO::quote()` uses the driver's single-quote
  doubling implementation.

## TLS / encrypted connections

PostgreSQL and MySQL connect over TLS with [rustls](https://github.com/rustls/rustls).
DBLIB encryption is negotiated by the installed FreeTDS configuration, just as it is
for php-src; SQLite is in-process and unaffected.

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
- `sslcertmode`: `allow` (default), `disable`, or `require` for client certificates.
- `sslsni`: `1` (default) or `0`; `ssl_min_protocol_version` /
  `ssl_max_protocol_version`: TLS 1.2/1.3 bounds honored by rustls.
- `sslcrl` / `sslcrldir`: PEM revocation lists applied to rustls verification.

Unlike libpq's bare `require` (which encrypts without verifying), elephc always
validates the server certificate once TLS is negotiated — the safer default — so
`require`, `verify-ca`, and `verify-full` all verify against the trust roots. For a
server with a self-signed certificate, pass its CA via `sslrootcert`.

**MySQL / MariaDB** — ships in the default build through mysql 28's ring-backed
rustls feature. Configure it with the `Pdo\Mysql::ATTR_SSL_*` constructor options:

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
- `ATTR_SSL_CAPATH`: a CA directory. Its PEM certificates are combined into the
  per-connection bundle rustls accepts, alongside `ATTR_SSL_CA` when both are set.
- `ATTR_SSL_CIPHER` restricts rustls to the named modern suites. `ATTR_SERVER_PUBLIC_KEY`
  supplies the trusted RSA key used by non-TLS `caching_sha2_password` authentication.
  Unsupported legacy OpenSSL cipher names fail the connection instead of silently
  broadening the negotiated suite set.

Presence of any `ATTR_SSL_*` option enables TLS. A custom minimal build that disables
the default `mysql-tls` feature raises a `PDOException` rather than silently falling back
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
on success) and `PDO::errorInfo()` starts with `[SQLSTATE, driver-specific code,
message]`, with `["00000", null, null]` on success. Every driver surfaces a real
`SQLSTATE`: SQLite through a php-src-matching table, MySQL from the `ERR` packet's
`#`-marked field, and PostgreSQL from the `ErrorResponse` `C` field. DBLIB follows
php-src's extended failure shape by appending the operating-system error code and
severity, then the operating-system message when present. `PDOStatement` tracks its
own error state through the same `errorCode()` / `errorInfo()` pair.

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

`PDOException` keeps PHP's inherited public constructor signature:
`new PDOException(string $message = "", int $code = 0, ?Throwable $previous = null)`.
Driver failures use a private prelude-only factory to attach structured metadata without
exposing a non-PHP constructor shape:

- **`$e->errorInfo`** is a real `[SQLSTATE, driver-code, message]` array for a server
  error (so `$e->errorInfo[0]` works — which is what frameworks read) and `null` when
  there is no structured info, matching PHP.
- **`getCode()`** returns PDO's SQLSTATE string when structured driver information exists;
  the driver-specific integer remains available in `errorInfo[1]`.
- **`getPrevious()`** returns the stored previous Throwable. The same value is also exposed
  as `$e->previous` because elephc's base Throwable layout has no private previous slot.

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
  `PDO::getAvailableDrivers()`. A bare, case-insensitive call is sufficient to
  auto-inject the PDO prelude and bridge; `--with-pdo` is not required.
- **PDO**: `__construct`, `exec`, `query`, `prepare`, `quote`, `lastInsertId`,
  `beginTransaction`, `commit`, `rollBack`, `inTransaction`, `errorCode`, `errorInfo`,
  `getAttribute`, `setAttribute`, `getAvailableDrivers` (static), `connect` (static
  factory), `__destruct`. `clone $pdo` throws (PHP forbids it too), and
  `serialize($pdo)` throws `Exception: Serialization of 'PDO' is not allowed` —
  php-src marks the class `@not-serializable`, and without the guard elephc's
  property-walking `serialize()` would emit the raw bridge handle into the blob and
  hand back a zombie object on `unserialize()`.
- **PDOStatement**: `execute`, `bindValue`, `bindParam`, `bindColumn`, `setFetchMode`,
  `fetch`, `fetchAll`, `fetchColumn`, `fetchObject`,
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
  every element `PARAM_STR`, whatever the PHP value's type). Emulated MySQL/PostgreSQL
  statements also print php-src's `Sent SQL: [n] ...` line using the exact SQL rendered
  by the bridge; native and SQLite statements omit it.
  Rebinding replaces the visible entry, and a named parameter reports `paramno=-1`
  until its first execute-time normalization, as in php-src.
- **Constants**: the selected PHP version's complete PDO set — fetch-mode (base modes plus the OR-able
  `FETCH_GROUP` / `FETCH_UNIQUE` / `FETCH_CLASSTYPE` / `FETCH_PROPS_LATE` / … flags),
  parameter (including `PARAM_STR_NATL` / `PARAM_STR_CHAR` / `PARAM_INPUT_OUTPUT`),
  cursor, case, null-handling, and `ATTR_*` constants, plus `ERR_NONE` (`"00000"`), the
  parameter-lifecycle `PARAM_EVT_*` constants (declared so code enumerating the class
  surface compiles — they are entirely inert here, since elephc's drivers are native Rust
  and expose no `param_hook` seam to PHP), and the **legacy `PDO::SQLITE_*` aliases**
  (`PDO::SQLITE_ATTR_OPEN_FLAGS`, `PDO::SQLITE_OPEN_*`, `PDO::SQLITE_DETERMINISTIC`,
  `PDO::SQLITE_ATTR_READONLY_STATEMENT`, `PDO::SQLITE_ATTR_EXTENDED_RESULT_CODES`),
  which php-src registers on the base class alongside the 8.1+ class-scoped spellings.
- **Driver subclasses**: `Pdo\Sqlite`, `Pdo\Mysql`, and `Pdo\Pgsql` (PHP 8.4+) extend
  `PDO` and inherit its full base surface, so `new \Pdo\Sqlite("sqlite::…")` works like
  `new \PDO(...)` and the instance is `instanceof \PDO`. A program that names only a
  subclass — never the base `PDO` — still injects the prelude, and `PDO::connect($dsn, …)`
  returns the matching subclass for the DSN's driver prefix (an unknown prefix throws).
  Each subclass declares its PHP 8.4 driver-specific constants.

  **Opening a foreign DSN through a subclass is rejected**, for constructors and the
  inherited static factory: `new Pdo\Sqlite("mysql:…")` and
  `Pdo\Sqlite::connect("mysql:…")` both fail before connecting.

  Driver methods: `Pdo\Pgsql::escapeIdentifier()`, `getPid()`, `lobCreate()` /
  `lobUnlink()` / `lobOpen()`, `copyFromArray()` / `copyFromFile()` / `copyToArray()` /
  `copyToFile()`, `getNotify()`, `setNoticeCallback()`; `Pdo\Mysql::getWarningCount()`;
  `Pdo\Sqlite::loadExtension()`, `openBlob()`, `createCollation()`, `createFunction()`,
  `createAggregate()`, and `setAuthorizer()` on PHP 8.5+.

  PHP 8.4's legacy driver-extension methods are also installed directly on `PDO`:
  `sqliteCreateFunction()`, `sqliteCreateAggregate()`, `sqliteCreateCollation()`,
  `pgsqlCopyFromArray()`, `pgsqlCopyFromFile()`, `pgsqlCopyToArray()`,
  `pgsqlCopyToFile()`, `pgsqlLOBCreate()`, `pgsqlLOBOpen()`, `pgsqlLOBUnlink()`,
  `pgsqlGetNotify()`, and `pgsqlGetPid()`. They use the same bridge behavior as the
  modern subclass spellings.

Connections and prepared statements release their underlying bridge resources
automatically through `__destruct`: a `PDO` closes its connection (finalizing any
remaining statements) and a `PDOStatement` finalizes itself when the object is released
— at the end of its scope, when its variable is reassigned or `unset()`, or at program
exit. You do not need to close them explicitly. A statement also **roots its owning
`PDO`**, so `return $db->query(...)` from inside a function whose local `$db` then goes
out of scope keeps the connection alive.

## SQLite user-defined functions and collations

`Pdo\Sqlite` runs compiled-PHP callables as SQLite callbacks:

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

Closures, function-name strings, static/instance callable arrays, invokable objects, and
first-class callables are accepted. A callback that throws never crashes or unwinds
across SQLite's engine: the exception is caught at the C boundary and the statement fails
with a `PDOException` (a throwing *collation* comparator is instead treated as "equal",
since SQLite's comparison has no error channel). A UDF/aggregate returning a **non-scalar**
value is reported as a callback error instead of being silently converted to SQL `NULL`.
Callbacks may register/replace another callback and execute nested statements on the same
SQLite connection. The bridge releases its global connection/statement table locks before
SQLite invokes PHP, while SQLite's own serialized connection mutex remains authoritative.

`Pdo\Pgsql::setNoticeCallback(?callable $callback): void` registers a callback invoked with
the text of each PostgreSQL server `NOTICE` (e.g. from `RAISE NOTICE`):

```php
$pg = new \Pdo\Pgsql("pgsql:host=localhost;dbname=app");
$pg->setNoticeCallback(fn($msg) => error_log("PG NOTICE: $msg"));
$pg->exec("DO $$ BEGIN RAISE NOTICE 'migrated'; END $$");   // callback fires with "migrated"
```

Passing `null` unregisters delivery. Delivery is **boundary-dispatched**: the driver buffers notices as they arrive and
dispatches them right after each `exec()` / `query()` on the connection, so a `NOTICE`
raised by a prepared-statement `execute()` is delivered on the next `exec()`/`query()`.

## Divergences from php-src

These are behavioral differences you can *observe* — not missing features. Read them
before assuming php-src semantics.

### Native and emulated prepare protocols

MySQL follows php-src's default and starts with `ATTR_EMULATE_PREPARES = true`. Its
scanner renders placeholders client-side, quotes values according to the connection's
`NO_BACKSLASH_ESCAPES` mode, skips strings/comments/backticks, treats `??` as a literal
question mark and sends the result through the text protocol. Setting
`ATTR_EMULATE_PREPARES` (or `Pdo\Mysql::ATTR_DIRECT_QUERY`) to `false` switches new
statements to server-side prepare.

PostgreSQL defaults to native extended-query prepares. `ATTR_EMULATE_PREPARES = true`
selects client-side rendering plus the simple-query protocol;
`Pdo\Pgsql::ATTR_DISABLE_PREPARES = true` selects the execute-only simple-query path.
The latter supports multi-command and utility SQL that cannot be represented by one
server-side prepared statement. Generated `$N` marker ranges are tracked separately so
a literal PostgreSQL `$1` already present in source SQL is never mistaken for a PDO bind.

Both emulated paths reject mixed placeholder styles and missing bindings with HY093,
preserve source SQL evaluation order, and retain the rendered text for
`debugDumpParams()`'s `Sent SQL:` line. The protocol choice is snapshotted on each
`PDOStatement`; changing the connection attribute only affects statements prepared later.

### Other divergences

- **`queryString` uses the language's `readonly` enforcement.** Writes are rejected with
  a catchable `Error`, including through a `PDOStatement|false` receiver, but the message
  is PHP's generic readonly-property text rather than pdo_stmt.c's custom wording.
- **PostgreSQL native error code.** The Rust PostgreSQL client exposes SQLSTATE but has no
  libpq `ExecStatusType`; `errorInfo()[1]` therefore uses stable non-zero marker `1` on
  failure instead of fabricating a `PGRES_*` enum value.
- **Notice timing.** PostgreSQL notices are buffered on the protocol callback and
  dispatched at the PHP boundary that completed the operation: `exec()`, `query()`, or
  prepared `execute()`. PHP is never re-entered from the client's protocol thread.
- **UPSTREAM php-src bug, deliberately NOT reproduced.** php-src 8.4's `copyToArray()` /
  `copyToFile()` build `COPY … TO STDIN` (`pgsql_driver.c:882,884,973,975`) — an invalid
  direction in PostgreSQL's COPY grammar. elephc correctly emits `TO STDOUT`. Do not "fix"
  this to match php-src.

## Limitations

### Driver matrix boundary

- **Compiled drivers.** SQLite, PostgreSQL, and MySQL / MariaDB are in the default
  profile; DBLIB, Firebird, ODBC, and OCI are available through their optional profiles.
  The central registry intentionally reports
  only drivers present in the selected archive rather than advertising inert names.

### Driver-specific client options

- MySQL constructor options implement buffered/unbuffered observation,
  `ATTR_LOCAL_INFILE` with an optional canonical `ATTR_LOCAL_INFILE_DIRECTORY` sandbox,
  compression, ignore-space, multi-statement control, init command, found-rows, and the
  supported TLS key/cert/CA/verification settings. A disabled local-infile handler rejects
  the server request instead of returning a successful empty upload.
- `Pdo\Mysql::ATTR_SSL_CAPATH` is adapted to rustls by building a deterministic
  multi-certificate PEM bundle. The pinned mysql 28 patch adds `ATTR_SSL_CIPHER` and
  `ATTR_SERVER_PUBLIC_KEY`, the two controls absent from its public API. No security option
  is accepted inertly.
- `Pdo\Pgsql::ATTR_RESULT_MEMORY_SIZE` reports the bytes owned by the native
  result and returns `null` with HY000 before statement execution. `ATTR_PREFETCH` is
  supported at connection and prepare-option scope.
- PDO_DBLIB's full 1000–1006 attribute range is implemented. Connection timeout is
  constructor-only; query timeout is write-only; version attributes are read-only;
  boolean value options are readable and writable, matching the driver's php-src hook.
- PDO_FIREBIRD's format, isolation, writable-transaction, autocommit, and
  fetch-table-name attributes are implemented, including their PHP-version aliases.
- PDO_OCI's autocommit, prefetch, call-timeout, action, module, client-info, and
  client-identifier attributes are implemented through Oracle Instant Client.
- `ATTR_MAX_COLUMN_LEN`, `ATTR_FETCH_CATALOG_NAMES`, and `ATTR_CURSOR_NAME` are rejected
  when the active driver has no corresponding php-src hook/capability.

### PostgreSQL DSN option handling

`tokio-postgres`'s connection-string parser hard-fails with `UnknownOption` on any key it
does not know. The bridge forwards the keys it can honor and rejects the others explicitly.
Forwarded: `user`, `password`, `dbname`, `options`,
`application_name`, `sslnegotiation`, `host`, `hostaddr`, `port`, `connect_timeout`,
`tcp_user_timeout`, `keepalives`, `keepalives_idle`, `keepalives_interval`,
`keepalives_retries`, `target_session_attrs`, `channel_binding`, `load_balance_hosts`
— plus `sslmode` / `sslrootcert` / `sslcert` / `sslkey`, which are consumed separately and
applied to the rustls connector.

`client_encoding` is translated into a validated post-connect session setting. Libpq-only
configuration is resolved before `tokio-postgres`: named `service`/`servicefile` sections,
`.pgpass`/`passfile`, the corresponding `PG*` environment variables,
`fallback_application_name`, `requiressl`, `sslcompression`, `sslsni`, `sslcertmode`, and
TLS 1.2/1.3 bounds all follow libpq-style precedence. A secure passfile is required and a
multi-host passfile is rejected because the native client can carry only one password.

The default bridge stays pure Rust. For the exact libpq behavior PHP delegates to — GSSAPI
and Kerberos authentication/encryption, encrypted-key `sslpassword`, `require_auth`, and
the `replication` startup parameter — build the PDO archive with:

```bash
cargo build -p elephc-pdo --features libpq-gss
```

With Homebrew's keg-only libpq on Apple Silicon, expose `pg_config` during that build:

```bash
PATH=/opt/homebrew/opt/libpq/bin:$PATH cargo build -p elephc-pdo --features libpq-gss
```

Then export `ELEPHC_PDO_LIBPQ=1` while compiling the PHP program so the final native link
adds `-lpq`. This profile sends the explicit PDO options to `PQconnectdb`; service files,
passfiles, `PG*` environment defaults, and Kerberos/GSS configuration are therefore resolved
by libpq itself. It uses libpq for the complete PostgreSQL connection lifetime, just as
php-src does, and requires a target-compatible libpq with the desired GSS/Kerberos support.
Individual keywords remain version-dependent: for example, a libpq predating
`require_auth` rejects it exactly as the same PHP build would.
The ordinary profile rejects these options explicitly and retains standalone pure-Rust binaries.

The repository's `scripts/test-pdo-gss.sh` performs the complete integration proof:
it starts an ephemeral MIT Kerberos realm, creates client/server principals and keytabs,
configures PostgreSQL with `hostgssenc`, obtains a client ticket, and connects with both
`gssencmode=require` and `require_auth=gss`. A second isolated process replaces
`KRB5CCNAME` with an empty cache and verifies that libpq fails closed. The PDO live CI
runs this fixture after the ordinary native and libpq suites.

### Resource and lifetime caveats

- Persistent checkout is serialized and validates liveness before reuse: MySQL sends
  `COM_PING`, PostgreSQL checks the live client state, and a dead handle plus its statements
  is evicted before an atomic reconnect. SQLite needs no external-server probe.
- **External-driver rows honor buffering.** MySQL
  `ATTR_USE_BUFFERED_QUERY=false` and native PostgreSQL `ATTR_PREFETCH=false` move the
  connection into a demand worker and decode one row per `fetch()`, reducing peak memory
  to the active row while preserving MySQL's 2014 busy diagnostic and PostgreSQL cursor
  invalidation. Buffered modes retain the complete result. PostgreSQL's emulated
  simple-query path follows the selected PHP version: PHP 8.0–8.4 retains its historical
  buffered behavior, while PHP 8.5+ consumes `simple_query_raw()` one row at a time, matching
  php-src's `PQsendQuery()` + `PQsetSingleRowMode()` implementation.
- **LOB streams.** SQLite `openBlob()` keeps only cursor/size state and uses bounded
  `sqlite3_blob_read` / `sqlite3_blob_write` slices; its native fixed-size rule rejects
  extending writes. PostgreSQL `lobOpen()` likewise keeps only cursor/size state: reads use
  bounded `lo_get` slices and writes use bounded `lo_put` patches, preserving binary data,
  seeks, sparse extension, and transaction ownership without copying the complete large
  object into client memory.
- **`Pdo\Sqlite::loadExtension()`** runs native code from the named library, weakening the
  standalone-binary guarantee. An empty name is a `ValueError`.
- **SQLite bridge threadsafety invariant.** The bridge keeps its connection table and its
  statement table under **two separate mutexes**, so two overlapping calls can touch the
  same `sqlite3*` — which is only defined under a **serialized** SQLite build
  (`SQLITE_THREADSAFE=1`). The bundled amalgamation is built that way and
  `assert_sqlite_threadsafe()` pins the invariant at the first open (it panics rather than
  corrupting memory if a future build ever flips it). Do not link a
  `SQLITE_THREADSAFE=0` amalgamation on a threaded target.
- **Database TLS provider.** PostgreSQL and MySQL/MariaDB TLS both ship in the default
  build and use rustls with the ring provider. mysql 28's `rustls-tls-ring` feature removes
  the former aws-lc-rs/C-toolchain cost. Custom `--no-default-features` builds still reject
  a requested TLS connection loudly rather than silently downgrading it.
