---
title: "getdate() — internals"
description: "Compiler internals for getdate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 72
---

## `getdate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:183](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L183) (`lower_getdate`)
- **Function symbol**: `lower_getdate()`


### Lowering notes

- Lowers `getdate([$timestamp])` through the shared decomposition runtime helper.
- Marshals the optional timestamp (the `-1` current-time sentinel when omitted; a boxed
- `Mixed`/`Union` argument is unboxed) into the integer result register where `__rt_getdate`
- reads it, then boxes the returned associative-array hash pointer into a `Mixed` cell — the same
- representation `stat`/`getdate` use, so the checker types the result `Mixed`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getdate`
- `__rt_mixed_from_value`

## Signature summary

```php
function getdate(int $timestamp): array
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `getdate()`](../../../php/builtins/date/getdate.md)

