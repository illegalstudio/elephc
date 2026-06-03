---
title: "PDO (Databases)"
description: "PDO database access with the SQLite driver: connections, prepared statements, fetch modes, and transactions."
sidebar:
  order: 16
---

elephc supports a practical subset of PHP's PDO database layer, backed by the
**SQLite** driver. `PDO`, `PDOStatement`, and `PDOException` behave like their
PHP counterparts for everyday use: connect, execute, prepare/bind, fetch, and run
transactions.

SQLite is statically bundled into the program, so a compiled PDO binary has **no
system database dependency** — it runs anywhere the elephc binary runs.

## Connecting

```php
<?php
// File-backed database (created if missing) or an in-memory database.
$db = new PDO("sqlite:/path/to/app.db");
$mem = new PDO("sqlite::memory:");
```

The DSN must start with `sqlite:`. The optional `$username`, `$password`, and
`$options` constructor arguments are accepted for signature compatibility but are
ignored by the SQLite driver. A failed connection throws a `PDOException`.

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

## Fetching results

```php
<?php
$stmt = $db->query("SELECT id, name FROM users");

$stmt->fetch(PDO::FETCH_ASSOC);  // ["id" => 1, "name" => "Ada"]
$stmt->fetch(PDO::FETCH_NUM);    // [0 => 1, 1 => "Ada"]
$stmt->fetch(PDO::FETCH_BOTH);   // both numeric and string keys
$stmt->fetch(PDO::FETCH_OBJ);    // stdClass { id: 1, name: "Ada" }

$all = $db->query("SELECT id FROM users")->fetchAll(PDO::FETCH_NUM);
$one = $db->query("SELECT name FROM users")->fetchColumn();  // first column of next row
```

`fetch()` returns `false` when the result set is exhausted. Column values are
returned with their SQLite type: INTEGER → int, REAL → float, TEXT → string,
NULL → null. `FETCH_BOTH` is the default mode.

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

`PDO::errorCode()` returns the SQLite result code as a string and
`PDO::errorInfo()` returns `[code, code, message]`.

## Supported surface

- **PDO**: `__construct`, `exec`, `query`, `prepare`, `lastInsertId`,
  `beginTransaction`, `commit`, `rollBack`, `errorCode`, `errorInfo`.
- **PDOStatement**: `execute`, `bindValue`, `bindParam`, `setFetchMode`, `fetch`,
  `fetchAll`, `fetchColumn`, `rowCount`, `columnCount`; Traversable, so a statement
  can be walked with `foreach`.
- **Fetch modes**: `FETCH_ASSOC`, `FETCH_NUM`, `FETCH_BOTH`, `FETCH_OBJ`.
- **Parameters**: positional `?` and named `:name`; `PARAM_INT` / `PARAM_STR` /
  `PARAM_NULL` / `PARAM_BOOL` constants.
- **Constants**: the fetch-mode, parameter, and `ATTR_ERRMODE` / `ERRMODE_*`
  constants used above.

## Limitations

- **SQLite only.** MySQL and PostgreSQL drivers are not yet implemented (the
  bridge is structured to add them later).
- **`bindParam()`** binds the current value, not a deferred by-reference read.
- **`FETCH_CLASS` / `FETCH_INTO`** are not implemented.
- **`FETCH_OBJ`** materializes the stdClass via a JSON round-trip, so a result
  set whose column names are `0, 1, 2, …` degrades to an array.
- **Binary / BLOB values with embedded NUL bytes** are not round-tripped through
  the text path.
- **`getAttribute` / `setAttribute`** (beyond the accepted constants) and
  persistent connections are not implemented.
- Avoid `new PDOStatement(...)` directly — statements are created by `query()` /
  `prepare()`.
