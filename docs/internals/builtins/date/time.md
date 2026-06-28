---
title: "time() — internals"
description: "Compiler internals for time(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 80
---

## `time()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:615](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L615) (`lower_time`)
- **Function symbol**: `lower_time()`


### Lowering notes

- Lowers `time()` through the shared wall-clock runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_time`

## Signature summary

```php
function time(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `time()`](../../../php/builtins/date/time.md)

