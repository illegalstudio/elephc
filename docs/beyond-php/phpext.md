---
title: "PHP extensions"
description: "Compile exported functions into modules loaded by real PHP 8.5."
sidebar:
  order: 7
---

`--emit phpext` builds a module that real PHP loads with `extension=...`.
Top-level functions carrying `#[Export]` become PHP functions, preserving their
PHP names, parameter names and supported type declarations.

```php
<?php
#[Export]
function add(int $left, int $right): int
{
    return $left + $right;
}
```

```sh
elephc --emit phpext math.php
php -n -d extension=./math.so -r 'echo add(right: 22, left: 20), "\n";'
```

The output is `math.so` on Linux and macOS. The macOS artifact is a PHP module
bundle. Its name defaults to the source stem, which must be a lowercase C
identifier. Its module version is the compiler version. Override these with
`ELEPHC_PHPEXT_MODULE_NAME` and `ELEPHC_PHPEXT_MODULE_VERSION`. Source top-level
statements are not a module initialization hook; put work in exported functions.

## Host SDK

This mode supports native Linux x86_64, Linux aarch64 and macOS aarch64 builds,
with PHP 8.5 NTS or ZTS. iOS and cross-SDK builds are rejected.

Install the development SDK belonging to the PHP that will load the extension.
`ELEPHC_PHPEXT_PHP_CONFIG` selects its `php-config`; the default is `php-config`
on PATH. When `php-config --php-binary` returns `NONE`, set
`ELEPHC_PHPEXT_PHP_BINARY` to the matching executable. `CC` selects the native C
compiler and flags. Flags are parsed without shell evaluation.

The compiler probes the real headers and interpreter, checks PHP version,
module API, complete build ID, NTS/ZTS, debug mode, target and value widths,
and verifies that a generated module loads. Mixed SDKs fail compilation.
The extension imports Zend from its host and links neither libphp nor Elephc's
replacement Zend engine. Its only intentional dynamic export is `get_module`.

## Values and exceptions

Supported boundary values are `int`, `float`, `bool`, binary `string`, `array`
and `null`, including supported nullable signatures and `mixed` values that
contain this subset. `void` and `never` returns are supported. Zend parses
arguments, applies caller strictness and handles named and optional arguments.
Supported defaults are scalar constants, null and empty arrays.

Arrays may be nested, sparse and heterogeneous. Copies preserve insertion
order, integer and string keys, binary keys and the next append index. Both
strings and arrays are copied in each direction. Neither allocator frees the
other runtime's memory, and changing a returned array cannot mutate an input.

By-reference and variadic exports, objects, resources, callables, recursive
arrays and references inside arrays are rejected. Nesting is limited to 64
arrays. An ordinary reference supplied to a by-value parameter is dereferenced
and copied. Use the PHP `array` declaration for inputs; Elephc's specialized
`array<T>` and `array<K,V>` input ABIs are not exposed by this mode.

Compiled `Exception`, `RuntimeException`, `Error`, `TypeError`, `ValueError`,
`ArithmeticError`, `DivisionByZeroError`, `ArgumentCountError` and `JsonException`
become host PHP exceptions with their class, binary-safe message and integer code. Internal native unwinding terminates
inside the invocation thunk. Native allocation and stack failures become PHP
errors and leave subsequent calls usable. An infrastructure failure rolls
back the native request image; ordinary PHP exceptions preserve completed
state changes. Custom class construction is deferred.

## Request state, output and concurrency

Each request owns a complete native state image, including the fixed heap,
allocator and GC bookkeeping, globals, statics, scratch buffers and exception
state. MINIT creates the immutable initial image; RINIT creates the request
copy; RSHUTDOWN releases it; MSHUTDOWN releases module resources.

A module-local mutex serializes native execution while the adapter exchanges
complete request images at stable addresses. This supports independent ZTS
requests; native calls within one module do not execute in parallel. Snapshot
copies add time and memory proportional to the native state region, including
the configured heap. This is a correctness-oriented initial implementation.

At output and warning callbacks, the adapter parks the active native image and
releases the mutex. PHP then runs its output buffers or error handlers at the
original operation, including calls back into the extension. Before native
execution resumes, the adapter restores the current request image and thread
or Fiber stack boundary. A C bailout guard contains host exceptions and request
termination until native cleanup completes. Output uses PHP's API and never
writes directly to file descriptor 1.

The initial capability policy permits the audited core value runtime and
builtin Throwables. Transitive `exit`/`die`, eval, opaque invocation, native
reference construction, bridges, hosted extensions, web mode and process
monitoring are rejected. Builtin helpers require an explicit audited producer
lifecycle; unsupported helpers receive a compilation diagnostic.

## Verification

The [example](../../examples/php-extension/main.php) contains integer, string,
array and exception exports. Its driver also runs against the identical PHP
source for a byte-for-byte comparison.

```sh
cargo test -p elephc --test phpext_tests -- --include-ignored
```

The dedicated CI matrix covers PHP 8.5 NTS/ZTS on all three native platforms.
It also builds a checksum-pinned real PHP debug CLI/embed SDK, executes repeated
requests, and runs eight concurrent ZTS request threads with small stacks.
Valgrind on Linux and Apple's `leaks` tool check the embed lifecycle. Native
heap pressure and Zend allocation growth are tested separately.

For a local embed check with a PHP SDK configured with `--enable-embed=shared`:

```sh
python3 scripts/phpext/test_embed.py --php-config /path/to/php-config
```
