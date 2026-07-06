---
title: "usleep() — internals"
description: "Compiler internals for usleep(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 310
---

## `usleep()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/usleep.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/usleep.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:625](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L625) (`lower_usleep`)
- **Function symbol**: `lower_usleep()`


### Lowering notes

- Lowers `usleep(microseconds)` through the target's C library symbol.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getenv`

## Signature summary

```php
function usleep(int $microseconds): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `usleep()`](../../../php/builtins/process/usleep.md)
