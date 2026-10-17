---
title: "Hosting PHP extensions"
description: "Build real PECL, PIE, or in-house C extensions from source and call them from compiled programs with elephc extension."
sidebar:
  order: 10
---

> **Strict mode:** hosted extensions are declared through the project's
> `elephc.toml`, not in source, so `--strict-php` programs can use them. What
> they add to a program is exactly the PHP surface the extension registers.

Elephc can host **real PHP extensions** — a PECL release, a PIE package from
Packagist, or an extension you maintain yourself — inside a compiled program.
The extension is built from its own C (or C++) sources, and every function it
registers becomes an ordinary PHP function your code calls by name:

```bash
elephc extension add simdjson@4.0.0
```

```php
<?php
$config = simdjson_decode(file_get_contents("config.json"), true);
echo $config["name"], "\n";
```

```bash
elephc main.php && ./main
```

No PHP installation is involved, at build time or at run time: the binary is
still standalone.

## How it works

A PHP extension is written against the Zend engine's C API: its hash tables,
strings, argument parsing, class registration, objects and exceptions. Elephc
does not use php-src for that. It ships its own implementation of that API,
written in Rust, in the `elephc-zend` crate (`libelephc_zend.a`, installed next
to the other bridge archives):

- the **types and functions** an extension sees — `zval`, `zend_string`,
  `HashTable` with PHP's exact memory layout, `zend_parse_parameters()`,
  `zend_hash_*`, `add_assoc_*`, `zend_throw_exception()`… — are Rust
  definitions. The C header the extension's compiler reads is generated from
  them when Elephc is built, so the two cannot drift;
- the **macros** (`RETURN_STR`, `Z_TYPE_P`, `ZEND_HASH_FOREACH_*`,
  `ZEND_PARSE_PARAMETERS_START`…) are the one part that must be C text, since
  an extension expands them at compile time. They live in a single
  `zend_macros.h`, beside a small C file for the variadic entry points
  (`zend_error()`, `spprintf()`, `zend_parse_parameters()`) Rust cannot
  define;
- the include files an extension asks for (`php.h`, `zend_API.h`,
  `ext/standard/info.h`…) all lead to that generated header.

Nothing is downloaded for this: the headers are embedded in the `elephc`
binary and written once to the managed cache.

`elephc extension add` then:

1. downloads the extension and pins the archive's SHA-256 in `elephc.toml`
   (PECL publishes no checksums, so the first download is trusted and every
   later one must match it);
2. evaluates the extension's own `config.m4` — the file `phpize` feeds to
   `configure` — to learn its sources, compiler flags and `config.h` defines.
   Options take the defaults the extension declares, and autoconf probes are
   answered as a modern POSIX system would;
3. compiles it with the target C/C++ toolchain into its own static archive;
4. **starts it** once — running its `MINIT` against the engine — and records
   what it registered: functions with their argument info, classes, constants
   and INI directives. Taking the surface from the running extension rather than
   from a `.stub.php` means `#ifdef`s, aliases and functions declared only in C
   arginfo (as simdjson's are) are all seen.

When you compile, Elephc declares that surface to your program and links the
extension's archive and the engine. A hosted function whose extension your
program never calls costs nothing: its declaration is pruned and nothing from
its archive is linked.

## Sources

```bash
elephc extension add apcu                  # newest stable PECL release
elephc extension add apcu@5.1.28           # an exact PECL release
elephc extension add vendor/package@1.2.3  # a PIE package (Packagist type php-ext)
elephc extension add demo --path ext/demo  # a source tree in your project
```

The manifest records each source:

```toml
[extension]
schema = 1

[extension.dependencies]
simdjson = { version = "4.0.0", sha256 = "1fb48fe5…" }
apcu = { pie = "apcu/apcu", version = "5.1.28", sha256 = "…" }
demo = { path = "ext/demo" }
```

A path source is rebuilt whenever its contents change. PIE packages must be
hosted on GitHub. Zend extensions (`php-ext-zend`, such as Xdebug) are refused:
they hook the PHP engine's executor, which a compiled program does not have, so
they would link cleanly and never run.

`elephc extension install` builds every declared extension that is missing (for
example on a fresh CI machine), `elephc extension list` shows what is declared and
built, and `elephc extension remove <name>` drops a declaration.

## INI directives

Directives in `[extension.ini]` are applied before any extension starts, the way
`php.ini` is:

```toml
[extension.ini]
"apc.enable_cli" = "1"
"apc.shm_size" = "64M"
```

## What crosses the boundary

| From your program | To the extension | Notes |
|---|---|---|
| `int`, `float`, `bool`, `null`, `string` | the same zval types | strings are copied |
| indexed and associative arrays, nested | PHP arrays | copied, so the extension may keep them |
| objects of the extension's classes | the same object | not copied: see [Classes](#classes) |
| closures, function names, first-class callables, `[$object, "method"]` | a callable the extension can call | see [Callables](#callables) |
| other objects | — | not yet: see below |

| From the extension | To your program |
|---|---|
| scalars, strings, arrays (any nesting) | the same Elephc values |
| `stdClass` objects, including inside arrays | `stdClass` objects |
| objects of the extension's classes, including inside arrays | instances of those classes |
| other objects, resources | an `Error` naming the type |
| a value that contains itself | an `Error`; a value shared along two paths arrives as two copies |

Exceptions thrown by the extension are rethrown as the same class when Elephc
declares it — the extension's own exception classes are declared for you, with
their parents — or as the nearest ancestor that is declared. An `E_WARNING`
is reported on stderr and the call returns, as in PHP; an `E_ERROR` ends the
program with exit status 255.

Argument types come from the extension's arginfo. An optional parameter whose
default is written in arginfo is declared with it; one whose default only the
C code knows is passed only when your call passes it, exactly as PHP does.
Named arguments work as in PHP, including PHP's errors for an unknown name
and for skipping a parameter whose default is not known. By-reference
arguments the extension wrote are written back even when it then throws.

## Classes

Every class and interface an extension registers is declared to your program
under its own name, with its constants and public methods:

```php
<?php
use Ds\Map;

$map = new Map(["a" => 1]);
$map->put("b", 2);
$map["c"] = 3;                     // ArrayAccess
echo count($map), "\n";            // Countable
echo json_encode($map), "\n";      // JsonSerializable
$pair = $map->first();             // a Ds\Pair the extension created
echo $pair->key, "=", $pair->value, "\n";
```

The object itself lives in the engine, exactly as the extension built it (its
own C struct included); your program holds it through an instance of the
declared class. So:

- `new`, instance and static methods, class constants and properties (read,
  written, `isset`, `unset`) go to the extension, which answers as it would in
  PHP.
- An object the extension returns — a factory's, an element of an array, a
  method returning `$this` — arrives as an instance of its class, and a
  method returning `$this` returns the very same instance.
- Such objects can be passed back as arguments, typed with their class.
- You may extend the classes; your subclass keeps the extension's behaviour.
- `Countable`, `ArrayAccess` and `JsonSerializable` are declared on a class
  whose methods have exactly their shape, so `count()`, `$object[...]` and
  `json_encode()` work.
- A class that is `Traversable` in the extension (an `IteratorAggregate`, an
  `Iterator`, or one with only a `get_iterator` handler) can be walked with
  `foreach`, keys included, and passed to `iterator_to_array()`,
  `iterator_count()` and `iterator_apply()`. Breaking out of the loop
  releases the extension's iterator.
- An object is released when its last instance is; the extension's
  destructor and `free_obj` handler then run, as in PHP.
- `clone` goes through the class's `clone_obj` handler, so the copy carries
  the extension's own state; a class without one throws PHP's `Error`
  ("Trying to clone an uncloneable object").

## Callables

A function or method taking a `callable` accepts any PHP callable:

```php
<?php
use Ds\Seq;

$v = new Seq([3, 1, 2]);
$v->sort(fn($a, $b) => $b <=> $a);
echo json_encode($v->map(fn($x) => $x * 10)), "\n";   // [30,20,10]
echo apcu_entry("k", "strtoupper"), "\n";             // K
```

The extension calls it with `zend_call_function()`, as it would in PHP: the
arguments it passes arrive as Elephc values, and the callable's result goes
back to it. An exception thrown by the callable stops the extension's work and
reaches your program as the very same object. An extension may keep the
callable after the call returns; it is released when the extension drops it.

## Current limits

- **Objects of your own classes cannot be passed in yet**, except to a
  parameter typed with the extension's own class. A function taking any other
  object is still declared, but calling it throws an `Error` explaining why,
  and `elephc extension add` lists them.
- **`var_dump()` and `print_r()` show an extension's object without its
  properties**: Elephc's dump routines read declared properties and do not
  call a computed `__debugInfo()`. Calling `$object->__debugInfo()` returns
  what PHP's `var_dump()` would show. Magic methods other than `__toString`
  (`__serialize`, `__invoke`...) are not forwarded.
- **The engine is PHP's extension API, not PHP's runtime.** Elephc's engine
  provides what extensions call to handle values, parse arguments, register
  functions, classes, constants and INI directives, throw exceptions,
  serialize, hash (MD5, SHA-1, SHA-256), convert with iconv and format numbers.
  It does not run PHP code: an extension calls back only the callables it is
  given, so one that calls a PHP function by name (`call_user_function()`) or
  needs the compiler (`ast`) cannot work.
- **Integrations with other extensions are compiled out.** Elephc does not
  host `session`, `pcre`, streams or output buffering, so an optional
  integration guarded by `HAVE_PHP_SESSION` and the like (msgpack's and
  igbinary's session serializers) is simply not built. An extension that
  *requires* another extension's C API fails to link, and `add` names the
  missing symbols; one that reaches a stream or output-buffering entry point
  at run time ends the program with a fatal error naming it.
- **A by-reference output parameter** must be passed a declared variable,
  whatever it holds (`$success = null; apcu_fetch("k", $success);`): it comes
  back holding whatever the extension wrote. An undefined variable is refused
  at compile time.
- **Extensions are built for the host target.** Their surface is read by
  running them, so cross-installing for another target is not supported yet.

## Tested extensions

These PECL extensions are built from their released sources and their output
compared line for line with PHP 8.5 loading the same extension:

| Extension | Version | Result |
|---|---|---|
| `apcu` | 5.1.28 | identical |
| `base58` | 1.0.2 | identical |
| `brotli` | 0.21.0 | identical |
| `geospatial` | 0.3.5 | identical |
| `igbinary` | 3.2.17RC1 | identical |
| `lzf` | 1.7.0 | identical |
| `msgpack` | 3.0.1 | identical |
| `scrypt` | 2.0.1 | identical |
| `simdjson` | 4.0.0 | identical (C++) |
| `trader` | 0.5.1 | identical |
| `var_representation` | 0.1.5 | identical |
| `mailparse` | 3.2.0 | identical, except that an uncaught exception omits PHP's `in FILE:LINE` and stack trace |
| `translit` | 0.7.2 | identical, except that warnings omit PHP's `in FILE on line N` suffix |
| `zstd` | 0.18.0 | identical, except that warnings omit PHP's `in FILE on line N` suffix |
| `bitset` | 3.2.1 | identical (classes, static factories, object arguments, subclassing) |
| `ds` | 2.0.0 | identical (callbacks, comparators, `foreach`, `Ds\Pair`) |
| `jsonpath` | 3.1.0 | identical |
| `ast` | | refused: it needs the PHP compiler's internals |

## See also

- [zval bridge](zval-bridge.md) — the value layer hosted calls are built on.
- [Native dependencies](../compiling/native-dependencies.md) — the managed
  cache extensions are built in.
- [`extern` functions](extern.md) — calling plain C libraries.
