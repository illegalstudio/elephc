# `--emit cdylib` end-to-end demo

This example compiles three `#[Export]` PHP functions into a shared library,
then loads it from a normal C host. The host includes Elephc's generated header
and exercises the unchanged scalar ABI plus the binary-safe owned-string ABI.

## Build and run

Linux:

```bash
cargo run -- --emit cdylib examples/cdylib/auth.php
cc -o examples/cdylib/host examples/cdylib/host.c -ldl
./examples/cdylib/host examples/cdylib/libauth.so
```

macOS:

```bash
cargo run -- --emit cdylib examples/cdylib/auth.php
cc -o examples/cdylib/host examples/cdylib/host.c
./examples/cdylib/host examples/cdylib/libauth.dylib
```

Compilation produces both the library and `examples/cdylib/libauth.h`.

Expected output:

```text
elephc cdylib demo OK: scalar ABI + recoverable binary string roundtrip
```

## What the demo covers

- Conventional `lib<stem>.{so,dylib}` output and a deterministic
  `lib<stem>.h` header.
- Stable C-safe naming for namespaced exports (`Demo\add` becomes `Demo_add`),
  with compile-time collision detection.
- `dlopen`/`dlsym` access to the ABI version, lifecycle, diagnostic, allocation,
  and declared export symbols.
- The unchanged scalar ABI for `int` returns and string input parameters.
- A binary-safe `string -> string` call containing an embedded NUL byte.
- Caller ownership of successful string results through `elephc_free`, including
  safe `elephc_free(NULL)` behavior.
- A PHP exception reported as `ELEPHC_STATUS_PHP_EXCEPTION`, followed by a
  successful call in the same host process and a cleared last-error state.

The example works on macOS aarch64, Linux aarch64, and Linux x86_64. Internal
compiler/runtime symbols are private on Mach-O and ELF; the public symbol table
contains only documented Elephc boundary symbols and `#[Export]` functions.

## Current limits

- String returns use the deliberately narrow exact signature
  `string function(string)`; mixed scalar/string layouts are not yet public ABI.
- Array, object, callable, nullable, variadic, and by-reference export values are
  not supported.
- The cdylib ABI is single-threaded. It does not recover from hardware faults
  or memory corruption.
- `exit`/`die`, `eval`, dynamic construction, foreign calls, and call paths
  whose dispatch prevents proving process termination unreachable are rejected
  for string-returning exports.
