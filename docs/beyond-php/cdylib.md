---
title: "Shared Libraries (cdylib)"
description: "Compile PHP functions into a C-callable shared library with --emit cdylib and #[Export]."
sidebar:
  order: 6
---

`--emit cdylib` compiles a PHP file into a loadable shared library instead of a
standalone executable. Top-level functions marked with `#[Export]` become
C-ABI symbols. Unnamespaced C-safe PHP names keep their existing spelling;
namespaced names replace namespace separators with underscores, so
`Demo\roundtrip` is exported as `Demo_roundtrip`. The compiler rejects any
resulting public-name collision and writes a deterministic C header containing
the exact mapped ABI.

The mode supports macOS aarch64 (`.dylib`), Linux aarch64 (`.so`), and Linux
x86_64 (`.so`). The ABI is single-threaded: a host must not call the same
Elephc library concurrently.

## Building a cdylib

```bash
elephc --emit cdylib auth.php
# Linux: auth.php -> libauth.so and libauth.h
# macOS: auth.php -> libauth.dylib and libauth.h
```

The aliases `--emit dylib` and `--emit shared` are accepted. A cdylib has no
`main` entry point and does not run top-level statements when loaded; the host
drives it through the public functions.

## Exporting functions

```php
<?php

#[Export]
function add_i64(int $a, int $b): int {
    return $a + $b;
}

#[Export]
function roundtrip(string $input): string {
    if ($input === "__elephc_fail__") {
        throw new RuntimeException("requested roundtrip failure");
    }
    return $input;
}
```

Only top-level functions can be exported. The fully-qualified spelling
`#[\Elephc\Export]` is also accepted. In executable mode the attribute is
ignored with a warning.

## Generated header and type marshaling

The generated `libauth.h` includes standard integer/size types, ABI and status
constants, C++ linkage guards, boundary declarations, every resolved export
prototype, and ownership comments.

Existing scalar signatures keep their original ABI:

| PHP type | C parameter | C scalar return |
|---|---|---|
| `int` | `int64_t` | `int64_t` |
| `float` | `double` | `double` |
| `bool` | `int64_t` (0 or 1) | `int64_t` |
| `string` | `const char *ptr, size_t len` | see the owned-string ABI below |
| `void` | — | `void` |

The first string-return ABI is deliberately exact: one by-value `string`
parameter and a `string` return. A function such as `roundtrip` has this C
prototype:

```c
int32_t roundtrip(
    const char *input_ptr,
    size_t input_len,
    char **output_ptr,
    size_t *output_len
);
```

`output_ptr` and `output_len` are required. The wrapper clears both outputs
before doing work and keeps them `NULL`/zero on every failure. A non-zero input
length requires a non-NULL input pointer; `NULL, 0` is an empty PHP string.
Input and output are byte sequences, so embedded NUL and non-UTF-8 bytes are
preserved.

On success the wrapper returns `ELEPHC_STATUS_OK`, publishes an independently
owned copy, and sets its exact byte length. The current buffer also has a
convenience trailing NUL, which is not included in `output_len`; hosts must use
the length as authoritative. Release every successful result with
`elephc_free()`. Never call `free()` on it or retain a borrowed runtime pointer.

Arrays, objects, callables, nullable, variadic, and by-reference export values
remain unsupported. Mixed scalar/string argument layouts that return strings
are also rejected until they have an explicit ABI.

## Boundary API and statuses

Every generated header declares:

```c
uint32_t  elephc_abi_version(void);
int32_t   elephc_init(void);
void      elephc_shutdown(void);
const char *elephc_last_error(void);
void      elephc_free(void *ptr);
```

Call `elephc_init()` after loading the library and `elephc_shutdown()` before
unloading it. `elephc_abi_version()` must match `ELEPHC_ABI_VERSION` from the
header. `elephc_free(NULL)` is safe.

The named status constants are:

| Status | Meaning |
|---|---|
| `ELEPHC_STATUS_OK` | Call succeeded and any output ownership transferred |
| `ELEPHC_STATUS_INVALID_ARGUMENT` | Required pointers or pointer/length pairs were invalid |
| `ELEPHC_STATUS_PHP_EXCEPTION` | A supported PHP exception escaped the exported function |
| `ELEPHC_STATUS_ALLOCATION_FAILURE` | Runtime or output-buffer allocation failed |
| `ELEPHC_STATUS_RUNTIME_FAILURE` | Another recoverable boundary failure occurred |

After a failure, `elephc_last_error()` returns a borrowed, NUL-terminated
diagnostic. It returns `NULL` when no error is recorded. The pointer is owned by
the library, must not be freed, and remains valid until the next exported call
or lifecycle reset. Every successful exported call and each lifecycle reset
clears the recorded error. An exception whose message is empty still records an
error: the function returns a non-NULL pointer to `""`, not `NULL`.

## Recoverable errors and restrictions

The owned-string wrapper installs Elephc's native exception boundary before it
calls PHP. Escaping supported Throwables are converted to
`ELEPHC_STATUS_PHP_EXCEPTION`; boundary-reachable allocation failures receive
their own status. Function-frame cleanup runs before control returns to C, so a
host can inspect the error and call the library successfully again.

ABI validation failures, supported PHP exceptions, and allocation failures are
recoverable. Hardware faults, memory corruption, and foreign code that aborts
the process are not contained.

`exit` and `die` cannot return a status, so the compiler rejects them when they
are transitively reachable from a string-returning export, including through a
fixed constructor or statically invoked closure. It also rejects `eval`,
runtime-selected constructors, foreign calls, and other opaque dynamic
invocation paths on that surface when it cannot prove process termination
unreachable. Scalar exports retain their existing ABI and do not gain a status
return channel.

## C consumption

Prefer including the generated header rather than copying declarations:

```c
#include "libauth.h"
#include <string.h>

int main(void) {
    const char input[] = {'A', 0, 'B'};
    char *output = NULL;
    size_t output_len = 0;

    if (elephc_init() != ELEPHC_STATUS_OK ||
        roundtrip(input, sizeof(input), &output, &output_len) != ELEPHC_STATUS_OK) {
        return 1;
    }
    int valid = output_len == sizeof(input) &&
                memcmp(input, output, sizeof(input)) == 0;
    elephc_free(output);
    elephc_shutdown();
    return valid ? 0 : 2;
}
```

The library can be linked normally with `-L. -lauth` and an appropriate loader
search path, or loaded with `dlopen`/`dlsym`. See `examples/cdylib/` for a
complete error-and-recovery host.

## Symbol visibility and PIC

The public symbol table contains only declared `#[Export]` functions plus
`elephc_abi_version`, `elephc_init`, `elephc_shutdown`, `elephc_last_error`, and
`elephc_free`. ELF internals use hidden visibility; Mach-O internals are private
externs. On Linux the CRT-supplied `_init`/`_fini` definitions are localized as
well. Runtime helpers, buffers, data constants, and non-exported PHP functions
therefore do not become host ABI or preempt another loaded Elephc library's
state.

Cdylib code generation is position-independent. Global references use the GOT
(`@GOTPCREL` on x86_64 and `:got:`/`:got_lo12:` on AArch64), allowing the
dynamic loader to relocate the library.

## Current limits

- One `.php` or `.lfc` entry source per cdylib; normal includes/requires still
  work.
- The ABI is single-threaded and exposes no per-host runtime context.
- Only the exact `string -> string` owned-result surface has a recoverable
  status boundary; the pre-existing scalar export ABI is unchanged.
- Array, object, callable, nullable, variadic, by-reference, and generic string
  return signatures are not public ABI.
