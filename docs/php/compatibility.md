---
title: "PHP Compatibility"
description: "How much of PHP elephc covers: builtin coverage by module, language constructs, extensions, and known limitations."
sidebar:
  order: 21
---

<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: python3 scripts/docs/gen_php_comparison.py -->

Baseline: **PHP 8.4.20** (CLI snapshot of 2026-08-11, 59 extensions, 2030 internal functions).

Overall builtin coverage: **490 / 2030** (24%).

## Builtin coverage by PHP module

| PHP module | Supported | Coverage | AOT | eval() |
|---|---|---|---|---|
| `bcmath` | 14 / 14 | 100% | 14 | 14 |
| `bz2` | 0 / 10 | 0% | 0 | 0 |
| `calendar`† | 0 / 18 | 0% | 0 | 0 |
| `core` | 36 / 59 | 61% | 33 | 32 |
| `ctype` | 4 / 11 | 36% | 4 | 4 |
| `curl` | 0 / 33 | 0% | 0 | 0 |
| `date`† | 11 / 48 | 23% | 11 | 11 |
| `dba` | 0 / 15 | 0% | 0 | 0 |
| `dom` | 0 / 2 | 0% | 0 | 0 |
| `exif` | 0 / 4 | 0% | 0 | 0 |
| `fileinfo` | 0 / 6 | 0% | 0 | 0 |
| `filter` | 0 / 7 | 0% | 0 | 0 |
| `ftp` | 0 / 36 | 0% | 0 | 0 |
| `gd` | 0 / 103 | 0% | 0 | 0 |
| `gettext` | 0 / 10 | 0% | 0 | 0 |
| `gmp` | 0 / 51 | 0% | 0 | 0 |
| `hash` | 9 / 15 | 60% | 9 | 9 |
| `iconv`† | 10 / 10 | 100% | 10 | 10 |
| `intl` | 0 / 183 | 0% | 0 | 0 |
| `json` | 5 / 5 | 100% | 5 | 5 |
| `ldap` | 0 / 55 | 0% | 0 | 0 |
| `libxml` | 0 / 8 | 0% | 0 | 0 |
| `mbstring` | 2 / 65 | 3% | 2 | 2 |
| `mysqli`† | 0 / 106 | 0% | 0 | 0 |
| `openssl` | 4 / 66 | 6% | 4 | 4 |
| `pcntl` | 0 / 25 | 0% | 0 | 0 |
| `pcre` | 5 / 11 | 45% | 5 | 5 |
| `pdo`† | 0 / 1 | 0% | 0 | 0 |
| `pgsql` | 0 / 122 | 0% | 0 | 0 |
| `posix` | 0 / 40 | 0% | 0 | 0 |
| `random` | 4 / 9 | 44% | 4 | 3 |
| `readline` | 1 / 13 | 8% | 1 | 1 |
| `session`† | 0 / 23 | 0% | 0 | 0 |
| `shmop` | 0 / 6 | 0% | 0 | 0 |
| `simplexml` | 0 / 3 | 0% | 0 | 0 |
| `soap` | 0 / 2 | 0% | 0 | 0 |
| `sockets` | 0 / 37 | 0% | 0 | 0 |
| `sodium` | 0 / 110 | 0% | 0 | 0 |
| `spl` | 15 / 15 | 100% | 15 | 15 |
| `standard` | 366 / 542 | 68% | 366 | 340 |
| `sysvmsg` | 0 / 7 | 0% | 0 | 0 |
| `sysvsem` | 0 / 4 | 0% | 0 | 0 |
| `sysvshm` | 0 / 7 | 0% | 0 | 0 |
| `tokenizer` | 0 / 2 | 0% | 0 | 0 |
| `xml` | 0 / 22 | 0% | 0 | 0 |
| `xmlwriter` | 0 / 42 | 0% | 0 | 0 |
| `zend opcache` | 0 / 7 | 0% | 0 | 0 |
| `zip` | 0 / 10 | 0% | 0 | 0 |
| `zlib` | 4 / 30 | 13% | 4 | 4 |

The table counts functions implemented as native registry builtins. Modules marked † have some or all of their functions implemented through other elephc mechanisms (compiler rewrites or runtime preludes); their real support status is tracked in the Extensions section below.

The remaining 10 baseline extensions expose classes but no procedural functions, so they have no row above: `ffi`, `mysqlnd`, `pdo_mysql`, `pdo_pgsql`, `pdo_sqlite`, `phar`, `reflection`, `sqlite3`, `xmlreader`, `xsl`.

7 extensions bundled with php-src were not loaded by the PHP build that produced this snapshot and are not counted: `enchant`, `odbc`, `pdo_dblib`, `pdo_firebird`, `pdo_odbc`, `snmp`, `tidy`.

In addition, elephc implements 3 PHP language constructs that PHP does not count as functions: `empty()`, `isset()`, `unset()`.

## Language constructs

| Feature | Status | Notes |
|---|---|---|
| [Classes & objects](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.php)) | ✅ Supported |  |
| [Inheritance, interfaces, abstract classes](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.inheritance.php)) | ✅ Supported |  |
| [Traits](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.traits.php)) | ✅ Supported |  |
| [Enums](./classes.md) ([PHP](https://www.php.net/manual/en/language.enumerations.php)) | ✅ Supported |  |
| [Anonymous classes](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.anonymous.php)) | ✅ Supported |  |
| [Constructor property promotion](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.decon.php)) | ✅ Supported |  |
| [Property hooks](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.property-hooks.php)) | ✅ Supported |  |
| [Attributes](./classes.md) ([PHP](https://www.php.net/manual/en/language.attributes.php)) | ✅ Supported |  |
| [Magic methods](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.magic.php)) | ✅ Supported | See the classes page for the supported set |
| [Static properties, methods, late static binding](./classes.md) ([PHP](https://www.php.net/manual/en/language.oop5.static.php)) | ✅ Supported |  |
| [Union & intersection types](./types.md) ([PHP](https://www.php.net/manual/en/language.types.type-system.php)) | ✅ Supported |  |
| [Closures & arrow functions](./functions.md) ([PHP](https://www.php.net/manual/en/functions.anonymous.php)) | ✅ Supported |  |
| [First-class callable syntax](./functions.md) ([PHP](https://www.php.net/manual/en/functions.first_class_callable_syntax.php)) | ✅ Supported |  |
| [Generators](./generators.md) ([PHP](https://www.php.net/manual/en/language.generators.php)) | ✅ Supported | Lowered onto stackful coroutines |
| [Fibers](./fibers.md) ([PHP](https://www.php.net/manual/en/language.fibers.php)) | ✅ Supported |  |
| [Exceptions (try / catch / finally)](./control-structures.md) ([PHP](https://www.php.net/manual/en/language.exceptions.php)) | ✅ Supported |  |
| [match expressions](./control-structures.md) ([PHP](https://www.php.net/manual/en/control-structures.match.php)) | ✅ Supported |  |
| References (&) ([PHP](https://www.php.net/manual/en/language.references.php)) | ✅ Supported |  |
| [Namespaces](./namespaces.md) ([PHP](https://www.php.net/manual/en/language.namespaces.php)) | ✅ Supported |  |
| include / require ([PHP](https://www.php.net/manual/en/function.include.php)) | ✅ Supported | Static, compile-time resolved paths |
| [eval()](./eval.md) ([PHP](https://www.php.net/manual/en/function.eval.php)) | 🟡 Partial | Experimental; embeds an optional interpreter bridge when runtime parsing is required |

## Extensions

| Feature | Status | Notes |
|---|---|---|
| [PDO](./pdo.md) ([PHP](https://www.php.net/manual/en/book.pdo.php)) | ✅ Supported | Driver matrix documented on the PDO page |
| [mysqli](./mysqli.md) ([PHP](https://www.php.net/manual/en/book.mysqli.php)) | 🟡 Partial | Locked v1 subset over the elephc-pdo bridge; divergences on the mysqli page |
| [Sessions](./sessions.md) ([PHP](https://www.php.net/manual/en/book.session.php)) | ✅ Supported | In --web binaries |
| [Streams](./streams.md) ([PHP](https://www.php.net/manual/en/book.stream.php)) | 🟡 Partial |  |
| [SPL](./spl.md) ([PHP](https://www.php.net/manual/en/book.spl.php)) | 🟡 Partial |  |
| [Reflection](./classes.md) ([PHP](https://www.php.net/manual/en/book.reflection.php)) | 🟡 Partial |  |
| [DateTime](./datetime.md) ([PHP](https://www.php.net/manual/en/book.datetime.php)) | 🟡 Partial |  |
| [Calendar](./calendar.md) ([PHP](https://www.php.net/manual/en/book.calendar.php)) | ✅ Supported |  |
| [iconv](./iconv.md) ([PHP](https://www.php.net/manual/en/book.iconv.php)) | ✅ Supported |  |
| [GD / image](./image.md) ([PHP](https://www.php.net/manual/en/book.image.php)) | 🟡 Partial | Enabled with --with-image |
| OpenSSL ([PHP](https://www.php.net/manual/en/book.openssl.php)) | 🟡 Partial | Encrypt/decrypt subset |
| [OPcache](./opcache.md) ([PHP](https://www.php.net/manual/en/book.opcache.php)) | 🟡 Partial | Compatibility surface; programs are AOT-compiled, there is no opcode cache |

## Beyond PHP

elephc-specific builtins with no PHP equivalent (not counted in coverage above):

| Builtin | Area | Description |
|---|---|---|
| `buffer_free()` | Buffer | Frees a buffer<T> and nulls the local variable that held it. |
| `buffer_len()` | Buffer | Returns the logical element count of a buffer<T>. |
| `class_attribute_args()` | Class | Returns the constructor arguments of a named attribute applied to a class. |
| `class_attribute_names()` | Class | Returns the list of attribute names applied to a class. |
| `class_get_attributes()` | Class | Returns an array of ReflectionAttribute objects for all attributes of a class. |
| `clamp()` | Math | Clamps a value to be within a specified range. *(No PHP equivalent (not in PHP 8.4/8.5))* |
| `log2()` | Math | Returns the base-2 logarithm of a number. *(No PHP equivalent (PHP has log(), log10(), log1p()))* |
| `buffer_new()` | Pointer | Allocates a raw byte buffer. |
| `ptr()` | Pointer | Returns a raw pointer to the given variable. |
| `ptr_get()` | Pointer | Reads one machine word through a raw pointer and returns it as an integer. |
| `ptr_is_null()` | Pointer | Returns true if the pointer is null. |
| `ptr_null()` | Pointer | Returns a null raw pointer. |
| `ptr_offset()` | Pointer | Returns a new pointer offset from the given pointer by the given byte count. |
| `ptr_read16()` | Pointer | Reads one unsigned 16-bit word through a raw pointer and returns it as an integer. |
| `ptr_read32()` | Pointer | Reads one unsigned 32-bit word through a raw pointer and returns it as an integer. |
| `ptr_read8()` | Pointer | Reads one unsigned byte through a raw pointer and returns it as an integer. |
| `ptr_read_string()` | Pointer | Copies raw bytes from a pointer into a PHP string of the given length. |
| `ptr_set()` | Pointer | Writes one machine word through a raw pointer. |
| `ptr_sizeof()` | Pointer | Returns the byte size of the named pointer target type. |
| `ptr_write16()` | Pointer | Writes one 16-bit word through a raw pointer. |
| `ptr_write32()` | Pointer | Writes one 32-bit word through a raw pointer. |
| `ptr_write8()` | Pointer | Writes one byte through a raw pointer. |
| `ptr_write_string()` | Pointer | Copies PHP string bytes into raw memory at the given pointer. |
| `zval_free()` | Pointer | Frees a PHP zval pointer allocated by `zval_pack`. |
| `zval_pack()` | Pointer | Packs an elephc runtime value into a heap-allocated PHP zval pointer. |
| `zval_type()` | Pointer | Returns the PHP zval type byte for a zval pointer. |
| `zval_unpack()` | Pointer | Unpacks a PHP zval pointer into an owned elephc Mixed value. |
| `grapheme_strrev()` | String | Reverses a string by grapheme cluster, returning false on failure. *(No PHP equivalent (not in PHP 8.4/8.5 intl))* |
| `is_real()` | Type | Alias of is_float(). *(Removed in PHP 8.0; kept as a compatibility alias of is_float())* |

## Known limitations

**Static subset, AOT only.** Ordinary source is compiled ahead of time with no opcode fallback; runtime code loading exists only through the experimental eval() interpreter bridge.

**Coverage measured against a pinned baseline.** Builtin percentages are computed against the vendored PHP CLI snapshot (version and extension set recorded in the page header). PHP modules outside that snapshot are not counted.

**Five native compile targets.** Standalone binaries target macOS ARM64, Linux ARM64, and Linux x86_64; macOS also cross-compiles libraries for iOS ARM64 devices and the iOS ARM64 Simulator. PHP itself runs on many more platforms.
