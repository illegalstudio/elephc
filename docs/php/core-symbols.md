---
title: "Core classes and constants"
description: "PHP Core attribute objects, enum interfaces, exceptions, weak object collections, and predefined constants."
sidebar:
  order: 24
---

## Weak object references

`WeakReference::create(object $object): WeakReference` returns the same live
wrapper when called repeatedly for an object. `get(): ?object` acquires a normal
reference for its result and returns `null` after the referent is destroyed.
Keeping the wrapper alive does not retain the referent. Direct construction,
cloning, and serialization are refused.

`WeakMap` implements `ArrayAccess`, `Countable`, and `IteratorAggregate`.
Keys must be objects, including closures. Values are owned by the map and are removed when their
key dies, including during cycle collection. Null values count as entries but
fail `isset()`. Cloning gives the copy an independent value table with the same
weak keys. Iteration yields object keys and pins the current key while the
iterator uses it.

Native entry references (`$value = &$map[$object]`) remain valid after the key
or map is destroyed. Replacing the entry leaves the old referenced value intact.
Cloning separates ordinary array values while preserving live PHP reference sets.
Native reference parameters and `settype()` use the entry's reference set.
Increment/decrement and appending to an entry array update connected references.
Plain and compound assignments replace the entry and detach its old references,
as does `??=` when it assigns a value.
Opaque eval supports the map API and cloning; its parser still rejects
array-element reference assignment for both ordinary arrays and WeakMap entries.

```php
$cache = new WeakMap();
$object = new stdClass();
$weak = WeakReference::create($object);
$cache[$object] = 'cached value';
unset($object);
echo count($cache);             // 0
var_dump($weak->get());         // NULL
```

## Exceptions and enum interfaces

Core provides `CompileError extends Error`, `ParseError extends CompileError`,
`ClosedGeneratorException extends Exception`, and
`RequestParseBodyException extends Exception`, alongside its existing throwable
hierarchy. Their constructors and getters follow the standard Exception API.

`ErrorException` extends `Exception` and adds a final `getSeverity(): int`:

```php
new ErrorException(
    message: 'Upload failed',
    code: UPLOAD_ERR_CANT_WRITE,
    severity: E_WARNING,
    filename: 'upload.php',
    line: 12,
    previous: null,
);
```

Filename and line overrides remain visible through an `Exception` parameter and
inside `eval()` and in the fatal uncaught report. A filename supplied without a
line uses line zero. Null location arguments keep the existing location, initially
the creation site. Explicit line zero and an empty filename remain visible.
Calling `__construct()` again preserves the existing message when no arguments
are supplied. A zero code and null previous exception preserve their existing
values; severity resets to its default unless supplied. Named arguments in later
positions still fill earlier defaults, so `__construct(filename: null)` resets
the message to an empty string.
These constructor rules follow [PHP 8.5.10](https://github.com/php/php-src/blob/php-8.5.10/Zend/zend_exceptions.c#L309-L413).
Native construction records the line of `new`, including inherited constructors
and runtime class names, before invoking the constructor. The line remains the
construction line when the exception is thrown later.

Every enum implicitly implements `UnitEnum`, whose static `cases()` method
returns its cases. Backed enums also implement `BackedEnum`, which extends
`UnitEnum` and adds `from(int|string $value): static` and
`tryFrom(int|string $value): ?static`. Ordinary classes cannot implement these
interfaces, including through user-defined marker interfaces.

## Legacy serialization

`Serializable` provides tentative `serialize(): ?string` and
`unserialize(string $data): void` signatures. Legacy implementations may omit return
hints, including methods marked `ReturnTypeWillChange`.
A string result produces PHP's `C:<name-length>:"<class>":<payload-length>:{<bytes>}`
wire form; null produces `N;`. Payloads can contain braces and NUL bytes.
Unserialization skips the constructor, preserves repeated object references, and
honors `allowed_classes`. `__serialize()` takes precedence for writing, while
`__unserialize()` handles modern `O:` payloads. A legacy serialize hook returning
a value other than string or null throws a catchable `Exception`. Results returned
by a legacy unserialize hook are ignored and released.

Nested wire operations inside legacy hooks share reference indices and object
identities with the enclosing operation, matching PHP. Calls made by modern
`__serialize()`, `__sleep()`, `__unserialize()`, or `__wakeup()` hooks use separate
reference state; the enclosing state is restored even when a hook throws.

## Attribute objects

The predefined final classes are `Attribute`, `AllowDynamicProperties`,
`ReturnTypeWillChange`, `SensitiveParameter`, `Override`, `Deprecated`,
`NoDiscard`, and `DelayedTargetValidation`. They can be constructed and inspected
through the supported Reflection APIs.

`Attribute` exposes mutable `int $flags`, defaulting to `Attribute::TARGET_ALL`,
and the `TARGET_CLASS`, `TARGET_FUNCTION`, `TARGET_METHOD`, `TARGET_PROPERTY`,
`TARGET_CLASS_CONSTANT`, `TARGET_PARAMETER`, `TARGET_CONSTANT`, `TARGET_ALL`, and
`IS_REPEATABLE` constants. The PHP 8.5 target mask is 127; repeatability uses the
separate bit 128. `Deprecated` exposes readonly nullable `message` and `since`
properties; `NoDiscard` exposes a readonly nullable `message` property.

`SensitiveParameterValue(mixed $value)` stores a private readonly value.
`getValue(): mixed` returns it, while `__debugInfo(): array` hides it from debug
output. Serialization is refused. `SensitiveParameter` wraps fixed and variadic backtrace
arguments without changing values read by `func_get_args()`. `NoDiscard` diagnoses
ignored function and method calls, including direct and dynamic instance calls in
eval. Warning masks and user handlers apply; assign to `$_` to consume a
deliberately ignored result.
`DelayedTargetValidation` defers Core target errors until `newInstance()`.
Attribute execution and Reflection limits
are documented in [Classes](classes.md#attributes).

## Predefined constants

All Core constants in the pinned PHP 8.5 baseline are registered through the
shared symbol catalog. They are available as bare constants, through
`constant()` and `defined()`, in `get_defined_constants(true)['Core']`, and in
opaque `eval()` fragments, except for the existing `STDIN`, `STDOUT`, and `STDERR`
resource restriction in eval. `TRUE`, `FALSE`, and `NULL` retain their PHP types and
special case-insensitive names. Other constant names remain case-sensitive.

The portable groups include error levels, `DEBUG_BACKTRACE_*`, integer and
floating-point limits, `PHP_FLOAT_DIG`, `PHP_OUTPUT_HANDLER_*`, `UPLOAD_ERR_*`,
and the debug/thread build flags.

Environment constants describe the compiled Elephc program:

| Constants | Value |
|---|---|
| `PHP_BINARY` | The running executable's path, resolved at runtime |
| `PHP_BUILD_DATE`, `PHP_BUILD_PROVIDER` | UTC compiler build date and `elephc` |
| `DEFAULT_INCLUDE_PATH` | `.` |
| `PHP_FD_SETSIZE` | `1024` |
| `PHP_MAXPATHLEN` | `4096` on Linux, `1024` on Apple targets |
| `PHP_SHLIB_SUFFIX` | `so` on Linux, `dylib` on Apple targets |
| `PHP_CLI_PROCESS_TITLE` | Whether the target supplies the process-title facility |
| `PHP_DEBUG`, `PHP_ZTS`, `ZEND_DEBUG_BUILD`, `ZEND_THREAD_SAFE` | `false`, since the program has no Zend debug or thread-safe engine |
| `ZEND_VM_KIND` | `ZEND_VM_KIND_NONE`, since there is no Zend VM |
| `PHP_PREFIX`, `PHP_BINDIR`, `PHP_SBINDIR`, `PHP_LIBDIR`, `PHP_DATADIR`, `PHP_SYSCONFDIR`, `PHP_LOCALSTATEDIR`, `PHP_MANDIR` | Empty strings, since a standalone program has no PHP installation |
| `PHP_CONFIG_FILE_PATH`, `PHP_CONFIG_FILE_SCAN_DIR`, `PHP_EXTENSION_DIR`, `PEAR_INSTALL_DIR`, `PEAR_EXTENSION_DIR` | Empty strings, since there are no PHP ini, extension, or PEAR directories |

`PHP_BINARY` parameter defaults resolve the running path for direct calls,
first-class callables, and calls from opaque eval. Constant aliases and array
defaults containing the path preserve that runtime value.

The compiler host's PHP installation paths and the baseline Docker image's
paths are never copied into these constants.
