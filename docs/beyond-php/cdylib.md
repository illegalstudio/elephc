---
title: "Shared Libraries (cdylib)"
description: "Compile PHP functions into a C-callable shared library with --emit cdylib and #[Export]."
sidebar:
  order: 6
---

`--emit cdylib` compiles a PHP file into a loadable shared library instead of a
standalone executable. Functions marked with the `#[Export]` attribute become
C-ABI symbols under their unmangled PHP names, so any language with a C FFI
(C, Rust, Python `ctypes`, Go `cgo`, ...) can `dlopen` the library and call
into compiled PHP.

Supported on every elephc target: macOS aarch64 (`.dylib`), Linux aarch64 and
Linux x86_64 (`.so`).

## Building a cdylib

```bash
elephc --emit cdylib auth.php
# Linux:  auth.php -> libauth.so
# macOS:  auth.php -> libauth.dylib
```

The output follows the conventional `lib<stem>.so` / `lib<stem>.dylib` naming
that `dlopen(3)` and `-l` linker flags expect. The aliases `--emit dylib` and
`--emit shared` are accepted.

A cdylib has no `main` entry point and runs no top-level statements at load
time — the host drives everything through the exported functions.

## Exporting functions

```php
<?php

#[Export]
function validate_token(string $token): int {
    if (strlen($token) >= 8) {
        return 0;
    }
    return 1;
}

#[Export]
function add_i64(int $a, int $b): int {
    return $a + $b;
}

// Not exported: internal helpers stay invisible to the host.
function internal_helper(): int {
    return 42;
}
```

Only top-level functions can be exported — methods, closures, and arrow
functions are not eligible. The fully-qualified spelling `#[\Elephc\Export]`
is also accepted.

In `--emit executable` mode `#[Export]` attributes are ignored with a warning.

## v1 type marshaling

| PHP type | C parameter | C return |
|---|---|---|
| `int` | `int64_t` | `int64_t` |
| `float` | `double` | `double` |
| `bool` | `int64_t` (0 or 1) | `int64_t` |
| `string` | `const char *ptr, size_t len` (two arguments) | not supported in v1 |
| `void` | — | `void` |

Signatures outside this set (arrays, objects, callables, nullables, variadic
or by-reference parameters) are rejected at compile time with a diagnostic.
String returns arrive in a later iteration together with host-side ownership
through `elephc_free`.

## Lifecycle entry points

Every cdylib also exports four C-callable lifecycle symbols:

```c
int32_t     elephc_init(void);        // call once after dlopen; 0 = success
void        elephc_shutdown(void);    // call before dlclose
const char *elephc_last_error(void);  // NULL when no error is recorded
void        elephc_free(void *ptr);   // release elephc-owned pointers (v1: no-op)
```

In v1 these are inexpensive stubs — runtime state is zero-initialized BSS, so
`elephc_init` reports success without extra work — but hosts should call them
anyway so code keeps working as later versions add real setup and teardown.

## Minimal C host

```c
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>

int main(void) {
    void *lib = dlopen("./libauth.so", RTLD_NOW | RTLD_LOCAL);
    int32_t (*init)(void) = dlsym(lib, "elephc_init");
    int64_t (*add)(int64_t, int64_t) = dlsym(lib, "add_i64");
    int32_t (*validate)(const char *, size_t) = dlsym(lib, "validate_token");

    init();
    printf("%lld\n", (long long)add(40, 2));            // 42
    printf("%d\n", validate("supersecret", 11));        // 0
    return 0;
}
```

```bash
cc -o host host.c -ldl   # -ldl needed on Linux only
./host
```

See `examples/cdylib/` for a complete runnable demo.

## Symbol visibility

On ELF targets the dynamic symbol table of the produced `.so` contains only
the public ABI: the `#[Export]` trampolines and the four lifecycle entry
points. Every internal symbol — runtime helpers, buffers, data-section
constants, non-exported PHP functions — is emitted with hidden visibility.
This keeps internal state non-preemptible, so two elephc cdylibs loaded into
the same process never alias each other's runtime, and it keeps `dlsym`
lookups fast and unpolluted.

Internally, cdylib code generation runs in PIC mode: global data references go
through the GOT (`@GOTPCREL` on x86_64, `:got:`/`:got_lo12:` on AArch64) so the
dynamic loader can relocate the library anywhere in the address space.

## v1 limitations

- No string, array, object, callable, or nullable return values.
- No exception propagation to the host — a PHP fatal terminates the process.
- Single-threaded hosts only: the runtime has no locking around its heap.
- One PHP source file per cdylib (includes/requires work as in executables).
