---
title: "date_default_timezone_set() — internals"
description: "Compiler internals for date_default_timezone_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 71
---

## `date_default_timezone_set()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:84](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L84) (`lower_date_default_timezone_set`)
- **Function symbol**: `lower_date_default_timezone_set()`


### Lowering notes

- Lowers `date_default_timezone_set(timezoneId)` through the shared runtime helper.
- Materializes the identifier string into the registers the helper reads (ptr/len in
- `x1`/`x2` on ARM64, `rax`/`rdx` on x86_64), then `__rt_date_default_timezone_set`
- applies it via libc `putenv`+`tzset` and returns PHP `true` in the integer-result register.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_date_default_timezone_set`
- `__rt_microtime`
- `__rt_microtime_mixed`
- `__rt_microtime_str`

## Signature summary

```php
function date_default_timezone_set(string $timezoneId): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `date_default_timezone_set()`](../../../php/builtins/date/date_default_timezone_set.md)

