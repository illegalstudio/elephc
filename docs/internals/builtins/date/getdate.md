---
title: "getdate() — internals"
description: "Compiler internals for getdate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 93
---

## `getdate()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/getdate.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/getdate.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:183](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L183) (`lower_getdate`)
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
function getdate(int $timestamp = null): array
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/time/getdate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/getdate.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `getdate()`](../../../php/builtins/date/getdate.md)
