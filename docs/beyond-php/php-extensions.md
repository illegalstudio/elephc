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
builds that engine code from a pinned php-src release — the real `zend_hash.c`,
`zend_API.c`, `zend_operators.c` and their neighbours, not a reimplementation —
into `libelephc_zend.a`, and replaces only what an ahead-of-time binary does
not have: the executor that runs opcodes.

`elephc extension add` then:

1. declares the managed [`php-src`](../compiling/native-dependencies.md)
   package, which provides the PHP headers and that engine archive;
2. downloads the extension and pins the archive's SHA-256 in `elephc.toml`
   (PECL publishes no checksums, so the first download is trusted and every
   later one must match it);
3. evaluates the extension's own `config.m4` — the file `phpize` feeds to
   `configure` — to learn its sources, compiler flags and `config.h` defines.
   Options take the defaults the extension declares, and autoconf probes are
   answered as a modern POSIX system would;
4. compiles it with the target C/C++ toolchain into its own static archive;
5. **starts it** once — running its `MINIT` against the engine — and records
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
| objects, closures, callables | — | not yet: see below |

| From the extension | To your program |
|---|---|
| scalars, strings, arrays (any nesting) | the same Elephc values |
| `stdClass` objects, including inside arrays | `stdClass` objects |
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

## Current limits

- **Objects and callables cannot be passed in yet.** A function taking a
  `callable` (APCu's `apcu_entry()`, `array_map`-style helpers) or an object is
  still declared, but calling it throws an `Error` explaining why, and
  `elephc extension add` lists such functions. Class-based extensions (`ds`'s
  `Ds\Vector`, `mongodb`) therefore expose only their functions.
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

| Extension | Result |
|---|---|
| `apcu` | identical |
| `igbinary` | identical |
| `msgpack` | identical |
| `simdjson` | works (decoding, validation, key lookups) |
| `zstd` | identical, except that warnings omit PHP's `in FILE on line N` suffix |

## See also

- [zval bridge](zval-bridge.md) — the value layer hosted calls are built on.
- [Native dependencies](../compiling/native-dependencies.md) — the `php-src`
  package and the managed cache.
- [`extern` functions](extern.md) — calling plain C libraries.
