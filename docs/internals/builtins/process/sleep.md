---
title: "sleep() — internals"
description: "Compiler internals for sleep(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 311
---

## `sleep()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:473](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L473) (`lower_sleep`)
- **Function symbol**: `lower_sleep()`


### Lowering notes

- Lowers `sleep(seconds)` through the target's C library symbol.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strtotime`

## Signature summary

```php
function sleep(int $seconds): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `sleep()`](../../../php/builtins/process/sleep.md)

