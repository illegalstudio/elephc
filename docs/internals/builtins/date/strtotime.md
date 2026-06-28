---
title: "strtotime() — internals"
description: "Compiler internals for strtotime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 79
---

## `strtotime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:487](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L487) (`lower_strtotime`)
- **Function symbol**: `lower_strtotime()`


### Lowering notes

- Lowers `strtotime(datetime[, baseTimestamp])` through the shared parser runtime helper.
- Returns PHP's `int|false`: the `__rt_strtotime` `i64::MIN` parse-failure sentinel is boxed as
- `Mixed` `false`, and every other value (including a real `-1` pre-epoch timestamp) is boxed as
- a `Mixed` integer, so `=== false`, `=== -1`, and `echo` all observe the distinct results.
- Supports PHP's optional `$baseTimestamp`. (The `__elephc_strtotime_raw` alias keeps the plain
- `-1` integer shape for the synthetic `DateTime` internals.)

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_from_value`
- `__rt_strtotime`

## Signature summary

```php
function strtotime(string $datetime, int $baseTimestamp): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `strtotime()`](../../../php/builtins/date/strtotime.md)

