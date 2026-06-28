---
title: "localtime() — internals"
description: "Compiler internals for localtime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 76
---

## `localtime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:220](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L220) (`lower_localtime`)
- **Function symbol**: `lower_localtime()`


### Lowering notes

- Lowers `localtime([$timestamp[, $associative]])` through the shared decomposition runtime helper.
- `__rt_localtime` reads the timestamp from the integer result register (`x0`/`rax`) and the
- associative-keys flag from the second argument register (`x1`/`rsi`) — an irregular ABI, so the
- two values are staged in scratch (the flag may unbox a `Mixed`, clobbering the timestamp) and
- reloaded into their distinct registers with no intervening call, then the returned hash pointer
- is boxed into a `Mixed` associative-array cell like `getdate`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hrtime`
- `__rt_localtime`

## Signature summary

```php
function localtime(int $timestamp, bool $associative): array
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Cross-references

- [User reference for `localtime()`](../../../php/builtins/date/localtime.md)

